// SPDX-License-Identifier: Apache-2.0
//! Trusted host lifecycle intent; this is never an MCP capability.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service::RequestKey,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Stop,
    Restart,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleRequest {
    pub caller: Incarnation,
    pub key: RequestKey,
    pub action: LifecycleAction,
    pub expected_instance: Incarnation,
    pub runtime: Sha256Digest,
    pub manager: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Draining,
    Starting,
    Complete,
    Abandoned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleRecord {
    pub id: Sha256Digest,
    pub request: LifecycleRequest,
    pub phase: LifecyclePhase,
}
