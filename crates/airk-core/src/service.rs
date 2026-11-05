// SPDX-License-Identifier: Apache-2.0
//! Service protocol contracts; endpoint authentication and clocks belong to adapters.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_GRANT_TTL_MS: u64 = 30_000;

/// Complete native records observed after an accepted catalog request became
/// uncertain. Missing observation is not evidence that no effects occurred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogOutcomeObservation {
    AuthorityFence {
        fence: Box<crate::catalog_fences::FenceObservation>,
    },
    IndexSetup {
        receipt: Box<crate::catalog_setup::IndexSetupReceipt>,
    },
}

/// An explicit selection within a persisted native-reviewed grant. Host/caller
/// identity and backing registration are derived by the native owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedConnectRequest {
    pub schema: u32,
    pub context_key: Sha256Digest,
    pub namespace: Incarnation,
    pub expected_catalog_revision: crate::index_catalog::CatalogRevision,
    pub grant_id: crate::index_catalog::GrantId,
    pub expected_grant_revision: crate::index_catalog::CatalogRevision,
    pub index_id: crate::index_catalog::IndexId,
    pub project_ids: Vec<crate::index_catalog::QueryProjectId>,
}
impl ScopedConnectRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && !self.project_ids.is_empty(),
            crate::scoped_reads::ScopeReadRefused
        );
        crate::index_catalog::unique_bounded(
            &self.project_ids,
            crate::index_catalog::MAX_SELECTION,
        )?;
        crate::index_catalog::encoded_size(self, 4096)?;
        Ok(())
    }
}

/// Frozen Project selection inside one native-reviewed grant. This is inert
/// connection configuration, not a credential and not a new IPC grant shape.
/// Every selected index still uses the protected scoped connection protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedConnectRequest {
    pub schema: u32,
    pub context_key: Sha256Digest,
    pub namespace: Incarnation,
    pub expected_catalog_revision: crate::index_catalog::CatalogRevision,
    pub grant_id: crate::index_catalog::GrantId,
    pub expected_grant_revision: crate::index_catalog::CatalogRevision,
    pub project_ids: Vec<crate::index_catalog::QueryProjectId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "combined_present"
    )]
    pub index_ids: Option<Vec<crate::index_catalog::IndexId>>,
}

fn combined_present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl CombinedConnectRequest {
    pub fn selection(&self) -> crate::combined_reads::QuerySelection {
        crate::combined_reads::QuerySelection {
            project_ids: self.project_ids.clone(),
            index_ids: self.index_ids.clone(),
            member_availability: crate::combined_reads::MemberAvailability::Report,
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, crate::scoped_reads::ScopeReadRefused);
        self.selection().validate()?;
        crate::index_catalog::encoded_size(self, 4096)?;
        Ok(())
    }
}

/// One authenticated native delivery observation for a prepared combined reply.
/// It is neither a bearer credential nor authority to replay a read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedDeliveryReceipt {
    pub schema: u32,
    pub service_instance: Incarnation,
    pub namespace: Incarnation,
    pub catalog_revision: crate::index_catalog::CatalogRevision,
    pub scope_identity: Sha256Digest,
    pub result_digest: Sha256Digest,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlMethod {
    /// Validate a prepared combined reply against the live native admission
    /// gate. No source, worker election, grant issuance or query replay occurs.
    ValidateCombinedDelivery {
        selection: CombinedConnectRequest,
        query_selection: crate::combined_reads::QuerySelection,
        scope_identity: Sha256Digest,
        result_digest: Sha256Digest,
    },
    /// Native administrative read. Runtime consumes its worker credential;
    /// this route never returns a reusable evidence connection.
    NativeRead {
        project: ProjectBinding,
        request: crate::native_reads::NativeRead,
        #[serde(default = "unknown_client")]
        usage_client: crate::usage::ClientFamily,
    },
    ConnectScoped {
        request: ScopedConnectRequest,
    },
    /// Administrative metadata only; no evidence admission or implicit activation.
    CatalogPage {
        request: crate::catalog_management::CatalogPageRequest,
    },
    CatalogDetail {
        entity: crate::catalog_management::CatalogEntity,
        expected_catalog_revision: Option<crate::index_catalog::CatalogRevision>,
    },
    CatalogContext {
        host: Sha256Digest,
        key: Sha256Digest,
        expected_catalog_revision: Option<crate::index_catalog::CatalogRevision>,
    },
    /// Native administrative identity; never accepts a claimed wire host/caller.
    CatalogHostContext {
        key: Sha256Digest,
        expected_catalog_revision: Option<crate::index_catalog::CatalogRevision>,
    },
    CatalogLegacyBinding {
        project: ProjectBinding,
        expected_catalog_revision: Option<crate::index_catalog::CatalogRevision>,
    },
    /// Prepare an inert, complete reviewed grant; never installs or issues it.
    CatalogGrantDraft {
        context_key: Sha256Digest,
        selection: crate::catalog_host::GrantDraftSelection,
    },
    CatalogSourceMutationDraft {
        selection: crate::catalog_source_draft::SourceMutationDraftSelection,
    },
    CatalogSourceChangeObservation {
        begin_operation: Incarnation,
    },
    /// Explicit native recovery discovery; None is not completion evidence.
    CatalogPendingSourceChange {
        index_id: crate::index_catalog::IndexId,
    },
    CatalogPolicyStatus {
        index_id: crate::index_catalog::IndexId,
    },
    CatalogPolicyResume {
        index_id: crate::index_catalog::IndexId,
        operation_id: Incarnation,
        caller: Incarnation,
        key: RequestKey,
    },
    /// Read exact accepted retry evidence without replaying the mutation.
    CatalogPolicyResumeReceipt {
        index_id: crate::index_catalog::IndexId,
        operation_id: Incarnation,
        caller: Incarnation,
        key: RequestKey,
    },
    CatalogIndexRetirementEvidence {
        index_id: crate::index_catalog::IndexId,
    },
    CatalogSourceChangeResolve {
        begin_operation: Incarnation,
    },
    CatalogIndexSetupDraft {
        selection: crate::catalog_setup::IndexSetupSelection,
    },
    CatalogIndexSetupApply {
        draft: crate::catalog_setup::IndexSetupDraft,
    },
    CatalogIndexSetupReceipt {
        caller: Incarnation,
        key: RequestKey,
    },
    CatalogPreview {
        request: crate::catalog_management::CatalogRequest,
    },
    CatalogApply {
        request: crate::catalog_management::CatalogRequest,
        preview_digest: Sha256Digest,
    },
    CatalogReceipt {
        caller: Incarnation,
        key: RequestKey,
    },
    Logs {
        limit: usize,
    },
    Health,
    Shutdown,
    Register {
        project: ProjectRegistration,
    },
    /// Reserve disjoint canonical planned storage before a trusted adapter creates it.
    ReserveProject {
        project: ProjectRegistration,
        /// Atomically request native storage/model/source setup with this watch preference.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        setup_watch: Option<bool>,
        /// Startup recovery cannot undo a project removal racing its metadata read.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        only_if_missing: bool,
    },
    Projects {
        after: Option<ProjectBinding>,
        revision: Option<u64>,
        limit: usize,
    },
    RequestEpoch,
    RequestReceipt {
        caller: Incarnation,
        key: RequestKey,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<ProjectBinding>,
    },
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
    ConfigureProject {
        project: ProjectBinding,
        caller: Incarnation,
        key: RequestKey,
        request: Box<crate::service_configuration::ConfigurationRequest>,
    },
    ConfigurationResult {
        id: Sha256Digest,
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
    Console {
        after: Option<ProjectBinding>,
        revision: Option<u64>,
        limit: usize,
    },
    InactiveConsole {
        after: Option<ProjectBinding>,
        revision: Option<u64>,
        limit: usize,
    },
    Workers {
        projects: Vec<ProjectBinding>,
    },
    Worker {
        project: ProjectBinding,
    },
    RestartProject {
        project: ProjectBinding,
        expected: Option<WorkerIdentity>,
    },
    EmbeddingStatus,
    StartEmbedding,
    StopEmbedding {
        worker: Incarnation,
        #[serde(default)]
        force: bool,
    },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_instance: Option<Incarnation>,
    pub method: ControlMethod,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_clock: Option<CatalogClock>,
    #[serde(default)]
    pub storage_formats: Option<crate::storage_format::StorageFormats>,
    #[serde(default)]
    pub build: Option<RuntimeBuild>,
    #[serde(default)]
    pub build_error: Option<String>,
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

/// Cached native catalog identity/revision only, not authorization or readiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogClock {
    pub namespace: Incarnation,
    pub revision: crate::index_catalog::CatalogRevision,
}

/// Bounded, path-free build receipt metadata; not an executable approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBuild {
    pub source_sha256: crate::identity::Sha256Digest,
    pub source_files: u64,
    pub profile: String,
    pub rustc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProcess {
    pub version: String,
    pub protocol: u32,
    pub process_id: u32,
    pub os: String,
    pub arch: String,
    pub executable_sha256: Option<crate::identity::Sha256Digest>,
    pub payload_sha256: Option<crate::identity::Sha256Digest>,
    pub build: Option<RuntimeBuild>,
    pub metadata_error: Option<String>,
}

/// Qualifies mutable policy/job observations accompanying an exact admitted
/// request receipt. The current job can differ from that receipt's accepted job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatusBasis {
    CurrentPolicy,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlResponse {
    CombinedDeliveryValidated {
        receipt: CombinedDeliveryReceipt,
    },
    SourceChangeBusy {
        code: String,
        busy: crate::catalog_source_change::SourceChangeBusy,
    },
    NativeRead {
        result: serde_json::Value,
    },
    CatalogOutcomeUnknown {
        code: String,
        caller: Incarnation,
        key: RequestKey,
        request_digest: Sha256Digest,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        observation: Option<CatalogOutcomeObservation>,
    },
    CatalogPage {
        page: crate::catalog_management::CatalogPage,
    },
    CatalogDetail {
        detail: crate::catalog_management::CatalogDetail,
    },
    CatalogHostContext {
        descriptor: crate::catalog_host::NativeHostContextDescriptor,
    },
    CatalogLegacyBinding {
        observation: crate::catalog_host::NativeLegacyBindingObservation,
    },
    CatalogGrantDraft {
        draft: crate::catalog_host::ReviewedGrantDraft,
    },
    CatalogSourceMutationDraft {
        draft: crate::catalog_source_draft::SourceMutationDraft,
    },
    CatalogSourceChangeObservation {
        observation: crate::catalog_source_change::SourceChangeObservation,
    },
    CatalogPendingSourceChange {
        index_id: crate::index_catalog::IndexId,
        observation: Option<crate::catalog_source_change::SourceChangeObservation>,
    },
    CatalogPolicyStatus {
        index_id: crate::index_catalog::IndexId,
        snapshot: Option<crate::catalog_policy::PolicySnapshot>,
        job: Option<JobRecord>,
    },
    CatalogPolicyResumed {
        index_id: crate::index_catalog::IndexId,
        accepted_job_id: Sha256Digest,
        status_basis: PolicyStatusBasis,
        snapshot: crate::catalog_policy::PolicySnapshot,
        job: JobRecord,
    },
    CatalogPolicyResumeReceipt {
        index_id: crate::index_catalog::IndexId,
        operation_id: Incarnation,
        caller: Incarnation,
        key: RequestKey,
        receipt: Option<crate::catalog_policy::PolicyResumeReceipt>,
        expired: bool,
    },
    CatalogIndexRetirementEvidence {
        index_id: crate::index_catalog::IndexId,
        evidence: Option<crate::catalog_fences::NativeIndexRetirementEvidence>,
    },
    CatalogSourceChangeResolved {
        observation: crate::catalog_source_change::SourceChangeObservation,
    },
    SourceChangeResolutionOutcomeUnknown {
        code: String,
        begin_operation: Incarnation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        observation: Option<crate::catalog_source_change::SourceChangeObservation>,
    },
    CatalogIndexSetupDraft {
        draft: crate::catalog_setup::IndexSetupDraft,
        request_digest: Sha256Digest,
    },
    CatalogIndexSetupApplied {
        receipt: crate::catalog_setup::IndexSetupReceipt,
    },
    CatalogIndexSetupReceipt {
        receipt: Option<crate::catalog_setup::IndexSetupReceipt>,
    },
    CatalogPreview {
        preview: crate::catalog_management::CatalogPreview,
        preview_digest: Sha256Digest,
    },
    CatalogApplied {
        receipt: crate::catalog_management::CatalogReceipt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fenced_receipt: Option<crate::catalog_fences::FencedReceipt>,
    },
    CatalogReceipt {
        receipt: Option<crate::catalog_management::CatalogReceipt>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fence: Option<crate::catalog_fences::FenceObservation>,
    },
    /// Same-service observation of exact saved absence, not a durable receipt
    /// or permission to resubmit the expired/stale original mutation.
    CatalogReceiptUnprepared {
        request: crate::catalog_management::CatalogRequest,
        request_digest: Sha256Digest,
        catalog_clock: CatalogClock,
    },
    Configuration {
        job: JobRecord,
        result: Option<serde_json::Value>,
    },
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
        #[serde(default)]
        pending_job: Option<JobRecord>,
        #[serde(default)]
        queued_jobs: Vec<JobRecord>,
        #[serde(default)]
        queue_truncated: Option<bool>,
        #[serde(default)]
        project_paused: Option<bool>,
        #[serde(default)]
        pending_paused: Option<bool>,
        #[serde(default)]
        observed_unix_ms: Option<u64>,
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
    RequestReceipt {
        job: Option<JobRecord>,
        expired: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        removed_project: Option<ProjectBinding>,
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
    Console {
        page: crate::service_console::ConsolePage,
    },
    Workers {
        observations: Vec<crate::service_console::WorkerObservation>,
        age_ms: u64,
    },
    Worker {
        project: ProjectBinding,
        identity: Option<WorkerIdentity>,
        restarting: bool,
        automatic_start_blocked: bool,
        age_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        waiting: Option<crate::service_resources::WaitObservation>,
    },
    ProjectRestarting {
        project: ProjectBinding,
        previous: Option<WorkerIdentity>,
    },
    EmbeddingStatus {
        available: bool,
        process_id: Option<u32>,
        worker: Option<Incarnation>,
        loads: u64,
        reuses: u64,
        sessions: usize,
        observation_age_ms: Option<u64>,
    },
    EmbeddingStarting,
    EmbeddingStopped {
        worker: Incarnation,
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
    ModelSetup,
    Configure,
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
            Self::ModelSetup => "model_setup",
            Self::Configure => "configure",
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

/// Frozen native scope identity. This metadata is never a bearer credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedReadRef {
    pub schema: u32,
    /// Native-reserved, worker-monotonic installation lease; never a bearer.
    pub lease_generation: u64,
    pub index_id: crate::index_catalog::IndexId,
    pub grant_id: crate::index_catalog::GrantId,
    pub grant_revision: crate::index_catalog::CatalogRevision,
    pub fingerprint: Sha256Digest,
}
impl ScopedReadRef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.lease_generation > 0,
            "unsupported scoped read contract"
        );
        Ok(())
    }
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
    /// Informational accounting label, never authority or an authentication claim.
    #[serde(default = "unknown_client")]
    pub client: crate::usage::ClientFamily,
    /// Required by an authority-contract-1 worker; omission never widens access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopedReadRef>,
}
fn unknown_client() -> crate::usage::ClientFamily {
    crate::usage::ClientFamily::Unknown
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
    #[serde(default)]
    pub pending_objects: bool,
    #[serde(default)]
    pub objects_checked: bool,
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
