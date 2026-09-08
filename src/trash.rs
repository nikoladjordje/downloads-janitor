use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

use crate::{
    inbox::InboxEntry,
    move_execution::{SourceIdentity, rename_noreplace},
};

pub struct TrashReview {
    pub source: PathBuf,
    identity: SourceIdentity,
}

impl TrashReview {
    pub fn new(entry: &InboxEntry) -> Result<Self, String> {
        let identity = entry
            .identity()
            .ok_or("Source identity is unavailable; refresh Inbox")?;
        let review = Self {
            source: entry.path().to_owned(),
            identity,
        };
        review.validate()?;
        Ok(review)
    }

    fn validate(&self) -> Result<(), String> {
        let metadata = fs::symlink_metadata(&self.source).map_err(|e| e.to_string())?;
        if SourceIdentity::from_metadata(&metadata) != self.identity {
            return Err(
                "Source identity or entry type changed after review; cancel and refresh Inbox"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn preflight(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        crate::batch::check_removal_parent(&self.source)?;
        let source = fs::canonicalize(self.source.parent().ok_or("Source has no parent")?)
            .map_err(|e| e.to_string())?
            .join(self.source.file_name().ok_or("Source has no basename")?);
        let resolved = resolve_future_directory(root).map_err(|e| e.to_string())?;
        if source.starts_with(&resolved) || resolved.starts_with(&source) {
            return Err("Cannot trash Trash or one of its ancestors or contents".into());
        }
        for path in [root.to_owned(), root.join("files"), root.join("info")] {
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    // SAFETY: geteuid has no preconditions.
                    if !metadata.is_dir()
                        || metadata.uid() != unsafe { libc::geteuid() }
                        || metadata.mode() & 0o077 != 0
                    {
                        return Err(format!(
                            "Trash unavailable: {path:?} must be a private, owned, non-symlink directory"
                        ));
                    }
                    if metadata.dev() != self.identity.parts().0 {
                        return Err("Trash is on another filesystem; cross-filesystem trashing is unsupported".into());
                    }
                    crate::batch::check_removal_parent(&path.join("probe"))?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let mut ancestor = path.as_path();
                    while !ancestor.exists() {
                        ancestor = ancestor.parent().ok_or("Trash has no existing ancestor")?;
                    }
                    let metadata = fs::metadata(ancestor).map_err(|e| e.to_string())?;
                    if !metadata.is_dir() || metadata.dev() != self.identity.parts().0 {
                        return Err("Trash unavailable or on another filesystem".into());
                    }
                    crate::batch::check_removal_parent(&ancestor.join("probe"))?;
                }
                Err(e) => return Err(format!("Trash unavailable: {e}")),
            }
        }
        Ok(())
    }

    pub fn execute(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        // Canonicalize only the parent: a Symlink Entry is always the link itself.
        let source = fs::canonicalize(self.source.parent().ok_or("Source has no parent")?)
            .map_err(|e| e.to_string())?
            .join(self.source.file_name().ok_or("Source has no basename")?);
        let canonical_root = resolve_future_directory(root).map_err(|e| e.to_string())?;
        if source.starts_with(&canonical_root) || canonical_root.starts_with(&source) {
            return Err("Cannot trash Trash or one of its ancestors or contents".into());
        }
        let date = deletion_date().map_err(|e| e.to_string())?;
        prepare_root(root).map_err(|e| format!("Trash unavailable: {e}"))?;
        let files = root.join("files");
        let info = root.join("info");
        if fs::metadata(&files).map_err(|e| e.to_string())?.dev() != self.identity.parts().0 {
            return Err(
                "Trash is on another filesystem; cross-filesystem trashing is unsupported".into(),
            );
        }
        for suffix in 0..10_000 {
            // Bounded names also accommodate maximum-length source basenames.
            let name = format!("janitor-{}-{suffix}", std::process::id());
            let target = files.join(&name);
            let metadata_path = info.join(format!("{name}.trashinfo"));
            match fs::symlink_metadata(&target) {
                Ok(_) => continue,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
            let mut metadata = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&metadata_path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot reserve Trash metadata: {e}")),
            };
            let result = (|| {
                write!(
                    metadata,
                    "[Trash Info]\nPath={}\nDeletionDate={date}\n",
                    encode_path(&source)
                )
                .and_then(|()| metadata.sync_all())
                .map_err(|e| e.to_string())?;
                self.validate()?;
                if SourceIdentity::from_metadata(
                    &fs::symlink_metadata(&source).map_err(|e| e.to_string())?,
                ) != self.identity
                {
                    return Err("Source identity changed after review".into());
                }
                rename_noreplace(&source, &target).map_err(|e| e.to_string())
            })();
            return match result {
                Ok(()) => Ok(()),
                Err(error) => Err(cleanup_failure(error, &metadata_path, |path| {
                    fs::remove_file(path)
                })),
            };
        }
        Err("Cannot allocate a unique Trash name".into())
    }
}

pub fn home_trash(home: &Path, data_home: Option<PathBuf>) -> PathBuf {
    data_home
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
        .join("Trash")
}

// Resolve existing ancestry without creating directories inside a reviewed source.
fn resolve_future_directory(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("Trash has no existing ancestor"))?;
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("Invalid Trash path"))?;
            Ok(resolve_future_directory(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

fn prepare_root(root: &Path) -> io::Result<()> {
    fs::create_dir_all(
        root.parent()
            .ok_or_else(|| io::Error::other("Trash has no parent"))?,
    )?;
    for path in [root.to_owned(), root.join("files"), root.join("info")] {
        match DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let metadata = fs::symlink_metadata(&path)?;
        // SAFETY: geteuid has no preconditions.
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::other(format!(
                "{path:?} must be a private, owned, non-symlink directory"
            )));
        }
    }
    Ok(())
}

fn cleanup_failure(
    error: String,
    metadata: &Path,
    remove: impl FnOnce(&Path) -> io::Result<()>,
) -> String {
    match remove(metadata) {
        Ok(()) => format!("{error}; no source move completed"),
        Err(cleanup) => format!(
            "{error}; no source move completed, but orphan Trash metadata may remain at {metadata:?}: {cleanup}"
        ),
    }
}

fn encode_path(path: &Path) -> String {
    let mut encoded = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn deletion_date() -> io::Result<String> {
    // SAFETY: time and localtime_r receive valid pointers; output is checked before use.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
        if now == -1 || libc::localtime_r(&now, local.as_mut_ptr()).is_null() {
            return Err(io::Error::other("Cannot determine local deletion time"));
        }
        let local = local.assume_init();
        Ok(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            local.tm_year + 1900,
            local.tm_mon + 1,
            local.tm_mday,
            local.tm_hour,
            local.tm_min,
            local.tm_sec
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "janitor-trash-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("Downloads")).unwrap();
            Self(root)
        }
        fn source(&self) -> PathBuf {
            self.0.join("Downloads/source")
        }
        fn trash(&self) -> PathBuf {
            self.0.join("Trash")
        }
        fn review(&self) -> TrashReview {
            TrashReview::new(&inbox::scan_inbox(&self.0.join("Downloads")).unwrap()[0]).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn metadata_round_trips_exact_path_and_preserves_prior_trash() {
        use std::ffi::OsStr;
        let fixture = Fixture::new();
        let source = fixture
            .0
            .join("Downloads")
            .join(OsStr::from_bytes(b"a %\n\xff"));
        for contents in [b"first", b"other"] {
            fs::write(&source, contents).unwrap();
            fixture.review().execute(&fixture.trash()).unwrap();
        }
        let files = fs::read_dir(fixture.trash().join("files"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 2);
        let mut contents = files
            .iter()
            .map(|p| fs::read(p).unwrap())
            .collect::<Vec<_>>();
        contents.sort();
        assert_eq!(contents, [b"first".to_vec(), b"other".to_vec()]);
        for file in files {
            let info = fs::read_to_string(fixture.trash().join("info").join(format!(
                "{}.trashinfo",
                file.file_name().unwrap().to_str().unwrap()
            )))
            .unwrap();
            assert!(info.starts_with("[Trash Info]\nPath=/"));
            assert!(info.contains("a%20%25%0A%FF\nDeletionDate="));
            let encoded = info
                .lines()
                .nth(1)
                .unwrap()
                .strip_prefix("Path=")
                .unwrap()
                .as_bytes();
            let mut decoded = Vec::new();
            let mut i = 0;
            while i < encoded.len() {
                if encoded[i] == b'%' {
                    decoded.push(
                        u8::from_str_radix(
                            std::str::from_utf8(&encoded[i + 1..i + 3]).unwrap(),
                            16,
                        )
                        .unwrap(),
                    );
                    i += 3;
                } else {
                    decoded.push(encoded[i]);
                    i += 1;
                }
            }
            assert_eq!(Path::new(OsStr::from_bytes(&decoded)), source);
            let date = info
                .lines()
                .nth(2)
                .unwrap()
                .strip_prefix("DeletionDate=")
                .unwrap();
            assert_eq!(date.len(), 19);
            assert_eq!(&date[10..11], "T");
        }
    }

    #[test]
    fn nonempty_directories_and_links_preserve_contents_and_targets() {
        for link in [false, true] {
            let fixture = Fixture::new();
            let target = fixture.0.join("target");
            fs::create_dir(&target).unwrap();
            fs::write(target.join("nested"), b"contents").unwrap();
            if link {
                std::os::unix::fs::symlink(&target, fixture.source()).unwrap();
            } else {
                fs::rename(&target, fixture.source()).unwrap();
            }
            fixture.review().execute(&fixture.trash()).unwrap();
            let trashed = fs::read_dir(fixture.trash().join("files"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(fs::read(trashed.join("nested")).unwrap(), b"contents");
            assert_eq!(fs::symlink_metadata(&trashed).unwrap().is_symlink(), link);
            if link {
                assert_eq!(fs::read_link(trashed).unwrap(), target);
                assert!(target.exists());
            }
            assert!(fs::symlink_metadata(fixture.source()).is_err());
        }
    }

    #[test]
    fn changed_source_and_unavailable_trash_never_delete() {
        let fixture = Fixture::new();
        fs::write(fixture.source(), b"original").unwrap();
        let review = fixture.review();
        fs::rename(fixture.source(), fixture.0.join("original")).unwrap();
        fs::write(fixture.source(), b"replacement").unwrap();
        assert!(
            review
                .execute(&fixture.trash())
                .unwrap_err()
                .contains("changed")
        );
        fs::write(fixture.trash(), b"blocked").unwrap();
        assert!(
            fixture
                .review()
                .execute(&fixture.trash())
                .unwrap_err()
                .contains("unavailable")
        );
        assert_eq!(fs::read(fixture.source()).unwrap(), b"replacement");
        assert_eq!(fs::read(fixture.0.join("original")).unwrap(), b"original");
    }

    #[test]
    fn refuses_symlink_trash_and_preserves_orphan_payload_collisions() {
        let fixture = Fixture::new();
        fs::write(fixture.source(), b"source").unwrap();
        let target = fixture.0.join("target");
        fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, fixture.trash()).unwrap();
        assert!(fixture.review().execute(&fixture.trash()).is_err());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        fs::remove_file(fixture.trash()).unwrap();
        prepare_root(&fixture.trash()).unwrap();
        let orphan = fixture
            .trash()
            .join(format!("files/janitor-{}-0", std::process::id()));
        fs::write(&orphan, b"orphan").unwrap();
        fixture.review().execute(&fixture.trash()).unwrap();
        assert_eq!(fs::read(orphan).unwrap(), b"orphan");
        assert_eq!(
            fs::read_dir(fixture.trash().join("files")).unwrap().count(),
            2
        );
    }

    #[test]
    fn refuses_trashing_ancestor_before_creating_storage() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.source()).unwrap();
        let trash = fixture.source().join("new/data/Trash");
        assert!(
            fixture
                .review()
                .execute(&trash)
                .unwrap_err()
                .contains("ancestors")
        );
        assert!(fs::read_dir(fixture.source()).unwrap().next().is_none());
    }

    #[test]
    fn move_failure_cleans_reserved_metadata_without_deleting_source() {
        use std::os::unix::fs::PermissionsExt;
        // Root bypasses filesystem permissions, so this scenario needs an ordinary user.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let fixture = Fixture::new();
        fs::write(fixture.source(), b"source").unwrap();
        let review = fixture.review();
        let downloads = fixture.0.join("Downloads");
        fs::set_permissions(&downloads, fs::Permissions::from_mode(0o500)).unwrap();
        let result = review.execute(&fixture.trash());
        fs::set_permissions(&downloads, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.unwrap_err().contains("no source move completed"));
        assert_eq!(fs::read(fixture.source()).unwrap(), b"source");
        assert_eq!(
            fs::read_dir(fixture.trash().join("files")).unwrap().count(),
            0
        );
        assert_eq!(
            fs::read_dir(fixture.trash().join("info")).unwrap().count(),
            0
        );
    }

    #[test]
    fn partial_metadata_cleanup_failure_is_explicit() {
        let error = cleanup_failure(
            "move refused".into(),
            Path::new("/isolated/info/entry.trashinfo"),
            |_| Err(io::Error::other("cleanup refused")),
        );
        assert!(error.contains("orphan Trash metadata may remain"));
        assert!(error.contains("/isolated/info/entry.trashinfo"));
        assert!(error.contains("cleanup refused"));
    }

    #[test]
    fn xdg_location_uses_only_absolute_override() {
        assert_eq!(
            home_trash(Path::new("/home/test"), Some("/data".into())),
            Path::new("/data/Trash")
        );
        for value in [None, Some("relative".into()), Some("".into())] {
            assert_eq!(
                home_trash(Path::new("/home/test"), value),
                Path::new("/home/test/.local/share/Trash")
            );
        }
    }
}
