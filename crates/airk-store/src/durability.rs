// SPDX-License-Identifier: Apache-2.0
//! Durable replacement of bounded service metadata; callers serialize writers.
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn persist_synced(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    temporary
        .as_file()
        .sync_all()
        .context("sync metadata temporary file")?;
    replace(temporary, path).context("replace metadata directory entry")?;
    crate::maintenance::sync_parent(path.parent().context("metadata path has no parent")?)
        .context("sync metadata parent directory")
}

#[cfg(not(windows))]
fn replace(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    temporary.persist(path)?;
    Ok(())
}

#[cfg(windows)]
fn replace(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    // Close the synced file handle before replacing the directory entry. The
    // TempPath still removes abandoned scratch if the native move fails.
    let temporary = temporary.into_temp_path();
    let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both owned, NUL-terminated paths remain alive through the call.
    // REPLACE_EXISTING | WRITE_THROUGH preserves a committed allowance across
    // process death; actual power-loss qualification remains platform-specific.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
