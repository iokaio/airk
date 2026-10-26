// SPDX-License-Identifier: Apache-2.0
//! Durable scoped maintenance intents. Planning never grants a mutation capability.
use crate::{
    identity::{RevisionToken, Sha256Digest},
    service::{JobKind, JobOutcome, ProjectBinding},
    service_inspection::InspectionObservation,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairSelection {
    All,
    Selected(Vec<ProjectBinding>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairOperation {
    Auto,
    Verify,
    Recover,
    Reconcile,
    Vectors,
    Rebuild,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityImpact {
    ReadOnly,
    RecoveryMayBeUnavailable,
    PreservesCommittedEvidence,
    ReplacesIncarnation,
}
impl RepairOperation {
    pub fn availability(self) -> Option<AvailabilityImpact> {
        Some(match self {
            Self::Auto => return None,
            Self::Verify => AvailabilityImpact::ReadOnly,
            Self::Recover => AvailabilityImpact::RecoveryMayBeUnavailable,
            Self::Reconcile | Self::Vectors => AvailabilityImpact::PreservesCommittedEvidence,
            Self::Rebuild => AvailabilityImpact::ReplacesIncarnation,
        })
    }
    pub fn job_kind(self) -> Option<JobKind> {
        match self {
            Self::Auto => None,
            Self::Verify => Some(JobKind::Verify),
            Self::Recover => Some(JobKind::Recover),
            Self::Reconcile => Some(JobKind::Reconcile),
            Self::Vectors => Some(JobKind::Vectors),
            Self::Rebuild => Some(JobKind::Rebuild),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub selection: RepairSelection,
    pub operation: RepairOperation,
}
impl RepairRequest {
    pub fn canonical(&self) -> Result<Self> {
        let mut canonical = self.clone();
        if let RepairSelection::Selected(projects) = &mut canonical.selection {
            ensure!(
                !projects.is_empty() && projects.len() <= 256,
                "repair selection must contain 1..256 projects"
            );
            let sorted: BTreeSet<_> = projects.iter().copied().collect();
            ensure!(sorted.len() == projects.len(), "duplicate repair selection");
            *projects = sorted.into_iter().collect();
        }
        ensure!(
            serde_json::to_vec(&canonical)?.len() <= 20_000,
            "repair request exceeds byte bound"
        );
        Ok(canonical)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPhase {
    Planning,
    Ready,
    Running,
    Canceling,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemPhase {
    Unobserved,
    InspectQueued,
    Inspecting,
    Ready,
    ActionQueued,
    Running,
    Succeeded,
    Failed,
    Stale,
    Skipped,
    Canceled,
}
impl ItemPhase {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Stale | Self::Skipped | Self::Canceled
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanOutcome {
    Succeeded,
    PartialFailure,
    Canceled,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCounts {
    pub selected: u16,
    pub awaiting_inspection: u16,
    pub ready: u16,
    pub queued: u16,
    pub running: u16,
    pub succeeded: u16,
    pub failed: u16,
    pub stale: u16,
    pub skipped: u16,
    pub canceled: u16,
    pub cleanup_pending: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenancePlan {
    pub engine: Sha256Digest,
    pub id: Sha256Digest,
    pub operation: RepairOperation,
    pub all_registered: bool,
    pub phase: PlanPhase,
    pub paused: bool,
    pub counts: PlanCounts,
    pub preview_digest: Option<Sha256Digest>,
    pub outcome: Option<PlanOutcome>,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanItem {
    pub project: ProjectBinding,
    pub phase: ItemPhase,
    pub inspection_job: Option<Sha256Digest>,
    pub inspection: Option<InspectionObservation>,
    pub action: Option<RepairOperation>,
    pub availability: Option<AvailabilityImpact>,
    pub explicit_fallback_required: bool,
    pub action_job: Option<Sha256Digest>,
    pub outcome: Option<JobOutcome>,
    pub code: Option<String>,
    pub cleanup_pending: bool,
    pub result_revision: Option<RevisionToken>,
    pub progress: Option<crate::service_progress::ProgressObservation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPage {
    pub plan: MaintenancePlan,
    pub items: Vec<PlanItem>,
    pub after: Option<ProjectBinding>,
}

/// Exact approved work, fetched only by the fenced project owner. This is never
/// accepted from a caller as authority to bypass preview validation.
#[derive(Clone, Debug)]
pub struct RepairWork {
    pub operation: RepairOperation,
    pub expected: crate::service_inspection::InspectionSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    Unknown,
    Capacity,
    KeyExpired,
    KeyConflict,
    NotReady,
    Stale,
    ConfirmationRequired,
    InvalidTransition,
    SourceUnavailable,
    Canceled,
}
impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unknown => "unknown_maintenance_plan",
            Self::Capacity => "maintenance_plan_capacity",
            Self::KeyExpired => "maintenance_key_expired",
            Self::KeyConflict => "maintenance_key_conflict",
            Self::NotReady => "maintenance_plan_not_ready",
            Self::Stale => "maintenance_plan_stale",
            Self::ConfirmationRequired => "maintenance_confirmation_required",
            Self::InvalidTransition => "invalid_maintenance_transition",
            Self::SourceUnavailable => "plan_source_unavailable",
            Self::Canceled => "maintenance_plan_canceled",
        })
    }
}
impl std::error::Error for PlanError {}
