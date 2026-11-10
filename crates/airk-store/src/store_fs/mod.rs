// SPDX-License-Identifier: Apache-2.0
//! Store filesystem and lock layer: process-lifetime ownership files, the
//! descriptor-anchored setup directory handle, parent-directory sync and
//! canonical data locations. Leaf: it depends only on `capacity` and
//! `managed_collection`. See `CONTRACT.md` beside this file.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

mod setup_directory;
pub(crate) use setup_directory::{SetupDirectory, SetupFileMode, file_identity};
#[cfg(all(windows, test))]
pub(crate) use setup_directory::{native_file_identity, nt_result};

pub(crate) struct OwnershipFile(pub(crate) File, pub(crate) AtomicBool);
impl Drop for OwnershipFile {
    fn drop(&mut self) {
        // Unpinned owners unlock before close so an unrelated concurrent fork
        // cannot extend ownership until CLOEXEC. Guardian pins require last close.
        if !self.1.load(Ordering::Relaxed) {
            let _ = self.0.unlock();
        }
    }
}

pub(crate) fn acquire_file(root: &Path, name: &str) -> Result<OwnershipFile> {
    acquire_file_for(root, name, std::time::Duration::ZERO)
}

pub(crate) fn acquire_file_for(
    root: &Path,
    name: &str,
    timeout: std::time::Duration,
) -> Result<OwnershipFile> {
    acquire_file_mode(root, name, timeout, true)
}

pub(crate) fn acquire_existing_file(root: &Path, name: &str) -> Result<OwnershipFile> {
    acquire_file_mode(root, name, std::time::Duration::ZERO, false)
}

pub(crate) fn acquire_existing_file_for(
    root: &Path,
    name: &str,
    timeout: std::time::Duration,
) -> Result<OwnershipFile> {
    acquire_file_mode(root, name, timeout, false)
}

pub(crate) fn acquire_shared_file(root: &Path, name: &str) -> Result<OwnershipFile> {
    let file = open_ownership_file(root, name, true)?;
    file.try_lock_shared()?;
    Ok(OwnershipFile(file, AtomicBool::new(false)))
}

fn acquire_file_mode(
    root: &Path,
    name: &str,
    timeout: std::time::Duration,
    create: bool,
) -> Result<OwnershipFile> {
    let path = root.join(name);
    let file = open_ownership_file(root, name, create)?;
    let started = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "another AIRK process owns service resource: {}",
                        path.display()
                    )
                });
            }
        }
    }
    Ok(OwnershipFile(file, AtomicBool::new(false)))
}

pub(crate) fn open_existing_ownership_file(root: &Path, name: &str) -> Result<File> {
    open_ownership_file(root, name, false)
}
fn open_ownership_file(root: &Path, name: &str, create: bool) -> Result<File> {
    let path = root.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == 0,
            "invalid service ownership file"
        );
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(&path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && opened.len() == 0,
        "invalid opened service ownership file"
    );
    crate::managed_collection::reject_file_aliases(&path, &opened)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            file.metadata()?.file_attributes() & 0x400 == 0,
            "redirected service ownership file"
        );
    }
    Ok(file)
}

pub(crate) fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub(crate) fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
/// Resolve existing ancestors without creating a directory or accepting traversal.
pub fn canonical_location(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute()
            && path.to_str().is_some_and(|value| value.len() <= 4096)
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "service data location must be absolute without traversal"
    );
    let mut prefix = path.to_path_buf();
    let mut suffix = Vec::new();
    while !exists(&prefix)? {
        suffix.push(
            prefix
                .file_name()
                .context("service data ancestor missing")?
                .to_owned(),
        );
        ensure!(prefix.pop(), "service data ancestor missing");
    }
    ensure!(prefix.is_dir(), "service data ancestor is not a directory");
    let mut value = prefix.canonicalize()?;
    for name in suffix.into_iter().rev() {
        value.push(name);
    }
    ensure!(
        value
            .to_str()
            .is_some_and(|v| v.len() <= 4096 && !v.chars().any(char::is_control)),
        "invalid service data location"
    );
    Ok(value)
}
