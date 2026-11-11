// SPDX-License-Identifier: Apache-2.0
//! Exercise real read rejection without relying on a publication racing a caller.
mod common;
use airk_core::service::{ControlMethod, ControlResponse, JobKind};
use anyhow::{Context, Result, ensure};
use common::read_retry::{self, ReadRetry};
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};

fn envelope(hint: Value) -> Value {
    json!({"isError":true,"content":[{"type":"text","text":hint.to_string()}]})
}

fn single_required_retry(hint: Value) -> Value {
    let index = format!("idx_{}", "01".repeat(16));
    let mut value = json!({"schema":"airk.combined-read/1",
        "selection":{"project_ids":[format!("prj_{}","02".repeat(16))],"index_ids":[index],
            "grant_id":format!("grt_{}","03".repeat(16)),"grant_revision":1,
            "member_availability":"require","identity":"04".repeat(32)},
        "publication_vector":{"authority":"05".repeat(32),"observed_from_unix_ms":1,"observed_to_unix_ms":2,
            "members":[{"index_id":index,"publication":null}]},
        "members":[{"index_id":index,"is_error":true,"outcome":"failed",
            "allocation":{"index_id":index,"limit":1,"max_bytes":4096,"deadline_ms":2000,
                "verification_max_sources":1,"verification_max_bytes":4096,"verification_deadline_ms":2000},
            "result":hint}],"ranking":[],"partial":true,"truncated":false,
        "error":{"code":"member_unavailable","retryable":false,"failed_members":[index]}});
    value["read_retry"] = serde_json::to_value(
        airk_core::combined_reads::read_retry::proposed_retry(&value).expect("safe fixture hint"),
    )
    .unwrap();
    value
}

fn original_scoped_selection() -> airk_core::service::ScopedConnectRequest {
    serde_json::from_value(json!({"schema":1,"context_key":"01".repeat(32),"namespace":"02".repeat(16),
        "expected_catalog_revision":7,"grant_id":format!("grt_{}","03".repeat(16)),"expected_grant_revision":1,
        "index_id":format!("idx_{}","04".repeat(16)),"project_ids":[format!("prj_{}","05".repeat(16))]})).unwrap()
}
fn scoped_setup_hint(expected: &airk_core::service::ScopedConnectRequest) -> Value {
    json!({"error":{"code":"scoped_status_prepare_timeout","query_submitted":false,"retryable":true,
        "fresh_brokerage_required":true,"retry_after_ms":1},"requested_scope":{"confirmed":false,"selection":expected}})
}
fn scoped_setup_envelope(body: Value) -> Value {
    let mut result = envelope(body.clone());
    result["structuredContent"] = body;
    result
}

#[test]
fn scoped_setup_retry_requires_the_original_validated_selection_and_a_known_read_context() {
    let expected = original_scoped_selection();
    let hint = scoped_setup_hint(&expected);
    let refused = scoped_setup_envelope(hint.clone());
    assert!(
        read_retry::classify(&refused).is_none(),
        "generic callers cannot guess original scope"
    );
    assert!(read_retry::classify_for_scope(&refused, &expected).is_some());
    let mut advanced = hint.clone();
    advanced["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
    let advanced = scoped_setup_envelope(advanced);
    let original_receipt = advanced.clone();
    let original_selection = expected.clone();
    assert!(read_retry::classify_for_scope(&advanced, &expected).is_some());
    assert_eq!(
        advanced, original_receipt,
        "retain the exact native receipt"
    );
    assert_eq!(
        expected, original_selection,
        "never rewrite the caller selection"
    );
    assert!(
        read_retry::classify_for_scope(&envelope(hint.clone()), &expected).is_none(),
        "the exact known setup envelope is required"
    );
    for (pointer, value) in [
        ("/error/query_submitted", Value::Null),
        ("/error/retryable", json!(false)),
        ("/error/fresh_brokerage_required", json!(false)),
        ("/error/retry_after_ms", json!(1001)),
        ("/requested_scope/confirmed", json!(true)),
        ("/error/query_submitted", json!(true)),
        (
            "/requested_scope/selection/namespace",
            json!("09".repeat(16)),
        ),
        (
            "/requested_scope/selection/context_key",
            json!("09".repeat(32)),
        ),
        (
            "/requested_scope/selection/index_id",
            json!(format!("idx_{}", "09".repeat(16))),
        ),
        (
            "/requested_scope/selection/expected_catalog_revision",
            json!(6),
        ),
        (
            "/requested_scope/selection/expected_grant_revision",
            json!(2),
        ),
        (
            "/requested_scope/selection/grant_id",
            json!(format!("grt_{}", "09".repeat(16))),
        ),
        (
            "/requested_scope/selection/project_ids",
            json!([format!("prj_{}", "09".repeat(16))]),
        ),
    ] {
        let mut invalid = hint.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            read_retry::classify_for_scope(&scoped_setup_envelope(invalid), &expected).is_none(),
            "{pointer}"
        );
    }
    for (field, value) in [
        ("query_replay_safe", json!(false)),
        ("unexpected", json!(true)),
    ] {
        let mut invalid = hint.clone();
        invalid["error"][field] = value;
        assert!(
            read_retry::classify_for_scope(&scoped_setup_envelope(invalid), &expected).is_none()
        );
    }
    let mut mixed = hint.clone();
    mixed["hits"] = json!([]);
    assert!(read_retry::classify_for_scope(&scoped_setup_envelope(mixed), &expected).is_none());
    let mut native = hint.clone();
    native["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
    native["error"]["code"] = json!("scoped_connection_unavailable");
    native["error"]["native_control_response"] =
        json!({"status":"failure","code":"catalog_connection_busy"});
    assert!(
        read_retry::classify_for_scope(&scoped_setup_envelope(native.clone()), &expected).is_some()
    );
    native["error"]["native_control_response"]["code"] = json!("grant_revoked");
    assert!(read_retry::classify_for_scope(&scoped_setup_envelope(native), &expected).is_none());
    let mut command = std::process::Command::new("airk");
    command.args([
        "--catalog-session",
        &serde_json::to_string(&expected).unwrap(),
        "--service-runtime-dir",
        "owned-control",
        "tools",
        "call",
        "airk_status",
        "--args",
        "{}",
    ]);
    assert_eq!(
        read_retry::original_cli_scope(&command),
        Some(expected.clone())
    );
    for suffix in [
        vec!["tools", "call", "airk_draft_plan"],
        vec!["service", "stop"],
        vec!["job", "index"],
    ] {
        let mut mutation = std::process::Command::new("airk");
        mutation
            .args([
                "--catalog-session",
                &serde_json::to_string(&expected).unwrap(),
            ])
            .args(suffix);
        assert!(read_retry::original_cli_scope(&mutation).is_none());
    }
    let mut duplicate = std::process::Command::new("airk");
    for _ in 0..2 {
        duplicate.args([
            "--catalog-session",
            &serde_json::to_string(&expected).unwrap(),
        ]);
    }
    duplicate.args(["tools", "call", "airk_status"]);
    assert!(read_retry::original_cli_scope(&duplicate).is_none());
}

#[test]
fn combined_replay_requires_exact_outer_single_member_metadata_and_keeps_the_failure() {
    let hint = json!({"status":"retry","code":"selection_busy","retryable":true,
        "action":"repeat_same_request","retry_after_ms":25});
    let combined = single_required_retry(hint.clone());
    assert!(read_retry::classify(&envelope(combined.clone())).is_some());
    let mut framed = envelope(combined.clone());
    framed["structuredContent"] = combined.clone();
    assert!(read_retry::classify(&framed).is_some());
    framed["structuredContent"]["selection"]["identity"] = json!("09".repeat(32));
    assert!(
        read_retry::classify(&framed).is_none(),
        "contradictory outer representations cannot authorize combined replay"
    );
    let mut leaf = envelope(hint.clone());
    leaf["structuredContent"] = json!({"status":"complete"});
    assert!(
        read_retry::classify(&leaf).is_none(),
        "the same representation check applies to standalone retry hints"
    );
    assert_eq!(combined["members"][0]["result"], hint);
    let mut forged_legacy_fallback = combined.clone();
    forged_legacy_fallback["read_retry"]["selection_identity"] = json!("09".repeat(32));
    for (key, value) in hint.as_object().unwrap() {
        forged_legacy_fallback[key] = value.clone();
    }
    assert!(
        read_retry::classify(&envelope(forged_legacy_fallback)).is_none(),
        "an invalid composite cannot fall through to unrelated standalone retry fields"
    );
    for (pointer, replacement) in [
        ("/selection/member_availability", json!("report")),
        ("/read_retry/selection_identity", json!("09".repeat(32))),
        ("/members/0/result/retry_after_ms", json!(0)),
        ("/members/0/result/action", json!("review_required")),
    ] {
        let mut invalid = combined.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            read_retry::classify(&envelope(invalid)).is_none(),
            "{pointer}"
        );
    }
    let mut undeclared = combined.clone();
    undeclared.as_object_mut().unwrap().remove("read_retry");
    assert!(read_retry::classify(&envelope(undeclared)).is_none());
    for (field, value) in [
        ("future_detail", json!({"preserved":true})),
        ("index_state", json!("available")),
        ("chunks", json!(1)),
        ("symbols", json!([])),
    ] {
        let mut mixed = combined.clone();
        mixed["members"][0]["result"][field] = value;
        assert!(read_retry::classify(&envelope(mixed)).is_none(), "{field}");
    }
    let mut multiple = combined.clone();
    multiple["members"]
        .as_array_mut()
        .unwrap()
        .push(combined["members"][0].clone());
    assert!(read_retry::classify(&envelope(multiple)).is_none());
}

fn readiness_pair() -> Result<(Value, Value)> {
    use airk_core::{identity::Sha256Digest, verification::*};
    let mut available = json!({
        "request_id":"one", "items":["x".repeat(4096)],
        "discovery_readiness":{"version":1,"state":"ready","epoch":"00".repeat(16),
            "processed_artifacts":2,"committed_artifacts":2,"total_artifacts":2,"failure":null},
        "semantic_readiness":{"version":1,"state":"disabled","default_search_policy":"partial_hybrid",
            "selected_artifacts":0,"completed_artifacts":0,"selected_units":null,"completed_units":0,
            "first_ready_unix_ms":null,"fully_ready_unix_ms":null,
            "selection_digest":airk_core::vector_selection::VectorSelectionPolicy::default().digest()?,
            "pipeline_digest":null,"failure":null}
    });
    let mut unavailable = available.clone();
    let fields = unavailable.as_object_mut().unwrap();
    fields.remove("discovery_readiness");
    fields.remove("semantic_readiness");
    fields.insert(
        "discovery_observation".into(),
        json!("unavailable_after_read"),
    );
    fields.insert("request_id".into(), json!("two"));
    for value in [&mut available, &mut unavailable] {
        let mut limits: VerificationLimits =
            VerificationControls::default().resolve()?.unwrap().into();
        limits.receipt_bytes_budget = receipt_budget_for_reply(value)?;
        value["source_verification"] = serde_json::to_value(VerificationReceipt {
            schema: "airk.saved-source-verification/1".into(),
            method: "sha256-full-saved-source/1".into(),
            mode: VerificationMode::Report,
            status: "verified".into(),
            scope_digest: Sha256Digest::from_bytes([1; 32]),
            selection_digest: Sha256Digest::from_bytes([2; 32]),
            observed_from_unix_ms: 1,
            observed_to_unix_ms: 2,
            publication_fence: "current".into(),
            inventory_complete: true,
            all_references_accounted: true,
            source_rows_omitted: 0,
            requirements_satisfied: Some(true),
            counts: VerificationCounts::default(),
            limits,
            origins: vec![],
            sources: vec![],
            references: vec![],
            qualifications: vec![],
            error: None,
        })?;
    }
    Ok((available, unavailable))
}

#[test]
fn optional_readiness_normalization_checks_original_headroom_and_fixture_state() -> Result<()> {
    let (mut available, mut unavailable) = readiness_pair()?;
    assert_ne!(
        available["source_verification"]["limits"]["receipt_bytes_budget"],
        unavailable["source_verification"]["limits"]["receipt_bytes_budget"]
    );
    read_retry::normalize_dated_readiness_pair(&mut available, &mut unavailable, 2)?;
    read_retry::evidence_for_comparison(&mut available)?;
    read_retry::evidence_for_comparison(&mut unavailable)?;
    assert_eq!(available, unavailable);
    for (pointer, value) in [
        ("/source_verification/limits/receipt_bytes_budget", json!(1)),
        ("/discovery_readiness/committed_artifacts", json!(3)),
        ("/discovery_readiness/state", json!("partial")),
        ("/semantic_readiness/state", json!("ready")),
    ] {
        let (mut a, mut b) = readiness_pair()?;
        *a.pointer_mut(pointer).unwrap() = value;
        let original = (a.clone(), b.clone());
        assert!(read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2).is_err());
        assert_eq!((a, b), original, "failed validation mutated the inputs");
    }
    let (mut a, mut b) = readiness_pair()?;
    a["semantic_readiness"]["unexpected"] = json!(true);
    assert!(read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2).is_err());
    let (mut a, mut b) = readiness_pair()?;
    b["discovery_readiness"] = a["discovery_readiness"].clone();
    assert!(read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2).is_err());
    Ok(())
}

#[test]
fn optional_readiness_normalization_retains_evidence_and_work_budget_differences() -> Result<()> {
    for (pointer, value) in [
        ("/source_verification/limits/max_sources", json!(7)),
        ("/source_verification/scope_digest", json!("03".repeat(32))),
        ("/source_verification/counts/checked_sources", json!(1)),
    ] {
        let (mut a, mut b) = readiness_pair()?;
        *b.pointer_mut(pointer).unwrap() = value;
        read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2)?;
        read_retry::evidence_for_comparison(&mut a)?;
        read_retry::evidence_for_comparison(&mut b)?;
        assert_ne!(a, b);
    }
    let (mut a, _) = readiness_pair()?;
    let mut b = a.clone();
    b["discovery_readiness"]["epoch"] = json!("01".repeat(16));
    read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2)?;
    assert_ne!(a, b, "two available observations must compare exactly");
    Ok(())
}

#[test]
fn only_complete_typed_read_rejections_authorize_replay() {
    let hint = json!({"status":"retry","code":"publication_busy","retryable":true,
        "action":"repeat_same_request","retry_after_ms":25});
    assert!(read_retry::classify(&envelope(hint.clone())).is_some());
    for (field, value) in [
        ("code", json!("unknown_busy")),
        ("retryable", json!(false)),
        ("action", json!("refresh_revision")),
        ("retry_after_ms", json!(0)),
        ("retry_after_ms", json!(1001)),
    ] {
        let mut invalid = hint.clone();
        invalid[field] = value;
        assert!(read_retry::classify(&envelope(invalid)).is_none());
    }
    let mut invalid = envelope(hint.clone());
    invalid["isError"] = json!(false);
    assert!(read_retry::classify(&invalid).is_none());
    assert!(read_retry::within(&envelope(hint), Instant::now()).is_none());
    for (code, fresh) in [
        ("direct_query_admission", false),
        ("direct_query_setup", true),
    ] {
        let hint = json!({"error":{"code":code,"reason":"busy","query_submitted":false,
            "retryable":true,"retry_after_ms":100,"fresh_brokerage_required":fresh}});
        assert_eq!(
            read_retry::classify(&envelope(hint.clone()))
                .unwrap()
                .fresh_brokerage,
            fresh
        );
        for field in ["query_submitted", "retryable", "fresh_brokerage_required"] {
            let mut invalid = hint.clone();
            invalid["error"][field] = json!(!invalid["error"][field].as_bool().unwrap());
            assert!(read_retry::classify(&envelope(invalid)).is_none());
        }
    }
}

/// Reviewed top-level envelope cases shared with `readRetry.ts`
/// (`fixtures/wire/read_retry_envelopes.py`). `expected` is written
/// independently of both consumers; it is never computed by them.
fn envelope_fixture() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/wire/read_retry_envelopes.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["schema"],
        "airk.wire-fixture.read-retry-envelopes/1"
    );
    assert!(
        !fixture["cases"].as_array().unwrap().is_empty(),
        "zero selected envelope cases"
    );
    fixture
}

#[test]
fn shared_envelope_cases_match_the_native_decision() {
    for case in envelope_fixture()["cases"].as_array().unwrap() {
        let envelope = &case["envelope"];
        let decision = if case["expected_scope"].is_null() {
            read_retry::classify(envelope)
        } else {
            let scope: airk_core::service::ScopedConnectRequest =
                serde_json::from_value(case["expected_scope"].clone()).unwrap();
            read_retry::classify_for_scope(envelope, &scope)
        };
        let observed = decision.map_or(Value::Null, |retry| {
            json!({"retry_after_ms": u64::try_from(retry.delay.as_millis()).unwrap(),
                "fresh_brokerage_required": retry.fresh_brokerage})
        });
        assert_eq!(
            observed, case["expected"],
            "{}: {}",
            case["name"], case["rule"]
        );
    }
}

#[test]
fn envelope_emitters_produce_the_reviewed_fixture_envelopes() {
    let mut emitted = 0;
    for case in envelope_fixture()["cases"].as_array().unwrap() {
        let Some(emitter) = case.get("emitter") else {
            continue;
        };
        let expected = &case["envelope"];
        let actual = match emitter["function"].as_str().unwrap() {
            "airk_mcp::remote::scoped_setup_failure" => {
                let selection: airk_core::service::ScopedConnectRequest = serde_json::from_value(
                    expected["structuredContent"]["requested_scope"]["selection"].clone(),
                )
                .unwrap();
                let native = emitter["native_control_response"].clone();
                airk_mcp::remote::scoped_setup_failure(
                    &selection,
                    emitter["code"].as_str().unwrap(),
                    emitter["retryable"].as_bool().unwrap(),
                    (!native.is_null()).then_some(native),
                )
            }
            "airk_mcp::response::typed_error_result" => {
                let error = match emitter["error"].as_str().unwrap() {
                    "selection_contention" => {
                        anyhow::Error::new(airk_store::service::collection::SelectionContention)
                    }
                    "current_read_contention" => {
                        anyhow::Error::new(airk_store::current::CurrentReadContention)
                    }
                    other => panic!("unreviewed emitter error {other}"),
                };
                let result = serde_json::to_value(
                    airk_mcp::response::typed_error_result(&error).expect("typed read retry"),
                )
                .unwrap();
                // Compare the fields hosts read; the MCP result also carries its
                // own transport framing (for example `resultType`).
                json!({"isError": result["isError"], "content": result["content"]})
            }
            other => panic!("unreviewed emitter {other}"),
        };
        assert_eq!(
            actual["content"][0]["text"].as_str(),
            expected["content"][0]["text"].as_str(),
            "{}: exact text bytes",
            case["name"]
        );
        assert_eq!(&actual, expected, "{}", case["name"]);
        emitted += 1;
    }
    assert!(emitted > 0, "zero selected emitter cases");
}

#[tokio::test]
async fn backend_retries_keep_arguments_and_never_replay_submitted_or_unknown_errors() -> Result<()>
{
    use airk_mcp::remote::{RemoteFuture, RemoteTools};
    use std::sync::Mutex;
    struct Backend {
        replies: Mutex<std::collections::VecDeque<Value>>,
        calls: Mutex<Vec<(String, serde_json::Map<String, Value>)>>,
    }
    impl RemoteTools for Backend {
        fn call(
            &self,
            name: String,
            arguments: serde_json::Map<String, Value>,
        ) -> RemoteFuture<'_> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((name, arguments));
                Ok(self
                    .replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected replay"))
            })
        }
    }
    let arguments = json!({"revision":"original","cursor":"exact-cursor","name":"Copper"})
        .as_object()
        .unwrap()
        .clone();
    let success = json!({"isError":false,"content":[{"type":"text","text":"{}"}]});
    for (hint, repeats) in [
        (
            single_required_retry(
                json!({"status":"retry","code":"selection_busy","retryable":true,
                "action":"repeat_same_request","retry_after_ms":1}),
            ),
            true,
        ),
        (
            single_required_retry(json!({"error":{"code":"direct_query_setup","reason":"busy",
                "query_submitted":false,"retryable":true,"fresh_brokerage_required":true,"retry_after_ms":1}})),
            true,
        ),
        (
            json!({"status":"retry","code":"selection_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":1}),
            true,
        ),
        (
            json!({"error":{"code":"direct_query_setup","reason":"busy","query_submitted":false,"retryable":true,"fresh_brokerage_required":true,"retry_after_ms":1}}),
            true,
        ),
        (
            json!({"error":{"code":"direct_query_setup","reason":"busy","query_submitted":true,"retryable":true,"fresh_brokerage_required":true,"retry_after_ms":1}}),
            false,
        ),
        (
            json!({"status":"retry","code":"unknown_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":1}),
            false,
        ),
    ] {
        let refusal = envelope(hint);
        let backend = Backend {
            replies: Mutex::new([refusal.clone(), success.clone()].into()),
            calls: Mutex::new(Vec::new()),
        };
        let mut observed = Vec::new();
        let result = read_retry::backend(
            &backend,
            "airk_symbols",
            &arguments,
            Instant::now() + Duration::from_secs(5),
            |reply| observed.push(reply.clone()),
        )
        .await?;
        assert_eq!(
            result,
            if repeats {
                success.clone()
            } else {
                refusal.clone()
            }
        );
        assert_eq!(observed, if repeats { vec![refusal] } else { vec![] });
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), if repeats { 2 } else { 1 });
        assert!(
            calls
                .iter()
                .all(|(name, args)| name == "airk_symbols" && args == &arguments)
        );
    }
    let expected = original_scoped_selection();
    for (crossed, tool, repeats) in [
        (false, "airk_symbols", true),
        (true, "airk_symbols", false),
        (false, "airk_draft_plan", false),
    ] {
        let mut refusal = scoped_setup_hint(&expected);
        refusal["requested_scope"]["selection"]["expected_catalog_revision"] = json!(12);
        if crossed {
            refusal["requested_scope"]["selection"]["expected_grant_revision"] = json!(2);
        }
        let refusal = scoped_setup_envelope(refusal);
        let backend = Backend {
            replies: Mutex::new([refusal.clone(), success.clone()].into()),
            calls: Mutex::new(Vec::new()),
        };
        let mut observed = Vec::new();
        let result = read_retry::backend_for_scope(
            &backend,
            tool,
            &arguments,
            &expected,
            Instant::now() + Duration::from_secs(5),
            |reply| observed.push(reply.clone()),
        )
        .await?;
        assert_eq!(
            result,
            if repeats {
                success.clone()
            } else {
                refusal.clone()
            }
        );
        assert_eq!(observed, if repeats { vec![refusal] } else { vec![] });
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), if repeats { 2 } else { 1 });
        assert!(
            calls
                .iter()
                .all(|(name, args)| name == tool && args == &arguments)
        );
    }
    let backend = Backend {
        replies: Mutex::new([success].into()),
        calls: Mutex::new(Vec::new()),
    };
    assert!(
        read_retry::backend_for_scope(
            &backend,
            "airk_status",
            &arguments,
            &expected,
            Instant::now(),
            |_| {}
        )
        .await
        .is_err()
    );
    assert!(
        backend.calls.lock().unwrap().is_empty(),
        "an exhausted original deadline cannot dispatch another read"
    );
    Ok(())
}

fn hold_gate(path: &std::path::Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn body(reply: Value) -> Result<Value> {
    ensure!(reply["isError"] != true, "{reply}");
    Ok(serde_json::from_str(
        reply["content"][0]["text"]
            .as_str()
            .context("missing evidence")?,
    )?)
}

#[test]
fn publication_retries_keep_exact_cursor_revision_and_retained_bytes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = "Original retained northern observation.\n";
    for name in ["one.txt", "two.txt", "three.txt"] {
        fs::write(root.path().join(name), original)?;
    }
    let service = common::Service::start(root.path(), data.path(), "read-retry");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    ensure!(
        matches!(service.request(ControlMethod::Pause { project: service.project.binding, paused:true }),
        ControlResponse::Registered { project } if project.paused),
        "fixture did not pause source work"
    );
    let collection = airk_store::service::collection::Collection::existing(
        data.path(),
        &service.project.root,
        "read-retry",
    )?;
    let store = collection.selected_store()?;
    let directory = store.directory();
    let gate = directory
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".current-gates")
        .join(directory.file_name().unwrap());
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let mut connection = airk_cli::project_client::Connection::connect(&service.control, service.project.binding).await?;
        let arguments = json!({"selectors":[{"kind":"path","path":"one.txt"},
            {"kind":"path","path":"two.txt"},{"kind":"path","path":"three.txt"}],"limit":1,"response_mode":"full"});
        let first = body(connection.read_retry("airk_changes".into(), arguments.as_object().unwrap().clone()).await?)?;
        let token = first["to"].as_str().context("missing publication")?;
        let cursor = first["cursor"].as_str().context("fixture must have another page")?;
        let mut next = arguments.clone(); next["cursor"] = json!(cursor);
        let read = json!({"path":"one.txt","start_line":1,"end_line":1,"revision":token,"response_mode":"full"});
        fs::write(root.path().join("one.txt"), "LIVE_UNINDEXED_SENTINEL\n")?;
        for (name, arguments) in [("airk_changes", next), ("airk_read_file", read)] {
            let mut held = Some(hold_gate(&gate)?);
            let mut rejections = 0;
            let reply = read_retry::direct(&mut connection, name, arguments.as_object().unwrap(),
                Instant::now() + Duration::from_secs(5), |reply| {
                    assert!(read_retry::classify(reply).unwrap().publication);
                    rejections += 1;
                    // Release only after the actual native reader has observed the locked gate.
                    drop(held.take());
                }).await?;
            assert_eq!(rejections, 1, "fixture did not exercise one actual publication rejection: {name}: {reply}");
            let result = body(reply)?;
            assert_eq!(result["publication"]["token"], token);
            if name == "airk_changes" {
                assert_eq!(result["validations"][0]["ordinal"], 1, "retry discarded the cursor");
            } else {
                assert_eq!(result["hits"][0]["text"], original);
            }
        }
        let mut invalid: airk_core::identity::RevisionToken = token.parse()?;
        invalid.generation += 1;
        let args = json!({"path":"one.txt","revision":invalid.to_string()});
        let refused = read_retry::direct(&mut connection, "airk_file_info", args.as_object().unwrap(),
            Instant::now() + Duration::from_secs(5), |_| panic!("permanent revision rejection was retried")).await?;
        assert_eq!(refused["isError"], true);
        Ok::<_, anyhow::Error>(())
    })?;
    assert_eq!(
        fs::read_to_string(root.path().join("one.txt"))?,
        "LIVE_UNINDEXED_SENTINEL\n"
    );
    Ok(())
}

#[test]
fn publication_observations_require_scope_health_and_complete_snapshots() -> Result<()> {
    let available = json!({"index_state":"available","collection":"fixture","publication":{"collection":"fixture","token":"revision"},"files":1});
    assert!(!read_retry::publication_pending(
        &available, "fixture", "project"
    )?);
    for code in ["index_worker_unavailable", "index_status_unavailable"] {
        let value = json!({"index_state":"unavailable","collection":"fixture","project":"project",
            "publication":null,"files":null,"chunks":null,"index_error":{"code":code,"message":"PRIVATE_SENTINEL"},
            "runtime":{"service_error":null,"service":{"responsive":true,"accepting":true},"index_worker":{"process":null}},
            "work":{"basis":"instance_bound_service_metadata","serving_worker_unavailable":true,"failed_observations":[],
                "worker":{"automatic_start_blocked":false,"identity":null}}});
        assert!(read_retry::publication_pending(
            &value, "fixture", "project"
        )?);
        for pointer in [
            "/runtime/service/responsive",
            "/runtime/service/accepting",
            "/work/worker/automatic_start_blocked",
        ] {
            let mut invalid = value.clone();
            let field = invalid.pointer_mut(pointer).unwrap();
            *field = json!(!field.as_bool().unwrap());
            let error = read_retry::publication_pending(&invalid, "fixture", "project")
                .unwrap_err()
                .to_string();
            assert!(!error.contains("PRIVATE_SENTINEL"));
        }
        assert!(read_retry::publication_pending(&value, "foreign", "foreign").is_err());
    }
    for invalid in [
        json!({}),
        json!({"index_state":"available","collection":"fixture","publication":{"collection":"foreign","token":"revision"},"files":1}),
    ] {
        assert!(read_retry::publication_pending(&invalid, "fixture", "project").is_err());
    }
    Ok(())
}

#[test]
fn bounded_cli_read_never_starts_after_the_observation_deadline() -> Result<()> {
    let command = std::process::Command::new("nonexistent-must-not-spawn");
    assert_eq!(
        read_retry::once_until(&command, Instant::now())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_airk"));
    command.arg("--version");
    assert!(
        read_retry::once_until(&command, Instant::now() + Duration::from_secs(10))?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn bounded_cli_timeout_child() {
    if std::env::var_os("AIRK_TEST_BOUNDED_READ_CHILD").is_some() {
        std::thread::sleep(Duration::from_secs(30));
    }
}

#[test]
fn bounded_cli_timeout_terminates_its_owned_leaf_client() -> Result<()> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "bounded_cli_timeout_child", "--nocapture"])
        .env("AIRK_TEST_BOUNDED_READ_CHILD", "1");
    let started = Instant::now();
    let error = read_retry::once_until(&command, started + Duration::from_secs(1)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "timeout failed to reap the owned client"
    );
    Ok(())
}

#[cfg(unix)]
fn unix_leaf_command(mode: &str, directory: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "bounded_cli_unix_fixture", "--nocapture"])
        .env("AIRK_TEST_UNIX_LEAF", mode)
        .env("AIRK_TEST_UNIX_DIRECTORY", directory)
        .env_remove("PATH")
        .current_dir(directory);
    command
}

#[cfg(unix)]
#[test]
fn bounded_cli_unix_fixture() -> Result<()> {
    use std::io::Write;
    let Some(mode) = std::env::var_os("AIRK_TEST_UNIX_LEAF") else {
        return Ok(()); // Re-exec entry point, invoked by the owning tests below.
    };
    let directory = std::path::PathBuf::from(std::env::var_os("AIRK_TEST_UNIX_DIRECTORY").unwrap());
    assert_eq!(
        fs::canonicalize(std::env::current_dir()?)?,
        fs::canonicalize(&directory)?
    );
    assert!(std::env::var_os("PATH").is_none());
    if mode == "streams" {
        let stderr = std::thread::spawn(|| {
            std::io::stderr().write_all(&vec![0xb2; 262_144]).unwrap();
        });
        std::io::stdout().write_all(&vec![0xa1; 262_144])?;
        stderr.join().unwrap();
    } else if mode == "holder" {
        fs::write(directory.join("holder-ready"), b"ready")?;
        std::thread::sleep(Duration::from_secs(30));
    } else {
        assert!(mode == "exited-leader" || mode == "timeout-tree");
        // Deliberately inherit the owned leaf group and both output pipes.
        let mut descendant = unix_leaf_command("holder", &directory).spawn()?;
        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while !directory.join("holder-ready").exists() {
            ensure!(Instant::now() < ready_deadline, "descendant did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        if mode == "exited-leader" {
            std::process::exit(0);
        }
        std::thread::sleep(Duration::from_secs(30));
        descendant.wait()?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn bounded_cli_unix_preserves_all_output_and_native_child_ownership() -> Result<()> {
    airk_process::child_group::require_waitable_children()?;
    let directory = tempfile::tempdir()?;
    let output = read_retry::once_until(
        &unix_leaf_command("streams", directory.path()),
        Instant::now() + Duration::from_secs(10),
    )?;
    assert!(output.status.success());
    assert_eq!(
        output.stdout.iter().filter(|&&byte| byte == 0xa1).count(),
        262_144
    );
    assert_eq!(
        output.stderr.iter().filter(|&&byte| byte == 0xb2).count(),
        262_144
    );
    // A Tokio process driver would leave a global SIGCHLD handler here and
    // prevent the next real native GroupChild from acquiring its child.
    airk_process::child_group::require_waitable_children()?;
    let mut sibling =
        airk_process::GroupChild::spawn(&mut unix_leaf_command("holder", directory.path()))?;
    assert!(sibling.try_wait()?.is_none());
    sibling.terminate_and_wait(Duration::from_secs(2))?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn bounded_cli_unix_drains_inherited_pipes_after_exit_and_timeout() -> Result<()> {
    for mode in ["exited-leader", "timeout-tree"] {
        let directory = tempfile::tempdir()?;
        let sibling_directory = tempfile::tempdir()?;
        let mut sibling = airk_process::GroupChild::spawn(&mut unix_leaf_command(
            "holder",
            sibling_directory.path(),
        ))?;
        let started = Instant::now();
        let output = read_retry::once_until(
            &unix_leaf_command(mode, directory.path()),
            started + Duration::from_secs(2),
        );
        assert!(directory.path().join("holder-ready").exists());
        if mode == "exited-leader" {
            assert!(output?.status.success());
        } else {
            assert_eq!(output.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        }
        // The descendant holds both pipes for30s; a reader join before group
        // drain would hang. A separately owned service group must survive.
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(sibling.try_wait()?.is_none());
        sibling.terminate_and_wait(Duration::from_secs(2))?;
        airk_process::child_group::require_waitable_children()?;
    }
    Ok(())
}
