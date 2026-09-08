use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::{
    inbox::InboxEntry,
    move_execution::{self, SourceIdentity},
    proposed_move::ProposedMove,
};

#[derive(Debug, Eq, PartialEq)]
pub enum EntryOutcome {
    Unattempted,
    Completed,
    Failed(String),
}

pub struct BatchEntry {
    pub entry: InboxEntry,
    pub proposal: Option<ProposedMove>,
    pub problems: Vec<String>,
    pub outcome: EntryOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchAction {
    Move,
    Trash(PathBuf),
    Delete,
}

pub struct Batch {
    pub entries: Vec<BatchEntry>,
    home: PathBuf,
    pub action: BatchAction,
    pub running: bool,
    pub stopped: bool,
    finished: bool,
}

impl Batch {
    pub fn new(mut entries: Vec<InboxEntry>, destination: &Path, home: &Path) -> Self {
        entries.sort_by(|a, b| a.path().cmp(b.path()));
        let mut batch = Self {
            entries: entries
                .into_iter()
                .map(|entry| BatchEntry {
                    proposal: ProposedMove::new(&entry, destination),
                    entry,
                    problems: Vec::new(),
                    outcome: EntryOutcome::Unattempted,
                })
                .collect(),
            action: BatchAction::Move,
            home: home.to_path_buf(),
            running: false,
            stopped: false,
            finished: false,
        };
        batch.preflight();
        batch
    }

    pub fn removal(mut entries: Vec<InboxEntry>, action: BatchAction) -> Self {
        assert!(action != BatchAction::Move);
        entries.sort_by(|a, b| a.path().cmp(b.path()));
        let mut batch = Self {
            entries: entries
                .into_iter()
                .map(|entry| BatchEntry {
                    entry,
                    proposal: None,
                    problems: Vec::new(),
                    outcome: EntryOutcome::Unattempted,
                })
                .collect(),
            home: PathBuf::new(),
            action,
            running: false,
            stopped: false,
            finished: false,
        };
        batch.preflight();
        batch
    }

    fn preflight(&mut self) {
        for item in &mut self.entries {
            item.problems.clear();
            match &self.action {
                BatchAction::Trash(root) => {
                    if let Err(error) = crate::trash::TrashReview::new(&item.entry)
                        .and_then(|review| review.preflight(root))
                    {
                        item.problems.push(error);
                    }
                }
                BatchAction::Delete => {
                    if let Err(error) = crate::permanent_delete::DeleteReview::new(&item.entry)
                        .and_then(|review| review.preflight())
                    {
                        item.problems.push(error);
                    }
                }
                BatchAction::Move => {
                    let destination = item
                        .proposal
                        .as_ref()
                        .expect("move has proposal")
                        .destination()
                        .to_owned();
                    let proposal = ProposedMove::new(&item.entry, &destination)
                        .expect("Inbox entry has basename");
                    item.problems = proposal
                        .failures()
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    if SourceIdentity::capture(&proposal).ok() != item.entry.identity()
                        || item.entry.identity().is_none()
                    {
                        item.problems.push(
                            "the source identity changed or is unavailable; refresh Inbox".into(),
                        );
                    }
                    if let (Ok(source), Ok(destination)) = (
                        fs::symlink_metadata(item.entry.path()),
                        fs::symlink_metadata(&destination),
                    ) && source.dev() != destination.dev()
                    {
                        item.problems
                            .push("cross-filesystem moves are unsupported".into());
                    }
                    if let Err(error) = validate_destination(&self.home, &destination) {
                        item.problems.push(error);
                    }
                    item.proposal = Some(proposal);
                }
            }
        }
    }

    pub fn valid(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|item| item.problems.is_empty())
    }

    pub fn authorize(&mut self) {
        if self.finished || self.running {
            return;
        }
        self.preflight();
        self.running = self.valid();
    }

    // Exactly one entry per foreground turn: the UI renders and polls Esc between turns.
    pub fn step(&mut self) {
        if !self.running {
            return;
        }
        let Some(item) = self
            .entries
            .iter_mut()
            .find(|item| item.outcome == EntryOutcome::Unattempted)
        else {
            self.finish();
            return;
        };
        let result = match &self.action {
            BatchAction::Trash(root) => {
                crate::trash::TrashReview::new(&item.entry).and_then(|review| review.execute(root))
            }
            BatchAction::Delete => crate::permanent_delete::DeleteReview::new(&item.entry)
                .and_then(|review| review.execute()),
            BatchAction::Move => {
                let proposal = item.proposal.as_ref().expect("move has proposal");
                validate_destination(&self.home, proposal.destination()).and_then(|()| {
                    move_execution::execute_move(
                        proposal,
                        &item.entry,
                        item.entry.identity().expect("preflight checked identity"),
                    )
                    .map_err(|error| error.to_string())
                })
            }
        };
        match result {
            Ok(()) => item.outcome = EntryOutcome::Completed,
            Err(error) => {
                item.outcome = EntryOutcome::Failed(error);
                self.finish();
                return;
            }
        }
        if self
            .entries
            .iter()
            .all(|item| item.outcome == EntryOutcome::Completed)
        {
            self.finish();
        }
    }

    pub fn stop(&mut self) {
        if self.running {
            self.stopped = true;
            self.finish();
        }
    }

    fn finish(&mut self) {
        self.running = false;
        self.finished = true;
    }

    pub fn summary(&self) -> String {
        let completed = self
            .entries
            .iter()
            .filter(|item| item.outcome == EntryOutcome::Completed)
            .count();
        let failed = self
            .entries
            .iter()
            .filter(|item| matches!(item.outcome, EntryOutcome::Failed(_)))
            .count();
        format!(
            "{completed} completed, {failed} failed, {} unattempted{}",
            self.entries.len() - completed - failed,
            if self.stopped { " — stopped" } else { "" }
        )
    }
}

fn validate_destination(home: &Path, destination: &Path) -> Result<(), String> {
    let relative = destination
        .strip_prefix(home)
        .map_err(|_| "Destination is outside HOME")?;
    let mut path = home.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err("Destination is outside HOME".into());
            }
            path.push(component);
        }
        if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(
                "Destination and its ancestors must be real directories beneath HOME".into(),
            );
        }
    }
    Ok(())
}

// A read-only preflight check. Kernel execution remains authoritative for ACLs,
// concurrent permission changes, immutable flags, and recursive child failures.
pub(crate) fn check_removal_parent(source: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let parent = source.parent().ok_or("Source has no parent")?;
    let path =
        std::ffi::CString::new(parent.as_os_str().as_bytes()).map_err(|_| "Invalid parent path")?;
    // SAFETY: path is a live NUL-terminated string; access does not mutate it.
    if unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) } != 0 {
        return Err(format!(
            "Cannot remove entry from {parent:?}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
