// SPDX-License-Identifier: Apache-2.0
//! Per-window native session protocol: the framed stdin/stdout contract between
//! one editor window and its single `airk service session` child.
//!
//! The session is a temporary client, never the service: it exits on stdin EOF or
//! owner loss, never serves evidence, and never accepts connection grants on its
//! control relay. `invoke` runs an allowlisted one-shot CLI command inside the
//! session process and returns exactly what that command would have printed, so
//! the window no longer starts one `airk.exe` per operation.
//!
//! Frames reuse [`crate::service_ipc`] (4-byte big-endian length, then JSON) with
//! the direction-specific budgets below. Reviewed cases:
//! `fixtures/wire/session_frames.json`.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
};
use serde::{Deserialize, Serialize};

/// Bumped only with a reviewed contract change; `hello` refuses any other value.
pub const SESSION_PROTOCOL: u32 = 1;
/// Request frame budget: argv plus at most [`MAX_INVOKE_STDIN_BYTES`] of input.
pub const MAX_REQUEST_FRAME_BYTES: usize = 192 * 1024;
/// The same input budget the extension enforced for one-shot native commands.
pub const MAX_INVOKE_STDIN_BYTES: usize = 64 * 1024;
pub const MAX_INVOKE_ARGS: usize = 64;
pub const MAX_INVOKE_ARG_BYTES: usize = 32 * 1024;
/// Response frame budget, encoded: the 16 MiB stdout buffer the former one-shot
/// native calls allowed, so in-session output never fails where a one-shot
/// succeeded. Larger output is refused with `session_response_oversize`, never
/// truncated.
pub const MAX_RESPONSE_FRAME_BYTES: usize = 16 * 1024 * 1024;
/// Retained diagnostic stderr per invocation; the tail is kept.
pub const MAX_INVOKE_STDERR_BYTES: usize = 64 * 1024;
/// Concurrent requests per session. A further request is answered `session_busy`.
pub const MAX_IN_FLIGHT: usize = 8;
/// Upper bound for one invocation's deadline (the longest current one-shot is
/// replacement apply at 300 s).
pub const MAX_INVOKE_DEADLINE_MS: u64 = 600_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub id: Incarnation,
    pub request: SessionMethod,
}

// Frames are decoded once and moved; boxing would only reshape the Rust type.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionMethod {
    /// First request; answers the session's protocol, version and image digest.
    Hello { protocol: u32 },
    /// Same-version, instance-fenced metadata relay (the former `service console`).
    /// The session starts before any installation exists, so each relay names the
    /// control directory and, optionally, the approved runtime digest to pin.
    Control {
        runtime_dir: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        runtime_sha256: Option<String>,
        method: ControlMethod,
    },
    /// Run one allowlisted CLI command in-process. `argv` excludes the program name.
    Invoke {
        argv: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stdin: Option<String>,
        deadline_ms: u64,
    },
    /// Cooperatively cancel an in-flight request; the target answers `cancelled`.
    Cancel { target: Incarnation },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResponse {
    pub id: Incarnation,
    pub outcome: SessionOutcome,
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionOutcome {
    Hello {
        protocol: u32,
        version: String,
        executable_sha256: String,
    },
    Control {
        response: ControlResponse,
    },
    /// The command ran. `success` and the streams match what the one-shot process
    /// would have produced (stdout complete, stderr bounded to its tail).
    Invoked {
        success: bool,
        stdout: String,
        stderr: String,
    },
    /// Session-level refusal; the command did not run or its outcome is unknown.
    Failed {
        code: SessionFailure,
        retryable: bool,
    },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFailure {
    /// Malformed request or a budget violation before any side effect.
    SessionInvalidRequest,
    /// `hello` with another protocol, or any request before `hello`.
    SessionProtocolMismatch,
    /// The command is not allowlisted for in-session execution.
    SessionCommandRefused,
    /// [`MAX_IN_FLIGHT`] requests are already running.
    SessionBusy,
    /// The deadline elapsed; side effects of the command are unknown.
    SessionDeadlineExceeded,
    /// The command's output exceeded [`MAX_RESPONSE_FRAME_BYTES`].
    SessionResponseOversize,
    /// The session is shutting down (EOF or owner loss).
    SessionClosing,
}

impl SessionFailure {
    /// Only refusals that cannot have started the command are retryable.
    pub fn retryable(self) -> bool {
        matches!(self, Self::SessionBusy)
    }
}

#[cfg(test)]
mod tests;
