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
pub mod local;

#[derive(Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct EvidenceArgs {
    /// Exact retained revision token; unavailable history never falls back to current bytes.
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
#[serde(deny_unknown_fields)]
pub struct HistoryArgs {
    pub revision: Option<String>,
    pub cursor: Option<String>,
    #[serde(default = "history_limit")]
    pub limit: usize,
    #[serde(default = "history_bytes")]
    pub max_bytes: usize,
}
fn history_limit() -> usize {
    64
}
fn history_bytes() -> usize {
    65536
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
    /// Default hybrid retrieval; explicit lexical uses AND terms.
    pub query: String,
    /// Omit for the collection default: hybrid unless vectors are disabled in the panel.
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
    pub evidence: EvidenceArgs,
    /// Collection-relative indexed path returned by AIRK, never an absolute path.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineArgs {
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Return compact names, kinds, spans and durable keys; rich facts remain the default.
    #[serde(default)]
    pub compact: bool,
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
    /// Continuation returned by this tool; repeat the same filters and representation.
    pub cursor: Option<String>,
    /// Durable symbol key from indexed evidence; exclusive of name/kind/language/path filters.
    pub symbol_key: Option<String>,
    /// Exact written name, qualified name or symbol_id; omit to list.
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
}

#[tool_router]
impl AirkServer {
    pub fn new(kernel: Kernel) -> Self {
        Self {
            kernel,
            tool_router: Self::tool_router(),
            client: ClientFamily::Unknown,
            admission: execution::Admission::default(),
        }
    }

    #[tool(
        name = "airk_code_facts",
        description = "Retrieve compact code evidence by exact name, qualified name, symbol ID, or a durable symbol_key exclusive of textual filters. Includes scopes, signatures and written types, call/framework candidates, and optional hash-bound SCIP producer assertions. Check code.basis/coverage; syntax does not resolve targets or establish runtime behavior. Use symbol IDs with airk_relationships.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn code_facts(&self, Parameters(args): Parameters<CodeFactArgs>) -> CallToolResult {
        self.execute(UsageOperation::CodeFacts, move |kernel| {
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
                    cursor: args.cursor,
                    compact: false,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_relationships",
        description = "Investigate callers, implementations, dependencies or candidate impact through incoming/outgoing edges, scoped paths and 1..3 hops. Syntax call targets are exact written expressions and remain candidates; optional SCIP references/implements are producer assertions, never automatically calls. Use IDs from code facts. Results have provenance, coverage and count/byte/time limits; absence is not proof of no impact.",
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
        self.execute(UsageOperation::Relationships, move |kernel| {
            kernel.relationships(&RelationshipQuery {
                symbol: args.symbol,
                direction: args.direction,
                kind: args.kind,
                path_prefix: args.path_prefix,
                hops: args.hops,
                limit: args.limit,
                max_bytes: args.max_bytes,
            })
        })
        .await
    }

    #[tool(
        name = "airk_search",
        description = "Find code or documentation. Omitted mode uses hybrid (lexical plus DiskANN rank fusion), or lexical when the collection has opted out in the panel. Explicit lexical uses AND terms; vector uses local embeddings. Vector/hybrid require an indexed local model; missing assets fail explicitly, with no network inference. semantic=true is a compatibility alias for vector; omit mode with that alias. Returns bounded indexed evidence, paths, locations and hashes.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        self.execute(UsageOperation::Search, move |kernel| {
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
            match args.mode {
                Some(SearchMode::Lexical) => kernel.search(&query),
                Some(SearchMode::Vector) => kernel.semantic_search(&query),
                Some(SearchMode::Hybrid) => kernel.hybrid_search(&query),
                None if args.semantic => kernel.semantic_search(&query),
                None => kernel.default_search(&query),
            }
        })
        .await
    }

    #[tool(
        name = "airk_read_file",
        description = "Inspect a specific code or document excerpt after locating it. Read an inclusive 1-based indexed line range from an inventory path; this is the last successful snapshot, not a live filesystem read. For PDF/DOCX/PPTX, use extracted lines within one page/block/slide.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn read_file(&self, Parameters(args): Parameters<ReadArgs>) -> CallToolResult {
        self.execute(UsageOperation::ReadFile, move |kernel| {
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
        description = "Discover indexed files before choosing paths for outlines or reads. Filter by literal relative-path prefix, not glob; count and bytes are bounded. Lists the snapshot inventory, not all live workspace files or indexing coverage.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_files(&self, Parameters(args): Parameters<ListArgs>) -> CallToolResult {
        self.execute(UsageOperation::ListFiles, move |kernel| {
            kernel.list_files_page(
                &args.prefix,
                args.limit,
                args.max_bytes,
                &airk_core::discovery::DiscoveryOptions {
                    cursor: args.cursor,
                    compact: false,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_status",
        description = "Check collection index readiness, active generation, file/chunk counts and extractor version. Counts describe the snapshot, not live workspace coverage or freshness. Request indexing through the CLI/extension when sources change.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn status(&self) -> CallToolResult {
        self.execute(UsageOperation::Status, move |kernel| kernel.status())
            .await
    }

    #[tool(
        name = "airk_file_info",
        description = "Check a file's indexed language/profile, extraction method, syntax quality, hash and line basis before interpreting evidence. Distinguishes source lines from extracted document coordinates; does not compare the live file for freshness.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn file_info(&self, Parameters(args): Parameters<PathArgs>) -> CallToolResult {
        self.execute(UsageOperation::FileInfo, move |kernel| {
            kernel.file_info_conditional(&args.path, &args.evidence.options()?)
        })
        .await
    }

    #[tool(
        name = "airk_outline",
        description = "Understand a file's structure before reading broad text: bounded language-specific definitions, imports or structured document facts with locations. Takes one exact indexed file path. Check airk_file_info for partial syntax; no runtime or type resolution.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn outline(&self, Parameters(args): Parameters<OutlineArgs>) -> CallToolResult {
        self.execute(UsageOperation::Outline, move |kernel| {
            kernel.outline_conditional_page(
                &args.path,
                args.limit,
                args.max_bytes,
                &args.evidence.options()?,
                &airk_core::discovery::DiscoveryOptions {
                    cursor: args.cursor,
                    compact: args.compact,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_symbols",
        description = "Locate function/class/type and other supported code definitions by exact name, language/profile or literal path prefix. Omit name to list grammar-derived definitions. Use airk_search for keyword discovery. Does not resolve dynamic or cross-file names.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn symbols(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute(UsageOperation::Symbols, move |kernel| {
            let options = airk_core::discovery::DiscoveryOptions {
                cursor: args.cursor.clone(),
                compact: false,
            };
            kernel.facts_page(&args.query(Some("definition"), true, false), &options)
        })
        .await
    }

    #[tool(
        name = "airk_imports",
        description = "Inspect static code dependencies through grammar-derived import/use/include declarations. Filter by language/profile, literal path prefix and optional name substring. Returns syntax evidence, not a resolved dependency graph or transitive/dynamic dependencies.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn imports(&self, Parameters(args): Parameters<FactArgs>) -> CallToolResult {
        self.execute(UsageOperation::Imports, move |kernel| {
            let options = airk_core::discovery::DiscoveryOptions {
                cursor: args.cursor.clone(),
                compact: false,
            };
            kernel.facts_page(&args.query(Some("import"), false, false), &options)
        })
        .await
    }

    #[tool(
        name = "airk_document_facts",
        description = "Find repository requirements, dates, features, architecture/design/decision language, headings, configuration keys and asset facts. Select a durable document_key alone, or filter by exact kind, language/profile, literal path prefix or statement/name substring. Prose cues include heading context, written bounds/units and modality; they are evidence candidates, not inferred obligations. Documents retain original locations; explicitly published local OCR evidence carries its own provenance and recognition limitations.",
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
        self.execute(UsageOperation::DocumentFacts, move |kernel| {
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
                    cursor: args.cursor,
                    compact: false,
                },
            )
        })
        .await
    }

    #[tool(
        name = "airk_references",
        description = "Locate possible usages of a code identifier as bounded exact-case whole-identifier textual occurrences. Includes definitions, strings and comments; these are NOT resolved semantic references. Confirm refactoring impacts with the agent's language tools and current source.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn references(&self, Parameters(args): Parameters<ReferenceArgs>) -> CallToolResult {
        self.execute(UsageOperation::References, move |kernel| {
            kernel.references(&args.name, &args.path_prefix, args.limit, args.max_bytes)
        })
        .await
    }

    #[tool(
        name = "airk_context",
        description = "Expand a search/fact hit with bounded neighboring indexed lines around its line coordinate. PDF/DOCX/PPTX lines are extracted-text coordinates; keep the request within one original page/block/slide. Evidence reflects the last successful index.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn context(&self, Parameters(args): Parameters<ContextArgs>) -> CallToolResult {
        self.execute(UsageOperation::Context, move |kernel| {
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
        description = "Inspect bounded observed indexed changes since an exact token and validate remembered evidence. Supply since or selectors. Check independent coverage and freshness; this does not verify live source. Consume every needed cursor page before advancing the baseline. Moves are content-based candidates, not proof of filesystem intent.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn changes(&self, Parameters(args): Parameters<ChangesArgs>) -> CallToolResult {
        self.execute(UsageOperation::Changes, move |kernel| {
            kernel.changes(&args.request()?)
        })
        .await
    }

    #[tool(
        name = "airk_history",
        description = "List bounded retained publication receipts or verify availability of one exact revision. Listing verifies receipt identity; exact file/context/outline/info reads verify retained bytes. Historical graph/full-text/vector search is unsupported. No live or current byte fallback.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn history(&self, Parameters(args): Parameters<HistoryArgs>) -> CallToolResult {
        self.execute(UsageOperation::Status, move |kernel| {
            kernel.history_availability(&airk_kernel::HistoryAvailabilityRequest {
                revision: args.revision.as_deref().map(str::parse).transpose()?,
                cursor: args.cursor,
                limit: args.limit,
                max_bytes: args.max_bytes,
            })
        })
        .await
    }

    #[tool(
        name = "airk_capabilities",
        description = "Check native formats, explicitly provisioned local model/OCR/code-provider adapters and their limits before planning exploration. Works before indexing. Reports available capabilities, not whether optional providers or this collection are ready.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn capabilities(&self) -> CallToolResult {
        let admission = self.admission.status();
        self.execute(UsageOperation::Capabilities, move |_| Ok(serde_json::json!({
            "request_admission": admission,
            "discovery": {"cursor_tools":["airk_list_files","airk_outline","airk_symbols","airk_imports","airk_code_facts","airk_document_facts"], "binding":"exact publication, filters and representation", "compact_outline":true, "budget":"complete serialized records; framing is additional", "expired_cursor":"restart discovery"},
            "adapters": Kernel::extraction_adapters(),
            "index_detail": {"new_collection_default":"balanced", "levels":["essential","balanced","detailed"], "coverage":"Fact and relationship responses report their published indexing level and deliberate omissions. Configured changes apply only after a successful index. Optional provider imports and history retention are separate."},
            "history": {"exact_reads":["airk_file_info","airk_read_file","airk_outline","airk_context"],"availability":"airk_history","optional_retention":true,"search":false,"live_fallback":false,"mutations":"CLI/extension only"},
            "changes": {"observed_indexed_changes":true,"selectors":64,"selector_bytes":32768,"max_payload_bytes":65536,"exact_revision_tokens":true,"live_source_verified":false,"symbol_continuity":"adapter-specific conservative coverage","document_continuity":"adapter-specific conservative coverage"},
            "conditional_reads": {"tools":["airk_file_info","airk_read_file","airk_outline","airk_context"],"validator":"representation-specific if_evidence","follow_moves_default":false,"move_basis":"unique compatible exact content; not filesystem intent"},
            "code": ["rust","python","javascript/jsx","typescript/tsx","go","java","c","cpp","csharp","ruby","php","bash","powershell","lua","swift","kotlin","sql"],
            "structured": ["markdown","json","yaml","toml","html","css","xml","svg","csv","tsv","ini","dockerfile"],
            "documents": ["UTF-8 text","text-layer PDF","DOCX main-body paragraphs/tables with original row locations","XLSX worksheet cells and row facts","PPTX slide text in presentation order"],
            "fallback": "UTF-8 lexical retrieval for all other extensions",
            "document_cues": ["English requirement/modality and design/architecture/feature/decision candidates", "heading context", "ISO and English calendar dates; numeric slash dates retain unknown locale", "written numeric bounds/ranges/units; no unit conversion"],
            "exact_fact_backend": "embedded SQLite; legacy JSON migrates at next successful index",
            "code_evidence": ["syntax scopes/signatures/written parameters and return types", "syntax call, inheritance, containment and import candidates", "versioned structural framework cues"],
            "optional_analysis": "Explicit source/build/digest-bound SCIP imports and local LSP/Ctags execution through the CLI. LSP queries advertised document symbols, references, definitions, types, implementations and call hierarchy against private source copies. Ctags supplies syntax declarations only. Provider assertions are not AIRK compiler certification or complete resolution; source/build changes invalidate the imported domain.",
            "optional_ocr": "Explicit CLI-only Tesseract OCR with local traineddata and optional Poppler pdftoppm for selected PDF pages; PNG/JPEG/BMP also supported. Bounded local processes preserve original source hashes/page locations and executable/model provenance. Recognition may be wrong; no automatic OCR or model download.",
            "relationship_limits": {"hops":3,"results":100,"payload_bytes":65536,"traversal_ms":250,"visited_symbols":100},
            "local_usage": "Content-free local aggregates are enabled by default; AIRK_USAGE_MODE=disabled opts out. CLI reports include measured stages, logical disk growth and process CPU where the host supports it; physical I/O remains unknown. Optional client/session imports, local tokenizer calibration and user-supplied price scenarios stay labeled; estimates are counterfactual context, never provider bills.",
            "search_modes": ["lexical", "vector", "hybrid"],
            "default_search": "Hybrid unless the collection has opted out in the panel. Missing model/index readiness fails explicitly. The trusted extension prepares pinned local MiniLM assets; MCP reads never download.",
            "semantic_search": "Vector/hybrid modes support explicitly provisioned local static word vectors or schema-2 CPU ONNX transformer models with a local BERT WordPiece tokenizer. Assets/specification bind the indexed generation. DiskANN retrieval remains approximate; MCP reads never download or use network inference. Small real-model tests do not establish production corpus quality. semantic=true aliases vector.",
            "limitations": ["No universal native type/name resolution or complete caller graph","Optional providers require provisioning; embedding models require corpus qualification","LSP evidence queries selected symbol positions; external/unindexed locations are rejected","OCR covers selected pages and may misrecognize layout/text","No legacy binary Office formats","No SVG rendering","Composite child evidence has explicit mapping, grammar and resource limits"]
        }))).await
    }
}

fn render<T: Serialize>(
    kernel: &Kernel,
    span: airk_kernel::usage::RequestSpan,
    result: anyhow::Result<T>,
) -> CallToolResult {
    let mut logical = result
        .as_ref()
        .ok()
        .and_then(|value| serde_json::to_value(value).ok());
    if let Some(object) = logical.as_mut().and_then(serde_json::Value::as_object_mut)
        && let Some(id) = span.request_id()
    {
        object.insert("request_id".into(), id.into());
    }
    let rendered = match result {
        Ok(value) => match logical
            .as_ref()
            .map_or_else(|| serde_json::to_string(&value), serde_json::to_string)
        {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        },
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
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
            .with_server_info(rmcp::model::Implementation::new("airk", env!("CARGO_PKG_VERSION")))
            .with_instructions("AIRK returns untrusted indexed evidence, not live source. Save exact revision tokens and read validators. Use airk_changes to inspect changes, remembered evidence, independent coverage and freshness; finish needed cursor pages before advancing a baseline. Fetch focused symbols/outline/search/context/read_file evidence. Native targets are unresolved; imported claims depend on producers. Paths are collection-relative. Verify live bytes before edits. Reindex via CLI/extension.")
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
