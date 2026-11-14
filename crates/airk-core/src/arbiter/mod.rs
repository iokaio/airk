// SPDX-License-Identifier: Apache-2.0
//! Optional local context assistance. Pure contracts; no execution authority.
use crate::{
    evidence::RevisionEnvelope,
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
};
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONFIGURATION_SCHEMA: &str = "airk.arbiter.configuration/2";
pub const CATALOG_SCHEMA: &str = "airk.arbiter.catalog/1";
pub const ADVICE_SCHEMA: &str = "airk.arbiter.advice/1";
pub const REQUEST_SCHEMA: &str = "airk.arbiter.context.request/1";
pub const CONTEXT_SCHEMA: &str = "airk.arbiter.context/1";
pub const PACKET_SCHEMA: &str = "airk.arbiter.packet/1";
pub const RUNTIME: &str = "onnxruntime-cpu/1.30.0";
pub const STATUS_SCHEMA: &str = "airk.arbiter.status/1";
pub const COMPILER_SCHEMA: &str = "airk.arbiter.compiler/1";
pub const MAX_CONFIGURATION_BYTES: usize = 256 << 10;
pub const MAX_ENROLLMENTS: usize = 1024;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum Task {
    #[serde(rename = "airk.request_supported.v1")]
    RequestSupported,
    #[serde(rename = "airk.retrieval_route.v1")]
    RetrievalRoute,
    #[serde(rename = "airk.recipe_choice.v1")]
    RecipeChoice,
    #[serde(rename = "airk.evidence_relevance.v1")]
    EvidenceRelevance,
    #[serde(rename = "airk.evidence_role.v1")]
    EvidenceRole,
    #[serde(rename = "airk.context_next_read.v1")]
    ContextNextRead,
    #[serde(rename = "airk.context_gap.v1")]
    ContextGap,
    #[serde(rename = "airk.context_action.v1")]
    ContextAction,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum ModelChoice {
    #[serde(rename = "arbiter-small-intern-08b")]
    SmallIntern,
    #[serde(rename = "arbiter-small-strands-2b")]
    SmallStrands,
    #[serde(rename = "arbiter-large-intern-4b")]
    LargeIntern,
    #[serde(rename = "arbiter-large-kev-4b")]
    LargeKev,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    pub index_id: IndexId,
    pub tasks: BTreeSet<Task>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema: String,
    pub installation: Sha256Digest,
    pub catalog: Incarnation,
    pub revision: u64,
    pub enabled: bool,
    pub selected_profile: Option<Sha256Digest>,
    pub indexes: Vec<Enrollment>,
}
impl Configuration {
    pub fn disabled(installation: Sha256Digest, catalog: Incarnation) -> Self {
        Self {
            schema: CONFIGURATION_SCHEMA.into(),
            installation,
            catalog,
            revision: 1,
            enabled: false,
            selected_profile: None,
            indexes: vec![],
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == CONFIGURATION_SCHEMA && self.revision > 0,
            "invalid Arbiter configuration identity"
        );
        ensure!(
            self.indexes.len() <= MAX_ENROLLMENTS,
            "Arbiter enrollment limit"
        );
        let mut seen = BTreeSet::new();
        for enrollment in &self.indexes {
            ensure!(
                !enrollment.tasks.is_empty() && seen.insert(enrollment.index_id),
                "empty or duplicate Arbiter enrollment"
            );
        }
        ensure!(
            !self.enabled || (self.selected_profile.is_some() && !self.indexes.is_empty()),
            "Arbiter enablement requires a profile and named indexes"
        );
        Ok(())
    }
    pub fn eligible(&self, index: IndexId, task: Task) -> bool {
        self.enabled
            && self.selected_profile.is_some()
            && self
                .indexes
                .iter()
                .any(|entry| entry.index_id == index && entry.tasks.contains(&task))
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "Arbiter configuration byte limit"
        );
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8], installation: Sha256Digest, catalog: Incarnation) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "Arbiter configuration byte limit"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        ensure!(
            value.installation == installation && value.catalog == catalog,
            "Arbiter configuration belongs to another installation or catalog"
        );
        ensure!(
            value.canonical_bytes()? == bytes,
            "noncanonical Arbiter configuration"
        );
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Readout {
    DecisionSymbol,
    PointerHead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TensorType {
    Int64,
    Float32,
    Bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TensorContract {
    pub name: String,
    pub element: TensorType,
    /// Profile-pinned symbolic axes; batch is always one.
    pub axes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreAbi {
    pub readout: Readout,
    pub inputs: Vec<TensorContract>,
    pub output: TensorContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    Model,
    ExternalWeights,
    Tokenizer,
    Compiler,
    Calibration,
    Notice,
    Fixture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    pub role: AssetRole,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetPart {
    pub url: String,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetDownload {
    pub asset_name: String,
    pub parts: Vec<AssetPart>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub task_policies: BTreeMap<Task, Sha256Digest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualified_targets: Vec<QualifiedTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_fit: Option<CpuFit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub downloads: Vec<AssetDownload>,
    pub model: ModelChoice,
    pub profile_id: String,
    pub manifest: Sha256Digest,
    pub runtime: String,
    pub compiler: Sha256Digest,
    pub calibration: Sha256Digest,
    pub policy: Sha256Digest,
    pub qualification: Option<Sha256Digest>,
    pub tasks: BTreeSet<Task>,
    pub abi: ScoreAbi,
    pub max_tokens: usize,
    pub max_candidates: usize,
    pub memory_reservation_bytes: u64,
    pub peak_disk_bytes: u64,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualifiedTarget {
    pub target: String,
    pub execution_identity: String,
    pub receipt: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CpuFit {
    pub minimum_physical_cores: usize,
    pub intra_threads: usize,
    pub inter_threads: usize,
    pub sequential: bool,
    pub spinning: bool,
}
impl Default for CpuFit {
    fn default() -> Self {
        Self {
            minimum_physical_cores: 1,
            intra_threads: 1,
            inter_threads: 1,
            sequential: true,
            spinning: false,
        }
    }
}
impl CpuFit {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (self.minimum_physical_cores == 1 && self.intra_threads == 1
                || self.minimum_physical_cores == 8 && self.intra_threads == 8
                || self.minimum_physical_cores == 16 && self.intra_threads == 16
                || self.minimum_physical_cores == 24 && self.intra_threads == 24)
                && self.inter_threads == 1
                && self.sequential
                && !self.spinning,
            "unapproved Arbiter CPU execution tier"
        );
        Ok(())
    }
    pub fn runtime_identity(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "{RUNTIME}/physical{}/intra{}/inter1/sequential/spinning0",
            self.minimum_physical_cores, self.intra_threads
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompilerRecipe {
    InternSingleChoiceV1,
    StrandsSingleChoiceV1,
    KevSingleChoiceV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundToken {
    pub text: String,
    pub id: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompilerProfile {
    pub schema: String,
    pub recipe: CompilerRecipe,
    pub tokenizer: Sha256Digest,
    pub max_tokens: usize,
    pub max_options: usize,
    pub controls: Vec<BoundToken>,
    pub symbols: Vec<BoundToken>,
}
impl CompilerProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == COMPILER_SCHEMA
                && (1..=2048).contains(&self.max_tokens)
                && (1..=8).contains(&self.max_options),
            "invalid Arbiter compiler profile"
        );
        ensure!(
            self.controls.len() <= 16 && self.symbols.len() <= 8,
            "Arbiter compiler token catalog limit"
        );
        let mut names = BTreeSet::new();
        for token in self.controls.iter().chain(&self.symbols) {
            ensure!(
                !token.text.is_empty() && token.text.len() <= 128 && names.insert(&token.text),
                "invalid Arbiter compiler token binding"
            );
        }
        match self.recipe {
            CompilerRecipe::InternSingleChoiceV1 => ensure!(
                self.symbols.len() == self.max_options,
                "missing Intern decision symbols"
            ),
            _ => ensure!(
                self.symbols.is_empty(),
                "pointer compiler cannot use LM decision symbols"
            ),
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Disabled,
    IndexDisabled,
    Unqualified,
    UnsupportedSystem,
    UnsupportedTask,
    MissingAssets,
    Requested,
    Downloading,
    Verifying,
    Warming,
    Ready,
    Constrained,
    Draining,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileStatus {
    pub model: ModelChoice,
    pub profile: Option<Sha256Digest>,
    pub availability: Availability,
    pub reason: String,
    pub tasks: BTreeSet<Task>,
    pub download_bytes: Option<u64>,
    pub memory_reservation_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub schema: String,
    pub configuration: Configuration,
    pub selected_profile: Option<Sha256Digest>,
    pub active_profile: Option<Sha256Digest>,
    pub availability: Availability,
    pub reason: String,
    pub queued: usize,
    pub running: usize,
    pub profiles: Vec<ProfileStatus>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManagementRequest {
    Status,
    Configure {
        expected_revision: u64,
        enabled: bool,
        selected_profile: Option<Sha256Digest>,
        indexes: Vec<Enrollment>,
    },
    Provision {
        expected_revision: u64,
        profile: Sha256Digest,
    },
    Cancel {
        expected_revision: u64,
    },
    Remove {
        expected_revision: u64,
        profile: Sha256Digest,
    },
    Recover {
        expected_revision: u64,
    },
}
impl Profile {
    /// A compiled qualified release must bind every logical asset and each
    /// bounded GitHub release part. Downloaded metadata cannot supply this list.
    pub fn validate_release_downloads(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            self.downloads.len() == self.assets.len(),
            "incomplete Arbiter download closure"
        );
        let mut seen = BTreeSet::new();
        for download in &self.downloads {
            ensure!(
                seen.insert(&download.asset_name),
                "duplicate Arbiter asset download"
            );
            let asset = self
                .assets
                .iter()
                .find(|asset| asset.name == download.asset_name)
                .ok_or_else(|| anyhow::anyhow!("unknown Arbiter asset download"))?;
            ensure!(
                !download.parts.is_empty() && download.parts.len() <= 16,
                "Arbiter asset part limit"
            );
            let mut bytes = 0_u64;
            let mut urls = BTreeSet::new();
            for part in &download.parts {
                ensure!(
                    part.bytes > 0 && part.bytes <= 1_u64 << 30 && urls.insert(&part.url),
                    "invalid Arbiter asset part"
                );
                let path = part
                    .url
                    .strip_prefix("https://github.com/iokaio/airk/releases/download/")
                    .ok_or_else(|| anyhow::anyhow!("unapproved Arbiter asset source"))?;
                let pieces = path.split('/').collect::<Vec<_>>();
                ensure!(
                    pieces.len() == 2
                        && pieces.iter().all(|piece| {
                            !piece.is_empty()
                                && piece.len() <= 192
                                && *piece != "."
                                && *piece != ".."
                                && piece.bytes().all(|byte| {
                                    byte.is_ascii_alphanumeric() || b"._-".contains(&byte)
                                })
                        }),
                    "invalid Arbiter release asset URL"
                );
                bytes = bytes
                    .checked_add(part.bytes)
                    .ok_or_else(|| anyhow::anyhow!("Arbiter part byte overflow"))?;
            }
            ensure!(
                bytes == asset.bytes,
                "Arbiter logical asset length differs from parts"
            );
            ensure!(
                download.parts.len() != 1 || download.parts[0].sha256 == asset.sha256,
                "Arbiter single-part digest differs from logical asset"
            );
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.runtime == RUNTIME && !self.profile_id.is_empty() && self.profile_id.len() <= 128,
            "unsupported Arbiter profile runtime or identity"
        );
        if let Some(fit) = &self.cpu_fit {
            fit.validate()?;
        }
        ensure!(
            self.task_policies
                .keys()
                .all(|task| self.tasks.contains(task)),
            "policy for undeclared task"
        );
        let mut targets = BTreeSet::new();
        for target in &self.qualified_targets {
            ensure!(
                matches!(
                    target.target.as_str(),
                    "x86_64-pc-windows-msvc"
                        | "aarch64-apple-darwin"
                        | "x86_64-apple-darwin"
                        | "x86_64-unknown-linux-gnu"
                ) && targets.insert(&target.target),
                "unknown or duplicate qualification target"
            );
            let fit = self
                .cpu_fit
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("qualified target lacks explicit execution fit"))?;
            ensure!(
                target.execution_identity == fit.runtime_identity()?,
                "qualified target execution identity mismatch"
            );
        }

        ensure!(
            (1..=2048).contains(&self.max_tokens) && (1..=64).contains(&self.max_candidates),
            "unqualified Arbiter context profile"
        );
        ensure!(
            !self.tasks.is_empty() && self.memory_reservation_bytes > 0 && self.peak_disk_bytes > 0,
            "incomplete Arbiter task or resource profile"
        );
        let readout = match self.model {
            ModelChoice::SmallIntern | ModelChoice::LargeIntern => Readout::DecisionSymbol,
            ModelChoice::SmallStrands | ModelChoice::LargeKev => Readout::PointerHead,
        };
        ensure!(
            self.abi.readout == readout && self.abi.output.element == TensorType::Float32,
            "Arbiter trained readout mismatch"
        );
        ensure!(
            !self.abi.inputs.is_empty() && self.abi.inputs.len() <= 12,
            "invalid Arbiter input catalog"
        );
        let mut names = BTreeSet::new();
        for input in self
            .abi
            .inputs
            .iter()
            .chain(std::iter::once(&self.abi.output))
        {
            ensure!(
                !input.name.is_empty()
                    && input.name.len() <= 128
                    && names.insert(&input.name)
                    && !input.axes.is_empty()
                    && input.axes.len() <= 4,
                "invalid Arbiter tensor contract"
            );
        }
        ensure!(
            !self.assets.is_empty() && self.assets.len() <= 128,
            "invalid Arbiter assets"
        );
        let mut names = BTreeSet::new();
        for asset in &self.assets {
            ensure!(
                !asset.name.is_empty()
                    && asset.name.len() <= 128
                    && asset
                        .name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'-' | b'_' | b'.'))
                    && asset.name != "."
                    && asset.name != ".."
                    && names.insert(&asset.name)
                    && asset.bytes > 0
                    && asset.bytes <= 16u64 << 30,
                "unsafe or unbounded Arbiter asset"
            );
        }
        for role in [
            AssetRole::Model,
            AssetRole::Tokenizer,
            AssetRole::Compiler,
            AssetRole::Calibration,
            AssetRole::Notice,
            AssetRole::Fixture,
        ] {
            ensure!(
                self.assets.iter().any(|asset| asset.role == role),
                "incomplete Arbiter bundle"
            );
        }
        for role in [
            AssetRole::Model,
            AssetRole::Tokenizer,
            AssetRole::Compiler,
            AssetRole::Calibration,
        ] {
            ensure!(
                self.assets
                    .iter()
                    .filter(|asset| asset.role == role)
                    .count()
                    == 1,
                "duplicated Arbiter singleton asset role"
            );
        }
        ensure!(
            self.assets
                .iter()
                .any(|asset| asset.role == AssetRole::Compiler && asset.sha256 == self.compiler),
            "Arbiter compiler descriptor differs from exact raw asset bytes"
        );
        ensure!(
            self.assets
                .iter()
                .any(|asset| asset.role == AssetRole::Calibration
                    && asset.sha256 == self.calibration),
            "Arbiter calibration descriptor differs from exact raw asset bytes"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeBinding {
    pub installation: Sha256Digest,
    pub catalog: Incarnation,
    pub index_id: IndexId,
    pub authorization: Sha256Digest,
    pub configuration_revision: u64,
    pub publication: RevisionEnvelope,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub calls: usize,
    pub rounds: usize,
    pub fanout: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub max_bytes: usize,
    pub deadline_ms: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            calls: 8,
            rounds: 3,
            fanout: 8,
            input_tokens: 2048,
            candidates: 32,
            max_bytes: 262_144,
            deadline_ms: 30_000,
        }
    }
}
impl Budget {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=12).contains(&self.calls)
                && (1..=4).contains(&self.rounds)
                && (1..=12).contains(&self.fanout)
                && (1..=2048).contains(&self.input_tokens)
                && (1..=64).contains(&self.candidates)
                && (1024..=524_288).contains(&self.max_bytes)
                && (1..=30_000).contains(&self.deadline_ms),
            "invalid Arbiter context budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextRequest {
    pub schema: String,
    pub question: String,
    pub budget: Budget,
}
impl ContextRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == REQUEST_SCHEMA
                && !self.question.trim().is_empty()
                && self.question.len() <= 16_384,
            "invalid Arbiter context request"
        );
        self.budget.validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NativeRead {
    Search,
    Definitions,
    References,
    ReadEvidence,
    DocumentSections,
    Context,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadChoice {
    pub id: Sha256Digest,
    pub operation: NativeRead,
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    Sufficient,
    Definition,
    Constraint,
    Example,
    Test,
    Counterevidence,
    Ambiguous,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    Definition,
    Constraint,
    Example,
    Test,
    Counterevidence,
    Document,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Decision {
    Boolean(bool),
    Choice(usize),
    Relevance(i32),
    Role(EvidenceRole),
    NextRead(Sha256Digest),
    Gap(Gap),
    ContextAction(Sha256Digest),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Accepted { decision: Decision },
    Abstained { reason: String },
    Unsupported { reason: String },
    InvalidInput { reason: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub calls: usize,
    pub rounds: usize,
    pub fanout: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub bytes: usize,
    pub queue_us: Option<u64>,
    pub compile_us: Option<u64>,
    pub inference_us: Option<u64>,
    pub policy_us: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Advice {
    pub schema: String,
    pub task: Task,
    pub profile: Sha256Digest,
    pub runtime: String,
    pub compiler: Sha256Digest,
    pub calibration: Sha256Digest,
    pub policy: Sha256Digest,
    pub scope: ScopeBinding,
    pub input_digest: Sha256Digest,
    pub evidence: Vec<Sha256Digest>,
    pub work: Work,
    pub outcome: Outcome,
}
impl Advice {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == ADVICE_SCHEMA
                && self.runtime == RUNTIME
                && self.scope.configuration_revision > 0,
            "invalid Arbiter advice binding"
        );
        match &self.outcome {
            Outcome::Accepted { decision } => ensure!(
                matches!(
                    (self.task, decision),
                    (Task::RequestSupported, Decision::Boolean(_))
                        | (
                            Task::RetrievalRoute | Task::RecipeChoice,
                            Decision::Choice(_)
                        )
                        | (Task::EvidenceRelevance, Decision::Relevance(_))
                        | (Task::EvidenceRole, Decision::Role(_))
                        | (Task::ContextNextRead, Decision::NextRead(_))
                        | (Task::ContextGap, Decision::Gap(_))
                ),
                "Arbiter decision/task mismatch"
            ),
            Outcome::Abstained { reason }
            | Outcome::Unsupported { reason }
            | Outcome::InvalidInput { reason }
            | Outcome::Unavailable { reason } => {
                ensure!(
                    !reason.is_empty() && reason.len() <= 1024,
                    "invalid Arbiter outcome reason"
                );
            }
        }
        ensure!(self.evidence.len() <= 64, "Arbiter advice evidence limit");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub id: Sha256Digest,
    pub scope: ScopeBinding,
    pub operation: NativeRead,
    pub arguments: std::collections::BTreeMap<String, serde_json::Value>,
    pub role: EvidenceRole,
    pub canonical_result: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Covered,
    NoProgress,
    NoValidRead,
    Budget,
    Deadline,
    Stale,
    Refused,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundedParagraph {
    pub text: String,
    pub evidence: Vec<Sha256Digest>,
    pub interpretation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexApplication {
    pub scope: ScopeBinding,
    pub profile: Option<Sha256Digest>,
    pub availability: Availability,
    pub reason: String,
}
impl IndexApplication {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.reason.is_empty() && self.reason.len() <= 1024,
            "invalid application reason"
        );
        ensure!(
            self.scope.configuration_revision > 0
                || (self.availability == Availability::Failed && self.profile.is_none()),
            "unobserved configuration cannot grant optional execution"
        );
        ensure!(
            self.availability != Availability::Ready || self.profile.is_some(),
            "ready application lacks profile"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextResponse {
    pub schema: String,
    pub packet_schema: String,
    pub question: String,
    pub records: Vec<EvidenceRecord>,
    pub paragraphs: Vec<GroundedParagraph>,
    pub gaps: Vec<Gap>,
    pub stop: StopReason,
    pub work: Work,
    pub candidate_count: usize,
    pub omitted_count: usize,
    pub advice: Vec<Advice>,
    pub applications: Vec<IndexApplication>,
}
impl ContextResponse {
    pub fn validate(&self, budget: &Budget) -> Result<()> {
        budget.validate()?;
        ensure!(
            self.schema == CONTEXT_SCHEMA && self.packet_schema == PACKET_SCHEMA,
            "invalid Arbiter context schema"
        );
        ensure!(
            self.records.len().checked_add(self.omitted_count) == Some(self.candidate_count)
                && self.candidate_count <= budget.candidates,
            "invalid Arbiter omission accounting"
        );
        let ids: BTreeSet<_> = self.records.iter().map(|record| record.id).collect();
        ensure!(
            ids.len() == self.records.len(),
            "duplicate Arbiter evidence"
        );
        for paragraph in &self.paragraphs {
            ensure!(
                !paragraph.text.is_empty()
                    && !paragraph.evidence.is_empty()
                    && paragraph.evidence.iter().all(|id| ids.contains(id)),
                "ungrounded Arbiter paragraph"
            );
        }
        for application in &self.applications {
            application.validate()?;
            ensure!(
                application.scope.configuration_revision > 0 || self.advice.is_empty(),
                "unobserved configuration cannot carry advice"
            );
        }
        for advice in &self.advice {
            advice.validate()?;
        }
        ensure!(
            self.work.calls <= budget.calls
                && self.work.rounds <= budget.rounds
                && self.work.fanout <= budget.fanout
                && self.work.input_tokens <= budget.input_tokens
                && self.work.candidates <= budget.candidates
                && self.work.bytes <= budget.max_bytes,
            "Arbiter original work budget exceeded"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= budget.max_bytes,
            "Arbiter response byte budget"
        );
        Ok(())
    }
}

pub mod context_action;
pub mod literal;
pub mod schema;
#[cfg(test)]
mod tests;
