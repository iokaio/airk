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
    identity: WorkerIdentity,
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
                !request.tool.is_empty() && request.tool.len() <= 128,
                "invalid tool name"
            );
            let request_id = state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .begin_query(state.identity, session, MAX_FRAME_BYTES, Instant::now())?;
            // The permit and revision materialization survive transport cancellation.
            let response = match tokio::time::timeout(Duration::from_secs(125),
                state.tools.call(request.tool, request.arguments)).await {
                Ok(response) => response,
                Err(_) => {
                    // Cancelling a protocol future does not stop accepted native
                    // work or release its object lease. Exit this lock-bearing
                    // worker; only the supervisor's confirmed reap frees capacity.
                    std::process::exit(75);
                }
            };
            let value = response.and_then(|response|Ok(serde_json::to_value(response)?))
                .unwrap_or_else(|_|serde_json::json!({"content":[{"type":"text","text":"AIRK read-only tool failed or is unavailable."}],"isError":true}));
            let result = local_ipc::write_message(&mut stream,&value,Duration::from_secs(5)).await;
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
    result
}

fn execute(
    kernel: &Kernel,
    kind: JobKind,
    job: airk_core::identity::Sha256Digest,
) -> Result<JobOutcome> {
    if kind == JobKind::Recover {
        return Ok(JobOutcome::Succeeded {
            revision: kernel.recover_service_publication()?,
        });
    }
    if kind == JobKind::Maintain {
        return Ok(JobOutcome::Succeeded { revision: None });
    }
    let selected = kernel.refresh_service_snapshot()?;
    let kernel = &selected;
    match kind {
        JobKind::Index | JobKind::Reconcile => {
            kernel.index()?;
        }
        JobKind::Vectors => {
            kernel.resume_current_enrichment()?;
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
        JobKind::Recover | JobKind::Maintain => unreachable!("handled before read selection"),
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

pub async fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("missing private project initialization")?;
    local_ipc::prepare_root(&init.runtime)?;
    let mut lease = ProjectLease::acquire(&init.runtime, init.project)?;
    let mut registry = ServiceRegistry::open(&init.runtime, init.service)?;
    let registration = registry.get(init.project)?.registration;
    airk_kernel::service_projects::validate(&registration)?;
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
    #[cfg(not(feature = "local-inference"))]
    crate::embedding_pool_client::install(pool.map(|access| *access))?;
    #[cfg(feature = "local-inference")]
    let _ = pool;
    let kernel = Kernel::open_service_collection_for_diagnostics(
        Path::new(&registration.root),
        Some((&registration.data_root).into()),
        &registration.collection,
    )?;
    airk_kernel::service_execution::install(Path::new(&registration.data_root), identity)?;
    let receipt = kernel.service_rebuild_receipt()?;
    registry.recover_jobs_with_rebuild_receipt(&lease, receipt.as_ref())?;
    // Recovery is durable desired work, admitted by the service rather than an
    // unaccounted startup replay. Failed recovery leaves the owner reachable.
    if kernel.service_recovery_required().unwrap_or(false) {
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
    let state = Arc::new(State {
        identity,
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
    let jobs = tokio::task::spawn_blocking(move || -> Result<()> {
        while !job_closing.load(Ordering::Acquire) {
            let allowance = match work_receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(allowance) => allowance,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if let Some(job) = registry.claim_job_id(&job_lease, allowance.job)? {
                let outcome = if registry.cancellation_requested(job.id)? {
                    Ok(JobOutcome::Canceled)
                } else {
                    airk_kernel::service_execution::run(allowance.clone(), || {
                        let outcome = execute(&kernel, job.kind, job.id)?;
                        let maintenance = observe_maintenance(&kernel);
                        let _ = registry.record_maintenance(&job_lease, &maintenance);
                        let failed = job.kind == JobKind::Maintain && maintenance.failed;
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
                    if error
                        .downcast_ref::<airk_kernel::service_execution::Canceled>()
                        .is_some()
                    {
                        JobOutcome::Canceled
                    } else {
                        JobOutcome::Failed {
                            code: "project_operation_failed".into(),
                        }
                    }
                });
                registry.complete_job(&job_lease, job.id, outcome)?;
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
                    Some(Command::WorkStatus{identity:expected,control,keep_for_maintenance})=>{
                        if expected==identity {maintenance_waiting.store(keep_for_maintenance,Ordering::Release);}
                        if expected==identity && let Some(control)=control {airk_kernel::service_execution::control(identity,control.grant,control.paused,control.canceled)?;}
                        let status=work_status.lock().map_err(|_|anyhow::anyhow!("work status unavailable"))?;
                        let reply=if expected==identity{Reply::WorkStatus{observation:protocol::WorkObservation{active:status.active,finished:status.finished,maintenance:status.maintenance.clone()}}}else{Reply::Failed};
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
    while connections.join_next().await.is_some() {}
    jobs.await??;
    let mut state = Arc::try_unwrap(state)
        .map_err(|_| anyhow::anyhow!("project query handlers survived drain"))?;
    state.tools.close().await?;
    drop(lease);
    Ok(())
}
