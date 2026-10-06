use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::move_execution::SourceIdentity;

const HEADER: &str = "downloads-janitor-history-v1\n";
const RECORD_LIMIT: usize = 100;
static NEXT_RECORD_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryAction {
    Move,
    Rename,
    Trash,
    Delete,
}
impl HistoryAction {
    fn encoded(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Rename => "rename",
            Self::Trash => "trash",
            Self::Delete => "delete",
        }
    }
    fn decode(value: &str) -> Option<Self> {
        match value {
            "move" => Some(Self::Move),
            "rename" => Some(Self::Rename),
            "trash" => Some(Self::Trash),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rename => "Rename",
            Self::Trash => "Trash",
            Self::Delete => "Permanent deletion",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryRecord {
    id: String,
    timestamp_ms: u128,
    action: HistoryAction,
    source: PathBuf,
    current: PathBuf,
    identity: SourceIdentity,
    reversed_at_ms: Option<u128>,
}
impl HistoryRecord {
    pub fn action(&self) -> HistoryAction {
        self.action
    }
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn current(&self) -> &Path {
        &self.current
    }
    pub fn reversible(&self) -> bool {
        self.action != HistoryAction::Delete
    }
    pub fn reversed(&self) -> bool {
        self.reversed_at_ms.is_some()
    }
    pub fn supports_undo(&self) -> bool {
        matches!(
            self.action,
            HistoryAction::Move | HistoryAction::Rename | HistoryAction::Trash
        )
    }
    pub(crate) fn identity(&self) -> SourceIdentity {
        self.identity
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

pub struct History {
    path: PathBuf,
    records: Vec<HistoryRecord>,
    warning: Option<String>,
}

#[derive(Debug)]
pub enum UndoCommitError {
    Changed,
    Operation(io::Error),
    Persist(io::Error),
    Unavailable(io::Error),
}

impl History {
    pub fn load(home: PathBuf) -> Self {
        let path = home.join(".local/state/downloads-janitor/history-v1");
        match read(&path).and_then(|contents| decode(&contents)) {
            Ok(records) => Self {
                path,
                records,
                warning: None,
            },
            Err(error) => Self {
                path,
                records: Vec::new(),
                warning: Some(format!("History unavailable: {error}")),
            },
        }
    }
    pub fn records(&self) -> &[HistoryRecord] {
        &self.records
    }
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
    pub fn newest_reversible(&self) -> Option<&HistoryRecord> {
        self.records
            .iter()
            .rfind(|record| record.reversible() && !record.reversed())
    }
    pub fn refresh(&mut self) -> io::Result<()> {
        self.load_from_disk()
    }
    fn load_from_disk(&mut self) -> io::Result<()> {
        match read(&self.path).and_then(|contents| decode(&contents)) {
            Ok(records) => {
                self.records = records;
                self.warning = None;
                Ok(())
            }
            Err(error) => {
                self.records.clear();
                self.warning = Some(format!("History unavailable: {error}"));
                Err(error)
            }
        }
    }
    pub fn execute_undo(
        &mut self,
        reviewed_id: &str,
        preview_latest_record: Option<&HistoryRecord>,
        operation: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), UndoCommitError> {
        if let Some(warning) = &self.warning {
            return Err(UndoCommitError::Unavailable(io::Error::other(
                warning.clone(),
            )));
        }
        let _lock = self.lock().map_err(UndoCommitError::Unavailable)?;
        let mut records = read(&self.path)
            .and_then(|contents| decode(&contents))
            .map_err(|error| {
                self.records.clear();
                self.warning = Some(format!("History unavailable: {error}"));
                UndoCommitError::Unavailable(error)
            })?;
        if records.last() != preview_latest_record
            || records
                .iter()
                .rfind(|record| record.reversible() && !record.reversed())
                .is_none_or(|record| record.id() != reviewed_id)
        {
            self.records = records;
            return Err(UndoCommitError::Changed);
        }
        operation().map_err(UndoCommitError::Operation)?;
        let record = records
            .iter_mut()
            .find(|record| record.id() == reviewed_id)
            .expect("reviewed newest History record exists");
        record.reversed_at_ms = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)
                .map_err(UndoCommitError::Persist)?
                .as_millis(),
        );
        save(&self.path, &records).map_err(UndoCommitError::Persist)?;
        self.records = records;
        self.warning = None;
        Ok(())
    }
    pub fn record(
        &mut self,
        action: HistoryAction,
        source: PathBuf,
        current: PathBuf,
    ) -> io::Result<()> {
        if let Some(warning) = &self.warning {
            return Err(io::Error::other(warning.clone()));
        }
        let identity = SourceIdentity::from_metadata(&fs::symlink_metadata(&current)?);
        self.record_with_identity(action, source, current, identity)
    }
    pub fn record_deletion(&mut self, source: PathBuf, identity: SourceIdentity) -> io::Result<()> {
        self.record_with_identity(HistoryAction::Delete, source.clone(), source, identity)
    }
    fn record_with_identity(
        &mut self,
        action: HistoryAction,
        source: PathBuf,
        current: PathBuf,
        identity: SourceIdentity,
    ) -> io::Result<()> {
        if let Some(warning) = &self.warning {
            return Err(io::Error::other(warning.clone()));
        }
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        self.update(|records| {
            records.push(HistoryRecord {
                id: format!(
                    "{timestamp_ms}-{}-{}",
                    std::process::id(),
                    NEXT_RECORD_ID.fetch_add(1, Ordering::Relaxed)
                ),
                timestamp_ms,
                action,
                source,
                current,
                identity,
                reversed_at_ms: None,
            });
            if records.len() > RECORD_LIMIT {
                records.drain(..records.len() - RECORD_LIMIT);
            }
            Ok(())
        })
    }
    pub fn clear(&mut self) -> io::Result<()> {
        self.update(|records| {
            records.clear();
            Ok(())
        })
    }
    fn update(
        &mut self,
        change: impl FnOnce(&mut Vec<HistoryRecord>) -> io::Result<()>,
    ) -> io::Result<()> {
        if let Some(warning) = &self.warning {
            return Err(io::Error::other(warning.clone()));
        }
        let _lock = self.lock()?;
        let mut records = match read(&self.path).and_then(|contents| decode(&contents)) {
            Ok(records) => records,
            Err(error) => {
                self.records.clear();
                self.warning = Some(format!("History unavailable: {error}"));
                return Err(error);
            }
        };
        change(&mut records)?;
        save(&self.path, &records)?;
        self.records = records;
        self.warning = None;
        Ok(())
    }
    fn lock(&self) -> io::Result<HistoryLock> {
        let parent = self.path.parent().expect("history path has a parent");
        fs::create_dir_all(parent)?;
        HistoryLock::acquire(&parent.join("history-v1.lock"))
    }
}

struct HistoryLock(File);
impl HistoryLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }
}
impl Drop for HistoryLock {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

fn save(path: &Path, records: &[HistoryRecord]) -> io::Result<()> {
    let parent = path.parent().expect("history path has a parent");
    let temporary = parent.join(format!(
        ".history-{}-{}-{}.tmp",
        std::process::id(),
        records.len(),
        NEXT_RECORD_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&encode(records))?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read(path: &Path) -> io::Result<Vec<u8>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path),
        Ok(_) => Err(io::Error::other("history is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(HEADER.as_bytes().to_vec()),
        Err(error) => Err(error),
    }
}
fn encode(records: &[HistoryRecord]) -> Vec<u8> {
    let mut text = HEADER.to_owned();
    for record in records {
        let (device, inode, file_type) = record.identity.parts();
        text.push_str(&format!(
            "record {} {} {} {} {} {device} {inode} {file_type} {}\n",
            record.id,
            record.timestamp_ms,
            record.action.encoded(),
            hex(record.source.as_os_str().as_bytes()),
            hex(record.current.as_os_str().as_bytes()),
            record
                .reversed_at_ms
                .map(|value| value.to_string())
                .as_deref()
                .unwrap_or("-")
        ));
    }
    text.into_bytes()
}
fn decode(bytes: &[u8]) -> io::Result<Vec<HistoryRecord>> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| io::Error::other("history is not valid UTF-8"))?;
    let body = text
        .strip_prefix(HEADER)
        .ok_or_else(|| io::Error::other("missing or unsupported history header"))?;
    if !body.is_empty() && !body.ends_with('\n') {
        return Err(io::Error::other(
            "final history line is not newline-terminated",
        ));
    }
    let mut records = Vec::new();
    for (index, line) in body.lines().enumerate() {
        let invalid = |problem: &str| io::Error::other(format!("line {}: {problem}", index + 2));
        let fields = line.split(' ').collect::<Vec<_>>();
        let (id, timestamp, action, source, current, device, inode, file_type, reversed_at) =
            match fields.as_slice() {
                [
                    "record",
                    id,
                    timestamp,
                    action,
                    source,
                    current,
                    device,
                    inode,
                    file_type,
                ] => (
                    *id, *timestamp, *action, *source, *current, *device, *inode, *file_type, "-",
                ),
                [
                    "record",
                    id,
                    timestamp,
                    action,
                    source,
                    current,
                    device,
                    inode,
                    file_type,
                    reversed_at,
                ] => (
                    *id,
                    *timestamp,
                    *action,
                    *source,
                    *current,
                    *device,
                    *inode,
                    *file_type,
                    *reversed_at,
                ),
                _ => return Err(invalid("expected a History record")),
            };
        let path = |value: &str| {
            unhex(value)
                .map(OsString::from_vec)
                .map(PathBuf::from)
                .map_err(|_| invalid("path is not hexadecimal"))
        };
        let identity = SourceIdentity::from_parts(
            device.parse().map_err(|_| invalid("device is invalid"))?,
            inode.parse().map_err(|_| invalid("inode is invalid"))?,
            file_type
                .parse()
                .map_err(|_| invalid("file type is invalid"))?,
        )
        .ok_or_else(|| invalid("file type is unsupported"))?;
        records.push(HistoryRecord {
            id: id.to_owned(),
            timestamp_ms: timestamp
                .parse()
                .map_err(|_| invalid("timestamp is invalid"))?,
            action: HistoryAction::decode(action).ok_or_else(|| invalid("action is invalid"))?,
            source: path(source)?,
            current: path(current)?,
            identity,
            reversed_at_ms: match reversed_at {
                "-" => None,
                value => Some(
                    value
                        .parse()
                        .map_err(|_| invalid("reversal time is invalid"))?,
                ),
            },
        });
    }
    Ok(records)
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
    use super::{History, HistoryAction, UndoCommitError, encode};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn records_completed_moves_across_restart_and_keeps_the_newest_hundred() {
        let root = root();
        for index in 0..101 {
            let source = root.join(format!("before-{index}"));
            let current = root.join(format!("after-{index}"));
            fs::write(&current, b"contents").unwrap();
            History::load(root.clone())
                .record(HistoryAction::Move, source, current)
                .unwrap();
        }
        let reloaded = History::load(root.clone());
        assert_eq!(reloaded.records().len(), 100);
        assert_eq!(reloaded.records()[0].source(), root.join("before-1"));
        assert_eq!(
            reloaded.newest_reversible().unwrap().action(),
            HistoryAction::Move
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reversed_record_persists_and_is_not_the_newest_undo_candidate() {
        let root = root();
        let source = root.join("before");
        let current = root.join("after");
        fs::write(&current, b"contents").unwrap();
        let mut history = History::load(root.clone());
        history
            .record(HistoryAction::Move, source, current)
            .unwrap();
        let id = history.records()[0].id().to_owned();

        let latest_record = history.records().last().cloned();
        history
            .execute_undo(&id, latest_record.as_ref(), || Ok(()))
            .unwrap();

        let reloaded = History::load(root.clone());
        assert!(reloaded.records()[0].reversed());
        assert!(reloaded.newest_reversible().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn serializes_independently_loaded_updates_and_can_clear_history() {
        let root = root();
        let first_current = root.join("first-current");
        let second_current = root.join("second-current");
        fs::write(&first_current, b"first").unwrap();
        fs::write(&second_current, b"second").unwrap();
        let mut first = History::load(root.clone());
        let mut second = History::load(root.clone());

        first
            .record(
                HistoryAction::Move,
                root.join("first-source"),
                first_current,
            )
            .unwrap();
        second
            .record(
                HistoryAction::Rename,
                root.join("second-source"),
                second_current,
            )
            .unwrap();

        let mut reloaded = History::load(root.clone());
        assert_eq!(reloaded.records().len(), 2);
        reloaded.clear().unwrap();
        assert!(History::load(root.clone()).records().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn undo_rejects_an_externally_rewritten_newest_record() {
        let root = root();
        let current = root.join("current");
        fs::write(&current, b"contents").unwrap();
        let mut history = History::load(root.clone());
        history
            .record(HistoryAction::Move, root.join("source"), current)
            .unwrap();
        let preview_latest = history.records().last().cloned();
        let mut externally_changed = history.records().to_vec();
        externally_changed[0].source = root.join("changed-source");
        fs::write(&history.path, encode(&externally_changed)).unwrap();

        let preview = preview_latest.as_ref().unwrap();
        assert!(matches!(
            history.execute_undo(preview.id(), Some(preview), || Ok(())),
            Err(UndoCommitError::Changed)
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
