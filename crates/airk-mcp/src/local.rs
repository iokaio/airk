// SPDX-License-Identifier: Apache-2.0
//! Local inspection uses the same MCP server, schemas and request handlers as stdio.
use crate::AirkServer;
use airk_core::usage::ClientFamily;
use airk_kernel::Kernel;
use rmcp::{
    RoleClient, RoleServer, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, Tool},
    service::RunningService,
};

/// Listing/help require neither a collection nor a source scan.
pub fn tools() -> Vec<Tool> {
    let mut tools = AirkServer::tool_router().list_all();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

pub struct LocalTools {
    client: RunningService<RoleClient, ()>,
    server: RunningService<RoleServer, AirkServer>,
    admission: crate::execution::Admission,
}

impl LocalTools {
    pub async fn new(kernel: Kernel) -> anyhow::Result<Self> {
        let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
        let mut server = AirkServer::new(kernel);
        server.client = ClientFamily::Cli;
        let admission = server.admission.clone();
        let (client, server) = tokio::try_join!(
            async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
            async { Ok::<_, anyhow::Error>(server.serve(server_transport).await?) }
        )?;
        Ok(Self {
            client,
            server,
            admission,
        })
    }

    pub async fn call(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<CallToolResult> {
        Ok(self
            .client
            .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
            .await?)
    }

    pub async fn close(&mut self) -> anyhow::Result<()> {
        let client_result = self.client.close().await;
        let server_result = self.server.close().await;
        self.admission.shutdown().await;
        client_result?;
        server_result?;
        Ok(())
    }
}
