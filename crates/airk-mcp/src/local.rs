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
    kernel: Kernel,
    family: ClientFamily,
    owns_admission: bool,
    responses: std::sync::Arc<crate::response::Session>,
    client: RunningService<RoleClient, ()>,
    server: RunningService<RoleServer, AirkServer>,
    admission: crate::execution::Admission,
}

impl LocalTools {
    pub async fn new(kernel: Kernel) -> anyhow::Result<Self> {
        Self::with_client(kernel, ClientFamily::Cli).await
    }

    pub async fn with_client(kernel: Kernel, client: ClientFamily) -> anyhow::Result<Self> {
        Self::with_admission(kernel, client, Default::default(), true).await
    }

    async fn with_admission(
        kernel: Kernel,
        family: ClientFamily,
        admission: crate::execution::Admission,
        owns_admission: bool,
    ) -> anyhow::Result<Self> {
        let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
        let mut server = AirkServer::new(kernel.clone());
        server.client = family;
        server.admission = admission.clone();
        let responses = server.responses.clone();
        let (client, server) = tokio::try_join!(
            async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
            async { Ok::<_, anyhow::Error>(server.serve(server_transport).await?) }
        )?;
        Ok(Self {
            kernel,
            family,
            owns_admission,
            responses,
            client,
            server,
            admission,
        })
    }

    /// A direct client owns its response observations, while native work still
    /// shares the owning worker's finite admission budget.
    pub async fn session(&self) -> anyhow::Result<Self> {
        Self::with_admission(
            self.kernel.clone(),
            self.family,
            self.admission.clone(),
            false,
        )
        .await
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

    pub fn status_receipt(&self) -> crate::response::StatusReceipt {
        self.responses.receipt()
    }

    pub fn restore_status_receipt(&self, receipt: crate::response::StatusReceipt) {
        self.responses.restore(receipt);
    }

    pub async fn close(&mut self) -> anyhow::Result<()> {
        let client_result = self.client.close().await;
        let server_result = self.server.close().await;
        // The project owner drains global admission after all client sessions;
        // closing one client must not close admission for independent clients.
        if self.owns_admission {
            self.admission.shutdown().await;
        }
        client_result?;
        server_result?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(value: CallToolResult) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(!value.is_error.unwrap_or(false), "{value:?}");
        Ok(serde_json::from_str(
            &value.content[0].as_text().unwrap().text,
        )?)
    }
    #[tokio::test]
    async fn client_sessions_keep_status_separate_and_closing_one_preserves_other_admission()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() -> bool { true }\n")?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut owner = LocalTools::new(kernel).await?;
        let mut first = owner.session().await?;
        let mut second = owner.session().await?;
        let args = serde_json::json!({"name":"alpha","response_mode":"terse"})
            .as_object()
            .unwrap()
            .clone();
        let initial = body(first.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(initial.get("freshness").is_some());
        let repeated = body(first.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(repeated.get("freshness").is_none());
        let independent = body(second.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(independent.get("freshness").is_some());
        assert_eq!(
            initial["publication"]["token"],
            independent["publication"]["token"]
        );
        first.close().await?;
        let mut requested = args.clone();
        requested.insert("include_status".into(), true.into());
        let requested = body(second.call("airk_code_facts".into(), requested).await?)?;
        assert!(requested.get("freshness").is_some());
        let mut full = args;
        full.insert("response_mode".into(), "full".into());
        let full = body(second.call("airk_code_facts".into(), full).await?)?;
        assert!(full["facts"][0]["code"].get("producer").is_some());
        assert!(full.get("freshness").is_some());
        assert_eq!(
            initial["publication"]["token"],
            full["publication"]["token"]
        );
        let mut validators = Vec::new();
        for mode in ["terse", "full"] {
            let args = serde_json::json!({"path":"one.rs","start_line":1,"end_line":1,"response_mode":mode}).as_object().unwrap().clone();
            let read = body(second.call("airk_read_file".into(), args).await?)?;
            assert!(read["read"]["validator"].is_string());
            validators.push(read["read"]["validator"].clone());
            assert_eq!(read["hits"][0]["text"], "fn alpha() -> bool { true }\n");
        }
        assert_eq!(validators[0], validators[1]);
        second.close().await?;
        owner.close().await?;
        Ok(())
    }

    #[test]
    fn every_tool_advertises_response_controls_and_full_mode_guidance() {
        for tool in tools() {
            let schema = serde_json::to_value(&tool.input_schema).unwrap();
            assert!(
                schema["properties"].get("response_mode").is_some(),
                "{}",
                tool.name
            );
            assert!(
                schema["properties"].get("include_status").is_some(),
                "{}",
                tool.name
            );
            assert!(tool.description.unwrap().contains("response_mode=full"));
        }
    }
}
