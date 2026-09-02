use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::inbox::{EntryKind, InboxEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposedEntryType {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationFailure {
    DestinationMissing,
    DestinationNotDirectory,
    ResultExists,
    DirectoryInsideItself,
    SourceMissing,
    SourceEqualsResult,
}

impl fmt::Display for ValidationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::DestinationMissing => "the Destination no longer exists or cannot be inspected",
            Self::DestinationNotDirectory => "the Destination is not a real directory",
            Self::ResultExists => "the resulting path already exists or cannot be inspected",
            Self::DirectoryInsideItself => {
                "a directory cannot be placed inside itself or one of its descendants"
            }
            Self::SourceMissing => "the source no longer exists or cannot be inspected",
            Self::SourceEqualsResult => "the source and resulting path are identical",
        };
        formatter.write_str(reason)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProposedMove {
    entry_type: ProposedEntryType,
    source: PathBuf,
    destination: PathBuf,
    resulting_path: PathBuf,
    failures: Vec<ValidationFailure>,
}

impl ProposedMove {
    pub fn new(entry: &InboxEntry, destination: &Path) -> Option<Self> {
        let basename = entry.path().file_name()?;
        let entry_type = entry_type(entry);
        let source = entry.path().to_path_buf();
        let destination = destination.to_path_buf();
        let resulting_path = destination.join(basename);
        let failures = validate(
            entry_type,
            source.as_path(),
            destination.as_path(),
            resulting_path.as_path(),
        );

        Some(Self {
            entry_type,
            source,
            destination,
            resulting_path,
            failures,
        })
    }

    pub fn entry_type(&self) -> ProposedEntryType {
        self.entry_type
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn resulting_path(&self) -> &Path {
        &self.resulting_path
    }

    pub fn is_valid(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[ValidationFailure] {
        &self.failures
    }
}

fn entry_type(entry: &InboxEntry) -> ProposedEntryType {
    if entry.is_symlink() {
        ProposedEntryType::Symlink
    } else if entry.kind() == EntryKind::Directory {
        ProposedEntryType::Directory
    } else {
        ProposedEntryType::File
    }
}

fn validate(
    entry_type: ProposedEntryType,
    source: &Path,
    destination: &Path,
    resulting_path: &Path,
) -> Vec<ValidationFailure> {
    let mut failures = Vec::new();
    let source_exists = metadata_exists(source, &mut failures, ValidationFailure::SourceMissing);
    let destination_metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => Some(metadata),
        Err(_) => {
            failures.push(ValidationFailure::DestinationMissing);
            None
        }
    };
    if destination_metadata
        .as_ref()
        .is_some_and(|metadata| !metadata.is_dir())
    {
        failures.push(ValidationFailure::DestinationNotDirectory);
    }

    match fs::symlink_metadata(resulting_path) {
        Ok(_) => failures.push(ValidationFailure::ResultExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => failures.push(ValidationFailure::ResultExists),
    }

    if source == resulting_path {
        failures.push(ValidationFailure::SourceEqualsResult);
    }

    if entry_type == ProposedEntryType::Directory
        && source_exists
        && destination_metadata.is_some_and(|metadata| metadata.is_dir())
        && directory_contains_destination(source, destination)
    {
        failures.push(ValidationFailure::DirectoryInsideItself);
    }

    failures
}

fn metadata_exists(
    path: &Path,
    failures: &mut Vec<ValidationFailure>,
    failure: ValidationFailure,
) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(_) => {
            failures.push(failure);
            false
        }
    }
}

fn directory_contains_destination(source: &Path, destination: &Path) -> bool {
    match (fs::canonicalize(source), fs::canonicalize(destination)) {
        (Ok(source), Ok(destination)) => destination.starts_with(source),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::inbox::{EntryKind, InboxEntry};

    use super::{ProposedEntryType, ProposedMove, ValidationFailure};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "downloads-janitor-proposal-{}-{sequence}",
                std::process::id()
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

    fn proposal(
        source: &Path,
        kind: EntryKind,
        is_symlink: bool,
        destination: &Path,
    ) -> ProposedMove {
        ProposedMove::new(
            &InboxEntry::test_entry(source.to_path_buf(), kind, is_symlink),
            destination,
        )
        .unwrap()
    }

    #[test]
    fn missing_destination_is_invalid() {
        let root = TestDirectory::new();
        let source = root.0.join("source.txt");
        File::create(&source).unwrap();

        let proposal = proposal(&source, EntryKind::File, false, &root.0.join("missing"));

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::DestinationMissing)
        );
    }

    #[test]
    fn non_directory_destination_is_invalid() {
        let root = TestDirectory::new();
        let source = root.0.join("source.txt");
        let destination = root.0.join("not-a-directory");
        File::create(&source).unwrap();
        File::create(&destination).unwrap();

        let proposal = proposal(&source, EntryKind::File, false, &destination);

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::DestinationNotDirectory)
        );
    }

    #[test]
    fn existing_result_is_invalid() {
        let root = TestDirectory::new();
        let source = root.0.join("source.txt");
        let destination = root.0.join("destination");
        File::create(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        File::create(destination.join("source.txt")).unwrap();

        let proposal = proposal(&source, EntryKind::File, false, &destination);

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::ResultExists)
        );
    }

    #[test]
    fn directory_inside_itself_or_descendant_is_invalid() {
        let root = TestDirectory::new();
        let source = root.0.join("source");
        let destination = source.join("descendant");
        fs::create_dir_all(&destination).unwrap();

        let proposal = proposal(&source, EntryKind::Directory, false, &destination);

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::DirectoryInsideItself)
        );
    }

    #[test]
    fn missing_source_is_invalid() {
        let root = TestDirectory::new();
        let destination = root.0.join("destination");
        fs::create_dir(&destination).unwrap();

        let proposal = proposal(
            &root.0.join("missing.txt"),
            EntryKind::File,
            false,
            &destination,
        );

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::SourceMissing)
        );
    }

    #[test]
    fn identical_source_and_result_are_invalid() {
        let root = TestDirectory::new();
        let source = root.0.join("source.txt");
        File::create(&source).unwrap();

        let proposal = proposal(&source, EntryKind::File, false, &root.0);

        assert!(
            proposal
                .failures()
                .contains(&ValidationFailure::SourceEqualsResult)
        );
    }

    #[cfg(unix)]
    #[test]
    fn valid_file_directory_and_symlink_proposals_preserve_identity_and_basename() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new();
        let destination = root.0.join("destination");
        fs::create_dir(&destination).unwrap();
        let file = root.0.join("file.txt");
        let directory = root.0.join("directory");
        let link = root.0.join("link");
        File::create(&file).unwrap();
        fs::create_dir(&directory).unwrap();
        symlink(&file, &link).unwrap();

        for (source, kind, is_symlink, expected_type) in [
            (&file, EntryKind::File, false, ProposedEntryType::File),
            (
                &directory,
                EntryKind::Directory,
                false,
                ProposedEntryType::Directory,
            ),
            (&link, EntryKind::File, true, ProposedEntryType::Symlink),
        ] {
            let proposal = proposal(source, kind, is_symlink, &destination);
            assert!(proposal.is_valid());
            assert_eq!(proposal.entry_type(), expected_type);
            assert_eq!(proposal.source(), source);
            assert_eq!(
                proposal.resulting_path(),
                destination.join(source.file_name().unwrap())
            );
        }
    }
}
