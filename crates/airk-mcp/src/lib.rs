// SPDX-License-Identifier: Apache-2.0
//! MCP owns schemas and transport only; every operation goes through the kernel.
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
use airk_kernel::{FactQuery, Kernel};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

mod execution;
mod lifecycle;
pub mod response;
mod workbench;
use response::ResponseArgs;
pub mod explorer_offline;
pub mod help;
pub mod local;
pub mod query_capacity;
pub mod remote;
pub mod unified;

#[cfg(test)]
#[path = "optimization_tests.rs"]
mod optimization_tests;

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
    #[serde(flatten, skip_serializing)]
    pub response: ResponseArgs,
    pub schema: String,
    /// prepare (default) validates; execute collects read-only evidence.
    pub mode: Option<String>,
    pub text: Option<String>,
    pub recipe: Option<serde_json::Value>,
    pub plan: Option<serde_json::Value>,
    #[serde(default)]
    pub path_prefix: String,
    pub budgets: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
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
    /// Exact current token; a superseded token expires without historical fallback.
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
    pub limit: usize,
    #[serde(default = "default_budget")]
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
    pub response: ResponseArgs,
    /// Default hybrid retrieval; explicit lexical uses AND terms.
    pub query: String,
    /// Omit for the saved collection policy; pending vectors are disclosed in the response.
    pub mode: Option<SearchMode>,
    /// Compatibility alias for vector-only retrieval.
    #[serde(default)]
    pub semantic: bool,
    /// Optional literal collection-relative path prefix, not a glob.
    pub path_prefix: Option<String>,
    /// Exact indexed extraction-language/profile label, as reported by airk_file_info.
    pub language: Option<String>,
    /// Indexed evidence category, based on extraction metadata rather than filename suffix.
    pub evidence_kind: Option<EvidenceKind>,
    /// Optional literal collection-relative prefix to exclude; does not change source authorization.
    pub exclude_path_prefix: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
    /// Return complete indexed chunks instead of focused snippets.
    #[serde(default)]
    pub full_chunks: bool,
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
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Literal collection-relative path prefix, not a glob. Empty lists from the collection root.
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}

fn default_limit() -> usize {
    10
}
fn default_budget() -> usize {
    8192
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
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
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactArgs {
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
    /// Optional indexed language/profile label; obtain the label from airk_file_info.
    pub language: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
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
    /// Optional indexed language/profile label from airk_file_info.
    pub language: Option<String>,
    /// Exact category: requirement, date, architecture, design, feature, decision, acceptance, section, table, table_row, table_cell, key or element.
    pub kind: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceArgs {
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Exact-case whole identifier to locate textually, not a regex or semantic symbol ID.
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
    pub limit: usize,
    #[serde(default = "default_budget")]
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
    pub response: ResponseArgs,
    #[serde(flatten)]
    pub evidence: EvidenceArgs,
    /// Collection-relative indexed file path returned with evidence.
    pub path: String,
    /// 1-based indexed line from AIRK evidence; extracted document lines are not page/slide numbers.
    pub line: usize,
    #[serde(default = "default_radius")]
    pub before: usize,
    #[serde(default = "default_radius")]
    pub after: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}
fn default_radius() -> usize {
    5
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeFactArgs {
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
    /// Exact category: definition, import, call, inherits, reference, route_registration, route_annotation, database_call, dependency_injection, test_registration, test_declaration or exception_handler.
    pub kind: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub path_prefix: String,
    /// Exact indexed file path; takes precedence over path_prefix.
    pub path: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipArgs {
    #[serde(flatten)]
    pub response: ResponseArgs,
    /// Use code.symbol_id from code facts/symbols, a file:path owner, or an exact written syntax target (e.g. db.query). Syntax targets are not resolved symbol IDs.
    pub symbol: String,
    /// incoming for callers/implementation candidates; outgoing for dependencies/containment. Defaults incoming.
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Exact edge kind: call, contains, import, inherits, reference, implements, type_definition, definition or a framework category.
    pub kind: Option<String>,
    #[serde(default)]
    pub path_prefix: String,
    /// 1..3 graph hops; candidate impact is incomplete and references are not calls.
    #[serde(default = "default_hops")]
    pub hops: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
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
}

#[tool_router]
impl AirkServer {
    pub fn new(kernel: Kernel) -> Self {
        let admission = execution::Admission::for_kernel(&kernel);
        Self {
            work: Default::default(),
            runtime: Default::default(),
            kernel,
            tool_router: Self::tool_router(),
            client: ClientFamily::Unknown,
            admission,
            responses: Default::default(),
            response_mode: response::ResponseMode::from_environment(),
            plan_read: false,
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
        help::reply(args)
    }

    #[tool(
        name = "airk_parse",
        description = "Prepare or execute a bounded read-only AIRK evidence plan. Supply schema airk.parse.request/1, mode prepare or execute, and exactly one of text, recipe, plan. Explicit plans contain steps {id,tool,args,bind}; public operation names only. Bindings use earlier step IDs and named selectors. At most 6 steps, 12 reads, a 30-second budget checked between reads, 512 KiB. Learned text in this review build returns an inert proposed plan with needs_input; review its arguments and resubmit the explicit plan to execute. Other uncertainty, partial coverage and failures stay explicit. No writes, commands, management, recursive plans or agent reasoning. Works without Explorer. Use response_mode=full for provenance/coverage audits; raw inner evidence retains its full kernel representation.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn parse(&self, Parameters(args): Parameters<ParseArgs>) -> CallToolResult {
        let observations = serde_json::json!({"runtime":self.runtime.read().ok().and_then(|v|v.clone()),"work":self.work.read().ok().and_then(|v|v.clone())});
        let mut reader = self.clone();
        reader.plan_read = true;
        let handle = tokio::runtime::Handle::current();
        self.execute_response(UsageOperation::Parse, args.response, move |kernel| {
            reader.kernel = kernel.clone();
            let mut value = serde_json::to_value(args)?;
            if value["mode"].is_null() {
                value["mode"] = serde_json::json!("prepare");
            }
            kernel.evidence_plan_with_reader(
                serde_json::from_value(value)?,
                &observations,
                Some(&|tool, args, revision| {
                    handle.block_on(reader.workbench_read(tool, args, revision))
                }),
            )
        })
        .await
    }

    #[tool(
        name = "airk_code_facts",
        description = "Retrieve code facts when definition navigation needs signatures, scopes, written types, call/framework candidates or symbol IDs for airk_relationships. Exact name is default, including qualified names and terminal native call method names; name_match=suffix/contains explicitly broadens case-sensitive simple/qualified names. A durable symbol_key is exclusive of filters, matching and count options. Compact discovery is the default under Agent Lean; expand selected candidates with compact=false. Cursors bind the exact query/publication; include_count reports bounded precision, not a guessed total. Check basis/coverage; native syntax and optional source-bound producer assertions do not prove resolved calls or runtime behavior. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn code_facts(&self, Parameters(args): Parameters<CodeFactArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::CodeFacts, args.response, move |kernel| {
            let compact = args.compact.unwrap_or(
                kernel.applied_index_detail()?
                    == Some(airk_core::extraction::IndexDetail::AgentLean),
            );
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
                    minimal: false,
                    cursor: args.cursor,
                    compact,
                },
                &airk_core::discovery::FactDiscoveryOptions {
                    name_match: args.name_match.map(Into::into),
                    include_count: args.include_count,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_relationships",
        description = "Investigate bounded incoming/outgoing structural candidates with path scope and 1..3 hops. Written targets such as Type::new or obj.method identify textual caller candidates; a struct symbol ID alone does not prove a complete caller set. Verified type/method suggestions, when available, are navigation candidates rather than resolved associations. Agent Lean/Rich Structure retain supported calls; SCIP assertions are not calls. Check coverage and truncation: absent edges never prove no impact. Use response_mode=full for provenance/coverage audits.",
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
        self.execute_response(
            UsageOperation::Relationships,
            args.response,
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
        description = "Find concepts, code or documents; prose/comments and source can rank together. For a named type/function, prefer airk_symbols first, scoped airk_references for textual uses, then bounded airk_read_file; search supplements that investigation. Scope by literal path_prefix, exact indexed language, metadata-based evidence_kind or exclude_path_prefix; documents alone do not prove application behavior. Omitted mode retains saved hybrid/vector policy; explicit lexical uses AND terms. Vector/hybrid need the indexed local model. semantic=true aliases vector. Bounded native-definition pointers may accompany identifiers without changing ranking. Focused cited snippets are default; full_chunks requests whole indexed chunks. Check readiness/truncation and use response_mode=full for provenance audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Search, args.response, move |kernel| {
            anyhow::ensure!(
                !args.semantic || args.mode.is_none(),
                "choose mode or semantic, not both"
            );
            let query = SearchQuery {
                text: args.query,
                path_prefix: args.path_prefix,
                limit: args.limit,
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
            kernel.search_response_with_navigation(
                &query,
                mode,
                (!args.full_chunks).then_some(768),
                &airk_core::SearchFilters {
                    language: args.language,
                    evidence_kind: args.evidence_kind.map(Into::into),
                    exclude_path_prefix: args.exclude_path_prefix,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_read_file",
        description = "Inspect a specific code or document excerpt after locating it. Read an inclusive 1-based indexed line range from an inventory path; this is the last successful snapshot, not a live filesystem read. For PDF/DOCX/PPTX, use extracted lines within one page/block/slide. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn read_file(&self, Parameters(args): Parameters<ReadArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::ReadFile, args.response, move |kernel| {
            kernel.read_file_conditional(
                &args.path,
                args.start_line,
                args.end_line,
                args.max_bytes,
                &args.evidence.options()?,
            )
        })
        .await
    }

    #[tool(
        name = "airk_list_files",
        description = "Discover indexed files before choosing paths for outlines or reads. Filter by literal relative-path prefix, not glob; count and bytes are bounded. Lists the snapshot inventory, not all live workspace files or indexing coverage. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_files(&self, Parameters(args): Parameters<ListArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::ListFiles, args.response, move |kernel| {
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
        })
        .await
    }

    #[tool(
        name = "airk_status",
        description = "Quick AIRK version, basic health and project-index check: generation, counts, freshness and semantic coverage. The MCP frontend adds configured query_concurrency from admission. Completed same-authority observations superseded by scalar invalidation return successfully with status_observation.superseded and their original publication/time. Successful native snapshots may be cached for up to five seconds with explicit observation time and cache age. Reads scalar publication metadata without imported fact payloads, runtime/work probes or editor processes in every response mode. Use airk_diagnostics for process/version identities, detailed job state and diagnostic observations. Health is not progress; saved counts do not verify live source. response_mode=full retains the same lightweight scope. MCP does not manage jobs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn status(&self, Parameters(args): Parameters<StatusArgs>) -> CallToolResult {
        self.execute_response_policy(UsageOperation::Status,args.response,false,move|kernel|{
            let mut status = kernel.status_summary().or_else(|error| {
                if airk_kernel::retrieval_retry::read_retry_hint(&error).is_some() {
                    return Err(error);
                }
                Ok(serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","index_state":"unavailable","publication":null,"files":null,"chunks":null,"freshness":kernel.changes_freshness(),"index_error":{"code":"index_status_unavailable","message":"Published index metadata could not be read. Use diagnostics for details."}}))
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
        self.execute_response(UsageOperation::Status, args.response, move |kernel| {
            let mut status = match kernel.status() {
                Ok(status) => {
                    let mut status = serde_json::to_value(status)?;
                    status["index_state"] = "available".into();
                    status
                }
                Err(error) if airk_kernel::retrieval_retry::read_retry_hint(&error).is_some() => {
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
            Ok(status)
        })
        .await
    }

    #[tool(
        name = "airk_file_info",
        description = "Check a file's indexed language/profile, extraction method, syntax quality, hash and line basis before interpreting evidence. Distinguishes source lines from extracted document coordinates; does not compare the live file for freshness. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn file_info(&self, Parameters(args): Parameters<PathArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::FileInfo, args.response, move |kernel| {
            kernel.file_info_conditional(&args.path, &args.evidence.options()?)
        })
        .await
    }

    #[tool(
        name = "airk_outline",
        description = "Read one indexed file's structure in file order: imports, definitions, call candidates and document facts with locations. This is read-only source evidence; suggest_view proposes an Explorer screen instead. Takes one exact path and pages with cursor; a first page may not reach a known definition, so prefer scoped airk_symbols/airk_code_facts for that. Compact minimal navigation is default; compact=false expands facts and IDs. Bounded or empty output is not a whole-file or complete-language claim. Use response_mode=full for provenance audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn outline(&self, Parameters(args): Parameters<OutlineArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Outline, args.response, move |kernel| {
            kernel.outline_conditional_page(
                &args.path,
                args.limit,
                args.max_bytes,
                &args.evidence.options()?,
                &airk_core::discovery::DiscoveryOptions {
                    minimal: args.compact.unwrap_or(true),
                    cursor: args.cursor,
                    compact: false,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_symbols",
        description = "Locate supported written code definitions before broad text search. Exact name is default: Session does not match OwnerSession. Explicit name_match=suffix/contains matches simple/qualified names case-sensitively; omit name to list within indexed language/profile or literal path_prefix. Keep compact discovery for candidates, then request compact=false on a selected scope for rich IDs/details. Consume cursor pages; include_count reports bounded count precision. Does not resolve dynamic/cross-file names. Use airk_code_facts for additional fact categories and response_mode=full for provenance audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn symbols(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Symbols, args.response, move |kernel| {
            let options = airk_core::discovery::DiscoveryOptions {
                minimal: args.compact.unwrap_or(true),
                cursor: args.cursor.clone(),
                compact: false,
            };
            let matching = airk_core::discovery::FactDiscoveryOptions {
                name_match: args.name_match.map(Into::into),
                include_count: args.include_count,
            };
            kernel.facts_page_with_options(
                &args.query(Some("definition"), true, false),
                &options,
                &matching,
            )
        })
        .await
    }

    #[tool(
        name = "airk_imports",
        description = "Inspect written import/use/include declarations by indexed language/profile, literal path prefix and optional name. Omitted name_match keeps the existing case-insensitive substring behavior; explicit exact/suffix/contains is case-sensitive. Use cursor continuation and optional include_count for bounded count precision. These are syntax candidates, not a resolved, transitive or dynamic dependency graph. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn imports(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Imports, args.response, move |kernel| {
            let options = airk_core::discovery::DiscoveryOptions {
                minimal: args.compact.unwrap_or(true),
                cursor: args.cursor.clone(),
                compact: false,
            };
            let matching = airk_core::discovery::FactDiscoveryOptions {
                name_match: args.name_match.map(Into::into),
                include_count: args.include_count,
            };
            kernel.facts_page_with_options(
                &args.query(Some("import"), false, false),
                &options,
                &matching,
            )
        })
        .await
    }

    #[tool(
        name = "airk_document_facts",
        description = "Find repository requirements, dates, features, architecture/design/decision language, headings, configuration keys and asset facts. Select a durable document_key alone, or filter by exact kind, language/profile, literal path prefix or statement/name substring. Prose cues include heading context, written bounds/units and modality; they are evidence candidates, not inferred obligations. Documents retain original locations; explicitly published local OCR evidence carries its own provenance and recognition limitations. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
        self.execute_response(
            UsageOperation::DocumentFacts,
            args.response,
            move |kernel| {
                let compact = args.compact.unwrap_or(
                    kernel.applied_index_detail()?
                        == Some(airk_core::extraction::IndexDetail::AgentLean),
                );
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
        description = "Locate exact-case whole-identifier matching lines in retained indexed text, including definitions, strings and comments. Markdown, HTML, scripts and source can all match; scope by path_prefix, exact indexed language or metadata-based evidence_kind for impact work. These are textual candidates, never resolved references. Omit new paging options for legacy index ordering; explicit order=index/path/code_first, filters, cursor, include_files or include_count select bounded pageable scanning. Page file summaries/counts disclose their scope and matching-line unit; incomplete scans never imply an exact corpus total. Consume continuation and check truncation/coverage before concluding no impact; verify live source before edits. Use response_mode=full for provenance/coverage audits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn references(&self, Parameters(args): Parameters<ReferenceArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::References, args.response, move |kernel| {
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
        })
        .await
    }

    #[tool(
        name = "airk_context",
        description = "Expand a search/fact hit with bounded neighboring indexed lines around its line coordinate. PDF/DOCX/PPTX lines are extracted-text coordinates; keep the request within one original page/block/slide. Evidence reflects the last successful index. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn context(&self, Parameters(args): Parameters<ContextArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Context, args.response, move |kernel| {
            kernel.context_conditional(
                &args.path,
                args.line,
                args.before,
                args.after,
                args.max_bytes,
                &args.evidence.options()?,
            )
        })
        .await
    }

    #[tool(
        name = "airk_changes",
        description = "Inspect bounded observed indexed changes since an exact token and validate remembered evidence. Supply since or selectors. Check independent coverage and freshness; this does not verify live source. Consume every needed cursor page before advancing the baseline. index-current describes the index, not live files. Baselines retain bounded metadata across up to 64 publications/24 hours; pressure can advance explicit coverage floors. Moves are content-based candidates, not proof of filesystem intent. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn changes(&self, Parameters(args): Parameters<ChangesArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Changes, args.response, move |kernel| {
            kernel.changes(&args.request()?)
        })
        .await
    }

    #[tool(
        name = "airk_capabilities",
        description = "Check native formats, explicitly provisioned local model/OCR/code-provider adapters and their limits before planning exploration. Works before indexing. Reports available capabilities, not whether optional providers or this collection are ready. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
            "index_detail": {"new_collection_default":"agent_lean", "levels":["agent_lean","essential","balanced","detailed"], "labels":{"agent_lean":"Agent Lean","balanced":"Rich Structure","essential":"Search Only","detailed":"Detailed"}, "coverage":"Fact and relationship responses report their published indexing level and deliberate omissions. Configured changes apply only after a successful index. Optional provider imports preserve their source and producer binding."},
            "history": {"supported":false,"exact_reads":[],"optional_retention":false,"search":false,"live_fallback":false,"checkpoints":false,"reason":"Source reads select one current index; retained change metadata does not enable old source reads"},
            "changes": {"observed_indexed_changes":false,"remembered_evidence_validation":true,"expired_baseline":"baseline_unavailable","selectors":64,"selector_bytes":32768,"max_payload_bytes":65536,"exact_revision_tokens":true,"live_source_verified":false,"symbol_continuity":"adapter-specific conservative coverage","document_continuity":"adapter-specific conservative coverage"},
            "change_retention": {"generations":64,"age_seconds":86400,"max_records":10000,"max_bytes":16777216,"source_history":false,"validation_current":"index-current","coverage":"independent floors; whole publication groups expire under generation, age or budget pressure"},
            "conditional_reads": {"tools":["airk_file_info","airk_read_file","airk_outline","airk_context"],"validator":"representation-specific if_evidence","follow_moves_default":false,"move_basis":"unique compatible exact content; not filesystem intent"},
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
    mode: response::ResponseMode,
    include_status: bool,
    operation: UsageOperation,
) -> CallToolResult {
    let (span, record_usage) = usage;
    let mut logical = result
        .as_ref()
        .ok()
        .and_then(|value| serde_json::to_value(value).ok());
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
                match kernel.readiness_observation(publication.token) {
                    Ok(Some((discovery, semantic))) => {
                        object.entry("discovery_readiness").or_insert_with(|| {
                            serde_json::to_value(discovery).unwrap_or(serde_json::Value::Null)
                        });
                        object.entry("semantic_readiness").or_insert_with(|| {
                            serde_json::to_value(semantic).unwrap_or(serde_json::Value::Null)
                        });
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
    let mut display = logical.clone();
    if let Some(display) = display.as_mut() {
        response::fact_items(display, operation);
        responses.display(display, mode, include_status);
    }
    let rendered = match result {
        Ok(value) => match display
            .as_ref()
            .map_or_else(|| serde_json::to_string(&value), serde_json::to_string)
        {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        },
        Err(error) => response::error_result(&error),
    };
    let bytes = serde_json::to_vec(&rendered).map_or(0, |v| v.len());
    if record_usage {
        kernel.finish_usage(
            span,
            logical.as_ref(),
            bytes,
            rendered.is_error.unwrap_or(false),
            ResponseRepresentation::McpToolResultJson,
        );
    }
    rendered
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AirkServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
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
    let service = rmcp::service::serve_directly(
        lifecycle::StdioLifecycle::new(server),
        rmcp::transport::stdio(),
        None,
    );
    let result = service.waiting().await;
    admission.shutdown().await;
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for full in [false, true] {
            let args = serde_json::from_value(
                serde_json::json!({ "query": "ConfigureAuthentication", "mode": "lexical",
                "limit": if full { 2 } else { 10 }, "max_bytes": if full { 16384 } else { 4096 }, "full_chunks": full }),
            )?;
            let result = server.search(Parameters(args)).await;
            assert!(!result.is_error.unwrap_or(false), "{result:?}");
            let body: serde_json::Value =
                serde_json::from_str(&result.content[0].as_text().unwrap().text)?;
            let hits = body["hits"].as_array().unwrap();
            assert_eq!(hits.len(), if full { 2 } else { 10 });
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
        let hybrid: SearchArgs =
            serde_json::from_value(serde_json::json!({"query":"fixture", "mode":"hybrid"}))
                .unwrap();
        assert!(matches!(hybrid.mode, Some(SearchMode::Hybrid)));
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
