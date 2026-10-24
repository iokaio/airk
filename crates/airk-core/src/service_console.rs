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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectObservation {
    pub project: RegisteredProject,
    pub watch: Option<WatchObservation>,
    pub progress: Option<ProgressObservation>,
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
}
