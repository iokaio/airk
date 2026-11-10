// SPDX-License-Identifier: Apache-2.0
//! Installed-file names, the regular-file identity check, the bounded metadata
//! read and the directory sync shared by the installer, the install layout,
//! launchers, process roles and log readers. This is a leaf: it
//! depends only on `std`, `anyhow` and the Windows file API, never on another
//! `airk-cli` module. See `CONTRACT.md` beside this file.
use anyhow::{Result, ensure};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Refuse anything other than a regular, unlinked file with exactly one name:
/// symlinks, reparse points (Windows) and hard-link aliases are rejected.
pub(crate) fn safe_file(path: &Path) -> Result<()> {
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

/// Payload-relative name of the main executable on this platform.
pub fn main_name() -> &'static str {
    if cfg!(windows) {
        "airk.exe"
    } else if cfg!(target_os = "macos") {
        "AIRK.app/Contents/MacOS/airk"
    } else {
        "airk"
    }
}

/// Payload-relative name of the separately signed Mac launcher image.
pub(crate) const MAC_LAUNCHER_IMAGE: &str = "AIRK.app/Contents/MacOS/airk-launcher-image";

/// The stable launcher beside an installation root.
pub fn launcher_path(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "airk-launcher.exe"
    } else {
        "airk-launcher"
    })
}

/// Read a bounded regular file after `safe_file`; growth past `limit` is refused.
pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    safe_file(path)?;
    ensure!(
        fs::metadata(path)?.len() <= limit,
        "runtime metadata exceeds byte limit"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "runtime metadata grew beyond byte limit"
    );
    Ok(bytes)
}

/// Make a directory entry change durable on Unix; a no-op elsewhere.
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests;
