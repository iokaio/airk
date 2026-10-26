// SPDX-License-Identifier: Apache-2.0
//! Content-free retained service events; sequence positions never imply index freshness.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, ProjectBinding},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    CursorExpired,
    ScopeChanged,
    InvalidCursor,
    InvalidLimit,
}
impl std::fmt::Display for EventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CursorExpired => "event_cursor_expired",
            Self::ScopeChanged => "event_scope_changed",
            Self::InvalidCursor => "invalid_event_cursor",
            Self::InvalidLimit => "invalid_event_page_limit",
        })
    }
}
impl std::error::Error for EventError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Registered,
    Paused,
    Resumed,
    JobPending,
    JobRunning,
    JobSucceeded,
    JobFailed,
    JobCanceled,
    CancellationRequested,
    JobRetryRequested,
    WatchEnabled,
    WatchDisabled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceEvent {
    pub sequence: u64,
    pub observed_unix_ms: u64,
    pub project: ProjectBinding,
    pub kind: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_kind: Option<JobKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fencing_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// `through` pins a paged snapshot. A tail position without it starts a new poll.
/// Native authenticated management clients may use this; it grants no capabilities.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventCursor {
    pub namespace: Incarnation,
    pub project: Option<ProjectBinding>,
    pub after: u64,
    pub through: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPage {
    pub namespace: Incarnation,
    pub oldest_sequence: Option<u64>,
    pub snapshot_through: u64,
    pub retained_prefix_lost: bool,
    pub events: Vec<ServiceEvent>,
    pub next: Option<EventCursor>,
    /// Finish `next` pages before saving this position for the next observation.
    pub tail: EventCursor,
}
