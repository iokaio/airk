// SPDX-License-Identifier: Apache-2.0
//! A query client obtains one grant, then communicates only with the project worker.
use crate::{
    control_client,
    local_ipc::{self, Stream},
    project_protocol::{self as protocol, Accepted, Query},
};
use airk_core::service::{ControlMethod, ControlResponse, PROTOCOL_VERSION, ProjectBinding};
use anyhow::{Context, Result, ensure};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

mod status_observation;
pub(crate) use status_observation::StatusOwner;
use status_observation::{RequestTicket, RetiredRequest};
mod read_lifetime;
#[derive(Debug)]
struct RetrySetup(&'static str);
impl std::fmt::Display for RetrySetup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for RetrySetup {}

#[derive(Debug)]
struct ScopedConnectionRefused {
    response: serde_json::Value,
    retryable: bool,
}
impl std::fmt::Display for ScopedConnectionRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native scoped connection was refused before query submission")
    }
}
impl std::error::Error for ScopedConnectionRefused {}

fn scoped_setup_rejection(
    selection: &airk_core::service::ScopedConnectRequest,
    error: &anyhow::Error,
) -> serde_json::Value {
    let native = error.downcast_ref::<ScopedConnectionRefused>();
    airk_mcp::remote::scoped_setup_failure(
        selection,
        "scoped_connection_unavailable",
        setup_retryable(error),
        native.map(|failure| failure.response.clone()),
    )
}

/// Obtains one in-process selection connection grant: the service's own
/// `Management::connect_selection` then `ScopedConnectionDelivery::
/// take_in_process` (selection authority §3.2 step 4). Called for the first
/// connect and for every reconnect, so each re-runs admission.
pub type SelectionConnector = Arc<dyn Fn() -> SelectionGrantFuture + Send + Sync>;
pub type SelectionGrantFuture =
    std::pin::Pin<Box<dyn Future<Output = Result<airk_core::service::ConnectionGrant>> + Send>>;

/// A window's W4 grant as one index's read session sees it. It holds no
/// nonce, lease or service instance: the connector owns the authority and
/// the failure bodies echo only the selection and its identity (F4, N3).
pub struct SelectionSession {
    pub window_selection: airk_core::service::WindowScopeSelection,
    pub scope_identity: airk_core::identity::Sha256Digest,
    pub index_id: airk_core::index_catalog::IndexId,
    pub connect: SelectionConnector,
}

/// The selection-path analogue of `scoped_setup_rejection` (§3.2 step 5):
/// `requested_scope` names the window selection and its scope identity
/// only; the error's text is never included.
fn selection_setup_rejection(
    session: &SelectionSession,
    error: &anyhow::Error,
) -> serde_json::Value {
    airk_mcp::remote::scoped_scope_setup_failure(
        &airk_mcp::remote::ScopedScope::Selection {
            window_scope_selection: session.window_selection.clone(),
            scope_identity: session.scope_identity,
        },
        "scoped_connection_unavailable",
        setup_retryable(error),
        None,
    )
}

fn setup_retryable(error: &anyhow::Error) -> bool {
    error.is::<RetrySetup>()
        || error
            .downcast_ref::<ScopedConnectionRefused>()
            .is_some_and(|error| error.retryable)
        || error.chain().any(|cause| {
            cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::UnexpectedEof
                )
            })
        })
}
async fn retry_setup<T, F, Fut>(
    mut connect: F,
    budget: Duration,
    attempts: usize,
    backoff: Duration,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    tokio::time::timeout(budget, async {
        for attempt in 0..attempts {
            match connect().await {
                Ok(connection) => return Ok(connection),
                Err(error) if attempt + 1 < attempts && setup_retryable(&error) => {
                    tokio::time::sleep(
                        backoff
                            .saturating_mul(1u32 << attempt.min(6))
                            .min(Duration::from_secs(5)),
                    )
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
        anyhow::bail!("direct setup attempt budget exhausted")
    })
    .await
    .map_err(|_| {
        anyhow::Error::new(RetrySetup(
            "direct setup deadline exceeded before query submission",
        ))
    })?
}

fn setup_rejection(error: &anyhow::Error) -> Option<serde_json::Value> {
    if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
        return serde_json::to_value(airk_mcp::response::error_result(error)).ok();
    }
    if !setup_retryable(error) {
        return None;
    }
    let detail = serde_json::json!({"error":{"code":"direct_query_setup","reason":"busy",
        "query_submitted":false,"retryable":true,"retry_after_ms":100,
        "fresh_brokerage_required":true}});
    Some(serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true}))
}

pub struct Connection {
    stream: Option<Stream>,
    identity: airk_core::service::WorkerIdentity,
    authorized_until: Instant,
    last_finished: Instant,
    healthy: bool,
    scoped: bool,
    read_scope: Option<airk_core::service::ScopedReadRef>,
    read_attempt_supported: bool,
    status_receipt: airk_mcp::response::StatusReceipt,
}
impl Connection {
    pub async fn connect_scoped(
        root: &Path,
        project: ProjectBinding,
        selection: &airk_core::service::ScopedConnectRequest,
        client: airk_core::usage::ClientFamily,
    ) -> Result<Self> {
        selection.validate()?;
        retry_setup(
            || Self::connect_once_selected(root, project, client, None, Some(selection)),
            Duration::from_secs(30),
            8,
            Duration::from_millis(100),
        )
        .await
        .map_err(|error| {
            crate::usage_client::NativeReadFailure::from_response(scoped_setup_rejection(
                selection, &error,
            ))
            .into()
        })
    }
    pub async fn connect(root: &Path, project: ProjectBinding) -> Result<Self> {
        Self::connect_as(root, project, airk_core::usage::ClientFamily::Unknown).await
    }
    pub async fn connect_as(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
    ) -> Result<Self> {
        // No evidence query has been submitted. Every retry gets fresh health,
        // scope and a new short-lived grant; an uncertain query is never replayed.
        retry_setup(
            || Self::connect_once(root, project, client),
            Duration::from_secs(30),
            8,
            Duration::from_millis(100),
        )
        .await
    }
    async fn connect_frontend(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
        owner: &StatusOwner,
        ticket: RequestTicket,
        once: bool,
    ) -> Result<Self> {
        Self::connect_frontend_selected(root, project, client, owner, ticket, once, None).await
    }
    #[allow(clippy::too_many_arguments)]
    async fn connect_frontend_selected(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
        owner: &StatusOwner,
        ticket: RequestTicket,
        once: bool,
        selection: Option<&airk_core::service::ScopedConnectRequest>,
    ) -> Result<Self> {
        if once {
            return Self::connect_once_selected(
                root,
                project,
                client,
                Some((owner, &ticket)),
                selection,
            )
            .await;
        }
        retry_setup(
            || {
                Self::connect_once_selected(
                    root,
                    project,
                    client,
                    Some((owner, &ticket)),
                    selection,
                )
            },
            Duration::from_secs(30),
            8,
            Duration::from_millis(100),
        )
        .await
    }
    async fn connect_once(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
    ) -> Result<Self> {
        Self::connect_once_checked(root, project, client, None).await
    }
    async fn connect_once_checked(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
        owner: Option<(&StatusOwner, &RequestTicket)>,
    ) -> Result<Self> {
        Self::connect_once_selected(root, project, client, owner, None).await
    }
    async fn connect_once_selected(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
        owner: Option<(&StatusOwner, &RequestTicket)>,
        selection: Option<&airk_core::service::ScopedConnectRequest>,
    ) -> Result<Self> {
        let response = {
            let _health_request = owner
                .map(|(owner, ticket)| {
                    owner
                        .after_evidence_wait(*ticket)
                        .and_then(|fresh| owner.admit_ticket(&fresh))
                })
                .transpose()?;
            control_client::request(root, ControlMethod::Health).await?
        };
        let observation = match response {
            ControlResponse::Health { observation } => observation,
            // A misplaced positive credential is a protocol error, never an
            // error payload to retain in a journal or export.
            ControlResponse::Connection { .. } => anyhow::bail!("service health protocol mismatch"),
            response if selection.is_some() => {
                let retryable = matches!(&response, ControlResponse::Failure { code } if matches!(code.as_str(),
                    "broker_busy" | "project_starting_or_unavailable" | "project_unavailable_or_busy"
                    | "previous_workers_draining" | "catalog_revision_changed" | "catalog_clock_pending" | "catalog_connection_busy" | "project_capacity_busy" | "project_restart_backoff"));
                return Err(ScopedConnectionRefused {
                    response: serde_json::to_value(response)?,
                    retryable,
                }
                .into());
            }
            _ => anyhow::bail!("service health unavailable"),
        };
        ensure!(
            observation.version == env!("CARGO_PKG_VERSION"),
            "native service version differs from this client"
        );
        let selected = selection
            .map(|selection| -> Result<_> {
                let clock = observation
                    .catalog_clock
                    .as_ref()
                    .ok_or(RetrySetup("native catalog clock is not ready"))?;
                ensure!(
                    clock.namespace == selection.namespace,
                    "catalog session namespace changed; select reviewed scope again"
                );
                if clock.revision < selection.expected_catalog_revision {
                    return Err(RetrySetup("native catalog clock is stale").into());
                }
                let mut selected = selection.clone();
                selected.expected_catalog_revision = clock.revision;
                Ok(selected)
            })
            .transpose()?;
        let selection = selected.as_ref();
        let _broker_request = owner
            .map(|(owner, ticket)| {
                owner
                    .after_evidence_wait(*ticket)
                    .and_then(|fresh| owner.admit_ticket(&fresh))
            })
            .transpose()?; // one request after health
        let method = match selection {
            Some(selection) => ControlMethod::ConnectScoped {
                request: selection.clone(),
            },
            None => ControlMethod::Connect { project },
        };
        let response = control_client::request(root, method).await?;
        if selection.is_some() && !matches!(&response, ControlResponse::Connection { .. }) {
            let retryable = matches!(&response, ControlResponse::Failure { code } if matches!(code.as_str(),
                "broker_busy" | "project_starting_or_unavailable" | "project_unavailable_or_busy"
                | "previous_workers_draining" | "catalog_revision_changed" | "catalog_clock_pending" | "catalog_connection_busy" | "project_capacity_busy" | "project_restart_backoff"));
            return Err(ScopedConnectionRefused {
                response: serde_json::to_value(response)?,
                retryable,
            }
            .into());
        }
        let (role, grant) = match response {
            ControlResponse::Connection { role, grant } => (role, grant),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                return Err(airk_core::scoped_reads::ScopeReadRefused.into());
            }
            ControlResponse::Failure { code } if code == "project_unregistered" => {
                anyhow::bail!(
                    "project is unregistered; review native registration before connecting"
                )
            }
            ControlResponse::Failure { code }
                if matches!(
                    code.as_str(),
                    "broker_busy"
                        | "project_starting_or_unavailable"
                        | "project_unavailable_or_busy"
                        | "previous_workers_draining"
                        | "catalog_revision_changed"
                        | "catalog_clock_pending"
                        | "catalog_connection_busy"
                        | "project_capacity_busy"
                        | "project_restart_backoff"
                ) =>
            {
                return Err(RetrySetup("direct worker setup temporarily unavailable").into());
            }
            _ => anyhow::bail!("project unavailable; no direct-store fallback"),
        };
        ensure!(
            grant.protocol == PROTOCOL_VERSION
                && grant.identity.service == observation.instance
                && grant.identity.project == project
                && grant.caller == local_ipc::caller_identity()?
                && role == protocol::role(grant.identity.worker),
            "brokered worker identity mismatch"
        );
        if let Some(selection) = selection {
            let scope = grant
                .scope
                .context("scoped connection omitted its native authority")?;
            scope.validate()?;
            ensure!(
                scope.index_id == selection.index_id
                    && scope.catalog_grant()
                        == Some((selection.grant_id, selection.expected_grant_revision)),
                "scoped connection differs from the selected reviewed grant"
            );
        }
        Self::attach(
            root,
            &role,
            grant,
            client,
            airk_host::read_attempt::supported(&observation.version),
        )
        .await
    }
    /// Connect under a window's W4 grant: the grant comes from the service's
    /// in-process admission, never from a control-pipe brokerage (§3.2
    /// step 4), and must name the session's index and selection identity.
    async fn connect_selection(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
        owner: &StatusOwner,
        ticket: RequestTicket,
        session: &SelectionSession,
    ) -> Result<Self> {
        let _admitted = owner
            .after_evidence_wait(ticket)
            .and_then(|fresh| owner.admit_ticket(&fresh))?;
        let grant = (session.connect)().await?;
        let scope = grant
            .scope
            .context("selection connection omitted its native authority")?;
        scope.validate()?;
        ensure!(
            scope.index_id == session.index_id
                && scope.selection.is_some_and(|selection| {
                    selection.scope_identity == session.scope_identity
                        && selection.catalog_revision
                            == session.window_selection.expected_catalog_revision
                }),
            "selection connection differs from the window's grant"
        );
        ensure!(
            grant.protocol == PROTOCOL_VERSION
                && grant.identity.project == project
                && grant.caller == local_ipc::caller_identity()?,
            "brokered worker identity mismatch"
        );
        let role = protocol::role(grant.identity.worker);
        Self::attach(
            root,
            &role,
            grant,
            client,
            airk_host::read_attempt::supported(env!("CARGO_PKG_VERSION")),
        )
        .await
    }
    /// Hand an admitted grant to its worker over the private host-worker
    /// endpoint (the only serialized carrier of a selection scope, N3).
    async fn attach(
        root: &Path,
        role: &str,
        mut grant: airk_core::service::ConnectionGrant,
        client: airk_core::usage::ClientFamily,
        read_attempt_supported: bool,
    ) -> Result<Self> {
        let setup_started = Instant::now();
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            local_ipc::connect(&local_ipc::endpoint(root, role)?),
        )
        .await??;
        grant.client = client;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
        let accepted: Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .ok_or_else(|| anyhow::anyhow!("project rejected connection grant"))?;
        ensure!(
            accepted.identity == grant.identity,
            "direct worker identity mismatch"
        );
        Ok(Self {
            stream: Some(stream),
            identity: grant.identity,
            authorized_until: setup_started + Duration::from_secs(55),
            last_finished: Instant::now(),
            healthy: true,
            scoped: grant.scope.is_some(),
            read_scope: grant.scope,
            read_attempt_supported,
            status_receipt: Default::default(),
        })
    }
    pub(crate) fn reusable(&self, now: Instant) -> bool {
        self.healthy
            && now < self.authorized_until
            && now.saturating_duration_since(self.last_finished) < Duration::from_secs(25)
    }
    pub fn require_scope_fingerprint(
        &self,
        expected: airk_core::identity::Sha256Digest,
    ) -> Result<()> {
        if self
            .read_scope
            .is_some_and(|scope| scope.fingerprint == expected)
        {
            Ok(())
        } else {
            Err(airk_core::scoped_reads::ScopeReadRefused.into())
        }
    }

    fn reusable_for_source(&self, fence: SourceFence, now: Instant) -> bool {
        self.reusable(now)
            && self.identity.project == fence.project
            && self.identity.fencing_generation >= fence.fencing_generation
    }

    pub async fn call(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.call_with_read_context(
            tool,
            arguments,
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
        .await
    }
    pub async fn call_with_read_context(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<serde_json::Value> {
        self.exchange(
            Query {
                context_partition: None,
                frontend: if tool == "airk_diagnostics" && !self.scoped {
                    Some(airk_supervisor::runtime_metadata::observe_process())
                } else {
                    None
                },
                tool,
                arguments,
                native: None,
                status_receipt: self.status_receipt,
                read_attempt: None,
            },
            context,
        )
        .await
    }
    pub async fn read_native(
        &mut self,
        request: airk_mcp::local::NativeRead,
    ) -> Result<serde_json::Value> {
        self.read_native_with_context(
            request,
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
        .await
    }
    pub async fn read_native_with_context(
        &mut self,
        request: airk_mcp::local::NativeRead,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<serde_json::Value> {
        let context = if matches!(
            &request,
            airk_mcp::local::NativeRead::SourceProjection { .. }
                | airk_mcp::local::NativeRead::VerifyProjection { .. }
        ) {
            context
        } else {
            context.with_native_retry_deadline(Instant::now() + Duration::from_secs(5))
        };
        self.exchange(
            Query {
                context_partition: None,
                frontend: None,
                tool: String::new(),
                arguments: Default::default(),
                native: Some(request),
                status_receipt: self.status_receipt,
                read_attempt: None,
            },
            context,
        )
        .await
    }
    async fn exchange(
        &mut self,
        mut query: Query,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<serde_json::Value> {
        ensure!(
            self.reusable(Instant::now()),
            "direct connection expired before query submission; obtain fresh brokerage"
        );
        let context = context.narrowed(self.authorized_until);
        query.context_partition = context.context_partition().cloned();
        ensure!(
            query.context_partition.is_none() || self.read_attempt_supported,
            "context partitions require original native read attempts"
        );
        if self.read_attempt_supported {
            query.read_attempt = Some(airk_host::read_attempt::encode(
                &context,
                match context.request_id() {
                    Some(id) => id,
                    None => airk_kernel::service_broker::new_instance_id()?,
                },
            )?);
        }
        if let Some(partition) = &query.context_partition {
            let attempt = query
                .read_attempt
                .as_ref()
                .context("context partition original attempt missing")?;
            partition.validate_binding(attempt, partition.selection, partition.index_id)?;
            ensure!(
                query.native.is_none() && query.tool == "airk_answer",
                "context partition requires canonical Answer"
            );
        }
        loop {
            ensure!(
                !context.cancellation().is_cancelled() && Instant::now() < context.deadline(),
                "native read deadline or cancellation before submission"
            );
            // A dropped async caller may leave an unread result on this stream. Mark
            // it unusable before the first write; restore only after a complete reply.
            self.healthy = false;
            let stream = self
                .stream
                .take()
                .context("native query stream unavailable")?;
            // Detaching a canceled outer future must not drop the native stream
            // or accepted capacity while the worker still owns this operation.
            let exchange = tokio::spawn(read_lifetime::submit(
                stream,
                query.clone(),
                self.identity,
                self.read_scope,
                context.clone(),
            ));
            let (result, stream) = exchange.await.context("native read owner failed")?;
            self.stream = stream;
            let result = result?;
            ensure!(
                !context.cancellation().is_cancelled() && Instant::now() < context.deadline(),
                "native read completed after caller cancellation or deadline; delivery suppressed"
            );
            self.last_finished = Instant::now();
            self.healthy = !result.rebind;
            self.status_receipt = result.status_receipt;
            if result.read_admission.is_some() && context.wait_for_selection_admission().await {
                // Only this exact proven pre-handler refusal can repeat. Keep
                // the original serialized deadline, request ID, scope and args.
                continue;
            }
            return Ok(result.result);
        }
    }

    pub fn status_receipt(&self) -> airk_mcp::response::StatusReceipt {
        self.status_receipt
    }
    pub fn restore_status_receipt(&mut self, receipt: airk_mcp::response::StatusReceipt) {
        self.status_receipt = receipt;
    }
}

#[cfg(test)]
mod read_attempt_tests;

#[derive(Default)]
struct ClientState {
    connection: Option<Connection>,
    connection_scope: Option<CacheScope>,
    status_receipt: airk_mcp::response::StatusReceipt,
}

struct ChannelPool {
    capacity: Arc<airk_mcp::query_capacity::QueryCapacity>,
    slots: Vec<tokio::sync::Mutex<ClientState>>,
}
struct ChannelLease<'a> {
    state: Option<tokio::sync::MutexGuard<'a, ClientState>>,
    permit: Option<airk_mcp::query_capacity::QueryPermit>,
}
impl std::ops::Deref for ChannelLease<'_> {
    type Target = ClientState;
    fn deref(&self) -> &ClientState {
        self.state.as_deref().expect("leased channel")
    }
}
impl std::ops::DerefMut for ChannelLease<'_> {
    fn deref_mut(&mut self) -> &mut ClientState {
        self.state.as_deref_mut().expect("leased channel")
    }
}
impl Drop for ChannelLease<'_> {
    fn drop(&mut self) {
        // Return the actual channel before making its admission slot available.
        drop(self.state.take());
        drop(self.permit.take());
    }
}
impl Default for ChannelPool {
    fn default() -> Self {
        Self {
            capacity: airk_mcp::query_capacity::QueryCapacity::new(
                airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY,
            ),
            slots: (0..airk_core::service_resources::MAX_QUERY_CONCURRENCY)
                .map(|_| tokio::sync::Mutex::new(ClientState::default()))
                .collect(),
        }
    }
}
impl ChannelPool {
    async fn acquire(&self, limit: usize) -> ChannelLease<'_> {
        self.capacity.configure(limit);
        for slot in self.slots.iter().skip(limit) {
            if let Ok(mut idle) = slot.try_lock() {
                *idle = ClientState::default();
            }
        }
        let permit = self
            .capacity
            .clone()
            .acquire_owned()
            .await
            .expect("pool lifetime follows status owner");
        // A queued caller may have observed a lower policy before another
        // caller increased capacity. Its permit uses the current shared limit;
        // search the complete fixed pool rather than that older prefix.
        for slot in &self.slots {
            if let Ok(state) = slot.try_lock() {
                return ChannelLease {
                    state: Some(state),
                    permit: Some(permit),
                };
            }
        }
        unreachable!("an admitted channel always has a free slot")
    }
    #[cfg(test)]
    async fn lock(&self) -> ChannelLease<'_> {
        self.acquire(airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY)
            .await
    }
}

pub struct Backend {
    root: PathBuf,
    project: ProjectBinding,
    state: ChannelPool,
    state_waiters: Arc<tokio::sync::Semaphore>,
    latest_source: std::sync::Mutex<Option<SourceFence>>,
    status: Arc<StatusOwner>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
    catalog_session: Option<airk_core::service::ScopedConnectRequest>,
    /// A window's W4 grant (selection authority §3.2 step 4); exclusive
    /// with `catalog_session`.
    selection_session: Option<SelectionSession>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceFence {
    pub namespace: airk_core::identity::Incarnation,
    pub revision: u64,
    pub project: ProjectBinding,
    pub fencing_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CacheScope {
    Pinned(airk_core::identity::Incarnation),
    Registered(SourceFence),
    Source(SourceFence),
    Scoped {
        namespace: airk_core::identity::Incarnation,
        project: ProjectBinding,
        selection: airk_core::identity::Sha256Digest,
    },
}
impl From<airk_core::identity::Incarnation> for CacheScope {
    fn from(value: airk_core::identity::Incarnation) -> Self {
        Self::Pinned(value)
    }
}
pub(crate) fn source_scope_superseded() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"direct_query_setup","reason":"busy",
        "query_submitted":false,"retryable":true,"retry_after_ms":100,"fresh_brokerage_required":true}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}
fn query_admission_busy() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"direct_query_admission","reason":"busy",
        "query_submitted":false,"retryable":true,"retry_after_ms":25,"fresh_brokerage_required":false}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}
fn query_admission_closed() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"direct_query_admission","reason":"draining",
        "query_submitted":false,"retryable":false,"retry_after_ms":null,"fresh_brokerage_required":true}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}
struct CachedStatus {
    scope: CacheScope,
    captured: Instant,
    observed_unix_ms: u64,
    body: serde_json::Value,
}
impl CachedStatus {
    #[cfg(test)]
    fn agrees_with(&self, result: &Result<serde_json::Value>) -> bool {
        let Ok(result) = result else {
            return false;
        };
        if result["isError"] == true {
            return false;
        }
        let Some(body) = result["content"][0]["text"]
            .as_str()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        else {
            return false;
        };
        [
            &body["publication"]["token"],
            &body["read"]["current_publication"]["token"],
            &body["read"]["publication"]["token"],
        ]
        .into_iter()
        .find(|token| token.is_string())
        .is_some_and(|token| *token == self.body["publication"]["token"])
    }
    fn new(scope: impl Into<CacheScope>, result: &serde_json::Value) -> Option<Self> {
        if result["isError"] == true {
            return None;
        }
        let body: serde_json::Value =
            serde_json::from_str(result["content"][0]["text"].as_str()?).ok()?;
        if body["index_state"] != "available" || !body["publication"]["token"].is_string() {
            return None;
        }
        Some(Self {
            scope: scope.into(),
            captured: Instant::now(),
            observed_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()?,
            body,
        })
    }
    fn reply(
        &self,
        scope: impl Into<CacheScope>,
        now: Instant,
        cached: bool,
    ) -> Option<serde_json::Value> {
        let age = now.saturating_duration_since(self.captured);
        if scope.into() != self.scope || age >= Duration::from_secs(5) {
            return None;
        }
        let mut body = self.body.clone();
        body["observed_unix_ms"] = self.observed_unix_ms.into();
        body["cache_age_ms"] = (age.as_millis() as u64).into();
        body["cached"] = cached.into();
        Some(airk_mcp::unified::success(body))
    }
}

fn status_scope(root: &Path, project: ProjectBinding) -> Result<Option<CacheScope>> {
    if !root.join("service-1.4.sqlite3").try_exists()? {
        return Ok(None);
    }
    let deadline = Some(Instant::now() + Duration::from_secs(2));
    let registry =
        airk_store::service::registry::ServiceRegistry::open_saved_before(root, deadline)?;
    let before = registry.source_scope_stamp(deadline)?;
    let registered = registry.get(project)?;
    let after = registry.source_scope_stamp(deadline)?;
    if before != after {
        return Err(airk_store::service::registry::SavedScopeExpired.into());
    }
    Ok(Some(CacheScope::Registered(SourceFence {
        namespace: after.namespace,
        revision: after.revision,
        project,
        fencing_generation: registered.fencing_generation,
    })))
}
impl airk_mcp::remote::RemoteTools for Backend {
    fn local_help(&self) -> bool {
        !self.scoped()
    }
    fn scoped_selection(&self) -> Option<&airk_core::service::ScopedConnectRequest> {
        self.catalog_session.as_ref()
    }
    fn scoped_scope(&self) -> Option<airk_mcp::remote::ScopedScope> {
        match (&self.catalog_session, &self.selection_session) {
            (Some(selection), None) => {
                Some(airk_mcp::remote::ScopedScope::Catalog(selection.clone()))
            }
            (None, Some(session)) => Some(airk_mcp::remote::ScopedScope::Selection {
                window_scope_selection: session.window_selection.clone(),
                scope_identity: session.scope_identity,
            }),
            (None, None) => None,
            (Some(_), Some(_)) => {
                unreachable!("catalog and selection sessions are mutually exclusive")
            }
        }
    }
    fn query_concurrency(&self) -> usize {
        airk_host::service_resource_policy::query_concurrency(&self.root)
    }
    fn close(&self) {
        self.state_waiters.close();
        self.status.close();
    }
    fn shutdown(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move { self.status.shutdown().await })
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        if let Ok(mut label) = self.client.lock() {
            *label = client;
        }
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, None, None, None, None)
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        _limit: usize,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, None, None, None, Some(context))
    }
}

impl Backend {
    /// A catalog or selection session: help, status and diagnostics go to
    /// the restricted worker, never to the local or pinned paths.
    fn scoped(&self) -> bool {
        self.catalog_session.is_some() || self.selection_session.is_some()
    }
    fn accept_source_fence(&self, fence: SourceFence) -> bool {
        if fence.project != self.project {
            return false;
        }
        let mut latest = self
            .latest_source
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if latest.is_some_and(|old| {
            old.namespace != fence.namespace
                || old.revision > fence.revision
                || (old.revision == fence.revision
                    && old.fencing_generation > fence.fencing_generation)
        }) {
            return false;
        }
        *latest = Some(fence);
        true
    }
    fn source_fence_current(&self, scope: CacheScope) -> bool {
        match scope {
            CacheScope::Pinned(_) | CacheScope::Registered(_) | CacheScope::Scoped { .. } => true,
            CacheScope::Source(fence) => {
                self.latest_source
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .as_ref()
                    == Some(&fence)
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn call_observed(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        fence: SourceFence,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, Some(fence), None, None, None)
    }
    #[cfg(test)]
    pub(crate) fn call_observed_until(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        fence: SourceFence,
        expires: Instant,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, Some(fence), Some(expires), None, None)
    }
    pub(crate) fn call_observed_with_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        fence: SourceFence,
        expires: Instant,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(
            tool,
            arguments,
            Some(fence),
            Some(expires),
            None,
            Some(context),
        )
    }
    /// Typed native reads retain the caller's pool, budget and delivery tickets;
    /// no internal native operation becomes a callable MCP tool name.
    pub(crate) fn call_native_with_context(
        &self,
        request: airk_core::native_reads::NativeRead,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(
            String::new(),
            Default::default(),
            None,
            None,
            Some(request),
            Some(context),
        )
    }
    fn call_bound(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        observed: Option<SourceFence>,
        source_expires: Option<Instant>,
        native: Option<airk_core::native_reads::NativeRead>,
        context: Option<airk_mcp::read_attempt::ReadAttemptContext>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        let context = context.unwrap_or_else(|| {
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            )
        });
        let context =
            source_expires.map_or_else(|| context.clone(), |expires| context.narrowed(expires));
        Box::pin(async move {
            if tool == "airk_help" && !self.scoped() {
                return Ok(serde_json::to_value(airk_mcp::help::call(
                    serde_json::Value::Object(arguments),
                ))?);
            }
            if matches!(tool.as_str(), "airk_status" | "airk_diagnostics")
                && serde_json::from_value::<airk_mcp::StatusArgs>(serde_json::Value::Object(
                    arguments.clone(),
                ))
                .is_err()
            {
                return Ok(airk_mcp::unified::failure(
                    "INVALID_SCHEMA",
                    "Invalid status arguments.",
                ));
            }
            if let Some(fence) = observed
                && !self.accept_source_fence(fence)
            {
                return Ok(source_scope_superseded());
            }
            if source_expires.is_some_and(|until| Instant::now() >= until) {
                return Ok(source_scope_superseded());
            }
            if tool == "airk_status" && !self.scoped() {
                let client = self
                    .client
                    .lock()
                    .map(|value| *value)
                    .unwrap_or(airk_core::usage::ClientFamily::Unknown);
                return self
                    .status
                    .status_with_read_context(status_observation::StatusRequest {
                        root: self.root.clone(),
                        project: self.project,
                        args: arguments,
                        source: observed,
                        expires: source_expires,
                        client,
                        context,
                    })
                    .await;
            }
            if tool == "airk_diagnostics" {
                self.status.invalidate();
                airk_supervisor::runtime_metadata::observe_process();
            }
            if observed.is_none() && !self.scoped() {
                self.status
                    .bind_initial_pinned_scope(self.root.clone(), self.project)
                    .await?;
            }
            // Capture logical receipt and actual-authority generation before any
            // evidence-channel wait. A later grant returns its own fresh ticket.
            let mut ticket = self.status.request_ticket(self.project, source_expires);
            if observed.is_some_and(|expected| ticket.scope != Some(CacheScope::Source(expected))) {
                return Ok(source_scope_superseded());
            }
            if self.status.accepting().is_err() {
                return Ok(query_admission_closed());
            }
            let waiting = match self.state_waiters.clone().try_acquire_owned() {
                Ok(waiting) => waiting,
                Err(tokio::sync::TryAcquireError::Closed) => return Ok(query_admission_closed()),
                Err(tokio::sync::TryAcquireError::NoPermits) => return Ok(query_admission_busy()),
            };
            let queue_expires = source_expires
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(2))
                .min(Instant::now() + Duration::from_secs(2));
            let waited = tokio::select! {
                biased;
                _ = self.status.when_closed() => return Ok(query_admission_closed()),
                state = tokio::time::timeout_at(
                    tokio::time::Instant::from_std(queue_expires), self.state.acquire(airk_host::service_resource_policy::query_concurrency(&self.root)),
                ) => state,
            };
            let mut state = match waited {
                Ok(state) => state,
                Err(_) => {
                    return Ok(
                        if source_expires.is_some_and(|until| Instant::now() >= until) {
                            source_scope_superseded()
                        } else {
                            query_admission_busy()
                        },
                    );
                }
            };
            drop(waiting);
            if Instant::now() >= queue_expires {
                return Ok(
                    if source_expires.is_some_and(|until| Instant::now() >= until) {
                        source_scope_superseded()
                    } else {
                        query_admission_busy()
                    },
                );
            }
            ticket = match self.status.after_evidence_wait(ticket) {
                Ok(ticket) => ticket,
                Err(_) => return Ok(source_scope_superseded()),
            };
            if source_expires.is_some_and(|until| Instant::now() >= until)
                || observed
                    .is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence)))
                || !self.status.ticket_current(&ticket)
            {
                return Ok(source_scope_superseded());
            }
            if state.connection.as_ref().is_none_or(|connection| {
                (context.context_partition().is_some() && connection.authorized_until<context.deadline()) || !self
                    .status
                    .transport_reusable(&ticket, state.connection_scope, connection)
            }) {
                state.connection = None;
                state.connection_scope = None;
                let client = self
                    .client
                    .lock()
                    .map(|value| *value)
                    .unwrap_or(airk_core::usage::ClientFamily::Unknown);
                let connected = if let Some(selection) = &self.catalog_session {
                    Connection::connect_frontend_selected(
                        &self.root,
                        self.project,
                        client,
                        &self.status,
                        ticket,
                        tool == "airk_diagnostics",
                        Some(selection),
                    )
                    .await
                } else if let Some(session) = &self.selection_session {
                    Connection::connect_selection(
                        &self.root,
                        self.project,
                        client,
                        &self.status,
                        ticket,
                        session,
                    )
                    .await
                } else {
                    Connection::connect_frontend(
                        &self.root,
                        self.project,
                        client,
                        &self.status,
                        ticket,
                        tool == "airk_diagnostics",
                    )
                    .await
                };
                if source_expires.is_some_and(|until| Instant::now() >= until)
                    || observed
                        .is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence)))
                    || !self.status.ticket_scope_current(&ticket)
                {
                    return Ok(source_scope_superseded());
                }
                let connection = match connected {
                    Ok(connection) => connection,
                    Err(error) if error.is::<RetiredRequest>() => {
                        return Ok(source_scope_superseded());
                    }
                    Err(_) if tool == "airk_diagnostics" && !self.scoped() => {
                        return Ok(airk_supervisor::runtime_metadata::unavailable(
                            &self.root,
                            self.project,
                        )
                        .await);
                    }
                    Err(error) => {
                        if let Some(selection) = &self.catalog_session {
                            return Ok(scoped_setup_rejection(selection, &error));
                        }
                        if let Some(session) = &self.selection_session {
                            return Ok(selection_setup_rejection(session, &error));
                        }
                        return setup_rejection(&error).ok_or(error);
                    }
                };
                ticket = match self
                    .status
                    .authenticated_worker(ticket, connection.identity)
                {
                    Ok(ticket) => ticket,
                    Err(error) if error.is::<RetiredRequest>() => {
                        return Ok(source_scope_superseded());
                    }
                    Err(error) => return Err(error),
                };
                state.connection_scope = ticket.scope;
                state.connection = Some(connection);
            } else {
                ticket = match self.status.authenticated_worker(
                    ticket,
                    state
                        .connection
                        .as_ref()
                        .expect("checked evidence connection")
                        .identity,
                ) {
                    Ok(ticket) => ticket,
                    Err(error) if error.is::<RetiredRequest>() => {
                        return Ok(source_scope_superseded());
                    }
                    Err(error) => return Err(error),
                };
            }
            if source_expires.is_some_and(|until| Instant::now() >= until)
                || observed
                    .is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence)))
                || !self.status.ticket_current(&ticket)
            {
                return Ok(source_scope_superseded());
            }
            let receipt = if ticket.scope.is_some() {
                ticket.receipt
            } else {
                state.status_receipt
            };
            let connection = state
                .connection
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("project connection unavailable"))?;
            connection.restore_status_receipt(receipt);
            let _query = match self.status.admit_ticket(&ticket) {
                Ok(permit) => permit,
                Err(_) => return Ok(source_scope_superseded()),
            };
            let result = match native {
                Some(request) => connection.read_native_with_context(request, context).await,
                None => {
                    connection
                        .call_with_read_context(tool, arguments, context)
                        .await
                }
            };
            if result.is_ok() {
                let receipt = connection.status_receipt();
                state.status_receipt = receipt;
                if ticket.scope.is_some() {
                    self.status.adopt_receipt(
                        (ticket.scope, ticket.scalar_epoch, ticket.receipt),
                        receipt,
                    );
                }
            } else {
                state.connection = None;
                state.connection_scope = None;
            }
            // Logical invalidation does not retire authenticated transport authority.
            self.status.invalidate();
            result
        })
    }
}

pub fn tools(root: PathBuf, project: ProjectBinding) -> Arc<dyn airk_mcp::remote::RemoteTools> {
    source_tools(root, project)
}
pub(crate) fn source_tools(root: PathBuf, project: ProjectBinding) -> Arc<Backend> {
    Arc::new(Backend {
        root,
        project,
        state: ChannelPool::default(),
        state_waiters: Arc::new(tokio::sync::Semaphore::new(128)),
        latest_source: std::sync::Mutex::new(None),
        status: Arc::new(StatusOwner::default()),
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Cli),
        catalog_session: None,
        selection_session: None,
    })
}

/// Installed frontends use the elected broker and direct project transport.
/// They never initialize a local Kernel if installation, policy or health fails.
pub async fn bridge_installed(
    root: &Path,
    project: airk_core::service::ProjectBinding,
) -> Result<()> {
    bridge(
        airk_installer::runtime_install::installed_control(root).await?,
        project,
    )
    .await
}

pub async fn bridge(root: PathBuf, project: ProjectBinding) -> Result<()> {
    airk_mcp::remote::serve(tools(root, project)).await
}

pub(crate) fn source_tools_owned(
    root: PathBuf,
    project: ProjectBinding,
    status: Arc<StatusOwner>,
) -> Arc<Backend> {
    Arc::new(Backend {
        root,
        project,
        state: ChannelPool::default(),
        state_waiters: Arc::new(tokio::sync::Semaphore::new(128)),
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
        latest_source: std::sync::Mutex::new(None),
        status,
        catalog_session: None,
        selection_session: None,
    })
}

/// One index of a window's W4 grant read through its backing project's
/// worker. Every connect and reconnect calls `session.connect` (the
/// in-process admission), never the control-pipe brokerage (§3.2 step 4).
pub fn selection_tools(
    root: PathBuf,
    project: ProjectBinding,
    session: SelectionSession,
) -> Result<Arc<Backend>> {
    let status = Arc::new(StatusOwner::default());
    let semantic = (
        &session.window_selection,
        &session.scope_identity,
        &session.index_id,
    );
    status.scoped_scope(
        session.window_selection.namespace,
        project,
        airk_core::identity::framed_digest(
            b"airk/frontend-selection-session/1",
            &[&serde_json::to_vec(&semantic)?],
        ),
    );
    Ok(Arc::new(Backend {
        root,
        project,
        state: ChannelPool::default(),
        state_waiters: Arc::new(tokio::sync::Semaphore::new(128)),
        latest_source: Default::default(),
        status,
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Cli),
        catalog_session: None,
        selection_session: Some(session),
    }))
}

pub fn scoped_tools(
    root: PathBuf,
    project: ProjectBinding,
    selection: airk_core::service::ScopedConnectRequest,
) -> Result<Arc<dyn airk_mcp::remote::RemoteTools>> {
    Ok(scoped_backend(root, project, selection)?)
}

pub(crate) fn scoped_backend(
    root: PathBuf,
    project: ProjectBinding,
    selection: airk_core::service::ScopedConnectRequest,
) -> Result<Arc<Backend>> {
    selection.validate()?;
    let status = Arc::new(StatusOwner::default());
    let semantic = (
        &selection.schema,
        &selection.context_key,
        &selection.namespace,
        &selection.grant_id,
        &selection.expected_grant_revision,
        &selection.index_id,
        &selection.project_ids,
    );
    status.scoped_scope(
        selection.namespace,
        project,
        airk_core::identity::framed_digest(
            b"airk/frontend-catalog-session/1",
            &[&serde_json::to_vec(&semantic)?],
        ),
    );
    Ok(Arc::new(Backend {
        root,
        project,
        state: ChannelPool::default(),
        state_waiters: Arc::new(tokio::sync::Semaphore::new(128)),
        latest_source: Default::default(),
        status,
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Cli),
        catalog_session: Some(selection),
        selection_session: None,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_cache_is_success_only_expires_at_five_seconds_and_fences_recreated_registries() {
        let scope = airk_core::identity::Incarnation::from_bytes([1; 16]);
        let value = airk_mcp::unified::success(
            serde_json::json!({"version":"1.6.1","health":"ok","index_state":"available","publication":{"token":"exact-original-token"}}),
        );
        let cached = super::CachedStatus::new(scope, &value).unwrap();
        assert!(cached.agrees_with(&Ok(value.clone())));
        assert!(!cached.agrees_with(&Ok(airk_mcp::unified::success(
            serde_json::json!({"publication":{"token":"new-rebuilt-token"}})
        ))));
        assert!(!cached.agrees_with(&Ok(airk_mcp::unified::failure(
            "current_revision_expired",
            "retired token"
        ))));

        let reply = cached
            .reply(
                scope,
                cached.captured + std::time::Duration::from_millis(4999),
                true,
            )
            .unwrap();
        assert_eq!(reply["structuredContent"]["cache_age_ms"], 4999);
        assert_eq!(reply["structuredContent"]["cached"], true);
        assert_eq!(
            reply["structuredContent"]["publication"]["token"],
            "exact-original-token"
        );
        assert!(
            cached
                .reply(
                    scope,
                    cached.captured + std::time::Duration::from_secs(5),
                    true
                )
                .is_none()
        );
        assert!(
            cached
                .reply(
                    airk_core::identity::Incarnation::from_bytes([2; 16]),
                    cached.captured,
                    true
                )
                .is_none()
        );
        for state in ["unknown", "unavailable"] {
            assert!(
                super::CachedStatus::new(
                    scope,
                    &airk_mcp::unified::success(serde_json::json!({"index_state":state}))
                )
                .is_none()
            );
        }
        assert!(
            super::CachedStatus::new(scope, &airk_mcp::unified::failure("failure", "unavailable"))
                .is_none()
        );
    }
    #[tokio::test]
    async fn status_without_a_service_returns_unknown_counts_and_never_creates_an_index()
    -> anyhow::Result<()> {
        let control = tempfile::tempdir()?;
        let project = "ab".repeat(32).parse()?;
        let backend = super::tools(control.path().into(), project);
        let reply = backend
            .call(
                "airk_diagnostics".into(),
                serde_json::json!({"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_ne!(reply["isError"], true);
        let status: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(status["index_state"], "unavailable");
        assert!(status["publication"].is_null());
        assert!(status["files"].is_null());
        assert!(status["runtime"]["service"].is_null());
        assert!(
            !status["work"]["failed_observations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        let invalid = backend
            .call(
                "airk_status".into(),
                serde_json::json!({"project":"foreign"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_eq!(invalid["isError"], true);
        Ok(())
    }
    use super::*;

    #[tokio::test]
    async fn setup_retries_transient_admission_without_replaying_a_submitted_query() -> Result<()> {
        let mut attempts = 0;
        let result = retry_setup(
            || {
                attempts += 1;
                std::future::ready(if attempts < 3 {
                    Err(RetrySetup("starting").into())
                } else {
                    Ok(42)
                })
            },
            Duration::from_secs(1),
            4,
            Duration::from_millis(1),
        )
        .await?;
        assert_eq!(result, 42);
        assert_eq!(attempts, 3);
        let mut refused = 0;
        let result: Result<()> = retry_setup(
            || {
                refused += 1;
                std::future::ready(Err(anyhow::anyhow!("worker identity mismatch")))
            },
            Duration::from_secs(1),
            4,
            Duration::from_millis(1),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(refused, 1);
        Ok(())
    }
    #[tokio::test]
    async fn setup_has_finite_attempts_and_an_independent_deadline() {
        let mut attempts = 0;
        let result: Result<()> = retry_setup(
            || {
                attempts += 1;
                std::future::ready(Err(RetrySetup("starting").into()))
            },
            Duration::from_secs(1),
            3,
            Duration::from_millis(1),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts, 3);
        let result: Result<()> = retry_setup(
            std::future::pending,
            Duration::from_millis(10),
            3,
            Duration::from_millis(1),
        )
        .await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("before query submission")
        );
        assert!(!setup_retryable(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
        ));
        assert!(setup_retryable(
            &std::io::Error::from(std::io::ErrorKind::ConnectionReset).into()
        ));
    }

    #[test]
    fn only_temporary_pre_submission_setup_failures_get_a_retry_receipt() {
        let refusal = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused);
        assert!(!setup_retryable(&refusal));
        let envelope = setup_rejection(&refusal).unwrap();
        let detail: serde_json::Value =
            serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(envelope["isError"], true);
        assert_eq!(detail["error"]["code"], "scoped_read_unavailable");
        assert_eq!(detail["error"]["retryable"], false);
        for error in [
            anyhow::Error::new(RetrySetup("capacity")),
            std::io::Error::from(std::io::ErrorKind::ConnectionRefused).into(),
        ] {
            let envelope = setup_rejection(&error).unwrap();
            let detail: serde_json::Value =
                serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(envelope["isError"], true);
            assert_eq!(detail["error"]["code"], "direct_query_setup");
            assert_eq!(detail["error"]["query_submitted"], false);
            assert_eq!(detail["error"]["fresh_brokerage_required"], true);
            assert_eq!(detail["error"]["retryable"], true);
        }
        for error in [
            anyhow::anyhow!("identity mismatch"),
            anyhow::anyhow!("query outcome unknown"),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
        ] {
            assert!(setup_rejection(&error).is_none());
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_direct_exchange_is_not_reused_and_known_deadlines_precede_submission()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let endpoint = local_ipc::endpoint(&root, "direct-client-lifetime-test")?;
        let mut listener = local_ipc::Listener::bind(&endpoint)?;
        let sent = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let peer_sent = sent.clone();
        let peer_release = release.clone();
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let query: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .ok_or_else(|| anyhow::anyhow!("test query not submitted"))?;
            ensure!(query.tool == "airk_status", "unexpected fixture request");
            peer_sent.notify_one();
            peer_release.notified().await;
            let _ = local_ipc::write_message(
                &mut stream,
                &serde_json::json!({"fixture":"complete"}),
                Duration::from_secs(5),
            )
            .await;
            Ok::<_, anyhow::Error>(())
        });
        let stream = local_ipc::connect(&endpoint).await?;
        let now = Instant::now();
        let mut connection = Connection {
            stream: Some(stream),
            identity: airk_core::service::WorkerIdentity {
                service: airk_core::identity::Incarnation::from_bytes([1; 16]),
                worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
                project: "ab".repeat(32).parse()?,
                fencing_generation: 1,
            },
            authorized_until: now + Duration::from_secs(55),
            last_finished: now,
            healthy: true,
            scoped: false,
            read_scope: None,
            read_attempt_supported: false,
            status_receipt: Default::default(),
        };
        assert!(connection.reusable(now + Duration::from_secs(24)));
        assert!(!connection.reusable(now + Duration::from_secs(25)));
        assert!(!connection.reusable(now + Duration::from_secs(56)));
        {
            let request = connection.call("airk_status".into(), Default::default());
            tokio::pin!(request);
            tokio::select! {
                _=sent.notified()=>{},
                result=&mut request=>anyhow::bail!("fixture exchange completed before cancellation: {result:?}"),
            }
        }
        assert!(!connection.reusable(Instant::now()));
        drop(connection);
        release.notify_one();
        peer.await??;
        Ok(())
    }
}

#[cfg(test)]
mod selection_session_tests {
    use super::*;
    use airk_mcp::remote::RemoteTools as _;
    use serde_json::json;

    async fn selection_unified_setup_case(tool: &str) -> Result<()> {
        let root = tempfile::tempdir()?;
        let selection = serde_json::from_value(json!({
            "namespace": "0a".repeat(16), "expected_catalog_revision": 12,
            "index_ids": ["idx_01010101010101010101010101010101"],
            "project_ids": ["prj_02020202020202020202020202020202"],
        }))?;
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = calls.clone();
        let project = "44".repeat(32).parse()?;
        let backend = selection_tools(
            root.path().into(),
            project,
            SelectionSession {
                window_selection: selection,
                scope_identity: airk_core::identity::Sha256Digest::from_bytes([0x5c; 32]),
                index_id: "idx_01010101010101010101010101010101".parse()?,
                connect: Arc::new(move || {
                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Box::pin(async { anyhow::bail!("private nonce and source path must not leak") })
                }),
            },
        )?;
        let arguments = if tool.ends_with("help") {
            json!({"tool":"airk_search"})
        } else {
            json!({})
        };
        let expected = backend
            .call(tool.into(), arguments.as_object().unwrap().clone())
            .await?;
        assert_eq!(expected["isError"], true, "{expected}");
        let requested = expected["structuredContent"]["requested_scope"]["selection"].clone();
        assert_eq!(requested["scope_identity"], "5c".repeat(32));
        assert!(requested.get("window_scope_selection").is_some());
        let unified = airk_mcp::unified::Unified::new(
            vec![airk_mcp::unified::Project {
                binding: project.to_string(),
                name: "selected".into(),
                collection: "fixture".into(),
                backend,
            }],
            Some(project.to_string()),
            None,
        )?;
        let actual = unified
            .call(tool.into(), arguments.as_object().unwrap().clone())
            .await?;
        assert_eq!(
            actual, expected,
            "single-index selection must preserve the complete native result for {tool}"
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "one connector invocation per native call; no static help or replay"
        );
        Ok(())
    }

    #[tokio::test]
    async fn selection_unified_help_preserves_native_setup_refusal() -> Result<()> {
        selection_unified_setup_case("airk_help").await
    }
    #[tokio::test]
    async fn selection_unified_status_preserves_native_setup_refusal() -> Result<()> {
        selection_unified_setup_case("airk_status").await
    }
    #[tokio::test]
    async fn selection_unified_diagnostics_preserves_native_setup_refusal() -> Result<()> {
        selection_unified_setup_case("airk_diagnostics").await
    }
    #[tokio::test]
    async fn selection_unified_parse_preserves_native_setup_refusal() -> Result<()> {
        selection_unified_setup_case("airk_parse").await
    }

    /// N3 and §3.2 step 5: a refused selection connect echoes the window
    /// selection and its scope identity only. The error's text (which could
    /// name anything) and the grant's nonce or service instance never
    /// appear.
    #[test]
    fn a_selection_setup_failure_echoes_only_the_selection_and_its_identity() {
        let selection = json!({
            "namespace": "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
            "expected_catalog_revision": 12,
            "index_ids": ["idx_01010101010101010101010101010101"],
            "project_ids": ["prj_02020202020202020202020202020202"],
        });
        let session = SelectionSession {
            window_selection: serde_json::from_value(selection.clone()).unwrap(),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([0x5c; 32]),
            index_id: "idx_01010101010101010101010101010101".parse().unwrap(),
            connect: Arc::new(|| -> SelectionGrantFuture {
                Box::pin(async {
                    Result::<airk_core::service::ConnectionGrant>::Err(anyhow::anyhow!("unused"))
                })
            }),
        };
        let nonce = "91".repeat(16);
        let instance = "e1".repeat(16);
        let error = anyhow::anyhow!("grant_nonce {nonce} service_instance {instance}");
        let body = selection_setup_rejection(&session, &error);
        let text = body.to_string();
        for secret in [
            nonce.as_str(),
            instance.as_str(),
            "grant_nonce",
            "service_instance",
        ] {
            assert!(!text.contains(secret), "{secret}: {text}");
        }
        let expected = json!({
            "error": {"code": "scoped_connection_unavailable", "retryable": false,
                "query_submitted": false},
            "requested_scope": {"confirmed": false, "selection": {
                "window_scope_selection": selection, "scope_identity": "5c".repeat(32)}},
        });
        assert_eq!(body["structuredContent"], expected);
        assert_eq!(body["isError"], true);
        let echoed: serde_json::Value =
            serde_json::from_str(body["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(echoed, expected);
    }
}

#[cfg(test)]
mod source_observation_tests {
    use super::*;

    #[tokio::test]
    async fn queued_channel_uses_current_capacity_after_a_live_increase() {
        let pool = ChannelPool::default();
        let mut held = Vec::new();
        for _ in 0..4 {
            held.push(pool.acquire(4).await);
        }
        let mut queued = Box::pin(pool.acquire(4));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(queued.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        // The new caller publishes the increased limit before either queued
        // caller can obtain a channel; the first four channels remain leased.
        let mut increased = Box::pin(pool.acquire(16));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(increased.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        let old_policy = queued.await;
        let new_policy = increased.await;
        assert_eq!(pool.capacity.available_permits(), 10);
        drop((old_policy, new_policy, held));
        assert_eq!(pool.capacity.available_permits(), 16);
    }

    fn fence(namespace: u8, revision: u64) -> SourceFence {
        SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([namespace; 16]),
            revision,
            project: "ab".repeat(32).parse().unwrap(),
            fencing_generation: 1,
        }
    }

    #[test]
    fn revision_namespace_and_generation_are_cache_scope_not_mutable_authority() {
        let old = fence(1, 9);
        let value = airk_mcp::unified::success(serde_json::json!({
            "index_state":"available", "publication":{"token":"saved"}
        }));
        let cached = CachedStatus::new(CacheScope::Source(old), &value).unwrap();
        for changed in [
            fence(2, 9),
            fence(1, 10),
            SourceFence {
                fencing_generation: 2,
                ..old
            },
        ] {
            assert!(
                cached
                    .reply(CacheScope::Source(changed), cached.captured, true)
                    .is_none()
            );
        }
        assert!(
            cached
                .reply(
                    CacheScope::Source(old),
                    cached.captured + Duration::from_secs(5),
                    true
                )
                .is_none()
        );
    }

    #[tokio::test]
    async fn replaced_source_observation_is_unsubmitted_without_evidence_mutex() -> Result<()> {
        let control = tempfile::tempdir()?;
        let old = fence(1, 9);
        let backend = source_tools(control.path().into(), old.project);
        backend.status.source_scope(fence(1, 10));
        let guard = backend.state.lock().await;
        let reply = backend
            .call_observed("airk_status".into(), serde_json::Map::new(), old)
            .await?;
        let detail: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(detail["error"]["query_submitted"], false);
        assert!(guard.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn observed_cache_hit_does_not_reopen_registry_and_retains_age() -> Result<()> {
        let control = tempfile::tempdir()?;
        let scope = fence(1, 9);
        let backend = source_tools(control.path().into(), scope.project);
        let value = airk_mcp::unified::success(serde_json::json!({
            "index_state":"available", "publication":{"token":"saved"}
        }));
        let mut cached = CachedStatus::new(CacheScope::Source(scope), &value).unwrap();
        cached.captured -= Duration::from_secs(1);
        let timestamp = cached.observed_unix_ms;
        backend.status.seed(cached);
        // Hold the same mutex that a long evidence query owns. Scalar cache
        // reads must complete without acquiring it or reopening the registry.
        let guard = backend.state.lock().await;
        let reply = backend
            .call_observed("airk_status".into(), serde_json::Map::new(), scope)
            .await?;
        let body: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(body["cached"], true);
        assert_eq!(body["observed_unix_ms"], timestamp);
        assert!(body["cache_age_ms"].as_u64().unwrap() >= 1000);
        assert!(guard.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn frontend_close_retires_bounded_evidence_waiters_without_submitting() -> Result<()> {
        let control = tempfile::tempdir()?;
        let scope = fence(1, 9);
        let backend = source_tools(control.path().into(), scope.project);
        backend.status.source_scope(scope);
        let mut held = Vec::new();
        for _ in 0..16 {
            held.push(backend.state.lock().await);
        }
        let mut pending: Vec<_> = (0..128)
            .map(|_| {
                Box::pin(backend.call_observed("airk_read_file".into(), Default::default(), scope))
            })
            .collect();
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        for call in &mut pending {
            assert!(call.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(backend.state_waiters.available_permits(), 0);
        let refused = backend
            .call_observed("airk_read_file".into(), Default::default(), scope)
            .await?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                refused["content"][0]["text"].as_str().unwrap()
            )?["error"]["query_submitted"],
            false
        );
        backend.status.close();
        for call in pending {
            let reply = call.await?;
            let body: serde_json::Value =
                serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
            assert_eq!(body["error"]["reason"], "draining");
            assert_eq!(body["error"]["query_submitted"], false);
        }
        assert_eq!(backend.state_waiters.available_permits(), 128);
        assert!(held.iter().all(|state| state.connection.is_none()));
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }
}

#[cfg(test)]
mod source_cache_return_age_tests {
    use super::*;
    #[tokio::test]
    async fn expired_source_observation_cannot_hit_fresh_status_after_backend_wait() -> Result<()> {
        let control = tempfile::tempdir()?;
        let fence = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([81; 16]),
            revision: 1,
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        let backend = source_tools(control.path().into(), fence.project);
        let state = backend.state.lock().await;
        backend.status.seed(
            CachedStatus::new(
                CacheScope::Source(fence),
                &airk_mcp::unified::success(
                    serde_json::json!({"index_state":"available","publication":{"token":"saved"}}),
                ),
            )
            .unwrap(),
        );
        let reply = backend
            .call_observed_until(
                "airk_status".into(),
                serde_json::Map::new(),
                fence,
                Instant::now(),
            )
            .await?;
        assert!(state.connection.is_none());
        drop(state);
        let detail: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(detail["error"]["query_submitted"], false);
        assert_eq!(detail["error"]["retryable"], true);
        assert!(backend.state.lock().await.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }
}

#[cfg(test)]
mod source_brokerage_ipc_tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::{ConnectionGrant, ControlRequest, ServiceHealth, WorkerIdentity},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Peer {
        broker: tokio::task::JoinHandle<Result<()>>,
        worker: tokio::task::JoinHandle<Result<()>>,
        broker_began: Arc<tokio::sync::Notify>,
        broker_release: Arc<tokio::sync::Notify>,
        query_began: Arc<tokio::sync::Notify>,
        query_release: Arc<tokio::sync::Notify>,
        queries: Arc<AtomicUsize>,
        receipt: airk_mcp::response::StatusReceipt,
        control_calls: Arc<AtomicUsize>,
        handshakes: Arc<AtomicUsize>,
        received_receipts: Arc<std::sync::Mutex<Vec<airk_mcp::response::StatusReceipt>>>,
        received_options: Arc<std::sync::Mutex<Vec<serde_json::Map<String, serde_json::Value>>>>,
    }
    fn fence(generation: u64, revision: u64) -> SourceFence {
        SourceFence {
            namespace: Incarnation::from_bytes([101; 16]),
            revision,
            project: "ab".repeat(32).parse().unwrap(),
            fencing_generation: generation,
        }
    }
    // Real same-user control and worker transports; the peer implements only
    // Health/Connect/grant acceptance and records actual submitted Query frames.
    fn peer(
        root: &Path,
        old: SourceFence,
        generation: u64,
        pause_broker: bool,
        pause_reply: bool,
    ) -> Result<Peer> {
        peer_with_options(root, old, generation, pause_broker, pause_reply, false)
    }
    fn peer_with_options(
        root: &Path,
        old: SourceFence,
        generation: u64,
        pause_broker: bool,
        pause_reply: bool,
        render_options: bool,
    ) -> Result<Peer> {
        local_ipc::prepare_root(root)?;
        let mut control = local_ipc::Listener::bind(&local_ipc::endpoint(root, "control")?)?;
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([102; 16]),
            worker: Incarnation::from_bytes([103; 16]),
            project: old.project,
            fencing_generation: generation,
        };
        let role = protocol::role(identity.worker);
        let mut worker = local_ipc::Listener::bind(&local_ipc::endpoint(root, &role)?)?;
        let grant = ConnectionGrant {
            scope: None,
            protocol: PROTOCOL_VERSION,
            identity,
            caller: local_ipc::caller_identity()?,
            token: "cd".repeat(32).parse()?,
            ttl_ms: 5000,
            client: airk_core::usage::ClientFamily::Unknown,
        };
        let health = ServiceHealth {
            catalog_clock: None,
            storage_formats: None,
            build: None,
            build_error: None,
            logs: airk_core::service_logs::LogTelemetry {
                available: false,
                dropped_records: 0,
                write_failures: 0,
            },
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL_VERSION,
            instance: identity.service,
            process_id: std::process::id(),
            executable_sha256: "00".repeat(32),
            runtime_sha256: None,
            user: local_ipc::current_user()?,
            uptime_ms: 0,
            policy: airk_kernel::service_broker::Broker::new(identity.service, Default::default())?
                .status(Instant::now()),
        };
        let broker_began = Arc::new(tokio::sync::Notify::new());
        let broker_release = Arc::new(tokio::sync::Notify::new());
        let began = broker_began.clone();
        let release = broker_release.clone();
        let control_calls = Arc::new(AtomicUsize::new(0));
        let counted_control = control_calls.clone();
        let broker = tokio::spawn(async move {
            let mut stream = control.accept().await?;
            let request: ControlRequest =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("health request missing"))?;
            counted_control.fetch_add(1, Ordering::SeqCst);
            ensure!(
                matches!(request.method, ControlMethod::Health),
                "unexpected control request"
            );
            local_ipc::write_message(
                &mut stream,
                &ControlResponse::Health {
                    observation: Box::new(health),
                },
                Duration::from_secs(5),
            )
            .await?;
            drop(stream);
            let mut stream = control.accept().await?;
            let request: ControlRequest =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Connect missing"))?;
            ensure!(
                matches!(request.method,ControlMethod::Connect { project } if project==identity.project),
                "unexpected brokerage"
            );
            counted_control.fetch_add(1, Ordering::SeqCst);
            began.notify_one();
            if pause_broker {
                release.notified().await;
            }
            local_ipc::write_message(
                &mut stream,
                &ControlResponse::Connection { role, grant },
                Duration::from_secs(5),
            )
            .await?;
            Ok(())
        });
        let query_began = Arc::new(tokio::sync::Notify::new());
        let query_release = Arc::new(tokio::sync::Notify::new());
        let queries = Arc::new(AtomicUsize::new(0));
        let counted = queries.clone();
        let began = query_began.clone();
        let release = query_release.clone();
        let receipt: airk_mcp::response::StatusReceipt =
            serde_json::from_value(serde_json::json!({"freshness":"ef".repeat(32)}))?;
        let received_receipts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded_receipts = received_receipts.clone();
        let received_options = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded_options = received_options.clone();
        let handshakes = Arc::new(AtomicUsize::new(0));
        let counted_handshakes = handshakes.clone();
        let worker = tokio::spawn(async move {
            let mut stream = worker.accept().await?;
            let observed: ConnectionGrant =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("grant missing"))?;
            ensure!(observed.identity == identity, "grant identity changed");
            counted_handshakes.fetch_add(1, Ordering::SeqCst);
            local_ipc::write_message(&mut stream, &Accepted { identity }, Duration::from_secs(5))
                .await?;
            while let Some(query) =
                local_ipc::read_message::<_, Query>(&mut stream, Duration::from_secs(5)).await?
            {
                recorded_receipts.lock().unwrap().push(query.status_receipt);
                recorded_options
                    .lock()
                    .unwrap()
                    .push(query.arguments.clone());
                counted.fetch_add(1, Ordering::SeqCst);
                began.notify_one();
                if pause_reply {
                    release.notified().await;
                }
                // Distinct wire fixtures exercise the owner's rendering key;
                // native response-policy tests establish the real generation rule.
                let mut rendered = serde_json::json!({"index_state":"available","publication":{"token":"original-query"}});
                if render_options
                    && query
                        .arguments
                        .get("response_mode")
                        .and_then(|value| value.as_str())
                        == Some("full")
                {
                    rendered["generation"] = 17.into();
                }
                local_ipc::write_message(
                    &mut stream,
                    &protocol::QueryResult {
                        read_admission: None,
                        result: airk_mcp::unified::success(rendered),
                        status_receipt: receipt,
                        rebind: false,
                    },
                    Duration::from_secs(5),
                )
                .await?;
            }
            Ok(())
        });
        Ok(Peer {
            broker,
            worker,
            broker_began,
            broker_release,
            query_began,
            query_release,
            queries,
            receipt,
            received_receipts,
            received_options,
            control_calls,
            handshakes,
        })
    }
    fn body(reply: &serde_json::Value) -> Result<serde_json::Value> {
        Ok(serde_json::from_str(
            reply["content"][0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("reply body missing"))?,
        )?)
    }
    #[tokio::test]
    async fn superseded_during_actual_brokerage_never_submits_query_or_installs_receipt()
    -> Result<()> {
        for newer in [fence(1, 10), fence(2, 9)] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            let old = fence(1, 9);
            let peer = peer(&root, old, 2, true, false)?;
            let backend = source_tools(root.clone(), old.project);
            backend.status.source_scope(old);
            let mut request = backend.call_observed("airk_status".into(), Default::default(), old);
            tokio::select! { _=peer.broker_began.notified()=>{}, result=&mut request=>anyhow::bail!("broker did not block: {result:?}") }
            assert!(backend.accept_source_fence(newer));
            backend.status.source_scope(newer);
            peer.broker_release.notify_one();
            let reply = request.await?;
            {
                let state = backend.state.lock().await;
                assert!(state.connection.is_none());
                assert!(backend.status.cache_empty());
                assert_eq!(state.status_receipt, Default::default());
            }
            drop(backend);
            peer.broker.await??;
            peer.worker.await??;
            assert_eq!(peer.queries.load(Ordering::SeqCst), 0);
            assert_eq!(body(&reply)?["error"]["query_submitted"], false);
            assert_eq!(body(&reply)?["error"]["retryable"], true);
        }
        Ok(())
    }
    #[tokio::test]
    async fn cold_newer_grant_submits_but_newer_observed_epoch_discards_retained_channel()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(0, 9);
        let peer = peer(&root, old, 2, false, false)?;
        let backend = source_tools(root.clone(), old.project);
        backend.status.source_scope(old); // actual RootNative selector binds the shared owner before dispatch
        let first = backend
            .call_observed("airk_diagnostics".into(), Default::default(), old)
            .await?;
        assert_eq!(body(&first)?["index_state"], "available");
        peer.broker.await??; // No broker remains; a known stale channel must not hide that fact.
        {
            let state = backend.state.lock().await;
            let connection = state.connection.as_ref().unwrap();
            assert_eq!(connection.identity.fencing_generation, 2);
            assert!(connection.reusable_for_source(old, Instant::now()));
            assert!(connection.reusable_for_source(fence(2, 10), Instant::now()));
            assert!(!connection.reusable_for_source(fence(3, 11), Instant::now()));
        }
        backend.status.source_scope(fence(3, 11));
        let second = backend
            .call_observed("airk_diagnostics".into(), Default::default(), fence(3, 11))
            .await?;
        assert!(
            body(&second)?["publication"].is_null(),
            "fresh brokerage failure must not report retained old publication"
        );
        assert_eq!(
            backend.state.lock().await.status_receipt,
            peer.receipt,
            "logical complete receipt survives channel invalidation"
        );
        drop(backend);
        peer.worker.await??;
        assert_eq!(
            peer.queries.load(Ordering::SeqCst),
            1,
            "no query may use the known superseded channel"
        );
        Ok(())
    }
    #[tokio::test]
    async fn submitted_old_reply_preserves_own_receipt_but_cannot_repopulate_source_status_cache()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(1, 9);
        let peer = peer(&root, old, 1, false, true)?;
        let backend = source_tools(root.clone(), old.project);
        backend.status.source_scope(old); // actual RootNative selector binds the shared owner before dispatch
        let mut request = backend.call_observed("airk_read_file".into(), Default::default(), old);
        tokio::select! { _=peer.query_began.notified()=>{}, result=&mut request=>anyhow::bail!("query did not block: {result:?}") }
        assert!(backend.accept_source_fence(fence(2, 10)));
        backend.status.source_scope(fence(2, 10));
        peer.query_release.notify_one();
        let reply = request.await?;
        assert_eq!(body(&reply)?["publication"]["token"], "original-query");
        {
            let state = backend.state.lock().await;
            assert!(backend.status.cache_empty());
            assert_eq!(state.status_receipt, peer.receipt);
        }
        drop(backend);
        peer.broker.await??;
        peer.worker.await??;
        assert_eq!(
            peer.queries.load(Ordering::SeqCst),
            1,
            "submitted reads must never replay"
        );
        Ok(())
    }
    #[tokio::test]
    async fn submitted_obsolete_status_never_adopts_receipt_or_caches_or_replays() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(1, 9);
        let peer = peer(&root, old, 1, false, true)?;
        let backend = source_tools(root, old.project);
        backend.status.source_scope(old);
        let mut request = backend.call_observed("airk_status".into(), Default::default(), old);
        tokio::select! {_=peer.query_began.notified()=>{},result=&mut request=>anyhow::bail!("status query did not block: {result:?}")}
        let replacement = fence(2, 10);
        assert!(backend.accept_source_fence(replacement));
        backend.status.source_scope(replacement);
        peer.query_release.notify_one();
        let reply = request.await?;
        assert_eq!(
            body(&reply)?["error"]["code"],
            "status_observation_superseded"
        );
        assert_eq!(body(&reply)?["error"]["query_submitted"], true);
        assert_eq!(backend.status.receipt(), Default::default());
        assert!(backend.status.cache_empty());
        backend.status.shutdown().await?;
        drop(backend);
        peer.broker.await??;
        peer.worker.await??;
        assert_eq!(peer.queries.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn submitted_status_scalar_invalidation_returns_original_snapshot_without_replay()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let observed = fence(1, 9);
        let peer = peer(&root, observed, 1, false, true)?;
        let backend = source_tools(root, observed.project);
        backend.status.source_scope(observed);
        let mut request = backend.call_observed("airk_status".into(), Default::default(), observed);
        tokio::select! {
            _ = peer.query_began.notified() => {},
            result = &mut request => anyhow::bail!("status query did not block: {result:?}")
        }
        // Model an evidence read finishing while the actual status Query waits.
        backend.status.invalidate();
        peer.query_release.notify_one();
        let reply = request.await?;
        let result = body(&reply)?;
        assert_eq!(reply["isError"], false, "{reply}");
        assert_eq!(reply["structuredContent"], result);
        assert_eq!(result["index_state"], "available");
        assert_eq!(result["publication"]["token"], "original-query");
        assert_eq!(result["status_observation"]["superseded"], true);
        assert!(result["observed_unix_ms"].is_u64());
        assert_eq!(result["cached"], false);
        assert_eq!(backend.status.receipt(), Default::default());
        assert!(backend.status.cache_empty());
        assert_eq!(peer.queries.load(Ordering::SeqCst), 1);
        backend.status.shutdown().await?;
        drop(backend);
        peer.broker.await??;
        peer.worker.await??;
        Ok(())
    }

    #[tokio::test]
    async fn pinned_evidence_first_receipt_is_sent_on_first_independent_status_connection()
    -> Result<()> {
        use airk_core::service::ProjectRegistration;
        use airk_store::service::registry::ServiceRegistry;
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let project = ProjectRegistration::new(
            owner.path().join("source").to_string_lossy().into_owned(),
            owner.path().join("data").to_string_lossy().into_owned(),
            "fixture".into(),
        )?;
        let namespace = Incarnation::from_bytes([101; 16]);
        let mut registry = ServiceRegistry::open(&root, namespace)?;
        registry.register(&project)?;
        drop(registry);
        let mut observed = fence(0, 1);
        observed.project = project.binding;
        let first = peer(&root, observed, 1, false, false)?;
        let backend = source_tools(root.clone(), project.binding);
        let reply = airk_mcp::remote::RemoteTools::call(
            &*backend,
            "airk_read_file".into(),
            Default::default(),
        )
        .await?;
        assert_eq!(body(&reply)?["publication"]["token"], "original-query");
        assert_eq!(backend.status.receipt(), first.receipt);
        backend.state.lock().await.connection = None;
        first.broker.await??;
        first.worker.await??;
        let second = peer(&root, observed, 1, false, false)?;
        let reply = airk_mcp::remote::RemoteTools::call(
            &*backend,
            "airk_status".into(),
            Default::default(),
        )
        .await?;
        assert_eq!(body(&reply)?["index_state"], "available");
        assert_eq!(&*second.received_receipts.lock().unwrap(), &[first.receipt]);
        backend.status.shutdown().await?;
        drop(backend);
        second.broker.await??;
        second.worker.await??;
        Ok(())
    }

    #[tokio::test]
    async fn namespace_replaced_during_actual_status_brokerage_never_submits_or_carries_receipt()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(1, 9);
        let peer = peer(&root, old, 1, true, false)?;
        let backend = source_tools(root, old.project);
        backend.status.source_scope(old);
        // A previously observed logical receipt belongs only to this old scope.
        backend.status.observed_receipt(peer.receipt);
        let mut request = backend.call_observed("airk_status".into(), Default::default(), old);
        tokio::select! {_=peer.broker_began.notified()=>{},result=&mut request=>anyhow::bail!("status brokerage did not block: {result:?}")}
        let mut replacement = old;
        replacement.namespace = Incarnation::from_bytes([104; 16]);
        backend.status.source_scope(replacement); // serialized selector accepted replacement namespace
        peer.broker_release.notify_one();
        let reply = request.await?;
        assert_eq!(body(&reply)?["error"]["query_submitted"], false);
        assert_eq!(backend.status.receipt(), Default::default());
        assert!(backend.status.cache_empty());
        backend.status.shutdown().await?;
        drop(backend);
        peer.broker.await??;
        peer.worker.await??;
        assert_eq!(peer.queries.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn consecutive_status_misses_reuse_healthy_private_channel_after_scalar_invalidation()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let observed = fence(0, 9);
        let peer = peer(&root, observed, 2, false, false)?;
        let backend = source_tools(root, observed.project);
        backend.status.source_scope(observed);
        let first = backend
            .call_observed("airk_status".into(), Default::default(), observed)
            .await?;
        assert_eq!(body(&first)?["index_state"], "available");
        peer.broker.await??; // No broker remains: a second miss must use the valid private channel.
        backend.status.invalidate();
        let second = backend
            .call_observed("airk_status".into(), Default::default(), observed)
            .await?;
        assert_eq!(body(&second)?["index_state"], "available");
        assert_eq!(body(&second)?["cached"], false);
        assert_eq!(
            peer.control_calls.load(Ordering::SeqCst),
            2,
            "one Health and one Connect"
        );
        assert_eq!(
            peer.handshakes.load(Ordering::SeqCst),
            1,
            "one grant/Accepted exchange"
        );
        assert_eq!(
            peer.queries.load(Ordering::SeqCst),
            2,
            "one submitted Query per miss"
        );
        backend.status.shutdown().await?;
        drop(backend);
        peer.worker.await??;
        Ok(())
    }

    #[tokio::test]
    async fn private_status_channel_refuses_known_newer_worker_and_expired_authorization()
    -> Result<()> {
        for expired in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            let old = fence(0, 9);
            let peer = peer(&root, old, 2, false, false)?;
            let backend = source_tools(root, old.project);
            backend.status.source_scope(old);
            backend
                .call_observed("airk_status".into(), Default::default(), old)
                .await?;
            peer.broker.await??;
            let next = if expired {
                backend.status.expire_channel();
                old
            } else {
                let next = fence(3, 10);
                backend.status.source_scope(next);
                next
            };
            backend.status.invalidate();
            let reply = backend
                .call_observed("airk_status".into(), Default::default(), next)
                .await?;
            assert_eq!(
                body(&reply)?["index_error"]["code"],
                "index_worker_unavailable"
            );
            assert!(backend.status.channel_empty().await);
            assert_eq!(
                peer.queries.load(Ordering::SeqCst),
                1,
                "known stale channel submitted a Query"
            );
            backend.status.shutdown().await?;
            drop(backend);
            peer.worker.await??;
        }
        Ok(())
    }
    #[tokio::test]
    async fn uncertain_private_status_exchange_is_discarded_without_replay_then_fresh_call_can_recover()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let observed = fence(0, 9);
        let first = peer(&root, observed, 2, false, false)?;
        let backend = source_tools(root.clone(), observed.project);
        backend.status.source_scope(observed);
        backend
            .call_observed("airk_status".into(), Default::default(), observed)
            .await?;
        first.broker.await??;
        first.worker.abort();
        assert!(first.worker.await.unwrap_err().is_cancelled());
        backend.status.invalidate();
        assert!(
            backend
                .call_observed("airk_status".into(), Default::default(), observed)
                .await
                .is_err()
        );
        assert!(backend.status.channel_empty().await);
        assert_eq!(first.queries.load(Ordering::SeqCst), 1);
        let second = peer(&root, observed, 2, false, false)?;
        let reply = backend
            .call_observed("airk_status".into(), Default::default(), observed)
            .await?;
        assert_eq!(body(&reply)?["index_state"], "available");
        assert_eq!(second.queries.load(Ordering::SeqCst), 1);
        backend.status.shutdown().await?;
        drop(backend);
        second.broker.await??;
        second.worker.await??;
        Ok(())
    }
    #[tokio::test]
    async fn source_and_pinned_status_cache_keys_rendered_options_and_reuses_transport()
    -> Result<()> {
        use airk_core::service::ProjectRegistration;
        use airk_store::service::registry::ServiceRegistry;
        for pinned in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            local_ipc::prepare_root(&root)?;
            let mut observed = fence(0, 9);
            if pinned {
                let project = ProjectRegistration::new(
                    "/option-source".into(),
                    "/option-data".into(),
                    "fixture".into(),
                )?;
                let mut registry = ServiceRegistry::open(&root, observed.namespace)?;
                registry.register(&project)?;
                observed.project = project.binding;
            }
            let peer = peer_with_options(&root, observed, 2, false, false, true)?;
            let backend = source_tools(root, observed.project);
            if !pinned {
                backend.status.source_scope(observed);
            }
            let options = [
                serde_json::json!({}),
                serde_json::json!({"response_mode":"full"}),
                serde_json::json!({}),
                serde_json::json!({"include_status":true}),
                serde_json::json!({"include_status":true}),
            ];
            let mut captured = None;
            for (index, options) in options.iter().enumerate() {
                let arguments = options.as_object().unwrap().clone();
                let reply = if pinned {
                    airk_mcp::remote::RemoteTools::call(&*backend, "airk_status".into(), arguments)
                        .await?
                } else {
                    backend
                        .call_observed("airk_status".into(), arguments, observed)
                        .await?
                };
                let value = body(&reply)?;
                assert_eq!(value["index_state"], "available");
                assert_eq!(
                    value.get("generation").is_some(),
                    index == 1,
                    "wrong rendered mode at request {index}"
                );
                assert_eq!(
                    value["cached"],
                    index == 4,
                    "option changes must genuinely observe, same options must hit"
                );
                if index == 3 {
                    captured = Some(value["observed_unix_ms"].clone());
                }
                if index == 4 {
                    assert_eq!(Some(value["observed_unix_ms"].clone()), captured);
                }
            }
            assert_eq!(
                &*peer.received_options.lock().unwrap(),
                &options[..4]
                    .iter()
                    .map(|value| value.as_object().unwrap().clone())
                    .collect::<Vec<_>>()
            );
            assert_eq!(peer.control_calls.load(Ordering::SeqCst), 2);
            assert_eq!(peer.handshakes.load(Ordering::SeqCst), 1);
            assert_eq!(peer.queries.load(Ordering::SeqCst), 4);
            backend.status.shutdown().await?;
            drop(backend);
            peer.broker.await??;
            peer.worker.await??;
        }
        Ok(())
    }
    #[tokio::test]
    async fn newly_observed_namespace_never_submits_on_fresh_old_evidence_channel() -> Result<()> {
        for pinned in [true, false] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            let old = fence(1, 9);
            let peer = peer(&root, old, 1, false, false)?;
            let backend = source_tools(root, old.project);
            if pinned {
                backend.status.seed(CachedStatus::new(CacheScope::Registered(old),&airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":"old"}}))).unwrap());
            } else {
                backend.status.source_scope(old);
            }
            if pinned {
                airk_mcp::remote::RemoteTools::call(
                    &*backend,
                    "airk_read_file".into(),
                    Default::default(),
                )
                .await?;
            } else {
                backend
                    .call_observed("airk_read_file".into(), Default::default(), old)
                    .await?;
            }
            peer.broker.await??;
            assert_eq!(peer.queries.load(Ordering::SeqCst), 1);
            assert!(
                backend
                    .state
                    .lock()
                    .await
                    .connection
                    .as_ref()
                    .unwrap()
                    .reusable(Instant::now())
            );
            let mut replacement = old;
            replacement.namespace = Incarnation::from_bytes([104; 16]);
            // The status observation has authenticated the replacement domain.
            // Keep the old worker transport alive to prove known-fence refusal,
            // not incidental EOF/idle expiry or a retry after an uncertain write.
            if pinned {
                backend.status.seed(CachedStatus::new(CacheScope::Registered(replacement),&airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":"replacement"}}))).unwrap());
            } else {
                backend.status.source_scope(replacement);
            }
            let _result = if pinned {
                airk_mcp::remote::RemoteTools::call(
                    &*backend,
                    "airk_diagnostics".into(),
                    Default::default(),
                )
                .await
            } else {
                backend
                    .call_observed("airk_read_file".into(), Default::default(), old)
                    .await
            };
            assert_eq!(
                peer.queries.load(Ordering::SeqCst),
                1,
                "known replacement must prevent any additional old-channel Query"
            );
            backend.status.shutdown().await?;
            drop(backend);
            peer.worker.await??;
        }
        Ok(())
    }
}

#[cfg(test)]
mod shared_authority_ipc_tests;
