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
    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot locate Downloads inbox because $HOME is not set",
        )
    })?;

    scan_inbox(&downloads_path(&home))
}

fn downloads_path(home: &OsStr) -> PathBuf {
    PathBuf::from(home).join("Downloads")
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
            let file_type = entry.file_type().ok()?;
            let kind = if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
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
    fn downloads_path_is_resolved_relative_to_home() {
        assert_eq!(
            downloads_path(std::ffi::OsStr::new("/home/someone")),
            PathBuf::from("/home/someone/Downloads")
        );
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
