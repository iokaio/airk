// SPDX-License-Identifier: Apache-2.0
//! Service protocol contracts; endpoint authentication and clocks belong to adapters.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_GRANT_TTL_MS: u64 = 30_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlMethod {
    Logs {
        limit: usize,
    },
    Health,
    Shutdown,
    Register {
        project: ProjectRegistration,
    },
    Projects {
        after: Option<ProjectBinding>,
        revision: Option<u64>,
        limit: usize,
    },
    RequestEpoch,
    PlanCreate {
        caller: Incarnation,
        key: RequestKey,
        request: crate::service_maintenance::RepairRequest,
    },
    Plan {
        id: Sha256Digest,
    },
    PlanItems {
        id: Sha256Digest,
        after: Option<ProjectBinding>,
        limit: usize,
    },
    PlanExecute {
        id: Sha256Digest,
        preview: Sha256Digest,
        allow_fallback: bool,
    },
    PlanPause {
        id: Sha256Digest,
        paused: bool,
    },
    PlanCancel {
        id: Sha256Digest,
    },
    Events {
        project: Option<ProjectBinding>,
        cursor: Option<crate::service_events::EventCursor>,
        limit: usize,
    },
    Maintenance {
        project: ProjectBinding,
    },
    Watch {
        project: ProjectBinding,
    },
    Progress {
        project: ProjectBinding,
    },
    Inspection {
        project: ProjectBinding,
    },
    ConfigureWatch {
        project: ProjectBinding,
        enabled: bool,
    },
    Enqueue {
        project: ProjectBinding,
        caller: Incarnation,
        key: RequestKey,
        kind: JobKind,
    },
    Job {
        id: Sha256Digest,
    },
    Retry {
        id: Sha256Digest,
        caller: Incarnation,
        key: RequestKey,
    },
    Pause {
        project: ProjectBinding,
        paused: bool,
    },
    Cancel {
        id: Sha256Digest,
    },
    CancelPending {
        id: Sha256Digest,
    },
    Connect {
        project: ProjectBinding,
    },
    EmbeddingStatus,
    SourceStatus,
    Resources,
    ConfigureResources {
        configuration: crate::service_resources::ResourceConfiguration,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub protocol: u32,
    pub method: ControlMethod,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceHealth {
    pub logs: crate::service_logs::LogTelemetry,
    pub version: String,
    pub protocol: u32,
    pub instance: Incarnation,
    pub process_id: u32,
    pub executable_sha256: String,
    #[serde(default)]
    pub runtime_sha256: Option<String>,
    pub user: String,
    pub uptime_ms: u64,
    pub policy: BrokerStatus,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlResponse {
    Logs {
        page: crate::service_logs::LogPage,
    },
    Health {
        observation: Box<ServiceHealth>,
    },
    Resources {
        observation: Box<crate::service_resources::ResourceSnapshot>,
        age_ms: u64,
        configuration_error: bool,
    },
    Maintenance {
        project: ProjectBinding,
        observation: Option<MaintenanceObservation>,
        retrying: bool,
    },
    Watch {
        project: ProjectBinding,
        observation: crate::service_watch::WatchObservation,
    },
    Progress {
        project: ProjectBinding,
        observation: Option<Box<crate::service_progress::ProgressObservation>>,
    },
    Inspection {
        project: ProjectBinding,
        observation: Option<Box<crate::service_inspection::InspectionObservation>>,
        cached: bool,
    },
    SourceStatus {
        available: bool,
        process_id: Option<u32>,
        watched_directories: Option<u16>,
        projects: Option<u16>,
        pending_events: Option<u16>,
        observation_age_ms: Option<u64>,
        blocked: bool,
        predecessor_draining: bool,
    },
    Stopping,
    Incompatible {
        protocol: u32,
    },
    Registered {
        project: RegisteredProject,
    },
    Projects {
        page: ProjectPage,
    },
    RequestEpoch {
        namespace: Incarnation,
        epoch: u64,
    },
    Events {
        page: crate::service_events::EventPage,
    },
    Plan {
        plan: crate::service_maintenance::MaintenancePlan,
    },
    PlanItems {
        page: crate::service_maintenance::PlanPage,
    },
    Job {
        job: JobRecord,
    },
    Failure {
        code: String,
    },
    Connection {
        role: String,
        grant: ConnectionGrant,
    },
    EmbeddingStatus {
        available: bool,
        process_id: Option<u32>,
        loads: u64,
        reuses: u64,
        sessions: usize,
        observation_age_ms: Option<u64>,
    },
}

/// Digest of a canonical root/storage/collection registration, never a client path.
pub type ProjectBinding = Sha256Digest;

pub const MAX_PROJECT_REGISTRATION_BYTES: usize = 20_480;

/// Canonical paths must be established by the native host; these bytes are not ownership proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub binding: ProjectBinding,
    pub root: String,
    pub data_root: String,
    pub collection: String,
}

impl ProjectRegistration {
    pub fn new(root: String, data_root: String, collection: String) -> Result<Self> {
        let binding = crate::identity::framed_digest(
            b"airk-service-project/1",
            &[root.as_bytes(), data_root.as_bytes(), collection.as_bytes()],
        );
        let registration = Self {
            binding,
            root,
            data_root,
            collection,
        };
        registration.validate()?;
        Ok(registration)
    }
    pub fn validate(&self) -> Result<()> {
        for path in [&self.root, &self.data_root] {
            ensure!(
                !path.is_empty() && path.len() <= 4096 && !path.chars().any(char::is_control),
                "invalid service registration path"
            );
        }
        ensure!(
            !self.collection.is_empty()
                && self.collection.len() <= 128
                && !matches!(self.collection.as_str(), "." | "..")
                && self
                    .collection
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "invalid service collection name"
        );
        ensure!(
            self.binding
                == crate::identity::framed_digest(
                    b"airk-service-project/1",
                    &[
                        self.root.as_bytes(),
                        self.data_root.as_bytes(),
                        self.collection.as_bytes()
                    ]
                ),
            "service project binding mismatch"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_PROJECT_REGISTRATION_BYTES,
            "service registration exceeds budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredProject {
    pub registration: ProjectRegistration,
    pub fencing_generation: u64,
    pub paused: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPage {
    pub revision: u64,
    pub projects: Vec<RegisteredProject>,
    pub after: Option<ProjectBinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Inspect,
    PlanInspect,
    Repair,
    Index,
    Reconcile,
    Vectors,
    Verify,
    Rebuild,
    Recover,
    Maintain,
}

impl JobKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::PlanInspect => "plan_inspect",
            Self::Repair => "repair",
            Self::Index => "index",
            Self::Reconcile => "reconcile",
            Self::Vectors => "vectors",
            Self::Verify => "verify",
            Self::Rebuild => "rebuild",
            Self::Recover => "recover",
            Self::Maintain => "maintain",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestKey {
    pub namespace: Incarnation,
    pub epoch: u64,
    pub nonce: Incarnation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobOutcome {
    Succeeded {
        revision: Option<crate::identity::RevisionToken>,
    },
    Failed {
        code: String,
    },
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: Sha256Digest,
    pub project: ProjectBinding,
    pub kind: JobKind,
    pub state: JobState,
    pub fencing_generation: u64,
    pub attempts: u32,
    pub outcome: Option<JobOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobError {
    KeyExpired,
    KeyConflict,
    Capacity,
    UnknownJob,
    StaleWorker,
    NotRetryable,
}
impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for JobError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerIdentity {
    pub service: Incarnation,
    pub worker: Incarnation,
    pub project: ProjectBinding,
    pub fencing_generation: u64,
}

/// Bearer credential: serializable only for protected IPC, intentionally redacted in Debug.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionGrant {
    pub protocol: u32,
    pub identity: WorkerIdentity,
    pub caller: Incarnation,
    pub token: Sha256Digest,
    pub ttl_ms: u64,
}

impl std::fmt::Debug for ConnectionGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionGrant")
            .field("identity", &self.identity)
            .field("caller", &self.caller)
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryAllowance {
    pub concurrent_queries: usize,
    pub response_bytes: usize,
}

/// Content-free in-process policy observation, not proof of OS process health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerStatus {
    pub service: Incarnation,
    pub accepting: bool,
    pub projects: usize,
    pub draining_projects: usize,
    pub pending_connections: usize,
    pub allocated: QueryAllowance,
    pub active_queries: usize,
    pub reserved_response_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionError {
    InvalidRequest,
    UnknownWorker,
    StaleIdentity,
    InvalidGrant,
    Expired,
    Busy,
    Draining,
    OwnershipLost,
    UnknownRequest,
}

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AdmissionError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceObservation {
    pub pending_incarnations: Option<usize>,
    pub removed_files: u32,
    pub failed: bool,
    pub observed_unix_ms: u64,
}

impl MaintenanceObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pending_incarnations.is_none_or(|count| count <= 8) && self.removed_files <= 4096,
            "maintenance observation exceeds contract bounds"
        );
        ensure!(
            self.failed || self.pending_incarnations.is_some(),
            "successful maintenance observation lacks pending count"
        );
        Ok(())
    }
}
