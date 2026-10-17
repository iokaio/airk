// SPDX-License-Identifier: Apache-2.0
//! Charged, intent-bound retirement of searchable revisions. No history promise.
use crate::{
    FileStore, WriterLock,
    quota::{Quota, QuotaPressure, Reservation},
};
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_INTENTS: usize = 256;
const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_SCAN_ENTRIES: usize = 1_000_000;
#[path = "maintenance_legacy.rs"]
mod legacy;
pub use legacy::LegacyRevision;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionMode {
    Manual,
    Automatic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    pub version: u32,
    pub mode: RetentionMode,
    pub searchable_predecessors: u8,
}
impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            mode: RetentionMode::Manual,
            searchable_predecessors: 0,
        }
    }
}
impl RetentionPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.searchable_predecessors <= 8,
            "retention supports version 1 and zero through eight searchable predecessors"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOrigin {
    ExistingManual,
    NewCollectionDefault,
    Explicit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyReceipt {
    pub policy: RetentionPolicy,
    pub origin: PolicyOrigin,
    pub digest: Sha256Digest,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceBudget {
    pub max_candidates: usize,
    pub max_metadata_bytes: u64,
    pub max_removal_bytes: u64,
    pub max_duration_ms: u64,
}
impl Default for MaintenanceBudget {
    fn default() -> Self {
        Self {
            max_candidates: 32,
            max_metadata_bytes: 1024 * 1024 * 1024,
            max_removal_bytes: 4 * 1024 * 1024 * 1024,
            max_duration_ms: 10_000,
        }
    }
}
impl MaintenanceBudget {
    pub(crate) fn validate(self) -> Result<()> {
        ensure!(
            (1..=256).contains(&self.max_candidates)
                && (16 * 1024..=8 * 1024 * 1024 * 1024).contains(&self.max_metadata_bytes)
                && self.max_removal_bytes > 0
                && self.max_removal_bytes <= 64 * 1024 * 1024 * 1024
                && (1..=60_000).contains(&self.max_duration_ms),
            "maintenance budget outside supported bounds"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionReason {
    Active,
    Predecessor,
    Reader,
    History,
    Conversion,
    Checkpoint,
    HistorySelection,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtectedRevision {
    pub token: RevisionToken,
    pub reasons: Vec<ProtectionReason>,
    pub bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionCandidate {
    pub token: RevisionToken,
    pub bytes: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceTrigger {
    PostCommit,
    Pressure,
    ExplicitMaintenance,
    ExplicitLegacyPrune,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceState {
    Complete,
    Pending,
    Blocked,
    Failed,
    Disabled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceReceipt {
    pub head: Option<RevisionToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_digest: Option<Sha256Digest>,
    pub trigger: MaintenanceTrigger,
    pub state: MaintenanceState,
    pub started_unix_ms: u64,
    pub finished_unix_ms: u64,
    pub examined: usize,
    pub removed: usize,
    #[serde(default)]
    pub removed_objects: usize,
    #[serde(default)]
    pub retired_receipts: usize,
    pub reclaimed_bytes: u64,
    pub pending: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<QuotaPressure>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionStatus {
    pub collection: String,
    pub active: Option<RevisionToken>,
    pub policy: PolicyReceipt,
    pub history_supported: bool,
    pub checkpoints_supported: bool,
    pub logical_bytes: Option<u64>,
    pub protected_bytes: Option<u64>,
    pub protected: Vec<ProtectedRevision>,
    pub legacy_protected: Vec<LegacyRevision>,
    pub protected_truncated: bool,
    pub pending_intents: usize,
    pub pending_bytes: Option<u64>,
    pub last_maintenance: Option<MaintenanceReceipt>,
    pub pressure: Option<QuotaPressure>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionPreview {
    pub status: RetentionStatus,
    pub candidates: Vec<RetentionCandidate>,
    pub legacy_candidates: Vec<LegacyRevision>,
    pub candidates_truncated: bool,
    /// Potential from bounded selected candidates and verified orphan objects,
    /// not guaranteed one-pass reclaim. Apply separately enforces removal caps.
    pub estimated_reclaimable_bytes: Option<u64>,
    pub unreferenced_objects: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaintenanceRun {
    pub receipt: MaintenanceReceipt,
    pub receipt_persisted: bool,
    pub status: Option<RetentionStatus>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceStage {
    IntentDurable,
    RetirementAuthorized,
    Detached,
    PhysicalRemovalComplete,
    IntentComplete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceIntent {
    pub version: u32,
    pub id: Sha256Digest,
    pub token: RevisionToken,
    pub receipt: PublicationReceipt,
    pub authorized_head: RevisionToken,
    pub policy_digest: Sha256Digest,
    pub trigger: MaintenanceTrigger,
    pub stage: MaintenanceStage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetirementProof {
    pub token: RevisionToken,
    pub receipt_digest: Sha256Digest,
    pub intent_id: Sha256Digest,
    pub authorized_head: RevisionToken,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(u128::from(u64::MAX)) as u64)
}
fn digest<T: Serialize>(value: &T) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"retention-metadata/1",
        &[&serde_json::to_vec(value)?],
    ))
}
fn collection(store: &FileStore) -> Result<String> {
    Ok(store
        .directory()
        .file_name()
        .and_then(|name| name.to_str())
        .context("collection name missing")?
        .into())
}
pub(crate) fn check_writer(store: &FileStore, writer: &WriterLock, quota: &Quota) -> Result<()> {
    ensure!(
        writer.path == store.directory().canonicalize()?.join("writer.lock"),
        "foreign collection writer guard"
    );
    ensure!(
        store.directory().canonicalize()?
            == quota
                .root()
                .join("collections")
                .join(collection(store)?)
                .canonicalize()?,
        "maintenance quota root mismatch"
    );
    Ok(())
}
fn plain(path: &Path) -> Result<()> {
    ensure!(
        !path.symlink_metadata()?.file_type().is_symlink(),
        "linked maintenance state"
    );
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let metadata = match path.symlink_metadata() {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_RECORD_BYTES as u64,
        "invalid bounded maintenance metadata"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "maintenance metadata too large"
    );
    Ok(Some(serde_json::from_slice(&bytes)?))
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
pub(crate) fn write_json<T: Serialize>(
    store: &FileStore,
    path: &Path,
    value: &T,
    reservation: &Reservation,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "maintenance metadata exceeds 16 KiB"
    );
    write_bytes(store, path, &bytes, reservation)
}
fn write_bytes(
    store: &FileStore,
    path: &Path,
    bytes: &[u8],
    reservation: &Reservation,
) -> Result<()> {
    ensure!(
        bytes.len() <= 131_072,
        "maintenance metadata exceeds 128 KiB"
    );
    let parent = path.parent().context("metadata parent missing")?;
    ensure!(
        parent.starts_with(store.directory()),
        "metadata outside collection"
    );
    let mut current = store.directory().to_path_buf();
    plain(&current)?;
    for part in parent.strip_prefix(store.directory())?.components() {
        current.push(part);
        if !current.exists() {
            fs::create_dir(&current)?;
        }
        plain(&current)?;
    }
    if path.exists() {
        plain(path)?;
    }
    reservation.charge(bytes.len() as u64)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    sync_parent(parent)
}

fn intent_path(store: &FileStore, token: &RevisionToken) -> std::path::PathBuf {
    store
        .directory()
        .join("maintenance/intents")
        .join(format!("{}.json", token.revision))
}

pub(crate) fn retirement_pending(store: &FileStore, token: &RevisionToken) -> Result<bool> {
    Ok(intent_paths(store)?
        .iter()
        .any(|path| *path == intent_path(store, token)))
}

pub(crate) fn retirement_count(store: &FileStore) -> Result<usize> {
    Ok(intent_paths(store)?.len())
}
fn manifest_path(store: &FileStore, token: &RevisionToken) -> std::path::PathBuf {
    store
        .directory()
        .join("maintenance/manifests")
        .join(format!("{}.json", token.revision))
}

#[allow(clippy::too_many_arguments)]
fn process_intent(
    store: &FileStore,
    intent: MaintenanceIntent,
    active: &PublicationReceipt,
    reservation: &Reservation,
    work: &mut WorkBudget,
    report: &mut MaintenanceReceipt,
    observer: &dyn Fn(MaintenanceStage, &MaintenanceIntent) -> Result<()>,
) -> Result<()> {
    let source = store
        .directory()
        .join("generations")
        .join(intent.token.revision.to_string());
    let Some(_lease) = crate::retention::exclusive(&source)? else {
        return Ok(());
    };
    process_locked(store, intent, active, reservation, work, report, observer)
}

#[allow(clippy::too_many_arguments)]
fn process_locked(
    store: &FileStore,
    mut intent: MaintenanceIntent,
    active: &PublicationReceipt,
    reservation: &Reservation,
    work: &mut WorkBudget,
    report: &mut MaintenanceReceipt,
    observer: &dyn Fn(MaintenanceStage, &MaintenanceIntent) -> Result<()>,
) -> Result<()> {
    ensure!(
        intent.version == 1
            && intent.token == intent.receipt.token
            && intent.token.incarnation == active.token.incarnation
            && intent.token.generation < active.token.generation
            && intent.id
                == framed_digest(
                    b"retirement-intent/1",
                    &[&serde_json::to_vec(&intent.receipt)?]
                ),
        "invalid or foreign maintenance intent"
    );
    let generations = store.directory().join("generations");
    plain(&generations)?;
    let source = generations.join(intent.token.revision.to_string());
    let detached = generations.join(format!("removing-{}", intent.token.revision));
    let path = intent_path(store, &intent.token);
    let saved_manifest = manifest_path(store, &intent.token);
    let proof = crate::sqlite::retirement_proof(store.directory(), &intent.token)?;
    if let Some(proof) = &proof {
        work.metadata(serde_json::to_vec(proof)?.len() as u64)?;
        ensure!(
            proof.intent_id == intent.id && proof.receipt_digest == digest(&intent.receipt)?,
            "maintenance proof binding mismatch"
        );
    }
    if proof.is_none() {
        // A pre-authorization crash cannot override a subsequently protected head.
        if intent.trigger != MaintenanceTrigger::ExplicitLegacyPrune
            && retained_predecessors(
                store,
                active,
                store.retention_policy()?.policy.searchable_predecessors,
                work,
            )?
            .contains(&intent.token)
        {
            return Ok(());
        }
        ensure!(
            !detached.exists(),
            "detached representation lacks retirement authorization"
        );
        let Some(bytes) = tree_bytes(&source, work)? else {
            return Ok(());
        };
        if bytes > work.limit.max_removal_bytes.saturating_sub(work.removed) {
            return Ok(());
        }
        crate::blobs::verify_v2_for_query(store.directory(), &intent.receipt)?;
        let bytes = crate::blobs::read_verified(
            &source.join("manifest.json"),
            &intent.receipt.manifest_digest.to_string(),
            131_072,
        )?;
        work.metadata(bytes.len() as u64)?;
        write_bytes(store, &saved_manifest, &bytes, reservation)?;
        intent.authorized_head = active.token;
        write_json(store, &path, &intent, reservation)?;
        crate::sqlite::authorize_retirement(
            store.directory(),
            &RetirementProof {
                token: intent.token,
                receipt_digest: digest(&intent.receipt)?,
                intent_id: intent.id,
                authorized_head: active.token,
            },
            &intent.receipt,
            reservation,
        )?;
        report.retired_receipts += 1;
        intent.stage = MaintenanceStage::RetirementAuthorized;
        write_json(store, &path, &intent, reservation)?;
        observer(MaintenanceStage::RetirementAuthorized, &intent)?;
    }
    ensure!(
        !(source.exists() && detached.exists()),
        "retirement has two full representations"
    );
    if source.exists() {
        crate::blobs::verify_v2_for_query(store.directory(), &intent.receipt)?;
        fs::rename(&source, &detached)?;
        sync_parent(&generations)?;
        intent.stage = MaintenanceStage::Detached;
        write_json(store, &path, &intent, reservation)?;
        observer(MaintenanceStage::Detached, &intent)?;
    }
    if detached.exists() {
        if !delete_verified_remaining(store, &detached, &saved_manifest, &intent.receipt, work)? {
            return Ok(());
        }
        report.removed += 1;
        report.reclaimed_bytes = work.removed;
    } else {
        ensure!(
            matches!(
                intent.stage,
                MaintenanceStage::Detached
                    | MaintenanceStage::PhysicalRemovalComplete
                    | MaintenanceStage::IntentComplete
            ),
            "authorized representation disappeared before detach"
        );
    }
    intent.stage = MaintenanceStage::PhysicalRemovalComplete;
    write_json(store, &path, &intent, reservation)?;
    observer(MaintenanceStage::PhysicalRemovalComplete, &intent)?;
    intent.stage = MaintenanceStage::IntentComplete;
    write_json(store, &path, &intent, reservation)?;
    observer(MaintenanceStage::IntentComplete, &intent)?;
    if saved_manifest.exists() {
        plain(&saved_manifest)?;
        fs::remove_file(&saved_manifest)?;
        sync_parent(saved_manifest.parent().unwrap())?;
    }
    fs::remove_file(&path)?;
    sync_parent(path.parent().unwrap())?;
    Ok(())
}

/// Verify every remaining component against the separately retained anchored
/// manifest. Missing components are allowed only after durable authorization;
/// this makes interruption halfway through physical deletion resumable.
pub(crate) fn delete_verified_remaining(
    store: &FileStore,
    directory: &Path,
    saved_manifest: &Path,
    receipt: &PublicationReceipt,
    work: &mut WorkBudget,
) -> Result<bool> {
    use sha2::{Digest, Sha256};
    plain(
        saved_manifest
            .parent()
            .context("maintenance manifest parent missing")?,
    )?;
    let bytes = crate::blobs::read_verified(
        saved_manifest,
        &receipt.manifest_digest.to_string(),
        131_072,
    )?;
    work.metadata(bytes.len() as u64)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::from_value::<RevisionToken>(manifest["token"].clone())? == receipt.token,
        "retirement manifest scope mismatch"
    );
    let components = manifest["components"]
        .as_object()
        .context("retirement components missing")?;
    ensure!(
        components.len() <= 1024,
        "retirement component cap exceeded"
    );
    let mut files = vec![];
    let mut directories = vec![];
    let mut pending = vec![directory.to_path_buf()];
    while let Some(parent) = pending.pop() {
        plain(&parent)?;
        directories.push(parent.clone());
        for entry in fs::read_dir(&parent)? {
            work.entry()?;
            let entry = entry?;
            let metadata = entry.file_type()?;
            ensure!(!metadata.is_symlink(), "linked detached component");
            let path = entry.path();
            let relative = path
                .strip_prefix(directory)?
                .to_str()
                .context("non-UTF-8 detached component")?
                .replace('\\', "/");
            if metadata.is_dir() {
                ensure!(
                    components
                        .keys()
                        .any(|name| name.starts_with(&format!("{relative}/"))),
                    "unknown detached directory"
                );
                pending.push(path);
                continue;
            }
            ensure!(metadata.is_file(), "nonregular detached component");
            let (expected_hash, expected_bytes) = if relative == "manifest.json" {
                (receipt.manifest_digest.to_string(), bytes.len() as u64)
            } else {
                let component = components
                    .get(&relative)
                    .context("unknown detached component")?;
                (
                    component["sha256"]
                        .as_str()
                        .context("component hash missing")?
                        .to_owned(),
                    component["bytes"]
                        .as_u64()
                        .context("component size missing")?,
                )
            };
            let mut input = File::open(&path)?;
            ensure!(
                input.metadata()?.len() == expected_bytes,
                "detached component size mismatch"
            );
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65_536];
            loop {
                if work.expired() {
                    return Ok(false);
                }
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            ensure!(
                format!("{:x}", hash.finalize()) == expected_hash,
                "detached component checksum mismatch"
            );
            files.push((path, expected_bytes));
            ensure!(files.len() <= 1025, "detached component count exceeded");
        }
    }
    // Complete verification precedes each deletion pass. Each file is atomic at
    // this level; an oversized file leaves pending work for a larger budget.
    for (path, bytes) in files {
        if work.expired() || bytes > work.limit.max_removal_bytes.saturating_sub(work.removed) {
            return Ok(false);
        }
        fs::remove_file(path)?;
        work.removed += bytes;
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in directories {
        fs::remove_dir(path)?;
    }
    sync_parent(directory.parent().unwrap())?;
    let _ = store;
    Ok(true)
}

#[derive(Default)]
struct ObjectProjection {
    objects: usize,
    bytes: u64,
    pending: bool,
}
fn gc(
    store: &FileStore,
    work: &mut WorkBudget,
    excluded: &BTreeSet<String>,
    dry_run: bool,
    report: &mut ObjectProjection,
) -> Result<()> {
    if !intent_paths(store)?.is_empty()
        || !legacy::paths(store)?.is_empty()
        || crate::history::pending_count(store)? > 0
    {
        report.pending = true;
        return Ok(());
    }
    ensure!(
        !store.directory().join("checkpoints").exists(),
        "checkpoint roots require a qualified GC adapter"
    );
    let mut sources = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut embeddings = BTreeSet::new();
    let mut models = BTreeSet::new();
    for checkpoint in crate::sqlite::history_admin(store.directory())?
        .checkpoints
        .values()
    {
        crate::history::describe_revision(store, &checkpoint.publication.token, work)?;
    }
    for history in crate::history::verified_roots_work(store, work)? {
        for dependency in history.descriptor.dependencies {
            match dependency.kind {
                crate::history::DependencyKind::Source => {
                    sources.insert(dependency.digest.to_string());
                }
                crate::history::DependencyKind::Payload => {
                    artifacts.insert(dependency.digest.to_string());
                }
                _ => anyhow::bail!("unsupported historical GC dependency"),
            }
        }
    }
    let base = store.directory().join("generations");
    plain(&base)?;
    for entry in fs::read_dir(&base)? {
        work.entry()?;
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        ensure!(
            crate::blobs::valid_digest(&name),
            "unknown detached/staged generation prevents automatic object GC"
        );
        if work.expired() {
            report.pending = true;
            return Ok(());
        }
        let manifest: serde_json::Value = {
            let path = entry.path().join("manifest.json");
            let bytes = fs::metadata(&path)?.len();
            work.metadata(bytes)?;
            ensure!(bytes <= 131_072, "oversized GC manifest");
            serde_json::from_slice(&fs::read(&path)?)?
        };
        if manifest["schema"].as_u64() == Some(1) {
            let snapshot = legacy::snapshot(store, &entry.path(), work)?
                .context("legacy GC catalog missing")?;
            if excluded.contains(&name) {
                continue;
            }
            for file in snapshot.artifacts.values() {
                if let Some(reference) = &file.retained {
                    artifacts.insert(reference.object.clone());
                    if reference.raw_source {
                        sources.insert(file.content_hash.clone());
                    }
                }
            }
            continue;
        }
        let token: RevisionToken = serde_json::from_value(manifest["token"].clone())?;
        ensure!(token.revision.to_string() == name, "foreign GC generation");
        let receipt = crate::sqlite::receipt(store.directory(), &token)?
            .context("uncommitted representation prevents automatic object GC")?;
        let inventory_size = entry
            .path()
            .join("inventory.json")
            .symlink_metadata()?
            .len();
        work.metadata(inventory_size)?;
        let sidecar_path = entry.path().join("embedding-cache.json");
        match sidecar_path.symlink_metadata() {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && metadata.len() <= crate::vector_cache::MAX_CACHE_MANIFEST_BYTES as u64,
                    "invalid embedding sidecar during GC"
                );
                work.metadata(metadata.len())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let model_references =
            if manifest["schema"].as_u64() == Some(3) && !manifest["model_sha256"].is_null() {
                let sidecar = entry.path().join("model-objects.json");
                let size = sidecar.symlink_metadata()?.len();
                work.metadata(size)?;
                let references = store.model_object_references_with_budget(
                    &receipt,
                    work.verification_limit.saturating_sub(work.verification),
                )?;
                let bytes = references.iter().try_fold(0u64, |sum, reference| {
                    sum.checked_add(reference.bytes)
                        .context("model dependency size overflow")
                })?;
                work.verify(bytes)?;
                // Full representation verification below authenticates these assets again.
                work.verify(bytes)?;
                references
            } else {
                vec![]
            };
        for component in manifest["components"]
            .as_object()
            .context("GC components missing")?
            .values()
        {
            work.verify(
                component["bytes"]
                    .as_u64()
                    .context("GC component size missing")?,
            )?;
        }
        let inventory = crate::blobs::verify_v2_generation(store.directory(), &receipt)?;
        let embedding_references = store.embedding_cache_references(&receipt)?;
        if excluded.contains(&name) {
            continue;
        }
        embeddings.extend(
            embedding_references
                .into_iter()
                .map(|reference| reference.key.to_string()),
        );
        models.extend(
            model_references
                .into_iter()
                .map(|reference| reference.sha256.to_string()),
        );
        for file in inventory.files.values() {
            artifacts.insert(file.state.payload_digest.to_string());
            if file.raw_source {
                sources.insert(file.state.source_hash.to_string());
            }
        }
    }
    for (kind, retained) in [
        ("sources", sources),
        ("artifacts", artifacts),
        ("embeddings", embeddings),
        ("models", models),
    ] {
        let path = store.directory().join("objects").join(kind);
        if !path.exists() {
            continue;
        }
        plain(path.parent().unwrap())?;
        plain(&path)?;
        for entry in fs::read_dir(path)? {
            work.entry()?;
            if work.expired() {
                report.pending = true;
                return Ok(());
            }
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            ensure!(
                entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
                "invalid object during GC"
            );
            if !crate::blobs::valid_digest(&name) || retained.contains(&name) {
                continue;
            }
            let bytes = entry.metadata()?.len();
            let projected = if dry_run { report.bytes } else { 0 };
            if bytes
                > work
                    .limit
                    .max_removal_bytes
                    .saturating_sub(work.removed)
                    .saturating_sub(projected)
            {
                report.pending = true;
                continue;
            }
            if !dry_run {
                fs::remove_file(entry.path())?;
                work.removed += bytes;
            }
            report.objects += 1;
            report.bytes += bytes;
        }
    }
    Ok(())
}

impl FileStore {
    pub fn retention_policy(&self) -> Result<PolicyReceipt> {
        if let Some(receipt) = read_json::<PolicyReceipt>(&self.directory().join("retention.json"))?
        {
            receipt.policy.validate()?;
            ensure!(
                receipt.digest == digest(&receipt.policy)?,
                "retention policy digest mismatch"
            );
            Ok(receipt)
        } else {
            let policy = RetentionPolicy::default();
            Ok(PolicyReceipt {
                digest: digest(&policy)?,
                policy,
                origin: PolicyOrigin::ExistingManual,
            })
        }
    }
    pub fn last_maintenance(&self) -> Result<Option<MaintenanceReceipt>> {
        let path = self.directory().join("maintenance");
        if path.exists() {
            plain(&path)?;
        }
        read_json(&path.join("status.json"))
    }
    pub fn initialize_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        mode: RetentionMode,
    ) -> Result<PolicyReceipt> {
        ensure!(
            !self.directory().join("retention.json").exists(),
            "retention policy already initialized"
        );
        self.write_retention(
            writer,
            quota,
            RetentionPolicy {
                mode,
                ..Default::default()
            },
            PolicyOrigin::NewCollectionDefault,
        )
    }
    pub fn configure_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        policy: RetentionPolicy,
    ) -> Result<PolicyReceipt> {
        self.write_retention(writer, quota, policy, PolicyOrigin::Explicit)
    }
    fn write_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        policy: RetentionPolicy,
        origin: PolicyOrigin,
    ) -> Result<PolicyReceipt> {
        check_writer(self, writer, quota)?;
        policy.validate()?;
        let receipt = PolicyReceipt {
            digest: digest(&policy)?,
            policy,
            origin,
        };
        let reservation = quota.reserve(&collection(self)?, 32 * 1024)?;
        write_json(
            self,
            &self.directory().join("retention.json"),
            &receipt,
            &reservation,
        )?;
        Ok(receipt)
    }
}

#[derive(Debug)]
pub(crate) struct WorkLimit(pub(crate) &'static str);
impl std::fmt::Display for WorkLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "maintenance {} budget exhausted", self.0)
    }
}
impl std::error::Error for WorkLimit {}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanCursor {
    incarnation: airk_core::identity::Incarnation,
    after_generation: Option<u64>,
}

pub(crate) struct WorkBudget {
    limit: MaintenanceBudget,
    started: Instant,
    metadata: u64,
    verification: u64,
    verification_limit: u64,
    entries: usize,
    removed: u64,
    planning: bool,
    predecessors: Option<(RevisionToken, u8, Vec<RevisionToken>)>,
    scan_after: Option<u64>,
    scanned_through: Option<u64>,
}

impl WorkBudget {
    pub(crate) fn history(limit: crate::history_policy::HistoryWorkBudget) -> Result<Self> {
        limit.validate()?;
        let mut work = Self::new(limit.maintenance)?;
        work.verification_limit = limit.verification_bytes;
        Ok(work)
    }
    pub(crate) fn verify(&mut self, bytes: u64) -> Result<()> {
        self.verification = self
            .verification
            .checked_add(bytes)
            .context("verification accounting overflow")?;
        if self.expired() {
            return Err(WorkLimit("time").into());
        }
        if self.verification > self.verification_limit {
            return Err(WorkLimit("verification bytes").into());
        }
        Ok(())
    }
    pub(crate) fn remove_file(&mut self, path: &Path, bytes: u64) -> Result<bool> {
        if self.expired() || bytes > self.limit.max_removal_bytes.saturating_sub(self.removed) {
            return Ok(false);
        }
        fs::remove_file(path)?;
        self.removed += bytes;
        Ok(true)
    }
    pub(crate) fn removed_bytes(&self) -> u64 {
        self.removed
    }
    pub(crate) fn new(limit: MaintenanceBudget) -> Result<Self> {
        limit.validate()?;
        Ok(Self {
            limit,
            started: Instant::now(),
            metadata: 0,
            verification: 0,
            verification_limit: crate::history_policy::HistoryWorkBudget::default()
                .verification_bytes,
            entries: 0,
            removed: 0,
            planning: false,
            predecessors: None,
            scan_after: None,
            scanned_through: None,
        })
    }
    pub(crate) fn expired(&self) -> bool {
        self.started.elapsed() >= Duration::from_millis(self.limit.max_duration_ms)
    }
    pub(crate) fn metadata(&mut self, bytes: u64) -> Result<()> {
        self.metadata = self
            .metadata
            .checked_add(bytes)
            .context("metadata accounting overflow")?;
        if self.metadata > self.limit.max_metadata_bytes {
            return Err(WorkLimit("metadata bytes").into());
        }
        Ok(())
    }
    pub(crate) fn entry(&mut self) -> Result<()> {
        self.entries += 1;
        if self.entries > MAX_SCAN_ENTRIES {
            return Err(WorkLimit("entries").into());
        }
        Ok(())
    }
}

fn tree_bytes(path: &Path, work: &mut WorkBudget) -> Result<Option<u64>> {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(path) = pending.pop() {
        if work.expired() {
            return Ok(None);
        }
        work.entry()?;
        let metadata = path.symlink_metadata()?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "linked maintenance candidate"
        );
        if metadata.is_file() {
            total = total
                .checked_add(metadata.len())
                .context("maintenance size overflow")?;
        } else {
            ensure!(metadata.is_dir(), "nonregular maintenance candidate");
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(Some(total))
}

fn current_receipt(store: &FileStore) -> Result<PublicationReceipt> {
    let header = crate::sqlite::persistent_revision(store.directory())?
        .context("retention requires indexed collection")?;
    let token = header
        .token
        .context("automatic retention requires v2; migrate first")?;
    crate::sqlite::receipt(store.directory(), &token)?.context("active publication receipt missing")
}

// Follow authenticated ancestry only; raising retention cannot restore retired data.
// Cache within a bounded pass so each candidate does not reread the active inventory.
fn retained_predecessors(
    store: &FileStore,
    active: &PublicationReceipt,
    count: u8,
    work: &mut WorkBudget,
) -> Result<Vec<RevisionToken>> {
    if let Some((head, prior_count, tokens)) = &work.predecessors
        && *head == active.token
        && *prior_count == count
    {
        return Ok(tokens.clone());
    }
    let mut tokens = Vec::new();
    let mut receipt = active.clone();
    for _ in 0..count {
        let Some(parent) = active_parent(store, &receipt, work)? else {
            break;
        };
        ensure!(
            parent.incarnation == active.token.incarnation
                && parent.generation < receipt.token.generation,
            "invalid retention ancestry"
        );
        let Some(previous) = crate::sqlite::receipt(store.directory(), &parent)? else {
            break;
        };
        if crate::sqlite::retirement_proof(store.directory(), &parent)?.is_some()
            || crate::sqlite::history_binding(store.directory(), &parent)?.is_some()
        {
            break;
        }
        tokens.push(parent);
        receipt = previous;
    }
    work.predecessors = Some((active.token, count, tokens.clone()));
    Ok(tokens)
}

fn active_parent(
    store: &FileStore,
    receipt: &PublicationReceipt,
    work: &mut WorkBudget,
) -> Result<Option<RevisionToken>> {
    let generation = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let inventory_path = generation.join("inventory.json");
    work.metadata(inventory_path.symlink_metadata()?.len())?;
    let bytes = crate::blobs::read_verified(
        &inventory_path,
        &receipt.inventory_digest.to_string(),
        work.limit.max_metadata_bytes,
    )?;
    let inventory: airk_core::publication::InventoryV2 = serde_json::from_slice(&bytes)?;
    inventory.validate()?;
    ensure!(
        inventory.incarnation == receipt.token.incarnation
            && inventory.generation == receipt.token.generation,
        "active inventory scope mismatch"
    );
    let lineage_path = generation.join("lineage.json");
    work.metadata(lineage_path.symlink_metadata()?.len())?;
    let bytes = crate::blobs::read_verified(
        &lineage_path,
        &inventory.lineage_digest.to_string(),
        work.limit.max_metadata_bytes,
    )?;
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&bytes)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == receipt.token.incarnation
            && lineage.observation.generation == receipt.token.generation,
        "active lineage scope mismatch"
    );
    Ok(lineage.observation.parent)
}

fn intent_paths(store: &FileStore) -> Result<Vec<std::path::PathBuf>> {
    let base = store.directory().join("maintenance/intents");
    if !base.exists() {
        return Ok(vec![]);
    }
    plain(base.parent().unwrap())?;
    plain(&base)?;
    let mut paths = vec![];
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        ensure!(
            paths.len() < MAX_INTENTS,
            "maintenance intent bound exceeded"
        );
        let name = entry.file_name().to_string_lossy().into_owned();
        ensure!(
            name.strip_suffix(".json")
                .is_some_and(crate::blobs::valid_digest),
            "unknown maintenance intent name"
        );
        ensure!(
            entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
            "invalid maintenance intent"
        );
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn intent_tokens(directory: &Path) -> Result<Vec<RevisionToken>> {
    let store = FileStore::new(directory);
    let mut tokens = Vec::new();
    for path in intent_paths(&store)? {
        let intent =
            read_json::<MaintenanceIntent>(&path)?.context("missing maintenance intent")?;
        ensure!(
            intent.version == 1
                && path == intent_path(&store, &intent.token)
                && intent.token == intent.receipt.token
                && intent.id
                    == framed_digest(
                        b"retirement-intent/1",
                        &[&serde_json::to_vec(&intent.receipt)?]
                    ),
            "invalid maintenance intent during lifecycle operation"
        );
        tokens.push(intent.token);
    }
    Ok(tokens)
}
pub(crate) fn legacy_intent_revisions(directory: &Path) -> Result<Vec<String>> {
    legacy::revisions(&FileStore::new(directory))
}

fn preview(
    store: &FileStore,
    proposed: Option<&RetentionPolicy>,
    budget: MaintenanceBudget,
    legacy: bool,
) -> Result<RetentionPreview> {
    let mut work = WorkBudget::new(budget)?;
    preview_work(store, proposed, legacy, &mut work)
}

fn preview_work(
    store: &FileStore,
    proposed: Option<&RetentionPolicy>,
    legacy: bool,
    work: &mut WorkBudget,
) -> Result<RetentionPreview> {
    let budget = work.limit;
    let mut policy = store.retention_policy()?;
    if let Some(proposed) = proposed {
        proposed.validate()?;
        policy.policy = proposed.clone();
        policy.digest = digest(proposed)?;
    }
    let active = current_receipt(store)?;
    let parent_result =
        retained_predecessors(store, &active, policy.policy.searchable_predecessors, work);
    let history_state = crate::sqlite::history_admin(store.directory())?;
    let history_plan = if history_state.policy.policy.enabled {
        match crate::history_policy::preview_work(
            store,
            crate::history_policy::effective_collection_limit(store)?,
            &crate::history_policy::HistorySelectionRequest::default(),
            work,
        ) {
            Ok(plan) => Some(plan),
            Err(error) if error.is::<WorkLimit>() => None,
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let history_incomplete = history_state.policy.policy.enabled
        && history_plan.as_ref().is_none_or(|plan| !plan.complete);
    let mut status = RetentionStatus {
        collection: collection(store)?,
        active: Some(active.token),
        policy,
        history_supported: true,
        checkpoints_supported: true,
        logical_bytes: None,
        protected_bytes: None,
        protected: vec![],
        legacy_protected: vec![],
        protected_truncated: false,
        pending_intents: intent_paths(store)?.len()
            + legacy::paths(store)?.len()
            + crate::history::pending_count(store)?,
        pending_bytes: None,
        last_maintenance: store.last_maintenance()?,
        pressure: store
            .last_maintenance()?
            .and_then(|receipt| receipt.pressure),
    };
    let parents = match parent_result {
        Ok(parent) if !work.expired() => parent,
        result if result.is_ok() || result.as_ref().is_err_and(|error| error.is::<WorkLimit>()) => {
            status.protected_truncated = true;
            return Ok(RetentionPreview {
                status,
                candidates: vec![],
                legacy_candidates: vec![],
                candidates_truncated: true,
                estimated_reclaimable_bytes: None,
                unreferenced_objects: None,
            });
        }
        Err(error) => return Err(error),
        Ok(_) => unreachable!(),
    };
    let mut candidates = vec![];
    let mut truncated = false;
    let mut reclaimable = Some(0u64);
    let receipts = crate::sqlite::maintenance_receipts(store.directory(), work.scan_after, 257)?;
    if receipts.len() > 256 {
        truncated = true;
    }
    let mut seen = BTreeSet::new();
    let mut receipts = receipts.into_iter().take(256).collect::<Vec<_>>();
    // Active and exact predecessor are protected even outside the candidate page.
    for token in std::iter::once(active.token).chain(parents.iter().copied().filter(|_| !legacy)) {
        if !receipts.iter().any(|receipt| receipt.token == token)
            && let Some(receipt) = crate::sqlite::receipt(store.directory(), &token)?
        {
            receipts.push(receipt);
        }
    }
    for receipt in receipts {
        if !seen.insert(receipt.token.revision) {
            continue;
        }
        if work.expired() {
            truncated = true;
            reclaimable = None;
            break;
        }
        if let Err(error) = work.metadata(serde_json::to_vec(&receipt)?.len() as u64) {
            if error.is::<WorkLimit>() {
                truncated = true;
                break;
            }
            return Err(error);
        }
        let path = store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string());
        if let Some(binding) = crate::sqlite::history_binding(store.directory(), &receipt.token)? {
            if !crate::history::retired_package(store, &binding, &mut |bytes| work.metadata(bytes))?
            {
                crate::history::verify_package(
                    store,
                    &store
                        .directory()
                        .join("history")
                        .join(receipt.token.revision.to_string()),
                    &binding,
                    &mut |bytes| work.metadata(bytes),
                )?;
            }
            work.scanned_through = Some(
                work.scanned_through
                    .unwrap_or(0)
                    .max(receipt.token.generation),
            );
            if status.protected.len() < 256 {
                status.protected.push(ProtectedRevision {
                    token: receipt.token,
                    reasons: vec![ProtectionReason::History],
                    bytes: None,
                });
            } else {
                status.protected_truncated = true;
            }
            continue;
        }
        if crate::history::conversion_intent(store, &receipt.token)?.is_some() {
            if status.protected.len() < 256 {
                status.protected.push(ProtectedRevision {
                    token: receipt.token,
                    reasons: vec![ProtectionReason::Conversion],
                    bytes: None,
                });
            } else {
                status.protected_truncated = true;
            }
            continue;
        }
        ensure!(
            path.is_dir(),
            "committed representation missing without retirement proof"
        );
        plain(&path)?;
        let mut reasons = vec![];
        if history_state
            .checkpoints
            .values()
            .any(|pin| pin.publication.token == receipt.token)
        {
            reasons.push(ProtectionReason::Checkpoint);
        }
        if history_incomplete
            || history_plan.as_ref().is_some_and(|plan| {
                plan.retained
                    .iter()
                    .any(|entry| entry.revision == receipt.token)
            })
        {
            reasons.push(ProtectionReason::HistorySelection);
        }
        if receipt.token != active.token && !parents.contains(&receipt.token) {
            work.scanned_through = Some(
                work.scanned_through
                    .unwrap_or(0)
                    .max(receipt.token.generation),
            );
        }
        if receipt.token == active.token {
            reasons.push(ProtectionReason::Active);
        }
        if !legacy && parents.contains(&receipt.token) {
            reasons.push(ProtectionReason::Predecessor);
        }
        let lease = if reasons.is_empty() {
            crate::retention::exclusive(&path)?
        } else {
            None
        };
        if reasons.is_empty() && lease.is_none() {
            reasons.push(ProtectionReason::Reader);
        }
        let bytes = if work.planning && !reasons.is_empty() {
            None
        } else {
            tree_bytes(&path, work)?
        };
        if bytes.is_none() && (!work.planning || reasons.is_empty()) {
            truncated = true;
        }
        if !reasons.is_empty() {
            if status.protected.len() < 256 {
                status.protected.push(ProtectedRevision {
                    token: receipt.token,
                    reasons,
                    bytes,
                });
            } else {
                status.protected_truncated = true;
            }
        } else if candidates.len() < budget.max_candidates {
            reclaimable = reclaimable.zip(bytes).and_then(|(a, b)| a.checked_add(b));
            candidates.push(RetentionCandidate {
                token: receipt.token,
                bytes,
            });
        } else {
            truncated = true;
        }
        drop(lease);
    }
    let (legacy_candidates, legacy_protected, legacy_truncated) =
        match if work.planning && !candidates.is_empty() {
            Ok((vec![], vec![], true))
        } else {
            legacy::preview(
                store,
                parents.is_empty() && status.policy.policy.searchable_predecessors > 0 && !legacy,
                work,
            )
        } {
            Ok(value) => value,
            Err(error) if error.is::<WorkLimit>() => (vec![], vec![], true),
            Err(error) => return Err(error),
        };
    status.legacy_protected = legacy_protected;
    truncated |= legacy_truncated;
    for candidate in &legacy_candidates {
        reclaimable = reclaimable
            .zip(candidate.bytes)
            .and_then(|(a, b)| a.checked_add(b));
    }
    if !truncated && !work.planning {
        status.logical_bytes = tree_bytes(store.directory(), work)?;
    }
    let mut unreferenced_objects = None;
    if !truncated && !work.planning && !work.expired() {
        let excluded = candidates
            .iter()
            .map(|item| item.token.revision.to_string())
            .chain(legacy_candidates.iter().map(|item| item.revision.clone()))
            .collect();
        let mut objects = ObjectProjection::default();
        match gc(store, work, &excluded, true, &mut objects) {
            Ok(()) if !objects.pending => {
                unreferenced_objects = Some(objects.objects);
                reclaimable = reclaimable.and_then(|bytes| bytes.checked_add(objects.bytes));
            }
            Ok(()) => {
                reclaimable = None;
            }
            Err(error) if error.is::<WorkLimit>() => {
                reclaimable = None;
                truncated = true;
            }
            Err(error) => return Err(error),
        }
    }
    if truncated || work.expired() {
        status.protected_truncated = true;
        truncated = true;
        reclaimable = None;
        unreferenced_objects = None;
    }
    Ok(RetentionPreview {
        status,
        candidates,
        legacy_candidates,
        candidates_truncated: truncated,
        estimated_reclaimable_bytes: reclaimable,
        unreferenced_objects,
    })
}

impl FileStore {
    pub fn retention_preview(
        &self,
        proposed: Option<&RetentionPolicy>,
        budget: MaintenanceBudget,
    ) -> Result<RetentionPreview> {
        let _lifecycle = self.lifecycle(false)?;
        preview(self, proposed, budget, false)
    }
    pub fn retention_status(&self) -> Result<RetentionStatus> {
        Ok(self
            .retention_preview(None, MaintenanceBudget::default())?
            .status)
    }
    pub fn maintain_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        trigger: MaintenanceTrigger,
        budget: MaintenanceBudget,
    ) -> MaintenanceRun {
        self.maintain_retention_with_observer(writer, quota, trigger, budget, &|_, _| Ok(()))
    }
    #[doc(hidden)]
    pub fn maintain_retention_with_observer(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        trigger: MaintenanceTrigger,
        budget: MaintenanceBudget,
        observer: &dyn Fn(MaintenanceStage, &MaintenanceIntent) -> Result<()>,
    ) -> MaintenanceRun {
        self.maintain_locked(Some(writer), quota, trigger, budget, observer)
    }
    fn maintain_locked(
        &self,
        writer: Option<&WriterLock>,
        quota: &Quota,
        trigger: MaintenanceTrigger,
        budget: MaintenanceBudget,
        observer: &dyn Fn(MaintenanceStage, &MaintenanceIntent) -> Result<()>,
    ) -> MaintenanceRun {
        let mut receipt = MaintenanceReceipt {
            head: None,
            policy_digest: None,
            trigger,
            state: MaintenanceState::Pending,
            started_unix_ms: now(),
            finished_unix_ms: 0,
            examined: 0,
            removed: 0,
            removed_objects: 0,
            retired_receipts: 0,
            reclaimed_bytes: 0,
            pending: true,
            error: None,
            pressure: None,
        };
        let attempt = (|| -> Result<Option<Reservation>> {
            if let Some(writer) = writer {
                check_writer(self, writer, quota)?;
            } else {
                ensure!(
                    self.directory().join("writer.lock").is_file(),
                    "compatibility prune requires caller-held writer lock"
                );
            }
            budget.validate()?;
            let active = current_receipt(self)?;
            receipt.head = Some(active.token);
            let policy = self.retention_policy()?;
            receipt.policy_digest = Some(policy.digest);
            if matches!(
                trigger,
                MaintenanceTrigger::PostCommit | MaintenanceTrigger::Pressure
            ) && policy.policy.mode == RetentionMode::Manual
            {
                receipt.state = MaintenanceState::Disabled;
                receipt.pending = !intent_paths(self)?.is_empty()
                    || !legacy::paths(self)?.is_empty()
                    || crate::history::pending_count(self)? > 0;
                return Ok(None);
            }
            let reservation =
                quota.reserve_available(&collection(self)?, budget.max_metadata_bytes)?;
            let mut work = WorkBudget::history(crate::history_policy::HistoryWorkBudget {
                maintenance: budget,
                verification_bytes: crate::history_policy::HistoryWorkBudget::default()
                    .verification_bytes,
            })?;
            let history = crate::history_policy::maintain_work(
                self,
                crate::history_policy::effective_collection_limit(self)?,
                &reservation,
                &mut work,
                budget.max_candidates,
            )?;
            receipt.examined += history.conversions.len() + history.removals.len();
            receipt.reclaimed_bytes = work.removed_bytes();
            if let Some(error) = history.error {
                anyhow::bail!("history maintenance pending: {error}");
            }
            receipt.pending |= history.pending;
            work.planning = trigger != MaintenanceTrigger::ExplicitLegacyPrune;
            let cursor_path = self.directory().join("maintenance/scan-cursor.json");
            if work.planning
                && let Some(cursor) = read_json::<ScanCursor>(&cursor_path)?
            {
                ensure!(
                    cursor.incarnation == active.token.incarnation
                        && cursor
                            .after_generation
                            .is_none_or(|generation| generation <= active.token.generation),
                    "foreign maintenance scan cursor"
                );
                work.scan_after = cursor.after_generation;
            }
            let paths = intent_paths(self)?;
            for path in paths {
                if work.expired() || receipt.examined >= budget.max_candidates {
                    break;
                }
                work.metadata(path.symlink_metadata()?.len())?;
                let intent = read_json::<MaintenanceIntent>(&path)?
                    .context("maintenance intent disappeared")?;
                ensure!(
                    path == intent_path(self, &intent.token),
                    "intent filename identity mismatch"
                );
                receipt.examined += 1;
                process_intent(
                    self,
                    intent,
                    &active,
                    &reservation,
                    &mut work,
                    &mut receipt,
                    observer,
                )?;
            }
            let mut plan = preview_work(
                self,
                None,
                trigger == MaintenanceTrigger::ExplicitLegacyPrune,
                &mut work,
            )?;
            if work.scan_after.is_some()
                && plan.candidates.is_empty()
                && !plan.candidates_truncated
                && !work.expired()
            {
                work.scan_after = None;
                work.scanned_through = None;
                plan = preview_work(
                    self,
                    None,
                    trigger == MaintenanceTrigger::ExplicitLegacyPrune,
                    &mut work,
                )?;
            }
            let policy = self.retention_policy()?;
            legacy::maintain(
                self,
                &active,
                &plan.legacy_candidates,
                &reservation,
                &mut work,
                &mut receipt,
            )?;
            for candidate in &plan.candidates {
                if work.expired() || receipt.examined >= budget.max_candidates {
                    break;
                }
                let Some(bytes) = candidate.bytes else {
                    continue;
                };
                if bytes > budget.max_removal_bytes.saturating_sub(work.removed) {
                    continue;
                }
                let source = self
                    .directory()
                    .join("generations")
                    .join(candidate.token.revision.to_string());
                let Some(_lease) = crate::retention::exclusive(&source)? else {
                    continue;
                };
                let old = crate::sqlite::receipt(self.directory(), &candidate.token)?
                    .context("candidate receipt missing")?;
                crate::blobs::verify_v2_for_query(self.directory(), &old)?;
                let intent = MaintenanceIntent {
                    version: 1,
                    id: framed_digest(b"retirement-intent/1", &[&serde_json::to_vec(&old)?]),
                    token: old.token,
                    receipt: old,
                    authorized_head: active.token,
                    policy_digest: policy.digest,
                    trigger,
                    stage: MaintenanceStage::IntentDurable,
                };
                ensure!(
                    intent_paths(self)?.len() < MAX_INTENTS,
                    "maintenance intent limit reached"
                );
                let path = intent_path(self, &intent.token);
                write_json(self, &path, &intent, &reservation)?;
                observer(MaintenanceStage::IntentDurable, &intent)?;
                receipt.examined += 1;
                // process_intent receives the already-held canonical lease.
                process_locked(
                    self,
                    intent,
                    &active,
                    &reservation,
                    &mut work,
                    &mut receipt,
                    observer,
                )?;
            }
            if work.planning {
                write_json(
                    self,
                    &cursor_path,
                    &ScanCursor {
                        incarnation: active.token.incarnation,
                        after_generation: if plan.candidates_truncated {
                            work.scanned_through
                        } else {
                            None
                        },
                    },
                    &reservation,
                )?;
            }
            work.scan_after = None;
            let remaining = preview_work(
                self,
                None,
                trigger == MaintenanceTrigger::ExplicitLegacyPrune,
                &mut work,
            )?;
            receipt.pending = history.pending
                || remaining.candidates_truncated
                || !remaining.candidates.is_empty()
                || !remaining.legacy_candidates.is_empty()
                || remaining.status.protected.iter().any(|item| {
                    item.reasons
                        .iter()
                        .any(|reason| matches!(reason, ProtectionReason::Reader))
                })
                || remaining.status.legacy_protected.iter().any(|item| {
                    item.reasons
                        .iter()
                        .any(|reason| matches!(reason, ProtectionReason::Reader))
                })
                || !intent_paths(self)?.is_empty()
                || !legacy::paths(self)?.is_empty()
                || crate::history::pending_count(self)? > 0;
            // Unselected obsolete generations remain GC roots. A retirement
            // backlog must not postpone reclaim of already unreferenced objects.
            if !work.expired() {
                let mut objects = ObjectProjection::default();
                let result = gc(self, &mut work, &BTreeSet::new(), false, &mut objects);
                receipt.removed_objects += objects.objects;
                receipt.reclaimed_bytes = work.removed;
                receipt.pending |= objects.pending;
                result?;
            } else {
                // The deadline may cross after the remaining-generation
                // preview. Unexamined objects still require a continuation.
                receipt.pending = true;
            }
            receipt.state = if receipt.pending {
                MaintenanceState::Pending
            } else {
                MaintenanceState::Complete
            };
            Ok(Some(reservation))
        })();
        let mut reservation = None;
        match attempt {
            Ok(value) => reservation = value,
            Err(error) => {
                // A later bounded GC/receipt step may fail after the remaining
                // generation preview cleared this flag. The pass did not prove
                // completion, so a subsequent maintenance run must remain due.
                receipt.pending = true;
                receipt.pressure = error
                    .chain()
                    .find_map(|cause| cause.downcast_ref::<QuotaPressure>())
                    .cloned();
                receipt.state = if error.is::<WorkLimit>()
                    || (receipt.pressure.is_some() && receipt.reclaimed_bytes > 0)
                {
                    MaintenanceState::Pending
                } else if receipt.pressure.is_some() {
                    MaintenanceState::Blocked
                } else {
                    MaintenanceState::Failed
                };
                receipt.error = Some(format!("{error:#}").chars().take(4096).collect());
            }
        }
        receipt.finished_unix_ms = now();
        // Status failure never changes publication success. Best effort uses a
        // separate small reservation after a failed operation's clones are gone.
        let status_reservation =
            reservation.or_else(|| quota.reserve(&collection(self).ok()?, 32 * 1024).ok());
        let receipt_persisted = status_reservation.as_ref().is_some_and(|reservation| {
            write_json(
                self,
                &self.directory().join("maintenance/status.json"),
                &receipt,
                reservation,
            )
            .is_ok()
        });
        MaintenanceRun {
            receipt,
            receipt_persisted,
            status: None,
        }
    }
    pub fn prune_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        dry_run: bool,
    ) -> Result<crate::retention::PruneReport> {
        check_writer(self, writer, quota)?;
        prune_compat_inner(self, Some(writer), quota, dry_run)
    }
}

/// Compatibility for the pre-v2 free function; its caller-held writer lock
/// contract is unchanged. New callers pass the guard to FileStore directly.
pub(crate) fn prune_compat(
    directory: &Path,
    active: &str,
    dry_run: bool,
) -> Result<crate::retention::PruneReport> {
    let store = FileStore::new(directory);
    let current = current_receipt(&store)?;
    ensure!(
        current.token.revision.to_string() == active,
        "stale explicit prune head"
    );
    let root = directory
        .parent()
        .and_then(Path::parent)
        .context("prune root missing")?;
    let quota = Quota::open(root)?;
    prune_compat_inner(&store, None, &quota, dry_run)
}
fn prune_compat_inner(
    store: &FileStore,
    writer: Option<&WriterLock>,
    quota: &Quota,
    dry_run: bool,
) -> Result<crate::retention::PruneReport> {
    let budget = MaintenanceBudget {
        max_candidates: 256,
        max_metadata_bytes: 64 * 1024 * 1024,
        max_removal_bytes: 64 * 1024 * 1024 * 1024,
        max_duration_ms: 60_000,
    };
    let plan = preview(store, None, budget, true)?;
    let mut report = crate::retention::PruneReport {
        dry_run,
        active: plan
            .status
            .active
            .context("prune active missing")?
            .revision
            .to_string(),
        reclaimable_bytes: plan.estimated_reclaimable_bytes.unwrap_or(0),
        removed_bytes: 0,
        candidates: plan
            .candidates
            .iter()
            .map(|candidate| candidate.token.revision.to_string())
            .chain(
                plan.legacy_candidates
                    .iter()
                    .map(|candidate| candidate.revision.clone()),
            )
            .collect(),
        pinned: plan
            .status
            .protected
            .iter()
            .filter(|protected| {
                protected
                    .reasons
                    .iter()
                    .any(|reason| matches!(reason, ProtectionReason::Reader))
            })
            .map(|protected| protected.token.revision.to_string())
            .chain(
                plan.status
                    .legacy_protected
                    .iter()
                    .filter(|protected| {
                        protected
                            .reasons
                            .iter()
                            .any(|reason| matches!(reason, ProtectionReason::Reader))
                    })
                    .map(|protected| protected.revision.clone()),
            )
            .collect(),
        unreferenced_objects: plan.unreferenced_objects.unwrap_or(0),
        retired_receipts: 0,
        receipt_cleanup_pending: plan.candidates_truncated
            || plan.status.pending_intents > 0
            || plan.unreferenced_objects.is_none()
            || plan.estimated_reclaimable_bytes.is_none(),
        receipt_cleanup_error: None,
    };
    if !dry_run {
        let run = store.maintain_locked(
            writer,
            quota,
            MaintenanceTrigger::ExplicitLegacyPrune,
            budget,
            &|_, _| Ok(()),
        );
        report.removed_bytes = run.receipt.reclaimed_bytes;
        report.retired_receipts = run.receipt.retired_receipts;
        report.unreferenced_objects = run.receipt.removed_objects;
        report.receipt_cleanup_pending = run.receipt.pending || !run.receipt_persisted;
        report.receipt_cleanup_error = run.receipt.error.clone();
        ensure!(
            !matches!(
                run.receipt.state,
                MaintenanceState::Failed | MaintenanceState::Blocked
            ),
            "explicit prune failed: {}",
            run.receipt.error.unwrap_or_default()
        );
    }
    Ok(report)
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::history_policy::HistoryWorkBudget;

    #[test]
    fn catalog_authentication_is_not_limited_to_metadata_reservation() -> Result<()> {
        let gib = 1024 * 1024 * 1024;
        // Preview and automatic apply must both be able to authenticate a large
        // catalog. This accounts bytes without allocating a multi-GiB fixture.
        for mut work in [
            WorkBudget::new(MaintenanceBudget::default())?,
            WorkBudget::history(HistoryWorkBudget::default())?,
        ] {
            work.metadata(64 * 1024 * 1024)?;
            work.verify(2 * gib)?;
            work.verify(6 * gib)?;
            assert!(work.verify(1).is_err(), "verification must remain bounded");
            assert_eq!(work.metadata, 64 * 1024 * 1024);
        }
        Ok(())
    }

    #[test]
    fn explicit_smaller_verification_budget_still_stops_work() -> Result<()> {
        let mut work = WorkBudget::history(HistoryWorkBudget {
            verification_bytes: 256 * 1024 * 1024,
            ..Default::default()
        })?;
        work.verify(256 * 1024 * 1024)?;
        assert!(work.verify(1).is_err());
        Ok(())
    }
}
