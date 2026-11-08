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

fn retirement_fixture(
    workers: usize,
) -> Result<(
    tempfile::TempDir,
    tempfile::TempDir,
    ServiceRegistry,
    CatalogRequest,
    FencePlan,
)> {
    let (root, mut registry, mut request) = fixture()?;
    let source_owner = tempfile::tempdir()?;
    let base = source_owner.path().canonicalize()?;
    std::fs::create_dir(base.join("source"))?;
    let registration = ProjectRegistration::new(
        base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "retire-fence".into(),
    )?;
    registry.register(&registration)?;
    let index = IndexId::from_identity(Incarnation::from_bytes([70; 16]));
    registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Retire fixture',1,1,0,NULL,0,'pending_metadata')",params![index.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
    let tx = registry.connection.transaction()?;
    super::super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    request.operation = CatalogOperation::RetireIndex {
        id: index,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let service = Incarnation::from_bytes([71; 16]);
    let workers: Vec<_> = (0..workers)
        .map(|n| WorkerIdentity {
            service,
            worker: Incarnation::from_bytes((n as u128 + 200).to_be_bytes()),
            project: registration.binding,
            fencing_generation: 1,
        })
        .collect();
    let plan =
        FencePlan::new(service, 1, vec![])?.with_worker_obligation(WorkerDrainObligation {
            index_id: index,
            backing: registration.binding,
            // This Store component fixture supplies a native-known empty
            // provider history; it does not infer that history from no rows.
            providers: workers
                .iter()
                .map(|worker| WorkerProviderObligation {
                    worker: *worker,
                    history: WorkerProviderHistory::NativeNeverAuthorized,
                })
                .collect(),
            workers,
        })?;
    Ok((root, source_owner, registry, request, plan))
}

#[test]
fn retirement_needs_actual_worker_reaps_and_preserves_full_proof_across_reopen() -> Result<()> {
    let (root, _source_owner, mut registry, request, mut plan) = retirement_fixture(2)?;
    plan.worker_obligation.as_mut().unwrap().providers[0].history =
        WorkerProviderHistory::Helpers {
            generations: vec![
                Incarnation::from_bytes([80; 16]),
                Incarnation::from_bytes([81; 16]),
            ],
        };
    plan.validate()?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let unknown = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    assert_eq!(unknown.stage, FenceStage::FenceUnknown);
    assert!(
        registry
            .catalog_apply_fenced(&request, preview, intent.prepared_digest)
            .is_err()
    );
    let workers = &plan.worker_obligation.as_ref().unwrap().workers;
    let confirmations: Vec<_> = workers
        .iter()
        .map(|worker| WorkerFenceConfirmation {
            worker: *worker,
            method: WorkerFenceConfirmationMethod::NativeWorkerReap,
        })
        .collect();
    let partial = registry.catalog_record_worker_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirmations[..1],
    )?;
    assert_eq!(partial.stage, FenceStage::FenceUnknown);
    drop(registry);
    let mut registry = ServiceRegistry::open(root.path(), request.key.namespace)?;
    assert_eq!(registry.catalog_pending_fence()?, Some(partial.clone()));
    assert_eq!(
        registry.catalog_record_worker_fence_confirmations(
            request.caller,
            &request.key,
            intent.prepared_digest,
            &confirmations[..1]
        )?,
        partial
    );
    assert!(
        registry
            .catalog_record_worker_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &[]
            )
            .is_err()
    );
    let mut foreign = confirmations.clone();
    foreign[1].worker.fencing_generation += 1;
    assert!(
        registry
            .catalog_record_worker_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &foreign
            )
            .is_err()
    );
    assert!(
        registry
            .catalog_record_worker_fence_confirmations(
                request.caller,
                &request.key,
                Sha256Digest::from_bytes([0; 32]),
                &confirmations
            )
            .is_err()
    );
    let complete = registry.catalog_record_worker_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirmations,
    )?;
    assert_eq!(complete.stage, FenceStage::Fenced);
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(receipt.proof.worker_obligation, plan.worker_obligation);
    assert_eq!(receipt.proof.worker_confirmations, confirmations);
    assert_eq!(
        registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?,
        receipt
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}

#[test]
fn retirement_rejects_missing_or_foreign_backing_census_before_reservation() -> Result<()> {
    let (_root, _source_owner, mut registry, request, plan) = retirement_fixture(0)?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let no_census = FencePlan::new(plan.service, 1, vec![])?;
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &no_census)
            .is_err()
    );
    let mut foreign = plan.clone();
    foreign.worker_obligation.as_mut().unwrap().backing = Sha256Digest::from_bytes([72; 32]);
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &foreign)
            .is_err()
    );
    assert_eq!(receipt_count(&registry)?, 0);
    let (intent, _) = prepare(&mut registry, &request, &plan)?;
    let complete = registry.catalog_record_worker_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    assert_eq!(
        complete.stage,
        FenceStage::Fenced,
        "an explicitly observed empty worker census is complete"
    );
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    Ok(())
}

#[test]
fn worker_reap_and_scope_confirmation_are_independent_retirement_obligations() -> Result<()> {
    let (_root, _source_owner, mut registry, request, mut plan) = retirement_fixture(1)?;
    let obligation = plan.worker_obligation.as_ref().unwrap().clone();
    plan.targets.push(FenceTarget {
        worker: obligation.workers[0],
        scope: ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: obligation.index_id,
            grant_id: GrantId::from_identity(Incarnation::from_bytes([73; 16])),
            grant_revision: CatalogRevision::INITIAL,
            fingerprint: Sha256Digest::from_bytes([74; 32]),
        },
    });
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let worker = WorkerFenceConfirmation {
        worker: obligation.workers[0],
        method: WorkerFenceConfirmationMethod::NativeWorkerReap,
    };
    let worker_only = registry.catalog_record_worker_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        std::slice::from_ref(&worker),
    )?;
    assert_eq!(worker_only.stage, FenceStage::FenceUnknown);
    assert!(
        registry
            .catalog_apply_fenced(&request, preview, intent.prepared_digest)
            .is_err()
    );
    let complete = registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &confirm(&plan.targets),
    )?;
    assert_eq!(complete.stage, FenceStage::Fenced);
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(receipt.proof.worker_confirmations, vec![worker]);
    assert_eq!(receipt.proof.confirmations, confirm(&plan.targets));
    Ok(())
}

#[test]
fn retirement_reserves_completed_worker_proof_budget_before_native_effects() -> Result<()> {
    let (_root, _source_owner, mut registry, request, plan) = retirement_fixture(24)?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let rejected = registry
        .catalog_prepare_fence(&request, preview, &plan)
        .unwrap_err();
    assert_eq!(
        rejected.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::Capacity)
    );
    assert_eq!(receipt_count(&registry)?, 0);
    let registered: bool =
        registry
            .connection
            .query_row("SELECT registered FROM projects", [], |r| r.get(0))?;
    assert!(registered);
    assert!(registry.catalog_pending_fence()?.is_none());
    Ok(())
}

#[test]
fn retirement_requires_exact_native_provider_history_before_reservation() -> Result<()> {
    let (_root, _source_owner, mut registry, request, plan) = retirement_fixture(2)?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let mut missing = plan.clone();
    missing.worker_obligation.as_mut().unwrap().providers.pop();
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &missing)
            .is_err()
    );
    let mut foreign = plan.clone();
    foreign.worker_obligation.as_mut().unwrap().providers[0]
        .worker
        .fencing_generation += 1;
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &foreign)
            .is_err()
    );
    for generations in [
        vec![],
        vec![Incarnation::from_bytes([80; 16]); 2],
        (0u128..33)
            .map(|n| Incarnation::from_bytes(n.to_be_bytes()))
            .collect(),
    ] {
        let mut invalid = plan.clone();
        invalid.worker_obligation.as_mut().unwrap().providers[0].history =
            WorkerProviderHistory::Helpers { generations };
        assert!(
            registry
                .catalog_prepare_fence(&request, preview, &invalid)
                .is_err()
        );
    }
    assert_eq!(receipt_count(&registry)?, 0);
    assert!(registry.catalog_pending_fence()?.is_none());
    Ok(())
}

#[test]
fn retirement_provider_history_cannot_change_on_replay_or_saved_payload() -> Result<()> {
    let (_root, _source_owner, mut registry, request, mut plan) = retirement_fixture(1)?;
    plan.worker_obligation.as_mut().unwrap().providers[0].history =
        WorkerProviderHistory::Helpers {
            generations: vec![Incarnation::from_bytes([80; 16])],
        };
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    let mut changed = plan.clone();
    changed.worker_obligation.as_mut().unwrap().providers[0].history =
        WorkerProviderHistory::Helpers {
            generations: vec![Incarnation::from_bytes([81; 16])],
        };
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &changed)
            .is_err()
    );
    assert_eq!(registry.catalog_pending_fence()?, Some(intent.clone()));
    let mut tampered = intent.clone();
    tampered.plan = changed;
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&tampered)?,
            intent.operation_id.to_string()
        ],
    )?;
    assert!(
        registry.catalog_pending_fence().is_err(),
        "saved provider history is part of the prepared digest"
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&intent)?,
            intent.operation_id.to_string()
        ],
    )?;
    assert_eq!(registry.catalog_pending_fence()?, Some(intent));
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}
#[test]
fn inprocess_provider_history_is_canonical_and_cannot_replay_as_helper() -> Result<()> {
    let (root, _source_owner, mut registry, request, mut plan) = retirement_fixture(1)?;
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        12
    );
    let generations = vec![Incarnation::from_bytes([80; 16])];
    plan.worker_obligation.as_mut().unwrap().providers[0].history =
        WorkerProviderHistory::InProcess {
            generations: generations.clone(),
            completed_authorizations: None,
        };
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        13
    );
    let required = crate::service::registry::runtime_contract::required_capabilities(root.path())?;
    let mut predecessor = crate::service::registry::catalog::capabilities();
    predecessor.maximum_registry_schema = 12;
    assert!(required.require_supported(&predecessor).is_err());
    required.require_supported(&crate::service::registry::catalog::capabilities())?;
    let saved = ServiceRegistry::open_saved(root.path())?;
    assert_eq!(saved.catalog_pending_fence()?, Some(intent.clone()));
    let mut changed = plan.clone();
    changed.worker_obligation.as_mut().unwrap().providers[0].history =
        WorkerProviderHistory::Helpers { generations };
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &changed)
            .is_err()
    );
    assert_eq!(registry.catalog_pending_fence()?, Some(intent));
    for generations in [Vec::new(), vec![Incarnation::from_bytes([81; 16]); 2]] {
        let mut invalid = plan.clone();
        invalid.worker_obligation.as_mut().unwrap().providers[0].history =
            WorkerProviderHistory::InProcess {
                generations,
                completed_authorizations: None,
            };
        assert!(invalid.validate().is_err());
    }
    Ok(())
}

#[test]
fn retirement_helper_generations_reserve_full_terminal_budget_before_effects() -> Result<()> {
    let (_root, _source_owner, mut registry, request, mut plan) = retirement_fixture(9)?;
    for (worker, provider) in plan
        .worker_obligation
        .as_mut()
        .unwrap()
        .providers
        .iter_mut()
        .enumerate()
    {
        provider.history = WorkerProviderHistory::Helpers {
            generations: (0u128..32)
                .map(|generation| {
                    Incarnation::from_bytes((1000 + worker as u128 * 32 + generation).to_be_bytes())
                })
                .collect(),
        };
    }
    plan.validate()?;
    let preview = registry.catalog_preview(&request)?.identity()?;
    let failure = registry
        .catalog_prepare_fence(&request, preview, &plan)
        .unwrap_err();
    assert_eq!(
        failure.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::Capacity)
    );
    assert_eq!(receipt_count(&registry)?, 0);
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        state(&registry.connection)?.revision,
        request.expected_catalog_revision
    );
    let registered: bool =
        registry
            .connection
            .query_row("SELECT registered FROM projects", [], |row| row.get(0))?;
    assert!(registered, "capacity refusal precedes retirement effects");
    Ok(())
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
    assert_eq!(
        registry.catalog_saved_observation(request.caller, &request.key)?,
        Some(CatalogSavedObservation::Metadata(old.clone())),
        "a scalar receipt requiring a fence remains metadata-only diagnostic evidence"
    );
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

#[test]
fn native_saved_inspection_preserves_old_epoch_pending_and_terminal_records() -> Result<()> {
    let (_root, mut registry, request) = fixture()?;
    let (intent, preview) = prepare(&mut registry, &request, &plan(0)?)?;
    registry
        .connection
        .execute("UPDATE service_meta SET request_epoch=request_epoch+1", [])?;
    assert_eq!(
        registry.catalog_saved_observation(request.caller, &request.key)?,
        Some(CatalogSavedObservation::Fence(Box::new(
            FenceObservation::Pending(intent.clone())
        )))
    );
    let mut other = request.key.clone();
    other.nonce = Incarnation::from_bytes([96; 16]);
    assert!(
        registry
            .catalog_saved_observation(request.caller, &other)?
            .is_none()
    );
    assert!(
        registry
            .catalog_saved_observation(Incarnation::from_bytes([97; 16]), &request.key)?
            .is_none()
    );
    assert!(
        registry
            .catalog_fence_observation(request.caller, &other)
            .is_err(),
        "public absent old-epoch admission remains unchanged"
    );
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert_eq!(
        registry.catalog_saved_observation(request.caller, &request.key)?,
        Some(CatalogSavedObservation::Fence(Box::new(
            FenceObservation::Applied(receipt)
        )))
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}

#[test]
fn native_saved_inspection_keeps_metadata_receipts_diagnostic_and_refuses_other_or_corrupt_kinds()
-> Result<()> {
    let (_root, mut registry, mut request) = fixture()?;
    request.operation = CatalogOperation::CreateProject {
        id: QueryProjectId::from_identity(Incarnation::from_bytes([98; 16])),
        name: "Ordinary diagnostic receipt".into(),
    };
    let preview = registry.catalog_preview(&request)?.identity()?;
    let receipt = registry.catalog_apply(&request, preview)?;
    registry
        .connection
        .execute("UPDATE service_meta SET request_epoch=request_epoch+1", [])?;
    assert_eq!(
        registry.catalog_saved_observation(request.caller, &request.key)?,
        Some(CatalogSavedObservation::Metadata(receipt.clone()))
    );
    let mut foreign = request.key.clone();
    foreign.namespace = Incarnation::from_bytes([99; 16]);
    assert!(
        registry
            .catalog_saved_observation(request.caller, &foreign)
            .is_err()
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET kind='other_native_lane' WHERE id=?1",
        [receipt.operation_id.to_string()],
    )?;
    assert!(
        registry
            .catalog_saved_observation(request.caller, &request.key)
            .is_err(),
        "another lane is a conflict, never absence"
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET kind='catalog_management',payload=X'7b7d' WHERE id=?1",
        [receipt.operation_id.to_string()],
    )?;
    assert!(
        registry
            .catalog_saved_observation(request.caller, &request.key)
            .is_err(),
        "corrupt native receipt cannot become absence"
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}

#[test]
fn unprepared_absence_binds_original_request_after_independent_clock_and_epoch_advance()
-> Result<()> {
    let (_root, _sources, mut registry, request, plan) = retirement_fixture(0)?;
    let mut independent = request.clone();
    independent.key.nonce = Incarnation::from_bytes([90; 16]);
    independent.operation = CatalogOperation::CreateProject {
        id: QueryProjectId::from_identity(Incarnation::from_bytes([91; 16])),
        name: "Independent current clock".into(),
    };
    let preview = registry.catalog_preview(&independent)?.identity()?;
    let receipt = registry.catalog_apply(&independent, preview)?;
    registry
        .connection
        .execute("UPDATE service_meta SET request_epoch=request_epoch+1", [])?;
    let proof = registry.catalog_fence_unprepared_absence(&request)?;
    proof.validate_for(&request)?;
    assert_eq!(proof.catalog_state().revision, receipt.catalog_revision);
    assert!(
        registry.catalog_fence_absence(&request).is_err(),
        "the original exact-clock proof remains strict"
    );
    for changed in [
        CatalogRequest {
            caller: Incarnation::from_bytes([92; 16]),
            ..request.clone()
        },
        CatalogRequest {
            key: RequestKey {
                nonce: Incarnation::from_bytes([93; 16]),
                ..request.key.clone()
            },
            ..request.clone()
        },
        CatalogRequest {
            key: RequestKey {
                epoch: request.key.epoch + 1,
                ..request.key.clone()
            },
            ..request.clone()
        },
        CatalogRequest {
            expected_catalog_revision: receipt.catalog_revision,
            ..request.clone()
        },
        CatalogRequest {
            operation: independent.operation.clone(),
            ..request.clone()
        },
    ] {
        assert!(
            proof.validate_for(&changed).is_err(),
            "absence is bound to every original request byte"
        );
    }
    assert!(
        registry
            .catalog_fence_unprepared_absence(&independent)
            .is_err(),
        "metadata-only receipt of any kind is not absence"
    );
    assert!(
        registry
            .catalog_prepare_fence(&request, Sha256Digest::from_bytes([0; 32]), &plan)
            .is_err(),
        "read-only absence does not admit the expired/stale request"
    );
    assert_eq!(receipt_count(&registry)?, 1);
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        state(&registry.connection)?.revision,
        receipt.catalog_revision
    );
    Ok(())
}

#[test]
fn unprepared_absence_rejects_namespace_future_clock_pending_and_terminal_receipts() -> Result<()> {
    let (_root, _sources, mut registry, request, plan) = retirement_fixture(0)?;
    let mut foreign = request.clone();
    foreign.key.namespace = Incarnation::from_bytes([94; 16]);
    assert!(registry.catalog_fence_unprepared_absence(&foreign).is_err());
    let mut future = request.clone();
    future.expected_catalog_revision = request.expected_catalog_revision.next()?;
    assert!(registry.catalog_fence_unprepared_absence(&future).is_err());
    registry
        .catalog_fence_unprepared_absence(&request)?
        .validate_for(&request)?;
    let (intent, preview) = prepare(&mut registry, &request, &plan)?;
    assert!(registry.catalog_fence_unprepared_absence(&request).is_err());
    let mut other = request.clone();
    other.key.nonce = Incarnation::from_bytes([95; 16]);
    assert!(
        registry.catalog_fence_unprepared_absence(&other).is_err(),
        "another pending authority fence prevents reopening"
    );
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert!(
        registry.catalog_fence_unprepared_absence(&request).is_err(),
        "terminal fenced receipt is not absence"
    );
    assert_eq!(receipt_count(&registry)?, 1);
    assert!(registry.catalog_pending_fence()?.is_none());
    Ok(())
}

#[test]
fn unprepared_absence_refuses_legacy_authority_contract() -> Result<()> {
    let (_root, registry, request) = fixture()?;
    assert!(registry.catalog_fence_unprepared_absence(&request).is_err());
    assert_eq!(receipt_count(&registry)?, 0);
    Ok(())
}

#[test]
fn independent_reserved_setup_does_not_block_exact_absence_after_known_rejection() -> Result<()> {
    use airk_core::catalog_setup::{IndexSetupRequest, SetupSource, SetupStage};
    let (_control, mut registry, mut request) = fixture()?;
    let source_owner = tempfile::tempdir()?;
    let base = source_owner.path().canonicalize()?;
    let source = base.join("source");
    std::fs::create_dir(&source)?;
    let root = source.to_str().unwrap().to_owned();
    let setup = IndexSetupRequest {
        caller: Incarnation::from_bytes([40; 16]),
        key: RequestKey {
            nonce: Incarnation::from_bytes([41; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: request.expected_catalog_revision,
        name: "Independent pending setup".into(),
        registration: ProjectRegistration::new(
            root.clone(),
            base.join("data").to_str().unwrap().into(),
            "pending-setup".into(),
        )?,
        sources: vec![SetupSource {
            canonical_path: root,
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&setup)?;
    assert_eq!(reserved.stage, SetupStage::Reserved);
    request.expected_catalog_revision = reserved.catalog_revision;
    let rejected = registry
        .catalog_prepare_fence(&request, Sha256Digest::from_bytes([0; 32]), &plan(0)?)
        .unwrap_err();
    assert_eq!(
        rejected.downcast_ref::<FencePrepareRejected>(),
        Some(&FencePrepareRejected::StaleReview)
    );
    assert!(registry.catalog_pending_fence()?.is_none());
    registry
        .catalog_fence_absence(&request)?
        .validate_for(&request)?;
    assert_eq!(
        registry.index_setup_receipt(setup.caller, &setup.key)?,
        Some(reserved.clone())
    );
    assert_eq!(
        receipt_count(&registry)?,
        1,
        "absence must not mutate or evict the independent setup"
    );
    let CatalogOperation::DeleteProject { id: project_id, .. } = &request.operation else {
        unreachable!()
    };
    let project_id = *project_id;
    let mut grant_request = request.clone();
    grant_request.key.nonce = Incarnation::from_bytes([42; 16]);
    grant_request.operation = CatalogOperation::CreateGrant {
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([43; 16])),
            host: Sha256Digest::from_bytes([44; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id,
                    index_id: reserved.index_id,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: reserved.source_ids[0],
                        kind: PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                source_revision: reserved.source_revision,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
    };
    let refusal = registry.catalog_preview(&grant_request).unwrap_err();
    assert!(
        refusal
            .to_string()
            .contains("index source metadata unavailable"),
        "pending index grant must still reject for readiness: {refusal:#}"
    );
    assert_eq!(receipt_count(&registry)?, 1);
    Ok(())
}
