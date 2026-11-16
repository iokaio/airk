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
use anyhow::{Context, Result, ensure};

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
    // Legacy retained snapshots have no executable-owned model provider. The
    // installed HTTPS fixture does: preserve its real model references while
    // constructing the deliberately interrupted Current transaction.
    let runtime = std::env::var_os("AIRK_RUNTIME_CRASH_RUNTIME");
    let store = if let Some(runtime) = runtime.as_ref() {
        #[cfg(feature = "bundled-model")]
        {
            store.with_embedded_service_models(
                std::path::Path::new(runtime),
                airk_cli::bundled_assets::default_plan()?,
                Arc::new(airk_cli::bundled_assets::EmbeddedAssets),
            )?
        }
        #[cfg(not(feature = "bundled-model"))]
        {
            store.with_service_models(
                std::path::Path::new(runtime),
                airk_cli::bundled_assets::default_plan()?,
            )?
        }
    } else {
        store
    };
    let owner: Value =
        serde_json::from_slice(&fs::read(store.directory().join("current-owner.json"))?)?;
    let scope: CurrentScope = serde_json::from_value(owner["scope"].clone())?;
    let read = store.current_read(&scope)?;
    let configuration = read.collection_state()?.unwrap().configuration;
    let artifact = read.artifact("captured.txt")?.unwrap();
    drop(read);
    let quota = if runtime.is_some() {
        Quota::for_store(&store)?
    } else {
        Quota::open_service(data)?
    };
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
            let mut legacy_owner = None;
            let result = (|| -> Result<()> {
                let (old, modern): (airk_core::identity::RevisionToken, _) = if mcp {
                    let service = common::Service::reviewed(root.path(), data.path(), "project-a");
                    service.settle_reviewed_index(
                        root.path(),
                        data.path(),
                        std::time::Instant::now() + Duration::from_secs(90),
                    )?;
                    service.configure(airk_cli::service_configuration::Draft::Vectors {
                        enabled: false,
                    });
                    service.job(airk_core::service::JobKind::Index);
                    service.settle_reviewed_index(
                        root.path(),
                        data.path(),
                        std::time::Instant::now() + Duration::from_secs(90),
                    )?;
                    super::source_runtime::pause_and_wait(&service)?;
                    let status = common::published_status(root.path(), data.path(), "project-a")?;
                    let old = status["publication"]["token"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("missing recovery baseline: {status}"))?
                        .parse()?;
                    (old, Some(service))
                } else {
                    // Keep genuine prepopulated retained-state CLI coverage. A
                    // fresh catalog bootstrap would erase that migration boundary.
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
                    legacy_owner = Some(owner);
                    (old, None)
                };
                let crash = |runtime: Option<&std::path::Path>| -> Result<()> {
                    let mut child = Command::new(std::env::current_exe()?);
                    child
                        .args(["--exact", "current_recovery::crash_child", "--nocapture"])
                        .env(
                            "AIRK_RUNTIME_CRASH_CASE",
                            serde_json::to_string(&(
                                data.path(),
                                root.path().canonicalize()?,
                                boundary,
                            ))?,
                        );
                    if let Some(runtime) = runtime {
                        child.env("AIRK_RUNTIME_CRASH_RUNTIME", runtime);
                    } else {
                        child.env_remove("AIRK_RUNTIME_CRASH_RUNTIME");
                    }
                    let output = child.output()?;
                    ensure!(
                        output.status.code() == Some(73),
                        "crash child: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    // The interrupted transaction captures the original bytes;
                    // change live source only after that exact process has exited.
                    fs::write(
                        root.path().join("captured.txt"),
                        "LIVE_UNINDEXED_SENTINEL\n",
                    )?;
                    Ok(())
                };
                let _service = if let Some(service) = modern {
                    // The lifecycle helper verifies worker/owner exclusion and
                    // restarts this same approved installation and catalog scope.
                    let mut cold_files = None;
                    let restart = service.with_stopped_owner(|project| {
                        ensure!(project == &service.project, "recovery project changed");
                        let directory = if boundary.starts_with("cold-") {
                            // Resolve the owned slot before injecting damage;
                            // opening it afterwards can correctly refuse it.
                            let source = root.path().canonicalize()?;
                            let selected = airk_store::service::collection::Collection::existing(
                                data.path(),
                                source.to_str().context("source path")?,
                                "project-a",
                            )?
                            .selected_store()?;
                            Some(selected.directory().to_path_buf())
                        } else {
                            None
                        };
                        crash(Some(&service.control))?;
                        if let Some(directory) = directory {
                            let database = directory.join("current.sqlite3");
                            let journal = directory.join("current.sqlite3-journal");
                            cold_files = Some((
                                database.clone(),
                                journal.clone(),
                                fs::read(database)?,
                                fs::read(journal)?,
                            ));
                        }
                        Ok(())
                    });
                    if let Some((database, journal, database_bytes, journal_bytes)) = cold_files {
                        // Current corruption is project-local. Metadata stays
                        // reachable; evidence and writes must still refuse it.
                        restart
                            .context("approved metadata service restart with damaged project")?;
                        let before = service.request(airk_core::service::ControlMethod::Health);
                        let airk_core::service::ControlResponse::Health {
                            observation: before,
                        } = before
                        else {
                            anyhow::bail!("metadata unavailable with damaged project");
                        };
                        ensure!(
                            matches!(service.request(airk_core::service::ControlMethod::Pause {
                            project: service.project.binding, paused: true,
                        }), airk_core::service::ControlResponse::Registered { project } if project.paused),
                            "damaged project did not remain paused"
                        );
                        let mut session = Session::start(root.path(), data.path())?;
                        session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"cold-wal-refusal-fixture","version":"1"}}}))?;
                        session.receive(1)?;
                        session
                            .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
                        let refused = session.call(
                            2,
                            "airk_read_file",
                            json!({"path":"captured.txt","start_line":1,"end_line":1}),
                        )?;
                        ensure!(
                            refused["result"]["isError"] == true
                                && refused.to_string().contains(
                                    "rollback sidecar cannot overlay service WAL storage"
                                ),
                            "damaged project did not refuse its WAL overlay: {refused}"
                        );
                        drop(session);
                        let after = service.request(airk_core::service::ControlMethod::Health);
                        let airk_core::service::ControlResponse::Health { observation: after } =
                            after
                        else {
                            anyhow::bail!("damaged project read made service metadata unavailable");
                        };
                        ensure!(
                            before.instance == after.instance,
                            "metadata owner changed during project refusal"
                        );
                        ensure!(
                            fs::read(&database)? == database_bytes,
                            "refused read changed the Current database"
                        );
                        ensure!(
                            fs::read(&journal)? == journal_bytes,
                            "refused read changed the injected rollback sidecar"
                        );
                        ensure!(
                            fs::read_to_string(root.path().join("captured.txt"))?
                                == "LIVE_UNINDEXED_SENTINEL\n",
                            "source changed on WAL refusal"
                        );
                        let owner = service
                            .control
                            .parent()
                            .and_then(std::path::Path::parent)
                            .context("managed fixture owner missing")?
                            .canonicalize()?;
                        let evidence = tempfile::Builder::new()
                            .prefix("airk-current-wal-refusal-")
                            .tempdir()?
                            .keep();
                        fs::write(
                            evidence.join("injected-rollback-journal.bin"),
                            &journal_bytes,
                        )?;
                        fs::write(
                            evidence.join("cold-wal-refusal.json"),
                            serde_json::to_vec_pretty(&json!({
                                "boundary":boundary, "mcp":mcp, "outcome":"expected_project_read_refusal",
                                "service_before":before, "service_after":after, "read_refusal":refused,
                                "database_sha256":airk_cli::runtime_pair::digest_file(&database)?,
                                "database_unchanged":true, "journal_unchanged":true,
                                "injected_journal_bytes":journal_bytes.len(),
                                "cleanup_basis":"canonical Stop, exact test-injected sidecar removal under lease, leave fixture stopped without another Start",
                            }))?,
                        )?;
                        // This negative case needs no third startup. The normal
                        // restart circuit correctly throttles rapid successful
                        // starts too; cleanup must not reset or wait out policy.
                        service.shutdown_reviewed()?;
                        {
                            let _lease =
                                airk_store::service::ServiceLease::acquire(&service.control)?;
                            ensure!(
                                fs::read(&database)? == database_bytes
                                    && fs::read(&journal)? == journal_bytes,
                                "damaged bytes changed before fixture cleanup"
                            );
                            fs::copy(
                                owner.join("shutdown-complete.json"),
                                evidence.join("damaged-project-canonical-stop.json"),
                            )?;
                            fs::remove_file(&journal)?;
                            ensure!(
                                !journal.try_exists()?,
                                "test-injected sidecar survived cleanup"
                            );
                            ensure!(
                                fs::read(&database)? == database_bytes,
                                "fixture cleanup changed Current database bytes"
                            );
                            fs::write(
                                evidence.join("fixture-cleanup.json"),
                                serde_json::to_vec_pretty(&json!({
                                    "outcome":"fixture_left_canonically_stopped",
                                    "exclusion":"actual_service_lease_after_verified_stop",
                                    "removed":"only_exact_test_injected_rollback_sidecar",
                                    "database_unchanged":true, "additional_start_submitted":false,
                                }))?,
                            )?;
                        }
                        eprintln!(
                            "expected modern project WAL refusal with healthy service metadata; stopped-fixture evidence retained {}",
                            evidence.display()
                        );
                        return Ok(());
                    }
                    restart?;
                    service
                } else {
                    crash(None)?;
                    common::Service::in_owner(
                        root.path(),
                        data.path(),
                        "project-a",
                        legacy_owner.take().unwrap(),
                    )
                };
                // Worker startup queues recovery as durable service work. Hold its
                // admission while checking what an ordinary read may observe.
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
                                && result.to_string().contains("rollback sidecar"),
                            "{result}"
                        );
                        ensure!(
                            fs::read_to_string(root.path().join("captured.txt"))?
                                == "LIVE_UNINDEXED_SENTINEL\n",
                            "source modified on refusal"
                        );
                        return Ok(());
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
                        ensure!(
                            format!("{error:#}").contains("rollback sidecar"),
                            "{error:#}"
                        );
                        ensure!(
                            fs::read_to_string(root.path().join("captured.txt"))?
                                == "LIVE_UNINDEXED_SENTINEL\n",
                            "source modified on refusal"
                        );
                        return Ok(());
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
                let observed: airk_core::identity::RevisionToken = recovered["read"]["publication"]
                    ["token"]
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
                let current: airk_core::identity::RevisionToken = recovered["read"]["publication"]
                    ["token"]
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
                Ok(())
            })();
            if let Err(error) = result {
                let retained_source = root.keep();
                let retained_data = data.keep();
                return Err(error).with_context(|| format!(
                    "Current recovery boundary={boundary}, mcp={mcp}; retained source={}, data={}",
                    retained_source.display(), retained_data.display()
                ));
            }
        }
    }
    Ok(())
}
