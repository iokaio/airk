// SPDX-License-Identifier: Apache-2.0
//! MCP owns schemas and transport only; every operation goes through the kernel.
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
use airk_kernel::{FactQuery, Kernel};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

pub mod answer_jobs;
mod execution;
mod search_repair;
mod lifecycle;
pub mod read_attempt;
mod readiness;
pub mod response;
mod tool_catalog;
mod workbench;
use response::ResponseArgs;
// The Answer arguments with their canonical request. Every reader decodes
// the advertised call (`AnswerInput`: `task`, or the existing full request)
// through Core's one conversion, `AnswerCallV3::into_request`; both forms
// give the same `request`. Serializing writes the canonical request. Plain
// comments: a doc comment here would be advertised as the schema description.
#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(try_from = "AnswerInput")]
pub struct AnswerArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub request: airk_core::arbiter::answer_loop::ContextRequestV3,
    /// Beside the request, never inside it: the request digest is unchanged.
    #[serde(flatten)]
    pub job: airk_core::arbiter::answer_loop::AnswerJobOptions,
    #[serde(flatten, skip_serializing)]
    pub response: ResponseArgs,
}
// The advertised Answer arguments, as a caller sends them.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct AnswerInput {
    #[serde(flatten)]
    verification: response::VerificationArgs,
    #[serde(flatten)]
    call: airk_core::arbiter::answer_loop::AnswerCallV3,
    #[serde(flatten)]
    job: airk_core::arbiter::answer_loop::AnswerJobOptions,
    #[serde(flatten)]
    response: ResponseArgs,
}
impl TryFrom<AnswerInput> for AnswerArgs {
    type Error = airk_core::arbiter::answer_loop::AnswerCallError;
    fn try_from(input: AnswerInput) -> Result<Self, Self::Error> {
        Ok(Self {
            verification: input.verification,
            request: input.call.into_request()?,
            job: input.job,
            response: input.response,
        })
    }
}
pub mod combined;
/// A kernel Answer progress observation as its MCP progress notification.
fn answer_progress(item: airk_kernel::arbiter::AnswerProgress) -> progress::AnswerProgress {
    use airk_core::arbiter::answer_loop::LoopStage;
    let stage = match item.stage {
        LoopStage::Admission => progress::AnswerStage::Queued,
        LoopStage::Split => progress::AnswerStage::Split,
        LoopStage::Plan => progress::AnswerStage::Plan,
        LoopStage::Retrieve | LoopStage::Read => progress::AnswerStage::Gather,
        LoopStage::Coverage => progress::AnswerStage::Coverage,
        LoopStage::FollowUp => progress::AnswerStage::FollowUp,
        LoopStage::Compose => progress::AnswerStage::Compose,
        LoopStage::Union => progress::AnswerStage::Union,
    };
    let count = |value: usize| u8::try_from(value).unwrap_or(u8::MAX);
    progress::AnswerProgress {
        part: item.part,
        parts: item.parts,
        round: item.round,
        stage,
        sub_needs_covered: count(item.covered),
        sub_needs: count(item.sub_needs),
    }
}pub mod explorer_offline;
pub mod help;
pub mod local;
pub mod open;
pub mod progress;
pub mod query_capacity;
pub mod remote;
pub mod scope_instructions;
pub mod unified;

#[cfg(test)]
mod optimization_tests;

#[cfg(test)]
mod arbiter_calibration_capture;
#[cfg(test)]
mod arbiter_handler_tests;
#[cfg(test)]
mod arbiter_public_tests;
#[cfg(all(test, feature = "local-inference", feature = "test-support"))]
mod context_action_capture_tests;
#[cfg(test)]
mod focus_handler_tests;
#[cfg(test)]
mod scoped_device_diagnostics_tests;
#[cfg(test)]
mod scoped_observation_tests;
#[cfg(test)]
mod ui_pilot_cases;
#[cfg(test)]
mod verification_tests;

#[cfg(test)]
mod workbench_tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn parse_prepares_and_executes_without_an_editor_and_rejects_write_operations()
    -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("source");
        std::fs::create_dir(&root)?;
        let original = "# Queue policy\nRequests must preserve their retry budget.\n";
        std::fs::write(root.join("policy.md"), original)?;
        let kernel = Kernel::open(&root, Some(temporary.path().join("state")))?;
        kernel.index()?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let reply = tools
            .call(
                "airk_parse".into(),
                json!({"schema":"airk.parse.request/1","text":"/search retry --mode lexical"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert!(!reply.is_error.unwrap_or(false), "{reply:?}");
        let value: serde_json::Value =
            serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
        assert_eq!(value["status"], "prepared");
        assert_eq!(value["executed"], false);
        let run=tools.call("airk_parse".into(),json!({"schema":"airk.parse.request/1","mode":"execute","plan":value["plan"],"response_mode":"full"}).as_object().unwrap().clone()).await?;
        assert!(!run.is_error.unwrap_or(false), "{run:?}");
        let body: serde_json::Value =
            serde_json::from_str(&run.content[0].as_text().unwrap().text)?;
        assert_eq!(body["status"], "complete");
        assert_eq!(body["executed"], true);
        assert!(!body["evidence"].as_array().unwrap().is_empty());
        let bad=tools.call("airk_parse".into(),json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"bad","tool":"draft_plan","args":{}}]}}).as_object().unwrap().clone()).await?;
        assert!(bad.is_error.unwrap_or(false));
        assert_eq!(std::fs::read_to_string(root.join("policy.md"))?, original);
        tools.close().await?;
        Ok(())
    }
}

/// Strict envelope; nested plans are validated by the transport-independent kernel contract.
#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParseArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten, skip_serializing)]
    pub response: ResponseArgs,
    pub schema: String,
    /// prepare (default) validates; execute collects read-only evidence.
    pub mode: Option<String>,
    pub text: Option<String>,
    pub recipe: Option<serde_json::Map<String, serde_json::Value>>,
    pub plan: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub path_prefix: String,
    pub budgets: Option<serde_json::Map<String, serde_json::Value>>,
    pub output: Option<serde_json::Map<String, serde_json::Value>>,
    pub expected_digest: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesArgs {
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// One exact adapter ID; omit for supported-format summary.
    pub adapter: Option<String>,
    /// Include full adapter profiles and limitations instead of the compact summary.
    #[serde(default)]
    pub rich: bool,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusArgs {
    #[serde(flatten)]
    pub response: ResponseArgs,
}

#[derive(Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct EvidenceArgs {
    /// Publication token of the citation or reply you are following. The read uses that snapshot; a superseded token is refused, and no older copy is served.
    pub revision: Option<String>,
    /// Representation validator returned by this same read and options.
    pub if_evidence: Option<String>,
    /// Follow only a unique compatible exact-content move; false by default.
    #[serde(default)]
    pub follow_moves: bool,
    /// Remembered source hash; validates bytes, not extracted evidence.
    pub known_hash: Option<String>,
    pub artifact_id: Option<String>,
}

impl EvidenceArgs {
    fn options(&self) -> anyhow::Result<airk_core::evidence::EvidenceReadOptions> {
        Ok(serde_json::from_value(serde_json::to_value(self)?)?)
    }
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectorArgs {
    Path { path: String },
    Citation { path: String, source_hash: String },
    Artifact { artifact_id: String },
    Symbol { symbol_key: String },
    Document { document_key: String },
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangesArgs {
    #[serde(flatten, skip_serializing)]
    pub verification: response::VerificationArgs,
    #[serde(flatten, skip_serializing)]
    pub response: ResponseArgs,
    pub since: Option<String>,
    /// Up to 64 remembered selectors totaling at most 32 KiB.
    #[serde(default)]
    pub selectors: Vec<SelectorArgs>,
    /// Literal collection-relative prefix; a move matches either endpoint.
    #[serde(default)]
    pub path_prefix: String,
    #[serde(default)]
    pub include_symbols: bool,
    #[serde(default)]
    pub include_documents: bool,
    /// Bounded similarity/directory hints from retained evidence; never identity proof.
    #[serde(default)]
    pub include_suggestions: bool,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
    pub cursor: Option<String>,
}

impl ChangesArgs {
    fn request(&self) -> anyhow::Result<airk_core::changes::ChangesRequest> {
        Ok(serde_json::from_value(serde_json::to_value(self)?)?)
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// What to find. In lexical mode the words are AND terms; quotes and punctuation do not make a phrase.
    pub query: String,
    /// Omit to follow the configured policy, which serves lexical results during a semantic helper outage and says so. An explicit vector or hybrid mode is refused when the semantic index cannot serve it.
    pub mode: Option<SearchMode>,
    /// Compatibility alias for vector-only retrieval.
    #[serde(default)]
    pub semantic: bool,
    /// Optional literal collection-relative path prefix, not a glob.
    pub path_prefix: Option<String>,
    /// Exact indexed extraction-language/profile label, as replies report it (for example rust or markdown).
    pub language: Option<String>,
    /// Indexed evidence category, based on extraction metadata rather than filename suffix.
    pub evidence_kind: Option<EvidenceKind>,
    /// Optional literal collection-relative prefix to exclude; does not change source authorization.
    pub exclude_path_prefix: Option<String>,
    /// Maximum number of results. Omission lets Search choose the count.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "airk_core::verification::present_value"
    )]
    #[schemars(schema_with = "search_limit_schema")]
    pub limit: Option<usize>,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
    /// Return complete indexed chunks instead of focused snippets.
    pub full_chunks: Option<bool>,
}

fn search_limit_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"integer","minimum":1,"maximum":100})
}

#[derive(Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Lexical,
    Vector,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Code,
    Document,
    Configuration,
    Unknown,
}

impl From<EvidenceKind> for airk_core::EvidenceKind {
    fn from(value: EvidenceKind) -> Self {
        match value {
            EvidenceKind::Code => Self::Code,
            EvidenceKind::Document => Self::Document,
            EvidenceKind::Configuration => Self::Configuration,
            EvidenceKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NameMatch {
    Exact,
    Suffix,
    Contains,
}

impl From<NameMatch> for airk_core::discovery::NameMatch {
    fn from(value: NameMatch) -> Self {
        match value {
            NameMatch::Exact => Self::Exact,
            NameMatch::Suffix => Self::Suffix,
            NameMatch::Contains => Self::Contains,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    #[serde(flatten)]
    pub evidence: EvidenceArgs,
    /// Collection-relative indexed path returned by AIRK, never an absolute path.
    pub path: String,
    /// First indexed line, 1-based and inclusive. Extracted document lines are not page/slide numbers.
    pub start_line: usize,
    /// Last indexed line, 1-based and inclusive; keep extracted documents within one page/block/slide.
    pub end_line: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Literal collection-relative path prefix, not a glob. Empty lists from the collection root.
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

/// The bytes an Answer reply keeps back from Union for what the handler adds
/// after it. The receipt takes (frame - reply - 2048) / 2 of the escaped
/// reply (`receipt_budget_within`): twice its floor plus 2048 leaves the
/// floor, and another 2048 holds the annotations the handler adds after
/// Union, as the 4,096 reserve does without verification (change-log entry
/// 53). A job reply's delivery record is one of those annotations.
pub(crate) fn answer_reply_reserve(
    controls: airk_core::verification::VerificationControls,
) -> anyhow::Result<usize> {
    Ok(controls
        .resolve()?
        .map(|budget| {
            airk_kernel::verification::verification_reply_metadata_floor(budget)
                .map(|floor| floor.saturating_mul(2).saturating_add(4096))
        })
        .transpose()?
        .unwrap_or(4096))
}

fn default_limit() -> usize {
    10
}
fn default_budget() -> usize {
    8192
}

fn file_info_profile_families_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(airk_core::native_reads::file_info_profile_families_schema())
        .expect("static file-info family input")
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    #[serde(default)]
    #[schemars(schema_with = "file_info_profile_families_schema")]
    pub include_profile_families: bool,
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    #[serde(flatten)]
    pub evidence: EvidenceArgs,
    /// Collection-relative indexed path returned by AIRK, never an absolute path.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Minimal navigation by default. False requests rich facts, IDs and details; independent of response_mode.
    pub compact: Option<bool>,
    #[serde(flatten)]
    pub evidence: EvidenceArgs,
    /// Exact collection-relative indexed file path, not a prefix or glob.
    pub path: String,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Minimal navigation by default. False requests rich facts and symbol IDs; independent of response_mode.
    pub compact: Option<bool>,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Literal collection-relative path prefix, not a glob. Empty searches the active collection.
    #[serde(default)]
    pub path_prefix: String,
    /// Exact indexed file path; takes precedence over path_prefix.
    pub path: Option<String>,
    /// Definition name for symbols (exact by default); imports use case-insensitive substring by default. Omit to list.
    pub name: Option<String>,
    /// Explicit case-sensitive matching of simple/qualified names. Omit for this tool's existing default.
    pub name_match: Option<NameMatch>,
    /// Request bounded count precision; an incomplete scan never supplies a fabricated total.
    #[serde(default)]
    pub include_count: bool,
    /// Optional indexed language/profile label, as replies report it (for example rust or markdown).
    /// Exact indexed native or retained producer language/profile label; 1..128 UTF-8 bytes. JSON maxLength is a character bound; native validation enforces the byte bound.
    #[schemars(length(min = 1, max = 128))]
    pub language: Option<String>,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

impl FactArgs {
    fn query(self, kind: Option<&str>, exact_name: bool, document_only: bool) -> FactQuery {
        FactQuery {
            path_exact: self.path.is_some(),
            path_prefix: self.path.unwrap_or(self.path_prefix),
            name: self.name,
            language: self.language,
            kind: kind.map(str::to_owned),
            exact_name,
            limit: self.limit,
            max_bytes: self.max_bytes,
            document_only,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentFactArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Compact statement/location evidence; defaults on for Agent Lean. False requests rich details.
    pub compact: Option<bool>,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Durable document key from indexed evidence; exclusive of name/kind/language/path filters.
    pub document_key: Option<String>,
    /// Literal collection-relative path prefix, not a glob.
    #[serde(default)]
    pub path_prefix: String,
    /// Exact indexed file path; takes precedence over path_prefix.
    pub path: Option<String>,
    /// Optional case-insensitive substring of the fact's source statement or heading/date name.
    pub name: Option<String>,
    /// Optional indexed language/profile label, as replies report it (for example rust or markdown).
    /// Exact indexed native or retained producer language/profile label; 1..128 UTF-8 bytes. JSON maxLength is a character bound; native validation enforces the byte bound.
    #[schemars(length(min = 1, max = 128))]
    pub language: Option<String>,
    /// Native category examples: requirement, date, architecture, design, feature, decision, acceptance, section, table, table_row, table_cell, key or element.
    /// Exact native or retained producer kind label; 1..128 UTF-8 bytes. Labels are open, not an enum; native validation enforces the byte bound.
    #[schemars(length(min = 1, max = 128))]
    pub kind: Option<String>,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// The identifier exactly as written, whole and case-sensitive; not a regex or symbol ID.
    pub name: String,
    #[serde(default)]
    pub path_prefix: String,
    /// Exact indexed extraction-language/profile label.
    pub language: Option<String>,
    /// Indexed extraction category, not a filename-extension guess.
    pub evidence_kind: Option<EvidenceKind>,
    /// Literal indexed relative prefix to exclude from candidate selection.
    pub exclude_path_prefix: Option<String>,
    /// Continuation from references; repeat the same filters and ordering.
    pub cursor: Option<String>,
    /// Explicit bounded reference ordering; omit to preserve the legacy index order.
    pub order: Option<ReferenceOrder>,
    /// Include a bounded summary of matching lines per file from this page's scanned scope.
    #[serde(default)]
    pub include_files: bool,
    /// Include count precision; incomplete scans report a lower bound, never a guessed full total.
    #[serde(default)]
    pub include_count: bool,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceOrder {
    Index,
    Path,
    CodeFirst,
}

impl From<ReferenceOrder> for airk_core::references::ReferenceOrder {
    fn from(value: ReferenceOrder) -> Self {
        match value {
            ReferenceOrder::Index => Self::Index,
            ReferenceOrder::Path => Self::Path,
            ReferenceOrder::CodeFirst => Self::CodeFirst,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    #[serde(flatten)]
    pub evidence: EvidenceArgs,
    /// Collection-relative indexed file path returned with evidence.
    pub path: String,
    /// 1-based indexed line from AIRK evidence; extracted document lines are not page/slide numbers.
    pub line: usize,
    #[serde(default = "default_radius")]
    #[schemars(schema_with = "context_radius_schema")]
    pub before: usize,
    #[serde(default = "default_radius")]
    #[schemars(schema_with = "context_radius_schema")]
    pub after: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}
fn context_radius_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type":"integer","minimum":0,"maximum":airk_core::native_reads::CONTEXT_RADIUS_MAX
    }))
    .expect("static native context radius schema")
}
fn relationship_direction_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({"type":"string","enum":["incoming","outgoing"]}))
        .expect("static relationship direction schema")
}
fn default_radius() -> usize {
    airk_core::native_reads::CONTEXT_RADIUS_DEFAULT
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeFactArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Compact signatures, locations and provenance; defaults on for Agent Lean. False requests rich details.
    pub compact: Option<bool>,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Durable symbol key from indexed evidence; exclusive of name/kind/language/path filters.
    pub symbol_key: Option<String>,
    /// Exact written name, qualified name, symbol_id or terminal native call method name; case-sensitive. Omit to list.
    pub name: Option<String>,
    /// Exact by default; suffix/contains match simple/qualified names case-sensitively, never symbol-ID substrings.
    pub name_match: Option<NameMatch>,
    /// Request bounded count precision; exact totals are returned only for a complete matching scan.
    #[serde(default)]
    pub include_count: bool,
    /// Native category examples: definition, import, call, inherits, reference, route_registration, route_annotation, database_call, dependency_injection, test_registration, test_declaration or exception_handler.
    /// Exact native or retained producer kind label; 1..128 UTF-8 bytes. Labels are open, not an enum; native validation enforces the byte bound.
    #[schemars(length(min = 1, max = 128))]
    pub kind: Option<String>,
    /// Exact indexed native or retained producer language/profile label; 1..128 UTF-8 bytes. JSON maxLength is a character bound; native validation enforces the byte bound.
    #[schemars(length(min = 1, max = 128))]
    pub language: Option<String>,
    #[serde(default)]
    pub path_prefix: String,
    /// Exact indexed file path; takes precedence over path_prefix.
    pub path: Option<String>,
    /// First line, 1-based and inclusive, of a range in the exact path; requires end_line. A fact matches when its own lines overlap the range, e.g. the definition containing a line.
    #[schemars(range(min = 1))]
    pub start_line: Option<usize>,
    /// Last line, 1-based and inclusive; requires start_line and an exact path. Not combinable with symbol_key.
    #[schemars(range(min = 1))]
    pub end_line: Option<usize>,
    /// Minimal rows: name, kind, lines, signature and owner (innermost enclosing scope); path, content hash and provenance once per file group. Rows omit symbol IDs. Not combinable with compact=false, reply_detail=rich or symbol_key.
    #[serde(default)]
    pub minimal: bool,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipArgs {
    #[serde(flatten)]
    pub verification: response::VerificationArgs,
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Use code.symbol_id from code facts/symbols, a file:path owner, or an exact written syntax target (e.g. db.query). Syntax targets are not resolved symbol IDs.
    pub symbol: String,
    /// incoming for callers/implementation candidates; outgoing for dependencies/containment. Defaults incoming.
    #[serde(default = "default_direction")]
    #[schemars(schema_with = "relationship_direction_schema")]
    pub direction: String,
    /// Exact edge kind: call, contains, import, inherits, reference, implements, type_definition, definition or a framework category.
    pub kind: Option<String>,
    #[serde(default)]
    pub path_prefix: String,
    /// 1..3 graph hops; candidate impact is incomplete and references are not calls.
    #[serde(default = "default_hops")]
    #[schemars(range(min = 1, max = 3))]
    pub hops: usize,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    #[serde(default = "default_budget")]
    #[schemars(range(min = 1, max = 65536))]
    pub max_bytes: usize,
}

fn continuity_selector(
    key: Option<&str>,
    symbol: bool,
    has_filters: bool,
) -> anyhow::Result<Option<airk_core::continuity::ContinuityKey>> {
    let Some(key) = key else {
        return Ok(None);
    };
    anyhow::ensure!(
        !has_filters,
        "a continuity key cannot be combined with name, kind, language, path, matching, count or continuation options"
    );
    Ok(Some(if symbol {
        airk_core::continuity::ContinuityKey::Symbol(key.parse()?)
    } else {
        airk_core::continuity::ContinuityKey::Document(key.parse()?)
    }))
}
/// Code-fact representation `(compact, minimal)`. Without `minimal` this is
/// the existing choice: explicit compact/reply_detail, else compact under
/// Agent Lean. `minimal` and the line range address facts of one file and
/// are refused with a durable symbol key.
fn code_fact_projection(
    args: &CodeFactArgs,
    applied_detail: impl FnOnce() -> anyhow::Result<Option<airk_core::extraction::IndexDetail>>,
) -> anyhow::Result<(bool, bool)> {
    use airk_core::read_arguments::ReadArgumentError;
    if args.symbol_key.is_some() {
        for (field, used) in [
            ("start_line", args.start_line.is_some()),
            ("end_line", args.end_line.is_some()),
            ("minimal", args.minimal),
        ] {
            if used {
                return Err(ReadArgumentError::new(
                    "invalid_arguments",
                    field,
                    format!("{field} cannot be combined with symbol_key"),
                )
                .into());
            }
        }
    }
    let selected =
        airk_core::reply::ReplyDetail::compact(args.response.reply_detail, args.compact)?;
    if args.minimal {
        if selected == Some(false) {
            return Err(ReadArgumentError::new(
                "invalid_arguments",
                "minimal",
                "minimal rows cannot be combined with compact=false or reply_detail=rich",
            )
            .into());
        }
        return Ok((false, true));
    }
    Ok((
        match selected {
            Some(value) => value,
            None => applied_detail()? == Some(airk_core::extraction::IndexDetail::AgentLean),
        },
        false,
    ))
}
fn default_direction() -> String {
    "incoming".into()
}
fn default_hops() -> usize {
    1
}

#[derive(Clone)]
pub struct AirkServer {
    work: std::sync::Arc<std::sync::RwLock<Option<serde_json::Value>>>,
    runtime: std::sync::Arc<std::sync::RwLock<Option<serde_json::Value>>>,
    kernel: Kernel,
    tool_router: ToolRouter<Self>,
    client: ClientFamily,
    admission: execution::Admission,
    responses: std::sync::Arc<response::Session>,
    response_mode: response::ResponseMode,
    // Inner plan reads share the outer admitted lifetime and usage observation.
    plan_read: bool,
    answer_read: bool,
    read_attempt: Option<read_attempt::ReadAttemptContext>,
    // The service's Answer job table; empty outside an owner in the service.
    answer_jobs: answer_jobs::Slot,
}

#[tool_router]
impl AirkServer {
    /// Only fixed native definitions and context-taking handler functions are
    /// shared. Every server still owns its kernel, authority and observations.
    pub(crate) fn shared_tool_router() -> ToolRouter<Self> {
        static ROUTER: std::sync::OnceLock<ToolRouter<AirkServer>> = std::sync::OnceLock::new();
        ROUTER.get_or_init(Self::tool_router).clone()
    }

    pub fn new(kernel: Kernel) -> Self {
        let admission = execution::Admission::for_kernel(&kernel);
        let responses = std::sync::Arc::new(response::Session::for_scope(
            kernel.scoped_read_fingerprint().ok().flatten(),
        ));
        Self {
            work: Default::default(),
            runtime: Default::default(),
            kernel,
            tool_router: Self::shared_tool_router(),
            client: ClientFamily::Unknown,
            admission,
            responses,
            response_mode: response::ResponseMode::from_environment(),
            plan_read: false,
            answer_read: false,
            read_attempt: None,
            answer_jobs: Default::default(),
        }
    }

    #[tool(
        name = "airk_help",
        description = "Explain this AIRK version and how to use its CLI and MCP tools. Omit arguments for an overview and topic catalog; select a topic, an exact CLI command path, or an MCP tool name for guidance, examples and live schemas. Read-only bundled help: no workspace scan, indexing or command execution. Use start_line/max_bytes for bounded continuation. response_mode=full is accepted; help is the same in both modes. This is usage guidance, not live configuration or runtime-health evidence; use airk_status for that.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn help(&self, Parameters(args): Parameters<help::HelpArgs>) -> CallToolResult {
        response::with_scope(&self.kernel, help::reply(args))
    }

    #[tool(
        name = "airk_query_scope",
        description = "Discover only the connection's reviewed Project/index grant. Projects, indexes and sources have bounded pages and revision-bound continuations. recommendations returns bounded metadata-only advice and catalog-checked scope instructions, with optional explicit selection/situation and no cursor or limit. Legacy connections report configuration_required. Never enumerates the machine library, queries evidence or opens sources.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn query_scope(
        &self,
        Parameters(args): Parameters<combined::QueryScopeParameters>,
    ) -> CallToolResult {
        let _permit = match self.admission.admit(true).await {
            Ok(permit) => permit,
            Err(result) => return response::with_scope(&self.kernel, result),
        };
        match self.kernel.query_scope(&args.0) {
            Ok(body) => serde_json::from_value(unified::success(body))
                .expect("canonical installed discovery envelope"),
            Err(error) => response::scoped_error_result(&self.kernel, &error),
        }
    }

    #[tool(
        name = "airk_answer",
        description = "Use for a task that needs evidence from several places, such as a change that spans implementation and tests, even when the task names a symbol; call again with narrower needs as you learn more. Pass the task text as task, and optionally needs, anchors, deliverable and known. Returns a short map: for each need, where it is answered and the best passages, and what was not found. A code_change or test deliverable, or stated needs, also gathers the project rules, how tests run and the build file, and lists any it does not find. Ranges listed without text can be read with airk_read_file. For one definition use airk_symbols; for one known file, airk_outline. response_mode=full returns the complete reply.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn answer(
        &self,
        Parameters(args): Parameters<AnswerArgs>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> CallToolResult {
        let mut reader = self.clone();
        reader.plan_read = true;
        reader.answer_read = true;
        let handle = tokio::runtime::Handle::current();
        let reporter = progress::ProgressReporter::from_request(&context.meta, context.peer.clone());
        let sink = reporter
            .as_ref()
            .map_or_else(progress::no_progress, |reporter| reporter.sink());
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(
                args.request
                    .budget
                    .deadline_ms
                    .min(airk_core::arbiter::answer_loop::MAX_DEADLINE_MS),
            ))
            .unwrap_or_else(std::time::Instant::now);
        let deadline = self
            .read_attempt
            .as_ref()
            .map_or(deadline, |context| deadline.min(context.deadline()));
        let jobs = answer_jobs::for_call(self);
        let include_status = args.response.include_status;
        let answered = self.execute_verified_response(
            UsageOperation::Answer,
            args.response,
            args.verification.controls(),
            move |kernel| {
                // Job options are bounded caller input on every path.
                args.job.validate(&args.request.budget)?;
                let reserve = answer_reply_reserve(args.verification.controls())?;
                let kernel = kernel
                    .with_context_reply_reserve(reserve)?
                    .with_fresh_answer_accounting()?;
                reader.kernel = kernel.clone();
                // In the service, an admitted Answer is a job of its job
                // table (plan sections 12.4.1 and 12.4.2); this call waits
                // for it.
                if let Some((jobs, authority)) = jobs {
                    return answer_jobs::run(answer_jobs::Caller {
                        jobs,
                        authority,
                        reader,
                        kernel,
                        request: args.request,
                        options: args.job,
                        reply_reserve: reserve,
                        include_status,
                        deadline,
                        handle,
                        sink,
                    });
                }
                // Custody is dropped after delivery until the service job
                // table keeps it (change log entry 21); the reply keeps the
                // decision log digest.
                kernel
                    .answer_with(
                        args.request,
                        airk_kernel::arbiter::AnswerEnv {
                            deadline,
                            require_live: &|| {
                                if let Some(context) = &reader.read_attempt {
                                    context.require_live()?;
                                }
                                Ok(())
                            },
                            reader: &|tool, args, revision| {
                                handle.block_on(reader.workbench_read(tool, args, revision))
                            },
                            clock: &std::time::Instant::now,
                            progress: &|item| sink(answer_progress(item)),
                        },
                    )
                    .map(|outcome| answer_jobs::Answered {
                        response: outcome.response,
                        delivery: None,
                    })
            },
        );
        match &reporter {
            Some(reporter) => reporter.while_running(answered).await,
            None => answered.await,
        }
    }

    #[tool(
        name = "airk_parse",
        description = "Prepare or execute a bounded read-only AIRK evidence plan. Supply schema airk.parse.request/1, mode prepare or execute, and exactly one of text, recipe, plan. Explicit plans contain steps {id,tool,args,bind}; native operation names only, such as symbols or read_file. Bindings use earlier step IDs and named selectors. At most 6 steps, 12 reads, a 30-second budget checked between reads, 512 KiB. Learned text in this review build returns an inert proposed plan with needs_input; review its arguments and resubmit the explicit plan to execute. Other uncertainty, partial coverage and failures stay explicit. No writes, commands, management, recursive plans or agent reasoning. Works without Explorer. Use response_mode=full for provenance/coverage audits; raw inner evidence retains its full kernel representation.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn parse(&self, Parameters(args): Parameters<ParseArgs>) -> CallToolResult {
        let enabled = |value: Option<&serde_json::Value>| {
            value.is_some_and(|value| matches!(value.as_str(), Some("report" | "require")))
        };
        let track_verification = args
            .verification
            .verification
            .is_none_or(|mode| mode != airk_core::verification::VerificationMode::Off)
            || args.plan.as_ref().is_some_and(|plan| {
                enabled(plan.get("verification"))
                    || plan
                        .get("steps")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|steps| {
                            steps.iter().any(|step| {
                                enabled(step.get("args").and_then(|args| args.get("verification")))
                            })
                        })
            })
            || args.text.as_ref().is_some_and(|text| {
                text.contains("--verification report") || text.contains("--verification require")
            });
        let reply_detail = args.response.reply_detail;
        let observations = serde_json::json!({"runtime":self.runtime.read().ok().and_then(|v|v.clone()),"work":self.work.read().ok().and_then(|v|v.clone())});
        let mut reader = self.clone();
        reader.plan_read = true;
        let handle = tokio::runtime::Handle::current();
        self.execute_verification_aware(
            UsageOperation::Parse,
            args.response,
            track_verification,
            move |kernel| {
                reader.kernel = kernel.clone();
                let mut value = serde_json::to_value(args)?;
                if let Some(detail) = reply_detail {
                    value["reply_detail"] = serde_json::to_value(detail)?;
                }
                if value["mode"].is_null() {
                    value["mode"] = serde_json::json!("prepare");
                }
                kernel.evidence_plan_with_reader(
                    airk_core::workbench::decode_request(value)?,
                    &observations,
                    Some(&|tool, args, revision| {
                        handle.block_on(reader.workbench_read(tool, args, revision))
                    }),
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_code_facts",
        description = "Use when a definition needs its signature, scope, written types, call/framework candidates or a symbol ID for airk_relationships. Exact name is default, including qualified names and terminal native call method names; name_match=suffix/contains explicitly broadens case-sensitive simple/qualified names. A durable symbol_key is exclusive of filters, matching and count options. Compact discovery is the default under Agent Lean; expand selected candidates with compact=false. Cursors bind the exact query/publication; include_count reports bounded precision, not a guessed total. Check basis/coverage; call/framework targets are syntax candidates. Next: airk_relationships with a symbol ID, airk_read_file for the source. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn code_facts(&self, Parameters(args): Parameters<CodeFactArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::CodeFacts,
            args.response,
            args.verification.controls(),
            move |kernel| {
                let (compact, minimal) =
                    code_fact_projection(&args, || kernel.applied_index_detail())?;
                if let Some(key) = continuity_selector(
                    args.symbol_key.as_deref(),
                    true,
                    args.name.is_some()
                        || args.name_match.is_some()
                        || args.include_count
                        || args.kind.is_some()
                        || args.language.is_some()
                        || args.path.is_some()
                        || !args.path_prefix.is_empty()
                        || args.cursor.is_some(),
                )? {
                    return kernel.continuity_facts(key, args.limit, args.max_bytes);
                }
                kernel.code_facts_page_with_options(
                    &FactQuery {
                        path_exact: args.path.is_some(),
                        path_prefix: args.path.unwrap_or(args.path_prefix),
                        name: args.name,
                        kind: args.kind,
                        language: args.language,
                        exact_name: true,
                        limit: args.limit,
                        max_bytes: args.max_bytes,
                        document_only: false,
                    },
                    &airk_core::discovery::DiscoveryOptions {
                        minimal,
                        cursor: args.cursor,
                        compact,
                    },
                    &airk_core::discovery::FactDiscoveryOptions {
                        name_match: args.name_match.map(Into::into),
                        include_count: args.include_count,
                        start_line: args.start_line,
                        end_line: args.end_line,
                    },
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_relationships",
        description = "Use for impact work from a known symbol: returns bounded incoming/outgoing structural candidates with path scope and 1..3 hops. Written targets such as Type::new or obj.method identify textual caller candidates; a struct symbol ID alone does not prove a complete caller set. Verified type/method suggestions, when available, are navigation candidates rather than resolved associations. Agent Lean/Rich Structure retain supported calls; SCIP assertions are not calls. Check coverage and truncation: absent edges never prove no impact. Next: airk_read_file on candidates, airk_references for textual uses. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn relationships(
        &self,
        Parameters(args): Parameters<RelationshipArgs>,
    ) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Relationships,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.relationships(&RelationshipQuery {
                    symbol: args.symbol,
                    direction: args.direction,
                    kind: args.kind,
                    path_prefix: args.path_prefix,
                    hops: args.hops,
                    limit: args.limit,
                    max_bytes: args.max_bytes,
                })
            },
        )
        .await
    }

    #[tool(
        name = "airk_search",
        description = "Use to find a concept, behavior or phrase across the index, or within one area with path_prefix, when you do not have its exact name. Prose, comments and code are searched together. Returns cited passages: ranked by relevance in the default hybrid or vector mode; in explicit lexical mode, passages containing every term, in stable index order rather than by relevance. Omit mode to follow the configured policy: while the semantic helper is unavailable it serves lexical results and says so; an explicit vector or hybrid mode is refused instead. An empty result reports its served mode, filters and readiness. If a passage is not enough, read more of its file with airk_read_file.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        let attempt = self.read_attempt.clone();
        let runtime = tokio::runtime::Handle::current();
        self.execute_verified_response(
            UsageOperation::Search,
            args.response,
            args.verification.controls(),
            move |kernel| {
                anyhow::ensure!(
                    !args.semantic || args.mode.is_none(),
                    "choose mode or semantic, not both"
                );
                let query = SearchQuery {
                    text: args.query,
                    path_prefix: args.path_prefix,
                    limit: args.limit.unwrap_or(10),
                    max_bytes: args.max_bytes,
                };
                use airk_kernel::current::CurrentSearchMode;
                let mode = match args.mode {
                    Some(SearchMode::Lexical) => CurrentSearchMode::Lexical,
                    Some(SearchMode::Vector) => CurrentSearchMode::Vector,
                    Some(SearchMode::Hybrid) => CurrentSearchMode::Hybrid,
                    None if args.semantic => CurrentSearchMode::Vector,
                    None => CurrentSearchMode::Default,
                };
                let filters = airk_core::SearchFilters {
                    language: args.language,
                    evidence_kind: args.evidence_kind.map(Into::into),
                    exclude_path_prefix: args.exclude_path_prefix,
                };
                search_repair::search(
                    kernel,
                    search_repair::SearchRequest {
                        query,
                        mode,
                        snippet_bytes: (!args.full_chunks.unwrap_or(false)).then_some(768),
                        filters,
                        automatic_limit: args.limit.is_none(),
                    },
                    attempt,
                    runtime,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_read_file",
        description = "Use when you know the path and the lines, from a citation, a hit or the task. Returns that inclusive 1-based line range from the index. Pass the publication token of the citation as revision to read the snapshot it points to; if that publication has been superseded the read is refused rather than returning different lines. For PDF, DOCX and PPTX, lines are extracted-text coordinates; keep a range within one page, block or slide. If you know the file but not the lines, use airk_outline, or airk_search with path_prefix.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn read_file(&self, Parameters(args): Parameters<ReadArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::ReadFile,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.read_file_conditional(
                    &args.path,
                    args.start_line,
                    args.end_line,
                    args.max_bytes,
                    &args.evidence.options()?,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_list_files",
        description = "Use when you know a directory but not the file. prefix is a literal path prefix, not a glob. Returns the indexed files under it, bounded by limit and max_bytes and paged by cursor. Then, if useful: airk_outline or airk_read_file on a chosen path.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_files(&self, Parameters(args): Parameters<ListArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::ListFiles,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.list_files_page(
                    &args.prefix,
                    args.limit,
                    args.max_bytes,
                    &airk_core::discovery::DiscoveryOptions {
                        minimal: false,
                        cursor: args.cursor,
                        compact: false,
                    },
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_status",
        description = "Use before broad parallel retrieval, or when you need the publication token or readiness. Returns the index publication, source-update state, semantic readiness and query_concurrency, the ceiling on parallel evidence reads; a read beyond it waits or gets a typed retry. observed_unix_ms and cache_age_ms give the observation age; an unknown result means readiness was not observed in time. Does not start indexing or repair.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn status(&self, Parameters(args): Parameters<StatusArgs>) -> CallToolResult {
        self.execute_response_policy(UsageOperation::Status,args.response,false,move|kernel|{
            let mut status = kernel.status_summary().or_else(|error| {
                if error.downcast_ref::<airk_core::scoped_reads::ScopeReadRefused>().is_some()
                    || error.is::<airk_kernel::SourcePolicyReadUnavailable>() {
                    return Err(error);
                }
                if airk_kernel::retrieval_retry::read_retry_hint(&error).is_some() {
                    return Err(error);
                }
                kernel.attach_rebuild_observation(serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","index_state":"unavailable","publication":null,"files":null,"chunks":null,"freshness":kernel.changes_freshness(),"index_error":{"code":"index_status_unavailable","message":"Published index metadata could not be read. Use diagnostics for details."}}))
            })?;
            status["observed_unix_ms"] = serde_json::json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64);
            status["cache_age_ms"] = 0.into();
            status["cached"] = false.into();
            Ok(status)
        }).await
    }

    #[tool(
        name = "airk_diagnostics",
        description = "Troubleshoot AIRK: full published coverage, runtime versions and process identities, scoped job/queue state, worker progress, source watcher and sampled waits. The MCP frontend adds configured query_concurrency from admission. Read-only; may be slower and larger than airk_status. Use airk_status for routine readiness checks. response_mode=full detail is always included; does not start indexing or repair.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn diagnostics(&self, Parameters(mut args): Parameters<StatusArgs>) -> CallToolResult {
        args.response.response_mode = Some(response::ResponseMode::Full);
        args.response.include_status = true;
        let runtime = self.runtime.read().ok().and_then(|runtime| runtime.clone());
        let work = self.work.read().ok().and_then(|work| work.clone());
        let deadline = self.read_attempt.as_ref().map(|context| context.deadline());
        self.execute_response(UsageOperation::Status, args.response, move |kernel| {
            let mut status = match kernel.status() {
                Ok(status) => {
                    let mut status = serde_json::to_value(status)?;
                    status["index_state"] = "available".into();
                    status
                }
                Err(error)
                    if error
                        .downcast_ref::<airk_core::scoped_reads::ScopeReadRefused>()
                        .is_some()
                        || error.is::<airk_kernel::SourcePolicyReadUnavailable>()
                        || airk_kernel::retrieval_retry::read_retry_hint(&error).is_some() =>
                {
                    return Err(error);
                }
                Err(error) if runtime.is_some() || work.is_some() => {
                    unavailable_status(&kernel, runtime.clone(), work.clone(), &error.to_string())
                }
                Err(error) => return Err(error),
            };
            status["runtime"] = runtime.unwrap_or_else(
                || serde_json::json!({"schema":1,"error":"native_runtime_context_unavailable"}),
            );
            status["work"] = work
                .unwrap_or_else(|| serde_json::json!({"error":"native_work_context_unavailable"}));
            if let Some(observation) = kernel.scoped_device_status(deadline)? {
                status["scoped_device_status"] = serde_json::to_value(observation)?;
            }
            if status.get("rebuild").is_none() && status.get("rebuild_unavailable").is_none() {
                status = kernel.attach_rebuild_observation(status)?;
            }
            Ok(status)
        })
        .await
    }

    #[tool(
        name = "airk_file_info",
        description = "Use when a file's language, extraction method, syntax quality, revision or line coordinates need clarifying, for example before reading lines of a PDF or Office document. Returns its indexed language and profile, extraction method, syntax quality, content hash and line basis.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn file_info(&self, Parameters(args): Parameters<PathArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::FileInfo,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.file_info_conditional_with_profile_families(
                    &args.path,
                    &args.evidence.options()?,
                    args.include_profile_families,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_outline",
        description = "Use when you know one file and need to find where something is in it. Takes one exact path; returns its imports, definitions and document headings with line locations, in file order, paged by cursor. Compact by default; compact=false adds full facts and IDs. A large file can take several pages; airk_symbols with path finds one definition in it directly. Then, if useful: airk_read_file on a listed range.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn outline(&self, Parameters(args): Parameters<OutlineArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Outline,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.outline_conditional_page(
                    &args.path,
                    args.limit,
                    args.max_bytes,
                    &args.evidence.options()?,
                    &airk_core::discovery::DiscoveryOptions {
                        minimal: airk_core::reply::ReplyDetail::compact(
                            args.response.reply_detail,
                            args.compact,
                        )?
                        .unwrap_or(true),
                        cursor: args.cursor,
                        compact: false,
                    },
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_symbols",
        description = "Use to find where a named type, function or other definition is declared. Exact name by default (Session does not match OwnerSession); name_match=suffix or contains broadens it, case-sensitively; omit name to list the definitions under a path_prefix or language. Returns definition locations across all indexed files. It matches written names and does not resolve imports, aliases or dynamic dispatch. Compact results by default; compact=false on a chosen result adds full details and symbol IDs. Follow cursor only when you need more candidates. Then, if useful: airk_read_file for the body, airk_references for where the name is written.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn symbols(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Symbols,
            args.response,
            args.verification.controls(),
            move |kernel| {
                let options = airk_core::discovery::DiscoveryOptions {
                    minimal: airk_core::reply::ReplyDetail::compact(
                        args.response.reply_detail,
                        args.compact,
                    )?
                    .unwrap_or(true),
                    cursor: args.cursor.clone(),
                    compact: false,
                };
                let matching = airk_core::discovery::FactDiscoveryOptions {
                    name_match: args.name_match.map(Into::into),
                    include_count: args.include_count,
                    ..Default::default()
                };
                kernel.facts_page_with_options(
                    &args.query(Some("definition"), true, false),
                    &options,
                    &matching,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_imports",
        description = "Use to see what code declares it depends on: returns written import/use/include declarations by indexed language/profile, literal path prefix and optional name. Omitted name_match keeps the existing case-insensitive substring behavior; explicit exact/suffix/contains is case-sensitive. Use cursor continuation and optional include_count for bounded count precision. These are syntax candidates, not a resolved, transitive or dynamic dependency graph. Next: airk_outline or airk_read_file on a declaring file. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn imports(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Imports,
            args.response,
            args.verification.controls(),
            move |kernel| {
                let options = airk_core::discovery::DiscoveryOptions {
                    minimal: airk_core::reply::ReplyDetail::compact(
                        args.response.reply_detail,
                        args.compact,
                    )?
                    .unwrap_or(true),
                    cursor: args.cursor.clone(),
                    compact: false,
                };
                let matching = airk_core::discovery::FactDiscoveryOptions {
                    name_match: args.name_match.map(Into::into),
                    include_count: args.include_count,
                    ..Default::default()
                };
                kernel.facts_page_with_options(
                    &args.query(Some("import"), false, false),
                    &options,
                    &matching,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_document_facts",
        description = "Use to find repository requirements, dates, features, architecture/design/decision language, headings, configuration keys and asset facts. Select a durable document_key alone, or filter by exact kind, language/profile, literal path prefix or statement/name substring. Prose cues include heading context, written bounds/units and modality; they are evidence candidates, not inferred obligations. Documents retain original locations; explicitly published local OCR evidence carries its own provenance and recognition limitations. Next: airk_read_file or airk_context on a cited line. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn document_facts(
        &self,
        Parameters(args): Parameters<DocumentFactArgs>,
    ) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::DocumentFacts,
            args.response,
            args.verification.controls(),
            move |kernel| {
                let compact = match airk_core::reply::ReplyDetail::compact(
                    args.response.reply_detail,
                    args.compact,
                )? {
                    Some(value) => value,
                    None => {
                        kernel.applied_index_detail()?
                            == Some(airk_core::extraction::IndexDetail::AgentLean)
                    }
                };
                if let Some(key) = continuity_selector(
                    args.document_key.as_deref(),
                    false,
                    args.name.is_some()
                        || args.kind.is_some()
                        || args.language.is_some()
                        || args.path.is_some()
                        || !args.path_prefix.is_empty()
                        || args.cursor.is_some(),
                )? {
                    return kernel.continuity_facts(key, args.limit, args.max_bytes);
                }
                kernel.facts_page(
                    &FactQuery {
                        path_exact: args.path.is_some(),
                        path_prefix: args.path.unwrap_or(args.path_prefix),
                        name: args.name,
                        language: args.language,
                        kind: args.kind,
                        exact_name: false,
                        limit: args.limit,
                        max_bytes: args.max_bytes,
                        document_only: true,
                    },
                    &airk_core::discovery::DiscoveryOptions {
                        minimal: false,
                        cursor: args.cursor,
                        compact,
                    },
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_references",
        description = "Use to find every indexed line where an exact identifier is written, for usage or impact questions; call it directly when you know the identifier. Matches the exact-case whole identifier in code, comments, strings and documents, so results include its definitions, other names with the same spelling and mentions in text; a use written under another name is not matched. Scope with path_prefix, language or evidence_kind. Counts are matching lines. The reply says whether it scanned its whole scope; follow cursor to scan the rest. Then, if useful: airk_read_file around a chosen line.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn references(&self, Parameters(args): Parameters<ReferenceArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::References,
            args.response,
            args.verification.controls(),
            move |kernel| {
                let filters = airk_core::SearchFilters {
                    language: args.language,
                    evidence_kind: args.evidence_kind.map(Into::into),
                    exclude_path_prefix: args.exclude_path_prefix,
                };
                if args.cursor.is_none()
                    && args.order.is_none()
                    && !args.include_files
                    && !args.include_count
                    && filters.is_empty()
                {
                    return kernel.references(
                        &args.name,
                        &args.path_prefix,
                        args.limit,
                        args.max_bytes,
                    );
                }
                kernel.references_page(
                    &args.name,
                    &args.path_prefix,
                    args.limit,
                    args.max_bytes,
                    &airk_core::references::ReferenceOptions {
                        filters,
                        cursor: args.cursor,
                        order: args.order.map(Into::into).unwrap_or_default(),
                        include_files: args.include_files,
                        include_count: args.include_count,
                    },
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_context",
        description = "Use to see more lines around one line you already have from a hit or citation. Returns the neighboring indexed lines, bounded by before, after and max_bytes. Pass the publication token of the hit as revision to stay on its snapshot. For extracted documents, keep within one page, block or slide. For an exact larger range use airk_read_file.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn context(&self, Parameters(args): Parameters<ContextArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Context,
            args.response,
            args.verification.controls(),
            move |kernel| {
                kernel.context_conditional(
                    &args.path,
                    args.line,
                    args.before,
                    args.after,
                    args.max_bytes,
                    &args.evidence.options()?,
                )
            },
        )
        .await
    }

    #[tool(
        name = "airk_changes",
        description = "Use to see what the index observed changing since an exact token, or to validate remembered evidence. Supply since or selectors; returns bounded change observations. Consume every needed cursor page before advancing the baseline. Baselines retain bounded metadata across up to 64 publications/24 hours; pressure can advance explicit coverage floors. Moves are content-based candidates. Next: airk_read_file on changed ranges. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn changes(&self, Parameters(args): Parameters<ChangesArgs>) -> CallToolResult {
        self.execute_verified_response(
            UsageOperation::Changes,
            args.response,
            args.verification.controls(),
            move |kernel| kernel.changes(&args.request()?),
        )
        .await
    }

    #[tool(
        name = "airk_capabilities",
        description = "Check native formats, explicitly provisioned local model/OCR/code-provider adapters and their limits before planning exploration. Works before indexing. Reports available capabilities, not whether optional providers or this collection are ready. Terse replies summarize Workbench operation schemas; response_mode=full includes them. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn capabilities(&self, Parameters(args): Parameters<CapabilitiesArgs>) -> CallToolResult {
        let admission = self.admission.status();
        self.execute_response(UsageOperation::Capabilities, args.response, move |_| {
            anyhow::ensure!(args.adapter.as_ref().is_none_or(|id| id.len() <= 128), "adapter ID too long");
            let adapters = Kernel::extraction_adapters().into_iter()
                .filter(|item| args.adapter.as_ref().is_none_or(|id| &item.profile.adapter_id == id))
                .map(|item| if args.rich { serde_json::to_value(item) } else if args.adapter.is_some() {
                    Ok(serde_json::json!({"id":item.profile.adapter_id,"capabilities":item.capabilities}))
                } else {
                    Ok(serde_json::json!({"id":item.profile.adapter_id}))
                }).collect::<Result<Vec<_>, _>>()?;
            anyhow::ensure!(args.adapter.is_none() || !adapters.is_empty(), "unknown adapter ID");
            Ok(serde_json::json!({
            "request_admission": admission,
            "workbench": airk_kernel::workbench::operation_catalog(),
            "discovery": {"cursor_tools":["airk_list_files","airk_outline","airk_symbols","airk_imports","airk_code_facts","airk_document_facts"], "binding":"exact publication, filters and representation", "compact_outline":true, "compact_symbols":true, "fact_result_key":"items", "compact_false":"rich facts with symbol IDs; independent of response mode", "budget":"complete serialized records plus new file/provenance bindings; framing is additional", "expired_cursor":"restart discovery"},
            "adapters": adapters,
            "source_verification_capability": airk_core::verification::catalog(),
            "retained_profiles": airk_core::retention::catalog(),
            "index_detail": {"new_collection_default":"agent_lean", "levels":["agent_lean","essential","balanced","detailed"], "labels":{"agent_lean":"Agent Lean","balanced":"Rich Structure (legacy balanced)","essential":"Search Only","detailed":"Detailed"}, "coverage":"Fact and relationship responses report their published indexing level and deliberate omissions. Configured changes apply only after a successful index. Optional provider imports preserve their source and producer binding."},
            "history": {"supported":false,"exact_reads":[],"optional_retention":false,"search":false,"live_fallback":false,"checkpoints":false,"reason":"Source reads select one current index; retained change metadata does not enable old source reads"},
            "changes": {"observed_indexed_changes":false,"remembered_evidence_validation":true,"expired_baseline":"baseline_unavailable","selectors":64,"selector_bytes":32768,"max_payload_bytes":65536,"exact_revision_tokens":true,"live_source_verified":false,"symbol_continuity":"adapter-specific conservative coverage","document_continuity":"adapter-specific conservative coverage"},
            "change_retention": {"generations":64,"age_seconds":86400,"max_records":10000,"max_bytes":16777216,"source_history":false,"validation_current":"index-current","coverage":"independent floors; whole publication groups expire under generation, age or budget pressure"},
            "conditional_reads": {"tools":["airk_file_info","airk_read_file","airk_outline","airk_context"],"validator":"representation-specific if_evidence","follow_moves_default":false,"move_basis":"unique compatible exact content; not filesystem intent"},
            "file_info_profile_families":{"input":"include_profile_families","default":false,"schema":"airk.profile-families/1","families":airk_core::profile_families::Family::ALL,"basis":"metadata_only","retention":"policy eligibility, not observed presence or completeness","collection_policy":"typed unavailable under partial Project scope","configured_file_profile":"unknown when fresh source/header selection would be required","live_source_checked":false},
            "code": ["rust","python","javascript/jsx","typescript/tsx","go","java","c","cpp","csharp","ruby","php","bash","powershell","lua","swift","kotlin","sql"],
            "structured": ["markdown","json","yaml","toml","html","css","xml","svg","csv","tsv","ini","dockerfile"],
            "documents": ["UTF-8 text","text-layer PDF","DOCX main-body paragraphs/tables with original row locations","XLSX worksheet cells and row facts","PPTX slide text in presentation order"],
            "fallback": "UTF-8 lexical retrieval for all other extensions",
            "document_cues": ["English requirement/modality and design/architecture/feature/decision candidates", "heading context", "ISO and English calendar dates; numeric slash dates retain unknown locale", "written numeric bounds/ranges/units; no unit conversion"],
            "exact_fact_backend": "authenticated current SQLite; 1.1 indexes require a fresh 1.2 rebuild",
            "code_evidence": ["syntax scopes/signatures/written parameters and return types", "syntax call, inheritance, containment and import candidates", "versioned structural framework cues"],
            "optional_analysis": "Explicit source/build/digest-bound SCIP imports and local LSP/Ctags execution through the CLI. LSP queries advertised document symbols, references, definitions, types, implementations and call hierarchy against private source copies. Ctags supplies syntax declarations only. Provider assertions are not AIRK compiler certification or complete resolution; source/build changes invalidate the imported domain.",
            "optional_ocr": "Explicit CLI-only Tesseract OCR with local traineddata and optional Poppler pdftoppm for selected PDF pages; PNG/JPEG/BMP also supported. Bounded local processes preserve original source hashes/page locations and executable/model provenance. Recognition may be wrong; no automatic OCR or model download.",
            "relationship_limits": {"hops":3,"results":100,"payload_bytes":65536,"traversal_ms":250,"visited_symbols":100},
            "local_usage": "Content-free local aggregates are enabled by default; AIRK_USAGE_MODE=disabled opts out. CLI reports include measured stages, logical disk growth and process CPU where the host supports it; physical I/O remains unknown. Optional client/session imports, local tokenizer calibration and user-supplied price scenarios stay labeled; estimates are counterfactual context, never provider bills.",
            "search_modes": ["lexical", "vector", "hybrid"],
            "default_search": "Saved policy selects partial hybrid, lexical while pending, or requires complete vectors; vector opt-out selects lexical. Search and status disclose committed semantic readiness. MCP reads never start enrichment or download models.",
            "semantic_search": "Vector/hybrid modes support explicitly provisioned local static word vectors or schema-2 CPU ONNX transformer models with a local BERT WordPiece tokenizer. Assets/specification bind the indexed generation. DiskANN retrieval remains approximate; MCP reads never download or use network inference. Small real-model tests do not establish production corpus quality. semantic=true aliases vector.",
            "limitations": ["No universal native type/name resolution or complete caller graph","Optional providers require provisioning; embedding models require corpus qualification","LSP evidence queries selected symbol positions; external/unindexed locations are rejected","OCR covers selected pages and may misrecognize layout/text","No legacy binary Office formats","No SVG rendering","Composite child evidence has explicit mapping, grammar and resource limits"]
        })) }).await
    }
}

pub(crate) fn unavailable_status(
    kernel: &Kernel,
    runtime: Option<serde_json::Value>,
    work: Option<serde_json::Value>,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({"collection":kernel.collection(),"index_state":"unavailable",
        "index_error":{"code":"index_status_unavailable","message":message.chars().take(1024).collect::<String>()},
        "publication":null,"files":null,"chunks":null,"freshness":kernel.changes_freshness(),"runtime":runtime,"work":work})
}

fn render<T: Serialize>(
    kernel: &Kernel,
    usage: (airk_kernel::usage::RequestSpan, bool),
    result: anyhow::Result<T>,
    responses: &response::Session,
    projection: (
        response::ResponseMode,
        Option<airk_core::reply::ReplyDetail>,
        Option<airk_core::verification::VerificationBudget>,
        Option<std::time::Instant>,
    ),
    include_status: bool,
    operation: UsageOperation,
) -> CallToolResult {
    let (span, record_usage) = usage;
    if let Err(error) = kernel.scoped_read_fingerprint() {
        return response::scoped_error_result(kernel, &error);
    }
    let (mode, reply_detail, verification, read_deadline) = projection;
    let mut logical = result
        .as_ref()
        .ok()
        .and_then(|value| serde_json::to_value(value).ok());
    // An Answer carries its reads' status once, at reply level; a terse reply
    // keeps it only when the caller asked for status (change-log entry 54).
    if matches!(operation, UsageOperation::Answer)
        && mode != response::ResponseMode::Full
        && !include_status
        && let Some(object) = logical.as_mut().and_then(serde_json::Value::as_object_mut)
    {
        object.remove("read_status");
    }
    // A readiness observation still contended after its bounded repeats
    // fails the read with its typed retry hint (change-log entry 54).
    let mut readiness_error = None;
    if let Some(object) = logical.as_mut().and_then(serde_json::Value::as_object_mut) {
        let publication = [
            object.get("publication"),
            object
                .get("read")
                .and_then(|r| r.get("current_publication")),
            object.get("read").and_then(|r| r.get("publication")),
        ]
        .into_iter()
        .flatten()
        .find_map(|value| {
            serde_json::from_value::<airk_core::evidence::RevisionEnvelope>(value.clone()).ok()
        });
        if let Some(publication) = publication {
            object.entry("freshness").or_insert_with(|| {
                serde_json::to_value(kernel.changes_freshness()).unwrap_or(serde_json::Value::Null)
            });
            if !object.contains_key("discovery_readiness")
                || !object.contains_key("semantic_readiness")
            {
                match readiness::observe_with_retry(
                    || kernel.readiness_observation(publication.token),
                    readiness::READINESS_RETRY_BUDGET,
                    &std::time::Instant::now,
                    &std::thread::sleep,
                ) {
                    Ok(Some((discovery, semantic))) => {
                        object.entry("discovery_readiness").or_insert_with(|| {
                            serde_json::to_value(discovery).unwrap_or(serde_json::Value::Null)
                        });
                        object.entry("semantic_readiness").or_insert_with(|| {
                            serde_json::to_value(semantic).unwrap_or(serde_json::Value::Null)
                        });
                    }
                    Err(error) if airk_kernel::retrieval_retry::read_retry_hint(&error).is_some() => {
                        readiness_error = Some(error);
                    }
                    _ => {
                        object.insert(
                            "discovery_observation".into(),
                            "unavailable_after_read".into(),
                        );
                    }
                }
            }
        }
    }
    if let Some(object) = logical.as_mut().and_then(serde_json::Value::as_object_mut)
        && let Some(id) = span.request_id()
    {
        object.insert("request_id".into(), id.into());
    }
    // Parse follows its plan's validated output allowance; other tools keep theirs.
    let reply_limit = response::reply_limit_for(operation, logical.as_ref());
    let scope_error = readiness_error.or_else(|| {
        logical.as_mut().and_then(|logical| {
            kernel
                .annotate_scoped_read_within(logical, reply_limit)
                .err()
        })
    });
    let mut display = if scope_error.is_none() {
        logical.clone()
    } else {
        None
    };
    let mut verification_failed = false;
    let mut verification_error = scope_error;
    if let Some(display) = display.as_mut() {
        airk_core::reply::ReplyDetail::annotate(display, reply_detail);
        response::fact_items(display, operation);
        if let Some(budget) = verification {
            // The receipt shares the tool's own reply ceiling (Answer /3:
            // 131,072), the frame its Union packed the reply into.
            match kernel.verify_returned_sources_within(display, budget, read_deadline, reply_limit)
            {
                Ok(receipt) => match airk_core::verification::attach_response(display, receipt) {
                    Ok(failed) => verification_failed = failed,
                    Err(error) => verification_error = Some(error),
                },
                Err(error) => verification_error = Some(error),
            }
        }
        verification_failed |= airk_core::verification::mark_required_failure(display);
        // A verification fence may refuse this otherwise materialized read.
        // Do not acknowledge observations that the client never receives.
        if verification_error.is_none() {
            match kernel.scoped_read_fingerprint() {
                Ok(_) => responses.display_for(display, mode, include_status, operation),
                Err(error) => verification_error = Some(error),
            }
        }
    }
    let mut rendered = match verification_error.map_or(result, Err) {
        Ok(value) => match display
            .as_ref()
            .map_or_else(|| serde_json::to_string(&value), serde_json::to_string)
        {
            Ok(text) => {
                if verification_failed {
                    CallToolResult::error(vec![ContentBlock::text(text)])
                } else {
                    CallToolResult::success(vec![ContentBlock::text(text)])
                }
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        },
        Err(error) => response::scoped_error_result(kernel, &error),
    };
    let mut bytes = serde_json::to_vec(&rendered).map_or(0, |v| v.len());
    if kernel.scope_observation().is_some() && bytes.saturating_add(2048) > reply_limit {
        rendered = response::scoped_error_result(
            kernel,
            &airk_core::scoped_reads::ScopeReplyBudget {
                required_bytes: bytes.saturating_add(2048),
                maximum_bytes: reply_limit,
            }
            .into(),
        );
        bytes = serde_json::to_vec(&rendered).map_or(0, |value| value.len());
    }
    if record_usage {
        kernel.finish_usage(
            span,
            logical.as_ref(),
            bytes,
            rendered.is_error.unwrap_or(false),
            ResponseRepresentation::McpToolResultJson,
        );
    }
    match kernel.scoped_read_fingerprint() {
        Ok(_) => rendered,
        Err(error) => response::scoped_error_result(kernel, &error),
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AirkServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "airk",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(response::GUIDANCE)
    }
}

pub async fn serve(kernel: Kernel) -> anyhow::Result<()> {
    let server = AirkServer::new(kernel);
    let admission = server.admission.clone();
    let close = admission.clone();
    let result = remote::serve_bounded_io(
        server,
        tokio::io::stdin(),
        tokio::io::stdout(),
        std::sync::Arc::new(move || close.close()),
    )
    .await;
    admission.shutdown().await;
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terse_envelope_keeps_identity_citations_and_omission_state() {
        let mut response = serde_json::json!({
            "publication": {"token":"exact-token", "generation":7, "revision":"digest"},
            "generation": 7,
            "collection": "project-a",
            "hits": [{"citation":{"path":"src/a.cs","content_hash":"hash"}}],
            "truncated": true,
            "freshness": {"live_source_verified":false},
            "discovery_readiness": {"state":"ready"},
            "semantic_readiness": {"state":"partial"},
            "indexing": {"level":"agent_lean", "note":"long explanation"}
        });
        response::terse_envelope(response.as_object_mut().unwrap());
        assert_eq!(response["publication"]["token"], "exact-token");
        assert_eq!(response["hits"][0]["citation"]["path"], "src/a.cs");
        assert_eq!(response["truncated"], true);
        assert_eq!(response["semantic_readiness"]["state"], "partial");
        assert_eq!(response["indexing"]["level"], "agent_lean");
        assert_eq!(response["freshness"]["live_source_verified"], false);
        assert!(response.get("freshness").is_some());
    }

    #[tokio::test]
    async fn search_snippets_spend_budget_after_focusing_and_full_chunks_remains_available()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let text = format!("{} ConfigureAuthentication()\n", "unrelated ".repeat(700));
        for i in 0..10 {
            std::fs::write(root.path().join(format!("file{i}.txt")), &text)?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let server = AirkServer::new(kernel);
        for detail in [None, Some("lean"), Some("rich")] {
            for full_chunks in [None, Some(false), Some(true)] {
                let full = full_chunks.unwrap_or(false);
                let mut value = serde_json::json!({ "query": "ConfigureAuthentication", "mode": "lexical",
                    "limit": if full { 2 } else { 10 }, "max_bytes": if full { 16384 } else { 4096 }, "full_chunks":full_chunks });
                if let Some(detail) = detail {
                    value["reply_detail"] = json!(detail);
                }
                let args = serde_json::from_value(value)?;
                let result = server.search(Parameters(args)).await;
                assert!(!result.is_error.unwrap_or(false), "{result:?}");
                let body: serde_json::Value =
                    serde_json::from_str(&result.content[0].as_text().unwrap().text)?;
                let hits = body["hits"].as_array().unwrap();
                assert_eq!(
                    hits.len(),
                    if full { 2 } else { 10 },
                    "{detail:?} {full_chunks:?}"
                );
                for hit in hits {
                    let excerpt = hit["text"].as_str().unwrap();
                    assert!(excerpt.contains("ConfigureAuthentication"));
                    if full {
                        assert_eq!(excerpt, text);
                    } else {
                        assert!(excerpt.len() <= 768);
                    }
                    assert_eq!(hit["citation"]["start_line"], 1);
                }
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn agent_lean_defaults_to_useful_compact_facts_and_allows_rich_reads()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("auth.rs"),
            "fn authenticate(name: &str) -> bool { !name.is_empty() }\n",
        )?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let generation = kernel.status()?.generation;
        let server = AirkServer::new(kernel.clone());
        let capabilities = server
            .capabilities(Parameters(CapabilitiesArgs {
                response: Default::default(),
                adapter: None,
                rich: false,
            }))
            .await;
        let outer = serde_json::to_value(capabilities)?;
        let body: serde_json::Value =
            serde_json::from_str(outer["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(body["index_detail"]["new_collection_default"], "agent_lean");
        for compact in [None, Some(false)] {
            let args: CodeFactArgs = serde_json::from_value(
                serde_json::json!({"name":"authenticate","compact":compact,"include_status":true}),
            )?;
            let result = server.code_facts(Parameters(args)).await;
            assert!(!result.is_error.unwrap_or(false));
            let outer = serde_json::to_value(result)?;
            let body: serde_json::Value =
                serde_json::from_str(outer["content"][0]["text"].as_str().unwrap())?;
            if compact.is_none() {
                assert_eq!(body["items"][0]["qualified_name"], "authenticate");
                assert!(
                    body["items"][0]["signature"]
                        .as_str()
                        .unwrap()
                        .contains("-> bool")
                );
                assert_eq!(body["items"][0]["basis"], "syntax");
            } else {
                assert_eq!(body["items"][0]["name"], "authenticate");
            }
            assert_eq!(body["freshness"]["live_source_verified"], false);
        }
        assert_eq!(kernel.status()?.generation, generation);
        Ok(())
    }

    #[test]
    fn continuity_selectors_are_exact_typed_and_exclusive() {
        let key = "01".repeat(16);
        assert!(matches!(
            continuity_selector(Some(&key), true, false).unwrap(),
            Some(airk_core::continuity::ContinuityKey::Symbol(_))
        ));
        assert!(matches!(
            continuity_selector(Some(&key), false, false).unwrap(),
            Some(airk_core::continuity::ContinuityKey::Document(_))
        ));
        assert!(continuity_selector(Some(&key), true, true).is_err());
        assert!(continuity_selector(Some("short"), false, false).is_err());
        let code: CodeFactArgs =
            serde_json::from_value(serde_json::json!({"symbol_key":key})).unwrap();
        assert!(code.symbol_key.is_some());
        assert!(
            serde_json::from_value::<CodeFactArgs>(serde_json::json!({"document_key":key}))
                .is_err()
        );
        let schema = serde_json::to_value(schemars::schema_for!(DocumentFactArgs)).unwrap();
        assert!(schema["properties"].get("document_key").is_some());
    }

    #[test]
    fn code_fact_ranges_and_minimal_rows_are_opt_in_and_refuse_conflicts() {
        use airk_core::extraction::IndexDetail;
        let args =
            |value: serde_json::Value| -> CodeFactArgs { serde_json::from_value(value).unwrap() };
        let lean = || Ok(Some(IndexDetail::AgentLean));
        let detailed = || Ok(Some(IndexDetail::Detailed));
        // Omitted controls keep the existing compact default per index detail.
        assert_eq!(
            code_fact_projection(&args(serde_json::json!({})), lean).unwrap(),
            (true, false)
        );
        assert_eq!(
            code_fact_projection(&args(serde_json::json!({})), detailed).unwrap(),
            (false, false)
        );
        for value in [
            serde_json::json!({"minimal":true}),
            serde_json::json!({"minimal":true,"compact":true}),
            serde_json::json!({"minimal":true,"reply_detail":"lean"}),
        ] {
            assert_eq!(
                code_fact_projection(&args(value), detailed).unwrap(),
                (false, true)
            );
        }
        let key = "01".repeat(16);
        for (value, field) in [
            (
                serde_json::json!({"minimal":true,"compact":false}),
                "minimal",
            ),
            (
                serde_json::json!({"minimal":true,"reply_detail":"rich"}),
                "minimal",
            ),
            (
                serde_json::json!({"symbol_key":key,"start_line":1}),
                "start_line",
            ),
            (
                serde_json::json!({"symbol_key":key,"end_line":1}),
                "end_line",
            ),
            (
                serde_json::json!({"symbol_key":key,"minimal":true}),
                "minimal",
            ),
        ] {
            let error = code_fact_projection(&args(value), lean).unwrap_err();
            let typed = error
                .downcast_ref::<airk_core::read_arguments::ReadArgumentError>()
                .unwrap();
            assert_eq!(
                (typed.code, typed.field.as_str()),
                ("invalid_arguments", field)
            );
        }
        let schema = serde_json::to_value(schemars::schema_for!(CodeFactArgs)).unwrap();
        for field in ["start_line", "end_line", "minimal"] {
            assert!(schema["properties"].get(field).is_some(), "{field}");
            assert!(
                !schema["required"]
                    .as_array()
                    .is_some_and(|required| required.contains(&field.into())),
                "{field}"
            );
        }
    }

    #[test]
    fn changes_and_conditional_schemas_preserve_scope_and_bounds() {
        let args: ChangesArgs = serde_json::from_value(
            serde_json::json!({"selectors":[{"kind":"path","path":"one.rs"}]}),
        )
        .unwrap();
        let request = args.request().unwrap();
        request.validate().unwrap();
        assert_eq!(request.limit, 10);
        assert_eq!(request.max_bytes, 8192);
        assert!(!request.include_symbols && !request.include_documents);
        let mut excessive = request.clone();
        excessive.selectors = vec![request.selectors[0].clone(); 65];
        assert!(excessive.validate().is_err());
        for value in [
            serde_json::json!({"selectors":[],"collection":"other"}),
            serde_json::json!({"selectors":[{"kind":"path","path":"one.rs","unexpected":true}]}),
        ] {
            assert!(serde_json::from_value::<ChangesArgs>(value).is_err());
        }
        let args: ReadArgs = serde_json::from_value(
            serde_json::json!({"path":"one.rs","start_line":1,"end_line":2}),
        )
        .unwrap();
        assert!(!args.evidence.options().unwrap().follow_moves);
        assert!(serde_json::from_value::<ReadArgs>(serde_json::json!({"path":"one.rs","start_line":1,"end_line":2,"collection":"other"})).is_err());
        let args: PathArgs =
            serde_json::from_value(serde_json::json!({"path":"one.rs","if_evidence":"bad-digest"}))
                .unwrap();
        assert!(args.evidence.options().is_err());
        let schema = serde_json::to_value(schemars::schema_for!(ReadArgs)).unwrap();
        assert!(schema["properties"].get("if_evidence").is_some());
        assert!(schema["properties"].get("follow_moves").is_some());
    }

    #[test]
    fn search_schema_preserves_collection_default_and_bounds_modes() {
        let default: SearchArgs =
            serde_json::from_value(serde_json::json!({"query":"fixture"})).unwrap();
        assert!(default.mode.is_none());
        assert!(!default.semantic);
        assert_eq!(default.limit, None);
        let hybrid: SearchArgs =
            serde_json::from_value(serde_json::json!({"query":"fixture", "mode":"hybrid"}))
                .unwrap();
        assert!(matches!(hybrid.mode, Some(SearchMode::Hybrid)));
        for limit in [1, 10, 100] {
            let args: SearchArgs =
                serde_json::from_value(serde_json::json!({"query":"fixture", "limit":limit}))
                    .unwrap();
            assert_eq!(args.limit, Some(limit));
        }
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!("10"),
            serde_json::json!(1.5),
        ] {
            assert!(
                serde_json::from_value::<SearchArgs>(
                    serde_json::json!({"query":"fixture", "limit":invalid})
                )
                .is_err()
            );
        }
        let schema = serde_json::to_value(schemars::schema_for!(SearchArgs)).unwrap();
        let limit = &schema["properties"]["limit"];
        assert_eq!(limit["type"], "integer");
        assert_eq!(limit["minimum"], 1);
        assert_eq!(limit["maximum"], 100);
        assert!(
            !schema["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("limit"))
        );
        assert!(
            serde_json::from_value::<SearchArgs>(
                serde_json::json!({"query":"fixture", "mode":"remote"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<SearchArgs>(
                serde_json::json!({"query":"fixture", "collection":"other"})
            )
            .is_err()
        );
    }
}
