// SPDX-License-Identifier: Apache-2.0
//! Bounded SQLite snapshots in the private control directory; no collection copies.
use airk_core::identity::{Incarnation, Sha256Digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
    types::ValueRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) const MAX_ATTEMPTS: usize = 4;
const MAX_BYTES: u64 = super::super::MAX_DATABASE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub platform: String,
    pub volume: u64,
    pub file: u64,
}
impl FileIdentity {
    fn read(file: &File) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            Ok(Self {
                platform: "unix".into(),
                volume: metadata.dev(),
                file: metadata.ino(),
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
            };
            // SAFETY: the borrowed File keeps its native handle valid; the output
            // is a zero-initialized Windows metadata structure of the exact ABI.
            let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self {
                platform: "windows".into(),
                volume: u64::from(information.dwVolumeSerialNumber),
                file: u64::from(information.nFileIndexHigh) << 32
                    | u64::from(information.nFileIndexLow),
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = file;
            anyhow::bail!("native catalog file identity unavailable")
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupAttempt {
    pub slot: u8,
    pub file_identity: Option<FileIdentity>,
    pub bytes: Option<u64>,
    pub sha256: Option<Sha256Digest>,
    pub complete: bool,
    pub abandoned: bool,
}
impl BackupAttempt {
    pub fn reserved(slot: u8) -> Self {
        Self {
            slot,
            file_identity: None,
            bytes: None,
            sha256: None,
            complete: false,
            abandoned: false,
        }
    }
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            usize::from(self.slot) < MAX_ATTEMPTS && !(self.complete && self.abandoned),
            "invalid catalog backup attempt"
        );
        ensure!(
            !self.complete
                || self.file_identity.is_some()
                    && self.sha256.is_some()
                    && self
                        .bytes
                        .is_some_and(|bytes| bytes > 0 && bytes <= MAX_BYTES),
            "incomplete catalog backup identity"
        );
        Ok(())
    }
    pub(super) fn path(&self, root: &Path, upgrade: Incarnation) -> PathBuf {
        root.join(format!("catalog-schema11-{upgrade}-{}.sqlite3", self.slot))
    }
}

fn open_backup_file(path: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= MAX_BYTES,
        "catalog backup exceeds registry bound"
    );
    let file = File::open(path)?;
    crate::managed_collection::reject_file_aliases(path, &file.metadata()?)?;
    Ok(file)
}
pub(super) fn digest(path: &Path) -> Result<(u64, Sha256Digest)> {
    let file = open_backup_file(path)?;
    let identity = FileIdentity::read(&file)?;
    let mut input = file.take(MAX_BYTES + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut count = 0u64;
    loop {
        let bytes = input.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        count += bytes as u64;
        ensure!(count <= MAX_BYTES, "catalog backup grew beyond bound");
        hash.update(&buffer[..bytes]);
    }
    let current = open_backup_file(path)?;
    ensure!(
        FileIdentity::read(&current)? == identity && current.metadata()?.len() == count,
        "catalog backup changed during digest"
    );
    Ok((count, Sha256Digest::from_bytes(hash.finalize().into())))
}

pub(super) fn validate(
    root: &Path,
    upgrade: Incarnation,
    attempt: &BackupAttempt,
) -> Result<PathBuf> {
    attempt.validate()?;
    ensure!(attempt.complete, "catalog backup is not complete");
    let path = attempt.path(root, upgrade);
    for suffix in ["-journal", "-wal", "-shm"] {
        let overlay = path.with_file_name(format!(
            "{}{suffix}",
            path.file_name()
                .context("backup filename missing")?
                .to_string_lossy()
        ));
        match fs::symlink_metadata(&overlay) {
            Ok(_) => anyhow::bail!("qualified catalog backup has an unexpected overlay"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        Some(FileIdentity::read(&open_backup_file(&path)?)?) == attempt.file_identity,
        "catalog backup file identity changed"
    );
    let (bytes, sha256) = digest(&path)?;
    ensure!(
        Some(bytes) == attempt.bytes && Some(sha256) == attempt.sha256,
        "catalog backup digest changed"
    );
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let application: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        application == super::super::APPLICATION && version == 11,
        "catalog backup format changed"
    );
    super::validate_shape(&connection, 11)?;
    integrity(&connection)?;
    Ok(path)
}

pub(super) fn integrity(connection: &Connection) -> Result<()> {
    let check: String = connection.query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))?;
    ensure!(check == "ok", "registry integrity check failed");
    let foreign: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check LIMIT 1)",
        [],
        |row| row.get(0),
    )?;
    ensure!(!foreign, "registry contains a broken foreign key");
    Ok(())
}

pub(super) fn restore(
    root: &Path,
    upgrade: Incarnation,
    attempt: &BackupAttempt,
    expected_rows: Sha256Digest,
) -> Result<()> {
    let snapshot = validate(root, upgrade, attempt)?;
    let source = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    ensure!(
        legacy_data_digest(&source)? == expected_rows,
        "catalog restore snapshot rows changed"
    );
    crate::capacity::require_growth(root, MAX_BYTES * 2)?;
    let path = super::upgrade::validate_registry_files(root)?;
    let mut destination = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    destination.busy_timeout(Duration::from_millis(100))?;
    destination.execute_batch(
        "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
    )?;
    let deadline = Instant::now() + Duration::from_secs(30);
    {
        let backup = Backup::new(&source, &mut destination)?;
        loop {
            ensure!(
                Instant::now() < deadline,
                "catalog restore deadline exceeded; owned operation retained"
            );
            match backup.step(64)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    anyhow::bail!("catalog restore is busy; resume the owned operation")
                }
                _ => anyhow::bail!("unsupported SQLite restore outcome"),
            }
        }
    }
    super::validate_shape(&destination, 11)?;
    integrity(&destination)?;
    ensure!(
        legacy_data_digest(&destination)? == expected_rows,
        "catalog restore changed legacy rows"
    );
    drop(destination);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    crate::maintenance::sync_parent(root)?;
    Ok(())
}

/// Whole legacy registry rows only, used offline to prove preservation through
/// SQLite checkpoint/backup and additive migration. Never on a retrieval path.
pub(super) fn legacy_data_digest(connection: &Connection) -> Result<Sha256Digest> {
    fn part(hash: &mut Sha256, bytes: &[u8]) {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut hash = Sha256::new();
    part(&mut hash, b"airk/catalog-legacy-rows/1");
    let mut total = 0u64;
    for table in super::expected(11)?
        .iter()
        .filter(|object| object.kind == "table")
    {
        ensure!(
            table
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'),
            "invalid compiled registry table"
        );
        part(&mut hash, table.name.as_bytes());
        let mut info = connection.prepare(&format!("PRAGMA table_info(\"{}\")", table.name))?;
        let columns = info
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut primary = columns
            .iter()
            .filter(|(_, key)| *key > 0)
            .collect::<Vec<_>>();
        primary.sort_by_key(|(_, key)| *key);
        let names = if primary.is_empty() {
            columns.iter().collect::<Vec<_>>()
        } else {
            primary
        };
        ensure!(
            names.iter().all(|(name, _)| name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')),
            "invalid compiled registry column"
        );
        let order = names
            .iter()
            .map(|(name, _)| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",");
        let mut query = connection.prepare(&format!(
            "SELECT * FROM \"{}\" ORDER BY {order}",
            table.name
        ))?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            ensure!(
                Instant::now() < deadline,
                "legacy registry fingerprint deadline exceeded"
            );
            hash.update([1]);
            for column in 0..columns.len() {
                let value = row.get_ref(column)?;
                let encoded = match value {
                    ValueRef::Null => {
                        hash.update([0]);
                        0
                    }
                    ValueRef::Integer(number) => {
                        hash.update([1]);
                        part(&mut hash, &number.to_be_bytes());
                        8
                    }
                    ValueRef::Real(number) => {
                        hash.update([2]);
                        part(&mut hash, &number.to_bits().to_be_bytes());
                        8
                    }
                    ValueRef::Text(bytes) => {
                        hash.update([3]);
                        part(&mut hash, bytes);
                        bytes.len() as u64
                    }
                    ValueRef::Blob(bytes) => {
                        hash.update([4]);
                        part(&mut hash, bytes);
                        bytes.len() as u64
                    }
                };
                total = total
                    .checked_add(encoded)
                    .context("legacy fingerprint size overflow")?;
                ensure!(
                    total <= MAX_BYTES * 16,
                    "legacy fingerprint exceeds byte budget"
                );
            }
        }
        hash.update([0]);
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

/// Returns false only for an existing unclaimed slot; preserve it, reserve another
/// bounded slot and record the abandoned attempt instead of deleting unknown data.
pub(super) fn capture(
    root: &Path,
    upgrade: Incarnation,
    source: &Connection,
    expected_rows: Sha256Digest,
    attempt: &mut BackupAttempt,
    mut checkpoint: impl FnMut(&BackupAttempt) -> Result<()>,
) -> Result<bool> {
    attempt.validate()?;
    let path = attempt.path(root, upgrade);
    if attempt.complete {
        validate(root, upgrade, attempt)?;
        return Ok(true);
    }
    if attempt.file_identity.is_none() {
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let created = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        crate::managed_collection::reject_file_aliases(&path, &created.metadata()?)?;
        created.sync_all()?;
        attempt.file_identity = Some(FileIdentity::read(&created)?);
        checkpoint(attempt)?;
    }
    let current = open_backup_file(&path)?;
    ensure!(
        Some(FileIdentity::read(&current)?) == attempt.file_identity,
        "catalog backup destination was replaced"
    );
    // Only this operation's recorded, incomplete destination can be reset. It
    // has never been selected as a registry or accepted as a qualified backup.
    for suffix in ["-journal", "-wal", "-shm"] {
        let overlay = path.with_file_name(format!(
            "{}{suffix}",
            path.file_name()
                .context("backup filename missing")?
                .to_string_lossy()
        ));
        match fs::symlink_metadata(&overlay) {
            Ok(metadata) => {
                crate::managed_collection::reject_file_aliases(&overlay, &metadata)?;
                ensure!(
                    metadata.len() <= MAX_BYTES * 2,
                    "backup overlay exceeds bound"
                );
                // A partial destination's sidecar has no persisted file identity.
                // Preserve it and reserve another bounded slot rather than let
                // SQLite recover/delete an unclaimed file during reset.
                return Ok(false);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    crate::capacity::require_growth(root, MAX_BYTES * 2)?;
    let writable = OpenOptions::new().read(true).write(true).open(&path)?;
    ensure!(
        Some(FileIdentity::read(&writable)?) == attempt.file_identity,
        "catalog backup destination changed before reset"
    );
    writable.set_len(0)?;
    writable.sync_all()?;
    let mut destination = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    destination.busy_timeout(Duration::from_millis(100))?;
    destination.execute_batch("PRAGMA page_size=4096; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    {
        let backup = Backup::new(source, &mut destination)?;
        loop {
            ensure!(
                Instant::now() < deadline,
                "catalog backup deadline exceeded; owned destination preserved"
            );
            match backup.step(64)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    anyhow::bail!("catalog backup is busy; resume the owned operation")
                }
                _ => anyhow::bail!("unsupported SQLite backup outcome"),
            }
        }
    }
    destination.execute_batch("PRAGMA journal_mode=DELETE;")?;
    super::validate_shape(&destination, 11)?;
    integrity(&destination)?;
    ensure!(
        legacy_data_digest(&destination)? == expected_rows,
        "catalog backup did not preserve legacy rows"
    );
    drop(destination);
    writable.sync_all()?;
    crate::maintenance::sync_parent(root)?;
    let (bytes, sha256) = digest(&path)?;
    attempt.bytes = Some(bytes);
    attempt.sha256 = Some(sha256);
    attempt.complete = true;
    checkpoint(attempt)?;
    validate(root, upgrade, attempt)?;
    Ok(true)
}
