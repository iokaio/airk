// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor, service::ProjectBinding};

fn trim_request(namespace: Incarnation, authority: &ScopedReadAuthority) -> Result<CatalogRequest> {
    use airk_core::{
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
        source_policy::{
            DenyReason, DenyRule, POLICY_SCHEMA, PolicyChange, SourcePolicy, SourceRoot,
        },
    };
    let mut request = request(namespace, authority);
    let root = &authority.sources[0];
    let rule_id = DenyId::from_identity(Incarnation::from_bytes([52; 16]));
    let selector = ScopeAtom {
        source_root_id: root.source_root_id,
        kind: PathScope::Subtree,
        path: "private".into(),
    };
    let policy = SourcePolicy {
        schema: POLICY_SCHEMA.into(),
        index_id: authority.index_id,
        owner_binding: authority.backing.to_string().parse()?,
        namespace: "collections-1.4".into(),
        collection: "fixture".into(),
        epoch: CatalogRevision::INITIAL,
        roots: vec![SourceRoot {
            id: root.source_root_id,
            canonical_path: root.original_root.clone(),
            aliases: vec![],
            owner: true,
        }],
        positive_includes: None,
        denies: vec![DenyRule {
            source_root_id: selector.source_root_id,
            kind: selector.kind,
            path: selector.path.clone(),
            rule_id,
            reason: DenyReason::Trim,
            created_epoch: CatalogRevision::INITIAL,
            inherited_from: None,
        }],
    };
    let operation_id =
        airk_core::catalog_management::catalog_operation_id(request.caller, &request.key)?;
    request.operation = CatalogOperation::ChangeIndexPolicy {
        id: authority.index_id,
        expected_source_revision: CatalogRevision::INITIAL,
        expected_policy_epoch: 0,
        expected_policy_digest: None,
        change: PolicyChange::Trim { rule_id, selector },
        configuration: Box::new(ConfigurationRequest {
            prepare: false,
            incarnation: Incarnation::from_bytes([53; 16]),
            settings: Sha256Digest::from_bytes([54; 32]),
            configuration: ProjectConfiguration::ApplySourcePolicy {
                policy: Box::new(policy),
                operation_id,
            },
        }),
        engine_identity: Sha256Digest::from_bytes([55; 32]),
    };
    request.validate()?;
    Ok(request)
}

#[test]
fn permanent_trim_requires_unscoped_worker_census_and_provider_drain_before_commit() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let request = trim_request(namespace, &authority)?;
    let plan = gate.close_retirement_admission(&request, worker.project)?;
    assert!(
        plan.targets.is_empty(),
        "unscoped worker has no Project scope entries"
    );
    assert!(gate.require_worker_start(worker.project).is_err());
    assert!(
        gate.with_administrative_worker(namespace, worker, || ())
            .is_err()
    );
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    let plan = gate.capture_retirement_workers(
        &plan.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, vec![])],
    )?;
    assert_eq!(plan.drain_workers, vec![worker]);
    gate.begin_worker_drain(&plan.ticket, worker)?;
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.require_commit_ready(&plan.ticket).is_err(),
        "process exit alone cannot settle helper/resource work"
    );
    gate.observe_worker_resources_drained(worker)?;
    gate.require_commit_ready(&plan.ticket)?;
    Ok(())
}

#[test]
fn permanent_trim_closes_pre_persistence_combined_and_administrative_delivery() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let mut delivered = 0;
    gate.with_issue(issue, || delivered += 1)?;
    gate.with_administrative_worker(namespace, worker, || delivered += 1)?;
    let request = trim_request(namespace, &authority)?;
    let _closed = gate.close_retirement_admission(&request, worker.project)?;
    assert!(gate.with_issue(issue, || delivered += 1).is_err());
    assert!(
        gate.with_administrative_worker(namespace, worker, || delivered += 1)
            .is_err()
    );
    assert_eq!(
        delivered, 2,
        "neither retained delivery can cross the native policy close"
    );
    Ok(())
}

#[test]
fn combined_delivery_poll_refuses_the_pre_persistence_pending_fence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, _, authority, namespace) = fixture(directory.path().to_str().unwrap());
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let mut delivered = 0;
    gate.with_issue(issue, || delivered += 1)?;
    assert_eq!(delivered, 1);
    // This native close precedes any durable registry mutation. A persisted
    // metadata snapshot alone would still see the original catalog revision.
    let _pending = gate.close_request_admission(&request(namespace, &authority))?;
    assert!(gate.with_issue(issue, || delivered += 1).is_err());
    assert_eq!(delivered, 1, "revoked delivery effect must never be polled");
    assert!(
        gate.begin_issue(namespace, CatalogRevision::INITIAL)
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn combined_delivery_stops_an_actual_partial_native_write_after_admission_close() -> Result<()>
{
    use tokio::io::AsyncReadExt;
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let response = airk_core::service::ControlResponse::CombinedDeliveryValidated {
        receipt: airk_core::service::CombinedDeliveryReceipt {
            schema: 1,
            service_instance: worker.service,
            namespace,
            catalog_revision: CatalogRevision::INITIAL,
            scope_identity: Sha256Digest::from_bytes([21; 32]),
            result_digest: Sha256Digest::from_bytes([22; 32]),
        },
    };
    let (mut reader, mut writer) = tokio::io::duplex(1);
    let owner = gate.clone();
    let writing = tokio::spawn(async move {
        owner
            .deliver_issue(issue, &mut writer, &response, Duration::from_secs(5))
            .await
    });
    let mut prefix = [0; 1];
    reader.read_exact(&mut prefix).await?;
    let _pending = gate.close_request_admission(&request(namespace, &authority))?;
    // Wake a writer already waiting for buffer capacity; its next actual poll
    // must observe the close even though the registry clock is unchanged.
    let mut buffered = [0; 1];
    let buffered_bytes = reader.read(&mut buffered).await?;
    let error = writing.await?.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("combined_delivery_scope_expired"),
        "{error:#}"
    );
    let mut remainder = Vec::new();
    reader.read_to_end(&mut remainder).await?;
    assert!(
        buffered_bytes + remainder.len() < 4,
        "a partial native frame cannot become a complete receipt"
    );
    Ok(())
}

#[test]
fn retirement_captures_unscoped_worker_and_reopens_only_after_exact_reap_proof() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let mut request = request(namespace, &authority);
    request.operation = CatalogOperation::RetireIndex {
        id: authority.index_id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let closed = gate.close_request_admission(&request)?;
    assert!(closed.targets.is_empty(), "fixture has no evidence scopes");
    assert!(gate.require_commit_ready(&closed.ticket).is_err());
    assert!(gate.require_worker_start(worker.project).is_err());
    gate.bind_retirement_backing(&closed.ticket, authority.index_id, worker.project)?;
    let captured = gate.capture_retirement_workers(
        &closed.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, Vec::new())],
    )?;
    assert_eq!(captured.drain_workers, vec![worker]);
    assert_eq!(
        captured.durable.worker_obligation.as_ref().unwrap().workers,
        vec![worker]
    );
    assert!(
        gate.require_worker_start(ProjectBinding::from_bytes([33; 32]))
            .is_ok()
    );
    // Native owner exclusion / actual child reap is supplied at this ledger
    // boundary; this unit test does not claim an OS process was exercised.
    let mut wrong = worker;
    wrong.worker = Incarnation::from_bytes([45; 16]);
    assert!(gate.observe_worker_reaped(wrong).is_err());
    assert!(gate.require_commit_ready(&captured.ticket).is_err());
    gate.begin_worker_drain(&captured.ticket, worker)?;
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.require_commit_ready(&captured.ticket).is_err(),
        "child exit cannot acknowledge accepted provider work"
    );
    gate.observe_worker_resources_drained(worker)?;
    gate.require_commit_ready(&captured.ticket)?;
    assert_eq!(
        gate.worker_confirmations(&captured.ticket)?,
        vec![WorkerFenceConfirmation {
            worker,
            method: WorkerFenceConfirmationMethod::NativeWorkerReap
        }]
    );
    let (_, receipt) = durable_fixture(&request, &captured.durable)?;
    gate.finish_commit(
        &captured.ticket,
        &request,
        receipt.preview_digest,
        receipt.prepared_digest,
        &receipt,
    )?;
    gate.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    Ok(())
}

#[test]
fn retirement_restart_keeps_exact_census_and_cannot_replace_old_worker_with_empty_capture()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let mut request = request(namespace, &authority);
    request.operation = CatalogOperation::RetireIndex {
        id: authority.index_id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let closed = gate.close_request_admission(&request)?;
    gate.bind_retirement_backing(&closed.ticket, authority.index_id, worker.project)?;
    let captured = gate.capture_retirement_workers(
        &closed.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, Vec::new())],
    )?;
    let repeated =
        gate.capture_retirement_workers(&closed.ticket, worker.project, vec![], vec![])?;
    assert_eq!(
        repeated.durable, captured.durable,
        "uncertain census retry must reuse original obligation"
    );
    let (intent, _) = durable_fixture(&request, &captured.durable)?;
    let replacement = NativeCatalogAdmission::new(
        Incarnation::from_bytes([50; 16]),
        namespace,
        CatalogRevision::INITIAL,
        1,
    )?;
    let restored = replacement.restore_pending_fence(&intent)?;
    assert_eq!(restored.drain_workers, vec![worker]);
    assert_eq!(restored.durable, captured.durable);
    assert!(replacement.require_worker_start(worker.project).is_err());
    assert!(replacement.require_commit_ready(&restored.ticket).is_err());
    replacement.observe_worker_reaped(worker)?;
    assert!(replacement.require_commit_ready(&restored.ticket).is_err());
    replacement.observe_worker_resources_drained(worker)?;
    replacement.require_commit_ready(&restored.ticket)?;
    Ok(())
}

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
        worker_confirmations: Vec::new(),
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
            worker_obligation: plan.worker_obligation.clone(),
            worker_confirmations: plan.worker_obligation.as_ref().map(|o| o.workers.iter().map(|worker| WorkerFenceConfirmation {worker: *worker, method:WorkerFenceConfirmationMethod::NativeWorkerReap}).collect()).unwrap_or_default(),
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
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
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
        let mut reviewed_id = [n as u8; 16];
        reviewed_id[0] &= 0x7f;
        authority.grant.id = GrantId::from_identity(Incarnation::from_bytes(reviewed_id));
        gate.reserve_install(issue, worker, &authority, expiry())?;
    }
    let mut overflow_id = [255; 16];
    overflow_id[0] &= 0x7f;
    authority.grant.id = GrantId::from_identity(Incarnation::from_bytes(overflow_id));
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

// Selection read authority (spec §3.3, §3.4): in-memory W4/A2 grants.

fn selection_ref(service: Incarnation, nonce: u8) -> SelectionScopeRef {
    SelectionScopeRef {
        owner: SelectionOwnerKind::Window,
        grant_nonce: Incarnation::from_bytes([nonce; 16]),
        service_instance: service,
        scope_identity: Sha256Digest::from_bytes([0x33; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    }
}
/// The selection authority over the catalog fixture's current membership.
fn selection_authority(
    authority: &ScopedReadAuthority,
    selection: SelectionScopeRef,
) -> SelectionReadAuthority {
    let member = &authority.grant.memberships[0].membership;
    SelectionReadAuthority {
        schema: 1,
        backing: authority.backing,
        index_id: authority.index_id,
        project_ids: authority.project_ids.clone(),
        selection,
        atoms: member
            .scope
            .iter()
            .map(|atom| (member.project_id, atom.clone()))
            .collect(),
        sources: authority.sources.clone(),
        membership_revisions: vec![(member.project_id, member.revision)],
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    }
}
fn install_selection(
    gate: &NativeCatalogAdmission,
    worker: WorkerIdentity,
    authority: &SelectionReadAuthority,
    namespace: Incarnation,
) -> Result<(IssueTicket, InstallTicket)> {
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let reservation = gate.reserve_selection_scope(issue, worker, authority, expiry())?;
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    Ok((issue, reservation))
}
fn selection_live(gate: &NativeCatalogAdmission, nonce: u8) -> bool {
    gate.0
        .lock()
        .unwrap()
        .live_selections
        .contains_key(&Incarnation::from_bytes([nonce; 16]))
}

#[test]
fn selection_admission_requires_a_live_nonce_of_this_service_and_prunes_after_ack() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    let authority = selection_authority(&catalog, selection);
    // Not registered: nothing admits it.
    assert!(install_selection(&gate, worker, &authority, namespace).is_err());
    // Another service's ref, or a second registration of a nonce, is refused.
    assert!(
        gate.register_selection(&selection_ref(Incarnation::from_bytes([42; 16]), 0x5c))
            .is_err()
    );
    gate.register_selection(&selection)?;
    assert!(gate.register_selection(&selection).is_err());
    let (issue, reservation) = install_selection(&gate, worker, &authority, namespace)?;
    let scope = reservation.scope;
    assert_eq!((scope.grant_id, scope.grant_revision), (None, None));
    assert_eq!(scope.selection, Some(selection));
    gate.check_scope_issue(issue, worker, scope)?;
    // Owner revocation: nothing persisted, no generation bump.
    let generation = gate.0.lock().unwrap().generation;
    let pairs = gate.revoke_selection(&[selection.grant_nonce])?;
    assert_eq!(pairs, vec![(worker, scope)]);
    assert_eq!(gate.0.lock().unwrap().generation, generation);
    assert!(gate.check_scope_issue(issue, worker, scope).is_err());
    assert!(
        gate.reserve_selection_scope(issue, worker, &authority, expiry())
            .is_err()
    );
    assert!(gate.selection_revocation_pending(worker, scope)?);
    // The worker's acknowledgement prunes the entry and its high-water.
    gate.record_selection_revoked(worker, scope)?;
    {
        let state = gate.0.lock().unwrap();
        assert_eq!((state.installed.len(), state.highwater.len()), (0, 0));
    }
    assert!(!gate.selection_revocation_pending(worker, scope)?);
    gate.record_selection_revoked(worker, scope)?;
    // A catalog ref never settles through the selection path.
    let (_, catalog_install) = install(&gate, worker, &catalog, namespace)?;
    assert!(
        gate.record_selection_revoked(worker, catalog_install.scope)
            .is_err()
    );
    assert!(
        gate.selection_revocation_pending(worker, catalog_install.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn owner_revocation_races_install_in_either_order_and_never_admits() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    // Revoke before install: the reservation is refused.
    let early = selection_ref(worker.service, 0x61);
    gate.register_selection(&early)?;
    assert!(gate.revoke_selection(&[early.grant_nonce])?.is_empty());
    assert!(
        gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, early),
            expiry()
        )
        .is_err()
    );
    // Install in flight, then revoke: the acknowledgement cannot settle it
    // as installed, and the reply is classified as a revoked selection.
    let late = selection_ref(worker.service, 0x62);
    gate.register_selection(&late)?;
    let reservation = gate.reserve_selection_scope(
        issue,
        worker,
        &selection_authority(&catalog, late),
        expiry(),
    )?;
    assert!(!gate.selection_install_revoked(&reservation));
    assert_eq!(
        gate.revoke_selection(&[late.grant_nonce])?,
        vec![(worker, reservation.scope)]
    );
    assert!(
        gate.acknowledge_install(reservation, worker, reservation.scope)
            .is_err()
    );
    assert!(gate.selection_install_revoked(&reservation));
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn selection_and_grant_scopes_never_alias_and_keep_separate_capacity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let (issue, catalog_install) = install(&gate, worker, &catalog, namespace)?;
    // 80 live selection entries on one worker (N6), one per owner.
    for nonce in 1..=80u8 {
        let selection = selection_ref(worker.service, nonce);
        gate.register_selection(&selection)?;
        let reservation = gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, selection),
            expiry(),
        )?;
        gate.acknowledge_install(reservation, worker, reservation.scope)?;
    }
    let overflow = selection_ref(worker.service, 0x90);
    gate.register_selection(&overflow)?;
    assert!(
        gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, overflow),
            expiry()
        )
        .is_err(),
        "the worker's live selection bound"
    );
    // The full selection ledger leaves the catalog grant admitted, and a
    // grant revocation fences only the catalog scope.
    gate.check_scope_issue(issue, worker, catalog_install.scope)?;
    let plan = gate.close_request_admission(&request(namespace, &catalog))?;
    assert_eq!(
        plan.targets,
        vec![FenceTarget {
            worker,
            scope: catalog_install.scope
        }]
    );
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(selection_live(&gate, 1));
    Ok(())
}

#[test]
fn index_fence_revokes_selection_locally_and_persists_only_catalog_targets() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let (_, catalog_install) = install(&gate, worker, &catalog, namespace)?;
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    let (_, selected) = install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    let catalog_target = FenceTarget {
        worker,
        scope: catalog_install.scope,
    };
    assert_eq!(plan.targets, vec![catalog_target.clone()]);
    assert_eq!(plan.durable.targets, vec![catalog_target]);
    assert!(!selection_live(&gate, 0x5c));
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, selected.scope)]
    );
    gate.record_revoked(&plan.ticket, worker, catalog_install.scope)?;
    // The local revocation holds the commit until the worker acknowledges.
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.record_selection_revoked(worker, selected.scope)?;
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert_eq!(gate.require_commit_ready(&plan.ticket)?.len(), 1);
    Ok(())
}

#[test]
fn selection_uncertain_renewal_current_ack_retains_the_previous_revocation() -> Result<()> {
    selection_uncertain_renewal_fence(false)
}

#[test]
fn selection_uncertain_renewal_previous_ack_retains_the_current_revocation() -> Result<()> {
    selection_uncertain_renewal_fence(true)
}

fn uncertain_selection_renewal(
    gate: &NativeCatalogAdmission,
    worker: WorkerIdentity,
    catalog: &ScopedReadAuthority,
    namespace: Incarnation,
) -> Result<(SelectionReadAuthority, InstallTicket, InstallTicket)> {
    let selection = selection_ref(worker.service, 0x5c);
    let authority = selection_authority(catalog, selection);
    gate.register_selection(&selection)?;
    let (issue, initial) = install_selection(gate, worker, &authority, namespace)?;
    gate.expire_install_for_test(initial)?;
    let renewal = gate.reserve_selection_scope(issue, worker, &authority, expiry())?;
    assert_eq!(renewal.previous(), Some(initial.scope));
    assert_ne!(
        renewal.scope.lease_generation,
        initial.scope.lease_generation
    );
    // No private installation ACK: either generation may still be at the worker.
    Ok((authority, initial, renewal))
}

fn selection_uncertain_renewal_fence(previous_first: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let (_, initial, renewal) = uncertain_selection_renewal(&gate, worker, &catalog, namespace)?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    assert!(plan.targets.is_empty());
    assert!(
        plan.durable.targets.is_empty(),
        "selection leases stay in memory"
    );
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, renewal.scope), (worker, initial.scope)]
    );
    let [first, second] = if previous_first {
        [initial.scope, renewal.scope]
    } else {
        [renewal.scope, initial.scope]
    };
    gate.record_selection_revoked(worker, first)?;
    assert!(!gate.selection_revocation_pending(worker, first)?);
    assert!(gate.selection_revocation_pending(worker, second)?);
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, second)]
    );
    if !previous_first {
        // A duplicate current ACK must neither fail nor settle its sibling.
        gate.record_selection_revoked(worker, first)?;
        assert!(gate.selection_revocation_pending(worker, second)?);
        assert!(gate.require_commit_ready(&plan.ticket).is_err());
    }
    gate.record_selection_revoked(worker, second)?;
    assert!(gate.require_commit_ready(&plan.ticket)?.is_empty());
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(!gate.selection_revocation_pending(worker, first)?);
    assert!(!gate.selection_revocation_pending(worker, second)?);
    let state = gate.0.lock().unwrap();
    assert!(state.installed.is_empty());
    assert!(state.highwater.is_empty());
    Ok(())
}

#[test]
fn selection_uncertain_renewal_owner_revocation_retains_both_exact_acks() -> Result<()> {
    for previous_first in [false, true] {
        let root = tempfile::tempdir()?;
        let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
        let (authority, initial, renewal) =
            uncertain_selection_renewal(&gate, worker, &catalog, namespace)?;
        assert_eq!(
            gate.revoke_selection(&[authority.selection.grant_nonce])?,
            vec![(worker, renewal.scope), (worker, initial.scope)]
        );
        let [first, second] = if previous_first {
            [initial.scope, renewal.scope]
        } else {
            [renewal.scope, initial.scope]
        };
        gate.record_selection_revoked(worker, first)?;
        if !previous_first {
            gate.record_selection_revoked(worker, first)?;
        }
        assert!(!gate.selection_revocation_pending(worker, first)?);
        assert!(gate.selection_revocation_pending(worker, second)?);
        assert_eq!(
            gate.revoke_selection(&[authority.selection.grant_nonce])?,
            vec![(worker, second)]
        );
        assert_eq!(gate.0.lock().unwrap().installed.len(), 1);
        gate.record_selection_revoked(worker, second)?;
        assert!(gate.0.lock().unwrap().installed.is_empty());
        assert!(gate.0.lock().unwrap().highwater.is_empty());
    }
    Ok(())
}

#[test]
fn worker_reap_settles_local_selection_revocations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    assert!(plan.targets.is_empty());
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(gate.require_commit_ready(&plan.ticket)?.is_empty());
    Ok(())
}

#[test]
fn refused_close_revokes_no_selection_scope() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    let (issue, selected) = install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    // Another namespace, then another expected revision: both refused
    // before any local revocation.
    let foreign = request(Incarnation::from_bytes([42; 16]), &catalog);
    assert!(
        gate.close_admission(
            FenceKey::new(&foreign)?,
            ScopeTarget::Index(catalog.index_id)
        )
        .is_err()
    );
    let mut stale = request(namespace, &catalog);
    stale.expected_catalog_revision = CatalogRevision::INITIAL.next()?;
    assert!(
        gate.close_admission(FenceKey::new(&stale)?, ScopeTarget::Index(catalog.index_id))
            .is_err()
    );
    assert!(selection_live(&gate, 0x5c));
    gate.check_scope_issue(issue, worker, selected.scope)?;
    Ok(())
}

#[test]
fn restored_pending_fence_holds_only_catalog_targets_and_no_local_revocation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    install(&gate, worker, &catalog, namespace)?;
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let request = request(namespace, &catalog);
    let plan = gate.close_request_admission(&request)?;
    assert!(
        plan.durable
            .targets
            .iter()
            .all(|target| target.scope.selection.is_none())
    );
    let (intent, _) = durable_fixture(&request, &plan.durable)?;
    let restored = NativeCatalogAdmission::new(
        Incarnation::from_bytes([21; 16]),
        namespace,
        CatalogRevision::INITIAL,
        1,
    )?;
    let restored_plan = restored.restore_pending_fence(&intent)?;
    assert_eq!(restored_plan.durable, plan.durable);
    assert!(
        restored
            .unsettled_selection_revocations(&restored_plan.ticket)?
            .is_empty()
    );
    let state = restored.0.lock().unwrap();
    assert!(state.live_selections.is_empty());
    assert!(
        state
            .pending
            .as_ref()
            .unwrap()
            .local_selection_revocations
            .is_empty()
    );
    Ok(())
}

fn agent_ref(service: Incarnation, nonce: u8) -> SelectionScopeRef {
    SelectionScopeRef {
        owner: SelectionOwnerKind::Agent,
        ..selection_ref(service, nonce)
    }
}
/// U105: the open authority of the catalog fixture's whole index.
fn open_authority(
    authority: &ScopedReadAuthority,
    selection: SelectionScopeRef,
) -> OpenReadAuthority {
    OpenReadAuthority {
        schema: 1,
        backing: authority.backing,
        index_id: authority.index_id,
        selection,
        sources: authority.sources.clone(),
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    }
}

/// U105: agent nonces have their own budget; a full agent budget leaves
/// window/admin registration open and the reverse.
#[test]
fn agent_nonces_have_their_own_budget() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, _, _) = fixture(root.path().to_str().unwrap());
    for nonce in 0..MAX_AGENT_SELECTIONS as u8 {
        gate.register_selection(&agent_ref(worker.service, nonce))?;
    }
    assert!(
        gate.register_selection(&agent_ref(worker.service, 0xe5))
            .is_err(),
        "the agent budget is full"
    );
    for nonce in 0x40..0x40 + 160u8 {
        gate.register_selection(&selection_ref(worker.service, nonce))?;
    }
    assert!(
        gate.register_selection(&selection_ref(worker.service, 0xf0))
            .is_err()
    );
    // Revoking (pruning) one agent nonce frees exactly one agent slot.
    assert!(
        gate.revoke_selection(&[Incarnation::from_bytes([3; 16])])?
            .is_empty()
    );
    gate.register_selection(&agent_ref(worker.service, 0xe5))?;
    assert!(
        gate.register_selection(&agent_ref(worker.service, 0xe6))
            .is_err()
    );
    Ok(())
}

/// U105: an open read installs only under a live agent nonce, never under a
/// window or admin owner, and revokes like any selection.
#[test]
fn open_scope_requires_a_live_agent_nonce_and_revokes_like_a_selection() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let selection = agent_ref(worker.service, 0x71);
    let authority = open_authority(&catalog, selection);
    assert!(
        gate.reserve_open_scope(issue, worker, &authority, expiry())
            .is_err(),
        "unregistered nonce"
    );
    // A window nonce of the same bytes never admits an agent authority.
    gate.register_selection(&selection_ref(worker.service, 0x71))?;
    assert!(
        gate.reserve_open_scope(issue, worker, &authority, expiry())
            .is_err()
    );
    assert!(gate.revoke_selection(&[selection.grant_nonce])?.is_empty());
    gate.register_selection(&selection)?;
    let reservation = gate.reserve_open_scope(issue, worker, &authority, expiry())?;
    assert!(reservation.requires_install);
    assert_eq!(reservation.scope.selection, Some(selection));
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    gate.check_scope_issue(issue, worker, reservation.scope)?;
    let mut window = authority.clone();
    window.selection.owner = SelectionOwnerKind::Window;
    assert!(
        gate.reserve_open_scope(issue, worker, &window, expiry())
            .is_err()
    );
    assert_eq!(
        gate.revoke_selection(&[selection.grant_nonce])?,
        vec![(worker, reservation.scope)]
    );
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    gate.record_selection_revoked(worker, reservation.scope)?;
    assert_eq!(gate.0.lock().unwrap().installed.len(), 0);
    Ok(())
}

/// U105: agent installations are counted apart from W4/A2 selections.
#[test]
fn agent_installations_do_not_consume_window_selection_capacity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    for nonce in 0..MAX_AGENT_SELECTIONS as u8 {
        let selection = agent_ref(worker.service, nonce);
        gate.register_selection(&selection)?;
        let reservation = gate.reserve_open_scope(
            issue,
            worker,
            &open_authority(&catalog, selection),
            expiry(),
        )?;
        gate.acknowledge_install(reservation, worker, reservation.scope)?;
    }
    for nonce in 0x40..0x40 + 80u8 {
        let selection = selection_ref(worker.service, nonce);
        gate.register_selection(&selection)?;
        let reservation = gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, selection),
            expiry(),
        )?;
        gate.acknowledge_install(reservation, worker, reservation.scope)?;
    }
    let overflow = selection_ref(worker.service, 0xf0);
    gate.register_selection(&overflow)?;
    assert!(
        gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, overflow),
            expiry()
        )
        .is_err(),
        "the window bound is unchanged by agent entries"
    );
    Ok(())
}
