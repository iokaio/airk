// SPDX-License-Identifier: Apache-2.0
//! Bounded shared-pool framing. Service control carries capabilities, never input payloads.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ProjectRegistration, WorkerIdentity},
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
    pub inference_threads: u16,
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

pub const MAX_PROVIDER_DRAINS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDrainTicket {
    pub helper: Incarnation,
    pub worker: WorkerIdentity,
    pub ticket: Incarnation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDrainObservation {
    Pending,
    Drained,
    Refused,
    Forgotten,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Renew {
        worker: Incarnation,
    },
    Register {
        grant: Grant,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_identity: Option<WorkerIdentity>,
    },
    Revoke {
        worker: Incarnation,
    },
    BeginDrain {
        ticket: ProviderDrainTicket,
    },
    PollDrain {
        ticket: ProviderDrainTicket,
    },
    ForgetDrain {
        ticket: ProviderDrainTicket,
    },
    ConfigureThreads {
        threads: u16,
    },
    Status,
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    GuardedReady {
        namespace: Incarnation,
        service: Incarnation,
        pool: Incarnation,
    },
    Renewed,
    GrantMissing {
        worker: Incarnation,
    },
    Ready {
        pool: Incarnation,
        role: String,
    },
    Registered,
    Revoked,
    Drain {
        ticket: ProviderDrainTicket,
        observation: ProviderDrainObservation,
    },
    Status {
        loads: u64,
        reuses: u64,
        sessions: usize,
        age_ms: u64,
    },
    Failed,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Call { request: Box<Request> },
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
