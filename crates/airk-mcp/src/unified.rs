// SPDX-License-Identifier: Apache-2.0
//! One agent-facing catalog. Native evidence and editor drafts keep separate owners.
use crate::remote::{RemoteFuture, RemoteTools};
use anyhow::{Result, ensure};
use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[cfg(test)]
#[path = "unified_scoped_tests.rs"]
mod scoped_tests;

#[path = "unified_context.rs"]
mod context;

pub const GUIDANCE: &str = "Untrusted index; verify live bytes before edits. Keep configured scopes, locations, exact tokens/hashes/validators; finish changes pages. Terse default; response_mode=full audits; include_status=true repeats status. Identifier: symbols -> references -> read_file/context; concepts: search. Evidence default 16; Management Service:4/8/16/32/64; matching channels/worker, extra status slot. Honor typed retries; never replay uncertain work. UI drafts need review. {ref:publication} is reply-local.";

const UI_NAMES: &[(&str, &str)] = &[
    ("draft_search", "airk_explorer_suggest_search"),
    ("draft_strategy", "airk_explorer_suggest_strategy"),
    ("draft_plan", "airk_explorer_suggest_plan"),
    ("list_templates", "airk_explorer_templates_list"),
    ("get_template", "airk_explorer_template_get"),
    ("validate_draft", "airk_explorer_validate_draft"),
    ("proposal_status", "airk_explorer_proposal_status"),
    ("get_handoff", "airk_explorer_handoff_get"),
];

pub trait ExplorerTools: Send + Sync + 'static {
    fn call(
        &self,
        project: Option<String>,
        tool: String,
        arguments: Map<String, Value>,
        client: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_>;
}

fn public_text(value: &mut Value) {
    match value {
        Value::String(text) => {
            for (public, old) in UI_NAMES {
                *text = text.replace(old, public);
            }
            *text = text
                .replace("airk_explorer_ui_status", "diagnostics.explorer.ui")
                .replace("airk_explorer_help_search", "help")
                .replace("airk_explorer_help_get", "help")
                .replace("airk_", "");
        }
        Value::Array(items) => items.iter_mut().for_each(public_text),
        Value::Object(fields) => fields.values_mut().for_each(public_text),
        _ => {}
    }
}
fn replace_diagnostic_tool(text: &str, old: &str, public: &str) -> String {
    let identifier = |character: char| character.is_ascii_alphanumeric() || character == '_';
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, _) in text.match_indices(old) {
        let end = start + old.len();
        if text[..start].chars().next_back().is_some_and(identifier)
            || text[end..].chars().next().is_some_and(identifier)
        {
            continue;
        }
        result.push_str(&text[cursor..start]);
        result.push_str(public);
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn public_diagnostic_text(text: &str) -> String {
    let mut text = text.to_owned();
    for (public, old) in UI_NAMES {
        text = replace_diagnostic_tool(&text, old, public);
    }
    for (old, public) in [
        ("airk_explorer_ui_status", "diagnostics (explorer.ui)"),
        ("airk_explorer_help_search", "help"),
        ("airk_explorer_help_get", "help"),
        ("airk_explorer_suggest_navigation", "suggest_view"),
        ("airk_explorer_suggest_review", "suggest_view"),
        ("airk_explorer_suggest_management", "suggest_view"),
        ("airk_help", "help"),
    ] {
        text = replace_diagnostic_tool(&text, old, public);
    }
    text
}

/// Rewrite only adapter-authored diagnostics. Strategy operators, template
/// bodies, shared handoff content, and evidence retain their exact bytes.
fn public_ui_response(tool: &str, mut result: Value) -> Value {
    let Some(mut value) = body(&result) else {
        return result;
    };
    if let Some(error) = value.get_mut("error").and_then(Value::as_object_mut) {
        for field in ["message", "detail"] {
            if let Some(Value::String(text)) = error.get_mut(field) {
                *text = public_diagnostic_text(text);
            }
        }
    }
    if value.get("valid") == Some(&Value::Bool(false))
        && let Some(errors) = value.get_mut("errors").and_then(Value::as_array_mut)
    {
        for error in errors {
            if let Some(Value::String(text)) = error.get_mut("message") {
                *text = public_diagnostic_text(text);
            }
        }
    }
    if matches!(
        tool,
        "airk_explorer_help_get" | "airk_explorer_help_search" | "airk_explorer_ui_status"
    ) && let Some(notes) = value.get_mut("notes").and_then(Value::as_array_mut)
    {
        for note in notes {
            if let Value::String(text) = note {
                *text = public_diagnostic_text(text);
            }
        }
    }
    // Both MCP representations must carry the same projected diagnostics.
    if result.get("structuredContent").is_some() {
        result["structuredContent"] = value.clone();
    }
    if let Some(Value::String(text)) = result
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .and_then(|items| items.first_mut())
        .and_then(|item| item.get_mut("text"))
    {
        *text = value.to_string();
    }
    result
}

fn project_schema() -> Value {
    json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Configured source scope from projects. Required when this connection has no single default; never follows the Explorer dropdown."})
}
fn scoped(mut tool: Tool) -> Tool {
    let schema = Arc::make_mut(&mut tool.input_schema);
    schema.entry("properties").or_insert_with(|| json!({}))["project"] = project_schema();
    tool
}
fn definition(name: &str, description: &str, schema: Value, read_only: bool) -> Tool {
    serde_json::from_value(json!({"name":name,"description":description,"inputSchema":schema,
        "annotations":{"readOnlyHint":read_only,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})).expect("static tool definition")
}
pub fn tools() -> Vec<Tool> {
    static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_tools).clone()
}

// Exact bundled definitions only; per-session projects/admission/runtime state stays outside.
fn build_tools() -> Vec<Tool> {
    let mut result: Vec<Tool> = crate::local::tools()
        .into_iter()
        .map(|tool| {
            let mut value = serde_json::to_value(tool).expect("tool serialization");
            public_text(&mut value);
            let tool: Tool = serde_json::from_value(value).expect("public native tool");
            scoped(tool)
        })
        .collect();
    let legacy: Vec<Tool> = serde_json::from_str(include_str!("unified_explorer.json"))
        .expect("generated Explorer catalog");
    for (name, old) in UI_NAMES {
        let mut value = serde_json::to_value(
            legacy
                .iter()
                .find(|t| t.name == *old)
                .expect("Explorer schema"),
        )
        .unwrap();
        public_text(&mut value);
        value["name"] = json!(name);
        let tool: Tool = serde_json::from_value(value).unwrap();
        result.push(if matches!(*name, "list_templates" | "get_template") {
            tool
        } else {
            scoped(tool)
        });
    }
    let variants: Vec<Value> = ["navigation", "review", "management"]
        .iter()
        .map(|kind| {
            let old = format!("airk_explorer_suggest_{kind}");
            let tool = legacy.iter().find(|t| t.name == old).unwrap();
            let mut schema = Value::Object((*tool.input_schema).clone());
            schema["title"] = json!(format!("Draft for kind={kind}"));
            public_text(&mut schema);
            schema
        })
        .collect();
    result.push(scoped(definition("suggest_view", "Propose an Explorer screen for the user to open or prefill, including Review and Management guidance. Returns a proposal receipt, never source evidence. Routes such as code/outline name UI pages, not the evidence tools. The user applies the suggestion; it never runs a query, test or management action.",
        // Some assistants reject or silently omit tools with root composition.
        // Keep alternatives nested; the adapter validates the chosen kind again.
        json!({"type":"object","additionalProperties":false,"required":["kind","draft"],"properties":{"kind":{"type":"string","enum":["navigation","review","management"]},"draft":{"type":"object","description":"Use the draft schema matching kind. The editor validates that exact kind before admitting a suggestion.","anyOf":variants}}}), false)));
    result.push(definition("projects", "List only the project bindings configured for this AIRK connection and its default. Use these identifiers to scope evidence and Explorer requests; does not register or switch projects.", json!({"type":"object","additionalProperties":false,"properties":{}}), true));
    let status = result.iter_mut().find(|t| t.name == "status").unwrap();
    status.description = Some("Quick version, health and project-index check: counts, publication, source-update state, semantic readiness and configured query_concurrency at frontend admission. Obtain before broad evidence fan-out; lightweight does not mean exempt from bounded admission. Completed same-authority observations superseded by scalar invalidation return successfully with status_observation.superseded and their original publication/time. Successful snapshots may be cached for five seconds; observed_unix_ms, cache_age_ms and cached disclose their age. Allows up to ten seconds for shared cold runtime setup, followed by a two-second observation budget; expiry reports unknown readiness and null evidence. Always skips process diagnostics and editor probes. Call diagnostics for detailed work, runtime versions and Explorer attachment. Counts are saved evidence, not proof that live files match. Does not start indexing or repair.".into());
    let help = result.iter_mut().find(|t| t.name == "help").unwrap();
    help.description = Some("Explain AIRK usage, CLI commands, tool schemas and Explorer workflows. Omit selectors for an overview; query searches bundled guidance, topic_id reads a UI/engine guide, topic reads native guidance, command reads CLI help, and tool reads a public tool schema. Native guides retain prefixed CLI/evidence compatibility names; tools/list and tool selectors describe the unified public catalog. Returns documentation, never project evidence or live health.".into());
    let schema = Arc::make_mut(&mut help.input_schema);
    schema["properties"]
        .as_object_mut()
        .unwrap()
        .remove("project");
    schema["properties"]["tool"]["description"] = json!(
        "Exact public MCP tool name, for example search; returns its current description and input schema."
    );
    for (key, property) in [
        ("query", json!({"type":"string","maxLength":256})),
        ("topic_id", json!({"type":"string","maxLength":64})),
        (
            "detail",
            json!({"type":"string","enum":["summary","steps","full"],"description":"Explorer guidance detail; valid only with query or topic_id. Omit for native topic, command and tool help."}),
        ),
        ("area", json!({"type":"string","maxLength":32})),
        ("limit", json!({"type":"integer","minimum":1,"maximum":20})),
    ] {
        schema["properties"][key] = property;
    }
    result
}

pub fn success(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
}
pub fn failure(code: &str, message: &str) -> Value {
    let value = json!({"error":{"code":code,"message":message}});
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":true})
}
fn body(value: &Value) -> Option<Value> {
    value
        .get("structuredContent")
        .cloned()
        .or_else(|| serde_json::from_str(value["content"][0]["text"].as_str()?).ok())
}

pub struct Project {
    pub binding: String,
    pub name: String,
    pub collection: String,
    pub backend: Arc<dyn RemoteTools>,
}
pub struct Unified {
    projects: BTreeMap<String, Project>,
    default: Option<String>,
    explorer: Option<Arc<dyn ExplorerTools>>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
}
impl Unified {
    /// A combined connection has no invented legacy ProjectBinding. The
    /// sentinel is internal routing identity, never a discovery/grant token.
    pub fn combined(
        backend: Arc<dyn RemoteTools>,
        explorer: Option<Arc<dyn ExplorerTools>>,
    ) -> Result<Self> {
        ensure!(
            backend.combined_selection().is_some(),
            "combined session selection required"
        );
        backend
            .combined_selection()
            .expect("checked combined selector")
            .validate()?;
        Ok(Self {
            projects: BTreeMap::from([(
                "combined".into(),
                Project {
                    binding: "combined".into(),
                    name: "Selected Projects".into(),
                    collection: "combined".into(),
                    backend,
                },
            )]),
            default: Some("combined".into()),
            explorer,
            client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
        })
    }
    fn combined_backend(&self) -> Option<&Arc<dyn RemoteTools>> {
        // One service authority per combined session. A set of independently
        // configured legacy bindings never acquires a union implicitly.
        if self.projects.len() != 1 {
            return None;
        }
        self.projects
            .values()
            .next()
            .map(|project| &project.backend)
            .filter(|backend| backend.combined_selection().is_some())
    }
    fn validate_combined_search_draft(&self, args: &Map<String, Value>) -> Result<()> {
        let backend = self
            .combined_backend()
            .ok_or_else(|| anyhow::anyhow!("combined selection required"))?;
        let frozen = backend
            .combined_selection()
            .expect("selected combined backend");
        ensure!(
            !args.contains_key("project"),
            "combined draft cannot select a legacy binding"
        );
        let draft = args
            .get("draft")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("search draft requires an object"))?;
        let selector = Value::Object(
            draft
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "project_ids" | "index_ids" | "member_availability"
                    )
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let selected: airk_core::combined_reads::QuerySelection = serde_json::from_value(selector)?;
        selected.validate()?;
        let mut projects = selected.project_ids;
        projects.sort();
        let mut frozen_projects = frozen.project_ids.clone();
        frozen_projects.sort();
        let indexes = selected.index_ids.map(|mut ids| {
            ids.sort();
            ids
        });
        let frozen_indexes = frozen.index_ids.clone().map(|mut ids| {
            ids.sort();
            ids
        });
        ensure!(
            projects == frozen_projects && indexes == frozen_indexes,
            "search draft must retain the exact frozen connection selection"
        );
        Ok(())
    }
    pub fn new(
        projects: Vec<Project>,
        default: Option<String>,
        explorer: Option<Arc<dyn ExplorerTools>>,
    ) -> Result<Self> {
        ensure!(
            !projects.is_empty() && projects.len() <= 32,
            "invalid configured project count"
        );
        let count = projects.len();
        let projects: BTreeMap<_, _> = projects
            .into_iter()
            .map(|p| (p.binding.clone(), p))
            .collect();
        ensure!(
            projects.len() == count
                && projects.keys().all(|key| key.len() == 64
                    && key
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "invalid configured project identity"
        );
        ensure!(
            default.as_ref().is_none_or(|id| projects.contains_key(id)),
            "unknown default project"
        );
        Ok(Self {
            projects,
            default,
            explorer,
            client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
        })
    }
    fn select(&self, args: &mut Map<String, Value>) -> std::result::Result<&Project, Value> {
        if self.combined_backend().is_some() {
            return Err(failure(
                "QUALIFIED_UI_SCOPE_REQUIRED",
                "This connection has a combined query selection. Choose an originating index in Workbench before creating a scoped editor draft.",
            ));
        }
        let selected = match args.remove("project") {
            Some(Value::String(id)) => Some(id),
            Some(_) => {
                return Err(failure(
                    "INVALID_PROJECT",
                    "project must be a configured binding from projects.",
                ));
            }
            None => self.default.clone(),
        };
        selected.and_then(|id|self.projects.get(&id)).ok_or_else(|| failure("PROJECT_REQUIRED", "Choose an explicit configured binding from projects. Explorer selection does not select a retrieval project."))
    }
    async fn ui(&self, project: Option<String>, name: &str, args: Map<String, Value>) -> Value {
        let Some(explorer) = &self.explorer else {
            return failure(
                "UI_UNAVAILABLE",
                "Explorer integration is not configured. Native retrieval and native help remain available.",
            );
        };
        let client = *self.client.lock().unwrap();
        let result = explorer.call(project, name.into(), args, client).await.unwrap_or_else(|_| failure("UI_UNAVAILABLE", "The verified Explorer adapter is unavailable. Check diagnostics and the installed extension; native retrieval remains available."));
        public_ui_response(name, result)
    }
    async fn help(&self, mut args: Map<String, Value>) -> Value {
        if let Err(error) = crate::help::validate_detail_selector(&args) {
            return serde_json::to_value(crate::response::error_result(&error))
                .expect("typed help argument refusal");
        }
        let native_selector = !args.contains_key("query")
            && !args.contains_key("topic_id")
            && args.get("tool").is_none_or(|name| {
                name.as_str().is_some_and(|name| {
                    crate::local::tools().iter().any(|tool| {
                        tool.name == name || tool.name.strip_prefix("airk_") == Some(name)
                    })
                })
            });
        if native_selector {
            if let Some(backend) = self.combined_backend() {
                if args.contains_key("project") {
                    return failure(
                        "INVALID_QUERY_SCOPE",
                        "A combined session cannot select a legacy project binding.",
                    );
                }
                return match backend.call("airk_help".into(), args).await {
                    Ok(result) => result,
                    Err(error) => match crate::response::typed_error_result(&error) {
                        Some(reply) => {
                            serde_json::to_value(reply).expect("typed native help refusal")
                        }
                        None => crate::remote::combined_observation_failure(
                            backend
                                .combined_selection()
                                .expect("selected combined backend"),
                            "combined_observation_unavailable",
                        ),
                    },
                };
            }
            let mut native_args = args.clone();
            if let Ok(project) = self.select(&mut native_args)
                && let Some(selection) = project.backend.scoped_selection()
            {
                return match project.backend.call("airk_help".into(), native_args).await {
                    Ok(result) => result,
                    Err(error) => match crate::response::typed_error_result(&error) {
                        Some(reply) => {
                            serde_json::to_value(reply).expect("typed native help refusal")
                        }
                        None => crate::remote::scoped_observation_failure(
                            selection,
                            "scoped_observation_unavailable",
                            true,
                        ),
                    },
                };
            }
        }
        if let Some(detail) = args.get("reply_detail")
            && serde_json::from_value::<Option<airk_core::reply::ReplyDetail>>(detail.clone())
                .is_err()
        {
            return failure("INVALID_SCHEMA", "Invalid reply_detail.");
        }
        if args.keys().any(|k| {
            ![
                "query",
                "topic_id",
                "topic",
                "command",
                "tool",
                "detail",
                "area",
                "limit",
                "start_line",
                "max_bytes",
                "response_mode",
                "include_status",
                "reply_detail",
            ]
            .contains(&k.as_str())
        }) {
            return failure("INVALID_SCHEMA", "Unknown help argument.");
        }
        // Only bundled UI selectors add mutual exclusivity here. Native topic
        // qualifiers (for example topic=tools with tool=search) are validated by
        // the same canonical help implementation used by the native aliases.
        if (args.contains_key("query") || args.contains_key("topic_id"))
            && ["query", "topic_id", "topic", "command", "tool"]
                .iter()
                .filter(|key| args.contains_key(**key))
                .count()
                > 1
        {
            return failure("INVALID_HELP_SELECTOR", "Choose one help selector.");
        }
        if args.contains_key("query") || args.contains_key("topic_id") {
            args.remove("response_mode");
            args.remove("include_status");
            // Guidance has no Lean/Rich evidence projection; the UI adapter keeps
            // its existing strict contract, just as for metadata display controls.
            args.remove("reply_detail");
            let name = if args.contains_key("query") {
                "airk_explorer_help_search"
            } else {
                "airk_explorer_help_get"
            };
            return self.ui(None, name, args).await;
        }
        if let Some(Value::String(name)) = args.get("tool") {
            args.insert(
                "tool".into(),
                json!(name.strip_prefix("airk_").unwrap_or(name)),
            );
            let result = serde_json::to_value(crate::help::call_with_catalog(
                Value::Object(args),
                self.catalog(),
            ))
            .unwrap();
            return result;
        }
        // Native CLI examples and error envelopes retain their exact names.
        serde_json::to_value(crate::help::call(Value::Object(args))).unwrap()
    }
}
fn status_not_observed(project: &str, stage: &str, expired: bool) -> Value {
    success(
        json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project,
        "index_state":"unknown","publication":null,"files":null,"chunks":null,
        "index_error":{"code":if expired{"status_timeout"}else{"status_unavailable"},
        "stage":stage,"retryable":true,"message":"Status could not be confirmed within this stage. Retry or call diagnostics."}}),
    )
}
impl RemoteTools for Unified {
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: Map<String, Value>,
        limit: usize,
        context: crate::read_attempt::ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            context.require_live()?;
            // A per-request view keeps the exact selectors and shared backend
            // owners while binding every routing branch to this same attempt.
            let bound = context::bind(self, limit, context);
            bound.call_with_capacity(tool, arguments, limit).await
        })
    }

    fn call_with_capacity(
        &self,
        tool: String,
        args: Map<String, Value>,
        limit: usize,
    ) -> RemoteFuture<'_> {
        if let Some(backend) = self.combined_backend()
            && tool != "help"
            && let Some(native) = crate::local::tools().into_iter().find(|entry| {
                entry.name == tool || entry.name.strip_prefix("airk_") == Some(tool.as_str())
            })
        {
            if args.contains_key("project") {
                return Box::pin(async {
                    Ok(failure(
                        "INVALID_QUERY_SCOPE",
                        "A combined session cannot select a legacy project binding.",
                    ))
                });
            }
            return backend.call_with_capacity(native.name.to_string(), args, limit);
        }
        self.call(tool, args)
    }
    fn combined_selection(&self) -> Option<&airk_core::service::CombinedConnectRequest> {
        self.combined_backend()
            .and_then(|backend| backend.combined_selection())
    }
    fn query_concurrency(&self) -> usize {
        self.projects
            .values()
            .map(|p| p.backend.query_concurrency())
            .min()
            .unwrap_or(airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY)
    }
    fn close(&self) {
        for project in self.projects.values() {
            project.backend.close();
        }
    }
    fn shutdown(&self) -> crate::remote::ReadyFuture<'_> {
        Box::pin(async move {
            let mut failure: Option<anyhow::Error> = None;
            for project in self.projects.values() {
                if let Err(error) = project.backend.shutdown().await {
                    failure = Some(match failure {
                        None => error,
                        Some(primary) => {
                            primary.context(format!("additional project cleanup failed: {error:#}"))
                        }
                    });
                }
            }
            match failure {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
    }
    fn catalog(&self) -> Vec<Tool> {
        if self.combined_backend().is_some() {
            crate::combined::public_catalog()
        } else {
            tools()
        }
    }
    fn guidance(&self) -> &'static str {
        GUIDANCE
    }
    fn local_help(&self) -> bool {
        false
    }
    fn uses_response_mode(&self, tool: &str) -> bool {
        if matches!(tool, "query_scope" | "airk_query_scope") {
            return false;
        }
        crate::local::tools()
            .iter()
            .any(|t| t.name == tool || t.name.strip_prefix("airk_") == Some(tool))
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        *self.client.lock().unwrap() = client;
        for project in self.projects.values() {
            project.backend.client(client);
        }
    }
    fn call(&self, tool: String, mut args: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            if tool == "projects" {
                if self.combined_backend().is_some() && args.is_empty() {
                    return Ok(success(
                        json!({"default_project":null,"projects":[],"mode":"combined","query_scope":"Use query_scope to discover only the frozen reviewed grant."}),
                    ));
                }
                return Ok(if args.is_empty() {
                    success(
                        json!({"default_project":self.default,"projects":self.projects.values().map(|p|json!({"project":p.binding,"name":p.name,"collection":p.collection})).collect::<Vec<_>>()}),
                    )
                } else {
                    failure("INVALID_SCHEMA", "projects accepts no arguments.")
                });
            }
            if tool == "help" {
                return Ok(self.help(args).await);
            }
            if matches!(tool.as_str(), "query_scope" | "airk_query_scope") {
                if let Some(backend) = self.combined_backend() {
                    return backend.call("airk_query_scope".into(), args).await;
                }
                if self.projects.is_empty() {
                    return Ok(crate::combined::legacy_discovery(Value::Object(args)));
                }
                // Transport project selection is separate from native selectors.
                // Ambiguity and invalid selections must not become legacy success.
                let project = match self.select(&mut args) {
                    Ok(project) => project,
                    Err(error) => return Ok(error),
                };
                let valid = serde_json::from_value::<
                    airk_core::combined_reads::query_scope::QueryScopeArgs,
                >(Value::Object(args.clone()))
                .map_err(anyhow::Error::from)
                .and_then(|request| request.validate());
                if valid.is_ok() {
                    // The native single-index handler can disclose its installed
                    // authority. It never invents current catalog metadata or grants.
                    return project.backend.call("airk_query_scope".into(), args).await;
                }
                return Ok(crate::combined::legacy_discovery(Value::Object(args)));
            }
            if matches!(
                tool.as_str(),
                "draft_search" | "airk_explorer_suggest_search"
            ) && self.combined_backend().is_some()
            {
                if self.validate_combined_search_draft(&args).is_err() {
                    return Ok(failure(
                        "INVALID_QUERY_SCOPE",
                        "A combined Search draft must carry the exact frozen project_ids and index_ids selection inside draft; legacy project bindings are not accepted.",
                    ));
                }
                // Inert form proposal to the descriptor's window. The complete
                // explicit selection travels in the draft and grants no access.
                return Ok(self.ui(None, "airk_explorer_suggest_search", args).await);
            }
            if tool.starts_with("airk_explorer_")
                && serde_json::from_str::<Vec<Tool>>(include_str!("unified_explorer.json"))?
                    .iter()
                    .any(|entry| entry.name == tool)
            {
                let project = if matches!(
                    tool.as_str(),
                    "airk_explorer_help_search"
                        | "airk_explorer_help_get"
                        | "airk_explorer_templates_list"
                        | "airk_explorer_template_get"
                ) {
                    None
                } else {
                    match self.select(&mut args) {
                        Ok(p) => Some(p.binding.clone()),
                        Err(error) => return Ok(error),
                    }
                };
                return Ok(self.ui(project, &tool, args).await);
            }
            // Old names remain callable for saved permissions/scripts, but are never advertised twice.
            let native = crate::local::tools()
                .into_iter()
                .find(|t| t.name == tool || t.name.strip_prefix("airk_") == Some(tool.as_str()));
            if let Some(native) = native {
                if let Some(backend) = self.combined_backend() {
                    if args.contains_key("project") {
                        return Ok(failure(
                            "INVALID_QUERY_SCOPE",
                            "A combined session cannot select a legacy project binding.",
                        ));
                    }
                    // The native owner enforces the one outer deadline and
                    // selected-member budgets, including status and parse.
                    // Preserve its complete result without legacy/UI enrichment.
                    return match backend.call(native.name.to_string(), args).await {
                        Ok(result) => Ok(result),
                        Err(error) => Ok(match crate::response::typed_error_result(&error) {
                            Some(reply) => {
                                serde_json::to_value(reply).expect("typed native read refusal")
                            }
                            None => crate::remote::combined_observation_failure(
                                backend
                                    .combined_selection()
                                    .expect("selected combined backend"),
                                "combined_observation_unavailable",
                            ),
                        }),
                    };
                }
                if ["project_ids", "index_ids", "member_availability"]
                    .iter()
                    .any(|key| args.contains_key(*key))
                {
                    return Ok(failure(
                        "QUERY_SCOPE_CONFIGURATION_REQUIRED",
                        "Connect using an explicitly reviewed Project/index grant before selecting combined query scope.",
                    ));
                }
                if native.name == "airk_help" {
                    let mut native_args = args.clone();
                    if let Ok(project) = self.select(&mut native_args)
                        && let Some(selection) = project.backend.scoped_selection()
                    {
                        return Ok(
                            match project
                                .backend
                                .call(native.name.to_string(), native_args)
                                .await
                            {
                                Ok(result) => result,
                                Err(error) => match crate::response::typed_error_result(&error) {
                                    Some(reply) => serde_json::to_value(reply)
                                        .expect("typed native help refusal"),
                                    None => crate::remote::scoped_observation_failure(
                                        selection,
                                        "scoped_observation_unavailable",
                                        true,
                                    ),
                                },
                            },
                        );
                    }
                    return Ok(serde_json::to_value(crate::help::call(Value::Object(
                        args,
                    )))?);
                }
                let project = match self.select(&mut args) {
                    Ok(p) => p,
                    Err(error) => return Ok(error),
                };
                if let Some(selection) = project.backend.scoped_selection() {
                    if native.name == "airk_status" {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            project.backend.prepare(),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                if let Some(reply) = crate::response::typed_error_result(&error) {
                                    return Ok(serde_json::to_value(reply)?);
                                }
                                return Ok(crate::remote::scoped_setup_failure(
                                    selection,
                                    if error.is::<crate::remote::PreparationExpired>() {
                                        "scoped_status_prepare_timeout"
                                    } else {
                                        "scoped_status_prepare_unavailable"
                                    },
                                    true,
                                    None,
                                ));
                            }
                            Err(_) => {
                                return Ok(crate::remote::scoped_setup_failure(
                                    selection,
                                    "scoped_status_prepare_timeout",
                                    true,
                                    None,
                                ));
                            }
                        }
                        return Ok(
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                project.backend.call(native.name.to_string(), args),
                            )
                            .await
                            {
                                Ok(Ok(result)) => result,
                                Ok(Err(error)) => crate::response::typed_error_result(&error)
                                    .map(|reply| {
                                        serde_json::to_value(reply).expect("typed read receipt")
                                    })
                                    .unwrap_or_else(|| {
                                        crate::remote::scoped_observation_failure(
                                            selection,
                                            "scoped_status_unavailable",
                                            true,
                                        )
                                    }),
                                Err(_) => crate::remote::scoped_observation_failure(
                                    selection,
                                    "scoped_status_timeout",
                                    true,
                                ),
                            },
                        );
                    }
                    // Selected native results already contain canonical scope
                    // disclosure. No global/UI diagnostics enrichment is added.
                    return Ok(
                        match project.backend.call(native.name.to_string(), args).await {
                            Ok(result) => result,
                            Err(error) => crate::response::typed_error_result(&error)
                                .map(|reply| {
                                    serde_json::to_value(reply).expect("typed read receipt")
                                })
                                .unwrap_or_else(|| {
                                    crate::remote::scoped_observation_failure(
                                        selection,
                                        "scoped_observation_unavailable",
                                        true,
                                    )
                                }),
                        },
                    );
                }
                if matches!(native.name.as_ref(), "airk_status" | "airk_diagnostics") {
                    let _options = match serde_json::from_value::<crate::StatusArgs>(Value::Object(
                        args.clone(),
                    )) {
                        Ok(options) => options,
                        Err(_) => {
                            return Ok(failure("INVALID_SCHEMA", "Invalid status arguments."));
                        }
                    };
                    if native.name == "airk_status" {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            project.backend.prepare(),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) if error.is::<crate::remote::PreparationExpired>() => {
                                return Ok(status_not_observed(&project.binding, "setup", true));
                            }
                            Ok(Err(error)) => return Err(error),
                            Err(_) => {
                                return Ok(status_not_observed(&project.binding, "setup", true));
                            }
                        }
                        let index = match tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            project.backend.call(native.name.to_string(), args),
                        )
                        .await
                        {
                            Ok(Ok(result)) => result,
                            Ok(Err(error)) => {
                                if let Some(reply) = crate::response::typed_error_result(&error) {
                                    return Ok(serde_json::to_value(reply)?);
                                }
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","files":null,"chunks":null,"publication":null,"index_error":{"code":"status_unavailable","stage":"observation","message":"Basic status could not be observed. Call diagnostics for troubleshooting."}}),
                                ));
                            }
                            Err(_) => {
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","publication":null,"files":null,"chunks":null,"index_error":{"code":"status_timeout","stage":"observation","retryable":true,"message":"The quick status deadline expired; index readiness is unconfirmed. Retry or call diagnostics for diagnosis."}}),
                                ));
                            }
                        };
                        if index["isError"] == true {
                            return Ok(index);
                        }
                        let mut value =
                            body(&index).unwrap_or_else(|| json!({"index_state":"unavailable"}));
                        if let Some(fields) = value.as_object_mut() {
                            fields.insert("project".into(), json!(project.binding));
                        }
                        return Ok(success(value));
                    }
                    let (index, explorer) = tokio::join!(
                        project.backend.call(native.name.to_string(), args),
                        self.ui(
                            Some(project.binding.clone()),
                            "airk_explorer_ui_status",
                            Map::new()
                        )
                    );
                    let index = match index {
                        Ok(value) if value["isError"] == true => return Ok(value),
                        Ok(value) => value,
                        Err(error) => {
                            if let Some(reply) = crate::response::typed_error_result(&error) {
                                return Ok(serde_json::to_value(reply)?);
                            }
                            let code = if error.to_string().contains("service_explicitly_stopped") {
                                "service_explicitly_stopped"
                            } else {
                                "index_status_unavailable"
                            };
                            success(
                                json!({"index_state":"unavailable","collection":project.collection,"publication":null,"files":null,"chunks":null,"index_error":{"code":code,"message":"Native status could not be observed. Review service state and diagnostics before retrying."}}),
                            )
                        }
                    };
                    let mut value =
                        body(&index).unwrap_or_else(|| json!({"index_state":"unavailable"}));
                    if let Some(fields) = value.as_object_mut() {
                        fields.insert(
                            "explorer".into(),
                            body(&explorer).unwrap_or_else(|| json!({"available":false})),
                        );
                        fields.insert("project".into(), json!(project.binding));
                    }
                    return Ok(success(value));
                }
                let result = project.backend.call(native.name.to_string(), args).await?;
                if native.name == "airk_parse"
                    && result["isError"] != true
                    && let Some(value) = body(&result)
                {
                    let diagnostics = value["steps"].as_array().is_some_and(|steps| {
                        steps.iter().any(|step| {
                            step["tool"] == "diagnostics"
                                && step["status"] == "complete"
                                && step["result"].is_object()
                        })
                    });
                    let explorer = if diagnostics {
                        let response = self
                            .ui(
                                Some(project.binding.clone()),
                                "airk_explorer_ui_status",
                                Map::new(),
                            )
                            .await;
                        Some(body(&response).unwrap_or_else(|| json!({"available":false})))
                    } else {
                        None
                    };
                    let mapped = crate::remote::map_plan_results(value, |tool, fields| {
                        if matches!(tool, "status" | "diagnostics") {
                            fields.insert("project".into(), json!(project.binding));
                        }
                        if tool == "diagnostics"
                            && let Some(explorer) = &explorer
                        {
                            fields.insert("explorer".into(), explorer.clone());
                        }
                    });
                    let value = match mapped {
                        Ok(value) => value,
                        Err(error)
                            if error
                                .is::<airk_core::verification::VerificationProjectionChanged>()
                                || error
                                    .is::<airk_core::verification::VerificationReplyBudget>() =>
                        {
                            return Ok(serde_json::to_value(crate::response::error_result(
                                &error,
                            ))?);
                        }
                        Err(error) => return Err(error),
                    };
                    return Ok(success(value));
                }
                if result["isError"] == true
                    && let Some(value) = body(&result)
                    && value["error"]["code"] == "verification_required_unmet"
                    && value["source_verification"]["schema"] == "airk.saved-source-verification/1"
                {
                    // Materialize the complete structured error only after the
                    // bounded private native hop. Keep its original text/body.
                    let mut result = result;
                    result["structuredContent"] = value;
                    return Ok(result);
                }
                return Ok(result);
            }
            let ui = UI_NAMES
                .iter()
                .find(|(name, old)| *name == tool || *old == tool)
                .map(|(_, old)| *old);
            if let Some(name) = ui {
                let project = if matches!(
                    name,
                    "airk_explorer_templates_list" | "airk_explorer_template_get"
                ) {
                    None
                } else {
                    match self.select(&mut args) {
                        Ok(p) => Some(p.binding.clone()),
                        Err(error) => return Ok(error),
                    }
                };
                return Ok(self.ui(project, name, args).await);
            }
            if tool == "suggest_view" {
                let project = match self.select(&mut args) {
                    Ok(p) => p,
                    Err(error) => return Ok(error),
                };
                if args.len() != 2 {
                    return Ok(failure(
                        "INVALID_SCHEMA",
                        "suggest_view requires kind and draft.",
                    ));
                }
                let Some(kind) = args
                    .get("kind")
                    .and_then(Value::as_str)
                    .filter(|k| ["navigation", "review", "management"].contains(k))
                else {
                    return Ok(failure("INVALID_SCHEMA", "Unknown view suggestion kind."));
                };
                let name = format!("airk_explorer_suggest_{kind}");
                let Some(draft) = args.get("draft").and_then(Value::as_object) else {
                    return Ok(failure("INVALID_SCHEMA", "draft must be an object."));
                };
                return Ok(self
                    .ui(Some(project.binding.clone()), &name, draft.clone())
                    .await);
            }
            Ok(failure(
                "UNKNOWN_TOOL",
                "Use the unified tools/list catalog.",
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[tokio::test]
    async fn malformed_backend_plan_preserves_connection_failure_boundary() -> Result<()> {
        struct Malformed;
        impl RemoteTools for Malformed {
            fn call(&self, _: String, _: Map<String, Value>) -> RemoteFuture<'_> {
                Box::pin(async { Ok(success(json!({"private_backend_detail":"invalid plan"}))) })
            }
        }
        let binding = "ab".repeat(32);
        let unified = Unified::new(
            vec![Project {
                binding: binding.clone(),
                name: "fixture".into(),
                collection: "fixture".into(),
                backend: Arc::new(Malformed),
            }],
            Some(binding),
            None,
        )?;
        for alias in ["parse", "airk_parse"] {
            let error = unified.call(alias.into(), Map::new()).await.unwrap_err();
            assert!(error.is::<serde_json::Error>(), "{error:#}");
        }
        Ok(())
    }

    #[test]
    fn common_instructions_keep_the_protocol_byte_budget_and_native_names() {
        for (catalog, instructions) in
            [("unified", GUIDANCE), ("native", crate::response::GUIDANCE)]
        {
            // str::len is UTF-8 bytes, matching the actual initialize contract.
            assert!(
                instructions.len() <= 512,
                "{catalog}: {} bytes",
                instructions.len()
            );
            assert!(instructions.contains("response_mode=full"), "{catalog}");
            assert!(instructions.contains("include_status=true"), "{catalog}");
            assert!(
                instructions.contains("never replay uncertain work"),
                "{catalog}"
            );
        }
        for name in [
            "airk_changes",
            "airk_symbols",
            "airk_references",
            "airk_read_file",
            "airk_context",
            "airk_search",
        ] {
            assert!(
                crate::response::GUIDANCE.contains(name),
                "native alias missing: {name}"
            );
        }
        assert!(
            !GUIDANCE.contains("airk_"),
            "unified discovery uses public names"
        );
    }
    #[tokio::test]
    async fn public_and_legacy_status_preserve_genuine_setup_errors_but_expiry_has_null_evidence()
    -> Result<()> {
        struct Preparation {
            expired: bool,
        }
        impl RemoteTools for Preparation {
            fn prepare(&self) -> crate::remote::ReadyFuture<'_> {
                Box::pin(async move {
                    if self.expired {
                        return Err(crate::remote::PreparationExpired.into());
                    }
                    Err(
                        anyhow::Error::from(std::io::Error::other("payload authentication failed"))
                            .context("runtime verification"),
                    )
                })
            }
            fn call(&self, _: String, _: Map<String, Value>) -> RemoteFuture<'_> {
                Box::pin(async { panic!("observation admitted after failed setup") })
            }
        }
        for expired in [false, true] {
            let binding = "aa".repeat(32);
            let unified = Unified::new(
                vec![Project {
                    binding: binding.clone(),
                    name: "fixture".into(),
                    collection: "fixture".into(),
                    backend: Arc::new(Preparation { expired }),
                }],
                Some(binding),
                None,
            )?;
            for name in ["status", "airk_status"] {
                let result = unified.call(name.into(), Map::new()).await;
                if expired {
                    let value = body(&result?).expect("typed setup expiry body missing");
                    assert_eq!(value["index_error"]["code"], "status_timeout");
                    assert_eq!(value["index_error"]["stage"], "setup");
                    assert!(
                        value["publication"].is_null()
                            && value["files"].is_null()
                            && value["chunks"].is_null()
                    );
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(
                        format!("{error:#}"),
                        "runtime verification: payload authentication failed"
                    );
                    assert!(error.downcast_ref::<std::io::Error>().is_some());
                }
            }
        }
        Ok(())
    }
    #[tokio::test]
    async fn shutdown_attempts_every_project_after_an_earlier_failure() -> Result<()> {
        struct Cleanup {
            fail: bool,
            joined: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl RemoteTools for Cleanup {
            fn call(&self, _: String, _: Map<String, Value>) -> RemoteFuture<'_> {
                Box::pin(async { Ok(Value::Null) })
            }
            fn shutdown(&self) -> crate::remote::ReadyFuture<'_> {
                Box::pin(async move {
                    self.joined
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if self.fail {
                        anyhow::bail!("first cleanup failed");
                    }
                    Ok(())
                })
            }
        }
        let joined = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let projects = [("a", true), ("b", false)]
            .into_iter()
            .map(|(binding, fail)| Project {
                binding: binding.repeat(64),
                name: binding.into(),
                collection: binding.into(),
                backend: Arc::new(Cleanup {
                    fail,
                    joined: joined.clone(),
                }),
            })
            .collect();
        let unified = Unified::new(projects, None, None)?;
        assert!(
            format!("{:#}", unified.shutdown().await.unwrap_err()).contains("first cleanup failed")
        );
        assert_eq!(joined.load(std::sync::atomic::Ordering::SeqCst), 2);
        Ok(())
    }
    #[derive(Default)]
    struct Native {
        calls: Mutex<Vec<(String, Map<String, Value>)>>,
        stall_status: std::sync::atomic::AtomicBool,
        busy_status: std::sync::atomic::AtomicBool,
    }
    impl RemoteTools for Native {
        fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
            Box::pin(async move {
                let status = tool == "airk_status";
                let parse = tool == "airk_parse";
                let operation = if args.get("text") == Some(&json!("/diagnostics")) {
                    "diagnostics"
                } else {
                    "status"
                };
                self.calls.lock().unwrap().push((tool, args));
                if parse {
                    return Ok(success(
                        json!({"schema":"airk.workbench.result/1","collection":"one","publication":null,"status":"complete","executed":true,"plan":airk_core::workbench::Plan::default(),"requires_input":[],"steps":[{"id":"query","tool":operation,"arguments":{},"status":"complete","elapsed_ms":1,"result":{"files":1},"error":null}],"evidence":[],"warnings":[],"truncated":false,"completeness":[],"calls":1,"payload_bytes":11}),
                    ));
                }
                if status && self.stall_status.load(std::sync::atomic::Ordering::SeqCst) {
                    return std::future::pending().await;
                }
                if status && self.busy_status.load(std::sync::atomic::Ordering::SeqCst) {
                    // The native worker's unsubmitted admission refusal while an
                    // abandoned quick-status query still holds its slot.
                    let detail = json!({"error":{"code":"direct_query_admission","reason":"busy",
                        "query_submitted":false,"retryable":true,"retry_after_ms":25,
                        "fresh_brokerage_required":false}});
                    return Ok(
                        json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true}),
                    );
                }
                Ok(success(
                    json!({"index_state":"available","publication":{"token":"fixture"},"freshness":{"state":"idle"}}),
                ))
            })
        }
    }
    type UiCall = (Option<String>, String, Map<String, Value>);
    #[derive(Default)]
    struct Ui {
        calls: Mutex<Vec<UiCall>>,
    }
    impl ExplorerTools for Ui {
        fn call(
            &self,
            project: Option<String>,
            tool: String,
            args: Map<String, Value>,
            _: airk_core::usage::ClientFamily,
        ) -> RemoteFuture<'_> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((project, tool, args));
                Ok(success(
                    json!({"ui":{"attached":false,"code":"UI_NOT_ATTACHED"},"executed":false}),
                ))
            })
        }
    }
    fn fixture(multiple: bool, editor: bool) -> (Unified, Arc<Native>, Arc<Native>, Arc<Ui>) {
        let a = Arc::new(Native::default());
        let b = Arc::new(Native::default());
        let ui = Arc::new(Ui::default());
        let mut projects = vec![Project {
            binding: "aa".repeat(32),
            name: "one".into(),
            collection: "one".into(),
            backend: a.clone(),
        }];
        if multiple {
            projects.push(Project {
                binding: "bb".repeat(32),
                name: "two".into(),
                collection: "two".into(),
                backend: b.clone(),
            });
        }
        (
            Unified::new(
                projects,
                (!multiple).then(|| "aa".repeat(32)),
                editor.then(|| ui.clone() as Arc<dyn ExplorerTools>),
            )
            .unwrap(),
            a,
            b,
            ui,
        )
    }
    async fn call(u: &Unified, name: &str, args: Value) -> Value {
        u.call(name.into(), args.as_object().unwrap().clone())
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn plan_status_and_diagnostics_keep_unified_project_and_editor_observations() {
        let (unified, _, _, ui) = fixture(false, true);
        for tool in ["status", "diagnostics"] {
            let result = call(
                &unified,
                "parse",
                json!({"schema":"airk.parse.request/1","mode":"execute","text":format!("/{tool}")}),
            )
            .await;
            let value = body(&result).unwrap();
            assert_eq!(value["steps"][0]["result"]["project"], "aa".repeat(32));
            assert_eq!(value["steps"][0]["result"]["files"], 1);
            if tool == "diagnostics" {
                assert_eq!(
                    value["steps"][0]["result"]["explorer"]["ui"]["code"],
                    "UI_NOT_ATTACHED"
                );
            }
        }
        assert_eq!(
            ui.calls.lock().unwrap().len(),
            1,
            "only diagnostics probes the editor"
        );
    }
    #[test]
    fn immutable_catalog_preserves_exact_definitions_and_isolates_caller_mutation() {
        let expected = serde_json::to_vec(&build_tools()).unwrap();
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
        let mut changed = tools();
        changed[0].description = Some("caller-local description".into());
        std::sync::Arc::make_mut(&mut changed[0].input_schema)
            .insert("caller-local".into(), serde_json::json!(true));
        changed.reverse();
        changed.pop();
        assert_ne!(serde_json::to_vec(&changed).unwrap(), expected);
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
    }

    #[test]
    fn one_catalog_has_unique_unprefixed_names_and_honest_effect_annotations() {
        let catalog = tools();
        assert_eq!(catalog.len(), 29);
        let names: std::collections::BTreeSet<_> =
            catalog.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names.len(), 29);
        assert!(names.contains("query_scope"));
        assert!(names.iter().all(|n| !n.starts_with("airk_")));
        for tool in catalog {
            let value = serde_json::to_value(&tool).unwrap();
            let schema = &value["inputSchema"];
            assert_eq!(schema["type"], "object");
            for key in ["oneOf", "anyOf", "allOf", "not", "if", "then", "else"] {
                assert!(schema.get(key).is_none(), "{} uses root {key}", tool.name);
            }
            if tool.name == "suggest_view" {
                assert_eq!(schema["required"], json!(["kind", "draft"]));
                let variants = schema["properties"]["draft"]["anyOf"].as_array().unwrap();
                assert_eq!(variants.len(), 3);
                for (variant, field) in variants.iter().zip(["route", "schema_version", "intent"]) {
                    assert_eq!(variant["additionalProperties"], false);
                    assert!(
                        variant["required"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(field))
                    );
                }
            }
            let writes = tool.name.starts_with("draft_") || tool.name == "suggest_view";
            assert_eq!(
                value["annotations"]["readOnlyHint"], !writes,
                "{}",
                tool.name
            );
            assert_eq!(value["annotations"]["destructiveHint"], false);
        }
    }
    #[tokio::test]
    async fn no_editor_does_not_block_index_reads_and_status_keeps_availability_separate() {
        let (u, a, _, _) = fixture(false, false);
        assert_eq!(
            call(&u, "status", json!({"unexpected":true})).await["isError"],
            true
        );
        assert!(a.calls.lock().unwrap().is_empty());
        let status = call(&u, "diagnostics", json!({})).await;
        assert_eq!(status["isError"], false);
        assert_eq!(body(&status).unwrap()["index_state"], "available");
        assert_eq!(
            body(&status).unwrap()["explorer"]["error"]["code"],
            "UI_UNAVAILABLE"
        );
        let search = call(&u, "search", json!({"query":"fixture"})).await;
        assert_eq!(search["isError"], false);
        let calls = a.calls.lock().unwrap();
        assert_eq!(calls[1].0, "airk_search");
        assert!(!calls[1].1.contains_key("project"));
    }
    #[tokio::test]
    async fn status_never_calls_the_editor_and_diagnostics_does() {
        let (u, _, _, ui) = fixture(false, true);
        let quick = call(&u, "status", json!({})).await;
        assert!(body(&quick).unwrap().get("explorer").is_none());
        assert!(ui.calls.lock().unwrap().is_empty());
        for args in [
            json!({"response_mode":"full"}),
            json!({"include_status":true}),
        ] {
            assert!(
                body(&call(&u, "status", args.clone()).await)
                    .unwrap()
                    .get("explorer")
                    .is_none()
            );
            assert!(
                body(&call(&u, "diagnostics", args).await)
                    .unwrap()
                    .get("explorer")
                    .is_some()
            );
        }
        assert_eq!(ui.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn legacy_and_public_status_share_schema_response_and_refusal() {
        let (u, native, _, ui) = fixture(false, true);
        let public = call(&u, "status", json!({})).await;
        let legacy = call(&u, "airk_status", json!({})).await;
        assert_eq!(public, legacy);
        for name in ["status", "airk_status"] {
            assert_eq!(
                call(&u, name, json!({"unexpected":true})).await["isError"],
                true
            );
        }
        assert_eq!(native.calls.lock().unwrap().len(), 2);
        native
            .busy_status
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            call(&u, "status", json!({})).await,
            call(&u, "airk_status", json!({})).await
        );
        assert!(ui.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn legacy_status_has_the_public_observation_deadline_and_null_evidence() {
        let (u, native, _, ui) = fixture(false, true);
        native
            .stall_status
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = body(&call(&u, "airk_status", json!({})).await).unwrap();
        assert_eq!(result["index_error"]["code"], "status_timeout");
        assert_eq!(result["index_error"]["stage"], "observation");
        for key in ["publication", "files", "chunks"] {
            assert!(result[key].is_null());
        }
        assert_eq!(native.calls.lock().unwrap().len(), 1);
        assert!(ui.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn quick_status_forwards_typed_admission_refusal_after_a_deadline_without_evidence() {
        // Slow hosts expire the quick deadline while the native query still
        // holds admission; the next read is refused with a typed retry hint.
        let (u, native, _, ui) = fixture(false, true);
        native
            .stall_status
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let expired = body(&call(&u, "status", json!({})).await).unwrap();
        assert_eq!(expired["index_error"]["code"], "status_timeout");
        native
            .stall_status
            .store(false, std::sync::atomic::Ordering::SeqCst);
        native
            .busy_status
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let refused = call(&u, "status", json!({})).await;
        assert_eq!(refused["isError"], true);
        let hint = body(&refused).unwrap();
        assert_eq!(hint["error"]["code"], "direct_query_admission");
        assert_eq!(hint["error"]["reason"], "busy");
        assert_eq!(hint["error"]["query_submitted"], false);
        assert_eq!(hint["error"]["retryable"], true);
        assert_eq!(hint["error"]["retry_after_ms"], 25);
        for field in ["publication", "files", "chunks", "index_state"] {
            assert!(
                hint.get(field).is_none(),
                "a refusal must not invent {field}"
            );
        }
        native
            .busy_status
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let recovered = body(&call(&u, "status", json!({})).await).unwrap();
        assert_eq!(recovered["index_state"], "available");
        assert_eq!(recovered["project"], "aa".repeat(32));
        assert_eq!(native.calls.lock().unwrap().len(), 3);
        assert!(ui.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn quick_status_deadline_has_no_evidence_and_later_reads_can_recover() {
        let (u, native, _, ui) = fixture(false, true);
        native
            .stall_status
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            call(&u, "status", json!({})),
        )
        .await
        .expect("quick status must retain its bounded deadline");
        let status = body(&result).unwrap();
        assert_eq!(result["isError"], false);
        assert_eq!(status["index_state"], "unknown");
        assert_eq!(status["health"], "unknown");
        assert_eq!(status["project"], "aa".repeat(32));
        assert_eq!(status["index_error"]["code"], "status_timeout");
        assert_eq!(status["index_error"]["retryable"], true);
        for field in ["publication", "files", "chunks"] {
            assert!(status[field].is_null());
        }
        assert_eq!(native.calls.lock().unwrap().len(), 1);
        assert!(ui.calls.lock().unwrap().is_empty());
        native
            .stall_status
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            body(&call(&u, "status", json!({})).await).unwrap()["index_state"],
            "available"
        );
        assert!(ui.calls.lock().unwrap().is_empty());
        let detailed = body(&call(&u, "diagnostics", json!({})).await).unwrap();
        assert_eq!(detailed["index_state"], "available");
        assert!(detailed.get("explorer").is_some());
    }

    #[tokio::test]
    async fn multi_root_requires_explicit_authorized_scope_and_never_follows_ui() {
        let (u, a, b, _) = fixture(true, true);
        for args in [
            json!({"query":"fixture"}),
            json!({"query":"fixture","project":"cc".repeat(32)}),
        ] {
            assert_eq!(call(&u, "search", args).await["isError"], true);
        }
        assert!(a.calls.lock().unwrap().is_empty());
        assert!(b.calls.lock().unwrap().is_empty());
        assert_eq!(
            call(
                &u,
                "search",
                json!({"query":"fixture","project":"bb".repeat(32)})
            )
            .await["isError"],
            false
        );
        assert!(a.calls.lock().unwrap().is_empty());
        assert_eq!(b.calls.lock().unwrap().len(), 1);
        let projects = body(&call(&u, "projects", json!({})).await).unwrap();
        assert!(projects["default_project"].is_null());
        assert_eq!(projects["projects"].as_array().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn drafts_and_view_suggestions_only_reach_the_bound_editor_adapter() {
        let (u, a, _, ui) = fixture(false, true);
        let draft = json!({"schema_version":"1.0","idempotency_key":"one","title":"Review","route":"manage/activity"});
        assert_eq!(
            call(
                &u,
                "suggest_view",
                json!({"kind":"navigation","draft":draft})
            )
            .await["isError"],
            false
        );
        assert_eq!(
            call(
                &u,
                "suggest_view",
                json!({"kind":"navigation","draft":{},"run":true})
            )
            .await["isError"],
            true
        );
        assert!(a.calls.lock().unwrap().is_empty());
        let calls = ui.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, Some("aa".repeat(32)));
        assert_eq!(calls[0].1, "airk_explorer_suggest_navigation");
        assert_eq!(calls[0].2, draft.as_object().unwrap().clone());
    }
    #[tokio::test]
    async fn public_help_is_documentation_and_old_names_remain_callable() {
        let (u, a, _, ui) = fixture(false, true);
        let help = body(&call(&u, "help", json!({"tool":"draft_search"})).await).unwrap();
        assert_eq!(help["tool"], "draft_search");
        let schema: Value = serde_json::from_str(help["content"].as_str().unwrap()).unwrap();
        assert_eq!(schema["name"], "draft_search");
        assert_eq!(
            call(&u, "help", json!({"query":"repair","tool":"search"})).await["isError"],
            true
        );
        call(&u, "help", json!({"query":"repair"})).await;
        assert!(a.calls.lock().unwrap().is_empty());
        assert_eq!(ui.calls.lock().unwrap()[0].1, "airk_explorer_help_search");
        call(&u, "airk_search", json!({"query":"fixture"})).await;
        assert_eq!(a.calls.lock().unwrap()[0].0, "airk_search");
        call(&u, "airk_explorer_ui_status", json!({})).await;
        assert_eq!(ui.calls.lock().unwrap()[1].1, "airk_explorer_ui_status");
        for detail in ["lean", "rich"] {
            let native = call(&u, "help", json!({"tool":"search","reply_detail":detail})).await;
            assert_ne!(native["isError"], true, "{native}");
            let guidance = call(&u, "help", json!({"query":"repair","reply_detail":detail})).await;
            assert_ne!(guidance["isError"], true, "{guidance}");
            assert!(
                !ui.calls
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .2
                    .contains_key("reply_detail")
            );
        }
        assert_eq!(
            call(
                &u,
                "help",
                json!({"query":"repair","reply_detail":"detailed"})
            )
            .await["isError"],
            true
        );
        assert_eq!(
            a.calls.lock().unwrap().len(),
            1,
            "help choices cannot query an index"
        );
    }
    #[tokio::test]
    async fn public_help_preserves_canonical_native_qualified_selectors_and_ui_conflicts() {
        let (u, native, _, ui) = fixture(false, true);
        for topic in ["tools", "TOOLS", "mcp", "MCP"] {
            for tool in ["search", "airk_search"] {
                let args =
                    json!({"topic":topic,"tool":tool,"max_bytes":32768,"response_mode":"full"});
                let actual = call(&u, "help", args).await;
                let expected = serde_json::to_value(crate::help::call_with_catalog(
                    json!({"topic":topic,"tool":"search","max_bytes":32768,"response_mode":"full"}),
                    tools(),
                ))
                .unwrap();
                assert_eq!(actual, expected);
                assert_ne!(actual["isError"], true, "{actual}");
                let value = body(&actual).unwrap();
                let supplied: Value =
                    serde_json::from_str(value["content"].as_str().unwrap()).unwrap();
                assert_eq!(
                    supplied,
                    serde_json::to_value(tools().into_iter().find(|t| t.name == "search").unwrap())
                        .unwrap()
                );
            }
        }
        for args in [
            json!({"topic":"CLI","command":"airk tools list"}),
            json!({"topic":"COMMANDS","command":"tools list"}),
            json!({"topic":"TOOL_SELECTION"}),
            json!({"topic":"overview","command":"tools list"}),
            json!({"topic":"commands","tool":"search"}),
            json!({"command":"tools list","tool":"search"}),
        ] {
            let mut projected = args.clone();
            let expected = if projected.get("tool").is_some() {
                serde_json::to_value(crate::help::call_with_catalog(projected.take(), tools()))
                    .unwrap()
            } else {
                serde_json::to_value(crate::help::call(projected.take())).unwrap()
            };
            assert_eq!(call(&u, "help", args).await, expected);
        }
        let normalized = call(&u, "help", json!({"topic":"INDEX_HEALTH"})).await;
        assert_ne!(normalized["isError"], true);
        assert_eq!(body(&normalized).unwrap()["topic"], "index-health");
        for args in [
            json!({"topic":"TOOLS","tool":"airk_search","max_bytes":32768}),
            json!({"topic":"COMMANDS","command":"tools list"}),
            json!({"topic":"overview","tool":"airk_search"}),
        ] {
            let expected = serde_json::to_value(crate::help::call(args.clone())).unwrap();
            assert_eq!(call(&u, "airk_help", args).await, expected);
        }
        for args in [
            json!({"query":"repair","topic_id":"intro"}),
            json!({"query":"repair","topic":"tools","tool":"search"}),
            json!({"topic_id":"intro","command":"tools list"}),
        ] {
            let rejected = call(&u, "help", args).await;
            assert_eq!(rejected["isError"], true);
            assert_eq!(
                body(&rejected).unwrap()["error"]["code"],
                "INVALID_HELP_SELECTOR"
            );
        }
        assert!(native.calls.lock().unwrap().is_empty());
        assert!(ui.calls.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn tool_help_keeps_byte_budgets_and_resumable_lines_without_native_work() {
        let (u, native, _, _) = fixture(false, false);
        let mut line = 1;
        let mut content = String::new();
        loop {
            let reply = call(
                &u,
                "help",
                json!({"tool":"draft_strategy","max_bytes":4096,"start_line":line}),
            )
            .await;
            assert_ne!(reply["isError"], true);
            let payload = body(&reply).unwrap();
            assert!(serde_json::to_vec(&payload).unwrap().len() <= 4096);
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(payload["content"].as_str().unwrap());
            let Some(next) = payload["next_line"].as_u64() else {
                break;
            };
            assert!(next > line);
            line = next;
        }
        let schema: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(schema["name"], "draft_strategy");
        assert_eq!(
            call(&u, "help", json!({"tool":"search","max_bytes":1})).await["isError"],
            true
        );
        assert!(native.calls.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn native_guidance_retains_executable_cli_and_compatibility_names() {
        let (u, native, _, _) = fixture(false, false);
        let reply = call(&u, "help", json!({"topic":"tools","max_bytes":32768})).await;
        assert_ne!(reply["isError"], true);
        assert!(
            body(&reply).unwrap()["content"]
                .as_str()
                .unwrap()
                .contains("airk_search")
        );
        assert!(native.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn native_help_detail_refuses_its_exact_field_before_native_or_ui_dispatch() {
        let (u, native, _, ui) = fixture(false, true);
        let reply = call(&u, "help", json!({"topic":"workbench","detail":"full"})).await;
        assert_eq!(reply["isError"], true);
        let value = body(&reply).unwrap();
        assert_eq!(value["error"]["code"], "invalid_help_arguments");
        assert_eq!(value["error"]["field"], "detail");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("query or topic_id")
        );
        assert!(native.calls.lock().unwrap().is_empty());
        assert!(ui.calls.lock().unwrap().is_empty());
        assert_ne!(
            call(
                &u,
                "help",
                json!({"topic":"workbench","response_mode":"full"})
            )
            .await["isError"],
            true
        );
        call(&u, "help", json!({"query":"workbench","detail":"full"})).await;
        assert_eq!(ui.calls.lock().unwrap().last().unwrap().2["detail"], "full");
        let schema = serde_json::to_value(
            &tools()
                .into_iter()
                .find(|tool| tool.name == "help")
                .unwrap()
                .input_schema,
        )
        .unwrap();
        assert!(
            schema["properties"]["detail"]["description"]
                .as_str()
                .unwrap()
                .contains("query or topic_id")
        );
    }

    #[test]
    fn public_ui_diagnostics_use_advertised_tools_without_rewriting_user_content() {
        let original = json!({
            "valid":false,"errors":[{"field":"workspace_binding","message":"Read airk_explorer_ui_status again."}],
            "strategy":{"instructions":["Call airk_help"],"steps":[{"op":"airk.status"}]},
            "handoff":{"content":"User-authored airk_explorer_ui_status text"}
        });
        let projected =
            public_ui_response("airk_explorer_validate_draft", success(original.clone()));
        let value = body(&projected).unwrap();
        assert_eq!(
            value["errors"][0]["message"],
            "Read diagnostics (explorer.ui) again."
        );
        assert_eq!(value["strategy"], original["strategy"]);
        assert_eq!(value["handoff"], original["handoff"]);
        assert_eq!(
            serde_json::from_str::<Value>(projected["content"][0]["text"].as_str().unwrap())
                .unwrap(),
            projected["structuredContent"]
        );
        let error = body(&public_ui_response(
            "airk_explorer_suggest_search",
            failure(
                "STALE_BINDING",
                "Call airk_explorer_ui_status or airk_explorer_templates_list.",
            ),
        ))
        .unwrap();
        assert_eq!(
            error["error"]["message"],
            "Call diagnostics (explorer.ui) or list_templates."
        );
        assert_eq!(
            public_diagnostic_text("Unknown field custom_airk_help_suffix"),
            "Unknown field custom_airk_help_suffix"
        );
        let help = json!({"notes":["Call airk_help for CLI guidance."],"body":"Use airk_search in the native CLI."});
        let public_help = body(&public_ui_response(
            "airk_explorer_help_get",
            success(help.clone()),
        ))
        .unwrap();
        assert_eq!(public_help["notes"][0], "Call help for CLI guidance.");
        assert_eq!(public_help["body"], help["body"]);
        let catalog = tools();
        let help = catalog.iter().find(|tool| tool.name == "help").unwrap();
        let schema = serde_json::to_value(&help.input_schema).unwrap();
        assert!(
            !schema["properties"]["tool"]["description"]
                .as_str()
                .unwrap()
                .contains("search or search")
        );
        let draft = catalog
            .iter()
            .find(|tool| tool.name == "draft_search")
            .unwrap();
        let schema = serde_json::to_value(&draft.input_schema).unwrap();
        assert!(
            schema["properties"]["workspace_binding"]["description"]
                .as_str()
                .unwrap()
                .contains("diagnostics.explorer.ui")
        );
    }
}
