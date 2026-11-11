// SPDX-License-Identifier: Apache-2.0
//! Native process election/control uses its own private fixture, never an installed service.
#[path = "common/mod.rs"]
mod common;
use anyhow::{Context, Result, ensure};
use common::read_retry;
use read_retry::ReadRetry;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Children(Vec<Child>);

#[test]
fn configured_query_capacity_updates_live_worker_and_reads_through_matching_channels() -> Result<()>
{
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    use airk_core::service_resources::ResourceConfiguration;
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let original = "pub struct Copper;\n";
    fs::write(source.join("fixture.rs"), original)?;
    bind_fixture_data(&runtime, &data)?;
    {
        let kernel =
            airk_kernel::Kernel::open_service_collection(&source, Some(data.clone()), "capacity")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
    }
    let project = airk_kernel::service_projects::registration(&source, &data, "capacity")?;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    let result = tokio::runtime::Runtime::new()?.block_on(async {
        ensure!(matches!(airk_cli::service_runtime::request(&runtime, Method::Register { project: project.clone() }).await?, Response::Registered { .. }), "registration failed");
        let backend = airk_cli::project_client::tools(runtime.clone(), project.binding);
        // Establish the worker first; later configurations must update this same owner.
        let first = read_retry::backend(backend.as_ref(), "airk_symbols", serde_json::json!({"name":"Copper"}).as_object().unwrap(), Instant::now()+Duration::from_secs(5), |reply| eprintln!("capacity fixture initial refusal: {reply}")).await?;
        ensure!(first["isError"] != true, "initial read failed: {first}");
        for concurrency in [4,16,64,4] {
            let configuration = ResourceConfiguration { query_concurrency: concurrency, ..Default::default() };
            let configured = airk_cli::service_runtime::request(&runtime, Method::ConfigureResources { configuration }).await?;
            ensure!(matches!(configured, Response::Resources { ref observation, configuration_error:false, .. } if observation.configuration.query_concurrency == concurrency), "capacity update was not acknowledged");
            let mut requests = tokio::task::JoinSet::new();
            for _ in 0..concurrency {
                let backend = backend.clone();
                requests.spawn(async move { read_retry::backend(backend.as_ref(), "airk_symbols", serde_json::json!({"name":"Copper","max_bytes":4000}).as_object().unwrap(), Instant::now()+Duration::from_secs(5), |reply| eprintln!("capacity fixture {concurrency}-channel refusal: {reply}")).await });
            }
            while let Some(reply) = requests.join_next().await {
                let reply = reply??;
                ensure!(reply["isError"] != true, "{concurrency} channel read failed: {reply}");
            }
            let Response::Health { observation } = airk_cli::service_runtime::request(&runtime, Method::Health).await? else { anyhow::bail!("health missing") };
            ensure!(observation.policy.allocated.concurrent_queries == concurrency + 1, "service capacity differs from configured evidence channels plus status");
            let persisted = airk_cli::service_runtime::request(&runtime, Method::Resources).await?;
            ensure!(matches!(persisted, Response::Resources { observation, .. } if observation.configuration.query_concurrency == concurrency), "resource observation lost capacity");
        }
        backend.close();
        backend.shutdown().await?;
        drop(backend);
        Ok::<_,anyhow::Error>(())
    });
    let shutdown = control(&runtime, "shutdown");
    let exited = wait_exit(&mut children.0[0]);
    result?;
    shutdown?;
    ensure!(exited?, "service did not drain");
    ensure!(
        fs::read_to_string(source.join("fixture.rs"))? == original,
        "source changed"
    );
    Ok(())
}

// Existing evidence fixtures explicitly bind their storage before pre-populating
// it. Production workers bind after activation and never adopt unbound 1.4 data.
fn bind_fixture_data(runtime: &Path, data: &Path) -> Result<()> {
    airk_cli::local_ipc::prepare_root(runtime)?;
    let registry = airk_store::service::registry::ServiceRegistry::open(
        runtime,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    airk_store::service::data_scope::bind(data, runtime, registry.incarnation()?)?;
    Ok(())
}

#[test]
fn service_owner_retires_pressured_receipts_without_console_polling() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{ControlMethod, JobKind, RequestKey},
    };
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("untouched.txt"), b"original source")?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let caller = Incarnation::from_bytes([17; 16]);
    let mut registry = ServiceRegistry::open(&root, caller)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "receipts")?;
    registry.register(&project)?;
    registry.pause(project.binding, true)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: caller,
    };
    let job = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
    // All keys join one paused job. No admission occurs after pressure is full;
    // only the service owner's periodic turn can retire the canceled receipt.
    for number in 1..4096u64 {
        let mut nonce = [0u8; 16];
        nonce[..8].copy_from_slice(&number.to_le_bytes());
        let alias = RequestKey {
            nonce: Incarnation::from_bytes(nonce),
            ..key.clone()
        };
        ensure!(
            registry
                .enqueue(project.binding, caller, &alias, JobKind::Index)?
                .id
                == job.id,
            "alias did not join the paused job"
        );
    }
    registry.cancel_pending(job.id)?;
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while registry.saved_request_epoch()?.1 == epoch {
        ensure!(
            Instant::now() < deadline,
            "native owner did not maintain idle receipt pressure"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let (receipt, expired) = registry.request_receipt(caller, &key)?;
    ensure!(
        receipt.is_none() && expired,
        "retired request could be replayed"
    );
    ensure!(
        registry.get(project.binding)?.paused,
        "receipt maintenance resumed source work"
    );
    ensure!(
        fs::read(source.join("untouched.txt"))? == b"original source",
        "source changed"
    );
    tokio::runtime::Runtime::new()?.block_on(airk_cli::service_runtime::request(
        &root,
        ControlMethod::Shutdown,
    ))?;
    ensure!(
        wait_exit(&mut children.0[0])?,
        "service did not drain after receipt maintenance"
    );
    Ok(())
}

#[test]
fn process_logs_survive_shutdown_and_unknown_log_state_does_not_disable_health() -> Result<()> {
    use airk_core::{
        service::{ControlMethod as Method, ControlResponse as Response},
        service_logs::LogKind,
    };
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&root, method);
        let Response::Health { observation } = request(Method::Health).await? else {
            anyhow::bail!("health unavailable")
        };
        ensure!(
            observation.logs.available && observation.logs.write_failures == 0,
            "owned diagnostic log unavailable"
        );
        ensure!(
            matches!(
                request(Method::StartEmbedding).await?,
                Response::EmbeddingStarting
            ),
            "explicit helper recovery request was not admitted"
        );
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            let Response::Logs { page } = request(Method::Logs { limit: 32 }).await? else {
                anyhow::bail!("recovery log control unavailable")
            };
            if page.records.iter().any(|record| {
                record.kind == LogKind::ModelRecovery
                    && record.code.as_deref() == Some("helper_explicit_start")
            }) {
                break;
            }
            ensure!(
                Instant::now() < until,
                "native IPC omitted helper recovery record"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let Response::Logs { page } = request(Method::Logs { limit: 32 }).await? else {
            anyhow::bail!("cached log control unavailable")
        };
        ensure!(
            page.records
                .iter()
                .any(|record| record.kind == LogKind::ServiceStarted),
            "startup record missing"
        );
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(&mut children.0[0])?,
        "logged service did not drain"
    );
    let cached = airk_cli::service_logs::recent(&root, 32)?;
    for selection in [Vec::<&str>::new(), vec!["--all"]] {
        let exported = command()
            .args(["service", "logs", "--runtime-dir"])
            .arg(&root)
            .args(["--format", "json"])
            .args(selection)
            .output()?;
        ensure!(exported.status.success(), "cached native CLI export failed");
        ensure!(
            serde_json::from_slice::<Value>(&exported.stdout)? == serde_json::to_value(&cached)?,
            "CLI recent/retained page differs from the complete canonical native page"
        );
    }
    let readable = Command::new(env!("CARGO_BIN_EXE_airk"))
        .args([
            "--root",
            "/missing-source-must-not-open",
            "service",
            "logs",
            "--runtime-dir",
        ])
        .arg(&root)
        .args(["--format", "text"])
        .output()?;
    ensure!(
        readable.status.success()
            && String::from_utf8(readable.stdout)?.contains("Cached AIRK process log"),
        "down-service readable diagnostics tried to open a project or lost logs"
    );
    ensure!(
        cached
            .records
            .last()
            .is_some_and(|record| record.kind == LogKind::ServiceStopped),
        "down-service diagnostics lost final lifecycle boundary"
    );
    let other = owner.path().join("other-runtime");
    airk_cli::local_ipc::prepare_root(&other)?;
    fs::create_dir(other.join("logs-1.4"))?;
    let unknown = other.join("logs-1.4/unknown.txt");
    fs::write(&unknown, b"Preserve unrelated log fixture.")?;
    children.0.push(start(&other)?);
    wait_ready(&other)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let Response::Health { observation } =
            airk_cli::service_runtime::request(&other, Method::Health).await?
        else {
            anyhow::bail!("health failed with foreign logs")
        };
        ensure!(
            !observation.logs.available && observation.logs.write_failures > 0,
            "foreign logs were adopted or failure hidden"
        );
        airk_cli::service_runtime::request(&other, Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(&mut children.0[1])?,
        "unavailable-log service did not drain"
    );
    ensure!(
        fs::read(unknown)? == b"Preserve unrelated log fixture."
            && !other.join("logs-1.4.owner.json").exists(),
        "unknown log state was modified"
    );
    Ok(())
}

#[test]
fn maintenance_preview_survives_restart_and_rejects_changed_settings_before_mutation() -> Result<()>
{
    use airk_core::{
        service::{ControlMethod as Method, ControlResponse as Response, RequestKey},
        service_maintenance::{
            PlanOutcome, PlanPhase, RepairOperation, RepairRequest, RepairSelection,
        },
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(
        source.join("memo.md"),
        "# Slate harbor\nRetained slate harbor fixture.\n",
    )?;
    let project = airk_kernel::service_projects::registration(&source, &data, "maintenance")?;
    bind_fixture_data(&runtime, &data)?;
    let kernel = airk_kernel::Kernel::open_service_collection(&source, Some(data), "maintenance")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel
        .status()?
        .publication
        .context("initial publication missing")?
        .token;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    let executor = tokio::runtime::Runtime::new()?;
    let id = executor.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("request epoch unavailable")
        };
        let caller = airk_kernel::service_broker::new_instance_id()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: airk_kernel::service_broker::new_instance_id()?,
        };
        let create = Method::PlanCreate {
            caller,
            key,
            request: RepairRequest {
                selection: RepairSelection::All,
                operation: RepairOperation::Verify,
            },
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        let plan = loop {
            match request(create.clone()).await? {
                Response::Plan { plan } => break plan,
                Response::Failure { code }
                    if code == "registry_response_unknown_retry_with_key" => {}
                other => anyhow::bail!("plan creation failed: {}", serde_json::to_string(&other)?),
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "plan creation did not settle"
            );
        };
        loop {
            let Response::Plan { plan: observed } = request(Method::Plan { id: plan.id }).await?
            else {
                anyhow::bail!("plan unavailable")
            };
            if observed.phase == PlanPhase::Ready {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "preview did not become ready"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(plan.id)
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
    // The next service must retain the exact original preview, then reject its
    // guard after an independently saved setting changes. No source rebuild runs.
    kernel.configure_vectors(true)?;
    children.0.push(start(&runtime)?);
    wait_ready(&runtime)?;
    executor.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        let Response::Plan { plan } = request(Method::Plan { id }).await? else {
            anyhow::bail!("durable preview lost")
        };
        ensure!(
            plan.phase == PlanPhase::Ready,
            "restart executed an unapproved plan"
        );
        request(Method::PlanExecute {
            id,
            preview: plan.preview_digest.context("preview digest missing")?,
            allow_fallback: false,
        })
        .await?;
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let Response::Plan { plan } = request(Method::Plan { id }).await? else {
                anyhow::bail!("execution receipt missing")
            };
            if plan.phase == PlanPhase::Finished {
                ensure!(
                    plan.outcome == Some(PlanOutcome::PartialFailure) && plan.counts.stale == 1,
                    "changed settings were not rejected: {plan:?}"
                );
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "maintenance execution did not settle"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure!(
            kernel
                .refresh_service_snapshot()?
                .status()?
                .publication
                .context("retained publication missing")?
                .token
                == token,
            "stale repair mutated publication"
        );
        kernel.configure_vectors(false)?;
        for operation in [
            RepairOperation::Verify,
            RepairOperation::Reconcile,
            RepairOperation::Rebuild,
        ] {
            if operation == RepairOperation::Reconcile {
                fs::write(
                    source.join("memo.md"),
                    "# Slate harbor\nUpdated slate harbor fixture.\n",
                )?;
            }
            let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
            else {
                anyhow::bail!("request epoch unavailable")
            };
            let Response::Plan { plan } = request(Method::PlanCreate {
                caller: airk_kernel::service_broker::new_instance_id()?,
                key: RequestKey {
                    namespace,
                    epoch,
                    nonce: airk_kernel::service_broker::new_instance_id()?,
                },
                request: RepairRequest {
                    selection: RepairSelection::Selected(vec![project.binding]),
                    operation,
                },
            })
            .await?
            else {
                anyhow::bail!("fresh repair plan unavailable")
            };
            let until = tokio::time::Instant::now() + Duration::from_secs(60);
            let preview = loop {
                let Response::Plan { plan: observed } =
                    request(Method::Plan { id: plan.id }).await?
                else {
                    anyhow::bail!("fresh preview unavailable")
                };
                if observed.phase == PlanPhase::Ready {
                    break observed
                        .preview_digest
                        .context("fresh preview digest missing")?;
                }
                ensure!(
                    tokio::time::Instant::now() < until,
                    "fresh preview did not settle"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            };
            request(Method::PlanExecute {
                id: plan.id,
                preview,
                allow_fallback: false,
            })
            .await?;
            loop {
                let Response::Plan { plan: observed } =
                    request(Method::Plan { id: plan.id }).await?
                else {
                    anyhow::bail!("fresh repair receipt unavailable")
                };
                if observed.phase == PlanPhase::Finished {
                    ensure!(
                        observed.outcome == Some(PlanOutcome::Succeeded),
                        "approved {operation:?} failed: {observed:?}"
                    );
                    break;
                }
                ensure!(
                    tokio::time::Instant::now() < until,
                    "approved repair did not settle"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let Response::PlanItems { page } = request(Method::PlanItems {
                id: plan.id,
                after: None,
                limit: 32,
            })
            .await?
            else {
                anyhow::bail!("project receipt unavailable")
            };
            ensure!(
                page.items.len() == 1
                    && page.items[0].availability == operation.availability()
                    && page.items[0]
                        .inspection
                        .as_ref()
                        .is_some_and(|observation| observation.snapshot.storage.is_some()),
                "repair preview omitted availability/storage observations"
            );
        }
        let updated = kernel
            .refresh_service_snapshot()?
            .status()?
            .publication
            .context("replacement publication missing")?
            .token;
        ensure!(
            updated.incarnation != token.incarnation,
            "approved rebuild did not replace the incarnation"
        );
        ensure!(
            fs::read_to_string(source.join("memo.md"))?
                == "# Slate harbor\nUpdated slate harbor fixture.\n",
            "maintenance changed source bytes"
        );
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(&mut children.0[1])?,
        "restarted service did not drain"
    );
    Ok(())
}

#[test]
fn manually_changed_extra_roots_cannot_capture_another_projects_private_data() -> Result<()> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let other_source = owner.path().join("other-source");
    let data = owner.path().join("data");
    let other_data = owner.path().join("other-data");
    for directory in [&source, &other_source, &data, &other_data] {
        fs::create_dir(directory)?;
    }
    fs::write(
        source.join("memo.md"),
        "# Amber harbor\nPublic amber harbor fixture.\n",
    )?;
    fs::write(
        other_data.join("private.md"),
        "Confidential fictional radish blueprint.\n",
    )?;
    let project = airk_kernel::service_projects::registration(&source, &data, "public")?;
    let other = airk_kernel::service_projects::registration(&other_source, &other_data, "other")?;
    bind_fixture_data(&runtime, &data)?;
    let kernel =
        airk_kernel::Kernel::open_service_collection(&source, Some(data.clone()), "public")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel
        .status()?
        .publication
        .context("initial publication missing")?
        .token;
    // Directly prepare a pending configuration, as if it had been changed outside
    // the native management boundary. It must still be checked before capture.
    kernel.add_root("private", &other_data)?;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        for registered in [project.clone(), other] {
            ensure!(matches!(request(Method::Register { project: registered }).await?, Response::Registered { .. }), "registration failed");
        }
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await? else { anyhow::bail!("epoch missing") };
        let Response::Job { job } = request(Method::Enqueue { project: project.binding,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey { namespace, epoch, nonce: airk_kernel::service_broker::new_instance_id()? }, kind: JobKind::Index }).await?
            else { anyhow::bail!("durable job missing") };
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let Response::Job { job: observed } = request(Method::Job { id: job.id }).await? else { anyhow::bail!("receipt missing") };
            if let Some(outcome) = observed.outcome {
                ensure!(matches!(outcome, JobOutcome::Failed { code } if code == "protected_source_boundary"), "unsafe extra root was not refused before capture");
                break;
            }
            ensure!(tokio::time::Instant::now() < until, "source preflight did not settle");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let selected = kernel.refresh_service_snapshot()?;
        ensure!(selected.status()?.publication.context("retained publication missing")?.token == token, "unsafe root changed publication");
        let query = airk_core::SearchQuery { text: "radish blueprint".into(), path_prefix: None, limit: 5, max_bytes: 4096 };
        ensure!(selected.search(&query)?.hits.is_empty(), "private data entered retained evidence");
        ensure!(fs::read_to_string(other_data.join("private.md"))? == "Confidential fictional radish blueprint.\n", "private fixture changed");
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
    Ok(())
}

#[test]
fn unavailable_source_ancestor_keeps_retained_evidence_and_cancelable_typed_recovery() -> Result<()>
{
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source_parent = owner.path().join("source-volume");
    fs::create_dir(&source_parent)?;
    let source = source_parent.join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(
        source.join("memo.md"),
        "# Copper harbor\nRetained copper harbor fixture.\n",
    )?;
    let project = airk_kernel::service_projects::registration(&source, &data, "missing-source")?;
    bind_fixture_data(&runtime, &data)?;
    let kernel =
        airk_kernel::Kernel::open_service_collection(&source, Some(data), "missing-source")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel
        .status()?
        .publication
        .context("initial publication missing")?
        .token;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        let moved = owner.path().join("temporarily offline");
        // Simulate an unavailable volume/ancestor, not a confirmed deleted leaf
        // whose accessible parent authorizes automatic project removal.
        fs::rename(&source_parent, &moved)?;
        // The first project owner starts after the registered root disappears.
        let mut connection =
            airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        let result = connection
            .read_retry(
                "airk_search".into(),
                serde_json::json!({"query":"copper harbor", "mode":"lexical"})
                    .as_object()
                    .context("fixture query missing")?
                    .clone(),
            )
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) != Some(true)
                && result.to_string().contains("memo.md"),
            "missing source blocked retained evidence: {result}"
        );
        let body = |value: Value| -> Result<Value> {
            Ok(serde_json::from_str(value["content"][0]["text"].as_str().context("missing tool result")?)?)
        };
        ensure!(body(result)?.get("freshness").is_some(), "first client observation missing");
        let stable_until = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let sampled = body(connection.read_retry("airk_search".into(), serde_json::json!({"query":"copper harbor","mode":"lexical","include_status":true}).as_object().unwrap().clone()).await?)?;
            if sampled["freshness"]["state"] == "unregistered" { break }
            ensure!(tokio::time::Instant::now() < stable_until, "service freshness was not forwarded: {sampled}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let receipt = connection.status_receipt();
        ensure!(serde_json::to_vec(&receipt)?.len() <= 1024, "unbounded status receipt");
        drop(connection);
        let arguments = serde_json::json!({"query":"copper harbor", "mode":"lexical"}).as_object().unwrap().clone();
        let mut renewed = airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        renewed.restore_status_receipt(receipt);
        let repeated = body(renewed.read_retry("airk_search".into(), arguments.clone()).await?)?;
        ensure!(repeated.get("freshness").is_none(), "renewal forgot client observations");
        let mut independent = airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        let independent = body(independent.read_retry("airk_search".into(), arguments).await?)?;
        ensure!(independent.get("freshness").is_some(), "another client suppressed initial status");
        ensure!(repeated["publication"]["token"] == independent["publication"]["token"], "display state changed revision identity");
        drop(renewed);
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("request epoch missing")
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("index request was not durably accepted")
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let registry = airk_store::service::registry::ServiceRegistry::open_saved(&runtime)?;
            if registry.recovery_observations(project.binding)?.iter().any(|recovery|
                recovery.job == Some(job.id) && recovery.reason == "source_unavailable" && recovery.state == "waiting") {
                // Read after observing recovery: a receipt sampled before the
                // atomic defer transaction may legitimately still say Running.
                let Response::Job { job: observed } = request(Method::Job { id: job.id }).await? else {
                    anyhow::bail!("job receipt missing")
                };
                ensure!(observed.state == airk_core::service::JobState::Pending && observed.outcome.is_none(),
                    "missing source must wait without losing the accepted request");
                let mut cached = airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
                let sampled = body(cached.read_retry("airk_search".into(), serde_json::json!({"query":"copper harbor","mode":"lexical"}).as_object().unwrap().clone()).await?)?;
                ensure!(sampled["publication"]["token"] == token.to_string() && sampled.to_string().contains("memo.md"),
                    "waiting recovery lost retained evidence: {sampled}");
                break;
            }
            ensure!(tokio::time::Instant::now() < until, "missing-root job did not enter typed recovery");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        request(Method::Pause { project: project.binding, paused: true }).await?;
        request(Method::CancelPending { id: job.id }).await?;
        let retry_key = RequestKey {
            namespace,
            epoch,
            nonce: airk_kernel::service_broker::new_instance_id()?,
        };
        let retry_caller = airk_kernel::service_broker::new_instance_id()?;
        let Response::Job { job: retry } = request(Method::Retry {
            id: job.id,
            caller: retry_caller,
            key: retry_key.clone(),
        })
        .await?
        else {
            anyhow::bail!("explicit failed-job retry was not accepted")
        };
        ensure!(
            retry.id != job.id && retry.attempts == 0,
            "retry erased the original failure"
        );
        request(Method::CancelPending { id: retry.id }).await?;
        let Response::Job { job: replayed } = request(Method::Retry {
            id: job.id,
            caller: retry_caller,
            key: retry_key,
        })
        .await?
        else {
            anyhow::bail!("accepted retry receipt was lost")
        };
        ensure!(
            replayed.id == retry.id && replayed.state == airk_core::service::JobState::Canceled,
            "retry replay restarted canceled work"
        );
        let mut cursor = None;
        let mut found_failure = false;
        let mut found_retry = false;
        loop {
            let Response::Events { page } = request(Method::Events {
                project: Some(project.binding),
                cursor,
                limit: 1,
            })
            .await?
            else {
                anyhow::bail!("durable event page unavailable")
            };
            ensure!(
                page.events.len() <= 1 && serde_json::to_vec(&page)?.len() <= 48 * 1024,
                "event response exceeded requested bounds"
            );
            for event in &page.events {
                ensure!(
                    event.project == project.binding,
                    "foreign event entered project page"
                );
                if event.job == Some(job.id) && event.kind == airk_core::service_events::EventKind::JobCanceled
                {
                    found_failure = true;
                }
                if event.job == Some(retry.id)
                    && event.kind == airk_core::service_events::EventKind::JobRetryRequested
                {
                    found_retry = true;
                }
            }
            cursor = page.next;
            if cursor.is_none() {
                break;
            }
        }
        ensure!(
            found_failure,
            "canceling recovery lost its content-free durable event"
        );
        ensure!(found_retry, "explicit retry lost its durable event");
        ensure!(matches!(request(Method::Events { project: Some(project.binding),cursor:None,limit:0 }).await?,
            Response::Failure { code } if code=="invalid_event_page_limit"),"event page rejection lost its typed reason");
        let Response::Job {job:inspection}=request(Method::Enqueue {
            project:project.binding, caller:airk_kernel::service_broker::new_instance_id()?,
            key:RequestKey {namespace,epoch,nonce:airk_kernel::service_broker::new_instance_id()?},
            kind:JobKind::Inspect,
        }).await? else { anyhow::bail!("read-only inspection was not accepted") };
        let until=tokio::time::Instant::now()+Duration::from_secs(30);
        loop {
            let Response::Job {job:observed}=request(Method::Job{id:inspection.id}).await? else {anyhow::bail!("inspection receipt missing")};
            if let Some(outcome)=observed.outcome {
                ensure!(matches!(outcome,JobOutcome::Succeeded{revision:Some(found)} if found==token),
                    "missing-source inspection did not preserve its exact readable revision");
                break;
            }
            ensure!(tokio::time::Instant::now()<until,"paused-project inspection did not settle");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let Response::Inspection {observation:Some(observation),cached:true,..}=request(Method::Inspection{project:project.binding}).await?
            else {anyhow::bail!("cached inspection unavailable")};
        ensure!(observation.snapshot.publication==Some(token)
            && observation.snapshot.source_state==airk_core::service_inspection::SourceState::Unavailable,
            "source availability was confused with retained index availability");
        let Response::Registered {project:paused}=request(Method::Pause{project:project.binding,paused:true}).await?
            else {anyhow::bail!("project state unavailable")};
        ensure!(paused.paused,"inspection resumed paused source work");
        ensure!(!serde_json::to_string(&observation.snapshot)?.contains("memo.md"),"inspection retained source payload");
        ensure!(
            kernel
                .status()?
                .publication
                .context("retained publication missing")?
                .token
                == token,
            "missing-root scan changed retained evidence"
        );
        ensure!(
            fs::read_to_string(moved.join("source/memo.md"))?
                == "# Copper harbor\nRetained copper harbor fixture.\n",
            "source bytes changed"
        );
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service failed to drain");
    Ok(())
}

#[test]
fn service_watcher_owns_nested_changes_pause_and_restart_without_editor_events() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir_all(source.join("nested"))?;
    fs::create_dir(&data)?;
    let file = source.join("nested/original.txt");
    fs::write(&file, b"Initial native watcher fixture.\n")?;
    let project = airk_kernel::service_projects::registration(&source, &data, "watch-fixture")?;
    let service = qualified_process_fixture(&source, &data, "watch-fixture")?;
    let selection = selected_scope(service.http_selection())?;
    let root = service.control.clone();
    let mut children = Children(vec![start_qualified(&service)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&root, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "watch registration failed"
        );
        ensure!(
            matches!(
                request(Method::ConfigureWatch {
                    project: project.binding,
                    enabled: true
                })
                .await?,
                Response::Watch { .. }
            ),
            "watch configuration failed"
        );
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        let baseline = loop {
            if let Response::Watch { observation, .. } = request(Method::Watch {
                project: project.binding,
            })
            .await?
                && observation.backend == airk_core::service_watch::WatchBackend::Native
                && observation.completed_revision > 0
                && observation.completed_revision == observation.requested_revision
            {
                break observation.completed_revision;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "initial automatic reconciliation/native watch did not become current: {}",
                serde_json::to_string(
                    &request(Method::Console {
                        after: None,
                        revision: None,
                        limit: 1
                    })
                    .await?
                )?
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let Response::SourceStatus {
            available: true,
            process_id: Some(_),
            watched_directories: Some(directories),
            ..
        } = request(Method::SourceStatus).await?
        else {
            anyhow::bail!("source process was not independently supervised");
        };
        ensure!(
            (2..=512).contains(&directories),
            "nested directory watch count is invalid"
        );
        request(Method::Pause {
            project: project.binding,
            paused: true,
        })
        .await?;
        let changed = b"Paused saved source contains a copper lighthouse.\n";
        fs::write(&file, changed)?;
        let until = tokio::time::Instant::now() + Duration::from_secs(20);
        let desired = loop {
            if let Response::Watch { observation, .. } = request(Method::Watch {
                project: project.binding,
            })
            .await?
                && observation.requested_revision > baseline
            {
                ensure!(
                    observation.completed_revision == baseline,
                    "paused source work was claimed"
                );
                break observation.requested_revision;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "nested change was not retained while paused"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        request(Method::Pause {
            project: project.binding,
            paused: false,
        })
        .await?;
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Response::Watch { observation, .. } = request(Method::Watch {
                project: project.binding,
            })
            .await?
                && observation.completed_revision >= desired
                && observation.completed_revision == observation.requested_revision
            {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "resumed native source work did not settle"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut connection = connect_qualified(&root, project.binding, &selection).await?;
        let evidence = connection
            .read_retry(
                "airk_search".into(),
                serde_json::json!({"query":"copper lighthouse","mode":"lexical","max_bytes":4096})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        ensure!(
            evidence.to_string().contains("original.txt"),
            "native watch lost direct lexical citations"
        );
        ensure!(
            fs::read(&file)? == changed,
            "native watcher changed source bytes"
        );
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(&mut children.0[0])?,
        "service watcher did not drain"
    );
    drop(airk_store::service::SourceWatchLease::acquire(&root)?);
    let offline = b"Saved while the service was stopped: lavender signal.\n";
    fs::write(&file, offline)?;
    children.0.push(start_qualified(&service)?);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let mut connection = connect_qualified(&root, project.binding,&selection).await?;
            let evidence = connection.read_retry("airk_search".into(), serde_json::json!({"query":"lavender signal","mode":"lexical","max_bytes":4096}).as_object().unwrap().clone()).await?;
            if evidence.to_string().contains("original.txt") { break; }
            ensure!(tokio::time::Instant::now() < until, "restart did not reconcile changes saved without an editor");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        airk_cli::service_runtime::request(&root, Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(children.0.last_mut().unwrap())?,
        "restarted watcher did not drain"
    );
    ensure!(fs::read(file)? == offline, "restart changed source bytes");
    finish_qualified_process_fixture(&service)?;
    Ok(())
}
#[test]
fn over_budget_document_stays_bounded_across_service_restart() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir_all(&source)?;
    fs::create_dir(&data)?;
    let attributes = (0..1025)
        .map(|index| format!(" a{index}=\"v\""))
        .collect::<String>();
    fs::write(
        source.join("wide.svg"),
        format!("<svg{attributes}><title>wide</title></svg>"),
    )?;
    let notes = source.join("notes.md");
    fs::write(&notes, "# Zircon harbor\nZircon harbor notes.\n")?;
    let project = airk_kernel::service_projects::registration(&source, &data, "budget-fixture")?;
    let service = qualified_process_fixture(&source, &data, "budget-fixture")?;
    let selection = selected_scope(service.http_selection())?;
    let root = service.control.clone();
    // Settled automatic work must find `term` while the unchanged over-budget file
    // remains, and no job may stall or rely on the watchdog.
    async fn settle(
        root: &Path,
        project: &airk_core::service::ProjectRegistration,
        selection: &airk_core::service::ScopedConnectRequest,
        term: &str,
    ) -> Result<()> {
        let until = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            let watch = airk_cli::service_runtime::request(
                root,
                Method::Watch {
                    project: project.binding,
                },
            )
            .await?;
            let settled = matches!(&watch, Response::Watch { observation, .. }
                if observation.completed_revision > 0
                    && observation.completed_revision == observation.requested_revision);
            if settled {
                let mut connection = connect_qualified(root, project.binding, selection).await?;
                let evidence = connection
                    .read_retry(
                        "airk_search".into(),
                        serde_json::json!({"query": term, "mode": "lexical", "max_bytes": 4096})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )
                    .await?;
                if evidence.to_string().contains("notes.md") {
                    break;
                }
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "automatic work did not settle around the over-budget document"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let console = airk_cli::service_runtime::request(
            root,
            Method::Console {
                after: None,
                revision: None,
                limit: 64,
            },
        )
        .await?;
        ensure!(
            !serde_json::to_string(&console)?.contains("work_stalled"),
            "over-budget extraction reached the stall watchdog"
        );
        Ok(())
    }
    let mut children = Children(vec![start_qualified(&service)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&root, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "budget fixture registration failed"
        );
        ensure!(
            matches!(
                request(Method::ConfigureWatch {
                    project: project.binding,
                    enabled: true
                })
                .await?,
                Response::Watch { .. }
            ),
            "budget fixture watch configuration failed"
        );
        settle(&root, &project, &selection, "zircon harbor").await?;
        request(Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
    drop(airk_store::service::SourceWatchLease::acquire(&root)?);
    fs::write(&notes, "# Zircon harbor\nRestarted garnet beacon.\n")?;
    children.0.push(start_qualified(&service)?);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        settle(&root, &project, &selection, "garnet beacon").await?;
        airk_cli::service_runtime::request(&root, Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(children.0.last_mut().unwrap())?,
        "restarted service did not drain"
    );
    finish_qualified_process_fixture(&service)?;
    Ok(())
}

impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command() -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
        command.creation_flags(0x08000000);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(env!("CARGO_BIN_EXE_airk"))
    }
}

fn start(root: &Path) -> Result<Child> {
    start_command(root, command())
}

fn start_command(root: &Path, mut process: Command) -> Result<Child> {
    static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let log = root
        .parent()
        .context("fixture parent missing")?
        .join(format!("service-child-{sequence}.stderr"));
    let log = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    Ok(process
        .arg("serve")
        .arg("--runtime-dir")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?)
}

fn require_request_epoch(
    response: airk_core::service::ControlResponse,
    phase: &str,
) -> Result<(airk_core::identity::Incarnation, u64)> {
    match response {
        airk_core::service::ControlResponse::RequestEpoch { namespace, epoch } => {
            Ok((namespace, epoch))
        }
        unexpected => anyhow::bail!(
            "{phase}: unexpected RequestEpoch reply: {}",
            serde_json::to_string(&unexpected)?
        ),
    }
}

/// Observe this test's actual metadata precondition before submitting its one
/// RequestEpoch mutation. Health and checkpoint release do not synchronize an
/// already-entered frozen metadata receive. Only the read-only Projects probe
/// repeats, and only after its known unexecuted checkpoint refusal.
async fn observe_metadata_projects<F, Fut>(
    deadline: tokio::time::Instant,
    phase: &str,
    mut observe: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<airk_core::service::ControlResponse>>,
{
    use airk_core::service::ControlResponse;
    let mut last_reply = Value::Null;
    loop {
        ensure!(
            tokio::time::Instant::now() < deadline,
            "{phase}: metadata Projects readiness deadline; last_reply={last_reply}"
        );
        let reply = tokio::time::timeout_at(deadline, observe())
            .await
            .with_context(|| {
                format!("{phase}: metadata Projects readiness deadline; last_reply={last_reply}")
            })?
            .with_context(|| {
                format!("{phase}: metadata Projects observation failed; last_reply={last_reply}")
            })?;
        match reply {
            ControlResponse::Projects { ref page } => {
                ensure!(
                    page.projects.len() <= 1 && (page.after.is_none() || !page.projects.is_empty()),
                    "{phase}: invalid one-item Projects readiness page: {}",
                    serde_json::to_string(&reply)?
                );
                return Ok(());
            }
            ControlResponse::Failure { ref code } if code == "runtime_upgrade_in_progress" => {
                last_reply = serde_json::to_value(reply)?;
            }
            unexpected => anyhow::bail!(
                "{phase}: unexpected Projects readiness reply: {}",
                serde_json::to_string(&unexpected)?
            ),
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn metadata_readiness_observer_repeats_only_known_unexecuted_projects_refusal() -> Result<()>
{
    use airk_core::service::{ControlResponse, ProjectPage};
    let mut calls = 0;
    observe_metadata_projects(
        tokio::time::Instant::now() + Duration::from_secs(5),
        "post-release",
        || {
            calls += 1;
            std::future::ready(Ok(if calls == 1 {
                ControlResponse::Failure {
                    code: "runtime_upgrade_in_progress".into(),
                }
            } else {
                ControlResponse::Projects {
                    page: ProjectPage {
                        revision: 1,
                        projects: Vec::new(),
                        after: None,
                    },
                }
            }))
        },
    )
    .await?;
    assert_eq!(calls, 2);
    for code in [
        "registry_response_unknown_retry_with_key",
        "registry_unavailable",
        "service_instance_changed",
        "registry_busy",
    ] {
        calls = 0;
        let error = observe_metadata_projects(
            tokio::time::Instant::now() + Duration::from_secs(5),
            "post-release",
            || {
                calls += 1;
                std::future::ready(Ok(ControlResponse::Failure { code: code.into() }))
            },
        )
        .await
        .err()
        .context("unclassified reply must stop readiness observation")?;
        assert_eq!(calls, 1);
        assert!(
            error
                .to_string()
                .contains(&format!("{{\"status\":\"failure\",\"code\":\"{code}\"}}")),
            "{error:#}"
        );
        assert!(error.to_string().contains("post-release"));
    }
    calls = 0;
    let error = observe_metadata_projects(
        tokio::time::Instant::now() + Duration::from_secs(5),
        "post-release",
        || {
            calls += 1;
            std::future::ready(Err(anyhow::anyhow!("unknown transport outcome")))
        },
    )
    .await
    .err()
    .context("unknown transport outcome must stop observation")?;
    assert_eq!(calls, 1);
    assert!(format!("{error:#}").contains("unknown transport outcome"));
    assert!(error.to_string().contains("post-release"));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn metadata_readiness_deadline_and_epoch_failure_keep_complete_phase_observations()
-> Result<()> {
    use airk_core::service::ControlResponse;
    let mut calls = 0;
    let error = observe_metadata_projects(
        tokio::time::Instant::now() + Duration::from_secs(5),
        "post-release",
        || {
            calls += 1;
            let advance = Duration::from_millis(if calls == 1 { 4999 } else { 1 });
            async move {
                tokio::time::advance(advance).await;
                Ok(ControlResponse::Failure {
                    code: "runtime_upgrade_in_progress".into(),
                })
            }
        },
    )
    .await
    .err()
    .context("expired readiness must preserve its pending reply")?;
    assert!(
        calls <= 2,
        "no observation may be submitted after its original deadline"
    );
    assert!(error.to_string().contains("post-release"));
    assert!(
        error.to_string().contains("runtime_upgrade_in_progress"),
        "{error:#}"
    );
    let error = require_request_epoch(
        ControlResponse::Failure {
            code: "runtime_upgrade_in_progress".into(),
        },
        "after Projects readiness",
    )
    .err()
    .context("unexpected epoch reply must be retained")?;
    assert_eq!(
        error.to_string(),
        "after Projects readiness: unexpected RequestEpoch reply: {\"status\":\"failure\",\"code\":\"runtime_upgrade_in_progress\"}"
    );
    Ok(())
}

// Process-lifecycle campaigns still own real child handles, while fresh model
// publication uses the same approved catalog/storage setup as installed users.
fn qualified_process_fixture(
    source: &Path,
    data: &Path,
    label: &str,
) -> Result<std::sync::Arc<common::Service>> {
    let service = common::Service::reviewed(source, data, label);
    service.settle_reviewed_index(source, data, Instant::now() + Duration::from_secs(90))?;
    stop_qualified_bootstrap(&service)?;
    Ok(service)
}

fn selected_scope(
    selection: airk_core::service_http::HttpSelection,
) -> Result<airk_core::service::ScopedConnectRequest> {
    let airk_core::service_http::HttpSelection::Scoped { selection } = selection else {
        anyhow::bail!("process fixture requires its exact reviewed grant");
    };
    Ok(selection)
}

async fn refreshed_scope(
    root: &Path,
    project: airk_core::service::ProjectBinding,
    selection: &airk_core::service::ScopedConnectRequest,
) -> Result<airk_core::service::ScopedConnectRequest> {
    let refreshed = airk_cli::service_frontend::resolve_scoped(None, Some(root), selection).await?;
    ensure!(
        refreshed.project.registration.binding == project,
        "process fixture scoped backing changed"
    );
    Ok(refreshed.selection)
}

async fn connect_qualified(
    root: &Path,
    project: airk_core::service::ProjectBinding,
    selection: &airk_core::service::ScopedConnectRequest,
) -> Result<airk_cli::project_client::Connection> {
    airk_cli::project_client::Connection::connect_scoped(
        root,
        project,
        &refreshed_scope(root, project, selection).await?,
        airk_core::usage::ClientFamily::Unknown,
    )
    .await
}

fn stop_qualified_bootstrap(service: &common::Service) -> Result<()> {
    let response = service.request_until(
        airk_core::service::ControlMethod::Shutdown,
        Instant::now() + Duration::from_secs(5),
    )?;
    ensure!(
        matches!(response, airk_core::service::ControlResponse::Stopping),
        "qualified bootstrap did not acknowledge shutdown"
    );
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(lease) = airk_store::service::ServiceLease::acquire(&service.control) {
            drop(lease);
            break;
        }
        ensure!(
            Instant::now() < until,
            "qualified bootstrap retained native ownership"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

fn storage_fingerprints(
    root: &Path,
) -> Result<std::collections::BTreeMap<std::path::PathBuf, String>> {
    fn advisory(path: &Path) -> bool {
        let parts = path.iter().map(|value| value.to_str()).collect::<Vec<_>>();
        matches!(
            parts.as_slice(),
            [Some("service-data-1.4.lock" | ".lifecycle-1.4.lock")]
                | [Some(".quota-1.4"), Some("ledger.lock")]
                | [
                    Some("collections-1.4"),
                    Some(_),
                    Some("manifest.lock" | "replacement.lock")
                ]
        ) || matches!(
            parts.as_slice(),
            [
                Some("collections-1.4"),
                Some(".writer-locks" | ".reader-locks" | ".current-gates"),
                ..
            ]
        ) || matches!(
            // A caller holding the service lease OS-locks this advisory file,
            // wherever the fingerprinted root places its control directory.
            parts.as_slice(),
            [.., Some("control"), Some("instance-1.4.lock")]
        )
    }
    fn collect(
        root: &Path,
        path: &Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, String>,
    ) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let relative = entry.path().strip_prefix(root)?.to_owned();
            if kind.is_dir() {
                out.insert(relative, "directory".into());
                collect(root, &entry.path(), out)?;
            } else {
                ensure!(
                    kind.is_file(),
                    "qualified storage contains redirected entry"
                );
                // Native advisory bytes are OS-locked on Windows. Compare their
                // exact inventory/type/length; hash all durable index/owner bytes.
                let fingerprint = if advisory(&relative) {
                    format!("native-advisory:{}", entry.metadata()?.len())
                } else {
                    airk_cli::runtime_pair::digest_file(&entry.path()).with_context(|| {
                        format!(
                            "fingerprint durable qualified storage {}",
                            relative.display()
                        )
                    })?
                };
                out.insert(relative, fingerprint);
            }
        }
        Ok(())
    }
    let mut out = std::collections::BTreeMap::new();
    collect(root, root, &mut out)?;
    Ok(out)
}

fn cold_project_idle(
    service: &common::Service,
    project: airk_core::service::ProjectBinding,
    paused: bool,
    until: Instant,
) -> Result<bool> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobState};
    let progress = service.request_until(Method::Progress { project }, until)?;
    let Response::Progress {
        project: seen,
        pending_job,
        queued_jobs,
        queue_truncated,
        project_paused,
        observation,
        ..
    } = &progress
    else {
        anyhow::bail!(
            "cold project progress unavailable: {}",
            serde_json::to_string(&progress)?
        );
    };
    ensure!(*seen==project && pending_job.as_ref().is_none_or(|job|job.project==project)
        && queued_jobs.iter().all(|job|job.project==project)
        && observation.as_ref().is_none_or(|value|value.worker.project==project && value.job.project==project),"cold project progress crossed identity");
    let watch = service.request_until(Method::Watch { project }, until)?;
    let Response::Watch {
        project: seen,
        observation: watch,
    } = watch
    else {
        anyhow::bail!(
            "cold project watch unavailable: {}",
            serde_json::to_string(&watch)?
        );
    };
    ensure!(seen == project, "cold project watch crossed identity");
    watch.validate()?;
    ensure!(
        watch.failure.is_none() && !watch.automatic_retries_exhausted,
        "cold project watch failed: {watch:?}"
    );
    Ok(pending_job.is_none()
        && queued_jobs.is_empty()
        && *queue_truncated == Some(false)
        && *project_paused == Some(paused)
        && watch.completed_revision == watch.requested_revision
        && observation
            .as_ref()
            .is_none_or(|value| !matches!(value.job.state, JobState::Pending | JobState::Running)))
}

fn pause_cold_projects(
    service: &common::Service,
    projects: &[airk_core::service::ProjectRegistration],
    until: Instant,
) -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    loop {
        let mut idle = true;
        for project in projects {
            idle &= cold_project_idle(service, project.binding, false, until)?;
        }
        if idle {
            for project in projects {
                let response = service.request_until(
                    Method::Pause {
                        project: project.binding,
                        paused: true,
                    },
                    until,
                )?;
                ensure!(
                    matches!(response,Response::Registered { project:saved } if saved.registration==*project && saved.paused),
                    "cold project pause refused/crossed identity"
                );
            }
            let mut pinned = true;
            for project in projects {
                pinned &= cold_project_idle(service, project.binding, true, until)?;
            }
            if pinned {
                return Ok(());
            }
            // Work accepted between idle and Pause still owns its original job.
            // Resume only these fixture projects so that work can actually drain.
            for project in projects {
                let response = service.request_until(
                    Method::Pause {
                        project: project.binding,
                        paused: false,
                    },
                    until,
                )?;
                ensure!(
                    matches!(response,Response::Registered { project:saved } if saved.registration==*project && !saved.paused),
                    "cold project resume refused/crossed identity"
                );
            }
        }
        ensure!(
            Instant::now() + Duration::from_millis(25) < until,
            "initial native project jobs did not drain before cold metadata test"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn start_qualified(service: &common::Service) -> Result<Child> {
    let template = service.installed_command(
        Path::new(&service.project.root),
        Path::new(&service.project.data_root),
        &service.project.collection,
    );
    let mut command = Command::new(template.get_program());
    for (key, value) in template.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let log = tempfile::NamedTempFile::new_in(
        service
            .control
            .parent()
            .context("qualified control parent missing")?,
    )?;
    let (_, path) = log.keep()?;
    Ok(command
        .args(["serve", "--runtime-dir"])
        .arg(&service.control)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::OpenOptions::new().append(true).open(path)?)
        .spawn()?)
}

// These tests deliberately end with native Shutdown or owner loss. Re-establish
// only this exact owned image to obtain the canonical installation Stop receipt
// required before deleting the private managed fixture.
fn finish_qualified_process_fixture(service: &common::Service) -> Result<()> {
    let mut child = start_qualified(service)?;
    wait_ready(&service.control)?;
    let stopped = service.shutdown_reviewed();
    if stopped.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    stopped?;
    ensure!(
        wait_exit(&mut child)?,
        "qualified cleanup child did not drain"
    );
    Ok(())
}

fn control(root: &Path, action: &str) -> Result<Value> {
    let result = command()
        .args(["service", action, "--runtime-dir"])
        .arg(root)
        .output()?;
    ensure!(
        result.status.success(),
        "service control failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(serde_json::from_slice(&result.stdout)?)
}

fn wait_ready(root: &Path) -> Result<Value> {
    // Debug ELF images include large DWARF sections; full identity hashing is part of startup.
    let deadline = Instant::now() + Duration::from_secs(60);
    let client = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    loop {
        if let Ok(response) = client.block_on(airk_cli::service_runtime::request(
            root,
            airk_core::service::ControlMethod::Health,
        )) {
            return Ok(serde_json::to_value(response)?);
        }
        if Instant::now() >= deadline {
            use std::io::Read;
            let mut detail = String::new();
            for entry in fs::read_dir(root.parent().context("fixture parent missing")?)?.take(16) {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("service-child-")
                {
                    fs::File::open(entry.path())?
                        .take(4096)
                        .read_to_string(&mut detail)?;
                }
            }
            anyhow::bail!("native service failed to become ready: {detail}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_exit(child: &mut Child) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        ensure!(Instant::now() < deadline, "native service failed to drain");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn on_demand_idle_exit_does_not_require_login_manager_and_health_does_not_extend_it() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let child = command()
        .arg("serve")
        .arg("--runtime-dir")
        .arg(&root)
        .args(["--idle-exit-seconds", "3"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut children = Children(vec![child]);
    wait_ready(&root)?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let _ = control(&root, "health");
        if let Some(status) = children.0[0].try_wait()? {
            ensure!(status.success(), "idle service did not exit cleanly");
            break;
        }
        ensure!(
            Instant::now() < until,
            "read-only health polling prevented idle shutdown"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    ensure!(
        airk_store::service::ServiceLease::acquire(&root).is_ok(),
        "idle service retained ownership"
    );
    Ok(())
}

#[test]
fn enabled_background_watch_retains_idle_service_until_explicitly_disabled() -> Result<()> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "idle-watch")?;
    let child = command()
        .arg("serve")
        .arg("--runtime-dir")
        .arg(&runtime)
        .args(["--idle-exit-seconds", "3"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut children = Children(vec![child]);
    wait_ready(&runtime)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        // Pause prevents automatic indexing from accidentally keeping the process
        // alive. The watcher alone must retain ownership, even with no new events.
        request(Method::Pause {
            project: project.binding,
            paused: true,
        })
        .await?;
        // Registration alone has no authoritative root observation. Inspect is
        // admitted while paused and lets the native worker publish that metadata
        // without indexing source content or changing the pause preference.
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("inspection request epoch unavailable");
        };
        let Response::Job { job: inspection } = request(Method::Enqueue {
            project: project.binding,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            kind: JobKind::Inspect,
        })
        .await?
        else {
            anyhow::bail!("quiet-watcher inspection was not accepted");
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let Response::Job { job } = request(Method::Job { id: inspection.id }).await? else {
                anyhow::bail!("quiet-watcher inspection receipt unavailable");
            };
            ensure!(
                job.project == project.binding && job.kind == JobKind::Inspect,
                "quiet-watcher inspection receipt changed identity"
            );
            if let Some(outcome) = &job.outcome {
                ensure!(
                    matches!(outcome, JobOutcome::Succeeded { .. }),
                    "quiet-watcher inspection failed: {job:?}"
                );
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "quiet-watcher inspection did not settle: {job:?}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        request(Method::ConfigureWatch {
            project: project.binding,
            enabled: true,
        })
        .await?;
        let until = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let source = request(Method::SourceStatus).await?;
            let worker = request(Method::Worker {
                project: project.binding,
            })
            .await?;
            let resources = request(Method::Resources).await?;
            let console = request(Method::Console {
                after: None,
                revision: None,
                limit: 1,
            })
            .await?;
            // Worker waits retain the last sampled job even after it completes.
            // Only this already-succeeded Inspect may remain as historical wait
            // metadata; the worker and all live jobs/grants must still be gone.
            // An ordinary worker may also attempt the optional helper at startup.
            // Its stopped, jobless global failure is retained history, not active
            // recovery work. Every other recovery observation prevents readiness.
            let ready = matches!(
                &source,
                Response::SourceStatus {
                    available: true,
                    projects: Some(1),
                    pending_events: Some(0),
                    ..
                }
            ) && matches!(&worker, Response::Worker {
                project: found, identity: None, restarting: false, waiting, ..
            } if *found == project.binding
                && waiting.as_ref().is_none_or(|wait| wait.job == inspection.id))
                && matches!(&resources, Response::Resources {
                    observation, configuration_error: false, ..
                } if observation.resident_workers == 0
                    && observation.running_jobs == 0
                    && observation.waiting_projects == 0
                    && observation.pending_provider_drains == 0
                    && !observation.provider_drain_error
                    && observation.extraction_threads_reserved == 0
                    && observation.background_io_reserved_bytes_per_second == 0)
                && matches!(&console, Response::Console { page }
                    if page.after.is_none() && page.projects.len() == 1
                        && page.projects[0].project.registration.binding == project.binding
                        && page.projects[0].project.paused
                        && page.projects[0].pending_job.is_none()
                        && page.projects[0].pending_gate.is_none()
                        && page.projects[0].recovery.iter().all(|recovery|
                            recovery.reason == "helper_unavailable"
                                && recovery.state == "stopped"
                                && recovery.job.is_none())
                        && page.projects[0].failed_observations.is_empty()
                        && page.projects[0].watch.as_ref().is_some_and(|watch|
                            watch.configuration_observed && watch.enabled));
            if ready {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "quiet watcher or worker/grant drain did not become ready: {}",
                serde_json::to_string(&(source, worker, resources, console))?
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // No clients remain connected, no work is claimed and no health/status
        // polling can explain survival beyond the configured idle grace.
        tokio::time::sleep(Duration::from_secs(5)).await;
        ensure!(
            children.0[0].try_wait()?.is_none(),
            "idle shutdown discarded enabled background monitoring"
        );
        request(Method::ConfigureWatch {
            project: project.binding,
            enabled: false,
        })
        .await?;
        let until = tokio::time::Instant::now() + Duration::from_secs(25);
        loop {
            if let Some(status) = children.0[0].try_wait()? {
                ensure!(status.success(), "disabled watcher did not drain cleanly");
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "disabled background monitoring prevented idle exit"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok::<(), anyhow::Error>(())
    })?;
    ensure!(
        airk_store::service::ServiceLease::acquire(&runtime).is_ok(),
        "idle exit retained service ownership"
    );
    ensure!(
        airk_store::service::SourceWatchLease::acquire(&runtime).is_ok(),
        "idle exit retained watcher ownership"
    );
    Ok(())
}

fn probe_https_mcp(service: &common::Service) -> Result<()> {
    let mut session =
        common::http::HttpMcpSession::start(&service.control, service.http_selection())?;
    let initialized = session.request(1,"initialize", serde_json::json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-original-test","version":"1"}}))?;
    ensure!(
        initialized["result"]["serverInfo"]["name"] == "airk",
        "HTTPS initialization failed"
    );
    session.send(serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let listed = session.request(2, "tools/list", serde_json::json!({}))?;
    let tools = listed["result"]["tools"]
        .as_array()
        .context("HTTPS tool list missing")?;
    ensure!(
        tools.len() == 29
            && tools
                .iter()
                .all(|tool| !tool["name"].as_str().unwrap_or("").starts_with("airk_")),
        "public MCP catalog is not unified"
    );
    let canonical = serde_json::to_value(airk_mcp::unified::tools())?;
    ensure!(
        listed["result"]["tools"] == canonical,
        "HTTPS public catalog lost canonical schemas or annotations"
    );
    let scope = tools
        .iter()
        .find(|tool| tool["name"] == "query_scope")
        .context("query scope missing")?;
    let mut expected_schema = airk_core::combined_reads::query_scope::schema();
    expected_schema["$schema"] = serde_json::json!("https://json-schema.org/draft/2020-12/schema");
    let project = &scope["inputSchema"]["properties"]["project"];
    ensure!(
        project["type"] == "string"
            && project["pattern"] == "^[0-9a-f]{64}$"
            && !scope["inputSchema"]["required"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|field| field == "project")),
        "query scope optional binding changed"
    );
    expected_schema["properties"]["project"] = project.clone();
    ensure!(
        scope["annotations"]["readOnlyHint"] == true && scope["inputSchema"] == expected_schema,
        "query scope metadata diverged"
    );
    for field in ["project_ids", "index_ids", "verification", "response_mode"] {
        ensure!(
            scope["inputSchema"]["properties"].get(field).is_none(),
            "query scope advertised unsupported {field}"
        );
    }
    // The native 19-tool catalog remains complete, independent of its retired
    // public stdin role. Exercise metadata directly rather than relaunching it.
    let native = airk_mcp::local::tools();
    ensure!(
        native.len() == 19
            && native.iter().all(|tool| serde_json::to_value(tool)
                .is_ok_and(|tool| tool["annotations"]["readOnlyHint"] == true)),
        "native catalog exposed management"
    );
    let query = serde_json::json!({"query":"cobalt harbor","mode":"lexical","max_bytes":4096});
    let deadline = Instant::now() + Duration::from_secs(10);
    let evidence = loop {
        ensure!(
            Instant::now() < deadline,
            "HTTPS read retry deadline exhausted"
        );
        let reply = session.request(
            3,
            "tools/call",
            serde_json::json!({"name":"search","arguments":query}),
        )?;
        let Some(retry) = read_retry::within(&reply["result"], deadline) else {
            break reply;
        };
        std::thread::sleep(retry.delay);
    };
    ensure!(
        evidence["id"] == 3
            && evidence.to_string().contains("original.txt")
            && evidence.to_string().contains("cobalt harbor"),
        "HTTPS lost original evidence"
    );
    session.close()?;
    Ok(())
}

#[test]
fn reviewed_https_discovery_and_evidence_preserve_native_contracts() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.txt"),
        "cobalt harbor original evidence\n",
    )?;
    let service = common::Service::reviewed(source.path(), data.path(), "fixture");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(airk_core::service::JobKind::Index);
    probe_https_mcp(&service)
}

#[test]
fn actual_https_protocol_negotiation_preserves_legacy_fallback_and_metadata() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.txt"),
        "Protocol negotiation source\n",
    )?;
    let service = common::Service::reviewed(source.path(), data.path(), "http-negotiation");
    for (requested, negotiated) in [
        ("2024-11-05", "2024-11-05"),
        ("2025-11-25", "2025-11-25"),
        ("2026-07-28", "2025-11-25"),
    ] {
        let mut session =
            common::http::HttpMcpSession::start(&service.control, service.http_selection())?;
        let initialized = session.request(1, "initialize", serde_json::json!({"protocolVersion":requested,"capabilities":{},"clientInfo":{"name":"http-negotiation","version":"1"}}))?;
        ensure!(
            initialized["result"]["protocolVersion"] == negotiated,
            "requested {requested} negotiation changed: {initialized}"
        );
        session.send(serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        ensure!(
            session
                .request(2, "ping", serde_json::json!({}))?
                .get("result")
                .is_some()
        );
        ensure!(
            session.request(3, "tools/list", serde_json::json!({}))?["result"]["tools"].is_array()
        );
        // Raw probes deliberately keep the negotiated header instead of fixing
        // the body: mismatched per-request metadata must still be rejected.
        let other = if negotiated == "2024-11-05" {
            "2025-11-25"
        } else {
            "2024-11-05"
        };
        let error = session.probe(serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":4,"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":other}}}))?).expect_err("mismatched HTTP metadata accepted");
        ensure!(
            error
                .downcast_ref::<common::http::HttpRefused>()
                .context("expected HTTP metadata refusal")?
                .status
                == 400
        );
        session.close()?;
    }
    let mut mismatch =
        common::http::HttpMcpSession::start(&service.control, service.http_selection())?;
    let error = mismatch.probe(serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"deliberate-header-mismatch","version":"1"}}}))?).expect_err("mismatched initial header accepted");
    ensure!(
        error
            .downcast_ref::<common::http::HttpRefused>()
            .context("expected initial HTTP refusal")?
            .status
            == 400
    );
    mismatch.close()?;
    let mut inline =
        common::http::HttpMcpSession::start(&service.control, service.http_selection())?;
    let meta = serde_json::json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"http-inline-negotiation","version":"1"}});
    let discovered = inline.request(1, "server/discover", serde_json::json!({"_meta":meta}))?;
    ensure!(
        discovered["result"]["supportedVersions"]
            .as_array()
            .is_some_and(|versions| versions.iter().any(|version| version == "2026-07-28"))
    );
    ensure!(
        inline.request(2, "tools/list", serde_json::json!({"_meta":meta}))?["result"]["tools"]
            .is_array()
    );
    inline.close()?;
    Ok(())
}

#[test]
fn actual_https_body_bound_and_malformed_requests_are_content_free() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = b"Original immutable HTTPS framing source\n";
    fs::write(source.path().join("original.txt"), original)?;
    let service = common::Service::reviewed(source.path(), data.path(), "frame-bound");
    for kind in ["exact", "overflow", "malformed"] {
        let mut session =
            common::http::HttpMcpSession::start(&service.control, service.http_selection())?;
        let initialized = session.request(1,"initialize",serde_json::json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"https-bound-fixture","version":"1"}}))?;
        ensure!(initialized["result"]["serverInfo"]["name"] == "airk");
        session.send(serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        let mut bytes =
            serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"ping"}))?;
        match kind {
            "exact" => bytes.resize(airk_core::service::MAX_FRAME_BYTES, b' '),
            "overflow" => bytes.resize(airk_core::service::MAX_FRAME_BYTES + 1, b' '),
            _ => bytes = b"PRIVATE_MALFORMED_BODY".to_vec(),
        }
        if kind == "exact" {
            let reply = session
                .probe(bytes)?
                .context("exact-bound HTTPS reply missing")?;
            ensure!(
                reply["id"] == 2 && reply.get("result").is_some(),
                "exact-bound HTTPS request refused: {reply}"
            );
            let next = session.request(3, "ping", serde_json::json!({}))?;
            ensure!(
                next["id"] == 3 && next.get("result").is_some(),
                "body counter did not reset"
            );
        } else {
            let failure = session
                .probe(bytes)
                .expect_err("invalid HTTPS body accepted");
            let refusal = failure
                .downcast_ref::<common::http::HttpRefused>()
                .context("expected real HTTP refusal")?;
            ensure!(
                refusal.status == if kind == "overflow" { 413 } else { 415 },
                "unexpected HTTP body refusal: {failure}"
            );
            ensure!(
                !refusal.body.to_string().contains("PRIVATE_MALFORMED_BODY"),
                "HTTP parser reflected rejected bytes"
            );
            ensure!(
                serde_json::to_vec(&refusal.body)?.len() <= 256,
                "HTTP rejection lost its bounded diagnostic"
            );
            if kind == "malformed" {
                // rmcp expect_json returns 415 for malformed JSON, with only
                // the parser category/location, not the rejected input.
                ensure!(
                    refusal.body
                        == serde_json::json!(
                            "fail to deserialize request body expected value at line 1 column 1"
                        )
                );
            }
        }
        session.close()?;
    }
    ensure!(fs::read(source.path().join("original.txt"))? == original);
    ensure!(fs::read_dir(source.path())?.count() == 1);
    Ok(())
}

fn assert_retired_stdio_roles(
    root: &Path,
    project: airk_core::service::ProjectBinding,
) -> Result<()> {
    for role in ["mcp", "bridge"] {
        let mut invocation = command();
        invocation.arg(role);
        if role == "bridge" {
            invocation
                .arg("--runtime-dir")
                .arg(root)
                .arg("--project")
                .arg(project.to_string());
        }
        let output = invocation.stdin(Stdio::null()).output()?;
        ensure!(
            !output.status.success(),
            "retired {role} accepted modern stdin"
        );
        ensure!(
            output.stdout.is_empty(),
            "retired {role} emitted protocol data"
        );
        let error = std::str::from_utf8(&output.stderr)?;
        ensure!(
            error.len() <= 4096 && error.contains("stdio_mcp_retired"),
            "retired role did not give bounded reconnect guidance: {error}"
        );
    }
    Ok(())
}

fn exercise_disappearing_stalled_and_malformed_clients(root: &Path) -> Result<()> {
    use airk_cli::local_ipc;
    use airk_core::service::{ControlMethod, ControlRequest, ControlResponse};
    use tokio::io::AsyncWriteExt;
    tokio::runtime::Runtime::new()?.block_on(async {
        let address = local_ipc::endpoint(root, "control")?;
        for _ in 0..3 {
            drop(local_ipc::connect(&address).await?);
        }
        let mut malformed = local_ipc::connect(&address).await?;
        malformed.write_all(&u32::MAX.to_be_bytes()).await?;
        drop(malformed);
        let mut incompatible = local_ipc::connect(&address).await?;
        local_ipc::write_message(
            &mut incompatible,
            &ControlRequest {
                protocol: u32::MAX,
                expected_instance: None,
                method: ControlMethod::Health,
            },
            Duration::from_secs(5),
        )
        .await?;
        let response: Option<ControlResponse> =
            local_ipc::read_message(&mut incompatible, Duration::from_secs(5)).await?;
        ensure!(
            matches!(response, Some(ControlResponse::Incompatible { .. })),
            "service accepted incompatible protocol"
        );
        let _stalled = local_ipc::connect(&address).await?;
        ensure!(
            control(root, "health")?["status"] == "health",
            "stalled client blocked health"
        );
        Ok(())
    })
}

fn management_survives_restart_and_busy_registry_does_not_block_health(
    root: &Path,
    owner: &Path,
) -> Result<airk_core::service::JobRecord> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, RequestKey,
    };
    let source = owner.join("source");
    let data = owner.join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("original.txt"), b"source bytes remain original")?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let caller = airk_kernel::service_broker::new_instance_id()?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(root, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        request(Method::Pause {
            project: project.binding,
            paused: true,
        })
        .await?;
        let second = owner.join("source-two");
        fs::create_dir(&second)?;
        let collision = airk_kernel::service_projects::registration(&second, &data, "fixture")?;
        ensure!(
            matches!(
                request(Method::Register { project: collision }).await?,
                Response::Failure { .. }
            ),
            "conflicting storage accepted"
        );
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("missing request epoch")
        };
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key: key.clone(),
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("job was not accepted")
        };
        let Response::Job { job: again } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key,
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("receipt retry failed")
        };
        ensure!(job.id == again.id, "duplicate request repeated work");
        // Lock only the metadata ledger; health must still answer without waiting for SQLite.
        let lock = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        lock.execute_batch("BEGIN EXCLUSIVE")?;
        let blocked = request(Method::Projects {
            after: None,
            revision: None,
            limit: 8,
        });
        let health = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let started = Instant::now();
            ensure!(
                matches!(request(Method::Health).await?, Response::Health { .. }),
                "health failed while registry locked"
            );
            ensure!(
                started.elapsed() < Duration::from_secs(1),
                "health waited behind registry"
            );
            Ok::<_, anyhow::Error>(())
        };
        let (blocked, health) = tokio::join!(blocked, health);
        health?;
        ensure!(
            matches!(blocked?, Response::Failure { .. }),
            "locked ledger did not return bounded failure"
        );
        lock.execute_batch("ROLLBACK")?;
        ensure!(
            fs::read_dir(&data)?.next().is_none(),
            "management opened collection state"
        );
        ensure!(
            fs::read(source.join("original.txt"))? == b"source bytes remain original",
            "source changed"
        );
        Ok(job)
    })
}

#[test]
fn racing_launchers_elect_one_service_and_restart_changes_instance_without_project_access()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let missing = owner.path().join("missing");
    assert!(control(&missing, "health").is_err());
    assert!(!missing.exists(), "health must not initialize a runtime");
    let mut children = Children(Vec::new());
    for _ in 0..3 {
        children.0.push(start(&root)?);
    }
    let first = wait_ready(&root)?;
    assert_eq!(first["status"], "health");
    assert_eq!(first["observation"]["policy"]["projects"], 0);
    assert_eq!(first["observation"]["version"], env!("CARGO_PKG_VERSION"));
    exercise_disappearing_stalled_and_malformed_clients(&root)?;
    let job =
        management_survives_restart_and_busy_registry_does_not_block_health(&root, owner.path())?;
    assert_eq!(control(&root, "shutdown")?["status"], "stopping");
    let mut successes = 0;
    for child in &mut children.0 {
        successes += usize::from(wait_exit(child)?);
    }
    assert_eq!(
        successes, 1,
        "only the elected service should serve and exit normally"
    );
    children.0.push(start(&root)?);
    let second = wait_ready(&root)?;
    assert_ne!(
        first["observation"]["instance"],
        second["observation"]["instance"]
    );
    tokio::runtime::Runtime::new()?.block_on(async {
        let response = airk_cli::service_runtime::request(&root,airk_core::service::ControlMethod::Job { id:job.id }).await?;
        ensure!(matches!(response,airk_core::service::ControlResponse::Job { job:found } if found.id==job.id),"restart lost accepted receipt");
        Ok::<_,anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(children.0.last_mut().context("service child missing")?)?,
        "restarted service failed"
    );
    assert!(root.join("instance-1.4.lock").is_file());
    assert!(
        !root.join("collections-1.2").exists() && !root.join("collections-1.4").exists(),
        "health must not open collection state"
    );
    Ok(())
}

#[test]
fn supervised_jobs_and_direct_queries_preserve_grants_and_stop_on_service_loss() -> Result<()> {
    use airk_cli::{local_ipc, project_protocol};
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let text = b"Native supervised project retrieval fixture: cobalt harbor.";
    fs::write(
        source.join("original.txt"),
        b"Qualified canonical setup seed.\n",
    )?;
    let service = qualified_process_fixture(&source, &data, "fixture")?;
    let selection = selected_scope(service.http_selection())?;
    let root = service.control.clone();
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let mut children = Children(vec![start_qualified(&service)?]);
    wait_ready(&root)?;
    // Preserve this protocol fixture's original explicit lexical policy through
    // native configuration, after canonical model/storage ownership is sealed.
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    fs::write(source.join("original.txt"), text)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&root, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("missing request epoch")
        };
        let caller = airk_kernel::service_broker::new_instance_id()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key,
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("index job not accepted")
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        let indexed_revision = loop {
            let Response::Job { job: observation } = request(Method::Job { id: job.id }).await?
            else {
                anyhow::bail!("job inspection failed")
            };
            if observation.state == JobState::Succeeded {
                let Some(airk_core::service::JobOutcome::Succeeded {
                    revision: Some(revision),
                }) = observation.outcome
                else {
                    anyhow::bail!("index completion lost its revision")
                };
                break revision;
            }
            ensure!(
                observation.state != JobState::Failed,
                "native index job failed: {:?}",
                observation.outcome
            );
            ensure!(Instant::now() < deadline, "native job did not finish");
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Response::Progress {
                observation: Some(observation),
                ..
            } = request(Method::Progress {
                project: project.binding,
            })
            .await?
                && observation.progress.job == job.id
                && observation.progress.phase == airk_core::service_progress::WorkPhase::Finished
            {
                observation.progress.validate()?;
                ensure!(
                    observation.progress.captured_files == 1
                        && observation.progress.extracted_files == 1
                        && observation.progress.committed_changes >= 1
                        && observation.progress.publications >= 1
                        && observation.progress.total_files == Some(1)
                        && observation.progress.publication == Some(indexed_revision),
                    "native progress did not describe actual durable indexing boundaries: {:?}",
                    observation.progress
                );
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "native progress was not retained"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let Response::Connection { role, grant } = request(Method::ConnectScoped {
            request: refreshed_scope(&root, project.binding, &selection).await?,
        })
        .await?
        else {
            anyhow::bail!("broker did not return grant")
        };
        let address = local_ipc::endpoint(&root, &role)?;
        let mut first = local_ipc::connect(&address).await?;
        local_ipc::write_message(&mut first, &grant, Duration::from_secs(5)).await?;
        let accepted: Option<project_protocol::Accepted> =
            local_ipc::read_message(&mut first, Duration::from_secs(5)).await?;
        ensure!(
            accepted.is_some_and(|accepted| accepted.identity == grant.identity),
            "direct grant failed"
        );
        let mut replay = local_ipc::connect(&address).await?;
        local_ipc::write_message(&mut replay, &grant, Duration::from_secs(5)).await?;
        ensure!(
            local_ipc::read_message::<_, project_protocol::Accepted>(
                &mut replay,
                Duration::from_secs(5)
            )
            .await
            .ok()
            .flatten()
            .is_none(),
            "replayed grant admitted"
        );
        drop(first);
        drop(replay);
        ensure!(
            airk_store::service::ProjectLease::acquire(&root, project.binding).is_err(),
            "worker lost its ownership lock"
        );
        let mut connection = connect_qualified(&root, project.binding, &selection).await?;
        let arguments =
            serde_json::json!({"query":"cobalt harbor","mode":"lexical","max_bytes":4096})
                .as_object()
                .context("invalid fixture query")?
                .clone();
        let result = connection
            .read_retry("airk_search".into(), arguments.clone())
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) != Some(true),
            "direct query returned error: {result}"
        );
        ensure!(
            result.to_string().contains("original.txt")
                && result.to_string().contains("cobalt harbor"),
            "direct response lost original citation"
        );
        assert_retired_stdio_roles(&root, project.binding)?;
        let pinned = airk_cli::bundled_assets::bind_service_models(
            airk_kernel::Kernel::open_service_collection_for_diagnostics(
                Path::new(&project.root),
                Some(data.clone()),
                "fixture",
            )?,
            &root,
        )?
        .refresh_service_snapshot()?;
        let pinned_revision = pinned
            .status()?
            .publication
            .context("old native snapshot missing")?
            .token;
        let Response::Job { job: rebuild } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            kind: JobKind::Rebuild,
        })
        .await?
        else {
            anyhow::bail!("replacement job was not accepted")
        };
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let Response::Job { job } = request(Method::Job { id: rebuild.id }).await? else {
                anyhow::bail!("replacement job disappeared")
            };
            if job.state == JobState::Succeeded {
                let Some(airk_core::service::JobOutcome::Succeeded {
                    revision: Some(revision),
                }) = job.outcome
                else {
                    anyhow::bail!("replacement lost revision receipt")
                };
                ensure!(
                    revision.incarnation != indexed_revision.incarnation,
                    "replacement reused its old incarnation"
                );
                break;
            }
            ensure!(
                job.state != JobState::Failed && Instant::now() < until,
                "replacement did not complete: {:?}",
                job.outcome
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let replaced = connection
            .read_retry("airk_search".into(), arguments.clone())
            .await?;
        ensure!(
            replaced.get("isError").and_then(Value::as_bool) != Some(true)
                && replaced.to_string().contains("original.txt"),
            "existing direct client did not select the replacement incarnation"
        );
        ensure!(
            pinned
                .status()?
                .publication
                .context("pinned publication missing")?
                .token
                == pinned_revision,
            "replacement changed a pinned old request"
        );
        let registry = airk_store::service::registry::ServiceRegistry::open(&root, caller)?;
        ensure!(
            registry
                .maintenance_observation(project.binding)?
                .is_some_and(|observation| observation.pending_incarnations == Some(1)),
            "retirement ignored live old reader"
        );
        drop(pinned);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if registry
                .maintenance_observation(project.binding)?
                .is_some_and(|observation| {
                    observation.pending_incarnations == Some(0) && !observation.failed
                })
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "automatic owned retirement did not finish after old reader released"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // Retrieval-only connections expose neither management nor mutation tools.
        let result = connection
            .call("service_shutdown".into(), Default::default())
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) == Some(true),
            "worker exposed management"
        );
        ensure!(
            matches!(request(Method::Health).await?, Response::Health { .. }),
            "unauthorized tool stopped service"
        );
        children.0[0].kill()?;
        children.0[0].wait()?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        ensure!(
            connection
                .call("airk_search".into(), arguments)
                .await
                .is_err(),
            "worker continued admission after service loss"
        );
        drop(connection);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(lease) = airk_store::service::ProjectLease::acquire(&root, project.binding) {
                drop(lease);
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "orphan worker did not drain and release its lock"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        ensure!(
            fs::read(source.join("original.txt"))? == text,
            "source changed"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    finish_qualified_process_fixture(&service)?;
    Ok(())
}

#[test]
fn foreign_registry_state_is_preserved_and_does_not_disable_native_health() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    airk_cli::local_ipc::prepare_root(&root)?;
    let path = root.join("service-1.4.sqlite3");
    let original = b"foreign metadata fixture must not be initialized or deleted";
    fs::write(&path, original)?;
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        use airk_core::service::{ControlMethod,ControlResponse};
        ensure!(matches!(airk_cli::service_runtime::request(&root,ControlMethod::RequestEpoch).await?,ControlResponse::Failure{code} if code=="registry_unavailable"),"foreign registry was admitted");
        ensure!(matches!(airk_cli::service_runtime::request(&root,ControlMethod::Logs{limit:32}).await?,ControlResponse::Logs{..}),
            "registry failure blocked independent process diagnostics");
        Ok::<_,anyhow::Error>(())
    })?;
    ensure!(
        control(&root, "health")?["status"] == "health",
        "registry failure blocked health"
    );
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(&mut children.0[0])?,
        "failed registry prevented service shutdown"
    );
    ensure!(fs::read(path)? == original, "foreign registry changed");
    Ok(())
}

#[test]
fn restarted_service_defers_allocation_until_predecessor_ownership_releases() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    use airk_store::service::{EmbeddingLease, ProjectLease, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let mut registry =
        ServiceRegistry::open(&root, airk_kernel::service_broker::new_instance_id()?)?;
    registry.register(&project)?;
    let mut predecessor = ProjectLease::acquire(&root, project.binding)?;
    registry.admit_worker(&mut predecessor)?;
    let embedding_predecessor = EmbeddingLease::acquire(&root)?;
    drop(registry);
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Failure{code} if code=="previous_workers_draining"),"replacement ignored predecessor ownership");
        ensure!(matches!(airk_cli::service_runtime::request(&root,Method::Health).await?,Response::Health{..}),"predecessor drain blocked health");
        drop(predecessor);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Failure{code} if code=="previous_workers_draining"),"replacement ignored surviving inference ownership");
        drop(embedding_predecessor);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Connection{grant,..} if grant.identity.fencing_generation==2),"new worker was not admitted after predecessor exit");
        Ok::<_,anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "replacement did not drain");
    Ok(())
}

#[test]
fn late_child_with_buffered_initialization_cannot_open_collection_without_activation() -> Result<()>
{
    use airk_cli::{project_protocol, service_ipc};
    use airk_core::service::{JobKind, JobState, RequestKey};
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("original.txt"), b"late child fixture")?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let instance = airk_kernel::service_broker::new_instance_id()?;
    let mut registry = ServiceRegistry::open(&root, instance)?;
    registry.register(&project)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let job = registry.enqueue(
        project.binding,
        instance,
        &RequestKey {
            namespace,
            epoch,
            nonce: instance,
        },
        JobKind::Index,
    )?;
    let mut child = command()
        .arg("project-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("private input missing")?;
    let mut children = Children(vec![child]);
    service_ipc::write_frame(
        &mut input,
        &project_protocol::Init {
            engine: airk_core::identity::Sha256Digest::from_bytes([0; 32]),
            runtime: root.clone(),
            project: project.binding,
            service: instance,
            worker: airk_kernel::service_broker::new_instance_id()?,
        },
    )?;
    drop(input);
    ensure!(
        !wait_exit(&mut children.0[0])?,
        "unactivated child completed normally"
    );
    ensure!(
        fs::read_dir(data)?.next().is_none(),
        "unactivated child opened collection storage"
    );
    ensure!(
        registry.job(job.id)?.state == JobState::Pending,
        "unactivated child claimed a job"
    );
    ensure!(
        fs::read(source.join("original.txt"))? == b"late child fixture",
        "source changed"
    );
    Ok(())
}

#[test]
fn private_worker_channel_renews_only_the_live_scoped_generation() -> Result<()> {
    use airk_cli::{
        project_protocol::{self, Command as Control, Reply},
        service_ipc,
    };
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    for n in 0..8 {
        fs::write(
            source.join(format!("fixture-{n}.txt")),
            "cobalt harbor source record\n".repeat(12000),
        )?;
    }
    airk_cli::local_ipc::prepare_root(&root)?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    ServiceRegistry::open(&root, service)?.register(&project)?;
    let mut child = command()
        .arg("project-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("worker input missing")?;
    let mut output = child.stdout.take().context("worker output missing")?;
    let mut children = Children(vec![child]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        while let Ok(Some(reply)) = service_ipc::read_frame::<_, Reply>(&mut output) {
            if sender.try_send(reply).is_err() {
                break;
            }
        }
    });
    let result = (|| -> Result<()> {
        service_ipc::write_frame(
            &mut input,
            &project_protocol::Init {
                engine: airk_core::identity::Sha256Digest::from_bytes([0; 32]),
                runtime: root.clone(),
                project: project.binding,
                service,
                worker: airk_kernel::service_broker::new_instance_id()?,
            },
        )?;
        let Reply::Ready { identity, .. } = receiver.recv_timeout(Duration::from_secs(10))? else {
            anyhow::bail!("missing worker identity")
        };
        service_ipc::write_frame(
            &mut input,
            &Control::Activate {
                identity,
                allowance: airk_core::service::QueryAllowance {
                    concurrent_queries: 4,
                    response_bytes: 4 * airk_core::service::MAX_FRAME_BYTES,
                },
                pool: None,
                pool_pending: false,
            },
        )?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "worker activation failed"
        );
        service_ipc::write_frame(&mut input, &Control::Renew { identity })?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::Renewed
            ),
            "live worker did not renew"
        );
        let forged = airk_core::service::WorkerIdentity {
            fencing_generation: identity.fencing_generation + 1,
            ..identity
        };
        service_ipc::write_frame(&mut input, &Control::Renew { identity: forged })?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::Failed
            ),
            "worker accepted a foreign generation renewal"
        );
        use airk_core::{
            service::{JobKind, JobState, RequestKey},
            service_resources::WorkAllowance,
        };
        let mut registry = ServiceRegistry::open(&root, service)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let job = registry.enqueue(
            project.binding,
            service,
            &RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            JobKind::Index,
        )?;
        std::thread::sleep(Duration::from_millis(400));
        ensure!(
            registry.job(job.id)?.state == JobState::Pending,
            "worker claimed an ungranted job"
        );
        let allowance = WorkAllowance {
            id: airk_kernel::service_broker::new_instance_id()?,
            identity,
            job: job.id,
            extraction_threads: 1,
            inference_threads: 1,
            working_memory_bytes: 256 << 20,
            background_io_bytes_per_second: 1 << 20,
            ttl_ms: 300_000,
        };
        let grant = allowance.id;
        service_ipc::write_frame(
            &mut input,
            &Control::RunJob {
                identity,
                allowance,
            },
        )?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::WorkAccepted
            ),
            "worker rejected exact work grant"
        );
        service_ipc::write_frame(
            &mut input,
            &Control::WorkStatus {
                freshness: None,
                identity,
                keep_for_maintenance: false,
                control: Some(project_protocol::WorkControl {
                    grant,
                    paused: true,
                    canceled: false,
                    yield_to_source: false,
                }),
            },
        )?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::WorkStatus { .. }
            ),
            "worker lost paused work status"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while registry.job(job.id)?.state == JobState::Pending {
            ensure!(
                Instant::now() < deadline,
                "worker did not claim granted job"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        registry.request_cancellation(job.id)?;
        service_ipc::write_frame(
            &mut input,
            &Control::WorkStatus {
                freshness: None,
                identity,
                keep_for_maintenance: false,
                control: Some(project_protocol::WorkControl {
                    grant,
                    paused: false,
                    canceled: true,
                    yield_to_source: false,
                }),
            },
        )?;
        receiver.recv_timeout(Duration::from_secs(5))?;
        loop {
            let current = registry.job(job.id)?;
            if current.state == JobState::Canceled {
                break;
            }
            ensure!(
                current.state != JobState::Failed && Instant::now() < deadline,
                "running cancellation did not settle at a safe boundary: {:?}",
                current.state
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        ensure!(
            !fs::read_dir(&data)?
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("execution-1.2-")),
            "service used legacy root execution slots"
        );
        Ok(())
    })();
    drop(input);
    if result.is_err() {
        let _ = children.0[0].kill();
    }
    let exited = wait_exit(&mut children.0[0])?;
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("private worker reader failed"))?;
    result?;
    ensure!(exited, "worker did not drain after control loss");
    Ok(())
}

#[test]
fn resource_configuration_failure_is_distinct_from_memory_and_cached_status_stays_available()
-> Result<()> {
    use airk_core::{
        service::{ControlMethod, ControlResponse},
        service_resources::{ResourceConfiguration, ResourcePreset},
    };
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    airk_cli::local_ipc::prepare_root(&root)?;
    fs::write(root.join("resources-1.4.json"), b"{invalid}")?;
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let ControlResponse::Resources {
            observation,
            configuration_error,
            ..
        } = airk_cli::service_runtime::request(&root, ControlMethod::Resources).await?
        else {
            anyhow::bail!("resource cache unavailable")
        };
        ensure!(configuration_error, "invalid configuration was hidden");
        ensure!(
            observation.observed_rss_bytes != Some(u64::MAX),
            "configuration error impersonated memory pressure"
        );
        let configuration = ResourceConfiguration {
            preset: ResourcePreset::Responsive,
            memory_bytes: Some(2 << 30),
            ..Default::default()
        };
        let ControlResponse::Resources {
            configuration_error,
            observation,
            ..
        } = airk_cli::service_runtime::request(
            &root,
            ControlMethod::ConfigureResources { configuration },
        )
        .await?
        else {
            anyhow::bail!("valid explicit configuration rejected")
        };
        ensure!(
            !configuration_error && observation.memory_limit_bytes == 2 << 30,
            "configuration did not replace invalid saved state"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not stop");
    Ok(())
}

#[cfg(windows)]
#[test]
fn denied_registered_source_keeps_direct_queries_and_fails_new_work_without_data_loss() -> Result<()>
{
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    };
    use std::{
        os::windows::{ffi::OsStringExt, process::CommandExt},
        path::PathBuf,
    };
    let mut buffer = vec![0u16; 32768];
    // SAFETY: writable UTF-16 output buffer with its exact capacity.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    ensure!(
        length > 0 && length < buffer.len(),
        "system tool directory unavailable"
    );
    let tool = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length])).join("icacls.exe");
    struct Restore {
        tool: PathBuf,
        source: PathBuf,
        sid: String,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = Command::new(&self.tool)
                .arg(&self.source)
                .args(["/remove:d", &self.sid])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let runtime = owner.path().join("runtime");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let original = "Original copper permission fixture.\n";
    fs::write(source.join("memo.txt"), original)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "denied-source")?;
    bind_fixture_data(&runtime, &data)?;
    let kernel =
        airk_kernel::Kernel::open_service_collection(&source, Some(data), "denied-source")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel
        .status()?
        .publication
        .context("initial publication missing")?
        .token;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        let sid = airk_cli::local_ipc::current_user()?;
        let restore = Restore {
            tool,
            source: source.clone(),
            sid: format!("*{sid}"),
        };
        let output = Command::new(&restore.tool)
            .arg(&source)
            .args(["/deny", &format!("{}:(RD)", restore.sid)])
            .creation_flags(0x08000000)
            .output()?;
        ensure!(output.status.success(), "fixture directory denial failed");
        ensure!(
            fs::read_dir(&source).is_err(),
            "fixture directory listing remains readable"
        );
        let mut connection =
            airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        let result = connection
            .read_retry(
                "airk_search".into(),
                serde_json::json!({"query":"copper", "mode":"lexical"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) != Some(true)
                && result.to_string().contains("memo.txt"),
            "denied source blocked retained query: {result}"
        );
        let status = kernel.status()?;
        ensure!(
            status.publication.context("publication lost")?.token == token,
            "denied source changed publication"
        );
        ensure!(
            status
                .source_availability
                .context("availability missing")?
                .reason
                == Some(airk_kernel::service_projects::SourceProblem::PermissionDenied),
            "permission reason missing"
        );
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("request epoch missing")
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("index request not accepted")
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let Response::Job { job } = request(Method::Job { id: job.id }).await? else {
                anyhow::bail!("job receipt missing")
            };
            if let Some(outcome) = job.outcome {
                ensure!(
                    matches!(outcome, JobOutcome::Failed { code } if code == "source_unavailable"),
                    "permission failure lost its typed outcome"
                );
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "denied source job did not finish"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        drop(connection);
        request(Method::Shutdown).await?;
        ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
        drop(restore);
        ensure!(
            fs::read_to_string(source.join("memo.txt"))? == original,
            "source bytes changed"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn initialized_completed_reviewed_activation_replay_preserves_stopped_service_and_all_state()
-> Result<()> {
    use airk_cli::runtime_install::{InstallManifest, VerificationBasis};
    use airk_core::service::{ControlMethod, ControlResponse};
    use std::io::Write;
    let mut owner = tempfile::tempdir()?;
    owner.disable_cleanup(true);
    let runtime_owner = owner.path().canonicalize()?.join("runtime-owner");
    airk_cli::local_ipc::prepare_root(&runtime_owner)?;
    let (install, root, _) = common::installed_runtime_fixture(&runtime_owner)?;
    let (prior, _, _) = airk_cli::runtime_install::selected(&install)?;
    let retained_payload = install.join("runtimes").join(&prior.active);
    let candidate_payload = runtime_owner.join("reviewed-update-payload");
    airk_cli::local_ipc::prepare_root(&candidate_payload)?;
    let mut inventory: InstallManifest =
        serde_json::from_slice(&fs::read(retained_payload.join("airk-install.json"))?)?;
    // Both candidates execute the exact current Cargo image. A distinct, honest
    // fixture provenance notice makes this a real separately approved update.
    for name in inventory.files.keys() {
        let relative = Path::new(name);
        ensure!(
            !relative.is_absolute()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "fixture inventory escaped its payload"
        );
        let destination = candidate_payload.join(relative);
        fs::create_dir_all(
            destination
                .parent()
                .context("fixture payload parent missing")?,
        )?;
        fs::copy(retained_payload.join(relative), destination)?;
    }
    let provenance = "notices/reviewed-replay-fixture.json";
    ensure!(
        !inventory.files.contains_key(provenance),
        "replay fixture notice already exists"
    );
    fs::write(
        candidate_payload.join(provenance),
        serde_json::to_vec(&serde_json::json!({"schema":1,
            "purpose":"completed reviewed update, exact current Cargo image",
            "approval":"explicit local organization digest","release_qualified":false}))?,
    )?;
    inventory.files.insert(
        provenance.into(),
        airk_cli::runtime_pair::digest_file(&candidate_payload.join(provenance))?,
    );
    fs::write(
        candidate_payload.join("airk-install.json"),
        serde_json::to_vec(&inventory)?,
    )?;
    let candidate =
        airk_cli::runtime_pair::digest_file(&candidate_payload.join("airk-install.json"))?;
    ensure!(
        candidate != prior.active,
        "fixture update did not change its approved inventory"
    );
    airk_cli::runtime_install::stage(
        &candidate_payload,
        &install,
        &candidate,
        VerificationBasis::OrganizationDigest,
    )?;
    let executor = tokio::runtime::Runtime::new()?;
    let selected = executor.block_on(airk_cli::runtime_install::activate_reviewed(
        &install,
        &candidate,
        &prior.active,
    ))?;
    ensure!(selected.active == candidate && selected.previous.as_ref() == Some(&prior.active));
    let (health, namespace, epoch) = executor.block_on(async {
        let observation = match airk_cli::service_runtime::request(&root, ControlMethod::Health)
            .await
            .context("after reviewed activation: health transport failed")?
        {
            ControlResponse::Health { observation } => observation,
            unexpected => anyhow::bail!(
                "after reviewed activation: unexpected health reply: {}",
                serde_json::to_string(&unexpected)?
            ),
        };
        ensure!(
            observation.runtime_sha256.as_deref() == Some(candidate.as_str()),
            "after reviewed activation: health identifies another runtime: {}",
            serde_json::to_string(&observation)?
        );
        let readiness_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        observe_metadata_projects(
            readiness_deadline,
            "after reviewed activation admission release",
            || {
                airk_cli::service_runtime::request_for_version_and_instance(
                    &root,
                    ControlMethod::Projects {
                        after: None,
                        revision: None,
                        limit: 1,
                    },
                    env!("CARGO_PKG_VERSION"),
                    observation.instance,
                )
            },
        )
        .await?;
        let confirmed = match airk_cli::service_runtime::request_for_version_and_instance(
            &root,
            ControlMethod::Health,
            env!("CARGO_PKG_VERSION"),
            observation.instance,
        )
        .await
        .context("after Projects readiness: health transport failed")?
        {
            ControlResponse::Health { observation } => observation,
            unexpected => anyhow::bail!(
                "after Projects readiness: unexpected health reply: {}",
                serde_json::to_string(&unexpected)?
            ),
        };
        ensure!(
            confirmed.instance == observation.instance
                && confirmed.runtime_sha256.as_deref() == Some(candidate.as_str())
                && confirmed.executable_sha256 == observation.executable_sha256,
            "after Projects readiness: health identity changed: {}",
            serde_json::to_string(&confirmed)?
        );
        // Use the canonical epoch transport after observing the exact metadata
        // owner ready. Add no retry around an unknown outcome or other refusal.
        let (namespace, epoch) = require_request_epoch(
            airk_cli::service_runtime::request_for_version_and_instance(
                &root,
                ControlMethod::RequestEpoch,
                env!("CARGO_PKG_VERSION"),
                observation.instance,
            )
            .await
            .context("after Projects readiness: RequestEpoch transport failed")?,
            "after reviewed candidate Projects readiness",
        )?;
        Ok::<_, anyhow::Error>((observation, namespace, epoch))
    })?;
    let stop_request = serde_json::json!({
        "caller":airk_kernel::service_broker::new_instance_id()?,
        "key":{"namespace":namespace,"epoch":epoch,"nonce":airk_kernel::service_broker::new_instance_id()?},
        "action":"stop","expected_instance":health.instance,"runtime":candidate,"force":false,
    });
    let mut stop = command()
        .args(["runtime", "lifecycle", "--install-dir"])
        .arg(&install)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    stop.stdin
        .take()
        .context("fixture lifecycle stdin missing")?
        .write_all(&serde_json::to_vec(&stop_request)?)?;
    let stopped = stop.wait_with_output()?;
    ensure!(
        stopped.status.success(),
        "reviewed fixture Stop failed: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let stop_receipt: Value = serde_json::from_slice(&stopped.stdout)?;
    ensure!(
        stop_receipt["status"] == "lifecycle"
            && stop_receipt["record"]["phase"] == "complete"
            && stop_receipt["record"]["request"]["key"] == stop_request["key"]
            && stop_receipt["record"]["request"]["runtime"] == stop_request["runtime"]
            && stop_receipt["record"]["request"]["expected_instance"]
                == stop_request["expected_instance"],
        "Stop did not finish for the exact reviewed runtime and instance: {stop_receipt}"
    );
    let until = Instant::now() + Duration::from_secs(30);
    let excluded = loop {
        if let Ok(excluded) = airk_store::service::ServiceLease::acquire(&root) {
            break excluded;
        }
        ensure!(
            Instant::now() < until,
            "reviewed candidate retained native ownership after shutdown"
        );
        std::thread::yield_now();
    };
    ensure!(
        airk_store::service::registry::ServiceRegistry::open_saved(&root)?
            .service_explicitly_stopped()?,
        "fixture Stop did not persist the explicit user stop"
    );
    let sentinel = b"Completed replay must not interpret or replace this invalid startup marker.";
    fs::write(install.join("bootstrap.json"), sentinel)?;
    let before = storage_fingerprints(&runtime_owner)?;
    let replayed = executor.block_on(airk_cli::runtime_install::activate_reviewed(
        &install,
        &candidate,
        &prior.active,
    ))?;
    ensure!(serde_json::to_value(&replayed)? == serde_json::to_value(&selected)?);
    ensure!(
        storage_fingerprints(&runtime_owner)? == before,
        "completed replay changed payload, state, namespace, selection, receipt, logs or startup marker"
    );
    ensure!(fs::read(install.join("bootstrap.json"))? == sentinel);
    ensure!(
        airk_store::service::ServiceLease::acquire(&root).is_err(),
        "replay released the test's stopped-service exclusion"
    );
    drop(excluded);
    owner.disable_cleanup(false);
    Ok(())
}

#[test]
fn installed_log_binding_places_the_ring_outside_control_and_missing_ownership_never_falls_back()
-> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{ControlMethod as Method, ControlResponse as Response},
        service_logs::LogKind,
    };
    let mut owner = tempfile::tempdir()?;
    owner.disable_cleanup(true);
    let base = owner.path().canonicalize()?;
    let runtime_owner = base.join("runtime-owner");
    airk_cli::local_ipc::prepare_root(&runtime_owner)?;
    let (install, root, executable) = common::installed_runtime_fixture(&runtime_owner)?;
    let layout: Value = serde_json::from_slice(&fs::read(install.join("runtime-layout.json"))?)?;
    let state =
        std::path::PathBuf::from(layout["state"].as_str().context("fixture state missing")?);
    let logs = std::path::PathBuf::from(layout["logs"].as_str().context("fixture logs missing")?);
    let start_installed = || start_command(&root, Command::new(&executable));
    let mut children = Children(vec![start_installed()?]);
    wait_ready(&root)?;
    let executor = tokio::runtime::Runtime::new()?;
    let source = base.join("source");
    let data = base.join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let project = common::setup_fixture_project(&root, &source, &data, "logs")?;
    executor.block_on(async {
        let until = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let Response::Health { observation } =
                airk_cli::service_runtime::request(&root, Method::Health).await?
            else {
                anyhow::bail!("health unavailable")
            };
            if observation.logs.available {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "native log binding unavailable"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        ensure!(
            matches!(
                airk_cli::service_runtime::request(&root, Method::Register { project }).await?,
                Response::Registered { .. }
            ),
            "safe registration failed"
        );
        let Response::RequestEpoch { namespace, epoch } =
            airk_cli::service_runtime::request(&root, Method::RequestEpoch).await?
        else {
            anyhow::bail!("fixture request epoch missing")
        };
        let revision = airk_store::service::registry::ServiceRegistry::open_saved(&root)?
            .catalog_state()?.context("fixture catalog missing")?.revision;
        let mut draft = serde_json::json!({"catalog_index_setup_draft":{"selection":{
            "key":{"namespace":namespace,"epoch":epoch,"nonce":airk_kernel::service_broker::new_instance_id()?},
            "expected_catalog_revision":revision,"name":"Rejected log source",
            "primary_root":source,"data_root":base.join("private-data"),"collection":"private",
            "additional_roots":[],"setup_watch":false}}});
        let allowed = airk_cli::service_runtime::request(&root, serde_json::from_value(draft.clone())?).await?;
        ensure!(matches!(&allowed, Response::CatalogIndexSetupDraft { .. }), "matched safe source draft was refused: {}", serde_json::to_string(&allowed)?);
        // Change only the source root. Drafts are read-only, so both observations
        // retain the same request identity and catalog revision.
        draft["catalog_index_setup_draft"]["selection"]["primary_root"] = serde_json::json!(logs);
        let private = serde_json::from_value(draft)?;
        let refused = airk_cli::service_runtime::request(&root, private).await?;
        ensure!(
            matches!(&refused, Response::Failure { code } if code == "invalid_registry_request"),
            "native log location was admitted as source or refused for an unrelated reason: {}", serde_json::to_string(&refused)?
        );
        airk_cli::service_runtime::request(&root, Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not stop");
    let page = airk_cli::service_logs::recent(&root, 32)?;
    ensure!(
        page.records
            .iter()
            .any(|r| r.kind == LogKind::ServiceStopped),
        "down-service export lost final lifecycle record"
    );
    ensure!(
        logs.join("logs-1.4/current.jsonl").is_file() && !root.join("logs-1.4").exists(),
        "ring used the control directory instead of the pinned native location"
    );
    let original = fs::read(logs.join("logs-1.4/current.jsonl"))?;
    fs::remove_file(state.join("airk-state-1.4.owner.json"))?;
    ensure!(
        airk_cli::service_logs::recent(&root, 32).is_err(),
        "missing binding redirected cached export"
    );
    children.0.push(start_installed()?);
    wait_ready(&root)?;
    executor.block_on(async {
        let until = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let Response::Health { observation } =
                airk_cli::service_runtime::request(&root, Method::Health).await?
            else {
                anyhow::bail!("health unavailable after binding loss")
            };
            if observation.logs.write_failures > 0 {
                ensure!(
                    !observation.logs.available
                        && observation.instance != Incarnation::from_bytes([0; 16]),
                    "logging failure disabled health or was hidden"
                );
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "missing ownership was not observed"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        airk_cli::service_runtime::request(&root, Method::Shutdown).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        wait_exit(children.0.last_mut().unwrap())?,
        "service failed to stop after log binding loss"
    );
    ensure!(
        fs::read(logs.join("logs-1.4/current.jsonl"))? == original
            && !root.join("logs-1.4").exists(),
        "unknown ownership created another ring or changed retained logs"
    );
    owner.disable_cleanup(false);
    Ok(())
}

#[test]
fn installed_control_loss_rejects_native_startup_and_restoration_retains_registry_namespace()
-> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    let mut owner = tempfile::tempdir()?;
    owner.disable_cleanup(true);
    let base = owner.path().canonicalize()?;
    let runtime_owner = base.join("runtime-owner");
    airk_cli::local_ipc::prepare_root(&runtime_owner)?;
    let (install, root, executable) = common::installed_runtime_fixture(&runtime_owner)?;
    let layout: Value = serde_json::from_slice(&fs::read(install.join("runtime-layout.json"))?)?;
    let state =
        std::path::PathBuf::from(layout["state"].as_str().context("fixture state missing")?);
    let start_installed = || start_command(&root, Command::new(&executable));
    let mut children = Children(vec![start_installed()?]);
    wait_ready(&root)?;
    let executor = tokio::runtime::Runtime::new()?;
    let namespace = executor.block_on(async {
        let Response::RequestEpoch { namespace, .. } =
            airk_cli::service_runtime::request(&root, Method::RequestEpoch).await?
        else {
            anyhow::bail!("request namespace missing")
        };
        Ok::<_, anyhow::Error>(namespace)
    })?;
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "first service did not stop");
    let marker = fs::read(state.join("airk-control-1.4.owner.json"))?;
    let namespace_marker = fs::read(state.join("airk-registry-1.4.owner.json"))?;
    let namespace_binding: Value = serde_json::from_slice(&namespace_marker)?;
    ensure!(
        namespace_binding["ready"] == true
            && namespace_binding["namespace"] == namespace.to_string(),
        "service accepted metadata before its independent namespace binding"
    );
    let database = fs::read(root.join("service-1.4.sqlite3"))?;
    let saved = state.join("original-control");
    fs::rename(&root, &saved)?;
    ensure!(
        !airk_cli::service_logs::recent(&root, 32)?
            .records
            .is_empty(),
        "owned external logs unavailable after control loss"
    );
    ensure!(!root.exists(), "cached log export recreated control state");
    children.0.push(start_installed()?);
    ensure!(
        !wait_exit(children.0.last_mut().unwrap())?,
        "missing initialized control was silently recreated"
    );
    ensure!(
        !root.exists(),
        "failed startup created another control directory"
    );
    ensure!(
        fs::read(state.join("airk-control-1.4.owner.json"))? == marker,
        "initialization receipt changed"
    );
    ensure!(
        fs::read(saved.join("service-1.4.sqlite3"))? == database,
        "original registry changed"
    );
    fs::rename(&saved, &root)?;
    children.0.push(start_installed()?);
    wait_ready(&root)?;
    executor.block_on(async {
        let Response::RequestEpoch {
            namespace: actual, ..
        } = airk_cli::service_runtime::request(&root, Method::RequestEpoch).await?
        else {
            anyhow::bail!("restored namespace missing")
        };
        ensure!(
            actual == namespace,
            "restoration reset the durable namespace"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(children.0.last_mut().unwrap())?,
        "restored service did not stop"
    );
    let names = ["service-1.4.sqlite3", "service-1.4.owner.json"];
    let originals = names
        .iter()
        .map(|name| fs::read(root.join(name)))
        .collect::<std::io::Result<Vec<_>>>()?;
    for name in names {
        fs::remove_file(root.join(name))?;
    }
    children.0.push(start_installed()?);
    ensure!(
        !wait_exit(children.0.last_mut().unwrap())?,
        "lost registry and owner silently created another authority"
    );
    ensure!(
        names.iter().all(|name| !root.join(name).exists()),
        "failed admission recreated missing metadata"
    );
    ensure!(
        fs::read(state.join("airk-registry-1.4.owner.json"))? == namespace_marker,
        "independent namespace proof changed after loss"
    );
    for (name, bytes) in names.into_iter().zip(originals) {
        fs::write(root.join(name), bytes)?;
    }
    children.0.push(start_installed()?);
    wait_ready(&root)?;
    executor.block_on(async {
        let Response::RequestEpoch {
            namespace: actual, ..
        } = airk_cli::service_runtime::request(&root, Method::RequestEpoch).await?
        else {
            anyhow::bail!("twice-restored namespace missing")
        };
        ensure!(
            actual == namespace,
            "whole-ledger restoration changed authority"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(children.0.last_mut().unwrap())?,
        "twice-restored service did not stop"
    );
    owner.disable_cleanup(false);
    Ok(())
}

#[test]
fn separate_services_cannot_register_the_same_owned_data_root_or_change_its_evidence() -> Result<()>
{
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    let owner = tempfile::tempdir()?;
    let first = owner.path().join("first-runtime");
    let second = owner.path().join("second-runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(
        source.join("memo.md"),
        "# Copper beacon\nOriginal scope fixture.\n",
    )?;
    bind_fixture_data(&first, &data)?;
    let kernel =
        airk_kernel::Kernel::open_service_collection(&source, Some(data.clone()), "scoped")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel
        .status()?
        .publication
        .context("initial evidence missing")?
        .token;
    let receipt = fs::read(data.join("service-data-1.4.owner.json"))?;
    let project = airk_kernel::service_projects::registration(&source, &data, "scoped")?;
    let mut children = Children(vec![start(&first)?, start(&second)?]);
    wait_ready(&first)?;
    wait_ready(&second)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        ensure!(
            matches!(
                airk_cli::service_runtime::request(
                    &first,
                    Method::Register {
                        project: project.clone()
                    }
                )
                .await?,
                Response::Registered { .. }
            ),
            "original service rejected its owned data"
        );
        let response =
            airk_cli::service_runtime::request(&second, Method::Register { project }).await?;
        ensure!(
            matches!(response,Response::Failure {code} if code=="data_root_scope_conflict"),
            "foreign service did not report storage ownership conflict"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        kernel
            .status()?
            .publication
            .context("retained evidence lost")?
            .token
            == token,
        "foreign registration changed evidence"
    );
    ensure!(
        fs::read(data.join("service-data-1.4.owner.json"))? == receipt,
        "foreign registration changed ownership"
    );
    ensure!(
        fs::read_to_string(source.join("memo.md"))? == "# Copper beacon\nOriginal scope fixture.\n",
        "source changed"
    );
    for runtime in [&first, &second] {
        control(runtime, "shutdown")?;
    }
    for child in &mut children.0 {
        ensure!(wait_exit(child)?, "private fixture service did not stop");
    }
    Ok(())
}

#[test]
fn identity_bound_project_restart_preserves_other_workers_and_unavailable_source_evidence()
-> Result<()> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let mut projects = Vec::new();
    for n in 0..2 {
        let source = owner.path().join(format!("source-{n}"));
        let data = owner.path().join(format!("data-{n}"));
        fs::create_dir(&source)?;
        fs::create_dir(&data)?;
        fs::write(
            source.join("memo.txt"),
            b"Copper beacon original restart fixture.\n",
        )?;
        projects.push(airk_kernel::service_projects::registration(
            &source,
            &data,
            &format!("restart-{n}"),
        )?);
    }
    let service = common::Service::reviewed(
        Path::new(&projects[0].root),
        Path::new(&projects[0].data_root),
        &projects[0].collection,
    );
    let second = service.additional_project(
        Path::new(&projects[1].root),
        Path::new(&projects[1].data_root),
        &projects[1].collection,
    )?;
    ensure!(
        second == projects[1],
        "second canonical setup changed project identity"
    );
    let selections = [
        selected_scope(service.http_selection())?,
        selected_scope(common::reviewed_http_selection(&service.control, &second))?,
    ];
    service.settle_reviewed_index(
        Path::new(&projects[0].root),
        Path::new(&projects[0].data_root),
        Instant::now() + Duration::from_secs(90),
    )?;
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        let response = service.request_until(
            Method::Inspection {
                project: second.binding,
            },
            until,
        )?;
        if matches!(response,Response::Inspection { observation:Some(observation),.. }
            if observation.snapshot.index_state==airk_core::service_inspection::IndexState::Ready
                && observation.snapshot.semantic.as_ref().is_none_or(|semantic| matches!(semantic.state,airk_core::readiness::SemanticState::Ready | airk_core::readiness::SemanticState::Disabled)))
        {
            break;
        }
        ensure!(
            Instant::now() + Duration::from_millis(25) < until,
            "second canonical project did not become ready"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    pause_cold_projects(&service, &projects, until)?;
    stop_qualified_bootstrap(&service)?;
    let runtime = service.control.clone();
    let mut children = Children(vec![start_qualified(&service)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&runtime,method);
        // This scenario requires two simultaneous resident workers. A small
        // host's default Balanced policy intentionally admits only one.
        let configured = request(Method::ConfigureResources {
            configuration: airk_core::service_resources::ResourceConfiguration {
                memory_bytes: Some(4 << 30),
                extraction_threads: Some(1),
                ..Default::default()
            },
        }).await?;
        ensure!(matches!(&configured, Response::Resources { observation, configuration_error: false, .. }
            if observation.resident_limit >= 2), "two-worker fixture admission unavailable: {}", serde_json::to_string(&configured)?);
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await? else { anyhow::bail!("epoch missing") };
        for project in &projects {
            request(Method::Register { project: project.clone() }).await?;
            let before=storage_fingerprints(Path::new(&project.data_root))?;
            ensure!(matches!(request(Method::Worker { project: project.binding }).await?, Response::Worker { identity: None, restarting: false, .. }), "observation started a closed project");
            ensure!(storage_fingerprints(Path::new(&project.data_root))?==before, "worker observation changed qualified project data");
        }
        for project in &projects {
            ensure!(matches!(request(Method::Pause { project:project.binding,paused:false }).await?,Response::Registered { project:saved } if saved.registration==*project && !saved.paused),"cold fixture resume refused/crossed project");
            let caller = airk_kernel::service_broker::new_instance_id()?;
            let Response::Job { job } = request(Method::Enqueue { project: project.binding, caller,
                key: RequestKey { namespace, epoch, nonce: caller }, kind: JobKind::Index }).await? else { anyhow::bail!("index not accepted") };
            let deadline = Instant::now()+Duration::from_secs(30);
            loop {
                let Response::Job { job } = request(Method::Job { id: job.id }).await? else { anyhow::bail!("job lost") };
                if job.state == JobState::Succeeded { break; }
                ensure!(job.state != JobState::Failed && Instant::now()<deadline,"source job failed or stalled: {:?}", job.outcome);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let mut identities = Vec::new();
        for (project,selection) in projects.iter().zip(&selections) {
            let response = request(Method::ConnectScoped { request:refreshed_scope(&runtime,project.binding,selection).await? }).await?;
            let Response::Connection { grant, .. } = response else { anyhow::bail!("worker not connected: {}", serde_json::to_string(&response)?) };
            identities.push(grant.identity);
        }
        let mut unaffected = connect_qualified(&runtime,projects[1].binding,&selections[1]).await?;
        let moved = owner.path().join("offline-source");
        fs::rename(&projects[0].root,&moved)?;
        let old = identities[0];
        let mut foreign = old;
        foreign.worker = airk_kernel::service_broker::new_instance_id()?;
        ensure!(matches!(request(Method::RestartProject { project: old.project, expected: Some(foreign) }).await?, Response::Failure { code } if code=="project_worker_changed"),"foreign identity authorized restart");
        ensure!(matches!(request(Method::RestartProject { project: old.project, expected: Some(old) }).await?, Response::ProjectRestarting { previous: Some(identity), .. } if identity==old),"restart not accepted");
        let deadline = Instant::now()+Duration::from_secs(30);
        let replacement = loop {
            if let Response::Worker { identity: Some(identity), restarting: false, .. } = request(Method::Worker { project: old.project }).await?
                && identity.worker != old.worker { break identity; }
            if Instant::now() >= deadline {
                anyhow::bail!("replacement worker did not become ready: worker={}, resources={}, logs={}",
                    serde_json::to_string(&request(Method::Worker { project: old.project }).await?)?,
                    serde_json::to_string(&request(Method::Resources).await?)?,
                    serde_json::to_string(&request(Method::Logs { limit:32 }).await?)?);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        ensure!(replacement.service==old.service && replacement.project==old.project && replacement.fencing_generation>old.fencing_generation,"restart lost service/project fencing");
        ensure!(matches!(request(Method::Worker { project: projects[1].binding }).await?, Response::Worker { identity: Some(identity), .. } if identity==identities[1]),"restart changed unrelated worker");
        ensure!(matches!(request(Method::RestartProject { project: old.project, expected: Some(old) }).await?, Response::Failure { code } if code=="project_worker_changed"),"stale restart stopped a replacement");
        let mut retained = connect_qualified(&runtime,old.project,&selections[0]).await?;
        let arguments = serde_json::json!({"query":"copper beacon","mode":"lexical"}).as_object().unwrap().clone();
        for connection in [&mut unaffected,&mut retained] {
            let result = connection.read_retry("airk_search".into(),arguments.clone()).await?;
            ensure!(result.get("isError").and_then(Value::as_bool)!=Some(true) && result.to_string().contains("memo.txt"),"restart lost retained citation: {result}");
        }
        ensure!(fs::read(moved.join("memo.txt"))?==b"Copper beacon original restart fixture.\n","restart changed source bytes");
        drop(retained); drop(unaffected);
        request(Method::Shutdown).await?;
        Ok::<_,anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
    finish_qualified_process_fixture(&service)?;
    Ok(())
}

#[test]
fn failed_project_start_circuit_requires_explicit_restart_and_preserves_unknown_data() -> Result<()>
{
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("memo.txt"), b"Original circuit fixture.")?;
    let namespace_dir = data.join("collections-1.4");
    let foreign = namespace_dir.join("unclaimed.txt");
    let project = airk_kernel::service_projects::registration(&source, &data, "circuit")?;
    let mut children = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let request = |method| airk_cli::service_runtime::request(&runtime, method);
            request(Method::Register {
                project: project.clone(),
            })
            .await?;
            fs::create_dir(&namespace_dir)?;
            fs::write(&foreign, b"Preserve unclaimed fixture data.")?;
            let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
            else {
                anyhow::bail!("epoch missing")
            };
            for _ in 0..3 {
                let caller = airk_kernel::service_broker::new_instance_id()?;
                let Response::Job { job } = request(Method::Enqueue {
                    project: project.binding,
                    caller,
                    key: RequestKey {
                        namespace,
                        epoch,
                        nonce: caller,
                    },
                    kind: JobKind::Index,
                })
                .await?
                else {
                    anyhow::bail!("job not accepted")
                };
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let Response::Job { job } = request(Method::Job { id: job.id }).await? else {
                        anyhow::bail!("job lost")
                    };
                    if job.state == JobState::Failed {
                        break;
                    }
                    ensure!(
                        Instant::now() < deadline,
                        "failed startup was not settled: {}",
                        serde_json::to_string(
                            &request(Method::Console {
                                after: None,
                                revision: None,
                                limit: 1
                            })
                            .await?
                        )?
                    );
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if matches!(
                    request(Method::Worker {
                        project: project.binding
                    })
                    .await?,
                    Response::Worker {
                        identity: None,
                        automatic_start_blocked: true,
                        ..
                    }
                ) {
                    break;
                }
                ensure!(Instant::now() < deadline, "start circuit did not open");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            for _ in 0..2 {
                ensure!(
                    matches!(
                        request(Method::Connect {
                            project: project.binding
                        })
                        .await?,
                        Response::Failure { .. }
                    ),
                    "ordinary connection reset start circuit"
                );
            }
            ensure!(
                fs::read(&foreign)? == b"Preserve unclaimed fixture data.",
                "failed startup changed unknown data"
            );
            fs::remove_file(&foreign)?; // Remove only the exact file created by this isolated fixture.
            ensure!(
                matches!(
                    request(Method::RestartProject {
                        project: project.binding,
                        expected: None
                    })
                    .await?,
                    Response::ProjectRestarting { .. }
                ),
                "explicit reset rejected"
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if matches!(
                    request(Method::Worker {
                        project: project.binding
                    })
                    .await?,
                    Response::Worker {
                        identity: Some(_),
                        restarting: false,
                        automatic_start_blocked: false,
                        ..
                    }
                ) {
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "explicit restart did not clear start circuit"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            ensure!(
                fs::read(source.join("memo.txt"))? == b"Original circuit fixture.",
                "restart changed original source"
            );
            request(Method::Shutdown).await?;
            Ok::<_, anyhow::Error>(())
        })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
    Ok(())
}

#[test]
fn trusted_console_pipe_and_stdin_rpc_preserve_closed_projects_and_frame_bounds() -> Result<()> {
    use airk_cli::console_client::{Request, Response as ConsoleResponse};
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    use std::io::Write;
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "console")?;
    let mut services = Children(vec![start(&runtime)?]);
    wait_ready(&runtime)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(airk_cli::service_runtime::request(
            &runtime,
            Method::Register {
                project: project.clone(),
            },
        ))?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
    command
        .args([
            "--root",
            "/console-must-not-open-source",
            "service",
            "console",
            "--runtime-dir",
        ])
        .arg(&runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut consoles = Children(vec![command.spawn()?]);
    let mut input = consoles.0[0]
        .stdin
        .take()
        .context("console input missing")?;
    let mut output = consoles.0[0]
        .stdout
        .take()
        .context("console output missing")?;
    {
        let mut call = |method| -> Result<Response> {
            let id = airk_kernel::service_broker::new_instance_id()?;
            airk_cli::service_ipc::write_frame(&mut input, &Request { id, method })?;
            let response: ConsoleResponse = airk_cli::service_ipc::read_frame(&mut output)?
                .context("console closed before reply")?;
            ensure!(response.id == id, "console crossed request identity");
            Ok(response.response)
        };
        ensure!(
            matches!(call(Method::Health)?, Response::Health { .. }),
            "console health unavailable"
        );
        ensure!(
            matches!(call(Method::Connect {project:project.binding})?,Response::Failure {code} if code=="console_brokerage_forbidden"),
            "console exposed an evidence credential"
        );
        let mut pinned = command
            .arg("--runtime-sha256")
            .arg("f".repeat(64))
            .spawn()?;
        let mut pinned_input = pinned.stdin.take().unwrap();
        let mut pinned_output = pinned.stdout.take().unwrap();
        let pin_id = airk_kernel::service_broker::new_instance_id()?;
        airk_cli::service_ipc::write_frame(
            &mut pinned_input,
            &Request {
                id: pin_id,
                method: Method::Health,
            },
        )?;
        let mismatch: ConsoleResponse = airk_cli::service_ipc::read_frame(&mut pinned_output)?
            .context("pinned console closed")?;
        ensure!(
            mismatch.id == pin_id
                && matches!(mismatch.response,Response::Failure {code} if code=="service_rebind_required"),
            "console ignored an approved payload identity mismatch"
        );
        drop(pinned_input);
        drop(pinned_output);
        ensure!(wait_exit(&mut pinned)?, "mismatched console did not drain");
        let Response::Console { page } = call(Method::Console {
            after: None,
            revision: None,
            limit: 32,
        })?
        else {
            anyhow::bail!("console page unavailable")
        };
        ensure!(
            page.cached
                && page.projects.len() == 1
                && page.projects[0].inspection.is_none()
                && page.projects[0].progress.is_none(),
            "closed project metadata was invented"
        );
        ensure!(
            matches!(call(Method::Workers {projects:vec![project.binding]})?,Response::Workers {observations,..} if observations.len()==1&&observations[0].identity.is_none()),
            "console started a closed project"
        );
        ensure!(
            matches!(
                call(Method::Workers {
                    projects: vec![project.binding, project.binding]
                })?,
                Response::Failure { .. }
            ),
            "duplicate worker scope accepted"
        );
        ensure!(
            fs::read_dir(&data)?.next().is_none(),
            "console opened project state"
        );
        let mut rpc = Command::new(env!("CARGO_BIN_EXE_airk"))
            .args([
                "--root",
                "/rpc-must-not-open-source",
                "service",
                "rpc",
                "--runtime-dir",
            ])
            .arg(&runtime)
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut body = serde_json::to_vec(&Method::Console {
            after: None,
            revision: None,
            limit: 32,
        })?;
        body.resize(airk_core::service::MAX_FRAME_BYTES, b' ');
        rpc.stdin.take().unwrap().write_all(&body)?;
        let result = rpc.wait_with_output()?;
        ensure!(
            result.status.success(),
            "bounded stdin RPC failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        ensure!(
            serde_json::from_slice::<Value>(&result.stdout)?["status"] == "console",
            "stdin RPC returned wrong result"
        );
        let mut oversized = Command::new(env!("CARGO_BIN_EXE_airk"))
            .args(["service", "rpc", "--runtime-dir"])
            .arg(&runtime)
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        oversized
            .stdin
            .take()
            .unwrap()
            .write_all(&vec![b' '; airk_core::service::MAX_FRAME_BYTES + 1])?;
        ensure!(
            !oversized.wait()?.success(),
            "oversized stdin management request accepted"
        );
        let rejected = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(airk_cli::service_runtime::request_for_instance(
                &runtime,
                Method::Shutdown,
                airk_kernel::service_broker::new_instance_id()?,
            ))?;
        ensure!(
            matches!(rejected,Response::Failure {code} if code=="service_instance_changed"),
            "stale service identity authorized shutdown"
        );
        ensure!(
            matches!(call(Method::Health)?, Response::Health { .. }),
            "rejected stale shutdown stopped the service"
        );
        ensure!(
            matches!(call(Method::Shutdown)?, Response::Stopping),
            "console shutdown not acknowledged"
        );
        ensure!(wait_exit(&mut services.0[0])?, "service did not drain");
        ensure!(
            matches!(call(Method::Health)?,Response::Failure {code} if code=="service_unavailable"),
            "down-service console invented live health"
        );
    }
    drop(input);
    drop(output);
    ensure!(
        wait_exit(&mut consoles.0[0])?,
        "console did not exit at parent EOF"
    );
    ensure!(
        fs::read_dir(&data)?.next().is_none(),
        "console failure opened project state"
    );
    Ok(())
}

/// One `airkctl` command line run in process (`airkctl::cli::run`): exit
/// code, stdout and stderr.
fn airkctl(arguments: &[&str]) -> (u8, String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut io = airkctl::cli::Io {
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
    };
    let line = std::iter::once("airkctl")
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
    let code = airkctl::cli::run(line, &mut io);
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

/// The Health document without the one field that changes between two reads.
fn health_identity(mut health: Value) -> Value {
    let observation = health["observation"]
        .as_object_mut()
        .expect("health observation");
    assert!(observation.remove("uptime_ms").is_some());
    health
}

/// `airkctl service health|shutdown` print what `airk service health|shutdown
/// --runtime-dir` printed (split step 3b.1a).
#[test]
fn airkctl_service_health_and_shutdown_print_what_airk_printed() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let dir = root.to_str().context("runtime path")?;
    let old = health_identity(control(&root, "health")?);
    for line in [
        &["service", "health", "--runtime-dir", dir][..],
        &["--service-runtime-dir", dir, "service", "health"],
    ] {
        let (code, stdout, stderr) = airkctl(line);
        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stdout.lines().count(), 1, "one compact line");
        assert_eq!(health_identity(serde_json::from_str(&stdout)?), old);
    }
    let stopping = control(&root, "shutdown")?;
    assert_eq!(stopping, serde_json::json!({"status": "stopping"}));
    assert!(wait_exit(&mut children.0[0])?, "service exits cleanly");
    let (code, stdout, stderr) = airkctl(&["service", "health", "--runtime-dir", dir]);
    assert_eq!((code, stdout.as_str()), (3, ""), "{stderr}");
    children.0.push(start(&root)?);
    wait_ready(&root)?;
    let (code, stdout, stderr) = airkctl(&["service", "shutdown", "--runtime-dir", dir]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(serde_json::from_str::<Value>(&stdout)?, stopping);
    assert!(wait_exit(&mut children.0[1])?, "service exits cleanly");
    Ok(())
}

/// Shuts a detached test service down even when an assertion fails first.
struct ShutdownOnDrop(String);
impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        let _ = airkctl(&["service", "shutdown", "--runtime-dir", &self.0]);
    }
}

/// `airkctl service start` starts the verified `airk` image as the private
/// `service-detached` role through a single-use start request and waits for
/// its health; a second start reports the running service (split step 3b.1d).
#[test]
fn airkctl_service_start_runs_the_private_role_until_healthy() -> Result<()> {
    use airk_core::service::ControlResponse;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let dir = root.to_str().context("runtime path")?.to_owned();
    let airk = Path::new(env!("CARGO_BIN_EXE_airk"));
    let version = env!("CARGO_PKG_VERSION");
    let _guard = ShutdownOnDrop(dir.clone());
    let first = airkctl::start::start(airk, &root, version)
        .map_err(|line| anyhow::anyhow!("{}: {}", line.code, line.message))?;
    ensure!(first.started, "the first start starts the service");
    let ControlResponse::Health { observation } = &first.health else {
        anyhow::bail!("start did not return Health");
    };
    let instance = observation.instance;
    ensure!(
        fs::read_dir(root.join("start"))?.next().is_none(),
        "the single-use start request was consumed"
    );
    let second = airkctl::start::start(airk, &root, version)
        .map_err(|line| anyhow::anyhow!("{}: {}", line.code, line.message))?;
    ensure!(
        !second.started,
        "a second start reports the running service"
    );
    let ControlResponse::Health { observation } = &second.health else {
        anyhow::bail!("second start did not return Health");
    };
    ensure!(observation.instance == instance, "same service instance");
    let (code, _, stderr) = airkctl(&["service", "shutdown", "--runtime-dir", &dir]);
    ensure!(code == 0, "shutdown failed: {stderr}");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (code, _, _) = airkctl(&["service", "health", "--runtime-dir", &dir]);
        if code == 3 {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "the started service did not exit"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

/// M19 stop and restart through `airkctl`'s lifecycle and start logic on a
/// live service, with the ledger observed directly (split U85, 3b.1e).
#[test]
fn airkctl_lifecycle_stops_restarts_and_settles_the_ledger() -> Result<()> {
    use airk_core::service::ControlResponse;
    use airk_core::service_lifecycle::{LifecycleAction, LifecyclePhase};
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let dir = root.to_str().context("runtime path")?.to_owned();
    let airk = Path::new(env!("CARGO_BIN_EXE_airk"));
    let version = env!("CARGO_PKG_VERSION");
    let _guard = ShutdownOnDrop(dir.clone());
    let started = |root: &Path| -> Result<airk_core::identity::Incarnation> {
        let started = airkctl::start::start(airk, root, version)
            .map_err(|line| anyhow::anyhow!("{}: {}", line.code, line.message))?;
        let ControlResponse::Health { observation } = &started.health else {
            anyhow::bail!("start did not return Health");
        };
        Ok(observation.instance)
    };
    let stopped = |root: &Path, action| {
        airkctl::lifecycle::stop(root, version, action)
            .map_err(|line| anyhow::anyhow!("{}: {}", line.code, line.message))
    };
    let first = started(&root)?;
    let stop = stopped(&root, LifecycleAction::Stop)?;
    ensure!(stop.instance == first && stop.record.request.expected_instance == first);
    ensure!(stop.record.phase == LifecyclePhase::Draining);
    let saved = ServiceRegistry::open_saved(&root)?;
    ensure!(
        saved.pending_lifecycle()?.is_none(),
        "the drained stop completed"
    );
    ensure!(saved.service_explicitly_stopped()?, "the saved Stop is set");
    drop(saved);
    let second = started(&root)?;
    ensure!(second != first, "a new instance");
    ensure!(
        !ServiceRegistry::open_saved(&root)?.service_explicitly_stopped()?,
        "an explicit start clears the saved Stop"
    );
    let restart = stopped(&root, LifecycleAction::Restart)?;
    ensure!(restart.instance == second);
    let pending = ServiceRegistry::open_saved(&root)?
        .pending_lifecycle()?
        .context("a drained restart awaits its start")?;
    ensure!(pending.phase == LifecyclePhase::Starting && pending.id == restart.record.id);
    let third = started(&root)?;
    ensure!(third != second, "the restart started a new instance");
    let saved = ServiceRegistry::open_saved(&root)?;
    ensure!(
        saved.pending_lifecycle()?.is_none(),
        "the admitted start completed it"
    );
    ensure!(!saved.service_explicitly_stopped()?);
    Ok(())
}

/// `airkctl service logs` prints what `airk service logs` printed, from the
/// running service (recent pages over IPC) and from the files after it stopped;
/// `--output-file` is the U79 name of `--output` (split U86/U88/U89, 3b.1f-2).
#[test]
fn airkctl_service_logs_print_what_airk_printed() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let dir = root.to_str().context("runtime path")?.to_owned();
    let compare = |phase: &str| -> Result<()> {
        for case in [
            &[][..],
            &["--format", "text"],
            &["--limit", "3"],
            &["--all"],
            &["--raw"],
        ] {
            let old = command()
                .args(["service", "logs", "--runtime-dir", &dir])
                .args(case)
                .output()?;
            ensure!(
                old.status.success(),
                "airk {case:?}: {}",
                String::from_utf8_lossy(&old.stderr)
            );
            let line: Vec<&str> = ["service", "logs", "--runtime-dir", &dir]
                .into_iter()
                .chain(case.iter().copied())
                .collect();
            let (code, stdout, stderr) = airkctl(&line);
            ensure!(code == 0, "airkctl {case:?}: {stderr}");
            ensure!(
                stdout.as_bytes() == old.stdout.as_slice(),
                "{phase} {case:?} differs:\n{}\n---\n{stdout}",
                String::from_utf8_lossy(&old.stdout)
            );
        }
        let old_file = owner.path().join(format!("{phase}-airk.json"));
        let new_file = owner.path().join(format!("{phase}-airkctl.json"));
        let old = command()
            .args([
                "service",
                "logs",
                "--runtime-dir",
                &dir,
                "--all",
                "--output",
            ])
            .arg(&old_file)
            .output()?;
        ensure!(
            old.status.success(),
            "{}",
            String::from_utf8_lossy(&old.stderr)
        );
        let new_path = new_file.to_str().context("capture path")?;
        let (code, stdout, stderr) = airkctl(&[
            "service",
            "logs",
            "--runtime-dir",
            &dir,
            "--all",
            "--output-file",
            new_path,
        ]);
        ensure!(code == 0, "{stderr}");
        ensure!(
            stdout.as_bytes() == old.stdout.as_slice(),
            "{phase} capture line"
        );
        ensure!(
            fs::read(&old_file)? == fs::read(&new_file)?,
            "{phase} capture bytes"
        );
        Ok(())
    };
    compare("running")?;
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "service exits cleanly");
    compare("stopped")?;
    Ok(())
}

/// `airkctl service status` reports a running service with its health, saved
/// Stop and pending lifecycle record, and a stopped one as not running (U87,
/// split 3b.1f-3).
#[test]
fn airkctl_service_status_reports_running_and_stopped() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let mut children = Children(vec![start(&root)?]);
    let health = wait_ready(&root)?;
    let dir = root.to_str().context("runtime path")?.to_owned();
    let status = || -> Result<Value> {
        let (code, stdout, stderr) = airkctl(&[
            "--output",
            "json",
            "service",
            "status",
            "--runtime-dir",
            &dir,
        ]);
        ensure!(code == 0, "status failed: {stderr}");
        Ok(serde_json::from_str(&stdout)?)
    };
    let running = status()?;
    ensure!(running["schema"] == "airkctl-service-status/1", "{running}");
    let service = &running["service"];
    ensure!(service["state"] == "running", "{running}");
    ensure!(
        service["health"]["instance"] == health["observation"]["instance"],
        "{running}"
    );
    ensure!(service["explicitly_stopped"] == false, "{running}");
    ensure!(service["pending_lifecycle"].is_null(), "{running}");
    let (code, text, stderr) = airkctl(&["service", "status", "--runtime-dir", &dir]);
    ensure!(code == 0, "{stderr}");
    ensure!(
        text.contains("\nservice: running (instance ")
            && text.contains("\nsaved stop: no\npending lifecycle: none\n"),
        "{text}"
    );
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "service exits cleanly");
    let stopped = status()?;
    ensure!(stopped["service"]["state"] == "not_running", "{stopped}");
    Ok(())
}

/// One `airkctl` command line run in process with `input` on stdin.
fn airkctl_with_input(arguments: &[&str], input: &[u8]) -> (u8, String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut stdin = input;
    let mut io = airkctl::cli::Io {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
    };
    let line = std::iter::once("airkctl")
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
    let code = airkctl::cli::run(line, &mut io);
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

/// The raw job-error ring's records without their observation time.
fn raw_records(dir: &str) -> Result<Vec<Value>> {
    let output = command()
        .args(["service", "logs", "--runtime-dir", dir, "--raw"])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)?
        .lines()
        .map(|line| {
            let mut record: Value = serde_json::from_str(line)?;
            record
                .as_object_mut()
                .context("raw record")?
                .remove("observed_unix_ms")
                .context("observation time")?;
            Ok(record)
        })
        .collect()
}

/// `airkctl service record-error` records what `airk service record-error`
/// recorded, through M14 while the service runs and through the private
/// `record-error` role while it is down (U75, split 3b.2).
#[test]
fn airkctl_record_error_matches_airk_running_and_stopped() -> Result<()> {
    use std::io::Write as _;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let dir = root.to_str().context("runtime path")?.to_owned();
    let input =
        br#"{"code":"setup_failed","detail":{"chain":"download refused","truncated":false}}"#;
    let old = || -> Result<Vec<u8>> {
        let mut child = command()
            .args([
                "service",
                "record-error",
                "--runtime-dir",
                &dir,
                "--operation",
                "service_setup",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().context("stdin")?.write_all(input)?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    };
    let recorded = old()?;
    ensure!(
        recorded == b"{\"recorded\":true}\n",
        "{}",
        String::from_utf8_lossy(&recorded)
    );
    let (code, stdout, stderr) = airkctl_with_input(
        &[
            "service",
            "record-error",
            "--runtime-dir",
            &dir,
            "--operation",
            "service_setup",
        ],
        input,
    );
    ensure!(code == 0, "{stderr}");
    ensure!(stdout.as_bytes() == recorded.as_slice(), "{stdout}");
    let running = raw_records(&dir)?;
    ensure!(
        running.len() == 2 && running[0] == running[1],
        "{running:?}"
    );
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "service exits cleanly");
    ensure!(old()? == recorded);
    ensure!(matches!(
        airkctl::record::by_service(&root, "service_setup", input, env!("CARGO_PKG_VERSION")),
        Ok(airkctl::record::Recorded::ServiceDown)
    ));
    airkctl::record::by_role(
        Path::new(env!("CARGO_BIN_EXE_airk")),
        &root,
        "service_setup",
        input,
    )
    .map_err(|line| anyhow::anyhow!("{}: {}", line.code, line.message))?;
    let all = raw_records(&dir)?;
    ensure!(all.len() == 4, "{all:?}");
    ensure!(all.iter().all(|record| record == &running[0]), "{all:?}");
    Ok(())
}

/// An automatic start (a command that needs the service) honors a saved
/// explicit Stop; an explicit start clears it (contracts §3.2, step 3c.1).
#[test]
fn an_automatic_start_honors_a_saved_stop() -> Result<()> {
    use airk_core::{install_record::StartMode, service_lifecycle::LifecycleAction};
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let dir = root.to_str().context("runtime path")?.to_owned();
    let airk = Path::new(env!("CARGO_BIN_EXE_airk"));
    let version = env!("CARGO_PKG_VERSION");
    let _guard = ShutdownOnDrop(dir.clone());
    let error =
        |line: airkctl::contract::ErrorLine| anyhow::anyhow!("{}: {}", line.code, line.message);
    airkctl::start::start(airk, &root, version).map_err(error)?;
    airkctl::lifecycle::stop(&root, version, LifecycleAction::Stop).map_err(error)?;
    let refused = airkctl::start::start_with(airk, &root, version, StartMode::Automatic);
    let Err(line) = refused else {
        anyhow::bail!("an automatic start ran despite the saved Stop");
    };
    ensure!(line.code == "service_start_failed", "{}", line.code);
    ensure!(
        line.message.contains("airkctl service start"),
        "{}",
        line.message
    );
    let (code, _, _) = airkctl(&["service", "health", "--runtime-dir", &dir]);
    ensure!(
        code == 3,
        "no service runs after the refused automatic start"
    );
    let started = airkctl::start::start(airk, &root, version).map_err(error)?;
    ensure!(started.started, "an explicit start clears the saved Stop");
    Ok(())
}
