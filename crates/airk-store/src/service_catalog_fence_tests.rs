// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::{ScopedReadRef, WorkerIdentity};

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, CatalogRequest)> {
    let root = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let old = super::super::super::schema_digest(&registry.connection)?;
    super::super::super::install_schema(&registry.connection)?;
    registry.connection.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,1,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            old.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    registry
        .connection
        .pragma_update(None, "user_version", 12)?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([4; 16]));
    registry.connection.execute(
        "INSERT INTO query_projects VALUES(?1,'Reviewed',1,1,1)",
        [project.to_string()],
    )?;
    let (namespace, epoch): (String, i64) = registry.connection.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let request = CatalogRequest {
        caller: Incarnation::from_bytes([5; 16]),
        key: RequestKey {
            namespace: namespace.parse()?,
            epoch: u64::try_from(epoch)?,
            nonce: Incarnation::from_bytes([6; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::DeleteProject {
            id: project,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
    };
    Ok((root, registry, request))
}
fn plan(count: usize) -> Result<FencePlan> {
    let service = Incarnation::from_bytes([7; 16]);
    FencePlan::new(
        service,
        1,
        (0..count)
            .map(|n| {
                let id = Incarnation::from_bytes((n as u128 + 100).to_be_bytes());
                FenceTarget {
                    worker: WorkerIdentity {
                        service,
                        worker: id,
                        project: Sha256Digest::from_bytes([8; 32]),
                        fencing_generation: 1,
                    },
                    scope: ScopedReadRef {
                        schema: 1,
                        lease_generation: 1,
                        index_id: IndexId::from_identity(id),
                        grant_id: GrantId::from_identity(id),
                        grant_revision: CatalogRevision::INITIAL,
                        fingerprint: Sha256Digest::from_bytes([9; 32]),
                    },
                }
            })
            .collect(),
    )
}
fn prepare(
    registry: &mut ServiceRegistry,
    request: &CatalogRequest,
    plan: &FencePlan,
) -> Result<(FenceIntent, Sha256Digest)> {
    let digest = registry.catalog_preview(request)?.identity()?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(request, digest, plan)?
    else {
        anyhow::bail!("expected fresh intent")
    };
    Ok((intent, digest))
}
fn receipt_count(registry: &ServiceRegistry) -> Result<i64> {
    Ok(registry.connection.query_row(
        "SELECT count(*) FROM index_operation_receipts",
        [],
        |row| row.get(0),
    )?)
}
fn confirm(targets: &[FenceTarget]) -> Vec<FenceConfirmation> {
    targets
        .iter()
        .map(|target| FenceConfirmation {
            target: target.clone(),
            method: FenceConfirmationMethod::NativeScopeAck,
        })
        .collect()
}

#[test]
fn native_fence_blocks_mutations_until_exact_ack_and_finishes_after_epoch_rotation() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let plan = plan(1)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    assert_eq!(receipt_count(&registry)?, 1);
    assert!(
        registry
            .catalog_apply_fenced(&request, preview, intent.prepared_digest)
            .is_err()
    );
    let mut unrelated = request.clone();
    unrelated.key.nonce = Incarnation::from_bytes([11; 16]);
    unrelated.operation = CatalogOperation::CreateProject {
        id: QueryProjectId::from_identity(Incarnation::from_bytes([12; 16])),
        name: "Unrelated".into(),
    };
    let other_preview = registry.catalog_preview(&unrelated)?.identity()?;
    assert!(registry.catalog_apply(&unrelated, other_preview).is_err());
    let mut foreign = plan.targets.clone();
    foreign[0].worker.fencing_generation += 1;
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &confirm(&foreign)
            )
            .is_err()
    );
    assert!(
        registry.catalog_apply(&request, preview).is_err(),
        "intent must not decode as successful metadata"
    );
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets),
    )?;
    registry
        .connection
        .execute("UPDATE service_meta SET request_epoch=request_epoch+1", [])?;
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    receipt.validate_for(&request, preview, intent.prepared_digest, &plan)?;
    assert_eq!(
        receipt_count(&registry)?,
        1,
        "terminal result replaces reserved slot"
    );
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?,
        receipt
    );
    let newer_service_plan = FencePlan::new(Incarnation::from_bytes([13; 16]), 2, Vec::new())?;
    assert_eq!(
        registry.catalog_prepare_fence(&request, preview, &newer_service_plan)?,
        FencePreparation::Replay(receipt.clone())
    );
    assert_eq!(
        registry.catalog_receipt(request.caller, &request.key)?,
        Some(receipt.metadata)
    );
    let mut conflicting = request.clone();
    conflicting.operation = unrelated.operation;
    assert!(
        registry
            .catalog_apply_fenced(&conflicting, preview, intent.prepared_digest)
            .is_err()
    );
    Ok(())
}

#[test]
fn partial_acknowledgements_survive_reopen_and_never_regress_or_adopt_other_intent() -> Result<()> {
    let (root, mut registry, request) = fixture()?;
    let plan = plan(2)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let partial = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets[..1]),
    )?;
    assert_eq!(partial.stage, FenceStage::FenceUnknown);
    drop(registry);
    let mut registry = ServiceRegistry::open_saved(root.path())?;
    assert_eq!(registry.catalog_pending_fence()?, Some(partial.clone()));
    assert_eq!(
        registry.catalog_fence_observation(request.caller, &request.key)?,
        Some(FenceObservation::Pending(partial.clone()))
    );
    let read_only_failure = registry
        .catalog_record_fence_confirmations(
            request.caller,
            &request.key,
            intent.prepared_digest,
            &confirm(&plan.targets),
        )
        .unwrap_err();
    assert!(
        matches!(read_only_failure.downcast_ref::<rusqlite::Error>(),
        Some(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::ReadOnly),
        "saved observations must refuse writes with SQLITE_READONLY: {read_only_failure:#}"
    );
    assert_eq!(registry.catalog_pending_fence()?, Some(partial.clone()));
    drop(registry);
    let mut registry = ServiceRegistry::open(root.path(), request.key.namespace)?;
    // Prove this handle can perform a valid idempotent mutation before checking
    // negative transitions; readonly failures must not satisfy those assertions.
    assert_eq!(
        registry.catalog_record_fence_confirmations(
            request.caller,
            &request.key,
            intent.prepared_digest,
            &partial.confirmations,
        )?,
        partial
    );
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &[]
            )
            .is_err()
    );
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                Sha256Digest::from_bytes([88; 32]),
                &confirm(&plan.targets)
            )
            .is_err()
    );
    let mut other = request.clone();
    other.key.nonce = Incarnation::from_bytes([14; 16]);
    assert!(
        registry
            .catalog_prepare_fence(&other, preview, &plan)
            .is_err()
    );
    assert_eq!(registry.catalog_pending_fence()?, Some(partial));
    let done = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets),
    )?;
    assert_eq!(done.stage, FenceStage::Fenced);
    assert_eq!(
        registry.catalog_record_fence_confirmations(
            request.caller,
            &request.key,
            intent.prepared_digest,
            &confirm(&plan.targets)
        )?,
        done
    );
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &confirm(&plan.targets)
            )
            .is_err(),
        "terminal receipt cannot return to a pending stage"
    );
    Ok(())
}

#[test]
fn full_receipt_capacity_reuses_reserved_slot_without_eviction() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    for n in 1u128..=1023 {
        registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'fixture','metadata_applied',1,X'7b7d')",[Incarnation::from_bytes(n.to_be_bytes()).to_string()])?;
    }
    let plan = plan(0)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    assert_eq!(receipt_count(&registry)?, 1024);
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(receipt_count(&registry)?, 1024);
    Ok(())
}

#[test]
fn largest_acknowledgement_payload_is_reserved_before_effects() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    // A plan fits alone, but duplicating its exact acknowledgement set plus the
    // reviewed request cannot fit the durable 16 KiB intent.
    let plan = plan(24)?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let rejected = registry
        .catalog_prepare_fence(&request, preview, &plan)
        .unwrap_err();
    assert_eq!(
        rejected.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::Capacity)
    );
    assert_eq!(receipt_count(&registry)?, 0);
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        state(&registry.connection)?.revision,
        CatalogRevision::INITIAL
    );
    Ok(())
}

#[test]
fn legacy_metadata_receipt_cannot_be_upgraded_to_verified_fence() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let old = registry.catalog_apply(&request, preview)?;
    assert!(old.authority_fence_required);
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &plan(0)?)
            .is_err()
    );
    assert!(
        registry
            .catalog_fence_observation(request.caller, &request.key)?
            .is_none()
    );
    assert_eq!(
        registry.catalog_receipt(request.caller, &request.key)?,
        Some(old)
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}

#[test]
fn duplicate_and_foreign_target_plans_fail_before_any_receipt() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let plan = plan(1)?;
    assert!(
        FencePlan::new(
            plan.service,
            1,
            vec![plan.targets[0].clone(), plan.targets[0].clone()]
        )
        .is_err()
    );
    let mut foreign = plan.clone();
    foreign.targets[0].worker.service = Incarnation::from_bytes([99; 16]);
    let preview = registry.catalog_preview(&request)?.identity()?;
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &foreign)
            .is_err()
    );
    assert_eq!(receipt_count(&registry)?, 0);
    Ok(())
}

#[test]
fn saved_intent_stage_and_namespace_cannot_be_changed_to_bypass_review() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let plan = plan(0)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    registry.connection.execute(
        "UPDATE index_operation_receipts SET stage='fenced' WHERE id=?1",
        [intent.operation_id.to_string()],
    )?;
    assert!(
        registry
            .catalog_fence_observation(request.caller, &request.key)
            .is_err()
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET stage='prepared' WHERE id=?1",
        [intent.operation_id.to_string()],
    )?;
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.connection.execute(
        "UPDATE service_meta SET incarnation=?1",
        [Incarnation::from_bytes([100; 16]).to_string()],
    )?;
    assert!(
        registry
            .catalog_apply_fenced(&request, preview, intent.prepared_digest)
            .is_err(),
        "accepted old epoch does not permit a different registry namespace"
    );
    assert_eq!(
        state(&registry.connection)?.revision,
        CatalogRevision::INITIAL
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}

#[test]
fn parallel_subset_fingerprints_and_exact_lease_generations_each_require_acknowledgement()
-> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let original = plan(1)?;
    let mut first = original.targets[0].clone();
    first.scope.lease_generation = 2;
    let mut second = first.clone();
    second.scope.fingerprint = Sha256Digest::from_bytes([10; 32]);
    second.scope.lease_generation = 3;
    let plan = FencePlan::new(original.service, 1, vec![first.clone(), second.clone()])?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let mut old_lease = first.clone();
    old_lease.scope.lease_generation = 1;
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &confirm(&[old_lease])
            )
            .is_err()
    );
    let partial = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets[..1]),
    )?;
    assert_eq!(partial.stage, FenceStage::FenceUnknown);
    assert!(
        registry
            .catalog_apply_fenced(&request, preview, intent.prepared_digest)
            .is_err(),
        "another fingerprint remains live until its own exact acknowledgement"
    );
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets),
    )?;
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(receipt.proof.confirmations, confirm(&plan.targets));
    receipt.validate_for(&request, preview, intent.prepared_digest, &plan)?;
    let mut newer_lease = first.clone();
    newer_lease.scope.lease_generation = 4;
    let both_leases = FencePlan::new(original.service, 2, vec![first, newer_lease])?;
    assert_eq!(
        both_leases.targets.len(),
        2,
        "different exact leases of the same fingerprint cannot collapse"
    );
    assert!(FencePlan::new(original.service, 2, vec![second.clone(), second]).is_err());
    Ok(())
}

#[test]
fn actual_confirmation_methods_survive_partial_progress_and_never_rewrite_first_observation()
-> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let plan = plan(2)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let partial = confirm(&plan.targets[..1]);
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &partial,
    )?;
    let mut duplicate = partial.clone();
    let mut alternate = partial[0].clone();
    alternate.method = FenceConfirmationMethod::NativeWorkerReap;
    duplicate.push(alternate.clone());
    assert!(
        registry
            .catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &duplicate
            )
            .is_err(),
        "two methods cannot count as two captured targets"
    );
    let repeated = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[alternate],
    )?;
    assert_eq!(
        repeated.confirmations, partial,
        "first actual method survives later evidence of worker reaping"
    );
    let mut incoming = confirm(&plan.targets);
    for confirmation in &mut incoming {
        confirmation.method = FenceConfirmationMethod::NativeWorkerReap;
    }
    let completed = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &incoming,
    )?;
    assert_eq!(completed.stage, FenceStage::Fenced);
    assert_eq!(
        completed.confirmations[0].method,
        FenceConfirmationMethod::NativeScopeAck
    );
    assert_eq!(
        completed.confirmations[1].method,
        FenceConfirmationMethod::NativeWorkerReap
    );
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(receipt.proof.confirmations, completed.confirmations);
    let encoded = serde_json::to_value(&receipt)?;
    assert_eq!(
        encoded["proof"]["confirmations"][0]["method"],
        "native_scope_ack"
    );
    assert_eq!(
        encoded["proof"]["confirmations"][1]["method"],
        "native_worker_reap"
    );
    assert_eq!(
        registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?,
        receipt
    );
    Ok(())
}

#[test]
fn only_known_preparation_checks_classify_capacity_and_stale_review() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let plan = plan(1)?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let stale = registry
        .catalog_prepare_fence(&request, Sha256Digest::from_bytes([0; 32]), &plan)
        .unwrap_err();
    assert_eq!(
        stale.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::StaleReview)
    );
    assert_eq!(receipt_count(&registry)?, 0);
    let mut invalid = plan.clone();
    invalid.targets[0].scope.lease_generation = 0;
    let unknown = registry
        .catalog_prepare_fence(&request, preview, &invalid)
        .unwrap_err();
    assert!(
        unknown.downcast_ref::<FencePrepareRejected>().is_none(),
        "other validation errors cannot authorize reopening"
    );
    registry
        .connection
        .execute("UPDATE index_catalog_meta SET revision=2 WHERE id=1", [])?;
    let stale_revision = registry
        .catalog_prepare_fence(&request, preview, &plan)
        .unwrap_err();
    assert_eq!(
        stale_revision.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::StaleReview)
    );
    assert_eq!(receipt_count(&registry)?, 0);
    Ok(())
}

#[test]
fn native_exact_absence_can_observe_an_expired_epoch_without_admitting_a_write() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    registry
        .connection
        .execute("UPDATE service_meta SET request_epoch=request_epoch+1", [])?;
    let proof = registry.catalog_fence_absence(&request)?;
    proof.validate_for(&request)?;
    assert_eq!(proof.catalog_state().namespace, request.key.namespace);
    assert_eq!(
        proof.catalog_state().revision,
        request.expected_catalog_revision
    );
    let mut other = request.clone();
    other.key.nonce = Incarnation::from_bytes([90; 16]);
    assert!(proof.validate_for(&other).is_err());
    assert!(
        registry
            .catalog_prepare_fence(&request, Sha256Digest::from_bytes([0; 32]), &plan(0)?)
            .is_err(),
        "absence read does not make an expired key admissible"
    );
    assert_eq!(receipt_count(&registry)?, 0);
    registry.connection.execute(
        "UPDATE service_meta SET request_epoch=?1",
        [i64::try_from(request.key.epoch)?],
    )?;
    let (intent, preview) = prepare(&mut registry, &request, &plan(0)?)?;
    assert!(registry.catalog_fence_absence(&request).is_err());
    assert!(
        registry.catalog_fence_absence(&other).is_err(),
        "another pending mutation prevents safe reopening"
    );
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    let mut same_key_current_revision = request.clone();
    same_key_current_revision.expected_catalog_revision = state(&registry.connection)?.revision;
    assert!(
        registry
            .catalog_fence_absence(&same_key_current_revision)
            .is_err(),
        "a terminal receipt is never absence"
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}
