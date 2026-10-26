// SPDX-License-Identifier: Apache-2.0
//! Native process election/control uses its own private fixture, never an installed service.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Children(Vec<Child>);

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
fn missing_registered_source_still_brokers_retained_evidence_and_reports_a_typed_job_failure()
-> Result<()> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source = owner.path().join("source");
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
        fs::rename(&source, &moved)?;
        // The first project owner starts after the registered root disappears.
        let mut connection =
            airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        let result = connection
            .call(
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
            let sampled = body(connection.call("airk_search".into(), serde_json::json!({"query":"copper harbor","mode":"lexical","include_status":true}).as_object().unwrap().clone()).await?)?;
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
        let repeated = body(renewed.call("airk_search".into(), arguments.clone()).await?)?;
        ensure!(repeated.get("freshness").is_none(), "renewal forgot client observations");
        let mut independent = airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
        let independent = body(independent.call("airk_search".into(), arguments).await?)?;
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
            let Response::Job { job: observed } = request(Method::Job { id: job.id }).await? else {
                anyhow::bail!("job receipt missing")
            };
            if let Some(outcome) = observed.outcome {
                ensure!(
                    matches!(outcome, JobOutcome::Failed { code } if code == "source_unavailable"),
                    "missing root did not report typed source failure"
                );
                let fresh_until = tokio::time::Instant::now() + Duration::from_secs(10);
                let mut cached = airk_cli::project_client::Connection::connect(&runtime, project.binding).await?;
                loop {
                    let sampled = body(cached.call("airk_search".into(), serde_json::json!({"query":"copper harbor","mode":"lexical","include_status":true}).as_object().unwrap().clone()).await?)?;
                    if sampled["freshness"]["state"] == "failed" {
                        ensure!(sampled["freshness"]["source_job"] == job.id.to_string() && sampled["freshness"]["error"] == "source_unavailable", "source failure lost attempt identity: {sampled}");
                        break;
                    }
                    ensure!(tokio::time::Instant::now() < fresh_until, "source failure was not cached: {sampled}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "missing-root job failed to settle"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        request(Method::Pause {
            project: project.binding,
            paused: true,
        })
        .await?;
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
                if event.job == Some(job.id) && event.code.as_deref() == Some("source_unavailable")
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
            "failed scan lost its content-free durable event"
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
            fs::read_to_string(moved.join("memo.md"))?
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
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir_all(source.join("nested"))?;
    fs::create_dir(&data)?;
    let file = source.join("nested/original.txt");
    fs::write(&file, b"Initial native watcher fixture.\n")?;
    let project = airk_kernel::service_projects::registration(&source, &data, "watch-fixture")?;
    let mut children = Children(vec![start(&root)?]);
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
                "initial automatic reconciliation/native watch did not become current"
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
        let mut connection =
            airk_cli::project_client::Connection::connect(&root, project.binding).await?;
        let evidence = connection
            .call(
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
    children.0.push(start(&root)?);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let mut connection = airk_cli::project_client::Connection::connect(&root, project.binding).await?;
            let evidence = connection.call("airk_search".into(), serde_json::json!({"query":"lavender signal","mode":"lexical","max_bytes":4096}).as_object().unwrap().clone()).await?;
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
    Ok(command()
        .arg("serve")
        .arg("--runtime-dir")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?)
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
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
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
        request(Method::ConfigureWatch {
            project: project.binding,
            enabled: true,
        })
        .await?;
        let until = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            if matches!(
                request(Method::SourceStatus).await?,
                Response::SourceStatus {
                    available: true,
                    projects: Some(1),
                    pending_events: Some(0),
                    ..
                }
            ) {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < until,
                "quiet watcher did not become ready"
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

fn probe_stdio_bridge(root: &Path, project: airk_core::service::ProjectBinding) -> Result<()> {
    use std::io::{BufRead, Read, Write};
    let mut child = command()
        .arg("bridge")
        .arg("--runtime-dir")
        .arg(root)
        .arg("--project")
        .arg(project.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("bridge input missing")?;
    let output = child.stdout.take().context("bridge output missing")?;
    let mut children = Children(vec![child]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        let mut output = std::io::BufReader::new(output);
        loop {
            let mut line = Vec::new();
            let result = (&mut output)
                .take(128 * 1024 + 1)
                .read_until(b'\n', &mut line);
            if !matches!(result,Ok(count) if count>0&&count<=128*1024) {
                break;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(&line)
                && sender.try_send(value).is_err()
            {
                break;
            }
        }
    });
    let result = (|| -> Result<()> {
        let mut send = |value: &Value| -> Result<()> {
            serde_json::to_writer(&mut input, value)?;
            input.write_all(b"\n")?;
            input.flush()?;
            Ok(())
        };
        send(
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-original-test","version":"1"}}}),
        )?;
        let initialized = receiver.recv_timeout(Duration::from_secs(10))?;
        ensure!(
            initialized["id"] == 1 && initialized["result"]["serverInfo"]["name"] == "airk",
            "bridge initialization failed"
        );
        send(&serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        send(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))?;
        let listed = receiver.recv_timeout(Duration::from_secs(10))?;
        let tools = listed["result"]["tools"]
            .as_array()
            .context("bridge tool list missing")?;
        ensure!(
            tools.len() == 16
                && tools
                    .iter()
                    .all(|tool| tool["annotations"]["readOnlyHint"] == true),
            "bridge exposed a management tool"
        );
        send(
            &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"airk_search","arguments":{"query":"cobalt harbor","mode":"lexical","max_bytes":4096}}}),
        )?;
        let evidence = receiver.recv_timeout(Duration::from_secs(10))?;
        ensure!(
            evidence["id"] == 3 && evidence.to_string().contains("original.txt"),
            "stdio bridge did not preserve evidence"
        );
        Ok(())
    })();
    drop(input);
    children.0[0].kill()?;
    children.0[0].wait()?;
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("bridge reader panicked"))?;
    result
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
    use airk_cli::{local_ipc, project_client::Connection, project_protocol};
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let text = b"Native supervised project retrieval fixture: cobalt harbor.";
    fs::write(source.join("original.txt"), text)?;
    airk_kernel::Kernel::open_collection(&source, Some(data.clone()), "fixture")?
        .configure_vectors(false)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let mut children = Children(vec![start(&root)?]);
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
        let Response::Connection { role, grant } = request(Method::Connect {
            project: project.binding,
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
        let mut connection = Connection::connect(&root, project.binding).await?;
        let arguments =
            serde_json::json!({"query":"cobalt harbor","mode":"lexical","max_bytes":4096})
                .as_object()
                .context("invalid fixture query")?
                .clone();
        let result = connection
            .call("airk_search".into(), arguments.clone())
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
        probe_stdio_bridge(&root, project.binding)?;
        let pinned = airk_kernel::Kernel::open_service_collection_for_diagnostics(
            Path::new(&project.root),
            Some(data.clone()),
            "fixture",
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
            .call("airk_search".into(), arguments.clone())
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
            .call(
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
fn installed_log_binding_places_the_ring_outside_control_and_missing_ownership_never_falls_back()
-> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{ControlMethod as Method, ControlResponse as Response},
        service_logs::LogKind,
    };
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let install = base.join("install");
    let state = base.join("state");
    let logs = base.join("native-logs");
    for path in [&install, &state] {
        airk_cli::local_ipc::prepare_root(path)?;
    }
    let binding = serde_json::to_vec(
        &serde_json::json!({"schema":1,"install":install,"state":state,"logs":logs,"channel":"stable","ready":true}),
    )?;
    fs::write(install.join("runtime-layout.json"), &binding)?;
    fs::write(state.join("airk-state-1.4.owner.json"), &binding)?;
    fs::write(state.join("airk-state-1.4.lock"), [])?;
    let root = state.join("control");
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    let executor = tokio::runtime::Runtime::new()?;
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
        let source = base.join("source");
        let data = base.join("data");
        fs::create_dir(&source)?;
        fs::create_dir(&data)?;
        let project = airk_kernel::service_projects::registration(&source, &data, "logs")?;
        ensure!(
            matches!(
                airk_cli::service_runtime::request(&root, Method::Register { project }).await?,
                Response::Registered { .. }
            ),
            "safe registration failed"
        );
        let private = airk_kernel::service_projects::registration(&logs, &data, "private")?;
        ensure!(
            matches!(
                airk_cli::service_runtime::request(&root, Method::Register { project: private })
                    .await?,
                Response::Failure { .. }
            ),
            "native log location was admitted as source"
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
    children.0.push(start(&root)?);
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
    Ok(())
}

#[test]
fn installed_control_loss_rejects_native_startup_and_restoration_retains_registry_namespace()
-> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let install = base.join("install");
    let state = base.join("state");
    for path in [&install, &state] {
        airk_cli::local_ipc::prepare_root(path)?;
    }
    let logs = base.join("native-logs");
    let binding = serde_json::to_vec(
        &serde_json::json!({"schema":1,"install":install,"state":state,"logs":logs,"channel":"stable","ready":true}),
    )?;
    fs::write(install.join("runtime-layout.json"), &binding)?;
    fs::write(state.join("airk-state-1.4.owner.json"), &binding)?;
    fs::write(state.join("airk-state-1.4.lock"), [])?;
    let root = state.join("control");
    let mut children = Children(vec![start(&root)?]);
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
    children.0.push(start(&root)?);
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
    children.0.push(start(&root)?);
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
    children.0.push(start(&root)?);
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
    children.0.push(start(&root)?);
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
    let runtime = owner.path().join("runtime");
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
    let mut children = Children(vec![start(&runtime)?]);
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
            ensure!(matches!(request(Method::Worker { project: project.binding }).await?, Response::Worker { identity: None, restarting: false, .. }), "observation started a closed project");
            ensure!(fs::read_dir(&project.data_root)?.next().is_none(), "worker observation opened project data");
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
        for project in &projects {
            let response = request(Method::Connect { project: project.binding }).await?;
            let Response::Connection { grant, .. } = response else { anyhow::bail!("worker not connected: {}", serde_json::to_string(&response)?) };
            identities.push(grant.identity);
        }
        let mut unaffected = airk_cli::project_client::Connection::connect(&runtime,projects[1].binding).await?;
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
        let mut retained = airk_cli::project_client::Connection::connect(&runtime,old.project).await?;
        let arguments = serde_json::json!({"query":"copper beacon","mode":"lexical"}).as_object().unwrap().clone();
        for connection in [&mut unaffected,&mut retained] {
            let result = connection.call("airk_search".into(),arguments.clone()).await?;
            ensure!(result.get("isError").and_then(Value::as_bool)!=Some(true) && result.to_string().contains("memo.txt"),"restart lost retained citation: {result}");
        }
        ensure!(fs::read(moved.join("memo.txt"))?==b"Copper beacon original restart fixture.\n","restart changed source bytes");
        drop(retained); drop(unaffected);
        request(Method::Shutdown).await?;
        Ok::<_,anyhow::Error>(())
    })?;
    ensure!(wait_exit(&mut children.0[0])?, "service did not drain");
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
                    ensure!(Instant::now() < deadline, "failed startup was not settled");
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
