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
        let admission = crate::execution::Admission::for_kernel(&kernel);
        Self::with_admission(kernel, client, admission, true).await
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
        full.insert("compact".into(), false.into());
        let full = body(second.call("airk_code_facts".into(), full).await?)?;
        assert!(full["items"][0]["code"].get("producer").is_some());
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

    #[tokio::test]
    async fn retained_changes_page_across_response_modes_with_index_current_validation()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() -> u32 { 1 }\n")?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let baseline = kernel.status()?.publication.unwrap().token;
        for value in [2, 3] {
            std::fs::write(
                root.join("one.rs"),
                format!("fn alpha() -> u32 {{ {value} }}\n"),
            )?;
            kernel.index()?;
        }
        let mut tools = LocalTools::new(kernel).await?;
        let mut args = serde_json::json!({"since":baseline,"selectors":[{"kind":"path","path":"one.rs"}],"limit":1,"max_bytes":65536,"response_mode":"full"}).as_object().unwrap().clone();
        let first = body(tools.call("airk_changes".into(), args.clone()).await?)?;
        assert_eq!(first["baseline_status"], "verified");
        assert_eq!(first["changes"][0]["observations"], 2);
        assert!(first["cursor"].is_string());
        args.insert("cursor".into(), first["cursor"].clone());
        args.insert("response_mode".into(), "terse".into());
        let second = body(tools.call("airk_changes".into(), args).await?)?;
        assert_eq!(second["validations"][0]["outcome"], "index-current");
        assert_eq!(
            first["publication"]["token"],
            second["publication"]["token"]
        );
        assert_eq!(second["truncated"], false);
        assert_eq!(second["freshness"]["live_source_verified"], false);
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn terminal_call_names_keep_written_targets_and_candidate_labels() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(
            root.join("one.py"),
            "def run(x):\n    x.next().reclaim_objects()\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        for mode in ["terse", "full"] {
            for compact in [true, false] {
                let args = serde_json::json!({"name":"reclaim_objects","kind":"call","compact":compact,"response_mode":mode});
                let result = body(
                    tools
                        .call("airk_code_facts".into(), args.as_object().unwrap().clone())
                        .await?,
                )?;
                assert_eq!(result["items"].as_array().unwrap().len(), 1);
                assert_eq!(result["items"][0]["name"], "x.next().reclaim_objects");
                assert_eq!(result["items"][0]["candidate"], true);
                if !compact {
                    assert_eq!(
                        result["items"][0]["code"]["target"],
                        "x.next().reclaim_objects"
                    );
                    assert_eq!(result["items"][0]["code"]["basis"], "syntax");
                }
            }
        }
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn mixed_tools_measure_terse_bytes_without_changing_evidence() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        let source = format!(
            "import os\n{}",
            (0..12)
                .map(|n| format!("def item_{n}(value):\n    return os.path.exists(value)\n\n"))
                .collect::<String>()
        );
        std::fs::write(root.join("one.py"), source)?;
        std::fs::write(
            root.join("notes.md"),
            "# Local fixture\n\nThe client must preserve original source locations.\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut full_bytes = 0;
        let mut terse_bytes = 0;
        for (tool, args) in [
            (
                "airk_outline",
                serde_json::json!({"path":"one.py","compact":false}),
            ),
            ("airk_symbols", serde_json::json!({"compact":false})),
            ("airk_imports", serde_json::json!({"compact":false})),
            (
                "airk_code_facts",
                serde_json::json!({"kind":"call","compact":false}),
            ),
            ("airk_document_facts", serde_json::json!({"compact":false})),
            (
                "airk_relationships",
                serde_json::json!({"symbol":"file:one.py"}),
            ),
            (
                "airk_search",
                serde_json::json!({"query":"preserve","mode":"lexical"}),
            ),
            (
                "airk_read_file",
                serde_json::json!({"path":"one.py","start_line":1,"end_line":3}),
            ),
            (
                "airk_context",
                serde_json::json!({"path":"one.py","line":2}),
            ),
        ] {
            let mut results = Vec::new();
            let mut bytes = Vec::new();
            for mode in ["full", "terse"] {
                let mut args = args.as_object().unwrap().clone();
                args.insert("response_mode".into(), mode.into());
                let result = tools.call(tool.into(), args).await?;
                bytes.push(serde_json::to_vec(&result)?.len());
                results.push(body(result)?);
            }
            assert_eq!(
                results[0]["publication"]["token"],
                results[1]["publication"]["token"]
            );
            if results[0].get("read").is_some() {
                assert_eq!(
                    results[0]["read"]["validator"],
                    results[1]["read"]["validator"]
                );
            }
            for key in ["items", "hits"] {
                if let Some(rows) = results[0][key].as_array() {
                    assert_eq!(rows.len(), results[1][key].as_array().unwrap().len());
                    for (full, terse) in rows.iter().zip(results[1][key].as_array().unwrap()) {
                        for field in ["name", "citation", "text", "resolution"] {
                            assert_eq!(full.get(field), terse.get(field), "{tool}: {field}");
                        }
                    }
                }
            }
            assert!(bytes[1] < bytes[0], "{tool}: {bytes:?}");
            eprintln!(
                "mixed response fixture {tool}: full={} terse={}",
                bytes[0], bytes[1]
            );
            full_bytes += bytes[0];
            terse_bytes += bytes[1];
        }
        let rich = body(
            tools
                .call(
                    "airk_symbols".into(),
                    serde_json::json!({"compact":false,"response_mode":"terse","limit":100,"max_bytes":65536})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        let minimal = body(
            tools
                .call(
                    "airk_symbols".into(),
                    serde_json::json!({"response_mode":"terse","limit":100,"max_bytes":65536})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        assert_eq!(
            rich["items"].as_array().unwrap().len(),
            minimal["items"].as_array().unwrap().len()
        );
        assert!(serde_json::to_vec(&minimal)?.len() < serde_json::to_vec(&rich)?.len());
        eprintln!(
            "minimal symbol fixture: rich={} minimal={}",
            serde_json::to_vec(&rich)?.len(),
            serde_json::to_vec(&minimal)?.len()
        );
        eprintln!("mixed response fixture total: full={full_bytes} terse={terse_bytes}");
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn minimal_navigation_keeps_validators_and_has_one_fact_result_key() -> anyhow::Result<()>
    {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(
            root.join("one.rs"),
            "fn alpha() -> bool { true }\nfn beta() {}\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut validators = Vec::new();
        for mode in ["terse", "full"] {
            let args = serde_json::json!({"path":"one.rs","response_mode":mode})
                .as_object()
                .unwrap()
                .clone();
            let outline = body(tools.call("airk_outline".into(), args).await?)?;
            assert_eq!(outline["representation"], "minimal");
            assert_eq!(outline["items"][0]["name"], "alpha");
            assert!(outline["items"][0].get("qualified_name").is_none());
            assert!(outline["items"][0].get("file").is_none());
            assert_eq!(outline["files"][0]["path"], "one.rs");
            validators.push(outline["read"]["validator"].clone());
            assert!(validators.last().unwrap().is_string());
        }
        assert_eq!(validators[0], validators[1]);
        let conditional = body(
            tools
                .call(
                    "airk_outline".into(),
                    serde_json::json!({"path":"one.rs","if_evidence":validators[0]})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        assert!(conditional["items"].as_array().unwrap().is_empty());
        for (tool, args) in [
            ("airk_symbols", serde_json::json!({})),
            ("airk_imports", serde_json::json!({})),
            ("airk_code_facts", serde_json::json!({})),
            ("airk_document_facts", serde_json::json!({})),
            ("airk_relationships", serde_json::json!({"symbol":"absent"})),
            (
                "airk_outline",
                serde_json::json!({"path":"one.rs","compact":false}),
            ),
        ] {
            let result = body(
                tools
                    .call(tool.into(), args.as_object().unwrap().clone())
                    .await?,
            )?;
            assert!(result["items"].is_array(), "{tool}: {result}");
            for old in ["facts", "outline", "relationships", "minimal"] {
                assert!(result.get(old).is_none(), "{tool}: {old}");
            }
        }
        tools.close().await?;
        Ok(())
    }
    #[tokio::test]
    async fn service_clients_inherit_one_materialization_budget_and_close_independently()
    -> anyhow::Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&data)?;
        std::fs::write(root.join("fixture.rs"), "pub fn copper() {}\n")?;
        let kernel = Kernel::open_service_collection(&root, Some(data), "fixture")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut first = tools.session().await?;
        let mut second = tools.session().await?;
        for client in [&tools, &first, &second] {
            assert_eq!(client.admission.status()["shared_execution_budget"], true);
            assert_eq!(client.admission.status()["evidence_workers"], 1);
        }
        let args = serde_json::json!({"name":"copper"})
            .as_object()
            .unwrap()
            .clone();
        assert!(
            !first
                .call("airk_symbols".into(), args.clone())
                .await?
                .is_error
                .unwrap_or(false)
        );
        first.close().await?;
        assert!(
            !second
                .call("airk_symbols".into(), args)
                .await?
                .is_error
                .unwrap_or(false)
        );
        second.close().await?;
        tools.close().await?;
        Ok(())
    }
}
