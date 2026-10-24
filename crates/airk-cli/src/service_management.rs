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
        ControlMethod::Register { project } => {
            airk_kernel::service_projects::validate(&project)?;
            registry.register(&project)?;
            ControlResponse::Registered {
                project: registry.get(project.binding)?,
            }
        }
        ControlMethod::Projects {
            after,
            revision,
            limit,
        } => ControlResponse::Projects {
            page: registry.list(after, revision, limit)?,
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
        ControlMethod::Progress { project } => ControlResponse::Progress {
            project,
            observation: registry.progress_observation(project)?.map(Box::new),
        },
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
        | ControlMethod::ConfigureResources { .. } => {
            anyhow::bail!("not a registry method")
        }
    })
}

impl Management {
    pub fn start(
        root: PathBuf,
        instance: Incarnation,
        source: crate::source_supervisor::SourceWake,
        log: crate::service_logs::Log,
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
                let mut registry = ServiceRegistry::open(&root, instance);
                let _ = ready.send(());
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
                        ControlMethod::ConfigureWatch { .. } | ControlMethod::Register { .. }
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
                            Ok(registry) => execute(registry, method).unwrap_or_else(|error| {
                                let code = error
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
                                    .unwrap_or_else(|| "invalid_registry_request".into());
                                ControlResponse::Failure { code }
                            }),
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
