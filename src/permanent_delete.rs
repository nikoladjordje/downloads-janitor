use std::{fs, path::PathBuf};

use crate::{inbox::InboxEntry, move_execution::SourceIdentity};

pub struct DeleteReview {
    pub source: PathBuf,
    identity: SourceIdentity,
}

impl DeleteReview {
    pub fn new(entry: &InboxEntry) -> Result<Self, String> {
        let review = Self {
            source: entry.path().to_owned(),
            identity: entry
                .identity()
                .ok_or("Source identity unavailable; refresh Inbox")?,
        };
        review.validate()?;
        Ok(review)
    }

    fn validate(&self) -> Result<fs::Metadata, String> {
        let metadata = fs::symlink_metadata(&self.source).map_err(|e| e.to_string())?;
        if SourceIdentity::from_metadata(&metadata) != self.identity {
            return Err(
                "Source identity or entry type changed after review; cancel and refresh Inbox"
                    .into(),
            );
        }
        Ok(metadata)
    }

    pub fn preflight(&self) -> Result<(), String> {
        self.validate()?;
        crate::batch::check_removal_parent(&self.source)
    }

    pub fn execute(&self) -> Result<(), String> {
        let metadata = self.validate()?;
        if metadata.is_dir() {
            // On Linux the standard library removes recursively without following symlinks.
            fs::remove_dir_all(&self.source).map_err(|error| format!(
                "{error}; some directory contents may already have been permanently deleted. No rollback is possible"
            ))
        } else {
            // Includes Symlink Entries, even if their targets have disappeared.
            fs::remove_file(&self.source).map_err(|error| error.to_string())
        }
    }
}
