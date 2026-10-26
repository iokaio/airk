// SPDX-License-Identifier: Apache-2.0
//! Forced process exit followed by actual CLI and MCP startup recovery.
use super::*;
use airk_kernel::{
    Kernel,
    current::{CurrentUpdateStage, CurrentUpdater},
};
use airk_store::{
    current::{ArtifactChange, CapturedInput, CurrentScope},
    quota::Quota,
};
use anyhow::{Result, ensure};

#[test]
fn crash_child() -> Result<()> {
    let Ok(encoded) = std::env::var("AIRK_RUNTIME_CRASH_CASE") else {
        return Ok(());
    };
    let (data, root, boundary): (std::path::PathBuf, std::path::PathBuf, String) =
        serde_json::from_str(&encoded)?;
    let collection = airk_store::service::collection::Collection::existing(
        &data,
        root.to_str().unwrap(),
        "project-a",
    )?;
    let store = collection.selected_store()?;
    let owner: Value =
        serde_json::from_slice(&fs::read(store.directory().join("current-owner.json"))?)?;
    let scope: CurrentScope = serde_json::from_value(owner["scope"].clone())?;
    let read = store.current_read(&scope)?;
    let configuration = read.collection_state()?.unwrap().configuration;
    let artifact = read.artifact("captured.txt")?.unwrap();
    drop(read);
    let quota = Quota::open_service(data)?;
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
        &[ArtifactChange::Upsert(Box::new(CapturedInput::new(
            artifact,
        )))],
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
            let owner = tempfile::tempdir()?;
            common::bind_data(&owner.path().join("control"), data.path())?;
            let kernel = Kernel::open_service_collection(
                root.path(),
                Some(data.path().into()),
                "project-a",
            )?;
            kernel.configure_vectors(false)?;
            kernel.index()?;
            let old = kernel.status()?.publication.unwrap().token;
            drop(kernel);
            let output = Command::new(std::env::current_exe()?)
                .args(["--exact", "current_recovery::crash_child", "--nocapture"])
                .env(
                    "AIRK_RUNTIME_CRASH_CASE",
                    serde_json::to_string(&(data.path(), root.path().canonicalize()?, boundary))?,
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
            // Persist the pause before service startup can schedule recovery.
            // Pausing after registration races the worker's first source turn.
            let registration =
                airk_kernel::service_projects::registration(root.path(), data.path(), "project-a")?;
            {
                let mut registry = airk_store::service::registry::ServiceRegistry::open(
                    &owner.path().join("control"),
                    airk_kernel::service_broker::new_instance_id()?,
                )?;
                registry.register(&registration)?;
                registry.pause(registration.binding, true)?;
            }
            let _service = common::Service::in_owner(root.path(), data.path(), "project-a", owner);
            // Registration preserves the persisted pause. Verify it before
            // checking what an ordinary read may observe.
            ensure!(
                matches!(
                    _service.request(airk_core::service::ControlMethod::Pause {
                        project: _service.project.binding,
                        paused: true,
                    }),
                    airk_core::service::ControlResponse::Registered { project } if project.paused
                ),
                "recovery fixture did not pause"
            );
            let recovered: Value = if mcp {
                let mut session = Session::start(root.path(), data.path())?;
                session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"recovery-fixture","version":"1"}}}))?;
                session.receive(1)?;
                session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
                let arguments = json!({"path":"captured.txt","start_line":1,"end_line":1});
                let result = if boundary.starts_with("cold-") {
                    session.call(2, "airk_read_file", arguments)?
                } else {
                    session.read_retry(2, "airk_read_file", arguments)?
                };
                if boundary.starts_with("cold-") {
                    ensure!(
                        result["result"]["isError"] == true
                            && result.to_string().contains("fresh setup"),
                        "{result}"
                    );
                    ensure!(
                        fs::read_to_string(root.path().join("captured.txt"))?
                            == "LIVE_UNINDEXED_SENTINEL\n",
                        "source modified on refusal"
                    );
                    continue;
                }
                ensure!(result["result"]["isError"] != true, "{result}");
                serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap())?
            } else {
                if boundary.starts_with("cold-") {
                    let error = common::call(
                        root.path(),
                        data.path(),
                        "project-a",
                        &[
                            "read",
                            "captured.txt",
                            "--start-line",
                            "1",
                            "--end-line",
                            "1",
                        ],
                    )
                    .unwrap_err();
                    ensure!(format!("{error:#}").contains("fresh setup"), "{error:#}");
                    ensure!(
                        fs::read_to_string(root.path().join("captured.txt"))?
                            == "LIVE_UNINDEXED_SENTINEL\n",
                        "source modified on refusal"
                    );
                    continue;
                }
                common::call(
                    root.path(),
                    data.path(),
                    "project-a",
                    &[
                        "read",
                        "captured.txt",
                        "--start-line",
                        "1",
                        "--end-line",
                        "1",
                    ],
                )?
            };
            assert!(recovered.to_string().contains("captured original source"));
            assert!(!recovered.to_string().contains("LIVE_UNINDEXED_SENTINEL"));
            let observed: airk_core::identity::RevisionToken =
                recovered["read"]["publication"]["token"]
                    .as_str()
                    .unwrap()
                    .parse()?;
            // Opening a read-only connection cannot authorize prepared replay.
            assert_eq!(observed, old, "{boundary}, mcp={mcp}");
            ensure!(
                matches!(
                    _service.request(airk_core::service::ControlMethod::Pause {
                        project: _service.project.binding,
                        paused: false,
                    }),
                    airk_core::service::ControlResponse::Registered { project } if !project.paused
                ),
                "recovery fixture did not resume"
            );
            _service.job(airk_core::service::JobKind::Recover);
            let recovered = common::call(
                root.path(),
                data.path(),
                "project-a",
                &["info", "captured.txt"],
            )?;
            let current: airk_core::identity::RevisionToken =
                recovered["read"]["publication"]["token"]
                    .as_str()
                    .unwrap()
                    .parse()?;
            assert_eq!(current.generation, old.generation + 1);
            let previous = common::call(
                root.path(),
                data.path(),
                "project-a",
                &["info", "captured.txt", "--revision", &old.to_string()],
            );
            assert!(previous.is_err());
            let second = common::call(
                root.path(),
                data.path(),
                "project-a",
                &["info", "captured.txt"],
            )?;
            assert_eq!(second["read"]["publication"]["token"], current.to_string());
            assert_eq!(
                fs::read_to_string(root.path().join("captured.txt"))?,
                "LIVE_UNINDEXED_SENTINEL\n"
            );
        }
    }
    Ok(())
}
