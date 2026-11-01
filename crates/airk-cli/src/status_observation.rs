// SPDX-License-Identifier: Apache-2.0
//! One frontend-owned scalar observation; never an evidence-query replay queue.
use super::*;
use anyhow::Context;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
type Shared = std::result::Result<Arc<Snapshot>, Arc<anyhow::Error>>;
#[derive(Debug)]
struct ObservationFailure(Arc<anyhow::Error>);
impl std::fmt::Display for ObservationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
impl std::error::Error for ObservationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        let original: &(dyn std::error::Error + Send + Sync) = self.0.as_ref().as_ref();
        original.source()
    }
}

#[derive(Clone, PartialEq)]
struct Key {
    project: ProjectBinding,
    source: Option<SourceFence>,
    expires: Option<Instant>,
    args: serde_json::Value,
}
struct Snapshot {
    options: serde_json::Value,
    scope: CacheScope,
    epoch: u64,
    result: serde_json::Value,
    cached: Option<CachedStatus>,
    submitted: bool,
    reused: bool,
    receipt: airk_mcp::response::StatusReceipt,
}
struct Attempt {
    key: Key,
    result: tokio::sync::watch::Receiver<Option<Shared>>,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    joined: Mutex<Option<std::result::Result<(), Arc<anyhow::Error>>>>,
    failure: tokio::sync::OnceCell<Shared>,
}
#[derive(Default)]
enum State {
    #[default]
    Idle,
    Running(Arc<Attempt>),
    Closed,
}
// A permit covers exactly one immediately following request, never the pipeline.
// The retained observer owns its completion; it is not a durable service receipt.
pub(super) struct OperationPermit<'a> {
    _owner: std::marker::PhantomData<&'a StatusOwner>,
}
#[derive(Clone, Copy)]
pub(super) struct RequestTicket {
    pub(super) scope: Option<CacheScope>,
    pub(super) authority: u64,
    pub(super) scalar_epoch: u64,
    pub(super) receipt: airk_mcp::response::StatusReceipt,
    project: ProjectBinding,
    expires: Option<Instant>,
}
#[derive(Default)]
struct WorkerAuthority {
    generation: u64,
    known: Option<(
        airk_core::identity::Incarnation,
        airk_core::service::WorkerIdentity,
    )>,
}
#[derive(Debug)]
pub(super) struct RetiredRequest;
impl std::fmt::Display for RetiredRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("frontend observation retired before query submission")
    }
}
impl std::error::Error for RetiredRequest {}
pub(crate) struct StatusOwner {
    closed: AtomicBool,
    epoch: AtomicU64,
    state: tokio::sync::Mutex<State>,
    scope: Mutex<Option<CacheScope>>,
    authority: Mutex<WorkerAuthority>,
    cache: Mutex<Option<Arc<Snapshot>>>,
    scope_worker: tokio::sync::Mutex<Option<tokio::task::JoinHandle<Result<Option<CacheScope>>>>>,
    receipt: Mutex<airk_mcp::response::StatusReceipt>,
    connection: tokio::sync::Mutex<Option<(CacheScope, Connection)>>,
}
impl Default for StatusOwner {
    fn default() -> Self {
        Self {
            closed: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            state: Default::default(),
            scope: Mutex::new(None),
            authority: Default::default(),
            cache: Mutex::new(None),
            scope_worker: Default::default(),
            receipt: Default::default(),
            connection: Default::default(),
        }
    }
}
impl Drop for StatusOwner {
    fn drop(&mut self) {
        self.close();
    }
}
impl Attempt {
    async fn join(&self) -> Result<()> {
        let mut slot = self.task.lock().await;
        let remembered = self
            .joined
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let result = if let Some(result) = remembered {
            result
        } else {
            let result = if let Some(task) = slot.as_mut() {
                task.await
                    .context("owned status observer failed")
                    .map_err(Arc::new)
            } else {
                Ok(())
            };
            // Publish the actual outcome before any subsequent cancelable await.
            *self.joined.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
            *slot = None;
            result
        };
        result.map_err(|error| anyhow::Error::new(ObservationFailure(error)))
    }
    async fn failed(&self, owner: &StatusOwner) -> Shared {
        self.failure
            .get_or_init(|| async {
                let cause = self.join().await.err().unwrap_or_else(|| {
                    anyhow::anyhow!("owned status observer ended without result")
                });
                if let Err(error) = owner.drain_scope().await {
                    return Err(Arc::new(
                        cause.context(format!("status scope drain also failed: {error:#}")),
                    ));
                }
                Err(Arc::new(cause))
            })
            .await
            .clone()
    }
}
impl StatusOwner {
    pub(super) fn accepting(&self) -> Result<()> {
        ensure!(!self.closed.load(Ordering::Acquire), "status owner closed");
        Ok(())
    }
    pub(super) fn admit_request(&self) -> Result<OperationPermit<'_>> {
        // A successful RMW linearizes this exact request against close without
        // holding a mutex across I/O. Accepted-before-close work may drain.
        self.closed
            .compare_exchange(false, false, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow::anyhow!("status owner closed"))?;
        Ok(OperationPermit {
            _owner: std::marker::PhantomData,
        })
    }
    fn namespace(scope: Option<CacheScope>) -> Option<airk_core::identity::Incarnation> {
        scope.map(|scope| match scope {
            CacheScope::Pinned(namespace) => namespace,
            CacheScope::Source(fence) | CacheScope::Registered(fence) => fence.namespace,
        })
    }
    fn project(scope: Option<CacheScope>) -> Option<ProjectBinding> {
        match scope {
            Some(CacheScope::Source(fence) | CacheScope::Registered(fence)) => Some(fence.project),
            _ => None,
        }
    }
    fn minimum(scope: Option<CacheScope>) -> u64 {
        match scope {
            Some(CacheScope::Source(fence) | CacheScope::Registered(fence)) => {
                fence.fencing_generation
            }
            _ => 0,
        }
    }
    fn scope_domain_same(left: Option<CacheScope>, right: Option<CacheScope>) -> bool {
        Self::namespace(left) == Self::namespace(right)
            && Self::project(left) == Self::project(right)
    }
    pub(super) fn request_ticket(
        &self,
        project: ProjectBinding,
        expires: Option<Instant>,
    ) -> RequestTicket {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        let authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
        RequestTicket {
            scope: *scope,
            authority: authority.generation,
            scalar_epoch: self.epoch.load(Ordering::Acquire),
            receipt: self.receipt(),
            project,
            expires,
        }
    }
    pub(super) fn ticket_scope_current(&self, ticket: &RequestTicket) -> bool {
        !self.closed.load(Ordering::Acquire)
            && ticket.expires.is_none_or(|until| Instant::now() < until)
            && *self.scope.lock().unwrap_or_else(|e| e.into_inner()) == ticket.scope
    }
    pub(super) fn ticket_current(&self, ticket: &RequestTicket) -> bool {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        let authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
        !self.closed.load(Ordering::Acquire)
            && ticket.expires.is_none_or(|until| Instant::now() < until)
            && *scope == ticket.scope
            && authority.generation == ticket.authority
    }
    pub(super) fn admit_ticket(&self, ticket: &RequestTicket) -> Result<OperationPermit<'_>> {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        let authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
        if *scope != ticket.scope
            || authority.generation != ticket.authority
            || ticket.expires.is_some_and(|until| Instant::now() >= until)
        {
            return Err(RetiredRequest.into());
        }
        self.closed
            .compare_exchange(false, false, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RetiredRequest)?;
        Ok(OperationPermit {
            _owner: std::marker::PhantomData,
        })
    }
    pub(super) fn transport_reusable(
        &self,
        ticket: &RequestTicket,
        bound: Option<CacheScope>,
        connection: &Connection,
    ) -> bool {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        let authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.load(Ordering::Acquire)
            || ticket.expires.is_some_and(|until| Instant::now() >= until)
            || *scope != ticket.scope
            || authority.generation != ticket.authority
            || !Self::scope_domain_same(bound, *scope)
            || connection.identity.project != ticket.project
            || !match *scope {
                Some(CacheScope::Source(fence) | CacheScope::Registered(fence)) => {
                    connection.reusable_for_source(fence, Instant::now())
                }
                _ => connection.reusable(Instant::now()),
            }
        {
            return false;
        }
        authority.known.is_none_or(|(namespace, identity)| {
            Some(namespace) == Self::namespace(*scope) && identity == connection.identity
        })
    }
    pub(super) fn authenticated_worker(
        &self,
        ticket: RequestTicket,
        identity: airk_core::service::WorkerIdentity,
    ) -> Result<RequestTicket> {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        let mut authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.load(Ordering::Acquire)
            || ticket.expires.is_some_and(|until| Instant::now() >= until)
            || *scope != ticket.scope
            || identity.project != ticket.project
            || Self::project(*scope).is_some_and(|project| project != identity.project)
            || identity.fencing_generation < Self::minimum(*scope)
        {
            return Err(RetiredRequest.into());
        }
        let Some(namespace) = Self::namespace(*scope) else {
            return Ok(ticket);
        };
        let changed = match authority.known {
            None => true,
            Some((known_namespace, known)) => {
                if known_namespace != namespace
                    || known.project != identity.project
                    || identity.fencing_generation < known.fencing_generation
                    || identity.fencing_generation == known.fencing_generation && identity != known
                {
                    return Err(RetiredRequest.into());
                }
                identity != known
            }
        };
        if changed {
            authority.generation = authority.generation.checked_add(1).ok_or(RetiredRequest)?;
            authority.known = Some((namespace, identity));
            self.invalidate();
            *self.receipt.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
        }
        if changed || authority.generation != ticket.authority {
            Ok(RequestTicket {
                scope: *scope,
                authority: authority.generation,
                scalar_epoch: self.epoch.load(Ordering::Acquire),
                receipt: Default::default(),
                project: ticket.project,
                expires: ticket.expires,
            })
        } else {
            Ok(ticket)
        }
    }
    fn retire_domain(&self, old: Option<CacheScope>, new: Option<CacheScope>) {
        if !Self::scope_domain_same(old, new) {
            let mut authority = self.authority.lock().unwrap_or_else(|e| e.into_inner());
            authority.known = None;
            if let Some(next) = authority.generation.checked_add(1) {
                authority.generation = next;
            } else {
                self.closed.store(true, Ordering::Release);
            }
        }
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.invalidate();
    }
    pub(crate) fn invalidate(&self) {
        if self
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .is_err()
        {
            self.closed.store(true, Ordering::Release);
        }
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub(crate) fn source_scope(&self, fence: SourceFence) {
        self.set_scope(CacheScope::Source(fence));
    }
    pub(crate) fn clear_scope(&self) {
        let mut scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        self.retire_domain(*scope, None);
        self.invalidate();
        *scope = None;
        *self.receipt.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
    }
    fn set_scope(&self, scope: CacheScope) {
        let mut current = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        if *current != Some(scope) {
            self.retire_domain(*current, Some(scope));
            self.invalidate();
            *current = Some(scope);
            *self.receipt.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
        }
    }
    #[cfg(test)]
    pub(super) fn cache_empty(&self) -> bool {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
    }
    #[cfg(test)]
    pub(super) async fn channel_empty(&self) -> bool {
        self.connection.lock().await.is_none()
    }
    #[cfg(test)]
    pub(super) fn expire_channel(&self) {
        self.connection
            .try_lock()
            .expect("idle test channel")
            .as_mut()
            .expect("retained channel")
            .1
            .authorized_until = Instant::now();
    }
    pub(crate) fn receipt(&self) -> airk_mcp::response::StatusReceipt {
        *self.receipt.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn observed_receipt(&self, value: airk_mcp::response::StatusReceipt) {
        *self.receipt.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }
    pub(super) fn receipt_fence(
        &self,
    ) -> (Option<CacheScope>, u64, airk_mcp::response::StatusReceipt) {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        (*scope, self.epoch.load(Ordering::Acquire), self.receipt())
    }
    pub(super) fn adopt_receipt(
        &self,
        fence: (Option<CacheScope>, u64, airk_mcp::response::StatusReceipt),
        receipt: airk_mcp::response::StatusReceipt,
    ) {
        let scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        if !self.closed.load(Ordering::Acquire)
            && *scope == fence.0
            && self.epoch.load(Ordering::Acquire) == fence.1
        {
            self.observed_receipt(receipt);
        }
    }
    // A lookup completion cannot relabel a scope/receipt installed while it waited.
    fn install_pinned_scope(
        &self,
        before: (Option<CacheScope>, u64, airk_mcp::response::StatusReceipt),
        new: CacheScope,
    ) -> bool {
        let mut scope = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        if *scope == Some(new) {
            return true;
        }
        if *scope != before.0 || self.epoch.load(Ordering::Acquire) != before.1 {
            return false;
        }
        self.retire_domain(*scope, Some(new));
        self.invalidate();
        *scope = Some(new);
        *self.receipt.lock().unwrap_or_else(|e| e.into_inner()) = Default::default();
        true
    }
    pub(super) async fn bind_initial_pinned_scope(
        &self,
        root: PathBuf,
        project: ProjectBinding,
    ) -> Result<()> {
        let before = self.receipt_fence();
        if before.0.is_none()
            && let Some(scope) = self.pinned_scope(root, project).await?
        {
            self.install_pinned_scope(before, scope);
        }
        Ok(())
    }
    async fn pinned_scope(
        &self,
        root: PathBuf,
        project: ProjectBinding,
    ) -> Result<Option<CacheScope>> {
        let mut slot = self.scope_worker.lock().await;
        if let Some(previous) = slot.as_mut() {
            let result = previous.await;
            *slot = None;
            drop(result?);
        }
        let _scope_request = self.admit_request()?;
        *slot = Some(tokio::task::spawn_blocking(move || {
            status_scope(&root, project)
        }));
        let result = slot
            .as_mut()
            .expect("owned scope handle")
            .await
            .context("status scope worker failed");
        *slot = None;
        result?
    }
    async fn drain_scope(&self) -> Result<()> {
        let mut slot = self.scope_worker.lock().await;
        if let Some(worker) = slot.as_mut() {
            let result = worker.await;
            *slot = None;
            drop(result?);
        }
        Ok(())
    }
    fn reply(
        &self,
        snapshot: &Snapshot,
        source_expires: Option<Instant>,
        hit: bool,
    ) -> Result<serde_json::Value> {
        ensure!(!self.closed.load(Ordering::Acquire), "status owner closed");
        let current = self.scope.lock().unwrap_or_else(|e| e.into_inner());
        if *current != Some(snapshot.scope)
            || self.epoch.load(Ordering::Acquire) != snapshot.epoch
            || source_expires.is_some_and(|until| Instant::now() >= until)
            || snapshot.cached.as_ref().is_some_and(|cached| {
                Instant::now().saturating_duration_since(cached.captured) >= Duration::from_secs(5)
            })
        {
            return Ok(if snapshot.submitted && !hit {
                superseded()
            } else {
                source_scope_superseded()
            });
        }
        // Only a live caller observing a completed reply advances this frontend
        // receipt. Background completion after caller timeout never acknowledges it.
        self.observed_receipt(snapshot.receipt);
        Ok(snapshot
            .cached
            .as_ref()
            .and_then(|cached| cached.reply(snapshot.scope, Instant::now(), hit))
            .unwrap_or_else(|| snapshot.result.clone()))
    }
    pub(crate) async fn status(
        self: &Arc<Self>,
        root: PathBuf,
        project: ProjectBinding,
        args: serde_json::Map<String, serde_json::Value>,
        source: Option<SourceFence>,
        expires: Option<Instant>,
        client: airk_core::usage::ClientFamily,
    ) -> Result<serde_json::Value> {
        ensure!(!self.closed.load(Ordering::Acquire), "status owner closed");
        if let Some(fence) = source
            && *self.scope.lock().unwrap_or_else(|e| e.into_inner())
                != Some(CacheScope::Source(fence))
        {
            return Ok(source_scope_superseded());
        }
        let original_ticket = self.request_ticket(project, expires);
        let key = Key {
            project,
            source,
            expires,
            args: serde_json::Value::Object(args.clone()),
        };
        if let Some(fence) = source {
            let cached = self.cache.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(cached) = cached.filter(|cached| {
                cached.scope == CacheScope::Source(fence)
                    && cached.options == key.args
                    && cached.cached.as_ref().is_some_and(|snapshot| {
                        snapshot.scope == cached.scope
                            && Instant::now().saturating_duration_since(snapshot.captured)
                                < Duration::from_secs(5)
                    })
            }) {
                return self.reply(&cached, expires, true);
            }
        }
        self.observe(key, expires, move |owner| async move {
            owner
                .materialize(root, args, source, client, original_ticket)
                .await
        })
        .await
    }
    // Shared owner path. The private producer seam allows real IPC ownership
    // fixtures without constructing a second service broker in each unit test.
    async fn observe<F, Fut>(
        self: &Arc<Self>,
        key: Key,
        expires: Option<Instant>,
        produce: F,
    ) -> Result<serde_json::Value>
    where
        F: FnOnce(Arc<Self>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Snapshot>> + Send + 'static,
    {
        let attempt = loop {
            let mut state = self.state.lock().await;
            ensure!(!self.closed.load(Ordering::Acquire), "status owner closed");
            match &*state {
                State::Closed => anyhow::bail!("status owner closed"),
                State::Running(attempt) if attempt.result.borrow().is_some() => {
                    let joined = attempt.join().await;
                    *state = State::Idle;
                    joined?;
                    continue;
                }
                State::Running(attempt) => {
                    if attempt.key != key {
                        return Ok(busy());
                    }
                    break attempt.clone();
                }
                State::Idle => {
                    let (sender, result) = tokio::sync::watch::channel(None);
                    let attempt = Arc::new(Attempt {
                        key: key.clone(),
                        result,
                        task: Default::default(),
                        joined: Default::default(),
                        failure: Default::default(),
                    });
                    let owner = self.clone();
                    let keep = attempt.clone();
                    let task = tokio::spawn(async move {
                        let _keep = keep;
                        let result = produce(owner).await.map(Arc::new).map_err(Arc::new);
                        let _ = sender.send_replace(Some(result));
                    });
                    *attempt.task.try_lock().expect("new status task slot") = Some(task);
                    *state = State::Running(attempt.clone());
                    break attempt;
                }
            }
        };
        let mut ticket = attempt.result.clone();
        let result = loop {
            if let Some(result) = ticket.borrow_and_update().clone() {
                break result;
            }
            if ticket.changed().await.is_err() {
                break attempt.failed(self).await;
            }
        };
        {
            let mut state = self.state.lock().await;
            if matches!(&*state,State::Running(current) if Arc::ptr_eq(current,&attempt)) {
                let joined = attempt.join().await;
                *state = State::Idle;
                // The shared failure below retains the actual cohort cause.
                // A successful published outcome still cannot hide a driver panic.
                if result.is_ok() {
                    joined?;
                }
            }
        }
        match result {
            Ok(snapshot) => self.reply(&snapshot, expires, snapshot.reused),
            Err(error)
                if error.is::<RetiredRequest>()
                    || airk_store::service::registry::source_scope_retryable(&error) =>
            {
                Ok(source_scope_superseded())
            }
            Err(error) => {
                Err(anyhow::Error::new(ObservationFailure(error))
                    .context("status observation failed"))
            }
        }
    }
    async fn materialize(
        self: &Arc<Self>,
        root: PathBuf,
        args: serde_json::Map<String, serde_json::Value>,
        source: Option<SourceFence>,
        client: airk_core::usage::ClientFamily,
        original_ticket: RequestTicket,
    ) -> Result<Snapshot> {
        let project = original_ticket.project;
        let expires = original_ticket.expires;
        let request_args = serde_json::Value::Object(args.clone());
        let before = (
            original_ticket.scope,
            original_ticket.scalar_epoch,
            original_ticket.receipt,
        );
        let mut ticket = original_ticket;
        let scope = match source {
            Some(fence) => Some(CacheScope::Source(fence)),
            None => self.pinned_scope(root.clone(), project).await?,
        };
        let Some(scope) = scope else {
            return Err(anyhow::anyhow!("registered status scope unavailable"));
        };
        if source.is_none() && !self.install_pinned_scope(before, scope) {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch: self.epoch.load(Ordering::Acquire),
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: Default::default(),
            });
        }
        if *self.scope.lock().unwrap_or_else(|e| e.into_inner()) != Some(scope) {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch: self.epoch.load(Ordering::Acquire),
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: self.receipt(),
            });
        }
        if source.is_none() && ticket.scope != Some(scope) {
            ticket = self.request_ticket(project, expires);
            ticket.receipt = Default::default(); // freshly authenticated metadata, no unrelated acknowledgment
        }
        if ticket.scope != Some(scope) {
            return Err(RetiredRequest.into());
        }
        let mut epoch = ticket.scalar_epoch;
        ensure!(!self.closed.load(Ordering::Acquire), "status owner closed");
        if expires.is_some_and(|until| Instant::now() >= until) {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: self.receipt(),
            });
        }
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|cached| {
                cached.scope == scope
                    && cached.options == request_args
                    && cached.epoch == epoch
                    && cached.cached.as_ref().is_some_and(|value| {
                        value.scope == scope
                            && Instant::now().saturating_duration_since(value.captured)
                                < Duration::from_secs(5)
                    })
            })
        {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: cached.result.clone(),
                cached: cached.cached.as_ref().map(|value| CachedStatus {
                    scope: value.scope,
                    captured: value.captured,
                    observed_unix_ms: value.observed_unix_ms,
                    body: value.body.clone(),
                }),
                submitted: false,
                reused: true,
                receipt: cached.receipt,
            });
        }
        let mut channel = self.connection.lock().await;
        if expires.is_some_and(|until| Instant::now() >= until)
            || self.epoch.load(Ordering::Acquire) != epoch
            || *self.scope.lock().unwrap_or_else(|e| e.into_inner()) != Some(scope)
        {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: self.receipt(),
            });
        }
        let retained = channel.take().filter(|(bound, connection)| {
            self.transport_reusable(&ticket, Some(*bound), connection)
        });
        let mut connection = match retained {
            Some((_, connection)) => connection,
            None => {
                match Connection::connect_once_checked(
                    &root,
                    project,
                    client,
                    Some((self, &ticket)),
                )
                .await
                {
                    Ok(connection) => connection,
                    Err(error) => {
                        if error.is::<RetiredRequest>() || !self.ticket_current(&ticket) {
                            return Err(RetiredRequest.into());
                        }
                        self.accepting()?;
                        let _ = error; // no evidence query was submitted
                        let result = airk_mcp::unified::success(
                            serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown",
                    "index_state":"unavailable","publication":null,"files":null,"chunks":null,
                    "index_error":{"code":"index_worker_unavailable","message":"No index query was submitted. Call diagnostics for service and queued-job diagnosis."}}),
                        );
                        return Ok(Snapshot {
                            options: request_args,
                            scope,
                            epoch,
                            result,
                            cached: None,
                            submitted: false,
                            reused: false,
                            receipt: self.receipt(),
                        });
                    }
                }
            }
        };
        if expires.is_some_and(|until| Instant::now() >= until) {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: self.receipt(),
            });
        }
        self.accepting()?;
        ticket = self.authenticated_worker(ticket, connection.identity)?;
        epoch = ticket.scalar_epoch;
        if self.epoch.load(Ordering::Acquire) != epoch
            || *self.scope.lock().unwrap_or_else(|e| e.into_inner()) != Some(scope)
        {
            if self.transport_reusable(&ticket, Some(scope), &connection) {
                *channel = Some((scope, connection));
            }
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: ticket.receipt,
            });
        }
        connection.restore_status_receipt(ticket.receipt);
        let _request = self.admit_ticket(&ticket)?;
        let result = connection.call("airk_status".into(), args).await?;
        let receipt = connection.status_receipt();
        if self.transport_reusable(&ticket, Some(scope), &connection) {
            *channel = Some((scope, connection));
        }
        let snapshot = Snapshot {
            options: request_args.clone(),
            scope,
            epoch,
            cached: CachedStatus::new(scope, &result),
            result,
            submitted: true,
            reused: false,
            receipt,
        };
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if !self.closed.load(Ordering::Acquire)
            && self.epoch.load(Ordering::Acquire) == epoch
            && expires.is_none_or(|until| Instant::now() < until)
            && snapshot.cached.is_some()
        {
            // Immutable logical observation only: no stream/read/transaction retained.
            let saved = Snapshot {
                options: request_args,
                scope,
                epoch,
                result: serde_json::Value::Null,
                cached: snapshot.cached.as_ref().map(|value| CachedStatus {
                    scope: value.scope,
                    captured: value.captured,
                    observed_unix_ms: value.observed_unix_ms,
                    body: value.body.clone(),
                }),
                submitted: snapshot.submitted,
                reused: false,
                receipt: snapshot.receipt,
            };
            *cache = Some(Arc::new(saved));
        }
        Ok(snapshot)
    }
    #[cfg(test)]
    pub(super) fn seed(&self, cached: CachedStatus) {
        self.set_scope(cached.scope);
        let epoch = self.epoch.load(Ordering::Acquire);
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(Snapshot {
            options: serde_json::json!({}),
            scope: cached.scope,
            epoch,
            result: serde_json::Value::Null,
            cached: Some(cached),
            submitted: false,
            reused: false,
            receipt: Default::default(),
        }));
    }
    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.close();
        let mut state = self.state.lock().await;
        // Drain both owners even when the observer panics. A canceled drain keeps
        // the state and each actual handle in its slot for the next drain.
        let observed = if let State::Running(attempt) = &*state {
            attempt.join().await
        } else {
            Ok(())
        };
        let scoped = self.drain_scope().await;
        self.connection.lock().await.take();
        *state = State::Closed;
        match (observed, scoped) {
            (Err(error), Err(scope)) => {
                Err(error.context(format!("status scope drain also failed: {scope:#}")))
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        }
    }
}
fn busy() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"direct_query_admission","reason":"busy","query_submitted":false,
        "retryable":true,"retry_after_ms":25,"fresh_brokerage_required":false}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}
fn superseded() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"status_observation_superseded","query_submitted":true,
        "retryable":true,"fresh_brokerage_required":true}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[tokio::test]
    async fn source_and_pinned_cached_status_report_actual_reuse_and_original_age() -> Result<()> {
        use airk_core::service::ProjectRegistration;
        use airk_store::service::registry::ServiceRegistry;
        let temporary = tempfile::tempdir()?;
        let namespace = airk_core::identity::Incarnation::from_bytes([95; 16]);
        let mut registry = ServiceRegistry::open(temporary.path(), namespace)?;
        let project = ProjectRegistration::new(
            "/cache-source".into(),
            "/cache-data".into(),
            "fixture".into(),
        )?;
        registry.register(&project)?;
        let pinned =
            status_scope(temporary.path(), project.binding)?.context("pinned scope missing")?;
        let fence = match pinned {
            CacheScope::Registered(fence) => fence,
            _ => anyhow::bail!("registered pinned scope required"),
        };
        for source in [false, true] {
            let owner = Arc::new(StatusOwner::default());
            let scope = if source {
                CacheScope::Source(fence)
            } else {
                pinned
            };
            let result = airk_mcp::unified::success(
                serde_json::json!({"index_state":"available","publication":{"token":"cached-original"}}),
            );
            let mut cached =
                CachedStatus::new(scope, &result).context("cache construction failed")?;
            cached.captured = Instant::now() - Duration::from_secs(1);
            cached.observed_unix_ms = 123;
            let captured = cached.captured;
            owner.seed(cached);
            // No native control listener exists. Only actual cached reuse can
            // produce this available reply; pinned still observes real registry.
            let reply = owner
                .status(
                    temporary.path().into(),
                    project.binding,
                    Default::default(),
                    source.then_some(fence),
                    None,
                    airk_core::usage::ClientFamily::Unknown,
                )
                .await?;
            let body: serde_json::Value = serde_json::from_str(
                reply["content"][0]["text"]
                    .as_str()
                    .context("cache reply missing")?,
            )?;
            assert_eq!(body["cached"], true);
            assert_eq!(body["observed_unix_ms"], 123);
            assert!(body["cache_age_ms"].as_u64().context("cache age missing")? >= 1000);
            assert_eq!(body["publication"]["token"], "cached-original");
            assert_eq!(
                owner
                    .cache
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .cached
                    .as_ref()
                    .unwrap()
                    .captured,
                captured,
                "reuse must not rejuvenate its original capture time"
            );
            owner.shutdown().await?;
        }
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn coalesced_ipc_status_survives_all_waiters_then_eof_drain() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let endpoint = local_ipc::endpoint(&root, "owned-status-test")?;
        let mut listener = local_ipc::Listener::bind(&endpoint)?;
        let (sent, submitted) = tokio::sync::oneshot::channel();
        let (release, finish) = tokio::sync::oneshot::channel();
        let receipt: airk_mcp::response::StatusReceipt =
            serde_json::from_value(serde_json::json!({"freshness":"cd".repeat(32)}))?;
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let query: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("missing status query")?;
            ensure!(query.tool == "airk_status", "unexpected query");
            let _ = sent.send(());
            finish.await?;
            local_ipc::write_message(&mut stream,&protocol::QueryResult{
                result:airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":"same"}})),
                status_receipt:receipt,rebind:false},Duration::from_secs(5)).await?;
            Ok::<_, anyhow::Error>(())
        });
        let project = "ab".repeat(32).parse()?;
        let stream = local_ipc::connect(&endpoint).await?;
        let now = Instant::now();
        let mut connection = Connection {
            stream,
            identity: airk_core::service::WorkerIdentity {
                service: airk_core::identity::Incarnation::from_bytes([1; 16]),
                worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
                project,
                fencing_generation: 1,
            },
            authorized_until: now + Duration::from_secs(55),
            last_finished: now,
            healthy: true,
            status_receipt: Default::default(),
        };
        let owner = Arc::new(StatusOwner::default());
        let fence = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([94; 16]),
            revision: 1,
            project,
            fencing_generation: 1,
        };
        owner.source_scope(fence);
        let scope = CacheScope::Source(fence);
        let epoch = owner.epoch.load(Ordering::Acquire);
        let key = Key {
            project,
            source: Some(fence),
            expires: None,
            args: serde_json::json!({}),
        };
        {
            let first = owner.observe(key.clone(), None, move |owned| async move {
                let _accepted = owned.admit_request()?;
                let result = connection
                    .call("airk_status".into(), Default::default())
                    .await?;
                Ok(Snapshot {
                    options: serde_json::json!({}),
                    scope,
                    epoch,
                    result,
                    cached: None,
                    submitted: true,
                    reused: false,
                    receipt: connection.status_receipt(),
                })
            });
            tokio::pin!(first);
            tokio::select! {result=&mut first=>anyhow::bail!("completed before release: {result:?}"),result=submitted=>{result?;}}
            let mut other = Vec::new();
            for _ in 0..3 {
                let mut request = Box::pin(owner.observe(key.clone(), None, |_| async {
                    panic!("coalesced producer invoked")
                }));
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(request.as_mut().poll(cx)))
                        .await
                        .is_pending()
                );
                other.push(request);
            }
            drop(other);
            // Drop the final waiter before EOF: the actual submitted exchange stays owned.
        } // drops the actual first future, not merely its pinned reference
        owner.close();
        {
            let drain = owner.shutdown();
            tokio::pin!(drain);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(drain.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
        }
        assert!(matches!(*owner.state.lock().await, State::Running(_)));
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("peer disappeared"))?;
        owner.shutdown().await?;
        peer.await??;
        assert_eq!(owner.receipt(), Default::default());
        assert!(matches!(*owner.state.lock().await, State::Closed));
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_shutdown_retains_actual_registry_worker_for_second_drain() -> Result<()> {
        let owner = Arc::new(StatusOwner::default());
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        *owner.scope_worker.lock().await = Some(tokio::task::spawn_blocking(move || {
            let _ = entered.send(());
            wait.recv().context("release registry fixture")?;
            Ok(None)
        }));
        started.await?;
        {
            let shutdown = owner.shutdown();
            tokio::pin!(shutdown);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(shutdown.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
        }
        assert!(owner.closed.load(Ordering::Acquire));
        assert!(owner.scope_worker.lock().await.is_some());
        assert!(owner.admit_request().is_err());
        release.send(())?;
        owner.shutdown().await?;
        assert!(owner.scope_worker.lock().await.is_none());
        assert!(matches!(*owner.state.lock().await, State::Closed));
        Ok(())
    }
    #[tokio::test]
    async fn panic_fans_out_then_next_owned_observation_can_succeed() -> Result<()> {
        let project = "ab".repeat(32).parse()?;
        let owner = Arc::new(StatusOwner::default());
        let fence = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([95; 16]),
            revision: 1,
            project,
            fencing_generation: 1,
        };
        owner.source_scope(fence);
        let key = Key {
            project,
            source: Some(fence),
            expires: None,
            args: serde_json::json!({}),
        };
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        {
            let first = owner.observe(key.clone(), None, |_| async move {
                let _ = entered.send(());
                let _ = wait.await;
                panic!("cohort-status-panic")
            });
            tokio::pin!(first);
            tokio::select! {result=&mut first=>anyhow::bail!("premature result: {result:?}"),result=started=>{result?;}}
            let second = owner.observe(key.clone(), None, |_| async {
                panic!("duplicate producer")
            });
            tokio::pin!(second);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(second.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            release
                .send(())
                .map_err(|_| anyhow::anyhow!("driver disappeared"))?;
            let (first, second) = tokio::join!(first, second);
            for error in [first.unwrap_err(), second.unwrap_err()] {
                assert!(format!("{error:#}").contains("cohort-status-panic"));
            }
        }
        assert!(matches!(*owner.state.lock().await, State::Idle));
        let epoch = owner.epoch.load(Ordering::Acquire);
        let result = owner
            .observe(key, None, move |_| async move {
                Ok(Snapshot {
                    options: serde_json::json!({}),
                    scope: CacheScope::Source(fence),
                    epoch,
                    result: serde_json::json!({"recovered":true}),
                    cached: None,
                    submitted: true,
                    reused: false,
                    receipt: Default::default(),
                })
            })
            .await?;
        assert_eq!(result["recovered"], true);
        owner.shutdown().await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_failure_drain_retains_original_driver_panic() -> Result<()> {
        let owner = StatusOwner::default();
        let (sender, result) = tokio::sync::watch::channel(None);
        drop(sender);
        let attempt = Attempt {
            key: Key {
                project: "ab".repeat(32).parse()?,
                source: None,
                expires: None,
                args: serde_json::json!({}),
            },
            result,
            task: tokio::sync::Mutex::new(Some(tokio::spawn(async {
                panic!("original-status-driver-panic")
            }))),
            joined: Default::default(),
            failure: Default::default(),
        };
        let original = attempt.join().await.unwrap_err();
        assert!(format!("{original:#}").contains("original-status-driver-panic"));
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        *owner.scope_worker.lock().await = Some(tokio::task::spawn_blocking(move || {
            let _ = entered.send(());
            wait.recv()?;
            Ok(None)
        }));
        started.await?;
        {
            let failed = attempt.failed(&owner);
            tokio::pin!(failed);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(failed.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
        }
        assert!(owner.scope_worker.lock().await.is_some());
        release.send(())?;
        let error = match attempt.failed(&owner).await {
            Err(error) => error,
            Ok(_) => panic!("panic became success"),
        };
        assert!(format!("{error:#}").contains("original-status-driver-panic"));
        assert!(!format!("{error:#}").contains("ended without result"));
        assert!(owner.scope_worker.lock().await.is_none());
        Ok(())
    }
    #[test]
    fn exact_request_admission_linearizes_before_close_without_reusable_authority() {
        let owner = StatusOwner::default();
        let _accepted = owner.admit_request().unwrap();
        owner.close();
        assert!(owner.admit_request().is_err());
    }
    #[test]
    fn late_old_scope_receipt_cannot_acknowledge_replacement() -> Result<()> {
        let owner = StatusOwner::default();
        let old = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([92; 16]),
            revision: 7,
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        owner.source_scope(old);
        let captured = owner.receipt_fence();
        let receipt: airk_mcp::response::StatusReceipt =
            serde_json::from_value(serde_json::json!({"freshness":"cd".repeat(32)}))?;
        owner.source_scope(SourceFence { revision: 8, ..old });
        owner.adopt_receipt(captured, receipt);
        assert_eq!(owner.receipt(), Default::default());
        let current = owner.receipt_fence();
        owner.adopt_receipt(current, receipt);
        assert_eq!(owner.receipt(), receipt);
        Ok(())
    }
    #[test]
    fn same_head_invalidation_rejects_old_snapshot_without_receipt_ack() -> Result<()> {
        let owner = StatusOwner::default();
        let fence = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([93; 16]),
            revision: 7,
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        owner.source_scope(fence);
        let snapshot = Snapshot {
            options: serde_json::json!({}),
            scope: CacheScope::Source(fence),
            epoch: owner.epoch.load(Ordering::Acquire),
            result: serde_json::Value::Null,
            cached: None,
            submitted: true,
            reused: false,
            receipt: Default::default(),
        };
        owner.invalidate(); // model/work/pause may change while publication stays identical
        let reply = owner.reply(&snapshot, None, false)?;
        let detail: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(detail["error"]["code"], "status_observation_superseded");
        assert_eq!(detail["error"]["query_submitted"], true);
        Ok(())
    }
}
