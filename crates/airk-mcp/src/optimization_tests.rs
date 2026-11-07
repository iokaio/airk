// SPDX-License-Identifier: Apache-2.0
//! Public compatibility and cross-layer routing for optional query selectors.
use super::*;
use serde_json::{Value, json};

fn response_body(result: CallToolResult) -> anyhow::Result<Value> {
    anyhow::ensure!(!result.is_error.unwrap_or(false), "{result:?}");
    Ok(serde_json::from_str(
        &result.content[0].as_text().expect("text result").text,
    )?)
}

#[test]
fn optional_selectors_preserve_old_requests_and_reject_unknown_modes() {
    let search: SearchArgs = serde_json::from_value(json!({"query":"Session"})).unwrap();
    assert!(search.language.is_none() && search.evidence_kind.is_none());
    assert!(search.exclude_path_prefix.is_none() && search.mode.is_none());
    let references: ReferenceArgs = serde_json::from_value(json!({"name":"Session"})).unwrap();
    assert!(references.cursor.is_none() && references.order.is_none());
    assert!(!references.include_files && !references.include_count);
    let facts: CodeFactArgs = serde_json::from_value(json!({"name":"Session"})).unwrap();
    assert!(facts.name_match.is_none() && !facts.include_count);
    let symbols: FactArgs = serde_json::from_value(json!({"name":"Session"})).unwrap();
    assert!(symbols.name_match.is_none() && !symbols.include_count);

    for value in ["code", "document", "configuration", "unknown"] {
        let request: SearchArgs = serde_json::from_value(json!({
            "query":"Session", "evidence_kind":value,
        }))
        .unwrap();
        assert!(request.evidence_kind.is_some());
    }
    for value in ["exact", "suffix", "contains"] {
        assert!(
            serde_json::from_value::<CodeFactArgs>(json!({
                "name":"Session", "name_match":value,
            }))
            .is_ok()
        );
    }
    for value in ["index", "path", "code_first"] {
        assert!(
            serde_json::from_value::<ReferenceArgs>(json!({
                "name":"Session", "order":value,
            }))
            .is_ok()
        );
    }
    assert!(
        serde_json::from_value::<SearchArgs>(json!({
            "query":"Session", "evidence_kind":"all_sources",
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<CodeFactArgs>(json!({
            "name":"Session", "name_match":"regex",
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ReferenceArgs>(json!({
            "name":"Session", "order":"score",
        }))
        .is_err()
    );

    let native = local::tools();
    let public = unified::tools();
    for (name, fields) in [
        (
            "search",
            &["language", "evidence_kind", "exclude_path_prefix"][..],
        ),
        ("symbols", &["name_match", "include_count"][..]),
        ("code_facts", &["name_match", "include_count"][..]),
        (
            "references",
            &[
                "cursor",
                "order",
                "include_files",
                "include_count",
                "language",
                "evidence_kind",
            ][..],
        ),
    ] {
        for (catalog, selected) in [(&native, format!("airk_{name}")), (&public, name.into())] {
            let tool = catalog.iter().find(|tool| tool.name == selected).unwrap();
            let schema = serde_json::to_value(&tool.input_schema).unwrap();
            for field in fields {
                assert!(
                    schema["properties"].get(*field).is_some(),
                    "{selected}: {field}"
                );
            }
        }
    }
}

#[tokio::test]
async fn symbols_matching_filters_and_reference_paging_reach_the_kernel() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("one.rs"),
        "struct Session;\nstruct OwnerSession;\nfn use_session(_: Session) {}\n",
    )?;
    std::fs::write(root.path().join("two.rs"), "struct NativeSession;\n")?;
    std::fs::write(
        root.path().join("guide.md"),
        "# Session\nSession prose documentation.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let server = AirkServer::new(kernel.clone());

    let exact = response_body(
        server
            .symbols(Parameters(serde_json::from_value(json!({
                "name":"Session", "compact":false, "response_mode":"full",
            }))?))
            .await,
    )?;
    assert_eq!(exact["items"].as_array().unwrap().len(), 1, "{exact}");
    let suffix = response_body(
        server
            .symbols(Parameters(serde_json::from_value(json!({
                "name":"Session", "name_match":"suffix", "include_count":true,
                "compact":false, "response_mode":"full", "max_bytes":16384,
            }))?))
            .await,
    )?;
    let items = suffix["items"].as_array().unwrap();
    for name in ["Session", "OwnerSession", "NativeSession"] {
        assert!(items.iter().any(|item| item["name"] == name), "{suffix}");
    }
    assert!(suffix["match_count"].is_object(), "{suffix}");
    assert_eq!(suffix["match_count"]["total_matches"], 3, "{suffix}");
    assert_eq!(suffix["match_count"]["complete"], true, "{suffix}");
    assert_eq!(suffix["match_count"]["scope"], "query", "{suffix}");

    let filtered = response_body(
        server
            .search(Parameters(serde_json::from_value(json!({
                "query":"Session", "mode":"lexical", "language":"rust",
                "evidence_kind":"code", "exclude_path_prefix":"two", "limit":1,
                "response_mode":"full",
            }))?))
            .await,
    )?;
    assert_eq!(
        filtered["hits"][0]["citation"]["path"], "one.rs",
        "{filtered}"
    );
    let hint = filtered["definitions"]
        .as_array()
        .expect("named query has definition navigation");
    assert!(
        hint.iter()
            .any(|entry| entry["name"] == "Session" && entry["suggested_tool"] == "symbols"),
        "{filtered}"
    );
    for entry in hint {
        for pointer in entry["matches"].as_array().unwrap() {
            assert_eq!(pointer["citation"]["path"], "one.rs", "{filtered}");
            assert_eq!(
                pointer["citation"]["content_hash"],
                filtered["hits"][0]["citation"]["content_hash"]
            );
        }
    }

    let legacy = response_body(
        server
            .references(Parameters(serde_json::from_value(json!({
                "name":"Session", "response_mode":"full",
            }))?))
            .await,
    )?;
    assert!(
        legacy.get("page").is_none(),
        "legacy shape changed: {legacy}"
    );
    let paged = response_body(
        server
            .references(Parameters(serde_json::from_value(json!({
                "name":"Session", "order":"path", "include_files":true,
                "include_count":true, "language":"rust", "limit":1,
                "response_mode":"full",
            }))?))
            .await,
    )?;
    assert_eq!(paged["page"]["count_unit"], "matching_lines", "{paged}");
    assert!(paged["page"]["files"].is_array(), "{paged}");
    assert!(paged["page"]["match_count"].is_object(), "{paged}");
    assert_eq!(
        paged["evidence"]["hits"].as_array().unwrap().len(),
        1,
        "{paged}"
    );
    let cursor = paged["page"]["cursor"]
        .as_str()
        .expect("one-line page has continuation");
    assert!(
        paged["page"]["match_count"]["total_matches"].is_null(),
        "{paged}"
    );
    let next = response_body(
        server
            .references(Parameters(serde_json::from_value(json!({
                "name":"Session", "order":"path", "include_files":true,
                "include_count":true, "language":"rust", "limit":1, "cursor":cursor,
                "response_mode":"full",
            }))?))
            .await,
    )?;
    assert_ne!(
        paged["evidence"]["hits"][0]["citation"], next["evidence"]["hits"][0]["citation"],
        "{next}"
    );

    let invalid = server
        .code_facts(Parameters(serde_json::from_value(json!({
            "symbol_key":"01".repeat(16), "name_match":"suffix",
        }))?))
        .await;
    assert!(invalid.is_error.unwrap_or(false), "{invalid:?}");
    assert!(
        invalid.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("cannot be combined")
    );
    Ok(())
}

#[test]
fn native_public_and_workbench_catalogs_share_evidence_limits() {
    let native = local::tools();
    let public = unified::tools();
    let workbench = airk_kernel::workbench::operation_catalog();
    for tool in [
        "changes",
        "search",
        "read_file",
        "list_files",
        "outline",
        "symbols",
        "imports",
        "document_facts",
        "references",
        "context",
        "code_facts",
        "relationships",
    ] {
        let native_name = format!("airk_{tool}");
        for catalog in [&native, &public] {
            let selected = catalog
                .iter()
                .find(|entry| entry.name == tool || entry.name == native_name)
                .unwrap();
            let schema = serde_json::to_value(&selected.input_schema).unwrap();
            for (field, maximum) in [("limit", 100), ("max_bytes", 65536)] {
                if schema["properties"].get(field).is_none() {
                    continue;
                }
                assert_eq!(schema["properties"][field]["minimum"], 1, "{tool}.{field}");
                assert_eq!(
                    schema["properties"][field]["maximum"], maximum,
                    "{tool}.{field}"
                );
                assert_eq!(
                    workbench["operations"][tool]["properties"][field]["maximum"], maximum,
                    "Workbench {tool}.{field}"
                );
            }
            if tool == "context" {
                for field in ["before", "after"] {
                    assert_eq!(schema["properties"][field]["minimum"], 0);
                    assert_eq!(
                        schema["properties"][field]["maximum"],
                        airk_core::native_reads::CONTEXT_RADIUS_MAX
                    );
                    assert_eq!(
                        schema["properties"][field]["maximum"],
                        workbench["operations"][tool]["properties"][field]["maximum"]
                    );
                    assert_eq!(
                        schema["properties"][field]["default"],
                        airk_core::native_reads::CONTEXT_RADIUS_DEFAULT
                    );
                    assert_eq!(
                        schema["properties"][field]["default"],
                        workbench["operations"][tool]["properties"][field]["default"]
                    );
                }
            }
            if tool == "relationships" {
                assert_eq!(schema["properties"]["hops"]["minimum"], 1);
                assert_eq!(schema["properties"]["hops"]["maximum"], 3);
                assert_eq!(
                    schema["properties"]["direction"]["enum"],
                    json!(["incoming", "outgoing"])
                );
                assert_eq!(
                    schema["properties"]["direction"]["enum"],
                    workbench["operations"][tool]["properties"]["direction"]["enum"]
                );
            }
            if matches!(
                tool,
                "symbols" | "imports" | "code_facts" | "document_facts"
            ) {
                for field in ["kind", "language"] {
                    let Some(property) = schema["properties"].get(field) else {
                        continue;
                    };
                    let spec = property
                        .get("anyOf")
                        .and_then(Value::as_array)
                        .and_then(|variants| {
                            variants
                                .iter()
                                .find(|variant| variant.get("minLength").is_some())
                        })
                        .unwrap_or(property);
                    assert_eq!(spec["minLength"], 1, "{tool}.{field}");
                    assert_eq!(spec["maxLength"], 128, "{tool}.{field}");
                    assert!(
                        spec.get("enum").is_none(),
                        "retained producer labels remain open"
                    );
                    assert!(
                        property["description"]
                            .as_str()
                            .unwrap()
                            .contains("UTF-8 bytes")
                    );
                    assert_eq!(
                        workbench["operations"][tool]["properties"][field]["minLength"],
                        1
                    );
                    assert_eq!(
                        workbench["operations"][tool]["properties"][field]["maxLength"],
                        128
                    );
                }
            }
        }
    }
    // Help and parse keep their independent documented payload contracts.
    let help = public.iter().find(|entry| entry.name == "help").unwrap();
    let description = serde_json::to_value(&help.input_schema).unwrap()["properties"]["max_bytes"]
        ["description"]
        .clone();
    assert!(description.as_str().unwrap().contains("4096..32768"));
}

#[tokio::test]
async fn context_radius_100_reaches_workbench_and_native_handlers_without_clipping()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let text = (1..=240)
        .map(|line| format!("line-{line:03}\n"))
        .collect::<String>();
    std::fs::write(root.path().join("lines.txt"), text)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let server = AirkServer::new(kernel.clone());
    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                // Native transport identities and observation time are not evidence.
                fields.remove("request_id");
                fields.remove("observed_unix_ms");
                for value in fields.values_mut() {
                    stable(value);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(stable),
            _ => {}
        }
    }
    for selected_radius in [
        None,
        Some(0),
        Some(40),
        Some(airk_core::native_reads::CONTEXT_RADIUS_MAX),
    ] {
        let radius = selected_radius.unwrap_or(airk_core::native_reads::CONTEXT_RADIUS_DEFAULT);
        let mut arguments =
            json!({"path":"lines.txt","line":120,"max_bytes":65536,"verification":"off"});
        if let Some(radius) = selected_radius {
            arguments["before"] = json!(radius);
            arguments["after"] = json!(radius);
        }
        let run = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":"airk.parse.request/1","mode":"execute","verification":"off",
            "plan":{"steps":[{"id":"context","tool":"context","args":arguments}]}
        }))?)?;
        assert_eq!(run.status, "complete", "{run:?}");
        assert_eq!(run.steps[0].status, "complete");
        let expected = run.steps[0].result.clone().unwrap();
        let mut args = run.steps[0].arguments.clone();
        args["revision"] = json!(run.plan.publication);
        args["response_mode"] = json!("full");
        args["include_status"] = json!(true);
        let mut actual = response_body(
            server
                .context(Parameters(serde_json::from_value(args)?))
                .await,
        )?;
        // Reuse the transport decorator for its additional readiness observations;
        // every canonical Workbench field remains in the full comparison.
        let mut expected = response_body(
            server
                .execute_verified_response(
                    UsageOperation::Context,
                    ResponseArgs {
                        response_mode: Some(response::ResponseMode::Full),
                        include_status: true,
                        reply_detail: None,
                    },
                    airk_core::verification::VerificationControls::off(),
                    move |_| Ok(expected),
                )
                .await,
        )?;
        stable(&mut actual);
        stable(&mut expected);
        assert_eq!(actual, expected, "radius={radius}");
        let encoded = serde_json::to_string(&actual)?;
        assert!(encoded.contains(&format!("line-{:03}", 120 - radius)));
        assert!(encoded.contains(&format!("line-{:03}", 120 + radius)));
    }
    let invalid = airk_core::native_reads::CONTEXT_RADIUS_MAX + 1;
    assert!(kernel.evidence_plan(serde_json::from_value(json!({
        "schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"context","tool":"context","args":{
            "path":"lines.txt","line":120,"before":invalid,"after":0
        }}]}
    }))?).is_err());
    let refused = server
        .context(Parameters(serde_json::from_value(json!({
            "path":"lines.txt","line":120,"before":invalid,"after":0,"verification":"off"
        }))?))
        .await;
    assert!(refused.is_error.unwrap_or(false));
    Ok(())
}

#[tokio::test]
async fn annotated_evidence_budgets_reject_out_of_range_inputs_without_clamping()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("one.rs"),
        "pub struct Queue;\nfn leaf() {}\nfn entry() { leaf(); }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let catalog = local::tools();
    let mut tools = local::LocalTools::new(kernel.clone()).await?;
    for (tool, arguments) in [
        ("search", json!({"query":"Queue","mode":"lexical"})),
        (
            "changes",
            json!({"selectors":[{"kind":"path","path":"one.rs"}]}),
        ),
        (
            "read_file",
            json!({"path":"one.rs","start_line":1,"end_line":1}),
        ),
        ("list_files", json!({})),
        ("outline", json!({"path":"one.rs"})),
        ("symbols", json!({"name":"Queue"})),
        ("imports", json!({"name":"Queue"})),
        ("code_facts", json!({"name":"Queue"})),
        ("document_facts", json!({"name":"Queue"})),
        ("references", json!({"name":"Queue"})),
        ("context", json!({"path":"one.rs","line":1})),
        (
            "relationships",
            json!({"symbol":"leaf","direction":"incoming"}),
        ),
    ] {
        let name = format!("airk_{tool}");
        let schema = serde_json::to_value(
            &catalog
                .iter()
                .find(|entry| entry.name == name)
                .unwrap()
                .input_schema,
        )?;
        for (field, invalid) in [
            ("limit", 0),
            ("limit", 101),
            ("max_bytes", 0),
            ("max_bytes", 65537),
        ] {
            if schema["properties"].get(field).is_none() {
                continue;
            }
            let mut arguments = arguments.clone();
            arguments[field] = json!(invalid);
            arguments["verification"] = json!("off");
            let result = tools
                .call(name.clone(), arguments.as_object().unwrap().clone())
                .await?;
            assert!(
                result.is_error.unwrap_or(false),
                "{tool}.{field}={invalid} was accepted: {result:?}"
            );
            let text = &result.content[0].as_text().unwrap().text;
            assert!(text.contains(field), "{tool}.{field}={invalid}: {text}");
            assert!(
                kernel
                    .evidence_plan(serde_json::from_value(json!({
                        "schema":"airk.parse.request/1","mode":"execute",
                        "plan":{"steps":[{"id":"invalid","tool":tool,"args":arguments}]}
                    }))?)
                    .is_err(),
                "Workbench {tool}.{field}={invalid} was accepted"
            );
        }
    }
    tools.close().await?;
    Ok(())
}
