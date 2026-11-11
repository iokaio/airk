// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    catalog_management::CatalogOperation,
    identity::Incarnation,
    index_catalog::{CatalogRevision, QueryProjectId},
    service::RequestKey,
};
use airk_store::service::registry::catalog::upgrade::CatalogUpgrade;
use rusqlite::{Connection, params};

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
fn fixture() -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    CatalogRequest,
    Sha256Digest,
    NativeCatalogAdmission,
)> {
    let root = tempfile::tempdir()?;
    crate::local_ipc::prepare_root(root.path())?;
    let instance = Incarnation::from_bytes([1; 16]);
    drop(ServiceRegistry::open(root.path(), instance)?);
    let upgrade = CatalogUpgrade::acquire(root.path())?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, Sha256Digest::from_bytes([2; 32]))?;
    upgrade.migrate()?;
    drop(upgrade);
    // Isolated transaction fixture only. This does not exercise or qualify
    // managed readiness, authority activation, installers or runtime cutover.
    let sql = Connection::open(root.path().join("service-1.4.sqlite3"))?;
    sql.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    drop(sql);
    let owner = root.path().join("service-1.4.owner.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner)?)?;
    record.as_object_mut().unwrap().remove("catalog_upgrade");
    std::fs::write(owner, serde_json::to_vec(&record)?)?;
    let mut registry = ServiceRegistry::open(root.path(), instance)?;
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
        .expect("capacity must refuse");
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
