// SPDX-License-Identifier: Apache-2.0
//! Bounded HTTPS ingress. Credentials and native grants are checked for every request.
use airk_catalog_control::catalog_admission::{IssueTicket, NativeCatalogAdmission};
use airk_core::{identity::Incarnation, service_http::EndpointDescriptor};
use airk_endpoint::http_endpoint::{AuthenticatedCredential, BoundEndpoint, CredentialStore};
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
    collections::HashMap,
    convert::Infallible,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[cfg(all(test, feature = "local-inference"))]
mod inference_owner_tests;
#[cfg(test)]
pub(crate) mod tests;

/// Revalidates the exact live native selection on every request. Returned
/// factories must continue native admission checks immediately before each read.
pub type ValidateRequest = Arc<
    dyn Fn(
            AuthenticatedCredential,
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
    pub closed: AdmissionClosed,
    pub gate: NativeCatalogAdmission,
    pub admit: AdmitRequest,
    pub settle: airk_mcp::read_attempt::WorkerSettlementRequester,
}

const CONNECTIONS: usize = 128;
const HANDSHAKES: usize = 32;
const AUTH_TASKS: usize = 16;
const OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_FRAME: usize = 1024 * 1024;
const STREAM_SECONDS: u64 = 120;

struct CachedService {
    delivery: Arc<Mutex<CredentialDelivery>>,
    read_delivery: ReadDeliveryCell,
    ticket: IssueTicket,
    generation: u64,
    expires: u64,
    selection: airk_core::service_http::HttpSelection,
    service: HttpReadService,
}
struct State {
    authority: String,
    credentials: Arc<Mutex<CredentialStore>>,
    validate: ValidateRequest,
    checkpoint_closed: AdmissionClosed,
    factory: ReadSessionFactory,
    sessions: HttpSessionBudget,
    clients: Mutex<HashMap<Incarnation, CachedService>>,
    delivery_cells: Mutex<CredentialDeliveryRegistry>,
    connections: Arc<Semaphore>,
    handshakes: Arc<Semaphore>,
    authentication: Arc<Semaphore>,
    validation: Arc<Semaphore>,
    output: Arc<Semaphore>,
    tls: watch::Receiver<tokio_rustls::TlsAcceptor>,
    closed: CancellationToken,
    native_gate: NativeCatalogAdmission,
    admit: AdmitRequest,
    settle: airk_mcp::read_attempt::WorkerSettlementRequester,
    /// The listener's port (the exact `Host` and `Origin` of `/admin`).
    port: u16,
    /// The `/admin` plane (split Phase 5); set once by the service owner.
    admin: std::sync::OnceLock<Arc<crate::admin_plane::AdminPlane>>,
}
#[derive(Clone)]
pub struct Gateway(Arc<State>);
impl Gateway {
    pub fn new(
        endpoint: &EndpointDescriptor,
        credentials: Arc<Mutex<CredentialStore>>,
        tls: watch::Receiver<tokio_rustls::TlsAcceptor>,
        validate: ValidateRequest,
        native: NativeAdmission,
    ) -> anyhow::Result<Self> {
        endpoint.validate()?;
        Ok(Self(Arc::new(State {
            authority: format!("127.0.0.1:{}", endpoint.url.port()),
            credentials,
            validate,
            checkpoint_closed: native.closed,
            factory: ReadSessionFactory::default(),
            sessions: HttpSessionBudget::new(128)?,
            clients: Mutex::new(HashMap::new()),
            delivery_cells: Mutex::new(CredentialDeliveryRegistry::default()),
            connections: Arc::new(Semaphore::new(CONNECTIONS)),
            handshakes: Arc::new(Semaphore::new(HANDSHAKES)),
            authentication: Arc::new(Semaphore::new(AUTH_TASKS)),
            validation: Arc::new(Semaphore::new(AUTH_TASKS)),
            output: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            tls,
            closed: CancellationToken::new(),
            native_gate: native.gate,
            admit: native.admit,
            settle: native.settle,
            port: endpoint.url.port(),
            admin: std::sync::OnceLock::new(),
        })))
    }
    /// Serve `/admin` through `plane`. Without it every admin-plane target
    /// answers `not_yet_served`; other targets are unaffected.
    pub fn set_admin(&self, plane: Arc<crate::admin_plane::AdminPlane>) {
        let _ = self.0.admin.set(plane);
    }
    /// Called after native credential revocation/rotation commits. Authentication
    /// still rereads the ledger, so a missed notification cannot grant a read.
    pub async fn revoke(&self, id: Incarnation) {
        self.0
            .delivery_cells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .revoke(id);
        let removed = self
            .0
            .clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(client) = removed {
            client
                .delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .revoked = true;
            client.service.close().await;
        }
    }
    async fn retire_context(&self, id: Incarnation) {
        let removed = self
            .0
            .clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(client) = removed {
            // Scope refusal retires evidence delivery, not the authenticated
            // credential. A fresh TCP has not bound this cached read context;
            // its sanitized refusal still obeys its credential and native ticket.
            client
                .read_delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed = true;
            client.service.close().await;
        }
    }
    async fn expire_credentials(&self) {
        let now = now_ms();
        let expired = {
            let mut clients = self.0.clients.lock().unwrap_or_else(|e| e.into_inner());
            let ids: Vec<_> = clients
                .iter()
                .filter(|(_, entry)| entry.expires <= now)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| clients.remove(&id))
                .collect::<Vec<_>>()
        };
        for client in expired {
            client
                .delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .revoked = true;
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
        let mut expiry = tokio::time::interval(Duration::from_secs(1));
        let mut failure = None;
        loop {
            tokio::select! {
                _=shutdown.cancelled()=>break,
                _=self.0.closed.cancelled()=>break,
                _=expiry.tick()=>self.expire_credentials().await,
                Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
                accepted=listener.accept()=>{
                    let (socket,peer)=match accepted {Ok(value)=>value,Err(error)=>{failure=Some(error);break;}};
                    if !peer.ip().is_loopback(){continue;}
                    let Ok(connection)=self.0.connections.clone().try_acquire_owned() else{continue;};
                    let Ok(handshake)=self.0.handshakes.clone().try_acquire_owned() else{continue;};
                    let gateway=self.clone();let acceptor=self.0.tls.borrow().clone();
                    // The 4-tuple for the admin plane's same-user peer proof (H1).
                    let admin_peer=socket.local_addr().ok().map(|server|crate::admin_plane::ConnectionPeer::new(peer,server));
                    tasks.spawn(async move {
                        let _connection=connection;
                        let delivery=ConnectionFence::new(gateway.0.native_gate.clone());
                        // Fence the actual socket: TLS reads may also flush queued
                        // ciphertext (for example while sending a protocol alert).
                        let socket=FencedIo{inner:socket,delivery:delivery.clone()};
                        let tls=tokio::select! {_=gateway.0.closed.cancelled()=>return,
                            result=tokio::time::timeout(Duration::from_secs(15),acceptor.accept(socket))=>match result {Ok(Ok(tls))=>tls,_=>return}};
                        drop(handshake);
                        let disconnected=gateway.0.closed.child_token();let cleanup=CancelOnDrop{token:disconnected.clone(),armed:true};
                        let socket_delivery=delivery.clone();
                        let requests=service_fn(move|mut request:Request<Incoming>|{
                            let gateway=gateway.clone();let canceled=disconnected.child_token();let delivery=delivery.clone();
                            if let Some(admin_peer)=admin_peer.clone(){request.extensions_mut().insert(admin_peer);}
                            async move {Ok::<_,Infallible>(gateway.handle_on_connection(request,canceled,delivery).await)}
                        });
                        let mut builder=http1::Builder::new();builder.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(15)).max_buf_size(16*1024).max_headers(64);
                        let tls=FlushTrackedIo{inner:tls,delivery:socket_delivery};
                        let connection=builder.serve_connection(TokioIo::new(tls),requests);tokio::pin!(connection);
                        tokio::select! {_=cleanup.token.cancelled()=>{},_=connection.as_mut()=>{}}
                        drop(cleanup);
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
    pub async fn close(&self) {
        self.0.closed.cancel();
        self.0
            .delivery_cells
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .close();
        self.0.factory.close();
        let clients = {
            let mut entries = self.0.clients.lock().unwrap_or_else(|e| e.into_inner());
            entries.drain().map(|(_, entry)| entry).collect::<Vec<_>>()
        };
        for client in clients {
            client
                .delivery
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .revoked = true;
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
        let mut context =
            ReadAttemptContext::new(Instant::now() + Duration::from_secs(125), canceled)
                .with_worker_reap_observer(Arc::new(move |identity| {
                    native_gate.worker_was_reaped(identity)
                }))
                .with_worker_settlement_requester(self.0.settle.clone());
        let mut cleanup = CancelOnDrop {
            token: context.cancellation().clone(),
            armed: true,
        };
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
        // DNS rebinding and browser-origin checks precede credential processing.
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
        if request
            .headers()
            .get_all(http::header::AUTHORIZATION)
            .iter()
            .count()
            != 1
        {
            return Err(error(StatusCode::UNAUTHORIZED, "credential_required"));
        }
        let token = request
            .headers_mut()
            .remove(http::header::AUTHORIZATION)
            .and_then(|header| {
                header
                    .to_str()
                    .ok()
                    .and_then(|value| value.strip_prefix("Bearer "))
                    .filter(|value| value.len() <= 103)
                    .map(str::to_owned)
            })
            .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "credential_required"))?;
        let permit = self
            .0
            .authentication
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "authentication_busy"))?;
        let store = self.0.credentials.clone();
        // Capture before asynchronous ledger authentication. Revocation can
        // reclaim its map slot only because an older authentication epoch can
        // never create a new delivery cell afterward.
        let authentication_epoch = self
            .0
            .delivery_cells
            .lock()
            .map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "credential_delivery_unavailable",
                )
            })?
            .epoch;
        let authentication_context = context.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            // The bounded blocking pool owns this wait; normal simultaneous
            // authentication must not masquerade as invalid credentials.
            let store = store.lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            if authentication_context.cancellation().is_cancelled()
                || Instant::now() >= authentication_context.deadline()
            {
                return Err(StatusCode::GATEWAY_TIMEOUT);
            }
            store
                .authenticate(&token, now_ms())
                .map_err(|_| StatusCode::UNAUTHORIZED)
        });
        let authenticated = tokio::select! {_=context.cancellation().cancelled()=>return Err(error(StatusCode::SERVICE_UNAVAILABLE,"request_canceled")),
        result=tokio::time::timeout_at(tokio::time::Instant::from_std(context.deadline()),task)=>match result {
            Ok(Ok(Ok(authenticated)))=>authenticated,
            Ok(Ok(Err(StatusCode::UNAUTHORIZED)))=>return Err(error(StatusCode::UNAUTHORIZED,"credential_refused")),
            Ok(Ok(Err(StatusCode::GATEWAY_TIMEOUT)))=>return Err(error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted")),
            Ok(Ok(Err(status)))=>return Err(error(status,"authentication_unavailable")),
            Ok(Err(_))=>return Err(error(StatusCode::SERVICE_UNAVAILABLE,"authentication_unavailable")),
            Err(_)=>{context.cancellation().cancel();return Err(error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted"));}
        }};
        delivery
            .capture(&authenticated.selection)
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        let credential_delivery = self
            .0
            .delivery_cells
            .lock()
            .map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "credential_delivery_unavailable",
                )
            })?
            .capture(
                authentication_epoch,
                &authenticated,
                delivery.authenticated_ticket().map_err(|_| {
                    error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced")
                })?,
            )
            .map_err(|cause| {
                if cause.is::<CredentialAuthenticationEpochRetired>() {
                    error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "credential_authentication_epoch_retired",
                    )
                } else {
                    error(StatusCode::UNAUTHORIZED, "credential_delivery_refused")
                }
            })?;
        delivery
            .bind_credential(credential_delivery.clone())
            .map_err(|_| error(StatusCode::UNAUTHORIZED, "credential_connection_changed"))?;
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
        result=tokio::time::timeout_at(tokio::time::Instant::from_std(context.deadline()),(self.0.validate)(authenticated.clone(),validation_context))=>match result {
            Ok(Ok(backend))=>backend,Ok(Err(_))=>{self.retire_context(authenticated.connection_id).await;return Err(error(StatusCode::FORBIDDEN,"native_grant_refused"));},
            Err(_)=>{context.cancellation().cancel();return Err(error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted"));}
        }};
        delivery
            .check()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        if now_ms() >= authenticated.expires_unix_ms {
            return Err(error(StatusCode::UNAUTHORIZED, "credential_expired"));
        }
        if (self.0.checkpoint_closed)() && request.method() == http::Method::POST {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "upgrade_in_progress",
            ));
        }
        let ticket = delivery
            .authenticated_ticket()
            .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
        let (service, retired) = self.cache_validated_service(
            authenticated,
            backend,
            delivery,
            credential_delivery,
            ticket,
        )?;
        if let Some(old) = retired {
            old.close().await;
        }
        Ok(service)
    }

    fn cache_validated_service(
        &self,
        authenticated: AuthenticatedCredential,
        backend: AuthenticatedBackendFactory,
        delivery: &ConnectionFence,
        credential_delivery: Arc<Mutex<CredentialDelivery>>,
        ticket: IssueTicket,
    ) -> Result<(HttpReadService, Option<HttpReadService>), Box<HttpResponse>> {
        let mut retired = None;
        let service = {
            let mut clients = self
                .0
                .clients
                .lock()
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"))?;
            // Serialize cache publication with revocation's removal. A request
            // may have passed its earlier check before waiting for this lock.
            // If revocation fences afterward, it waits for this lock and removes
            // the entry; completed revocation can never be undone by insertion.
            delivery
                .check()
                .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "native_delivery_fenced"))?;
            if let Some(entry) = clients.get_mut(&authenticated.connection_id)
                && entry.generation == authenticated.generation
                && entry.ticket == ticket
            {
                // Re-enrollment can refresh only the saved minimum clock after
                // this service already validated the current native clock. It
                // must not replace the initialized SDK sessions or their bound
                // factory. Native generation turnover still takes the replacement
                // path below and fences all old buffered delivery.
                if !airk_endpoint::http_endpoint::same_authority(
                    &entry.selection,
                    &authenticated.selection,
                ) {
                    return Err(error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "native_read_context_changed",
                    ));
                }
                delivery
                    .bind_read_context(entry.read_delivery.clone())
                    .map_err(|_| {
                        error(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "native_read_context_fenced",
                        )
                    })?;
                entry.selection = authenticated.selection;
                return Ok((entry.service.clone(), None));
            }
            if clients.len() >= 128 && !clients.contains_key(&authenticated.connection_id) {
                return Err(error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "credential_context_limit",
                ));
            }
            let service = HttpReadService::new(
                self.0.factory.clone(),
                self.0.sessions.clone(),
                backend,
                ReadSessionPreferences {
                    response_mode: ResponseMode::Terse,
                    client: authenticated.client,
                },
                self.0.authority.clone(),
                vec![],
                HttpReadLimits::default(),
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
            if let Some(old) = clients.insert(
                authenticated.connection_id,
                CachedService {
                    delivery: credential_delivery.clone(),
                    read_delivery,
                    ticket,
                    generation: authenticated.generation,
                    expires: authenticated.expires_unix_ms,
                    selection: authenticated.selection,
                    service: service.clone(),
                },
            ) {
                old.read_delivery
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .closed = true;
                if !Arc::ptr_eq(&old.delivery, &credential_delivery) {
                    old.delivery
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .revoked = true;
                }
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
struct CredentialDelivery {
    id: Incarnation,
    generation: u64,
    expires: u64,
    ticket: IssueTicket,
    revoked: bool,
}
impl CredentialDelivery {
    fn check(&self) -> std::io::Result<()> {
        if self.revoked || now_ms() >= self.expires {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "HTTP credential delivery revoked or expired",
            ));
        }
        Ok(())
    }
}
#[derive(Default)]
struct CredentialDeliveryRegistry {
    cells: HashMap<Incarnation, Arc<Mutex<CredentialDelivery>>>,
    epoch: u64,
    closed: bool,
}

#[derive(Debug)]
struct CredentialAuthenticationEpochRetired;
impl fmt::Display for CredentialAuthenticationEpochRetired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("credential authentication epoch retired")
    }
}
impl std::error::Error for CredentialAuthenticationEpochRetired {}

impl CredentialDeliveryRegistry {
    fn capture(
        &mut self,
        authentication_epoch: u64,
        credential: &AuthenticatedCredential,
        ticket: IssueTicket,
    ) -> anyhow::Result<Arc<Mutex<CredentialDelivery>>> {
        anyhow::ensure!(!self.closed, "credential delivery registry closed");
        if authentication_epoch != self.epoch {
            return Err(CredentialAuthenticationEpochRetired.into());
        }
        self.cells.retain(|_, cell| {
            let mut state = cell.lock().unwrap_or_else(|e| e.into_inner());
            if state.check().is_err() {
                state.revoked = true;
                false
            } else {
                true
            }
        });
        if let Some(old) = self.cells.get(&credential.connection_id) {
            let mut state = old
                .lock()
                .map_err(|_| std::io::Error::other("credential fence unavailable"))?;
            state.check()?;
            anyhow::ensure!(
                credential.generation >= state.generation,
                "credential generation regressed"
            );
            anyhow::ensure!(
                credential.generation > state.generation
                    || credential.expires_unix_ms == state.expires,
                "credential expiry changed without generation rotation"
            );
            if state.id == credential.connection_id
                && state.generation == credential.generation
                && state.expires == credential.expires_unix_ms
                && state.ticket == ticket
            {
                return Ok(old.clone());
            }
            state.revoked = true;
        } else {
            anyhow::ensure!(
                self.cells.len() < 128,
                "credential delivery registry limit reached"
            );
        }
        let cell = Arc::new(Mutex::new(CredentialDelivery {
            id: credential.connection_id,
            generation: credential.generation,
            expires: credential.expires_unix_ms,
            ticket,
            revoked: false,
        }));
        cell.lock()
            .map_err(|_| std::io::Error::other("credential fence unavailable"))?
            .check()?;
        self.cells.insert(credential.connection_id, cell.clone());
        Ok(cell)
    }
    fn revoke(&mut self, id: Incarnation) {
        match self.epoch.checked_add(1) {
            Some(epoch) => self.epoch = epoch,
            None => self.close(),
        }
        if let Some(cell) = self.cells.remove(&id) {
            cell.lock().unwrap_or_else(|e| e.into_inner()).revoked = true;
        }
    }
    #[cfg(test)]
    fn retire(&mut self, id: Incarnation) {
        if let Some(cell) = self.cells.remove(&id) {
            cell.lock().unwrap_or_else(|e| e.into_inner()).revoked = true;
        }
    }
    fn close(&mut self) {
        self.closed = true;
        for cell in self.cells.values() {
            cell.lock().unwrap_or_else(|e| e.into_inner()).revoked = true;
        }
    }
}

/// One ticket for the entire TCP connection. An invalid old ticket is never
/// replaced while Hyper or rustls can still retain an earlier response.
#[derive(Clone)]
struct ConnectionFence {
    gate: NativeCatalogAdmission,
    ticket: Arc<Mutex<Option<IssueTicket>>>,
    credential: Arc<Mutex<Option<Arc<Mutex<CredentialDelivery>>>>>,
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
            credential: Arc::new(Mutex::new(None)),
            read_context: Arc::new(Mutex::new(None)),
            requests: Default::default(),
            flushed: Default::default(),
        }
    }
    async fn bind_request(&self, context: ReadAttemptContext) -> anyhow::Result<()> {
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
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(context.deadline())) => { self.fail_requests(); anyhow::bail!("previous output did not drain within request deadline"); },
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
    fn bind_credential(&self, cell: Arc<Mutex<CredentialDelivery>>) -> anyhow::Result<()> {
        let mut held = self
            .credential
            .lock()
            .map_err(|_| std::io::Error::other("credential fence unavailable"))?;
        if let Some(previous) = held.as_ref() {
            anyhow::ensure!(
                Arc::ptr_eq(previous, &cell),
                "HTTP connection cannot change credential identity or generation"
            );
        } else {
            *held = Some(cell.clone());
        }
        cell.lock()
            .map_err(|_| std::io::Error::other("credential fence unavailable"))?
            .check()?;
        Ok(())
    }
    /// Only validated evidence contexts bind this cell. Once captured it cannot
    /// be replaced while an old response may remain buffered in Hyper or TLS.
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
    fn capture(&self, selection: &airk_core::service_http::HttpSelection) -> anyhow::Result<()> {
        use airk_core::service_http::HttpSelection;
        let revision = match selection {
            HttpSelection::Scoped { selection } => selection.expected_catalog_revision,
            HttpSelection::Combined { selection } => selection.expected_catalog_revision,
            HttpSelection::ImportedLegacy { selection } => selection.expected_catalog_revision,
            HttpSelection::ImportedLegacyUnion { selection } => selection
                .selections
                .iter()
                .map(|member| member.expected_catalog_revision)
                .max()
                .ok_or_else(|| std::io::Error::other("empty imported HTTP authority"))?,
            // U105: an open connection has no saved catalog clock; any current
            // revision of its namespace admits it, and a later move fences the
            // captured ticket as for every other selection.
            HttpSelection::Open { .. } => airk_core::index_catalog::CatalogRevision::INITIAL,
        };
        let mut held = self
            .ticket
            .lock()
            .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?;
        if let Some(old) = *held {
            self.gate.check_issue(old)?;
        }
        let current = self
            .gate
            .capture_current_issue(selection.namespace(), revision)?;
        if let Some(old) = *held {
            anyhow::ensure!(
                old == current,
                "HTTP connection delivery generation changed"
            );
        } else {
            *held = Some(current);
        }
        Ok(())
    }
    fn authenticated_ticket(&self) -> anyhow::Result<IssueTicket> {
        let ticket = self
            .ticket
            .lock()
            .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?
            .ok_or_else(|| {
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
        if let Some(cell) = self
            .credential
            .lock()
            .map_err(|_| std::io::Error::other("credential fence unavailable"))?
            .as_ref()
        {
            cell.lock()
                .map_err(|_| std::io::Error::other("credential fence unavailable"))?
                .check()?;
        }
        if let Some(ticket) = *self
            .ticket
            .lock()
            .map_err(|_| std::io::Error::other("HTTP delivery fence unavailable"))?
        {
            self.gate.check_issue(ticket)?;
        }
        Ok(())
    }
    fn poll<T>(
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
                active.context.cancellation().is_cancelled() || Instant::now() >= active.deadline
            })
        {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "request delivery expired or canceled",
            )));
        }
        let held = match self.credential.lock() {
            Ok(held) => held,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("credential fence unavailable")));
            }
        };
        // Hold this cell through the actual raw socket write. Revoke cannot
        // acknowledge until any earlier permitted write has completed its poll.
        let credential = match held.as_ref().map(|cell| cell.lock()).transpose() {
            Ok(credential) => credential,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other("credential fence unavailable")));
            }
        };
        if let Some(credential) = credential.as_ref()
            && let Err(error) = credential.check()
        {
            return Poll::Ready(Err(error));
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
        let ticket = match self.ticket.lock() {
            Ok(held) => *held,
            Err(_) => {
                return Poll::Ready(Err(std::io::Error::other(
                    "HTTP delivery fence unavailable",
                )));
            }
        };
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
struct FencedIo<S> {
    inner: S,
    delivery: ConnectionFence,
}

/// Observe the TLS flush boundary while enforcement stays below TLS. Hyper's
/// HTTP/1 writer flushes its queued plaintext before this flush can succeed.
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
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(u64::MAX)
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
}
impl OutputBody {
    fn new(
        inner: http_body_util::combinators::BoxBody<Bytes, Infallible>,
        budget: Arc<Semaphore>,
        remaining: usize,
        context: ReadAttemptContext,
    ) -> Self {
        let deadline = tokio::time::Instant::from_std(context.deadline())
            .min(tokio::time::Instant::now() + Duration::from_secs(STREAM_SECONDS));
        Self {
            inner,
            budget,
            remaining,
            deadline: Box::pin(tokio::time::sleep_until(deadline)),
            context,
            done: false,
            delivery: None,
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
