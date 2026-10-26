// SPDX-License-Identifier: Apache-2.0
//! Explicit pruning of obsolete immutable components, respecting live reader leases.
pub use crate::maintenance::{
    MaintenanceBudget, MaintenanceIntent, MaintenanceReceipt, MaintenanceRun, MaintenanceStage,
    MaintenanceState, MaintenanceTrigger, PolicyOrigin, PolicyReceipt, ProtectedRevision,
    ProtectionReason, RetentionCandidate, RetentionMode, RetentionPolicy, RetentionPreview,
    RetentionStatus,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct PruneReport {
    pub dry_run: bool,
    pub active: String,
    pub reclaimable_bytes: u64,
    pub removed_bytes: u64,
    pub candidates: Vec<String>,
    pub pinned: Vec<String>,
    pub unreferenced_objects: usize,
    pub retired_receipts: usize,
    pub receipt_cleanup_pending: bool,
    pub receipt_cleanup_error: Option<String>,
}

pub(crate) fn generations(directory: &Path) -> Result<Vec<PathBuf>> {
    let base = directory.join("generations");
    if !base.exists() {
        return Ok(Vec::new());
    }
    ensure!(
        !fs::symlink_metadata(&base)?.file_type().is_symlink(),
        "linked generation directory"
    );
    let mut result = Vec::new();
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 64
            && name
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            ensure!(
                entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink(),
                "invalid generation entry"
            );
            result.push(entry.path());
        }
    }
    result.sort();
    Ok(result)
}

pub(crate) fn exclusive(path: &Path) -> Result<Option<File>> {
    let base = path
        .parent()
        .and_then(Path::parent)
        .context("collection parent missing")?;
    let lock_path = base
        .parent()
        .context("collection parent missing")?
        .join(".reader-locks")
        .join(base.file_name().context("collection name missing")?)
        .join(path.file_name().context("generation name missing")?);
    ensure!(
        !fs::symlink_metadata(&lock_path)?.file_type().is_symlink()
            && !fs::symlink_metadata(lock_path.parent().context("lease parent missing")?)?
                .file_type()
                .is_symlink()
            && !fs::symlink_metadata(
                lock_path
                    .parent()
                    .and_then(Path::parent)
                    .context("lease root missing")?
            )?
            .file_type()
            .is_symlink(),
        "linked generation lease"
    );
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .context("open exclusive generation lease")?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(error) => Err(anyhow::anyhow!(error).context("acquire exclusive generation lease")),
    }
}

/// Collection removal must acquire every lease before deleting any evidence.
pub fn exclusive_collection_leases(directory: &Path) -> Result<Vec<File>> {
    let paths = generations(directory)?;
    paths
        .iter()
        .map(|path| {
            exclusive(path)?
                .context("collection is in use by a reader; disconnect clients before removal")
        })
        .collect()
}
