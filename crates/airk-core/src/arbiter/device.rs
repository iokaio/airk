// SPDX-License-Identifier: Apache-2.0
//! Device requirements and deterministic selection data; never native execution authority.
use super::*;
use crate::identity::framed_digest;
use anyhow::Context;

pub const DEVICE_EXECUTION_SCHEMA: &str = "airk.arbiter.device-execution/1";
pub const DEVICE_CATALOG_SCHEMA: &str = "airk.arbiter.device-catalog/2";
pub const DEVICE_CONFIGURATION_SCHEMA: &str = "airk.arbiter.configuration/5";
/// The configuration before Large was retired. It is read only to migrate it
/// (`DeviceConfiguration::migrate_v4`); nothing writes it.
pub const DEVICE_CONFIGURATION_V4_SCHEMA: &str = "airk.arbiter.configuration/4";
pub const DEVICE_STATUS_SCHEMA: &str = "airk.arbiter.device-status/1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DevicePolicy {
    #[default]
    Auto,
    Cpu,
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SelectedModel {
    /// The Focus reranker (Qwen3-Reranker-0.6B), the one model of the Focus stage.
    Focus,
    /// The Arbiter (Qwen3-1.7B, `ModelChoice::ContextQwenCompact`), the one
    /// model of the Context stage. Wire value `arbiter`.
    Arbiter,
}
impl SelectedModel {
    /// Supported model-family roles are descriptive metadata, not admission.
    pub fn supported_tasks(self) -> BTreeSet<Task> {
        match self {
            Self::Focus => BTreeSet::from([Task::FocusRank]),
            Self::Arbiter => BTreeSet::from([Task::ContextAction]),
        }
    }
    pub fn model(self) -> ModelChoice {
        match self {
            Self::Focus => ModelChoice::FocusQwenReranker06B,
            Self::Arbiter => ModelChoice::ContextQwenCompact,
        }
    }
    pub fn stage(self) -> Stage {
        match self {
            Self::Focus => Stage::Focus,
            Self::Arbiter => Stage::Context,
        }
    }
    /// The selectable model of a catalog model. The retired Large
    /// (`ContextQwenLarge`) and the historical four-choice models have none.
    pub fn from_model(model: ModelChoice) -> Result<Self> {
        match model {
            ModelChoice::FocusQwenReranker06B => Ok(Self::Focus),
            ModelChoice::ContextQwenCompact => Ok(Self::Arbiter),
            ModelChoice::ContextQwenLarge => {
                anyhow::bail!("the retired Large Context model cannot acquire a device slot")
            }
            _ => anyhow::bail!("historical model cannot acquire a device slot"),
        }
    }
}
/// A selection the service retired and cleared on migration. Status reports it
/// until the stage is configured again; it is never a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetiredModel {
    /// The Large Context model (`context_large` in configuration/4).
    ContextLarge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GpuBackend {
    Cuda,
    DirectMl,
    CoreMl,
}
impl GpuBackend {
    pub fn runtime(self) -> &'static str {
        match self {
            Self::Cuda => "onnxruntime-cuda/1.30.0",
            Self::DirectMl => "onnxruntime-directml/1.30.0",
            Self::CoreMl => "onnxruntime-coreml/1.30.0",
        }
    }
    fn priority(self) -> u8 {
        match self {
            Self::Cuda => 0,
            Self::DirectMl => 1,
            Self::CoreMl => 2,
        }
    }
    /// Whether this backend is a qualified route for an adapter of that vendor.
    /// CUDA serves NVIDIA. No route is qualified for any other adapter yet.
    pub fn serves(self, vendor: AdapterVendor) -> bool {
        match self {
            Self::Cuda => vendor == AdapterVendor::Nvidia,
            Self::DirectMl | Self::CoreMl => false,
        }
    }
}
/// The vendor of the adapter a GPU candidate was observed on. Advisory planner
/// input: never serialized, never an execution or qualification claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterVendor {
    Nvidia,
    Other,
}
/// Service-wide GPU use joined with what adapter enumeration found. Advisory
/// planner input: never serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuUse {
    /// Not turned on: never chosen, turned off or reset.
    NotOptedIn,
    /// Turned on, and an adapter enumeration ran and found no adapter.
    NoAdapter,
    /// Turned on. Each bound GPU candidate carries the vendor of its adapter.
    /// With no GPU candidate the model has no qualified route or variant.
    OptedIn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DevicePrecision {
    Float32,
    Float16,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GpuPrecisionPreference {
    #[default]
    Auto,
    Fp32,
    Fp16,
}
impl GpuPrecisionPreference {
    /// A preference restricts GPU variants only. A CPU tier has no precision
    /// and is always admitted.
    pub fn admits(self, precision: Option<DevicePrecision>) -> bool {
        match (self, precision) {
            (_, None) | (Self::Auto, Some(_)) => true,
            (Self::Fp32, Some(precision)) => precision == DevicePrecision::Float32,
            (Self::Fp16, Some(precision)) => precision == DevicePrecision::Float16,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlacementPolicy {
    StrictGpu,
    BoundedHybrid { policy: Sha256Digest },
}
// Serde internally tagged unit variants ignore extra fields. Decode through
// an empty struct variant so the advertised closed wire shape is enforced.
impl<'de> Deserialize<'de> for PlacementPolicy {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            StrictGpu {},
            BoundedHybrid { policy: Sha256Digest },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::StrictGpu {} => Self::StrictGpu,
            Wire::BoundedHybrid { policy } => Self::BoundedHybrid { policy },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceMemory {
    Dedicated {
        required_peak_bytes: u64,
        headroom_bytes: u64,
    },
    Unified {
        required_joint_peak_bytes: u64,
        headroom_bytes: u64,
    },
}
impl DeviceMemory {
    pub fn required_bytes(&self) -> Result<u64> {
        let (peak, headroom) = match self {
            Self::Dedicated {
                required_peak_bytes,
                headroom_bytes,
            } => (*required_peak_bytes, *headroom_bytes),
            Self::Unified {
                required_joint_peak_bytes,
                headroom_bytes,
            } => (*required_joint_peak_bytes, *headroom_bytes),
        };
        ensure!(
            peak > 0 && headroom > 0,
            "declared device capacity/headroom required"
        );
        peak.checked_add(headroom)
            .context("device capacity overflow")
    }
}
/// Unqualified probe request bounds only, never measured fit, residency or
/// release authority. Actual native owner must independently admit these limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GpuProbeReservation {
    pub schema: String,
    pub host_limit_bytes: u64,
    pub device_limit_bytes: u64,
    pub deadline_ms: u64,
}
impl GpuProbeReservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "airk.arbiter.gpu-probe-reservation/1"
                && self.host_limit_bytes > 0
                && self.device_limit_bytes > 0
                && self.deadline_ms > 0
                && self.deadline_ms <= 120000,
            "invalid unqualified GPU probe bounds"
        );
        Ok(())
    }
}
/// GPU-only additive Profile field. None preserves every legacy CPU serialization.
/// Hashes name compiled, authenticated native policies, not ambient strings or capabilities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceExecution {
    pub schema: String,
    pub backend: GpuBackend,
    pub runtime_build: Sha256Digest,
    pub compatibility_policy: Sha256Digest,
    pub provider_options: Sha256Digest,
    pub math_policy: Sha256Digest,
    pub precision: DevicePrecision,
    pub tf32: bool,
    pub placement: PlacementPolicy,
    pub graph: Sha256Digest,
    pub host_fit: CpuFit,
    pub memory: DeviceMemory,
}
impl DeviceExecution {
    pub fn validate(&self, profile: &Profile) -> Result<()> {
        ensure!(!self.tf32, "FP32 GPU policy requires TF32 off");
        ensure!(
            self.schema == DEVICE_EXECUTION_SCHEMA
                && profile.runtime == self.backend.runtime()
                && profile.cpu_fit.is_none()
                && profile.model.stage().is_some(),
            "device profile/version/runtime or legacy CPU tier mismatch"
        );
        self.host_fit.validate()?;
        self.memory.required_bytes()?;
        ensure!(
            profile
                .assets
                .iter()
                .filter(|asset| asset.role == AssetRole::Model)
                .all(|asset| asset.sha256 == self.graph),
            "device graph identity mismatch"
        );
        Ok(())
    }
    pub fn execution_identity(&self) -> Result<String> {
        self.host_fit.validate()?;
        self.memory.required_bytes()?;
        ensure!(
            self.schema == DEVICE_EXECUTION_SCHEMA && !self.tf32,
            "unknown device execution schema"
        );
        let bytes = serde_json::to_vec(self)?;
        let digest = framed_digest(b"airk.arbiter.device-execution/1", &[&bytes]);
        Ok(format!(
            "{}/device-execution/{digest}",
            self.backend.runtime()
        ))
    }
}
/// Exactly two logical model slots; variants do not add selected models.
/// Each reference resolves only through the authenticated native catalog owner.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariantReference {
    pub manifest: Sha256Digest,
    pub execution_identity: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceCatalog {
    pub schema: String,
    pub focus: Vec<VariantReference>,
    pub arbiter: Vec<VariantReference>,
}
impl DeviceCatalog {
    pub fn slot(&self, slot: SelectedModel) -> &[VariantReference] {
        match slot {
            SelectedModel::Focus => &self.focus,
            SelectedModel::Arbiter => &self.arbiter,
        }
    }
    /// Resolve exactly once per variant and return the SAME reference checked
    /// below. An interior-mutating Fn cannot swap in an unchecked second lookup.
    /// This remains descriptive data, never an ExecutionProfile or permit.
    pub fn materialize<'a>(
        &self,
        slot: SelectedModel,
        manifest: Sha256Digest,
        resolve: impl Fn(Sha256Digest) -> Option<&'a Profile>,
    ) -> Result<&'a Profile> {
        self.resolve_validated(&resolve, Some((slot, manifest)))?
            .context("variant is outside selected model slot")
    }
    pub fn validate<'a>(
        &self,
        resolve: impl Fn(Sha256Digest) -> Option<&'a Profile>,
    ) -> Result<()> {
        self.resolve_validated(&resolve, None)?;
        Ok(())
    }
    fn resolve_validated<'a>(
        &self,
        resolve: &impl Fn(Sha256Digest) -> Option<&'a Profile>,
        selected: Option<(SelectedModel, Sha256Digest)>,
    ) -> Result<Option<&'a Profile>> {
        let mut verified = None;
        ensure!(
            self.schema == DEVICE_CATALOG_SCHEMA,
            "unknown device catalog schema"
        );
        let mut manifests = BTreeSet::new();
        let mut count = 0usize;
        for slot in [SelectedModel::Focus, SelectedModel::Arbiter] {
            ensure!(!self.slot(slot).is_empty(), "missing selected model slot");
            for row in self.slot(slot) {
                count += 1;
                ensure!(
                    count <= 16 && manifests.insert(row.manifest),
                    "device catalog capacity/duplicate variant"
                );
                let profile = resolve(row.manifest).context("unresolved native catalog variant")?;
                profile.validate()?;
                ensure!(
                    profile.manifest == row.manifest
                        && profile.model == slot.model()
                        && row.execution_identity == profile.device_execution_identity()?,
                    "foreign model/execution variant"
                );
                if selected == Some((slot, row.manifest)) {
                    verified = Some(profile);
                }
            }
        }
        Ok(verified)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceStageConfiguration {
    pub enabled: bool,
    pub selected_model: Option<SelectedModel>,
    pub device_policy: DevicePolicy,
    pub gpu_precision: GpuPrecisionPreference,
    pub indexes: Vec<Enrollment>,
}
impl DeviceStageConfiguration {
    pub fn validate(&self, stage: Stage) -> Result<()> {
        ensure!(
            self.selected_model
                .is_none_or(|model| model.stage() == stage),
            "wrong stage/model slot"
        );
        let legacy = StageConfiguration {
            enabled: false,
            selected_profile: None,
            indexes: self.indexes.clone(),
        };
        legacy.validate(stage)?;
        ensure!(
            !self.enabled || (self.selected_model.is_some() && !self.indexes.is_empty()),
            "enablement requires model/enrollment"
        );
        Ok(())
    }
}
impl DeviceStageConfiguration {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            selected_model: None,
            device_policy: DevicePolicy::Auto,
            gpu_precision: GpuPrecisionPreference::Auto,
            indexes: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfiguration {
    pub schema: String,
    pub installation: Sha256Digest,
    pub catalog: Incarnation,
    pub revision: u64,
    pub device_catalog: DeviceCatalogBinding,
    pub focus: DeviceStageConfiguration,
    pub context: DeviceStageConfiguration,
    /// The Context selection a migration retired (a Large selection of
    /// configuration/4 or /3). The Context stage is then disabled with no
    /// model; the next Context configuration clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_retired: Option<RetiredModel>,
}
impl DeviceConfiguration {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == DEVICE_CONFIGURATION_SCHEMA && self.revision > 0,
            "unknown device configuration"
        );
        self.focus.validate(Stage::Focus)?;
        self.context.validate(Stage::Context)?;
        ensure!(
            self.context_retired.is_none()
                || (!self.context.enabled && self.context.selected_model.is_none()),
            "a retired Context selection leaves the stage disabled with no model"
        );
        let indexes: BTreeSet<_> = self
            .focus
            .indexes
            .iter()
            .chain(&self.context.indexes)
            .map(|row| row.index_id)
            .collect();
        ensure!(
            indexes.len() <= MAX_ENROLLMENTS,
            "device enrollment capacity"
        );
        Ok(())
    }
}
/// A saved configuration/3 names a selected profile that the compiled catalog
/// cannot resolve (for example a private evaluation-only profile written by a
/// legacy Configure), so it cannot be migrated to the device configuration.
/// Migration refuses and keeps the saved bytes. The service renders it as
/// `arbiter_configuration_migration_refused`; `airkctl arbiter status-legacy`
/// still reads that configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyProfileUnresolved;
impl std::fmt::Display for LegacyProfileUnresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "legacy selected profile unresolved: the saved configuration/3 cannot be migrated; read it with `airkctl arbiter status-legacy`",
        )
    }
}
impl std::error::Error for LegacyProfileUnresolved {}
impl DeviceConfiguration {
    /// Explicit migration preserves consent/indexes and uses the user-approved Auto policy while preserving exact model,
    /// consent and enrollment. Existing /4 Cpu settings are never migrated.
    pub fn migrate_legacy<'a>(
        legacy: &Configuration,
        device_catalog: DeviceCatalogBinding,
        resolve: impl Fn(Sha256Digest) -> Option<&'a Profile>,
    ) -> Result<Self> {
        legacy.validate()?;
        let mut context_retired = None;
        let mut stage = |old: &StageConfiguration,
                         expected: Stage|
         -> Result<DeviceStageConfiguration> {
            let mut enabled = old.enabled;
            let selected_model = match old.selected_profile {
                None => None,
                Some(manifest) => {
                    let profile = resolve(manifest).ok_or(LegacyProfileUnresolved)?;
                    profile.validate()?;
                    ensure!(
                        profile.manifest == manifest
                            && profile.device_execution.is_none()
                            && profile.runtime == RUNTIME,
                        "legacy profile cannot acquire GPU identity"
                    );
                    if expected == Stage::Context && profile.model == ModelChoice::ContextQwenLarge
                    {
                        // Large was retired: the stage is disabled with no model,
                        // its enrollments kept; Arbiter is never selected for it.
                        context_retired = Some(RetiredModel::ContextLarge);
                        enabled = false;
                        None
                    } else {
                        Some(SelectedModel::from_model(profile.model)?)
                    }
                }
            };
            let value = DeviceStageConfiguration {
                enabled,
                selected_model,
                device_policy: DevicePolicy::Auto,
                gpu_precision: GpuPrecisionPreference::Auto,
                indexes: old.indexes.clone(),
            };
            value.validate(expected)?;
            Ok(value)
        };
        let focus = stage(&legacy.focus, Stage::Focus)?;
        let context = stage(&legacy.context, Stage::Context)?;
        let value = Self {
            schema: DEVICE_CONFIGURATION_SCHEMA.into(),
            installation: legacy.installation,
            catalog: legacy.catalog,
            revision: legacy
                .revision
                .checked_add(1)
                .context("configuration revision exhausted")?,
            device_catalog,
            focus,
            context,
            context_retired,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "device configuration byte limit"
        );
        Ok(bytes)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReason {
    ForcedCpu,
    NoGpu,
    Unqualified,
    TargetUnsupported,
    RuntimeUnavailable,
    DriverUnsupported,
    DeviceUnsupported,
    GraphUnsupported,
    ProviderInitializationFailed,
    AllocationFailed,
    DeviceMemoryUnknown,
    DeviceMemoryInsufficient,
    HostMemoryUnknown,
    HostMemoryInsufficient,
    CpuFitInsufficient,
    CircuitOpen,
    AuthorityChanged,
    ObservationStale,
    DeadlineExpired,
    SettlementUnknown,
    NoQualifiedCpu,
    GpuPrecisionExcluded,
    // Plain comments keep the derived schema one closed string list.
    // Service-wide GPU use is not turned on: never chosen, turned off or reset.
    GpuNotOptedIn,
    // GPU use is on and no qualified route or variant serves this model here.
    NoQualifiedGpuRoute,
    // Reserved: a step down to a smaller model. Nothing emits it since the
    // Large model and its ladder were retired (change-log entry 56, decision 7).
    SmallerModelSelected,
}
/// available_bytes is an actual native observation after existing real device
/// usage; subtract ONLY broker reservations not yet represented in that usage.
/// Unified required_joint_peak is one shared pool, never host+device summed twice.
pub fn device_capacity_refusal(
    requirement: &DeviceMemory,
    available_bytes: Option<u64>,
    pending_unbacked_bytes: u64,
) -> Result<Option<SelectionReason>> {
    let required = requirement.required_bytes()?;
    let Some(available) = available_bytes else {
        return Ok(Some(SelectionReason::DeviceMemoryUnknown));
    };
    Ok(
        if available
            .checked_sub(pending_unbacked_bytes)
            .is_some_and(|free| free >= required)
        {
            None
        } else {
            Some(SelectionReason::DeviceMemoryInsufficient)
        },
    )
}
/// Advisory diagnostic facts ONLY: no Deserialize and no qualification/admission
/// authority. In particular Profile+target+None cannot construct a native permit.
/// Production Auto remains REFUSED until Inference holds real ExecutionProfile,
/// OptionalReservation/OptionalReuse and authentic device-runtime/capacity witness.
#[derive(Clone, Debug)]
pub struct AdvisoryCandidate {
    pub manifest: Sha256Digest,
    pub model: SelectedModel,
    pub execution_identity: String,
    pub backend: Option<GpuBackend>,
    pub precision: Option<DevicePrecision>,
    /// The adapter this candidate was observed on, or `cpu` for a CPU tier.
    /// None for a GPU variant that no adapter was bound to. Such a variant
    /// carries a rejection or is excluded by precision; it is never offered.
    pub adapter_key: Option<String>,
    /// Vendor of that adapter. None for a CPU tier and for an unbound variant.
    pub vendor: Option<AdapterVendor>,
    pub capability_epoch: u64,
    /// The profile's token budget.
    pub max_tokens: usize,
    /// The profile's CPU class: the physical cores its fit requires.
    pub cpu_cores: usize,
    pub rejection: Option<SelectionReason>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateRejection {
    pub manifest: Option<Sha256Digest>,
    pub adapter_key: Option<String>,
    pub reason: SelectionReason,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectedVariant {
    pub manifest: Sha256Digest,
    pub execution_identity: String,
    pub backend: Option<GpuBackend>,
    pub precision: Option<DevicePrecision>,
    pub adapter_key: String,
    pub capability_epoch: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceSelection {
    pub schema: String,
    pub model: SelectedModel,
    pub policy: DevicePolicy,
    pub gpu_precision: GpuPrecisionPreference,
    pub selected: Option<SelectedVariant>,
    pub reasons: Vec<CandidateRejection>,
    pub operation_failure: Option<SelectionReason>,
}
/// Advisory result of `attempt_order`. It grants nothing: the caller rechecks
/// source, consent, qualification, capacity, epoch and the original deadline
/// before each native dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptPlan {
    /// Indices into the candidates, in the order to try them.
    pub order: Vec<usize>,
    /// Global rows first, then one row per candidate refused before dispatch.
    pub reasons: Vec<CandidateRejection>,
    /// Deadline, authority or settlement failure. Nothing may be tried.
    pub operation_failure: Option<SelectionReason>,
    /// The row that closes the order when nothing in it is selected: present
    /// only for a model with no CPU candidate at all.
    pub no_fit: Option<CandidateRejection>,
}
/// Auto preference among GPU routes: FP32 before FP16, then backend priority.
/// The planner orders by it, and a caller that sizes one probe for several
/// variants takes the first by it, so the two never disagree.
pub fn gpu_route_rank(backend: GpuBackend, precision: DevicePrecision) -> (u8, u8) {
    (
        match precision {
            DevicePrecision::Float32 => 0,
            DevicePrecision::Float16 => 1,
        },
        backend.priority(),
    )
}
fn stops_operation(reason: SelectionReason) -> bool {
    matches!(
        reason,
        SelectionReason::AuthorityChanged
            | SelectionReason::SettlementUnknown
            | SelectionReason::DeadlineExpired
    )
}
/// The one owner of the Auto route order. Pure and advisory: a qualified GPU
/// route for the observed adapter first, then CPU tiers by larger token budget,
/// then by larger CPU class. Device policy `cpu`, or GPU use that is not turned
/// on, removes every GPU candidate. Candidates of another model are refused.
pub fn attempt_order(
    requested: SelectedModel,
    policy: DevicePolicy,
    gpu_precision: GpuPrecisionPreference,
    gpu_use: GpuUse,
    remaining_us: u64,
    candidates: &[AdvisoryCandidate],
) -> Result<AttemptPlan> {
    ensure!(candidates.len() <= 16, "device observation capacity");
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        let gpu = candidate.backend.is_some();
        ensure!(
            candidate.model == requested
                && gpu == candidate.precision.is_some()
                && !candidate.execution_identity.is_empty()
                && candidate.execution_identity.len() <= 4096
                && candidate
                    .adapter_key
                    .as_ref()
                    .is_none_or(|key| !key.is_empty() && key.len() <= 256)
                && (gpu || candidate.adapter_key.is_some())
                && (candidate.vendor.is_none() || (gpu && candidate.adapter_key.is_some()))
                && candidate.capability_epoch > 0
                && candidate.max_tokens > 0
                && candidate.cpu_cores > 0
                && seen.insert((candidate.manifest, candidate.adapter_key.clone())),
            "foreign/duplicate/unbounded device observation"
        );
    }
    let global = |reason| CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason,
    };
    let mut plan = AttemptPlan {
        order: Vec::new(),
        reasons: Vec::new(),
        operation_failure: None,
        no_fit: None,
    };
    // These are operation-wide failures, including when seen on an unselected
    // adapter or under forced Cpu. Never continue to any other candidate.
    let stopped = if remaining_us == 0 {
        Some(SelectionReason::DeadlineExpired)
    } else {
        candidates
            .iter()
            .filter_map(|candidate| candidate.rejection)
            .find(|reason| stops_operation(*reason))
    };
    if let Some(reason) = stopped {
        plan.operation_failure = Some(reason);
        plan.reasons.push(global(reason));
        return Ok(plan);
    }
    let gpu_count = candidates
        .iter()
        .filter(|candidate| candidate.backend.is_some())
        .count();
    let gpu_offered = policy == DevicePolicy::Auto && gpu_use != GpuUse::NotOptedIn;
    match (policy, gpu_use) {
        (DevicePolicy::Cpu, _) => plan.reasons.push(global(SelectionReason::ForcedCpu)),
        (DevicePolicy::Auto, GpuUse::NotOptedIn) => {
            plan.reasons.push(global(SelectionReason::GpuNotOptedIn));
        }
        (DevicePolicy::Auto, GpuUse::NoAdapter) => {
            // A variant that was never bound keeps its own row below.
            ensure!(
                candidates
                    .iter()
                    .all(|candidate| candidate.vendor.is_none()),
                "GPU candidate bound to an adapter that was not observed"
            );
            plan.reasons.push(global(SelectionReason::NoGpu));
        }
        (DevicePolicy::Auto, GpuUse::OptedIn) => {
            if gpu_count == 0 {
                plan.reasons
                    .push(global(SelectionReason::NoQualifiedGpuRoute));
            }
        }
    }
    let mut ordered: Vec<usize> = (0..candidates.len()).collect();
    ordered.sort_by_key(|index| {
        let row = &candidates[*index];
        // Every GPU candidate has a precision: the roster check above requires it.
        match row.backend.zip(row.precision) {
            Some((backend, precision)) => {
                let (precision_rank, backend_rank) = gpu_route_rank(backend, precision);
                (
                    0u8,
                    precision_rank,
                    backend_rank,
                    row.adapter_key.as_deref(),
                    std::cmp::Reverse(0),
                    std::cmp::Reverse(0),
                    row.manifest,
                )
            }
            None => (
                1,
                0,
                0,
                None,
                std::cmp::Reverse(row.max_tokens),
                std::cmp::Reverse(row.cpu_cores),
                row.manifest,
            ),
        }
    });
    for index in ordered {
        let candidate = &candidates[index];
        let refusal = match (candidate.backend, candidate.vendor) {
            (Some(_), _) if !gpu_offered => continue,
            (Some(backend), Some(vendor)) if !backend.serves(vendor) => {
                Some(SelectionReason::NoQualifiedGpuRoute)
            }
            (Some(_), _) if !gpu_precision.admits(candidate.precision) => {
                Some(SelectionReason::GpuPrecisionExcluded)
            }
            _ => candidate.rejection,
        };
        if let Some(reason) = refusal {
            plan.reasons.push(CandidateRejection {
                manifest: Some(candidate.manifest),
                adapter_key: candidate.adapter_key.clone(),
                reason,
            });
            continue;
        }
        ensure!(
            candidate.backend.is_none() || candidate.vendor.is_some(),
            "unbound GPU variant cannot be offered"
        );
        plan.order.push(index);
    }
    // A CPU candidate that was refused has its own row. Only a model with no
    // CPU candidate at all closes with this one.
    if gpu_count == candidates.len() {
        plan.no_fit = Some(global(SelectionReason::NoQualifiedCpu));
    }
    Ok(plan)
}
/// Advisory walk of `attempt_order` at one observation epoch: the first
/// candidate in the order is selected. Caller must recheck source/consent,
/// qualification, actual capacity, epoch and original deadline before native dispatch.
pub fn select(
    model: SelectedModel,
    policy: DevicePolicy,
    gpu_precision: GpuPrecisionPreference,
    gpu_use: GpuUse,
    epoch: u64,
    remaining_us: u64,
    candidates: &[AdvisoryCandidate],
) -> Result<DeviceSelection> {
    ensure!(epoch > 0, "device observation epoch");
    let observed: Vec<AdvisoryCandidate> = candidates
        .iter()
        .map(|candidate| {
            let mut candidate = candidate.clone();
            if candidate.capability_epoch != epoch
                && !candidate.rejection.is_some_and(stops_operation)
            {
                candidate.rejection = Some(SelectionReason::ObservationStale);
            }
            candidate
        })
        .collect();
    let plan = attempt_order(
        model,
        policy,
        gpu_precision,
        gpu_use,
        remaining_us,
        &observed,
    )?;
    let selected = match plan.order.first() {
        Some(index) => {
            let candidate = &candidates[*index];
            Some(SelectedVariant {
                manifest: candidate.manifest,
                execution_identity: candidate.execution_identity.clone(),
                backend: candidate.backend,
                precision: candidate.precision,
                adapter_key: candidate
                    .adapter_key
                    .clone()
                    .context("offered candidate has no adapter key")?,
                capability_epoch: epoch,
            })
        }
        None => None,
    };
    let mut reasons = plan.reasons;
    if selected.is_none() && plan.operation_failure.is_none() {
        reasons.extend(plan.no_fit);
    }
    Ok(DeviceSelection {
        schema: DEVICE_STATUS_SCHEMA.into(),
        model,
        policy,
        gpu_precision,
        selected,
        reasons,
        operation_failure: plan.operation_failure,
    })
}
/// No automatic replay after uncertain work. The one original deadline is input,
/// never created/refreshed here. Real native settlement proof remains owner-held.
pub fn fallback_permitted(
    work_started: bool,
    settlement_observed: bool,
    authority_current: bool,
    remaining_us: u64,
) -> std::result::Result<(), SelectionReason> {
    if !authority_current {
        return Err(SelectionReason::AuthorityChanged);
    }
    if work_started && !settlement_observed {
        return Err(SelectionReason::SettlementUnknown);
    }
    if remaining_us == 0 {
        return Err(SelectionReason::DeadlineExpired);
    }
    Ok(())
}

#[cfg(test)]
mod planner_tests;
#[cfg(test)]
mod tests;

mod read_status;
pub use read_status::*;
mod state;
pub use state::*;
mod disclosure;
pub use disclosure::*;
mod migration;
pub use migration::{Configuration4Transition, configuration4_binding};
pub mod capacity;

mod receipt_runtime;
pub use receipt_runtime::valid_advice_runtime;
#[cfg(test)]
mod receipt_tests;
