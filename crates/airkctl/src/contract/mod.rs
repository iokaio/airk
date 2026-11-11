// SPDX-License-Identifier: Apache-2.0
//! The I2 contract between `airkctl` and its callers (split plan §4.2 I2,
//! `docs/split/contracts.md` §3): exit classes, the stderr error line, the
//! version document and the editor session (protocol 2) frames.
//!
//! Reviewed cases: `fixtures/split/i2/*.json` and
//! `fixtures/wire/airkctl_session_frames.json`. Expectations there are written
//! independently of this code; tests only compare against them.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
    service_events::EventCursor,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod schema;
pub use schema::document as schema_document;

/// The I2 contract version this build implements. Bumped only with a reviewed
/// contract change; callers decide compatibility from this and capabilities,
/// never from product-version ordering.
pub const I2_CONTRACT: u32 = 1;
/// Every I2 contract version this build can serve.
pub const I2_CONTRACTS_SUPPORTED: &[u32] = &[1];
/// Schema of the `airkctl version --output json` document.
pub const VERSION_SCHEMA: &str = "airkctl-version/1";
/// The product this client belongs to (U31).
pub const PRODUCT: &str = "AIRK Service";
/// Editor session protocol. Version 1 stays with `airk service session` until
/// that command retires.
pub const SESSION_PROTOCOL: u32 = 2;
/// Upper bound of the `message` of an error line, in characters.
pub const MAX_ERROR_MESSAGE_CHARS: usize = 4096;

// Session budgets: unchanged from protocol 1 except the invoke deadline, whose
// 600 s installer exception ends with the split (plan §6.5).
pub const MAX_REQUEST_FRAME_BYTES: usize = 192 * 1024;
pub const MAX_INVOKE_STDIN_BYTES: usize = 64 * 1024;
pub const MAX_INVOKE_ARGS: usize = 64;
pub const MAX_INVOKE_ARG_BYTES: usize = 32 * 1024;
pub const MAX_RESPONSE_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_INVOKE_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_IN_FLIGHT: usize = 8;
pub const MAX_INVOKE_DEADLINE_MS: u64 = 120_000;
/// Event buffer per session, kept separate from reply budgets. Overflow drops
/// the buffer and emits one `resync`; it never reports a gap as history.
pub const MAX_BUFFERED_EVENTS: usize = 256;
pub const MAX_BUFFERED_EVENT_BYTES: usize = 1024 * 1024;

/// Process exit classes. The numeric codes are part of the contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExitClass {
    /// Completed as requested.
    Success,
    /// The service answered a typed refusal, or a native read failed with its
    /// complete result on stdout.
    Refused,
    /// Argument error.
    Usage,
    /// Not installed, not running and not startable, or a saved explicit Stop.
    ServiceMissing,
    /// `airk`/`airkctl` version, protocol or record digest mismatch, or an
    /// incomplete installation; detected before any mutation.
    IncompatiblePair,
    /// `not_yet_served`, or a required contract or capability is absent.
    UnsupportedCapability,
    /// Deadline or disconnect after a mutation was sent; recover by receipt.
    OutcomeUnknown,
    /// Install pointer or record ownership, link or integrity check failed.
    InstallationUnverified,
    /// A bug or I/O failure inside `airkctl`; mutating callers treat it as
    /// [`ExitClass::OutcomeUnknown`].
    Internal,
}

/// Whether a class can follow a mutation the service may have applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MutationPossible {
    Yes,
    No,
    Maybe,
    Unknown,
}

impl ExitClass {
    pub const ALL: [Self; 9] = [
        Self::Success,
        Self::Refused,
        Self::Usage,
        Self::ServiceMissing,
        Self::IncompatiblePair,
        Self::UnsupportedCapability,
        Self::OutcomeUnknown,
        Self::InstallationUnverified,
        Self::Internal,
    ];

    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Refused => 1,
            Self::Usage => 2,
            Self::ServiceMissing => 3,
            Self::IncompatiblePair => 4,
            Self::UnsupportedCapability => 5,
            Self::OutcomeUnknown => 6,
            Self::InstallationUnverified => 7,
            Self::Internal => 8,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.code() == code)
    }

    pub const fn mutation_possible(self) -> MutationPossible {
        match self {
            Self::Success => MutationPossible::Yes,
            Self::OutcomeUnknown => MutationPossible::Maybe,
            Self::Internal => MutationPossible::Unknown,
            _ => MutationPossible::No,
        }
    }
}

/// The last stderr line of a failed command run with `--output json`. It
/// extends the `AIRK_ERROR_FORMAT=json` line under its own key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorLine {
    /// Always 1; marks the line and versions its shape.
    pub airkctl_error: u32,
    pub class: ExitClass,
    pub code: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    pub message: String,
    /// The complete native failure or response, never reduced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<serde_json::Value>,
}

impl ErrorLine {
    /// Build an error line, bounding the message to
    /// [`MAX_ERROR_MESSAGE_CHARS`] characters.
    pub fn new(class: ExitClass, code: &str, retryable: bool, message: &str) -> Self {
        Self {
            airkctl_error: 1,
            class,
            code: code.to_owned(),
            retryable,
            retry_after_ms: None,
            message: message.chars().take(MAX_ERROR_MESSAGE_CHARS).collect(),
            native: None,
        }
    }

    /// Reject lines that violate the contract after decoding.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.airkctl_error != 1 {
            return Err("airkctl_error_version");
        }
        if self.code.is_empty()
            || self.code.len() > 128
            || !self
                .code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("airkctl_error_code");
        }
        if self.message.chars().count() > MAX_ERROR_MESSAGE_CHARS {
            return Err("airkctl_error_message");
        }
        if self.class == ExitClass::Success {
            return Err("airkctl_error_class");
        }
        Ok(())
    }
}

/// `airkctl version --output json`. Local only: it never starts or contacts
/// the service. Readers ignore unknown fields (forward-compatible handshake).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VersionDocument {
    pub schema: String,
    pub product: String,
    pub version: String,
    /// The installed channel; `None` until an installation record is found.
    pub channel: Option<String>,
    pub i2_contract: u32,
    pub i2_contracts_supported: Vec<u32>,
    pub i1_protocol: u32,
    /// Served operations of this exact pair; an unserved one is absent.
    pub capabilities: Vec<String>,
    /// SHA-256 of this `airkctl` executable, when it could be read.
    pub executable_sha256: Option<String>,
    pub installation: Installation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Installation {
    pub state: InstallationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airk_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair: Option<PairState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallationState {
    /// The pointer and record verified and the pair is complete.
    Complete,
    /// The record describes a partially placed or partially replaced pair.
    Incomplete,
    /// No install pointer exists for this user.
    NotFound,
    /// The pointer or record failed an ownership, link, size or integrity check.
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairState {
    Exact,
    Mismatch,
}

/// Session protocol 2: one request frame from the editor window.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    #[schemars(schema_with = "schema::incarnation")]
    pub id: Incarnation,
    pub request: SessionMethod,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventTopic {
    Status,
    Jobs,
    Assistance,
}

/// A subscription cursor, scoped to one service incarnation and one window.
/// A cursor from another incarnation or window is refused
/// (`event_cursor_scope_mismatch`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionCursor {
    #[schemars(schema_with = "schema::incarnation")]
    pub service_instance: Incarnation,
    #[schemars(schema_with = "schema::incarnation")]
    pub window_id: Incarnation,
    #[schemars(schema_with = "schema::native_value")]
    pub native: EventCursor,
}

// Frames are decoded once and moved; boxing would only reshape the Rust type.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionMethod {
    /// First request. Anything else first is `session_protocol_mismatch`.
    Hello {
        protocol: u32,
        client: ClientIdentity,
        i2_contracts: Vec<u32>,
    },
    /// Run one allowlisted command. `argv` excludes the program name.
    Invoke {
        argv: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stdin: Option<String>,
        deadline_ms: u64,
    },
    /// Metadata relay to the installed service (found through the install
    /// pointer; no control directory is named). Brokerage and token-returning
    /// methods are refused.
    Control {
        #[schemars(schema_with = "schema::native_value")]
        method: ControlMethod,
    },
    /// Cooperatively cancel an in-flight request.
    Cancel {
        #[schemars(schema_with = "schema::incarnation")]
        target: Incarnation,
    },
    Subscribe {
        topics: Vec<EventTopic>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<SessionCursor>,
    },
    Unsubscribe {},
    /// Select existing indexes as this window's search scope (U77, W4). The
    /// service issues an in-memory read grant bound to the window lease; it
    /// never creates or changes indexes, index sets or agent grants.
    ScopeSelect {
        selection: WindowSelection,
    },
    /// Release this window's scope grant (W4b); idempotent.
    ScopeRelease {},
    /// Acknowledge Assistance drafts through `through` (W7); idempotent.
    AssistanceAck {
        through: u64,
    },
    /// Publish an Explorer handoff for this window (W8).
    HandoffPublish {
        handoff: serde_json::Value,
        ttl_ms: u64,
    },
}

/// Existing indexes and projects selected as one window's scope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowSelection {
    #[schemars(schema_with = "schema::incarnation")]
    pub namespace: Incarnation,
    pub expected_catalog_revision: u64,
    pub index_ids: Vec<String>,
    pub project_ids: Vec<String>,
}

/// One reply frame. Events travel in [`SessionEventFrame`] (disjoint keys).
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionResponse {
    #[schemars(schema_with = "schema::incarnation")]
    pub id: Incarnation,
    pub outcome: SessionOutcome,
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionOutcome {
    Hello {
        protocol: u32,
        version: String,
        i2_contract: u32,
        capabilities: Vec<String>,
        executable_sha256: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(schema_with = "schema::optional_incarnation")]
        service_instance: Option<Incarnation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(schema_with = "schema::optional_incarnation")]
        window_id: Option<Incarnation>,
    },
    Control {
        #[schemars(schema_with = "schema::native_value")]
        response: ControlResponse,
    },
    /// The command ran; the three channels a one-shot would have produced.
    Invoked {
        exit_code: u8,
        class: ExitClass,
        stdout: String,
        stderr: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorLine>,
    },
    Subscribed {
        cursor: SessionCursor,
    },
    Unsubscribed,
    Scope {
        scope_identity: String,
        catalog_revision: u64,
        expires_at_ms: u64,
    },
    ScopeReleased,
    Acknowledged,
    Handoff {
        handoff_id: String,
        expires_at_ms: u64,
    },
    /// A typed service refusal for a window-plane frame (for example
    /// `window_scope_not_granted`, `scoped_read_unavailable`,
    /// `assistance_ack_ahead`, `handoff_oversize`); the service changed nothing.
    Refused {
        code: String,
        retryable: bool,
    },
    /// Session-level refusal; the command did not run or its outcome is unknown.
    Failed {
        code: SessionFailure,
        retryable: bool,
    },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionFailure {
    SessionInvalidRequest,
    SessionProtocolMismatch,
    /// Not allowlisted in a session, including token-returning one-shots.
    SessionCommandRefused,
    SessionBusy,
    SessionDeadlineExceeded,
    SessionResponseOversize,
    SessionClosing,
    /// A cursor from another service incarnation or window.
    EventCursorScopeMismatch,
    /// The service restarted; in-flight reads fail and may be retried.
    ServiceRestarted,
}

impl SessionFailure {
    /// Only refusals that cannot have started work are retryable.
    pub fn retryable(self) -> bool {
        matches!(self, Self::SessionBusy | Self::ServiceRestarted)
    }
}

/// An unsolicited event frame.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionEventFrame {
    pub event: SessionEvent,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionEvent {
    Status {
        cursor: SessionCursor,
        status: serde_json::Value,
    },
    Job {
        cursor: SessionCursor,
        job: serde_json::Value,
    },
    AssistanceDraft {
        cursor: SessionCursor,
        draft: serde_json::Value,
    },
    /// The buffer overflowed, the native stream had a gap, or the service
    /// restarted; the client re-reads current state.
    Resync {
        reason: ResyncReason,
    },
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    Overflow,
    NativeGap,
    ServiceRestarted,
}

#[cfg(test)]
mod tests;
