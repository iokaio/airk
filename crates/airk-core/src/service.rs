// SPDX-License-Identifier: Apache-2.0
//! Service protocol contracts; endpoint authentication and clocks belong to adapters.
use crate::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_GRANT_TTL_MS: u64 = 30_000;

/// Digest of a canonical root/storage/collection registration, never a client path.
pub type ProjectBinding = Sha256Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerIdentity {
    pub service: Incarnation,
    pub worker: Incarnation,
    pub project: ProjectBinding,
    pub fencing_generation: u64,
}

/// Bearer credential: serializable only for protected IPC, intentionally redacted in Debug.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionGrant {
    pub protocol: u32,
    pub identity: WorkerIdentity,
    pub caller: Incarnation,
    pub token: Sha256Digest,
    pub ttl_ms: u64,
}

impl std::fmt::Debug for ConnectionGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionGrant")
            .field("identity", &self.identity)
            .field("caller", &self.caller)
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryAllowance {
    pub concurrent_queries: usize,
    pub response_bytes: usize,
}

/// Content-free in-process policy observation, not proof of OS process health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerStatus {
    pub service: Incarnation,
    pub accepting: bool,
    pub projects: usize,
    pub draining_projects: usize,
    pub pending_connections: usize,
    pub allocated: QueryAllowance,
    pub active_queries: usize,
    pub reserved_response_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionError {
    InvalidRequest,
    UnknownWorker,
    StaleIdentity,
    InvalidGrant,
    Expired,
    Busy,
    Draining,
    OwnershipLost,
    UnknownRequest,
}

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AdmissionError {}
