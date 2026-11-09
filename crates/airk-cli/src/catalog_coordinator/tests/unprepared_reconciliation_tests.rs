// SPDX-License-Identifier: Apache-2.0
//! Staged under catalog_coordinator_tests after the native proof API review.
use super::*;
use anyhow::Context;

#[test]
fn natural_reap_during_preparation_does_not_become_a_catalog_issued_effect() -> Result<()> {
    use airk_core::{
        grant_provenance::GrantProvenance, index_catalog::*, scoped_reads::SourceDescriptor,
        service::ProjectBinding,
    };
    use airk_kernel::scoped_reads::ScopedReadAuthority;
    let (root, mut registry, request, preview, gate) = fixture()?;
    let CatalogOperation::DeleteProject { id: project_id, .. } = request.operation else {
        anyhow::bail!("fixture Project changed")
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([70; 16]),
        project: ProjectBinding::from_bytes([71; 32]),
        fencing_generation: 1,
    };
    let index_id = IndexId::from_identity(Incarnation::from_bytes([72; 16]));
    let source_id = SourceRootId::from_identity(Incarnation::from_bytes([73; 16]));
    // A native authority/reap boundary fixture, not an OS worker or grant-
    // issuance qualification. The Store rejection and absence read are real.
    let authority = ScopedReadAuthority {
        schema: 1,
        provenance: GrantProvenance::UserReviewedProjectGrant,
        backing: worker.project,
        index_id,
        project_ids: vec![project_id],
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([74; 16])),
            host: Sha256Digest::from_bytes([75; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id,
                    index_id,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source_id,
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
            source_root_id: source_id,
            original_root: root.path().to_str().unwrap().into(),
        }],
    };
    gate.register_worker(worker)?;
    let old_issue = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    let install = gate.reserve_install(
        old_issue,
        worker,
        &authority,
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )?;
    gate.acknowledge_install(install, worker, install.scope())?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    for n in 0..1023u64 {
        sql.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,'capacity_fixture','saved',1,?2)",
            params![format!("eeeeeeeeeeeeeeee{n:016x}"), b"fixture".as_slice()],
        )?;
    }
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let error = coordinator
        .apply_with_hook(&mut registry, &request, preview, || {
            gate.observe_worker_reaped(worker).unwrap();
            gate.observe_worker_provider_history(worker, vec![])
                .unwrap();
        })
        .err()
        .context("receipt capacity must refuse before preparing")?;
    assert!(error.is::<CatalogApplyRejectedNoEffects>());
    assert!(registry.catalog_pending_fence()?.is_none());
    assert!(gate.worker_was_reaped(worker)?);
    assert_eq!(gate.worker_provider_history(worker)?, Some(vec![]));
    assert!(gate.check_issue(old_issue).is_err());
    gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    Ok(())
}

#[test]
fn explicit_unprepared_reconciliation_advances_only_to_native_same_namespace_snapshot() -> Result<()>
{
    let (_root, mut registry, request, _preview, gate) = fixture()?;
    let CatalogOperation::DeleteProject { id, .. } = request.operation else {
        anyhow::bail!("fixture Project changed")
    };
    let old_issue = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    gate.close_request_admission(&request)?;
    // Exercise an actual independent native metadata commit while the host gate
    // is closed but no durable intent exists. Private SourceComplete advances
    // this same catalog clock; its held-writer producer is qualified separately.
    let rename = CatalogRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([76; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: request.expected_catalog_revision,
        operation: CatalogOperation::RenameProject {
            id,
            expected_definition_revision: CatalogRevision::INITIAL,
            name: "Independent native completion clock".into(),
        },
    };
    let preview = registry.catalog_preview(&rename)?;
    let committed = registry.catalog_apply(&rename, preview.identity()?)?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    coordinator.reconcile_unprepared(&registry, &request)?;
    assert!(gate.check_issue(old_issue).is_err());
    gate.begin_issue(request.key.namespace, committed.catalog_revision)?;
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        committed.catalog_revision
    );
    assert!(
        registry
            .catalog_fence_observation(request.caller, &request.key)?
            .is_none()
    );
    // Reconciliation never makes the original stale write current.
    assert!(registry.catalog_preview(&request).is_err());
    Ok(())
}

#[test]
fn public_receipt_uses_native_retained_body_and_old_epoch_absence() -> Result<()> {
    let (root, mut registry, request, preview, gate) = fixture()?;
    gate.close_request_admission(&request)?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    for (caller, key) in [
        (Incarnation::from_bytes([81; 16]), request.key.clone()),
        (
            request.caller,
            RequestKey {
                nonce: Incarnation::from_bytes([82; 16]),
                ..request.key.clone()
            },
        ),
    ] {
        let response =
            CatalogCoordinator::observe_receipt(&registry, Some(&mut coordinator), caller, &key)?;
        assert!(matches!(
            response,
            airk_core::service::ControlResponse::CatalogReceipt {
                receipt: None,
                fence: None
            }
        ));
        assert!(
            gate.matching_plan(&request)?.is_some(),
            "foreign receipt identity released a native claim"
        );
    }
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    sql.execute(
        "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
        [],
    )?;
    assert!(
        registry
            .catalog_fence_observation(request.caller, &request.key)
            .is_err()
    );
    assert!(
        registry
            .catalog_receipt(request.caller, &request.key)
            .is_err()
    );
    let response = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        request.caller,
        &request.key,
    )?;
    let first = serde_json::to_value(&response)?;
    let airk_core::service::ControlResponse::CatalogReceiptUnprepared {
        request: original,
        request_digest,
        catalog_clock,
    } = response
    else {
        anyhow::bail!("exact native non-preparation observation missing")
    };
    assert_eq!(original, request);
    assert_eq!(request_digest, request.identity()?);
    assert_eq!(catalog_clock.namespace, request.key.namespace);
    assert_eq!(catalog_clock.revision, request.expected_catalog_revision);
    gate.begin_issue(catalog_clock.namespace, catalog_clock.revision)?;
    // Treat the first response as lost. A readonly retry regenerates it from
    // retained native identity plus a NEW consistent absence observation.
    assert_eq!(
        serde_json::to_value(CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut coordinator),
            request.caller,
            &request.key
        )?)?,
        first
    );
    let mut other = request.clone();
    other.key.nonce = Incarnation::from_bytes([83; 16]);
    gate.close_request_admission(&other)?;
    assert!(
        CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut coordinator),
            request.caller,
            &request.key
        )
        .is_err()
    );
    assert!(
        gate.matching_plan(&other)?.is_some(),
        "cached observation cleared another pending claim"
    );
    coordinator.reconcile_unprepared(&registry, &other)?;
    assert_eq!(
        serde_json::to_value(CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut coordinator),
            request.caller,
            &request.key
        )?)?,
        first,
        "another caller operation evicted recoverable identity"
    );
    assert!(gate.close_request_admission(&request).is_err());
    let plan = airk_store::service::registry::catalog::operations::FencePlan::new(
        Incarnation::from_bytes([1; 16]),
        1,
        vec![],
    )?;
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &plan)
            .is_err(),
        "readonly observation admitted an expired-key mutation"
    );
    Ok(())
}

#[test]
fn native_retention_capacity_refuses_before_dispatch_with_proven_saved_absence() -> Result<()> {
    let (_root, mut registry, request, preview, gate) = fixture()?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let mut full = None;
    for n in 1..=33u8 {
        let mut candidate = request.clone();
        candidate.key.nonce = Incarnation::from_bytes([100 + n; 16]);
        match gate.close_request_admission(&candidate) {
            Ok(_) => coordinator.reconcile_unprepared(&registry, &candidate)?,
            Err(error) => {
                assert!(error.is::<crate::catalog_admission::NativeCatalogRetentionFull>());
                full = Some(candidate);
                break;
            }
        }
    }
    let refused = full.context("bounded native observation retention did not refuse")?;
    let error = coordinator
        .apply_fenced(&mut registry, &refused, preview)
        .err()
        .context("capacity overflow dispatched")?;
    assert!(error.is::<crate::catalog_admission::NativeCatalogRetentionFull>());
    assert!(error.is::<CatalogApplyRejectedNoEffects>());
    assert!(gate.pending_fence_key()?.is_none());
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        request.expected_catalog_revision
    );
    // Earliest retained identity survives pressure; no silent eviction.
    let mut oldest = request.clone();
    oldest.key.nonce = Incarnation::from_bytes([101; 16]);
    assert!(
        matches!(CatalogCoordinator::observe_receipt(&registry,Some(&mut coordinator),oldest.caller,&oldest.key)?,airk_core::service::ControlResponse::CatalogReceiptUnprepared {request:original,..} if original==oldest)
    );
    Ok(())
}

#[test]
fn captured_provider_growth_rechecks_shared_retention_budget_without_partial_census() -> Result<()>
{
    use airk_core::{
        catalog_fences::{WorkerDrainObligation, WorkerProviderHistory, WorkerProviderObligation},
        index_catalog::IndexId,
        service::ProjectBinding,
    };
    let (_root, registry, request, _preview, gate) = fixture()?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    for n in 1..=5u8 {
        let mut prior = request.clone();
        prior.key.nonce = Incarnation::from_bytes([150 + n; 16]);
        gate.close_request_admission(&prior)?;
        coordinator.reconcile_unprepared(&registry, &prior)?;
    }
    let backing = ProjectBinding::from_bytes([90; 32]);
    let index = IndexId::from_identity(Incarnation::from_bytes([91; 16]));
    let mut retiring = request.clone();
    retiring.key.nonce = Incarnation::from_bytes([92; 16]);
    retiring.operation = CatalogOperation::RetireIndex {
        id: index,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let workers: Vec<_> = (1..=9u8)
        .map(|n| WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([130 + n; 16]),
            project: backing,
            fencing_generation: u64::from(n),
        })
        .collect();
    for worker in &workers {
        gate.register_worker(*worker)?;
    }
    let closed = gate.close_retirement_admission(&retiring, backing)?;
    let providers: Vec<_> = workers
        .iter()
        .map(|worker| WorkerProviderObligation {
            worker: *worker,
            history: WorkerProviderHistory::Helpers {
                generations: (1..=32u8)
                    .map(|n| Incarnation::from_bytes([n; 16]))
                    .collect(),
            },
        })
        .collect();
    // The candidate plan alone fits; retained full originals account for the
    // additional shared memory pressure. These are native-ledger boundary
    // fixtures, not a claim of real child/provider completion.
    let _fits = closed
        .durable
        .clone()
        .with_worker_obligation(WorkerDrainObligation {
            index_id: index,
            backing,
            workers: workers.clone(),
            providers: providers.clone(),
        })?;
    let error = gate
        .capture_retirement_workers(&closed.ticket, backing, workers, providers)
        .err()
        .context("captured history bypassed aggregate memory bound")?;
    assert!(error.is::<crate::catalog_admission::NativeCatalogRetentionFull>());
    let unchanged = gate.matching_plan(&retiring)?.unwrap();
    assert_eq!(unchanged.durable, closed.durable);
    assert!(gate.require_commit_ready(&closed.ticket).is_err());
    coordinator.reconcile_unprepared(&registry, &retiring)?;
    gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    Ok(())
}

#[test]
fn public_receipt_returns_old_epoch_saved_fence_before_stale_ephemeral_body() -> Result<()> {
    use airk_core::service::ControlResponse;
    let (root, mut registry, request, preview, gate) = fixture()?;
    let plan = gate.close_request_admission(&request)?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&request, preview, &plan.durable)?
    else {
        anyhow::bail!("fixture replayed")
    };
    let mut stale = request.clone();
    if let CatalogOperation::DeleteProject {
        expected_scope_revision,
        ..
    } = &mut stale.operation
    {
        *expected_scope_revision = CatalogRevision::INITIAL.next()?;
    }
    let stale_gate = NativeCatalogAdmission::new(
        Incarnation::from_bytes([1; 16]),
        request.key.namespace,
        request.expected_catalog_revision,
        1,
    )?;
    stale_gate.close_request_admission(&stale)?;
    let mut stale_coordinator = CatalogCoordinator::new(
        stale_gate.clone(),
        NoTargets {
            gate: stale_gate.clone(),
        },
    )?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    sql.execute(
        "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
        [],
    )?;
    let pending = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut stale_coordinator),
        request.caller,
        &request.key,
    )?;
    let ControlResponse::CatalogReceipt {
        receipt: None,
        fence: Some(FenceObservation::Pending(saved)),
    } = pending
    else {
        anyhow::bail!("old-epoch full pending intent lost")
    };
    assert_eq!(saved, intent);
    assert!(stale_gate.matching_plan(&stale)?.is_some());
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    let terminal = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    let applied = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut stale_coordinator),
        request.caller,
        &request.key,
    )?;
    let ControlResponse::CatalogReceipt {
        receipt: Some(metadata),
        fence: Some(FenceObservation::Applied(saved)),
    } = applied
    else {
        anyhow::bail!("old-epoch full terminal receipt lost")
    };
    assert_eq!(metadata, terminal.metadata);
    assert_eq!(saved, terminal);
    assert!(
        stale_gate.matching_plan(&stale)?.is_some(),
        "conflicting ephemeral identity was silently cleared"
    );
    let mut matching = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    assert!(matches!(
        CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut matching),
            request.caller,
            &request.key
        )?,
        ControlResponse::CatalogReceipt {
            fence: Some(FenceObservation::Applied(_)),
            ..
        }
    ));
    gate.begin_issue(request.key.namespace, terminal.metadata.catalog_revision)?;
    Ok(())
}

#[test]
fn public_receipt_keeps_unknown_reads_and_catalog_issued_effects_closed() -> Result<()> {
    let (root, registry, request, _preview, gate) = fixture()?;
    let plan = gate.close_request_admission(&request)?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    sql.execute_batch("BEGIN EXCLUSIVE")?;
    let error = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        request.caller,
        &request.key,
    )
    .err()
    .context("uncertain saved read became absence")?;
    let unknown = error
        .downcast_ref::<CatalogApplyOutcomeUnknown>()
        .expect("original native identity lost");
    assert_eq!(unknown.request, request);
    assert!(gate.matching_plan(&request)?.is_some());
    sql.execute_batch("ROLLBACK")?;
    gate.begin_revocations(&plan.ticket)?;
    let error = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        request.caller,
        &request.key,
    )
    .err()
    .context("issued native effect was treated as unprepared")?;
    assert!(error.is::<CatalogApplyOutcomeUnknown>());
    assert!(gate.matching_plan(&request)?.is_some());
    Ok(())
}
