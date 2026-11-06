// SPDX-License-Identifier: Apache-2.0
//! Exercise the actual metadata actor's final response boundary with real saved
//! registry receipts and an independently pending native admission gate.
use super::*;
use crate::{
    catalog_admission::{FenceTicket, NativeCatalogAdmission},
    catalog_coordinator::{CatalogControl, CatalogCoordinator},
};
use airk_core::{
    catalog_fences::{FenceObservation, FencePreparation, FencedReceipt},
    catalog_management::{
        CatalogKind, CatalogOperation, CatalogPageRequest, CatalogRequest, MAX_PAGE_BYTES,
    },
    identity::Sha256Digest,
    index_catalog::{CatalogRevision, QueryProjectId},
    service::{RequestKey, WorkerIdentity},
};
use airk_store::service::registry::catalog::operations::FenceTarget;

struct Control {
    gate: NativeCatalogAdmission,
}
impl CatalogControl for Control {
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool {
        self.gate.same_instance(gate)
    }
    fn revoke(&self, _: &FenceTicket, _: &FenceTarget) -> Result<()> {
        anyhow::bail!("fixture has no installed scopes")
    }
    fn confirm_reaped(&self, _: WorkerIdentity) -> Result<bool> {
        Ok(false)
    }
}

fn request(
    registry: &ServiceRegistry,
    nonce: u8,
    operation: CatalogOperation,
) -> Result<CatalogRequest> {
    let (namespace, epoch) = registry.saved_request_epoch()?;
    Ok(CatalogRequest {
        caller: Incarnation::from_bytes([1; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([nonce; 16]),
        },
        expected_catalog_revision: registry.catalog_state()?.unwrap().revision,
        operation,
    })
}

fn commit(
    registry: &mut ServiceRegistry,
    gate: &NativeCatalogAdmission,
    request: &CatalogRequest,
) -> Result<FencedReceipt> {
    let preview = registry.catalog_preview(request)?.identity()?;
    let plan = gate.close_request_admission(request)?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(request, preview, &plan.durable)?
    else {
        anyhow::bail!("fresh actor fixture unexpectedly replayed")
    };
    gate.begin_revocations(&plan.ticket)?;
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(request, preview, intent.prepared_digest)
}

fn reconcile_registry(gate: &NativeCatalogAdmission, registry: &ServiceRegistry) -> Result<()> {
    // Same canonical metadata/clock routine called by CatalogRuntime::reconcile.
    crate::catalog_coordinator::refresh_catalog_clock(gate, registry).map(|_| ())
}

#[test]
fn actor_preserves_full_terminal_receipt_while_unrelated_cleanup_keeps_admission_closed()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let mut registry =
        crate::catalog_test_fixture::authority_one(&root, Incarnation::from_bytes([1; 16]))?;
    let projects = [
        QueryProjectId::from_identity(Incarnation::from_bytes([2; 16])),
        QueryProjectId::from_identity(Incarnation::from_bytes([3; 16])),
    ];
    for (offset, id) in projects.iter().enumerate() {
        let create = request(
            &registry,
            10 + offset as u8,
            CatalogOperation::CreateProject {
                id: *id,
                name: format!("Actor receipt fixture {offset}"),
            },
        )?;
        let preview = registry.catalog_preview(&create)?.identity()?;
        registry.catalog_apply(&create, preview)?;
    }
    let state = registry.catalog_state()?.unwrap();
    let gate = NativeCatalogAdmission::new(
        Incarnation::from_bytes([1; 16]),
        state.namespace,
        state.revision,
        1,
    )?;
    let mut coordinator = CatalogCoordinator::new(gate.clone(), Control { gate: gate.clone() })?;
    let deletion = |id| CatalogOperation::DeleteProject {
        id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_scope_revision: CatalogRevision::INITIAL,
        expected_defaults_revision: CatalogRevision::INITIAL,
    };
    let first = request(&registry, 20, deletion(projects[0]))?;
    let first_receipt = commit(&mut registry, &gate, &first)?;
    CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        first.caller,
        &first.key,
    )?;
    assert!(coordinator.has_pending_cleanup());
    assert!(gate.pending_fence_key()?.is_none());

    let second = request(&registry, 21, deletion(projects[1]))?;
    let second_receipt = commit(&mut registry, &gate, &second)?;
    let observed = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        second.caller,
        &second.key,
    )?;
    let expected = serde_json::to_vec(&ControlResponse::CatalogReceipt {
        receipt: Some(second_receipt.metadata.clone()),
        fence: Some(FenceObservation::Applied(second_receipt.clone())),
    })?;
    assert_eq!(serde_json::to_vec(&observed)?, expected);
    assert!(reconcile_registry(&gate, &registry).is_err());
    let calls = std::cell::Cell::new(0);
    let final_response = finalize_catalog_response(observed, || {
        calls.set(calls.get() + 1);
        reconcile_registry(&gate, &registry)
    });
    assert_eq!(
        calls.get(),
        1,
        "saved receipt still observes live reconciliation failure"
    );
    assert_eq!(
        serde_json::to_vec(&final_response)?,
        expected,
        "actor wrapper reduced immutable terminal evidence"
    );
    assert!(gate.matching_plan(&second)?.is_some());
    assert!(coordinator.has_pending_cleanup());

    // An ordinary current metadata page and mutation response retain the same
    // post-handler failure; terminal evidence is the sole narrow exception.
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Projects,
        project_id: None,
        index_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 16,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    for ordinary in [
        ControlResponse::CatalogPage {
            page,
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlResponse::CatalogApplied {
            receipt: second_receipt.metadata.clone(),
            fenced_receipt: Some(second_receipt.clone()),
        },
    ] {
        assert!(
            matches!(finalize_catalog_response(ordinary, || reconcile_registry(&gate, &registry)), ControlResponse::Failure { code } if code == "catalog_metadata_saved_outcome_unknown")
        );
    }
    // Even a receipt-shaped response cannot use the exception with mismatched
    // metadata or an invalid internal proof.
    let mismatched = ControlResponse::CatalogReceipt {
        receipt: Some(first_receipt.metadata.clone()),
        fence: Some(FenceObservation::Applied(second_receipt.clone())),
    };
    assert!(matches!(
        finalize_catalog_response(mismatched, || reconcile_registry(&gate, &registry)),
        ControlResponse::Failure { .. }
    ));
    let mut invalid = second_receipt;
    invalid.metadata.request_digest = Sha256Digest::from_bytes([0; 32]);
    let invalid_response = ControlResponse::CatalogReceipt {
        receipt: Some(invalid.metadata.clone()),
        fence: Some(FenceObservation::Applied(invalid)),
    };
    assert!(matches!(
        finalize_catalog_response(invalid_response, || reconcile_registry(&gate, &registry)),
        ControlResponse::Failure { .. }
    ));
    assert!(gate.matching_plan(&second)?.is_some());
    assert!(coordinator.has_pending_cleanup());
    assert_eq!(
        coordinator.resume_pending(&mut registry)?,
        Some(first_receipt)
    );
    assert!(
        gate.matching_plan(&second)?.is_some(),
        "unrelated cleanup must not reopen the pending gate"
    );
    Ok(())
}
