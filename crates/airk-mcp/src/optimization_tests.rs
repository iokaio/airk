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
