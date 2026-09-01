use std::{
    env,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

use crate::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EntryKind {
    Directory,
    File,
}

#[derive(Debug, Eq, PartialEq)]
pub struct InboxEntry {
    name: OsString,
    path: PathBuf,
    kind: EntryKind,
}

impl InboxEntry {
    #[cfg(test)]
    pub(crate) fn test_file(name: &str) -> Self {
        Self {
            name: OsString::from(name),
            path: PathBuf::from(name),
            kind: EntryKind::File,
        }
    }

    pub fn is_directory(&self) -> bool {
        self.kind == EntryKind::Directory
    }

    pub fn display_name(&self) -> String {
        debug_assert_eq!(self.path.file_name(), Some(self.name.as_os_str()));
        let mut name = self.name.to_string_lossy().into_owned();
        if self.is_directory() {
            name.push('/');
        }
        name
    }
}

pub fn scan_downloads() -> Result<Vec<InboxEntry>> {
    let home = env::var_os("HOME");
    scan_inbox(&downloads_path(home.as_deref())?)
}

fn downloads_path(home: Option<&OsStr>) -> io::Result<PathBuf> {
    let home = home.filter(|path| !path.is_empty()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot locate Downloads inbox because $HOME is not set or is empty",
        )
    })?;

    Ok(PathBuf::from(home).join("Downloads"))
}

fn scan_inbox(path: &Path) -> Result<Vec<InboxEntry>> {
    let directory = fs::read_dir(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to read Downloads inbox at {}: {error}",
                path.display()
            ),
        )
    })?;

    let mut entries = directory
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            // `metadata` follows symlinks, so links are classified by their targets
            // while the entry's own name and path remain visible in the inbox.
            let metadata = fs::metadata(entry.path()).ok()?;
            let kind = if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                return None;
            };

            Some(InboxEntry {
                name: entry.file_name(),
                path: entry.path(),
                kind,
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{EntryKind, downloads_path, scan_inbox};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "downloads-janitor-{}-{sequence}",
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
    fn unset_home_is_rejected() {
        let error = downloads_path(None).expect_err("unset home should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("$HOME"));
    }

    #[test]
    fn empty_home_is_rejected() {
        let error =
            downloads_path(Some(std::ffi::OsStr::new(""))).expect_err("empty home should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("empty"));
    }

    #[test]
    fn downloads_path_is_resolved_relative_to_valid_home() {
        assert_eq!(
            downloads_path(Some(std::ffi::OsStr::new("/home/someone")))
                .expect("valid home should resolve"),
            PathBuf::from("/home/someone/Downloads")
        );
    }

    #[cfg(unix)]
    #[test]
    fn downloads_path_supports_non_unicode_home() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let home = OsString::from_vec(b"/home/non-unicode-\xff".to_vec());

        assert_eq!(
            downloads_path(Some(&home)).expect("non-Unicode home should resolve"),
            PathBuf::from(&home).join("Downloads")
        );
    }

    #[test]
    fn empty_inbox_returns_an_empty_entry_list() {
        let inbox = TestDirectory::new();

        let entries = scan_inbox(&inbox.0).expect("empty inbox should be scanned");

        assert!(entries.is_empty());
    }

    #[test]
    fn scanner_lists_only_immediate_entries_in_directory_first_order() {
        let inbox = TestDirectory::new();
        fs::create_dir(inbox.0.join("zebra-dir")).expect("directory should be created");
        fs::create_dir(inbox.0.join("alpha-dir")).expect("directory should be created");
        File::create(inbox.0.join("zebra.txt")).expect("file should be created");
        File::create(inbox.0.join("alpha.txt")).expect("file should be created");
        File::create(inbox.0.join("alpha-dir").join("nested.txt"))
            .expect("nested file should be created");

        let entries = scan_inbox(&inbox.0).expect("test inbox should be scanned");
        let actual = entries
            .iter()
            .map(|entry| (entry.display_name(), entry.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            actual,
            vec![
                ("alpha-dir/".to_owned(), EntryKind::Directory),
                ("zebra-dir/".to_owned(), EntryKind::Directory),
                ("alpha.txt".to_owned(), EntryKind::File),
                ("zebra.txt".to_owned(), EntryKind::File),
            ]
        );
        assert!(entries.iter().all(|entry| entry.path.starts_with(&inbox.0)));
    }

    #[cfg(unix)]
    #[test]
    fn scanner_represents_non_unicode_filenames() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let inbox = TestDirectory::new();
        let filename = OsString::from_vec(b"not-unicode-\xff".to_vec());
        File::create(inbox.0.join(&filename)).expect("file should be created");

        let entries = scan_inbox(&inbox.0).expect("test inbox should be scanned");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, filename);
        assert!(entries[0].display_name().starts_with("not-unicode-"));
    }

    #[cfg(unix)]
    #[test]
    fn scanner_classifies_symlinks_by_their_targets_and_skips_broken_links() {
        use std::os::unix::fs::symlink;

        let inbox = TestDirectory::new();
        let targets = TestDirectory::new();
        let target_file = targets.0.join("target.txt");
        let target_directory = targets.0.join("target-dir");
        File::create(&target_file).expect("target file should be created");
        fs::create_dir(&target_directory).expect("target directory should be created");
        File::create(target_directory.join("nested.txt"))
            .expect("nested target file should be created");
        symlink(&target_file, inbox.0.join("linked-file")).expect("file symlink should be created");
        symlink(&target_directory, inbox.0.join("linked-directory"))
            .expect("directory symlink should be created");
        symlink(targets.0.join("missing"), inbox.0.join("broken-link"))
            .expect("broken symlink should be created");

        let entries = scan_inbox(&inbox.0).expect("test inbox should be scanned");
        let actual = entries
            .iter()
            .map(|entry| (entry.display_name(), entry.kind, entry.path.clone()))
            .collect::<Vec<_>>();

        assert_eq!(
            actual,
            vec![
                (
                    "linked-directory/".to_owned(),
                    EntryKind::Directory,
                    inbox.0.join("linked-directory"),
                ),
                (
                    "linked-file".to_owned(),
                    EntryKind::File,
                    inbox.0.join("linked-file"),
                ),
            ]
        );
    }

    #[test]
    fn missing_inbox_returns_a_contextual_error() {
        let inbox = TestDirectory::new();
        let missing = inbox.0.join("missing");

        let error = scan_inbox(&missing).expect_err("missing inbox should fail");

        assert!(error.to_string().contains("failed to read Downloads inbox"));
        assert!(
            error
                .to_string()
                .contains(missing.to_string_lossy().as_ref())
        );
    }
}
