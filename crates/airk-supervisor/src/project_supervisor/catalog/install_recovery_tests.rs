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

#[test]
fn selection_uncertain_renewal_fence_settles_both_actual_private_acks() -> Result<()> {
    use airk_core::{
        catalog_management::CatalogRequest,
        catalog_source_change::{SourceChangePlan, SourceConfigurationSnapshot, source_job_id},
        scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
        service::{ProjectRegistration, RequestKey},
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
        source_policy::SourceRoot,
    };
    use airk_kernel::scoped_reads::SelectionReadAuthority;
    use std::collections::BTreeMap;

    for previous_first in [false, true] {
        let directory = tempfile::tempdir()?;
        let root = directory.path().to_string_lossy().into_owned();
        let registration = ProjectRegistration::new(
            root.clone(),
            directory.path().join("data").to_string_lossy().into_owned(),
            "selection-renewal".into(),
        )?;
        let f = Fixture::new()?;
        let worker = WorkerIdentity {
            worker: Incarnation::from_bytes([70; 16]),
            project: registration.binding,
            ..f.affected
        };
        f.gate.register_worker(worker)?;
        let catalog = authority(worker, 60);
        let member = &catalog.grant.memberships[0].membership;
        let selection = SelectionScopeRef {
            owner: SelectionOwnerKind::Window,
            grant_nonce: Incarnation::from_bytes([71; 16]),
            service_instance: worker.service,
            scope_identity: Sha256Digest::from_bytes([72; 32]),
            catalog_revision: CatalogRevision::INITIAL,
        };
        let selected = SelectionReadAuthority {
            schema: 1,
            backing: worker.project,
            index_id: catalog.index_id,
            project_ids: catalog.project_ids.clone(),
            selection,
            atoms: member
                .scope
                .iter()
                .map(|atom| (member.project_id, atom.clone()))
                .collect(),
            sources: vec![SourceDescriptor {
                source_root_id: catalog.sources[0].source_root_id,
                original_root: root.clone(),
            }],
            membership_revisions: vec![(member.project_id, member.revision)],
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        };
        f.gate.register_selection(&selection)?;
        let initial = f.gate.reserve_selection_scope(
            f.issue,
            worker,
            &selected,
            Instant::now() + Duration::from_secs(60),
        )?;
        assert!(matches!(
            f.pending(initial).accept(Reply::SelectionScopeInstalled {
                identity: worker,
                lease: protocol::SelectionLease::of(&initial.scope()).context("selection lease")?,
            })??,
            Exchange::Installed
        ));
        f.gate.expire_install_for_test(initial)?;
        let renewal = f.gate.reserve_selection_scope(
            f.issue,
            worker,
            &selected,
            Instant::now() + Duration::from_secs(60),
        )?;
        assert_eq!(renewal.previous(), Some(initial.scope()));
        // The private renewal exchange has not acknowledged installation.
        let caller = Incarnation::from_bytes([73; 16]);
        let key = RequestKey {
            namespace: f.namespace,
            epoch: 1,
            nonce: Incarnation::from_bytes([74; 16]),
        };
        let job = source_job_id(registration.binding, caller, &key)?;
        let request = CatalogRequest {
            caller,
            key,
            expected_catalog_revision: CatalogRevision::INITIAL,
            operation: CatalogOperation::BeginIndexSourceChange {
                id: selected.index_id,
                expected_source_revision: CatalogRevision::INITIAL,
                plan: Box::new(SourceChangePlan {
                    job,
                    registration,
                    engine_identity: Sha256Digest::from_bytes([75; 32]),
                    configuration: ConfigurationRequest {
                        prepare: false,
                        incarnation: Incarnation::from_bytes([76; 16]),
                        settings: Sha256Digest::from_bytes([77; 32]),
                        configuration: ProjectConfiguration::RemoveRoot {
                            alias: "obsolete".into(),
                        },
                    },
                    before: SourceConfigurationSnapshot {
                        primary_root: root.clone(),
                        aliases: BTreeMap::from([(
                            "obsolete".into(),
                            directory
                                .path()
                                .join("obsolete")
                                .to_string_lossy()
                                .into_owned(),
                        )]),
                    },
                    after: SourceConfigurationSnapshot {
                        primary_root: root.clone(),
                        aliases: BTreeMap::new(),
                    },
                    sources: vec![SourceRoot {
                        id: selected.sources[0].source_root_id,
                        canonical_path: root,
                        aliases: vec![],
                        owner: true,
                    }],
                }),
            },
        };
        request.validate()?;
        let fence = f.gate.close_request_admission(&request)?;
        assert!(fence.targets.is_empty());
        assert!(fence.durable.targets.is_empty());
        assert_eq!(
            f.gate.unsettled_selection_revocations(&fence.ticket)?,
            vec![(worker, renewal.scope()), (worker, initial.scope())]
        );
        let [first, second] = if previous_first {
            [initial.scope(), renewal.scope()]
        } else {
            [renewal.scope(), initial.scope()]
        };
        for (position, scope) in [first, second].into_iter().enumerate() {
            // The supervisor checks this before dispatch. An early false here
            // skips the real ACK and strands the fence's previous obligation.
            assert!(f.gate.selection_revocation_pending(worker, scope)?);
            assert!(matches!(
                (Pending {
                    gate: f.gate.clone(),
                    expected: Expected::RevokeSelection { worker, scope }
                })
                .accept(Reply::SelectionScopeRevoked {
                    identity: worker,
                    lease: protocol::SelectionLease::of(&scope)
                        .context("revoked selection lease")?
                })??,
                Exchange::Revoked
            ));
            assert!(!f.gate.selection_revocation_pending(worker, scope)?);
            if position == 0 {
                assert!(f.gate.require_commit_ready(&fence.ticket).is_err());
                assert_eq!(
                    f.gate.unsettled_selection_revocations(&fence.ticket)?,
                    vec![(worker, second)]
                );
            }
        }
        assert!(f.gate.require_commit_ready(&fence.ticket)?.is_empty());
        assert!(
            f.gate
                .unsettled_selection_revocations(&fence.ticket)?
                .is_empty()
        );
        assert!(!f.gate.worker_was_reaped(worker)?);
    }
    Ok(())
}
