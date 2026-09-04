use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationEntryKind {
    Parent,
    Directory,
    DisabledSymlink,
}

#[derive(Debug, Eq, PartialEq)]
pub struct DestinationEntry {
    name: OsString,
    path: PathBuf,
    kind: DestinationEntryKind,
}

impl DestinationEntry {
    pub fn display_name(&self) -> String {
        match self.kind {
            DestinationEntryKind::Parent => "..".to_owned(),
            DestinationEntryKind::Directory => {
                format!("{}/", self.name.to_string_lossy())
            }
            DestinationEntryKind::DisabledSymlink => {
                format!("{}@ (disabled)", self.name.to_string_lossy())
            }
        }
    }

    pub fn kind(&self) -> DestinationEntryKind {
        self.kind
    }
}

#[derive(Debug)]
pub struct DestinationBrowser {
    home: PathBuf,
    current: PathBuf,
    entries: Vec<DestinationEntry>,
    selected: Option<usize>,
    error: Option<String>,
}

impl DestinationBrowser {
    pub fn new(home: PathBuf) -> Self {
        Self {
            current: home.clone(),
            home,
            entries: Vec::new(),
            selected: None,
            error: None,
        }
    }

    pub fn current(&self) -> &Path {
        &self.current
    }

    pub fn entries(&self) -> &[DestinationEntry] {
        &self.entries
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn refresh(&mut self) {
        match scan_directory(&self.home, &self.current) {
            Ok(entries) => {
                self.entries = entries;
                self.selected = (!self.entries.is_empty()).then_some(0);
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn move_down(&mut self) {
        if let Some(selected) = self.selected {
            self.selected = Some((selected + 1).min(self.entries.len().saturating_sub(1)));
        }
    }

    pub fn move_up(&mut self) {
        if let Some(selected) = self.selected {
            self.selected = Some(selected.saturating_sub(1));
        }
    }

    pub fn move_to_first(&mut self) {
        self.selected = (!self.entries.is_empty()).then_some(0);
    }

    pub fn move_to_last(&mut self) {
        self.selected = self.entries.len().checked_sub(1);
    }

    pub fn enter_selected(&mut self) {
        let Some(entry) = self.selected.and_then(|index| self.entries.get(index)) else {
            return;
        };
        if entry.kind == DestinationEntryKind::DisabledSymlink {
            return;
        }
        self.navigate_to(entry.path.clone());
    }

    pub fn enter_parent(&mut self) {
        if self.current == self.home {
            return;
        }
        let parent = self
            .current
            .parent()
            .filter(|parent| parent.starts_with(&self.home))
            .unwrap_or(&self.home)
            .to_path_buf();
        self.navigate_to(parent);
    }

    fn navigate_to(&mut self, path: PathBuf) {
        match scan_directory(&self.home, &path) {
            Ok(entries) => {
                self.current = path;
                self.entries = entries;
                self.selected = (!self.entries.is_empty()).then_some(0);
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

fn scan_directory(home: &Path, current: &Path) -> io::Result<Vec<DestinationEntry>> {
    let directory = fs::read_dir(current).map_err(|error| {
        contextual_error(
            error,
            format!(
                "failed to read destination directory at {}",
                current.display()
            ),
        )
    })?;
    let mut directories = Vec::new();
    let mut symlinks = Vec::new();

    for result in directory {
        let entry = result.map_err(|error| {
            contextual_error(
                error,
                format!(
                    "destination entry changed while reading {}",
                    current.display()
                ),
            )
        })?;
        let name = entry.file_name();
        if is_hidden(&name) {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            contextual_error(
                error,
                format!(
                    "failed to inspect destination entry {}",
                    entry.path().display()
                ),
            )
        })?;

        if file_type.is_symlink() {
            match fs::metadata(entry.path()) {
                Ok(metadata) if metadata.is_dir() => symlinks.push(DestinationEntry {
                    name,
                    path: entry.path(),
                    kind: DestinationEntryKind::DisabledSymlink,
                }),
                Ok(_) => {}
                Err(error) => {
                    return Err(contextual_error(
                        error,
                        format!(
                            "failed to inspect destination symlink {}",
                            entry.path().display()
                        ),
                    ));
                }
            }
        } else if file_type.is_dir() {
            directories.push(DestinationEntry {
                name,
                path: entry.path(),
                kind: DestinationEntryKind::Directory,
            });
        }
    }

    directories.sort_by(|left, right| left.name.cmp(&right.name));
    symlinks.sort_by(|left, right| left.name.cmp(&right.name));
    let mut entries =
        Vec::with_capacity(directories.len() + symlinks.len() + usize::from(current != home));
    if current != home {
        entries.push(DestinationEntry {
            name: OsString::from(".."),
            path: current.parent().unwrap_or(home).to_path_buf(),
            kind: DestinationEntryKind::Parent,
        });
    }
    entries.extend(directories);
    entries.extend(symlinks);
    Ok(entries)
}

fn is_hidden(name: &OsStr) -> bool {
    name.as_encoded_bytes().first() == Some(&b'.')
}

fn contextual_error(source: io::Error, context: String) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{DestinationBrowser, DestinationEntryKind, scan_directory};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "downloads-janitor-destination-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("test directory should be created");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("test directory should be removed");
        }
    }

    #[test]
    fn scan_omits_files_and_hidden_directories_and_sorts_rows_by_kind() {
        let home = TestDirectory::new();
        fs::create_dir(home.0.join("zebra")).unwrap();
        fs::create_dir(home.0.join("alpha")).unwrap();
        fs::create_dir(home.0.join(".hidden")).unwrap();
        File::create(home.0.join("file.txt")).unwrap();
        let child = home.0.join("child");
        fs::create_dir(&child).unwrap();
        fs::create_dir(child.join("beta")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(home.0.join("zebra"), child.join("z-link")).unwrap();
            symlink(home.0.join("alpha"), child.join("a-link")).unwrap();
        }

        let entries = scan_directory(&home.0, &child).unwrap();
        let rows = entries
            .iter()
            .map(|entry| (entry.display_name(), entry.kind()))
            .collect::<Vec<_>>();
        #[cfg(unix)]
        assert_eq!(
            rows,
            vec![
                ("..".to_owned(), DestinationEntryKind::Parent),
                ("beta/".to_owned(), DestinationEntryKind::Directory),
                (
                    "a-link@ (disabled)".to_owned(),
                    DestinationEntryKind::DisabledSymlink
                ),
                (
                    "z-link@ (disabled)".to_owned(),
                    DestinationEntryKind::DisabledSymlink
                ),
            ]
        );
        #[cfg(not(unix))]
        assert_eq!(
            rows,
            vec![
                ("..".to_owned(), DestinationEntryKind::Parent),
                ("beta/".to_owned(), DestinationEntryKind::Directory),
            ]
        );

        let home_entries = scan_directory(&home.0, &home.0).unwrap();
        assert_eq!(
            home_entries
                .iter()
                .map(|entry| entry.display_name())
                .collect::<Vec<_>>(),
            vec!["alpha/", "child/", "zebra/"]
        );
    }

    #[test]
    fn navigation_is_bounded_and_disabled_symlinks_are_not_followed() {
        let home = TestDirectory::new();
        let child = home.0.join("child");
        fs::create_dir(&child).unwrap();
        let mut browser = DestinationBrowser::new(home.0.clone());
        browser.refresh();

        browser.enter_parent();
        assert_eq!(browser.current(), home.0);
        browser.enter_selected();
        assert_eq!(browser.current(), child);
        browser.enter_parent();
        assert_eq!(browser.current(), home.0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&child, home.0.join("link")).unwrap();
            browser.refresh();
            browser.move_down();
            browser.enter_selected();
            assert_eq!(browser.current(), home.0);
        }
    }

    #[test]
    fn jumps_keep_empty_and_single_row_selections_valid() {
        let home = TestDirectory::new();
        let mut browser = DestinationBrowser::new(home.0.clone());
        browser.refresh();
        browser.move_to_first();
        browser.move_to_last();
        assert_eq!(browser.selected(), None);

        fs::create_dir(home.0.join("only")).unwrap();
        browser.refresh();
        browser.move_to_last();
        browser.move_to_first();
        assert_eq!(browser.selected(), Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn jumps_include_parent_and_disabled_symlink_rows() {
        use std::os::unix::fs::symlink;

        let home = TestDirectory::new();
        let child = home.0.join("child");
        let target = home.0.join("target");
        fs::create_dir(&child).unwrap();
        fs::create_dir(&target).unwrap();
        symlink(&target, child.join("z-link")).unwrap();
        let mut browser = DestinationBrowser::new(home.0.clone());
        browser.refresh();
        browser.enter_selected();

        browser.move_to_last();
        let last = browser.selected().unwrap();
        assert_eq!(
            browser.entries()[last].kind(),
            DestinationEntryKind::DisabledSymlink
        );

        browser.move_to_first();
        assert_eq!(browser.entries()[0].kind(), DestinationEntryKind::Parent);
    }

    #[test]
    fn returning_to_a_parent_rescans_it() {
        let home = TestDirectory::new();
        fs::create_dir(home.0.join("child")).unwrap();
        let mut browser = DestinationBrowser::new(home.0.clone());
        browser.refresh();
        browser.enter_selected();

        fs::create_dir(home.0.join("new-directory")).unwrap();
        browser.enter_parent();

        assert!(
            browser
                .entries()
                .iter()
                .any(|entry| entry.display_name() == "new-directory/")
        );
    }

    #[test]
    fn failed_entry_keeps_current_directory_and_exposes_context() {
        let home = TestDirectory::new();
        fs::create_dir(home.0.join("vanished")).unwrap();
        let mut browser = DestinationBrowser::new(home.0.clone());
        browser.refresh();
        fs::remove_dir(home.0.join("vanished")).unwrap();

        browser.enter_selected();

        assert_eq!(browser.current(), home.0);
        assert!(
            browser
                .error()
                .unwrap()
                .contains("failed to read destination")
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_supports_non_unicode_directory_names() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let home = TestDirectory::new();
        let name = OsString::from_vec(b"directory-\xff".to_vec());
        fs::create_dir(home.0.join(&name)).unwrap();

        let entries = scan_directory(&home.0, &home.0).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, name);
    }
}
