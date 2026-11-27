// SPDX-License-Identifier: Apache-2.0
//! U105/U110 (`docs/split/open-agent-access.md` §7): the HTTPS backend of the
//! built-in local agent session. `airk-https` cannot depend on `airk-service`,
//! so the service injects [`OpenHooks`]: the open-read path (`OpenReader`, the
//! same function M3 `ToolSelection::Open` reaches). Nothing is captured at
//! connect time: every call names its scope, or its workspace, and reads
//! through the service.
use airk_core::{service::ControlResponse, usage::ClientFamily};
use airk_mcp::remote::{RemoteFuture, RemoteTools, Tool, http::AuthenticatedBackendFactory};
use serde_json::{Map, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

/// One open read as the HTTPS backend hands it to the service: the call's
/// selectors removed from its arguments and passed as names.
pub struct OpenRead {
    pub index_names: Vec<String>,
    pub set_names: Vec<String>,
    /// U110: the caller's workspace folder (explicit, or filled from the
    /// client's MCP roots), removed from the arguments.
    pub workspace: Option<String>,
    pub tool: String,
    pub arguments: Map<String, Value>,
    pub client: ClientFamily,
}

/// The service's open-read path.
pub type OpenReader =
    Arc<dyn Fn(OpenRead) -> Pin<Box<dyn Future<Output = ControlResponse> + Send>> + Send + Sync>;

/// The service-injected hooks of the local agent session.
#[derive(Clone)]
pub struct OpenHooks {
    pub reader: OpenReader,
}

/// The session factory of the local agent session.
pub fn open_factory(hooks: OpenHooks) -> AuthenticatedBackendFactory {
    Arc::new(move || {
        Ok(Arc::new(OpenBackend {
            hooks: hooks.clone(),
            client: Mutex::new(ClientFamily::Unknown),
            responses: airk_mcp::response::OpenStatusSession::default(),
        }) as Arc<dyn RemoteTools>)
    })
}

struct OpenBackend {
    hooks: OpenHooks,
    client: Mutex<ClientFamily>,
    responses: airk_mcp::response::OpenStatusSession,
}

/// One selector argument: absent, or an array of strings. Any other shape
/// refuses the call before it reaches the service.
fn selector(arguments: &mut Map<String, Value>, key: &str) -> Option<Vec<String>> {
    match arguments.remove(key) {
        None => Some(Vec::new()),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(|item| match item {
                Value::String(name) => Some(name),
                _ => None,
            })
            .collect(),
        Some(_) => None,
    }
}

/// The MCP envelope of a service answer: a result as is; a refusal as a
/// tool error carrying the same stable code the CLI reports.
fn envelope(response: ControlResponse) -> Value {
    match response {
        ControlResponse::ToolResult { result } => result,
        ControlResponse::Failure { code } => {
            airk_mcp::unified::failure(&code, &format!("Open read refused: {code}."))
        }
        ControlResponse::ToolCallFailed { message } => {
            airk_mcp::unified::failure("open_read_failed", &message)
        }
        _ => airk_mcp::unified::failure(
            "open_read_unavailable",
            "The AIRK service gave no open read answer.",
        ),
    }
}

impl RemoteTools for OpenBackend {
    fn catalog(&self) -> Vec<Tool> {
        airk_mcp::open::local_catalog()
    }
    /// U110: the local agent session's reads and list take `workspace`,
    /// which the bridge may fill from the client's MCP roots.
    fn fills_workspace(&self, tool: &str) -> bool {
        airk_mcp::open::takes_workspace(tool)
    }
    /// Help is answered by the service against this connection's catalog.
    fn local_help(&self) -> bool {
        false
    }
    /// The list tool takes only its cursor and workspace.
    fn uses_response_mode(&self, tool: &str) -> bool {
        // query_scope's schema takes no response_mode (as on every backend).
        !matches!(tool, "query_scope" | "airk_query_scope") && tool != airk_mcp::open::LIST_SCOPES
    }
    fn client(&self, client: ClientFamily) {
        *self
            .client
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = client;
    }
    fn call(&self, tool: String, mut arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            let (Some(index_names), Some(set_names)) = (
                selector(&mut arguments, "index_ids"),
                selector(&mut arguments, "index_set_ids"),
            ) else {
                return Ok(airk_mcp::unified::failure(
                    "INVALID_SCHEMA",
                    "index_ids and index_set_ids are arrays of ids or exact names.",
                ));
            };
            let workspace = match arguments.remove("workspace") {
                None => None,
                Some(Value::String(path))
                    if airk_core::agent_workspace::validate_path(&path).is_ok() =>
                {
                    Some(path)
                }
                Some(_) => {
                    return Ok(airk_mcp::unified::failure(
                        "INVALID_SCHEMA",
                        "workspace is an absolute folder path.",
                    ));
                }
            };
            let client = *self
                .client
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let response_tool = tool.clone();
            let response_arguments = arguments.clone();
            let response = (self.hooks.reader)(OpenRead {
                index_names,
                set_names,
                workspace,
                tool,
                arguments,
                client,
            })
            .await;
            let mut result = envelope(response);
            self.responses
                .display(&response_tool, &response_arguments, &mut result);
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    type Seen = Arc<Mutex<Vec<(Vec<String>, Vec<String>, String, Map<String, Value>)>>>;

    fn hooks(seen: Seen, answer: ControlResponse) -> OpenHooks {
        // `ControlResponse` is not `Clone`; each call answers a decoded copy.
        let answer = serde_json::to_value(answer).unwrap();
        OpenHooks {
            reader: Arc::new(move |read: OpenRead| {
                seen.lock().unwrap().push((
                    read.index_names,
                    read.set_names,
                    read.tool,
                    read.arguments,
                ));
                let answer: ControlResponse = serde_json::from_value(answer.clone()).unwrap();
                Box::pin(async move { answer })
            }),
        }
    }
    fn names(tools: &[Tool]) -> Vec<String> {
        tools.iter().map(|tool| tool.name.to_string()).collect()
    }

    #[tokio::test]
    async fn the_local_session_offers_the_local_catalog() -> anyhow::Result<()> {
        let backend = open_factory(hooks(Seen::default(), ControlResponse::Stopping))()?;
        assert_eq!(
            names(&backend.catalog()),
            names(&airk_mcp::open::local_catalog())
        );
        assert!(!backend.local_help());
        assert!(!backend.uses_response_mode("list_scopes"));
        assert!(backend.uses_response_mode("search_passages"));
        assert!(!backend.uses_response_mode("query_scope"));
        Ok(())
    }

    #[tokio::test]
    async fn calls_pass_selectors_as_names_and_keep_service_codes() -> anyhow::Result<()> {
        let seen = Seen::default();
        let result = json!({"content":[],"structuredContent":{"ok":true},"isError":false});
        let backend = open_factory(hooks(
            seen.clone(),
            ControlResponse::ToolResult {
                result: result.clone(),
            },
        ))()?;
        let arguments = json!({"query":"q","index_ids":["docs","idx_01010101010101010101010101010101"],
            "index_set_ids":["web"]});
        let answer = backend
            .call(
                "search_passages".into(),
                arguments.as_object().unwrap().clone(),
            )
            .await?;
        assert_eq!(answer, result);
        let calls = seen.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![(
                vec![
                    "docs".to_owned(),
                    "idx_01010101010101010101010101010101".to_owned()
                ],
                vec!["web".to_owned()],
                "search_passages".to_owned(),
                json!({"query":"q"}).as_object().unwrap().clone(),
            )]
        );
        // A malformed selector never reaches the service.
        for bad in [json!({"index_ids":"docs"}), json!({"index_set_ids":[1]})] {
            let answer = backend
                .call("search_passages".into(), bad.as_object().unwrap().clone())
                .await?;
            assert_eq!(
                answer["structuredContent"]["error"]["code"],
                "INVALID_SCHEMA"
            );
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
        // A refusal keeps the service's stable code.
        let refusing = open_factory(hooks(
            Seen::default(),
            ControlResponse::Failure {
                code: "scope_required".into(),
            },
        ))()?;
        let answer = refusing.call("search_passages".into(), Map::new()).await?;
        assert_eq!(answer["isError"], true);
        assert_eq!(
            answer["structuredContent"]["error"]["code"],
            "scope_required"
        );
        Ok(())
    }

    #[tokio::test]
    async fn open_status_memory_belongs_to_each_http_backend_session() -> anyhow::Result<()> {
        let body = json!({"collection":"one",
            "publication":{"token":"airk-r1:01010101010101010101010101010101:1:0202020202020202020202020202020202020202020202020202020202020202"},
            "freshness":{"state":"idle","observed_unix_ms":1},
            "semantic_readiness":{"state":"ready"},
            "scope_observation":{"schema":1,"qualifier":"selected_subset",
                "fingerprint":"0303030303030303030303030303030303030303030303030303030303030303",
                "selection":{"owner":"agent","scope_identity":"0404040404040404040404040404040404040404040404040404040404040404","catalog_revision":1},
                "members":[{"index_id":"idx_01010101010101010101010101010101","memberships":[],
                    "source_root_ids":[],"source_revision":1,"policy_epoch":0,"policy_digest":null}]},
            "read_scope":{"index_ids":["idx_01010101010101010101010101010101"],"source":"explicit"},
            "hits":[{"path":"native.md","text":"unchanged evidence"}]});
        let result = airk_mcp::unified::success(body.clone());
        let factory = open_factory(hooks(
            Seen::default(),
            ControlResponse::ToolResult {
                result: result.clone(),
            },
        ));
        let first = factory()?;
        let second = factory()?;
        let args = json!({"index_ids":["one"]}).as_object().unwrap().clone();
        assert_eq!(
            first.call("search_passages".into(), args.clone()).await?,
            result
        );
        let repeated = first.call("search_passages".into(), args.clone()).await?;
        let mut expected = body.clone();
        expected.as_object_mut().unwrap().remove("freshness");
        expected
            .as_object_mut()
            .unwrap()
            .remove("semantic_readiness");
        assert_eq!(repeated, airk_mcp::unified::success(expected));
        // The same built-in service connection does not share observations
        // between independently initialized MCP protocol sessions.
        assert_eq!(
            second.call("search_passages".into(), args.clone()).await?,
            result
        );
        let mut explicit = args.clone();
        explicit.insert("include_status".into(), true.into());
        assert_eq!(
            first.call("search_passages".into(), explicit).await?,
            result
        );
        let mut full = args.clone();
        full.insert("response_mode".into(), "full".into());
        assert_eq!(first.call("search_passages".into(), full).await?, result);
        assert_eq!(first.call("index_status".into(), args).await?, result);
        Ok(())
    }
}
