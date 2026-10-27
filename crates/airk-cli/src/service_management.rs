// SPDX-License-Identifier: Apache-2.0
//! Bounded metadata actor keeps registry I/O off independent native health handlers.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobError},
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::mpsc::{SyncSender, sync_channel},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::oneshot;

type Envelope = (ControlMethod, oneshot::Sender<ControlResponse>);
pub struct Management {
    sender: Option<SyncSender<Envelope>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pending: Arc<AtomicUsize>,
    plans: Arc<AtomicBool>,
}

impl Drop for Management {
    fn drop(&mut self) {
        // Do not surrender the service ownership lock while metadata work survives.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn execute_request(registry: &mut ServiceRegistry, method: ControlMethod) -> ControlResponse {
    // A failed observation has no effects to replay. RequestEpoch prunes receipts,
    // and all mutations remain outside this whitelist, even when receipt-keyed.
    let read_only = matches!(
        method,
        ControlMethod::Job { .. }
            | ControlMethod::ConfigurationResult { .. }
            | ControlMethod::Plan { .. }
            | ControlMethod::PlanItems { .. }
            | ControlMethod::Projects { .. }
            | ControlMethod::Events { .. }
            | ControlMethod::Maintenance { .. }
            | ControlMethod::Watch { .. }
            | ControlMethod::Progress { .. }
            | ControlMethod::Inspection { .. }
            | ControlMethod::Console { .. }
            | ControlMethod::InactiveConsole { .. }
    );
    execute(registry, method).unwrap_or_else(|error| {
        let code = if read_only && airk_store::service::registry::is_read_contention(&error) {
            "registry_busy".into()
        } else {
            error
                .downcast_ref::<JobError>()
                .map(|code| format!("{code:?}"))
                .or_else(|| {
                    error
                        .downcast_ref::<airk_core::service_events::EventError>()
                        .map(ToString::to_string)
                })
                .or_else(|| {
                    error
                        .downcast_ref::<airk_core::service_maintenance::PlanError>()
                        .map(ToString::to_string)
                })
                .or_else(|| {
                    (error.is::<airk_store::service::data_scope::ForeignDataRoot>()
                        || error.is::<airk_store::service::data_scope::UnboundDataRoot>())
                    .then(|| "data_root_scope_conflict".into())
                })
                .unwrap_or_else(|| "invalid_registry_request".into())
        };
        ControlResponse::Failure { code }
    })
}

fn execute(registry: &mut ServiceRegistry, method: ControlMethod) -> Result<ControlResponse> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    Ok(match method {
        ControlMethod::PlanCreate {
            caller,
            key,
            request,
        } => ControlResponse::Plan {
            plan: registry.create_maintenance_plan(
                caller,
                &key,
                &request,
                now_ms,
                crate::runtime_pair::engine_identity()?,
            )?,
        },
        ControlMethod::Plan { id } => ControlResponse::Plan {
            plan: registry.maintenance_plan(id)?,
        },
        ControlMethod::PlanItems { id, after, limit } => ControlResponse::PlanItems {
            page: registry.maintenance_plan_page(id, after, limit)?,
        },
        ControlMethod::PlanExecute {
            id,
            preview,
            allow_fallback,
        } => ControlResponse::Plan {
            plan: registry.approve_maintenance_plan(id, preview, allow_fallback, now_ms)?,
        },
        ControlMethod::PlanPause { id, paused } => ControlResponse::Plan {
            plan: registry.pause_maintenance_plan(id, paused, now_ms)?,
        },
        ControlMethod::PlanCancel { id } => ControlResponse::Plan {
            plan: registry.cancel_maintenance_plan(id, now_ms)?,
        },
        ControlMethod::ReserveProject { project } => {
            airk_kernel::service_projects::validate_planned(&project)?;
            registry.register(&project)?;
            ControlResponse::Registered {
                project: registry.get(project.binding)?,
            }
        }
        ControlMethod::Register { project } => {
            airk_kernel::service_projects::validate(&project)?;
            registry.register(&project)?;
            ControlResponse::Registered {
                project: registry.get(project.binding)?,
            }
        }
        ControlMethod::Console {
            after,
            revision,
            limit,
        } => ControlResponse::Console {
            page: console_page(registry, after, revision, limit, now_ms)?,
        },
        ControlMethod::Projects {
            after,
            revision,
            limit,
        } => ControlResponse::Projects {
            page: registry.list(after, revision, limit)?,
        },
        ControlMethod::InactiveConsole {
            after,
            revision,
            limit,
        } => ControlResponse::Console {
            page: console_page_role(registry, after, revision, limit, now_ms, true)?,
        },
        ControlMethod::Maintenance { project } => {
            registry.get(project)?;
            ControlResponse::Maintenance {
                project,
                observation: registry.maintenance_observation(project)?,
                retrying: registry.maintenance_retrying(project)?,
            }
        }
        ControlMethod::Watch { project } => ControlResponse::Watch {
            project,
            observation: registry.watch(project)?.observation,
        },
        ControlMethod::Progress { project } => {
            let paused = registry.get(project)?.paused;
            let pending_job = registry.pending_job_observation(project)?;
            let mut queued_jobs = registry.queued_jobs_observation(project, 17)?;
            let queue_truncated = queued_jobs.len() > 16;
            queued_jobs.truncate(16);
            let pending_paused = paused
                && pending_job
                    .as_ref()
                    .map(|job| registry.job_runs_while_paused(job.id))
                    .transpose()?
                    .is_some_and(|runs| !runs);
            ControlResponse::Progress {
                project,
                observation: registry.progress_observation(project)?.map(Box::new),
                pending_job,
                queued_jobs,
                queue_truncated: Some(queue_truncated),
                project_paused: Some(paused),
                pending_paused: Some(pending_paused),
                observed_unix_ms: Some(now_ms),
            }
        }
        ControlMethod::Inspection { project } => ControlResponse::Inspection {
            project,
            observation: registry.inspection(project)?.map(Box::new),
            cached: true,
        },
        ControlMethod::ConfigureWatch { project, enabled } => {
            registry.configure_watch(project, enabled)?;
            ControlResponse::Watch {
                project,
                observation: registry.watch(project)?.observation,
            }
        }
        ControlMethod::RequestEpoch => {
            let (namespace, epoch) = registry.request_epoch()?;
            ControlResponse::RequestEpoch { namespace, epoch }
        }
        ControlMethod::Events {
            project,
            cursor,
            limit,
        } => ControlResponse::Events {
            page: registry.events(project, cursor.as_ref(), limit)?,
        },
        ControlMethod::ConfigurationResult { id } => ControlResponse::Configuration {
            job: registry.job(id)?,
            result: registry.configuration_result(id)?,
        },
        ControlMethod::ConfigureProject {
            project,
            caller,
            key,
            request,
        } => ControlResponse::Job {
            job: registry.enqueue_configuration(
                project,
                caller,
                &key,
                &request,
                crate::runtime_pair::engine_identity()?,
            )?,
        },
        ControlMethod::Enqueue {
            project,
            caller,
            key,
            kind,
        } => ControlResponse::Job {
            job: registry.enqueue(project, caller, &key, kind)?,
        },
        ControlMethod::Job { id } => ControlResponse::Job {
            job: registry.job(id)?,
        },
        ControlMethod::Retry { id, caller, key } => ControlResponse::Job {
            job: registry.retry_job(id, caller, &key)?,
        },
        ControlMethod::Pause { project, paused } => {
            registry.pause(project, paused)?;
            ControlResponse::Registered {
                project: registry.get(project)?,
            }
        }
        ControlMethod::Cancel { id } => ControlResponse::Job {
            job: registry.request_cancellation(id)?,
        },
        ControlMethod::CancelPending { id } => ControlResponse::Job {
            job: registry.cancel_pending(id)?,
        },
        ControlMethod::Logs { .. }
        | ControlMethod::Health
        | ControlMethod::Shutdown
        | ControlMethod::Connect { .. }
        | ControlMethod::EmbeddingStatus
        | ControlMethod::SourceStatus
        | ControlMethod::Resources
        | ControlMethod::Workers { .. }
        | ControlMethod::Worker { .. }
        | ControlMethod::RestartProject { .. }
        | ControlMethod::StartEmbedding
        | ControlMethod::StopEmbedding { .. }
        | ControlMethod::ConfigureResources { .. } => {
            anyhow::bail!("not a registry method")
        }
    })
}

fn observed<T>(
    value: Result<T>,
    kind: airk_core::service_console::ObservationKind,
    failed: &mut Vec<airk_core::service_console::ObservationKind>,
) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(_) => {
            failed.push(kind);
            None
        }
    }
}
pub(crate) fn console_page(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
) -> Result<airk_core::service_console::ConsolePage> {
    console_page_role(registry, after, revision, limit, now_ms, false)
}
fn console_page_role(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
    inactive: bool,
) -> Result<airk_core::service_console::ConsolePage> {
    use airk_core::service_console::{
        ConsolePage, MAX_PAGE_BYTES, ObservationKind as Kind, ProjectObservation,
    };
    let source = if inactive {
        registry.list_inactive(after, revision, limit)?
    } else {
        registry.list(after, revision, limit)?
    };
    let mut page = ConsolePage {
        revision: source.revision,
        observed_unix_ms: now_ms,
        cached: true,
        projects: Vec::new(),
        after: source.after,
    };
    let source_after = source.after;
    let mut overflow = false;
    for project in source.projects {
        let binding = project.registration.binding;
        let mut failed = Vec::new();
        let pending_job = observed(
            registry.pending_job_observation(binding),
            Kind::PendingJob,
            &mut failed,
        )
        .flatten();
        let pending_paused = project.paused
            && pending_job.as_ref().is_some_and(|job| {
                !observed(
                    registry.job_runs_while_paused(job.id),
                    Kind::PendingJob,
                    &mut failed,
                )
                .unwrap_or(true)
            });
        let row = ProjectObservation {
            pending_job,
            pending_paused,
            configuration: observed(
                registry.latest_configuration(binding),
                Kind::Configuration,
                &mut failed,
            )
            .flatten(),
            watch: observed(
                registry.watch(binding).map(|watch| watch.observation),
                Kind::Watch,
                &mut failed,
            ),
            progress: observed(
                registry.progress_observation(binding),
                Kind::Progress,
                &mut failed,
            )
            .flatten(),
            inspection: observed(registry.inspection(binding), Kind::Inspection, &mut failed)
                .flatten(),
            maintenance: observed(
                registry.maintenance_observation(binding),
                Kind::Maintenance,
                &mut failed,
            )
            .flatten(),
            maintenance_retrying: observed(
                registry.maintenance_retrying(binding),
                Kind::MaintenanceRetry,
                &mut failed,
            ),
            failed_observations: failed,
            project,
        };
        page.projects.push(row);
        page.after = Some(binding);
        if serde_json::to_vec(&page)?.len() > MAX_PAGE_BYTES {
            page.projects.pop();
            overflow = true;
            break;
        }
    }
    anyhow::ensure!(
        !overflow || !page.projects.is_empty(),
        "console project observation exceeds page budget"
    );
    page.after = if overflow || source_after.is_some() {
        page.projects
            .last()
            .map(|row| row.project.registration.binding)
    } else {
        None
    };
    Ok(page)
}

impl Management {
    pub fn start(
        root: PathBuf,
        instance: Incarnation,
        source: crate::source_supervisor::SourceWake,
        log: crate::service_logs::Log,
        ownership: Arc<airk_store::service::ServiceLease>,
    ) -> Result<Self> {
        let (sender, receiver) = sync_channel::<Envelope>(32);
        let pending = Arc::new(AtomicUsize::new(0));
        let active = pending.clone();
        let plans = Arc::new(AtomicBool::new(true));
        let runnable_plans = plans.clone();
        let (ready, initialized) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("airk-registry".into())
            .spawn(move || {
                let _ownership = ownership;
                let mut registry =
                    ServiceRegistry::open(&root, instance).and_then(|mut registry| {
                        registry.record_service_instance(&_ownership, instance)?;
                        crate::runtime_install::record_registry_root(
                            &_ownership,
                            registry.incarnation()?,
                        )?;
                        Ok(registry)
                    });
                let _ = ready.send(());
                source.changed();
                let mut next_tick = std::time::Instant::now();
                let mut metadata_failed = false;
                loop {
                    if std::time::Instant::now() >= next_tick {
                        let running = registry.as_mut().ok().and_then(|registry| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()?;
                            registry
                                .advance_maintenance_plans(now.as_millis().try_into().ok()?)
                                .ok()
                        });
                        if running.is_none() && !metadata_failed {
                            log.record(
                                airk_core::service_logs::LogKind::MetadataUnavailable,
                                None,
                                None,
                            );
                        }
                        metadata_failed = running.is_none();
                        // Unknown metadata state retains service ownership until a
                        // later bounded turn can establish whether work remains.
                        runnable_plans.store(running.unwrap_or(true), Ordering::Release);
                        next_tick = std::time::Instant::now()
                            + Duration::from_millis(if running.is_some() { 250 } else { 1000 });
                    }
                    let (method, reply) = match receiver.recv_timeout(
                        next_tick.saturating_duration_since(std::time::Instant::now()),
                    ) {
                        Ok(envelope) => envelope,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let source_changed = matches!(
                        method,
                        ControlMethod::ConfigureWatch { .. }
                            | ControlMethod::Register { .. }
                            | ControlMethod::ReserveProject { .. }
                    );
                    let plan_changed = matches!(
                        method,
                        ControlMethod::PlanCreate { .. }
                            | ControlMethod::PlanExecute { .. }
                            | ControlMethod::PlanPause { .. }
                            | ControlMethod::PlanCancel { .. }
                    );
                    let response = match method {
                        ControlMethod::Logs { limit } => {
                            match crate::service_logs::recent(&root, limit) {
                                Ok(page) => ControlResponse::Logs { page },
                                Err(_) => ControlResponse::Failure {
                                    code: "diagnostic_logs_unavailable".into(),
                                },
                            }
                        }
                        method => match registry.as_mut() {
                            Ok(registry) => execute_request(registry, method),
                            Err(_) => ControlResponse::Failure {
                                code: "registry_unavailable".into(),
                            },
                        },
                    };
                    if source_changed {
                        source.changed();
                    }
                    if plan_changed {
                        // Accepted plan work must prevent idle exit immediately,
                        // including the interval before its first dispatch turn.
                        runnable_plans.store(true, Ordering::Release);
                        next_tick = std::time::Instant::now();
                    }
                    // A timed-out/disconnected caller cannot release this charge
                    // while its accepted metadata operation can still commit.
                    active.fetch_sub(1, Ordering::AcqRel);
                    let _ = reply.send(response);
                }
            })?;
        // Finish owned-ledger bootstrap before other service adapters open it.
        // Failure remains a cached unavailable state so health can still run.
        let _ = initialized.recv_timeout(Duration::from_secs(5));
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            pending,
            plans,
        })
    }

    pub async fn request(&self, method: ControlMethod) -> ControlResponse {
        let (sender, receiver) = oneshot::channel();
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self
            .sender
            .as_ref()
            .is_none_or(|queue| queue.try_send((method, sender)).is_err())
        {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return ControlResponse::Failure {
                code: "registry_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(3), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "registry_response_unknown_retry_with_key".into(),
            },
        }
    }
    pub fn can_idle_exit(&self) -> bool {
        self.pending.load(Ordering::Acquire) == 0 && !self.plans.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::ProjectRegistration,
        service_console::{MAX_PAGE_BYTES, ObservationKind},
    };
    #[test]
    fn registry_read_contention_preserves_receipts_and_other_failures() -> Result<()> {
        use airk_core::service::{JobKind, RequestKey};
        for code in [
            rusqlite::ffi::SQLITE_CORRUPT,
            rusqlite::ffi::SQLITE_IOERR,
            rusqlite::ffi::SQLITE_CONSTRAINT,
        ] {
            let error =
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into();
            assert!(!airk_store::service::registry::is_read_contention(&error));
        }
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let caller = Incarnation::from_bytes([92; 16]);
        let mut registry = ServiceRegistry::open(&root, caller)?;
        let project = ProjectRegistration::new(
            owner.path().join("source").to_str().unwrap().into(),
            owner.path().join("data").to_str().unwrap().into(),
            "read-contention".into(),
        )?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let job = registry.enqueue(
            project.binding,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce: caller,
            },
            JobKind::Inspect,
        )?;
        let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        sql.execute_batch("BEGIN EXCLUSIVE")?;
        let error = execute(&mut registry, ControlMethod::Job { id: job.id })
            .err()
            .expect("held registry must block a read");
        assert!(airk_store::service::registry::is_read_contention(&error));
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Job { id: job.id }),
            ControlResponse::Failure { code } if code == "registry_busy")
        );
        // A mutation must not acquire the read-only retry contract.
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Pause { project: project.binding, paused: true }),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::RequestEpoch),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        sql.execute_batch("ROLLBACK")?;
        assert_eq!(registry.saved_request_epoch()?, (namespace, epoch));
        let ControlResponse::Job { job: observed } =
            execute_request(&mut registry, ControlMethod::Job { id: job.id })
        else {
            panic!("same receipt unavailable after releasing the registry")
        };
        assert_eq!(observed.id, job.id);
        assert_eq!(observed.attempts, 0);
        assert!(!registry.get(project.binding)?.paused);
        // Incoherent stored state is still a failure, never retried as contention.
        sql.execute(
            "UPDATE service_jobs SET state='succeeded' WHERE id=?1",
            [job.id.to_string()],
        )?;
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Job { id: job.id }),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        assert!(!owner.path().join("source").exists());
        assert!(!owner.path().join("data").exists());
        Ok(())
    }

    #[test]
    fn timed_out_metadata_keeps_ownership_and_exact_key_before_and_after_commit() -> Result<()> {
        use airk_core::service::{JobKind, RequestKey};
        for commit_before_timeout in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            crate::local_ipc::prepare_root(&root)?;
            let caller = Incarnation::from_bytes([91; 16]);
            let mut registry = ServiceRegistry::open(&root, caller)?;
            let project = ProjectRegistration::new(
                owner.path().join("source").to_str().unwrap().into(),
                owner.path().join("data").to_str().unwrap().into(),
                "unknown-receipt".into(),
            )?;
            registry.register(&project)?;
            let (namespace, epoch) = registry.request_epoch()?;
            let request = ControlMethod::Enqueue {
                project: project.binding,
                caller,
                key: RequestKey {
                    namespace,
                    epoch,
                    nonce: caller,
                },
                kind: JobKind::Index,
            };
            let (sender, receiver) = sync_channel::<Envelope>(2);
            let (release, gate) = sync_channel(1);
            let (committed, receipt) = sync_channel(1);
            let pending = Arc::new(AtomicUsize::new(0));
            let active = pending.clone();
            // Gate the actual registry operation or its acknowledgement. This
            // forces both ambiguous outcomes without relying on disk speed.
            let thread = std::thread::spawn(move || {
                for attempt in 0..2 {
                    let (method, reply) = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
                    if attempt == 0 && !commit_before_timeout {
                        gate.recv_timeout(Duration::from_secs(15)).unwrap();
                    }
                    let response = execute(&mut registry, method).unwrap();
                    if attempt == 0 {
                        let ControlResponse::Job { job } = &response else {
                            panic!("missing committed job")
                        };
                        committed.send(job.id).unwrap();
                        if commit_before_timeout {
                            gate.recv_timeout(Duration::from_secs(15)).unwrap();
                        }
                    }
                    active.fetch_sub(1, Ordering::AcqRel);
                    let _ = reply.send(response);
                }
            });
            let management = Management {
                sender: Some(sender),
                thread: Some(thread),
                pending,
                plans: Arc::new(AtomicBool::new(false)),
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let unknown = runtime.block_on(management.request(request.clone()));
            assert!(
                matches!(unknown, ControlResponse::Failure { code } if code == "registry_response_unknown_retry_with_key")
            );
            assert!(
                !management.can_idle_exit(),
                "unknown work released ownership"
            );
            let before = receipt.try_recv();
            assert_eq!(before.is_ok(), commit_before_timeout);
            release.send(())?;
            let original = before.or_else(|_| receipt.recv_timeout(Duration::from_secs(15)))?;
            let ControlResponse::Job { job } = runtime.block_on(management.request(request)) else {
                panic!("same-key retry lost its receipt")
            };
            assert_eq!(job.id, original, "same-key retry duplicated native work");
            assert!(management.can_idle_exit());
            assert!(!owner.path().join("source").exists());
            assert!(!owner.path().join("data").exists());
        }
        Ok(())
    }
    #[test]
    fn console_observes_paused_queue_without_admitting_or_opening_project_state() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let service = Incarnation::from_bytes([77; 16]);
        let mut registry = ServiceRegistry::open(&root, service)?;
        let project = ProjectRegistration::new("/source".into(), "/data".into(), "queued".into())?;
        registry.register(&project)?;
        registry.pause(project.binding, true)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let job = registry.enqueue(
            project.binding,
            service,
            &airk_core::service::RequestKey {
                namespace,
                epoch,
                nonce: service,
            },
            airk_core::service::JobKind::Index,
        )?;
        let page = console_page(&mut registry, None, None, 1, 100)?;
        let row = &page.projects[0];
        assert_eq!(row.pending_job.as_ref().unwrap().id, job.id);
        assert!(row.pending_paused);
        assert!(row.progress.is_none());
        assert!(registry.next_pending_job(project.binding)?.is_none());
        assert!(row.failed_observations.is_empty());
        Ok(())
    }
    #[test]
    fn console_pages_bound_complete_rows_and_isolate_damaged_cached_observations() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let service = Incarnation::from_bytes([77; 16]);
        let mut registry = ServiceRegistry::open(&root, service)?;
        let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        let mut bindings = Vec::new();
        for n in 0..32u8 {
            let project = ProjectRegistration::new(
                owner
                    .path()
                    .join("sources")
                    .join("s".repeat(160))
                    .join(format!("p-{n}"))
                    .to_str()
                    .unwrap()
                    .into(),
                owner
                    .path()
                    .join("data")
                    .join("d".repeat(160))
                    .join(format!("p-{n}"))
                    .to_str()
                    .unwrap()
                    .into(),
                format!("fixture-{n:02}-{}", "x".repeat(100)),
            )?;
            registry.register(&project)?;
            let mut lease = airk_store::service::ProjectLease::acquire(&root, project.binding)?;
            let epoch = registry.admit_worker(&mut lease)?;
            let worker = airk_core::service::WorkerIdentity {
                service,
                worker: Incarnation::from_bytes([n + 1; 16]),
                project: project.binding,
                fencing_generation: epoch,
            };
            let observation = serde_json::json!({"worker":worker,"current_fencing_generation":epoch,"job":project.binding,"observed_unix_ms":1,
                "snapshot":{"project":project.binding,"incarnation":worker.worker,"publication":null,"settings_digest":null,
                "index_state":"unavailable","source_state":"unknown","recovery_required":null,
                "indexed_files":u64::MAX,"indexed_vectors":u64::MAX,"vectors_enabled":true,
                "semantic":{"state":"partial","selected_artifacts":u64::MAX,"completed_artifacts":u64::MAX-1,
                    "selected_units":u64::MAX,"completed_units":u64::MAX-1,"failed":false},
                "discovery":{"state":"ready","processed_artifacts":u64::MAX,"committed_artifacts":u64::MAX,"total_artifacts":u64::MAX,"failed":false},
                "failure":"index_observation_failed","storage":{"collection_used_bytes":u64::MAX,"collection_reserved_bytes":u64::MAX,
                    "collection_limit_bytes":u64::MAX,"root_available_bytes":u64::MAX,"physical_available_growth_bytes":u64::MAX,"replacement_estimate_bytes":u64::MAX}}});
            let parsed: airk_core::service_inspection::InspectionObservation =
                serde_json::from_value(observation)?;
            parsed.snapshot.validate()?;
            sql.execute(
                "INSERT INTO service_inspections(project,payload) VALUES(?1,?2)",
                rusqlite::params![project.binding.to_string(), serde_json::to_vec(&parsed)?],
            )?;
            bindings.push(project.binding);
        }
        let mut after = None;
        let mut revision = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        loop {
            let page = console_page(&mut registry, after, revision, 32, 100)?;
            pages += 1;
            assert!(page.cached);
            assert_eq!(page.observed_unix_ms, 100);
            assert!(serde_json::to_vec(&page)?.len() <= MAX_PAGE_BYTES);
            for row in &page.projects {
                assert!(seen.insert(row.project.registration.binding));
                assert!(row.inspection.is_some());
            }
            revision = Some(page.revision);
            match page.after {
                Some(next) => {
                    assert!(after.is_none_or(|previous| next > previous));
                    after = Some(next);
                }
                None => break,
            }
            assert!(pages < 32);
        }
        assert_eq!(seen.len(), 32);
        assert!(pages > 1, "fixture must exercise byte-budget continuation");
        sql.execute(
            "UPDATE service_inspections SET payload=?1 WHERE project=?2",
            rusqlite::params![b"{broken".as_slice(), bindings[0].to_string()],
        )?;
        let mut after = None;
        let mut found = false;
        loop {
            let page = console_page(&mut registry, after, revision, 32, 200)?;
            if let Some(row) = page
                .projects
                .iter()
                .find(|row| row.project.registration.binding == bindings[0])
            {
                assert!(row.inspection.is_none());
                assert!(
                    row.failed_observations
                        .iter()
                        .any(|kind| matches!(kind, ObservationKind::Inspection))
                );
                found = true;
            }
            match page.after {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert!(found);
        assert!(
            !owner.path().join("sources").exists() && !owner.path().join("data").exists(),
            "console metadata created/opened project namespaces"
        );
        registry.register(&ProjectRegistration::new(
            owner.path().join("fresh-source").to_str().unwrap().into(),
            owner.path().join("fresh-data").to_str().unwrap().into(),
            "fresh".into(),
        )?)?;
        assert!(
            console_page(&mut registry, None, revision, 32, 300).is_err(),
            "changed registry accepted old pagination identity"
        );
        Ok(())
    }
}
