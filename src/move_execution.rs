use std::{
    ffi::CString,
    fmt, fs, io,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};

use crate::{
    inbox::InboxEntry,
    proposed_move::{ProposedEntryType, ProposedMove},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceIdentity {
    device: u64,
    inode: u64,
    file_type: u32,
}

#[derive(Debug)]
pub enum MoveError {
    NotRegularFile,
    SourceChanged,
    Validation(String),
    Collision,
    CrossFilesystem,
    Filesystem(io::Error),
    InvalidPath,
}

impl fmt::Display for MoveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRegularFile => formatter.write_str("only regular files can be moved yet"),
            Self::SourceChanged => {
                formatter.write_str("the source identity or entry type changed after review")
            }
            Self::Validation(reason) => write!(formatter, "fresh validation failed: {reason}"),
            Self::Collision => {
                formatter.write_str("the resulting path already exists; nothing was overwritten")
            }
            Self::CrossFilesystem => formatter.write_str(
                "cross-filesystem moves are unsupported; no copy or deletion was performed",
            ),
            Self::Filesystem(error) => {
                write!(formatter, "the operating system refused the move: {error}")
            }
            Self::InvalidPath => {
                formatter.write_str("a move path contains an unsupported NUL byte")
            }
        }
    }
}

impl SourceIdentity {
    pub fn capture(proposal: &ProposedMove) -> Result<Self, MoveError> {
        if proposal.entry_type() != ProposedEntryType::File {
            return Err(MoveError::NotRegularFile);
        }
        let metadata = fs::symlink_metadata(proposal.source()).map_err(MoveError::Filesystem)?;
        if !metadata.file_type().is_file() {
            return Err(MoveError::SourceChanged);
        }
        Ok(Self::from_metadata(&metadata))
    }

    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            file_type: metadata.mode() & libc::S_IFMT,
        }
    }
}

pub fn execute_regular_file(
    reviewed: &ProposedMove,
    entry: &InboxEntry,
    identity: SourceIdentity,
) -> Result<(), MoveError> {
    let fresh = ProposedMove::new(entry, reviewed.destination())
        .ok_or_else(|| MoveError::Validation("the source has no basename".to_owned()))?;
    if !fresh.is_valid() {
        let reasons = fresh
            .failures()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(MoveError::Validation(reasons));
    }
    if fresh.entry_type() != ProposedEntryType::File
        || fresh.source() != reviewed.source()
        || fresh.resulting_path() != reviewed.resulting_path()
    {
        return Err(MoveError::SourceChanged);
    }

    let metadata = fs::symlink_metadata(fresh.source()).map_err(MoveError::Filesystem)?;
    if !metadata.file_type().is_file() || SourceIdentity::from_metadata(&metadata) != identity {
        return Err(MoveError::SourceChanged);
    }

    rename_noreplace(fresh.source(), fresh.resulting_path())
}

fn rename_noreplace(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), MoveError> {
    let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| MoveError::InvalidPath)?;
    let destination =
        CString::new(destination.as_os_str().as_bytes()).map_err(|_| MoveError::InvalidPath)?;
    // SAFETY: both pointers refer to live, NUL-terminated C strings for this call.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }

    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EEXIST) => Err(MoveError::Collision),
        Some(libc::EXDEV) => Err(MoveError::CrossFilesystem),
        _ => Err(MoveError::Filesystem(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::MetadataExt,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        inbox::{EntryKind, InboxEntry},
        proposed_move::ProposedMove,
    };

    use super::{MoveError, SourceIdentity, execute_regular_file, rename_noreplace};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "downloads-janitor-move-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn moves_a_regular_file_without_changing_contents() {
        let root = root();
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::write(&source, b"contents").unwrap();
        fs::create_dir(&destination).unwrap();
        let entry = InboxEntry::test_entry(source.clone(), EntryKind::File, false);
        let proposal = ProposedMove::new(&entry, &destination).unwrap();
        let identity = SourceIdentity::capture(&proposal).unwrap();

        execute_regular_file(&proposal, &entry, identity).unwrap();

        assert!(!source.exists());
        assert_eq!(
            fs::read(destination.join("source.txt")).unwrap(),
            b"contents"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_source_replacement() {
        let root = root();
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::write(&source, b"original").unwrap();
        fs::create_dir(&destination).unwrap();
        let entry = InboxEntry::test_entry(source.clone(), EntryKind::File, false);
        let proposal = ProposedMove::new(&entry, &destination).unwrap();
        let identity = SourceIdentity::capture(&proposal).unwrap();
        fs::rename(&source, root.join("original.txt")).unwrap();
        fs::write(&source, b"replacement").unwrap();

        assert!(matches!(
            execute_regular_file(&proposal, &entry, identity),
            Err(MoveError::SourceChanged)
        ));
        assert_eq!(fs::read(&source).unwrap(), b"replacement");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_collision_refusal_preserves_both_files() {
        let root = root();
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::write(&source, b"source").unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("source.txt"), b"collision").unwrap();

        assert!(matches!(
            rename_noreplace(&source, &destination.join("source.txt")),
            Err(MoveError::Collision)
        ));
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(
            fs::read(destination.join("source.txt")).unwrap(),
            b"collision"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cross_filesystem_move_is_rejected_without_copy_or_deletion_when_available() {
        let root = root();
        let source = root.join("source.txt");
        fs::write(&source, b"source").unwrap();
        let Some(other_root) = ["/dev/shm", "/run/shm"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| {
                path.is_dir()
                    && fs::metadata(path)
                        .ok()
                        .is_some_and(|m| m.dev() != fs::metadata(&root).unwrap().dev())
            })
        else {
            fs::remove_dir_all(root).unwrap();
            return;
        };
        let destination = other_root.join(format!(
            "downloads-janitor-cross-device-{}",
            std::process::id()
        ));

        let result = rename_noreplace(&source, &destination);

        assert!(matches!(result, Err(MoveError::CrossFilesystem)));
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert!(!destination.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
