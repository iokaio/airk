// SPDX-License-Identifier: Apache-2.0
//! Forced process exit followed by actual CLI and MCP startup recovery.
use super::*;
use airk_kernel::{
    Kernel,
    current::{CurrentUpdateStage, CurrentUpdater},
};
use airk_store::{
    FileStore,
    current::{ArtifactChange, CapturedInput, CurrentScope},
    quota::Quota,
};
use anyhow::{Result, ensure};

#[test]
fn crash_child() -> Result<()> {
    let Ok(encoded) = std::env::var("AIRK_RUNTIME_CRASH_CASE") else {
        return Ok(());
    };
    let (data, boundary): (std::path::PathBuf, String) = serde_json::from_str(&encoded)?;
    let store = FileStore::new(data.join("collections-1.2/project-a"));
    let owner: Value =
        serde_json::from_slice(&fs::read(store.directory().join("current-owner.json"))?)?;
    let scope: CurrentScope = serde_json::from_value(owner["scope"].clone())?;
    let read = store.current_read(&scope)?;
    let configuration = read.collection_state()?.unwrap().configuration;
    let artifact = read.artifact("captured.txt")?.unwrap();
    drop(read);
    let quota = Quota::open(data)?;
    let reservation = quota.reserve("project-a", 512 << 20)?;
    let writer = store.lock()?;
    if boundary.starts_with("cold-") {
        let _grant = quota.reserve_current(
            &writer,
            airk_store::quota::CurrentGrantScope {
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                transaction: airk_core::identity::Sha256Digest::from_bytes([117; 32]),
            },
            1 << 20,
        )?;
        fs::write(
            store.directory().join("current.sqlite3-journal"),
            vec![0u8; if boundary == "cold-empty" { 0 } else { 512 }],
        )?;
        std::process::exit(73);
    }
    CurrentUpdater {
        store: &store,
        quota: &quota,
        scope: &scope,
        writer: &writer,
        reservation: &reservation,
        configuration: Some(&configuration),
    }
    .apply_observed(
        &[ArtifactChange::Upsert(Box::new(CapturedInput { artifact }))],
        None,
        &mut |stage| {
            if stage
                == if boundary == "applying" {
                    CurrentUpdateStage::Applying
                } else {
                    CurrentUpdateStage::Prepared
                }
            {
                std::process::exit(73);
            }
        },
    )?;
    anyhow::bail!("crash boundary was not reached")
}

#[test]
fn cli_and_mcp_recover_captured_updates_after_process_exit_and_expire_old_tokens() -> Result<()> {
    for boundary in ["prepared", "applying", "cold-empty", "cold-zero"] {
        for mcp in [false, true] {
            let root = tempfile::tempdir()?;
            let data = tempfile::tempdir()?;
            fs::write(
                root.path().join("captured.txt"),
                "captured original source\n",
            )?;
            let kernel =
                Kernel::open_collection(root.path(), Some(data.path().into()), "project-a")?;
            kernel.configure_vectors(false)?;
            kernel.index()?;
            let old = kernel.status()?.publication.unwrap().token;
            let output = Command::new(std::env::current_exe()?)
                .args(["--exact", "current_recovery::crash_child", "--nocapture"])
                .env(
                    "AIRK_RUNTIME_CRASH_CASE",
                    serde_json::to_string(&(data.path(), boundary))?,
                )
                .output()?;
            ensure!(
                output.status.code() == Some(73),
                "crash child: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            fs::write(
                root.path().join("captured.txt"),
                "LIVE_UNINDEXED_SENTINEL\n",
            )?;
            let recovered: Value = if mcp {
                let mut session = Session::start(root.path(), data.path())?;
                session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"recovery-fixture","version":"1"}}}))?;
                session.receive(1)?;
                session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
                let result = session.call(
                    2,
                    "airk_read_file",
                    json!({"path":"captured.txt","start_line":1,"end_line":1}),
                )?;
                ensure!(result["result"]["isError"] != true, "{result}");
                serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap())?
            } else {
                let output = Command::new(env!("CARGO_BIN_EXE_airk"))
                    .arg("--root")
                    .arg(root.path())
                    .arg("--data-dir")
                    .arg(data.path())
                    .args([
                        "--collection",
                        "project-a",
                        "read",
                        "captured.txt",
                        "--start-line",
                        "1",
                        "--end-line",
                        "1",
                    ])
                    .output()?;
                ensure!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_slice(&output.stdout)?
            };
            assert!(recovered.to_string().contains("captured original source"));
            assert!(!recovered.to_string().contains("LIVE_UNINDEXED_SENTINEL"));
            let reopened =
                Kernel::open_collection(root.path(), Some(data.path().into()), "project-a")?;
            let current = reopened.status()?.publication.unwrap().token;
            let published = !boundary.starts_with("cold-");
            assert_eq!(current.generation, old.generation + u64::from(published));
            let previous = reopened.file_info_conditional(
                "captured.txt",
                &airk_core::evidence::EvidenceReadOptions {
                    revision: Some(old),
                    ..Default::default()
                },
            );
            assert_eq!(previous.is_err(), published);
            let second =
                Kernel::open_collection(root.path(), Some(data.path().into()), "project-a")?;
            assert_eq!(second.status()?.publication.unwrap().token, current);
            assert_eq!(
                fs::read_to_string(root.path().join("captured.txt"))?,
                "LIVE_UNINDEXED_SENTINEL\n"
            );
        }
    }
    Ok(())
}
