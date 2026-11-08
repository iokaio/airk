// SPDX-License-Identifier: Apache-2.0
//! Authenticated transport adapters. TLS, credential verification and native scope
//! enrollment remain the owning CLI listener's responsibility on every request.
use super::{
    ReadCallError, ReadClientFactory, ReadSession, ReadSessionFactory, ReadSessionPreferences,
    RemoteTools,
};
use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::{BodyExt, Full, Limited, combinators::BoxBody};
use rmcp::transport::streamable_http_server::{
    SessionId, SessionManager, StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    fmt,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
#[path = "remote_http_tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(super) struct ReadResponsePreference(pub crate::response::ResponseMode);

pub type HttpResponse = Response<BoxBody<Bytes, Infallible>>;
/// Must create a session-local backend bound to the already authenticated exact
/// credential, grant, namespace, service instance and endpoint generation.
pub type AuthenticatedBackendFactory =
    Arc<dyn Fn() -> anyhow::Result<Arc<dyn RemoteTools>> + Send + Sync>;

#[derive(Clone, Copy)]
pub struct HttpReadLimits {
    pub request_bytes: usize,
    pub response_bytes: usize,
    pub sessions_per_client: usize,
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
}
impl Default for HttpReadLimits {
    fn default() -> Self {
        Self {
            request_bytes: airk_core::service::MAX_FRAME_BYTES,
            response_bytes: super::PUBLIC_TOOL_ENVELOPE_BYTES,
            sessions_per_client: 8,
            handshake_timeout: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(300),
        }
    }
}
impl HttpReadLimits {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=airk_core::service::MAX_FRAME_BYTES).contains(&self.request_bytes)
                && (airk_core::service::MAX_FRAME_BYTES..=4 * 1024 * 1024)
                    .contains(&self.response_bytes)
                && (1..=32).contains(&self.sessions_per_client)
                && !self.handshake_timeout.is_zero()
                && self.handshake_timeout <= Duration::from_secs(60)
                && !self.idle_timeout.is_zero()
                && self.idle_timeout <= Duration::from_secs(600),
            "invalid local HTTP bounds"
        );
        Ok(())
    }
}

/// One global budget, cloned into every credential's independently scoped service.
#[derive(Clone)]
pub struct HttpSessionBudget(Arc<Semaphore>);
impl HttpSessionBudget {
    pub fn new(limit: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (1..=256).contains(&limit),
            "invalid global HTTP session bound"
        );
        Ok(Self(Arc::new(Semaphore::new(limit))))
    }
}

struct SessionLease {
    _global: OwnedSemaphorePermit,
    _client: OwnedSemaphorePermit,
    expires: tokio::time::Instant,
}
struct SessionState {
    inner: Arc<LocalSessionManager>,
    sessions: Mutex<HashMap<SessionId, SessionLease>>,
    global: HttpSessionBudget,
    client: Arc<Semaphore>,
    idle: Duration,
    handshake: Duration,
    closed: CancellationToken,
}
#[derive(Clone)]
struct BoundedSessions(Arc<SessionState>);
#[derive(Debug)]
struct SessionError;
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HTTP session unavailable; initialize a new session")
    }
}
impl std::error::Error for SessionError {}
impl BoundedSessions {
    fn new(global: HttpSessionBudget, limits: HttpReadLimits, closed: CancellationToken) -> Self {
        let mut inner = LocalSessionManager::default();
        inner.session_config.channel_capacity = 8;
        inner.session_config.keep_alive = Some(limits.idle_timeout);
        inner.session_config.init_timeout = Some(limits.handshake_timeout);
        inner.session_config.completed_cache_ttl = Duration::from_secs(5);
        inner.session_config.sse_retry = None;
        Self(Arc::new(SessionState {
            inner: Arc::new(inner),
            sessions: Mutex::new(HashMap::new()),
            global,
            client: Arc::new(Semaphore::new(limits.sessions_per_client)),
            idle: limits.idle_timeout,
            handshake: limits.handshake_timeout,
            closed,
        }))
    }
    fn touch(&self, id: &SessionId, renew: bool) -> Result<(), SessionError> {
        if self.0.closed.is_cancelled() {
            return Err(SessionError);
        }
        let mut sessions = self.0.sessions.lock().map_err(|_| SessionError)?;
        let session = sessions.get_mut(id).ok_or(SessionError)?;
        let now = tokio::time::Instant::now();
        if now >= session.expires {
            return Err(SessionError);
        }
        if renew {
            session.expires = now + self.0.idle;
        }
        Ok(())
    }
    async fn remove(&self, id: &SessionId) -> Result<(), SessionError> {
        self.0
            .inner
            .close_session(id)
            .await
            .map_err(|_| SessionError)?;
        self.0.sessions.lock().map_err(|_| SessionError)?.remove(id);
        Ok(())
    }
    async fn shutdown(&self) {
        self.0.closed.cancel();
        let ids: Vec<_> = self
            .0
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.remove(&id).await;
        }
    }
}
async fn expire(weak: Weak<SessionState>, id: SessionId) {
    loop {
        let Some(state) = weak.upgrade() else {
            return;
        };
        let deadline = state
            .sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&id).map(|entry| entry.expires));
        let Some(deadline) = deadline else {
            return;
        };
        let closed = state.closed.clone();
        drop(state);
        tokio::select! {_=tokio::time::sleep_until(deadline)=>{},_=closed.cancelled()=>{}}
        let Some(state) = weak.upgrade() else {
            return;
        };
        let expired = state.closed.is_cancelled()
            || state
                .sessions
                .lock()
                .ok()
                .and_then(|sessions| {
                    sessions
                        .get(&id)
                        .map(|entry| tokio::time::Instant::now() >= entry.expires)
                })
                .unwrap_or(true);
        if expired {
            let _ = BoundedSessions(state).remove(&id).await;
            return;
        }
    }
}
impl SessionManager for BoundedSessions {
    type Error = SessionError;
    type Transport = <LocalSessionManager as SessionManager>::Transport;
    async fn create_session(&self) -> Result<(SessionId, Self::Transport), Self::Error> {
        if self.0.closed.is_cancelled() {
            return Err(SessionError);
        }
        let global = self
            .0
            .global
            .0
            .clone()
            .try_acquire_owned()
            .map_err(|_| SessionError)?;
        let client = self
            .0
            .client
            .clone()
            .try_acquire_owned()
            .map_err(|_| SessionError)?;
        let (id, transport) = self
            .0
            .inner
            .create_session()
            .await
            .map_err(|_| SessionError)?;
        self.0.sessions.lock().map_err(|_| SessionError)?.insert(
            id.clone(),
            SessionLease {
                _global: global,
                _client: client,
                expires: tokio::time::Instant::now() + self.0.handshake.min(self.0.idle),
            },
        );
        tokio::spawn(expire(Arc::downgrade(&self.0), id.clone()));
        Ok((id, transport))
    }
    async fn initialize_session(
        &self,
        id: &SessionId,
        message: rmcp::model::ClientJsonRpcMessage,
    ) -> Result<rmcp::model::ServerJsonRpcMessage, Self::Error> {
        self.touch(id, false)?;
        let result = self
            .0
            .inner
            .initialize_session(id, message)
            .await
            .map_err(|_| SessionError)?;
        self.touch(id, true)?;
        Ok(result)
    }
    async fn has_session(&self, id: &SessionId) -> Result<bool, Self::Error> {
        if self.touch(id, true).is_err() {
            return Ok(false);
        }
        self.0.inner.has_session(id).await.map_err(|_| SessionError)
    }
    async fn close_session(&self, id: &SessionId) -> Result<(), Self::Error> {
        self.remove(id).await
    }
    async fn create_stream(
        &self,
        id: &SessionId,
        message: rmcp::model::ClientJsonRpcMessage,
    ) -> Result<
        impl futures::Stream<Item = rmcp::transport::common::server_side_http::ServerSseMessage>
        + Send
        + Sync
        + 'static,
        Self::Error,
    > {
        self.touch(id, true)?;
        self.0
            .inner
            .create_stream(id, message)
            .await
            .map_err(|_| SessionError)
    }
    async fn accept_message(
        &self,
        id: &SessionId,
        message: rmcp::model::ClientJsonRpcMessage,
    ) -> Result<(), Self::Error> {
        self.touch(id, true)?;
        self.0
            .inner
            .accept_message(id, message)
            .await
            .map_err(|_| SessionError)
    }
    async fn create_standalone_stream(
        &self,
        id: &SessionId,
    ) -> Result<
        impl futures::Stream<Item = rmcp::transport::common::server_side_http::ServerSseMessage>
        + Send
        + Sync
        + 'static,
        Self::Error,
    > {
        self.touch(id, true)?;
        self.0
            .inner
            .create_standalone_stream(id)
            .await
            .map_err(|_| SessionError)
    }
    async fn resume(
        &self,
        _id: &SessionId,
        _last_event_id: String,
    ) -> Result<
        impl futures::Stream<Item = rmcp::transport::common::server_side_http::ServerSseMessage>
        + Send
        + Sync
        + 'static,
        Self::Error,
    > {
        Err::<futures::stream::Empty<_>, _>(SessionError)
    }
}

/// Keep one instance per authenticated credential/generation. The listener must
/// authenticate and revalidate that binding before every call, even a session GET.
#[derive(Clone)]
pub struct HttpReadService(Arc<HttpState>);
struct HttpState {
    mcp: StreamableHttpService<ReadSession, BoundedSessions>,
    sessions: Arc<BoundedSessions>,
    client: ReadClientFactory,
    backend: AuthenticatedBackendFactory,
    preferences: ReadSessionPreferences,
    authority: String,
    origins: Vec<String>,
    limits: HttpReadLimits,
    closed: CancellationToken,
}
impl Drop for HttpState {
    fn drop(&mut self) {
        self.closed.cancel();
        self.client.close();
    }
}
impl HttpReadService {
    pub fn new(
        factory: ReadSessionFactory,
        budget: HttpSessionBudget,
        backend: AuthenticatedBackendFactory,
        preferences: ReadSessionPreferences,
        authority: String,
        origins: Vec<String>,
        limits: HttpReadLimits,
    ) -> anyhow::Result<Self> {
        limits.validate()?;
        let parsed: http::uri::Authority = authority.parse()?;
        anyhow::ensure!(
            parsed.host() == "127.0.0.1" && parsed.port_u16().is_some_and(|port| port > 0),
            "HTTP authority must be literal IPv4 loopback and its actual port"
        );
        anyhow::ensure!(
            origins
                .iter()
                .all(|origin| origin == &format!("https://{authority}")),
            "only the exact local HTTPS origin may be allowed"
        );
        let closed = CancellationToken::new();
        let sessions = Arc::new(BoundedSessions::new(budget, limits, closed.clone()));
        let client = factory.client();
        let session_factory = client.clone();
        let create = backend.clone();
        let config = StreamableHttpServerConfig::default()
            .with_allowed_hosts([authority.clone()])
            .with_allowed_origins(origins.clone())
            .with_json_response(true)
            .with_max_request_body_bytes(limits.request_bytes)
            .with_cancellation_token(closed.clone())
            .with_sse_retry(None);
        let mcp = StreamableHttpService::new(
            move || {
                create()
                    .map(|backend| session_factory.session(backend, preferences))
                    .map_err(|_| std::io::Error::other("authenticated AIRK session is unavailable"))
            },
            sessions.clone(),
            config,
        );
        Ok(Self(Arc::new(HttpState {
            mcp,
            sessions,
            client,
            backend,
            preferences,
            authority,
            origins,
            limits,
            closed,
        })))
    }
    fn json_session(&self) -> Result<ReadSession, Box<HttpResponse>> {
        let backend = (self.0.backend)().map_err(|_| {
            Box::new(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "native_scope_unavailable",
            ))
        })?;
        let mut preferences = self.0.preferences;
        preferences.response_mode = crate::response::ResponseMode::Full;
        Ok(self.0.client.session(backend, preferences))
    }
    pub async fn close(&self) {
        self.0.client.close();
        self.0.sessions.shutdown().await;
    }
    pub async fn handle<B>(&self, request: Request<B>) -> HttpResponse
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: fmt::Display + Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.handle_with_context(
            request,
            crate::read_attempt::ReadAttemptContext::new(
                std::time::Instant::now() + Duration::from_secs(125),
                self.0.closed.child_token(),
            ),
        )
        .await
    }
    /// The TLS listener captures this context before body parsing, authenticates
    /// every request and keeps the same cancellation/deadline through dispatch.
    pub async fn handle_with_context<B>(
        &self,
        mut request: Request<B>,
        context: crate::read_attempt::ReadAttemptContext,
    ) -> HttpResponse
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: fmt::Display + Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        if self.0.closed.is_cancelled() {
            return error(StatusCode::SERVICE_UNAVAILABLE, "service_draining");
        }
        if context.cancellation().is_cancelled() {
            return error(StatusCode::SERVICE_UNAVAILABLE, "request_canceled");
        }
        if std::time::Instant::now() >= context.deadline() {
            return error(StatusCode::GATEWAY_TIMEOUT, "request_deadline_exhausted");
        }
        if request.headers().get_all(http::header::HOST).iter().count() != 1
            || request
                .headers()
                .get(http::header::HOST)
                .and_then(|v| v.to_str().ok())
                != Some(self.0.authority.as_str())
            || request
                .uri()
                .authority()
                .is_some_and(|authority| authority.as_str() != self.0.authority)
        {
            return error(StatusCode::FORBIDDEN, "host_refused");
        }
        if request
            .headers()
            .get_all(http::header::ORIGIN)
            .iter()
            .count()
            > 1
        {
            return error(StatusCode::FORBIDDEN, "origin_refused");
        }
        if request.headers().contains_key(http::header::ORIGIN)
            && !request
                .headers()
                .get(http::header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|value| self.0.origins.iter().any(|origin| origin == value))
        {
            return error(StatusCode::FORBIDDEN, "origin_refused");
        }
        if request.uri().query().is_some() {
            return error(StatusCode::BAD_REQUEST, "query_parameters_unsupported");
        }
        let preference = match request
            .headers()
            .get_all("x-airk-response-mode")
            .iter()
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] => None,
            [value] => match value.to_str().ok() {
                Some("terse") => Some(crate::response::ResponseMode::Terse),
                Some("full") => Some(crate::response::ResponseMode::Full),
                _ => return error(StatusCode::BAD_REQUEST, "invalid_response_mode"),
            },
            _ => return error(StatusCode::BAD_REQUEST, "invalid_response_mode"),
        };
        let path = request.uri().path().to_owned();
        if path == "/mcp" {
            request.extensions_mut().insert(context.clone());
            if let Some(preference) = preference {
                request
                    .extensions_mut()
                    .insert(ReadResponsePreference(preference));
            }
            let response = tokio::select! {
                _=context.cancellation().cancelled()=>return error(StatusCode::SERVICE_UNAVAILABLE,"request_canceled"),
                response=tokio::time::timeout_at(tokio::time::Instant::from_std(context.deadline()),self.0.mcp.handle(request))=>match response {
                    Ok(response)=>response,Err(_)=>{context.cancellation().cancel();return error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted");}
                }
            };
            return if response.status() == StatusCode::INTERNAL_SERVER_ERROR {
                error(StatusCode::SERVICE_UNAVAILABLE, "mcp_transport_unavailable")
            } else {
                response
            };
        }
        if matches!(preference, Some(crate::response::ResponseMode::Terse)) {
            return error(StatusCode::BAD_REQUEST, "json_api_requires_full_response");
        }
        if request.headers().contains_key("mcp-session-id") {
            return error(StatusCode::BAD_REQUEST, "json_api_has_no_mcp_session");
        }
        if path == "/api/v1/tools" {
            if request.method() != Method::GET {
                return method_not_allowed("GET");
            }
            let session = match self.json_session() {
                Ok(session) => session,
                Err(response) => return *response,
            };
            let tools = session.read_catalog();
            let aliases:Vec<_>=tools.iter().filter_map(|tool| {
                let canonical=tool.name.strip_prefix("airk_").unwrap_or(&tool.name);
                crate::local::tools().iter().find(|native|native.name.strip_prefix("airk_")==Some(canonical)).map(|native|serde_json::json!({"canonical":tool.name,"aliases":[if tool.name==native.name {canonical}else{native.name.as_ref()}]}))
            }).collect();
            return json(
                StatusCode::OK,
                &serde_json::json!({"schema":"airk.local-api/1","contract_version":1,"tools":tools,"aliases":aliases,
                "transport":{"call_result":"complete_mcp_tool_result","mcp_session":false}}),
                self.0.limits.response_bytes,
            );
        }
        let Some(name) = path.strip_prefix("/api/v1/tools/").filter(|name| {
            !name.is_empty()
                && name.len() <= 64
                && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        }) else {
            return error(StatusCode::NOT_FOUND, "route_not_found");
        };
        if request.method() != Method::POST {
            return method_not_allowed("POST");
        }
        if !request
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
            })
        {
            return error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "json_content_type_required",
            );
        }
        let body = match tokio::time::timeout_at(
            tokio::time::Instant::from_std(context.deadline()),
            Limited::new(request.into_body(), self.0.limits.request_bytes).collect(),
        )
        .await
        {
            Ok(Ok(body)) => body.to_bytes(),
            Ok(Err(failure))
                if failure
                    .downcast_ref::<http_body_util::LengthLimitError>()
                    .is_some() =>
            {
                return error(StatusCode::PAYLOAD_TOO_LARGE, "request_body_limit");
            }
            Ok(Err(_)) => return error(StatusCode::BAD_REQUEST, "invalid_request_body"),
            Err(_) => {
                context.cancellation().cancel();
                return error(StatusCode::GATEWAY_TIMEOUT, "request_deadline_exhausted");
            }
        };
        let arguments =
            match serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&body) {
                Ok(value) => value,
                Err(_) => return error(StatusCode::BAD_REQUEST, "argument_object_required"),
            };
        let session = match self.json_session() {
            Ok(session) => session,
            Err(response) => return *response,
        };
        let result = tokio::select! {
            _=context.cancellation().cancelled()=>return error(StatusCode::SERVICE_UNAVAILABLE,"request_canceled"),
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(context.deadline()),session.call_read_with_context(name.into(),arguments,context.clone()))=>match result {
                Ok(result)=>result,Err(_)=>{context.cancellation().cancel();return error(StatusCode::GATEWAY_TIMEOUT,"request_deadline_exhausted");}
            }
        };
        match result {
            Ok(result) => json(StatusCode::OK, &result, self.0.limits.response_bytes),
            Err(ReadCallError::UnknownTool) => error(StatusCode::NOT_FOUND, "read_tool_not_found"),
            Err(ReadCallError::Busy) => {
                error(StatusCode::TOO_MANY_REQUESTS, "query_admission_busy")
            }
            Err(ReadCallError::Closed | ReadCallError::Canceled) => {
                error(StatusCode::SERVICE_UNAVAILABLE, "service_draining")
            }
        }
    }
}
fn json(status: StatusCode, value: &impl serde::Serialize, max: usize) -> HttpResponse {
    match serde_json::to_vec(value) {
        Ok(bytes) if bytes.len() <= max => Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(bytes)).boxed())
            .expect("fixed HTTP response"),
        _ => error(StatusCode::BAD_GATEWAY, "canonical_response_limit"),
    }
}
fn error(status: StatusCode, code: &str) -> HttpResponse {
    let body = serde_json::json!({"schema":"airk.local-api-error/1","error":{"code":code}});
    Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body.to_string())).boxed())
        .expect("fixed HTTP error")
}
fn method_not_allowed(allow: &str) -> HttpResponse {
    let mut response = error(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    response.headers_mut().insert(
        http::header::ALLOW,
        allow.parse().expect("fixed allowed method"),
    );
    response
}
