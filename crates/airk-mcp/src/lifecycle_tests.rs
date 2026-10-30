// SPDX-License-Identifier: Apache-2.0
use super::*;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

struct Catalog;
impl ServerHandler for Catalog {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("airk", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: crate::unified::tools(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({"client":context.client_info().map(|i|i.name),"protocol":context.protocol_version()}).to_string(),
        )]).into())
    }
}

struct Wire {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    replies: std::collections::BTreeMap<u64, Value>,
    server: rmcp::service::RunningService<RoleServer, StdioLifecycle<Catalog>>,
}
impl Wire {
    fn new() -> Self {
        let (client, server) = tokio::io::duplex(65536);
        let (reader, writer) = tokio::io::split(client);
        Self {
            reader: BufReader::new(reader),
            writer,
            replies: Default::default(),
            server: rmcp::service::serve_directly(StdioLifecycle::new(Catalog), server, None),
        }
    }
    async fn send(&mut self, id: u64, method: &str, params: Value) {
        let request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.writer
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
    }
    async fn reply(&mut self, id: u64) -> Value {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Some(reply) = self.replies.remove(&id) {
                    return reply;
                }
                let mut line = String::new();
                assert!(self.reader.read_line(&mut line).await.unwrap() > 0);
                let reply: Value = serde_json::from_str(&line).unwrap();
                self.replies.insert(reply["id"].as_u64().unwrap(), reply);
            }
        })
        .await
        .expect("bounded protocol exchange")
    }
    async fn catalog(&mut self, id: u64, params: Value) {
        self.send(id, "tools/list", params).await;
        let reply = self.reply(id).await;
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["result"]["tools"].as_array().unwrap().len(), 28);
    }
    async fn client(&mut self, id: u64, params: Value) -> Value {
        self.send(id, "tools/call", params).await;
        let reply = self.reply(id).await;
        assert!(reply.get("error").is_none(), "{reply}");
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}
fn initialize(client: &str, version: &str) -> Value {
    json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":client,"version":"fixture"}})
}
fn metadata(client: &str) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{},
        "io.modelcontextprotocol/clientInfo":{"name":client,"version":"fixture"}})
}

#[tokio::test]
async fn discovery_then_pipelined_initialize_keeps_legacy_requests_working() {
    for version in ["2024-11-05", "2025-03-26", "2025-11-25"] {
        let mut wire = Wire::new();
        // The fallback is already buffered when the slow-starting server sees
        // discovery. No sleep/race is needed to reproduce the 1.5.8 failure.
        wire.send(1, "server/discover", json!({"_meta":metadata("probe")}))
            .await;
        wire.send(2, "initialize", initialize("fallback-client", version))
            .await;
        assert_eq!(wire.reply(2).await["result"]["protocolVersion"], version);
        assert!(wire.reply(1).await["result"]["supportedVersions"].is_array());
        wire.catalog(3, json!({})).await;
        wire.catalog(4, json!({"_meta":{"progressToken":"legacy-progress"}}))
            .await;
        wire.send(5, "ping", json!({})).await;
        assert!(wire.reply(5).await.get("error").is_none());
        // A later discovery request does not overwrite the negotiated peer.
        wire.send(
            6,
            "server/discover",
            json!({"_meta":metadata("later-probe")}),
        )
        .await;
        assert!(wire.reply(6).await.get("error").is_none());
        assert_eq!(
            wire.client(7, json!({"name":"projects","arguments":{}}))
                .await,
            json!({"client":"fallback-client","protocol":version})
        );
        wire.server.close().await.unwrap();
    }
}

#[tokio::test]
async fn legacy_and_inline_connections_remain_independent_when_used_concurrently() {
    let mut legacy = Wire::new();
    let mut fallback = Wire::new();
    let mut modern = Wire::new();
    legacy
        .send(1, "initialize", initialize("codex", "2025-11-25"))
        .await;
    fallback
        .send(
            1,
            "server/discover",
            json!({"_meta":metadata("claude-code")}),
        )
        .await;
    modern
        .send(
            1,
            "server/discover",
            json!({"_meta":metadata("github-copilot")}),
        )
        .await;
    fallback
        .send(2, "initialize", initialize("claude-code", "2025-11-25"))
        .await;
    assert!(legacy.reply(1).await.get("error").is_none());
    assert!(fallback.reply(2).await.get("error").is_none());
    assert!(modern.reply(1).await.get("error").is_none());
    for id in 3..6 {
        tokio::join!(
            legacy.catalog(id, json!({})),
            fallback.catalog(id, json!({})),
            modern.catalog(id, json!({"_meta":metadata("github-copilot")}))
        );
    }
    assert_eq!(
        legacy
            .client(6, json!({"name":"projects","arguments":{}}))
            .await["client"],
        "codex"
    );
    assert_eq!(
        fallback
            .client(6, json!({"name":"projects","arguments":{}}))
            .await["client"],
        "claude-code"
    );
    assert_eq!(
        modern
            .client(
                6,
                json!({"name":"projects","arguments":{},"_meta":metadata("github-copilot")})
            )
            .await,
        json!({"client":"github-copilot","protocol":"2026-07-28"})
    );
    modern.send(7, "tools/list", json!({})).await;
    assert_eq!(
        modern.reply(7).await["error"]["code"],
        -32602,
        "another connection's initialize cannot authorize missing metadata"
    );
    legacy.server.close().await.unwrap();
    fallback.server.close().await.unwrap();
    modern.server.close().await.unwrap();
}

#[tokio::test]
async fn metadata_validation_and_negotiation_survive_bad_requests() {
    let mut wire = Wire::new();
    wire.send(1, "ping", json!({})).await;
    assert!(wire.reply(1).await.get("error").is_none());
    wire.send(2, "tools/list", json!({})).await;
    assert_eq!(wire.reply(2).await["error"]["code"], -32602);
    wire.send(3, "initialize", initialize("old-client", "2099-01-01"))
        .await;
    assert_eq!(
        wire.reply(3).await["result"]["protocolVersion"],
        "2025-11-25"
    );
    for (id, bad) in [
        (
            4,
            json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28"}),
        ),
        (
            5,
            json!({"io.modelcontextprotocol/protocolVersion":4,"io.modelcontextprotocol/clientCapabilities":{}}),
        ),
        (
            6,
            json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":"invalid"}),
        ),
    ] {
        wire.send(id, "tools/list", json!({"_meta":bad})).await;
        assert_eq!(wire.reply(id).await["error"]["code"], -32602);
    }
    let mut unsupported = metadata("future-client");
    unsupported["io.modelcontextprotocol/protocolVersion"] = json!("2099-01-01");
    wire.send(7, "tools/list", json!({"_meta":unsupported}))
        .await;
    assert!(wire.reply(7).await.get("error").is_some());
    wire.send(8, "initialize", initialize("replacement", "2024-11-05"))
        .await;
    assert_eq!(wire.reply(8).await["error"]["code"], -32600);
    wire.catalog(9, json!({})).await;
    assert_eq!(
        wire.client(10, json!({"name":"projects","arguments":{}}))
            .await,
        json!({"client":"old-client","protocol":"2025-11-25"})
    );
    wire.server.close().await.unwrap();
}
