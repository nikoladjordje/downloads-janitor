use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const HEADER: &str = "downloads-janitor-configuration-v1\n";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FavoriteDestination {
    name: String,
    path: PathBuf,
}

/// The Inbox Entry kind a Rule applies to. A Symlink Entry is always a symlink,
/// even when its target is a file or directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleKind {
    Any,
    File,
    Directory,
    Symlink,
}

impl RuleKind {
    pub const ALL: [Self; 4] = [Self::Any, Self::File, Self::Directory, Self::Symlink];

    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::File => "File",
            Self::Directory => "Directory",
            Self::Symlink => "Symlink",
        }
    }

    fn encoded(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        Some(match value {
            "any" => Self::Any,
            "file" => Self::File,
            "directory" => Self::Directory,
            "symlink" => Self::Symlink,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    pattern: String,
    kind: RuleKind,
    favorite_name: String,
}

impl Rule {
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
    pub fn kind(&self) -> RuleKind {
        self.kind
    }
    pub fn favorite_name(&self) -> &str {
        &self.favorite_name
    }
}

impl FavoriteDestination {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct Favorites {
    home: PathBuf,
    path: PathBuf,
    entries: Vec<FavoriteDestination>,
    rules: Vec<Rule>,
    warning: Option<String>,
}

impl Favorites {
    pub fn load(home: PathBuf) -> Self {
        let path = home.join(".config/downloads-janitor/configuration-v1");
        let mut favorites = Self {
            home,
            path,
            entries: Vec::new(),
            rules: Vec::new(),
            warning: None,
        };
        favorites.reload();
        favorites
    }

    /// Reloads Configuration from disk. A bad on-disk Configuration remains
    /// untouched and clears loaded Rules so they cannot suggest destinations
    /// until the user repairs it and reloads again.
    pub fn reload(&mut self) {
        match read(&self.path).and_then(|contents| decode(&contents)) {
            Ok((entries, rules)) => {
                self.entries = entries;
                self.rules = rules;
                self.warning = None;
            }
            Err(error) => {
                self.entries.clear();
                self.rules.clear();
                self.warning = Some(format!(
                    "Configuration error: {error}. Repair {:?}, then press R to reload",
                    self.path
                ));
            }
        }
    }

    pub fn entries(&self) -> &[FavoriteDestination] {
        &self.entries
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn rule_has_available_favorite(&self, rule: &Rule) -> bool {
        self.entries
            .iter()
            .any(|favorite| favorite.name == rule.favorite_name && self.available(favorite))
    }

    pub fn suggested_match(
        &self,
        entry: &crate::inbox::InboxEntry,
    ) -> crate::rule_match::RuleMatch {
        crate::rule_match::evaluate(entry, &self.rules, |name| {
            self.entries
                .iter()
                .find(|favorite| favorite.name == name && self.available(favorite))
                .map(|favorite| favorite.path.clone())
        })
    }

    pub fn add_rule(
        &mut self,
        pattern: String,
        kind: RuleKind,
        favorite_name: String,
    ) -> io::Result<()> {
        self.ensure_writable()?;
        self.validate_rule(&pattern, &favorite_name)?;
        self.rules.push(Rule {
            pattern,
            kind,
            favorite_name,
        });
        self.save_or_revert_rule()
    }

    pub fn replace_rule(
        &mut self,
        index: usize,
        pattern: String,
        kind: RuleKind,
        favorite_name: String,
    ) -> io::Result<()> {
        self.ensure_writable()?;
        self.validate_rule(&pattern, &favorite_name)?;
        let rule = self
            .rules
            .get_mut(index)
            .ok_or_else(|| io::Error::other("Rule no longer exists"))?;
        let old = std::mem::replace(
            rule,
            Rule {
                pattern,
                kind,
                favorite_name,
            },
        );
        match self.save() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.rules[index] = old;
                Err(error)
            }
        }
    }

    pub fn remove_rule(&mut self, index: usize) -> io::Result<Rule> {
        self.ensure_writable()?;
        if index >= self.rules.len() {
            return Err(io::Error::other("Rule no longer exists"));
        }
        let removed = self.rules.remove(index);
        match self.save() {
            Ok(()) => Ok(removed),
            Err(error) => {
                self.rules.insert(index, removed);
                Err(error)
            }
        }
    }

    pub fn move_rule(&mut self, index: usize, direction: isize) -> io::Result<usize> {
        self.ensure_writable()?;
        let destination = index
            .checked_add_signed(direction)
            .filter(|destination| *destination < self.rules.len())
            .ok_or_else(|| io::Error::other("Rule is already at the boundary"))?;
        self.rules.swap(index, destination);
        if let Err(error) = self.save() {
            self.rules.swap(index, destination);
            return Err(error);
        }
        Ok(destination)
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn available(&self, favorite: &FavoriteDestination) -> bool {
        valid_destination(&self.home, favorite.path()).is_ok()
    }

    pub fn add(&mut self, name: String, path: PathBuf) -> io::Result<()> {
        self.ensure_writable()?;
        validate_name(&name)?;
        if self.entries.iter().any(|favorite| favorite.name == name) {
            return Err(io::Error::other(
                "Favorite names must be unique (case-sensitive)",
            ));
        }
        valid_destination(&self.home, &path)?;
        self.entries.push(FavoriteDestination { name, path });
        self.save_or_revert()
    }

    pub fn replace(&mut self, index: usize, name: String, path: PathBuf) -> io::Result<()> {
        self.ensure_writable()?;
        validate_name(&name)?;
        if self
            .entries
            .iter()
            .enumerate()
            .any(|(other, favorite)| other != index && favorite.name == name)
        {
            return Err(io::Error::other(
                "Favorite names must be unique (case-sensitive)",
            ));
        }
        valid_destination(&self.home, &path)?;
        let favorite = self
            .entries
            .get_mut(index)
            .ok_or_else(|| io::Error::other("Favorite no longer exists"))?;
        let old = std::mem::replace(favorite, FavoriteDestination { name, path });
        match self.save() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.entries[index] = old;
                Err(error)
            }
        }
    }

    pub fn remove(&mut self, index: usize) -> io::Result<FavoriteDestination> {
        self.ensure_writable()?;
        if index >= self.entries.len() {
            return Err(io::Error::other("Favorite no longer exists"));
        }
        let removed = self.entries.remove(index);
        match self.save() {
            Ok(()) => Ok(removed),
            Err(error) => {
                self.entries.insert(index, removed);
                Err(error)
            }
        }
    }

    fn ensure_writable(&mut self) -> io::Result<()> {
        // Re-read before writing so an externally introduced malformed file is
        // never replaced by a configuration edit from this process.
        self.reload();
        if self.warning.is_some() {
            Err(io::Error::other(
                "Configuration has errors; repair it and reload before changing Favorites or Rules",
            ))
        } else {
            Ok(())
        }
    }

    fn save_or_revert(&mut self) -> io::Result<()> {
        match self.save() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.entries.pop();
                Err(error)
            }
        }
    }

    fn save_or_revert_rule(&mut self) -> io::Result<()> {
        match self.save() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.rules.pop();
                Err(error)
            }
        }
    }

    fn validate_rule(&self, pattern: &str, favorite_name: &str) -> io::Result<()> {
        if pattern.is_empty() {
            return Err(io::Error::other("Rule basename pattern cannot be empty"));
        }
        if !self
            .entries
            .iter()
            .any(|favorite| favorite.name == favorite_name)
        {
            return Err(io::Error::other(
                "Rule must reference an existing Favorite Destination",
            ));
        }
        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        let parent = self.path.parent().expect("configuration path has a parent");
        fs::create_dir_all(parent)?;
        let bytes = encode(&self.entries, &self.rules);
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = parent.join(format!(
            ".configuration-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn valid_destination(home: &Path, path: &Path) -> io::Result<()> {
    if !path.starts_with(home) {
        return Err(io::Error::other(
            "Favorite Destination must be at or below $HOME",
        ));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(io::Error::other(
            "Favorite Destination must be an existing non-symlink directory",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> io::Result<()> {
    if name.is_empty() {
        Err(io::Error::other("Favorite name cannot be empty"))
    } else {
        Ok(())
    }
}

fn read(path: &Path) -> io::Result<Vec<u8>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path),
        Ok(_) => Err(io::Error::other("configuration is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(HEADER.as_bytes().to_vec()),
        Err(error) => Err(error),
    }
}

fn encode(entries: &[FavoriteDestination], rules: &[Rule]) -> Vec<u8> {
    let mut text = HEADER.to_owned();
    for favorite in entries {
        text.push_str(&format!(
            "favorite {} {}\n",
            hex(favorite.name.as_bytes()),
            hex(favorite.path.as_os_str().as_bytes())
        ));
    }
    for rule in rules {
        text.push_str(&format!(
            "rule {} {} {}\n",
            hex(rule.pattern.as_bytes()),
            rule.kind.encoded(),
            hex(rule.favorite_name.as_bytes())
        ));
    }
    text.into_bytes()
}

fn decode(bytes: &[u8]) -> io::Result<(Vec<FavoriteDestination>, Vec<Rule>)> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| io::Error::other("configuration is not valid UTF-8"))?;
    let body = text
        .strip_prefix(HEADER)
        .ok_or_else(|| io::Error::other("missing or unsupported configuration header"))?;
    if !body.is_empty() && !body.ends_with('\n') {
        return Err(io::Error::other(
            "final configuration line is not newline-terminated",
        ));
    }
    let mut entries = Vec::new();
    let mut rules = Vec::new();
    for (index, line) in body.lines().enumerate() {
        let line_number = index + 2;
        let invalid = |problem: &str| io::Error::other(format!("line {line_number}: {problem}"));
        let fields = line.split(' ').collect::<Vec<_>>();
        match fields.as_slice() {
            ["favorite", name, path] => {
                let name = String::from_utf8(
                    unhex(name).map_err(|_| invalid("Favorite name is not hexadecimal UTF-8"))?,
                )
                .map_err(|_| invalid("Favorite name is not valid UTF-8"))?;
                validate_name(&name).map_err(|_| invalid("Favorite name cannot be empty"))?;
                let path = PathBuf::from(OsString::from_vec(
                    unhex(path).map_err(|_| invalid("Favorite path is not hexadecimal"))?,
                ));
                if !path.is_absolute() {
                    return Err(invalid("Favorite path must be absolute"));
                }
                if entries
                    .iter()
                    .any(|favorite: &FavoriteDestination| favorite.name == name)
                {
                    return Err(invalid("Favorite names must be unique"));
                }
                entries.push(FavoriteDestination { name, path });
            }
            ["rule", pattern, kind, favorite_name] => {
                let pattern = String::from_utf8(
                    unhex(pattern).map_err(|_| invalid("Rule pattern is not hexadecimal UTF-8"))?,
                )
                .map_err(|_| invalid("Rule pattern is not valid UTF-8"))?;
                let favorite_name = String::from_utf8(
                    unhex(favorite_name)
                        .map_err(|_| invalid("Rule Favorite name is not hexadecimal UTF-8"))?,
                )
                .map_err(|_| invalid("Rule Favorite name is not valid UTF-8"))?;
                if pattern.is_empty() || favorite_name.is_empty() {
                    return Err(invalid("Rule pattern and Favorite name cannot be empty"));
                }
                rules.push(Rule {
                    pattern,
                    kind: RuleKind::decode(kind).ok_or_else(|| {
                        invalid("Rule kind must be any, file, directory, or symlink")
                    })?,
                    favorite_name,
                });
            }
            _ => return Err(invalid("expected a Favorite or Rule record")),
        }
    }
    Ok((entries, rules))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, ()> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return Err(());
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::ffi::OsStringExt,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{FavoriteDestination, Favorites, Rule, RuleKind, decode, encode};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "downloads-janitor-favorites-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn favorites_persist_and_case_sensitive_names_are_unique() {
        let home = TestDirectory::new();
        let destination = home.0.join("Projects");
        fs::create_dir(&destination).unwrap();
        let mut favorites = Favorites::load(home.0.clone());
        favorites
            .add("Projects".into(), destination.clone())
            .unwrap();
        favorites
            .add("projects".into(), destination.clone())
            .unwrap();
        assert!(
            favorites
                .add("Projects".into(), destination.clone())
                .is_err()
        );
        let reloaded = Favorites::load(home.0.clone());
        assert_eq!(reloaded.entries().len(), 2);
        assert_eq!(reloaded.entries()[0].path(), destination);
    }

    #[test]
    fn favorites_reject_outside_files_and_symlinks_but_keep_stale_entries_visible() {
        let home = TestDirectory::new();
        let directory = home.0.join("directory");
        let file = home.0.join("file");
        fs::create_dir(&directory).unwrap();
        fs::write(&file, b"not a directory").unwrap();
        let mut favorites = Favorites::load(home.0.clone());
        assert!(
            favorites
                .add("outside".into(), PathBuf::from("/tmp"))
                .is_err()
        );
        assert!(favorites.add("file".into(), file).is_err());
        favorites
            .add("directory".into(), directory.clone())
            .unwrap();
        fs::remove_dir(&directory).unwrap();
        assert!(!favorites.available(&favorites.entries()[0]));
    }

    #[test]
    fn format_round_trips_paths_and_rejects_corruption() {
        let entries = vec![FavoriteDestination {
            name: "Raw name".into(),
            path: PathBuf::from(std::ffi::OsString::from_vec(b"/home/test/\xff".to_vec())),
        }];
        assert_eq!(decode(&encode(&entries, &[])).unwrap().0, entries);
        assert!(decode(b"bad").is_err());
    }

    #[test]
    fn rules_persist_in_order_and_keep_missing_references() {
        let home = TestDirectory::new();
        let destination = home.0.join("Projects");
        fs::create_dir(&destination).unwrap();
        let mut favorites = Favorites::load(home.0.clone());
        favorites.add("projects".into(), destination).unwrap();
        favorites
            .add_rule("*.rs".into(), RuleKind::File, "projects".into())
            .unwrap();
        favorites
            .add_rule("*".into(), RuleKind::Any, "projects".into())
            .unwrap();
        favorites.move_rule(1, -1).unwrap();
        assert_eq!(Favorites::load(home.0.clone()).rules()[0].pattern(), "*");
        favorites.remove(0).unwrap();
        let reloaded = Favorites::load(home.0.clone());
        assert_eq!(reloaded.rules()[0].favorite_name(), "projects");
        assert!(!reloaded.rule_has_available_favorite(&reloaded.rules()[0]));
    }

    #[test]
    fn rule_format_round_trips() {
        let rules = vec![Rule {
            pattern: "*.txt".into(),
            kind: RuleKind::File,
            favorite_name: "Archive".into(),
        }];
        let (_, decoded) = decode(&encode(&[], &rules)).unwrap();
        assert_eq!(decoded, rules);
    }

    #[test]
    fn malformed_configuration_is_preserved_with_a_specific_error_until_repaired() {
        let home = TestDirectory::new();
        let configuration = home.0.join(".config/downloads-janitor/configuration-v1");
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        let broken = b"downloads-janitor-configuration-v1\nfavorite nope 2f746d70\n";
        fs::write(&configuration, broken).unwrap();

        let mut favorites = Favorites::load(home.0.clone());
        assert!(favorites.warning().unwrap().contains("line 2"));
        assert!(favorites.warning().unwrap().contains("Favorite name"));
        assert!(favorites.entries().is_empty());
        assert!(favorites.rules().is_empty());
        assert!(favorites.add("new".into(), home.0.clone()).is_err());
        assert_eq!(fs::read(&configuration).unwrap(), broken);

        fs::write(&configuration, encode(&[], &[])).unwrap();
        favorites.reload();
        assert_eq!(favorites.warning(), None);
        favorites.add("home".into(), home.0.clone()).unwrap();
    }
}
