// SPDX-License-Identifier: Apache-2.0
//! Native catalog ports inside the existing Supervisor actor; no second owner.
#![allow(dead_code)]
use super::*;
use crate::{
    catalog_admission::{FenceTicket, InstallTicket, NativeCatalogAdmission},
    catalog_authority::NativeCatalogAuthoritySelection,
    catalog_coordinator::CatalogControl,
};
use airk_core::service::{ConnectionGrant, MAX_GRANT_TTL_MS, PROTOCOL_VERSION, ScopedReadRef};
use airk_store::service::registry::catalog::operations::FenceTarget;
use std::{
    sync::Weak,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) enum Operation {
    Attach,
    Connect {
        selection: NativeCatalogAuthoritySelection,
        lifetime: Duration,
    },
    Revoke {
        ticket: FenceTicket,
        target: FenceTarget,
    },
    Reaped {
        worker: WorkerIdentity,
    },
}
pub(super) enum Response {
    Attached,
    Connection(ControlResponse),
    Revoked,
    Reaped(bool),
}

/// Weak queue ownership lets Supervisor stop/drain even if a caller retains a port.
#[derive(Clone)]
pub(crate) struct CatalogControlHandle {
    sender: Weak<SyncSender<Envelope>>,
    gate: NativeCatalogAdmission,
}
impl Supervisor {
    pub(crate) fn catalog_control(
        &self,
        gate: NativeCatalogAdmission,
    ) -> Result<CatalogControlHandle> {
        let handle = CatalogControlHandle {
            sender: Arc::downgrade(self.sender.as_ref().context("supervisor closed")?),
            gate,
        };
        ensure!(
            matches!(handle.call(Operation::Attach)?, Response::Attached),
            "catalog port attachment failed"
        );
        Ok(handle)
    }
}
impl CatalogControlHandle {
    fn call(&self, operation: Operation) -> Result<Response> {
        let (reply, receive) = mpsc::sync_channel(1);
        let sender = self.sender.upgrade().context("catalog supervisor closed")?;
        sender
            .try_send(Envelope::Catalog {
                gate: self.gate.clone(),
                operation,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("catalog supervisor busy"))?;
        drop(sender);
        // Dropping this waiter never cancels accepted actor work or frees native
        // query/work capacity. A late exact ACK updates the shared fence ledger.
        receive
            .recv_timeout(Duration::from_secs(15))
            .context("catalog native outcome unknown")?
    }
    pub(crate) fn issue_scoped(
        &self,
        selection: NativeCatalogAuthoritySelection,
        lifetime: Duration,
    ) -> Result<ControlResponse> {
        airk_core::index_catalog::unique_bounded(
            &selection.project_ids,
            airk_core::index_catalog::MAX_SELECTION,
        )?;
        ensure!(
            !selection.project_ids.is_empty()
                && !lifetime.is_zero()
                && lifetime <= Duration::from_secs(300),
            "invalid native scoped connection budget"
        );
        match self.call(Operation::Connect {
            selection,
            lifetime,
        })? {
            Response::Connection(value) => Ok(value),
            _ => anyhow::bail!("unexpected catalog connection response"),
        }
    }
}
impl CatalogControl for CatalogControlHandle {
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool {
        self.gate.same_instance(gate)
    }
    fn revoke(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()> {
        ensure!(
            matches!(
                self.call(Operation::Revoke {
                    ticket: ticket.clone(),
                    target: target.clone()
                })?,
                Response::Revoked
            ),
            "unexpected catalog revocation response"
        );
        Ok(())
    }
    fn confirm_reaped(&self, worker: WorkerIdentity) -> Result<bool> {
        match self.call(Operation::Reaped { worker })? {
            Response::Reaped(value) => Ok(value),
            _ => anyhow::bail!("unexpected catalog reap response"),
        }
    }
}

pub(super) enum Expected {
    Install(InstallTicket),
    Revoke {
        ticket: FenceTicket,
        target: FenceTarget,
    },
    Grant {
        worker: WorkerIdentity,
        scope: ScopedReadRef,
        caller: Incarnation,
    },
}
pub(super) struct Pending {
    gate: NativeCatalogAdmission,
    expected: Expected,
}
enum Exchange {
    Installed,
    Revoked,
    Grant(ConnectionGrant),
}
impl Pending {
    fn accept(self, reply: Reply) -> Result<Result<Exchange>> {
        // Outer failure means an unidentifiable/corrupt reply: quarantine. Inner
        // failure means the exact exchange settled but authority changed/refused.
        if matches!(reply, Reply::Failed) {
            return Ok(Err(anyhow::anyhow!("native catalog command refused")));
        }
        match (self.expected, reply) {
            (Expected::Install(ticket), Reply::ScopeInstalled { identity, scope }) => {
                ensure!(
                    identity == ticket.worker() && scope == ticket.scope(),
                    "foreign native scope installation reply"
                );
                Ok(self
                    .gate
                    .acknowledge_install(ticket, identity, scope)
                    .map(|_| Exchange::Installed))
            }
            (Expected::Revoke { ticket, target }, Reply::ScopeRevoked { identity, scope }) => {
                ensure!(
                    identity == target.worker && scope == target.scope,
                    "foreign native scope revocation reply"
                );
                Ok(self
                    .gate
                    .record_revoked(&ticket, identity, scope)
                    .map(|_| Exchange::Revoked))
            }
            (
                Expected::Grant {
                    worker,
                    scope,
                    caller,
                },
                Reply::Grant { grant },
            ) => {
                ensure!(
                    grant.protocol == PROTOCOL_VERSION
                        && grant.identity == worker
                        && grant.scope == Some(scope)
                        && grant.caller == caller
                        && (1..=MAX_GRANT_TTL_MS).contains(&grant.ttl_ms),
                    "foreign native scoped grant reply"
                );
                Ok(Ok(Exchange::Grant(grant)))
            }
            _ => anyhow::bail!("unidentifiable native catalog reply"),
        }
    }
}
impl ChildWorker {
    fn settle_catalog_reply(&mut self, reply: Reply) -> Result<Result<Exchange>> {
        let pending = self
            .catalog_pending
            .take()
            .context("catalog exchange missing")?;
        match pending.accept(reply) {
            Ok(result) => Ok(result),
            Err(error) => {
                self.catalog_quarantined = true;
                Err(error)
            }
        }
    }
    pub(super) fn catalog_poll_ready(&mut self) -> Result<bool> {
        ensure!(
            !self.catalog_quarantined,
            "native catalog control quarantined"
        );
        if self.catalog_pending.is_none() {
            return Ok(true);
        }
        match self.output.try_recv() {
            Ok(reply) => {
                let _settled = self.settle_catalog_reply(reply)?;
                Ok(true)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(false),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.catalog_quarantined = true;
                anyhow::bail!("native catalog control closed with unknown outcome")
            }
        }
    }
    pub(super) fn require_catalog_ready(&mut self) -> Result<()> {
        ensure!(
            self.catalog_poll_ready()?,
            "native catalog exchange still pending"
        );
        Ok(())
    }
    fn catalog_exchange(
        &mut self,
        command: WorkerCommand,
        gate: &NativeCatalogAdmission,
        expected: Expected,
    ) -> Result<Exchange> {
        self.require_catalog_ready()?;
        self.catalog_gate = Some(gate.clone());
        self.catalog_pending = Some(Pending {
            gate: gate.clone(),
            expected,
        });
        if let Err(error) = service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &command,
        ) {
            self.catalog_quarantined = true;
            return Err(error);
        }
        let reply = self
            .output
            .recv_timeout(Duration::from_secs(3))
            .context("native catalog exchange outcome unknown")?;
        self.settle_catalog_reply(reply)?
    }
}

fn expires_unix_ms(expires: Instant) -> Result<u64> {
    let remaining = expires
        .checked_duration_since(Instant::now())
        .context("native scope lease expired")?;
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .checked_add(remaining)
        .context("native scope expiry overflow")?
        .as_millis();
    u64::try_from(value).context("native scope expiry overflow")
}
fn confirmed_reap(
    gate: &NativeCatalogAdmission,
    workers: &BTreeMap<ProjectBinding, ChildWorker>,
    root: &Path,
    worker: WorkerIdentity,
    may_allocate: bool,
) -> Result<bool> {
    if gate.worker_was_reaped(worker)? {
        return Ok(true);
    }
    if workers.contains_key(&worker.project) || !may_allocate {
        return Ok(false);
    }
    // Actual native owner exclusion, after existing predecessor/provider drain,
    // proves the old scope cannot still be served. A missing map/EOF alone does not.
    let Ok(_exclusion) = ProjectLease::acquire(root, worker.project) else {
        return Ok(false);
    };
    gate.observe_worker_reaped(worker)?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    bound: &mut Option<NativeCatalogAdmission>,
    gate: NativeCatalogAdmission,
    operation: Operation,
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    registry: &mut Option<ServiceRegistry>,
    root: &Path,
    instance: Incarnation,
    may_allocate: bool,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    admission: WorkerAdmission<'_>,
) -> Result<Response> {
    gate.require_service(instance)?;
    if let Some(previous) = bound.as_ref() {
        ensure!(previous.same_instance(&gate), "native catalog gate changed");
    } else {
        ensure!(
            matches!(operation, Operation::Attach),
            "native catalog port is not attached"
        );
        for worker in workers.values_mut() {
            let identity = worker.identity.context("project identity unavailable")?;
            gate.register_worker(identity)?;
            worker.catalog_gate = Some(gate.clone());
        }
        *bound = Some(gate.clone());
    }
    match operation {
        Operation::Attach => Ok(Response::Attached),
        Operation::Connect {
            selection,
            lifetime,
        } => {
            ensure!(
                !lifetime.is_zero() && lifetime <= Duration::from_secs(300) && may_allocate,
                "native scoped connection unavailable"
            );
            let issue = gate.begin_issue(selection.namespace, selection.catalog_revision)?;
            let registry = registry.as_mut().context("catalog registry unavailable")?;
            let authority =
                crate::catalog_authority::build_catalog_read_authority(registry, &selection)?;
            let registration = registry.get(selection.backing)?.registration;
            start_worker(
                workers,
                root,
                registration,
                instance,
                false,
                backoff,
                admission,
            )?;
            let worker = workers
                .get_mut(&selection.backing)
                .context("native scoped worker unavailable")?;
            ensure!(!worker.derived, "native scoped worker role changed");
            worker.require_catalog_ready()?;
            let identity = worker
                .identity
                .context("native scoped worker identity unavailable")?;
            gate.register_worker(identity)?;
            worker.catalog_gate = Some(gate.clone());
            // Reservation and actual private sends share this one actor turn.
            let ticket = gate.reserve_validated_scope(
                issue,
                identity,
                &authority,
                Instant::now() + lifetime,
            )?;
            if ticket.requires_install() {
                let command = match ticket.previous() {
                    Some(previous) => WorkerCommand::RenewScopeLease {
                        identity,
                        authority: Box::new(authority),
                        previous,
                        lease_generation: ticket.scope().lease_generation,
                        expires_unix_ms: expires_unix_ms(ticket.expires())?,
                    },
                    None => WorkerCommand::InstallScope {
                        identity,
                        authority: Box::new(authority),
                        lease_generation: ticket.scope().lease_generation,
                        expires_unix_ms: expires_unix_ms(ticket.expires())?,
                    },
                };
                ensure!(
                    matches!(
                        worker.catalog_exchange(command, &gate, Expected::Install(ticket))?,
                        Exchange::Installed
                    ),
                    "native installation acknowledgement missing"
                );
            }
            gate.check_scope_issue(issue, identity, ticket.scope())?;
            let caller = local_ipc::caller_identity()?;
            let Exchange::Grant(grant) = worker.catalog_exchange(
                WorkerCommand::GrantScoped {
                    identity,
                    caller,
                    scope: ticket.scope(),
                },
                &gate,
                Expected::Grant {
                    worker: identity,
                    scope: ticket.scope(),
                    caller,
                },
            )?
            else {
                anyhow::bail!("native scoped grant missing")
            };
            gate.with_scope_issue(issue, identity, ticket.scope(), || {
                Response::Connection(ControlResponse::Connection {
                    role: worker.role.clone(),
                    grant,
                })
            })
        }
        Operation::Revoke { ticket, target } => {
            gate.validate_target(&ticket, &target)?;
            if confirmed_reap(&gate, workers, root, target.worker, may_allocate)? {
                return Ok(Response::Revoked);
            }
            let worker = workers
                .get_mut(&target.worker.project)
                .context("native revocation target still unavailable")?;
            ensure!(
                worker.identity == Some(target.worker),
                "native revocation worker changed"
            );
            let command = WorkerCommand::RevokeScope {
                identity: target.worker,
                scope: target.scope,
            };
            ensure!(
                matches!(
                    worker.catalog_exchange(command, &gate, Expected::Revoke { ticket, target })?,
                    Exchange::Revoked
                ),
                "native revocation acknowledgement missing"
            );
            Ok(Response::Revoked)
        }
        Operation::Reaped { worker } => Ok(Response::Reaped(confirmed_reap(
            &gate,
            workers,
            root,
            worker,
            may_allocate,
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(
        NativeCatalogAdmission,
        WorkerIdentity,
        ScopedReadRef,
        Incarnation,
    )> {
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        let gate = NativeCatalogAdmission::new(
            worker.service,
            Incarnation::from_bytes([4; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
            1,
        )?;
        let scope = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes(
                [5; 16],
            )),
            grant_id: airk_core::index_catalog::GrantId::from_identity(Incarnation::from_bytes(
                [6; 16],
            )),
            grant_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
            fingerprint: airk_core::identity::Sha256Digest::from_bytes([7; 32]),
        };
        Ok((gate, worker, scope, Incarnation::from_bytes([8; 16])))
    }
    fn grant(worker: WorkerIdentity, scope: ScopedReadRef, caller: Incarnation) -> ConnectionGrant {
        ConnectionGrant {
            protocol: PROTOCOL_VERSION,
            identity: worker,
            caller,
            token: airk_core::identity::Sha256Digest::from_bytes([9; 32]),
            ttl_ms: 15_000,
            client: airk_core::usage::ClientFamily::Unknown,
            scope: Some(scope),
        }
    }
    #[test]
    fn actual_private_frame_grant_handler_retains_full_scope_and_refuses_foreign_lease_ttl_or_reply_kind()
    -> Result<()> {
        let (gate, worker, scope, caller) = fixture()?;
        let mut frame = Vec::new();
        service_ipc::write_frame(
            &mut frame,
            &Reply::Grant {
                grant: grant(worker, scope, caller),
            },
        )?;
        let observed = service_ipc::read_frame::<_, Reply>(&mut std::io::Cursor::new(frame))?
            .context("missing private fixture reply")?;
        let pending = Pending {
            gate: gate.clone(),
            expected: Expected::Grant {
                worker,
                scope,
                caller,
            },
        };
        assert!(
            matches!(pending.accept(observed)??,Exchange::Grant(value) if value.scope==Some(scope))
        );
        for mutate in 0..4 {
            let mut value = grant(worker, scope, caller);
            match mutate {
                0 => value.scope.as_mut().unwrap().lease_generation += 1,
                1 => value.ttl_ms = MAX_GRANT_TTL_MS + 1,
                2 => value.identity.fencing_generation += 1,
                _ => value.caller = Incarnation::from_bytes([10; 16]),
            }
            let pending = Pending {
                gate: gate.clone(),
                expected: Expected::Grant {
                    worker,
                    scope,
                    caller,
                },
            };
            assert!(pending.accept(Reply::Grant { grant: value }).is_err());
        }
        let pending = Pending {
            gate,
            expected: Expected::Grant {
                worker,
                scope,
                caller,
            },
        };
        assert!(pending.accept(Reply::Renewed).is_err());
        Ok(())
    }
    #[test]
    fn weak_control_port_cannot_retain_owner_or_replay_after_supervisor_queue_closes() -> Result<()>
    {
        let (gate, _, _, _) = fixture()?;
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(1);
        let owner = Arc::new(sender);
        let handle = CatalogControlHandle {
            sender: Arc::downgrade(&owner),
            gate,
        };
        drop(owner);
        assert!(handle.call(Operation::Attach).is_err());
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        Ok(())
    }
}
