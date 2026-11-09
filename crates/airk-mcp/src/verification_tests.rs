// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

fn body(reply: &CallToolResult) -> Result<Value> {
    Ok(serde_json::from_str(
        &reply.content[0].as_text().unwrap().text,
    )?)
}
fn controls(mode: &str) -> Value {
    json!({"verification":mode,"verification_max_sources":25,"verification_max_bytes":16777216,"verification_deadline_ms":2000})
}

// Typed metadata-only fixture for the exact native delivery serialization.
// Actual service validation is qualified by the CLI/MCP transport regression.
fn fixture_delivery_body(mut body: Value) -> Result<Value> {
    let receipt = airk_core::service::CombinedDeliveryReceipt {
        schema: 1,
        service_instance: airk_core::identity::Incarnation::from_bytes([1; 16]),
        namespace: airk_core::identity::Incarnation::from_bytes([2; 16]),
        catalog_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
        scope_identity: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
        result_digest: airk_core::identity::framed_digest(
            b"airk/combined-delivery-result/1",
            &[&serde_json::to_vec(&body)?],
        ),
    };
    body["delivery_observation"] =
        json!({"receipt":receipt,"digest_basis":"canonical_body_without_delivery_observation"});
    Ok(body)
}

#[tokio::test]
async fn full_capability_catalog_fits_installed_body_budget_without_schema_loss() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let server = AirkServer::new(Kernel::open(source.path(), Some(data.path().into()))?);
    let reply = server
        .capabilities(Parameters(serde_json::from_value(json!({
            "response_mode": "full"
        }))?))
        .await;
    ensure!(
        !reply.is_error.unwrap_or(false),
        "canonical capabilities failed: {reply:?}"
    );
    let complete = body(&reply)?;
    let bytes = serde_json::to_vec(&complete)?.len();
    eprintln!("canonical full capabilities: {bytes} bytes; installed body budget: 65536");
    ensure!(
        bytes <= 65_536,
        "full capabilities exceed the installed body budget: {bytes}"
    );
    assert_eq!(
        complete["workbench"]["operations"],
        airk_kernel::workbench::operation_catalog()["operations"]
    );
    Ok(())
}

#[tokio::test]
async fn actual_combined_capabilities_and_parse_fit_their_public_body_budgets() -> Result<()> {
    use airk_core::{
        combined_reads::{MemberAvailability, QuerySelection, SharedBudgets},
        identity::{Incarnation, Sha256Digest},
        index_catalog::{
            CatalogRevision, GrantId, GrantedMembership, IndexId, Membership, PathScope,
            QueryProjectId, ReviewedGrant, ScopeAtom, SourceRootId,
        },
    };
    use airk_kernel::combined_reads::{CompositionRequest, MemberInput, compose_capabilities};

    let size = |value: &Value| -> Result<usize> { Ok(serde_json::to_vec(value)?.len()) };
    let identity = Sha256Digest::from_bytes([7; 32]);
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let second_project = QueryProjectId::from_identity(Incarnation::from_bytes([2; 16]));
    let indexes = [1_u8, 2].map(|n| IndexId::from_identity(Incarnation::from_bytes([n; 16])));
    let grant = ReviewedGrant {
        id: GrantId::from_identity(Incarnation::from_bytes([1; 16])),
        host: identity,
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: [
            (project, 0, "api"),
            (second_project, 0, "impl"),
            (project, 1, "api"),
        ]
        .into_iter()
        .map(|(project_id, position, path)| GrantedMembership {
            membership: Membership {
                project_id,
                index_id: indexes[position],
                revision: CatalogRevision::INITIAL,
                scope: vec![ScopeAtom {
                    source_root_id: SourceRootId::from_identity(Incarnation::from_bytes(
                        [position as u8 + 1; 16],
                    )),
                    kind: PathScope::Subtree,
                    path: path.into(),
                }],
            },
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        })
        .collect(),
    };
    let mut fixtures = Vec::new();
    for member in 1..=2 {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        for folder in ["api", "impl"] {
            std::fs::create_dir(source.path().join(folder))?;
            std::fs::write(
                source.path().join(folder).join("member.md"),
                format!("# Member {member} {folder}\nIndependent indexed source.\n"),
            )?;
        }
        let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        fixtures.push((source, data, kernel));
    }
    let mut native_bodies = Vec::new();
    for (position, (_, _, kernel)) in fixtures.iter().enumerate() {
        let backing = airk_core::service::ProjectRegistration::new(
            kernel
                .root()
                .to_str()
                .context("fixture source path")?
                .into(),
            kernel
                .data_dir()
                .to_str()
                .context("fixture data path")?
                .into(),
            kernel.collection().into(),
        )?
        .binding;
        let authority = airk_kernel::scoped_reads::ScopedReadAuthority {
            schema: 1,
            backing,
            index_id: indexes[position],
            project_ids: if position == 0 {
                vec![project, second_project]
            } else {
                vec![project]
            },
            provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
            grant: grant.clone(),
            sources: vec![airk_core::scoped_reads::SourceDescriptor {
                source_root_id: SourceRootId::from_identity(Incarnation::from_bytes(
                    [position as u8 + 1; 16],
                )),
                original_root: kernel
                    .root()
                    .to_str()
                    .context("fixture source path")?
                    .into(),
            }],
        };
        let server = AirkServer::new(kernel.with_scoped_reads(authority)?);
        // The installed combined adapter requests full child status. Actual
        // independent publications must not be hidden by cloning one member.
        let reply = server
            .capabilities(Parameters(serde_json::from_value(json!({
                "response_mode":"full", "include_status":true
            }))?))
            .await;
        ensure!(
            !reply.is_error.unwrap_or(false),
            "actual capabilities failed"
        );
        let native = body(&reply)?;
        eprintln!(
            "actual scoped indexed member {} capability bytes: full={}, member_workbench={}, scope_observation={}",
            position + 1,
            size(&native)?,
            size(&native["workbench"])?,
            size(&native["scope_observation"])?
        );
        for (field, value) in native.as_object().context("capability body")? {
            eprintln!(
                "actual member {} field {field}: {} bytes",
                position + 1,
                size(value)?
            );
        }
        native_bodies.push(native);
    }
    assert_ne!(
        native_bodies[0], native_bodies[1],
        "indexed member observations must be independent"
    );
    let combined_catalog = airk_kernel::workbench::combined::operation_catalog();
    eprintln!(
        "actual combined_workbench bytes={}",
        size(&combined_catalog)?
    );
    let mut failures = Vec::new();
    for count in [1_u8, 2] {
        let indexes = indexes[..usize::from(count)].to_vec();
        let selection = QuerySelection {
            project_ids: vec![project, second_project],
            index_ids: Some(indexes.clone()),
            member_availability: MemberAvailability::Report,
        };
        let request = CompositionRequest {
            selection: selection.resolve(&grant)?,
            budgets: SharedBudgets {
                limit: usize::from(count),
                max_bytes: airk_core::MAX_RESPONSE_BYTES,
                deadline_ms: 2000,
                verification_max_sources: 8,
                verification_max_bytes: 16384,
                verification_deadline_ms: 2000,
            },
            authority: identity,
            observed_from_unix_ms: 10,
            observed_to_unix_ms: 20,
        };
        let inputs = indexes
            .iter()
            .enumerate()
            .map(|(position, index)| MemberInput {
                index_id: *index,
                result: native_bodies[position].clone(),
                is_error: false,
                exhausted: false,
                sources: vec![],
            })
            .collect();
        let mut combined = serde_json::to_value(compose_capabilities(&request, inputs)?)?;
        for (position, index) in indexes.into_iter().enumerate() {
            assert_eq!(
                airk_core::combined_reads::shared_values::materialize_combined_member_result(
                    &combined, index
                )?,
                native_bodies[position],
                "complete actual member reply must round-trip"
            );
        }
        let before_catalog = size(&combined)?;
        // Match the public combined adapter's post-composition catalog addition.
        combined["workbench"] = combined_catalog.clone();
        let combined_bytes = size(&combined)?;
        let delivered_combined = fixture_delivery_body(combined.clone())?;
        let delivered_bytes = size(&delivered_combined)?;
        let mut original = delivered_combined.clone();
        original
            .as_object_mut()
            .context("combined body")?
            .remove("delivery_observation");
        assert_eq!(
            original, combined,
            "delivery adds metadata without changing native data"
        );
        let callback_reply = json!({"content":[{"type":"text","text":delivered_combined.to_string()}],
            "structuredContent":delivered_combined,"isError":false});
        let parse_request = airk_core::workbench::decode_request(json!({
            "schema": airk_core::workbench::REQUEST_SCHEMA, "mode":"execute",
            "plan":{"steps":[{"id":"capabilities","tool":"capabilities","args":{}}]}
        }))?;
        let parsed = airk_kernel::workbench::combined::execute(
            parse_request,
            selection,
            identity,
            |tool, _| {
                assert_eq!(tool, "capabilities");
                let reply = callback_reply.clone();
                async move { Ok(reply) }
            },
        )
        .await?;
        let parsed_bytes = serde_json::to_vec(&parsed)?.len();
        eprintln!(
            "actual {count}-member bytes: composed={before_catalog}, with_combined_catalog={combined_bytes}, callback_envelope={}, parse={parsed_bytes}, parse_output_budget={}",
            size(&callback_reply)?,
            parsed.plan.output.max_bytes
        );
        if parsed.truncated || parsed.status != "complete" {
            failures.push(format!(
                "{count}-member parse status={} truncated={}",
                parsed.status, parsed.truncated
            ));
        }
        assert_eq!(parsed.steps.len(), 1);
        assert_eq!(parsed.steps[0].status, "complete");
        assert_eq!(parsed.steps[0].result, Some(callback_reply.clone()));
        ensure!(
            parsed_bytes <= parsed.plan.output.max_bytes,
            "actual parse exceeds its declared plan output budget"
        );
        if combined_bytes > airk_core::MAX_RESPONSE_BYTES
            || delivered_bytes > airk_core::MAX_RESPONSE_BYTES
        {
            failures.push(format!(
                "{count}-member capabilities pre={combined_bytes} delivered={delivered_bytes}"
            ));
        }
        eprintln!(
            "actual {count}-member ordinary pre-delivery={combined_bytes}, typed-delivered={delivered_bytes}, receipt-overhead={}, fits={} (max={})",
            delivered_bytes - combined_bytes,
            delivered_bytes <= airk_core::MAX_RESPONSE_BYTES,
            airk_core::MAX_RESPONSE_BYTES
        );
        let output_budget = parsed.plan.output.max_bytes;
        let parsed_body = fixture_delivery_body(serde_json::to_value(parsed)?)?;
        ensure!(
            size(&parsed_body)? <= output_budget,
            "complete parse with typed receipt must fit its declared output budget"
        );
        let parsed_reply = json!({"content":[{"type":"text","text":parsed_body.to_string()}],
            "structuredContent":parsed_body,"isError":false});
        ensure!(
            size(&parsed_reply)? <= (3 * output_budget + 4096).min(1024 * 1024 - 2048),
            "complete parse transport must retain the existing public framing headroom"
        );
    }
    ensure!(
        failures.is_empty(),
        "installed public body guard rejects canonical results: {}",
        failures.join(", ")
    );
    Ok(())
}

#[tokio::test]
async fn search_execution_survives_verification_results_and_all_reply_projections() -> Result<()> {
    use airk_core::{
        SearchFilters, SearchQuery,
        reply::ReplyDetail,
        search_execution::{IncompleteReason, RequestedMode, SearchExecution, ServedMode},
    };
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    let source = root.join("queue.md");
    let original = "# Queue\nQueue evidence remains independently verifiable.\n";
    std::fs::write(&source, original)?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut materialized = serde_json::to_value(kernel.search_response_with_navigation(
        &SearchQuery {
            text: "Queue".into(),
            path_prefix: None,
            limit: 10,
            max_bytes: 8192,
        },
        airk_kernel::current::CurrentSearchMode::Lexical,
        None,
        &SearchFilters::default(),
    )?)?;
    let execution = serde_json::to_value(SearchExecution::new(
        RequestedMode::Default,
        ServedMode::Lexical,
        Some(IncompleteReason::HelperUnavailable),
    ))?;
    // Exercise actual framing, projection and source verification of a completed
    // search body. The separate native outage fixture establishes its producer.
    materialized["execution"] = execution.clone();
    assert!(!materialized["hits"].as_array().unwrap().is_empty());
    let server = AirkServer::new(kernel);
    for response_mode in [response::ResponseMode::Terse, response::ResponseMode::Full] {
        for detail in [ReplyDetail::Lean, ReplyDetail::Rich] {
            for (verification, changed) in [("report", false), ("require", true), ("off", true)] {
                std::fs::write(
                    &source,
                    if changed {
                        "Changed saved source bytes.\n"
                    } else {
                        original
                    },
                )?;
                let value = materialized.clone();
                let verification_controls = if verification == "off" {
                    // Off intentionally rejects verification-only budgets.
                    airk_core::verification::VerificationControls::off()
                } else {
                    serde_json::from_value(controls(verification))?
                };
                let reply = server
                    .execute_verified_response(
                        UsageOperation::Search,
                        response::ResponseArgs {
                            reply_detail: Some(detail),
                            response_mode: Some(response_mode),
                            include_status: false,
                        },
                        verification_controls,
                        move |_| Ok(value),
                    )
                    .await;
                let body = body(&reply).with_context(|| {
                    let text: String = reply.content[0]
                        .as_text()
                        .map_or("<non-text>", |content| content.text.as_str())
                        .chars()
                        .take(1024)
                        .collect();
                    format!(
                        "search projection case mode={response_mode:?}, detail={detail:?}, verification={verification}, is_error={:?}, body={text:?}",
                        reply.is_error
                    )
                })?;
                assert_eq!(body["execution"], execution);
                assert_eq!(body["hits"], materialized["hits"]);
                assert_eq!(
                    body["publication"]["token"],
                    materialized["publication"]["token"]
                );
                assert_eq!(body["reply_detail"], detail.as_str());
                match verification {
                    "report" => {
                        assert_ne!(reply.is_error, Some(true), "{body}");
                        assert_eq!(body["source_verification"]["status"], "verified");
                        assert_eq!(body["execution"]["incomplete"], true);
                    }
                    "require" => {
                        assert_eq!(reply.is_error, Some(true), "{body}");
                        assert_eq!(body["source_verification"]["status"], "requirements_unmet");
                        assert_eq!(
                            body["source_verification"]["sources"][0]["state"],
                            "mismatch"
                        );
                    }
                    _ => {
                        assert_ne!(reply.is_error, Some(true), "{body}");
                        assert!(body.get("source_verification").is_none());
                    }
                }
                if verification != "off" {
                    airk_core::verification::validate_projection(&body)?;
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn verification_refusal_does_not_acknowledge_undelivered_observations() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("policy.md"), "# Queue\nSaved source.\n")?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let server = AirkServer::new(kernel);
    let before = server.responses.receipt();
    let reply = server
        .execute_verified_response(
            airk_core::usage::UsageOperation::FileInfo,
            Default::default(),
            Default::default(),
            |kernel| {
                let mut value = serde_json::to_value(
                    kernel.file_info_conditional("policy.md", &Default::default())?,
                )?;
                value["padding"] = "x".repeat(65536).into();
                Ok(value)
            },
        )
        .await;
    assert_eq!(reply.is_error, Some(true), "{reply:?}");
    assert_eq!(body(&reply)?["error"]["code"], "verification_reply_budget");
    assert_eq!(server.responses.receipt(), before);
    Ok(())
}

#[tokio::test]
async fn real_mcp_checks_saved_sources_preserves_failures_and_keeps_off_unchanged() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(
        root.join("policy.md"),
        "# Queue\nPreserve original revision identity.\n",
    )?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let mut client = local::LocalTools::new(kernel.clone()).await?;
    let mut args = controls("report");
    args["path"] = json!("policy.md");
    args["response_mode"] = json!("full");
    let checked = client
        .call("airk_file_info".into(), args.as_object().unwrap().clone())
        .await?;
    ensure!(checked.is_error != Some(true), "{checked:?}");
    let value = body(&checked)?;
    assert_eq!(value["source_verification"]["status"], "verified");
    assert_eq!(
        value["source_verification"]["counts"]["requested_sources"],
        1
    );
    assert_eq!(value["source_verification"]["counts"]["references"], 2);
    for reference in value["source_verification"]["references"]
        .as_array()
        .unwrap()
    {
        assert!(
            value
                .pointer(reference["result_pointer"].as_str().unwrap())
                .is_some()
        );
    }
    let publication = value["publication"].clone();
    let default = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    ensure!(default.is_error != Some(true), "{default:?}");
    let default = body(&default)?;
    assert_eq!(default["source_verification"]["mode"], "report");
    assert_eq!(default["source_verification"]["status"], "verified");
    assert_eq!(default["source_verification"]["limits"]["max_sources"], 8);
    assert_eq!(
        default["source_verification"]["limits"]["max_bytes"],
        16777216
    );
    assert_eq!(
        default["source_verification"]["limits"]["deadline_ms"],
        2000
    );
    let terse = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","response_mode":"terse"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    let terse = body(&terse)?;
    assert_eq!(terse["source_verification"]["status"], "verified");
    airk_core::verification::validate_projection(&terse)?;
    let metadata = client
        .call("airk_status".into(), Default::default())
        .await?;
    assert!(body(&metadata)?.get("source_verification").is_none());
    std::fs::remove_file(root.join("policy.md"))?;
    let off = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","verification":"off","response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    ensure!(off.is_error != Some(true), "{off:?}");
    assert!(body(&off)?.get("source_verification").is_none());
    args["verification"] = json!("require");
    let failed = client
        .call("airk_file_info".into(), args.as_object().unwrap().clone())
        .await?;
    assert_eq!(failed.is_error, Some(true));
    let failure = body(&failed)?;
    assert_eq!(
        failure["source_verification"]["status"],
        "requirements_unmet"
    );
    assert_eq!(
        failure["source_verification"]["sources"][0]["state"],
        "missing"
    );
    assert_eq!(
        failure["source_verification"]["error"]["code"],
        "verification_required_unmet"
    );
    assert!(
        failed.structured_content.is_none(),
        "native error text retains complete JSON without duplicating the bounded IPC body"
    );
    assert_eq!(failure["publication"], publication);
    assert_eq!(failure["content_hash"], value["content_hash"]);
    assert!(failure["source_verification"]["counts"]["bytes_read"] == 0);
    for bad in [
        json!({"verification":"report","verification_max_sources":0}),
        json!({"verification":"off","verification_max_bytes":1}),
        json!({"verification":null}),
    ] {
        let mut bad = bad.as_object().unwrap().clone();
        bad.insert("path".into(), json!("policy.md"));
        assert_eq!(
            client.call("airk_file_info".into(), bad).await?.is_error,
            Some(true)
        );
    }
    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn global_and_step_verification_use_real_plan_handlers_and_bind_prepared_controls()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    let source = "# Queue\nSaved source verification preserves receipts.\n";
    std::fs::write(root.join("policy.md"), source)?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let mut client = local::LocalTools::new(kernel).await?;
    let mut request = controls("report");
    request["schema"] = json!("airk.parse.request/1");
    request["plan"] = json!({"steps":[{"id":"first","tool":"file_info","args":{"path":"policy.md"}},{"id":"again","tool":"read_file","args":{"path":"policy.md","start_line":1,"end_line":2}}]});
    let prepared = client
        .call("airk_parse".into(), request.as_object().unwrap().clone())
        .await?;
    ensure!(prepared.is_error != Some(true), "{prepared:?}");
    let prepared = body(&prepared)?;
    assert!(prepared.get("source_verification").is_none());
    assert_eq!(prepared["plan"]["verification"], "report");
    let execute = json!({"schema":"airk.parse.request/1","mode":"execute","plan":prepared["plan"],"response_mode":"full"});
    let run = client
        .call("airk_parse".into(), execute.as_object().unwrap().clone())
        .await?;
    ensure!(run.is_error != Some(true), "{run:?}");
    let run = body(&run)?;
    assert_eq!(run["status"], "complete");
    assert_eq!(run["source_verification"]["status"], "verified");
    assert!(run["warnings"].as_array().unwrap().iter().any(|warning| {
        warning
            .as_str()
            .is_some_and(|text| text.contains("Saved sources verified at"))
    }));
    assert_eq!(run["source_verification"]["counts"]["requested_sources"], 1);
    assert_eq!(
        run["source_verification"]["counts"]["bytes_read"],
        source.len()
    );
    assert!(
        run["source_verification"]["counts"]["references"]
            .as_u64()
            .unwrap()
            > 2
    );
    for reference in run["source_verification"]["references"].as_array().unwrap() {
        assert!(
            run.pointer(reference["result_pointer"].as_str().unwrap())
                .is_some()
        );
    }
    let mut edited = execute.clone();
    edited["verification"] = json!("require");
    edited["verification_max_sources"] = json!(25);
    edited["verification_max_bytes"] = json!(16777216);
    edited["verification_deadline_ms"] = json!(2000);
    assert_eq!(
        client
            .call("airk_parse".into(), edited.as_object().unwrap().clone())
            .await?
            .is_error,
        Some(true)
    );
    // Preparing a metadata-only plan serializes the default controls but must
    // never turn its eventual execution into a source verification request.
    let metadata = json!({"schema":"airk.parse.request/1","plan":{"steps":[{"id":"health","tool":"status","args":{}}]}});
    let prepared = client
        .call("airk_parse".into(), metadata.as_object().unwrap().clone())
        .await?;
    let prepared = body(&prepared)?;
    let execute_metadata =
        json!({"schema":"airk.parse.request/1","mode":"execute","plan":prepared["plan"]});
    let metadata = client
        .call(
            "airk_parse".into(),
            execute_metadata.as_object().unwrap().clone(),
        )
        .await?;
    ensure!(metadata.is_error != Some(true), "{metadata:?}");
    let metadata = body(&metadata)?;
    assert!(metadata.get("source_verification").is_none());
    assert!(
        metadata["steps"][0]["result"]
            .get("source_verification")
            .is_none()
    );
    let mut step = controls("report");
    step["path"] = json!("policy.md");
    let per_step = json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"check","tool":"file_info","args":step}]}});
    let checked = client
        .call("airk_parse".into(), per_step.as_object().unwrap().clone())
        .await?;
    ensure!(checked.is_error != Some(true), "{checked:?}");
    let checked = body(&checked)?;
    assert_eq!(
        checked["steps"][0]["result"]["source_verification"]["status"],
        "verified"
    );
    let mut evidence_only = per_step.clone();
    evidence_only["plan"]["output"] = json!({"view":"evidence","max_bytes":262144});
    let rejected = client
        .call(
            "airk_parse".into(),
            evidence_only.as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(rejected.is_error, Some(true));
    assert!(
        rejected.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("retain complete step receipts")
    );
    let mut conflict = per_step.clone();
    conflict
        .as_object_mut()
        .unwrap()
        .extend(controls("report").as_object().unwrap().clone());
    assert_eq!(
        client
            .call("airk_parse".into(), conflict.as_object().unwrap().clone())
            .await?
            .is_error,
        Some(true)
    );
    std::fs::write(root.join("policy.md"), "changed saved bytes\n")?;
    let required = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"require",
        "plan":{"steps":[{"id":"check","tool":"file_info","args":{"path":"policy.md"}}]}});
    let failed = client
        .call("airk_parse".into(), required.as_object().unwrap().clone())
        .await?;
    assert_eq!(failed.is_error, Some(true));
    let failed = body(&failed)?;
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["error"]["code"], "verification_required_unmet");
    assert_eq!(
        failed["source_verification"]["sources"][0]["state"],
        "mismatch"
    );
    assert_eq!(
        failed["steps"][0]["result"]["content_hash"],
        run["steps"][0]["result"]["content_hash"]
    );
    client.close().await?;
    Ok(())
}

#[test]
fn only_evidence_and_parse_catalogs_offer_explicit_verification_controls() {
    for tool in local::tools() {
        let offered = tool.input_schema["properties"]
            .get("verification")
            .is_some();
        assert_eq!(
            offered,
            !matches!(
                tool.name.as_ref(),
                "airk_status"
                    | "airk_diagnostics"
                    | "airk_capabilities"
                    | "airk_help"
                    | "airk_query_scope"
            ),
            "{}",
            tool.name
        );
        if offered {
            assert_eq!(
                tool.input_schema["properties"]["verification"]["enum"],
                json!(["off", "report", "require"])
            );
            for field in [
                "verification_max_sources",
                "verification_max_bytes",
                "verification_deadline_ms",
            ] {
                assert_eq!(tool.input_schema["properties"][field]["type"], "integer");
            }
        }
    }
}

#[tokio::test]
async fn cancelling_an_accepted_verification_keeps_capacity_until_the_blocking_owner_returns()
-> Result<()> {
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("policy.md"), "# Queue\nSaved source.\n")?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let value =
        serde_json::to_value(kernel.file_info_conditional("policy.md", &Default::default())?)?;
    let server = Arc::new(AirkServer::new(kernel));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (receipt_tx, receipt_rx) = tokio::sync::oneshot::channel();
    let controls: airk_core::verification::VerificationControls =
        serde_json::from_value(controls("report"))?;
    let budget = controls.resolve()?.unwrap();
    let request = tokio::spawn({
        let server = server.clone();
        async move {
            server
                .execute_verified_response(
                    UsageOperation::FileInfo,
                    Default::default(),
                    controls,
                    move |kernel| {
                        let _ = started_tx.send(());
                        release_rx.recv_timeout(Duration::from_secs(5))?;
                        let receipt = kernel.verify_returned_sources(&value, budget)?;
                        let _ = receipt_tx.send(receipt);
                        Ok(value)
                    },
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), started_rx).await??;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    // The blocking owner is still accepted; cancelling its waiter did not finish
    // or detach its capacity lifetime. It observes cancellation before source I/O.
    release_tx.send(())?;
    let receipt = tokio::time::timeout(Duration::from_secs(5), receipt_rx).await??;
    assert_eq!(receipt.counts.bytes_read, 0);
    assert_eq!(receipt.counts.checked_sources, 0);
    assert!(receipt.limits.hit.iter().any(|hit| hit == "cancellation"));
    server.admission.shutdown().await;
    Ok(())
}
