// SPDX-License-Identifier: Apache-2.0
//! Native catalog ports inside the existing Supervisor actor; no second owner.
#![allow(dead_code)]
use super::*;
use crate::{
    catalog_admission::{FenceTicket, InstallTicket, IssueTicket, NativeCatalogAdmission},
    catalog_authority::NativeCatalogAuthoritySelection,
    catalog_coordinator::CatalogControl,
};
use airk_core::catalog_management::CatalogOperation;
use airk_core::service::{ConnectionGrant, MAX_GRANT_TTL_MS, PROTOCOL_VERSION, ScopedReadRef};
use airk_store::service::registry::catalog::operations::FenceTarget;
use std::{
    sync::Weak,
    time::{SystemTime, UNIX_EPOCH},
};

/// A drained in-process owner has not been observed to exit yet; the caller
/// asks again. Never a reap receipt and never a release of its resources.
#[derive(Debug)]
struct WorkerDrainPending;
impl std::fmt::Display for WorkerDrainPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native worker drain pending: the owner is stopping")
    }
}
impl std::error::Error for WorkerDrainPending {}
/// The owner's stop grace and drain period, plus actor-turn margin.
const WORKER_DRAIN_BUDGET: Duration = OWNER_STOP_GRACE
    .saturating_add(IN_PROCESS_DRAIN)
    .saturating_add(Duration::from_secs(5));

pub(super) enum Operation {
    Attach,
    AdministrativeRead {
        project: ProjectBinding,
    },
    ResolveSourceChange {
        project: ProjectBinding,
        begin_operation: Incarnation,
    },
    Connect {
        selection: ConnectionSelection,
        lifetime: Duration,
    },
    Revoke {
        ticket: FenceTicket,
        target: FenceTarget,
    },
    Reaped {
        worker: WorkerIdentity,
    },
    CaptureWorkers {
        ticket: FenceTicket,
        backing: ProjectBinding,
    },
    DrainWorker {
        ticket: FenceTicket,
        worker: WorkerIdentity,
    },
    FinishWorkerDrains {
        receipt: Box<airk_core::catalog_fences::FencedReceipt>,
    },
}
pub(crate) struct NativeLegacyAuthoritySelection {
    pub namespace: Incarnation,
    pub host: airk_core::identity::Sha256Digest,
    pub catalog_revision: airk_core::index_catalog::CatalogRevision,
    pub binding: ProjectBinding,
}
pub(super) enum ConnectionSelection {
    Reviewed(NativeCatalogAuthoritySelection),
    Legacy(NativeLegacyAuthoritySelection),
}
pub(super) enum Response {
    Attached,
    AdministrativeRead(Box<NativeAdministrativeRead>),
    SourceChangeResolved(Box<airk_core::catalog_source_change::SourceChangeObservation>),
    Connection(Box<ScopedConnectionDelivery>),
    Revoked,
    Reaped(bool),
    Captured(Box<crate::catalog_admission::FencePlan>),
    Drained,
}
/// Internal one-use credential owner. There is no credential/response getter or
/// serde implementation: Runtime can only execute the canonical native read.
pub(crate) struct NativeAdministrativeRead {
    root: PathBuf,
    gate: NativeCatalogAdmission,
    issue: IssueTicket,
    role: String,
    grant: ConnectionGrant,
}
impl NativeAdministrativeRead {
    pub(crate) async fn connect(
        mut self,
        client: airk_core::usage::ClientFamily,
    ) -> Result<NativeAdministrativeConnection> {
        self.gate.check_issue(self.issue)?;
        self.gate
            .require_worker_start(self.grant.identity.project)?;
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            local_ipc::connect(&local_ipc::endpoint(&self.root, &self.role)?),
        )
        .await??;
        self.grant.client = client;
        local_ipc::write_message(&mut stream, &self.grant, Duration::from_secs(5)).await?;
        let accepted: protocol::Accepted =
            local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("native administrative grant refused")?;
        ensure!(
            accepted.identity == self.grant.identity,
            "native administrative worker changed"
        );
        self.gate.check_issue(self.issue)?;
        let clock = self
            .gate
            .catalog_clock()
            .context("native administrative clock unavailable")?;
        self.gate
            .with_administrative_worker(clock.namespace, self.grant.identity, || ())?;
        Ok(NativeAdministrativeConnection {
            gate: self.gate,
            key: AdministrativeKey {
                namespace: clock.namespace,
                worker: self.grant.identity,
                caller: self.grant.caller,
                client,
            },
            stream,
            authorized_until: Instant::now() + Duration::from_secs(55),
            last_finished: Instant::now(),
            healthy: true,
            status_receipt: Default::default(),
        })
    }
}
/// The purpose is enforced by this distinct native type, never a wire field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdministrativeKey {
    pub namespace: Incarnation,
    pub worker: WorkerIdentity,
    pub caller: Incarnation,
    pub client: airk_core::usage::ClientFamily,
}
pub(crate) struct NativeAdministrativeConnection {
    gate: NativeCatalogAdmission,
    key: AdministrativeKey,
    stream: local_ipc::Stream,
    authorized_until: Instant,
    last_finished: Instant,
    healthy: bool,
    status_receipt: airk_mcp::response::StatusReceipt,
}
impl NativeAdministrativeConnection {
    pub(crate) fn key(&self) -> AdministrativeKey {
        self.key
    }
    pub(crate) fn reusable(&self, key: AdministrativeKey) -> bool {
        self.key == key
            && self.healthy
            && Instant::now() < self.authorized_until
            && self.last_finished.elapsed() < Duration::from_secs(25)
            && self
                .gate
                .with_administrative_worker(key.namespace, key.worker, || ())
                .is_ok()
    }
    pub(crate) async fn read(
        &mut self,
        request: airk_core::native_reads::NativeRead,
    ) -> Result<serde_json::Value> {
        ensure!(
            self.reusable(self.key),
            "native_administrative_connection_rebind"
        );
        // A canceled reader must never put a stream with an unread reply back
        // into the cache. Worker admission remains owned until actual finish.
        self.healthy = false;
        let query = protocol::Query {
            read_attempt: None,
            frontend: None,
            tool: String::new(),
            arguments: Default::default(),
            native: Some(request),
            status_receipt: self.status_receipt,
        };
        self.write(&query).await?;
        let result: protocol::QueryResult =
            local_ipc::read_message(&mut self.stream, Duration::from_secs(120))
                .await?
                .context("native administrative read unavailable")?;
        self.status_receipt = result.status_receipt;
        self.last_finished = Instant::now();
        self.healthy = !result.rebind;
        Ok(result.result)
    }
    async fn write(&mut self, query: &protocol::Query) -> Result<()> {
        let mut frame = Vec::new();
        service_ipc::write_frame_limited(&mut frame, query, MAX_FRAME_BYTES)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            let mut offset = 0;
            while offset < frame.len() {
                let count = std::future::poll_fn(|cx| {
                    if Instant::now() >= deadline {
                        return std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "native administrative write deadline",
                        )));
                    }
                    self.gate
                        .with_administrative_worker(self.key.namespace, self.key.worker, || {
                            if Instant::now() >= deadline {
                                return std::task::Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    "native administrative write deadline",
                                )));
                            }
                            tokio::io::AsyncWrite::poll_write(
                                std::pin::Pin::new(&mut self.stream),
                                cx,
                                &frame[offset..],
                            )
                        })
                        .unwrap_or_else(|_| {
                            std::task::Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                "native_administrative_connection_rebind",
                            )))
                        })
                })
                .await?;
                ensure!(count > 0, "native administrative stream closed");
                offset += count;
            }
            std::future::poll_fn(|cx| {
                if Instant::now() >= deadline {
                    return std::task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "native administrative flush deadline",
                    )));
                }
                self.gate
                    .with_administrative_worker(self.key.namespace, self.key.worker, || {
                        if Instant::now() >= deadline {
                            return std::task::Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "native administrative flush deadline",
                            )));
                        }
                        tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.stream), cx)
                    })
                    .unwrap_or_else(|_| {
                        std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "native_administrative_connection_rebind",
                        )))
                    })
            })
            .await?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("native administrative write deadline")??;
        Ok(())
    }
}

/// Owns protected connection bytes until their actual delivery boundary. This
/// has no raw response accessor, Clone, Debug or Serialize implementation.
pub(crate) struct ScopedConnectionDelivery {
    gate: NativeCatalogAdmission,
    issue: IssueTicket,
    worker: WorkerIdentity,
    scope: ScopedReadRef,
    response: ControlResponse,
}
impl ScopedConnectionDelivery {
    /// Guard the metadata actor's nonblocking handoff too; the receiver still
    /// cannot expose bytes except through the guarded final writer below.
    pub(crate) fn handoff_native(self, reply: tokio::sync::oneshot::Sender<Result<Self>>) {
        let gate = self.gate.clone();
        let mut sender = Some(reply);
        let checked = gate.with_scope_issue(self.issue, self.worker, self.scope, || {
            // Discard an undelivered owner under this same guard. This type has
            // no custom Drop and cannot recursively acquire the gate. Accepted
            // installation/resource effects remain owned by the worker.
            let _ = sender.take().expect("single native handoff").send(Ok(self));
        });
        if let Err(error) = checked
            && let Some(sender) = sender
        {
            let _ = sender.send(Err(error));
        }
    }
    fn handoff(self, reply: &SyncSender<Result<Response>>) {
        let gate = self.gate.clone();
        let checked = gate.with_scope_issue(self.issue, self.worker, self.scope, || {
            // The real one-slot handoff is nonblocking and stays inside the
            // guard. A closed receiver never rolls back accepted native work.
            let _ = reply.try_send(Ok(Response::Connection(Box::new(self))));
        });
        if let Err(error) = checked {
            let _ = reply.try_send(Err(error));
        }
    }
    /// Runtime must consume this object directly at its final IPC write. It
    /// must close a partial exchange on error, without a legacy response fallback.
    pub(crate) async fn deliver<S: tokio::io::AsyncWrite + Unpin>(
        self,
        stream: &mut S,
        budget: Duration,
    ) -> Result<()> {
        let deadline = Instant::now()
            .checked_add(budget)
            .context("native scoped delivery deadline overflow")?;
        let mut frame = Vec::new();
        service_ipc::write_frame_limited(&mut frame, &self.response, MAX_FRAME_BYTES)?;
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            let mut offset = 0;
            while offset < frame.len() {
                let count = std::future::poll_fn(|cx| {
                    self.poll_guarded(deadline, || {
                        tokio::io::AsyncWrite::poll_write(
                            std::pin::Pin::new(&mut *stream),
                            cx,
                            &frame[offset..],
                        )
                    })
                })
                .await?;
                ensure!(count > 0, "native scoped delivery writer closed");
                offset += count;
            }
            std::future::poll_fn(|cx| {
                self.poll_guarded(deadline, || {
                    tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut *stream), cx)
                })
            })
            .await?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("native scoped delivery deadline exceeded")??;
        Ok(())
    }
    fn poll_guarded<T>(
        &self,
        deadline: Instant,
        poll: impl FnOnce() -> std::task::Poll<std::io::Result<T>>,
    ) -> std::task::Poll<std::io::Result<T>> {
        if Instant::now() >= deadline {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "native scoped delivery deadline exceeded",
            )));
        }
        match self
            .gate
            .with_scope_issue(self.issue, self.worker, self.scope, || {
                if Instant::now() >= deadline {
                    std::task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "native scoped delivery deadline exceeded",
                    )))
                } else {
                    poll()
                }
            }) {
            Ok(result) => result,
            Err(error) => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                error,
            ))),
        }
    }
}

/// Used by the actual Supervisor reply site, rather than guarding construction
/// of a response that could cross the queue after admission had already closed.
pub(super) fn send_reply(reply: &SyncSender<Result<Response>>, result: Result<Response>) {
    match result {
        Ok(Response::Connection(delivery)) => (*delivery).handoff(reply),
        other => {
            let _ = reply.try_send(other);
        }
    }
}

/// Weak queue ownership lets Supervisor stop/drain even if a caller retains a port.
#[derive(Clone)]
pub(crate) struct CatalogControlHandle {
    sender: Weak<SyncSender<Envelope>>,
    gate: NativeCatalogAdmission,
    source: Option<crate::source_supervisor::SourceWake>,
}
#[derive(Debug)]
struct NativePredecessorsPending;
impl std::fmt::Display for NativePredecessorsPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native_predecessors_pending")
    }
}
impl std::error::Error for NativePredecessorsPending {}
impl Supervisor {
    pub(crate) fn catalog_control(
        &self,
        gate: NativeCatalogAdmission,
    ) -> Result<CatalogControlHandle> {
        let handle = CatalogControlHandle {
            sender: Arc::downgrade(self.sender.as_ref().context("supervisor closed")?),
            gate,
            source: None,
        };
        handle.attach_before(Instant::now() + Duration::from_secs(30))?;
        Ok(handle)
    }
}
impl CatalogControlHandle {
    pub(crate) fn with_source_wake(mut self, source: crate::source_supervisor::SourceWake) -> Self {
        self.source = Some(source);
        self
    }
    pub(crate) fn resolve_source_change(
        &self,
        project: ProjectBinding,
        begin_operation: Incarnation,
    ) -> Result<airk_core::catalog_source_change::SourceChangeObservation> {
        match self.call(Operation::ResolveSourceChange {
            project,
            begin_operation,
        })? {
            Response::SourceChangeResolved(observation) => Ok(*observation),
            _ => anyhow::bail!("unexpected native source resolution response"),
        }
    }
    fn attach_before(&self, deadline: Instant) -> Result<()> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(
                !remaining.is_zero(),
                "native catalog startup predecessor deadline exceeded"
            );
            match self.call_with_timeout(Operation::Attach, remaining.min(Duration::from_secs(15)))
            {
                Ok(Response::Attached) => return Ok(()),
                Err(error) if error.downcast_ref::<NativePredecessorsPending>().is_some() => {
                    let delay = deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(100));
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                }
                Ok(_) => anyhow::bail!("catalog port attachment failed"),
                Err(error) => return Err(error),
            }
        }
    }
    pub(crate) fn administrative_read(
        &self,
        project: ProjectBinding,
    ) -> Result<NativeAdministrativeRead> {
        match self.call(Operation::AdministrativeRead { project })? {
            Response::AdministrativeRead(read) => Ok(*read),
            _ => anyhow::bail!("unexpected native administrative response"),
        }
    }
    fn call(&self, operation: Operation) -> Result<Response> {
        self.call_with_timeout(operation, Duration::from_secs(15))
    }
    fn call_with_timeout(&self, operation: Operation, budget: Duration) -> Result<Response> {
        let (reply, receive) = mpsc::sync_channel(1);
        let sender = self.sender.upgrade().context("catalog supervisor closed")?;
        let connection_setup = matches!(&operation, Operation::Connect { .. });
        sender
            .try_send(Envelope::Catalog {
                gate: self.gate.clone(),
                operation: Box::new(operation),
                reply,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) if connection_setup => {
                    anyhow::Error::new(ConnectionSetupPending::Queue)
                }
                _ => anyhow::anyhow!("catalog supervisor unavailable"),
            })?;
        drop(sender);
        // Dropping this waiter never cancels accepted actor work or frees native
        // query/work capacity. A late exact ACK updates the shared fence ledger.
        receive
            .recv_timeout(budget)
            .context("catalog native outcome unknown")?
    }
    pub(crate) fn issue_scoped(
        &self,
        selection: NativeCatalogAuthoritySelection,
        lifetime: Duration,
    ) -> Result<super::ScopedConnectionDelivery> {
        airk_core::index_catalog::unique_bounded(
            &selection.project_ids,
            airk_core::index_catalog::MAX_SELECTION,
        )?;
        ensure!(
            !selection.project_ids.is_empty()
                && !lifetime.is_zero()
                && lifetime <= Duration::from_secs(300),
            "invalid native scoped connection budget"
        );
        match self.call(Operation::Connect {
            selection: ConnectionSelection::Reviewed(selection),
            lifetime,
        })? {
            Response::Connection(value) => Ok(*value),
            _ => anyhow::bail!("unexpected catalog connection response"),
        }
    }
    pub(crate) fn issue_legacy(
        &self,
        selection: super::NativeLegacyAuthoritySelection,
        lifetime: Duration,
    ) -> Result<super::ScopedConnectionDelivery> {
        ensure!(
            !lifetime.is_zero() && lifetime <= Duration::from_secs(300),
            "invalid native compatibility lease budget"
        );
        match self.call(Operation::Connect {
            selection: ConnectionSelection::Legacy(selection),
            lifetime,
        })? {
            Response::Connection(delivery) => Ok(*delivery),
            _ => anyhow::bail!("unexpected native compatibility response"),
        }
    }
}
impl CatalogControl for CatalogControlHandle {
    fn fence_source_policy(
        &self,
        project: ProjectBinding,
        operation_id: Incarnation,
    ) -> Result<()> {
        self.source
            .as_ref()
            .context("native source policy watcher port unavailable")?
            .fence_policy(project, operation_id)
    }
    fn release_source_policy(
        &self,
        project: ProjectBinding,
        operation_id: Incarnation,
    ) -> Result<()> {
        self.source
            .as_ref()
            .context("native source policy watcher port unavailable")?
            .release_policy(project, operation_id)
    }
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool {
        self.gate.same_instance(gate)
    }
    fn revoke(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()> {
        ensure!(
            matches!(
                self.call(Operation::Revoke {
                    ticket: ticket.clone(),
                    target: target.clone()
                })?,
                Response::Revoked
            ),
            "unexpected catalog revocation response"
        );
        Ok(())
    }
    fn confirm_reaped(&self, worker: WorkerIdentity) -> Result<bool> {
        match self.call(Operation::Reaped { worker })? {
            Response::Reaped(value) => Ok(value),
            _ => anyhow::bail!("unexpected catalog reap response"),
        }
    }
    fn capture_workers(
        &self,
        ticket: &FenceTicket,
        backing: ProjectBinding,
    ) -> Result<crate::catalog_admission::FencePlan> {
        match self.call(Operation::CaptureWorkers {
            ticket: ticket.clone(),
            backing,
        })? {
            Response::Captured(plan) => Ok(*plan),
            _ => anyhow::bail!("unexpected native worker census response"),
        }
    }
    fn drain_worker(&self, ticket: &FenceTicket, worker: WorkerIdentity) -> Result<()> {
        // An in-process owner stops cooperatively and is reaped on a later
        // actor turn, so a pending drain is asked again until the owner's stop
        // grace and drain period have passed; after that it is quarantined.
        let deadline = Instant::now() + WORKER_DRAIN_BUDGET;
        loop {
            match self.call(Operation::DrainWorker {
                ticket: ticket.clone(),
                worker,
            }) {
                Ok(Response::Drained) => return Ok(()),
                Ok(_) => anyhow::bail!("unexpected native worker drain response"),
                Err(error) if error.is::<WorkerDrainPending>() => {
                    ensure!(
                        Instant::now() < deadline,
                        "native worker drain incomplete: the owner did not stop by its drain deadline and is quarantined"
                    );
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(error),
            }
        }
    }
    fn finish_worker_drains(
        &self,
        receipt: &airk_core::catalog_fences::FencedReceipt,
    ) -> Result<()> {
        if receipt.proof.worker_obligation.is_none() {
            return Ok(());
        }
        ensure!(
            matches!(
                self.call(Operation::FinishWorkerDrains {
                    receipt: Box::new(receipt.clone())
                })?,
                Response::Drained
            ),
            "unexpected native provider completion response"
        );
        Ok(())
    }
}

pub(super) enum Expected {
    ResolveSourceChange {
        worker: WorkerIdentity,
        nonce: Incarnation,
        begin_operation: Incarnation,
    },
    Admin {
        worker: WorkerIdentity,
        caller: Incarnation,
        nonce: Incarnation,
    },
    Install(InstallTicket),
    Revoke {
        ticket: FenceTicket,
        target: FenceTarget,
    },
    Grant {
        worker: WorkerIdentity,
        scope: ScopedReadRef,
        caller: Incarnation,
    },
}
pub(super) struct Pending {
    gate: NativeCatalogAdmission,
    expected: Expected,
}
enum Exchange {
    SourceChangeResolved(Box<airk_core::catalog_source_change::SourceChangeObservation>),
    AdminGrant(ConnectionGrant),
    Installed,
    Revoked,
    Grant(ConnectionGrant),
}
impl Pending {
    fn accept(self, reply: Reply) -> Result<Result<Exchange>> {
        // Outer failure means an unidentifiable/corrupt reply or an installation
        // that cannot settle its retained lease: quarantine. Inner failure means
        // the exact non-install exchange settled but authority changed/refused.
        // A failed installation cannot settle the supervisor's retained lease:
        // the worker may have rejected the first install or retained an older
        // generation. Retire this worker through the existing native drain/reap
        // path instead of repeatedly renewing a scope it may never have owned.
        if matches!(reply, Reply::Failed) && matches!(&self.expected, Expected::Install(_)) {
            anyhow::bail!("native scope installation refused; worker retirement required");
        }
        if matches!(reply, Reply::Failed)
            && !matches!(
                &self.expected,
                Expected::Admin { .. } | Expected::ResolveSourceChange { .. }
            )
        {
            return Ok(Err(anyhow::anyhow!("native catalog command refused")));
        }
        match (self.expected, reply) {
            (
                Expected::ResolveSourceChange {
                    worker,
                    nonce,
                    begin_operation,
                },
                Reply::SourceChangeResolution {
                    identity,
                    nonce: echoed,
                    observation,
                },
            ) => {
                ensure!(
                    identity == worker && echoed == nonce,
                    "foreign native source resolution reply"
                );
                let Some(observation) = observation else {
                    return Ok(Err(anyhow::anyhow!(
                        "native source resolution refused or uncertain"
                    )));
                };
                observation.begin.validate()?;
                ensure!(
                    observation.begin.metadata.operation_id == begin_operation
                        && observation.completion.is_some(),
                    "native source resolution phase differs"
                );
                let CatalogOperation::BeginIndexSourceChange { plan, .. } =
                    &observation.begin.request.operation
                else {
                    anyhow::bail!("native source begin required");
                };
                ensure!(
                    plan.registration.binding == worker.project,
                    "native source resolution backing changed"
                );
                if let Some(completion) = &observation.completion {
                    let _ = self.gate.reconcile_catalog_snapshot(
                        observation.begin.request.key.namespace,
                        completion.catalog_revision,
                    );
                }
                Ok(Ok(Exchange::SourceChangeResolved(observation)))
            }
            (
                Expected::Admin {
                    worker,
                    caller,
                    nonce,
                },
                Reply::AdminGrant {
                    identity,
                    nonce: echoed,
                    grant,
                },
            ) => {
                ensure!(
                    identity == worker && echoed == nonce,
                    "foreign native administrative grant reply"
                );
                let Some(grant) = grant else {
                    return Ok(Err(anyhow::anyhow!("native administrative grant refused")));
                };
                ensure!(
                    grant.protocol == PROTOCOL_VERSION
                        && grant.identity == worker
                        && grant.caller == caller
                        && grant.scope.is_none()
                        && (1..=MAX_GRANT_TTL_MS).contains(&grant.ttl_ms),
                    "foreign native administrative grant payload"
                );
                Ok(Ok(Exchange::AdminGrant(grant)))
            }
            (Expected::Install(ticket), Reply::ScopeInstalled { identity, scope }) => {
                ensure!(
                    identity == ticket.worker() && scope == ticket.scope(),
                    "foreign native scope installation reply"
                );
                self.gate
                    .acknowledge_install(ticket, identity, scope)
                    .context("native scope installation acknowledgement could not settle; worker retirement required")?;
                Ok(Ok(Exchange::Installed))
            }
            (Expected::Revoke { ticket, target }, Reply::ScopeRevoked { identity, scope }) => {
                ensure!(
                    identity == target.worker && scope == target.scope,
                    "foreign native scope revocation reply"
                );
                Ok(self
                    .gate
                    .record_revoked(&ticket, identity, scope)
                    .map(|_| Exchange::Revoked))
            }
            (
                Expected::Grant {
                    worker,
                    scope,
                    caller,
                },
                Reply::Grant { grant },
            ) => {
                ensure!(
                    grant.protocol == PROTOCOL_VERSION
                        && grant.identity == worker
                        && grant.scope == Some(scope)
                        && grant.caller == caller
                        && (1..=MAX_GRANT_TTL_MS).contains(&grant.ttl_ms),
                    "foreign native scoped grant reply"
                );
                Ok(Ok(Exchange::Grant(grant)))
            }
            _ => anyhow::bail!("unidentifiable native catalog reply"),
        }
    }
}
impl ChildWorker {
    fn settle_catalog_reply(&mut self, reply: Reply) -> Result<Result<Exchange>> {
        let pending = self
            .catalog_pending
            .take()
            .context("catalog exchange missing")?;
        match pending.accept(reply) {
            Ok(result) => Ok(result),
            Err(error) => {
                self.catalog_quarantined = true;
                Err(error)
            }
        }
    }
    pub(super) fn catalog_poll_ready(&mut self) -> Result<bool> {
        ensure!(!self.exiting, "native worker resources are draining");
        if self
            .read_settlement
            .as_ref()
            .is_some_and(ReadSettlement::control_pending)
        {
            return Ok(false);
        }
        ensure!(
            !self.catalog_quarantined,
            "native catalog control quarantined"
        );
        if self.catalog_pending.is_none() {
            return Ok(true);
        }
        match self.output.try_recv() {
            Ok(reply) => {
                let _settled = self.settle_catalog_reply(reply)?;
                Ok(true)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(false),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.catalog_quarantined = true;
                anyhow::bail!("native catalog control closed with unknown outcome")
            }
        }
    }
    pub(super) fn require_catalog_ready(&mut self) -> Result<()> {
        ensure!(!self.exiting, "native worker resources are draining");
        ensure!(
            self.catalog_poll_ready()?,
            "native catalog exchange still pending"
        );
        Ok(())
    }
    fn catalog_exchange(
        &mut self,
        command: WorkerCommand,
        gate: &NativeCatalogAdmission,
        expected: Expected,
    ) -> Result<Exchange> {
        ensure!(
            self.read_settlement.is_none(),
            "native read worker is retiring"
        );
        self.require_catalog_ready()?;
        self.catalog_gate = Some(gate.clone());
        self.catalog_pending = Some(Pending {
            gate: gate.clone(),
            expected,
        });
        if let Err(error) = self
            .input
            .as_mut()
            .context("project input missing")?
            .send(command)
        {
            self.catalog_quarantined = true;
            return Err(error.into());
        }
        let reply = self
            .output
            .recv_timeout(Duration::from_secs(3))
            .context("native catalog exchange outcome unknown")?;
        self.settle_catalog_reply(reply)?
    }
}

fn expires_unix_ms(expires: Instant) -> Result<u64> {
    let remaining = expires
        .checked_duration_since(Instant::now())
        .context("native scope lease expired")?;
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .checked_add(remaining)
        .context("native scope expiry overflow")?
        .as_millis();
    u64::try_from(value).context("native scope expiry overflow")
}
fn confirmed_reap(
    gate: &NativeCatalogAdmission,
    workers: &BTreeMap<ProjectBinding, ChildWorker>,
    root: &Path,
    worker: WorkerIdentity,
    may_allocate: bool,
) -> Result<bool> {
    if gate.worker_was_reaped(worker)? {
        return Ok(true);
    }
    if workers.contains_key(&worker.project) || !may_allocate {
        return Ok(false);
    }
    // Actual native owner exclusion, after existing predecessor/provider drain,
    // proves the old scope cannot still be served. A missing map/EOF alone does not.
    let Ok(_exclusion) = ProjectLease::acquire(root, worker.project) else {
        return Ok(false);
    };
    gate.observe_worker_reaped(worker)?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    bound: &mut Option<NativeCatalogAdmission>,
    gate: NativeCatalogAdmission,
    operation: Operation,
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    registry: &mut Option<ServiceRegistry>,
    root: &Path,
    instance: Incarnation,
    may_allocate: bool,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    admission: WorkerAdmission<'_>,
    hooks: crate::project_worker::Hooks,
) -> Result<Response> {
    gate.require_service(instance)?;
    // Before attaching a new empty authority ledger, the existing owner must
    // prove predecessor project/provider/descendant drain and endpoint cleanup.
    // The caller retries only this typed pre-effect condition while this same
    // Supervisor keeps its recovery loop alive; no listener/Ready is published.
    if matches!(operation, Operation::Attach) {
        ensure!(may_allocate, NativePredecessorsPending);
    }
    if let Some(previous) = bound.as_ref() {
        ensure!(previous.same_instance(&gate), "native catalog gate changed");
    } else {
        ensure!(
            matches!(operation, Operation::Attach),
            "native catalog port is not attached"
        );
        for worker in workers.values_mut() {
            let identity = worker.identity.context("project identity unavailable")?;
            gate.register_worker(identity)?;
            worker.catalog_gate = Some(gate.clone());
        }
        *bound = Some(gate.clone());
    }
    let mut live_helpers: Vec<_> = workers
        .values()
        .flat_map(|worker| worker.provider_helpers.iter().copied())
        .collect();
    live_helpers.extend(admission.drains.referenced_helpers());
    let mut live_ledgers: Vec<_> = workers
        .values()
        .filter_map(|worker| {
            worker
                .completed_authorizations()
                .map(|anchor| anchor.head_sha256)
        })
        .collect();
    live_ledgers.extend(admission.drains.referenced_ledgers());
    gate.prune_provider_reaps(&live_helpers, &live_ledgers)?;
    for proof in admission.recovery.reap_witnesses() {
        gate.observe_provider_reaped(*proof)?;
    }
    admission.recovery.clear_reap_witnesses();
    match operation {
        Operation::Attach => Ok(Response::Attached),
        Operation::ResolveSourceChange {
            project,
            begin_operation,
        } => {
            ensure!(
                may_allocate && admission.resources.running(project).is_none(),
                "native source owner still running work"
            );
            gate.require_worker_start(project)?;
            let registry = registry
                .as_ref()
                .context("native source registry unavailable")?;
            let before = registry.catalog_source_change_observation(begin_operation)?;
            if before.completion.is_some() {
                return Ok(Response::SourceChangeResolved(Box::new(before)));
            }
            let CatalogOperation::BeginIndexSourceChange { plan, .. } =
                &before.begin.request.operation
            else {
                anyhow::bail!("native source begin required");
            };
            ensure!(
                plan.registration.binding == project,
                "native source resolution backing changed"
            );
            let snapshot = registry
                .catalog_state()?
                .context("native source catalog unavailable")?;
            gate.begin_issue(snapshot.namespace, snapshot.revision)?;
            let registration = registry.get(project)?.registration;
            start_worker(
                workers,
                WorkerOwnerContext {
                    root,
                    instance,
                    gate: Some(&gate),
                    hooks,
                },
                registration,
                false,
                backoff,
                admission,
            )?;
            let worker = workers
                .get_mut(&project)
                .context("native source worker unavailable")?;
            let identity = worker
                .identity
                .context("native source worker identity unavailable")?;
            let nonce = new_instance_id()?;
            let Exchange::SourceChangeResolved(observation) = worker.catalog_exchange(
                WorkerCommand::ResolveSourceChange {
                    identity,
                    nonce,
                    begin_operation,
                },
                &gate,
                Expected::ResolveSourceChange {
                    worker: identity,
                    nonce,
                    begin_operation,
                },
            )?
            else {
                anyhow::bail!("native source resolution missing");
            };
            Ok(Response::SourceChangeResolved(observation))
        }
        Operation::FinishWorkerDrains { receipt } => {
            receipt.validate()?;
            gate.reconcile_terminal(&receipt.request, &receipt)?;
            if let Some(obligation) = &receipt.proof.worker_obligation {
                for worker in &obligation.workers {
                    ensure!(
                        receipt
                            .proof
                            .worker_confirmations
                            .iter()
                            .any(|confirmation| confirmation.worker == *worker),
                        "durable native worker proof missing"
                    );
                    if let Some(helper) = admission.pool.as_mut()
                        && let Some(ticket) = helper.retirement_ticket(*worker)?
                    {
                        helper.finish_retirement(&ticket)?;
                    }
                }
            }
            Ok(Response::Drained)
        }
        Operation::AdministrativeRead { project } => {
            ensure!(
                may_allocate,
                "native administrative predecessor still draining"
            );
            gate.require_worker_start(project)?;
            let registry = registry
                .as_ref()
                .context("native administrative registry unavailable")?;
            let snapshot = registry
                .catalog_state()?
                .context("native administrative catalog unavailable")?;
            let issue = gate.begin_issue(snapshot.namespace, snapshot.revision)?;
            let registration = registry.get(project)?.registration;
            start_worker(
                workers,
                WorkerOwnerContext {
                    root,
                    instance,
                    gate: Some(&gate),
                    hooks,
                },
                registration,
                false,
                backoff,
                admission,
            )?;
            let worker = workers
                .get_mut(&project)
                .context("native administrative worker unavailable")?;
            worker.require_catalog_ready()?;
            let identity = worker
                .identity
                .context("native administrative worker identity unavailable")?;
            gate.register_worker(identity)?;
            worker.catalog_gate = Some(gate.clone());
            let caller = local_ipc::caller_identity()?;
            let nonce = new_instance_id()?;
            let Exchange::AdminGrant(grant) = worker.catalog_exchange(
                WorkerCommand::GrantAdmin {
                    identity,
                    caller,
                    nonce,
                },
                &gate,
                Expected::Admin {
                    worker: identity,
                    caller,
                    nonce,
                },
            )?
            else {
                anyhow::bail!("native administrative grant missing");
            };
            gate.check_issue(issue)?;
            Ok(Response::AdministrativeRead(Box::new(
                NativeAdministrativeRead {
                    root: root.to_owned(),
                    gate,
                    issue,
                    role: worker.role.clone(),
                    grant,
                },
            )))
        }
        Operation::CaptureWorkers { ticket, backing } => {
            ensure!(may_allocate, "predecessor worker census still draining");
            admission.resources.forget_pending(backing);
            let mut census = Vec::new();
            let mut providers = Vec::new();
            if let Some(worker) = workers.get_mut(&backing) {
                let identity = worker
                    .identity
                    .context("native census worker identity unavailable")?;
                worker.catalog_gate = Some(gate.clone());
                census.push(identity);
                providers.push(
                    crate::catalog_admission::native_provider_obligation_completed(
                        identity,
                        worker.provider_helpers.clone(),
                        worker.provider_in_process,
                        worker.completed_authorizations(),
                    ),
                );
            } else {
                // A missing actor entry is not an empty native census while
                // another project owner can still hold the writer lease.
                let _exclusion = ProjectLease::acquire(root, backing)?;
            }
            if let Some(grant) = admission.resources.running(backing)
                && !census.contains(&grant.identity)
            {
                census.push(grant.identity);
            }
            for identity in admission.drains.for_project(backing) {
                if !census.contains(&identity) {
                    census.push(identity);
                }
                if !providers.iter().any(|provider| provider.worker == identity) {
                    providers.push(
                        crate::catalog_admission::native_provider_obligation_completed(
                            identity,
                            admission.drains.helpers(identity)?,
                            admission.drains.in_process(identity)?,
                            admission.drains.completed_authorizations(identity)?,
                        ),
                    );
                }
            }
            Ok(Response::Captured(Box::new(
                gate.capture_retirement_workers(&ticket, backing, census, providers)?,
            )))
        }
        Operation::DrainWorker { ticket, worker } => {
            gate.require_worker_target(&ticket, worker)?;
            if !gate.worker_requires_resources_drain(worker)? {
                return Ok(Response::Drained);
            }
            if !confirmed_reap(&gate, workers, root, worker, may_allocate)? {
                // A stop already began on an earlier attempt: the in-process
                // owner is reaped by a later actor turn, never waited for here.
                if !workers.contains_key(&worker.project) && super::stopping_project(worker.project)
                {
                    return Err(anyhow::Error::new(WorkerDrainPending));
                }
                let current = workers
                    .get(&worker.project)
                    .context("native drain worker still unavailable")?;
                ensure!(
                    current.identity == Some(worker),
                    "native drain worker changed"
                );
                gate.begin_worker_drain(&ticket, worker)?;
                // Existing single ownership performs cooperative Stop, bounded
                // drain, contained termination and actual descendant/provider reap.
                begin_worker_exit(workers, worker.project, admission.pool, admission.drains)?;
                if !gate.worker_was_reaped(worker)? && super::stopping_project(worker.project) {
                    return Err(anyhow::Error::new(WorkerDrainPending));
                }
            }
            ensure!(
                gate.worker_was_reaped(worker)?,
                "native worker drain incomplete"
            );
            let helpers = gate
                .worker_provider_history(worker)?
                .context("native predecessor provider history unknown")?;
            if !admission.drains.contains(worker) {
                admission.drains.reserve_history_completed(
                    worker,
                    helpers,
                    gate.worker_provider_in_process(worker)?,
                    gate.worker_completed_authorizations(worker)?,
                )?;
                admission.drains.child_reaped(worker)?;
            }
            let ledger_ready = match admission.drains.completed_authorizations(worker)? {
                Some(anchor) => recover_completed_ledger(
                    root,
                    instance,
                    &anchor,
                    Some(&gate),
                    admission.recovery,
                )?,
                None => true,
            };
            ensure!(
                admission.drains.poll_completed(
                    worker,
                    admission.pool.as_mut(),
                    |service, helper, in_process| {
                        recover_retired_helper(
                            root,
                            instance,
                            service,
                            helper,
                            in_process,
                            Some(&gate),
                            admission.recovery,
                        )
                    },
                    |_| Ok(ledger_ready),
                )? == DrainProgress::Ready,
                "native provider drain pending"
            );
            gate.observe_worker_resources_drained(worker)?;
            poll_worker_exits(
                admission.drains,
                admission.pool,
                admission.recovery,
                admission.resources,
                admission.policy,
                WorkerOwnerContext {
                    root,
                    instance,
                    gate: Some(&gate),
                    hooks,
                },
            )?;
            Ok(Response::Drained)
        }
        Operation::Connect {
            selection,
            lifetime,
        } => {
            ensure!(
                !lifetime.is_zero() && lifetime <= Duration::from_secs(300),
                "invalid native scoped connection budget"
            );
            ensure!(may_allocate, ConnectionSetupPending::Draining);
            let (namespace, catalog_revision, backing) = match &selection {
                ConnectionSelection::Reviewed(selected) => (
                    selected.namespace,
                    selected.catalog_revision,
                    selected.backing,
                ),
                ConnectionSelection::Legacy(selected) => (
                    selected.namespace,
                    selected.catalog_revision,
                    selected.binding,
                ),
            };
            let issue = gate.begin_issue(namespace, catalog_revision)?;
            let registry = registry.as_mut().context("catalog registry unavailable")?;
            let authority = match selection {
                ConnectionSelection::Reviewed(selected) => {
                    crate::catalog_authority::build_catalog_read_authority(registry, &selected)?
                }
                ConnectionSelection::Legacy(selected) => {
                    crate::catalog_authority::build_legacy_catalog_read_authority(
                        registry,
                        selected.namespace,
                        selected.host,
                        selected.catalog_revision,
                        selected.binding,
                    )?
                }
            };
            let registration = registry.get(backing)?.registration;
            start_worker(
                workers,
                WorkerOwnerContext {
                    root,
                    instance,
                    gate: Some(&gate),
                    hooks,
                },
                registration,
                false,
                backoff,
                admission,
            )?;
            let worker = workers
                .get_mut(&backing)
                .context("native scoped worker unavailable")?;
            ensure!(!worker.derived, "native scoped worker role changed");
            worker.require_catalog_ready()?;
            let identity = worker
                .identity
                .context("native scoped worker identity unavailable")?;
            gate.register_worker(identity)?;
            worker.catalog_gate = Some(gate.clone());
            // Reservation and actual private sends share this one actor turn.
            let ticket = gate.reserve_validated_scope(
                issue,
                identity,
                &authority,
                Instant::now() + lifetime,
            )?;
            if ticket.requires_install() {
                let command = match ticket.previous() {
                    Some(previous) => WorkerCommand::RenewScopeLease {
                        identity,
                        authority: Box::new(authority),
                        previous,
                        lease_generation: ticket.scope().lease_generation,
                        expires_unix_ms: expires_unix_ms(ticket.expires())?,
                    },
                    None => WorkerCommand::InstallScope {
                        identity,
                        authority: Box::new(authority),
                        lease_generation: ticket.scope().lease_generation,
                        expires_unix_ms: expires_unix_ms(ticket.expires())?,
                    },
                };
                ensure!(
                    matches!(
                        worker.catalog_exchange(command, &gate, Expected::Install(ticket))?,
                        Exchange::Installed
                    ),
                    "native installation acknowledgement missing"
                );
            }
            gate.check_scope_issue(issue, identity, ticket.scope())?;
            let caller = local_ipc::caller_identity()?;
            let Exchange::Grant(grant) = worker.catalog_exchange(
                WorkerCommand::GrantScoped {
                    identity,
                    caller,
                    scope: ticket.scope(),
                },
                &gate,
                Expected::Grant {
                    worker: identity,
                    scope: ticket.scope(),
                    caller,
                },
            )?
            else {
                anyhow::bail!("native scoped grant missing")
            };
            Ok(Response::Connection(Box::new(ScopedConnectionDelivery {
                gate,
                issue,
                worker: identity,
                scope: ticket.scope(),
                response: ControlResponse::Connection {
                    role: worker.role.clone(),
                    grant,
                },
            })))
        }
        Operation::Revoke { ticket, target } => {
            gate.validate_target(&ticket, &target)?;
            if confirmed_reap(&gate, workers, root, target.worker, may_allocate)? {
                return Ok(Response::Revoked);
            }
            let worker = workers
                .get_mut(&target.worker.project)
                .context("native revocation target still unavailable")?;
            ensure!(
                worker.identity == Some(target.worker),
                "native revocation worker changed"
            );
            let command = WorkerCommand::RevokeScope {
                identity: target.worker,
                scope: target.scope,
            };
            ensure!(
                matches!(
                    worker.catalog_exchange(command, &gate, Expected::Revoke { ticket, target })?,
                    Exchange::Revoked
                ),
                "native revocation acknowledgement missing"
            );
            Ok(Response::Revoked)
        }
        Operation::Reaped { worker } => Ok(Response::Reaped(confirmed_reap(
            &gate,
            workers,
            root,
            worker,
            may_allocate,
        )?)),
    }
}

#[cfg(test)]
mod startup_order_tests;

#[cfg(test)]
mod install_recovery_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(
        NativeCatalogAdmission,
        WorkerIdentity,
        ScopedReadRef,
        Incarnation,
    )> {
        use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor};
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        let gate = NativeCatalogAdmission::new(
            worker.service,
            Incarnation::from_bytes([4; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
            1,
        )?;
        let index_id = IndexId::from_identity(Incarnation::from_bytes([5; 16]));
        let project_id = QueryProjectId::from_identity(Incarnation::from_bytes([10; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([11; 16]));
        let authority = airk_kernel::scoped_reads::ScopedReadAuthority {
            schema: 1,
            backing: worker.project,
            index_id,
            project_ids: vec![project_id],
            provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
            grant: ReviewedGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([6; 16])),
                host: airk_core::identity::Sha256Digest::from_bytes([7; 32]),
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id,
                        index_id,
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: source,
                            kind: PathScope::Subtree,
                            path: String::new(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: std::env::temp_dir().to_string_lossy().into_owned(),
            }],
        };
        gate.register_worker(worker)?;
        let issue = gate.begin_issue(Incarnation::from_bytes([4; 16]), CatalogRevision::INITIAL)?;
        let installed = gate.reserve_install(
            issue,
            worker,
            &authority,
            Instant::now() + Duration::from_secs(60),
        )?;
        let scope = installed.scope();
        gate.acknowledge_install(installed, worker, scope)?;
        Ok((gate, worker, scope, Incarnation::from_bytes([8; 16])))
    }
    fn grant(worker: WorkerIdentity, scope: ScopedReadRef, caller: Incarnation) -> ConnectionGrant {
        ConnectionGrant {
            protocol: PROTOCOL_VERSION,
            identity: worker,
            caller,
            token: airk_core::identity::Sha256Digest::from_bytes([9; 32]),
            ttl_ms: 15_000,
            client: airk_core::usage::ClientFamily::Unknown,
            scope: Some(scope),
        }
    }
    fn delivery(
        gate: &NativeCatalogAdmission,
        worker: WorkerIdentity,
        scope: ScopedReadRef,
        caller: Incarnation,
    ) -> Result<ScopedConnectionDelivery> {
        let issue = gate.begin_issue(
            Incarnation::from_bytes([4; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
        )?;
        Ok(ScopedConnectionDelivery {
            gate: gate.clone(),
            issue,
            worker,
            scope,
            response: ControlResponse::Connection {
                role: protocol::role(worker.worker),
                grant: grant(worker, scope, caller),
            },
        })
    }
    fn close(gate: &NativeCatalogAdmission, scope: ScopedReadRef) -> Result<()> {
        let request = airk_core::catalog_management::CatalogRequest {
            caller: Incarnation::from_bytes([8; 16]),
            key: airk_core::service::RequestKey {
                namespace: Incarnation::from_bytes([4; 16]),
                epoch: 1,
                nonce: Incarnation::from_bytes([12; 16]),
            },
            expected_catalog_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
            operation: airk_core::catalog_management::CatalogOperation::RevokeGrant {
                id: scope.grant_id.unwrap(),
                expected_revision: scope.grant_revision.unwrap(),
            },
        };
        gate.close_request_admission(&request)?;
        Ok(())
    }
    #[test]
    fn actual_connection_queue_refusal_is_retryable_only_before_acceptance() -> Result<()> {
        let (gate, worker, _, _) = fixture()?;
        let (sender, receiver) = mpsc::sync_channel(0);
        let owner = Arc::new(sender);
        let handle = CatalogControlHandle {
            sender: Arc::downgrade(&owner),
            gate,
            source: None,
        };
        let connect = || Operation::Connect {
            selection: ConnectionSelection::Legacy(NativeLegacyAuthoritySelection {
                namespace: Incarnation::from_bytes([4; 16]),
                host: airk_core::identity::Sha256Digest::from_bytes([7; 32]),
                catalog_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
                binding: worker.project,
            }),
            lifetime: Duration::from_secs(300),
        };
        let rejected = handle
            .call(connect())
            .err()
            .context("full queue must reject")?;
        assert!(rejected.is::<ConnectionSetupPending>());
        assert_eq!(
            rejected
                .downcast_ref::<ConnectionSetupPending>()
                .unwrap()
                .code(),
            "catalog_connection_busy"
        );
        let other = handle
            .call(Operation::Attach)
            .err()
            .context("full queue must reject Attach")?;
        assert!(
            !other.is::<ConnectionSetupPending>(),
            "other actor operations gain no replay permission"
        );
        drop(receiver);
        let closed = handle
            .call(connect())
            .err()
            .context("closed queue must reject")?;
        assert!(
            !closed.is::<ConnectionSetupPending>(),
            "a disconnected owner is not known busy"
        );
        Ok(())
    }
    #[test]
    fn actual_actor_handoff_refuses_close_between_connection_build_and_send() -> Result<()> {
        let (gate, worker, scope, caller) = fixture()?;
        let built = delivery(&gate, worker, scope, caller)?;
        let (send, receive) = mpsc::sync_channel(1);
        close(&gate, scope)?;
        send_reply(&send, Ok(Response::Connection(Box::new(built))));
        assert!(
            matches!(receive.try_recv(), Ok(Err(_))),
            "closed authority handed off a connection bearer"
        );
        Ok(())
    }
    #[test]
    fn administrative_reply_requires_exact_nonce_identity_and_private_scope_none() -> Result<()> {
        let (gate, worker, scope, caller) = fixture()?;
        let nonce = Incarnation::from_bytes([41; 16]);
        let pending = || Pending {
            gate: gate.clone(),
            expected: Expected::Admin {
                worker,
                caller,
                nonce,
            },
        };
        let mut credential = grant(worker, scope, caller);
        credential.scope = None;
        assert!(matches!(
            pending().accept(Reply::AdminGrant {
                identity: worker,
                nonce,
                grant: Some(credential)
            })??,
            Exchange::AdminGrant(_)
        ));
        assert!(
            pending()
                .accept(Reply::AdminGrant {
                    identity: worker,
                    nonce: Incarnation::from_bytes([42; 16]),
                    grant: None
                })
                .is_err()
        );
        let mut foreign = worker;
        foreign.worker = Incarnation::from_bytes([43; 16]);
        assert!(
            pending()
                .accept(Reply::AdminGrant {
                    identity: foreign,
                    nonce,
                    grant: None
                })
                .is_err()
        );
        assert!(
            pending()
                .accept(Reply::AdminGrant {
                    identity: worker,
                    nonce,
                    grant: Some(grant(worker, scope, caller))
                })
                .is_err()
        );
        assert!(
            pending()
                .accept(Reply::AdminGrant {
                    identity: worker,
                    nonce,
                    grant: None
                })?
                .is_err()
        );
        assert!(
            pending().accept(Reply::Failed).is_err(),
            "an unidentifiable refusal must quarantine instead of consuming a future reply"
        );
        Ok(())
    }
    #[test]
    fn closed_handoff_receiver_does_not_roll_back_accepted_installation() -> Result<()> {
        let (gate, worker, scope, caller) = fixture()?;
        let built = delivery(&gate, worker, scope, caller)?;
        let issue = built.issue;
        let (send, receive) = mpsc::sync_channel(1);
        drop(receive);
        send_reply(&send, Ok(Response::Connection(Box::new(built))));
        gate.check_scope_issue(issue, worker, scope)?;
        let built = delivery(&gate, worker, scope, caller)?;
        let (send, receive) = tokio::sync::oneshot::channel();
        drop(receive);
        built.handoff_native(send);
        gate.check_scope_issue(issue, worker, scope)?;
        Ok(())
    }
    #[tokio::test]
    async fn final_delivery_rechecks_after_successful_handoff_and_backpressure() -> Result<()> {
        use std::future::Future;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (gate, worker, scope, caller) = fixture()?;
        let (send, receive) = mpsc::sync_channel(1);
        send_reply(
            &send,
            Ok(Response::Connection(Box::new(delivery(
                &gate, worker, scope, caller,
            )?))),
        );
        let Response::Connection(built) = receive.recv()?? else {
            anyhow::bail!("connection delivery missing")
        };
        let (mut writer, mut reader) = tokio::io::duplex(1);
        writer.write_all(b"x").await?;
        let mut pending = Box::pin((*built).deliver(&mut writer, Duration::from_secs(5)));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(pending.as_mut().poll(&mut cx).is_pending());
        close(&gate, scope)?;
        let mut preexisting = [0];
        reader.read_exact(&mut preexisting).await?;
        assert_eq!(&preexisting, b"x");
        assert!(pending.await.is_err());
        drop(writer);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        assert!(bytes.is_empty(), "queued bearer escaped after scope close");
        Ok(())
    }
    #[tokio::test]
    async fn blocked_final_delivery_has_one_deadline_and_never_writes_after_expiry() -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (gate, worker, scope, caller) = fixture()?;
        let (mut writer, mut reader) = tokio::io::duplex(1);
        writer.write_all(b"x").await?;
        let result = delivery(&gate, worker, scope, caller)?
            .deliver(&mut writer, Duration::from_millis(10))
            .await;
        assert!(result.is_err());
        drop(writer);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        assert_eq!(bytes, b"x");
        Ok(())
    }
    #[test]
    fn actual_private_frame_grant_handler_retains_full_scope_and_refuses_foreign_lease_ttl_or_reply_kind()
    -> Result<()> {
        let (gate, worker, scope, caller) = fixture()?;
        let mut frame = Vec::new();
        service_ipc::write_frame(
            &mut frame,
            &Reply::Grant {
                grant: grant(worker, scope, caller),
            },
        )?;
        let observed = service_ipc::read_frame::<_, Reply>(&mut std::io::Cursor::new(frame))?
            .context("missing private fixture reply")?;
        let pending = Pending {
            gate: gate.clone(),
            expected: Expected::Grant {
                worker,
                scope,
                caller,
            },
        };
        assert!(
            matches!(pending.accept(observed)??,Exchange::Grant(value) if value.scope==Some(scope))
        );
        for mutate in 0..4 {
            let mut value = grant(worker, scope, caller);
            match mutate {
                0 => value.scope.as_mut().unwrap().lease_generation += 1,
                1 => value.ttl_ms = MAX_GRANT_TTL_MS + 1,
                2 => value.identity.fencing_generation += 1,
                _ => value.caller = Incarnation::from_bytes([10; 16]),
            }
            let pending = Pending {
                gate: gate.clone(),
                expected: Expected::Grant {
                    worker,
                    scope,
                    caller,
                },
            };
            assert!(pending.accept(Reply::Grant { grant: value }).is_err());
        }
        let pending = Pending {
            gate,
            expected: Expected::Grant {
                worker,
                scope,
                caller,
            },
        };
        assert!(pending.accept(Reply::Renewed).is_err());
        Ok(())
    }
    #[test]
    fn weak_control_port_cannot_retain_owner_or_replay_after_supervisor_queue_closes() -> Result<()>
    {
        let (gate, _, _, _) = fixture()?;
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(1);
        let owner = Arc::new(sender);
        let handle = CatalogControlHandle {
            sender: Arc::downgrade(&owner),
            gate,
            source: None,
        };
        drop(owner);
        assert!(handle.call(Operation::Attach).is_err());
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        Ok(())
    }
}
