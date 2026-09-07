use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::OpenOptionsExt,
        },
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{inbox::InboxEntry, move_execution::SourceIdentity};

type Records = BTreeMap<PathBuf, SourceIdentity>;
const HEADER: &str = "downloads-janitor-ignored-v1\n";

pub struct IgnoredEntries {
    path: PathBuf,
    records: Records,
    original: Option<Vec<u8>>,
    warning: Option<String>,
}

impl IgnoredEntries {
    pub fn load(home: &Path) -> Self {
        let path = home.join(".local/state/downloads-janitor/ignored-v1");
        let mut state = Self {
            path,
            records: Records::new(),
            original: None,
            warning: None,
        };
        match read_state(&state.path) {
            Ok(bytes) => match bytes.as_deref().map(decode).transpose() {
                Ok(records) => {
                    state.records = records.unwrap_or_default();
                    state.original = bytes;
                }
                Err(error) => state.warning = Some(error.to_string()),
            },
            Err(error) => state.warning = Some(error.to_string()),
        }
        if let Some(reason) = state.warning.take() {
            state.warning = Some(format!(
                "Ignored state unreadable; showing all entries. {reason}. Preserve or repair {:?}, then R to reload",
                state.path
            ));
        }
        state
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn contains(&self, entry: &InboxEntry) -> bool {
        entry.identity().is_some() && self.records.get(entry.path()).copied() == entry.identity()
    }

    /// Commit the entire set together. Visibility may change only after this succeeds.
    pub fn update(&mut self, entries: &[&InboxEntry], ignore: bool) -> io::Result<Option<String>> {
        if self.warning.is_some() {
            return Err(io::Error::other(
                "ignored state is unreadable; repair it and refresh before saving",
            ));
        }
        let mut records = self.records.clone();
        for entry in entries {
            let identity = entry
                .identity()
                .ok_or_else(|| io::Error::other("entry identity unavailable"))?;
            let current = SourceIdentity::from_metadata(&fs::symlink_metadata(entry.path())?);
            if current != identity {
                return Err(io::Error::other(
                    "entry identity changed; refresh before retrying",
                ));
            }
            if ignore {
                records.insert(entry.path().to_path_buf(), identity);
            } else {
                records.remove(entry.path());
            }
        }
        let bytes = encode(&records);
        let parent = self.path.parent().expect("state has a parent");
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(parent.join("ignored.lock"))?;
        // SAFETY: lock owns a valid descriptor; the lock is released on close.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::other(
                "another session is saving ignored state; retry",
            ));
        }
        if read_state(&self.path)? != self.original {
            return Err(io::Error::other(
                "ignored state changed in another session; press R before retrying",
            ));
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = parent.join(format!(
            ".ignored-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        // The rename has committed even if directory durability cannot be confirmed.
        self.records = records;
        self.original = Some(bytes);
        let durability_warning = fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .err()
            .map(|error| {
                format!("State saved, but crash durability could not be confirmed: {error}")
            });
        Ok(durability_warning)
    }
}

fn read_state(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            Err(io::Error::other("ignored state is not a regular file"))
        }
        Ok(_) => fs::read(path).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn encode(records: &Records) -> Vec<u8> {
    let mut text = HEADER.to_owned();
    for (path, identity) in records {
        let hex = path
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let (device, inode, kind) = identity.parts();
        text.push_str(&format!("{hex} {device} {inode} {kind}\n"));
    }
    text.into_bytes()
}

fn decode(bytes: &[u8]) -> io::Result<Records> {
    let invalid = || io::Error::other("invalid ignored-state format");
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let body = text.strip_prefix(HEADER).ok_or_else(invalid)?;
    if !body.is_empty() && !body.ends_with('\n') {
        return Err(invalid());
    }
    let mut records = Records::new();
    for line in body.lines() {
        let fields = line.split(' ').collect::<Vec<_>>();
        if fields.len() != 4 || fields[0].len() % 2 != 0 || !fields[0].is_ascii() {
            return Err(invalid());
        }
        let raw = (0..fields[0].len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&fields[0][index..index + 2], 16).map_err(|_| invalid())
            })
            .collect::<io::Result<Vec<_>>>()?;
        if raw.contains(&0) {
            return Err(invalid());
        }
        let path = PathBuf::from(OsString::from_vec(raw));
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(invalid());
        }
        let identity = SourceIdentity::from_parts(
            fields[1].parse().map_err(|_| invalid())?,
            fields[2].parse().map_err(|_| invalid())?,
            fields[3].parse().map_err(|_| invalid())?,
        )
        .ok_or_else(invalid)?;
        if records.insert(path, identity).is_some() {
            return Err(invalid());
        }
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_preserves_raw_paths_and_rejects_corrupt_or_truncated_records() {
        let identity = SourceIdentity::from_parts(1, 2, libc::S_IFLNK).unwrap();
        let path = PathBuf::from(OsString::from_vec(
            b"/home/test/Downloads/raw-\xff\n".to_vec(),
        ));
        let records = BTreeMap::from([(path, identity)]);
        let encoded = encode(&records);
        assert_eq!(decode(&encoded).unwrap(), records);
        for end in 0..encoded.len() {
            if end == HEADER.len() {
                continue;
            } // a valid empty state
            assert!(decode(&encoded[..end]).is_err(), "truncation at {end}");
        }
        let duplicated = format!(
            "{}{}",
            String::from_utf8(encoded.clone()).unwrap(),
            String::from_utf8(encoded)
                .unwrap()
                .strip_prefix(HEADER)
                .unwrap()
        );
        assert!(decode(duplicated.as_bytes()).is_err());
        for bad in [
            "2f6100 1 2 32768\n", // NUL
            "61 1 2 32768\n",     // relative path
            "2f61 1 2 0\n",       // unsupported identity type
            "zz 1 2 32768\n",
        ] {
            assert!(decode(format!("{HEADER}{bad}").as_bytes()).is_err());
        }
    }
}
