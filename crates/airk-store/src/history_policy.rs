// SPDX-License-Identifier: Apache-2.0
//! Opt-in historical selection and bounded operator mutations, separate from publication identity.
use crate::retention::MaintenanceBudget;
use airk_core::identity::{RevisionToken, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const HISTORY_POLICY_VERSION: u32 = 1;
pub const HISTORY_ADMIN_VERSION: u32 = 1;
pub const MAX_HISTORY_ADMIN_BYTES: usize = 65536;
pub const MAX_OPERATOR_RECEIPTS: usize = 32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryWorkBudget {
    #[serde(flatten)]
    pub maintenance: MaintenanceBudget,
    pub verification_bytes: u64,
}
impl Default for HistoryWorkBudget {
    fn default() -> Self {
        Self {
            maintenance: MaintenanceBudget::default(),
            // Authentication streams whole surviving catalogs, not just their
            // inventories. Keep this I/O allowance separate from the metadata
            // reservation so a multi-GiB catalog can finish object GC.
            verification_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}
impl HistoryWorkBudget {
    pub fn validate(self) -> Result<()> {
        self.maintenance.validate()?;
        ensure!(
            (16384..=8 * 1024 * 1024 * 1024).contains(&self.verification_bytes),
            "history verification budget outside supported bounds"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicyV1 {
    pub version: u32,
    pub enabled: bool,
    pub recent: u8,
    pub hourly_hours: u8,
    pub max_packages: u16,
    pub max_bytes: u64,
    pub quota_basis_points: u16,
}
impl Default for HistoryPolicyV1 {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            recent: 3,
            hourly_hours: 24,
            max_packages: 12,
            max_bytes: 256 * 1024 * 1024,
            quota_basis_points: 1000,
        }
    }
}
impl HistoryPolicyV1 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == HISTORY_POLICY_VERSION
                && self.recent <= 3
                && self.hourly_hours <= 24
                && (1..=12).contains(&self.max_packages)
                && (1..=256 * 1024 * 1024).contains(&self.max_bytes)
                && (1..=1000).contains(&self.quota_basis_points),
            "invalid history policy bounds"
        );
        Ok(())
    }
    pub fn byte_limit(&self, collection_limit: u64) -> u64 {
        self.max_bytes.min(
            (u128::from(collection_limit) * u128::from(self.quota_basis_points) / 10000) as u64,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicyReceipt {
    pub policy: HistoryPolicyV1,
    pub sequence: u64,
    pub digest: Sha256Digest,
    pub explicit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    ConfigureHistory,
    CreateCheckpoint,
    RemoveCheckpoint,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorReceipt {
    pub version: u32,
    pub sequence: u64,
    pub head: RevisionToken,
    pub action: OperatorAction,
    pub operation_digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRepresentation {
    Full,
    Historical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProtection {
    Active,
    Predecessor,
    Reader,
    Checkpoint,
    Conversion,
    ClockUncertain,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    pub revision: RevisionToken,
    pub representation: HistoryRepresentation,
    pub observed_unix_seconds: u64,
    pub metadata_bytes: u64,
    pub protected: Vec<HistoryProtection>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistorySelectionRequest {
    pub as_of_unix_seconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryPlan {
    pub version: u32,
    pub head: RevisionToken,
    pub operator_sequence: u64,
    pub policy_digest: Sha256Digest,
    pub as_of_unix_seconds: u64,
    pub retained: Vec<HistoryEntry>,
    pub convert: Vec<RevisionToken>,
    pub remove: Vec<RevisionToken>,
    pub projected_history_bytes: Option<u64>,
    pub byte_limit: u64,
    pub over_target_protected: bool,
    pub complete: bool,
    pub digest: Sha256Digest,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryMaintenanceRun {
    pub head: RevisionToken,
    pub conversions: Vec<crate::history::HistoryConversion>,
    pub removals: Vec<crate::history::HistoryRemoval>,
    pub pending: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeRequest {
    pub revisions: Vec<RevisionToken>,
    #[serde(default)]
    pub checkpoints: Vec<crate::checkpoints::RemoveCheckpoint>,
    #[serde(default)]
    pub clear_lineage: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeApplyRequest {
    pub request: PurgeRequest,
    pub expected_plan_digest: Sha256Digest,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgeTarget {
    pub revision: RevisionToken,
    pub descriptor_digest: Sha256Digest,
    pub protected: Vec<HistoryProtection>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgePlan {
    pub version: u32,
    pub head: RevisionToken,
    pub operator_sequence: u64,
    pub policy_digest: Sha256Digest,
    pub request_digest: Sha256Digest,
    pub targets: Vec<PurgeTarget>,
    pub clear_lineage: bool,
    pub digest: Sha256Digest,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgeReceipt {
    pub mutation: OperatorReceipt,
    pub removals: Vec<crate::history::HistoryRemoval>,
    pub protected: Vec<RevisionToken>,
    pub pending: bool,
    pub error: Option<String>,
    /// The kernel must publish an explicit lineage-clearing revision separately.
    pub lineage_publication_required: bool,
}
