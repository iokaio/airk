// SPDX-License-Identifier: Apache-2.0
//! Public contracts for unsupported historical controls and native writer guards.
use crate::{
    FileStore, WriterLock,
    quota::{Quota, QuotaPressure},
};
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub fn validate(self) -> Result<()> {
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
pub struct LegacyRevision {
    pub revision: String,
    pub bytes: Option<u64>,
    pub reasons: Vec<ProtectionReason>,
}
pub(crate) fn check_writer(store: &FileStore, writer: &WriterLock, quota: &Quota) -> Result<()> {
    crate::service::collection::check_mutation_location(store)?;
    ensure!(
        store.collection_namespace()? == quota.collection_namespace(),
        "maintenance quota namespace mismatch"
    );
    ensure!(
        writer.path == store.directory().canonicalize()?.join("writer.lock"),
        "foreign collection writer guard"
    );
    ensure!(
        store.managed_path()?.logical.canonicalize()?
            == quota
                .root()
                .join(store.collection_namespace()?)
                .join(store.collection_id()?)
                .canonicalize()?,
        "maintenance quota root mismatch"
    );
    Ok(())
}
pub(crate) fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
