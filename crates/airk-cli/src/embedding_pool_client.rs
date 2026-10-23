// SPDX-License-Identifier: Apache-2.0
//! Scoped direct embedding-pool connector. It never forwards model inputs through the service.
use crate::{embedding_pool_protocol as protocol, local_ipc};
use airk_core::identity::Incarnation;
use airk_retrieval::embedding_worker::{
    self, Connector, MAX_FRAME_BYTES, ModelLocation, PROTOCOL, RemoteSession, Request, Response,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

struct Work {
    session: Incarnation,
    operation: protocol::Operation,
    reply: Option<mpsc::SyncSender<Result<Response>>>,
}
struct Client {
    sender: mpsc::SyncSender<Work>,
    failed: Arc<AtomicBool>,
    closed: Arc<Mutex<BTreeSet<Incarnation>>>,
}

async fn exchange(
    stream: &mut local_ipc::Stream,
    access: &protocol::Access,
    session: Incarnation,
    operation: protocol::Operation,
) -> Result<Response> {
    let request_id = airk_kernel::service_broker::new_instance_id()?;
    let digest = protocol::input_digest(&operation)?;
    let envelope = protocol::Envelope {
        project: access.grant.project.binding,
        worker: access.grant.worker,
        session,
        request_id,
        input_digest: digest,
        operation,
    };
    local_ipc::write_message_limited(stream, &envelope, Duration::from_secs(5), MAX_FRAME_BYTES)
        .await?;
    let result: protocol::ResultEnvelope =
        local_ipc::read_message_limited(stream, Duration::from_secs(120), MAX_FRAME_BYTES)
            .await?
            .context("embedding pool disconnected")?;
    ensure!(
        result.project == envelope.project
            && result.worker == envelope.worker
            && result.session == envelope.session
            && result.request_id == envelope.request_id
            && result.input_digest == envelope.input_digest,
        "embedding result crossed a scoped request"
    );
    Ok(result.response)
}
impl Client {
    fn new(access: protocol::Access) -> Result<Self> {
        ensure!(
            access.role == protocol::role(access.grant.pool),
            "invalid embedding pool role"
        );
        let (sender, receiver) = mpsc::sync_channel::<Work>(8);
        let (ready, initialized) = mpsc::sync_channel(1);
        let failed = Arc::new(AtomicBool::new(false));
        let exchange_failed = failed.clone();
        let closed = Arc::new(Mutex::new(BTreeSet::<Incarnation>::new()));
        let pending_closed = closed.clone();
        std::thread::Builder::new().name("airk-pool-client".into()).spawn(move||{
            let result=(||->Result<()> {
                let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
                runtime.block_on(async {
                    let address=local_ipc::endpoint(&access.runtime,&access.role)?;
                    let mut stream=tokio::time::timeout(Duration::from_secs(5),local_ipc::connect(&address)).await??;
                    local_ipc::write_message(&mut stream,&access.grant,Duration::from_secs(5)).await?;
                    let observation:protocol::Observation=local_ipc::read_message(&mut stream,Duration::from_secs(5)).await?.context("embedding pool rejected capability")?;
                    ensure!(matches!(observation,protocol::Observation::Ready{pool,role} if pool==access.grant.pool&&role==access.role),"embedding pool identity mismatch");
                    let _=ready.send(Ok(()));
                    while let Ok(work)=receiver.recv() {
                        ensure!(!exchange_failed.load(Ordering::Acquire),"embedding pool client closed");
                        // A full request queue cannot lose session retirement. Drain the
                        // separate bounded set before the next queued operation.
                        let retired=std::mem::take(&mut *pending_closed.lock().map_err(|_|anyhow::anyhow!("pool session retirement unavailable"))?);
                        let closed_current=retired.contains(&work.session);
                        for session in retired{exchange(&mut stream,&access,session,protocol::Operation::Close).await?;}
                        let skipped=closed_current && matches!(work.operation,protocol::Operation::Call{..});
                        let response=if skipped{Err(anyhow::anyhow!("embedding session closed before queued operation"))}else{exchange(&mut stream,&access,work.session,work.operation).await};
                        let disconnected=!skipped && response.is_err();
                        if let Some(reply)=work.reply{let _=reply.send(response);}
                        if disconnected{break;}
                    }
                    Ok::<_,anyhow::Error>(())
                })
            })();
            if result.is_err(){let _=ready.send(Err("embedding pool transport unavailable"));}
            exchange_failed.store(true,Ordering::Release);
        })?;
        initialized
            .recv_timeout(Duration::from_secs(10))?
            .map_err(|reason| anyhow::anyhow!(reason))?;
        Ok(Self {
            sender,
            failed,
            closed,
        })
    }

    fn request(&self, session: Incarnation, request: Request) -> Result<Response> {
        ensure!(
            !self.failed.load(Ordering::Acquire),
            "embedding pool connection failed"
        );
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(Work {
                session,
                operation: protocol::Operation::Call { request },
                reply: Some(reply),
            })
            .map_err(|_| anyhow::anyhow!("embedding pool client queue full"))?;
        let result = receiver
            .recv_timeout(Duration::from_secs(120))
            .context("embedding pool deadline exceeded")??;
        if let Response::Failed { reason } = &result {
            anyhow::bail!("embedding pool rejected request: {reason}");
        }
        Ok(result)
    }
}

struct Session {
    client: Arc<Client>,
    id: Incarnation,
}
impl Session {
    fn new(client: Arc<Client>) -> Result<Self> {
        Ok(Self {
            client,
            id: airk_kernel::service_broker::new_instance_id()?,
        })
    }
}
impl RemoteSession for Session {
    fn request(&self, request: Request) -> Result<Response> {
        self.client.request(self.id, request)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        match self.client.closed.lock() {
            Ok(mut closed) if closed.len() < 64 => {
                closed.insert(self.id);
            }
            _ => {
                self.client.failed.store(true, Ordering::Release);
            }
        }
        let _ = self.client.sender.try_send(Work {
            session: self.id,
            operation: protocol::Operation::Close,
            reply: None,
        });
    }
}
struct PoolConnector {
    client: Arc<Client>,
}
impl Connector for PoolConnector {
    fn tokenizer(&self, _path: &Path) -> Result<Arc<dyn RemoteSession>> {
        Ok(Arc::new(Session::new(self.client.clone())?))
    }
    fn open(&self, location: ModelLocation, identity: &str) -> Result<Arc<dyn RemoteSession>> {
        let session = Session::new(self.client.clone())?;
        let response = session.request(Request::Open {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            location,
            identity: identity.into(),
        })?;
        ensure!(
            matches!(response,Response::Opened{protocol,version,identity:actual} if protocol==PROTOCOL&&version==env!("CARGO_PKG_VERSION")&&actual==identity),
            "embedding pool model handshake mismatch"
        );
        Ok(Arc::new(session))
    }
}
struct Unavailable;
impl Connector for Unavailable {
    fn open(&self, _location: ModelLocation, _identity: &str) -> Result<Arc<dyn RemoteSession>> {
        anyhow::bail!("shared embedding pool unavailable; lexical retrieval remains available")
    }
}

pub fn install(access: Option<protocol::Access>) -> Result<()> {
    let connector: Arc<dyn Connector> = match access {
        Some(access) => match Client::new(access) {
            Ok(client) => Arc::new(PoolConnector {
                client: Arc::new(client),
            }),
            Err(_) => Arc::new(Unavailable),
        },
        None => Arc::new(Unavailable),
    };
    embedding_worker::install_connector(connector)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_client_queue_preserves_close_and_cannot_reopen_a_retired_session() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let instance = airk_kernel::service_broker::new_instance_id()?;
        let access = protocol::Access {
            runtime: root.clone(),
            role: protocol::role(instance),
            grant: protocol::Grant {
                service: instance,
                pool: instance,
                worker: instance,
                token: airk_core::identity::Sha256Digest::from_bytes([1; 32]),
                project: airk_core::service::ProjectRegistration::new(
                    "/source".into(),
                    "/state".into(),
                    "fixture".into(),
                )?,
            },
        };
        let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(&root, &access.role)?)?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let peer_entered = entered.clone();
        let peer_release = release.clone();
        let role = access.role.clone();
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let _: protocol::Grant = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("fixture grant missing")?;
            local_ipc::write_message(
                &mut stream,
                &protocol::Observation::Ready {
                    pool: instance,
                    role,
                },
                Duration::from_secs(5),
            )
            .await?;
            let mut trace = Vec::new();
            while let Some(envelope) = local_ipc::read_message_limited::<_, protocol::Envelope>(
                &mut stream,
                Duration::from_secs(5),
                MAX_FRAME_BYTES,
            )
            .await?
            {
                if trace.is_empty() {
                    peer_entered.notify_one();
                    peer_release.notified().await;
                }
                trace.push((
                    envelope.session,
                    matches!(envelope.operation, protocol::Operation::Close),
                    matches!(
                        envelope.operation,
                        protocol::Operation::Call {
                            request: Request::Open { .. }
                        }
                    ),
                ));
                local_ipc::write_message_limited(
                    &mut stream,
                    &protocol::ResultEnvelope {
                        project: envelope.project,
                        worker: envelope.worker,
                        session: envelope.session,
                        request_id: envelope.request_id,
                        input_digest: envelope.input_digest,
                        response: Response::Peak { bytes: 0 },
                    },
                    Duration::from_secs(5),
                    MAX_FRAME_BYTES,
                )
                .await?;
            }
            Ok::<_, anyhow::Error>(trace)
        });
        let client = Arc::new(tokio::task::spawn_blocking(move || Client::new(access)).await??);
        let first = airk_kernel::service_broker::new_instance_id()?;
        client
            .sender
            .try_send(Work {
                session: first,
                operation: protocol::Operation::Call {
                    request: Request::Peak,
                },
                reply: None,
            })
            .map_err(|_| anyhow::anyhow!("fixture first request failed"))?;
        entered.notified().await;
        let closed = airk_kernel::service_broker::new_instance_id()?;
        for n in 0..8 {
            let session = if n == 0 {
                closed
            } else {
                airk_kernel::service_broker::new_instance_id()?
            };
            let request = if n == 0 {
                Request::Open {
                    protocol: PROTOCOL,
                    version: env!("CARGO_PKG_VERSION").into(),
                    location: ModelLocation::Directory { path: root.clone() },
                    identity: "fixture".into(),
                }
            } else {
                Request::Peak
            };
            client
                .sender
                .try_send(Work {
                    session,
                    operation: protocol::Operation::Call { request },
                    reply: None,
                })
                .map_err(|_| anyhow::anyhow!("fixture queue fill failed"))?;
        }
        drop(Session {
            client: client.clone(),
            id: closed,
        }); // Close wakeup cannot enter the full queue.
        release.notify_one();
        drop(client);
        let trace = tokio::time::timeout(Duration::from_secs(10), peer).await???;
        ensure!(
            trace
                .iter()
                .any(|(id, is_close, _)| *id == closed && *is_close),
            "full queue lost session retirement"
        );
        ensure!(
            trace.len() == 9,
            "retired queued operation disrupted unrelated queued sessions"
        );
        ensure!(
            !trace
                .iter()
                .any(|(id, _, is_open)| *id == closed && *is_open),
            "queued work reopened retired session"
        );
        Ok(())
    }
}
