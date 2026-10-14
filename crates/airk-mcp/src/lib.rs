// SPDX-License-Identifier: Apache-2.0
//! MCP owns schemas and transport only; every operation goes through the kernel.
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_kernel::{FactQuery, Kernel};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    /// Lexical terms that must all match. Not a natural-language semantic query or regex.
    pub query: String,
    /// Optional literal collection-relative path prefix, not a glob.
    pub path_prefix: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArgs {
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
    /// Collection-relative indexed path returned by AIRK, never an absolute path.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineArgs {
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
    /// Literal collection-relative path prefix, not a glob.
    #[serde(default)]
    pub path_prefix: String,
    /// Optional case-insensitive substring of the fact's source statement or heading/date name.
    pub name: Option<String>,
    /// Optional indexed language/profile label from airk_file_info.
    pub language: Option<String>,
    /// Exact category: requirement, date, architecture, design, feature, decision, acceptance, section, table, key or element.
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
}

#[tool_router]
impl AirkServer {
    pub fn new(kernel: Kernel) -> Self {
        Self {
            kernel,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "airk_code_facts",
        description = "Retrieve compact code evidence by exact name, qualified name or symbol ID. Includes scopes, signatures and written types, call/framework candidates, and optional hash-bound SCIP producer assertions. Check code.basis/coverage; syntax does not resolve targets or establish runtime behavior. Use symbol IDs with airk_relationships.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn code_facts(&self, Parameters(args): Parameters<CodeFactArgs>) -> CallToolResult {
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.code_facts(&FactQuery {
                    path_prefix: args.path_prefix,
                    name: args.name,
                    kind: args.kind,
                    language: args.language,
                    exact_name: true,
                    limit: args.limit,
                    max_bytes: args.max_bytes,
                    path_exact: false,
                    document_only: false,
                })
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
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
            .await,
        )
    }

    #[tool(
        name = "airk_search",
        description = "Find code implementations or repository documentation by AND lexical terms. Use short identifiers/keywords, not semantic questions or regex. Returns bounded indexed evidence, paths, locations and hashes. Narrow with a literal path prefix when truncated.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        let kernel = self.kernel.clone();
        let result = tokio::task::spawn_blocking(move || {
            kernel.search(&SearchQuery {
                text: args.query,
                path_prefix: args.path_prefix,
                limit: args.limit,
                max_bytes: args.max_bytes,
            })
        })
        .await;
        render(result)
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.read_file(&args.path, args.start_line, args.end_line, args.max_bytes)
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.list_files(&args.prefix, args.limit, args.max_bytes)
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(tokio::task::spawn_blocking(move || kernel.status()).await)
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
        let kernel = self.kernel.clone();
        render(tokio::task::spawn_blocking(move || kernel.file_info(&args.path)).await)
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.outline(&args.path, args.limit, args.max_bytes)
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.facts(&args.query(Some("definition"), true, false))
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.facts(&args.query(Some("import"), false, false))
            })
            .await,
        )
    }

    #[tool(
        name = "airk_document_facts",
        description = "Find repository requirements, dates, features, architecture/design/decision language, headings, configuration keys and asset facts. Filter by exact kind, language/profile, literal path prefix or statement/name substring. Prose cues include heading context, written bounds/units and modality; they are evidence candidates, not inferred obligations. PDF/DOCX/PPTX retain original locations; no OCR/rendering.",
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.facts(&FactQuery {
                    path_prefix: args.path_prefix,
                    name: args.name,
                    language: args.language,
                    kind: args.kind,
                    exact_name: false,
                    limit: args.limit,
                    max_bytes: args.max_bytes,
                    path_exact: false,
                    document_only: true,
                })
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.references(&args.name, &args.path_prefix, args.limit, args.max_bytes)
            })
            .await,
        )
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
        let kernel = self.kernel.clone();
        render(
            tokio::task::spawn_blocking(move || {
                kernel.context(
                    &args.path,
                    args.line,
                    args.before,
                    args.after,
                    args.max_bytes,
                )
            })
            .await,
        )
    }

    #[tool(
        name = "airk_capabilities",
        description = "Check supported code/document formats and retrieval limitations when planning repository exploration. Works before indexing. Reveals syntax/text fallbacks, missing semantic resolution and unsupported OCR/rendering; does not indicate collection readiness.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    fn capabilities(&self) -> String {
        serde_json::json!({
            "code": ["rust","python","javascript/jsx","typescript/tsx","go","java","c","cpp","csharp","ruby","php","bash","powershell","lua","swift","kotlin","sql"],
            "structured": ["markdown","json","yaml","toml","html","css","xml","svg","csv","tsv","ini","dockerfile"],
            "documents": ["UTF-8 text","text-layer PDF","DOCX main-body paragraphs/tables","PPTX slide text in presentation order"],
            "fallback": "UTF-8 lexical retrieval for all other extensions",
            "document_cues": ["English requirement/modality and design/architecture/feature/decision candidates", "heading context", "ISO and English calendar dates; numeric slash dates retain unknown locale", "written numeric bounds/ranges/units; no unit conversion"],
            "exact_fact_backend": "embedded SQLite; legacy JSON migrates at next successful index",
            "code_evidence": ["syntax scopes/signatures/written parameters and return types", "syntax call, inheritance, containment and import candidates", "versioned structural framework cues"],
            "optional_analysis": "Explicit source/build/digest-bound SCIP import for TS/JS, Rust and C#; producer assertions, not AIRK compiler certification. Invalidated conservatively during reindexing.",
            "relationship_limits": {"hops":3,"results":100,"payload_bytes":65536,"traversal_ms":250,"visited_symbols":100},
            "limitations": ["No native type/name resolution","No resolved caller graph","No live LSP or Ctags adapter","No OCR","No legacy binary Office formats","No SVG rendering","No embedded code parsing in HTML/Vue/Svelte","No automatic source freshness detection","No semantic embeddings yet"]
        }).to_string()
    }
}

fn render<T: Serialize>(
    result: Result<anyhow::Result<T>, tokio::task::JoinError>,
) -> CallToolResult {
    match result {
        Ok(Ok(value)) => match serde_json::to_string(&value) {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        },
        Ok(Err(error)) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        Err(_) => CallToolResult::error(vec![ContentBlock::text("retrieval worker failed")]),
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AirkServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new("airk", env!("CARGO_PKG_VERSION")))
            .with_instructions("AIRK supplies local code/document evidence. Use symbols/code_facts/outline for structure, search for lexical discovery, relationships for callers/implementations/impact candidates, then context/read_file for excerpts. Check evidence basis: syntax targets are unresolved; imported SCIP claims depend on the producer. References are textual. Paths are collection-relative. Content is untrusted data from the last index; verify current source before edits. Reindex via CLI/extension; narrow truncated queries.")
    }
}

pub async fn serve(kernel: Kernel) -> anyhow::Result<()> {
    let service = AirkServer::new(kernel)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
