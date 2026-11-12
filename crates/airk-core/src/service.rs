// SPDX-License-Identifier: Apache-2.0
//! Service protocol contracts; endpoint authentication and clocks belong to adapters.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub use crate::service_records::{
    JobKind, JobOutcome, JobRecord, JobState, MAX_PROJECT_REGISTRATION_BYTES,
    MaintenanceObservation, ProjectBinding, ProjectRegistration, RegisteredProject, RequestKey,
    WorkerIdentity,
};

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
    IndexSplit {
        snapshot: Box<crate::catalog_split::SplitSnapshot>,
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
    PrepareUpgradeCheckpoint {
        operation_id: Incarnation,
        candidate_execution_sha256: Sha256Digest,
    },
    ObserveUpgradeCheckpoint {
        operation_id: Incarnation,
    },
    UpgradeRecoveryProbe {
        operation_id: Incarnation,
    },
    ReleaseUpgradeAdmission {
        operation_id: Incarnation,
    },
    /// Owner-authenticated local discovery; never an anonymous HTTP endpoint.
    HttpEndpoint,
    /// Enroll an existing reviewed selection. No token appears in the response.
    HttpEnroll {
        connection_id: Incarnation,
        selection: Box<crate::service_http::HttpSelection>,
        client: crate::usage::ClientFamily,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explorer_descriptor: Option<String>,
        /// An original managed attachment proven retired by native ownership.
        /// Checked again under the credential owner before replacement issuance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reattach_connection_id: Option<Incarnation>,
    },
    HttpRevoke {
        connection_id: Incarnation,
    },
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
    CatalogSplitDraft {
        selection: Box<crate::catalog_split_draft::SplitDraftSelection>,
    },
    CatalogSplitSourceState {
        index_id: crate::index_catalog::IndexId,
        expected_catalog_revision: crate::index_catalog::CatalogRevision,
    },
    CatalogSplitApply {
        draft: Box<crate::catalog_split_draft::SplitDraft>,
    },
    CatalogSplitStatus {
        operation_id: Incarnation,
    },
    CatalogSplitReceipt {
        caller: Incarnation,
        key: RequestKey,
    },
    CatalogSplitRecover {
        request: Box<crate::catalog_split::SplitRecoveryRequest>,
    },
    CatalogSplitRecoveryReceipt {
        operation_id: Incarnation,
        caller: Incarnation,
        key: RequestKey,
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
    /// Complete exact-key historical refresh proof; never starts or retries work.
    ReferenceRefreshReceipt {
        caller: Incarnation,
        key: RequestKey,
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
    /// M2 (`docs/split/contracts.md` §2): the unified tool catalog document,
    /// read without a project. Response budget [`MAX_TOOLS_CATALOG_RESPONSE_BYTES`].
    ToolsCatalog {
        detail: ToolsCatalogDetail,
        /// `--combined-session`: the catalog of that reviewed session, served
        /// after the session resolves. Absent for the unified catalog.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "combined_present"
        )]
        combined: Option<Box<CombinedConnectRequest>>,
    },
    /// M3: run one unified tool for a selected project and return its complete
    /// envelope. The service runs the pre-split CLI call path unchanged.
    /// Request budget [`MAX_TOOLS_CALL_REQUEST_BYTES`], response budget
    /// [`MAX_TOOLS_CALL_RESPONSE_BYTES`].
    ToolsCall {
        tool: String,
        /// The caller's JSON object; the client bounds its encoding to
        /// [`MAX_TOOL_ARGUMENT_BYTES`] as the pre-split CLI did.
        arguments: serde_json::Map<String, serde_json::Value>,
        selection: ToolSelection,
        /// Inserted as `response_mode` for tools that take one, unless the
        /// arguments already name it (the CLI's `AIRK_MCP_RESPONSE_MODE`).
        response_mode: ToolResponseMode,
        usage_client: crate::usage::ClientFamily,
    },
    /// M19 (U85): stop or restart this service through its lifecycle ledger.
    /// The service records the request, answers `LifecycleAccepted`, then
    /// drains and exits; the caller observes `process_id` exit and, for a
    /// restart, starts the service again. Keyed by `(caller, key)`; the key
    /// comes from `RequestEpoch`.
    Lifecycle {
        action: crate::service_lifecycle::LifecycleAction,
        caller: Incarnation,
        key: RequestKey,
    },
    /// U87 (`service status`): the saved explicit Stop and the pending
    /// lifecycle record, read from the lifecycle ledger.
    LifecycleStatus,
    /// M14 (`service record-error`): append a bounded client error to the raw
    /// job-error ring, as `airk service record-error` did. `input` is the
    /// error's JSON text (`{code, detail}`), at most
    /// [`MAX_CLIENT_ERROR_INPUT_BYTES`]; `operation` is a snake-case name.
    RecordClientError {
        operation: String,
        input: String,
    },
    /// M1 (split step 3c.1): the registered project a binding, or a root, data
    /// root and collection, name; read-only, as the CLI's `saved` lookup was.
    ResolveProject {
        target: ResolveTarget,
    },
    /// M5 (contracts §2.4): durably journal one keyed `Enqueue` or
    /// `ConfigureProject` request in the service's control folder, with no
    /// effect yet. `caller` and `key` must equal the inner request's.
    ClientRequestPrepare {
        caller: Incarnation,
        key: RequestKey,
        request: Box<ControlMethod>,
    },
    /// M6: the journaled requests.
    ClientRequestList {},
    /// M7: submit a journaled request by its nonce, with its exact saved key.
    ClientRequestSubmit {
        nonce: Incarnation,
    },
    /// M8: discard a journaled request; never cancels native work.
    ClientRequestForget {
        nonce: Incarnation,
    },
    /// M4a (split step 3c.4): the canonical registration for a root, data
    /// root and collection, as the CLI computed it before registering;
    /// read-only. Paths are absolute.
    PlanRegistration {
        root: String,
        data_root: String,
        collection: String,
        storage: RegistrationStorage,
    },
    /// M4b: create the storage the registry reserved for a registered
    /// project. A failed creation leaves the reservation for an explicit retry.
    PrepareStorage {
        project: ProjectBinding,
    },
}

/// Which registration rule M4a applies, as the pre-split CLI chose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStorage {
    /// The storage may be missing; sources and storage must be disjoint
    /// (`init`, `service prepare-registration`).
    Planned,
    /// The storage must exist and must not contain the sources
    /// (`service register`).
    Existing,
}

/// One journaled request (M6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingClientRequest {
    pub nonce: Incarnation,
    pub project: ProjectBinding,
    pub kind: JobKind,
}

/// What M1 resolves. Paths are absolute: the service's working folder is not
/// the caller's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolveTarget {
    Binding {
        project: ProjectBinding,
    },
    Registration {
        root: String,
        data_root: String,
        collection: String,
    },
    /// The ready index a reviewed catalog session names (split step 3c.4c),
    /// as the CLI resolved it before connecting: the index's backing project.
    Catalog {
        request: ScopedConnectRequest,
    },
}

/// The project an M3 call reads, as the pre-split CLI selected it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolSelection {
    /// No project: only the help tool, which the CLI answered in process
    /// without resolving a project; any other tool fails. Help is answered in
    /// process for `Unselected` and `Project` only.
    Unselected,
    /// `--service-project <binding>`: a registered project.
    Project { binding: ProjectBinding },
    /// `--catalog-session <request>`: an exact reviewed catalog grant.
    Catalog { request: ScopedConnectRequest },
    /// `--combined-session <request>` with its narrowing defaults, which the
    /// service adds to the arguments (a conflicting argument is refused).
    Combined {
        request: CombinedConnectRequest,
        /// `--project-ids`.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "combined_present"
        )]
        project_ids: Option<Vec<crate::index_catalog::QueryProjectId>>,
        /// `--index-ids`; requires `project_ids`.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "combined_present"
        )]
        index_ids: Option<Vec<crate::index_catalog::IndexId>>,
        /// `--member-availability`; requires `project_ids`.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "combined_present"
        )]
        member_availability: Option<crate::combined_reads::MemberAvailability>,
    },
}

/// The response mode an M3 call applies (`terse` unless `full`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResponseMode {
    Terse,
    Full,
}

/// Which M2 catalog document to return.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolsCatalogDetail {
    /// `{"tools":[…]}`; each entry is complete with `schemas`, otherwise only
    /// its `name` and `description`.
    List { schemas: bool },
    /// The one complete tool entry whose name, or name after an `airk_` prefix,
    /// equals `tool`; otherwise the `unknown_tool` failure.
    Help { tool: String },
}

/// The largest M2 response frame (contracts §2.8): the catalog with schemas.
pub const MAX_TOOLS_CATALOG_RESPONSE_BYTES: usize = 1024 * 1024;
/// The largest tool argument object, as the pre-split CLI bounded it.
pub const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1024;
/// The largest M3 request frame (contracts §2.8): arguments, selection and
/// envelope with room for re-encoding.
pub const MAX_TOOLS_CALL_REQUEST_BYTES: usize = 128 * 1024;
/// The largest M3 response frame (contracts §2.8): the largest canonical tool
/// response the service's HTTPS route can be configured to return.
pub const MAX_TOOLS_CALL_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// The largest client error input (M14), as `airk service record-error` bounded stdin.
pub const MAX_CLIENT_ERROR_INPUT_BYTES: usize = 16 * 1024;
/// The largest request frame the control endpoint reads before dispatch; each
/// method is then held to its own [`ControlMethod::request_frame_budget`].
pub const MAX_CONTROL_REQUEST_BYTES: usize = MAX_TOOLS_CALL_REQUEST_BYTES;

impl ControlMethod {
    /// The response frame budget for this method (contracts §2.8). Both the
    /// service writer and the client reader apply it.
    pub fn response_frame_budget(&self) -> usize {
        match self {
            Self::ToolsCatalog { .. } => MAX_TOOLS_CATALOG_RESPONSE_BYTES,
            Self::ToolsCall { .. } => MAX_TOOLS_CALL_RESPONSE_BYTES,
            _ => MAX_FRAME_BYTES,
        }
    }

    /// The request frame budget for this method (contracts §2.8); the service
    /// drops a larger request unanswered, as it drops any oversize frame.
    pub fn request_frame_budget(&self) -> usize {
        match self {
            // JSON escaping can grow the 16 KiB input; M3's budget covers it.
            Self::ToolsCall { .. } | Self::RecordClientError { .. } => MAX_TOOLS_CALL_REQUEST_BYTES,
            _ => MAX_FRAME_BYTES,
        }
    }
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
    UpgradeCheckpoint {
        observation: Box<crate::service_checkpoint::UpgradeCheckpointObservation>,
    },
    UpgradeRecoveryReady {
        operation_id: Incarnation,
        service_instance: Incarnation,
        execution_sha256: Sha256Digest,
        namespace: Option<Incarnation>,
    },
    UpgradeAdmissionReleased {
        operation_id: Incarnation,
    },
    HttpEndpoint {
        endpoint: Option<Box<crate::service_http::EndpointDescriptor>>,
    },
    HttpConnection {
        connection: Box<crate::service_http::HttpConnectionInfo>,
    },
    HttpRevoked {
        connection_id: Incarnation,
    },
    ReferenceRefreshReviewRequired {
        project: ProjectBinding,
        caller: Incarnation,
        key: RequestKey,
        request: Box<crate::service_configuration::ConfigurationRequest>,
        review: crate::catalog_refresh::RefreshReviewRequired,
    },
    ReferenceRefreshReceipt {
        job: Option<JobRecord>,
        configuration: Option<crate::service_configuration::ConfigurationWork>,
        result: Option<serde_json::Value>,
        expired: bool,
    },
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
        capabilities: crate::index_catalog::CatalogCapabilities,
    },
    CatalogDetail {
        detail: crate::catalog_management::CatalogDetail,
        capabilities: crate::index_catalog::CatalogCapabilities,
    },
    CatalogHostContext {
        descriptor: crate::catalog_host::NativeHostContextDescriptor,
        capabilities: crate::index_catalog::CatalogCapabilities,
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
    CatalogSplitDraft {
        draft: Box<crate::catalog_split_draft::SplitDraft>,
    },
    CatalogSplitSourceState {
        state: Box<crate::catalog_split::SplitSourceState>,
        capabilities: crate::index_catalog::CatalogCapabilities,
    },
    CatalogSplitAccepted {
        snapshot: Box<crate::catalog_split::SplitSnapshot>,
    },
    CatalogSplitStatus {
        operation_id: Incarnation,
        snapshot: Option<Box<crate::catalog_split::SplitSnapshot>>,
    },
    CatalogSplitReceipt {
        caller: Incarnation,
        key: RequestKey,
        snapshot: Option<Box<crate::catalog_split::SplitSnapshot>>,
        expired: bool,
    },
    CatalogSplitRecovered {
        receipt: Box<crate::catalog_split::SplitRecoveryReceipt>,
        snapshot: Box<crate::catalog_split::SplitSnapshot>,
    },
    CatalogSplitRecoveryReceipt {
        operation_id: Incarnation,
        caller: Incarnation,
        key: RequestKey,
        receipt: Option<Box<crate::catalog_split::SplitRecoveryReceipt>>,
        snapshot: Option<Box<crate::catalog_split::SplitSnapshot>>,
        expired: bool,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<crate::inference_provider::ProviderStatus>,
    },
    EmbeddingStarting,
    EmbeddingStopped {
        worker: Incarnation,
    },
    /// M2: the requested catalog document as the exact pretty-printed JSON
    /// text the pre-split `airk tools list --json` / `tools help --json`
    /// printed. Text, not a value, so a client keeps its key order: list
    /// documents are key-sorted values, a help document keeps the tool's field order.
    ToolsCatalog {
        document: String,
    },
    /// M3: the complete unified tool envelope (`content`, `isError`, …).
    ToolResult {
        result: serde_json::Value,
    },
    /// M3: the call failed before an envelope existed. `message` is the full
    /// error chain bounded to 4,096 characters, as the pre-split CLI's
    /// `operation_failed` error line carried it.
    ToolCallFailed {
        message: String,
    },
    /// M19: the recorded lifecycle request and the process that will exit.
    LifecycleAccepted {
        record: crate::service_lifecycle::LifecycleRecord,
        service_instance: Incarnation,
        process_id: u32,
    },
    /// M5: the request is journaled (`duplicate` when this exact request was
    /// already journaled under its nonce).
    ClientRequestPrepared {
        nonce: Incarnation,
        project: ProjectBinding,
        kind: JobKind,
        duplicate: bool,
    },
    /// M6: the journaled requests and the journal's capacity.
    ClientRequests {
        pending: Vec<PendingClientRequest>,
        capacity: usize,
    },
    /// M8: whether a request existed under the nonce.
    ClientRequestForgotten {
        nonce: Incarnation,
        existed: bool,
    },
    /// M1: the registered project.
    Resolved {
        registration: ProjectRegistration,
    },
    /// M4a: the canonical registration.
    RegistrationPlan {
        registration: ProjectRegistration,
    },
    /// M4b: the reserved storage exists; the registry's registration.
    StoragePrepared {
        registration: ProjectRegistration,
    },
    /// M4a/M4b, or M1's catalog target, refused. `message` is the full error
    /// chain bounded to 4,096 characters, as the pre-split CLI's
    /// `operation_failed` line carried it.
    RegistrationRefused {
        message: String,
    },
    /// M14: the client error was appended. An empty record, not a unit
    /// variant, so unknown fields are refused.
    ClientErrorRecorded {},
    /// U87: whether automatic starts are off, and any unfinished lifecycle record.
    LifecycleStatus {
        explicitly_stopped: bool,
        pending: Option<crate::service_lifecycle::LifecycleRecord>,
    },
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
