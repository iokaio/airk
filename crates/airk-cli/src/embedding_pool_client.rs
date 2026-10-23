// SPDX-License-Identifier: Apache-2.0
//! Scoped direct embedding-pool connector. It never forwards model inputs through the service.
use crate::{embedding_pool_protocol as protocol, local_ipc};
use airk_core::identity::Incarnation;
use airk_retrieval::embedding_worker::{
    self, Connector, MAX_FRAME_BYTES, ModelLocation, PROTOCOL, RemoteSession, Request, Response,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc,
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
                        let response=async {
                            let request_id=airk_kernel::service_broker::new_instance_id()?;
                            let digest=protocol::input_digest(&work.operation)?;
                            let envelope=protocol::Envelope{project:access.grant.project.binding,worker:access.grant.worker,session:work.session,request_id,input_digest:digest,operation:work.operation};
                            local_ipc::write_message_limited(&mut stream,&envelope,Duration::from_secs(5),MAX_FRAME_BYTES).await?;
                            let result:protocol::ResultEnvelope=local_ipc::read_message_limited(&mut stream,Duration::from_secs(120),MAX_FRAME_BYTES).await?.context("embedding pool disconnected")?;
                            ensure!(result.project==envelope.project&&result.worker==envelope.worker&&result.session==envelope.session&&result.request_id==envelope.request_id&&result.input_digest==envelope.input_digest,"embedding result crossed a scoped request");
                            Ok::<_,anyhow::Error>(result.response)
                        }.await;
                        let disconnected=response.is_err();
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
        Ok(Self { sender, failed })
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
