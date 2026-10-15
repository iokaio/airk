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
    let mut paths = generations(directory)?;
    for token in crate::history::lease_tokens(&crate::FileStore::new(directory))? {
        let path = directory
            .join("generations")
            .join(token.revision.to_string());
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    for revision in crate::maintenance::legacy_intent_revisions(directory)? {
        let path = directory.join("generations").join(revision);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    for token in crate::maintenance::intent_tokens(directory)? {
        let path = directory
            .join("generations")
            .join(token.revision.to_string());
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
        .iter()
        .map(|path| {
            exclusive(path)?
                .context("collection is in use by a reader; disconnect clients before removal")
        })
        .collect()
}

/// Caller holds the collection writer lock from active-ID lookup through completion.
pub fn prune(directory: &Path, active: &str, dry_run: bool) -> Result<PruneReport> {
    if directory.join("index.sqlite3").exists()
        && crate::sqlite::persistent_revision(directory)?
            .is_some_and(|revision| revision.token.is_some())
    {
        return crate::maintenance::prune_compat(directory, active, dry_run);
    }
    ensure!(
        active.len() == 64 && active.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid active revision identity"
    );
    ensure!(
        directory.join("generations").join(active).is_dir(),
        "active generation is missing; refusing pruning"
    );
    let mut result = PruneReport {
        dry_run,
        active: active.into(),
        reclaimable_bytes: 0,
        removed_bytes: 0,
        candidates: Vec::new(),
        pinned: Vec::new(),
        unreferenced_objects: 0,
        retired_receipts: 0,
        receipt_cleanup_pending: false,
        receipt_cleanup_error: None,
    };
    for path in generations(directory)? {
        let name = path
            .file_name()
            .context("missing generation name")?
            .to_string_lossy()
            .into_owned();
        if name == active {
            continue;
        }
        let Some(lease) = exclusive(&path)? else {
            result.pinned.push(name);
            continue;
        };
        let bytes = super::directory_usage(&path)?.0;
        result.reclaimable_bytes = result
            .reclaimable_bytes
            .checked_add(bytes)
            .context("prune usage overflow")?;
        result.candidates.push(name.clone());
        if !dry_run {
            // Rename before releasing the lease: new readers cannot open this revision.
            let removed = path.with_file_name(format!("removing-{name}"));
            ensure!(
                !removed.exists(),
                "interrupted pruning requires operator recovery"
            );
            fs::rename(&path, &removed).context("rename unpinned obsolete generation")?;
            drop(lease);
            fs::remove_dir_all(&removed).context("remove detached obsolete generation")?;
            result.removed_bytes = result
                .removed_bytes
                .checked_add(bytes)
                .context("prune usage overflow")?;
        }
    }
    prune_objects(directory, &mut result)?;
    if !dry_run && directory.join("index.sqlite3").exists() {
        match crate::sqlite::retire_unrepresented_receipts(directory) {
            Ok(count) => result.retired_receipts = count,
            Err(error) => {
                result.receipt_cleanup_pending = true;
                result.receipt_cleanup_error =
                    Some(format!("{error:#}").chars().take(4096).collect());
            }
        }
    }
    Ok(result)
}

/// Writer exclusion makes the generation inventory stable. Reader-pinned old
/// generations retain every object they reference; dry runs model candidate removal.
fn prune_objects(directory: &Path, report: &mut PruneReport) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeSet, io::Read};
    let objects = directory.join("objects");
    if !objects.exists() {
        return Ok(());
    }
    ensure!(
        !fs::symlink_metadata(&objects)?.file_type().is_symlink(),
        "linked object directory"
    );
    let mut artifacts = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for generation in generations(directory)? {
        let revision = generation
            .file_name()
            .context("generation name missing")?
            .to_string_lossy();
        if report.dry_run
            && report
                .candidates
                .iter()
                .any(|value| value == revision.as_ref())
        {
            continue;
        }
        let mut manifest_bytes = Vec::new();
        File::open(generation.join("manifest.json"))?
            .take(131_073)
            .read_to_end(&mut manifest_bytes)?;
        ensure!(
            manifest_bytes.len() <= 131_072,
            "oversized generation manifest"
        );
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
        if manifest["schema"].as_u64() == Some(2) {
            let token: airk_core::identity::RevisionToken =
                serde_json::from_value(manifest["token"].clone())?;
            ensure!(
                token.revision.to_string() == revision,
                "retained v2 directory identity mismatch"
            );
            let receipt = crate::sqlite::receipt(directory, &token)?
                .context("retained publication receipt missing; refusing object deletion")?;
            let inventory = crate::blobs::verify_v2_generation(directory, &receipt)?;
            for entry in inventory.files.values() {
                artifacts.insert(entry.state.payload_digest.to_string());
                if entry.raw_source {
                    sources.insert(entry.state.source_hash.to_string());
                }
            }
            continue;
        }
        let engine = manifest["engine"]
            .as_str()
            .context("generation engine missing")?;
        let inventory = generation.join("inventory.json");
        if !inventory.exists() && engine.ends_with("/generation-2") {
            continue;
        }
        let mut bytes = Vec::new();
        File::open(inventory)?
            .take(crate::blobs::MAX_CATALOG_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= crate::blobs::MAX_CATALOG_BYTES,
            "oversized retained catalog"
        );
        let mut digest = Sha256::new();
        digest.update(engine.as_bytes());
        digest.update(&bytes);
        ensure!(
            format!("{:x}", digest.finalize()) == revision,
            "retained catalog revision digest mismatch; refusing object deletion"
        );
        let snapshot: airk_core::Snapshot = serde_json::from_slice(&bytes)?;
        for entry in snapshot.artifacts.values() {
            if let Some(reference) = &entry.retained {
                artifacts.insert(reference.object.clone());
                if reference.raw_source {
                    sources.insert(entry.content_hash.clone());
                }
            }
        }
    }
    for (kind, retained) in [("artifacts", artifacts), ("sources", sources)] {
        let path = objects.join(kind);
        if !path.exists() {
            continue;
        }
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "linked object directory"
        );
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = entry.file_type()?;
            ensure!(
                metadata.is_file() && !metadata.is_symlink(),
                "invalid retained object entry"
            );
            if (!crate::blobs::valid_digest(&name) && !name.starts_with("building-"))
                || retained.contains(&name)
            {
                continue;
            }
            let bytes = entry.metadata()?.len();
            report.unreferenced_objects += 1;
            report.reclaimable_bytes = report
                .reclaimable_bytes
                .checked_add(bytes)
                .context("object usage overflow")?;
            if !report.dry_run {
                fs::remove_file(entry.path())?;
                report.removed_bytes = report
                    .removed_bytes
                    .checked_add(bytes)
                    .context("object usage overflow")?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pruning_protects_active_and_pinned_generations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let collection = directory.path().join("collections/default");
        let leases = directory.path().join("collections/.reader-locks/default");
        fs::create_dir_all(&leases)?;
        let active = "a".repeat(64);
        let stale = "b".repeat(64);
        for name in [&active, &stale] {
            let path = collection.join("generations").join(name);
            fs::create_dir_all(&path)?;
            File::create(leases.join(name))?;
            fs::write(path.join("payload"), b"original fixture")?;
        }
        let lease = File::open(leases.join(&stale))?;
        lease.lock_shared()?;
        assert_eq!(
            prune(&collection, &active, false)?.pinned,
            vec![stale.clone()]
        );
        assert!(exclusive_collection_leases(&collection).is_err());
        drop(lease);
        let preview = prune(&collection, &active, true)?;
        assert!(preview.reclaimable_bytes > 0);
        assert_eq!(preview.removed_bytes, 0);
        assert_eq!(
            prune(&collection, &active, false)?.removed_bytes,
            preview.reclaimable_bytes
        );
        assert!(collection.join("generations").join(active).is_dir());
        Ok(())
    }
}
