// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    catalog_management::CatalogOperation,
    identity::Incarnation,
    index_catalog::{CatalogRevision, QueryProjectId},
    service::RequestKey,
};
use anyhow::Context;
use rusqlite::{Connection, params};

#[path = "catalog_cached_clock_test.rs"]
mod cached_clock_test;
#[path = "catalog_unprepared_reconciliation_tests.rs"]
mod unprepared_reconciliation_tests;

#[test]
fn accepted_prepared_sql_failure_reports_full_unknown_then_exact_receipt_settles() -> Result<()> {
    let (root, mut registry, request, preview, gate) = fixture()?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let error = coordinator
        .apply_fenced_after_prepare(&mut registry, &request, preview, || {
            // Keep the exact compiled schema intact. An actual external SHARED
            // read transaction starts only AFTER the durable intent commits,
            // blocks the next write commit and still permits full saved reads.
            sql.execute_batch("BEGIN").unwrap();
            let revision: i64 = sql
                .query_row(
                    "SELECT revision FROM index_catalog_meta WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(revision, request.expected_catalog_revision.get() as i64);
        })
        .err()
        .context("commit must refuse")?;
    #[derive(serde::Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum UnboxedObservation<'a> {
        AuthorityFence { fence: &'a FenceObservation },
    }
    let observation = error
        .downcast_ref::<CatalogApplyOutcomeUnknown>()
        .and_then(|unknown| unknown.observation.as_ref())
        .expect("saved native observation");
    assert_eq!(
        serde_json::to_vec(
            &airk_core::service::CatalogOutcomeObservation::AuthorityFence {
                fence: Box::new(observation.clone()),
            }
        )?,
        serde_json::to_vec(&UnboxedObservation::AuthorityFence { fence: observation })?,
    );
    let unknown = error
        .downcast_ref::<CatalogApplyOutcomeUnknown>()
        .expect("accepted durable intent must remain unknown");
    let airk_core::service::ControlResponse::CatalogOutcomeUnknown {
        caller,
        key,
        request_digest,
        observation: Some(airk_core::service::CatalogOutcomeObservation::AuthorityFence { fence }),
        ..
    } = unknown.response()
    else {
        anyhow::bail!("complete pending observation missing");
    };
    let FenceObservation::Pending(intent) = *fence else {
        anyhow::bail!("complete pending observation missing");
    };
    assert_eq!(caller, request.caller);
    assert_eq!(key, request.key);
    assert_eq!(request_digest, request.identity()?);
    assert_eq!(intent.request, request);
    assert!(
        error
            .downcast_ref::<CatalogApplyRejectedNoEffects>()
            .is_none()
    );
    assert!(
        gate.begin_issue(request.key.namespace, request.expected_catalog_revision)
            .is_err()
    );
    sql.execute_batch("ROLLBACK")?;
    let receipt = coordinator.apply_fenced(&mut registry, &request, preview)?;
    assert_eq!(receipt.request, request);
    assert_eq!(
        coordinator.apply_fenced(&mut registry, &request, preview)?,
        receipt
    );
    gate.begin_issue(request.key.namespace, receipt.metadata.catalog_revision)?;
    Ok(())
}

#[test]
fn postcommit_failure_capture_preserves_full_terminal_proof_and_replay_finishes_gate() -> Result<()>
{
    let (_root, mut registry, request, preview, gate) = fixture()?;
    let plan = gate.close_request_admission(&request)?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&request, preview, &plan.durable)?
    else {
        anyhow::bail!("fixture unexpectedly replayed");
    };
    gate.begin_revocations(&plan.ticket)?;
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    let receipt = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    let error = CatalogApplyOutcomeUnknown::capture(
        &registry,
        &request,
        anyhow::anyhow!("native handoff lost after commit"),
    );
    let unknown = error
        .downcast_ref::<CatalogApplyOutcomeUnknown>()
        .expect("saved outcome must remain unknown until observed");
    let airk_core::service::ControlResponse::CatalogOutcomeUnknown {
        observation: Some(airk_core::service::CatalogOutcomeObservation::AuthorityFence { fence }),
        ..
    } = unknown.response()
    else {
        anyhow::bail!("complete applied proof missing");
    };
    let FenceObservation::Applied(saved) = *fence else {
        anyhow::bail!("complete applied proof missing");
    };
    assert_eq!(saved, receipt);
    assert!(
        gate.begin_issue(request.key.namespace, receipt.metadata.catalog_revision)
            .is_err()
    );
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    assert_eq!(
        coordinator.apply_fenced(&mut registry, &request, preview)?,
        receipt
    );
    gate.begin_issue(request.key.namespace, receipt.metadata.catalog_revision)?;
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        receipt.metadata.catalog_revision
    );
    Ok(())
}

struct NoTargets {
    gate: NativeCatalogAdmission,
}
impl CatalogControl for NoTargets {
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool {
        self.gate.same_instance(gate)
    }
    fn revoke(&self, _: &FenceTicket, _: &FenceTarget) -> Result<()> {
        anyhow::bail!("fixture has no installed native scopes")
    }
    fn confirm_reaped(&self, _: WorkerIdentity) -> Result<bool> {
        Ok(false)
    }
}
struct PrivateRuntimeFixture {
    _owner: tempfile::TempDir,
    root: std::path::PathBuf,
}
impl PrivateRuntimeFixture {
    fn path(&self) -> &std::path::Path {
        &self.root
    }
}

fn fixture() -> Result<(
    PrivateRuntimeFixture,
    ServiceRegistry,
    CatalogRequest,
    Sha256Digest,
    NativeCatalogAdmission,
)> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    // The native creator owns this new fixture child. Existing permissive
    // directories must still fail the production runtime privacy check.
    crate::local_ipc::prepare_root(&control)?;
    let root = PrivateRuntimeFixture {
        _owner: owner,
        root: control.canonicalize()?,
    };
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = crate::catalog_test_fixture::authority_one(root.path(), instance)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([3; 16]));
    let create = CatalogRequest {
        caller: instance,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([4; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::CreateProject {
            id: project,
            name: "Native coordinator fixture".into(),
        },
    };
    let preview = registry.catalog_preview(&create)?;
    let created = registry.catalog_apply(&create, preview.identity()?)?;
    let request = CatalogRequest {
        caller: instance,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([5; 16]),
        },
        expected_catalog_revision: created.catalog_revision,
        operation: CatalogOperation::DeleteProject {
            id: project,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
    };
    let digest = registry.catalog_preview(&request)?.identity()?;
    let gate = NativeCatalogAdmission::new(instance, namespace, created.catalog_revision, 1)?;
    Ok((root, registry, request, digest, gate))
}

#[test]
fn actual_receipt_capacity_rejection_reconciles_absence_and_reopens_without_native_effects()
-> Result<()> {
    let (root, mut registry, request, preview, gate) = fixture()?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    for n in 0..1023u64 {
        sql.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,'capacity_fixture','saved',1,?2)",
            params![format!("ffffffffffffffff{n:016x}"), b"fixture".as_slice()],
        )?;
    }
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let error = coordinator
        .apply_fenced(&mut registry, &request, preview)
        .err()
        .context("capacity must refuse")?;
    assert!(error.downcast_ref::<airk_store::service::registry::catalog::operations::FencePrepareRejected>().is_some());
    assert!(registry.catalog_pending_fence()?.is_none());
    gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        request.expected_catalog_revision
    );
    Ok(())
}

#[test]
fn unknown_prepare_failure_retains_closed_gate_until_explicit_native_absence_reconciliation()
-> Result<()> {
    let (root, mut registry, request, preview, gate) = fixture()?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let result = coordinator.apply_with_hook(&mut registry, &request, preview, || {
        sql.execute_batch("BEGIN EXCLUSIVE").unwrap()
    });
    assert!(result.is_err());
    assert!(
        gate.begin_issue(request.key.namespace, request.expected_catalog_revision)
            .is_err()
    );
    sql.execute_batch("ROLLBACK")?;
    assert!(registry.catalog_pending_fence()?.is_none());
    coordinator.reconcile_unprepared(&registry, &request)?;
    gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    Ok(())
}

#[test]
fn actual_terminal_replay_closes_commit_to_gate_finish_gap_without_second_mutation() -> Result<()> {
    let (_root, mut registry, request, preview, gate) = fixture()?;
    let plan = gate.close_request_admission(&request)?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&request, preview, &plan.durable)?
    else {
        anyhow::bail!("new fixture replayed")
    };
    gate.begin_revocations(&plan.ticket)?;
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    let committed = registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    assert!(
        gate.begin_issue(request.key.namespace, committed.metadata.catalog_revision)
            .is_err()
    );
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    assert_eq!(
        coordinator.apply_fenced(&mut registry, &request, preview)?,
        committed
    );
    gate.begin_issue(request.key.namespace, committed.metadata.catalog_revision)?;
    assert_eq!(
        coordinator.apply_fenced(&mut registry, &request, preview)?,
        committed
    );
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        committed.metadata.catalog_revision
    );
    Ok(())
}

#[test]
fn epoch_rotation_after_gate_close_can_prove_old_key_absence_without_new_write_admission()
-> Result<()> {
    let (root, mut registry, request, preview, gate) = fixture()?;
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let result = coordinator.apply_with_hook(&mut registry, &request, preview, || {
        // Models the persisted epoch change made by independent job-receipt
        // maintenance after this exact metadata operation was accepted.
        sql.execute(
            "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
            [],
        )
        .unwrap();
    });
    assert!(result.is_err());
    assert!(
        gate.begin_issue(request.key.namespace, request.expected_catalog_revision)
            .is_err()
    );
    assert!(
        registry
            .catalog_fence_observation(request.caller, &request.key)
            .is_err(),
        "public old-epoch admission changed"
    );
    coordinator.reconcile_unprepared(&registry, &request)?;
    gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    assert!(
        gate.close_request_admission(&request).is_err(),
        "known unprepared key was redispatched"
    );
    let plan = airk_store::service::registry::catalog::operations::FencePlan::new(
        Incarnation::from_bytes([1; 16]),
        1,
        vec![],
    )?;
    assert!(
        registry
            .catalog_prepare_fence(&request, preview, &plan)
            .is_err(),
        "absence proof authorized a new expired-key mutation"
    );
    assert!(matches!(
        CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut coordinator),
            request.caller,
            &request.key
        )?,
        airk_core::service::ControlResponse::CatalogReceiptUnprepared { .. }
    ));
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        request.expected_catalog_revision
    );
    Ok(())
}

#[test]
fn exact_revision_absence_cannot_reopen_after_namespace_or_catalog_revision_changes() -> Result<()>
{
    for namespace_change in [true, false] {
        let (root, registry, request, _preview, gate) = fixture()?;
        gate.close_request_admission(&request)?;
        let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
        if namespace_change {
            sql.execute(
                "UPDATE service_meta SET incarnation=?1 WHERE id=1",
                [Incarnation::from_bytes([14; 16]).to_string()],
            )?;
        } else {
            sql.execute(
                "UPDATE index_catalog_meta SET revision=revision+1 WHERE id=1",
                [],
            )?;
        }
        let plan = gate.matching_plan(&request)?.unwrap();
        let exact = KnownUnprepared::observe(&registry, &request)
            .and_then(|proof| gate.abort_known_unprepared(&plan.ticket, proof));
        assert!(exact.is_err());
        assert!(
            gate.begin_issue(request.key.namespace, request.expected_catalog_revision)
                .is_err()
        );
        assert!(gate.matching_plan(&request)?.is_some());
    }
    Ok(())
}

#[test]
fn independent_pending_setup_does_not_wedge_unaffected_issuance_after_prepare_rejection()
-> Result<()> {
    use airk_core::{
        catalog_setup::{IndexSetupRequest, SetupSource, SetupStage},
        index_catalog::*,
        scoped_reads::SourceDescriptor,
        service::ProjectRegistration,
    };
    use airk_kernel::scoped_reads::ScopedReadAuthority;
    let (control, mut registry, mut request, _old_preview, gate) = fixture()?;
    let sources = tempfile::tempdir()?;
    let base = sources.path().canonicalize()?;
    let source = base.join("pending");
    let available = base.join("available");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&available)?;
    let pending_root = source.to_str().unwrap().to_owned();
    let setup = IndexSetupRequest {
        caller: Incarnation::from_bytes([40; 16]),
        key: RequestKey {
            nonce: Incarnation::from_bytes([41; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: request.expected_catalog_revision,
        name: "Independent pending index".into(),
        registration: ProjectRegistration::new(
            pending_root.clone(),
            base.join("pending-data").to_str().unwrap().into(),
            "pending".into(),
        )?,
        sources: vec![SetupSource {
            canonical_path: pending_root,
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&setup)?;
    assert_eq!(reserved.stage, SetupStage::Reserved);
    gate.observe_catalog_commit(request.expected_catalog_revision, reserved.catalog_revision)?;
    request.expected_catalog_revision = reserved.catalog_revision;

    // This component fixture supplies an already reviewed unrelated scope to the
    // native gate. It does not qualify catalog-grant issuance or worker IPC.
    let available_root = available.to_str().unwrap().to_owned();
    let backing = ProjectRegistration::new(
        available_root.clone(),
        base.join("available-data").to_str().unwrap().into(),
        "available".into(),
    )?;
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([50; 16]),
        project: backing.binding,
        fencing_generation: 1,
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([51; 16]));
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([52; 16]));
    let source_id = SourceRootId::from_identity(Incarnation::from_bytes([53; 16]));
    let authority = ScopedReadAuthority {
        schema: 1,
        backing: backing.binding,
        index_id: index,
        project_ids: vec![project],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([54; 16])),
            host: Sha256Digest::from_bytes([55; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
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
            original_root: available_root,
        }],
    };
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    let install = gate.reserve_install(
        issue,
        worker,
        &authority,
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )?;
    let scope = install.scope();
    gate.acknowledge_install(install, worker, scope)?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    let rejected = coordinator
        .apply_fenced(&mut registry, &request, Sha256Digest::from_bytes([0; 32]))
        .unwrap_err();
    assert_eq!(
        rejected.downcast_ref::<airk_core::catalog_fences::FencePrepareRejected>(),
        Some(&airk_core::catalog_fences::FencePrepareRejected::StaleReview)
    );
    let fresh = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    gate.check_scope_issue(fresh, worker, scope)?;

    let rejected_request = request.clone();
    // A known, retained Unprepared result must never be redispatched. Model a
    // distinct explicit native review for the separate SQL-uncertain attempt.
    let (namespace, epoch) = registry.request_epoch()?;
    request.key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([54; 16]),
    };
    let preview = registry.catalog_preview(&request)?.identity()?;
    assert!(
        matches!(CatalogCoordinator::observe_receipt(&registry,Some(&mut coordinator),rejected_request.caller,&rejected_request.key)?,airk_core::service::ControlResponse::CatalogReceiptUnprepared {request:original,..} if original==rejected_request)
    );

    let sql = Connection::open(control.path().join("service-1.4.sqlite3"))?;
    assert!(
        coordinator
            .apply_with_hook(&mut registry, &request, preview, || sql
                .execute_batch("BEGIN EXCLUSIVE")
                .unwrap())
            .is_err()
    );
    assert!(
        gate.begin_issue(request.key.namespace, request.expected_catalog_revision)
            .is_err()
    );
    sql.execute_batch("ROLLBACK")?;
    coordinator.reconcile_unprepared(&registry, &request)?;
    let fresh = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    gate.check_scope_issue(fresh, worker, scope)?;
    assert_eq!(
        registry.index_setup_receipt(setup.caller, &setup.key)?,
        Some(reserved)
    );
    assert!(registry.catalog_pending_fence()?.is_none());
    Ok(())
}
