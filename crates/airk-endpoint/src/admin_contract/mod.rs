// SPDX-License-Identifier: Apache-2.0
//! The I3 admin HTTP contract (split plan §7; `docs/split/contracts.md` §4 as
//! amended by the adopted security review, §4.11 and
//! `docs/split/security-review-i3.md`).
//!
//! Contract step C5: types, pure decision functions and their reviewed
//! fixtures (`fixtures/split/i3/*.json`). Its one consumer is the Phase 5
//! admin plane handler. It is a leaf: it
//! depends only on serde, serde_json and schemars, never on another module of
//! this crate.
//!
//! Every decision takes injected observations (headers, peer proof, session
//! lookup result, clock), so each platform's and browser's rule is tested on
//! every host.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

mod schema;
pub use schema::document as schema_document;

/// The I3 contract version this build implements (path version `v1`).
pub const I3_CONTRACT: u32 = 1;
/// The `schema` field of every admin API response body.
pub const ENVELOPE_SCHEMA: &str = "airk-admin/1";
/// The only admin cookie name; matched exactly (case-sensitive).
pub const COOKIE_NAME: &str = "__Host-airk_admin";
/// The anti-forgery request header (HTTP header names are case-insensitive).
pub const CSRF_HEADER: &str = "x-airk-csrf";
/// Launch codes, cookies and anti-forgery tokens are independent 256-bit
/// random values, carried as unpadded base64url (43 characters).
pub const SECRET_TEXT_CHARS: usize = 43;
pub const LAUNCH_CODE_TTL_MS: u64 = 60_000;
pub const SESSION_IDLE_MS: u64 = 30 * 60 * 1000;
pub const SESSION_ABSOLUTE_MS: u64 = 12 * 60 * 60 * 1000;
pub const MAX_SESSIONS: usize = 16;
pub const MAX_EXCHANGE_BODY_BYTES: u64 = 1024;
pub const MAX_REQUEST_BODY_BYTES: u64 = 64 * 1024;
pub const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_STREAMS_PER_SESSION: usize = 2;
pub const MAX_STREAMS: usize = 16;
pub const STREAM_BUFFER_EVENTS: usize = 256;
pub const STREAM_BUFFER_BYTES: usize = 1024 * 1024;
pub const STREAM_SECONDS: u64 = 120;
pub const MAX_ASSET_NAME_BYTES: usize = 128;
pub const MAX_OPERATION_BYTES: usize = 64;
/// U98: the typed confirmation text of a destructive call.
pub const MAX_CONFIRM_TEXT_BYTES: usize = 256;
/// Review custody per session. Ordinary preview tokens retain their existing
/// eviction policy; reserved receipt-only recovery custody (§4.4.1) must not
/// be evicted to admit a mutation. The capacity is unchanged.
pub const MAX_REVIEWS_PER_SESSION: usize = 32;

pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";
pub const PERMISSIONS_POLICY: &str = "accelerometer=(), autoplay=(), camera=(), display-capture=(), \
geolocation=(), gyroscope=(), magnetometer=(), microphone=(), midi=(), payment=(), \
publickey-credentials-get=(), screen-wake-lock=(), serial=(), usb=(), xr-spatial-tracking=()";
pub const ASSET_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";
/// `Set-Cookie` that clears the session under `__Host-` rules (L4).
pub const CLEAR_SESSION_COOKIE: &str =
    "__Host-airk_admin=; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
/// Request headers whose values are never logged or reported (L9).
pub const REDACTED_REQUEST_HEADERS: &[&str] = &["authorization", "cookie", "x-airk-csrf"];
/// Body fields of the session exchange whose values are never logged (L9).
pub const EXCHANGE_SECRET_FIELDS: &[&str] = &["code", "csrf"];
/// Statuses hyper may answer bare, without the envelope, for an oversize or
/// malformed header block (cookie bombing is a denial of service only; L5).
pub const BARE_STATUS_EXCEPTIONS: &[u16] = &[400, 431];

/// The exact `Host` the listener accepts.
pub fn expected_host(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

/// The exact `Origin` the admin API accepts.
pub fn expected_origin(port: u16) -> String {
    format!("https://127.0.0.1:{port}")
}

/// The launch URL `airkctl admin open` delivers through its owner-only
/// redirect file. The code is in the fragment, never in a query (review I3).
pub fn launch_url(port: u16, code: &str) -> String {
    format!("https://127.0.0.1:{port}/admin/launch#c={code}")
}

/// The `Set-Cookie` value of a freshly minted session (M2 rule 2).
pub fn session_cookie(value: &str) -> String {
    format!("{COOKIE_NAME}={value}; Secure; HttpOnly; SameSite=Strict; Path=/")
}

/// Canonical unpadded base64url of exactly 32 bytes.
pub fn is_secret_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    let alphabet = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
    bytes.len() == SECRET_TEXT_CHARS
        && bytes.iter().all(|byte| alphabet(*byte))
        // 43 characters carry 258 bits; the final two must be zero.
        && b"AEIMQUYcgkosw048".contains(&bytes[SECRET_TEXT_CHARS - 1])
}

fn is_incarnation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_snake_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// `segment(.segment)+`, each segment `[a-z][a-z0-9_]*`, at most 64 bytes.
pub fn is_operation_id(value: &str) -> bool {
    value.len() <= MAX_OPERATION_BYTES
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            let mut bytes = segment.bytes();
            bytes.next().is_some_and(|first| first.is_ascii_lowercase())
                && bytes
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

/// An embedded asset name: dot-separated non-empty `[a-z0-9_-]` segments, the
/// first starting with `[a-z0-9]`, at most 128 bytes. No separator or dot
/// segment can appear, and lookup is by exact name in the sealed inventory.
pub fn is_asset_name(value: &str) -> bool {
    value.len() <= MAX_ASSET_NAME_BYTES
        && value
            .as_bytes()
            .first()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
        })
}

/// HTTP status classes of the admin API (§4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Malformed,
    Unauthenticated,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    /// A typed native refusal; nothing changed.
    Refused,
    TooLarge,
    Internal,
    Unavailable,
}

impl ErrorClass {
    pub const ALL: [Self; 9] = [
        Self::Malformed,
        Self::Unauthenticated,
        Self::Forbidden,
        Self::NotFound,
        Self::MethodNotAllowed,
        Self::Refused,
        Self::TooLarge,
        Self::Internal,
        Self::Unavailable,
    ];

    pub const fn status(self) -> u16 {
        match self {
            Self::Malformed => 400,
            Self::Unauthenticated => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::Refused => 409,
            Self::TooLarge => 413,
            Self::Internal => 500,
            Self::Unavailable => 503,
        }
    }
}

/// Codes the admin plane itself produces. Native refusals keep their own
/// code with class `refused`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminCode {
    AdminHostRefused,
    AdminOriginRefused,
    AdminFetchMetadataRefused,
    /// The TCP peer is not proven to run as the service's OS user (H1).
    AdminPeerRefused,
    /// CORS preflight; no CORS header is ever sent.
    AdminPreflightRefused,
    AdminBearerRefused,
    /// Zero or several admin cookies, an unknown cookie, an expired or
    /// foreign-instance session, or a missing or wrong anti-forgery token:
    /// one uniform answer (M2 rule 3).
    AdminSessionInvalid,
    /// Unknown, expired, used, foreign-instance or malformed launch code.
    AdminLaunchCodeInvalid,
    AdminNotFound,
    AdminOperationUnknown,
    AdminMethodNotAllowed,
    AdminUpgradeRefused,
    AdminMalformedRequest,
    AdminRequestTooLarge,
    AdminStreamLimit,
    AdminExchangeRateLimited,
    AdminInternal,
    NotYetServed,
    ServiceDraining,
    /// Admin mutations during an upgrade checkpoint (review I5).
    UpgradeInProgress,
    /// The exact admin cookie on `/mcp` or `/api/v1`, after their existing
    /// Host and `Origin` checks (L6; an I3 addition under U24).
    CookieOnBearerPlane,
    /// A review-mode call named no review token, or one this session does
    /// not hold (another session, logout, restart, eviction). Run the review
    /// again (U98).
    AdminReviewUnknown,
    /// Receipt-only recovery custody could not be reserved before dispatch.
    /// Nothing was sent; the existing review capacity is not enlarged.
    AdminReviewCapacity,
    /// A destructive call's typed text does not equal the target's current
    /// display name, or its digest does not match the review (U98).
    AdminConfirmationMismatch,
}

impl AdminCode {
    pub const ALL: [Self; 24] = [
        Self::AdminHostRefused,
        Self::AdminOriginRefused,
        Self::AdminFetchMetadataRefused,
        Self::AdminPeerRefused,
        Self::AdminPreflightRefused,
        Self::AdminBearerRefused,
        Self::AdminSessionInvalid,
        Self::AdminLaunchCodeInvalid,
        Self::AdminNotFound,
        Self::AdminOperationUnknown,
        Self::AdminMethodNotAllowed,
        Self::AdminUpgradeRefused,
        Self::AdminMalformedRequest,
        Self::AdminRequestTooLarge,
        Self::AdminStreamLimit,
        Self::AdminExchangeRateLimited,
        Self::AdminInternal,
        Self::NotYetServed,
        Self::ServiceDraining,
        Self::UpgradeInProgress,
        Self::CookieOnBearerPlane,
        Self::AdminReviewUnknown,
        Self::AdminReviewCapacity,
        Self::AdminConfirmationMismatch,
    ];

    pub const fn class(self) -> ErrorClass {
        match self {
            Self::AdminHostRefused
            | Self::AdminOriginRefused
            | Self::AdminFetchMetadataRefused
            | Self::AdminPeerRefused
            | Self::AdminPreflightRefused
            | Self::CookieOnBearerPlane => ErrorClass::Forbidden,
            Self::AdminBearerRefused | Self::AdminSessionInvalid | Self::AdminLaunchCodeInvalid => {
                ErrorClass::Unauthenticated
            }
            Self::AdminNotFound | Self::AdminOperationUnknown => ErrorClass::NotFound,
            Self::AdminMethodNotAllowed => ErrorClass::MethodNotAllowed,
            Self::AdminUpgradeRefused
            | Self::AdminMalformedRequest
            | Self::AdminReviewUnknown
            | Self::AdminConfirmationMismatch => ErrorClass::Malformed,
            Self::AdminRequestTooLarge => ErrorClass::TooLarge,
            Self::AdminReviewCapacity => ErrorClass::Refused,
            Self::AdminInternal => ErrorClass::Internal,
            Self::AdminStreamLimit
            | Self::AdminExchangeRateLimited
            | Self::NotYetServed
            | Self::ServiceDraining
            | Self::UpgradeInProgress => ErrorClass::Unavailable,
        }
    }

    /// Only refusals made before acceptance, for a busy or transitional
    /// service, are retryable.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::AdminStreamLimit
                | Self::AdminExchangeRateLimited
                | Self::ServiceDraining
                | Self::UpgradeInProgress
        )
    }

    pub const fn status(self) -> u16 {
        self.class().status()
    }

    pub fn parse(code: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(code.to_owned())).ok()
    }
}

/// UI state of a durable request (§2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    Submitted,
    Accepted,
    Completed,
    Refused,
    Unknown,
}

/// The service-journal request behind a mutation; the key is generated by
/// the admin handler and never reaches the browser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestProgress {
    #[schemars(schema_with = "schema::incarnation")]
    pub nonce: String,
    pub state: RequestState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SuccessEnvelope {
    #[schemars(schema_with = "schema::envelope_schema")]
    pub schema: String,
    #[schemars(schema_with = "schema::constant_true")]
    pub ok: bool,
    pub operation: String,
    /// The complete native result, never reduced.
    pub result: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestProgress>,
    /// The review token of a generate-mode row (§2.4, U98): the request
    /// identity it names stays in the admin session and never reaches the
    /// browser. A later review-mode call of the same session names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "schema::incarnation_option")]
    pub review: Option<String>,
}

/// The body of a call to a row with a review-mode request key or a
/// confirmation (U98). Every other row's body is the bare method object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCall {
    /// The `ControlMethod` object, without the row's request-key fields.
    pub method: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "schema::incarnation_option")]
    pub review: Option<String>,
    /// Required on destructive rows; at most `MAX_CONFIRM_TEXT_BYTES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorBody {
    pub code: String,
    pub class: ErrorClass,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// The complete native response, never reduced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FailureEnvelope {
    #[schemars(schema_with = "schema::envelope_schema")]
    pub schema: String,
    #[schemars(schema_with = "schema::constant_false")]
    pub ok: bool,
    pub error: ErrorBody,
    /// Receipt-only recovery handle for an unknown generated mutation
    /// (§4.4.1). It holds the original identity in this admin session and
    /// never authorizes another mutation or a different receipt operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "schema::incarnation_option")]
    pub review: Option<String>,
}

/// The initial, reviewed generated-mutation recovery applicability (§4.4.1).
/// A handle may restore its identity only into this matching read operation;
/// ordinary U98 review tokens retain their existing applicability.
pub fn recovery_receipt(operation: &str) -> Option<&'static str> {
    match operation {
        "catalog.policy_resume" => Some("catalog.policy_resume_receipt"),
        "catalog.split_recover" => Some("catalog.split_recovery_receipt"),
        _ => None,
    }
}

/// Existing unknown-outcome codes whose failure envelope is unavailable,
/// nonretryable and may carry a receipt-only handle when applicable. This is
/// envelope validation, not classification of a native control response.
pub fn is_unknown_outcome_code(code: &str) -> bool {
    matches!(
        code,
        "service_request_outcome_unknown"
            | "catalog_apply_outcome_unknown"
            | "catalog_index_setup_outcome_unknown"
            | "catalog_split_apply_outcome_unknown"
            | "catalog_metadata_saved_outcome_unknown"
            | "source_change_resolution_outcome_unknown"
    )
}

/// Every admin API body, including the session exchange and logout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Envelope {
    Success(SuccessEnvelope),
    Failure(FailureEnvelope),
}

impl SuccessEnvelope {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != ENVELOPE_SCHEMA {
            return Err("envelope_schema");
        }
        if !self.ok {
            return Err("envelope_ok");
        }
        if !is_operation_id(&self.operation) {
            return Err("envelope_operation");
        }
        if self
            .request
            .as_ref()
            .is_some_and(|request| !is_incarnation(&request.nonce))
        {
            return Err("envelope_nonce");
        }
        if self
            .review
            .as_deref()
            .is_some_and(|review| !is_incarnation(review))
        {
            return Err("envelope_review");
        }
        Ok(())
    }
}

impl FailureEnvelope {
    pub fn new(code: AdminCode) -> Self {
        Self {
            schema: ENVELOPE_SCHEMA.to_owned(),
            ok: false,
            error: ErrorBody {
                code: serde_json::to_value(code)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                class: code.class(),
                retryable: code.retryable(),
                retry_after_ms: None,
                native: None,
            },
            review: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != ENVELOPE_SCHEMA {
            return Err("envelope_schema");
        }
        if self.ok {
            return Err("envelope_ok");
        }
        let error = &self.error;
        if !is_snake_code(&error.code) {
            return Err("error_code");
        }
        match AdminCode::parse(&error.code) {
            Some(code) if code.class() != error.class => return Err("error_class"),
            Some(code) if code.retryable() != error.retryable => return Err("error_retryable"),
            None if is_unknown_outcome_code(&error.code) => {
                if error.class != ErrorClass::Unavailable {
                    return Err("error_class");
                }
                if error.retryable {
                    return Err("error_retryable");
                }
            }
            None if error.class != ErrorClass::Refused => return Err("error_class"),
            _ => {}
        }
        if error.retry_after_ms.is_some() && !error.retryable {
            return Err("error_retry_after");
        }
        if let Some(review) = &self.review {
            if !is_incarnation(review) {
                return Err("envelope_review");
            }
            if !is_unknown_outcome_code(&error.code)
                || error.class != ErrorClass::Unavailable
                || error.retryable
            {
                return Err("envelope_recovery_review");
            }
        }
        Ok(())
    }
}

/// One NDJSON line of `/admin/api/v1/events`. Cursors are opaque and scoped
/// to the service instance and the session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamLine {
    Event {
        cursor: String,
        event: serde_json::Value,
    },
    /// Re-read current state; a gap is never reported as history.
    Resync { reason: StreamResync },
    /// The server ends the stream (after `STREAM_SECONDS`); reconnect with the
    /// last cursor. A struct variant, so an unknown field is refused.
    Closing {},
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StreamResync {
    Overflow,
    NativeGap,
    ServiceRestarted,
}

/// `POST /admin/api/v1/session` body. `Debug` redacts the code.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExchangeRequest {
    #[schemars(schema_with = "schema::secret")]
    pub code: String,
}

impl fmt::Debug for ExchangeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExchangeRequest")
            .field("code", &"<redacted>")
            .finish()
    }
}

/// The `result` of a successful exchange. The token is returned only here
/// and kept in page memory (M1). `Debug` redacts it.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExchangeResponse {
    #[schemars(schema_with = "schema::secret")]
    pub csrf: String,
    pub idle_expires_in_ms: u64,
    pub absolute_expires_in_ms: u64,
    #[schemars(schema_with = "schema::incarnation")]
    pub service_instance: String,
}

impl fmt::Debug for ExchangeResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExchangeResponse")
            .field("csrf", &"<redacted>")
            .field("idle_expires_in_ms", &self.idle_expires_in_ms)
            .field("absolute_expires_in_ms", &self.absolute_expires_in_ms)
            .field("service_instance", &self.service_instance)
            .finish()
    }
}

impl ExchangeResponse {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !is_secret_text(&self.csrf) {
            return Err("exchange_csrf");
        }
        if !is_incarnation(&self.service_instance) {
            return Err("exchange_service_instance");
        }
        Ok(())
    }
}

/// The stored launch code found by the atomic compare-and-remove of its
/// SHA-256; `None` when unknown or already consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchCodeFacts {
    pub issued_at_ms: u64,
    pub same_instance: bool,
}

/// Decode an exchange body. Size and JSON shape errors are not about the code.
pub fn decode_exchange(body: &[u8]) -> Result<ExchangeRequest, AdminCode> {
    if body.len() as u64 > MAX_EXCHANGE_BODY_BYTES {
        return Err(AdminCode::AdminRequestTooLarge);
    }
    serde_json::from_slice(body).map_err(|_| AdminCode::AdminMalformedRequest)
}

/// Every code failure gets the same refusal (no oracle).
pub fn exchange(
    request: &ExchangeRequest,
    stored: Option<&LaunchCodeFacts>,
    now_ms: u64,
) -> Result<(), AdminCode> {
    let live = stored.is_some_and(|code| {
        code.same_instance && now_ms.saturating_sub(code.issued_at_ms) < LAUNCH_CODE_TTL_MS
    });
    if is_secret_text(&request.code) && live {
        Ok(())
    } else {
        Err(AdminCode::AdminLaunchCodeInvalid)
    }
}

/// The values of every `__Host-airk_admin` cookie across all `Cookie`
/// headers. Other cookies are ignored and never logged (M2 rule 4).
pub fn admin_cookies(headers: &[String]) -> Vec<&str> {
    headers
        .iter()
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .filter(|(name, _)| name.trim_matches([' ', '\t']) == COOKIE_NAME)
        .map(|(_, value)| value.trim_matches([' ', '\t']))
        .collect()
}

/// The bearer plane's only I3 addition, applied after its existing Host and
/// `Origin` checks (L6).
pub fn bearer_plane_cookie(headers: &[String]) -> Option<AdminCode> {
    (!admin_cookies(headers).is_empty()).then_some(AdminCode::CookieOnBearerPlane)
}

/// Result of the platform same-user peer observer (H1), cached per TLS
/// connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerProof {
    SameUser,
    OtherUser,
    Unavailable,
}

/// What a platform observer reports about the connection's 4-tuple; `None`
/// when the observation could not be made.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerObservation {
    /// Owning PID from `GetExtendedTcpTable`, then its token user SID.
    Windows {
        service_sid: String,
        peer_sid: Option<String>,
    },
    /// The `uid` column of the 4-tuple's row in `/proc/net/tcp`.
    Linux {
        service_uid: u32,
        peer_uid: Option<u32>,
    },
    /// Whether the 4-tuple is among the service user's own sockets.
    Macos {
        found_among_service_user_sockets: Option<bool>,
    },
}

pub fn peer_proof(observation: &PeerObservation) -> PeerProof {
    let same = match observation {
        PeerObservation::Windows {
            service_sid,
            peer_sid,
        } => peer_sid.as_ref().map(|peer| peer == service_sid),
        PeerObservation::Linux {
            service_uid,
            peer_uid,
        } => peer_uid.map(|peer| peer == *service_uid),
        PeerObservation::Macos {
            found_among_service_user_sockets,
        } => *found_among_service_user_sockets,
    };
    match same {
        Some(true) => PeerProof::SameUser,
        Some(false) => PeerProof::OtherUser,
        None => PeerProof::Unavailable,
    }
}

/// An admin route (§4.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Route {
    /// `/admin` → 308 `/admin/`.
    Redirect,
    /// `/admin/`: the one shell document.
    Shell,
    /// `/admin/launch`: serves the same shell document (M1).
    Launch,
    Asset {
        name: String,
    },
    Exchange,
    Logout,
    Call {
        operation: String,
    },
    Events,
}

impl Route {
    fn is_api(&self) -> bool {
        matches!(
            self,
            Self::Exchange | Self::Logout | Self::Call { .. } | Self::Events
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "match", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathMatch {
    /// Not under `/admin`: the existing pipeline handles it unchanged. A
    /// target that is not origin-form never reaches admin routing.
    NotAdmin,
    NotFound,
    Route {
        route: Route,
    },
}

/// Allowlist match on the raw request-target bytes (L1). Every target that
/// starts with `/admin` belongs to the admin plane; only `[A-Za-z0-9/._-]`
/// can match, so encodings, queries, parameters, backslashes, NUL and
/// non-ASCII bytes are never routed.
pub fn route(target: &[u8]) -> PathMatch {
    if !target.starts_with(b"/admin") {
        return PathMatch::NotAdmin;
    }
    let allowed = |byte: &u8| byte.is_ascii_alphanumeric() || b"/._-".contains(byte);
    let path = match std::str::from_utf8(target) {
        Ok(path) if target.iter().all(allowed) => path,
        _ => return PathMatch::NotFound,
    };
    let route = match path {
        "/admin" => Some(Route::Redirect),
        "/admin/" => Some(Route::Shell),
        "/admin/launch" => Some(Route::Launch),
        "/admin/api/v1/session" => Some(Route::Exchange),
        "/admin/api/v1/session/logout" => Some(Route::Logout),
        "/admin/api/v1/events" => Some(Route::Events),
        _ => {
            if let Some(name) = path.strip_prefix("/admin/assets/") {
                is_asset_name(name).then(|| Route::Asset {
                    name: name.to_owned(),
                })
            } else if let Some(operation) = path.strip_prefix("/admin/api/v1/call/") {
                is_operation_id(operation).then(|| Route::Call {
                    operation: operation.to_owned(),
                })
            } else {
                None
            }
        }
    };
    match route {
        Some(route) => PathMatch::Route { route },
        None => PathMatch::NotFound,
    }
}

/// Injected observations of one request. Header lists hold every occurrence,
/// so a duplicated header fails closed. No `Debug`: it carries cookie values.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestFacts {
    /// The local port the connection arrived on.
    pub listener_port: u16,
    /// The injected service clock.
    pub now_ms: u64,
    pub method: String,
    /// The raw request target.
    pub target: String,
    #[serde(default)]
    pub host: Vec<String>,
    #[serde(default)]
    pub origin: Vec<String>,
    #[serde(default)]
    pub sec_fetch_site: Vec<String>,
    /// Recorded, not required (§4.7).
    #[serde(default)]
    pub sec_fetch_mode: Option<String>,
    /// Recorded, not required (§4.7).
    #[serde(default)]
    pub sec_fetch_dest: Option<String>,
    #[serde(default)]
    pub content_type: Vec<String>,
    /// Declared length; a streamed body is bounded by the reader instead.
    #[serde(default)]
    pub content_length: Option<u64>,
    /// An `Upgrade` header or `Connection: upgrade` is present.
    #[serde(default)]
    pub upgrade: bool,
    /// Number of `Authorization` headers.
    #[serde(default)]
    pub authorization: u32,
    /// Raw `Cookie` header values.
    #[serde(default)]
    pub cookie: Vec<String>,
    /// Number of `X-AIRK-CSRF` headers.
    #[serde(default)]
    pub csrf_headers: u32,
    pub peer: PeerProof,
}

/// The session found by the SHA-256 of the single admin cookie, with the
/// constant-time token comparison already made by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionFacts {
    pub created_at_ms: u64,
    pub last_used_at_ms: u64,
    pub same_instance: bool,
    pub token_matches: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum Admission {
    NotAdmin,
    /// `touches_idle`: whether the request renews the idle timer. Event
    /// streams and logout never do (L4).
    Admit {
        route: Route,
        touches_idle: bool,
    },
    Refuse {
        status: u16,
        code: AdminCode,
    },
}

fn refuse(code: AdminCode) -> Admission {
    Admission::Refuse {
        status: code.status(),
        code,
    }
}

fn single_equals(values: &[String], expected: &str) -> bool {
    matches!(values, [only] if only == expected)
}

fn json_content_type(values: &[String]) -> bool {
    matches!(values, [only] if only
        .split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json")))
}

fn session_valid(request: &RequestFacts, session: Option<&SessionFacts>) -> bool {
    session.is_some_and(|session| {
        admin_cookies(&request.cookie).len() == 1
            && session.same_instance
            && request.now_ms.saturating_sub(session.last_used_at_ms) < SESSION_IDLE_MS
            && request.now_ms.saturating_sub(session.created_at_ms) < SESSION_ABSOLUTE_MS
            && request.csrf_headers == 1
            && session.token_matches
    })
}

/// Admission of one request (§4.7 with review H1, M2, L1, L4). Order:
/// path plane, Host, route, upgrade, method, then `Origin`, Fetch Metadata and
/// peer (403), bearer and session (401), content type (400) and size (413).
pub fn admit(request: &RequestFacts, session: Option<&SessionFacts>) -> Admission {
    let route = match route(request.target.as_bytes()) {
        PathMatch::NotAdmin => return Admission::NotAdmin,
        PathMatch::NotFound => None,
        PathMatch::Route { route } => Some(route),
    };
    if !single_equals(&request.host, &expected_host(request.listener_port)) {
        return refuse(AdminCode::AdminHostRefused);
    }
    let Some(route) = route else {
        return refuse(AdminCode::AdminNotFound);
    };
    if request.upgrade {
        return refuse(AdminCode::AdminUpgradeRefused);
    }
    if request.method == "OPTIONS" {
        return refuse(AdminCode::AdminPreflightRefused);
    }
    let api = route.is_api();
    let expected_method = if api { "POST" } else { "GET" };
    if request.method != expected_method {
        return refuse(AdminCode::AdminMethodNotAllowed);
    }
    let origin = expected_origin(request.listener_port);
    if !api {
        if !(request.origin.is_empty() || single_equals(&request.origin, &origin)) {
            return refuse(AdminCode::AdminOriginRefused);
        }
        let site_allowed = match request.sec_fetch_site.as_slice() {
            [] => true,
            [site] => {
                site == "none"
                    || site == "same-origin"
                    || (site == "cross-site" && route == Route::Launch)
            }
            _ => false,
        };
        if !site_allowed {
            return refuse(AdminCode::AdminFetchMetadataRefused);
        }
        if request.authorization > 0 {
            return refuse(AdminCode::AdminBearerRefused);
        }
        return Admission::Admit {
            route,
            touches_idle: false,
        };
    }
    if !single_equals(&request.origin, &origin) {
        return refuse(AdminCode::AdminOriginRefused);
    }
    if !single_equals(&request.sec_fetch_site, "same-origin") {
        return refuse(AdminCode::AdminFetchMetadataRefused);
    }
    if request.peer != PeerProof::SameUser {
        return refuse(AdminCode::AdminPeerRefused);
    }
    if request.authorization > 0 {
        return refuse(AdminCode::AdminBearerRefused);
    }
    let exchange = route == Route::Exchange;
    if !exchange && !session_valid(request, session) {
        return refuse(AdminCode::AdminSessionInvalid);
    }
    if !json_content_type(&request.content_type) {
        return refuse(AdminCode::AdminMalformedRequest);
    }
    let limit = if exchange {
        MAX_EXCHANGE_BODY_BYTES
    } else {
        MAX_REQUEST_BODY_BYTES
    };
    if request.content_length.is_some_and(|length| length > limit) {
        return refuse(AdminCode::AdminRequestTooLarge);
    }
    let touches_idle = matches!(route, Route::Call { .. });
    Admission::Admit {
        route,
        touches_idle,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKind {
    Api,
    Events,
    /// The shell document, also served at `/admin/launch`.
    Shell,
    Asset,
    Redirect,
}

/// The fixed headers of every admin response of a kind. No CORS header is
/// ever sent. An asset's `content-type` comes from the sealed inventory.
pub fn response_headers(kind: ResponseKind) -> Vec<(&'static str, &'static str)> {
    let mut headers = vec![
        ("content-security-policy", CONTENT_SECURITY_POLICY),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("permissions-policy", PERMISSIONS_POLICY),
    ];
    headers.push((
        "cache-control",
        if kind == ResponseKind::Asset {
            ASSET_CACHE_CONTROL
        } else {
            "no-store"
        },
    ));
    match kind {
        ResponseKind::Api => headers.push(("content-type", "application/json")),
        ResponseKind::Events => headers.push(("content-type", "application/x-ndjson")),
        ResponseKind::Shell => headers.push(("content-type", "text/html; charset=utf-8")),
        ResponseKind::Redirect => headers.push(("location", "/admin/")),
        ResponseKind::Asset => {}
    }
    headers
}

/// Operation table (§4.3). Rows are compared with the reviewed
/// `fixtures/split/i3/admin_operations.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Plane {
    Management,
    /// Never queues behind the metadata actor (plan §4.5).
    Evidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Pipe,
    Airkctl,
    Session,
    Admin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Authorization {
    OwnerPipe,
    WindowLease,
    /// Session cookie, anti-forgery token and same-user peer proof.
    AdminSessionCsrf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Mutation,
    /// Native draft, preview, apply-with-digest and typed confirmation.
    Destructive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Idempotency {
    Read,
    RequestKey,
    OperationDigest,
    Natural,
}

/// Simplification-plan §4.6 properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Conformance {
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    P7,
    P8,
    P9,
}

impl Plane {
    pub const ALL: [Self; 2] = [Self::Management, Self::Evidence];
}
impl Surface {
    pub const ALL: [Self; 4] = [Self::Pipe, Self::Airkctl, Self::Session, Self::Admin];
}
impl Authorization {
    pub const ALL: [Self; 3] = [Self::OwnerPipe, Self::WindowLease, Self::AdminSessionCsrf];
}
impl Effect {
    pub const ALL: [Self; 3] = [Self::Read, Self::Mutation, Self::Destructive];
}
impl Idempotency {
    pub const ALL: [Self; 4] = [
        Self::Read,
        Self::RequestKey,
        Self::OperationDigest,
        Self::Natural,
    ];
}
impl Conformance {
    pub const ALL: [Self; 9] = [
        Self::P1,
        Self::P2,
        Self::P3,
        Self::P4,
        Self::P5,
        Self::P6,
        Self::P7,
        Self::P8,
        Self::P9,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationRow {
    pub operation: String,
    /// The one handler: a `ControlMethod` variant, `Variant.Action` for one
    /// action of it, or a canonical admin handler id.
    pub method: String,
    pub plane: Plane,
    pub surfaces: Vec<Surface>,
    pub authorization: Vec<Authorization>,
    pub effect: Effect,
    pub idempotency: Idempotency,
    pub conformance: Vec<Conformance>,
    /// Where the method's request identity lives (§2.4, U98).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_key: Option<RequestKeyRule>,
    /// The typed confirmation of a destructive row (U98).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
}

/// Request-identity placement. Paths are dotted field names inside the
/// method's own object (`{"<variant>": {..}}`), 1 to 4 lowercase segments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestKeyRule {
    pub mode: KeyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    pub key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    /// The browser omits the fields; the service fills a fresh caller and
    /// key, keeps them under a new review token and removes the key (and the
    /// caller beside it) from the native result.
    Generate,
    /// The browser omits the fields and names a review token in a
    /// `ReviewedCall`; the service fills the identity that token holds.
    Review,
}

/// How a destructive row's typed text is checked (U98). The service reads
/// the target's current display name; the browser's text must equal it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confirmation {
    /// `CatalogApply`: the name of the operation's target index or index set
    /// (a grant's id) at the request's expected revision. Operations that
    /// create, rename or add need no text.
    CatalogTarget,
    /// `CatalogSplitApply`: the original index's name.
    SplitOriginalIndex,
    /// `PlanExecute`: the first 12 hexadecimal characters of the plan id.
    PlanId,
    /// `StoragePurge`: the purged location's path as the inventory lists it.
    StorageTarget,
}

impl KeyMode {
    pub const ALL: [Self; 2] = [Self::Generate, Self::Review];
}
impl Confirmation {
    pub const ALL: [Self; 4] = [
        Self::CatalogTarget,
        Self::SplitOriginalIndex,
        Self::PlanId,
        Self::StorageTarget,
    ];
}

fn is_key_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('.').collect();
    (1..=4).contains(&segments.len())
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
}

/// An item that never has an HTTP route (§4.9 with review L4, L8). An admin
/// row may name none of its methods, nor an action of one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeverOverHttp {
    pub item: String,
    pub methods: Vec<String>,
    pub reason: String,
}

fn distinct<T: PartialEq>(items: &[T]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(index, item)| !items[..index].contains(item))
}

/// The reviewed table rules (§4.3); the first violation is reported.
pub fn validate_table(rows: &[OperationRow], never: &[NeverOverHttp]) -> Result<(), &'static str> {
    let forbidden: Vec<&str> = never
        .iter()
        .flat_map(|item| item.methods.iter().map(String::as_str))
        .collect();
    for (index, row) in rows.iter().enumerate() {
        let has = |property| row.conformance.contains(&property);
        let surface = |surface| row.surfaces.contains(&surface);
        let authorized = |rule| row.authorization.contains(&rule);
        if !is_operation_id(&row.operation) {
            return Err("operation_id");
        }
        if rows[..index]
            .iter()
            .any(|other| other.operation == row.operation)
        {
            return Err("operation_duplicate");
        }
        if row.method.is_empty() {
            return Err("method_empty");
        }
        if row.surfaces.is_empty() || !distinct(&row.surfaces) {
            return Err("surfaces");
        }
        if !distinct(&row.authorization) || !distinct(&row.conformance) {
            return Err("duplicate_entry");
        }
        if surface(Surface::Admin) != authorized(Authorization::AdminSessionCsrf) {
            return Err("authorization_admin");
        }
        if (surface(Surface::Pipe) || surface(Surface::Airkctl) || surface(Surface::Session))
            && !authorized(Authorization::OwnerPipe)
        {
            return Err("authorization_owner_pipe");
        }
        let consistent = match row.effect {
            Effect::Read => row.idempotency == Idempotency::Read,
            Effect::Mutation => row.idempotency != Idempotency::Read,
            Effect::Destructive => row.idempotency == Idempotency::OperationDigest,
        };
        if !consistent {
            return Err("effect_idempotency");
        }
        if let Some(rule) = &row.request_key {
            if !is_key_path(&rule.key)
                || rule
                    .caller
                    .as_deref()
                    .is_some_and(|path| !is_key_path(path))
            {
                return Err("request_key_path");
            }
            let allowed = match rule.mode {
                KeyMode::Generate => row.idempotency != Idempotency::OperationDigest,
                KeyMode::Review => matches!(
                    row.idempotency,
                    Idempotency::Read | Idempotency::OperationDigest
                ),
            };
            if !allowed {
                return Err("request_key_mode");
            }
        }
        if (row.effect == Effect::Destructive) != row.confirmation.is_some() {
            return Err("confirmation");
        }
        let read = row.effect == Effect::Read;
        let missing = !has(Conformance::P3)
            || !has(Conformance::P5)
            || (!read && !has(Conformance::P2))
            || (row.idempotency == Idempotency::RequestKey && !has(Conformance::P1))
            || (read && surface(Surface::Admin) && !has(Conformance::P8))
            || (read
                && row.plane == Plane::Evidence
                && !(has(Conformance::P6) && has(Conformance::P7)));
        if missing {
            return Err("conformance_missing");
        }
        if surface(Surface::Admin)
            && forbidden.iter().any(|method| {
                row.method == *method
                    || row
                        .method
                        .strip_prefix(*method)
                        .is_some_and(|rest| rest.starts_with('.'))
            })
        {
            return Err("never_over_http");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
