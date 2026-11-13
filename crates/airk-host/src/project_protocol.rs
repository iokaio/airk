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
    Checkpoint {
        identity: WorkerIdentity,
        operation_id: Incarnation,
        seal: bool,
    },
    ResolveSourceChange {
        identity: WorkerIdentity,
        nonce: Incarnation,
        begin_operation: Incarnation,
    },
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
    /// Private host-only installation of an in-memory W4/A2 selection grant's
    /// authority, built at its exact catalog revision. `previous` names the
    /// expired lease of the same live selection being renewed. A separate
    /// command keeps every catalog-grant frame unchanged.
    InstallSelectionScope {
        identity: WorkerIdentity,
        authority: Box<airk_kernel::scoped_reads::SelectionReadAuthority>,
        previous: Option<airk_core::service::ScopedReadRef>,
        lease_generation: u64,
        expires_unix_ms: u64,
    },
    /// U105 (contract step): private host-only installation of an open
    /// agent read's authority. A separate command keeps every other frame
    /// unchanged; the worker refuses it (`Failed`) until open reads land.
    InstallOpenScope {
        identity: WorkerIdentity,
        authority: Box<airk_kernel::scoped_reads::OpenReadAuthority>,
        previous: Option<airk_core::service::ScopedReadRef>,
        lease_generation: u64,
        expires_unix_ms: u64,
    },
    GrantScoped {
        identity: WorkerIdentity,
        caller: Incarnation,
        scope: airk_core::service::ScopedReadRef,
    },
    /// Fast private native-admin issuance. The owning runtime consumes the
    /// credential internally; it is never a public evidence connection.
    GrantAdmin {
        identity: WorkerIdentity,
        caller: Incarnation,
        nonce: Incarnation,
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
    Checkpoint {
        observation: WorkerCheckpointObservation,
    },
    SourceChangeResolution {
        identity: WorkerIdentity,
        nonce: Incarnation,
        observation: Option<Box<airk_core::catalog_source_change::SourceChangeObservation>>,
    },
    AdminGrant {
        identity: WorkerIdentity,
        nonce: Incarnation,
        grant: Option<ConnectionGrant>,
    },
    ScopeInstalled {
        identity: WorkerIdentity,
        scope: airk_core::service::ScopedReadRef,
    },
    ScopeRevoked {
        identity: WorkerIdentity,
        scope: airk_core::service::ScopedReadRef,
    },
    /// Selection-scope replies never echo a selection ref (N3): the lease
    /// generation and the fingerprint (which binds the selection) identify it.
    SelectionScopeInstalled {
        identity: WorkerIdentity,
        lease: SelectionLease,
    },
    SelectionScopeRevoked {
        identity: WorkerIdentity,
        lease: SelectionLease,
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

/// The reply identity of one selection-scope lease: never the selection ref.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionLease {
    pub index_id: airk_core::index_catalog::IndexId,
    pub lease_generation: u64,
    pub fingerprint: airk_core::identity::Sha256Digest,
}
impl SelectionLease {
    /// `None` for a catalog-grant ref, which keeps its existing replies.
    pub fn of(scope: &airk_core::service::ScopedReadRef) -> Option<Self> {
        scope.selection.map(|_| Self {
            index_id: scope.index_id,
            lease_generation: scope.lease_generation,
            fingerprint: scope.fingerprint,
        })
    }
}

/// The scope a worker reply may echo: catalog-grant refs only. A selection
/// ref never leaves the private command direction (N3).
pub fn reply_scope(
    scope: Option<airk_core::service::ScopedReadRef>,
) -> Option<airk_core::service::ScopedReadRef> {
    scope.filter(|scope| scope.selection.is_none())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerCheckpointObservation {
    pub schema: u32,
    pub identity: WorkerIdentity,
    pub operation_id: Incarnation,
    pub engine_sha256: airk_core::identity::Sha256Digest,
    pub admission_closed: bool,
    pub sealed: bool,
    pub active_work_grant: Option<Incarnation>,
    pub active_queries: usize,
    /// True only after the job thread has committed its native outcome and all
    /// post-completion storage effects, and query delivery has released charges.
    pub durable_boundary: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// Feature-version gated; omitted for legacy workers rejecting unknown keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_admission: Option<SelectionReadAdmission>,
    pub result: serde_json::Value,
    pub status_receipt: airk_mcp::response::StatusReceipt,
    #[serde(default)]
    pub rebind: bool,
}

/// Private worker-authenticated proof that no handler, verification or usage
/// observation began. Never reconstructed from public error JSON.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionReadAdmission {
    pub request_id: Incarnation,
    pub identity: WorkerIdentity,
    pub scope: Option<airk_core::service::ScopedReadRef>,
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
    fn source_yield_control_requires_an_explicit_compatible_private_frame() -> anyhow::Result<()> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PreviousControl {
            #[serde(rename = "grant")]
            _grant: Incarnation,
            #[serde(rename = "paused")]
            _paused: bool,
            #[serde(rename = "canceled")]
            _canceled: bool,
        }
        let mut frame = serde_json::json!({
            "grant":Incarnation::from_bytes([9;16]), "paused":false, "canceled":false
        });
        assert!(serde_json::from_value::<WorkControl>(frame.clone()).is_err());
        frame["yield_to_source"] = true.into();
        let current: WorkControl = serde_json::from_value(frame.clone())?;
        assert!(current.yield_to_source);
        assert!(serde_json::from_value::<PreviousControl>(frame.clone()).is_err());
        frame["yield_to_source"] = "true".into();
        assert!(serde_json::from_value::<WorkControl>(frame).is_err());
        Ok(())
    }

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
    fn selection_scope_replies_and_admission_proofs_never_carry_a_selection_ref()
    -> anyhow::Result<()> {
        use airk_core::{
            identity::Sha256Digest,
            index_catalog::{CatalogRevision, IndexId},
            scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
            service::ScopedReadRef,
        };
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: ProjectBinding::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        let nonce = Incarnation::from_bytes([0x5c; 16]);
        let scope = ScopedReadRef {
            schema: 1,
            lease_generation: 7,
            index_id: IndexId::from_identity(Incarnation::from_bytes([4; 16])),
            grant_id: None,
            grant_revision: None,
            selection: Some(SelectionScopeRef {
                owner: SelectionOwnerKind::Window,
                grant_nonce: nonce,
                service_instance: identity.service,
                scope_identity: Sha256Digest::from_bytes([5; 32]),
                catalog_revision: CatalogRevision::INITIAL,
            }),
            fingerprint: Sha256Digest::from_bytes([6; 32]),
        };
        let lease = SelectionLease::of(&scope).expect("a selection lease");
        let nonce_text = serde_json::to_string(&nonce)?;
        let nonce_text = nonce_text.trim_matches('"');
        for reply in [
            Reply::SelectionScopeInstalled { identity, lease },
            Reply::SelectionScopeRevoked { identity, lease },
        ] {
            let frame = serde_json::to_string(&reply)?;
            assert!(!frame.contains(nonce_text), "{frame}");
            assert!(!frame.contains("selection\":{"), "{frame}");
            assert!(!frame.contains("grant_nonce"), "{frame}");
        }
        let proof = SelectionReadAdmission {
            request_id: Incarnation::from_bytes([7; 16]),
            identity,
            scope: reply_scope(Some(scope)),
        };
        assert!(proof.scope.is_none());
        assert!(!serde_json::to_string(&proof)?.contains(nonce_text));
        // A catalog-grant ref keeps its existing echo unchanged.
        let catalog = ScopedReadRef {
            grant_id: Some(airk_core::index_catalog::GrantId::from_identity(
                Incarnation::from_bytes([8; 16]),
            )),
            grant_revision: Some(CatalogRevision::INITIAL),
            selection: None,
            ..scope
        };
        assert_eq!(reply_scope(Some(catalog)), Some(catalog));
        assert_eq!(SelectionLease::of(&catalog), None);
        Ok(())
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
    pub yield_to_source: bool,
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
    pub failure: Option<crate::service_logs::JobFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_observation_failure: Option<String>,
}
