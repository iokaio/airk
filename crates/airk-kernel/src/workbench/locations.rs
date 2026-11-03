// SPDX-License-Identifier: Apache-2.0
//! Resolve compact bindings inside their own page, without additional reads.
use super::safe_path;
use airk_core::{Citation, DocumentLocation, discovery::MinimalFacts};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Extract self-contained cited records from rich reads or minimal discovery pages.
/// No table binding is inherited by a sibling, nested result or subsequent page.
pub fn locations(value: &Value) -> Result<Vec<Value>> {
    fn visit(value: &Value, out: &mut Vec<Value>) -> Result<()> {
        match value {
            Value::Object(map)
                if map.get("representation").and_then(Value::as_str) == Some("minimal")
                    // The typed native MinimalFacts wrapper precedes MCP's discriminator.
                    || (map.len()==3 && ["items","files","provenance"].iter().all(|key|map.get(*key).is_some_and(Value::is_array))) =>
            {
                let page = MinimalFacts::deserialize(value).context("invalid minimal page")?;
                ensure!(
                    page.files.len() <= 256 && page.provenance.len() <= 64,
                    "compact binding table limit exceeded"
                );
                // Validate the entire page before returning any of its evidence.
                for file in &page.files {
                    ensure!(
                        !file.path.is_empty() && safe_path(&file.path),
                        "invalid compact path"
                    );
                    ensure!(
                        file.content_hash.len() == 64
                            && file
                                .content_hash
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                        "invalid compact source hash"
                    );
                    ensure!(
                        file.provenance < page.provenance.len(),
                        "invalid file provenance binding"
                    );
                }
                for (row, raw) in page
                    .items
                    .iter()
                    .zip(value["items"].as_array().context("invalid compact items")?)
                {
                    ensure!(
                        row.file.is_some() || page.files.len() == 1,
                        "ambiguous implicit compact file"
                    );
                    let file = page
                        .files
                        .get(row.file.unwrap_or(0))
                        .context("invalid compact file binding")?;
                    let provenance = page
                        .provenance
                        .get(row.provenance.unwrap_or(file.provenance))
                        .context("invalid compact provenance binding")?;
                    let [start, end] = row.lines;
                    ensure!(start > 0 && end >= start, "invalid compact line range");
                    let location = match &row.location {
                        Some(location) => location.clone(),
                        None => {
                            ensure!(
                                file.line_basis == "source",
                                "original compact document location missing"
                            );
                            DocumentLocation::Lines { start, end }
                        }
                    };
                    if let DocumentLocation::Mapped { region } = &location {
                        region.validate_shape()?;
                    }
                    let citation = Citation {
                        path: file.path.clone(),
                        content_hash: file.content_hash.clone(),
                        start_line: start,
                        end_line: end,
                        location,
                    };
                    // Keep all row observations, but replace page-local indexes with
                    // their values so aggregation/copies never contain dangling IDs.
                    let mut fact = raw.clone();
                    fact["file"] = serde_json::to_value(file)?;
                    fact["file"].as_object_mut().unwrap().remove("provenance");
                    fact["provenance"] = serde_json::to_value(provenance)?;
                    out.push(json!({"citation":citation,"text":row.name,"fact":fact}));
                }
            }
            Value::Object(map) => {
                if let Some(citation) = map
                    .get("citation")
                    .filter(|c| c.get("path").and_then(Value::as_str).is_some())
                {
                    out.push(json!({"citation":citation,"text":map.get("text").or_else(|| map.get("name")).unwrap_or(&Value::Null),"fact":value}));
                } else {
                    for child in map.values() {
                        visit(child, out)?;
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, out)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(value, &mut out)?;
    let mut seen = BTreeSet::new();
    out.retain(|v| seen.insert(v["citation"].to_string()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{expand, step};
    use airk_core::workbench::Binding;
    use std::collections::BTreeMap;

    fn page(path: &str) -> Value {
        json!({"representation":"minimal","items":[{"name":"Queue","kind":"definition","lines":[2,3],"signature":"struct Queue","file":0,"provenance":0}],
            "files":[{"path":path,"content_hash":"ab".repeat(32),"language":"rust","parse_status":"syntax","line_basis":"source","provenance":0}],
            "provenance":[{"basis":"syntax","producer":"original-fixture","coverage":"written syntax"}]})
    }

    #[test]
    fn compact_bindings_are_page_local_and_materialize_complete_descriptors() -> Result<()> {
        let mut first = page("one.rs");
        first["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("provenance");
        let mut second = page("two.rs");
        second["files"][0]["content_hash"] = json!("cd".repeat(32));
        second["provenance"]
            .as_array_mut()
            .unwrap()
            .push(json!({"basis":"imported","producer":"fixture-2"}));
        second["items"][0]["provenance"] = json!(1);
        second["items"][0]["candidate"] = json!(true);
        let rows = locations(&json!([first, second]))?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["citation"]["path"], "one.rs");
        assert_eq!(rows[1]["citation"]["path"], "two.rs");
        assert_eq!(rows[1]["citation"]["content_hash"], "cd".repeat(32));
        assert_eq!(rows[1]["fact"]["provenance"]["producer"], "fixture-2");
        assert_eq!(rows[1]["fact"]["file"]["parse_status"], "syntax");
        assert_eq!(rows[1]["fact"]["candidate"], true);
        assert!(rows[1]["fact"]["file"].get("provenance").is_none());
        assert!(rows[1]["fact"].get("symbol_id").is_none());
        Ok(())
    }

    #[test]
    fn compact_original_document_locations_and_observations_survive() -> Result<()> {
        for location in [
            json!({"Page":{"number":4}}),
            json!({"Block":{"number":7}}),
            json!({"TableRow":{"table":2,"row":3}}),
            json!({"WorksheetRow":{"sheet":"Costs","row":9}}),
            json!({"Mapped":{"region":{"kind":"embedded","region_id":"cell-1","local_span":{"start":0,"end":6},"original_spans":[{"start":24,"end":30}]}}}),
        ] {
            let mut value = page("document.pdf");
            value["items"][0].as_object_mut().unwrap().remove("file");
            value["files"][0]["line_basis"] = json!("extracted-text; cite original location");
            value["items"][0]["location"] = location.clone();
            value["items"][0]["details_truncated"] = json!(true);
            value["items"][0]["statement_truncated"] = json!(true);
            let table = airk_core::TableEvidence {
                row: 3,
                cells: vec![airk_core::TableCellEvidence {
                    column: 1,
                    value: "8".into(),
                    formula: Some("SUM(A1:A2)".into()),
                    cached_value: true,
                    ..Default::default()
                }],
                ..Default::default()
            };
            value["items"][0]["table"] = serde_json::to_value(table)?;
            let row = locations(&value)?.remove(0);
            assert_eq!(row["citation"]["location"], location);
            assert_eq!(row["citation"]["start_line"], 2);
            assert_eq!(row["fact"]["details_truncated"], true);
            assert_eq!(row["fact"]["statement_truncated"], true);
            assert_eq!(row["fact"]["table"], value["items"][0]["table"]);
            value["items"][0]
                .as_object_mut()
                .unwrap()
                .remove("location");
            assert!(
                locations(&value).is_err(),
                "never invent source lines for extracted documents"
            );
        }
        Ok(())
    }

    #[test]
    fn invalid_compact_bindings_fail_the_entire_result_without_borrowing_tables() {
        for (pointer, invalid) in [
            ("/items/0/file", json!(1)),
            ("/items/0/provenance", json!(1)),
            ("/files/0/provenance", json!(1)),
            ("/items/0/lines", json!([0, 3])),
            ("/items/0/lines", json!([4, 3])),
            ("/items/0/lines", json!([2])),
            ("/files/0/content_hash", json!("unknown")),
            ("/files/0/path", json!("../outside.rs")),
            ("/provenance", json!([])),
            ("/files", json!([])),
        ] {
            let mut invalid_page = page("two.rs");
            *invalid_page.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                locations(&json!([page("one.rs"), invalid_page])).is_err(),
                "{pointer}"
            );
        }
        let mut missing = page("two.rs");
        missing.as_object_mut().unwrap().remove("files");
        assert!(locations(&json!([page("one.rs"), missing])).is_err());
        let mut ambiguous = page("one.rs");
        ambiguous["files"]
            .as_array_mut()
            .unwrap()
            .push(page("two.rs")["files"][0].clone());
        ambiguous["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("file");
        assert!(locations(&ambiguous).is_err());
    }

    #[test]
    fn compact_expansion_retains_ambiguity_continuations_and_symbol_id_refusal() -> Result<()> {
        let mut bound = step("structure", "file_info", json!({}));
        bound.bind.push(Binding {
            arg: "path".into(),
            from: "find".into(),
            select: "unique_definition_path".into(),
        });
        let mut outputs =
            BTreeMap::from([("find".into(), json!([page("one.rs"), page("two.rs")]))]);
        assert!(
            expand(&bound, &outputs, 12).is_err(),
            "multiple definitions need a choice"
        );
        let mut partial = page("one.rs");
        partial["cursor"] = json!("next");
        outputs.insert("find".into(), partial);
        assert!(
            expand(&bound, &outputs, 12).is_err(),
            "a partial page cannot prove uniqueness"
        );
        outputs.insert("find".into(), page("one.rs"));
        assert_eq!(expand(&bound, &outputs, 12)?.0[0]["path"], "one.rs");
        bound.bind[0].select = "selected_symbol_ids".into();
        let error = expand(&bound, &outputs, 12).unwrap_err().to_string();
        assert!(error.contains("explicit compact:false"), "{error}");
        bound.bind[0].select = "hit_locations".into();
        let (args, omitted) = expand(&bound, &outputs, 0)?;
        assert!(args.is_empty() && omitted, "fanout exhaustion is disclosed");
        Ok(())
    }

    #[test]
    fn compact_plan_admission_fences_scope_revision_and_malformed_pages() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "pub struct Queue;\n")?;
        let kernel = crate::Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let publication = kernel.status_summary()?["publication"]["token"].clone();
        for case in ["outside", "malformed", "revision", "valid", "bounded"] {
            let mut value = page(if case == "outside" {
                "outside.rs"
            } else {
                "allowed/one.rs"
            });
            value["publication"] = json!({"token":publication});
            if case == "malformed" {
                value["items"][0]["file"] = json!(99);
            }
            if case == "revision" {
                value["publication"]["token"] = json!("superseded");
            }
            if case == "bounded" {
                value["items"][0]["name"] = json!("Q".repeat(6000));
            }
            let mut request = json!({"schema":"airk.parse.request/1","mode":"execute","path_prefix":"allowed/",
                "plan":{"path_prefix":"allowed/","steps":[{"id":"find","tool":"symbols","args":{"name":"Queue","compact":true}}]}});
            if case == "bounded" {
                request["output"] = json!({"max_bytes":4096});
                // Keep the original small-output projection contract. Enabled
                // verification needs separate space for its complete receipt.
                request["verification"] = json!("off");
            }
            let result = kernel.evidence_plan_with_reader(
                serde_json::from_value(request)?,
                &Value::Null,
                Some(&|_, _, _| Ok(value.clone())),
            )?;
            match case {
                "valid" => {
                    assert_eq!(result.status, "complete");
                    assert_eq!(result.evidence.len(), 1);
                }
                "bounded" => {
                    assert!(result.truncated);
                    assert!(serde_json::to_vec(&result)?.len() <= 4096);
                }
                _ => {
                    assert!(result.evidence.is_empty(), "{case}");
                    assert_eq!(result.steps[0].status, "failed", "{case}");
                    assert!(
                        result.steps[0].result.is_none(),
                        "rejected raw page cannot leak: {case}"
                    );
                }
            }
        }
        Ok(())
    }
}
