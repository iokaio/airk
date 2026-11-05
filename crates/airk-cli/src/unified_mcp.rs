// SPDX-License-Identifier: Apache-2.0
//! Unified transport composition; the broker and kernel never own UI behavior.
use airk_core::service::{ProjectBinding, ProjectRegistration};
use airk_mcp::{
    remote::{RemoteFuture, RemoteTools},
    unified::{ExplorerTools, Project, Unified},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

struct Explorer {
    descriptor: PathBuf,
    roots: BTreeMap<String, PathBuf>,
    scopes: BTreeMap<String, crate::service_frontend::RootScope>,
    control: PathBuf,
}

// A shared ticket keeps one attempt's failure available to every admitted waiter.
type SharedReadiness = std::result::Result<(), Arc<anyhow::Error>>;

#[derive(Debug)]
struct SharedReadinessError(Arc<anyhow::Error>);
impl SharedReadinessError {
    // Sharing changes top-level anyhow identity; keep explicit access to the
    // original typed root instead of duplicating it in the displayed chain.
    fn original(&self) -> &anyhow::Error {
        self.0.as_ref()
    }
}
impl std::fmt::Display for SharedReadinessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
impl std::error::Error for SharedReadinessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        let original: &(dyn std::error::Error + Send + Sync + 'static) = self.original().as_ref();
        original.source()
    }
}

#[derive(Default)]
struct RuntimeReadiness {
    state: tokio::sync::Mutex<ReadinessState>,
    shutdown_gate: tokio::sync::Mutex<()>,
    closing: std::sync::atomic::AtomicBool,
    admission: Arc<crate::runtime_install::SetupAdmission>,
}
#[derive(Default)]
enum ReadinessState {
    #[default]
    Idle,
    Running(Arc<ReadinessAttempt>),
    Ready,
    Closed,
}
struct ReadinessAttempt {
    started: Instant,
    completed: std::sync::OnceLock<Instant>,
    status_deadline: tokio::sync::watch::Sender<bool>,
    result: tokio::sync::watch::Receiver<Option<SharedReadiness>>,
    driver_result: tokio::sync::OnceCell<SharedReadiness>,
    failed_completion: tokio::sync::OnceCell<SharedReadiness>,
    setup: Option<Arc<crate::runtime_install::SetupCancellation>>,
    abort: std::sync::OnceLock<tokio::task::AbortHandle>,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl ReadinessAttempt {
    fn close(&self) {
        if let Some(setup) = &self.setup {
            setup.close();
        } else if let Some(abort) = self.abort.get() {
            abort.abort();
        }
    }
}
impl Drop for ReadinessAttempt {
    fn drop(&mut self) {
        self.close();
    }
}
impl Drop for RuntimeReadiness {
    fn drop(&mut self) {
        self.close_admission();
        if let ReadinessState::Running(attempt) = self.state.get_mut() {
            // Cooperative owners retain their own attempt/worker handles until
            // completion. Drop signals close; ordinary EOF explicitly joins.
            attempt.close();
        }
    }
}
impl ReadinessAttempt {
    async fn joined_driver(&self) -> SharedReadiness {
        self.driver_result
            .get_or_init(|| async {
                let mut slot = self.task.lock().await;
                let outcome = if let Some(task) = slot.as_mut() {
                    task.await.map_err(|error| {
                        Arc::new(
                            anyhow::Error::from(error)
                                .context("installed runtime verification task failed"),
                        )
                    })
                } else {
                    Ok(())
                };
                // No cancelable await between consuming the handle and persisting
                // this exact outcome in the OnceCell.
                *slot = None;
                outcome
            })
            .await
            .clone()
    }
    async fn failed_join(&self) -> SharedReadiness {
        let cause = match self.joined_driver().await {
            Err(error) => error,
            Ok(()) => Arc::new(anyhow::anyhow!(
                "installed runtime verification task ended without result"
            )),
        };
        // Cause availability is NOT attempt completion. All tickets await this
        // retained worker drain before any failed attempt can return to Idle.
        self.failed_completion
            .get_or_init(|| async {
                if let Some(setup) = &self.setup {
                    setup.close();
                    if let Err(cleanup) = setup.drain().await {
                        return Err(Arc::new(
                            anyhow::Error::new(SharedReadinessError(cause.clone()))
                                .context(format!("owned setup drain also failed: {cleanup:#}")),
                        ));
                    }
                }
                Err(cause)
            })
            .await
            .clone()
    }
}
impl RuntimeReadiness {
    fn close_admission(&self) {
        self.closing
            .store(true, std::sync::atomic::Ordering::Release);
        self.admission.close();
    }
    async fn ensure(&self, install: &std::path::Path, control: &std::path::Path) -> Result<()> {
        self.ensure_mode(install, control, false).await
    }
    async fn prepare_status(
        &self,
        install: &std::path::Path,
        control: &std::path::Path,
    ) -> Result<()> {
        self.ensure_mode(install, control, true).await
    }
    async fn ensure_mode(
        &self,
        install: &std::path::Path,
        control: &std::path::Path,
        status: bool,
    ) -> Result<()> {
        let install = install.to_path_buf();
        let control = control.to_path_buf();
        self.run_inner_status(
            move |setup| async move {
                let observed = crate::runtime_install::installed_control_cooperative(
                    install,
                    setup.expect("owned setup control"),
                )
                .await?;
                ensure!(observed == control, "selected service control changed");
                Ok(())
            },
            true,
            status,
        )
        .await
    }

    #[cfg(test)]
    async fn run<F, Fut>(&self, initialize: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        self.run_inner(move |_| initialize(), false).await
    }

    #[cfg(test)]
    async fn run_inner<F, Fut>(&self, initialize: F, cooperative: bool) -> Result<()>
    where
        F: FnOnce(Option<Arc<crate::runtime_install::SetupCancellation>>) -> Fut,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        self.run_inner_status(initialize, cooperative, false).await
    }
    async fn run_inner_status<F, Fut>(
        &self,
        initialize: F,
        cooperative: bool,
        status: bool,
    ) -> Result<()>
    where
        F: FnOnce(Option<Arc<crate::runtime_install::SetupCancellation>>) -> Fut,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let waiter_started = Instant::now();
        let attempt = {
            let mut state = self.state.lock().await;
            ensure!(
                !self.closing.load(std::sync::atomic::Ordering::Acquire),
                "runtime readiness session closed"
            );
            match &*state {
                ReadinessState::Ready => return Ok(()),
                ReadinessState::Closed => anyhow::bail!("runtime readiness session closed"),
                ReadinessState::Running(attempt) => attempt.clone(),
                ReadinessState::Idle => {
                    let (sender, result) = tokio::sync::watch::channel(None);
                    let setup = cooperative.then(|| {
                        Arc::new(crate::runtime_install::SetupCancellation::for_session(
                            self.admission.clone(),
                        ))
                    });
                    let future = initialize(setup.clone());
                    let (status_deadline, mut status_requested) =
                        tokio::sync::watch::channel(false);
                    let started = Instant::now();
                    let attempt = Arc::new(ReadinessAttempt {
                        started,
                        completed: Default::default(),
                        status_deadline,
                        result,
                        driver_result: tokio::sync::OnceCell::new(),
                        failed_completion: tokio::sync::OnceCell::new(),
                        setup,
                        abort: std::sync::OnceLock::new(),
                        task: tokio::sync::Mutex::new(None),
                    });
                    let keep_attempt = attempt.clone();
                    let task = tokio::spawn(async move {
                        // Retain the owner and its real handle while cooperative
                        // work drains, even if the outer session is dropped.
                        let owner = keep_attempt;
                        let mut future = Box::pin(future);
                        let mut armed = false;
                        let outcome = loop {
                            tokio::select! {
                                result=&mut future=>break result.map_err(Arc::new),
                                changed=status_requested.changed(),if !armed=>{
                                    if changed.is_err(){break Err(Arc::new(anyhow::anyhow!("status setup deadline owner closed")));}
                                    armed = *status_requested.borrow();
                                }
                                _=tokio::time::sleep_until(tokio::time::Instant::from_std(owner.started+Duration::from_secs(10))),if armed=>{
                                    if let Some(setup)=&owner.setup{
                                        setup.expire(); // no new election; already admitted exact work drains once
                                        break future.await.map_err(Arc::new);
                                    }
                                    break Err(Arc::new(anyhow::Error::new(airk_mcp::remote::PreparationExpired)));
                                }
                            }
                        };
                        // Publish the completion time before the shared outcome;
                        // delayed waiters must not infer it from their own poll time.
                        let _ = owner.completed.set(Instant::now());
                        let _ = sender.send_replace(Some(outcome));
                    });
                    let _ = attempt.abort.set(task.abort_handle());
                    *attempt.task.try_lock().expect("new readiness task slot") = Some(task);
                    *state = ReadinessState::Running(attempt.clone());
                    attempt
                }
            }
        };
        let mut ticket = attempt.result.clone();
        if status {
            attempt.status_deadline.send_replace(true);
        }
        let observation = async {
            loop {
                if let Some(outcome) = ticket.borrow_and_update().clone() {
                    break outcome;
                }
                if ticket.changed().await.is_err() {
                    break attempt.failed_join().await;
                }
            }
        };
        let outcome = if status {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(attempt.started + Duration::from_secs(10)),
                observation,
            )
            .await
            .map_err(|_| airk_mcp::remote::PreparationExpired)?
        } else {
            observation.await
        };
        {
            let mut state = self.state.lock().await;
            ensure!(
                !self.closing.load(std::sync::atomic::Ordering::Acquire),
                "runtime readiness session closed"
            );
            // Arc identity is the attempt generation fence; an old ticket cannot reset a later attempt.
            if matches!(&*state, ReadinessState::Running(current) if Arc::ptr_eq(current, &attempt))
            {
                *state = if outcome.is_ok() {
                    ReadinessState::Ready
                } else {
                    ReadinessState::Idle
                };
            }
            if matches!(*state, ReadinessState::Closed) {
                anyhow::bail!("runtime readiness session closed");
            }
        }
        let deadline = attempt.started + Duration::from_secs(10);
        if status
            && waiter_started < deadline
            && attempt
                .completed
                .get()
                .is_some_and(|finished| *finished >= deadline)
        {
            // Settle the real outcome first, retaining physical failure/success.
            // An original status waiter still receives its expired budget even
            // if timeout_at polls a prompt cancellation completion first.
            return Err(airk_mcp::remote::PreparationExpired.into());
        }
        outcome.map_err(|error| {
            if error.is::<airk_mcp::remote::PreparationExpired>() {
                // The shared wrapper preserves display chains, not root type
                // identity. Keep this explicit deadline receipt recognizable.
                anyhow::Error::new(airk_mcp::remote::PreparationExpired)
            } else {
                anyhow::Error::new(SharedReadinessError(error))
            }
        })
    }

    /// EOF closes new setup admission and joins the owned driver/worker.
    /// Cooperative work is signaled, never aborted or detached; an already blocked
    /// OS call has no promised hard shutdown deadline.
    async fn shutdown(&self) -> Result<()> {
        self.close_admission();
        let _shutdown = self.shutdown_gate.lock().await;
        self.closing
            .store(true, std::sync::atomic::Ordering::Release);
        let attempt = {
            let state = self.state.lock().await;
            match &*state {
                ReadinessState::Running(attempt) => Some(attempt.clone()),
                _ => None,
            }
        };
        let mut joined: SharedReadiness = Ok(());
        let mut drained = Ok(());
        if let Some(attempt) = attempt {
            attempt.close();
            joined = attempt.joined_driver().await;
            // Explicit shutdown aborts only the legacy synthetic async path;
            // cooperative production drivers/workers are always closed and joined.
            if attempt.setup.is_none()
                && joined.as_ref().err().is_some_and(|error| {
                    error
                        .chain()
                        .find_map(|cause| cause.downcast_ref::<tokio::task::JoinError>())
                        .is_some_and(tokio::task::JoinError::is_cancelled)
                })
            {
                joined = Ok(());
            }
            if let Some(setup) = &attempt.setup {
                drained = setup.drain().await;
            }
        }
        *self.state.lock().await = ReadinessState::Closed;
        match (joined, drained) {
            (Err(error), Err(cleanup)) => Err(anyhow::Error::new(SharedReadinessError(error))
                .context(format!(
                    "runtime readiness cleanup also failed: {cleanup:#}"
                ))),
            (Err(error), _) => Err(anyhow::Error::new(SharedReadinessError(error))),
            (Ok(()), drained) => drained,
        }
    }
}
struct SourceLookupOwner {
    closed: std::sync::atomic::AtomicBool,
    permit: Arc<tokio::sync::Semaphore>,
    worker: tokio::sync::Mutex<
        Option<tokio::task::JoinHandle<Result<Arc<crate::service_frontend::RootObservation>>>>,
    >,
}
impl SourceLookupOwner {
    fn new() -> Self {
        Self {
            closed: std::sync::atomic::AtomicBool::new(false),
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
            worker: tokio::sync::Mutex::new(None),
        }
    }
    fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.permit.close();
    }
    async fn run<F>(
        self: &Arc<Self>,
        deadline: Instant,
        work: F,
    ) -> Result<Arc<crate::service_frontend::RootObservation>>
    where
        F: FnOnce() -> Result<Arc<crate::service_frontend::RootObservation>> + Send + 'static,
    {
        let permit = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.permit.clone().acquire_owned(),
        )
        .await
        .map_err(|_| airk_store::service::registry::SavedScopeExpired)?
        .map_err(|_| airk_store::service::registry::SavedScopeExpired)?;
        if self.closed.load(std::sync::atomic::Ordering::Acquire) || Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let mut slot =
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), self.worker.lock())
                .await
                .map_err(|_| airk_store::service::registry::SavedScopeExpired)?;
        // A cancelled caller leaves its actual handle in this owner. The permit
        // is released only by that worker, so any previous handle is completed.
        if let Some(previous) = slot.as_mut() {
            let joined = previous
                .await
                .context("previous source lookup worker failed");
            *slot = None;
            drop(joined?);
        }
        if self.closed.load(std::sync::atomic::Ordering::Acquire) || Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let keep_owner = self.clone();
        *slot = Some(tokio::task::spawn_blocking(move || {
            let _owner = keep_owner;
            let _permit = permit;
            work()
        }));
        let result = slot
            .as_mut()
            .expect("owned source lookup")
            .await
            .context("source lookup worker failed");
        *slot = None;
        result?
    }
    async fn shutdown(&self) -> Result<()> {
        self.close();
        let mut slot = self.worker.lock().await;
        if let Some(worker) = slot.as_mut() {
            let joined = worker.await.context("source lookup drain failed");
            *slot = None;
            // Metadata result belongs to its request. Cleanup still reports a
            // worker panic; cancellation does not invent a replacement result.
            drop(joined?);
        }
        Ok(())
    }
}
impl Drop for SourceLookupOwner {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Default)]
struct SourceBackendCache {
    latest_sequence: Option<u64>,
    current: Option<(
        (ProjectBinding, airk_core::identity::Incarnation),
        Arc<crate::project_client::Backend>,
    )>,
}
impl SourceBackendCache {
    fn admit_sequence(&mut self, sequence: u64) -> bool {
        if self.latest_sequence.is_some_and(|latest| sequence < latest) {
            return false;
        }
        self.latest_sequence = Some(sequence);
        true
    }
}
struct RootNative {
    scope: crate::service_frontend::RootScope,
    control: PathBuf,
    install: PathBuf,
    ready: Arc<RuntimeReadiness>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
    sequence: std::sync::atomic::AtomicU64,
    observation: std::sync::Mutex<Option<Arc<crate::service_frontend::RootObservation>>>,
    lookup: Arc<SourceLookupOwner>,
    status: Arc<crate::project_client::StatusOwner>,
    backend: tokio::sync::Mutex<SourceBackendCache>,
}
impl RemoteTools for RootNative {
    fn query_concurrency(&self) -> usize {
        crate::service_resource_policy::query_concurrency(&self.control)
    }
    fn close(&self) {
        self.lookup.close();
        self.status.close();
    }
    fn shutdown(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move { self.status.shutdown().await })
    }
    fn prepare(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move {
            self.ready
                .prepare_status(&self.install, &self.control)
                .await
        })
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        *self.client.lock().unwrap() = client;
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.ready.ensure(&self.install, &self.control).await?;
            let sequence = self
                .sequence
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |value| value.checked_add(1),
                )
                .map_err(|_| anyhow::anyhow!("source observation sequence exhausted"))?;
            let control = self.control.clone();
            let scope = self.scope.clone();
            let observed_control = control.clone();
            let admission = Instant::now() + Duration::from_secs(2);
            let candidate = self
                .observation
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone();
            let previous = candidate.clone();
            let observed = match self
                .lookup
                .run(admission, move || {
                    scope.observe_cached(&observed_control, candidate, admission)
                })
                .await
            {
                Ok(observed) => observed,
                Err(error) if airk_store::service::registry::source_scope_retryable(&error) => {
                    return Ok(crate::project_client::source_scope_superseded());
                }
                Err(error) => return Err(error),
            };
            let source_cached = previous
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, &observed));
            let mut cached = self.backend.lock().await;
            // An older snapshot cannot resurrect a backend/receipt namespace
            // after a later request already admitted its replacement.
            if Instant::now() >= admission
                || !observed.fresh_at(Instant::now())
                || !cached.admit_sequence(sequence)
            {
                return Ok(crate::project_client::source_scope_superseded());
            }
            *self
                .observation
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(observed.clone());
            let registration = match &observed.resolution {
                crate::service_frontend::RootResolution::Registered(registration) => registration,
                other => {
                    cached.current = None;
                    self.status.clear_scope();
                    let (code, state, message) = root_failure(match other {
                        crate::service_frontend::RootResolution::Missing => {
                            crate::service_frontend::RootResolution::Missing
                        }
                        crate::service_frontend::RootResolution::Removed => {
                            crate::service_frontend::RootResolution::Removed
                        }
                        crate::service_frontend::RootResolution::Ambiguous => {
                            crate::service_frontend::RootResolution::Ambiguous
                        }
                        _ => unreachable!(),
                    });
                    return Ok(if tool == "airk_status" {
                        airk_mcp::unified::success(
                            json!({"version":env!("CARGO_PKG_VERSION"),"health":"needs_attention","index_state":"unavailable","files":null,"chunks":null,"publication":null,"registration_state":state,"source_root":self.scope.root,"observed_unix_ms":observed.observed_unix_ms,"cache_age_ms":observed.age_ms(),"cached":source_cached,"index_error":{"code":code,"message":message}}),
                        )
                    } else {
                        airk_mcp::unified::failure(code, message)
                    });
                }
            };
            let fence = observed
                .fence
                .context("registered source observation has no fence")?;
            ensure!(
                fence.project == registration.binding,
                "crossed source observation"
            );
            // This is inside the same monotonic RootNative selector admission.
            // Older backend callers cannot revert this owner to an old namespace.
            self.status.source_scope(fence);
            let key = (registration.binding, fence.namespace);
            if cached
                .current
                .as_ref()
                .is_none_or(|(current, _)| *current != key)
            {
                cached.current = Some((
                    key,
                    crate::project_client::source_tools_owned(
                        control,
                        registration.binding,
                        self.status.clone(),
                    ),
                ));
            }
            let backend = cached.current.as_ref().unwrap().1.clone();
            drop(cached);
            backend.client(*self.client.lock().unwrap());
            backend
                .call_observed_until(tool, arguments, fence, observed.expires_at())
                .await
        })
    }
}
fn root_failure(
    resolution: crate::service_frontend::RootResolution,
) -> (&'static str, &'static str, &'static str) {
    match resolution {
        crate::service_frontend::RootResolution::Missing => (
            "PROJECT_NOT_REGISTERED",
            "missing",
            "No index is registered for this source folder. Open the trusted folder in AIRK Explorer to prepare it. This MCP connection remains usable after registration.",
        ),
        crate::service_frontend::RootResolution::Removed => (
            "PROJECT_REMOVED",
            "removed",
            "This source folder was explicitly removed from AIRK. Restore it in Management before using its index.",
        ),
        crate::service_frontend::RootResolution::Ambiguous => (
            "PROJECT_AMBIGUOUS",
            "ambiguous",
            "Multiple active indexes use this source folder. Configure --index-collection to choose one explicitly.",
        ),
        crate::service_frontend::RootResolution::Registered(_) => unreachable!(),
    }
}

struct Native {
    install: Option<PathBuf>,
    control: PathBuf,
    ready: Arc<RuntimeReadiness>,
    backend: Arc<dyn RemoteTools>,
    owns_readiness: bool,
}
impl RemoteTools for Native {
    fn local_help(&self) -> bool {
        self.backend.local_help()
    }
    fn scoped_selection(&self) -> Option<&airk_core::service::ScopedConnectRequest> {
        self.backend.scoped_selection()
    }
    fn combined_selection(&self) -> Option<&airk_core::service::CombinedConnectRequest> {
        self.backend.combined_selection()
    }
    fn catalog(&self) -> Vec<airk_mcp::remote::Tool> {
        self.backend.catalog()
    }
    fn uses_response_mode(&self, tool: &str) -> bool {
        self.backend.uses_response_mode(tool)
    }
    fn query_concurrency(&self) -> usize {
        crate::service_resource_policy::query_concurrency(&self.control)
    }
    fn close(&self) {
        if self.owns_readiness {
            self.ready.close_admission();
        }
        self.backend.close();
    }
    fn shutdown(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move {
            let backend = self.backend.shutdown().await;
            let readiness = if self.owns_readiness {
                self.ready.shutdown().await
            } else {
                Ok(())
            };
            backend.and(readiness)
        })
    }
    fn prepare(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move {
            if let Some(install) = &self.install {
                self.ready.prepare_status(install, &self.control).await?;
            }
            Ok(())
        })
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        self.backend.client(client);
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        self.call_with_capacity(tool, arguments, self.query_concurrency())
    }
    fn call_with_capacity(
        &self,
        tool: String,
        arguments: Map<String, Value>,
        capacity: usize,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            if matches!(tool.as_str(), "status" | "airk_status")
                && let Some(selection) = self.backend.combined_selection()
            {
                if let Some(install) = &self.install {
                    match tokio::time::timeout(
                        Duration::from_secs(10),
                        self.ready.prepare_status(install, &self.control),
                    )
                    .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            return Ok(combined_status_unavailable(
                                selection,
                                if error.is::<airk_mcp::remote::PreparationExpired>() {
                                    "combined_status_prepare_timeout"
                                } else {
                                    "combined_status_prepare_unavailable"
                                },
                                "prepare",
                                Some(false),
                            ));
                        }
                        Err(_) => {
                            return Ok(combined_status_unavailable(
                                selection,
                                "combined_status_prepare_timeout",
                                "prepare",
                                Some(false),
                            ));
                        }
                    }
                }
                return match tokio::time::timeout(
                    Duration::from_secs(2),
                    self.backend.call_with_capacity(tool, arguments, capacity),
                )
                .await
                {
                    Ok(Ok(result)) => Ok(result),
                    Ok(Err(_)) => Ok(combined_status_unavailable(
                        selection,
                        "combined_status_observation_unavailable",
                        "observation",
                        None,
                    )),
                    Err(_) => Ok(combined_status_unavailable(
                        selection,
                        "combined_status_timeout",
                        "observation",
                        None,
                    )),
                };
            }
            if let Some(install) = &self.install {
                self.ready.ensure(install, &self.control).await?;
            }
            self.backend
                .call_with_capacity(tool, arguments, capacity)
                .await
        })
    }
}

fn combined_status_unavailable(
    selection: &airk_core::service::CombinedConnectRequest,
    code: &str,
    stage: &str,
    submitted: Option<bool>,
) -> Value {
    let body = json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","index_state":"unknown","publication":null,"publication_vector":null,"files":null,"chunks":null,
        "member_observations":{"state":"unknown","members":null,"selection_confirmed":false},
        "error":{"code":code,"stage":stage,"retryable":true,"query_submitted":submitted,"query_replay_safe":false,"fresh_brokerage_required":true},
        "requested_scope":{"confirmed":false,"selection":selection}});
    json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":true})
}

/// Reuse the installed-runtime lifecycle owner for new read frontends. The
/// wrapped owner remains responsible for its selected native connections.
pub(crate) fn with_runtime_readiness(
    install: Option<PathBuf>,
    control: PathBuf,
    backend: Arc<dyn RemoteTools>,
) -> Arc<dyn RemoteTools> {
    Arc::new(Native {
        install,
        control,
        ready: Arc::new(RuntimeReadiness::default()),
        backend,
        owns_readiness: true,
    })
}

/// One combined native authority under the same unified MCP endpoint. Explorer
/// requests carry no arbitrary legacy binding or guessed source-folder origin.
pub async fn serve_combined(
    backend: Arc<dyn RemoteTools>,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    ensure!(
        backend.combined_selection().is_some(),
        "combined native selection required"
    );
    let explorer = descriptor.map(|descriptor| {
        Arc::new(Explorer {
            descriptor,
            roots: BTreeMap::new(),
            scopes: BTreeMap::new(),
            control: PathBuf::new(),
        }) as Arc<dyn ExplorerTools>
    });
    let backend = Arc::new(Unified::combined(backend, explorer)?);
    crate::client_owner::watch()?;
    let close = backend.clone();
    let result = airk_mcp::remote::serve_with_input_lifecycle(
        backend.clone(),
        Arc::new(move || close.close()),
    )
    .await;
    let cleanup = backend.shutdown().await;
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("combined native cleanup also failed: {cleanup:#}")))
        }
        (Err(error), _) => Err(error),
        (Ok(()), cleanup) => cleanup,
    }
}
impl ExplorerTools for Explorer {
    fn call(
        &self,
        project: Option<String>,
        tool: String,
        arguments: Map<String, Value>,
        client: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_> {
        let descriptor = self.descriptor.clone();
        let root = project.as_ref().and_then(|id| self.roots.get(id)).cloned();
        let scope = project.as_ref().and_then(|id| self.scopes.get(id)).cloned();
        let control = self.control.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let (project, root) = if let Some(scope) = scope {
                    match scope.resolve(&control)? {
                        crate::service_frontend::RootResolution::Registered(registration) => {
                            (Some(registration.binding.to_string()), Some(scope.root))
                        }
                        other => {
                            let (code, _, message) = root_failure(other);
                            return Ok(airk_mcp::unified::failure(code, message));
                        }
                    }
                } else {
                    (project, root)
                };
                call_explorer(descriptor, project, root, tool, arguments, client)
            })
            .await?
        })
    }
}
/// One-shot call to a running Explorer window through its verified launch descriptor.
pub fn call_explorer(
    descriptor: PathBuf,
    project: Option<String>,
    root: Option<PathBuf>,
    tool: String,
    arguments: Map<String, Value>,
    client: airk_core::usage::ClientFamily,
) -> Result<Value> {
    let launch = crate::explorer_assistance::plan(&descriptor)?;
    let input = serde_json::to_vec(
        &json!({"tool":tool,"arguments":arguments,"client":format!("{client:?}")}),
    )?;
    ensure!(input.len() <= 65536, "Explorer request exceeds bound");
    let mut command = crate::explorer_assistance::command(&launch);
    command
        .arg("--one-shot")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let (Some(project), Some(root)) = (project, root) {
        command
            .arg("--project-binding")
            .arg(project)
            .arg("--project-root")
            .arg(root);
    }
    let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
    let mut stdin = child.stdin.take().context("Explorer stdin unavailable")?;
    let stdout = child.stdout.take().context("Explorer stdout unavailable")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Drain our exact child before joining its pipe threads, including timeout/error paths.
    drop(child);
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("Explorer input thread failed"))??;
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Explorer output thread failed"))??;
    ensure!(
        status.is_some_and(|s| s.success()) && bytes.len() <= 1_048_576,
        "Explorer exchange failed or timed out"
    );
    let reply: Value = serde_json::from_slice(&bytes)?;
    if reply["ok"] == true {
        Ok(airk_mcp::unified::success(reply["value"].clone()))
    } else {
        Ok(airk_mcp::unified::failure(reply["code"].as_str().unwrap_or("UI_UNAVAILABLE"),reply["detail"].as_str().unwrap_or("Explorer request unavailable; inspect status for attachment and project selection.")))
    }
}

pub async fn serve(
    control: PathBuf,
    install: Option<PathBuf>,
    registration: ProjectRegistration,
    additional: Vec<ProjectBinding>,
    no_default: bool,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    serve_selected(
        control,
        install,
        registration,
        additional,
        no_default,
        descriptor,
        None,
    )
    .await
}

pub async fn serve_scoped(
    control: PathBuf,
    install: Option<PathBuf>,
    registration: ProjectRegistration,
    selection: airk_core::service::ScopedConnectRequest,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    selection.validate()?;
    serve_selected(
        control,
        install,
        registration,
        vec![],
        false,
        descriptor,
        Some(selection),
    )
    .await
}

async fn serve_selected(
    control: PathBuf,
    install: Option<PathBuf>,
    registration: ProjectRegistration,
    additional: Vec<ProjectBinding>,
    no_default: bool,
    descriptor: Option<PathBuf>,
    selection: Option<airk_core::service::ScopedConnectRequest>,
) -> Result<()> {
    ensure!(additional.len() < 32, "too many configured projects");
    let primary = registration.binding.to_string();
    let mut registrations = vec![registration];
    if !additional.is_empty() {
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
        for binding in additional {
            let saved = registry.get(binding)?;
            registrations.push(saved.registration);
        }
    }
    let roots = registrations
        .iter()
        .map(|p| (p.binding.to_string(), PathBuf::from(&p.root)))
        .collect();
    let ready = Arc::new(RuntimeReadiness::default());
    let projects = registrations
        .into_iter()
        .map(|p| -> Result<Project> {
            Ok(Project {
                binding: p.binding.to_string(),
                name: PathBuf::from(&p.root)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.collection.clone()),
                collection: p.collection,
                backend: Arc::new(Native {
                    install: install.clone(),
                    control: control.clone(),
                    ready: ready.clone(),
                    owns_readiness: false,
                    backend: match &selection {
                        Some(selection) => crate::project_client::scoped_tools(
                            control.clone(),
                            p.binding,
                            selection.clone(),
                        )?,
                        None => crate::project_client::tools(control.clone(), p.binding),
                    },
                }),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let explorer = descriptor.map(|descriptor| {
        Arc::new(Explorer {
            descriptor,
            roots,
            scopes: BTreeMap::new(),
            control: control.clone(),
        }) as Arc<dyn ExplorerTools>
    });
    let backend = Arc::new(Unified::new(
        projects,
        (!no_default).then_some(primary),
        explorer,
    )?);
    crate::client_owner::watch()?;
    let close = ready.clone();
    let observation_close = backend.clone();
    let served = airk_mcp::remote::serve_with_input_lifecycle(
        backend.clone(),
        Arc::new(move || {
            close.close_admission();
            observation_close.close();
        }),
    )
    .await;
    let observation_cleanup = backend.shutdown().await;
    let cleanup = match (ready.shutdown().await, observation_cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "status observation cleanup also failed: {cleanup:#}"
        ))),
        (Err(error), _) => Err(error),
        (_, result) => result,
    };
    match (served, cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "runtime readiness cleanup also failed: {cleanup:#}"
        ))),
        (Err(error), _) => Err(error),
        (Ok(()), cleanup) => cleanup,
    }
}

pub async fn serve_roots(
    control: PathBuf,
    install: PathBuf,
    scopes: Vec<crate::service_frontend::RootScope>,
    no_default: bool,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    ensure!(
        !scopes.is_empty() && scopes.len() <= 32,
        "invalid configured source count"
    );
    let primary = scopes[0].key.clone();
    let ready = Arc::new(RuntimeReadiness::default());
    let configured = scopes
        .iter()
        .map(|scope| (scope.key.clone(), scope.clone()))
        .collect();
    let mut lookups = Vec::with_capacity(scopes.len());
    let projects = scopes
        .into_iter()
        .map(|scope| {
            let lookup = Arc::new(SourceLookupOwner::new());
            lookups.push(lookup.clone());
            Project {
                binding: scope.key.clone(),
                name: scope.root.to_string_lossy().into_owned(),
                collection: scope
                    .collection
                    .clone()
                    .unwrap_or_else(|| "automatic".into()),
                backend: Arc::new(RootNative {
                    scope,
                    control: control.clone(),
                    install: install.clone(),
                    ready: ready.clone(),
                    client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
                    sequence: std::sync::atomic::AtomicU64::new(0),
                    observation: std::sync::Mutex::new(None),
                    lookup,
                    status: Arc::new(crate::project_client::StatusOwner::default()),
                    backend: tokio::sync::Mutex::new(SourceBackendCache::default()),
                }),
            }
        })
        .collect();
    let explorer = descriptor.map(|descriptor| {
        Arc::new(Explorer {
            descriptor,
            roots: BTreeMap::new(),
            scopes: configured,
            control,
        }) as Arc<dyn ExplorerTools>
    });
    let backend = Arc::new(Unified::new(
        projects,
        (!no_default).then_some(primary),
        explorer,
    )?);
    crate::client_owner::watch()?;
    let close = ready.clone();
    let observation_close = backend.clone();
    let lookup_close = lookups.clone();
    let served = airk_mcp::remote::serve_with_input_lifecycle(
        backend.clone(),
        Arc::new(move || {
            close.close_admission();
            observation_close.close();
            for lookup in &lookup_close {
                lookup.close();
            }
        }),
    )
    .await;
    let observation_cleanup = backend.shutdown().await;
    let mut cleanup = match (ready.shutdown().await, observation_cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "status observation cleanup also failed: {cleanup:#}"
        ))),
        (Err(error), _) => Err(error),
        (_, result) => result,
    };
    for lookup in &lookups {
        if let Err(error) = lookup.shutdown().await {
            cleanup = Err(match cleanup {
                Ok(()) => error,
                Err(primary) => {
                    primary.context(format!("source lookup cleanup also failed: {error:#}"))
                }
            });
        }
    }
    match (served, cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "runtime readiness cleanup also failed: {cleanup:#}"
        ))),
        (Err(error), _) => Err(error),
        (Ok(()), cleanup) => cleanup,
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    use std::future::Future;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn status_setup_uses_original_start_and_drains_accepted_work_after_waiter_expiry()
    -> Result<()> {
        let ready = Arc::new(RuntimeReadiness::default());
        let (release, released) = std::sync::mpsc::channel();
        let (entered, began) = tokio::sync::oneshot::channel();
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let done = completed.clone();
        let first_owner = ready.clone();
        let first = tokio::spawn(async move {
            first_owner
                .run_inner_status(
                    move |setup| async move {
                        let setup = setup.context("cooperative setup required")?;
                        setup
                            .run_worker(move |control| {
                                let _ = entered.send(control);
                                released
                                    .recv()
                                    .context("release accepted setup operation")?;
                                done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                Ok(PathBuf::from("accepted-completion"))
                            })
                            .await?;
                        Ok(())
                    },
                    true,
                    true,
                )
                .await
        });
        let setup = began.await?;
        // The late waiter must inherit the first attempt's original deadline,
        // rather than receiving another ten seconds from its own admission.
        tokio::time::sleep(Duration::from_secs(8)).await;
        let late_owner = ready.clone();
        let late = tokio::spawn(async move {
            late_owner
                .run_inner_status(
                    |_| async { anyhow::bail!("replacement setup admitted") },
                    true,
                    true,
                )
                .await
        });
        let first_error = tokio::time::timeout(Duration::from_secs(5), first)
            .await??
            .unwrap_err();
        let late_error = tokio::time::timeout(Duration::from_secs(1), late)
            .await??
            .unwrap_err();
        assert!(first_error.is::<airk_mcp::remote::PreparationExpired>());
        assert!(late_error.is::<airk_mcp::remote::PreparationExpired>());
        assert_eq!(completed.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(
            setup.worker_is_owned(),
            "waiter expiry detached the accepted worker"
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            while !setup.admission_is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("accepted worker was dropped"))?;
        await_published_readiness(&ready).await?;
        tokio::time::timeout(
            Duration::from_secs(5),
            ready.run_inner_status(
                |_| async { anyhow::bail!("replacement setup admitted") },
                true,
                true,
            ),
        )
        .await??;
        assert_eq!(completed.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            setup
                .run_worker(|_| Ok(PathBuf::from("forbidden-later-work")))
                .await
                .is_err(),
            "original setup deadline did not close later admission"
        );
        ready.shutdown().await?;
        Ok(())
    }
    async fn await_published_readiness(ready: &RuntimeReadiness) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let published = match &*ready.state.lock().await {
                    ReadinessState::Running(attempt) => attempt.result.borrow().is_some(),
                    _ => false,
                };
                if published {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        Ok(())
    }
    #[tokio::test]
    async fn prompt_deadline_cancellation_is_expiry_even_when_inner_completion_is_polled_first()
    -> Result<()> {
        let ready = RuntimeReadiness::default();
        let mut waiter = Box::pin(ready.run_inner_status(
            |setup| async move {
                let setup = setup.context("cooperative setup required")?;
                while !setup.admission_is_closed() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                anyhow::bail!("runtime setup cancelled")
            },
            true,
            true,
        ));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(waiter.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        // Deliberately stop polling the waiter. The actual driver closes and
        // publishes its prompt cancellation before timeout_at polls inner first.
        tokio::time::sleep(Duration::from_secs(10)).await;
        await_published_readiness(&ready).await?;
        let error = waiter.await.unwrap_err();
        assert!(
            error.is::<airk_mcp::remote::PreparationExpired>(),
            "wrong deadline cause: {error:#}"
        );
        assert!(matches!(*ready.state.lock().await, ReadinessState::Idle));
        ready
            .run_inner_status(|_| async { Ok(()) }, true, true)
            .await?;
        ready.shutdown().await?;
        Ok(())
    }
    #[tokio::test]
    async fn genuine_setup_error_published_before_deadline_remains_error_when_observed_late()
    -> Result<()> {
        let ready = RuntimeReadiness::default();
        let mut waiter = Box::pin(ready.run_inner_status(
            |_| async {
                Err(anyhow::Error::from(std::io::Error::other(
                    "early verification failure",
                )))
            },
            true,
            true,
        ));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(waiter.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        await_published_readiness(&ready).await?;
        tokio::time::sleep(Duration::from_secs(10)).await;
        let error = waiter.await.unwrap_err();
        assert!(!error.is::<airk_mcp::remote::PreparationExpired>());
        assert_eq!(format!("{error:#}"), "early verification failure");
        assert!(matches!(*ready.state.lock().await, ReadinessState::Idle));
        ready.shutdown().await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn status_deadline_cancellation_remains_expiry_for_later_waiter() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (release, released) = std::sync::mpsc::channel();
        let mut first = Box::pin(ready.run_inner_status(
            move |setup| async move {
                let setup = setup.context("cooperative setup required")?;
                setup
                    .run_worker(move |_| {
                        released.recv().context("expired setup release")?;
                        Ok(PathBuf::from("accepted-operation"))
                    })
                    .await?;
                // Use the production admission seam after accepted work drains.
                setup
                    .run_worker(|_| Ok(PathBuf::from("forbidden-later-operation")))
                    .await?;
                Ok(())
            },
            true,
            true,
        ));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(first.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert!(
            first
                .await
                .unwrap_err()
                .is::<airk_mcp::remote::PreparationExpired>()
        );
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("accepted setup operation was lost"))?;
        await_published_readiness(&ready).await?;
        let error = ready
            .run_inner_status(
                |_| async { anyhow::bail!("replacement admitted before expired setup settled") },
                true,
                true,
            )
            .await
            .unwrap_err();
        assert!(
            error.is::<airk_mcp::remote::PreparationExpired>(),
            "late waiter lost the setup deadline cause: {error:#}"
        );
        assert!(matches!(*ready.state.lock().await, ReadinessState::Idle));
        ready
            .run_inner_status(|_| async { Ok(()) }, true, true)
            .await?;
        ready.shutdown().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn accepted_late_setup_failure_preserves_actual_cause_for_later_caller_and_allows_retry()
    -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (release, released) = std::sync::mpsc::channel();
        let mut first = Box::pin(ready.run_inner_status(
            move |setup| async move {
                setup
                    .context("cooperative setup required")?
                    .run_worker(move |_| {
                        released.recv().context("accepted late failure release")?;
                        Err(std::io::Error::other("actual accepted operation failure").into())
                    })
                    .await?;
                Ok(())
            },
            true,
            true,
        ));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(first.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        let error = first.await.unwrap_err();
        assert!(error.is::<airk_mcp::remote::PreparationExpired>());
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("accepted operation lost"))?;
        await_published_readiness(&ready).await?;
        let error = ready
            .run_inner_status(
                |_| async { anyhow::bail!("replacement admitted before actual cause") },
                true,
                true,
            )
            .await
            .unwrap_err();
        assert_eq!(format!("{error:#}"), "actual accepted operation failure");
        assert!(matches!(*ready.state.lock().await, ReadinessState::Idle));
        ready
            .run_inner_status(|_| async { Ok(()) }, true, true)
            .await?;
        ready.shutdown().await?;
        Ok(())
    }
    #[test]
    fn shared_error_preserves_exact_plain_and_nested_chains() {
        for nested in [false, true] {
            let original = anyhow::Error::from(std::io::Error::other("typed failure"));
            let original = if nested {
                original.context("verification context")
            } else {
                original
            };
            let expected = format!("{original:#}");
            let wrapped = anyhow::Error::new(SharedReadinessError(Arc::new(original)));
            assert_eq!(format!("{wrapped:#}"), expected);
            assert!(
                wrapped
                    .downcast_ref::<SharedReadinessError>()
                    .unwrap()
                    .original()
                    .downcast_ref::<std::io::Error>()
                    .is_some()
            );
            assert_eq!(wrapped.chain().count(), if nested { 2 } else { 1 });
        }
    }

    #[tokio::test]
    async fn cancelled_failure_and_shutdown_drains_preserve_join_cause() -> Result<()> {
        for shutdown in [false, true] {
            let ready = RuntimeReadiness::default();
            let setup = Arc::new(crate::runtime_install::SetupCancellation::default());
            let (release, released) = std::sync::mpsc::channel();
            let (started, began) = tokio::sync::oneshot::channel();
            let worker_setup = setup.clone();
            let driver = tokio::spawn(async move {
                let mut work = Box::pin(worker_setup.run_worker(move |_| {
                    let _ = started.send(());
                    released.recv().context("worker release missing")?;
                    Ok(PathBuf::from("joined"))
                }));
                tokio::select! {
                    _ = &mut work => panic!("worker unexpectedly finished"),
                    _ = began => panic!("retained driver panic"),
                }
            });
            let (_sender, result) = tokio::sync::watch::channel(None);
            let attempt = Arc::new(ReadinessAttempt {
                started: Instant::now(),
                completed: Default::default(),
                status_deadline: tokio::sync::watch::channel(false).0,
                result,
                driver_result: tokio::sync::OnceCell::new(),
                failed_completion: tokio::sync::OnceCell::new(),
                setup: Some(setup.clone()),
                abort: std::sync::OnceLock::new(),
                task: tokio::sync::Mutex::new(Some(driver)),
            });
            *ready.state.lock().await = ReadinessState::Running(attempt.clone());
            let original = attempt.joined_driver().await.unwrap_err();
            drop(_sender);
            assert!(
                original
                    .chain()
                    .any(|e| e.downcast_ref::<tokio::task::JoinError>().is_some())
            );
            let mut first: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>> =
                if shutdown {
                    Box::pin(async {
                        let _ = ready.shutdown().await;
                    })
                } else {
                    Box::pin(async {
                        let _ = attempt.failed_join().await;
                    })
                };
            std::future::poll_fn(|cx| {
                assert!(first.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            drop(first);
            assert!(setup.worker_is_owned());
            assert!(attempt.failed_completion.get().is_none());
            assert!(matches!(
                *ready.state.lock().await,
                ReadinessState::Running(_)
            ));
            if !shutdown {
                let mut second =
                    Box::pin(ready.run(|| async { panic!("replacement before drain") }));
                std::future::poll_fn(|cx| {
                    assert!(second.as_mut().poll(cx).is_pending());
                    std::task::Poll::Ready(())
                })
                .await;
                drop(second);
            }
            release.send(())?;
            let error = if shutdown {
                ready.shutdown().await.unwrap_err()
            } else {
                anyhow::Error::new(SharedReadinessError(
                    attempt.failed_join().await.unwrap_err(),
                ))
            };
            assert!(format!("{error:#}").contains("retained driver panic"));
            assert!(
                error
                    .chain()
                    .any(|e| e.downcast_ref::<tokio::task::JoinError>().is_some())
            );
            assert!(!setup.worker_is_owned());
        }
        Ok(())
    }

    #[tokio::test]
    async fn request_deadline_retains_one_initialization_and_its_result() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let mut waiting = Box::pin(ready.run(|| async move {
            let _ = started.send(());
            released.await?;
            Ok(())
        }));
        tokio::select! {
            _ = &mut waiting => panic!("verification must wait for its owned observation"),
            result = began => result?,
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(25), waiting)
                .await
                .is_err()
        );
        release.send(()).unwrap();
        ready
            .run(|| async { panic!("a timed-out request must not restart verification") })
            .await?;
        ready
            .run(|| async { panic!("verified session must reuse readiness") })
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn verification_failure_is_reported_and_never_cached_as_ready() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let mut waiting = Box::pin(ready.run(|| async move {
            let _ = started.send(());
            released.await?;
            anyhow::bail!("selected service control changed")
        }));
        tokio::select! {
            _ = &mut waiting => panic!("verification must remain pending"),
            result = began => result?,
        }
        drop(waiting);
        release.send(()).unwrap();
        let error = ready
            .run(|| async { panic!("retain the original failure") })
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("selected service control changed")
        );
        let mut retried = false;
        ready
            .run(|| {
                retried = true;
                async { Ok(()) }
            })
            .await?;
        assert!(retried);
        Ok(())
    }

    #[tokio::test]
    async fn session_drop_cancels_only_its_pending_verification() -> Result<()> {
        struct Cancelled(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Cancelled {
            fn drop(&mut self) {
                if let Some(sender) = self.0.take() {
                    let _ = sender.send(());
                }
            }
        }
        let ready = RuntimeReadiness::default();
        let (started, began) = tokio::sync::oneshot::channel();
        let (cancelled, observed) = tokio::sync::oneshot::channel();
        let mut waiting = Box::pin(ready.run(|| async move {
            let _guard = Cancelled(Some(cancelled));
            let _ = started.send(());
            std::future::pending::<Result<()>>().await
        }));
        tokio::select! {
            _ = &mut waiting => panic!("verification must remain pending"),
            result = began => result?,
        }
        drop(waiting);
        drop(ready);
        tokio::time::timeout(Duration::from_secs(5), observed).await??;
        Ok(())
    }

    #[tokio::test]
    async fn coalesced_failure_tickets_do_not_poison_later_repair_or_reset_ready() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (release, released) = tokio::sync::oneshot::channel();
        let mut first = Box::pin(ready.run(|| async move {
            released.await?;
            Err(
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, "owned fixture denial")
                    .into(),
            )
        }));
        let mut second =
            Box::pin(ready.run(|| async { panic!("shared waiter must not initialize") }));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(first.as_mut(), cx).is_pending());
            assert!(std::future::Future::poll(second.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        release.send(()).unwrap();
        let first_error = first.await.unwrap_err();
        assert!(
            first_error
                .downcast_ref::<SharedReadinessError>()
                .expect("shared failure")
                .original()
                .chain()
                .any(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
                })
        );
        ready.run(|| async { Ok(()) }).await?;
        let second_error = second.await.unwrap_err();
        assert_eq!(second_error.to_string(), first_error.to_string());
        ready
            .run(|| async { panic!("old failure ticket must not reset repaired readiness") })
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn explicit_owner_shutdown_joins_pending_attempt_and_closes_admission() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let mut waiting =
            Box::pin(ready.run(|| async { std::future::pending::<Result<()>>().await }));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(waiting.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        ready.shutdown().await?;
        assert!(
            waiting
                .await
                .unwrap_err()
                .to_string()
                .contains("session closed")
        );
        assert!(
            ready
                .run(|| async { panic!("closed owner cannot initialize") })
                .await
                .is_err()
        );
        Ok(())
    }
    #[tokio::test]
    async fn panic_and_abort_fan_out_real_join_failures_then_allow_repair() -> Result<()> {
        for panic_case in [false, true] {
            let ready = RuntimeReadiness::default();
            let (release, released) = tokio::sync::oneshot::channel();
            let mut first = Box::pin(ready.run(|| async move {
                released.await?;
                if panic_case {
                    panic!("original owned initializer panic");
                }
                std::future::pending::<Result<()>>().await
            }));
            let mut second = Box::pin(ready.run(|| async { panic!("must share attempt") }));
            std::future::poll_fn(|cx| {
                assert!(std::future::Future::poll(first.as_mut(), cx).is_pending());
                assert!(std::future::Future::poll(second.as_mut(), cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            if panic_case {
                release.send(()).unwrap();
            } else {
                let attempt = {
                    let state = ready.state.lock().await;
                    let ReadinessState::Running(attempt) = &*state else {
                        panic!("owned attempt required")
                    };
                    attempt.clone()
                };
                let slot = attempt.task.lock().await;
                slot.as_ref().unwrap().abort();
            }
            let first_error = first.await.unwrap_err();
            let second_error = second.await.unwrap_err();
            assert_eq!(
                first_error.to_string(),
                "installed runtime verification task failed"
            );
            assert_eq!(second_error.to_string(), first_error.to_string());
            for error in [&first_error, &second_error] {
                let join = error
                    .chain()
                    .find_map(|cause| cause.downcast_ref::<tokio::task::JoinError>())
                    .expect("real task JoinError remains discoverable");
                assert_eq!(join.is_panic(), panic_case);
                assert_eq!(join.is_cancelled(), !panic_case);
                if panic_case {
                    assert!(format!("{error:#}").contains("original owned initializer panic"));
                }
            }
            ready.run(|| async { Ok(()) }).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn shared_failure_keeps_display_classification_and_source_chain() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let error = ready
            .run(|| async {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "service_explicitly_stopped: original owned refusal",
                )
                .into())
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "service_explicitly_stopped: original owned refusal"
        );
        assert!(error.to_string().contains("service_explicitly_stopped"));
        assert!(
            error
                .downcast_ref::<SharedReadinessError>()
                .expect("shared failure")
                .original()
                .chain()
                .any(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|value| value.kind() == std::io::ErrorKind::PermissionDenied)
                })
        );
        ready.run(|| async { Ok(()) }).await?;
        Ok(())
    }
    #[tokio::test]
    async fn cancelled_shutdown_retains_attempt_handle_for_a_later_drain() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let mut waiting =
            Box::pin(ready.run(|| async { std::future::pending::<Result<()>>().await }));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(waiting.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        let attempt = {
            let state = ready.state.lock().await;
            let ReadinessState::Running(attempt) = &*state else {
                panic!("owned attempt required")
            };
            attempt.clone()
        };
        // Stall disposal at its owner-held handle, not with wall-clock sleeps.
        let slot = attempt.task.lock().await;
        let mut shutdown = Box::pin(ready.shutdown());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(shutdown.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(shutdown);
        assert!(
            slot.is_some(),
            "cancelled shutdown cannot take/detach the task"
        );
        assert!(
            matches!(&*ready.state.lock().await, ReadinessState::Running(current)
        if Arc::ptr_eq(current, &attempt))
        );
        assert!(
            ready
                .run(|| async { panic!("closing owner cannot admit replacement") })
                .await
                .is_err()
        );
        drop(slot);
        ready.shutdown().await?;
        assert!(
            attempt.task.lock().await.is_none(),
            "later shutdown joins the retained task"
        );
        assert!(waiting.await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn cooperative_setup_shutdown_keeps_worker_owned_until_join() -> Result<()> {
        let ready = RuntimeReadiness::default();
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let mut waiting = Box::pin(ready.run_inner(
            move |setup| async move {
                setup
                    .expect("owned setup")
                    .run_worker(move |_| {
                        let _ = started.send(());
                        released.recv().context("owned release missing")?;
                        Ok(PathBuf::from("owned-complete"))
                    })
                    .await?;
                Ok(())
            },
            true,
        ));
        tokio::select! {
            _ = &mut waiting => anyhow::bail!("worker must remain pending"),
            result = began => result?,
        }
        assert!(
            tokio::time::timeout(
                Duration::ZERO,
                ready.run(|| async { panic!("a waiter cannot replace owned setup") })
            )
            .await
            .is_err()
        );
        let attempt = {
            let state = ready.state.lock().await;
            let ReadinessState::Running(attempt) = &*state else {
                panic!("owned attempt required")
            };
            attempt.clone()
        };
        let setup = attempt
            .setup
            .as_ref()
            .expect("cooperative worker owner")
            .clone();
        let mut shutdown = Box::pin(ready.shutdown());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(shutdown.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(shutdown);
        assert!(
            attempt.task.lock().await.is_some(),
            "driver handle remains owned after canceled join"
        );
        assert!(
            setup.worker_is_owned(),
            "blocking worker handle remains owned while blocked"
        );
        assert!(
            ready
                .run(|| async { panic!("closed setup cannot restart") })
                .await
                .is_err()
        );
        release.send(())?;
        ready.shutdown().await?;
        assert!(
            attempt.task.lock().await.is_none(),
            "second shutdown joined the driver"
        );
        assert!(
            !setup.worker_is_owned(),
            "second shutdown joined the blocking worker"
        );
        assert!(waiting.await.is_err());
        Ok(())
    }
}

#[cfg(test)]
mod source_backend_sequence_tests {
    use super::*;

    #[test]
    fn old_namespace_observation_cannot_repopulate_replaced_backend() {
        let project = "ab".repeat(32).parse().unwrap();
        let old_namespace = airk_core::identity::Incarnation::from_bytes([1; 16]);
        let new_namespace = airk_core::identity::Incarnation::from_bytes([2; 16]);
        let mut cache = SourceBackendCache::default();
        assert!(cache.admit_sequence(1));
        cache.current = Some((
            (project, old_namespace),
            crate::project_client::source_tools(PathBuf::new(), project),
        ));
        assert!(cache.admit_sequence(3));
        let replacement = crate::project_client::source_tools(PathBuf::new(), project);
        cache.current = Some(((project, new_namespace), replacement.clone()));
        // Snapshot 2 was taken in namespace 1, but resumes after namespace 2 won.
        assert!(!cache.admit_sequence(2));
        let (key, retained) = cache.current.as_ref().unwrap();
        assert_eq!(*key, (project, new_namespace));
        assert!(Arc::ptr_eq(retained, &replacement));
    }
}

#[cfg(test)]
mod source_lookup_ownership_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test(flavor = "current_thread")]
    async fn four_cancelled_callers_cannot_release_blocked_lookup_or_accumulate_workers()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let scope = crate::service_frontend::RootScope::new(temporary.path(), None)?;
        let control = temporary.path().join("control");
        std::fs::create_dir(&control)?;
        let owner = Arc::new(SourceLookupOwner::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, held) = std::sync::mpsc::channel();
        let request_owner = owner.clone();
        let counted = calls.clone();
        let root = scope.clone();
        let path = control.clone();
        let first = tokio::spawn(async move {
            request_owner
                .run(Instant::now() + Duration::from_secs(30), move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    let _ = entered.send(());
                    held.recv()?;
                    Ok(Arc::new(root.observe(&path)?))
                })
                .await
        });
        started.await?;
        let mut requests = vec![first];
        for _ in 0..3 {
            let request_owner = owner.clone();
            let counted = calls.clone();
            let root = scope.clone();
            let path = control.clone();
            requests.push(tokio::spawn(async move {
                request_owner
                    .run(Instant::now() + Duration::from_secs(30), move || {
                        counted.fetch_add(1, Ordering::SeqCst);
                        Ok(Arc::new(root.observe(&path)?))
                    })
                    .await
            }));
        }
        tokio::task::yield_now().await;
        for request in &requests {
            request.abort();
        }
        for request in requests {
            assert!(matches!(request.await,Err(error) if error.is_cancelled()));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            owner.permit.available_permits(),
            0,
            "caller Drop cannot release actual worker admission"
        );
        assert!(
            owner
                .worker
                .lock()
                .await
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
        );
        let root = scope.clone();
        let path = control.clone();
        assert!(
            owner
                .run(Instant::now(), move || Ok(Arc::new(root.observe(&path)?)))
                .await
                .is_err()
        );
        let mut drain = Box::pin(owner.shutdown());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(drain.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(drain);
        assert!(
            owner.worker.lock().await.is_some(),
            "cancelled drain retains actual handle"
        );
        release.send(())?;
        owner.shutdown().await?;
        assert!(owner.worker.lock().await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(std::fs::read_dir(&control)?.next().is_none());
        let path = control.clone();
        assert!(
            owner
                .run(Instant::now() + Duration::from_secs(2), move || Ok(
                    Arc::new(scope.observe(&path)?)
                ))
                .await
                .is_err()
        );
        Ok(())
    }
}

#[cfg(test)]
mod source_selector_wait_tests {
    use super::*;
    #[tokio::test]
    async fn scope_deadline_expired_while_selector_waited_is_unsubmitted_and_never_cached()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let control = owner.path().join("control");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&control)?;
        let ready = Arc::new(RuntimeReadiness::default());
        *ready.state.lock().await = ReadinessState::Ready; // isolate selection after authenticated setup.
        let native = Arc::new(RootNative {
            scope: crate::service_frontend::RootScope::new(&source, None)?,
            control: control.clone(),
            install: owner.path().join("unused-install"),
            ready,
            client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
            sequence: std::sync::atomic::AtomicU64::new(0),
            observation: std::sync::Mutex::new(None),
            lookup: Arc::new(SourceLookupOwner::new()),
            status: Arc::new(crate::project_client::StatusOwner::default()),
            backend: tokio::sync::Mutex::new(SourceBackendCache::default()),
        });
        let guard = native.backend.lock().await;
        let request_native = native.clone();
        let request =
            tokio::spawn(
                async move { request_native.call("airk_status".into(), Map::new()).await },
            );
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if native.sequence.load(std::sync::atomic::Ordering::Relaxed) == 1
                    && native.lookup.permit.available_permits() == 1
                    && native.lookup.worker.lock().await.is_none()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        // Metadata completed; only the selector lock remains. Advance beyond
        // the original two-second absolute admission, with no latency assertion.
        tokio::time::sleep(Duration::from_millis(2050)).await;
        drop(guard);
        let reply = request.await??;
        let detail: Value = serde_json::from_str(
            reply["content"][0]["text"]
                .as_str()
                .context("retry body missing")?,
        )?;
        assert_eq!(detail["error"]["query_submitted"], false);
        assert_eq!(detail["error"]["retryable"], true);
        assert!(native.observation.lock().unwrap().is_none());
        assert!(native.backend.lock().await.current.is_none());
        native.lookup.shutdown().await?;
        assert!(std::fs::read_dir(control)?.next().is_none());
        Ok(())
    }
}
