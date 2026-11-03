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
        ) && arguments.get("reply_detail").is_none_or(Value::is_null)
            && arguments.get("compact").is_none_or(Value::is_null)
        {
            arguments.insert("compact".into(), json!(false));
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
    use anyhow::Context;

    fn payload(reply: CallToolResult) -> Result<Value> {
        ensure!(reply.is_error != Some(true), "{reply:?}");
        Ok(serde_json::from_str(
            &reply.content[0].as_text().unwrap().text,
        )?)
    }

    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                // Separate reads have different observation times; retain every
                // verification outcome, identity, limit and association.
                for key in [
                    "request_id",
                    "observed_unix_ms",
                    "cache_age_ms",
                    "cached",
                    "observed_from_unix_ms",
                    "observed_to_unix_ms",
                ] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(stable);
            }
            Value::Array(items) => items.iter_mut().for_each(stable),
            _ => {}
        }
    }

    #[test]
    fn parse_advertises_object_inputs_for_nested_contracts() {
        let tool = local::tools()
            .into_iter()
            .find(|t| t.name == "airk_parse")
            .unwrap();
        for key in ["plan", "recipe", "budgets", "output"] {
            let schema = &tool.input_schema["properties"][key];
            // Optional JSON objects may also admit null, never strings or arbitrary JSON.
            let kinds = schema["type"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![schema["type"].clone()]);
            assert!(kinds.contains(&json!("object")), "{key}: {schema}");
            assert!(
                kinds.iter().all(|v| v == "object" || v == "null"),
                "{key}: {schema}"
            );
            for invalid in [json!("{}"), json!([]), json!(true)] {
                let mut args = json!({"schema":"airk.parse.request/1"});
                args[key] = invalid;
                assert!(
                    serde_json::from_value::<ParseArgs>(args).is_err(),
                    "{key} requires an object"
                );
            }
        }
    }

    #[tokio::test]
    async fn missing_evidence_guidance_uses_returned_observations_without_reindexing() -> Result<()>
    {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("queue.rs"), "pub struct Queue;\n")?;
        std::fs::write(
            root.join("long.txt"),
            format!("{} Needle\n", "background ".repeat(200)),
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.configure_index_detail(airk_core::extraction::IndexDetail::Essential)?;
        kernel.configure_vector_selection(serde_json::from_value(
            json!({"version":1,"exclusions":[{"id":"exclude-queue","path_prefix":"queue.rs"}]}),
        )?)?;
        kernel.index()?;
        let publication = kernel.status_summary()?["publication"].clone();
        let mut tools = local::LocalTools::new(kernel.clone()).await?;
        for (tool, args, reason) in [
            (
                "airk_code_facts",
                json!({"name":"Queue","response_mode":"full"}),
                "not_retained",
            ),
            (
                "airk_capabilities",
                json!({"adapter":"text","response_mode":"full"}),
                "unsupported",
            ),
            (
                "airk_search",
                json!({"query":"AbsentIdentifier","mode":"lexical","response_mode":"full"}),
                "unknown",
            ),
            (
                "airk_file_info",
                json!({"path":"queue.rs","response_mode":"full"}),
                "excluded",
            ),
            (
                "airk_search",
                json!({"query":"Needle","mode":"lexical","max_bytes":32,"response_mode":"full"}),
                "truncated",
            ),
            (
                "airk_symbols",
                json!({"name":"Queue","reply_detail":"lean","response_mode":"full"}),
                "reply_omitted",
            ),
        ] {
            let result = payload(
                tools
                    .call(tool.into(), args.as_object().unwrap().clone())
                    .await?,
            )?;
            assert!(
                result["guidance"]["observations"]
                    .as_array()
                    .context("guidance missing")?
                    .iter()
                    .any(|v| v["reason"] == reason),
                "{result}"
            );
        }
        assert_eq!(kernel.status_summary()?["publication"], publication);
        assert_eq!(
            std::fs::read_to_string(root.join("queue.rs"))?,
            "pub struct Queue;\n"
        );
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn source_work_guidance_uses_the_fenced_cache_and_never_certifies_live_files()
    -> Result<()> {
        use airk_core::{
            changes::{FreshnessObservation, FreshnessState},
            identity::Incarnation,
            service::{ProjectRegistration, WorkerIdentity},
        };
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        let data = temp.path().join("data");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&data)?;
        let root = root.canonicalize()?;
        let data = data.canonicalize()?;
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: ProjectRegistration::new(
                root.to_str().unwrap().into(),
                data.to_str().unwrap().into(),
                "one".into(),
            )?
            .binding,
            fencing_generation: 1,
        };
        let kernel = Kernel::open_service_collection(&root, Some(data), "one")?
            .with_service_freshness(identity)?;
        let mut tools = local::LocalTools::new(kernel.clone()).await?;
        for (state, expected) in [
            (FreshnessState::Pending, "pending"),
            (FreshnessState::Failed, "failed"),
            (FreshnessState::Idle, "stale"),
        ] {
            let sample: FreshnessObservation = serde_json::from_value(
                json!({"observed_unix_ms":1,"state":state,"requested_watermark":2,"completed_watermark":1,"running_watermark":null,"last_success_unix_ms":null,"error":if state==FreshnessState::Failed {Some("fixture_failure")}else{None},"live_source_verified":false}),
            )?;
            kernel.observe_service_freshness(identity, Some(sample))?;
            let result = payload(
                tools
                    .call(
                        "airk_status".into(),
                        json!({"response_mode":"full"}).as_object().unwrap().clone(),
                    )
                    .await?,
            )?;
            assert!(
                result["guidance"]["observations"]
                    .as_array()
                    .context("guidance missing")?
                    .iter()
                    .any(|v| v["reason"] == expected),
                "{result}"
            );
            assert_eq!(result["freshness"]["live_source_verified"], false);
            assert_eq!(result["freshness"]["requested_watermark"], 2);
            assert_eq!(result["freshness"]["completed_watermark"], 1);
        }
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn explicit_reply_detail_is_independent_and_preserves_legacy_defaults() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("queue.rs"), "pub struct Queue;\n")?;
        std::fs::write(
            root.join("extra.rs"),
            "pub struct Second;\npub struct Third;\n",
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        for retained in [
            airk_core::extraction::IndexDetail::AgentLean,
            airk_core::extraction::IndexDetail::Detailed,
        ] {
            kernel.configure_index_detail(retained)?;
            kernel.index()?;
            let publication = kernel.status_summary()?["publication"].clone();
            let mut tools = local::LocalTools::new(kernel.clone()).await?;
            for (tool, mut args) in [
                ("symbols", json!({"name":"Queue"})),
                ("code_facts", json!({"name":"Queue"})),
                ("outline", json!({"path":"queue.rs"})),
            ] {
                args["response_mode"] = json!("full");
                // This compatibility check compares pre-verification reply
                // projections. A dated receipt binds the exact projection,
                // including reply_detail's echo, so it necessarily differs.
                // Default Report and complete receipt parity are tested separately.
                args["verification"] = json!("off");
                let legacy = payload(
                    tools
                        .call(format!("airk_{tool}"), args.as_object().unwrap().clone())
                        .await?,
                )?;
                assert!(legacy.get("reply_detail").is_none());
                let default_compact = tool != "code_facts"
                    || retained == airk_core::extraction::IndexDetail::AgentLean;
                let mut defaults = args.clone();
                defaults["compact"] = json!(default_compact);
                let mut expected_default = payload(
                    tools
                        .call(
                            format!("airk_{tool}"),
                            defaults.as_object().unwrap().clone(),
                        )
                        .await?,
                )?;
                let mut actual_default = legacy.clone();
                stable(&mut actual_default);
                stable(&mut expected_default);
                assert_eq!(
                    actual_default, expected_default,
                    "absent-argument compatibility: {tool} {retained:?}"
                );
                if tool == "code_facts" && !default_compact {
                    assert!(legacy["items"][0]["code"]["symbol_id"].is_string());
                }
                if tool != "code_facts" {
                    assert_eq!(legacy["representation"], "minimal");
                }
                for (choice, compact) in [("lean", true), ("rich", false)] {
                    let mut explicit = args.clone();
                    explicit["reply_detail"] = json!(choice);
                    let actual = payload(
                        tools
                            .call(
                                format!("airk_{tool}"),
                                explicit.as_object().unwrap().clone(),
                            )
                            .await?,
                    )?;
                    assert_eq!(actual["reply_detail"], choice);
                    let mut compatibility = args.clone();
                    compatibility["compact"] = json!(compact);
                    let mut expected = payload(
                        tools
                            .call(
                                format!("airk_{tool}"),
                                compatibility.as_object().unwrap().clone(),
                            )
                            .await?,
                    )?;
                    let mut normalized = actual.clone();
                    normalized.as_object_mut().unwrap().remove("reply_detail");
                    stable(&mut expected);
                    stable(&mut normalized);
                    assert_eq!(normalized, expected, "{tool} {choice} {retained:?}");
                    explicit["compact"] = json!(!compact);
                    assert!(
                        tools
                            .call(
                                format!("airk_{tool}"),
                                explicit.as_object().unwrap().clone()
                            )
                            .await?
                            .is_error
                            == Some(true)
                    );
                }
            }
            for (detail, compact, other) in [("lean", true, "rich"), ("rich", false, "lean")] {
                let first = payload(
                    tools
                        .call(
                            "airk_symbols".into(),
                            json!({"limit":1,"reply_detail":detail})
                                .as_object()
                                .unwrap()
                                .clone(),
                        )
                        .await?,
                )?;
                assert!(first["cursor"].is_string(), "{first}");
                let rejected = tools
                    .call(
                        "airk_symbols".into(),
                        json!({"limit":1,"reply_detail":other,"cursor":first["cursor"]})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )
                    .await?;
                assert_eq!(
                    rejected.is_error,
                    Some(true),
                    "changing projection must invalidate the cursor"
                );
                let continued = payload(
                    tools
                        .call(
                            "airk_symbols".into(),
                            json!({"limit":1,"compact":compact,"cursor":first["cursor"]})
                                .as_object()
                                .unwrap()
                                .clone(),
                        )
                        .await?,
                )?;
                assert!(!continued["items"].as_array().unwrap().is_empty());
            }
            let request = json!({"schema":"airk.parse.request/1","mode":"execute","reply_detail":"lean","text":"/symbols Queue"});
            let result = payload(
                tools
                    .call("airk_parse".into(), request.as_object().unwrap().clone())
                    .await?,
            )?;
            assert_eq!(result["steps"][0]["arguments"]["reply_detail"], "lean");
            assert_eq!(result["steps"][0]["result"]["representation"], "minimal");
            for detail in ["lean", "rich"] {
                let request = json!({"schema":"airk.parse.request/1","mode":"execute","reply_detail":detail,"recipe":{"id":"definition","parameters":{"name":"Queue"}}});
                let result = payload(
                    tools
                        .call("airk_parse".into(), request.as_object().unwrap().clone())
                        .await?,
                )?;
                assert_eq!(result["status"], "complete", "{result}");
                assert_eq!(result["calls"], 4);
            }
            let request = json!({"schema":"airk.parse.request/1","mode":"execute","reply_detail":"lean","plan":{"steps":[{"id":"rich","tool":"symbols","args":{"name":"Queue","reply_detail":"rich"}}]}});
            let result = payload(
                tools
                    .call("airk_parse".into(), request.as_object().unwrap().clone())
                    .await?,
            )?;
            assert_eq!(result["steps"][0]["result"]["reply_detail"], "rich");
            assert!(result["steps"][0]["result"]["items"][0]["code"]["symbol_id"].is_string());
            for default in [None, Some("lean")] {
                let mut request = json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"default","tool":"symbols","args":{"name":"Queue","reply_detail":null,"compact":null}}]}});
                if let Some(detail) = default {
                    request["reply_detail"] = json!(detail);
                }
                let result = payload(
                    tools
                        .call("airk_parse".into(), request.as_object().unwrap().clone())
                        .await?,
                )?;
                assert_eq!(result["status"], "complete", "{result}");
                if default.is_some() {
                    assert_eq!(result["steps"][0]["result"]["representation"], "minimal");
                } else {
                    assert!(
                        result["steps"][0]["result"]["items"][0]["code"]["symbol_id"].is_string()
                    );
                }
            }
            assert_eq!(
                kernel.status_summary()?["publication"],
                publication,
                "reply choices cannot publish or change retention"
            );
            tools.close().await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn compact_pages_feed_cited_dependent_reads_through_actual_handlers() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("source");
        std::fs::create_dir(&root)?;
        let source = "pub struct Queue;\n";
        std::fs::write(root.join("queue.rs"), source)?;
        let kernel = Kernel::open(&root, Some(temporary.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let mut citations = Vec::new();
        for compact in [true, false] {
            let request = json!({"schema":"airk.parse.request/1","mode":"execute",
            "plan":{"steps":[
                {"id":"find","tool":"symbols","args":{"name":"Queue","compact":compact}},
                {"id":"structure","tool":"file_info","args":{},"bind":[
                    {"arg":"path","from":"find","select":"unique_definition_path"}]},
                {"id":"excerpt","tool":"read_file","args":{},"bind":[
                    {"arg":"location","from":"find","select":"hit_locations"}]}
            ]}});
            let value = payload(
                tools
                    .call("airk_parse".into(), request.as_object().unwrap().clone())
                    .await?,
            )?;
            assert_eq!(value["status"], "complete", "{value}");
            assert_eq!(
                value["calls"], 4,
                "one publication read and three explicit evidence reads"
            );
            for step in value["steps"].as_array().unwrap() {
                assert_eq!(step["status"], "complete", "{step}");
            }
            assert_eq!(value["steps"][1]["arguments"]["path"], "queue.rs");
            assert_eq!(
                value["steps"][2]["arguments"]["known_hash"],
                value["evidence"][0]["citation"]["content_hash"]
            );
            citations.push(value["evidence"][0]["citation"].clone());
            assert!(
                serde_json::to_vec(&value)?.len()
                    <= value["plan"]["output"]["max_bytes"].as_u64().unwrap() as usize
            );
            if compact {
                assert_eq!(value["steps"][0]["result"]["representation"], "minimal");
                let fact = &value["evidence"][0]["fact"];
                assert_eq!(fact["file"]["path"], "queue.rs");
                assert_eq!(fact["provenance"]["basis"], "syntax");
                assert!(fact["file"].get("provenance").is_none());
                assert!(fact.get("symbol_id").is_none());
            }
        }
        assert_eq!(
            citations[0], citations[1],
            "compact/rich retain exact original citations"
        );
        // ID selectors require an explicit rich locator read; it is a charged plan step.
        let request = json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[
            {"id":"find","tool":"symbols","args":{"name":"Queue","compact":true}},
            {"id":"locate","tool":"symbols","args":{"name":"Queue","compact":false},"bind":[{"arg":"path","from":"find","select":"unique_definition_path"}]},
            {"id":"links","tool":"relationships","args":{"direction":"incoming"},"bind":[{"arg":"symbol","from":"locate","select":"selected_symbol_ids"}]}
        ]}});
        let value = payload(
            tools
                .call("airk_parse".into(), request.as_object().unwrap().clone())
                .await?,
        )?;
        assert_eq!(value["status"], "complete", "{value}");
        assert_eq!(value["calls"], 4);
        assert!(
            value["steps"][2]["arguments"]["symbol"]
                .as_str()
                .is_some_and(|s| s.starts_with("syntax:"))
        );
        assert_eq!(std::fs::read_to_string(root.join("queue.rs"))?, source);
        tools.close().await?;
        Ok(())
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
                "plan":{"verification_scope":"steps","steps":[{"id":"query","tool":tool,"args":args}]}});
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
