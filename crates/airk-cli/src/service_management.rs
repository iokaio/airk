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
    time::Duration,
};
use tokio::sync::oneshot;

type Envelope = (ControlMethod, oneshot::Sender<ControlResponse>);
pub struct Management {
    sender: Option<SyncSender<Envelope>>,
    thread: Option<std::thread::JoinHandle<()>>,
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
    Ok(match method {
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
        ControlMethod::RequestEpoch => {
            let (namespace, epoch) = registry.request_epoch()?;
            ControlResponse::RequestEpoch { namespace, epoch }
        }
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
        ControlMethod::Health
        | ControlMethod::Shutdown
        | ControlMethod::Connect { .. }
        | ControlMethod::EmbeddingStatus
        | ControlMethod::Resources
        | ControlMethod::ConfigureResources { .. } => {
            anyhow::bail!("not a registry method")
        }
    })
}

impl Management {
    pub fn start(root: PathBuf, instance: Incarnation) -> Result<Self> {
        let (sender, receiver) = sync_channel::<Envelope>(32);
        let (ready, initialized) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("airk-registry".into())
            .spawn(move || {
                let mut registry = ServiceRegistry::open(&root, instance);
                let _ = ready.send(());
                while let Ok((method, reply)) = receiver.recv() {
                    let response = match registry.as_mut() {
                        Ok(registry) => execute(registry, method).unwrap_or_else(|error| {
                            let code = error
                                .downcast_ref::<JobError>()
                                .map(|code| format!("{code:?}"))
                                .unwrap_or_else(|| "invalid_registry_request".into());
                            ControlResponse::Failure { code }
                        }),
                        Err(_) => ControlResponse::Failure {
                            code: "registry_unavailable".into(),
                        },
                    };
                    let _ = reply.send(response);
                }
            })?;
        // Finish owned-ledger bootstrap before other service adapters open it.
        // Failure remains a cached unavailable state so health can still run.
        let _ = initialized.recv_timeout(Duration::from_secs(5));
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub async fn request(&self, method: ControlMethod) -> ControlResponse {
        let (sender, receiver) = oneshot::channel();
        if self
            .sender
            .as_ref()
            .is_none_or(|queue| queue.try_send((method, sender)).is_err())
        {
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
}
