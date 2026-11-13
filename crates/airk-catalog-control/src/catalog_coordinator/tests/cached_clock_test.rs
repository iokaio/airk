// SPDX-License-Identifier: Apache-2.0
//! Staged under catalog_coordinator_tests after the current native check.
use super::*;

#[test]
fn cached_unprepared_receipt_advances_new_native_clock_once_and_invalidates_old_issue() -> Result<()>
{
    let (_root, mut registry, request, _preview, gate) = fixture()?;
    let CatalogOperation::DeleteProject { id, .. } = request.operation else {
        anyhow::bail!("fixture Project changed")
    };
    let mut coordinator = CatalogCoordinator::new(gate.clone(), NoTargets { gate: gate.clone() })?;
    gate.close_request_admission(&request)?;
    coordinator.reconcile_unprepared(&registry, &request)?;
    let old = gate.begin_issue(request.key.namespace, request.expected_catalog_revision)?;
    let rename = CatalogRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([84; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: request.expected_catalog_revision,
        operation: CatalogOperation::RenameProject {
            id,
            expected_definition_revision: CatalogRevision::INITIAL,
            name: "Later native catalog clock".into(),
        },
    };
    let reviewed = registry.catalog_preview(&rename)?;
    let committed = registry.catalog_apply(&rename, reviewed.identity()?)?;
    let response = CatalogCoordinator::observe_receipt(
        &registry,
        Some(&mut coordinator),
        request.caller,
        &request.key,
    )?;
    assert!(
        matches!(response,airk_core::service::ControlResponse::CatalogReceiptUnprepared {request:original,catalog_clock,..} if original==request && catalog_clock.namespace==request.key.namespace && catalog_clock.revision==committed.catalog_revision)
    );
    assert!(gate.check_issue(old).is_err());
    let current = gate.begin_issue(request.key.namespace, committed.catalog_revision)?;
    assert!(matches!(
        CatalogCoordinator::observe_receipt(
            &registry,
            Some(&mut coordinator),
            request.caller,
            &request.key
        )?,
        airk_core::service::ControlResponse::CatalogReceiptUnprepared { .. }
    ));
    gate.check_issue(current)?;
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        committed.catalog_revision
    );
    Ok(())
}
