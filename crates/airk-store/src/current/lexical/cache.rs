// SPDX-License-Identifier: Apache-2.0
//! Bounded process-local verified seals. A digest alone never authorizes reuse:
//! bind it to the database file identity/change time, a SQLite change counter
//! and, on local Windows volumes, a directory change notification. Silent device
//! corruption is not a change notification: entries expire after thirty seconds,
//! and explicit integrity diagnostics always bypass this cache.
//! No cache entry retains a collection gate or a SQLite read transaction.
use super::{LexicalManifest, Snapshot};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use rusqlite::Connection;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 8;
const MAX_BYTES: usize = 32 << 20;

fn change_counter(path: &Path) -> Option<u32> {
    // Persistent SQLite handles pin deletion on Windows and unlinked bytes on
    // Unix. Rollback databases expose a durable counter in their header, so no
    // connection/file handle needs to survive between requests on any platform.
    use std::io::Read;
    let mut header = [0u8; 100];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    if &header[..16] != b"SQLite format 3\0" || header[18..20] != [1, 1] {
        return None;
    }
    Some(u32::from_be_bytes(header[24..28].try_into().ok()?))
}

// Windows timestamps can be restored while another handle is open. Observe
// directory writes as well, without a callback thread or an unbounded event queue.
#[cfg(windows)]
struct ChangeGuard(isize);
#[cfg(windows)]
impl ChangeGuard {
    fn new(path: &Path) -> Option<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE, Storage::FileSystem::*};
        let name: Vec<u16> = path
            .parent()?
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut volume = vec![0u16; 32768];
        // Remote notification delivery is not guaranteed. Use the full verifier
        // when the directory is not on a known local fixed volume.
        const LOCAL_FIXED_VOLUME: u32 = 3; // Win32 DRIVE_FIXED.
        unsafe {
            if GetVolumePathNameW(name.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) == 0
                || GetDriveTypeW(volume.as_ptr()) != LOCAL_FIXED_VOLUME
            {
                return None;
            }
            let handle = FindFirstChangeNotificationW(
                name.as_ptr(),
                0,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    | FILE_NOTIFY_CHANGE_SECURITY,
            );
            (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Self(handle as isize))
        }
    }
    fn unchanged(&self) -> bool {
        // Zero timeout: failed or signaled guards always fall back to hashing.
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.0 as _, 0)
                == windows_sys::Win32::Foundation::WAIT_TIMEOUT
        }
    }
}
#[cfg(windows)]
impl Drop for ChangeGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Storage::FileSystem::FindCloseChangeNotification(self.0 as _);
        }
    }
}
#[cfg(not(windows))]
struct ChangeGuard;
#[cfg(not(windows))]
impl ChangeGuard {
    fn new(_path: &Path) -> Option<Self> {
        Some(Self)
    }
    fn unchanged(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    identity: [u64; 3],
    change: [i64; 2],
    bytes: u64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(Stamp {
            identity: [metadata.dev(), metadata.ino(), 0],
            change: [metadata.ctime(), metadata.ctime_nsec()],
            bytes: metadata.len(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_BASIC_INFO, FILE_ID_INFO, FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx,
        };
        let file = std::fs::File::open(path).ok()?;
        let mut basic: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
        let mut id: FILE_ID_INFO = unsafe { std::mem::zeroed() };
        // Fixed-size OS records; the handle and buffers remain live for each call.
        unsafe {
            if GetFileInformationByHandleEx(
                file.as_raw_handle() as _,
                FileBasicInfo,
                &mut basic as *mut _ as _,
                std::mem::size_of_val(&basic) as u32,
            ) == 0
                || GetFileInformationByHandleEx(
                    file.as_raw_handle() as _,
                    FileIdInfo,
                    &mut id as *mut _ as _,
                    std::mem::size_of_val(&id) as u32,
                ) == 0
            {
                return None;
            }
        }
        if basic.ChangeTime == 0 {
            return None;
        }
        Some(Stamp {
            identity: [
                id.VolumeSerialNumber,
                u64::from_le_bytes(id.FileId.Identifier[..8].try_into().ok()?),
                u64::from_le_bytes(id.FileId.Identifier[8..].try_into().ok()?),
            ],
            change: [basic.ChangeTime, basic.LastWriteTime],
            bytes: file.metadata().ok()?.len(),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

struct Entry {
    path: PathBuf,
    commit: Sha256Digest,
    stamp: Stamp,
    version: u32,
    snapshot: Arc<Snapshot>,
    bytes: usize,
    verified_at: std::time::Instant,
    guard: ChangeGuard,
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

impl Cache {
    fn get(&mut self, path: &Path, commit: Sha256Digest) -> Option<Arc<Snapshot>> {
        let position = self
            .entries
            .iter()
            .position(|entry| entry.path == path && entry.commit == commit)?;
        let entry = self.entries.remove(position)?;
        self.bytes -= entry.bytes;
        let current = stamp(path);
        let version = change_counter(path);
        if entry.verified_at.elapsed() >= std::time::Duration::from_secs(30)
            || current.as_ref() != Some(&entry.stamp)
            || version != Some(entry.version)
            || stamp(path) != current
            || !entry.guard.unchanged()
        {
            return None;
        }
        let snapshot = entry.snapshot.clone();
        self.bytes += entry.bytes;
        self.entries.push_back(entry);
        Some(snapshot)
    }

    fn remember(&mut self, path: &Path, commit: Sha256Digest, snapshot: Arc<Snapshot>) {
        let Some(guard) = ChangeGuard::new(path) else {
            return;
        };
        self.remember_guarded(path, commit, snapshot, guard);
    }

    fn remember_guarded(
        &mut self,
        path: &Path,
        commit: Sha256Digest,
        snapshot: Arc<Snapshot>,
        guard: ChangeGuard,
    ) {
        self.entries.retain(|entry| entry.path != path);
        self.bytes = self.entries.iter().map(|entry| entry.bytes).sum();
        let bytes = snapshot.cache_bytes();
        if bytes > MAX_BYTES {
            return;
        }
        let Some(before) = stamp(path) else {
            return;
        };
        let Some(version) = change_counter(path) else {
            return;
        };
        if stamp(path).as_ref() != Some(&before) || !guard.unchanged() {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let Some(old) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            path: path.into(),
            commit,
            stamp: before,
            version,
            snapshot,
            bytes,
            verified_at: std::time::Instant::now(),
            guard,
        });
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

pub(in crate::current) fn verified(
    connection: &Connection,
    path: &Path,
    manifest: &LexicalManifest,
) -> Result<Arc<Snapshot>> {
    let commit = manifest.digest()?;
    if let Ok(mut cache) = cache().lock()
        && let Some(snapshot) = cache.get(path, commit)
    {
        return Ok(snapshot);
    }
    let guard = ChangeGuard::new(path);
    let before = stamp(path);
    let snapshot = Arc::new(Snapshot::capture(connection)?);
    ensure!(
        snapshot.manifest(manifest.updates)? == *manifest,
        "current FTS5 physical index digest mismatch"
    );
    if before.is_some()
        && stamp(path) == before
        && let Some(guard) = guard
        && let Ok(mut cache) = cache().lock()
    {
        cache.remember_guarded(path, commit, snapshot.clone(), guard);
    }
    Ok(snapshot)
}

pub(in crate::current) fn remember(path: &Path, commit: Sha256Digest, snapshot: Arc<Snapshot>) {
    if let Ok(mut cache) = cache().lock() {
        cache.remember(path, commit, snapshot);
    }
}

pub(in crate::current) fn clear() {
    if let Ok(mut cache) = cache().lock() {
        *cache = Cache::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn idle_cache_does_not_pin_database_deletion_by_another_process() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.sqlite3");
        let connection = Connection::open(&path)?;
        super::super::create(&connection)?;
        let snapshot = Arc::new(Snapshot::capture(&connection)?);
        let commit = snapshot
            .manifest(super::super::updates_digest(&[])?)?
            .digest()?;
        let mut cache = Cache::default();
        cache.remember(&path, commit, snapshot);
        assert!(cache.get(&path, commit).is_some());
        drop(connection);
        // No cache clear: this models a peer process that cannot clear our map.
        std::fs::remove_file(&path)?;
        std::fs::remove_dir(directory.path())?;
        assert!(cache.get(&path, commit).is_none());
        Ok(())
    }
    #[test]
    fn reuse_is_bound_to_database_and_digest_and_has_a_fixed_expiry() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.sqlite3");
        let connection = Connection::open(&path)?;
        super::super::create(&connection)?;
        let snapshot = Arc::new(Snapshot::capture(&connection)?);
        let commit = snapshot
            .manifest(super::super::updates_digest(&[])?)?
            .digest()?;
        let mut cache = Cache::default();
        cache.remember(&path, commit, snapshot.clone());
        assert!(Arc::ptr_eq(&snapshot, &cache.get(&path, commit).unwrap()));
        drop(connection);
        // CurrentWriter does this under its exclusive gate after closing SQLite.
        cache.remember(&path, commit, snapshot.clone());
        assert!(Arc::ptr_eq(&snapshot, &cache.get(&path, commit).unwrap()));
        assert!(
            cache
                .get(&directory.path().join("other.sqlite3"), commit)
                .is_none()
        );
        assert!(
            cache
                .get(&path, Sha256Digest::from_bytes([42; 32]))
                .is_none()
        );
        cache.entries[0].verified_at -= std::time::Duration::from_secs(31);
        assert!(cache.get(&path, commit).is_none());
        assert_eq!(cache.bytes, 0);
        Ok(())
    }
    #[test]
    fn guard_detects_sql_changes_and_raw_writes_with_restored_mtime() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.sqlite3");
        let connection = Connection::open(&path)?;
        super::super::create(&connection)?;
        let snapshot = Arc::new(Snapshot::capture(&connection)?);
        let commit = snapshot
            .manifest(super::super::updates_digest(&[])?)?
            .digest()?;
        let mut cache = Cache::default();
        cache.remember(&path, commit, snapshot.clone());
        assert!(cache.get(&path, commit).is_some());
        let other = Connection::open(&path)?;
        other.execute("UPDATE current_fts_config SET v=5 WHERE k='version'", [])?;
        assert!(cache.get(&path, commit).is_none());
        cache.remember(&path, commit, snapshot);
        let modified = std::fs::metadata(&path)?.modified()?;
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new().write(true).open(&path)?;
            file.seek(SeekFrom::Start(100))?;
            file.write_all(&[0, 0, 0, 0])?;
            file.sync_all()?;
            file.set_times(std::fs::FileTimes::new().set_modified(modified))?;
        }
        assert!(cache.get(&path, commit).is_none());
        Ok(())
    }
}
