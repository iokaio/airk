// SPDX-License-Identifier: Apache-2.0
// Included beside the canonical engine. No child process or synthetic PID.
use airk_core::inference_provider::{CompletionObservation, CompletionState, ProviderIdentity};
use airk_core::service::WorkerIdentity;
use tokio::{sync::Semaphore, task::JoinSet};

/// Native capability: construction is private and requires the actual owner
/// barrier. The serializable observation alone cannot create this capability.
#[derive(Clone)]
pub(crate) struct NativeCompletionWitness {
    observation: CompletionObservation,
    _owner: Arc<State>,
    namespace: Incarnation,
    _lease: Arc<airk_store::service::ServiceLease>,
}
impl NativeCompletionWitness {
    pub(crate) fn observation(&self) -> &CompletionObservation {
        &self.observation
    }
    pub(crate) fn matches(&self, service: Incarnation, generation: Incarnation) -> bool {
        self.observation.provider.service_instance == service
            && self.observation.provider.provider_generation == generation
            && self.observation.state == CompletionState::Complete
    }
    pub(crate) fn namespace(&self) -> Incarnation {
        self.namespace
    }
}

pub(crate) struct InferenceOwner {
    state: Arc<State>,
    provider: ProviderIdentity,
    threads: Arc<AtomicUsize>,
    observation: Arc<Mutex<(u64, u64, usize, Instant)>>,
    owner: Option<std::thread::JoinHandle<Result<()>>>,
    stopped: bool,
    stop_requested: bool,
    failure: bool,
    barrier_sent: bool,
    completion: Option<NativeCompletionWitness>,
    // The actual service lease remains owned through any pending FFI drain.
    _lease: Arc<airk_store::service::ServiceLease>,
}
impl InferenceOwner {
    pub(crate) fn start(
        lease: Arc<airk_store::service::ServiceLease>,
        service: Incarnation,
        inference_threads: u16,
    ) -> Result<Self> {
        ensure!(
            (1..=4).contains(&inference_threads),
            "invalid inference allowance"
        );
        let runtime = lease.root().to_path_buf();
        local_ipc::require_existing_root(&runtime)?;
        let pool = airk_kernel::service_broker::new_instance_id()?;
        let provider = ProviderIdentity {
            schema: 1,
            service_instance: service,
            approved_execution_sha256: airk_cli::runtime_pair::current_executable_sha256()?
                .parse()?,
            provider_generation: pool,
        };
        let (sender, mut receiver) = mpsc::channel::<Work>(8);
        let closing = Arc::new(AtomicBool::new(false));
        let accounting = Arc::new(accounting::Accounting::default());
        let state = Arc::new(State {
            service,
            pool,
            role: protocol::role(pool),
            grants: Mutex::new(BTreeMap::new()),
            retirement: Mutex::new(retirement::Retirements::default()),
            sender,
            closing: closing.clone(),
            accounting: Some(accounting),
        });
        let threads = Arc::new(AtomicUsize::new(usize::from(inference_threads)));
        let observation = Arc::new(Mutex::new((0, 0, 0, Instant::now())));
        let engine_state = state.clone();
        let engine_threads = threads.clone();
        let engine_observation = observation.clone();
        let owner_state = state.clone();
        let compute_ownership = lease.clone();
        let (ready, readiness) = std::sync::mpsc::sync_channel(1);
        let owner = std::thread::Builder::new().name("airk-inference-owner".into()).spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
            rt.block_on(async move {
                let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(&runtime, &owner_state.role)?)?;
                // Create the model owner before accepting grants. FFI/model
                // initialization occurs only inside this service-owned role.
                let mut engine = Engine::new(&runtime)?;
                engine.embedded=Some(airk_store::service::embedded_model::EmbeddedModelBinding::new(airk_cli::bundled_assets::EmbeddedAssets::plan()?,Arc::new(airk_cli::bundled_assets::EmbeddedAssets))?);
                engine.inference_threads = engine_threads;
                let (compute_ready, compute_start) = std::sync::mpsc::sync_channel(1);
                let compute = std::thread::Builder::new().name("airk-inference-compute".into()).stack_size(8 << 20).spawn(move || {
                    airk_kernel::inference_owner::run_service_inference(compute_ownership, move || {
                    let _ = compute_ready.send(());
                    while let Some(work) = receiver.blocking_recv() {
                        if let Work::StopEngine { reply } = work {
                            let _ = reply.send(());
                            break;
                        }
                        process_work(&mut engine, work, &engine_state.closing);
                        if let Some(accounting) = &engine_state.accounting {
                            accounting.model_cached(engine.model.is_some());
                        }
                        if let Ok(mut status) = engine_observation.lock() {
                            *status = (engine.loads, engine.reuses, engine.sessions.len(), Instant::now());
                        }
                    }
                    drop(engine);
                    if let Some(accounting) = &engine_state.accounting {
                        accounting.model_cached(false);
                        accounting.mark_engine_barrier();
                    }
                    })
                })?;
                if compute_start.recv_timeout(Duration::from_secs(5)).is_err() {
                    compute.join().map_err(|_| anyhow::anyhow!("inference compute startup panicked"))??;
                    anyhow::bail!("inference compute startup failed");
                }
                let _ = ready.send(());
                let slots = Arc::new(Semaphore::new(64));
                let mut connections = JoinSet::new();
                let mut tick = tokio::time::interval(Duration::from_millis(50));
                let mut failure = None;
                while !owner_state.closing.load(Ordering::Acquire) {
                    tokio::select! {
                        stream = listener.accept() => match stream {
                            Ok(stream) => if let Ok(permit) = slots.clone().try_acquire_owned() {
                                let state = owner_state.clone();
                                connections.spawn(async move { let _permit = permit; connection(stream, state).await });
                            },
                            Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some() => {},
                            Err(error) => { failure = Some(error); break; }
                        },
                        _ = connections.join_next(), if !connections.is_empty() => {},
                        _ = tick.tick() => {
                            match owner_state.retirement.lock() {
                                Ok(mut retirement) => retirement.retry(&owner_state.sender),
                                Err(_) => { failure=Some(anyhow::anyhow!("inference retirement unavailable")); break; }
                            }
                            if owner_state.sender.capacity()==8 {let _=owner_state.sender.try_send(Work::Sweep);}
                            if let Err(error)=owner_state.expire_grants(Instant::now()) {failure=Some(error);break;}
                        }
                    }
                }
                owner_state.closing.store(true, Ordering::Release);
                connections.abort_all();
                while connections.join_next().await.is_some() {}
                // State retains the sender for polling, so use an explicit
                // ordered Stop instead of channel EOF or detaching compute.
                let (reply, complete) = oneshot::channel();
                let stop_sent = owner_state.sender.send(Work::StopEngine { reply }).await.is_ok();
                if stop_sent { let _ = complete.await; }
                compute.join().map_err(|_| anyhow::anyhow!("inference compute panicked"))??;
                ensure!(stop_sent, "inference engine unavailable during shutdown");
                if let Some(error) = failure { return Err(error); }
                Ok(())
            })
        })?;
        if readiness.recv_timeout(Duration::from_secs(10)).is_err() {
            state.closing.store(true, Ordering::Release);
            // No active model work can precede startup readiness. Join keeps
            // ownership explicit even when listener/model setup fails.
            owner
                .join()
                .map_err(|_| anyhow::anyhow!("inference owner startup panicked"))??;
            anyhow::bail!("inference owner startup failed");
        }
        Ok(Self {
            state,
            provider,
            threads,
            observation,
            owner: Some(owner),
            stopped: false,
            stop_requested: false,
            failure: false,
            barrier_sent: false,
            completion: None,
            _lease: lease,
        })
    }
    pub(crate) fn generation(&self) -> Incarnation {
        self.provider.provider_generation
    }
    pub(crate) fn provider_status(&self) -> Result<airk_core::inference_provider::ProviderStatus> {
        let mut status = self
            .state
            .accounting
            .as_ref()
            .context("missing inference accounting")?
            .status(self.provider)?;
        status.admission_open &= !self.state.closing.load(Ordering::Acquire);
        if self.failed() {
            status.failure = Some(airk_core::inference_provider::ProviderFailure::OwnerFailed);
            status.admission_open = false;
        }
        status.validate()?;
        Ok(status)
    }
    pub(crate) fn completion(&self) -> Option<&NativeCompletionWitness> {
        self.completion.as_ref()
    }
    pub(crate) fn worker_settled(&self, worker: Incarnation) -> Result<bool> {
        self.state
            .accounting
            .as_ref()
            .context("missing inference accounting")?
            .worker_settled(worker)
    }
    pub(crate) fn forget_worker(&self, worker: Incarnation) -> Result<()> {
        self.state
            .accounting
            .as_ref()
            .context("missing inference accounting")?
            .forget_worker(worker)
    }
    pub(crate) fn register(&self, grant: protocol::Grant, identity: WorkerIdentity) -> Result<()> {
        ensure!(
            !self.state.closing.load(Ordering::Acquire),
            "inference owner closing"
        );
        ensure!(
            self.state
                .accounting
                .as_ref()
                .context("missing inference accounting")?
                .admission_open()?,
            "inference admission closed"
        );
        let mut retirement = self
            .state
            .retirement
            .lock()
            .map_err(|_| anyhow::anyhow!("inference retirement unavailable"))?;
        let mut grants = self
            .state
            .grants
            .lock()
            .map_err(|_| anyhow::anyhow!("inference authorization unavailable"))?;
        ensure!(
            matches!(
                retirement.register(
                    &mut grants,
                    (self.state.service, self.state.pool),
                    grant,
                    Some(identity),
                    Instant::now()
                ),
                protocol::Observation::Registered
            ),
            "inference grant refused"
        );
        Ok(())
    }
    pub(crate) fn renew(&self, worker: Incarnation) -> Result<bool> {
        let mut grants = self
            .state
            .grants
            .lock()
            .map_err(|_| anyhow::anyhow!("inference authorization unavailable"))?;
        if let Some(auth) = grants.values_mut().find(|auth| {
            auth.grant.worker == worker
                && auth.alive.load(Ordering::Acquire)
                && Instant::now() < auth.expires
        }) {
            auth.expires = Instant::now() + Duration::from_secs(300);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    #[cfg(test)]
    pub(crate) fn revoke(&self, worker: Incarnation) -> Result<()> {
        let mut retirement = self
            .state
            .retirement
            .lock()
            .map_err(|_| anyhow::anyhow!("inference retirement unavailable"))?;
        let mut grants = self
            .state
            .grants
            .lock()
            .map_err(|_| anyhow::anyhow!("inference authorization unavailable"))?;
        retirement.revoke(&mut grants, worker);
        let _ = self.state.sender.try_send(Work::Release {
            worker,
            owner: None,
        });
        Ok(())
    }
    pub(crate) fn begin_retirement(
        &self,
        ticket: protocol::ProviderDrainTicket,
    ) -> Result<protocol::Observation> {
        drain_control(&self.state, retirement::Control::Begin, ticket)
    }
    pub(crate) fn poll_retirement(
        &self,
        ticket: protocol::ProviderDrainTicket,
    ) -> Result<protocol::Observation> {
        drain_control(&self.state, retirement::Control::Poll, ticket)
    }
    pub(crate) fn finish_retirement(
        &self,
        ticket: protocol::ProviderDrainTicket,
    ) -> Result<protocol::Observation> {
        drain_control(&self.state, retirement::Control::Forget, ticket)
    }
    pub(crate) fn status(&self) -> Result<protocol::Observation> {
        let value = self
            .observation
            .lock()
            .map_err(|_| anyhow::anyhow!("inference status unavailable"))?;
        Ok(protocol::Observation::Status {
            loads: value.0,
            reuses: value.1,
            sessions: value.2,
            age_ms: value.3.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
    pub(crate) fn configure_threads(&self, threads: u16) -> Result<()> {
        ensure!((1..=4).contains(&threads), "invalid inference allowance");
        self.threads.store(usize::from(threads), Ordering::Release);
        Ok(())
    }
    /// The actual accounting owner retains the first accepted operation and
    /// full capture, including workers without an inference grant. Stop must
    /// continue this barrier rather than inventing another operation/roster.
    pub(crate) fn checkpoint_context(&self) -> Result<Option<CompletionObservation>> {
        let accounting = self
            .state
            .accounting
            .as_ref()
            .context("missing inference accounting")?;
        accounting
            .operation()?
            .map(|operation| accounting.observation(self.provider, operation))
            .transpose()
    }
    pub(crate) fn checkpoint_drain(
        &mut self,
        operation: Incarnation,
        workers: Vec<WorkerIdentity>,
    ) -> Result<Option<&NativeCompletionWitness>> {
        let accounting = self
            .state
            .accounting
            .as_ref()
            .context("missing inference accounting")?;
        // Validate exact native capture before closing admission.
        let mut capture = CompletionObservation {
            completion_basis: airk_core::inference_provider::CompletionBasis::NativeEngineBarrier,
            schema: 1,
            provider: self.provider,
            operation_id: operation,
            captured_workers: workers.clone(),
            admitted_high_water: 0,
            admission_closed: false,
            queued_outstanding: 0,
            running_outstanding: 0,
            callback_outstanding: 0,
            delivery_outstanding: 0,
            model_references_outstanding: 0,
            state: CompletionState::Pending,
            completion_ledger: None,
        };
        capture.validate()?;
        accounting.close(operation, workers)?;
        if self.stopped {
            accounting.mark_engine_barrier();
        }
        if !self.barrier_sent
            && accounting.settled()?
            && self
                .state
                .sender
                .try_send(Work::CloseEngine {
                    accounting: accounting.clone(),
                })
                .is_ok()
        {
            self.barrier_sent = true;
        }
        capture = accounting.observation(self.provider, operation)?;
        if capture.state == CompletionState::Complete && self.completion.is_none() {
            self.completion = Some(NativeCompletionWitness {
                observation: capture,
                _owner: self.state.clone(),
                namespace: airk_store::service::registry::retained_namespace(self._lease.root())?,
                _lease: self._lease.clone(),
            });
        }
        Ok(self.completion.as_ref())
    }
    pub(crate) fn stop(&mut self) -> Result<bool> {
        self.failure |= !self.stop_requested
            && self
                .owner
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished);
        self.stop_requested = true;
        self.state.closing.store(true, Ordering::Release);
        if self
            .owner
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            let result = self
                .owner
                .take()
                .context("missing inference owner")?
                .join()
                .map_err(|_| anyhow::anyhow!("inference owner panicked"));
            match result {
                Ok(Ok(())) => {}
                // A returned error follows the owned compute join in the
                // shutdown finally path. Its native model resources ended.
                Ok(Err(_)) => {
                    self.failure = true;
                }
                Err(error) => {
                    self.failure = true;
                    return Err(error);
                }
            }
            self.stopped = true;
            if let Some(accounting) = &self.state.accounting {
                accounting.model_cached(false);
                accounting.mark_engine_barrier();
            }
        }
        Ok(self.stopped)
    }
    pub(crate) fn failed(&self) -> bool {
        self.failure
            || (!self.stopped
                && !self.stop_requested
                && self
                    .owner
                    .as_ref()
                    .is_some_and(std::thread::JoinHandle::is_finished))
    }
}
impl Drop for InferenceOwner {
    fn drop(&mut self) {
        self.state.closing.store(true, Ordering::Release);
        // FFI has no safe thread kill/unload. Retain the actual owner until its
        // native execution and all reply deliveries have really terminated.
        if let Some(owner) = self.owner.take() {
            let _ = owner.join();
        }
    }
}
