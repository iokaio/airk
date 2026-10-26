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
