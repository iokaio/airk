// SPDX-License-Identifier: Apache-2.0
//! Local usage contracts. No queries, paths, source text, prompts or credentials.
//! Collection recording/storage and adapter hooks are planned in docs/usage-reporting.md.
use serde::{Deserialize, Serialize};

pub const USAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    Disabled,
    AggregatesOnly,
    DiagnosticEvents,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsagePolicy {
    pub mode: RecordingMode,
    pub aggregate_retention_days: u16,
    pub diagnostic_retention_hours: u16,
    pub max_collection_bytes: u64,
    pub max_data_root_bytes: u64,
}

impl Default for UsagePolicy {
    fn default() -> Self {
        Self {
            mode: RecordingMode::AggregatesOnly,
            aggregate_retention_days: 30,
            diagnostic_retention_hours: 24,
            max_collection_bytes: 1_048_576,
            max_data_root_bytes: 33_554_432,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageScope {
    pub collection: String,
    /// Digest of the canonical root binding; never the original filesystem path.
    pub root_binding: [u8; 32],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum UsageOperation {
    Search,
    ReadFile,
    ListFiles,
    Status,
    FileInfo,
    Outline,
    Symbols,
    Imports,
    DocumentFacts,
    References,
    Context,
    Capabilities,
    Index,
    Reindex,
    CollectionMaintenance,
    UsageReport,
}

impl UsageOperation {
    pub fn returns_source_evidence(self) -> bool {
        matches!(
            self,
            Self::Search
                | Self::ReadFile
                | Self::Outline
                | Self::Symbols
                | Self::Imports
                | Self::DocumentFacts
                | Self::References
                | Self::Context
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClientFamily {
    Unknown,
    Cli,
    Vscode,
    ClaudeCode,
    Codex,
    Cursor,
    Gemini,
    Cline,
    Roo,
    Junie,
    CopilotCli,
    OpenCode,
    Kilo,
    AmazonQ,
    Windsurf,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Code,
    StructuredText,
    Markdown,
    Pdf,
    Docx,
    Pptx,
    Svg,
    PlainText,
    Mixed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageOutcome {
    Success,
    Empty,
    Failed,
    NotReady,
    Cancelled,
    CapacityLimited,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheObservation {
    Warm,
    Cold,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseRepresentation {
    LogicalResultJson,
    McpToolResultJson,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    FullReturnedSources,
    ObservedAlternative,
    UserSupplied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextBaseline {
    pub kind: BaselineKind,
    pub generation: u64,
    pub representation: ResponseRepresentation,
    /// Normalized, serialized textual context, including comparable metadata.
    /// Never compressed PDF/OOXML/SVG image size or the entire workspace size.
    pub utf8_bytes: u64,
    pub unique_sources: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexWork {
    pub indexed: u64,
    pub reused: u64,
    pub removed: u64,
    pub skipped: u64,
    pub source_bytes: u64,
    pub index_disk_bytes: Option<u64>,
}

/// A transient completed-request observation. Detailed persistence is opt-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageObservation {
    pub schema_version: u32,
    pub event_id: [u8; 16],
    pub scope: UsageScope,
    pub unix_ms: u64,
    pub generation: Option<u64>,
    pub operation: UsageOperation,
    /// An informational, self-reported family; never used for authorization.
    pub client: ClientFamily,
    pub content_kind: ContentKind,
    pub outcome: UsageOutcome,
    /// Monotonic duration through serialization, excluding provider/model work.
    pub elapsed_us: u64,
    pub cache: CacheObservation,
    pub returned_items: u32,
    pub truncated: bool,
    pub response_representation: ResponseRepresentation,
    pub response_utf8_bytes: Option<u64>,
    pub baseline: Option<ContextBaseline>,
    pub index_work: Option<IndexWork>,
}

/// Future local recorder: the caller must handle failure without failing retrieval.
pub trait UsageRecorder: Send + Sync {
    fn record(&self, observation: &UsageObservation) -> anyhow::Result<()>;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Usefulness {
    Useful,
    PartlyUseful,
    NotUseful,
}

/// Optional explicit feedback; absence is unknown rather than an inferred success.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageFeedback {
    pub scope: UsageScope,
    pub event_id: [u8; 16],
    pub usefulness: Usefulness,
}
