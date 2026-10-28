// SPDX-License-Identifier: Apache-2.0
//! Bounded cached metadata for trusted consoles; observations never open an index.
use crate::{
    service::{MaintenanceObservation, ProjectBinding, RegisteredProject},
    service_inspection::InspectionObservation,
    service_progress::ProgressObservation,
    service_watch::WatchObservation,
};
use serde::{Deserialize, Serialize};
pub const MAX_PAGE_BYTES: usize = 48 * 1024;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Watch,
    Progress,
    Inspection,
    Maintenance,
    MaintenanceRetry,
    Configuration,
    PendingJob,
    Recovery,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryObservation {
    pub reason: String,
    pub state: String,
    pub attempts: u32,
    pub limit: u32,
    pub next_check_unix_ms: u64,
    pub started_unix_ms: u64,
    pub job: Option<crate::identity::Sha256Digest>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexingPreference {
    pub job: crate::identity::Sha256Digest,
    pub vectors_enabled: Option<bool>,
    pub detail: Option<crate::extraction::IndexDetail>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingJobGate {
    pub job: crate::identity::Sha256Digest,
    pub code: String,
    pub plan: Option<crate::identity::Sha256Digest>,
    pub plan_phase: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preference: Option<IndexingPreference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovery: Vec<RecoveryObservation>,
    pub project: RegisteredProject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_job: Option<crate::service::JobRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_gate: Option<PendingJobGate>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending_paused: bool,
    pub watch: Option<WatchObservation>,
    pub progress: Option<ProgressObservation>,
    #[serde(default)]
    pub configuration: Option<crate::service::JobRecord>,
    pub inspection: Option<InspectionObservation>,
    pub maintenance: Option<MaintenanceObservation>,
    pub maintenance_retrying: Option<bool>,
    pub failed_observations: Vec<ObservationKind>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolePage {
    pub revision: u64,
    pub observed_unix_ms: u64,
    pub cached: bool,
    pub projects: Vec<ProjectObservation>,
    pub after: Option<ProjectBinding>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerObservation {
    pub project: ProjectBinding,
    pub identity: Option<crate::service::WorkerIdentity>,
    pub restarting: bool,
    pub automatic_start_blocked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<crate::service_resources::WaitObservation>,
}
