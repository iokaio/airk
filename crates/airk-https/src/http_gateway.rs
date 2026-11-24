// SPDX-License-Identifier: Apache-2.0
//! Bounded loopback HTTP ingress (local trust plan C: plain TCP on
//! 127.0.0.1, no TLS). Every `/mcp` and JSON API request is the built-in
//! local agent session (U110); native admission is checked for every request.
use airk_catalog_control::catalog_admission::{IssueTicket, NativeCatalogAdmission};
use airk_core::{identity::Incarnation, service_http::EndpointDescriptor};
use airk_endpoint::http_endpoint::BoundEndpoint;
use airk_mcp::{
    read_attempt::ReadAttemptContext,
    remote::{
        ReadSessionFactory, ReadSessionPreferences,
        http::{
            AuthenticatedBackendFactory, HttpReadLimits, HttpReadService, HttpResponse,
            HttpSessionBudget,
        },
    },
    response::ResponseMode,
};
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body::{Body, Frame, SizeHint};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    convert::Infallible,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[cfg(all(test, feature = "local-inference"))]
mod inference_owner_tests;
#[cfg(test)]
pub(crate) mod tests;

/// Revalidates the local agent session's catalog on every request. Returned
/// factories must continue native admission checks immediately before each read.
pub type ValidateRequest = Arc<
    dyn Fn(
            ReadAttemptContext,
        )
            -> Pin<Box<dyn Future<Output = anyhow::Result<AuthenticatedBackendFactory>> + Send>>
        + Send
        + Sync,
>;
pub type AdmissionClosed = Arc<dyn Fn() -> bool + Send + Sync>;
pub type AdmitRequest =
    Arc<dyn Fn() -> anyhow::Result<Arc<dyn std::any::Any + Send + Sync>> + Send + Sync>;
pub struct NativeAdmission {
    pub delivery_guard_factory: airk_mcp::read_attempt::ReadDeliveryGuardFactory,
    pub closed: AdmissionClosed,
    pub gate: NativeCatalogAdmission,
    pub admit: AdmitRequest,
    pub settle: airk_mcp::read_attempt::WorkerSettlementRequester,
}

const CONNECTIONS: usize = 128;
const AUTH_TASKS: usize = 16;
const OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_FRAME: usize = 1024 * 1024;
const STREAM_SECONDS: u64 = 120;

/// Waits before the request body names its tool (native validation,
/// a previous response still draining) keep the ordinary 125 s even though the
/// transport context is minted at Answer's ceiling: they are admission, not
/// the Answer's work.
/// The connection IO deadline's clock. It is Tokio's, the clock of the
/// `OutputBody` stream bound it is widened and tightened from, so the two
/// cannot disagree; outside paused tests it is the system monotonic clock.
fn io_now() -> Instant {
    tokio::time::Instant::now().into_std()
}

fn admission_deadline(context: &ReadAttemptContext) -> Instant {
    context
        .deadline()
        .min(Instant::now() + airk_core::read_attempt::MAX_READ_BUDGET)
}
/// Concurrent MCP sessions across every agent (U110: all share one client).
const SESSIONS: usize = 128;

/// The local agent session's cached read service (U110: the only one).
struct CachedService {
    read_delivery: ReadDeliveryCell,
    ticket: IssueTicket,
    service: HttpReadService,
}
struct State {
    authority: String,
    validate: ValidateRequest,
    checkpoint_closed: AdmissionClosed,
    factory: ReadSessionFactory,
    sessions: HttpSessionBudget,
    client: Mutex<Option<CachedService>>,
    connections: Arc<Semaphore>,
    validation: Arc<Semaphore>,
    output: Arc<Semaphore>,
    closed: CancellationToken,
    native_gate: NativeCatalogAdmission,
    admit: AdmitRequest,
    settle: airk_mcp::read_attempt::WorkerSettlementRequester,
    delivery_guard_factory: airk_mcp::read_attempt::ReadDeliveryGuardFactory,
    /// The listener's port (the exact `Host` and `Origin` of `/admin`).
    port: u16,
    /// U110: the endpoint's catalog namespace, the local agent session's.
    namespace: Incarnation,
    /// The `/admin` plane (split Phase 5); set once by the service owner.
    admin: std::sync::OnceLock<Arc<crate::admin_plane::AdminPlane>>,
    /// Tests that jump paused Tokio time past rmcp's real-clock SSE
    /// keep-alive build their read services without it (remote CONTRACT.md).
    #[cfg(test)]
    sse_keep_alive_off: bool,
}
#[derive(Clone)]
pub struct Gateway(Arc<State>);
impl Gateway {
    pub fn new(
        endpoint: &EndpointDescriptor,
        validate: ValidateRequest,
        native: NativeAdmission,
    ) -> anyhow::Result<Self> {
        endpoint.validate()?;
        Ok(Self(Arc::new(State {
            authority: format!("127.0.0.1:{}", endpoint.url.port()),
            validate,
            checkpoint_closed: native.closed,
            factory: ReadSessionFactory::default(),
            sessions: HttpSessionBudget::new(SESSIONS)?,
            client: Mutex::new(None),
            connections: Arc::new(Semaphore::new(CONNECTIONS)),
            validation: Arc::new(Semaphore::new(AUTH_TASKS)),
            output: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            closed: CancellationToken::new(),
            native_gate: native.gate,
            admit: native.admit,
            settle: native.settle,
            delivery_guard_factory: native.delivery_guard_factory,
            port: endpoint.url.port(),
            namespace: endpoint.namespace,
            admin: std::sync::OnceLock::new(),
            #[cfg(test)]
            sse_keep_alive_off: false,
        })))
    }
    /// Serve `/admin` through `plane`. Without it every admin-plane target
    /// answers `not_yet_served`; other targets are unaffected.
    pub fn set_admin(&self, plane: Arc<crate::admin_plane::AdminPlane>) {
        let _ = self.0.admin.set(plane);
    }
    async fn retire_context(&self) {
        let removed = self
            .0
            .client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(client) = removed {
            // Scope refusal retires evidence delivery. A fresh TCP has not
            // bound this cached read context; its sanitized refusal still
            // obeys its native ticket.
            client
                .read_delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed = true;
            client.service.close().await;
        }
    }
    pub async fn run(
        &self,
        held: BoundEndpoint,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            format!("127.0.0.1:{}", held.port()?) == self.0.authority,
            "HTTP socket and descriptor disagree"
        );
        let listener = held.into_tokio()?;
        let mut tasks = JoinSet::new();
        let mut failure = None;
        loop {
            tokio::select! {
                _=shutdown.cancelled()=>break,
                _=self.0.closed.cancelled()=>break,
                Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
                accepted=listener.accept()=>{
                    let (socket,peer)=match accepted {Ok(value)=>value,Err(error)=>{failure=Some(error);break;}};
                    if !peer.ip().is_loopback(){continue;}
                    let Ok(connection)=self.0.connections.clone().try_acquire_owned() else{continue;};
                    let gateway=self.clone();
                    // The 4-tuple for the admin plane's same-user peer proof (H1).
                    let admin_peer=socket.local_addr().ok().map(|server|crate::admin_plane::ConnectionPeer::new(peer,server));
                    tasks.spawn(async move {
                        let _connection=connection;
                        gateway.serve_accepted(socket,admin_peer).await;
                    });
                }
            }
        }
        self.close().await;
        while tasks.join_next().await.is_some() {}
        self.0.factory.shutdown().await;
        match failure {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
    /// One accepted connection: HTTP/1.1 over the fenced socket, until it
    /// ends or the gateway closes. The listener and the tests that drive a
    /// real connection over an in-memory socket share it. Hyper's 15 s
    /// header read timeout bounds an idle or slow client.
    async fn serve_accepted<S>(
        self,
        socket: S,
        admin_peer: Option<crate::admin_plane::ConnectionPeer>,
    ) where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let gateway = self;
        if gateway.0.closed.is_cancelled() {
            return;
        }
        let delivery = ConnectionFence::new(gateway.0.native_gate.clone());
        // Fence the actual socket: every byte Hyper writes or flushes passes
        // the delivery fence before it reaches the TCP stream.
        let socket = FencedIo {
            inner: socket,
            delivery: delivery.clone(),
        };
        let disconnected = gateway.0.closed.child_token();
        let cleanup = CancelOnDrop {
            token: disconnected.clone(),
            armed: true,
        };
        let socket_delivery = delivery.clone();
        let requests = service_fn(move |mut request: Request<Incoming>| {
            let gateway = gateway.clone();
            let canceled = disconnected.child_token();
            let delivery = delivery.clone();
            if let Some(admin_peer) = admin_peer.clone() {
                request.extensions_mut().insert(admin_peer);
            }
            async move {
                Ok::<_, Infallible>(
                    gateway
                        .handle_on_connection(request, canceled, delivery)
                        .await,
                )
            }
        });
        let mut builder = http1::Builder::new();
        builder
            .timer(TokioTimer::new())
            .header_read_timeout(Duration::from_secs(15))
            .max_buf_size(16 * 1024)
            .max_headers(64);
        let io = FlushTrackedIo {
            inner: socket,
            delivery: socket_delivery,
        };
        let connection = builder.serve_connection(TokioIo::new(io), requests);
        tokio::pin!(connection);
        tokio::select! {
            _ = cleanup.token.cancelled() => {},
            _ = connection.as_mut() => {},
        }
        drop(cleanup);
    }
    pub async fn close(&self) {
        self.0.closed.cancel();
        self.0.factory.close();
        let client = self
            .0
            .client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(client) = client {
            client
                .read_delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed = true;
            client.service.close().await;
        }
    }
    #[cfg(test)]
    async fn handle<B>(
        &self,
        request: Request<B>,
        canceled: CancellationToken,
    ) -> Response<OutputBody>
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: fmt::Display + Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.handle_on_connection(
            request,
            canceled,
            ConnectionFence::new(self.0.native_gate.clone()),
        )
        .await
    }
    async fn handle_on_connection<B>(
        &self,
        mut request: Request<B>,
        canceled: CancellationToken,
        delivery: ConnectionFence,
    ) -> Response<OutputBody>
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: fmt::Display + Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let native_gate = self.0.native_gate.clone();
        // Minted before the body names a tool: Answer's ceiling, which each MCP
        // or JSON API tool call narrows to its own (`ReadAttemptContext::for_call`).
        let mut context = ReadAttemptContext::for_transport(canceled)
            .with_worker_reap_observer(Arc::new(move |identity| {
                native_gate.worker_was_reaped(identity)
            }))
            .with_worker_settlement_requester(self.0.settle.clone());
        let mut cleanup = CancelOnDrop {
            token: context.cancellation().clone(),
            armed: true,
        };
        context = context.with_delivery_guard_factory(self.0.delivery_guard_factory.clone());
        let bound = delivery.bind_request(context.clone()).await;
        // Targets `admin_contract::route` places in the admin plane never reach
        // the bearer dispatch; every other target is handled as before.
        if is_admin_target(&request) {
            let response = match (bound, self.0.admin.get()) {
                (Ok(()), Some(plane)) => {
                    let peer = request
                        .extensions()
                        .get::<crate::admin_plane::ConnectionPeer>()
                        .cloned();
                    plane
                        .with_read_parent(context.clone())
                        .handle(request, peer, self.0.port, self.0.closed.is_cancelled())
                        .await
                }
                (Ok(()), None) => crate::admin_plane::refusal(
                    airk_endpoint::admin_contract::AdminCode::NotYetServed,
                ),
                (Err(_), _) => crate::admin_plane::refusal(
                    airk_endpoint::admin_contract::AdminCode::AdminInternal,
                ),
            };
            cleanup.armed = false;
            let (parts, body) = response.into_parts();
            return Response::from_parts(
                parts,
                OutputBody::new(body, self.0.output.clone(), OUTPUT_BYTES, context)
                    .with_delivery(delivery),
            );
        }
        let response = match bound {
            Ok(()) => self.dispatch(&mut request, &mut context, &delivery).await,
            Err(_) => Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "previous_response_not_flushed",
            )),
        };
        let response = match response {
            Ok(service) => service.handle_with_context(request, context.clone()).await,
            Err(response) => *response,
        };
        let stream = response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let (mut parts, body) = response.into_parts();
        parts.headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        );
        parts.headers.insert(
            "x-content-type-options",
            http::HeaderValue::from_static("nosniff"),
        );
        cleanup.armed = false;
        Response::from_parts(
            parts,
            OutputBody::new(
                body,
                self.0.output.clone(),
                if stream { OUTPUT_BYTES } else { MAX_FRAME },
                context,
            )
            .with_delivery(delivery),
        )
    }
    async fn dispatch<B>(
        &self,
        request: &mut Request<B>,
        context: &mut ReadAttemptContext,
        delivery: &ConnectionFence,
    ) -> Result<HttpReadService, Box<HttpResponse>> {
        if self.0.closed.is_cancelled() {
            return Err(error(StatusCode::SERVICE_UNAVAILABLE, "service_draining"));
        }
        // DNS rebinding and browser-origin checks precede everything else.
        if request.headers().get_all(http::header::HOST).iter().count() != 1
            || request
                .headers()
                .get(http::header::HOST)
                .and_then(|v| v.to_str().ok())
                != Some(self.0.authority.as_str())
            || request
                .uri()
                .authority()
                .is_some_and(|a| a.as_str() != self.0.authority)
        {
            return Err(error(StatusCode::FORBIDDEN, "host_refused"));
        }
        if request.headers().contains_key(http::header::ORIGIN) {
            return Err(error(StatusCode::FORBIDDEN, "origin_refused"));
        }
        // The planes stay separate (L6): the exact admin cookie is refused
        // here, after the existing Host and `Origin` checks.
        let cookies: Vec<String> = request
            .headers()
            .get_all(http::header::COOKIE)
            .iter()
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
            .collect();
        if airk_endpoint::admin_contract::bearer_plane_cookie(&cookies).is_some() {
            return Err(error(StatusCode::FORBIDDEN, "cookie_on_bearer_plane"));
        }
        // U110: `/mcp` takes no credential. Any `Authorization` header is
        // stripped unread; every request is the built-in local agent session,
        // an open session with no stored default (scope is named per call).
        request.headers_mut().remove(http::header::AUTHORIZATION);
        delivery
            .capture(self.0.namespace)
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        if request.method() == http::Method::POST {
            if (self.0.checkpoint_closed)() {
                return Err(error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "upgrade_in_progress",
                ));
            }
            let guard = (self.0.admit)()
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "upgrade_in_progress"))?;
            context.retain_lifetime_guard(guard).map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "request_lifetime_unavailable",
                )
            })?;
        }
        let validation_permit = self
            .0
            .validation
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_validation_busy"))?;
        // A separate context retains this permit through blocked metadata work,
        // without charging it for the later evidence read or network delivery.
        let validation_context = context
            .clone()
            .with_lifetime_guard(Arc::new((context.lifetime_guard(), validation_permit)));
        let backend = tokio::select! {_=context.cancellation().cancelled()=>return Err(error(StatusCode::SERVICE_UNAVAILABLE,"request_canceled")),
        result=tokio::time::timeout_at(tokio::time::Instant::from_std(admission_deadline(context)),(self.0.validate)(validation_context))=>match result {
            Ok(Ok(backend))=>backend,Ok(Err(_))=>{self.retire_context().await;return Err(error(StatusCode::FORBIDDEN,"native_grant_refused"));},
            Err(_)=>{context.cancellation().cancel();return Err(error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted"));}
        }};
        delivery
            .check()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        if (self.0.checkpoint_closed)() && request.method() == http::Method::POST {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "upgrade_in_progress",
            ));
        }
        let ticket = delivery
            .authenticated_ticket()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        let (service, retired) = self.cache_validated_service(backend, delivery, ticket)?;
        if let Some(old) = retired {
            old.close().await;
        }
        Ok(service)
    }

    fn cache_validated_service(
        &self,
        backend: AuthenticatedBackendFactory,
        delivery: &ConnectionFence,
        ticket: IssueTicket,
    ) -> Result<(HttpReadService, Option<HttpReadService>), Box<HttpResponse>> {
        let mut retired = None;
        let service = {
            let mut client = self
                .0
                .client
                .lock()
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"))?;
            // Serialize cache publication with retirement's removal. A request
            // may have passed its earlier check before waiting for this lock.
            delivery
                .check()
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
            // The local session's ticket follows the current issue (U106(f)),
            // so its initialized SDK sessions are reused across a revision
            // move; native generation turnover still fences old delivery.
            if let Some(entry) = client.as_mut() {
                delivery
                    .bind_read_context(entry.read_delivery.clone())
                    .map_err(|_| {
                        error(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "native_read_context_fenced",
                        )
                    })?;
                entry.ticket = ticket;
                return Ok((entry.service.clone(), None));
            }
            #[cfg(test)]
            let build = if self.0.sse_keep_alive_off {
                HttpReadService::without_sse_keep_alive
            } else {
                HttpReadService::new
            };
            #[cfg(not(test))]
            let build = HttpReadService::new;
            let service = build(
                self.0.factory.clone(),
                self.0.sessions.clone(),
                backend,
                ReadSessionPreferences {
                    response_mode: ResponseMode::Terse,
                    client: airk_core::usage::ClientFamily::Unknown,
                },
                self.0.authority.clone(),
                vec![],
                // U110: every agent shares the one local agent session, so its
                // per-client bound is the gateway's global session bound.
                HttpReadLimits {
                    sessions_per_client: SESSIONS,
                    ..HttpReadLimits::default()
                },
            )
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_scope_unavailable"))?;
            let read_delivery = ReadDeliveryCell::default();
            delivery
                .bind_read_context(read_delivery.clone())
                .map_err(|_| {
                    error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "native_read_context_fenced",
                    )
                })?;
            if let Some(old) = client.replace(CachedService {
                read_delivery,
                ticket,
                service: service.clone(),
            }) {
                old.read_delivery
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .closed = true;
                retired = Some(old.service);
            }
            service
        };
        Ok((service, retired))
    }
}
type ReadDeliveryCell = Arc<Mutex<ReadContextDelivery>>;
#[derive(Default)]
struct ReadContextDelivery {
    closed: bool,
}
impl ReadContextDelivery {
    fn check(&self) -> std::io::Result<()> {
        if self.closed {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "HTTP read-context delivery retired",
            ));
        }
        Ok(())
    }
}
/// One ticket for the entire TCP connection. An invalid old ticket is never
/// replaced while Hyper can still retain an earlier response.
#[derive(Clone)]
struct ConnectionFence {
    gate: NativeCatalogAdmission,
    ticket: Arc<Mutex<Option<IssueTicket>>>,
    /// U106(f): the namespace of the local agent session, whose ticket
    /// follows the gate's current issue instead of pinning a captured
    /// revision: each call re-resolves and leases at the current revision,
    /// so a revision move does not fence it. A pending catalog fence and
    /// service end still deny it. `None` until a bearer-plane request
    /// captures it (admin-plane connections never do).
    follows: Arc<Mutex<Option<Incarnation>>>,
    read_context: Arc<Mutex<Option<ReadDeliveryCell>>>,
    requests: Arc<Mutex<RequestDeliveryState>>,
    flushed: Arc<tokio::sync::Notify>,
}
#[derive(Default)]
struct RequestDeliveryState {
    closed: bool,
    active: Option<RequestDelivery>,
}
struct RequestDelivery {
    context: ReadAttemptContext,
    deadline: Instant,
    body_complete: bool,
}
impl ConnectionFence {
    fn new(gate: NativeCatalogAdmission) -> Self {
        Self {
            gate,
            ticket: Arc::new(Mutex::new(None)),
            follows: Arc::new(Mutex::new(None)),
            read_context: Arc::new(Mutex::new(None)),
            requests: Default::default(),
            flushed: Default::default(),
        }
    }
    async fn bind_request(&self, context: ReadAttemptContext) -> anyhow::Result<()> {
        let wait_until = admission_deadline(&context);
        loop {
            let changed = self.flushed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self
                    .requests
                    .lock()
                    .map_err(|_| anyhow::anyhow!("request delivery unavailable"))?;
                anyhow::ensure!(!state.closed, "request delivery closed");
                if state.active.is_none() {
                    state.active = Some(RequestDelivery {
                        deadline: context.deadline(),
                        context,
                        body_complete: false,
                    });
                    return Ok(());
                }
            }
            tokio::select! {
                _ = &mut changed => {},
                _ = context.cancellation().cancelled() => { self.fail_requests(); anyhow::bail!("request canceled while previous output drains"); },
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(wait_until)) => { self.fail_requests(); anyhow::bail!("previous output did not drain within request deadline"); },
            }
        }
    }
    fn body_complete(&self) {
        if let Some(active) = self
            .requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .as_mut()
        {
            active.body_complete = true;
        }
    }
    fn tighten_output_deadline(&self, deadline: Instant) {
        if let Some(active) = self
            .requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .as_mut()
        {
            active.deadline = active.deadline.min(deadline);
        }
    }
    /// Widen the active request's IO deadline to an Answer stream's bound,
    /// never past its own context's deadline (OutputBody::widen_for_answer).
    fn widen_output_deadline(&self, deadline: Instant) {
        if let Some(active) = self
            .requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .as_mut()
        {
            active.deadline = active.deadline.max(deadline.min(active.context.deadline()));
        }
    }
    fn complete_flush(&self) {
        let mut state = self.requests.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.body_complete)
        {
            state.active.take();
            self.flushed.notify_waiters();
        }
    }
    fn close_requests(&self) {
        let mut state = self.requests.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        if let Some(active) = state.active.take() {
            active.context.cancellation().cancel();
        }
        self.flushed.notify_waiters();
    }
    fn fail_requests(&self) {
        let mut state = self.requests.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        if let Some(active) = state.active.as_ref() {
            active.context.cancellation().cancel();
        }
        self.flushed.notify_waiters();
    }
    /// Only validated evidence contexts bind this cell. Once captured it cannot
    /// be replaced while an old response may remain buffered in Hyper.
    fn bind_read_context(&self, cell: ReadDeliveryCell) -> anyhow::Result<()> {
        let mut held = self
            .read_context
            .lock()
            .map_err(|_| std::io::Error::other("read-context fence unavailable"))?;
        if let Some(old) = held.as_ref() {
            anyhow::ensure!(
                Arc::ptr_eq(old, &cell),
                "HTTP connection cannot replace its read context"
            );
        } else {
            *held = Some(cell.clone());
        }
        cell.lock()
            .map_err(|_| std::io::Error::other("read-context fence unavailable"))?
            .check()?;
        Ok(())
    }
    /// Capture the local session's namespace: its ticket follows the
    /// current issue of that namespace (U106(f)). `Err` when the gate admits
    /// no delivery, or the connection was captured for another namespace.
    fn capture(&self, namespace: Incarnation) -> anyhow::Result<()> {
        {
            let mut follows = self
                .follows
                .lock()
                .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?;
            anyhow::ensure!(
                follows.is_none_or(|held| held == namespace),
                "HTTP connection delivery namespace changed"
            );
            *follows = Some(namespace);
        }
        self.held_ticket()?;
        Ok(())
    }
    /// The ticket this connection delivers under: the gate's current issue
    /// of its captured namespace (stored as the held ticket). `None` before
    /// a capture. `Err` when the gate admits no delivery.
    fn held_ticket(&self) -> anyhow::Result<Option<IssueTicket>> {
        let follows = *self
            .follows
            .lock()
            .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?;
        let mut held = self
            .ticket
            .lock()
            .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?;
        if let Some(namespace) = follows {
            *held = Some(self.gate.capture_current_issue(
                namespace,
                airk_core::index_catalog::CatalogRevision::INITIAL,
            )?);
        }
        Ok(*held)
    }
    fn authenticated_ticket(&self) -> anyhow::Result<IssueTicket> {
        let ticket = self.held_ticket()?.ok_or_else(|| {
            std::io::Error::other("HTTP authentication has no native delivery ticket")
        })?;
        self.gate.check_issue(ticket)?;
        Ok(ticket)
    }
    fn check(&self) -> anyhow::Result<()> {
        if let Some(cell) = self
            .read_context
            .lock()
            .map_err(|_| std::io::Error::other("read-context fence unavailable"))?
            .as_ref()
        {
            cell.lock()
                .map_err(|_| std::io::Error::other("read-context fence unavailable"))?
                .check()?;
        }
        if let Some(ticket) = self.held_ticket()? {
            self.gate.check_issue(ticket)?;
        }
        Ok(())
    }
    fn poll<T>(
        &self,
        effect: impl FnOnce() -> Poll<std::io::Result<T>>,
    ) -> Poll<std::io::Result<T>> {
        // Copy opaque custody before Store policy ownership: never invert the
        // Store -> request/read-cell -> NativeGate lock order.
        let context = match self.requests.lock() {
            Ok(state) => state.active.as_ref().map(|active| active.context.clone()),
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("request delivery unavailable")));
            }
        };
        match context {
            Some(context) => {
                poll_custody(&context, DeliveryLayer::Policy, || self.poll_native(effect))
            }
            None => self.poll_native(effect),
        }
    }
    fn poll_native<T>(
        &self,
        effect: impl FnOnce() -> Poll<std::io::Result<T>>,
    ) -> Poll<std::io::Result<T>> {
        let requests = match self.requests.lock() {
            Ok(state) => state,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("request delivery unavailable")));
            }
        };
        if requests.closed
            || requests.active.as_ref().is_some_and(|active| {
                active.context.cancellation().is_cancelled() || io_now() >= active.deadline
            })
        {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "request delivery expired or canceled",
            )));
        }
        let held_read = match self.read_context.lock() {
            Ok(held) => held,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("read-context fence unavailable")));
            }
        };
        // Hold the read-context cell across the actual raw write, so retirement
        // cannot acknowledge while older buffered evidence is being delivered.
        let read_context = match held_read.as_ref().map(|cell| cell.lock()).transpose() {
            Ok(read_context) => read_context,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("read-context fence unavailable")));
            }
        };
        if let Some(read_context) = read_context.as_ref()
            && let Err(error) = read_context.check()
        {
            return Poll::Ready(Err(error));
        }
        let ticket = match self.held_ticket() {
            Ok(held) => held,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "HTTP delivery fenced; open a fresh connection",
                )));
            }
        };
        let effect = || {
            if let Some(active) = requests.active.as_ref() {
                if active.context.cancellation().is_cancelled() || io_now() >= active.deadline {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "HTTP delivery expired at actual IO cut",
                    )));
                }
                if let Err(error) = active.context.check_delivery_live() {
                    return Poll::Ready(Err(error));
                }
            }
            effect()
        };
        if let Some(context) = requests
            .active
            .as_ref()
            .map(|active| &active.context)
            .filter(|context| context.has_source_delivery())
        {
            if let Some(ticket) = ticket
                && self.gate.check_issue(ticket).is_err()
            {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "HTTP issue changed before source delivery",
                )));
            }
            // The source owner holds the fixed original NativeGate issue and
            // actual live selection nonce around this poll. Never reenter it.
            return poll_custody(context, DeliveryLayer::Source, effect);
        }
        match ticket {
            None => effect(),
            Some(ticket) => self.gate.with_issue(ticket, effect).unwrap_or_else(|_| {
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "HTTP delivery fenced; open a fresh connection",
                )))
            }),
        }
    }
}
// The usize is only the opaque guard's effect-status carrier for non-write
// polls. Return the original IO result, including the actual byte count.
enum DeliveryLayer {
    Policy,
    Source,
}
fn poll_custody<T>(
    context: &ReadAttemptContext,
    layer: DeliveryLayer,
    effect: impl FnOnce() -> Poll<std::io::Result<T>>,
) -> Poll<std::io::Result<T>> {
    let mut effect = Some(effect);
    let mut observed = None;
    let mut actual = || {
        let Some(effect) = effect.take() else {
            return Poll::Ready(Err(std::io::Error::other("delivery effect repeated")));
        };
        let result = effect();
        let status = match &result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(0)),
            Poll::Ready(Err(error)) => {
                Poll::Ready(Err(std::io::Error::new(error.kind(), error.to_string())))
            }
        };
        observed = Some(result);
        status
    };
    let status = match layer {
        DeliveryLayer::Policy => context.poll_delivery(&mut actual),
        DeliveryLayer::Source => context.poll_source_delivery(&mut actual),
    };
    if let Poll::Ready(Err(error)) = status {
        return match observed {
            Some(Poll::Ready(Err(original))) => Poll::Ready(Err(original)),
            _ => Poll::Ready(Err(error)),
        };
    }
    observed.unwrap_or_else(|| {
        Poll::Ready(Err(std::io::Error::other(
            "delivery gate omitted actual IO poll",
        )))
    })
}

struct FencedIo<S> {
    inner: S,
    delivery: ConnectionFence,
}

/// Observe Hyper's flush boundary while enforcement stays at the socket
/// (`FlushTrackedIo<FencedIo<TcpStream>>`). Hyper's HTTP/1 writer flushes its
/// queued bytes through the fence before this flush can succeed.
struct FlushTrackedIo<S> {
    inner: S,
    delivery: ConnectionFence,
}
impl<S: AsyncRead + Unpin> AsyncRead for FlushTrackedIo<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for FlushTrackedIo<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, bytes)
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write_vectored(cx, bytes)
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_flush(cx);
        if matches!(result, Poll::Ready(Ok(()))) {
            this.delivery.complete_flush();
        }
        result
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_shutdown(cx);
        if matches!(result, Poll::Ready(Ok(()))) {
            this.delivery.close_requests();
        }
        result
    }
}
impl<S> Drop for FlushTrackedIo<S> {
    fn drop(&mut self) {
        self.delivery.close_requests();
    }
}
impl<S: AsyncRead + Unpin> AsyncRead for FencedIo<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for FencedIo<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        this.delivery
            .poll(|| Pin::new(&mut this.inner).poll_write(cx, bytes))
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        this.delivery
            .poll(|| Pin::new(&mut this.inner).poll_write_vectored(cx, bytes))
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        this.delivery
            .poll(|| Pin::new(&mut this.inner).poll_flush(cx))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        this.delivery
            .poll(|| Pin::new(&mut this.inner).poll_shutdown(cx))
    }
}
struct CancelOnDrop {
    token: CancellationToken,
    armed: bool,
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.token.cancel();
        }
    }
}
/// Whether `admin_contract::route` places the origin-form request target in
/// the admin plane. An absolute-form target is never admin-routed.
fn is_admin_target<B>(request: &Request<B>) -> bool {
    let uri = request.uri();
    uri.scheme().is_none()
        && uri.authority().is_none()
        && !matches!(
            airk_endpoint::admin_contract::route(
                uri.path_and_query()
                    .map_or("", |target| target.as_str())
                    .as_bytes()
            ),
            airk_endpoint::admin_contract::PathMatch::NotAdmin
        )
}
fn error(status: StatusCode, code: &'static str) -> Box<HttpResponse> {
    Box::new(
        Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(
                Full::new(Bytes::from(
                    serde_json::json!({"schema":"airk.local-api-error/1","error":{"code":code}})
                        .to_string(),
                ))
                .boxed(),
            )
            .expect("fixed gateway error"),
    )
}

/// The charge follows every network clone of a frame, not merely the body poll.
struct ChargedBytes {
    bytes: Bytes,
    _permit: OwnedSemaphorePermit,
    _admission: Option<Arc<dyn std::any::Any + Send + Sync>>,
}
impl AsRef<[u8]> for ChargedBytes {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}
#[derive(Debug)]
pub struct OutputError;
impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded HTTP output terminated")
    }
}
impl std::error::Error for OutputError {}
pub struct OutputBody {
    inner: http_body_util::combinators::BoxBody<Bytes, Infallible>,
    budget: Arc<Semaphore>,
    remaining: usize,
    deadline: Pin<Box<tokio::time::Sleep>>,
    context: ReadAttemptContext,
    done: bool,
    delivery: Option<ConnectionFence>,
    /// When the stream began, for the Answer bound below.
    started: tokio::time::Instant,
    /// Whether the bound has been widened for an Answer call.
    answer_bound: bool,
}
impl OutputBody {
    fn new(
        inner: http_body_util::combinators::BoxBody<Bytes, Infallible>,
        budget: Arc<Semaphore>,
        remaining: usize,
        context: ReadAttemptContext,
    ) -> Self {
        // Every response starts with the 120 s stream bound. The tool is often
        // not yet known here (rmcp returns the event-stream head before the
        // call runs), so poll_frame widens it once the call is marked an
        // Answer (nswer_bound).
        let started = tokio::time::Instant::now();
        let deadline = tokio::time::Instant::from_std(context.deadline())
            .min(started + Duration::from_secs(STREAM_SECONDS));
        let mut body = Self {
            inner,
            budget,
            remaining,
            deadline: Box::pin(tokio::time::sleep_until(deadline)),
            context,
            done: false,
            delivery: None,
            started,
            answer_bound: false,
        };
        body.widen_for_answer();
        body
    }
    /// An Answer call's stream (progress, then its result) runs to Answer's
    /// own ceiling, within the request context's deadline; the connection's
    /// IO deadline is widened to match. Every other response keeps 120 s.
    fn widen_for_answer(&mut self) {
        if self.answer_bound || !self.context.is_answer_call() {
            return;
        }
        self.answer_bound = true;
        let deadline = tokio::time::Instant::from_std(self.context.deadline())
            .min(self.started + airk_core::read_attempt::MAX_ANSWER_READ_BUDGET);
        if deadline > self.deadline.deadline() {
            self.deadline.as_mut().reset(deadline);
            if let Some(delivery) = &self.delivery {
                delivery.widen_output_deadline(deadline.into_std());
            }
        }
    }
    fn with_delivery(mut self, delivery: ConnectionFence) -> Self {
        delivery.tighten_output_deadline(self.deadline.deadline().into_std());
        self.delivery = Some(delivery);
        if self.inner.is_end_stream() {
            self.complete();
        }
        self
    }
    fn complete(&mut self) {
        self.done = true;
        if let Some(delivery) = &self.delivery {
            delivery.body_complete();
        }
    }
    fn fail(&mut self) -> Poll<Option<Result<Frame<Bytes>, OutputError>>> {
        self.done = true;
        self.context.cancellation().cancel();
        Poll::Ready(Some(Err(OutputError)))
    }
}
impl Body for OutputBody {
    type Data = Bytes;
    type Error = OutputError;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, OutputError>>> {
        if self.done {
            return Poll::Ready(None);
        }
        self.widen_for_answer();
        if self.context.cancellation().is_cancelled() || self.deadline.as_mut().poll(cx).is_ready()
        {
            return self.fail();
        }
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                self.complete();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(never))) => match never {},
            Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(bytes) => {
                    if bytes.len() > MAX_FRAME || bytes.len() > self.remaining {
                        return self.fail();
                    }
                    let Ok(permit) = self
                        .budget
                        .clone()
                        .try_acquire_many_owned(bytes.len() as u32)
                    else {
                        return self.fail();
                    };
                    self.remaining -= bytes.len();
                    if self.delivery.is_some() && self.inner.is_end_stream() {
                        self.complete();
                    }
                    Poll::Ready(Some(Ok(Frame::data(Bytes::from_owner(ChargedBytes {
                        bytes,
                        _permit: permit,
                        _admission: self.context.lifetime_guard(),
                    })))))
                }
                Err(frame) => {
                    if self.delivery.is_some() && self.inner.is_end_stream() {
                        self.complete();
                    }
                    Poll::Ready(Some(Ok(frame)))
                }
            },
        }
    }
    fn is_end_stream(&self) -> bool {
        self.done || self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        let mut hint = SizeHint::new();
        if let Some(size) = self.inner.size_hint().exact() {
            hint.set_upper(size.min(self.remaining as u64));
        }
        hint
    }
}
impl Drop for OutputBody {
    fn drop(&mut self) {
        if self.delivery.is_none() || !self.done {
            self.context.cancellation().cancel();
        }
    }
}
