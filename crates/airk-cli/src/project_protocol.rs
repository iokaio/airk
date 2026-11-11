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
    pub engine: airk_core::identity::Sha256Digest,
    pub runtime: PathBuf,
    pub project: ProjectBinding,
    pub service: Incarnation,
    pub worker: Incarnation,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Private host-only installation after live catalog validation.
    InstallScope {
        identity: WorkerIdentity,
        authority: Box<airk_kernel::scoped_reads::ScopedReadAuthority>,
        lease_generation: u64,
        expires_unix_ms: u64,
    },
    RenewScopeLease {
        identity: WorkerIdentity,
        authority: Box<airk_kernel::scoped_reads::ScopedReadAuthority>,
        previous: airk_core::service::ScopedReadRef,
        lease_generation: u64,
        expires_unix_ms: u64,
    },
    RevokeScope {
        identity: WorkerIdentity,
        scope: airk_core::service::ScopedReadRef,
    },
    GrantScoped {
        identity: WorkerIdentity,
        caller: Incarnation,
        scope: airk_core::service::ScopedReadRef,
    },
    ConfigureQueries {
        identity: WorkerIdentity,
        concurrency: usize,
    },
    Renew {
        identity: WorkerIdentity,
    },
    Activate {
        identity: WorkerIdentity,
        allowance: QueryAllowance,
        pool: Option<Box<crate::embedding_pool_protocol::Access>>,
        #[serde(default)]
        pool_pending: bool,
    },
    ConfigurePool {
        identity: WorkerIdentity,
        pool: Option<Box<crate::embedding_pool_protocol::Access>>,
        #[serde(default)]
        pool_pending: bool,
    },
    RunJob {
        identity: WorkerIdentity,
        allowance: airk_core::service_resources::WorkAllowance,
    },
    WorkStatus {
        identity: WorkerIdentity,
        control: Option<WorkControl>,
        keep_for_maintenance: bool,
        freshness: Option<Box<airk_core::changes::FreshnessObservation>>,
    },
    Grant {
        caller: Incarnation,
    },
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    ScopeInstalled {
        identity: WorkerIdentity,
        scope: airk_core::service::ScopedReadRef,
    },
    ScopeRevoked {
        identity: WorkerIdentity,
        scope: airk_core::service::ScopedReadRef,
    },
    QueriesConfigured,
    Renewed,
    Activated,
    PoolConfigured,
    WorkAccepted,
    WorkStatus {
        #[serde(flatten)]
        observation: WorkObservation,
    },
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
    #[serde(default)]
    pub frontend: Option<airk_core::service::RuntimeProcess>,
    pub tool: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub native: Option<airk_mcp::local::NativeRead>,
    #[serde(default)]
    pub status_receipt: airk_mcp::response::StatusReceipt,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResult {
    pub result: serde_json::Value,
    pub status_receipt: airk_mcp::response::StatusReceipt,
    #[serde(default)]
    pub rebind: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub identity: WorkerIdentity,
}

pub fn role(worker: Incarnation) -> String {
    format!("q-{worker}")
}

#[cfg(test)]
mod helper_admission_tests {
    use super::*;

    #[test]
    fn scoped_private_commands_require_explicit_contract_and_worker_identity() {
        for missing in [
            serde_json::json!({"install_scope":{"expires_unix_ms":1}}),
            serde_json::json!({"grant_scoped":{"caller":"missing-scope"}}),
            serde_json::json!({"revoke_scope":{}}),
        ] {
            assert!(serde_json::from_value::<Command>(missing).is_err());
        }
    }

    #[test]
    fn legacy_activation_and_pool_updates_never_invent_pending_authority() -> anyhow::Result<()> {
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: ProjectBinding::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        for (kind, body) in [
            (
                "activate",
                serde_json::json!({"identity":identity,"allowance":{"concurrent_queries":1,"response_bytes":airk_core::service::MAX_FRAME_BYTES},"pool":null}),
            ),
            (
                "configure_pool",
                serde_json::json!({"identity":identity,"pool":null}),
            ),
        ] {
            let mut frame = serde_json::json!({});
            frame[kind] = body;
            let command: Command = serde_json::from_value(frame.clone())?;
            assert!(matches!(
                command,
                Command::Activate {
                    pool: None,
                    pool_pending: false,
                    ..
                } | Command::ConfigurePool {
                    pool: None,
                    pool_pending: false,
                    ..
                }
            ));
            let mut pending = frame.clone();
            pending[kind]["pool_pending"] = true.into();
            let command: Command = serde_json::from_value(pending)?;
            assert!(matches!(
                command,
                Command::Activate {
                    pool: None,
                    pool_pending: true,
                    ..
                } | Command::ConfigurePool {
                    pool: None,
                    pool_pending: true,
                    ..
                }
            ));
            let mut invalid = frame;
            invalid[kind]["pool_pending"] = "true".into();
            assert!(serde_json::from_value::<Command>(invalid).is_err());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkControl {
    pub grant: Incarnation,
    pub paused: bool,
    pub canceled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkObservation {
    pub active: Option<Incarnation>,
    pub finished: Option<Incarnation>,
    pub maintenance: Option<airk_core::service::MaintenanceObservation>,
    pub progress: Option<Box<airk_core::service_progress::WorkProgress>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_for: Option<airk_core::service_resources::WaitReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) failure: Option<crate::service_logs::JobFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) integrity_observation_failure: Option<String>,
}
