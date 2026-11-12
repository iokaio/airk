// SPDX-License-Identifier: Apache-2.0
//! The service's `/admin` plane (split plan §7, Phase 5): launch codes (A1),
//! sessions, embedded assets and admin dispatch over the I3 contract.
//!
//! Every decision is `admin_contract`'s; this module supplies observations
//! (headers, the same-user peer proof, the session lookup, the injected
//! clock) and carries out the admitted route. An admin `call` hands the
//! decoded `ControlMethod` to the injected `Dispatch`, which the service sets
//! to its own control endpoint, so the browser reaches the same canonical
//! handlers as the pipe. See `CONTRACT.md`.
mod assets;
mod operations;
mod peer;
mod secrets;
#[cfg(test)]
mod tests;

pub(crate) use peer::ConnectionPeer;

use crate::admin_contract::{
    self as contract, AdminCode, Admission, CLEAR_SESSION_COOKIE, CSRF_HEADER, Effect, ErrorBody,
    ErrorClass, ExchangeResponse, FailureEnvelope, MAX_EXCHANGE_BODY_BYTES, MAX_REQUEST_BODY_BYTES,
    PathMatch, PeerProof, RequestFacts, ResponseKind, Route, SESSION_ABSOLUTE_MS, SESSION_IDLE_MS,
    SuccessEnvelope,
};
use airk_core::{
    identity::Incarnation,
    service::{AdminLaunchSecret, ControlMethod, ControlResponse},
};
use airk_mcp::remote::http::HttpResponse;
use bytes::Bytes;
use http::{HeaderMap, Request, Response};
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use serde_json::Value;
use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

/// The injected service clock (Unix milliseconds).
pub(crate) type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;
/// 256 random bits per call; `None` when the OS source failed.
pub(crate) type Random = Arc<dyn Fn() -> Option<[u8; 32]> + Send + Sync>;
/// Sends one control method to this service instance's canonical handlers
/// (the service supplies its own control endpoint).
pub(crate) type Dispatch = Arc<
    dyn Fn(ControlMethod) -> Pin<Box<dyn Future<Output = anyhow::Result<ControlResponse>> + Send>>
        + Send
        + Sync,
>;

/// Bodies are read under this deadline (the listener's header deadline).
const BODY_SECONDS: u64 = 15;
/// Response frames stay well under the listener's 1 MiB frame bound.
const CHUNK_BYTES: usize = 256 * 1024;

struct Secrets {
    codes: secrets::LaunchCodes,
    sessions: secrets::Sessions,
}

/// The admin plane of one service instance. Codes and sessions live only
/// in memory, so a restart ends all of them (§4.5 step 5).
pub(crate) struct AdminPlane {
    dispatch: Dispatch,
    instance: Incarnation,
    clock: Clock,
    random: Random,
    secrets: Mutex<Secrets>,
}

fn system_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn system_random() -> Random {
    let source = ring::rand::SystemRandom::new();
    Arc::new(move || {
        let mut bytes = [0u8; 32];
        ring::rand::SecureRandom::fill(&source, &mut bytes).ok()?;
        Some(bytes)
    })
}

impl AdminPlane {
    /// The plane of service `instance`, whose calls go to `dispatch`, with
    /// the system clock and the OS random source.
    pub(crate) fn new(dispatch: Dispatch, instance: Incarnation) -> Self {
        Self::with_sources(dispatch, instance, Arc::new(system_clock), system_random())
    }

    pub(crate) fn with_sources(
        dispatch: Dispatch,
        instance: Incarnation,
        clock: Clock,
        random: Random,
    ) -> Self {
        Self {
            dispatch,
            instance,
            clock,
            random,
            secrets: Mutex::new(Secrets {
                codes: secrets::LaunchCodes::new(instance),
                sessions: secrets::Sessions::new(instance),
            }),
        }
    }

    fn secrets(&self) -> std::sync::MutexGuard<'_, Secrets> {
        self.secrets
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// A1 (contracts §2.7): issue one launch code for the listener on `port`.
    /// The reply is the only place the code text exists.
    pub(crate) fn launch_code(&self, port: u16) -> ControlResponse {
        let unavailable = || ControlResponse::Failure {
            code: "admin_launch_unavailable".into(),
        };
        let Some(random) = (self.random)() else {
            return unavailable();
        };
        let now = (self.clock)();
        let (text, expires_in_ms) = self.secrets().codes.issue(&random, now);
        match AdminLaunchSecret::try_from(text) {
            Ok(code) => ControlResponse::AdminLaunchCode {
                code,
                port,
                expires_in_ms,
            },
            Err(_) => unavailable(),
        }
    }

    /// Serve one admin-plane request (`contract::route` placed it here).
    /// `port` is the listener's; `draining` refuses API work while the
    /// listener closes.
    pub(crate) async fn handle<B>(
        &self,
        request: Request<B>,
        peer: Option<ConnectionPeer>,
        port: u16,
        draining: bool,
    ) -> HttpResponse
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let target = request
            .uri()
            .path_and_query()
            .map_or_else(String::new, |target| target.as_str().to_owned());
        let api = matches!(
            contract::route(target.as_bytes()),
            PathMatch::Route { route } if is_api(&route)
        );
        // Static GETs need no proof; it is observed once per connection.
        let proof = match (&peer, api) {
            (Some(peer), true) => peer.proof().await,
            _ => PeerProof::Unavailable,
        };
        let now = (self.clock)();
        let facts = request_facts(&request, target, port, now, proof);
        let cookies: Vec<String> = contract::admin_cookies(&facts.cookie)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let csrf: Vec<String> = request
            .headers()
            .get_all(CSRF_HEADER)
            .iter()
            .filter_map(|value| value.to_str().ok().map(str::to_owned))
            .collect();
        let admission = {
            let mut secrets = self.secrets();
            let cookie_refs: Vec<&str> = cookies.iter().map(String::as_str).collect();
            let csrf_refs: Vec<&str> = csrf.iter().map(String::as_str).collect();
            let session = secrets.sessions.facts(&cookie_refs, &csrf_refs, now);
            let admission = contract::admit(&facts, session.as_ref());
            if let (
                Admission::Admit {
                    touches_idle: true, ..
                },
                [cookie],
            ) = (&admission, cookies.as_slice())
            {
                secrets.sessions.touch(cookie, now);
            }
            admission
        };
        let route = match admission {
            Admission::Admit { route, .. } => route,
            Admission::Refuse { code, .. } => return refusal(code),
            Admission::NotAdmin => return refusal(AdminCode::AdminNotFound),
        };
        if draining && is_api(&route) {
            return refusal(AdminCode::ServiceDraining);
        }
        match route {
            Route::Redirect => respond(308, ResponseKind::Redirect, None, None, Bytes::new()),
            Route::Shell | Route::Launch => static_asset(assets::SHELL, ResponseKind::Shell),
            Route::Asset { name } => static_asset(&name, ResponseKind::Asset),
            Route::Exchange => match self.exchange(request.into_body()).await {
                Ok(response) => response,
                Err(code) => refusal(code),
            },
            Route::Logout => {
                if let [cookie] = cookies.as_slice() {
                    self.secrets().sessions.remove(cookie);
                }
                success(
                    "session.logout",
                    Value::Object(Default::default()),
                    Some(CLEAR_SESSION_COOKIE),
                )
            }
            Route::Call { operation } => match self.call(&operation, request.into_body()).await {
                Ok(response) => response,
                Err(code) => refusal(code),
            },
            // The bounded event stream (§4.8) is not served yet.
            Route::Events => refusal(AdminCode::NotYetServed),
        }
    }

    /// §4.5 step 3-4: consume the code, then mint a fresh session.
    async fn exchange<B>(&self, body: B) -> Result<HttpResponse, AdminCode>
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let body = read_body(body, MAX_EXCHANGE_BODY_BYTES).await?;
        let request = contract::decode_exchange(&body)?;
        let (Some(cookie), Some(csrf)) = ((self.random)(), (self.random)()) else {
            return Err(AdminCode::AdminInternal);
        };
        let now = (self.clock)();
        let minted = {
            let mut secrets = self.secrets();
            let stored = secrets.codes.consume(&request.code);
            contract::exchange(&request, stored.as_ref(), now)?;
            secrets.sessions.create(&cookie, &csrf, now)
        };
        let result = ExchangeResponse {
            csrf: minted.csrf,
            idle_expires_in_ms: SESSION_IDLE_MS,
            absolute_expires_in_ms: SESSION_ABSOLUTE_MS,
            service_instance: self.instance.to_string(),
        };
        let result = serde_json::to_value(&result).map_err(|_| AdminCode::AdminInternal)?;
        let cookie = contract::session_cookie(&minted.cookie);
        Ok(success("session.exchange", result, Some(&cookie)))
    }

    /// One reviewed operation through the canonical control handler.
    async fn call<B>(&self, operation: &str, body: B) -> Result<HttpResponse, AdminCode>
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let table = operations::table().map_err(|_| AdminCode::AdminInternal)?;
        let row = table
            .admin_row(operation)
            .ok_or(AdminCode::AdminOperationUnknown)?;
        let body = read_body(body, MAX_REQUEST_BODY_BYTES).await?;
        let method: ControlMethod =
            serde_json::from_slice(&body).map_err(|_| AdminCode::AdminMalformedRequest)?;
        let encoded = serde_json::to_value(&method).map_err(|_| AdminCode::AdminInternal)?;
        // Defense in depth: the validated table already names none of these.
        if table.never_over_http(&encoded) {
            return Err(AdminCode::AdminOperationUnknown);
        }
        if !operations::names(&row.method, &encoded) {
            return Err(AdminCode::AdminMalformedRequest);
        }
        // Mutations need the service journal with a server-generated key
        // (§4.4, §2.4); they are not served until that path lands.
        if row.effect != Effect::Read {
            return Err(AdminCode::NotYetServed);
        }
        let response = (self.dispatch)(method)
            .await
            .map_err(|_| AdminCode::AdminInternal)?;
        Ok(native(operation, response))
    }

    #[cfg(test)]
    fn outstanding_codes(&self) -> usize {
        self.secrets().codes.outstanding()
    }

    #[cfg(test)]
    fn sessions(&self) -> usize {
        self.secrets().sessions.len()
    }
}

fn is_api(route: &Route) -> bool {
    matches!(
        route,
        Route::Exchange | Route::Logout | Route::Call { .. } | Route::Events
    )
}

fn values(headers: &HeaderMap, name: &'static str) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
        .collect()
}

fn count(headers: &HeaderMap, name: &'static str) -> u32 {
    u32::try_from(headers.get_all(name).iter().count()).unwrap_or(u32::MAX)
}

fn request_facts<B>(
    request: &Request<B>,
    target: String,
    port: u16,
    now_ms: u64,
    peer: PeerProof,
) -> RequestFacts {
    let headers = request.headers();
    let first = |name: &'static str| values(headers, name).into_iter().next();
    let content_length = match values(headers, "content-length").as_slice() {
        [only] => only.trim().parse().ok(),
        _ => None,
    };
    let upgrade = headers.contains_key("upgrade")
        || values(headers, "connection").iter().any(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        });
    RequestFacts {
        listener_port: port,
        now_ms,
        method: request.method().as_str().to_owned(),
        target,
        host: values(headers, "host"),
        origin: values(headers, "origin"),
        sec_fetch_site: values(headers, "sec-fetch-site"),
        sec_fetch_mode: first("sec-fetch-mode"),
        sec_fetch_dest: first("sec-fetch-dest"),
        content_type: values(headers, "content-type"),
        content_length,
        upgrade,
        authorization: count(headers, "authorization"),
        cookie: values(headers, "cookie"),
        csrf_headers: count(headers, CSRF_HEADER),
        peer,
    }
}

async fn read_body<B>(body: B, limit: u64) -> Result<Bytes, AdminCode>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let limited = http_body_util::Limited::new(body, limit);
    match tokio::time::timeout(Duration::from_secs(BODY_SECONDS), limited.collect()).await {
        Ok(Ok(collected)) => Ok(collected.to_bytes()),
        Ok(Err(error)) if error.is::<http_body_util::LengthLimitError>() => {
            Err(AdminCode::AdminRequestTooLarge)
        }
        _ => Err(AdminCode::AdminMalformedRequest),
    }
}

/// A body sent in bounded frames, so a large native result never exceeds
/// the listener's per-frame bound.
struct Chunks {
    bytes: Bytes,
}

impl Body for Chunks {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        if self.bytes.is_empty() {
            return Poll::Ready(None);
        }
        let take = self.bytes.len().min(CHUNK_BYTES);
        let frame = self.bytes.split_to(take);
        Poll::Ready(Some(Ok(Frame::data(frame))))
    }
    fn is_end_stream(&self) -> bool {
        self.bytes.is_empty()
    }
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.bytes.len() as u64)
    }
}

fn respond(
    status: u16,
    kind: ResponseKind,
    content_type: Option<&str>,
    set_cookie: Option<&str>,
    body: Bytes,
) -> HttpResponse {
    let mut builder = Response::builder().status(status);
    for (name, value) in contract::response_headers(kind) {
        builder = builder.header(name, value);
    }
    if let Some(content_type) = content_type {
        builder = builder.header(http::header::CONTENT_TYPE, content_type);
    }
    if let Some(cookie) = set_cookie {
        builder = builder.header(http::header::SET_COOKIE, cookie);
    }
    builder = builder.header(http::header::CONTENT_LENGTH, body.len().to_string());
    builder
        .body(Chunks { bytes: body }.boxed())
        .unwrap_or_else(|_| {
            let mut response = Response::new(
                Chunks {
                    bytes: Bytes::new(),
                }
                .boxed(),
            );
            *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
            response
        })
}

fn api(status: u16, envelope: &impl serde::Serialize, set_cookie: Option<&str>) -> HttpResponse {
    let body = serde_json::to_vec(envelope).unwrap_or_default();
    respond(
        status,
        ResponseKind::Api,
        None,
        set_cookie,
        Bytes::from(body),
    )
}

/// An admin-plane refusal in the §4.4 envelope.
pub(crate) fn refusal(code: AdminCode) -> HttpResponse {
    api(code.status(), &FailureEnvelope::new(code), None)
}

fn success(operation: &str, result: Value, set_cookie: Option<&str>) -> HttpResponse {
    let envelope = SuccessEnvelope {
        schema: contract::ENVELOPE_SCHEMA.to_owned(),
        ok: true,
        operation: operation.to_owned(),
        result,
        request: None,
    };
    api(200, &envelope, set_cookie)
}

fn static_asset(name: &str, kind: ResponseKind) -> HttpResponse {
    match assets::find(name) {
        Some(asset) => {
            let content_type = (kind == ResponseKind::Asset).then_some(asset.content_type);
            respond(
                200,
                kind,
                content_type,
                None,
                Bytes::from_static(asset.bytes),
            )
        }
        None => refusal(AdminCode::AdminNotFound),
    }
}

fn snake_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 128
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The complete native response: success with the whole result, or a
/// typed refusal (409, class `refused`) that keeps the native response.
fn native(operation: &str, response: ControlResponse) -> HttpResponse {
    let Ok(value) = serde_json::to_value(&response) else {
        return refusal(AdminCode::AdminInternal);
    };
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let refused = status == "failure"
        || status == "incompatible"
        || status.ends_with("_refused")
        || status.ends_with("_failed");
    if !refused {
        return success(operation, value, None);
    }
    let code = match status {
        "failure" => value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("failure"),
        "incompatible" => "runtime_incompatible",
        other => other,
    };
    let code = if snake_code(code) {
        code.to_owned()
    } else {
        "native_refusal".to_owned()
    };
    // A native code spelled like an admin code keeps that code's class.
    if let Some(admin) = AdminCode::parse(&code) {
        let mut envelope = FailureEnvelope::new(admin);
        envelope.error.native = Some(value);
        return api(admin.status(), &envelope, None);
    }
    let retryable = code == "registry_busy" || code == "runtime_upgrade_in_progress";
    let envelope = FailureEnvelope {
        schema: contract::ENVELOPE_SCHEMA.to_owned(),
        ok: false,
        error: ErrorBody {
            code,
            class: ErrorClass::Refused,
            retryable,
            retry_after_ms: None,
            native: Some(value),
        },
    };
    api(ErrorClass::Refused.status(), &envelope, None)
}
