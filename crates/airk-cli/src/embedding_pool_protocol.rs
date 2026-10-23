// SPDX-License-Identifier: Apache-2.0
//! Bounded shared-pool framing. Service control carries capabilities, never input payloads.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::ProjectRegistration,
};
use airk_retrieval::embedding_worker::{Request, Response};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Init {
    pub runtime: PathBuf,
    pub service: Incarnation,
    pub pool: Incarnation,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub service: Incarnation,
    pub pool: Incarnation,
    pub project: ProjectRegistration,
    pub worker: Incarnation,
    pub token: Sha256Digest,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    pub runtime: PathBuf,
    pub role: String,
    pub grant: Grant,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Register { grant: Grant },
    Revoke { worker: Incarnation },
    Status,
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Ready {
        pool: Incarnation,
        role: String,
    },
    Registered,
    Revoked,
    Status {
        loads: u64,
        reuses: u64,
        sessions: usize,
    },
    Failed,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Call { request: Request },
    Close,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub project: Sha256Digest,
    pub worker: Incarnation,
    pub session: Incarnation,
    pub request_id: Incarnation,
    pub input_digest: Sha256Digest,
    pub operation: Operation,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultEnvelope {
    pub project: Sha256Digest,
    pub worker: Incarnation,
    pub session: Incarnation,
    pub request_id: Incarnation,
    pub input_digest: Sha256Digest,
    pub response: Response,
}

pub fn role(pool: Incarnation) -> String {
    format!("e-{pool}")
}
pub fn input_digest(operation: &Operation) -> anyhow::Result<Sha256Digest> {
    let mut bytes = Vec::new();
    crate::service_ipc::write_frame_limited(
        &mut bytes,
        operation,
        airk_retrieval::embedding_worker::MAX_FRAME_BYTES,
    )?;
    use sha2::Digest;
    Ok(Sha256Digest::from_bytes(sha2::Sha256::digest(bytes).into()))
}
