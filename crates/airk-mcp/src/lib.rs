// SPDX-License-Identifier: Apache-2.0
//! MCP owns schemas and transport only; every operation goes through the kernel.
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
use airk_kernel::{FactQuery, Kernel};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

mod execution;
pub mod response;
use response::ResponseArgs;
pub mod local;
pub mod remote;

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
    /// Definition name for symbols (exact); name substring for imports/document facts. Omit to list.
    pub name: Option<String>,
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
            path_prefix: self.path_prefix,
            name: self.name,
            language: self.language,
            kind: kind.map(str::to_owned),
            exact_name,
            limit: self.limit,
            max_bytes: self.max_bytes,
            path_exact: false,
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
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
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
    /// Exact category: definition, import, call, inherits, reference, route_registration, route_annotation, database_call, dependency_injection, test_registration, test_declaration or exception_handler.
    pub kind: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub path_prefix: String,
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
        "a continuity key cannot be combined with name, kind, language or path filters"
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
    kernel: Kernel,
    tool_router: ToolRouter<Self>,
    client: ClientFamily,
    admission: execution::Admission,
    responses: std::sync::Arc<response::Session>,
    response_mode: response::ResponseMode,
}

#[tool_router]
impl AirkServer {
    pub fn new(kernel: Kernel) -> Self {
        let admission = execution::Admission::for_kernel(&kernel);
        Self {
            kernel,
            tool_router: Self::tool_router(),
            client: ClientFamily::Unknown,
            admission,
            responses: Default::default(),
            response_mode: response::ResponseMode::from_environment(),
        }
    }

    #[tool(
        name = "airk_code_facts",
        description = "Retrieve compact code evidence by exact name (including terminal native call method names), qualified name, symbol ID, or a durable symbol_key exclusive of textual filters. Includes scopes, signatures and written types, call/framework candidates, and optional hash-bound SCIP producer assertions. Check code.basis/coverage; syntax does not resolve targets or establish runtime behavior. Use symbol IDs with airk_relationships. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
                    || args.kind.is_some()
                    || args.language.is_some()
                    || !args.path_prefix.is_empty()
                    || args.cursor.is_some(),
            )? {
                return kernel.continuity_facts(key, args.limit, args.max_bytes);
            }
            kernel.code_facts_page(
                &FactQuery {
                    path_prefix: args.path_prefix,
                    name: args.name,
                    kind: args.kind,
                    language: args.language,
                    exact_name: true,
                    limit: args.limit,
                    max_bytes: args.max_bytes,
                    path_exact: false,
                    document_only: false,
                },
                &airk_core::discovery::DiscoveryOptions {
                    minimal: false,
                    cursor: args.cursor,
                    compact,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_relationships",
        description = "Investigate available structural links and candidate impact through incoming/outgoing edges, scoped paths and 1..3 hops. Agent Lean and Rich Structure retain supported call occurrences, but written syntax targets remain unresolved candidates; SCIP assertions are not calls. Check coverage; absent edges do not prove no impact. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
        description = "Find code or documents. Use path_prefix when the question concerns one project or component; a relevant document does not prove application behavior. Omitted mode uses hybrid, or lexical after vector opt-out. Lexical uses AND terms; vector/hybrid need an indexed local model. semantic=true aliases vector. Results default to focused snippets with exact citations; full_chunks requests complete indexed chunks. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
            kernel.search_response(&query, mode, (!args.full_chunks).then_some(768))
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
        description = "Check collection index readiness, active generation, file/chunk counts and extractor version. Counts describe the snapshot, not live workspace coverage or freshness. Request indexing through the CLI/extension when sources change. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn status(&self, Parameters(args): Parameters<StatusArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::Status, args.response, move |kernel| {
            kernel.status()
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
        description = "Understand a file's structure before reading broad text: bounded language-specific definitions, imports or structured document facts with locations. Takes one exact indexed file path. Check airk_file_info for partial syntax; no runtime or type resolution. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
        description = "Locate function/class/type and other supported code definitions by exact name, language/profile or literal path prefix. Omit name to list grammar-derived definitions. Use airk_search for keyword discovery. Does not resolve dynamic or cross-file names. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
            kernel.facts_page(&args.query(Some("definition"), true, false), &options)
        })
        .await
    }

    #[tool(
        name = "airk_imports",
        description = "Inspect static code dependencies through grammar-derived import/use/include declarations. Filter by language/profile, literal path prefix and optional name substring. Returns syntax evidence, not a resolved dependency graph or transitive/dynamic dependencies. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
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
            kernel.facts_page(&args.query(Some("import"), false, false), &options)
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
                        || !args.path_prefix.is_empty()
                        || args.cursor.is_some(),
                )? {
                    return kernel.continuity_facts(key, args.limit, args.max_bytes);
                }
                kernel.facts_page(
                    &FactQuery {
                        path_prefix: args.path_prefix,
                        name: args.name,
                        language: args.language,
                        kind: args.kind,
                        exact_name: false,
                        limit: args.limit,
                        max_bytes: args.max_bytes,
                        path_exact: false,
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
        description = "Locate exact-case whole-identifier textual occurrences using indexed lexical candidates verified against retained source, including CamelCase. Includes definitions, strings and comments, never resolved semantic references. Check truncation before impact analysis and verify live source. Use response_mode=full for provenance/coverage audits, readiness diagnosis or unexpected results.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn references(&self, Parameters(args): Parameters<ReferenceArgs>) -> CallToolResult {
        self.execute_response(UsageOperation::References, args.response, move |kernel| {
            kernel.references(&args.name, &args.path_prefix, args.limit, args.max_bytes)
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

fn render<T: Serialize>(
    kernel: &Kernel,
    span: airk_kernel::usage::RequestSpan,
    result: anyhow::Result<T>,
    responses: &response::Session,
    mode: response::ResponseMode,
    include_status: bool,
    operation: UsageOperation,
) -> CallToolResult {
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
    kernel.finish_usage(
        span,
        logical.as_ref(),
        bytes,
        rendered.is_error.unwrap_or(false),
        ResponseRepresentation::McpToolResultJson,
    );
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
    let service = server.serve(rmcp::transport::stdio()).await?;
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
