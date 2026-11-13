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
pub fn safe_file(path: &Path) -> Result<()> {
    // One implementation (U90): moved to `airk-ipc` for `airkctl`'s records.
    airk_ipc::private_record::require_regular_unlinked(path)
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
pub const MAC_LAUNCHER_IMAGE: &str = "AIRK.app/Contents/MacOS/airk-launcher-image";

/// The stable launcher beside an installation root.
pub fn launcher_path(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "airk-launcher.exe"
    } else {
        "airk-launcher"
    })
}

/// Read a bounded regular file after `safe_file`; growth past `limit` is refused.
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
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
pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests;
