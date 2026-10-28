// SPDX-License-Identifier: Apache-2.0
//! Private inactive-project owner. No query endpoint, Kernel or inference session.
use crate::{
    project_protocol::{self as protocol, Command, Reply},
    service_ipc,
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

pub fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("missing private cleanup initialization")?;
    crate::local_ipc::prepare_root(&init.runtime)?;
    let mut lease = ProjectLease::acquire(&init.runtime, init.project)?;
    let mut registry = ServiceRegistry::open(&init.runtime, init.service)?;
    let epoch = registry.admit_derived_worker(&mut lease)?;
    let registration = registry.validate_inactive_worker(&lease)?;
    let lease = Arc::new(lease);
    ensure!(
        PROCESS_LEASE.set(lease.clone()).is_ok(),
        "cleanup role already owns a process lease"
    );
    let identity = WorkerIdentity {
        service: init.service,
        worker: init.worker,
        project: init.project,
        fencing_generation: epoch,
    };
    service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        &Reply::Ready {
            identity,
            role: protocol::role(init.worker),
        },
    )?;
    let Some(Command::Activate {
        identity: accepted,
        pool,
        ..
    }) = service_ipc::read_frame(&mut std::io::stdin().lock())?
    else {
        anyhow::bail!("cleanup worker was not activated")
    };
    ensure!(
        accepted == identity && pool.is_none(),
        "cleanup activation identity or role differs"
    );
    airk_kernel::service_execution::install(Path::new(&registration.data_root), identity)?;
    let collection = Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?;
    registry.recover_jobs(&lease)?;
    service_ipc::write_frame(&mut std::io::stdout().lock(), &Reply::Activated)?;
    let status = Arc::new(Mutex::new(protocol::WorkObservation {
        waiting_for: None,
        failure: None,
        active: None,
        finished: None,
        maintenance: None,
        progress: None,
    }));
    let closing = Arc::new(AtomicBool::new(false));
    let mut task: Option<std::thread::JoinHandle<Result<()>>> = None;
    loop {
        let Some(command) = service_ipc::read_frame(&mut std::io::stdin().lock())? else {
            break;
        };
        let reply = match command {
            Command::Renew { identity: expected } if expected == identity => {
                airk_kernel::service_execution::renew(identity)?;
                Reply::Renewed
            }
            Command::RunJob {
                identity: expected,
                allowance,
            } if expected == identity
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
                    if let Some(previous) = task.take() {
                        previous
                            .join()
                            .map_err(|_| anyhow::anyhow!("cleanup thread panicked"))??;
                    }
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
                    task = Some(std::thread::spawn(move || {
                        let mut registry = ServiceRegistry::open(&runtime, identity.service)?;
                        if let Some(job) = registry.claim_job_id(&lease, allowance.job)? {
                            let mut failure_detail = None;
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
                                                    anyhow::bail!("cleanup owner is draining")
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
                            // Leave running work for the next fenced owner to recover.
                            if !closing.load(Ordering::Acquire) {
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
                    std::process::exit(76);
                }
                if let Some(control) = control {
                    airk_kernel::service_execution::control(
                        identity,
                        control.grant,
                        control.paused,
                        control.canceled,
                    )?;
                }
                let mut observation = status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("cleanup status unavailable"))?
                    .clone();
                observation.progress = airk_kernel::service_execution::progress().map(Box::new);
                if task.as_ref().is_some_and(|task| task.is_finished()) {
                    task.take()
                        .expect("observed cleanup task")
                        .join()
                        .map_err(|_| anyhow::anyhow!("cleanup thread panicked"))??;
                }
                Reply::WorkStatus { observation }
            }
            Command::ConfigurePool {
                identity: expected,
                pool: None,
            } if expected == identity => Reply::PoolConfigured,
            Command::Stop => break,
            _ => Reply::Failed,
        };
        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
    }
    closing.store(true, Ordering::Release);
    if let Some(task) = task {
        task.join()
            .map_err(|_| anyhow::anyhow!("cleanup thread panicked"))??;
    }
    Ok(())
}
