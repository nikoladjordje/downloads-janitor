use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
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
}
impl HistoryAction {
    fn encoded(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Rename => "rename",
            Self::Trash => "trash",
        }
    }
    fn decode(value: &str) -> Option<Self> {
        match value {
            "move" => Some(Self::Move),
            "rename" => Some(Self::Rename),
            "trash" => Some(Self::Trash),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rename => "Rename",
            Self::Trash => "Trash",
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
        true
    }
}

pub struct History {
    path: PathBuf,
    records: Vec<HistoryRecord>,
    warning: Option<String>,
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
        self.records.last()
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
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        self.records.push(HistoryRecord {
            id: format!(
                "{timestamp_ms}-{}",
                NEXT_RECORD_ID.fetch_add(1, Ordering::Relaxed)
            ),
            timestamp_ms,
            action,
            source,
            current,
            identity,
        });
        if self.records.len() > RECORD_LIMIT {
            self.records.drain(..self.records.len() - RECORD_LIMIT);
        }
        if let Err(error) = self.save() {
            self.reload();
            return Err(error);
        }
        Ok(())
    }
    fn reload(&mut self) {
        match read(&self.path).and_then(|contents| decode(&contents)) {
            Ok(records) => {
                self.records = records;
                self.warning = None;
            }
            Err(error) => {
                self.records.clear();
                self.warning = Some(format!("History unavailable: {error}"));
            }
        }
    }
    fn save(&self) -> io::Result<()> {
        let parent = self.path.parent().expect("history path has a parent");
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(
            ".history-{}-{}.tmp",
            std::process::id(),
            self.records.len()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&encode(&self.records))?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
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
            "record {} {} {} {} {} {device} {inode} {file_type}\n",
            record.id,
            record.timestamp_ms,
            record.action.encoded(),
            hex(record.source.as_os_str().as_bytes()),
            hex(record.current.as_os_str().as_bytes())
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
        let [
            "record",
            id,
            timestamp,
            action,
            source,
            current,
            device,
            inode,
            file_type,
        ] = fields.as_slice()
        else {
            return Err(invalid("expected a History record"));
        };
        let path = |value: &&str| {
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
            id: (*id).to_owned(),
            timestamp_ms: timestamp
                .parse()
                .map_err(|_| invalid("timestamp is invalid"))?,
            action: HistoryAction::decode(action).ok_or_else(|| invalid("action is invalid"))?,
            source: path(source)?,
            current: path(current)?,
            identity,
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
    use super::{History, HistoryAction};
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
}
