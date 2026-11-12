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
