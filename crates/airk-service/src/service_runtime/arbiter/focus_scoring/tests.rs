// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    combined_context::PartitionAllowance,
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
};
#[test]
fn native_focus_preserves_context_snapshot_identity_and_checks_singleton_selection() -> Result<()> {
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/split/arbiter/focus_only_allowance_cases.json"
    ))?;
    for owner in [SelectionOwnerKind::Window, SelectionOwnerKind::Agent] {
        for row in ["ordinary_focus", "context", "zero_optional_work"] {
            let partition: PartitionAllowance =
                serde_json::from_value(fixture[row]["allowance"].clone())?;
            let mut selected = SelectionScopeRef {
                owner,
                grant_nonce: Incarnation::from_bytes([31; 16]),
                service_instance: Incarnation::from_bytes([32; 16]),
                scope_identity: partition.selection,
                catalog_revision: CatalogRevision::INITIAL,
            };
            validate_partition_selection(Some(&partition), Some(selected))?;
            selected.scope_identity = Sha256Digest::from_bytes([33; 32]);
            assert_ne!(partition.selection, selected.scope_identity);
            assert_eq!(
                validate_partition_selection(Some(&partition), Some(selected)).is_err(),
                partition.is_ordinary_focus()
            );
            assert_eq!(
                validate_partition_selection(Some(&partition), None).is_err(),
                partition.is_ordinary_focus()
            );
            let mut malformed = partition.clone();
            malformed.resources.calls = malformed.parent_budget.calls + 1;
            assert!(validate_partition_selection(Some(&malformed), Some(selected)).is_err());
            validate_partition_selection(None, Some(selected))?;
        }
    }
    Ok(())
}
