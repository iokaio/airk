// SPDX-License-Identifier: Apache-2.0
//! Owner-private bounded record files (split U90): the HTTPS identity,
//! credential and trust records and their locks, read and written with the
//! same checks by the service and by `airkctl`'s offline recovery (U75).
//! Moved from `airk-cli` (`http_endpoint.rs`, `http_endpoint/rotation.rs`;
//! the regular-file check from `install_files`) by
//! `scripts/dev/split_u90b_cert_trust.py`.
use anyhow::{Context, Result, ensure};
use std::{fs::File, io, path::Path};

/// Refuse anything other than a regular, unlinked file with exactly one name:
/// symlinks, reparse points (Windows) and hard-link aliases are rejected.
pub fn require_regular_unlinked(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "runtime file is not a regular unlinked file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "aliased runtime file");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::io::AsRawHandle;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected runtime file"
        );
        let file = File::open(path)?;
        // SAFETY: initialized output storage and a live read-only file handle.
        let mut information: windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION =
            unsafe { std::mem::zeroed() };
        // SAFETY: native API writes exactly the fixed output structure.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                    file.as_raw_handle(),
                    &mut information,
                )
            } != 0,
            "cannot inspect runtime file aliases"
        );
        ensure!(information.nNumberOfLinks == 1, "aliased runtime file");
    }
    Ok(())
}

pub fn private_file(path: &Path) -> Result<()> {
    require_regular_unlinked(path)?;
    #[cfg(windows)]
    crate::endpoint::require_owner_private(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.uid().to_string() == crate::endpoint::current_user()?
                && metadata.permissions().mode() & 0o077 == 0,
            "HTTP file is not owner-private"
        );
    }
    Ok(())
}

pub fn read_private(root: &Path, name: &str, limit: usize) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    crate::endpoint::require_existing_root(root)?;
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => {
            result?;
        }
    }
    private_file(&path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(&path)?;
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "HTTP private record exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "HTTP private record exceeds bound");
    private_file(&path)?;
    Ok(Some(bytes))
}

pub fn write_private(
    root: &Path,
    name: &str,
    bytes: &[u8],
    limit: usize,
    replace: bool,
) -> Result<()> {
    use std::io::Write;
    ensure!(bytes.len() <= limit, "HTTP private record exceeds bound");
    crate::endpoint::require_existing_root(root)?;
    let path = root.join(name);
    if std::fs::symlink_metadata(&path).is_ok() {
        private_file(&path)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    private_file(temporary.path())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(&path).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(&path)
            .map_err(|error| error.error)?;
    }
    private_file(&path)?;
    #[cfg(unix)]
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

/// An exclusive owner-private lock file, released on drop.
pub struct PrivateLock(File);
impl PrivateLock {
    pub fn as_file(&self) -> &File {
        &self.0
    }
}
impl Drop for PrivateLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
pub fn lock(root: &Path, name: &str, busy: &'static str) -> Result<PrivateLock> {
    crate::endpoint::require_existing_root(root)?;
    let path = root.join(name);
    if path.try_exists()? {
        private_file(&path)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(&path)?;
    private_file(&path)?;
    file.try_lock().context(busy)?;
    Ok(PrivateLock(file))
}

/// Read a private record relative to an already-pinned owner-private directory.
pub fn read_private_at(
    root: &airk_process::directory::OpenedDirectory,
    name: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>> {
    use std::{ffi::OsStr, io::Read};
    crate::endpoint::require_private_directory_handle(root.as_file())?;
    let file = match root.open_file(OsStr::new(name)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    crate::endpoint::require_private_metadata_handle(&file)?;
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "private record exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "private record exceeds bound");
    Ok(Some(bytes))
}

/// Publish a bounded private record under a caller-held private directory.
pub fn write_private_at(
    root: &airk_process::directory::OpenedDirectory,
    name: &str,
    bytes: &[u8],
    limit: usize,
    replace: bool,
) -> Result<()> {
    crate::endpoint::require_private_directory_handle(root.as_file())?;
    write_owner_at(root, name, bytes, limit, replace)
}

/// Publish a private file under a protected parent that may permit foreign reads.
/// Caller owns the logical replacement authorization and serialization.
pub fn write_owner_at(
    root: &airk_process::directory::OpenedDirectory,
    name: &str,
    bytes: &[u8],
    limit: usize,
    replace: bool,
) -> Result<()> {
    use std::{
        ffi::OsStr,
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    ensure!(bytes.len() <= limit, "private record exceeds bound");
    airk_process::directory::validate_child_name(OsStr::new(name))?;
    crate::endpoint::require_protected_parent_handle(root.as_file())?;
    // Opening the old leaf refuses redirection/aliases; publication replaces
    // only its directory entry. Logical old-byte authority belongs to caller.
    match root.open_file(OsStr::new(name)) {
        Ok(file) => drop(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let mut staged = None;
    for _ in 0..32 {
        let temporary = format!(
            ".airk-private-{}-{stamp}-{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        match crate::endpoint::create_private_file_at(root, OsStr::new(&temporary)) {
            Ok(file) => {
                staged = Some((temporary, file));
                break;
            }
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    let (temporary, mut file) = staged.context("private staging name collisions")?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    root.publish_file(OsStr::new(&temporary), OsStr::new(name), replace)?;
    let published = root.open_file(OsStr::new(name))?;
    crate::endpoint::require_private_metadata_handle(&published)?;
    root.sync_all()?;
    Ok(())
}

/// Remove exact private bytes within a held parent. Windows compares and
/// disposes one stable handle; Unix uses original-dirfd unlink under the
/// caller's serialization (not atomic compare/unlink against hostile writers).
pub fn remove_private_at(
    root: &airk_process::directory::OpenedDirectory,
    name: &str,
    expected: &[u8],
) -> Result<bool> {
    use std::{ffi::OsStr, io::Read};
    crate::endpoint::require_protected_parent_handle(root.as_file())?;
    let verify = |file: &mut File| -> io::Result<bool> {
        #[cfg(unix)]
        crate::endpoint::require_private_metadata_handle(file)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let mut bytes = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes != expected {
            return Err(io::Error::other("private record changed before removal"));
        }
        Ok(true)
    };
    #[cfg(windows)]
    let result = root.remove_file_if(OsStr::new(name), |file, proof| {
        crate::endpoint::require_private_pending_metadata_handle(file, proof)
            .map_err(|error| io::Error::other(error.to_string()))?;
        verify(file)
    });
    #[cfg(unix)]
    let result = (|| -> io::Result<bool> {
        let mut file = root.open_file(OsStr::new(name))?;
        verify(&mut file)?;
        root.unlink_file(OsStr::new(name))?;
        Ok(true)
    })();
    match result {
        Ok(removed) => Ok(removed),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Acquire an exclusive lock through the original pinned parent.
pub fn lock_at(
    root: &airk_process::directory::OpenedDirectory,
    name: &str,
    busy: &'static str,
) -> Result<PrivateLock> {
    use std::ffi::OsStr;
    crate::endpoint::require_private_directory_handle(root.as_file())?;
    let file = match root.open_file(OsStr::new(name)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match crate::endpoint::create_private_file_at(root, OsStr::new(name)) {
                Ok(file) => drop(file),
                Err(error)
                    if error
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) => {}
                Err(error) => return Err(error),
            }
            root.open_file(OsStr::new(name))?
        }
        Err(error) => return Err(error.into()),
    };
    crate::endpoint::require_private_metadata_handle(&file)?;
    file.try_lock().context(busy)?;
    Ok(PrivateLock(file))
}

#[cfg(test)]
mod rooted_tests;
