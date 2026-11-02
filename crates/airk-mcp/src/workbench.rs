// SPDX-License-Identifier: Apache-2.0
//! Workbench reads use the MCP handlers, under the outer plan's admitted lifetime.
use crate::*;
use anyhow::{Result, bail, ensure};
use serde_json::{Map, Value, json};

impl AirkServer {
    pub(crate) async fn workbench_read(
        &self,
        tool: &str,
        arguments: &Map<String, Value>,
        revision: Option<&str>,
    ) -> Result<Value> {
        ensure!(
            self.plan_read,
            "plan reader requires an admitted outer request"
        );
        let mut arguments = arguments.clone();
        arguments.insert("response_mode".into(), json!("full"));
        arguments.insert("include_status".into(), json!(true));
        if matches!(tool, "read_file" | "file_info" | "outline" | "context")
            && let Some(revision) = revision
        {
            arguments
                .entry("revision")
                .or_insert_with(|| json!(revision));
        }
        // Retain Workbench's documented rich fact default explicitly.
        if matches!(
            tool,
            "symbols" | "imports" | "code_facts" | "document_facts" | "outline"
        ) {
            arguments.entry("compact").or_insert(json!(false));
        }
        let args = Value::Object(arguments);
        macro_rules! read {
            ($method:ident) => {
                self.$method(Parameters(serde_json::from_value(args)?))
                    .await
            };
        }
        let mut reply = match tool {
            "search" => read!(search),
            "symbols" => read!(symbols),
            "references" => read!(references),
            "read_file" => read!(read_file),
            "outline" => read!(outline),
            "list_files" => read!(list_files),
            "file_info" => read!(file_info),
            "imports" => read!(imports),
            "code_facts" => read!(code_facts),
            "document_facts" => read!(document_facts),
            "changes" => read!(changes),
            "context" => read!(context),
            "relationships" => read!(relationships),
            "status" => read!(status),
            "diagnostics" => read!(diagnostics),
            "capabilities" => read!(capabilities),
            "help" => read!(help),
            _ => bail!("unknown evidence operation"),
        };
        // The SDK's complete response wrapper removes this discriminator on the
        // wire. Retain the same error receipt that a CLI/MCP peer receives.
        reply.result_type = None;
        if reply.is_error == Some(true) {
            return Err(airk_kernel::workbench::ReadFailure(serde_json::to_value(reply)?).into());
        }
        let value = reply
            .structured_content
            .or_else(|| {
                reply.content.iter().find_map(|content| {
                    content
                        .as_text()
                        .and_then(|text| serde_json::from_str(&text.text).ok())
                })
            })
            .ok_or_else(|| anyhow::anyhow!("MCP handler returned no JSON result"))?;
        ensure!(reply.is_error != Some(true), "{value}");
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(reply: CallToolResult) -> Result<Value> {
        ensure!(reply.is_error != Some(true), "{reply:?}");
        Ok(serde_json::from_str(
            &reply.content[0].as_text().unwrap().text,
        )?)
    }

    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for key in ["request_id", "observed_unix_ms", "cache_age_ms", "cached"] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(stable);
            }
            Value::Array(items) => items.iter_mut().for_each(stable),
            _ => {}
        }
    }

    #[tokio::test]
    async fn every_workbench_operation_retains_the_full_mcp_result() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(
            root.join("queue.rs"),
            "pub struct Queue;\nimpl Queue { pub fn run(&self) {} }\n",
        )?;
        std::fs::write(
            root.join("policy.md"),
            "# Queue policy\nRequests must preserve retry budgets.\n",
        )?;
        let kernel = Kernel::open(&root, Some(temporary.path().join("state")))?;
        kernel.index()?;
        let revision = kernel.status_summary()?["publication"]["token"].clone();
        let mut tools = local::LocalTools::new(kernel.clone()).await?;
        tools.set_runtime(json!({"schema":1,"service":{"responsive":true}}));
        tools.set_work(json!({"schema":1,"pending_job":null}));
        for (tool, args) in [
            (
                "search",
                json!({"query":"Queue","mode":"lexical","full_chunks":true}),
            ),
            (
                "symbols",
                json!({"name":"Queue","compact":false,"include_count":true}),
            ),
            (
                "references",
                json!({"name":"Queue","order":"path","include_files":true,"include_count":true}),
            ),
            (
                "read_file",
                json!({"path":"queue.rs","start_line":1,"end_line":2}),
            ),
            ("outline", json!({"path":"queue.rs","compact":false})),
            ("list_files", json!({})),
            ("file_info", json!({"path":"queue.rs"})),
            ("imports", json!({"compact":false})),
            ("code_facts", json!({"name":"Queue","compact":false})),
            ("document_facts", json!({"compact":false})),
            (
                "changes",
                json!({"since":revision,"include_symbols":true,"include_documents":true}),
            ),
            ("context", json!({"path":"queue.rs","line":1})),
            ("relationships", json!({"symbol":"Queue"})),
            ("status", json!({})),
            ("diagnostics", json!({})),
            ("capabilities", json!({"rich":true})),
            ("help", json!({})),
        ] {
            let mut direct = args.clone();
            direct["response_mode"] = json!("full");
            direct["include_status"] = json!(true);
            let mut expected = payload(
                tools
                    .call(format!("airk_{tool}"), direct.as_object().unwrap().clone())
                    .await?,
            )?;
            let request = json!({"schema":"airk.parse.request/1","mode":"execute",
                "plan":{"steps":[{"id":"query","tool":tool,"args":args}]}});
            let envelope = payload(
                tools
                    .call("airk_parse".into(), request.as_object().unwrap().clone())
                    .await?,
            )?;
            ensure!(
                envelope["steps"][0]["status"] == "complete",
                "{tool}: {envelope}"
            );
            let mut actual = envelope["steps"][0]["result"].clone();
            ensure!(
                actual.get("request_id").is_some() || tool == "help",
                "{tool} lost request identity"
            );
            stable(&mut expected);
            stable(&mut actual);
            assert_eq!(actual, expected, "{tool} dropped or changed MCP fields");
            // The closed form inventory must include every native input other than
            // transport display controls, which Workbench always requests in full.
            let catalog = airk_kernel::workbench::operation_catalog();
            let native = local::tools()
                .into_iter()
                .find(|t| t.name == format!("airk_{tool}"))
                .unwrap();
            for field in native.input_schema["properties"]
                .as_object()
                .unwrap()
                .keys()
            {
                if !matches!(field.as_str(), "response_mode" | "include_status") {
                    assert!(
                        catalog["operations"][tool]["properties"]
                            .get(field)
                            .is_some(),
                        "{tool} missing {field}"
                    );
                }
            }
        }
        let args = json!({"adapter":"fixture-unknown-adapter"});
        let failed = tools
            .call(
                "airk_capabilities".into(),
                args.as_object().unwrap().clone(),
            )
            .await?;
        ensure!(failed.is_error == Some(true), "{failed:?}");
        let request = json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"missing","tool":"capabilities","args":args}]}});
        let envelope = payload(
            tools
                .call("airk_parse".into(), request.as_object().unwrap().clone())
                .await?,
        )?;
        assert_eq!(envelope["steps"][0]["status"], "failed");
        assert_eq!(
            envelope["steps"][0]["result"],
            serde_json::to_value(failed)?
        );
        let scoped = kernel.evidence_plan_with_reader(
            serde_json::from_value(json!({"schema":"airk.parse.request/1","mode":"execute","text":"/search Queue","path_prefix":"allowed/"}))?,
            &Value::Null,
            Some(&|_, _, _| Ok(json!({"publication":{"token":revision},"hits":[{"citation":{"path":"outside.rs","start_line":1,"end_line":1},"text":"outside evidence"}]}))),
        )?;
        assert_eq!(scoped.steps[0].status, "failed");
        assert!(scoped.steps[0].result.is_none());
        assert!(scoped.evidence.is_empty());
        tools.close().await?;
        Ok(())
    }
}
