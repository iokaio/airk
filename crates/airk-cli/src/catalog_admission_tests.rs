// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor, service::ProjectBinding};

fn durable_fixture(
    request: &CatalogRequest,
    plan: &StoreFencePlan,
) -> Result<(FenceIntent, FencedReceipt)> {
    use airk_store::service::registry::catalog::operations::{CatalogFenceProof, FenceStage};
    let preview_digest = Sha256Digest::from_bytes([19; 32]);
    let prepared_digest = airk_core::identity::framed_digest(
        b"airk/catalog-fence-prepare/1",
        &[
            &serde_json::to_vec(request)?,
            &serde_json::to_vec(&preview_digest)?,
            &serde_json::to_vec(plan)?,
        ],
    );
    let key = FenceKey::new(request)?;
    let intent = FenceIntent {
        schema: 1,
        operation_id: key.operation_id,
        request: request.clone(),
        preview_digest,
        prepared_digest,
        plan: plan.clone(),
        stage: FenceStage::Prepared,
        confirmations: Vec::new(),
    };
    intent.validate()?;
    let receipt = FencedReceipt {
        request: request.clone(),
        metadata: airk_core::catalog_management::CatalogReceipt {
            schema: 1,
            operation_id: key.operation_id,
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            catalog_revision: request.expected_catalog_revision.next()?,
            authority_fence_required: true,
        },
        preview_digest,
        prepared_digest,
        proof: CatalogFenceProof {
            plan_digest: plan.identity()?,
            service: plan.service,
            gate_generation: plan.gate_generation,
            confirmations: plan.targets.iter().cloned().map(|target| airk_store::service::registry::catalog::operations::FenceConfirmation { target, method: airk_store::service::registry::catalog::operations::FenceConfirmationMethod::NativeScopeAck }).collect(),
        },
    };
    receipt.validate_for(request, preview_digest, prepared_digest, plan)?;
    Ok((intent, receipt))
}

fn fixture(
    root: &str,
) -> (
    NativeCatalogAdmission,
    WorkerIdentity,
    ScopedReadAuthority,
    Incarnation,
) {
    let namespace = Incarnation::from_bytes([1; 16]);
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([2; 16]),
        worker: Incarnation::from_bytes([3; 16]),
        project: ProjectBinding::from_bytes([4; 32]),
        fencing_generation: 1,
    };
    let index_id = IndexId::from_identity(Incarnation::from_bytes([5; 16]));
    let project_id = QueryProjectId::from_identity(Incarnation::from_bytes([6; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([7; 16]));
    let authority = ScopedReadAuthority {
        schema: 1,
        backing: worker.project,
        index_id,
        project_ids: vec![project_id],
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([8; 16])),
            host: Sha256Digest::from_bytes([9; 32]),
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
            original_root: root.into(),
        }],
    };
    let gate = NativeCatalogAdmission::new(worker.service, namespace, CatalogRevision::INITIAL, 1)
        .unwrap();
    (gate, worker, authority, namespace)
}
fn expiry() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn request(namespace: Incarnation, authority: &ScopedReadAuthority) -> CatalogRequest {
    CatalogRequest {
        caller: Incarnation::from_bytes([10; 16]),
        key: RequestKey {
            namespace,
            epoch: 1,
            nonce: Incarnation::from_bytes([11; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::RevokeGrant {
            id: authority.grant.id,
            expected_revision: authority.grant.revision,
        },
    }
}
fn install(
    gate: &NativeCatalogAdmission,
    worker: WorkerIdentity,
    authority: &ScopedReadAuthority,
    namespace: Incarnation,
) -> Result<(IssueTicket, InstallTicket)> {
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let reservation = gate.reserve_install(issue, worker, authority, expiry())?;
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    Ok((issue, reservation))
}

#[test]
fn admission_requires_ack_and_exact_service_namespace_revision_and_worker() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    assert!(
        gate.begin_issue(Incarnation::from_bytes([12; 16]), CatalogRevision::INITIAL)
            .is_err()
    );
    assert!(
        gate.begin_issue(namespace, CatalogRevision::INITIAL.next()?)
            .is_err()
    );
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    assert!(
        gate.reserve_install(issue, worker, &authority, expiry())
            .is_err()
    );
    gate.register_worker(worker)?;
    let reservation = gate.reserve_install(issue, worker, &authority, expiry())?;
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    let mut foreign = worker;
    foreign.fencing_generation += 1;
    assert!(
        gate.acknowledge_install(reservation, foreign, reservation.scope)
            .is_err()
    );
    let mut wrong_ref = reservation.scope;
    wrong_ref.fingerprint = Sha256Digest::from_bytes([13; 32]);
    assert!(
        gate.acknowledge_install(reservation, worker, wrong_ref)
            .is_err()
    );
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    gate.check_scope_issue(issue, worker, reservation.scope)?;
    assert!(
        gate.check_scope_issue(issue, foreign, reservation.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn full_confirmation_methods_preserve_first_native_observation_after_actual_reap() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (_, installed) = install(&gate, worker, &authority, namespace)?;
    let request = request(namespace, &authority);
    let plan = gate.close_request_admission(&request)?;
    gate.record_revoked(&plan.ticket, worker, installed.scope)?;
    gate.observe_worker_reaped(worker)?;
    gate.record_revoked(&plan.ticket, worker, installed.scope)?;
    let confirmations = gate.confirmations(&plan.ticket)?;
    assert_eq!(confirmations.len(), 1);
    assert_eq!(
        confirmations[0].target,
        FenceTarget {
            worker,
            scope: installed.scope
        }
    );
    assert_eq!(
        confirmations[0].method,
        FenceConfirmationMethod::NativeScopeAck
    );
    Ok(())
}

#[test]
fn close_captures_inflight_installs_and_ack_or_expiry_cannot_reopen_admission() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let reservation = gate.reserve_install(issue, worker, &authority, expiry())?;
    let request = request(namespace, &authority);
    let key = FenceKey::new(&request)?;
    let plan =
        gate.close_admission(key.clone(), ScopeTarget::for_operation(&request.operation)?)?;
    assert_eq!(
        plan.targets,
        vec![FenceTarget {
            worker,
            scope: reservation.scope
        }]
    );
    assert!(gate.check_issue(issue).is_err());
    assert!(
        gate.begin_issue(namespace, CatalogRevision::INITIAL)
            .is_err()
    );
    assert!(
        gate.acknowledge_install(reservation, worker, reservation.scope)
            .is_err()
    );
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    let mut wrong = worker;
    wrong.worker = Incarnation::from_bytes([14; 16]);
    assert!(
        gate.record_revoked(&plan.ticket, wrong, reservation.scope)
            .is_err()
    );
    gate.record_revoked(&plan.ticket, worker, reservation.scope)?;
    gate.record_revoked(&plan.ticket, worker, reservation.scope)?;
    assert_eq!(gate.require_commit_ready(&plan.ticket)?, plan.targets);
    assert!(
        gate.check_issue(issue).is_err(),
        "ACK alone reopened admission before metadata commit"
    );
    let replay = gate.close_admission(key.clone(), ScopeTarget::Grant(authority.grant.id))?;
    assert_eq!(replay.ticket, plan.ticket);
    let mut conflict = key;
    conflict.request_digest = Sha256Digest::from_bytes([15; 32]);
    assert!(
        gate.close_admission(conflict, ScopeTarget::Grant(authority.grant.id))
            .is_err()
    );
    Ok(())
}

#[test]
fn expiry_retains_tombstone_and_requires_explicit_revoke_or_actual_reap() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, mut authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, reservation) = install(&gate, worker, &authority, namespace)?;
    // Advance only the owned fixture's monotonic expiry; no wall-clock sleeps.
    gate.0
        .lock()
        .unwrap()
        .installed
        .values_mut()
        .next()
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    gate.expire()?;
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    assert!(
        gate.reserve_install(issue, worker, &authority, expiry())
            .is_err()
    );
    authority.grant.revision = authority.grant.revision.next()?;
    assert!(
        gate.reserve_install(issue, worker, &authority, expiry())
            .is_err(),
        "expiry replaced native revocation acknowledgement"
    );
    let request = request(namespace, &authority);
    let plan = gate.close_admission(
        FenceKey::new(&request)?,
        ScopeTarget::Grant(authority.grant.id),
    )?;
    gate.expire()?;
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    let mut foreign = worker;
    foreign.service = Incarnation::from_bytes([16; 16]);
    assert!(gate.observe_worker_reaped(foreign).is_err());
    gate.observe_worker_reaped(worker)?;
    assert!(gate.require_commit_ready(&plan.ticket).is_ok());
    Ok(())
}

#[test]
fn affected_scope_associations_are_immutable_and_legacy_promotion_requires_all_reaps() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (gate, worker, mut authority, namespace) = fixture(root.path().to_str().unwrap());
    let (_, reservation) = install(&gate, worker, &authority, namespace)?;
    let project = authority.project_ids[0];
    authority.project_ids.clear();
    let req = request(namespace, &authority);
    let plan = gate.close_admission(
        FenceKey::new(&req)?,
        ScopeTarget::Membership {
            project_id: project,
            index_id: authority.index_id,
        },
    )?;
    assert_eq!(
        plan.targets,
        vec![FenceTarget {
            worker,
            scope: reservation.scope
        }]
    );

    let gate = NativeCatalogAdmission::new(worker.service, namespace, CatalogRevision::INITIAL, 0)?;
    gate.register_worker(worker)?;
    let mut second = worker;
    second.worker = Incarnation::from_bytes([17; 16]);
    second.project = ProjectBinding::from_bytes([18; 32]);
    gate.register_worker(second)?;
    gate.check_legacy_admission()?;
    let plan = gate.close_admission(FenceKey::new(&req)?, ScopeTarget::LegacyPromotion)?;
    assert!(gate.check_legacy_admission().is_err());
    assert_eq!(plan.drain_workers.len(), 2);
    gate.observe_worker_reaped(worker)?;
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.observe_worker_reaped(second)?;
    assert!(gate.require_commit_ready(&plan.ticket).is_ok());
    Ok(())
}

#[test]
fn bounded_install_ledger_never_evicts_expired_or_revoked_authority() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, mut authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    for n in 0..MAX_SCOPES {
        authority.grant.id = GrantId::from_identity(Incarnation::from_bytes([n as u8; 16]));
        gate.reserve_install(issue, worker, &authority, expiry())?;
    }
    authority.grant.id = GrantId::from_identity(Incarnation::from_bytes([255; 16]));
    assert!(
        gate.reserve_install(issue, worker, &authority, expiry())
            .is_err()
    );
    assert_eq!(gate.0.lock().unwrap().installed.len(), MAX_SCOPES);
    // A full target census larger than the durable lane's byte bound refuses
    // before closing or changing the issuance generation; no subset is used.
    let req = request(namespace, &authority);
    assert!(
        gate.close_admission(FenceKey::new(&req)?, ScopeTarget::LegacyPromotion)
            .is_err()
    );
    gate.check_issue(issue)?;
    assert!(
        gate.reserve_install(
            issue,
            worker,
            &authority,
            Instant::now() + MAX_LIFETIME + Duration::from_secs(1)
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn synchronous_delivery_effect_retains_gate_until_actual_poll_returns() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, reservation) = install(&gate, worker, &authority, namespace)?;
    let observed = gate.with_scope_issue(issue, worker, reservation.scope, || {
        assert!(matches!(
            gate.0.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        std::task::Poll::<()>::Pending
    })?;
    assert!(observed.is_pending());
    let req = request(namespace, &authority);
    gate.close_admission(FenceKey::new(&req)?, ScopeTarget::Grant(authority.grant.id))?;
    let called = std::cell::Cell::new(false);
    assert!(
        gate.with_scope_issue(issue, worker, reservation.scope, || called.set(true))
            .is_err()
    );
    assert!(!called.get());
    Ok(())
}

#[test]
fn only_exact_typed_terminal_receipt_and_complete_native_acknowledgements_reopen() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, mut authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, reservation) = install(&gate, worker, &authority, namespace)?;
    let request = request(namespace, &authority);
    let plan = gate.close_request_admission(&request)?;
    let (intent, receipt) = durable_fixture(&request, &plan.durable)?;
    // A structurally valid complete metadata/fence receipt is insufficient until
    // this native owner's exact worker acknowledgements have been observed.
    assert!(
        gate.finish_commit(
            &plan.ticket,
            &request,
            intent.preview_digest,
            intent.prepared_digest,
            &receipt
        )
        .is_err()
    );
    gate.record_revoked(&plan.ticket, worker, reservation.scope)?;
    let mut wrong = receipt.clone();
    wrong.metadata.request_digest = Sha256Digest::from_bytes([20; 32]);
    assert!(
        gate.finish_commit(
            &plan.ticket,
            &request,
            intent.preview_digest,
            intent.prepared_digest,
            &wrong
        )
        .is_err()
    );
    let mut wrong = receipt.clone();
    wrong.proof.gate_generation += 1;
    assert!(
        gate.finish_commit(
            &plan.ticket,
            &request,
            intent.preview_digest,
            intent.prepared_digest,
            &wrong
        )
        .is_err()
    );
    assert!(gate.check_issue(issue).is_err());
    gate.finish_commit(
        &plan.ticket,
        &request,
        intent.preview_digest,
        intent.prepared_digest,
        &receipt,
    )?;
    assert!(
        gate.check_issue(issue).is_err(),
        "old issuance ticket survived native commit"
    );
    let next = gate.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    assert!(
        gate.reserve_install(next, worker, &authority, expiry())
            .is_err(),
        "same revoked revision resurrected"
    );
    authority.grant.revision = authority.grant.revision.next()?;
    let replacement = gate.reserve_install(next, worker, &authority, expiry())?;
    gate.acknowledge_install(replacement, worker, replacement.scope)?;
    gate.check_scope_issue(next, worker, replacement.scope)?;
    assert!(
        gate.check_scope_issue(next, worker, reservation.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn restart_restores_closed_gate_and_preserves_exact_original_durable_plan() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (_, reservation) = install(&gate, worker, &authority, namespace)?;
    let request = request(namespace, &authority);
    let plan = gate.close_request_admission(&request)?;
    let (intent, mut receipt) = durable_fixture(&request, &plan.durable)?;
    let replacement_service = Incarnation::from_bytes([21; 16]);
    let restored =
        NativeCatalogAdmission::new(replacement_service, namespace, CatalogRevision::INITIAL, 1)?;
    let restored_plan = restored.restore_pending_fence(&intent)?;
    assert_eq!(restored_plan.durable, plan.durable);
    assert!(
        restored
            .begin_issue(namespace, CatalogRevision::INITIAL)
            .is_err()
    );
    assert!(
        restored
            .require_commit_ready(&restored_plan.ticket)
            .is_err()
    );
    assert!(
        restored
            .observe_worker_reaped(WorkerIdentity {
                service: replacement_service,
                ..worker
            })
            .is_err()
    );
    restored.observe_worker_reaped(worker)?;
    restored.observe_worker_reaped(worker)?;
    for confirmation in &mut receipt.proof.confirmations {
        confirmation.method = airk_store::service::registry::catalog::operations::FenceConfirmationMethod::NativeWorkerReap;
    }
    restored.finish_commit(
        &restored_plan.ticket,
        &request,
        intent.preview_digest,
        intent.prepared_digest,
        &receipt,
    )?;
    assert!(restored.register_worker(worker).is_err());
    let new_worker = WorkerIdentity {
        service: replacement_service,
        worker: Incarnation::from_bytes([22; 16]),
        ..worker
    };
    restored.register_worker(new_worker)?;
    let issue = restored.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    assert!(
        restored
            .check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn parallel_same_revision_subsets_have_exact_fanout_and_new_revision_waits_for_all_native_fences()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, mut one, namespace) = fixture(root.path().to_str().unwrap());
    one.grant.memberships[0].membership.scope[0].path = "one".into();
    let mut other = one.grant.memberships[0].clone();
    other.membership.project_id = QueryProjectId::from_identity(Incarnation::from_bytes([23; 16]));
    other.membership.scope[0].path = "two".into();
    one.grant.memberships.push(other.clone());
    let mut two = one.clone();
    two.project_ids = vec![other.membership.project_id];
    let (issue, first) = install(&gate, worker, &one, namespace)?;
    let (_, second) = install(&gate, worker, &two, namespace)?;
    assert_ne!(first.scope.fingerprint, second.scope.fingerprint);
    assert!(second.scope.lease_generation > first.scope.lease_generation);
    let cached = gate.reserve_install(issue, worker, &one, expiry())?;
    assert_eq!(cached.scope, first.scope);
    assert!(!cached.requires_install());
    let mut newer = one.clone();
    newer.grant.revision = newer.grant.revision.next()?;
    assert!(
        gate.reserve_install(issue, worker, &newer, expiry())
            .is_err()
    );
    gate.check_scope_issue(issue, worker, first.scope)?;
    gate.check_scope_issue(issue, worker, second.scope)?;
    let request = request(namespace, &one);
    let plan = gate.close_request_admission(&request)?;
    assert_eq!(plan.targets.len(), 2);
    gate.record_revoked(&plan.ticket, worker, first.scope)?;
    assert_eq!(gate.confirmations(&plan.ticket)?.len(), 1);
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.record_revoked(&plan.ticket, worker, second.scope)?;
    let (intent, receipt) = durable_fixture(&request, &plan.durable)?;
    gate.finish_commit(
        &plan.ticket,
        &request,
        intent.preview_digest,
        intent.prepared_digest,
        &receipt,
    )?;
    let next = gate.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    let replacement = gate.reserve_install(next, worker, &newer, expiry())?;
    gate.acknowledge_install(replacement, worker, replacement.scope)?;
    assert!(gate.check_scope_issue(next, worker, first.scope).is_err());
    assert!(gate.check_scope_issue(next, worker, second.scope).is_err());
    assert!(gate.reserve_install(next, worker, &two, expiry()).is_err());
    assert_eq!(gate.0.lock().unwrap().installed.len(), 3);
    Ok(())
}

#[test]
fn native_lease_renewal_reconnects_same_authority_without_growth_or_old_lease_reactivation()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, initial) = install(&gate, worker, &authority, namespace)?;
    let mut current = initial.scope;
    for _ in 0..20 {
        gate.0
            .lock()
            .unwrap()
            .installed
            .values_mut()
            .next()
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        gate.expire()?;
        assert!(
            gate.reserve_install(issue, worker, &authority, expiry())
                .is_err()
        );
        let expires = expiry();
        let renewal = gate.renew_expired_lease(issue, worker, &authority, current, expires)?;
        let replay = gate.renew_expired_lease(issue, worker, &authority, current, expires)?;
        assert_eq!(
            replay, renewal,
            "uncertain renewal allocated another lease generation"
        );
        assert_eq!(renewal.previous(), Some(current));
        assert!(renewal.requires_install());
        assert!(renewal.scope.lease_generation > current.lease_generation);
        assert_eq!(renewal.scope.fingerprint, initial.scope.fingerprint);
        assert_eq!(renewal.scope.grant_revision, initial.scope.grant_revision);
        assert!(gate.check_scope_issue(issue, worker, current).is_err());
        gate.acknowledge_install(renewal, worker, renewal.scope)?;
        assert!(gate.check_scope_issue(issue, worker, current).is_err());
        current = renewal.scope;
        gate.check_scope_issue(issue, worker, current)?;
    }
    let state = gate.0.lock().unwrap();
    assert_eq!(state.installed.len(), 1);
    assert_eq!(state.highwater.len(), 1);
    assert_eq!(state.lease_highwater.len(), 1);
    assert!(
        state
            .installed
            .values()
            .next()
            .unwrap()
            .pending_previous
            .is_none()
    );
    drop(state);
    assert!(
        gate.renew_expired_lease(issue, worker, &authority, initial.scope, expiry())
            .is_err()
    );
    Ok(())
}

#[test]
fn uncertain_renewal_captures_both_leases_and_requires_both_exact_fences() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, old) = install(&gate, worker, &authority, namespace)?;
    gate.0
        .lock()
        .unwrap()
        .installed
        .values_mut()
        .next()
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    let renewal = gate.renew_expired_lease(issue, worker, &authority, old.scope, expiry())?;
    let request = request(namespace, &authority);
    let plan = gate.close_request_admission(&request)?;
    assert_eq!(plan.targets.len(), 2);
    assert!(plan.targets.contains(&FenceTarget {
        worker,
        scope: old.scope
    }));
    assert!(plan.targets.contains(&FenceTarget {
        worker,
        scope: renewal.scope
    }));
    plan.durable.validate()?;
    assert!(
        gate.acknowledge_install(renewal, worker, renewal.scope)
            .is_err()
    );
    gate.record_revoked(&plan.ticket, worker, old.scope)?;
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.record_revoked(&plan.ticket, worker, renewal.scope)?;
    assert_eq!(gate.require_commit_ready(&plan.ticket)?.len(), 2);
    let (intent, receipt) = durable_fixture(&request, &plan.durable)?;
    gate.finish_commit(
        &plan.ticket,
        &request,
        intent.preview_digest,
        intent.prepared_digest,
        &receipt,
    )?;
    let next = gate.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    assert!(
        gate.renew_expired_lease(next, worker, &authority, renewal.scope, expiry())
            .is_err()
    );
    Ok(())
}

#[test]
fn expiry_narrowing_requires_exact_ack_and_generation_overflow_preserves_live_scopes() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, original) = install(&gate, worker, &authority, namespace)?;
    let narrowed = gate.reserve_install(
        issue,
        worker,
        &authority,
        Instant::now() + Duration::from_secs(10),
    )?;
    assert!(narrowed.requires_install());
    assert_eq!(narrowed.scope, original.scope);
    assert!(
        gate.check_scope_issue(issue, worker, original.scope)
            .is_err()
    );
    assert!(
        gate.acknowledge_install(original, worker, original.scope)
            .is_err()
    );
    gate.acknowledge_install(narrowed, worker, narrowed.scope)?;
    gate.check_scope_issue(issue, worker, narrowed.scope)?;
    let mut another = authority.clone();
    another.grant.memberships[0].membership.scope[0].path = "different".into();
    gate.0
        .lock()
        .unwrap()
        .lease_highwater
        .insert(worker.worker, u64::MAX - 1);
    assert!(
        gate.reserve_install(issue, worker, &another, expiry())
            .is_err()
    );
    gate.check_scope_issue(issue, worker, narrowed.scope)?;
    assert_eq!(gate.0.lock().unwrap().installed.len(), 1);
    Ok(())
}
