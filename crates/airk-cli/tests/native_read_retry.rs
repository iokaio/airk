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
