// SPDX-License-Identifier: Apache-2.0
//! Reports stay direct; a saved clear request cannot erase later observations on replay.
mod common;
use airk_core::{
    service::{ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome},
    usage::{ClientFamily, RecordingMode},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

#[test]
fn direct_usage_counts_outer_clients_once_and_exact_clear_replay_preserves_later_calls()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = "pub fn polar_fixture() -> bool { true }\n";
    std::fs::write(root.path().join("original.rs"), original)?;
    let service = common::Service::start(root.path(), data.path(), "usage");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.configure(airk_cli::service_configuration::Draft::Usage {
        mode: RecordingMode::AggregatesOnly,
    });
    service.job(JobKind::Index);
    // Index acknowledgement does not settle the separate native audit cohort.
    // Complete actual canonical verification before our measurement lock can
    // prevent its physical work; the protected reads below still enforce it.
    let verified = service.job(JobKind::Verify);
    ensure!(
        matches!(
            verified.outcome,
            Some(JobOutcome::Succeeded { revision: Some(_) })
        ),
        "canonical Verify did not finish its native publication audit: {verified:?}",
    );
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = || native_report(root.path(), data.path());
    // Count actual outer calls. A rejected read is another call, so this accounting
    // fixture excludes concurrent writers instead of silently retrying a query.
    let collection = airk_store::service::collection::Collection::existing(
        data.path(),
        &service.project.root,
        "usage",
    )?;
    let store = collection.selected_store()?;
    let writer = hold_writer(&store).context("initial measured-read writer exclusion")?;
    let before = report().context("initial native CLI usage report")?;
    assert!(
        before["report"]["clients"]["native_scheduler"]
            .as_u64()
            .is_some_and(|calls| calls >= 1)
    );
    let indexed: u128 = before["report"]["operations"]["index"]["index_work"]["indexed"]
        .as_str()
        .unwrap()
        .parse()?;
    assert_eq!(
        indexed, 1,
        "native work must be counted once across source turns and waiting clients"
    );
    single_search(root.path(), data.path()).with_context(|| {
        format!(
            "initial measured CLI search; native observations: {}",
            integrity_observation(&service)
        )
    })?;
    executor
        .block_on(async {
            let mut connection = airk_cli::project_client::Connection::connect_as(
                &service.control,
                service.project.binding,
                ClientFamily::Codex,
            )
            .await
            .context("measured Codex outline brokerage")?;
            let reply = connection
                .call(
                    "airk_outline".into(),
                    json!({"path":"original.rs"}).as_object().unwrap().clone(),
                )
                .await?;
            ensure!(reply["isError"] != true, "{reply}");
            Ok::<_, anyhow::Error>(())
        })
        .with_context(|| {
            format!(
                "measured Codex outline; native observations: {}",
                integrity_observation(&service)
            )
        })?;
    let after = report().context("native CLI usage report after CLI and Codex reads")?;
    assert_eq!(
        after["report"]["calls"].as_u64(),
        before["report"]["calls"].as_u64().map(|calls| calls + 2)
    );
    assert_eq!(after["report"]["clients"]["cli"], 1);
    assert_eq!(after["report"]["clients"]["codex"], 1);
    drop(writer);
    let output = common::command(root.path(), data.path(), "usage")
        .args(["usage", "clear"])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let accepted: Value = serde_json::from_slice(&output.stdout)?;
    let cleared = service.wait(serde_json::from_value(accepted["job"]["id"].clone())?);
    let Response::Configuration {
        result: Some(result),
        ..
    } = service.request(Method::ConfigurationResult { id: cleared.id })
    else {
        anyhow::bail!("clear receipt missing")
    };
    assert_eq!(result["cleared"], true);
    assert_eq!(
        report().context("native CLI usage report after clear")?["report"]["calls"],
        0
    );
    service.job(JobKind::Inspect);
    let draft_file = data.path().join("clear-draft.json");
    std::fs::write(&draft_file, br#"{"operation":"clear_usage"}"#)?;
    let prepared = common::native_command()
        .args(["service", "requests", "--runtime-dir"])
        .arg(&service.control)
        .args([
            "prepare-configuration",
            "--project",
            &service.project.binding.to_string(),
            "--file",
        ])
        .arg(&draft_file)
        .output()?;
    ensure!(
        prepared.status.success(),
        "{}",
        String::from_utf8_lossy(&prepared.stderr)
    );
    let prepared: Value = serde_json::from_slice(&prepared.stdout)?;
    let nonce = prepared["prepared"].as_str().unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(
        service
            .control
            .join("client-requests")
            .join(format!("{nonce}.json")),
    )?)?;
    let Response::Job { job: first } =
        service.request(serde_json::from_value(saved["request"].clone())?)
    else {
        anyhow::bail!("clear not accepted")
    };
    service.wait(first.id);
    // Simulate acceptance whose acknowledgement was lost before journal removal.
    let writer = hold_writer(&store).context("clear-replay measured-read writer exclusion")?;
    single_search(root.path(), data.path()).with_context(|| {
        format!(
            "clear-replay measured CLI search; native observations: {}",
            integrity_observation(&service)
        )
    })?;
    drop(writer);
    let repeated = common::native_command()
        .args(["service", "requests", "--runtime-dir"])
        .arg(&service.control)
        .args(["retry", nonce])
        .output()?;
    ensure!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    let repeated: Value = serde_json::from_slice(&repeated.stdout)?;
    assert_eq!(repeated["job"]["id"], serde_json::to_value(first.id)?);
    assert_eq!(
        report().context("native CLI usage report after exact clear replay")?["report"]["calls"],
        1
    );
    assert!(airk_cli::client_requests::list(&service.control)?.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.path().join("original.rs"))?,
        original
    );
    Ok(())
}

#[test]
fn explorer_mcp_client_info_is_recorded_as_its_own_family() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("explorer.rs"),
        "pub fn explorer_fixture() -> bool { true }\n",
    )?;
    let service = common::Service::start(root.path(), data.path(), "usage");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.configure(airk_cli::service_configuration::Draft::Usage {
        mode: RecordingMode::AggregatesOnly,
    });
    service.job(JobKind::Index);
    let collection = airk_store::service::collection::Collection::existing(
        data.path(),
        &service.project.root,
        "usage",
    )?;
    let store = collection.selected_store()?;
    let writer = hold_writer(&store)?;
    // The frontend's own environment opts out of usage, as the extension once tried;
    // attribution and recording are decided by the worker from clientInfo alone.
    let mut child = common::command(root.path(), data.path(), "usage")
        .env("AIRK_CLIENT", "vscode")
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().expect("stdin");
    let (sender, output) = std::sync::mpsc::channel::<String>();
    let stdout = child.stdout.take().expect("stdout");
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut exchange = |message: Value, id: Option<u64>| -> Result<Option<Value>> {
        use std::io::Write;
        writeln!(input, "{message}")?;
        input.flush()?;
        let Some(id) = id else { return Ok(None) };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| anyhow::anyhow!("stdio reply deadline exhausted"))?;
            let value: Value = serde_json::from_str(&output.recv_timeout(remaining)?)?;
            if value["id"] == id {
                return Ok(Some(value));
            }
        }
    };
    exchange(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-explorer","version":"1"}}}),
        Some(1),
    )?;
    exchange(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        None,
    )?;
    // native read: one measured attribution call while the fixture deliberately
    // holds the writer; saved-source publication fencing is tested separately.
    let reply = exchange(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"airk_search","arguments":{"query":"explorer_fixture","mode":"lexical","verification":"off"}}}),
        Some(2),
    )?
    .unwrap();
    ensure!(reply["result"]["isError"] != true, "{reply}");
    drop(input);
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    let report = native_report(root.path(), data.path())
        .context("native CLI usage report after Explorer MCP read")?;
    let clients = &report["report"]["clients"];
    assert_eq!(clients["explorer"], 1, "{clients}");
    assert!(clients.get("vscode").is_none(), "{clients}");
    assert!(clients.get("unknown").is_none(), "{clients}");
    Ok(())
}

fn single_search(root: &std::path::Path, data: &std::path::Path) -> Result<()> {
    let output = common::command(root, data, "usage")
        .args([
            "tools",
            "call",
            "airk_search",
            "--args",
            r#"{"query":"polar_fixture","mode":"lexical"}"#,
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "single measured read failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(reply["isError"] != true, "{reply}");
    Ok(())
}

fn hold_writer(store: &airk_store::FileStore) -> Result<airk_store::WriterLock> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match store.lock() {
            Ok(writer) => return Ok(writer),
            Err(error)
                if error
                    .downcast_ref::<std::fs::TryLockError>()
                    .is_some_and(|cause| matches!(cause, std::fs::TryLockError::WouldBlock))
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
}

/// Exercise the real same-image native client, including administrative
/// preflight and the sole positive legacy-zero direct transport handoff.
fn native_report(root: &std::path::Path, data: &std::path::Path) -> Result<Value> {
    let output = common::command(root, data, "usage")
        .args(["usage", "report", "--days", "30", "--format", "json"])
        .output()
        .context("spawn native CLI usage report")?;
    ensure!(
        output.status.success(),
        "native CLI usage report refused; complete native output: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("decode complete native CLI usage report")
}

#[test]
fn foreign_admin_client_preserves_full_preflight_refusal_then_native_cli_succeeds() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("original.txt"),
        "Retain source bytes across refused admin preflight.\n",
    )?;
    let before = std::fs::read(root.path().join("original.txt"))?;
    let service = common::Service::start(root.path(), data.path(), "usage");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let Response::Health { observation } = service.request(Method::Health) else {
        anyhow::bail!("native service health unavailable")
    };
    ensure!(
        airk_cli::runtime_pair::digest_file(&std::env::current_exe()?)?
            != observation.executable_sha256,
        "regression requires a genuinely foreign test executable",
    );
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(airk_cli::usage_client::report(
            &service.control,
            service.project.binding,
            30,
        ))
        .err()
        .context("foreign executable unexpectedly passed admin preflight")?;
    let refusal = error
        .downcast_ref::<airk_cli::usage_client::NativeReadFailure>()
        .context("native preflight refusal lost its complete typed response")?;
    assert_eq!(
        refusal.body(),
        &serde_json::to_value(Response::Failure {
            code: "service_rebind_required".into(),
        })?
    );
    // Non-submission follows from console_client's engine check BEFORE
    // request_for_instance. Usage-report counters cannot establish that fact.
    native_report(root.path(), data.path())
        .context("correct native executable after explicit foreign refusal")?;
    assert_eq!(std::fs::read(root.path().join("original.txt"))?, before);
    Ok(())
}

fn integrity_observation(service: &common::Service) -> Value {
    // Failure-only bounded native metadata. Never retry a measured evidence read.
    json!({
        "console": service.request(Method::Console { after: None, revision: None, limit: 1 }),
        "worker": service.request(Method::Worker { project: service.project.binding }),
        "sources": service.request(Method::SourceStatus),
        "resources": service.request(Method::Resources),
    })
}
