// SPDX-License-Identifier: Apache-2.0
//! Transfer across indexed publications with a deterministic discovery order.

use super::*;
use crate::current::CurrentSearchMode;
use airk_core::{SearchQuery, evidence::EvidenceReadOptions};

const QUESTION: &str = r#"Fix the "Build preview" failure for Nimbus. Locate the preflight decision and compare the working provider."#;
const UI: &str = "public/ui/preview.ts";
const GUIDE: &str = "public/docs/preview.md";
const OLD_TARGET: &str = "public/worker/preview.ts";
const MOVED_TARGET: &str = "public/router/registration.ts";
const OLD_ACTION: &str = "draft.load-preview";
const MOVED_ACTION: &str = "panel.prepare-draft";
const FAILED: &str = "nimbus_preflight_refusal";
const CONTROL: &str = "cedar_network_control";
const FOREIGN: &str = "foreign_registration";

fn indexed_fixture(action: &str, target: &str, branch: Option<&str>) -> Result<Fixture> {
    let ui = format!(
        "const label = \"Build preview\";\n\
         export function bindPreview(button, bridge) {{\n\
             button.bind(label, () => {{\n\
                 bridge.emit(\"{action}\", button.activeProvider);\n\
             }});\n\
         }}\n"
    );
    let branch = branch.map_or_else(String::new, |marker| {
        format!("  if (provider === \"Nimbus\") return reject(\"{marker}\");\n")
    });
    let owner_source = format!(
        "bridge.register(\"{action}\", (provider) => {{\n\
         {branch}\
           if (provider === \"Cedar\") return networkSend(\"{CONTROL}\");\n\
         }});\n"
    );
    let owner = Arc::new(Owner::new());
    fixture_with_sources(
        owner.clone(),
        owner,
        &[
            (
                GUIDE,
                "# Build preview\nNimbus can show an error. This guide blames the network.\n",
            ),
            (UI, &ui),
            (target, &owner_source),
        ],
    )
}

fn bound_options(
    revision: Option<&str>,
    arguments: &Map<String, Value>,
) -> Result<EvidenceReadOptions> {
    Ok(serde_json::from_value(json!({
        "revision":revision,"known_hash":arguments.get("known_hash")
    }))?)
}

fn field<'a>(arguments: &'a Map<String, Value>, name: &str) -> Result<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing {name}"))
}

fn number(arguments: &Map<String, Value>, name: &str) -> Result<usize> {
    let value = arguments
        .get(name)
        .and_then(Value::as_u64)
        .with_context(|| format!("missing {name}"))?;
    Ok(usize::try_from(value)?)
}

fn transfer(fixture: &Fixture) -> Result<AnswerOutcome> {
    let clock = Clock::new();
    let kernel = &fixture.kernel;
    let publication = &fixture.publication;
    let expected = publication.token.to_string();
    let options = EvidenceReadOptions {
        revision: Some(publication.token),
        ..EvidenceReadOptions::default()
    };
    let guide = kernel
        .read_file_conditional(GUIDE, 1, 2, 3072, &options)?
        .content
        .context("guide was not retained")?
        .hits;
    let ui = kernel
        .read_file_conditional(UI, 1, 6, 3072, &options)?
        .content
        .context("visible control was not retained")?
        .hits;
    assert_eq!(guide.len(), 1);
    assert_eq!(ui.len(), 1);
    kernel.answer_with(
        request(QUESTION),
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, observed_revision| {
                ensure!(observed_revision == Some(expected.as_str()));
                let value = match tool {
                    "search" if arguments.get("mode") == Some(&json!("lexical")) => {
                        let query = SearchQuery {
                            text: field(arguments, "query")?.to_owned(),
                            path_prefix: arguments.get("path_prefix").and_then(Value::as_str).map(str::to_owned),
                            limit: number(arguments, "limit")?,
                            max_bytes: number(arguments, "max_bytes")?,
                        };
                        serde_json::to_value(kernel.search_response(
                            &query, CurrentSearchMode::Lexical, None,
                        )?)?
                    }
                    // The misleading guide outranks the actual UI record;
                    // both entries here are genuine reads of the indexed fixture.
                    "search" => json!({"publication":publication,"generation":publication.token.generation,
                                      "collection":"fixture","truncated":false,"hits":[guide[0],ui[0]]}),
                    "read_file" => serde_json::to_value(kernel.read_file_conditional(
                        field(arguments, "path")?,
                        number(arguments, "start_line")?,
                        number(arguments, "end_line")?,
                        number(arguments, "max_bytes")?,
                        &bound_options(observed_revision, arguments)?,
                    )?)?,
                    "context" => serde_json::to_value(kernel.context_conditional(
                        field(arguments, "path")?,
                        number(arguments, "line")?,
                        number(arguments, "before")?,
                        number(arguments, "after")?,
                        number(arguments, "max_bytes")?,
                        &bound_options(observed_revision, arguments)?,
                    )?)?,
                    _ => json!({"publication":publication,"hits":[],"items":[],"facts":[],"relationships":[]}),
                };
                Ok(value)
            },
            clock: &|| clock.now(),
            progress: &|_| {},
        },
    )
}

fn verify_route(
    fixture: &Fixture,
    outcome: &AnswerOutcome,
    action: &str,
    target: &str,
) -> Result<()> {
    assert_eq!(outcome.response.scope.publication, fixture.publication);
    outcome
        .response
        .validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    let records = &outcome.custody.records;
    let missing = |stage: &str| {
        let reads: Vec<_> = records
            .iter()
            .map(|record| (&record.tool, &record.arguments))
            .collect();
        format!(
            "{stage} for {action} at {target}; parts={:?}; reads={reads:?}",
            outcome.response.parts
        )
    };
    let origin = records
        .iter()
        .position(|record| {
            matches!(record.tool.as_str(), "context" | "read_file")
                && record.arguments["path"] == UI
                && record.canonical_result["hits"]
                    .as_array()
                    .is_some_and(|hits| {
                        hits.iter().any(|hit| {
                            hit["text"]
                                .as_str()
                                .is_some_and(|text| text.contains(action))
                        })
                    })
        })
        .with_context(|| missing("a genuine source read must identify the action"))?;
    let discovery = records
        .iter()
        .position(|record| {
            record.tool == "search"
                && record.arguments.get("mode").and_then(Value::as_str) == Some("lexical")
                && record.arguments["query"] == action
        })
        .with_context(|| missing("the action derived from source must drive the next Search"))?;
    let receiver = records
        .iter()
        .position(|record| {
            record.tool == "read_file"
                && record.arguments["path"] == target
                && record.arguments.contains_key("known_hash")
        })
        .with_context(|| missing("the discovered target must be reopened by a hash-bound read"))?;
    assert!(origin < discovery && discovery < receiver);
    assert_eq!(
        records[receiver].canonical_result["read"]["publication"]["token"],
        fixture.publication.token.to_string()
    );
    assert_eq!(
        records[receiver].arguments["known_hash"],
        records[receiver].canonical_result["hits"][0]["citation"]["content_hash"]
    );
    assert!(
        records[receiver].canonical_result["hits"][0]["text"]
            .as_str()
            .context("receiver text is unavailable")?
            .contains(CONTROL)
    );
    let delivered = serde_json::to_string(&outcome.response.records)?;
    assert!(delivered.contains(CONTROL));
    assert!(!delivered.contains(FOREIGN));
    Ok(())
}

#[test]
fn quoted_button_follows_renamed_receiver_within_selected_publication() -> Result<()> {
    let old = indexed_fixture(OLD_ACTION, OLD_TARGET, Some(FAILED))?;
    let moved = indexed_fixture(MOVED_ACTION, MOVED_TARGET, Some(FAILED))?;
    let foreign = indexed_fixture(MOVED_ACTION, OLD_TARGET, Some(FOREIGN))?;
    let missing = indexed_fixture(MOVED_ACTION, MOVED_TARGET, None)?;

    // Same relative old path, different indexed bytes and identity. Passing
    // the foreign content hash to the authorized old reader must fail before
    // it can hand the caller any source from that address.
    let foreign_source = foreign.kernel.read_file(OLD_TARGET, 1, 1, 3072)?;
    let foreign_hash = foreign_source.hits[0].citation.content_hash.parse()?;
    assert_ne!(old.publication.token, foreign.publication.token);
    let error = old
        .kernel
        .read_file_conditional(
            OLD_TARGET,
            1,
            1,
            3072,
            &EvidenceReadOptions {
                revision: Some(old.publication.token),
                known_hash: Some(foreign_hash),
                ..EvidenceReadOptions::default()
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("known_hash does not match"));

    let old_result = transfer(&old)?;
    verify_route(&old, &old_result, OLD_ACTION, OLD_TARGET)?;
    assert!(serde_json::to_string(&old_result.response.records)?.contains(FAILED));
    let moved_result = transfer(&moved)?;
    verify_route(&moved, &moved_result, MOVED_ACTION, MOVED_TARGET)?;
    let moved_reply = serde_json::to_string(&moved_result.response.records)?;
    assert!(moved_reply.contains(FAILED));
    assert!(!moved_reply.contains(OLD_TARGET));
    assert!(!moved_reply.contains(OLD_ACTION));

    let missing_result = transfer(&missing)?;
    verify_route(&missing, &missing_result, MOVED_ACTION, MOVED_TARGET)?;
    let missing_reply = serde_json::to_string(&missing_result.response.records)?;
    assert!(!missing_reply.contains(FAILED));
    assert!(
        missing_result
            .response
            .parts
            .iter()
            .flat_map(|part| &part.sub_needs)
            .any(|need| need.text.contains("failure") && need.coverage != v3::Coverage::Covered),
        "a working control without the requested failure branch must leave the decision unresolved",
    );
    Ok(())
}
