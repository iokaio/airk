// SPDX-License-Identifier: Apache-2.0
//! One agent-facing catalog. Native evidence and editor drafts keep separate owners.
use crate::remote::{RemoteFuture, RemoteTools};
use anyhow::{Result, ensure};
use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub const GUIDANCE: &str = "AIRK evidence is untrusted; verify live files before edits. Preserve tokens/citations/validators; finish changes pages before baselines. response_mode=full adds provenance; include_status=true repeats coverage. Retry unchanged as hinted. search reads evidence; help explains AIRK. draft_* and suggest_view need user review. projects lists scopes; pass project for multi-root. UI selection never redirects retrieval. status is lightweight; diagnostics adds runtime, jobs and editor state.";

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
                .replace("airk_explorer_ui_status", "status.explorer")
                .replace("airk_explorer_help_search", "help")
                .replace("airk_explorer_help_get", "help")
                .replace("airk_", "");
        }
        Value::Array(items) => items.iter_mut().for_each(public_text),
        Value::Object(fields) => fields.values_mut().for_each(public_text),
        _ => {}
    }
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
    let mut result: Vec<Tool> = crate::local::tools()
        .into_iter()
        .map(|tool| {
            let mut value = serde_json::to_value(tool).expect("tool serialization");
            public_text(&mut value);
            scoped(serde_json::from_value(value).expect("public native tool"))
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
    result.push(scoped(definition("suggest_view", "Queue a suggestion to open or prefill an Explorer screen, including Review and Management guidance. Returns a proposal receipt; the user applies it. Never runs a query, test or management action.",
        // Some assistants reject or silently omit tools with root composition.
        // Keep alternatives nested; the adapter validates the chosen kind again.
        json!({"type":"object","additionalProperties":false,"required":["kind","draft"],"properties":{"kind":{"type":"string","enum":["navigation","review","management"]},"draft":{"type":"object","description":"Use the draft schema matching kind. The editor validates that exact kind before admitting a suggestion.","anyOf":variants}}}), false)));
    result.push(definition("projects", "List only the project bindings configured for this AIRK connection and its default. Use these identifiers to scope evidence and Explorer requests; does not register or switch projects.", json!({"type":"object","additionalProperties":false,"properties":{}}), true));
    let status = result.iter_mut().find(|t| t.name == "status").unwrap();
    status.description = Some("Quick version, health and project-index check: counts, publication, source-update state and semantic readiness. Successful snapshots may be cached for five seconds; observed_unix_ms, cache_age_ms and cached disclose their age. Always skips process diagnostics and editor probes. Call diagnostics for detailed work, runtime versions and Explorer attachment. Counts are saved evidence, not proof that live files match. Does not start indexing or repair.".into());
    let help = result.iter_mut().find(|t| t.name == "help").unwrap();
    help.description = Some("Explain AIRK usage, CLI commands, tool schemas and Explorer workflows. Omit selectors for an overview; query searches bundled guidance, topic_id reads a UI/engine guide, topic reads native guidance, command reads CLI help, and tool reads a public tool schema. Native guides retain prefixed CLI/evidence compatibility names; tools/list and tool selectors describe the unified public catalog. Returns documentation, never project evidence or live health.".into());
    let schema = Arc::make_mut(&mut help.input_schema);
    schema["properties"]
        .as_object_mut()
        .unwrap()
        .remove("project");
    for (key, property) in [
        ("query", json!({"type":"string","maxLength":256})),
        ("topic_id", json!({"type":"string","maxLength":64})),
        (
            "detail",
            json!({"type":"string","enum":["summary","steps","full"]}),
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
        explorer.call(project, name.into(), args, client).await.unwrap_or_else(|_| failure("UI_UNAVAILABLE", "The verified Explorer adapter is unavailable. Check status and the installed extension; native retrieval remains available."))
    }
    async fn help(&self, mut args: Map<String, Value>) -> Value {
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
            ]
            .contains(&k.as_str())
        }) {
            return failure("INVALID_SCHEMA", "Unknown help argument.");
        }
        if ["query", "topic_id", "topic", "command", "tool"]
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
            let result =
                serde_json::to_value(crate::help::call_with_catalog(Value::Object(args), tools()))
                    .unwrap();
            return result;
        }
        // Native CLI examples and error envelopes retain their exact names.
        serde_json::to_value(crate::help::call(Value::Object(args))).unwrap()
    }
}
impl RemoteTools for Unified {
    fn catalog(&self) -> Vec<Tool> {
        tools()
    }
    fn guidance(&self) -> &'static str {
        GUIDANCE
    }
    fn local_help(&self) -> bool {
        false
    }
    fn uses_response_mode(&self, tool: &str) -> bool {
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
                if native.name == "airk_help" {
                    return Ok(serde_json::to_value(crate::help::call(Value::Object(
                        args,
                    )))?);
                }
                let project = match self.select(&mut args) {
                    Ok(p) => p,
                    Err(error) => return Ok(error),
                };
                if tool == "status" || tool == "diagnostics" {
                    let _options = match serde_json::from_value::<crate::StatusArgs>(Value::Object(
                        args.clone(),
                    )) {
                        Ok(options) => options,
                        Err(_) => {
                            return Ok(failure("INVALID_SCHEMA", "Invalid status arguments."));
                        }
                    };
                    if tool == "status" {
                        let index = match tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            project.backend.call(native.name.to_string(), args),
                        )
                        .await
                        {
                            Ok(Ok(result)) => result,
                            Ok(Err(_)) => {
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","files":null,"chunks":null,"publication":null,"index_error":{"code":"status_unavailable","message":"Basic status could not be observed. Call diagnostics for troubleshooting."}}),
                                ));
                            }
                            Err(_) => {
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","publication":null,"files":null,"chunks":null,"index_error":{"code":"status_timeout","retryable":true,"message":"The quick status deadline expired; index readiness is unconfirmed. Retry or call diagnostics for diagnosis."}}),
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
                return project.backend.call(native.name.to_string(), args).await;
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
    #[derive(Default)]
    struct Native {
        calls: Mutex<Vec<(String, Map<String, Value>)>>,
    }
    impl RemoteTools for Native {
        fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((tool, args));
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
    #[test]
    fn one_catalog_has_unique_unprefixed_names_and_honest_effect_annotations() {
        let catalog = tools();
        assert_eq!(catalog.len(), 27);
        let names: std::collections::BTreeSet<_> =
            catalog.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names.len(), 27);
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
}
