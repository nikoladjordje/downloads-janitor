use std::{fs, io, os::unix::fs::MetadataExt, path::Path};

use crate::{
    history::HistoryRecord,
    move_execution::{self, MoveError, SourceIdentity},
};

pub fn validate(record: &HistoryRecord) -> Result<(), MoveError> {
    if !record.supports_undo() {
        return Err(MoveError::Validation(
            "this action cannot be undone".to_owned(),
        ));
    }
    let metadata = fs::symlink_metadata(record.current()).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => {
            MoveError::Validation("the current entry no longer exists".to_owned())
        }
        _ => MoveError::Filesystem(error),
    })?;
    if SourceIdentity::from_metadata(&metadata) != record.identity() {
        return Err(MoveError::SourceChanged);
    }
    validate_original_path(record.source())?;
    let current_device = SourceIdentity::from_metadata(&metadata).parts().0;
    let parent_device = fs::metadata(
        record
            .source()
            .parent()
            .expect("history source has a parent"),
    )
    .map_err(MoveError::Filesystem)?
    .dev();
    if current_device != parent_device {
        return Err(MoveError::CrossFilesystem);
    }
    Ok(())
}

pub fn execute(record: &HistoryRecord) -> Result<(), MoveError> {
    validate(record)?;
    move_execution::rename_noreplace(record.current(), record.source())
}

fn validate_original_path(path: &Path) -> Result<(), MoveError> {
    let Some(parent) = path.parent() else {
        return Err(MoveError::Validation(
            "the original path has no parent directory".to_owned(),
        ));
    };
    let parent_metadata = fs::metadata(parent).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => {
            MoveError::Validation("the original parent directory no longer exists".to_owned())
        }
        _ => MoveError::Filesystem(error),
    })?;
    if !parent_metadata.is_dir() {
        return Err(MoveError::Validation(
            "the original parent is not a directory".to_owned(),
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(_) => Err(MoveError::Collision),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MoveError::Filesystem(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        history::{History, HistoryAction},
        move_execution::MoveError,
    };

    use super::validate;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-undo-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn validation_refuses_missing_replaced_and_colliding_undo_sources() {
        let root = root();
        let source = root.join("original");
        let current = root.join("current");
        fs::write(&current, b"recorded").unwrap();
        let mut history = History::load(root.clone());
        history
            .record(HistoryAction::Move, source.clone(), current.clone())
            .unwrap();
        let record = history.records()[0].clone();

        fs::remove_file(&current).unwrap();
        assert!(
            validate(&record)
                .unwrap_err()
                .to_string()
                .contains("no longer exists")
        );
        fs::write(&current, b"replacement").unwrap();
        assert!(matches!(validate(&record), Err(MoveError::SourceChanged)));
        fs::remove_file(&current).unwrap();
        fs::write(&current, b"recorded").unwrap();
        history = History::load(root.clone());
        history
            .record(HistoryAction::Move, source.clone(), current.clone())
            .unwrap();
        let fresh_record = history.records()[1].clone();
        fs::write(&source, b"collision").unwrap();
        assert!(
            validate(&fresh_record)
                .unwrap_err()
                .to_string()
                .contains("already exists")
        );
        fs::remove_file(&source).unwrap();
        fs::remove_file(&current).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
