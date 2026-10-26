// SPDX-License-Identifier: Apache-2.0
//! Reports stay direct; a saved clear request cannot erase later observations on replay.
mod common;
use airk_core::{
    service::{ControlMethod as Method, ControlResponse as Response, JobKind},
    usage::{ClientFamily, RecordingMode},
};
use anyhow::{Result, ensure};
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
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = || {
        executor.block_on(airk_cli::usage_client::report(
            &service.control,
            service.project.binding,
            30,
        ))
    };
    // Count actual outer calls. A rejected read is another call, so this accounting
    // fixture excludes concurrent writers instead of silently retrying a query.
    let collection = airk_store::service::collection::Collection::existing(
        data.path(),
        &service.project.root,
        "usage",
    )?;
    let store = collection.selected_store()?;
    let writer = hold_writer(&store)?;
    let before = report()?;
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
    single_search(root.path(), data.path())?;
    executor.block_on(async {
        let mut connection = airk_cli::project_client::Connection::connect_as(
            &service.control,
            service.project.binding,
            ClientFamily::Codex,
        )
        .await?;
        let reply = connection
            .call(
                "airk_outline".into(),
                json!({"path":"original.rs"}).as_object().unwrap().clone(),
            )
            .await?;
        ensure!(reply["isError"] != true, "{reply}");
        Ok::<_, anyhow::Error>(())
    })?;
    let after = report()?;
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
    assert_eq!(report()?["report"]["calls"], 0);
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
    let writer = hold_writer(&store)?;
    single_search(root.path(), data.path())?;
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
    assert_eq!(report()?["report"]["calls"], 1);
    assert!(airk_cli::client_requests::list(&service.control)?.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.path().join("original.rs"))?,
        original
    );
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
