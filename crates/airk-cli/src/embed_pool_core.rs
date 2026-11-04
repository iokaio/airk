// SPDX-License-Identifier: Apache-2.0
// Canonical inference-only pool implementation included by the production
// wrapper and explicit instrumented fixture. Unit harness modules live in the wrapper.
use airk_cli::{
    embedding_pool_protocol as protocol,
    local_ipc::{self, Listener, Stream},
    service_ipc,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::ProjectRegistration,
};
use airk_retrieval::{
    embedding_worker::{MAX_FRAME_BYTES, ModelLocation, PROTOCOL, Request, Response},
    model::{self, EmbeddingModel},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
static PROCESS_LEASE: OnceLock<airk_store::service::EmbeddingLease> = OnceLock::new();
use tokio::{
    sync::{Semaphore, mpsc, oneshot},
    task::JoinSet,
};

#[derive(Clone)]
enum Binding {
    Model {
        location: Box<ModelLocation>,
        identity: String,
        key: Sha256Digest,
        files: Arc<airk_store::service::model_files::ModelFileLease>,
    },
    Tokenizer {
        path: PathBuf,
    },
}
struct Session {
    binding: Binding,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ComputeContext {
    pub worker: Option<airk_core::service::WorkerIdentity>,
    pub helper: Incarnation,
    pub request_id: Incarnation,
    pub session: Incarnation,
}
#[cfg(test)]
type ComputeHook = Box<dyn FnMut(Option<&ComputeContext>, &Request) + Send>;

pub(crate) struct Engine {
    runtime: PathBuf,
    owners: BTreeMap<Incarnation, Weak<AtomicBool>>,
    sessions: BTreeMap<(Incarnation, Incarnation), Session>,
    model: Option<EmbeddingModel>,
    model_lease: Option<Arc<airk_store::service::model_files::ModelFileLease>>,
    model_files: airk_store::service::model_files::ServiceModelFiles,
    model_key: Option<Sha256Digest>,
    tokenizer: Option<airk_kernel::usage::OfflineTokenizer>,
    tokenizer_path: Option<PathBuf>,
    loads: u64,
    reuses: u64,
    used: Instant,
    inference_threads: Arc<AtomicUsize>,
    #[cfg(test)]
    before_compute: Option<ComputeHook>,
    #[cfg(test)]
    compute_context: Option<ComputeContext>,
    #[cfg(test)]
    after_model_selection: Option<Box<dyn FnMut() + Send>>,
}

#[derive(Clone, Copy)]
struct RequestOwner<'a> {
    connection: &'a AtomicBool,
    worker: &'a AtomicBool,
    closing: &'a AtomicBool,
}
fn require_live(owner: Option<RequestOwner<'_>>) -> Result<()> {
    ensure!(
        owner.is_none_or(|owner| owner.connection.load(Ordering::Acquire)
            && owner.worker.load(Ordering::Acquire)
            && !owner.closing.load(Ordering::Acquire)),
        "pool request canceled"
    );
    Ok(())
}

fn location_key(
    project: &ProjectRegistration,
    location: &ModelLocation,
    identity: &str,
    runtime: &Path,
) -> Result<Sha256Digest> {
    let data = Path::new(&project.data_root);
    match location {
        ModelLocation::Shared { objects } => {
            ensure!(
                objects.legacy_identity == identity,
                "shared model identity mismatch"
            );
            let source = airk_store::service::model_files::InstalledModel::open(runtime, objects)?;
            model::descriptor_from_asset_metadata(&source, objects)?.identity()
        }
        ModelLocation::Directory { path } => {
            ensure!(
                path.canonicalize()? == data.join("models/local").canonicalize()?,
                "model directory is outside admitted project storage"
            );
            ensure!(
                model::inspect(path)?.identity == identity,
                "model identity mismatch"
            );
            model::inspect_cache_descriptor(path)?.identity()
        }
        ModelLocation::Retained {
            collection,
            objects,
        } => {
            ensure!(
                airk_store::service::collection::validate_location(
                    collection,
                    data,
                    &project.collection
                )
                .is_ok(),
                "model objects are outside admitted collection"
            );
            ensure!(
                objects.legacy_identity == identity,
                "retained model identity mismatch"
            );
            let _lease = airk_store::FileStore::new(collection).pin_service_objects()?;
            model::inspect_objects_cache_descriptor(collection, objects)?.identity()
        }
    }
}

impl Engine {
    fn admit_owner(&mut self, worker: Incarnation, alive: &Arc<AtomicBool>) -> Result<()> {
        // A queued request from an expired connection cannot retire a repaired owner.
        ensure!(alive.load(Ordering::Acquire), "pool request canceled");
        self.retire_disconnected();
        let owner = Arc::downgrade(alive);
        if self
            .owners
            .get(&worker)
            .is_some_and(|old| !old.ptr_eq(&owner))
        {
            self.sessions.retain(|(member, _), _| *member != worker);
        }
        self.owners.insert(worker, owner);
        Ok(())
    }

    fn release_owner(&mut self, worker: Incarnation, expected: Option<&Weak<AtomicBool>>) {
        if expected.is_none_or(|expected| {
            self.owners
                .get(&worker)
                .is_some_and(|owner| owner.ptr_eq(expected))
        }) {
            self.owners.remove(&worker);
            self.sessions.retain(|(member, _), _| *member != worker);
        }
    }

    pub(crate) fn new(runtime: &Path) -> Result<Self> {
        Ok(Self {
            runtime: runtime.canonicalize()?,
            owners: BTreeMap::new(),
            sessions: BTreeMap::new(),
            model: None,
            model_lease: None,
            model_files: airk_store::service::model_files::ServiceModelFiles::open(runtime)?,
            model_key: None,
            tokenizer: None,
            tokenizer_path: None,
            loads: 0,
            reuses: 0,
            used: Instant::now(),
            inference_threads: Arc::new(AtomicUsize::new(1)),
            #[cfg(test)]
            before_compute: None,
            #[cfg(test)]
            compute_context: None,
            #[cfg(test)]
            after_model_selection: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_compute_hook(mut self, hook: ComputeHook) -> Self {
        self.before_compute = Some(hook);
        self
    }

    fn retire_disconnected(&mut self) {
        let dead: Vec<_> = self
            .owners
            .iter()
            .filter_map(|(worker, alive)| {
                alive
                    .upgrade()
                    .is_none_or(|alive| !alive.load(Ordering::Acquire))
                    .then_some(*worker)
            })
            .collect();
        for worker in dead {
            self.sessions.retain(|(owner, _), _| *owner != worker);
            self.owners.remove(&worker);
        }
    }

    fn select_model(&mut self, binding: &Binding, owner: Option<RequestOwner<'_>>) -> Result<()> {
        let Binding::Model {
            location,
            identity,
            key,
            files,
        } = binding
        else {
            anyhow::bail!("not a model session")
        };
        if self.model_key == Some(*key) && self.model.is_some() {
            return Ok(());
        }
        // Retire incompatible inference state before allocating its replacement.
        self.model = None;
        self.model_lease = None;
        self.model_key = None;
        require_live(owner)?;
        super::handle(
            Request::Open {
                protocol: PROTOCOL,
                version: env!("CARGO_PKG_VERSION").into(),
                location: *location.clone(),
                identity: identity.clone(),
            },
            &mut self.model,
            &mut None,
        )?;
        self.model_key = Some(*key);
        self.model_lease = Some(files.clone());
        self.loads = self.loads.saturating_add(1);
        Ok(())
    }

    fn select_tokenizer(&mut self, path: &Path, owner: Option<RequestOwner<'_>>) -> Result<()> {
        if self.tokenizer_path.as_deref() == Some(path) && self.tokenizer.is_some() {
            return Ok(());
        }
        self.tokenizer = None;
        self.tokenizer_path = None;
        require_live(owner)?;
        super::handle(
            Request::TokenizerOpen {
                protocol: PROTOCOL,
                version: env!("CARGO_PKG_VERSION").into(),
                path: path.into(),
            },
            &mut None,
            &mut self.tokenizer,
        )?;
        self.tokenizer_path = Some(path.into());
        Ok(())
    }

    fn request_guarded(
        &mut self,
        grant: &protocol::Grant,
        session: Incarnation,
        request: Request,
        owner: Option<RequestOwner<'_>>,
    ) -> Result<Response> {
        require_live(owner)?;
        let key = (grant.worker, session);
        self.used = Instant::now();
        match request {
            Request::ProvisionModel { directory } => {
                ensure!(
                    directory.is_absolute() && directory.to_str().is_some_and(|p| p.len() <= 4096),
                    "model source must be a bounded absolute path"
                );
                let directory = directory.canonicalize()?;
                let set = model::inspect_object_plan(&directory)?;
                self.model = None;
                self.model_lease = None;
                self.model_key = None;
                let _lease = self
                    .model_files
                    .retain(&set, |name| directory.join(name.filename()))?;
                let _installed = self.model_files.install_readers(&set, |_| {
                    anyhow::bail!("verified model unexpectedly requires another copy")
                })?;
                Ok(Response::Provisioned {
                    objects: self.model_files.installed_reference(&set)?,
                })
            }
            Request::ProvisionBundled => {
                let set = airk_cli::bundled_assets::EmbeddedAssets::plan()?;
                if self.model_files.installed_reference(&set).is_ok() {
                    // Reusing installed files must not unload a compatible resident model.
                    return Ok(Response::Provisioned {
                        objects: self.model_files.publish_default(&set)?,
                    });
                }
                self.model = None;
                self.model_lease = None;
                self.model_key = None;
                let _lease = self.model_files.install_readers(&set, |name| {
                    Ok(Box::new(std::io::Cursor::new(
                        airk_cli::bundled_assets::EmbeddedAssets::bytes(name)?,
                    )))
                })?;
                Ok(Response::Provisioned {
                    objects: self.model_files.publish_default(&set)?,
                })
            }
            Request::Open {
                protocol,
                version,
                location,
                identity,
            } => {
                ensure!(
                    protocol == PROTOCOL && version == env!("CARGO_PKG_VERSION"),
                    "pool protocol mismatch"
                );
                ensure!(
                    !self.sessions.contains_key(&key) && self.sessions.len() < 64,
                    "pool session capacity or duplicate open"
                );
                let pipeline = location_key(&grant.project, &location, &identity, &self.runtime)?;
                let _source_pin = match &location {
                    ModelLocation::Retained { collection, .. } => {
                        Some(airk_store::FileStore::new(collection).pin_service_objects()?)
                    }
                    ModelLocation::Directory { .. } | ModelLocation::Shared { .. } => None,
                };
                let set = match &location {
                    ModelLocation::Retained { objects, .. } | ModelLocation::Shared { objects } => {
                        objects.clone()
                    }
                    ModelLocation::Directory { path } => model::inspect_object_plan(path)?,
                };
                ensure!(
                    set.pipeline_digest == pipeline,
                    "model changed during shared-file planning"
                );
                let files = if matches!(&location, ModelLocation::Shared { .. }) {
                    self.model_files.installed(&set)?
                } else {
                    self.model_files.retain(&set, |name| match &location {
                        ModelLocation::Directory { path } => path.join(name.filename()),
                        ModelLocation::Retained { collection, .. } => collection
                            .join("objects/models")
                            .join(set.assets[&name].sha256.to_string()),
                        ModelLocation::Shared { .. } => unreachable!("handled by installed store"),
                    })?
                };
                drop(_source_pin);
                let binding = Binding::Model {
                    location: Box::new(ModelLocation::Directory {
                        path: files.directory().into(),
                    }),
                    identity: identity.clone(),
                    key: pipeline,
                    files,
                };
                if self.model_key == Some(pipeline) && self.model.is_some() {
                    self.reuses = self.reuses.saturating_add(1);
                }
                self.select_model(&binding, owner)?;
                self.sessions.insert(key, Session { binding });
                Ok(Response::Opened {
                    protocol,
                    version,
                    identity,
                })
            }
            Request::TokenizerOpen {
                protocol,
                version,
                path,
            } => {
                ensure!(
                    protocol == PROTOCOL && version == env!("CARGO_PKG_VERSION"),
                    "tokenizer protocol mismatch"
                );
                ensure!(
                    !self.sessions.contains_key(&key) && self.sessions.len() < 64,
                    "pool session capacity"
                );
                // An explicitly selected calibration input is captured once in
                // the service-owned finite cache; sessions never bind project files.
                let path = self.model_files.retain_calibration(&path)?;
                // Captured bytes verify on every opening. Reuse compatible
                // native state and drop incompatible state before loading more.
                self.select_tokenizer(&path, owner)?;
                let verified = self
                    .tokenizer
                    .as_ref()
                    .context("calibration tokenizer missing")?;
                let response = Response::TokenizerOpened {
                    identity: verified.identity().clone(),
                    description: verified.description(),
                };
                self.sessions.insert(
                    key,
                    Session {
                        binding: Binding::Tokenizer { path },
                    },
                );
                Ok(response)
            }
            Request::Extract { .. } => anyhow::bail!("pool cannot extract or install assets"),
            request => {
                let session = self
                    .sessions
                    .get_mut(&key)
                    .context("pool session expired")?;
                let binding = session.binding.clone();
                match &binding {
                    Binding::Model { .. } => {
                        ensure!(
                            matches!(
                                request,
                                Request::Plan { .. } | Request::Embed { .. } | Request::Peak
                            ),
                            "invalid model operation"
                        );
                        self.select_model(&binding, owner)?;
                        #[cfg(test)]
                        if let Some(hook) = self.after_model_selection.as_mut() {
                            hook();
                        }
                        let request = match request {
                            Request::Embed {
                                texts,
                                workers,
                                memory_bytes,
                            } => {
                                ensure!(
                                    (1..=8).contains(&workers)
                                        && (8 << 20..=4 << 30).contains(&memory_bytes),
                                    "invalid pool inference budget"
                                );
                                Request::Embed {
                                    texts,
                                    workers: workers
                                        .min(self.inference_threads.load(Ordering::Acquire)),
                                    memory_bytes: memory_bytes.min(512 << 20),
                                }
                            }
                            request => request,
                        };
                        // Model selection may perform a slow native load. Recheck
                        // the authoritative fence after it, at actual inference.
                        require_live(owner)?;
                        #[cfg(test)]
                        if let Some(hook) = self.before_compute.as_mut() {
                            hook(self.compute_context.as_ref(), &request);
                        }
                        super::handle(request, &mut self.model, &mut None)
                    }
                    Binding::Tokenizer { path } => {
                        ensure!(
                            matches!(&request,Request::TokenCount{text} if text.len()<=262144),
                            "invalid tokenizer input"
                        );
                        self.select_tokenizer(path, owner)?;
                        require_live(owner)?;
                        #[cfg(test)]
                        if let Some(hook) = self.before_compute.as_mut() {
                            hook(self.compute_context.as_ref(), &request);
                        }
                        super::handle(request, &mut None, &mut self.tokenizer)
                    }
                }
            }
        }
    }
}

enum Work {
    Sweep,
    Request {
        grant: Box<protocol::Grant>,
        envelope: Box<protocol::Envelope>,
        alive: Arc<AtomicBool>,
        worker_alive: Arc<AtomicBool>,
        reply: oneshot::Sender<protocol::ResultEnvelope>,
        #[cfg(test)]
        native_worker: Option<airk_core::service::WorkerIdentity>,
    },
    Release {
        worker: Incarnation,
        owner: Option<Weak<AtomicBool>>,
    },
    DrainRelease {
        worker: Incarnation,
        completed: Arc<AtomicBool>,
    },
}
struct Authorization {
    grant: protocol::Grant,
    used: bool,
    expires: Instant,
    alive: Arc<AtomicBool>,
    worker_alive: Arc<AtomicBool>,
}

fn register_authorization(
    grants: &mut BTreeMap<Sha256Digest, Authorization>,
    service: Incarnation,
    pool: Incarnation,
    grant: protocol::Grant,
    now: Instant,
) -> protocol::Observation {
    // Validate the entire replacement before revoking any current capability.
    // In particular, even a duplicate token must not reset its consumed state.
    if grant.service != service
        || grant.pool != pool
        || grant.project.validate().is_err()
        || grants.contains_key(&grant.token)
        || grants
            .values()
            .filter(|authorization| {
                authorization.grant.worker != grant.worker
                    && authorization.alive.load(Ordering::Acquire)
                    && now < authorization.expires
            })
            .take(32)
            .count()
            >= 32
    {
        return protocol::Observation::Failed;
    }
    grants.retain(|_, authorization| {
        let retain = authorization.grant.worker != grant.worker
            && authorization.alive.load(Ordering::Acquire)
            && now < authorization.expires;
        if !retain {
            authorization.alive.store(false, Ordering::Release);
        }
        retain
    });
    grants.insert(
        grant.token,
        Authorization {
            grant,
            used: false,
            expires: now + Duration::from_secs(300),
            alive: Arc::new(AtomicBool::new(true)),
            worker_alive: Arc::new(AtomicBool::new(true)),
        },
    );
    // Old sockets are fenced by their own alive flag and exact-owner cleanup.
    // An unfenced worker-wide Release here could retire the replacement later.
    protocol::Observation::Registered
}

struct State {
    service: Incarnation,
    pool: Incarnation,
    role: String,
    grants: Mutex<BTreeMap<Sha256Digest, Authorization>>,
    retirement: Mutex<retirement::Retirements>,
    sender: mpsc::Sender<Work>,
    closing: Arc<AtomicBool>,
}

fn drain_control(
    state: &State,
    action: retirement::Control,
    ticket: protocol::ProviderDrainTicket,
) -> Result<protocol::Observation> {
    let mut retirement = state
        .retirement
        .lock()
        .map_err(|_| anyhow::anyhow!("pool membership unavailable"))?;
    let mut grants = state
        .grants
        .lock()
        .map_err(|_| anyhow::anyhow!("pool authorization unavailable"))?;
    Ok(retirement.control(
        action,
        (state.service, state.pool),
        &mut grants,
        &state.sender,
        ticket,
    ))
}

fn process_work(engine: &mut Engine, work: Work, closing: &AtomicBool) {
    match work {
        Work::Sweep => {
            engine.retire_disconnected();
            if engine.sessions.is_empty() && engine.used.elapsed() > Duration::from_secs(60) {
                engine.model = None;
                engine.model_lease = None;
                engine.model_key = None;
                engine.tokenizer = None;
                engine.tokenizer_path = None;
            }
        }
        Work::Request {
            grant,
            envelope,
            alive,
            worker_alive,
            reply,
            #[cfg(test)]
            native_worker,
        } => {
            let envelope = *envelope;
            #[cfg(test)]
            {
                engine.compute_context = Some(ComputeContext {
                    worker: native_worker,
                    helper: grant.pool,
                    request_id: envelope.request_id,
                    session: envelope.session,
                });
            }
            let response = (|| {
                ensure!(
                    alive.load(Ordering::Acquire)
                        && worker_alive.load(Ordering::Acquire)
                        && !closing.load(Ordering::Acquire),
                    "pool request canceled"
                );
                engine.admit_owner(grant.worker, &alive)?;
                // Queue-waiting work retains both exact connection and native
                // worker fences. No inference starts after its Release barrier.
                ensure!(
                    alive.load(Ordering::Acquire)
                        && worker_alive.load(Ordering::Acquire)
                        && !closing.load(Ordering::Acquire),
                    "pool request canceled"
                );
                match envelope.operation {
                    protocol::Operation::Call { request } => engine.request_guarded(
                        &grant,
                        envelope.session,
                        *request,
                        Some(RequestOwner {
                            connection: &alive,
                            worker: &worker_alive,
                            closing,
                        }),
                    ),
                    protocol::Operation::Close => {
                        engine.sessions.remove(&(grant.worker, envelope.session));
                        Ok(Response::Peak { bytes: 0 })
                    }
                }
            })()
            .unwrap_or(Response::Failed {
                reason: "pool request rejected or verified model unavailable".into(),
            });
            // Result errors must not leak this call's observation into later
            // work. This is instrumentation only, never an admission decision.
            #[cfg(test)]
            {
                engine.compute_context = None;
            }
            let _ = reply.send(protocol::ResultEnvelope {
                project: envelope.project,
                worker: envelope.worker,
                session: envelope.session,
                request_id: envelope.request_id,
                input_digest: envelope.input_digest,
                response,
            });
        }
        Work::Release { worker, owner } => engine.release_owner(worker, owner.as_ref()),
        Work::DrainRelease { worker, completed } => {
            engine.release_owner(worker, None);
            // Only the actual engine queue, after earlier compute completes,
            // can publish this exact generation's provider completion witness.
            completed.store(true, Ordering::Release);
        }
    }
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(grant): Option<protocol::Grant> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    let (alive, worker_alive) = {
        let mut grants = state
            .grants
            .lock()
            .map_err(|_| anyhow::anyhow!("pool authorization unavailable"))?;
        let authorization = grants
            .get_mut(&grant.token)
            .context("unknown pool authorization")?;
        ensure!(
            !state.closing.load(Ordering::Acquire)
                && !authorization.used
                && authorization.worker_alive.load(Ordering::Acquire)
                && Instant::now() < authorization.expires
                && authorization.grant.worker == grant.worker
                && authorization.grant.project == grant.project
                && grant.service == state.service
                && grant.pool == state.pool,
            "invalid pool authorization"
        );
        authorization.used = true;
        (
            authorization.alive.clone(),
            authorization.worker_alive.clone(),
        )
    };
    let result = async {
        local_ipc::write_message(
            &mut stream,
            &protocol::Observation::Ready {
                pool: state.pool,
                role: state.role.clone(),
            },
            Duration::from_secs(5),
        )
        .await?;
        while alive.load(Ordering::Acquire)
            && worker_alive.load(Ordering::Acquire)
            && !state.closing.load(Ordering::Acquire)
        {
            let Some(envelope): Option<protocol::Envelope> = local_ipc::read_message_limited(
                &mut stream,
                Duration::from_secs(300),
                MAX_FRAME_BYTES,
            )
            .await?
            else {
                return Ok(());
            };
            ensure!(
                envelope.project == grant.project.binding
                    && envelope.worker == grant.worker
                    && protocol::input_digest(&envelope.operation)? == envelope.input_digest,
                "pool request identity mismatch"
            );
            ensure!(
                alive.load(Ordering::Acquire)
                    && worker_alive.load(Ordering::Acquire)
                    && !state.closing.load(Ordering::Acquire),
                "pool admission revoked"
            );
            let (reply, receiver) = oneshot::channel();
            #[cfg(test)]
            let native_worker = {
                let retirement = state
                    .retirement
                    .lock()
                    .map_err(|_| anyhow::anyhow!("pool membership unavailable"))?;
                retirement.native_identity(grant.worker)
            };
            state
                .sender
                .try_send(Work::Request {
                    grant: Box::new(grant.clone()),
                    envelope: Box::new(envelope),
                    alive: alive.clone(),
                    worker_alive: worker_alive.clone(),
                    reply,
                    #[cfg(test)]
                    native_worker,
                })
                .map_err(|_| anyhow::anyhow!("pool queue is full"))?;
            // Capacity stays charged in the engine until execution completes, even if the peer leaves.
            let response = receiver.await?;
            local_ipc::write_message_limited(
                &mut stream,
                &response,
                Duration::from_secs(5),
                MAX_FRAME_BYTES,
            )
            .await?;
        }
        Ok(())
    }
    .await;
    alive.store(false, Ordering::Release);
    let _ = state.sender.try_send(Work::Release {
        worker: grant.worker,
        owner: Some(Arc::downgrade(&alive)),
    });
    result
}

pub(crate) async fn run_guarded_with_engine(
    bootstrap: Option<airk_cli::embedding_pool_bootstrap::GuardedBootstrap>,
    make_engine: impl FnOnce(&protocol::Init, Arc<AtomicBool>) -> Result<Engine>,
) -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("pool initialization missing")?;
    if let Some(bootstrap) = bootstrap.as_ref() {
        bootstrap.validate_init(&init)?;
    } else {
        ensure!(
            !airk_cli::embedding_pool_bootstrap::shared_mode(&init.runtime)?,
            "activated authority-1 helper requires guarded native startup"
        );
    }
    ensure!(
        (1..=4).contains(&init.inference_threads),
        "invalid pool inference allowance"
    );
    local_ipc::prepare_root(&init.runtime)?;
    ensure!(
        PROCESS_LEASE
            .set(airk_store::service::EmbeddingLease::acquire(&init.runtime)?)
            .is_ok(),
        "embedding process already owns a pool lease"
    );
    let role = protocol::role(init.pool);
    let mut listener = Listener::bind(&local_ipc::endpoint(&init.runtime, &role)?)?;
    let (sender, mut receiver) = mpsc::channel::<Work>(8);
    let closing = Arc::new(AtomicBool::new(false));
    let engine_closing = closing.clone();
    let inference_threads = Arc::new(AtomicUsize::new(usize::from(init.inference_threads)));
    let engine_threads = inference_threads.clone();
    let observation = Arc::new(Mutex::new((0u64, 0u64, 0usize, Instant::now())));
    let engine_observation = observation.clone();
    let model_engine = make_engine(&init, closing.clone())?;
    let engine = std::thread::Builder::new()
        .name("airk-pooled-inference".into())
        .stack_size(8 << 20)
        .spawn(move || {
            let mut engine = model_engine;
            engine.inference_threads = engine_threads;
            while let Some(work) = receiver.blocking_recv() {
                process_work(&mut engine, work, &engine_closing);
                if let Ok(mut observation) = engine_observation.lock() {
                    *observation = (
                        engine.loads,
                        engine.reuses,
                        engine.sessions.len(),
                        Instant::now(),
                    );
                }
            }
        })?;
    let state = Arc::new(State {
        service: init.service,
        pool: init.pool,
        role: role.clone(),
        grants: Mutex::new(BTreeMap::new()),
        retirement: Mutex::new(retirement::Retirements::default()),
        sender,
        closing: closing.clone(),
    });
    service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        &protocol::Observation::Ready {
            pool: init.pool,
            role,
        },
    )?;
    let (control, mut commands) = mpsc::channel::<protocol::Control>(8);
    let input_closing = closing.clone();
    std::thread::Builder::new()
        .name("airk-pool-control".into())
        .spawn(move || {
            while let Ok(Some(command)) = service_ipc::read_frame(&mut std::io::stdin().lock()) {
                if control.blocking_send(command).is_err() {
                    break;
                }
            }
            input_closing.store(true, Ordering::Release);
        })?;
    let slots = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    while !closing.load(Ordering::Acquire) {
        tokio::select! {
            command=commands.recv()=>{
                let observation=match command {
                    Some(protocol::Control::ConfigureThreads{threads})=>{
                        if (1..=4).contains(&threads){inference_threads.store(usize::from(threads),Ordering::Release);protocol::Observation::Renewed}else{protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Renew{worker})=>{
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        let authorization=grants.values_mut().find(|auth|auth.grant.worker==worker&&auth.alive.load(Ordering::Acquire)&&Instant::now()<auth.expires);
                        if let Some(authorization)=authorization {authorization.expires=Instant::now()+Duration::from_secs(300);protocol::Observation::Renewed}else{protocol::Observation::GrantMissing{worker}}
                    },
                    Some(protocol::Control::Register{grant,worker_identity})=>{
                        let mut retirement=state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?;
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        retirement.register(&mut grants,(state.service,state.pool),grant,worker_identity,Instant::now())
                    },
                    Some(protocol::Control::Revoke{worker})=>{
                        let mut retirement=state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?;
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        retirement.revoke(&mut grants,worker);
                        let _=state.sender.try_send(Work::Release{worker,owner:None});
                        protocol::Observation::Revoked
                    },
                    Some(protocol::Control::BeginDrain{ticket})=>drain_control(&state,retirement::Control::Begin,ticket)?,
                    Some(protocol::Control::PollDrain{ticket})=>drain_control(&state,retirement::Control::Poll,ticket)?,
                    Some(protocol::Control::ForgetDrain{ticket})=>drain_control(&state,retirement::Control::Forget,ticket)?,
                    Some(protocol::Control::Status)=>{
                        match observation.lock(){Ok(observation)=>protocol::Observation::Status{loads:observation.0,reuses:observation.1,sessions:observation.2,age_ms:observation.3.elapsed().as_millis().min(u64::MAX as u128) as u64},Err(_)=>protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Stop)|None=>break,
                };
                service_ipc::write_frame(&mut std::io::stdout().lock(),&observation)?;
            },
            stream=listener.accept()=>{
                let stream=match stream{Ok(stream)=>stream,Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some()=>continue,Err(error)=>return Err(error)};
                if let Ok(permit)=slots.clone().try_acquire_owned(){let state=state.clone();connections.spawn(async move{let _permit=permit;connection(stream,state).await});}
            },
            _=connections.join_next(),if !connections.is_empty()=>{},
            _=tick.tick()=>{
                if let Some(bootstrap) = bootstrap.as_ref() { bootstrap.enforce_native_owner()?; }
                state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?.retry(&state.sender);
                if state.sender.capacity()==8{let _=state.sender.try_send(Work::Sweep);}
                let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                grants.retain(|_,auth|{if Instant::now()>=auth.expires{auth.alive.store(false,Ordering::Release);false}else{true}});
            }
        }
    }
    closing.store(true, Ordering::Release);
    // Idle peers may be waiting on a five-minute read deadline. Closing
    // admission must also wake those transports before joining the engine.
    // Aborting a connection never releases the process lease or its charged
    // inference work: the engine drains and joins below before this owner exits.
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    drop(state);
    engine
        .join()
        .map_err(|_| anyhow::anyhow!("pool execution thread failed"))?;
    Ok(())
}
