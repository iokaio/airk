// SPDX-License-Identifier: Apache-2.0
//! Private inactive-project owner. No query endpoint, Kernel or inference session.
use crate::{
    project_protocol::{self as protocol, Command, Reply},
    worker_scope::Residency,
    worker_transport::{FrameSink, FrameSource, StdinFrames, StdoutFrames},
};
use airk_core::{
    service::{JobOutcome, WorkerIdentity},
    service_configuration::ProjectConfiguration,
};
use airk_store::service::{ProjectLease, collection::Collection, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

static PROCESS_LEASE: OnceLock<Arc<ProjectLease>> = OnceLock::new();

/// The cleanup job thread, joined on every return from [`serve`], including
/// early `?` returns: an in-process owner's thread then ends only after its
/// accepted cleanup did, exactly what a process exit proved for a child.
struct CleanupJob {
    closing: Arc<AtomicBool>,
    task: Option<std::thread::JoinHandle<Result<()>>>,
}
impl CleanupJob {
    /// Join a finished or previous job, reporting its own failure.
    fn join(&mut self) -> Result<()> {
        match self.task.take() {
            Some(task) => task
                .join()
                .map_err(|_| anyhow::anyhow!("cleanup thread panicked"))?,
            None => Ok(()),
        }
    }
    fn finished(&self) -> bool {
        self.task.as_ref().is_some_and(|task| task.is_finished())
    }
}
impl Drop for CleanupJob {
    fn drop(&mut self) {
        // A closing owner leaves running work unsettled for the next owner.
        self.closing.store(true, Ordering::Release);
        let _ = self.join();
    }
}

pub fn run() -> Result<()> {
    let mut commands = StdinFrames;
    let init: protocol::Init = FrameSource::<protocol::Init>::recv(&mut commands)?
        .context("missing private cleanup initialization")?;
    serve(
        init,
        Box::new(commands),
        Box::new(StdoutFrames),
        Residency::Process,
    )
}

/// One inactive-project cleanup owner over either control carrier. An
/// in-process owner's caller binds its scope on the calling thread.
pub(crate) fn serve(
    init: protocol::Init,
    mut commands: Box<dyn FrameSource<Command>>,
    mut replies: Box<dyn FrameSink<Reply>>,
    residency: Residency,
) -> Result<()> {
    crate::local_ipc::prepare_root(&init.runtime)?;
    let mut lease = ProjectLease::acquire(&init.runtime, init.project)?;
    let mut registry = ServiceRegistry::open(&init.runtime, init.service)?;
    let epoch = registry.admit_derived_worker(&mut lease)?;
    let registration = registry.validate_inactive_worker(&lease)?;
    // Dropped after the cleanup job joins and its collection closes.
    let _caches = crate::worker_scope::CollectionCaches::release_on_exit(
        &registration.data_root,
        &registration.collection,
    );
    let lease = Arc::new(lease);
    match residency.scope() {
        None => {
            ensure!(
                PROCESS_LEASE.set(lease.clone()).is_ok(),
                "cleanup role already owns a process lease"
            );
        }
        // Held until this owner's thread returns, never by this body alone.
        Some(scope) => scope.retain_lease(lease.clone())?,
    }
    let identity = WorkerIdentity {
        service: init.service,
        worker: init.worker,
        project: init.project,
        fencing_generation: epoch,
    };
    replies.send(Reply::Ready {
        identity,
        role: protocol::role(init.worker),
    })?;
    let Some(Command::Activate {
        identity: accepted,
        pool,
        pool_pending,
        ..
    }) = commands.recv()?
    else {
        anyhow::bail!("cleanup worker was not activated")
    };
    ensure!(
        accepted == identity && pool.is_none() && !pool_pending,
        "cleanup activation identity or role differs"
    );
    match residency.scope() {
        None => {
            airk_kernel::service_execution::install(Path::new(&registration.data_root), identity)?
        }
        Some(scope) => scope
            .execution
            .install(Path::new(&registration.data_root), identity)?,
    }
    // A stop requested during activation is observed here, before any
    // recovery or acknowledgement; nothing is settled by leaving now.
    airk_kernel::service_execution::checkpoint()?;
    let collection = Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?;
    registry.recover_jobs(&lease)?;
    airk_kernel::service_execution::checkpoint()?;
    replies.send(Reply::Activated)?;
    let stuck = || {
        residency
            .scope()
            .is_some_and(|scope| scope.execution.stuck().is_some())
    };
    let status = Arc::new(Mutex::new(protocol::WorkObservation {
        waiting_for: None,
        failure: None,
        integrity_observation_failure: None,
        active: None,
        finished: None,
        maintenance: None,
        progress: None,
    }));
    let closing = Arc::new(AtomicBool::new(false));
    let mut job = CleanupJob {
        closing: closing.clone(),
        task: None,
    };
    let mut checkpoint_sealed = false;
    while let Some(command) = commands.recv()? {
        let reply = match command {
            Command::Checkpoint {
                identity: expected,
                operation_id,
                seal,
            } if expected == identity => {
                if job.finished() {
                    job.join()?;
                }
                let status = status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?;
                let durable_boundary = job.task.is_none() && status.active.is_none();
                if seal && durable_boundary {
                    checkpoint_sealed = true;
                }
                Reply::Checkpoint {
                    observation: protocol::WorkerCheckpointObservation {
                        schema: 1,
                        identity,
                        operation_id,
                        engine_sha256: init.engine,
                        admission_closed: true,
                        sealed: checkpoint_sealed,
                        active_work_grant: status.active,
                        active_queries: 0,
                        durable_boundary,
                    },
                }
            }
            Command::Renew { identity: expected } if expected == identity => {
                airk_kernel::service_execution::renew(identity)?;
                Reply::Renewed
            }
            Command::RunJob {
                identity: expected,
                allowance,
            } if !checkpoint_sealed
                && !stuck()
                && expected == identity
                && allowance.identity == identity
                && allowance.validate().is_ok() =>
            {
                let busy = status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?
                    .active
                    .is_some();
                if busy {
                    Reply::Failed
                } else {
                    job.join()?;
                    airk_kernel::service_execution::activate(&allowance)?;
                    {
                        let mut status = status
                            .lock()
                            .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?;
                        status.active = Some(allowance.id);
                        status.finished = None;
                        status.failure = None;
                    }
                    let runtime = init.runtime.clone();
                    let lease = lease.clone();
                    let collection = collection.clone();
                    let status = status.clone();
                    let closing = closing.clone();
                    let engine = init.engine;
                    let scope = residency.scope().cloned();
                    job.task = Some(std::thread::spawn(move || {
                        let _bound = scope
                            .as_ref()
                            .map(crate::worker_scope::WorkerScope::bind_thread);
                        let mut registry = ServiceRegistry::open(&runtime, identity.service)?;
                        if let Some(job) = registry.claim_job_id(&lease, allowance.job)? {
                            let mut failure_detail = None;
                            let mut drained = false;
                            let outcome =
                                airk_kernel::service_execution::run(allowance.clone(), || {
                                    registry.validate_inactive_worker(&lease)?;
                                    let work = registry
                                        .configuration_work(job.id)?
                                        .context("cleanup work missing")?;
                                    ensure!(
                                        work.engine == engine,
                                        "cleanup engine changed; reinspect"
                                    );
                                    airk_kernel::service_execution::checkpoint()?;
                                    let result = match work.request.configuration {
                                        ProjectConfiguration::PrepareDerivedCleanup => {
                                            serde_json::to_value(
                                                collection.derived_cleanup_preview(&lease)?,
                                            )?
                                        }
                                        ProjectConfiguration::DeleteDerived { preview } => {
                                            airk_kernel::service_execution::progress_phase(
                                                airk_core::service_progress::WorkPhase::Retiring,
                                            );
                                            let mut removed = 0u64;
                                            loop {
                                                if closing.load(Ordering::Acquire) {
                                                    anyhow::bail!(
                                                        airk_kernel::service_execution::Draining
                                                    )
                                                }
                                                airk_kernel::service_execution::checkpoint()?;
                                                if registry.cancellation_requested(job.id)? {
                                                    return Ok(JobOutcome::Canceled);
                                                }
                                                let report = collection
                                                    .delete_derived_turn(&lease, job.id, preview)?;
                                                airk_kernel::service_execution::progress_verified(
                                                    report
                                                        .removed_known_files
                                                        .saturating_sub(removed),
                                                );
                                                removed = report.removed_known_files;
                                                if report.complete {
                                                    break serde_json::to_value(report)?;
                                                }
                                                std::thread::sleep(Duration::from_millis(10));
                                            }
                                        }
                                        _ => anyhow::bail!(
                                            "ordinary configuration cannot run in a cleanup owner"
                                        ),
                                    };
                                    registry
                                        .record_configuration_result(&lease, job.id, &result)?;
                                    Ok(JobOutcome::Succeeded { revision: None })
                                })
                                .unwrap_or_else(|error| {
                                    drained = error.chain().any(|cause| {
                                        cause.is::<airk_kernel::service_execution::Draining>()
                                    });
                                    if error.is::<airk_kernel::service_execution::Canceled>() {
                                        JobOutcome::Canceled
                                    } else {
                                        failure_detail = Some(
                                            crate::service_logs::FailureDetail::capture(&error),
                                        );
                                        JobOutcome::Failed {
                                            code: "derived_cleanup_failed_reinspect".into(),
                                        }
                                    }
                                });
                            // Parent EOF is not user cancellation or a failed completed effect.
                            // Leave running work for the next fenced owner to recover; a
                            // drained in-process owner leaves it exactly the same way.
                            if !closing.load(Ordering::Acquire) && !drained {
                                if let (JobOutcome::Failed { code }, Some(detail)) =
                                    (&outcome, failure_detail)
                                {
                                    status
                                        .lock()
                                        .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?
                                        .failure = Some(crate::service_logs::JobFailure {
                                        job: job.id,
                                        grant: allowance.id,
                                        worker: identity.worker,
                                        operation: job.kind,
                                        code: code.clone(),
                                        detail,
                                    });
                                }
                                registry.complete_job(&lease, job.id, outcome)?;
                            }
                        }
                        let mut status = status
                            .lock()
                            .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?;
                        status.active = None;
                        status.finished = Some(allowance.id);
                        Ok(())
                    }));
                    Reply::WorkAccepted
                }
            }
            Command::WorkStatus {
                identity: expected,
                control,
                ..
            } if expected == identity => {
                if airk_kernel::service_execution::watchdog() {
                    // Cleanup never reached a safe boundary after cancellation.
                    match residency.scope() {
                        // A child exits; its parent keeps the work allowance
                        // until it observes that exit.
                        None => std::process::exit(76),
                        // In process, this owner quarantines itself: it takes
                        // no more work, its stuck thread keeps the lease, and
                        // its supervisor observes the reason.
                        Some(scope) => scope
                            .execution
                            .quarantine(airk_kernel::service_execution::StuckReason::WorkStalled),
                    }
                }
                if let Some(control) = control {
                    airk_kernel::service_execution::control(
                        identity,
                        control.grant,
                        control.paused,
                        control.canceled,
                        control.yield_to_source,
                    )?;
                }
                let mut observation = status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?
                    .clone();
                observation.progress = airk_kernel::service_execution::progress().map(Box::new);
                if job.finished() {
                    job.join()?;
                }
                Reply::WorkStatus { observation }
            }
            Command::ConfigurePool {
                identity: expected,
                pool: None,
                pool_pending: false,
            } if expected == identity => Reply::PoolConfigured,
            Command::Stop => break,
            _ => Reply::Failed,
        };
        replies.send(reply)?;
    }
    closing.store(true, Ordering::Release);
    job.join()
}
