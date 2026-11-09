// SPDX-License-Identifier: Apache-2.0
//! Installed-file names and the regular-file identity check shared by the
//! installer, launchers, process roles and log readers. This is a leaf: it
//! depends only on `std`, `anyhow` and the Windows file API, never on another
//! `airk-cli` module. See `CONTRACT.md` beside this file.
use anyhow::{Result, ensure};
#[cfg(windows)]
use std::fs::File;
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

#[cfg(test)]
mod tests;
