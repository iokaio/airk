// SPDX-License-Identifier: Apache-2.0
//! Inference-only shared helper. This module is compiled only into airk-embed.
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
struct Engine {
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
    fn new(runtime: &Path) -> Result<Self> {
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
        })
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

    fn select_model(&mut self, binding: &Binding) -> Result<()> {
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

    fn select_tokenizer(&mut self, path: &Path) -> Result<()> {
        if self.tokenizer_path.as_deref() == Some(path) && self.tokenizer.is_some() {
            return Ok(());
        }
        self.tokenizer = None;
        self.tokenizer_path = None;
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

    fn request(
        &mut self,
        grant: &protocol::Grant,
        session: Incarnation,
        request: Request,
    ) -> Result<Response> {
        let key = (grant.worker, session);
        self.used = Instant::now();
        match request {
            Request::ProvisionModel { directory } => {
                ensure!(
                    directory.is_absolute() && directory.to_str().is_some_and(|p| p.len() <= 4096),
                    "model source must be a bounded absolute path"
                );
                let directory = directory.canonicalize()?;
                self.model = None;
                self.model_lease = None;
                self.model_key = None;
                let set = model::inspect_object_plan(&directory)?;
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
                self.select_model(&binding)?;
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
                let path = path.canonicalize()?;
                ensure!(
                    path.starts_with(Path::new(&grant.project.data_root)),
                    "tokenizer outside admitted storage"
                );
                // Verify every opening, even if compatible native state is already resident.
                let verified = airk_kernel::usage::OfflineTokenizer::open(&path)?;
                let response = Response::TokenizerOpened {
                    identity: verified.identity().clone(),
                    description: verified.description(),
                };
                self.tokenizer = Some(verified);
                self.tokenizer_path = Some(path.clone());
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
                        self.select_model(&binding)?;
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
                        super::handle(request, &mut self.model, &mut None)
                    }
                    Binding::Tokenizer { path } => {
                        ensure!(
                            matches!(&request,Request::TokenCount{text} if text.len()<=65536),
                            "invalid tokenizer input"
                        );
                        self.select_tokenizer(path)?;
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
        reply: oneshot::Sender<protocol::ResultEnvelope>,
    },
    Release {
        worker: Incarnation,
    },
}
struct Authorization {
    grant: protocol::Grant,
    used: bool,
    expires: Instant,
    alive: Arc<AtomicBool>,
}
struct State {
    service: Incarnation,
    pool: Incarnation,
    role: String,
    grants: Mutex<BTreeMap<Sha256Digest, Authorization>>,
    sender: mpsc::Sender<Work>,
    closing: Arc<AtomicBool>,
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(grant): Option<protocol::Grant> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    let alive = {
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
                && Instant::now() < authorization.expires
                && authorization.grant.worker == grant.worker
                && authorization.grant.project == grant.project
                && grant.service == state.service
                && grant.pool == state.pool,
            "invalid pool authorization"
        );
        authorization.used = true;
        authorization.alive.clone()
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
        while alive.load(Ordering::Acquire) && !state.closing.load(Ordering::Acquire) {
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
                alive.load(Ordering::Acquire) && !state.closing.load(Ordering::Acquire),
                "pool admission revoked"
            );
            let (reply, receiver) = oneshot::channel();
            state
                .sender
                .try_send(Work::Request {
                    grant: Box::new(grant.clone()),
                    envelope: Box::new(envelope),
                    alive: alive.clone(),
                    reply,
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
    });
    result
}

pub async fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("pool initialization missing")?;
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
    let model_engine = Engine::new(&init.runtime)?;
    let engine = std::thread::Builder::new()
        .name("airk-pooled-inference".into())
        .stack_size(8 << 20)
        .spawn(move || {
            let mut engine = model_engine;
            engine.inference_threads = engine_threads;
            while let Some(work) = receiver.blocking_recv() {
                match work {
                    Work::Sweep => {
                        engine.retire_disconnected();
                        if engine.sessions.is_empty()
                            && engine.used.elapsed() > Duration::from_secs(60)
                        {
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
                        reply,
                    } => {
                        let envelope = *envelope;
                        engine.owners.insert(grant.worker, Arc::downgrade(&alive));
                        engine.retire_disconnected();
                        let response = (|| {
                            ensure!(
                                alive.load(Ordering::Acquire)
                                    && !engine_closing.load(Ordering::Acquire),
                                "pool request canceled"
                            );
                            match envelope.operation {
                                protocol::Operation::Call { request } => {
                                    engine.request(&grant, envelope.session, *request)
                                }
                                protocol::Operation::Close => {
                                    engine.sessions.remove(&(grant.worker, envelope.session));
                                    Ok(Response::Peak { bytes: 0 })
                                }
                            }
                        })()
                        .unwrap_or(Response::Failed {
                            reason: "pool request rejected or verified model unavailable".into(),
                        });
                        let _ = reply.send(protocol::ResultEnvelope {
                            project: envelope.project,
                            worker: envelope.worker,
                            session: envelope.session,
                            request_id: envelope.request_id,
                            input_digest: envelope.input_digest,
                            response,
                        });
                    }
                    Work::Release { worker } => {
                        engine.owners.remove(&worker);
                        engine.sessions.retain(|(owner, _), _| *owner != worker)
                    }
                }
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
                        if let Some(authorization)=authorization {authorization.expires=Instant::now()+Duration::from_secs(300);protocol::Observation::Renewed}else{protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Register{grant})=>{
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        grants.retain(|_,auth|{if auth.alive.load(Ordering::Acquire)&&Instant::now()<auth.expires{true}else{auth.alive.store(false,Ordering::Release);false}});
                        if grant.service==state.service&&grant.pool==state.pool&&grant.project.validate().is_ok()&&grants.len()<32 {
                            grants.insert(grant.token,Authorization{grant,used:false,expires:Instant::now()+Duration::from_secs(300),alive:Arc::new(AtomicBool::new(true))});
                            protocol::Observation::Registered
                        }else{protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Revoke{worker})=>{
                        state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?.retain(|_,auth|{if auth.grant.worker==worker{auth.alive.store(false,Ordering::Release);false}else{true}});
                        let _=state.sender.try_send(Work::Release{worker});
                        protocol::Observation::Revoked
                    },
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
                if state.sender.capacity()==8{let _=state.sender.try_send(Work::Sweep);}
                let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                grants.retain(|_,auth|{if Instant::now()>=auth.expires{auth.alive.store(false,Ordering::Release);false}else{true}});
            }
        }
    }
    closing.store(true, Ordering::Release);
    while connections.join_next().await.is_some() {}
    drop(state);
    engine
        .join()
        .map_err(|_| anyhow::anyhow!("pool execution thread failed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn fixture(
        root: &Path,
        label: &str,
        name: &str,
    ) -> Result<(protocol::Grant, ModelLocation, String)> {
        let root = root.canonicalize()?;
        let source = root.join(format!("source-{label}"));
        let data = root.join(format!("data-{label}"));
        let model = data.join("models/local");
        std::fs::create_dir_all(&source)?;
        std::fs::create_dir_all(&model)?;
        let vocabulary = br#"{"cobalt":[1.0,0.0],"harbor":[0.0,1.0]}"#;
        let identity = format!("{:x}", Sha256::digest(vocabulary));
        let manifest = airk_retrieval::vector::ModelManifest {
            schema: 1,
            name: name.into(),
            license: "Apache-2.0".into(),
            sha256: identity.clone(),
            dimensions: 2,
        };
        std::fs::write(model.join("model.json"), serde_json::to_vec(&manifest)?)?;
        std::fs::write(model.join("vocabulary.json"), vocabulary)?;
        let project = airk_kernel::service_projects::registration(&source, &data, label)?;
        Ok((
            protocol::Grant {
                service: Incarnation::from_bytes([1; 16]),
                pool: Incarnation::from_bytes([2; 16]),
                project,
                worker: airk_kernel::service_broker::new_instance_id()?,
                token: Sha256Digest::from_bytes([3; 32]),
            },
            ModelLocation::Directory {
                path: model.canonicalize()?,
            },
            identity,
        ))
    }

    fn open(location: ModelLocation, identity: String) -> Request {
        Request::Open {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            location,
            identity,
        }
    }

    #[test]
    fn compatible_projects_share_verified_inference_and_foreign_assets_are_rejected() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let (first, location, identity) = fixture(root.path(), "first", "original fixture")?;
        let (second, other, other_identity) = fixture(root.path(), "second", "original fixture")?;
        let session = Incarnation::from_bytes([4; 16]);
        let mut engine = Engine::new(root.path())?;
        engine.request(&first, session, open(location.clone(), identity.clone()))?;
        engine.request(&second, session, open(other.clone(), other_identity))?;
        assert_eq!((engine.loads, engine.reuses), (1, 1));
        let embed = || Request::Embed {
            texts: vec!["cobalt harbor".into()],
            workers: 8,
            memory_bytes: 1 << 30,
        };
        let Response::Embedded { vectors: a, .. } = engine.request(&first, session, embed())?
        else {
            anyhow::bail!("missing first vector")
        };
        let Response::Embedded { vectors: b, .. } = engine.request(&second, session, embed())?
        else {
            anyhow::bail!("missing second vector")
        };
        assert_eq!(a, b);
        assert!(
            engine
                .request(
                    &second,
                    Incarnation::from_bytes([5; 16]),
                    open(location, identity)
                )
                .is_err()
        );
        assert!(
            engine
                .request(
                    &first,
                    session,
                    Request::Extract {
                        directory: root.path().into()
                    }
                )
                .is_err()
        );
        assert_eq!(engine.loads, 1);
        Ok(())
    }

    #[test]
    fn equal_legacy_weight_digest_does_not_alias_distinct_pipeline_descriptors() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (first, location, identity) = fixture(root.path(), "first", "original fixture")?;
        let (second, other, other_identity) =
            fixture(root.path(), "second", "different provenance")?;
        assert_eq!(identity, other_identity);
        let session = Incarnation::from_bytes([4; 16]);
        let mut engine = Engine::new(root.path())?;
        engine.request(&first, session, open(location, identity))?;
        engine.request(&second, session, open(other, other_identity))?;
        assert_eq!((engine.loads, engine.reuses), (2, 0));
        engine.request(&first, session, Request::Peak)?;
        assert_eq!(engine.loads, 3);
        Ok(())
    }

    #[test]
    fn interleaved_incompatible_models_return_only_their_scoped_vectors() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (first, location, identity) = fixture(root.path(), "first", "first fixture")?;
        let (second, other, _) = fixture(root.path(), "second", "second fixture")?;
        let ModelLocation::Directory { path } = &other else {
            unreachable!()
        };
        let vocabulary = br#"{"cobalt":[0.0,1.0],"harbor":[1.0,0.0]}"#;
        let changed = format!("{:x}", Sha256::digest(vocabulary));
        let manifest = airk_retrieval::vector::ModelManifest {
            schema: 1,
            name: "second fixture".into(),
            license: "Apache-2.0".into(),
            sha256: changed.clone(),
            dimensions: 2,
        };
        std::fs::write(path.join("vocabulary.json"), vocabulary)?;
        std::fs::write(path.join("model.json"), serde_json::to_vec(&manifest)?)?;
        let session = Incarnation::from_bytes([4; 16]);
        let mut engine = Engine::new(root.path())?;
        engine.request(&first, session, open(location, identity))?;
        engine.request(&second, session, open(other, changed))?;
        let embed = || Request::Embed {
            texts: vec!["cobalt".into()],
            workers: 1,
            memory_bytes: 8 << 20,
        };
        let Response::Embedded { vectors: a, .. } = engine.request(&first, session, embed())?
        else {
            anyhow::bail!("missing first vector")
        };
        let Response::Embedded { vectors: b, .. } = engine.request(&second, session, embed())?
        else {
            anyhow::bail!("missing second vector")
        };
        assert_eq!(a, vec![Some(vec![1.0, 0.0])]);
        assert_eq!(b, vec![Some(vec![0.0, 1.0])]);
        let Response::Embedded { vectors: again, .. } = engine.request(&first, session, embed())?
        else {
            anyhow::bail!("missing repeated vector")
        };
        assert_eq!(a, again);
        Ok(())
    }
    #[test]
    fn bundled_provision_is_global_persistent_and_does_not_unload_compatible_inference()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let (first, _, _) = fixture(root.path(), "bundled-first", "unused fixture")?;
        let (second, _, _) = fixture(root.path(), "bundled-second", "unused fixture")?;
        // Test projects have no per-project model input directories.
        for grant in [&first, &second] {
            std::fs::remove_dir_all(Path::new(&grant.project.data_root).join("models"))?;
        }
        let mut engine = Engine::new(root.path())?;
        let session = Incarnation::from_bytes([9; 16]);
        let Response::Provisioned { objects } =
            engine.request(&first, session, Request::ProvisionBundled)?
        else {
            anyhow::bail!("default model provisioning failed")
        };
        assert_eq!(
            objects.local_set(),
            airk_cli::bundled_assets::default_plan()?
        );
        let Response::Provisioned { objects: other } =
            engine.request(&second, session, Request::ProvisionBundled)?
        else {
            anyhow::bail!("second default model provisioning failed")
        };
        assert_eq!(objects, other);
        engine.request(
            &first,
            session,
            open(
                ModelLocation::Shared {
                    objects: objects.clone(),
                },
                objects.legacy_identity.clone(),
            ),
        )?;
        engine.request(
            &second,
            session,
            open(
                ModelLocation::Shared { objects: other },
                objects.legacy_identity.clone(),
            ),
        )?;
        assert_eq!((engine.loads, engine.reuses), (1, 1));
        engine.request(
            &second,
            Incarnation::from_bytes([10; 16]),
            Request::ProvisionBundled,
        )?;
        assert!(engine.model.is_some());
        assert_eq!(engine.loads, 1);
        assert_eq!(
            std::fs::read_dir(root.path().join("model-files-1.4/sets"))?.count(),
            1
        );
        for grant in [&first, &second] {
            assert!(!Path::new(&grant.project.data_root).join("models").exists());
        }
        drop(engine);
        let mut engine = Engine::new(root.path())?;
        assert_eq!(
            airk_store::service::model_files::ServiceModelFiles::default_reference(
                root.path(),
                &objects.local_set()
            )?,
            Some(objects.clone())
        );
        engine.request(
            &first,
            session,
            open(
                ModelLocation::Shared {
                    objects: objects.clone(),
                },
                objects.legacy_identity.clone(),
            ),
        )?;
        assert_eq!(engine.loads, 1);
        Ok(())
    }

    #[test]
    fn custom_provision_captures_once_and_shared_sessions_reject_other_runtime() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (first, location, identity) = fixture(root.path(), "custom-first", "custom fixture")?;
        let (second, _, _) = fixture(root.path(), "custom-second", "unused fixture")?;
        let ModelLocation::Directory { path } = location else {
            unreachable!()
        };
        let before = std::fs::read(path.join("vocabulary.json"))?;
        let mut engine = Engine::new(root.path())?;
        let session = Incarnation::from_bytes([9; 16]);
        let Response::Provisioned { objects } = engine.request(
            &first,
            session,
            Request::ProvisionModel {
                directory: path.clone(),
            },
        )?
        else {
            anyhow::bail!("custom model provisioning failed")
        };
        assert!(
            airk_store::service::model_files::ServiceModelFiles::default_reference(
                root.path(),
                &objects.local_set()
            )?
            .is_none()
        );
        // Captured installed assets no longer depend on the original selected directory.
        std::fs::rename(&path, path.with_file_name("selected-original"))?;
        engine.request(
            &first,
            session,
            open(
                ModelLocation::Shared {
                    objects: objects.clone(),
                },
                identity.clone(),
            ),
        )?;
        engine.request(
            &second,
            session,
            open(
                ModelLocation::Shared {
                    objects: objects.clone(),
                },
                identity.clone(),
            ),
        )?;
        assert_eq!((engine.loads, engine.reuses), (1, 1));
        let foreign = tempfile::tempdir()?;
        let mut other = Engine::new(foreign.path())?;
        assert!(
            other
                .request(
                    &first,
                    session,
                    open(ModelLocation::Shared { objects }, identity)
                )
                .is_err()
        );
        assert_eq!(
            std::fs::read(
                path.with_file_name("selected-original")
                    .join("vocabulary.json")
            )?,
            before
        );
        Ok(())
    }
}
