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
    warning: Option<String>,
}

impl Favorites {
    pub fn load(home: PathBuf) -> Self {
        let path = home.join(".config/downloads-janitor/configuration-v1");
        let mut favorites = Self {
            home,
            path,
            entries: Vec::new(),
            warning: None,
        };
        match read(&favorites.path).and_then(|contents| decode(&contents)) {
            Ok(entries) => favorites.entries = entries,
            Err(error) => {
                favorites.warning = Some(format!(
                    "Configuration is unreadable; Favorites cannot be changed. {error}. Repair {:?}",
                    favorites.path
                ));
            }
        }
        favorites
    }

    pub fn entries(&self) -> &[FavoriteDestination] {
        &self.entries
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

    fn ensure_writable(&self) -> io::Result<()> {
        if self.warning.is_some() {
            Err(io::Error::other(
                "Configuration is unreadable; repair it before changing Favorites",
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

    fn save(&self) -> io::Result<()> {
        let parent = self.path.parent().expect("configuration path has a parent");
        fs::create_dir_all(parent)?;
        let bytes = encode(&self.entries);
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

fn encode(entries: &[FavoriteDestination]) -> Vec<u8> {
    let mut text = HEADER.to_owned();
    for favorite in entries {
        text.push_str(&format!(
            "favorite {} {}\n",
            hex(favorite.name.as_bytes()),
            hex(favorite.path.as_os_str().as_bytes())
        ));
    }
    text.into_bytes()
}

fn decode(bytes: &[u8]) -> io::Result<Vec<FavoriteDestination>> {
    let invalid = || io::Error::other("invalid configuration format");
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let body = text.strip_prefix(HEADER).ok_or_else(invalid)?;
    if !body.is_empty() && !body.ends_with('\n') {
        return Err(invalid());
    }
    let mut entries = Vec::new();
    for line in body.lines() {
        let fields = line.split(' ').collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != "favorite" {
            return Err(invalid());
        }
        let name =
            String::from_utf8(unhex(fields[1]).map_err(|_| invalid())?).map_err(|_| invalid())?;
        validate_name(&name).map_err(|_| invalid())?;
        let path = PathBuf::from(OsString::from_vec(unhex(fields[2]).map_err(|_| invalid())?));
        if !path.is_absolute()
            || entries
                .iter()
                .any(|favorite: &FavoriteDestination| favorite.name == name)
        {
            return Err(invalid());
        }
        entries.push(FavoriteDestination { name, path });
    }
    Ok(entries)
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

    use super::{FavoriteDestination, Favorites, decode, encode};

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
        assert_eq!(decode(&encode(&entries)).unwrap(), entries);
        assert!(decode(b"bad").is_err());
    }
}
