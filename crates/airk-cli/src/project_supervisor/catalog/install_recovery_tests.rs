// SPDX-License-Identifier: Apache-2.0
//! Private exchange failures must reach the supervisor's native worker retirement
//! path. These tests model its already-verified reap callback, not an OS proof.
use super::*;
use airk_core::{
    identity::Sha256Digest,
    index_catalog::*,
    scoped_reads::{ScopeReadRefused, SourceDescriptor},
};
use airk_kernel::scoped_reads::ScopedReadAuthority;

struct Fixture {
    gate: NativeCatalogAdmission,
    namespace: Incarnation,
    affected: WorkerIdentity,
    authority: ScopedReadAuthority,
    healthy: WorkerIdentity,
    healthy_scope: ScopedReadRef,
    issue: IssueTicket,
}

fn authority(worker: WorkerIdentity, seed: u8) -> ScopedReadAuthority {
    let index_id = IndexId::from_identity(Incarnation::from_bytes([seed; 16]));
    let project_id = QueryProjectId::from_identity(Incarnation::from_bytes([seed + 1; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([seed + 2; 16]));
    ScopedReadAuthority {
        schema: 1,
        backing: worker.project,
        index_id,
        project_ids: vec![project_id],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([seed + 3; 16])),
            host: Sha256Digest::from_bytes([seed + 4; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id,
                    index_id,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: std::env::temp_dir().to_string_lossy().into_owned(),
        }],
    }
}

impl Fixture {
    fn new() -> Result<Self> {
        let namespace = Incarnation::from_bytes([1; 16]);
        let affected = WorkerIdentity {
            service: Incarnation::from_bytes([2; 16]),
            worker: Incarnation::from_bytes([3; 16]),
            project: Sha256Digest::from_bytes([4; 32]),
            fencing_generation: 1,
        };
        let healthy = WorkerIdentity {
            worker: Incarnation::from_bytes([5; 16]),
            project: Sha256Digest::from_bytes([6; 32]),
            ..affected
        };
        let gate =
            NativeCatalogAdmission::new(affected.service, namespace, CatalogRevision::INITIAL, 1)?;
        gate.register_worker(affected)?;
        gate.register_worker(healthy)?;
        let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
        let installed = gate.reserve_install(
            issue,
            healthy,
            &authority(healthy, 30),
            Instant::now() + Duration::from_secs(60),
        )?;
        let healthy_scope = installed.scope();
        gate.acknowledge_install(installed, healthy, healthy_scope)?;
        Ok(Self {
            gate,
            namespace,
            affected,
            authority: authority(affected, 10),
            healthy,
            healthy_scope,
            issue,
        })
    }

    fn reserve(&self, lifetime: Duration) -> Result<InstallTicket> {
        self.gate.reserve_validated_scope(
            self.issue,
            self.affected,
            &self.authority,
            Instant::now() + lifetime,
        )
    }

    fn pending(&self, ticket: InstallTicket) -> Pending {
        Pending {
            gate: self.gate.clone(),
            expected: Expected::Install(ticket),
        }
    }

    fn assert_healthy(&self) -> Result<()> {
        self.gate
            .check_scope_issue(self.issue, self.healthy, self.healthy_scope)
    }

    fn recover_after_verified_reap(&self, rejected: ScopedReadRef) -> Result<()> {
        self.assert_healthy()?;
        assert!(!self.gate.worker_was_reaped(self.affected)?);
        assert!(
            self.gate
                .check_scope_issue(self.issue, self.affected, rejected)
                .is_err()
        );
        let foreign = WorkerIdentity {
            fencing_generation: self.affected.fencing_generation + 1,
            ..self.affected
        };
        assert!(self.gate.observe_worker_reaped(foreign).is_err());
        self.assert_healthy()?;
        // This is the callback used only after the existing supervisor has
        // completed its owned process/provider drain and native lease exclusion.
        self.gate.observe_worker_reaped(self.affected)?;
        assert!(self.gate.worker_was_reaped(self.affected)?);
        assert!(self.reserve(Duration::from_secs(60)).is_err());
        let replacement = WorkerIdentity {
            worker: Incarnation::from_bytes([7; 16]),
            fencing_generation: self.affected.fencing_generation + 1,
            ..self.affected
        };
        self.gate.register_worker(replacement)?;
        let issue = self
            .gate
            .begin_issue(self.namespace, CatalogRevision::INITIAL)?;
        let install = self.gate.reserve_validated_scope(
            issue,
            replacement,
            &self.authority,
            Instant::now() + Duration::from_secs(60),
        )?;
        assert!(install.previous().is_none());
        let scope = install.scope();
        assert!(matches!(
            self.pending(install).accept(Reply::ScopeInstalled {
                identity: replacement,
                scope,
            })??,
            Exchange::Installed
        ));
        self.gate.check_scope_issue(issue, replacement, scope)?;
        self.assert_healthy()
    }
}

fn expire(gate: &NativeCatalogAdmission, ticket: InstallTicket) -> Result<()> {
    while Instant::now() < ticket.expires() {
        std::thread::sleep(ticket.expires().saturating_duration_since(Instant::now()));
    }
    gate.expire()
}

#[test]
fn refused_first_install_requires_worker_retirement_without_resetting_other_authority() -> Result<()>
{
    let f = Fixture::new()?;
    let ticket = f.reserve(Duration::from_secs(60))?;
    let error = f
        .pending(ticket)
        .accept(Reply::Failed)
        .err()
        .expect("refused installation must quarantine");
    assert!(
        error
            .to_string()
            .contains("native scope installation refused")
    );
    f.recover_after_verified_reap(ticket.scope())
}

#[test]
fn expired_first_install_ack_requires_worker_retirement_without_issuing_a_grant() -> Result<()> {
    let f = Fixture::new()?;
    let ticket = f.reserve(Duration::from_millis(100))?;
    expire(&f.gate, ticket)?;
    let error = f
        .pending(ticket)
        .accept(Reply::ScopeInstalled {
            identity: f.affected,
            scope: ticket.scope(),
        })
        .err()
        .expect("expired installation ACK must quarantine");
    assert!(error.is::<ScopeReadRefused>());
    assert!(
        error
            .to_string()
            .contains("acknowledgement could not settle")
    );
    f.recover_after_verified_reap(ticket.scope())
}

#[test]
fn expired_uncertain_renewal_requires_reap_before_a_new_worker_can_install() -> Result<()> {
    let f = Fixture::new()?;
    let initial = f.reserve(Duration::from_millis(100))?;
    f.gate
        .acknowledge_install(initial, f.affected, initial.scope())?;
    expire(&f.gate, initial)?;
    let renewal = f.reserve(Duration::from_millis(100))?;
    assert_eq!(renewal.previous(), Some(initial.scope()));
    expire(&f.gate, renewal)?;
    assert!(
        f.reserve(Duration::from_secs(60)).is_err(),
        "an uncertain expired generation cannot be reset as absent"
    );
    let error = f
        .pending(renewal)
        .accept(Reply::ScopeInstalled {
            identity: f.affected,
            scope: renewal.scope(),
        })
        .err()
        .expect("expired renewal ACK must quarantine");
    assert!(error.is::<ScopeReadRefused>());
    f.assert_healthy()?;
    assert!(
        f.gate
            .check_scope_issue(f.issue, f.affected, initial.scope())
            .is_err()
    );
    f.recover_after_verified_reap(renewal.scope())
}
