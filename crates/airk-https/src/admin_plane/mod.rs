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
mod confirm;
mod mutations;
mod operations;
mod peer;
mod secrets;
#[cfg(test)]
mod tests;

/// The embedded admin bundle's I4 digests `(inventory, bundle)` (split Phase 7).
pub use assets::digests as embedded_asset_digests;
pub(crate) use peer::ConnectionPeer;

use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
    service::{
        AdminLaunchSecret, ControlMethod, ControlResponse, ToolSelection, WindowScopeSelection,
    },
};
use airk_endpoint::admin_contract::{
    self as contract, AdminCode, Admission, CLEAR_SESSION_COOKIE, CSRF_HEADER, Effect, ErrorBody,
    ErrorClass, ExchangeResponse, FailureEnvelope, KeyMode, MAX_EXCHANGE_BODY_BYTES,
    MAX_REQUEST_BODY_BYTES, PathMatch, PeerProof, RequestFacts, RequestKeyRule, RequestProgress,
    ResponseKind, ReviewedCall, Route, SESSION_ABSOLUTE_MS, SESSION_IDLE_MS, SuccessEnvelope,
};
use airk_mcp::remote::http::HttpResponse;
use bytes::Bytes;
use http::{HeaderMap, Request, Response};
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use operations::Served;
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
pub type Dispatch = Arc<
    dyn Fn(ControlMethod) -> Pin<Box<dyn Future<Output = anyhow::Result<ControlResponse>> + Send>>
        + Send
        + Sync,
>;

/// The A2 catalog check (the one W4 uses; `service_runtime::scope_grant`):
/// the selection's scope identity and catalog revision, or the refusal code.
pub type ScopeCheck = Arc<
    dyn Fn(
            WindowScopeSelection,
        )
            -> Pin<Box<dyn Future<Output = Result<(Sha256Digest, CatalogRevision), String>> + Send>>
        + Send
        + Sync,
>;
/// `https.trust_status`: the read-only trust inspection, run in the service.
pub type TrustInspect =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>> + Send + Sync>;

/// The scoped-read admission gate's side of an A2 grant's in-memory read
/// authority (selection authority §3.1, §3.3), injected by
/// `service_runtime`. `register` makes a new grant's nonce live; it is
/// called nested inside the plane's lock (owner, then gate; §3.7) and
/// answers whether the gate accepted it. `revoke` revokes dropped nonces and
/// is called only after the plane's lock is released.
#[derive(Clone)]
pub struct SelectionAuthority {
    pub register: Arc<dyn Fn(&SelectionScopeRef) -> bool + Send + Sync>,
    pub revoke: Arc<dyn Fn(Vec<Incarnation>) + Send + Sync>,
}

/// One A2 read (selection authority S2): the session's checked grant and
/// the browser's tools call. The nonce inside `selection_ref` reaches only
/// the in-process admission; the answer never carries it (N3).
pub struct SelectedRead {
    pub selection: WindowScopeSelection,
    pub scope_identity: Sha256Digest,
    pub selection_ref: SelectionScopeRef,
    pub tool: String,
    pub arguments: serde_json::Map<String, Value>,
    pub mode: airk_core::service::ToolResponseMode,
    pub client: airk_core::usage::ClientFamily,
}

/// The A2 read itself, run in process by `service_runtime` through the same
/// selection read path as W4 (one index, or the combined facade).
pub type SelectionReader = Arc<
    dyn Fn(SelectedRead) -> Pin<Box<dyn Future<Output = ControlResponse> + Send>> + Send + Sync,
>;

/// Bodies are read under this deadline (the listener's header deadline).
const BODY_SECONDS: u64 = 15;
/// Response frames stay well under the listener's 1 MiB frame bound.
const CHUNK_BYTES: usize = 256 * 1024;
/// A native failure message in an envelope, bounded as the CLI's error line.
const MAX_MESSAGE_CHARS: usize = 4096;

struct Secrets {
    codes: secrets::LaunchCodes,
    sessions: secrets::Sessions,
}

/// The admin plane of one service instance. Codes and sessions live only
/// in memory, so a restart ends all of them (§4.5 step 5).
pub struct AdminPlane {
    dispatch: Dispatch,
    instance: Incarnation,
    clock: Clock,
    random: Random,
    secrets: Mutex<Secrets>,
    /// Without it `search.select_scope` answers `not_yet_served`.
    scope_check: Option<ScopeCheck>,
    /// Without it `https.trust_status` answers `not_yet_served`.
    trust_inspect: Option<TrustInspect>,
    /// Without it an A2 grant holds no read authority.
    selection: Option<SelectionAuthority>,
    /// Without it a search under an A2 grant answers `not_yet_served`.
    selection_reader: Option<SelectionReader>,
}

/// The plane's lock. On release it revokes the selection nonces its holder
/// dropped (grant replacement, logout, expiry, eviction, service end), after
/// the lock itself is released (§3.3, §3.7, N4), so no path that drops a
/// grant can skip its revocation.
struct SecretsGuard<'a> {
    guard: Option<std::sync::MutexGuard<'a, Secrets>>,
    selection: Option<&'a SelectionAuthority>,
}

impl std::ops::Deref for SecretsGuard<'_> {
    type Target = Secrets;
    fn deref(&self) -> &Secrets {
        self.guard.as_ref().expect("held until drop")
    }
}

impl std::ops::DerefMut for SecretsGuard<'_> {
    fn deref_mut(&mut self) -> &mut Secrets {
        self.guard.as_mut().expect("held until drop")
    }
}

impl Drop for SecretsGuard<'_> {
    fn drop(&mut self) {
        let dropped = self
            .guard
            .as_mut()
            .map(|secrets| secrets.sessions.take_dropped())
            .unwrap_or_default();
        self.guard = None;
        if let (false, Some(selection)) = (dropped.is_empty(), self.selection) {
            (selection.revoke)(dropped);
        }
    }
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
    pub fn new(dispatch: Dispatch, instance: Incarnation) -> Self {
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
            scope_check: None,
            trust_inspect: None,
            selection: None,
            selection_reader: None,
        }
    }

    /// Serve searches under A2 grants (selection authority S2).
    pub fn with_selection_reader(mut self, reader: SelectionReader) -> Self {
        self.selection_reader = Some(reader);
        self
    }

    /// Give each A2 grant an in-memory read authority whose nonce is live
    /// with the scoped-read admission gate while the grant is stored.
    pub fn with_selection_authority(mut self, selection: SelectionAuthority) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Service end (§3.3): every session ends and every grant's nonce is
    /// revoked once the lock is released.
    pub fn end(&self) {
        self.secrets().sessions.end();
    }

    /// Serve A2 grants with the shared catalog check.
    pub fn with_scope_check(mut self, check: ScopeCheck) -> Self {
        self.scope_check = Some(check);
        self
    }

    /// Serve `https.trust_status` with the service's trust inspection.
    pub fn with_trust_inspect(mut self, inspect: TrustInspect) -> Self {
        self.trust_inspect = Some(inspect);
        self
    }

    fn secrets(&self) -> SecretsGuard<'_> {
        SecretsGuard {
            guard: Some(
                self.secrets
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()),
            ),
            selection: self.selection.as_ref(),
        }
    }

    /// A1 (contracts §2.7): issue one launch code for the listener on `port`.
    /// The reply is the only place the code text exists.
    pub fn launch_code(&self, port: u16) -> ControlResponse {
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
            Route::Call { operation } => {
                // Admission named exactly one admin cookie for this session.
                let cookie = match cookies.as_slice() {
                    [cookie] => Some(cookie.as_str()),
                    _ => None,
                };
                match self.call(&operation, cookie, request.into_body()).await {
                    Ok(response) => response,
                    Err(code) => refusal(code),
                }
            }
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
    async fn call<B>(
        &self,
        operation: &str,
        cookie: Option<&str>,
        body: B,
    ) -> Result<HttpResponse, AdminCode>
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let table = operations::table().map_err(|_| AdminCode::AdminInternal)?;
        let row = table
            .admin_row(operation)
            .ok_or(AdminCode::AdminOperationUnknown)?;
        let body = read_body(body, MAX_REQUEST_BODY_BYTES).await?;
        let body: Value =
            serde_json::from_slice(&body).map_err(|_| AdminCode::AdminMalformedRequest)?;
        // U98: a review-mode or destructive row takes a `ReviewedCall`; every
        // other row takes the bare method object.
        let review_rule = row
            .request_key
            .as_ref()
            .filter(|rule| rule.mode == KeyMode::Review);
        let (mut body, token, confirm_text) = if review_rule.is_some() || row.confirmation.is_some()
        {
            let call: ReviewedCall =
                serde_json::from_value(body).map_err(|_| AdminCode::AdminMalformedRequest)?;
            (call.method, call.review, call.confirm_text)
        } else {
            (body, None, None)
        };
        // Defense in depth: the validated table already names none of these.
        if table.never_over_http(&body) {
            return Err(AdminCode::AdminOperationUnknown);
        }
        let target = match operations::served(&row.method) {
            Served::Direct => row.method.as_str(),
            Served::Equivalent(method) => method,
            Served::TrustInspect => return self.trust_status(operation, &body).await,
        };
        if !operations::names(target, &body) {
            return Err(AdminCode::AdminMalformedRequest);
        }
        match (review_rule, token) {
            (Some(rule), Some(token)) => {
                let token: Incarnation = serde_json::from_value(Value::String(token))
                    .map_err(|_| AdminCode::AdminReviewUnknown)?;
                let now = (self.clock)();
                let review = cookie
                    .and_then(|cookie| self.secrets().sessions.review(cookie, &token, now))
                    .ok_or(AdminCode::AdminReviewUnknown)?;
                mutations::restore(rule, &mut body, &review)?;
            }
            (Some(_), None) => return Err(AdminCode::AdminReviewUnknown),
            (None, Some(_)) => return Err(AdminCode::AdminMalformedRequest),
            (None, None) => {}
        }
        let generate = row
            .request_key
            .as_ref()
            .filter(|rule| rule.mode == KeyMode::Generate);
        let key = match generate {
            Some(rule) => {
                match mutations::generate(&self.dispatch, &self.random, rule, &mut body).await? {
                    Ok(key) => Some(key),
                    Err(refused) => return Ok(native(operation, refused)),
                }
            }
            None => None,
        };
        let method: ControlMethod =
            serde_json::from_value(body).map_err(|_| AdminCode::AdminMalformedRequest)?;
        if let Some(rule) = row.confirmation {
            confirm::check(&self.dispatch, rule, &method, confirm_text.as_deref()).await?;
        } else if confirm_text.is_some() {
            return Err(AdminCode::AdminMalformedRequest);
        }
        if row.effect == Effect::Read {
            let Some((rule, key)) = generate.zip(key) else {
                return self.read(operation, cookie, method).await;
            };
            // A draft or preview: the identity stays in the session.
            let response = (self.dispatch)(method)
                .await
                .map_err(|_| AdminCode::AdminInternal)?;
            return self.issue(operation, cookie, rule, key, response, None);
        }
        if let ControlMethod::AdminScopeGrant { selection } = method {
            return self.grant(operation, cookie, selection).await;
        }
        let issued = generate.zip(key.clone());
        match mutations::execute(&self.dispatch, &self.random, method, key).await? {
            mutations::Executed::Answered(response, request) => match issued {
                Some((rule, key)) => self.issue(operation, cookie, rule, key, *response, request),
                None => Ok(native_request(operation, *response, request)),
            },
            mutations::Executed::Unknown => Ok(outcome_unknown()),
        }
    }

    /// The answer of a generate-mode row: the identity leaves the native
    /// result and stays in the session under a new review token, which the
    /// success envelope names (contracts §4.3b). A refusal issues no token.
    fn issue(
        &self,
        operation: &str,
        cookie: Option<&str>,
        rule: &RequestKeyRule,
        key: mutations::Key,
        response: ControlResponse,
        request: Option<RequestProgress>,
    ) -> Result<HttpResponse, AdminCode> {
        let mut value = serde_json::to_value(&response).map_err(|_| AdminCode::AdminInternal)?;
        if refused(&value) {
            return Ok(native_value(operation, value, request, None));
        }
        let key_value = serde_json::json!(key.key);
        let redacted = mutations::redact(&mut value, &key_value);
        let caller = rule
            .caller
            .as_ref()
            .map(|_| serde_json::json!(key.caller))
            .or(redacted);
        let token = mutations::identity(&self.random)?;
        let cookie = cookie.ok_or(AdminCode::AdminSessionInvalid)?;
        let now = (self.clock)();
        let review = secrets::Review {
            token,
            caller,
            key: key_value,
        };
        // The session may have ended while the native call ran.
        self.secrets()
            .sessions
            .add_review(cookie, review, now)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        Ok(native_value(
            operation,
            value,
            request,
            Some(token.to_string()),
        ))
    }

    /// A read row. Search runs only unselected (help) or under the
    /// session's A2 grant; a browser never names a project, catalog grant,
    /// combined session or window.
    async fn read(
        &self,
        operation: &str,
        cookie: Option<&str>,
        method: ControlMethod,
    ) -> Result<HttpResponse, AdminCode> {
        if let ControlMethod::ToolsCall { selection, .. } = &method {
            match selection {
                ToolSelection::Unselected => {}
                ToolSelection::Admin => return self.granted_read(operation, cookie, method).await,
                _ => return Err(AdminCode::AdminMalformedRequest),
            }
        }
        let response = (self.dispatch)(method)
            .await
            .map_err(|_| AdminCode::AdminInternal)?;
        Ok(native(operation, response))
    }

    /// M3 under the session's A2 grant. The grant is rechecked against the
    /// current catalog first; the read itself needs a scoped-read admission
    /// for in-memory grants, which does not exist yet (`CONTRACT.md`).
    async fn granted_read(
        &self,
        operation: &str,
        cookie: Option<&str>,
        method: ControlMethod,
    ) -> Result<HttpResponse, AdminCode> {
        let now = (self.clock)();
        let grant = cookie.and_then(|cookie| self.secrets().sessions.grant(cookie, now));
        let Some(grant) = grant else {
            let response = ControlResponse::Failure {
                code: "admin_scope_not_granted".into(),
            };
            return Ok(native(operation, response));
        };
        let check = self.scope_check.as_ref().ok_or(AdminCode::NotYetServed)?;
        match check(grant.selection.clone()).await {
            Ok(current) if current == (grant.scope_identity, grant.catalog_revision) => {
                let (Some(reader), Some(selection_ref)) =
                    (self.selection_reader.as_ref(), grant.selection_ref)
                else {
                    return Err(AdminCode::NotYetServed);
                };
                let ControlMethod::ToolsCall {
                    tool,
                    arguments,
                    response_mode,
                    usage_client,
                    ..
                } = method
                else {
                    return Err(AdminCode::AdminMalformedRequest);
                };
                let response = reader(SelectedRead {
                    selection: grant.selection,
                    scope_identity: grant.scope_identity,
                    selection_ref,
                    tool,
                    arguments,
                    mode: response_mode,
                    client: usage_client,
                })
                .await;
                Ok(native(operation, response))
            }
            Ok(_) => Ok(native(
                operation,
                ControlResponse::Failure {
                    code: "scoped_read_unavailable".into(),
                },
            )),
            Err(code) => Ok(native(operation, ControlResponse::Failure { code })),
        }
    }

    /// A2: check the selection, then bind the grant to this session and
    /// instance. A failed check leaves the previous grant (W4's rule).
    async fn grant(
        &self,
        operation: &str,
        cookie: Option<&str>,
        selection: WindowScopeSelection,
    ) -> Result<HttpResponse, AdminCode> {
        let check = self.scope_check.as_ref().ok_or(AdminCode::NotYetServed)?;
        let cookie = cookie.ok_or(AdminCode::AdminSessionInvalid)?;
        let (scope_identity, catalog_revision) = match check(selection.clone()).await {
            Ok(checked) => checked,
            Err(code) => return Ok(native(operation, ControlResponse::Failure { code })),
        };
        // A fresh nonce per stored grant (§3.1); a re-selection gets a new one.
        let selection_ref = match &self.selection {
            Some(_) => {
                let random = (self.random)().ok_or(AdminCode::AdminInternal)?;
                let mut nonce = [0u8; 16];
                nonce.copy_from_slice(&random[..16]);
                Some(SelectionScopeRef {
                    owner: SelectionOwnerKind::Admin,
                    grant_nonce: Incarnation::from_bytes(nonce),
                    service_instance: self.instance,
                    scope_identity,
                    catalog_revision,
                })
            }
            None => None,
        };
        let grant = secrets::Grant {
            selection,
            scope_identity,
            catalog_revision,
            instance: self.instance,
            selection_ref,
        };
        let now = (self.clock)();
        let mut secrets = self.secrets();
        // Registered nested inside the plane's lock (owner, then gate), so a
        // concurrent replacement or logout revokes it after this store.
        if let (Some(selection), Some(selection_ref)) = (&self.selection, &selection_ref)
            && !(selection.register)(selection_ref)
        {
            return Ok(native(
                operation,
                ControlResponse::Failure {
                    code: "scoped_read_unavailable".into(),
                },
            ));
        }
        // The session may have ended while the catalog was read; a grant
        // that is not stored is revoked when the lock is released.
        let expires_at_ms = secrets
            .sessions
            .set_grant(cookie, grant, now)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        drop(secrets);
        Ok(native(
            operation,
            ControlResponse::AdminScopeGranted {
                scope_identity,
                catalog_revision,
                expires_at_ms,
            },
        ))
    }

    /// `https.trust_status` (mapping `HttpTrust.Inspect`): the inspection's
    /// complete status document.
    async fn trust_status(&self, operation: &str, body: &Value) -> Result<HttpResponse, AdminCode> {
        if *body != serde_json::json!({"http_trust": {"action": "inspect"}}) {
            return Err(AdminCode::AdminMalformedRequest);
        }
        let inspect = self.trust_inspect.as_ref().ok_or(AdminCode::NotYetServed)?;
        Ok(match inspect().await {
            Ok(status) => success(operation, status, None),
            Err(error) => {
                let message: String = format!("{error:#}")
                    .chars()
                    .take(MAX_MESSAGE_CHARS)
                    .collect();
                let envelope = FailureEnvelope {
                    schema: contract::ENVELOPE_SCHEMA.to_owned(),
                    ok: false,
                    error: ErrorBody {
                        code: "operation_failed".into(),
                        class: ErrorClass::Refused,
                        retryable: false,
                        retry_after_ms: None,
                        native: Some(serde_json::json!({ "message": message })),
                    },
                    review: None,
                };
                api(ErrorClass::Refused.status(), &envelope, None)
            }
        })
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
    success_request(operation, result, set_cookie, None)
}

fn success_request(
    operation: &str,
    result: Value,
    set_cookie: Option<&str>,
    request: Option<RequestProgress>,
) -> HttpResponse {
    success_review(operation, result, set_cookie, request, None)
}

fn success_review(
    operation: &str,
    result: Value,
    set_cookie: Option<&str>,
    request: Option<RequestProgress>,
    review: Option<String>,
) -> HttpResponse {
    let envelope = SuccessEnvelope {
        schema: contract::ENVELOPE_SCHEMA.to_owned(),
        ok: true,
        operation: operation.to_owned(),
        result,
        request,
        review,
    };
    api(200, &envelope, set_cookie)
}

/// A mutation whose reply was lost twice: it may have been accepted, so it
/// is neither refused nor retryable with a new key (§2.4). A journaled
/// request stays listed by `requests.list`.
fn outcome_unknown() -> HttpResponse {
    let envelope = FailureEnvelope {
        schema: contract::ENVELOPE_SCHEMA.to_owned(),
        ok: false,
        error: ErrorBody {
            code: "service_request_outcome_unknown".into(),
            class: ErrorClass::Unavailable,
            retryable: false,
            retry_after_ms: None,
            native: None,
        },
        review: None,
    };
    api(ErrorClass::Unavailable.status(), &envelope, None)
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
    native_request(operation, response, None)
}

/// `native`, with the request a successful mutation belongs to; a refusal
/// carries no request (nothing was accepted under it).
fn native_request(
    operation: &str,
    response: ControlResponse,
    request: Option<RequestProgress>,
) -> HttpResponse {
    let Ok(value) = serde_json::to_value(&response) else {
        return refusal(AdminCode::AdminInternal);
    };
    native_value(operation, value, request, None)
}

/// Whether an encoded native response is a refusal.
fn refused(value: &Value) -> bool {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    status == "failure"
        || status == "incompatible"
        || status.ends_with("_refused")
        || status.ends_with("_failed")
}

/// `native_request` over an encoded response; `review` only on success.
fn native_value(
    operation: &str,
    value: Value,
    request: Option<RequestProgress>,
    review: Option<String>,
) -> HttpResponse {
    if !refused(&value) {
        return success_review(operation, value, None, request, review);
    }
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
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
        review: None,
    };
    api(ErrorClass::Refused.status(), &envelope, None)
}
