// SPDX-License-Identifier: Apache-2.0
//! One lock-bearing project owner, with direct bounded read-only query admission.
use crate::{
    local_ipc::{self, Listener, Stream},
    project_protocol::{self as protocol, Command, Reply},
    service_ipc,
};
use airk_core::service::{ConnectionGrant, JobKind, JobOutcome, MAX_FRAME_BYTES, WorkerIdentity};
use airk_kernel::{
    Kernel,
    service_broker::{Broker, BrokerLimits},
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

// All accepted blocking kernel work finishes before the process exits. Retain the OS
// ownership handle until that exit, including error paths through the async runtime.
static PROCESS_LEASE: OnceLock<Arc<ProjectLease>> = OnceLock::new();

fn retry_semantic_turn<T>(mut turn: impl FnMut() -> Result<T>, budget: Duration) -> Result<T> {
    let deadline = Instant::now() + budget;
    loop {
        if let Err(error) = airk_kernel::service_execution::checkpoint() {
            airk_kernel::service_execution::progress_waiting(None);
            return Err(error);
        }
        match turn() {
            Err(error)
                if Kernel::service_enrichment_contention(&error) && Instant::now() < deadline =>
            {
                airk_kernel::service_execution::progress_waiting(
                    Kernel::service_enrichment_wait_reason(&error),
                );
                // Retained work is fenced/idempotent. Keep the same accepted job
                // and charged grant through temporary read/publication exclusion.
                std::thread::sleep(
                    Duration::from_millis(25)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            result => {
                airk_kernel::service_execution::progress_waiting(None);
                return result;
            }
        }
    }
}

fn semantic_publication<T: PartialEq, R>(
    mut turn: impl FnMut() -> Result<()>,
    mut observe: impl FnMut() -> Result<(T, R)>,
    budget: Duration,
) -> Result<R> {
    let before = retry_semantic_turn(&mut observe, budget)?.0;
    let mut inspect = false;
    let mut contention = None;
    retry_semantic_turn(
        || {
            if !inspect {
                match turn() {
                    Ok(()) => {}
                    Err(error) if Kernel::service_enrichment_contention(&error) => {
                        contention = Some(error)
                    }
                    Err(error) => return Err(error),
                }
                inspect = true;
            }
            // Once publication advanced, no mutation is repeated. This worker is
            // the sole writer, and temporary exclusion never publishes a failure.
            let (publication, result) = observe()?;
            if publication == before
                && let Some(error) = contention.take()
            {
                inspect = false;
                return Err(error);
            }
            Ok(result)
        },
        budget,
    )
}
struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
use tokio::{
    sync::{Notify, Semaphore, mpsc},
    task::JoinSet,
};

struct State {
    control: std::path::PathBuf,
    identity: WorkerIdentity,
    storage: airk_store::service::data_scope::BoundDataRoot,
    broker: Mutex<Broker>,
    tools: airk_mcp::local::LocalTools,
    closing: Arc<AtomicBool>,
}

#[derive(Default)]
struct WorkState {
    active: Option<airk_core::identity::Incarnation>,
    finished: Option<airk_core::identity::Incarnation>,
    maintenance: Option<airk_core::service::MaintenanceObservation>,
}
fn record_watch_roots(kernel: &Kernel, registry: &mut ServiceRegistry, lease: &ProjectLease) {
    let roots = kernel
        .refresh_service_snapshot()
        .and_then(|selected| selected.configured_roots());
    let recorded = roots.and_then(|roots| {
        registry.record_watch_roots(lease, &roots.into_values().collect::<Vec<_>>())
    });
    if recorded.is_err() {
        let _ = registry.invalidate_watch_roots(lease);
    }
}

fn observe_maintenance(kernel: &Kernel) -> airk_core::service::MaintenanceObservation {
    let result = kernel.maintain_service_collection();
    airk_core::service::MaintenanceObservation {
        pending_incarnations: result.as_ref().ok().map(|report| report.pending),
        removed_files: result.as_ref().map_or(0, |report| report.removed_files),
        failed: result.is_err(),
        observed_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
    }
}
async fn query(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(grant): Option<ConnectionGrant> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    ensure!(
        !state.closing.load(Ordering::Acquire),
        "project ownership is draining"
    );
    let session = state
        .broker
        .lock()
        .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
        .redeem(
            &grant,
            state.identity,
            local_ipc::caller_identity()?,
            Instant::now(),
            Duration::from_secs(60),
        )?;
    let mut tools = match state.tools.session_with_client(grant.client).await {
        Ok(tools) => tools,
        Err(error) => {
            state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .disconnect(state.identity, session, Instant::now())?;
            return Err(error);
        }
    };
    let result = async {
        local_ipc::write_message(
            &mut stream,
            &protocol::Accepted {
                identity: state.identity,
            },
            Duration::from_secs(5),
        )
        .await?;
        let authorized_until = Instant::now() + Duration::from_secs(60);
        loop {
            let Some(request): Option<protocol::Query> =
                local_ipc::read_message(&mut stream, Duration::from_secs(30)).await?
            else {
                return Ok(());
            };
            ensure!(
                !state.closing.load(Ordering::Acquire) && Instant::now() < authorized_until,
                "project authorization expired"
            );
            ensure!(
                if request.native.is_some() { request.tool.is_empty() && request.arguments.is_empty() } else { !request.tool.is_empty() && request.tool.len() <= 128 },
                "invalid tool name"
            );
            state.storage.check()?;
            let admission = state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .begin_query(state.identity, session, MAX_FRAME_BYTES, Instant::now());
            let request_id = match admission {
                Ok(request) => request,
                Err(error) => {
                    let mut reply = admission_rejection(error);
                    reply.status_receipt = tools.status_receipt();
                    local_ipc::write_message(&mut stream, &reply, Duration::from_secs(5)).await?;
                    if reply.rebind { return Ok(()); }
                    continue;
                }
            };
            tools.restore_status_receipt(request.status_receipt);
            // The permit and revision materialization survive transport cancellation.
            let native_read = request.native.is_some();
            let response = match tokio::time::timeout(Duration::from_secs(125), async {
                match request.native {
                    Some(native) => tools.read_native(native, state.identity.project).await,
                    None => {
                        if request.tool == "airk_status" {
                            tools.set_runtime(crate::runtime_metadata::bundle(&state.control, state.identity, request.frontend).await);
                            tools.set_work(crate::runtime_metadata::work(&state.control, state.identity).await);
                        }
                        Ok(serde_json::to_value(tools.call(request.tool, request.arguments).await?)?)
                    },
                }
            }).await {
                Ok(response) => response,
                Err(_) => {
                    // Cancelling a protocol future does not stop accepted native
                    // work or release its object lease. Exit this lock-bearing
                    // worker; only the supervisor's confirmed reap frees capacity.
                    std::process::exit(75);
                }
            };
            let value = response.unwrap_or_else(|error| {
                if native_read { serde_json::json!({"error":{"code":"native_read_failed","message":format!("{error:#}").chars().take(1024).collect::<String>()}}) }
                else { serde_json::json!({"content":[{"type":"text","text":"AIRK read-only tool failed or is unavailable."}],"isError":true}) }
            });
            let reply = protocol::QueryResult { result: value, status_receipt: tools.status_receipt(), rebind: false };
            let result = local_ipc::write_message(&mut stream,&reply,Duration::from_secs(5)).await;
            state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .finish_query(state.identity, request_id)?;
            result?;
        }
    }
    .await;
    state
        .broker
        .lock()
        .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
        .disconnect(state.identity, session, Instant::now())?;
    tools.close().await?;
    result
}

fn admission_rejection(error: airk_core::service::AdmissionError) -> protocol::QueryResult {
    let busy = error == airk_core::service::AdmissionError::Busy;
    let detail = serde_json::json!({"error":{"code":"direct_query_admission", "reason":error,
        "query_submitted":false,"retryable":busy,"retry_after_ms":if busy {Some(25)} else {None},
        "fresh_brokerage_required":!busy}});
    protocol::QueryResult {
        result: serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true}),
        status_receipt: Default::default(),
        rebind: !busy,
    }
}

fn execute(
    kernel: &Kernel,
    kind: JobKind,
    job: airk_core::identity::Sha256Digest,
) -> Result<JobOutcome> {
    use airk_core::service_progress::WorkPhase;
    airk_kernel::service_execution::progress_phase(match kind {
        JobKind::Index | JobKind::Reconcile | JobKind::Rebuild => WorkPhase::Discovering,
        JobKind::Vectors | JobKind::ModelSetup => WorkPhase::Embedding,
        JobKind::Configure => WorkPhase::Starting,
        JobKind::Inspect | JobKind::PlanInspect | JobKind::Repair | JobKind::Verify => {
            WorkPhase::Verifying
        }
        JobKind::Recover => WorkPhase::Recovering,
        JobKind::Maintain => WorkPhase::Retiring,
    });
    if kind == JobKind::Recover {
        return Ok(JobOutcome::Succeeded {
            revision: kernel.recover_service_publication()?,
        });
    }
    if kind == JobKind::Maintain {
        return Ok(JobOutcome::Succeeded { revision: None });
    }
    if kind == JobKind::Vectors {
        let selected = retry_semantic_turn(
            || kernel.refresh_service_snapshot(),
            Duration::from_secs(30),
        )?;
        return semantic_publication(
            || selected.service_enrichment_turn().map(|_| ()),
            || {
                let publication = selected.status()?.publication;
                Ok((
                    publication.clone(),
                    JobOutcome::Succeeded {
                        revision: publication.map(|value| value.token),
                    },
                ))
            },
            Duration::from_secs(30),
        );
    }
    let selected =
        kernel
            .refresh_service_snapshot()
            .context(airk_kernel::diagnostics::IndexFailureStage(
                "select_snapshot",
            ))?;
    let kernel = &selected;
    match kind {
        JobKind::Index | JobKind::Reconcile => {
            kernel.index_service_sources()?;
        }
        JobKind::Vectors => unreachable!("vector turns use publication-aware retry"),
        JobKind::ModelSetup => {
            // Re-registration preserves an explicit text-only preference.
            if kernel.vectors_enabled()? {
                kernel.install_service_default_vector_model()?;
            }
            return Ok(JobOutcome::Succeeded { revision: None });
        }
        JobKind::Verify => {
            let mut after = None;
            let mut revision = None;
            loop {
                airk_kernel::service_execution::checkpoint()?;
                let report = kernel.diagnose_current(revision, after.as_deref(), 32, 64 << 20)?;
                ensure!(
                    report.failed_objects == 0 && report.skipped_large == 0,
                    "retained verification incomplete"
                );
                revision = Some(report.publication.token);
                if report.complete {
                    break;
                }
                after = Some(
                    report
                        .next_after
                        .context("integrity continuation missing")?,
                );
            }
        }
        JobKind::Recover
        | JobKind::Configure
        | JobKind::Maintain
        | JobKind::Inspect
        | JobKind::PlanInspect
        | JobKind::Repair => {
            unreachable!("handled outside ordinary execution")
        }
        JobKind::Rebuild => {
            return Ok(JobOutcome::Succeeded {
                revision: Some(kernel.rebuild_service(job)?),
            });
        }
    }
    Ok(JobOutcome::Succeeded {
        revision: kernel
            .status()?
            .publication
            .map(|publication| publication.token),
    })
}

fn reconcile_semantic_setup(
    kernel: &Kernel,
    registry: &mut airk_store::service::registry::ServiceRegistry,
    lease: &airk_store::service::ProjectLease,
    engine: airk_core::identity::Sha256Digest,
) -> Result<()> {
    let project = lease.identity().0;
    let snapshot = kernel.inspect_service(project)?;
    if snapshot.vectors_enabled == Some(true)
        && snapshot
            .semantic
            .as_ref()
            .is_some_and(|s| s.state == airk_core::readiness::SemanticState::Disabled)
        && let Some(settings) = snapshot.settings_digest
    {
        registry.reconcile_semantic_setup(lease, &format!("{engine}:{settings}"))?;
    }
    Ok(())
}

pub async fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("missing private project initialization")?;
    local_ipc::prepare_root(&init.runtime)?;
    let mut lease = ProjectLease::acquire(&init.runtime, init.project)?;
    let mut registry = ServiceRegistry::open(&init.runtime, init.service)?;
    let registration = registry.get(init.project)?.registration;
    airk_kernel::service_projects::validate_retained(&registration)?;
    let epoch = registry.admit_worker(&mut lease)?;
    let lease = Arc::new(lease);
    ensure!(
        PROCESS_LEASE.set(lease.clone()).is_ok(),
        "project role already owns a process lease"
    );
    let identity = WorkerIdentity {
        service: init.service,
        worker: init.worker,
        project: init.project,
        fencing_generation: epoch,
    };
    let role = protocol::role(init.worker);
    let mut listener = Listener::bind(&local_ipc::endpoint(&init.runtime, &role)?)?;
    service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        &Reply::Ready { identity, role },
    )?;
    // A late child may receive a buffered Init after its parent died. It cannot touch
    // collection state until the service accounts its lock-bearing generation.
    let Some(Command::Activate {
        identity: accepted,
        allowance,
        pool,
    }) = service_ipc::read_frame(&mut std::io::stdin().lock())?
    else {
        anyhow::bail!("project owner was not activated")
    };
    ensure!(
        accepted == identity
            && (1..=4).contains(&allowance.concurrent_queries)
            && allowance.response_bytes == allowance.concurrent_queries * MAX_FRAME_BYTES,
        "project activation identity or allowance mismatch"
    );
    crate::embedding_pool_client::install(&init.runtime, identity, pool.map(|access| *access))?;
    let storage = airk_store::service::data_scope::bind(
        Path::new(&registration.data_root),
        &init.runtime,
        registry.incarnation()?,
    )?;
    #[cfg(unix)]
    crate::unix_guardian::install(
        &init.runtime,
        registry.incarnation()?,
        identity,
        &lease,
        &std::env::current_exe()?,
    )?;
    let kernel = Kernel::open_service_collection_for_diagnostics(
        Path::new(&registration.root),
        Some((&registration.data_root).into()),
        &registration.collection,
    )?
    .with_service_freshness(identity)?
    .with_service_models(&init.runtime, crate::bundled_assets::default_plan()?)?;
    airk_kernel::service_execution::install(Path::new(&registration.data_root), identity)?;
    let receipt = kernel.service_rebuild_receipt()?;
    registry.recover_jobs_with_rebuild_receipt(&lease, receipt.as_ref())?;
    record_watch_roots(&kernel, &mut registry, &lease);
    let _ = reconcile_semantic_setup(&kernel, &mut registry, &lease, init.engine);
    // Recovery is durable desired work, admitted by the service rather than an
    // unaccounted startup replay. Failed recovery leaves the owner reachable.
    if kernel.service_recovery_required().unwrap_or(false)
        && !registry.source_failure_blocked(init.project)?
    {
        registry.enqueue_internal(
            init.project,
            init.service,
            JobKind::Recover,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
    }
    let pending_retirements = kernel.pending_service_retirements()?;
    if pending_retirements > 0 {
        registry.record_maintenance(
            &lease,
            &airk_core::service::MaintenanceObservation {
                pending_incarnations: Some(pending_retirements),
                removed_files: 0,
                failed: false,
                observed_unix_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
            },
        )?;
    }
    let mut broker = Broker::new(init.service, BrokerLimits::default())?;
    broker.register(
        identity,
        allowance,
        Instant::now(),
        Duration::from_secs(300),
    )?;
    let closing = Arc::new(AtomicBool::new(false));
    let _stop = StopOnDrop(closing.clone());
    let activity = Arc::new(Notify::new());
    let busy = Arc::new(AtomicBool::new(false));
    let maintenance_waiting = Arc::new(AtomicBool::new(false));
    let job_storage = storage.clone();
    let state = Arc::new(State {
        control: init.runtime.clone(),
        identity,
        storage,
        broker: Mutex::new(broker),
        tools: airk_mcp::local::LocalTools::with_client(
            kernel.clone(),
            airk_core::usage::ClientFamily::Unknown,
        )
        .await?,
        closing: closing.clone(),
    });
    service_ipc::write_frame(&mut std::io::stdout().lock(), &Reply::Activated)?;
    let (sender, mut commands) = mpsc::channel(8);
    let control_closing = closing.clone();
    let control_activity = activity.clone();
    std::thread::Builder::new()
        .name("airk-project-control".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Ok(Some(command)) = service_ipc::read_frame::<_, Command>(&mut input) {
                if sender.blocking_send(command).is_err() {
                    break;
                }
            }
            control_closing.store(true, Ordering::Release);
            control_activity.notify_one();
        })?;
    let job_closing = closing.clone();
    let job_activity = activity.clone();
    let job_busy = busy.clone();
    let job_lease = lease.clone();
    let (work_sender, work_receiver) =
        std::sync::mpsc::sync_channel::<airk_core::service_resources::WorkAllowance>(1);
    let work_status = Arc::new(Mutex::new(WorkState::default()));
    let job_status = work_status.clone();
    let freshness_kernel = kernel.clone();
    let jobs = tokio::task::spawn_blocking(move || -> Result<()> {
        while !job_closing.load(Ordering::Acquire) {
            let allowance = match work_receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(allowance) => allowance,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            // Unregister can commit between supervisor observation and dispatch.
            // Never claim inactive cleanup with a query/index owner.
            registry.get(identity.project)?;
            ensure!(
                !registry
                    .configuration_work(allowance.job)?
                    .is_some_and(|work| work.request.configuration.is_derived_cleanup()),
                "cleanup requires a distinct inactive owner"
            );
            if let Some(job) = registry.claim_job_id(&job_lease, allowance.job)? {
                let mut completed_maintenance = None;
                use airk_store::service::registry::RecoveryCondition;
                let mut recovery_condition = RecoveryCondition::Delay;
                let outcome = if registry.cancellation_requested(job.id)? {
                    Ok(JobOutcome::Canceled)
                } else {
                    airk_kernel::service_execution::run(allowance.clone(), || {
                        job_storage.check()?;
                        airk_kernel::service_execution::checkpoint()?;
                        if job.kind == JobKind::Configure {
                            let mut work = registry
                                .configuration_work(job.id)?
                                .context("configuration work missing")?;
                            ensure!(
                                work.engine == init.engine,
                                airk_kernel::service_configuration::ConfigurationChanged
                            );
                            if work.request.prepare {
                                work.request.validate()?;
                                let observed = kernel.inspect_service(identity.project)?;
                                work.request.incarnation = observed.incarnation;
                                work.request.settings = observed
                                    .settings_digest
                                    .context("settings inspection unavailable")?;
                                work.request.prepare = false;
                            }
                            kernel
                                .validate_service_configuration(identity.project, &work.request)?;
                            if let Some(roots) =
                                kernel.service_configuration_roots(&work.request)?
                            {
                                registry.record_watch_roots(&job_lease, &roots)?;
                            }
                            let result = kernel
                                .configure_service_project(identity.project, &work.request)?;
                            registry.record_configuration_result(&job_lease, job.id, &result)?;
                            if let Ok(snapshot) = kernel.inspect_service(identity.project) {
                                let observed = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)?
                                    .as_millis()
                                    .try_into()?;
                                let _ = registry.record_inspection(
                                    &job_lease, identity, job.id, observed, &snapshot,
                                );
                            }
                            return Ok(JobOutcome::Succeeded { revision: None });
                        }
                        let kind = if job.kind == JobKind::Repair {
                            use airk_core::service_maintenance::{PlanError, RepairOperation};
                            let approved = registry.repair_work(&job_lease, job.id, init.engine)?;
                            let observed = kernel.inspect_service(identity.project)?;
                            ensure!(
                                observed.guard_digest()? == approved.expected.guard_digest()?,
                                PlanError::Stale
                            );
                            if matches!(
                                approved.operation,
                                RepairOperation::Reconcile | RepairOperation::Rebuild
                            ) {
                                ensure!(
                                    observed.source_state
                                        == airk_core::service_inspection::SourceState::Available,
                                    PlanError::SourceUnavailable
                                );
                                kernel
                                    .refresh_service_snapshot()?
                                    .require_service_sources()
                                    .map_err(|_| PlanError::SourceUnavailable)?;
                            }
                            if approved.operation == RepairOperation::Vectors {
                                // The approved regeneration supersedes an older
                                // continuation. A crash/failure before its success
                                // must not silently resume that old chain.
                                registry.set_background_pending(
                                    &job_lease,
                                    JobKind::Vectors,
                                    false,
                                )?;
                                kernel
                                    .refresh_service_snapshot()?
                                    .begin_service_vector_regeneration(job.id)?;
                            }
                            approved
                                .operation
                                .job_kind()
                                .context("unresolved approved repair operation")?
                        } else {
                            job.kind
                        };
                        if matches!(kind, JobKind::Index | JobKind::Reconcile | JobKind::Rebuild) {
                            let selected = kernel.refresh_service_snapshot()?;
                            selected.require_service_sources()?;
                            let roots = selected
                                .configured_roots()?
                                .into_values()
                                .collect::<Vec<_>>();
                            registry.record_watch_roots(&job_lease, &roots)?;
                        }
                        if matches!(kind, JobKind::Inspect | JobKind::PlanInspect) {
                            airk_kernel::service_execution::progress_phase(
                                airk_core::service_progress::WorkPhase::Verifying,
                            );
                            let snapshot = kernel.inspect_service(identity.project)?;
                            let observed_unix_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_millis()
                                .try_into()?;
                            registry.record_inspection(
                                &job_lease,
                                identity,
                                job.id,
                                observed_unix_ms,
                                &snapshot,
                            )?;
                            return Ok(JobOutcome::Succeeded {
                                revision: snapshot.publication,
                            });
                        }
                        let outcome = execute(&kernel, kind, job.id)?;
                        // Successful changes refresh console metadata while this exact job
                        // still owns its fenced lease. Refreshing the UI must not require
                        // a second Inspect job to see new counts or semantic readiness.
                        if matches!(
                            kind,
                            JobKind::Index
                                | JobKind::Reconcile
                                | JobKind::Vectors
                                | JobKind::ModelSetup
                                | JobKind::Rebuild
                                | JobKind::Repair
                                | JobKind::Recover
                        ) && matches!(outcome, JobOutcome::Succeeded { .. })
                            && let Ok(snapshot) = kernel.inspect_service(identity.project)
                        {
                            let observed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_millis()
                                .try_into()?;
                            let _ = registry.record_inspection(
                                &job_lease, identity, job.id, observed, &snapshot,
                            );
                        }
                        airk_kernel::service_execution::progress_phase(
                            airk_core::service_progress::WorkPhase::Retiring,
                        );
                        let maintenance = observe_maintenance(&kernel);
                        let _ = registry.record_maintenance(&job_lease, &maintenance);
                        let failed = kind == JobKind::Maintain && maintenance.failed;
                        completed_maintenance = Some(maintenance.clone());
                        job_status
                            .lock()
                            .map_err(|_| anyhow::anyhow!("work status unavailable"))?
                            .maintenance = Some(maintenance);
                        if failed {
                            Ok(JobOutcome::Failed {
                                code: "owned_retirement_failed".into(),
                            })
                        } else {
                            Ok(outcome)
                        }
                    })
                }
                .unwrap_or_else(|error| {
                    for cause in error.chain() {
                        if let Some(pressure) =
                            cause.downcast_ref::<airk_store::quota::QuotaPressure>()
                        {
                            recovery_condition = RecoveryCondition::Quota {
                                required: pressure.required_bytes,
                                scope: pressure.scope,
                            };
                            break;
                        }
                        if let Some(pressure) =
                            cause.downcast_ref::<airk_store::capacity::StoragePressure>()
                        {
                            recovery_condition = RecoveryCondition::Capacity {
                                required: pressure.required_bytes,
                            };
                            break;
                        }
                    }
                    if error.is::<airk_kernel::service_projects::SourceUnavailable>() {
                        recovery_condition = RecoveryCondition::Source;
                    } else if error.is::<crate::embedding_pool_client::HelperUnavailable>() {
                        recovery_condition = RecoveryCondition::Helper;
                    } else if matches!(
                        airk_store::capacity::storage_pressure_code(&error),
                        Some("filesystem_storage_full" | "sqlite_storage_full")
                    ) {
                        recovery_condition = RecoveryCondition::Capacity {
                            required: 16 * 1024 * 1024,
                        };
                    }
                    if error
                        .downcast_ref::<airk_kernel::service_execution::Canceled>()
                        .is_some()
                        || error.downcast_ref::<airk_core::service_maintenance::PlanError>()
                            == Some(&airk_core::service_maintenance::PlanError::Canceled)
                    {
                        JobOutcome::Canceled
                    } else {
                        JobOutcome::Failed {
                            code: if let Some(code) =
                                error.downcast_ref::<airk_core::service_maintenance::PlanError>()
                            {
                                return JobOutcome::Failed {
                                    code: code.to_string(),
                                };
                            } else if let Some(code) =
                                airk_store::capacity::storage_pressure_code(&error)
                            {
                                code
                            } else if error.is::<crate::embedding_pool_client::HelperUnavailable>()
                            {
                                "helper_unavailable"
                            } else if job.kind == JobKind::Vectors
                                && Kernel::service_enrichment_contention(&error)
                            {
                                "semantic_contention_retry_exhausted"
                            } else if error.is::<airk_store::current::CurrentReadContention>()
                                || error.is::<airk_store::current::CurrentOpenContention>()
                            {
                                "current_database_admission_busy"
                            } else if error.is::<airk_store::quota::QuotaContention>() {
                                "quota_ledger_busy"
                            } else if error.is::<airk_kernel::service_execution::WorkStalled>() {
                                "work_stalled"
                            } else if error.is::<airk_kernel::service_projects::SourceUnavailable>()
                            {
                                "source_unavailable"
                            } else if error.is::<airk_store::service::registry::SourceBoundary>() {
                                "protected_source_boundary"
                            } else if error
                                .is::<airk_kernel::service_inspection::InspectionChanged>()
                            {
                                if job.kind == JobKind::Repair {
                                    "maintenance_plan_stale"
                                } else {
                                    "inspection_changed"
                                }
                            } else if error
                                .is::<airk_kernel::service_configuration::ConfigurationChanged>()
                            {
                                "configuration_settings_or_engine_changed"
                            } else if job.kind == JobKind::Configure {
                                "configuration_failed_reinspect"
                            } else {
                                let cause = airk_kernel::diagnostics::indexing_failure_code(&error);
                                return JobOutcome::Failed {
                                    code: match error
                                        .downcast_ref::<airk_kernel::current::RecoveryFailure>()
                                    {
                                        Some(stage) => format!(
                                            "{}_{}",
                                            stage.0,
                                            cause.as_deref().unwrap_or("failed")
                                        ),
                                        None => cause
                                            .unwrap_or_else(|| "project_operation_failed".into()),
                                    },
                                };
                            }
                            .into(),
                        }
                    }
                });
                // Persist the next phase before acknowledging this one. These
                // bounded flags survive public queue saturation and owner exit.
                if matches!(outcome, JobOutcome::Succeeded { .. }) {
                    if job.kind == JobKind::ModelSetup {
                        registry.set_background_pending(&job_lease, JobKind::Index, true)?;
                    } else if matches!(
                        job.kind,
                        JobKind::Index
                            | JobKind::Reconcile
                            | JobKind::Rebuild
                            | JobKind::Repair
                            | JobKind::Recover
                            | JobKind::Vectors
                    ) {
                        let readiness =
                            airk_kernel::service_execution::with_database_admission(|| {
                                Ok(kernel
                                    .refresh_service_snapshot()?
                                    .status()?
                                    .semantic_readiness)
                            })?;
                        registry.set_background_pending(
                            &job_lease,
                            JobKind::Vectors,
                            matches!(
                                readiness.state,
                                airk_core::readiness::SemanticState::Pending
                                    | airk_core::readiness::SemanticState::Partial
                            ),
                        )?;
                    }
                } else if job.kind == JobKind::Vectors
                    || matches!(outcome, JobOutcome::Canceled)
                        && matches!(
                            job.kind,
                            JobKind::Index
                                | JobKind::Reconcile
                                | JobKind::Rebuild
                                | JobKind::Repair
                                | JobKind::Recover
                        )
                {
                    // A failed/canceled semantic phase requires explicit retry.
                    // Source failures leave independent older enrichment intact.
                    registry.set_background_pending(&job_lease, JobKind::Vectors, false)?;
                }
                record_watch_roots(&kernel, &mut registry, &job_lease);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    .try_into()?;
                let deferred = if let JobOutcome::Failed { code } = &outcome {
                    registry.defer_job_failure(
                        &job_lease,
                        job.id,
                        code,
                        &recovery_condition,
                        now_ms,
                    )?
                } else {
                    false
                };
                if !deferred {
                    registry.complete_job_with_maintenance_at(
                        &job_lease,
                        job.id,
                        outcome.clone(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_millis()
                            .try_into()?,
                        completed_maintenance.as_ref(),
                    )?;
                    if job.kind == JobKind::Configure {
                        let _ = reconcile_semantic_setup(
                            &kernel,
                            &mut registry,
                            &job_lease,
                            init.engine,
                        );
                    }
                }
                if matches!(outcome, JobOutcome::Succeeded { revision: Some(_) }) {
                    registry.reset_recovery_budget(&identity.project.to_string())?;
                    registry.reset_recovery_budget(&format!("work:{}", identity.project))?;
                    if job.kind == JobKind::Vectors {
                        registry.reset_recovery_budget("helper")?;
                    }
                    let _ =
                        reconcile_semantic_setup(&kernel, &mut registry, &job_lease, init.engine);
                }
                airk_kernel::service_execution::progress_phase(
                    airk_core::service_progress::WorkPhase::Finished,
                );
                if let Some(progress) = airk_kernel::service_execution::progress() {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .and_then(|time| u64::try_from(time.as_millis()).ok());
                    if let Some(now_ms) = now_ms {
                        let _ = registry.observe_progress(identity, &progress, now_ms);
                    }
                }
            }
            {
                let mut status = job_status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("work status unavailable"))?;
                status.active = None;
                status.finished = Some(allowance.id);
            }
            job_busy.store(false, Ordering::Release);
            job_activity.notify_one();
        }
        Ok(())
    });
    let permits = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    let mut idle = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        if closing.load(Ordering::Acquire) {
            break;
        }
        tokio::select! {
            command=commands.recv()=>{
                match command {
                    Some(Command::Renew{identity:expected})=>{
                        let renewed=expected==identity&&airk_kernel::service_execution::renew(identity).is_ok()&&state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.renew(identity,Instant::now(),Duration::from_secs(300)).is_ok();
                        service_ipc::write_frame(&mut std::io::stdout().lock(),&if renewed{Reply::Renewed}else{Reply::Failed})?;
                    },
                    Some(Command::ConfigurePool{identity:expected,pool})=>{
                        let accepted=expected==identity && crate::embedding_pool_client::update(pool.map(|access| *access)).is_ok();
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &if accepted{Reply::PoolConfigured}else{Reply::Failed})?;
                    },
                    Some(Command::RunJob{identity:expected, allowance})=>{
                        let accepted=expected==identity && allowance.identity==identity && allowance.validate().is_ok()
                            && !busy.swap(true, Ordering::AcqRel);
                        let accepted=if accepted {
                            airk_kernel::service_execution::activate(&allowance)?;
                            {let mut status=work_status.lock().map_err(|_|anyhow::anyhow!("work status unavailable"))?;status.active=Some(allowance.id);status.finished=None;}
                            work_sender.try_send(allowance).is_ok()
                        }else{false};
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &if accepted{Reply::WorkAccepted}else{Reply::Failed})?;
                        idle=Instant::now();
                    },
                    Some(Command::WorkStatus{identity:expected,control,keep_for_maintenance,freshness})=>{
                        if expected==identity {freshness_kernel.observe_service_freshness(identity, freshness.map(|sample| *sample))?;}
                        if expected==identity {maintenance_waiting.store(keep_for_maintenance,Ordering::Release);}
                        if expected==identity && let Some(control)=control {airk_kernel::service_execution::control(identity,control.grant,control.paused,control.canceled)?;}
                        let status=work_status.lock().map_err(|_|anyhow::anyhow!("work status unavailable"))?;
                        let reply=if expected==identity{Reply::WorkStatus{observation:protocol::WorkObservation{active:status.active,finished:status.finished,maintenance:status.maintenance.clone(),progress:airk_kernel::service_execution::progress().map(Box::new),waiting_for:status.active.and_then(|_|airk_kernel::service_execution::waiting_for())}}}else{Reply::Failed};
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                    },
                    Some(Command::Grant{caller})=>{
                        let grant=state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.issue(identity,caller,Instant::now(),Duration::from_secs(15));
                        let reply=match grant{Ok(grant)=>Reply::Grant{grant},Err(_)=>Reply::Failed};
                        service_ipc::write_frame(&mut std::io::stdout().lock(),&reply)?;
                        idle=Instant::now();
                    },
                    Some(Command::Stop|Command::Activate{..})|None=>break,
                }
            },
            stream=listener.accept()=>{
                let stream=match stream{Ok(stream)=>stream,Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some()=>continue,Err(error)=>return Err(error)};
                if let Ok(permit)=permits.clone().try_acquire_owned(){let state=state.clone();connections.spawn(async move{let _permit=permit;query(stream,state).await});idle=Instant::now();}
            },
            _=activity.notified()=>{idle=Instant::now();},
            _=connections.join_next(),if !connections.is_empty()=>{idle=Instant::now();},
            _=tick.tick()=>{
                if busy.load(Ordering::Acquire) && airk_kernel::service_execution::watchdog() {
                    // A native call that never reaches a safe boundary cannot be
                    // canceled by dropping its future. The parent retains the
                    // work allowance until this process actually exits.
                    std::process::exit(76);
                }
                if jobs.is_finished(){break;}
                if busy.load(Ordering::Acquire)||maintenance_waiting.load(Ordering::Acquire){idle=Instant::now();}
                if connections.is_empty()&&idle.elapsed()>Duration::from_secs(15){break;}
                // A generation has a bounded lifetime; replacement requires fresh brokerage.
                if state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.status(Instant::now()).draining_projects>0 {break;}
            }
        }
    }
    closing.store(true, Ordering::Release);
    drop(work_sender);
    // Idle client reads must not hold an intentionally stopped owner open.
    // Blocking materialization retains its inner admission permit until actual
    // completion; process/job ownership is still held through the native drain.
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    jobs.await??;
    let mut state = Arc::try_unwrap(state)
        .map_err(|_| anyhow::anyhow!("project query handlers survived drain"))?;
    state.tools.close().await?;
    drop(lease);
    Ok(())
}

#[cfg(test)]
mod admission_tests {
    #[test]
    fn semantic_retry_after_commit_only_repeats_observation() -> anyhow::Result<()> {
        let turns = std::cell::Cell::new(0);
        let reads = std::cell::Cell::new(0);
        let result = super::semantic_publication(
            || {
                turns.set(turns.get() + 1);
                // The batch committed, but its follow-up report was excluded.
                Err(airk_store::current::CurrentReadContention.into())
            },
            || {
                reads.set(reads.get() + 1);
                match reads.get() {
                    1 => Ok((0, 0)),
                    2 => Err(airk_store::current::CurrentReadContention.into()),
                    _ => Ok((1, 42)),
                }
            },
            std::time::Duration::from_secs(1),
        )?;
        assert_eq!(result, 42);
        assert_eq!(turns.get(), 1);
        assert_eq!(reads.get(), 3);
        Ok(())
    }
    #[test]
    fn semantic_turn_waits_for_native_lock_release_and_bounds_persistent_contention()
    -> anyhow::Result<()> {
        use std::{
            fs::OpenOptions,
            time::{Duration, Instant},
        };
        let owner = tempfile::tempdir()?;
        let path = owner.path().join("owned-semantic-turn.lock");
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        held.try_lock()?;
        let (seen, waiting) = std::sync::mpsc::sync_channel(1);
        let release = std::thread::spawn(move || {
            let _ = waiting.recv();
            std::thread::sleep(Duration::from_millis(100));
            drop(held);
        });
        let mut attempts = 0;
        let result = super::retry_semantic_turn(
            || {
                attempts += 1;
                let lock = OpenOptions::new().read(true).write(true).open(&path)?;
                if let Err(error) = lock.try_lock() {
                    let _ = seen.try_send(());
                    return Err(error.into());
                }
                Ok(42)
            },
            Duration::from_secs(2),
        )?;
        release.join().unwrap();
        assert_eq!(result, 42);
        assert!(attempts >= 2);
        let held = OpenOptions::new().read(true).write(true).open(&path)?;
        held.try_lock()?;
        let started = Instant::now();
        let result = super::retry_semantic_turn(
            || {
                let lock = OpenOptions::new().read(true).write(true).open(&path)?;
                lock.try_lock()?;
                Ok(())
            },
            Duration::from_millis(50),
        );
        assert!(result.unwrap_err().is::<std::fs::TryLockError>());
        assert!(started.elapsed() < Duration::from_secs(2));
        let mut failed = 0;
        let result: anyhow::Result<()> = super::retry_semantic_turn(
            || {
                failed += 1;
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into())
            },
            Duration::from_secs(1),
        );
        assert!(result.is_err());
        assert_eq!(failed, 1);
        Ok(())
    }
    #[test]
    fn temporary_admission_keeps_the_session_and_expired_authority_requires_rebinding() {
        use airk_core::service::AdmissionError;
        for reason in [
            AdmissionError::Busy,
            AdmissionError::Expired,
            AdmissionError::StaleIdentity,
        ] {
            let reply = super::admission_rejection(reason);
            let detail: serde_json::Value =
                serde_json::from_str(reply.result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(detail["error"]["query_submitted"], false);
            assert_eq!(
                detail["error"]["reason"],
                serde_json::to_value(reason).unwrap()
            );
            assert_eq!(reply.rebind, reason != AdmissionError::Busy);
            assert_eq!(detail["error"]["retryable"], reason == AdmissionError::Busy);
            assert_eq!(
                detail["error"]["retry_after_ms"],
                if reason == AdmissionError::Busy {
                    serde_json::json!(25)
                } else {
                    serde_json::Value::Null
                }
            );
        }
    }
}
