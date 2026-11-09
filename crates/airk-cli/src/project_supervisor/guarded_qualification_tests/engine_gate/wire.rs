// SPDX-License-Identifier: Apache-2.0
//! Private qualification-image latch. These observations never grant authority
//! and contain no model text, paths, credentials, process IDs, or model bytes.
#![cfg(test)]
use airk_core::{identity::Incarnation, service::WorkerIdentity};
use serde::{Deserialize, Serialize};

pub(crate) const ROLE: &str = "qualification-engine";
pub(crate) const MAX_FRAME_BYTES: usize = 1024;
pub(crate) const DEADLINE_SECONDS: u64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
    pub worker: WorkerIdentity,
    pub helper: Incarnation,
    pub request_id: Incarnation,
    pub session: Incarnation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Event {
    Reached {
        context: Context,
    },
    Entered {
        context: Context,
        nonce: Incarnation,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    /// None is an immediate non-target continuation. A held call requires the
    /// exact nonce that the parent supplied and the helper acknowledged.
    Resume {
        context: Context,
        nonce: Option<Incarnation>,
    },
    Hold {
        context: Context,
        nonce: Incarnation,
    },
}
