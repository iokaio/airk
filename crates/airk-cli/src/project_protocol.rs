// SPDX-License-Identifier: Apache-2.0
//! Private child control carries identity/grants only; evidence uses the direct endpoint.
use airk_core::{
    identity::Incarnation,
    service::{ConnectionGrant, ProjectBinding, QueryAllowance, WorkerIdentity},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Init {
    pub runtime: PathBuf,
    pub project: ProjectBinding,
    pub service: Incarnation,
    pub worker: Incarnation,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Activate {
        identity: WorkerIdentity,
        allowance: QueryAllowance,
        pool: Option<Box<crate::embedding_pool_protocol::Access>>,
    },
    Grant {
        caller: Incarnation,
    },
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Activated,
    Ready {
        identity: WorkerIdentity,
        role: String,
    },
    Grant {
        grant: ConnectionGrant,
    },
    Failed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub tool: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub identity: WorkerIdentity,
}

pub fn role(worker: Incarnation) -> String {
    format!("q-{worker}")
}
