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
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Work {
    session: Incarnation,
    operation: protocol::Operation,
    reply: Option<mpsc::SyncSender<Result<Response>>>,
}
pub use airk_retrieval::embedding_worker::HelperUnavailable;
fn transient_helper_error(error: anyhow::Error) -> anyhow::Error {
    let transient = error.chain().any(|cause| {
        cause.is::<tokio::time::error::Elapsed>()
            || cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
                matches!(
                    io.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::NotConnected
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::UnexpectedEof
                )
            })
    });
    if transient {
        error.context(HelperUnavailable)
    } else {
        error
    }
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
        .await
        .map_err(transient_helper_error)?;
    let result: protocol::ResultEnvelope =
        local_ipc::read_message_limited(stream, Duration::from_secs(120), MAX_FRAME_BYTES)
            .await
            .map_err(transient_helper_error)?
            .ok_or(HelperUnavailable)?;
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
fn admission_remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .context("embedding admission deadline expired before inference submission")
        .map_err(|error| error.context(HelperUnavailable))
}
impl Client {
    #[cfg(test)]
    fn new(access: protocol::Access) -> Result<Self> {
        Self::new_before(access, Instant::now() + Duration::from_secs(10))
    }
    fn new_before(access: protocol::Access, deadline: Instant) -> Result<Self> {
        admission_remaining(deadline)?;
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
                    let mut stream=tokio::time::timeout(admission_remaining(deadline)?.min(Duration::from_secs(5)),local_ipc::connect(&address)).await??;
                    local_ipc::write_message(&mut stream,&access.grant,admission_remaining(deadline)?.min(Duration::from_secs(5))).await?;
                    let observation:protocol::Observation=local_ipc::read_message(&mut stream,admission_remaining(deadline)?.min(Duration::from_secs(5))).await?.context("embedding pool rejected capability")?;
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
            if let Err(error)=result{let _=ready.send(Err(transient_helper_error(error)));}
            exchange_failed.store(true,Ordering::Release);
        })?;
        initialized
            .recv_timeout(admission_remaining(deadline)?)
            .map_err(|_| HelperUnavailable)??;
        Ok(Self {
            sender,
            failed,
            closed,
        })
    }

    fn request(&self, session: Incarnation, request: Request) -> Result<Response> {
        ensure!(!self.failed.load(Ordering::Acquire), HelperUnavailable);
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(Work {
                session,
                operation: protocol::Operation::Call {
                    request: Box::new(request),
                },
                reply: Some(reply),
            })
            .map_err(|_| anyhow::anyhow!("embedding pool client queue full"))?;
        let result = receiver
            .recv_timeout(Duration::from_secs(120))
            .map_err(|_| HelperUnavailable)??;
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
    fn provision_model(&self, source: &Path) -> Result<airk_core::model_objects::ModelObjectSet> {
        // The project owner already authorized this exact canonical spelling.
        // Resolving again could redirect an admitted input before helper capture.
        let source = source.to_path_buf();
        let session = Session::new(self.client.clone())?;
        match session.request(Request::ProvisionModel { directory: source })? {
            Response::Provisioned { objects } => {
                objects.validate()?;
                Ok(objects)
            }
            _ => anyhow::bail!("invalid custom model provisioning response"),
        }
    }
    fn provision_bundled(&self) -> Result<airk_core::model_objects::ModelObjectSet> {
        let session = Session::new(self.client.clone())?;
        match session.request(Request::ProvisionBundled)? {
            Response::Provisioned { objects } => {
                objects.validate()?;
                Ok(objects)
            }
            _ => anyhow::bail!("invalid shared model provisioning response"),
        }
    }
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
// Updating authority never performs transport I/O on the project control loop.
struct HubState {
    access: Option<protocol::Access>,
    client: Option<Arc<Client>>,
    attempts: u32,
    retry_at: Instant,
    generation: u64,
    pending: bool,
    closed: bool,
    connecting: bool,
}
struct Hub {
    runtime: PathBuf,
    identity: airk_core::service::WorkerIdentity,
    state: Mutex<HubState>,
    changed: Condvar,
    #[cfg(test)]
    waiting: Mutex<Option<mpsc::Sender<()>>>,
}
impl Hub {
    fn new(runtime: &Path, identity: airk_core::service::WorkerIdentity) -> Result<Self> {
        Ok(Self {
            runtime: runtime.canonicalize()?,
            identity,
            state: Mutex::new(HubState {
                access: None,
                client: None,
                attempts: 0,
                retry_at: Instant::now(),
                generation: 0,
                pending: false,
                closed: false,
                connecting: false,
            }),
            changed: Condvar::new(),
            #[cfg(test)]
            waiting: Mutex::new(None),
        })
    }
    fn update(&self, access: Option<protocol::Access>) -> Result<()> {
        self.update_pending(access, false)
    }
    fn update_pending(&self, access: Option<protocol::Access>, pending: bool) -> Result<()> {
        ensure!(
            !(pending && access.is_some()),
            "ready helper cannot also be pending"
        );
        if let Some(access) = &access {
            access.grant.project.validate()?;
            ensure!(
                access.runtime.canonicalize()? == self.runtime
                    && access.role == protocol::role(access.grant.pool)
                    && access.grant.worker == self.identity.worker
                    && access.grant.service == self.identity.service
                    && access.grant.project.binding == self.identity.project,
                "embedding capability crossed worker authority"
            );
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("pool authority unavailable"))?;
        ensure!(!state.closed, "embedding authority owner closed");
        // A duplicate control acknowledgment must not reset a failed connection's retry budget.
        let unchanged = match (&state.access, &access) {
            (Some(old), Some(new)) => {
                old.grant.pool == new.grant.pool && old.grant.token == new.grant.token
            }
            (None, None) => true,
            _ => false,
        };
        if !unchanged || state.pending != pending {
            state.generation = state
                .generation
                .checked_add(1)
                .context("pool authority generation exhausted")?;
            state.access = access;
            state.pending = pending;
            state.client = None;
            state.attempts = 0;
            state.retry_at = Instant::now();
        }
        self.changed.notify_all();
        Ok(())
    }
    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        state.pending = false;
        state.access = None;
        state.client = None;
        state.generation = state.generation.saturating_add(1);
        self.changed.notify_all();
    }
    fn client(&self) -> Result<Arc<Client>> {
        self.client_before(Instant::now() + Duration::from_secs(10))
    }
    fn client_before(&self, deadline: Instant) -> Result<Arc<Client>> {
        let (access, generation) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("pool authority unavailable"))?;
            loop {
                admission_remaining(deadline)?;
                if state.closed {
                    return Err(anyhow::anyhow!("embedding authority owner closed")
                        .context(HelperUnavailable));
                }
                if let Some(client) = &state.client
                    && !client.failed.load(Ordering::Acquire)
                {
                    return Ok(client.clone());
                }
                if state.access.is_none() && !state.pending {
                    return Err(
                        anyhow::anyhow!("embedding authority unavailable or revoked")
                            .context(HelperUnavailable),
                    );
                }
                if state.access.is_some() && !state.connecting {
                    break;
                }
                let now = Instant::now();
                if now >= deadline {
                    return Err(anyhow::anyhow!(
                        "embedding authority admission deadline expired; no inference submitted"
                    )
                    .context(HelperUnavailable));
                }
                #[cfg(test)]
                if let Some(waiting) = self.waiting.lock().unwrap().as_ref() {
                    let _ = waiting.send(());
                }
                let (next, _) = self
                    .changed
                    .wait_timeout(state, deadline.saturating_duration_since(now))
                    .map_err(|_| anyhow::anyhow!("pool authority unavailable"))?;
                state = next;
            }
            ensure!(
                state.attempts < 3 && Instant::now() >= state.retry_at,
                HelperUnavailable
            );
            state.connecting = true;
            state.attempts += 1;
            state.retry_at = Instant::now() + Duration::from_secs(1 << state.attempts);
            (
                state
                    .access
                    .clone()
                    .context("ready helper authority missing")?,
                state.generation,
            )
        };
        // Handshake is outside the authority mutex. Updates/close wake pending
        // callers and fence this result; no request is replayed here.
        let connected = Client::new_before(access, deadline).map(Arc::new);
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("pool authority unavailable"))?;
        state.connecting = false;
        self.changed.notify_all();
        ensure!(
            !state.closed && state.generation == generation,
            "embedding authority changed during connection"
        );
        let client = connected?;
        admission_remaining(deadline)?;
        state.client = Some(client.clone());
        Ok(client)
    }
}
struct ModelBinding {
    client: Arc<Client>,
    session: Arc<dyn RemoteSession>,
}
struct RebindingModel {
    hub: Arc<Hub>,
    location: ModelLocation,
    identity: String,
    binding: Mutex<ModelBinding>,
}
impl RemoteSession for RebindingModel {
    fn request(&self, request: Request) -> Result<Response> {
        let client = self.hub.client()?;
        let mut binding = self
            .binding
            .lock()
            .map_err(|_| anyhow::anyhow!("model session unavailable"))?;
        if !Arc::ptr_eq(&client, &binding.client) {
            let session = PoolConnector {
                client: client.clone(),
            }
            .open(self.location.clone(), &self.identity)?;
            *binding = ModelBinding { client, session };
        }
        // An uncertain request is never replayed. Only a later call may reopen the descriptor.
        binding.session.request(request)
    }
}
struct HubConnector(Arc<Hub>);
impl Connector for HubConnector {
    fn provision_model(&self, source: &Path) -> Result<airk_core::model_objects::ModelObjectSet> {
        PoolConnector {
            client: self.0.client()?,
        }
        .provision_model(source)
    }
    fn provision_bundled(&self) -> Result<airk_core::model_objects::ModelObjectSet> {
        PoolConnector {
            client: self.0.client()?,
        }
        .provision_bundled()
    }
    fn tokenizer(&self, path: &Path) -> Result<Arc<dyn RemoteSession>> {
        PoolConnector {
            client: self.0.client()?,
        }
        .tokenizer(path)
    }
    fn open(&self, location: ModelLocation, identity: &str) -> Result<Arc<dyn RemoteSession>> {
        let client = self.0.client()?;
        let session = PoolConnector {
            client: client.clone(),
        }
        .open(location.clone(), identity)?;
        Ok(Arc::new(RebindingModel {
            hub: self.0.clone(),
            location,
            identity: identity.into(),
            binding: Mutex::new(ModelBinding { client, session }),
        }))
    }
}
static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
pub fn install(
    runtime: &Path,
    identity: airk_core::service::WorkerIdentity,
    access: Option<protocol::Access>,
    pending: bool,
) -> Result<()> {
    let hub = Arc::new(Hub::new(runtime, identity)?);
    hub.update_pending(access, pending)?;
    ensure!(
        HUB.set(hub.clone()).is_ok(),
        "pool connector already installed"
    );
    embedding_worker::install_connector(Arc::new(HubConnector(hub)))
}
pub fn update(access: Option<protocol::Access>) -> Result<()> {
    HUB.get()
        .context("pool connector not installed")?
        .update(access)
}

pub fn update_pending(access: Option<protocol::Access>, pending: bool) -> Result<()> {
    HUB.get()
        .context("pool connector not installed")?
        .update_pending(access, pending)
}
pub fn close() {
    if let Some(hub) = HUB.get() {
        hub.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn frame_deadlines_keep_helper_kind_and_protocol_errors_stay_permanent() -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let (_sender, mut receiver) = tokio::io::duplex(32);
        let error = local_ipc::read_message::<_, serde_json::Value>(&mut receiver, Duration::ZERO)
            .await
            .expect_err("silent peer must expire");
        assert!(error.is::<tokio::time::error::Elapsed>(), "{error:#}");
        let error = transient_helper_error(error);
        assert!(error.is::<HelperUnavailable>(), "{error:#}");
        assert!(error.is::<tokio::time::error::Elapsed>(), "{error:#}");

        let (mut sender, _receiver) = tokio::io::duplex(1);
        let error = local_ipc::write_message(&mut sender, &vec![1u8; 32], Duration::ZERO)
            .await
            .expect_err("undrained peer must expire");
        assert!(error.is::<tokio::time::error::Elapsed>(), "{error:#}");
        let error = transient_helper_error(error);
        assert!(error.is::<HelperUnavailable>(), "{error:#}");
        assert!(error.is::<tokio::time::error::Elapsed>(), "{error:#}");

        let (mut sender, mut receiver) = tokio::io::duplex(8);
        sender.write_all(&0u32.to_be_bytes()).await?;
        let error =
            local_ipc::read_message::<_, serde_json::Value>(&mut receiver, Duration::from_secs(1))
                .await
                .expect_err("invalid frame must be refused");
        let error = transient_helper_error(error);
        assert!(!error.is::<HelperUnavailable>(), "{error:#}");
        assert!(!error.is::<tokio::time::error::Elapsed>(), "{error:#}");
        assert!(error.to_string().contains("invalid native frame length"));
        Ok(())
    }

    fn fixture_access(runtime: &Path) -> Result<protocol::Access> {
        let instance = airk_kernel::service_broker::new_instance_id()?;
        Ok(protocol::Access {
            in_process: false,
            runtime: runtime.into(),
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
        })
    }
    fn fixture_hub(access: &protocol::Access) -> Result<Arc<Hub>> {
        Ok(Arc::new(Hub::new(
            &access.runtime,
            airk_core::service::WorkerIdentity {
                service: access.grant.service,
                worker: access.grant.worker,
                project: access.grant.project.binding,
                fencing_generation: 1,
            },
        )?))
    }
    #[test]
    fn authority_updates_are_lazy_and_reject_foreign_capabilities() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let access = fixture_access(directory.path())?;
        let hub = fixture_hub(&access)?;
        hub.update(Some(access.clone()))?; // No endpoint exists: control performs no connection.
        for field in 0..5 {
            let mut foreign = access.clone();
            let other = airk_kernel::service_broker::new_instance_id()?;
            match field {
                0 => foreign.grant.service = other,
                1 => foreign.grant.worker = other,
                2 => {
                    foreign.grant.project = airk_core::service::ProjectRegistration::new(
                        "/other".into(),
                        "/state".into(),
                        "fixture".into(),
                    )?
                }
                3 => foreign.role = protocol::role(other),
                _ => foreign.runtime = directory.path().join("missing"),
            }
            assert!(hub.update(Some(foreign)).is_err());
            assert_eq!(hub.state.lock().unwrap().generation, 1);
        }
        hub.state.lock().unwrap().attempts = 3;
        hub.update(Some(access))?;
        assert!(hub.client().is_err()); // Duplicate grants cannot reset exhaustion.
        hub.update(None)?;
        assert!(hub.state.lock().unwrap().access.is_none());
        Ok(())
    }
    #[test]
    fn authority_deadline_does_not_spend_connection_retry_budget() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let access = fixture_access(directory.path())?;
        let hub = fixture_hub(&access)?;
        hub.update_pending(None, true)?;
        let error = hub
            .client_before(Instant::now())
            .err()
            .context("pending wait unexpectedly succeeded")?;
        assert!(error.is::<HelperUnavailable>());
        assert!(format!("{error:#}").contains("before inference submission"));
        let state = hub.state.lock().unwrap();
        assert_eq!(state.attempts, 0);
        assert!(!state.connecting);
        Ok(())
    }
    #[test]
    fn pending_authority_wait_expires_at_its_original_deadline_without_connecting() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let access = fixture_access(directory.path())?;
        let hub = fixture_hub(&access)?;
        hub.update_pending(None, true)?;
        let started = Instant::now();
        let deadline = started + Duration::from_millis(100);
        let error = hub
            .client_before(deadline)
            .err()
            .context("unpublished authority became a client")?;
        assert!(error.is::<HelperUnavailable>());
        assert!(
            Instant::now() >= deadline,
            "pending authority must actually wait until the original deadline"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let state = hub.state.lock().unwrap();
        assert_eq!(state.attempts, 0);
        assert!(!state.connecting);
        assert!(state.client.is_none());
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn held_ready_uses_original_admission_deadline_and_sends_no_operation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let access = fixture_access(&root)?;
        let mut listener =
            local_ipc::Listener::bind(&local_ipc::endpoint(&access.runtime, &access.role)?)?;
        let (entered, started) = tokio::sync::oneshot::channel();
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let _: protocol::Grant = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("missing grant")?;
            let _ = entered.send(());
            // Withhold Ready. The actual client transport must close when its
            // original allowance expires, rather than waiting another five seconds.
            let operation: Option<protocol::Envelope> =
                local_ipc::read_message(&mut stream, Duration::from_secs(3)).await?;
            ensure!(operation.is_none(), "unadmitted helper sent an operation");
            Ok::<_, anyhow::Error>(())
        });
        let hub = fixture_hub(&access)?;
        hub.update(Some(access))?;
        let deadline = Instant::now() + Duration::from_secs(1);
        let joining = hub.clone();
        let call = tokio::task::spawn_blocking(move || joining.client_before(deadline));
        tokio::time::timeout(Duration::from_secs(5), started).await??;
        let error = call
            .await?
            .err()
            .context("unready helper became a client")?;
        assert!(error.is::<HelperUnavailable>(), "{error:#}");
        assert!(Instant::now() >= deadline);
        {
            let state = hub.state.lock().unwrap();
            assert_eq!(state.attempts, 1);
            assert!(!state.connecting);
            assert!(state.client.is_none());
        }
        peer.await??;
        hub.close();
        Ok(())
    }
    #[test]
    fn pending_wait_wakes_on_suppression_and_close_without_submission() -> Result<()> {
        for close in [false, true] {
            let directory = tempfile::tempdir()?;
            let access = fixture_access(directory.path())?;
            let hub = fixture_hub(&access)?;
            hub.update_pending(None, true)?;
            let (waiting, observed) = mpsc::channel();
            *hub.waiting.lock().unwrap() = Some(waiting);
            let other = hub.clone();
            let call = std::thread::spawn(move || other.client());
            observed.recv_timeout(Duration::from_secs(5))?;
            if close {
                hub.close();
            } else {
                hub.update(None)?;
            }
            let error = call
                .join()
                .map_err(|_| anyhow::anyhow!("waiter panic"))?
                .err()
                .context("terminal state became a client")?;
            assert!(error.is::<HelperUnavailable>());
            assert!(format!("{error:#}").contains(if close {
                "owner closed"
            } else {
                "unavailable or revoked"
            }));
            assert_eq!(hub.state.lock().unwrap().attempts, 0);
            if close {
                assert!(hub.update(Some(access)).is_err());
            }
        }
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delayed_authority_coalesces_four_first_connections_without_inference() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let access = fixture_access(&root)?;
        let peer = fixture_pool(access.clone(), 0).await?;
        let hub = fixture_hub(&access)?;
        hub.update_pending(None, true)?;
        let (waiting, observed) = mpsc::channel();
        *hub.waiting.lock().unwrap() = Some(waiting);
        let mut calls = Vec::new();
        for _ in 0..4 {
            let hub = hub.clone();
            calls.push(tokio::task::spawn_blocking(move || hub.client()));
        }
        tokio::task::spawn_blocking(move || -> Result<()> {
            for _ in 0..4 {
                observed.recv_timeout(Duration::from_secs(5))?;
            }
            Ok(())
        })
        .await??;
        assert_eq!(hub.state.lock().unwrap().attempts, 0);
        hub.update(Some(access))?;
        let mut clients = Vec::new();
        for call in calls {
            clients.push(call.await??);
        }
        assert!(
            clients
                .iter()
                .all(|client| Arc::ptr_eq(client, &clients[0]))
        );
        assert_eq!(hub.state.lock().unwrap().attempts, 1);
        assert!(
            hub.client_before(Instant::now()).is_err(),
            "cached connection cannot renew an expired admission"
        );
        assert_eq!(hub.state.lock().unwrap().attempts, 1);
        hub.close();
        drop(clients);
        drop(hub);
        assert!(peer.await??.is_empty());
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn replacement_during_handshake_cannot_publish_or_submit_old_authority() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let old = fixture_access(&root)?;
        let mut replacement = old.clone();
        replacement.grant.pool = airk_kernel::service_broker::new_instance_id()?;
        replacement.grant.token = airk_core::identity::Sha256Digest::from_bytes([72; 32]);
        replacement.role = protocol::role(replacement.grant.pool);
        let next_peer = fixture_pool(replacement.clone(), 0).await?;
        let mut listener =
            local_ipc::Listener::bind(&local_ipc::endpoint(&old.runtime, &old.role)?)?;
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let old_access = old.clone();
        let old_peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let _: protocol::Grant = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("missing old grant")?;
            let _ = entered.send(());
            wait.await?;
            local_ipc::write_message(
                &mut stream,
                &protocol::Observation::Ready {
                    pool: old_access.grant.pool,
                    role: old_access.role,
                },
                Duration::from_secs(5),
            )
            .await?;
            let operation: Option<protocol::Envelope> =
                local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?;
            ensure!(
                operation.is_none(),
                "retired handshake submitted an operation"
            );
            Ok::<_, anyhow::Error>(())
        });
        let hub = fixture_hub(&old)?;
        hub.update(Some(old))?;
        let joining = hub.clone();
        let first = tokio::task::spawn_blocking(move || joining.client());
        started.await?;
        hub.update_pending(None, true)?;
        hub.update(Some(replacement))?;
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("old peer disappeared"))?;
        let error = first
            .await?
            .err()
            .context("retired connection became current")?;
        assert!(format!("{error:#}").contains("authority changed"));
        assert!(hub.state.lock().unwrap().client.is_none());
        assert_eq!(hub.state.lock().unwrap().attempts, 0);
        let joining = hub.clone();
        let current = tokio::task::spawn_blocking(move || joining.client()).await??;
        hub.close();
        drop(current);
        drop(hub);
        old_peer.await??;
        assert!(next_peer.await??.is_empty());
        Ok(())
    }
    async fn fixture_pool(
        access: protocol::Access,
        bytes: u64,
    ) -> Result<tokio::task::JoinHandle<Result<Vec<String>>>> {
        let mut listener =
            local_ipc::Listener::bind(&local_ipc::endpoint(&access.runtime, &access.role)?)?;
        Ok(tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let _: protocol::Grant = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("fixture grant missing")?;
            local_ipc::write_message(
                &mut stream,
                &protocol::Observation::Ready {
                    pool: access.grant.pool,
                    role: access.role,
                },
                Duration::from_secs(5),
            )
            .await?;
            let mut opened = Vec::new();
            while let Some(envelope) = local_ipc::read_message_limited::<_, protocol::Envelope>(
                &mut stream,
                Duration::from_secs(10),
                MAX_FRAME_BYTES,
            )
            .await?
            {
                let response = match envelope.operation {
                    protocol::Operation::Call { request } => match *request {
                        Request::Open {
                            protocol,
                            version,
                            identity,
                            ..
                        } => {
                            opened.push(identity.clone());
                            Response::Opened {
                                protocol,
                                version,
                                identity,
                            }
                        }
                        Request::Peak => Response::Peak { bytes },
                        _ => anyhow::bail!("unexpected fixture operation"),
                    },
                    protocol::Operation::Close => Response::Peak { bytes: 0 },
                };
                local_ipc::write_message_limited(
                    &mut stream,
                    &protocol::ResultEnvelope {
                        project: envelope.project,
                        worker: envelope.worker,
                        session: envelope.session,
                        request_id: envelope.request_id,
                        input_digest: envelope.input_digest,
                        response,
                    },
                    Duration::from_secs(5),
                    MAX_FRAME_BYTES,
                )
                .await?;
            }
            Ok(opened)
        }))
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retained_model_session_rebinds_without_replacing_project_worker() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let first = fixture_access(&root)?;
        let mut second = first.clone();
        second.grant.pool = airk_kernel::service_broker::new_instance_id()?;
        second.grant.token = airk_core::identity::Sha256Digest::from_bytes([2; 32]);
        second.role = protocol::role(second.grant.pool);
        let first_peer = fixture_pool(first.clone(), 11).await?;
        let second_peer = fixture_pool(second.clone(), 22).await?;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let hub = fixture_hub(&first)?;
            hub.update(Some(first))?;
            let model = HubConnector(hub.clone()).open(
                ModelLocation::Directory {
                    path: root.join("immutable-model"),
                },
                "retained-model-identity",
            )?;
            assert!(matches!(
                model.request(Request::Peak)?,
                Response::Peak { bytes: 11 }
            ));
            hub.update(None)?;
            assert!(model.request(Request::Peak).is_err());
            hub.update(Some(second))?;
            assert!(matches!(
                model.request(Request::Peak)?,
                Response::Peak { bytes: 22 }
            ));
            assert!(matches!(
                model.request(Request::Peak)?,
                Response::Peak { bytes: 22 }
            ));
            Ok(())
        })
        .await??;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), first_peer).await???,
            ["retained-model-identity"]
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), second_peer).await???,
            ["retained-model-identity"]
        );
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_client_queue_preserves_close_and_cannot_reopen_a_retired_session() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let instance = airk_kernel::service_broker::new_instance_id()?;
        let access = protocol::Access {
            in_process: false,
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
                        protocol::Operation::Call { ref request } if matches!(**request, Request::Open { .. })
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
                    request: Box::new(Request::Peak),
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
                    operation: protocol::Operation::Call {
                        request: Box::new(request),
                    },
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
