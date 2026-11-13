// SPDX-License-Identifier: Apache-2.0
//! U105/U106 (`docs/split/open-agent-access.md` §7): the HTTPS backend of an
//! open agent connection. `airk-https` cannot depend on `airk-service`, so the
//! service injects [`OpenHooks`]: the open-read path (`OpenReader`, the same
//! function M3 `ToolSelection::Open` reaches) and a reader of the connection's
//! current agent choice. Nothing is captured at connect time: `catalog()`
//! reads the choice on every `tools/list`, and every call reads the stored
//! defaults through the service, so a defaults or choice change applies to
//! the next request on the same session (the credential generation, and so
//! the gateway's cached service, never changes).
use airk_core::{
    identity::Incarnation, service::ControlResponse, service_http_credentials::AgentChoice,
    usage::ClientFamily,
};
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
    pub connection_id: Incarnation,
    pub index_names: Vec<String>,
    pub set_names: Vec<String>,
    pub tool: String,
    pub arguments: Map<String, Value>,
    pub client: ClientFamily,
}

/// The service's open-read path.
pub type OpenReader =
    Arc<dyn Fn(OpenRead) -> Pin<Box<dyn Future<Output = ControlResponse> + Send>> + Send + Sync>;
/// The connection's current agent choice (`None`: not an active open
/// connection), read from the live credential store without disk I/O.
pub type OpenChoice = Arc<dyn Fn(Incarnation) -> Option<AgentChoice> + Send + Sync>;

/// The service-injected hooks of every open connection.
#[derive(Clone)]
pub struct OpenHooks {
    pub reader: OpenReader,
    pub choice: OpenChoice,
}

/// The session factory of one authenticated open connection.
pub fn open_factory(connection_id: Incarnation, hooks: OpenHooks) -> AuthenticatedBackendFactory {
    Arc::new(move || {
        Ok(Arc::new(OpenBackend {
            connection_id,
            hooks: hooks.clone(),
            client: Mutex::new(ClientFamily::Unknown),
        }) as Arc<dyn RemoteTools>)
    })
}

struct OpenBackend {
    connection_id: Incarnation,
    hooks: OpenHooks,
    client: Mutex<ClientFamily>,
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
        airk_mcp::open::catalog((self.hooks.choice)(self.connection_id).unwrap_or_default())
    }
    /// Help is answered by the service against this connection's catalog.
    fn local_help(&self) -> bool {
        false
    }
    /// The list tool takes only its cursor.
    fn uses_response_mode(&self, tool: &str) -> bool {
        tool != airk_mcp::open::LIST_INDEXES
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
            let client = *self
                .client
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let response = (self.hooks.reader)(OpenRead {
                connection_id: self.connection_id,
                index_names,
                set_names,
                tool,
                arguments,
                client,
            })
            .await;
            Ok(envelope(response))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    type Seen = Arc<Mutex<Vec<(Vec<String>, Vec<String>, String, Map<String, Value>)>>>;

    fn hooks(
        choice: Arc<Mutex<Option<AgentChoice>>>,
        seen: Seen,
        answer: ControlResponse,
    ) -> OpenHooks {
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
            choice: Arc::new(move |_| *choice.lock().unwrap()),
        }
    }
    fn names(tools: &[Tool]) -> Vec<String> {
        tools.iter().map(|tool| tool.name.to_string()).collect()
    }

    #[tokio::test]
    async fn the_catalog_follows_the_current_choice_without_a_new_session() -> anyhow::Result<()> {
        let choice = Arc::new(Mutex::new(Some(AgentChoice::Off)));
        let seen = Seen::default();
        let factory = open_factory(
            Incarnation::from_bytes([3; 16]),
            hooks(choice.clone(), seen, ControlResponse::Stopping),
        );
        let backend = factory()?;
        assert_eq!(
            names(&backend.catalog()),
            names(&airk_mcp::open::catalog(AgentChoice::Off))
        );
        *choice.lock().unwrap() = Some(AgentChoice::On);
        assert_eq!(
            names(&backend.catalog()),
            names(&airk_mcp::open::catalog(AgentChoice::On))
        );
        // A revoked or unknown connection offers the choice-off catalog.
        *choice.lock().unwrap() = None;
        assert!(!names(&backend.catalog()).contains(&"list_indexes".to_owned()));
        assert!(!backend.local_help());
        assert!(!backend.uses_response_mode("list_indexes"));
        assert!(backend.uses_response_mode("search"));
        Ok(())
    }

    #[tokio::test]
    async fn calls_pass_selectors_as_names_and_keep_service_codes() -> anyhow::Result<()> {
        let choice = Arc::new(Mutex::new(Some(AgentChoice::On)));
        let seen = Seen::default();
        let result = json!({"content":[],"structuredContent":{"ok":true},"isError":false});
        let backend = open_factory(
            Incarnation::from_bytes([3; 16]),
            hooks(
                choice.clone(),
                seen.clone(),
                ControlResponse::ToolResult {
                    result: result.clone(),
                },
            ),
        )()?;
        let arguments = json!({"query":"q","index_ids":["docs","idx_01010101010101010101010101010101"],
            "index_set_ids":["web"]});
        let answer = backend
            .call("search".into(), arguments.as_object().unwrap().clone())
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
                "search".to_owned(),
                json!({"query":"q"}).as_object().unwrap().clone(),
            )]
        );
        // A malformed selector never reaches the service.
        for bad in [json!({"index_ids":"docs"}), json!({"index_set_ids":[1]})] {
            let answer = backend
                .call("search".into(), bad.as_object().unwrap().clone())
                .await?;
            assert_eq!(
                answer["structuredContent"]["error"]["code"],
                "INVALID_SCHEMA"
            );
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
        // A refusal keeps the service's stable code.
        let refusing = open_factory(
            Incarnation::from_bytes([3; 16]),
            hooks(
                choice,
                Seen::default(),
                ControlResponse::Failure {
                    code: "agent_scope_choice_disabled".into(),
                },
            ),
        )()?;
        let answer = refusing.call("search".into(), Map::new()).await?;
        assert_eq!(answer["isError"], true);
        assert_eq!(
            answer["structuredContent"]["error"]["code"],
            "agent_scope_choice_disabled"
        );
        Ok(())
    }
}
