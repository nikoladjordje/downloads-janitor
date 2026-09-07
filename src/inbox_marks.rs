use std::{collections::BTreeMap, path::PathBuf};

use crate::{inbox::InboxEntry, move_execution::SourceIdentity};

type Marks = BTreeMap<PathBuf, SourceIdentity>;

#[derive(Default)]
pub struct InboxMarks {
    marks: Marks,
    visual: Option<(usize, Marks)>,
}

impl InboxMarks {
    pub fn count(&self) -> usize {
        self.marks.len()
    }

    pub fn contains(&self, entry: &InboxEntry) -> bool {
        self.marks.get(entry.path()).copied() == entry.identity() && entry.identity().is_some()
    }

    pub fn visual(&self) -> bool {
        self.visual.is_some()
    }

    pub fn exit_visual(&mut self) {
        self.visual = None;
    }

    pub fn clear(&mut self) {
        self.exit_visual();
        self.marks.clear();
    }

    pub fn toggle(&mut self, entry: &InboxEntry) {
        self.exit_visual();
        if self.contains(entry) {
            self.marks.remove(entry.path());
        } else if let Some(identity) = entry.identity() {
            self.marks.insert(entry.path().to_path_buf(), identity);
        }
    }

    pub fn toggle_all(&mut self, entries: &[InboxEntry]) {
        self.exit_visual();
        if entries.iter().all(|entry| self.contains(entry)) {
            self.clear();
        } else {
            for entry in entries {
                if let Some(identity) = entry.identity() {
                    self.marks.insert(entry.path().to_path_buf(), identity);
                }
            }
        }
    }

    pub fn toggle_visual(&mut self, cursor: Option<usize>, entries: &[InboxEntry]) {
        if self.visual() {
            self.exit_visual();
        } else if let Some(anchor) = cursor {
            self.visual = Some((anchor, self.marks.clone()));
            self.extend(cursor, entries);
        }
    }

    pub fn extend(&mut self, cursor: Option<usize>, entries: &[InboxEntry]) {
        if let (Some((anchor, baseline)), Some(cursor)) = (&self.visual, cursor) {
            self.marks = baseline.clone();
            for entry in &entries[(*anchor).min(cursor)..=(*anchor).max(cursor)] {
                if let Some(identity) = entry.identity() {
                    self.marks.insert(entry.path().to_path_buf(), identity);
                }
            }
        }
    }

    pub fn retain_present(&mut self, entries: &[InboxEntry]) {
        self.exit_visual();
        self.marks.retain(|path, identity| {
            entries
                .iter()
                .any(|entry| entry.path() == path && entry.identity() == Some(*identity))
        });
    }
}
