// SPDX-License-Identifier: Apache-2.0
//! Independent expectations for grouping publication and set-only deletion.
use super::*;
use airk_core::index_catalog::{Membership, QueryProjectId};

fn grouped(request: &mut SplitRequest, plan: &SplitPolicyPlan) -> Result<()> {
    request.grouping = Some(SplitGrouping {
        mode: SplitGroupingMode::Create,
        project_id: SplitGrouping::project_id(request.operation_id()?)?,
        name: "Project Atlas æ—¥æœ¬èªž".into(),
        original_scope: SplitGrouping::whole_scope(&plan.remainder),
        destination_scope: SplitGrouping::whole_scope(&plan.destination),
    });
    request.required_policy_contract = 4;
    Ok(())
}
fn rename_original(db: &Connection, request: &SplitRequest) -> Result<()> {
    db.execute(
        "UPDATE index_definitions SET name=?1 WHERE id=?2",
        params![
            request.grouping.as_ref().unwrap().name,
            request.original_index.to_string()
        ],
    )?;
    Ok(())
}
fn install_destination(
    registry: &mut ServiceRegistry,
    request: &SplitRequest,
    baseline: &SourcePolicy,
    plan: &SplitPolicyPlan,
) -> Result<()> {
    // Use the owning split registration path: the index backing foreign key is
    // an actual registered participant, never a synthetic bare index row.
    let accepted = registry.catalog_reserve_split(request, baseline, plan)?;
    registry.catalog_reserve_split_destination(
        accepted.receipt.operation_id,
        accepted.receipt_digest,
    )?;
    // This unit fixture isolates grouping metadata. Supervisor's real split
    // journeys separately prove native build/policy/readiness publication.
    registry.connection.execute(
        "UPDATE index_definitions SET source_import_state='complete' WHERE id=?1",
        [request.destination.index_id.to_string()],
    )?;
    registry.connection.execute(
        "UPDATE index_sources SET enabled=1 WHERE index_id=?1",
        [request.destination.index_id.to_string()],
    )?;
    Ok(())
}
fn count(db: &Connection, table: &str) -> Result<u32> {
    Ok(
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}

#[test]
fn grouped_request_preserves_legacy_bytes_and_rejects_missing_member_scope() -> Result<()> {
    let (_temp, _registry, mut request, baseline, plan) = super::super::tests::fixture()?;
    let legacy = serde_json::to_vec(&request)?;
    assert!(!String::from_utf8(legacy.clone())?.contains("grouping"));
    assert_eq!(
        serde_json::to_vec(&serde_json::from_slice::<SplitRequest>(&legacy)?)?,
        legacy
    );
    grouped(&mut request, &plan)?;
    request.validate_plan(&baseline, &plan)?;
    assert_eq!(
        request.grouping.as_ref().unwrap().original_scope,
        vec![airk_core::index_catalog::ScopeAtom {
            source_root_id: airk_core::index_catalog::SourceRootId::from_identity(
                Incarnation::from_bytes([5; 16])
            ),
            kind: airk_core::index_catalog::PathScope::Subtree,
            path: String::new(),
        }]
    );
    assert_eq!(
        request.grouping.as_ref().unwrap().destination_scope,
        vec![airk_core::index_catalog::ScopeAtom {
            source_root_id: airk_core::index_catalog::SourceRootId::from_identity(
                Incarnation::from_bytes([9; 16])
            ),
            kind: airk_core::index_catalog::PathScope::Subtree,
            path: String::new(),
        }]
    );
    request.grouping.as_mut().unwrap().destination_scope.clear();
    assert!(request.validate_plan(&baseline, &plan).is_err());
    Ok(())
}

#[test]
fn grouped_reservation_replays_exact_identity_and_refuses_name_changes_and_collisions() -> Result<()>
{
    let (_temp, mut registry, mut request, baseline, plan) = super::super::tests::fixture()?;
    grouped(&mut request, &plan)?;
    rename_original(&registry.connection, &request)?;
    let other = QueryProjectId::from_identity(Incarnation::from_bytes([88; 16]));
    registry.connection.execute(
        "INSERT INTO query_projects VALUES(?1,?2,1,1,1)",
        params![other.to_string(), request.grouping.as_ref().unwrap().name],
    )?;
    assert!(
        registry
            .catalog_reserve_split(&request, &baseline, &plan)
            .is_err()
    );
    assert!(
        registry
            .catalog_split_receipt(request.caller, &request.key)?
            .is_none()
    );
    registry.connection.execute(
        "DELETE FROM query_projects WHERE id=?1",
        [other.to_string()],
    )?;
    registry.connection.execute(
        "UPDATE index_definitions SET name='changed' WHERE id=?1",
        [request.original_index.to_string()],
    )?;
    assert!(
        registry
            .catalog_reserve_split(&request, &baseline, &plan)
            .is_err()
    );
    rename_original(&registry.connection, &request)?;
    let first = registry.catalog_reserve_split(&request, &baseline, &plan)?;
    assert_eq!(
        registry.catalog_reserve_split(&request, &baseline, &plan)?,
        first
    );
    assert_eq!(count(&registry.connection, "index_split_operations")?, 1);
    assert_eq!(
        super::super::super::state(&registry.connection)?.policy_contract,
        4
    );
    assert_eq!(count(&registry.connection, "query_projects")?, 0);
    Ok(())
}

#[test]
fn grouped_publication_rolls_back_partial_members_and_replays_without_duplicates() -> Result<()> {
    let (_temp, mut registry, mut request, _baseline, plan) = super::super::tests::fixture()?;
    grouped(&mut request, &plan)?;
    rename_original(&registry.connection, &request)?;
    {
        let tx = registry.connection.transaction()?;
        assert!(publish(&tx, &request, &registry.root).is_err());
        // Dropping the owning transaction undoes the first membership and set.
    }
    assert_eq!(count(&registry.connection, "query_projects")?, 0);
    assert_eq!(count(&registry.connection, "project_index_memberships")?, 0);
    install_destination(&mut registry, &request, &_baseline, &plan)?;
    {
        let tx = registry.connection.transaction()?;
        publish(&tx, &request, &registry.root)?;
        tx.commit()?;
    }
    assert_eq!(count(&registry.connection, "query_projects")?, 1);
    assert_eq!(count(&registry.connection, "project_index_memberships")?, 2);
    // A replay cannot adopt the already-created name or add another member.
    assert!(publish(&registry.connection, &request, &registry.root).is_err());
    assert_eq!(count(&registry.connection, "project_index_memberships")?, 2);
    Ok(())
}

#[test]
fn deleting_split_group_preserves_member_sources_pending_work_and_other_memberships() -> Result<()>
{
    let (_temp, mut registry, mut request, _baseline, plan) = super::super::tests::fixture()?;
    grouped(&mut request, &plan)?;
    rename_original(&registry.connection, &request)?;
    install_destination(&mut registry, &request, &_baseline, &plan)?;
    let original_before = index_record(&registry.connection, request.original_index)?;
    let destination_before = index_record(&registry.connection, request.destination.index_id)?;
    let pending = registry.enqueue(
        request.original_registration.binding,
        request.caller,
        &RequestKey {
            nonce: Incarnation::from_bytes([89; 16]),
            ..request.key.clone()
        },
        JobKind::Inspect,
    )?;
    super::super::super::super::watch::ensure_row(
        &registry.connection,
        request.original_registration.binding,
    )?;
    registry.connection.execute(
        "UPDATE service_watch SET enabled=1 WHERE project=?1",
        [request.original_registration.binding.to_string()],
    )?;
    let root = registry.root.clone();
    let tx = registry.connection.transaction()?;
    publish(&tx, &request, &root)?;
    let other = QueryProjectId::from_identity(Incarnation::from_bytes([90; 16]));
    let change = |operation| CatalogRequest {
        caller: request.caller,
        key: request.key.clone(),
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation,
    };
    super::super::super::operations::mutate(
        &tx,
        &change(CatalogOperation::CreateProject {
            id: other,
            name: "Independent selection".into(),
        }),
        &root,
    )?;
    super::super::super::operations::mutate(
        &tx,
        &change(CatalogOperation::SetMembership {
            membership: Membership {
                project_id: other,
                index_id: request.original_index,
                revision: CatalogRevision::INITIAL,
                scope: request.grouping.as_ref().unwrap().original_scope.clone(),
            },
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_membership_revision: None,
        }),
        &root,
    )?;
    let id = request.grouping.as_ref().unwrap().project_id;
    let removal = change(CatalogOperation::DeleteProject {
        id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_scope_revision: CatalogRevision::INITIAL.next()?.next()?,
        expected_defaults_revision: CatalogRevision::INITIAL,
    });
    super::super::super::operations::mutate(&tx, &removal, &root)?;
    assert!(super::super::super::operations::mutate(&tx, &removal, &root).is_err());
    tx.commit()?;
    assert_eq!(count(&registry.connection, "index_definitions")?, 2);
    assert_eq!(
        index_record(&registry.connection, request.original_index)?,
        original_before
    );
    assert_eq!(
        index_record(&registry.connection, request.destination.index_id)?,
        destination_before
    );
    assert_eq!(count(&registry.connection, "index_sources")?, 2);
    assert_eq!(count(&registry.connection, "project_index_memberships")?, 1);
    assert_eq!(
        registry
            .next_pending_job(request.original_registration.binding)?
            .unwrap()
            .id,
        pending.id
    );
    assert!(registry.connection.query_row(
        "SELECT enabled FROM service_watch WHERE project=?1",
        [request.original_registration.binding.to_string()],
        |row| row.get::<_, bool>(0)
    )?);
    assert_eq!(
        std::fs::read_to_string(
            Path::new(&request.original_registration.root).join("original.md")
        )?,
        "original remainder"
    );
    Ok(())
}

#[test]
fn reviewed_existing_original_set_adds_destination_without_rewriting_original_scope() -> Result<()>
{
    let (_temp, mut registry, mut request, _baseline, plan) = super::super::tests::fixture()?;
    grouped(&mut request, &plan)?;
    rename_original(&registry.connection, &request)?;
    install_destination(&mut registry, &request, &_baseline, &plan)?;
    let id = QueryProjectId::from_identity(Incarnation::from_bytes([91; 16]));
    let root = registry.root.clone();
    let create = CatalogRequest {
        caller: request.caller,
        key: request.key.clone(),
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::CreateProject {
            id,
            name: request.grouping.as_ref().unwrap().name.clone(),
        },
    };
    super::super::super::operations::mutate(&registry.connection, &create, &root)?;
    let original = Membership {
        project_id: id,
        index_id: request.original_index,
        revision: CatalogRevision::INITIAL,
        scope: request.grouping.as_ref().unwrap().original_scope.clone(),
    };
    let member = CatalogRequest {
        operation: CatalogOperation::SetMembership {
            membership: original.clone(),
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_membership_revision: None,
        },
        ..create
    };
    super::super::super::operations::mutate(&registry.connection, &member, &root)?;
    let reviewed = registry
        .catalog_review_split_grouping(request.original_index, request.grouping.take().unwrap())?;
    assert_eq!(reviewed.project_id, id);
    assert!(matches!(
        reviewed.mode,
        SplitGroupingMode::ExistingNamedSet { .. }
    ));
    request.grouping = Some(reviewed);
    let tx = registry.connection.transaction()?;
    publish(&tx, &request, &root)?;
    tx.commit()?;
    assert_eq!(count(&registry.connection, "query_projects")?, 1);
    assert_eq!(count(&registry.connection, "project_index_memberships")?, 2);
    assert_eq!(
        super::super::super::operations::membership(
            &registry.connection,
            &id.to_string(),
            &request.original_index.to_string()
        )?,
        original
    );
    assert!(review(&registry.connection, &request).is_err());
    // Same-name replacement cannot satisfy the saved exact identity/revisions.
    registry
        .connection
        .execute("DELETE FROM query_projects WHERE id=?1", [id.to_string()])?;
    let other = QueryProjectId::from_identity(Incarnation::from_bytes([92; 16]));
    registry.connection.execute(
        "INSERT INTO query_projects VALUES(?1,?2,1,1,1)",
        params![other.to_string(), request.grouping.as_ref().unwrap().name],
    )?;
    assert!(review(&registry.connection, &request).is_err());
    Ok(())
}

#[test]
fn empty_set_deletion_refuses_stale_revision_and_never_touches_indexes() -> Result<()> {
    let (_temp, registry, request, _baseline, _plan) = super::super::tests::fixture()?;
    let id = QueryProjectId::from_identity(Incarnation::from_bytes([93; 16]));
    registry.connection.execute(
        "INSERT INTO query_projects VALUES(?1,'Empty set',2,1,1)",
        [id.to_string()],
    )?;
    let mut removal = CatalogRequest {
        caller: request.caller,
        key: request.key.clone(),
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::DeleteProject {
            id,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
    };
    assert!(
        super::super::super::operations::mutate(&registry.connection, &removal, &registry.root)
            .is_err()
    );
    assert_eq!(count(&registry.connection, "query_projects")?, 1);
    if let CatalogOperation::DeleteProject {
        expected_definition_revision,
        ..
    } = &mut removal.operation
    {
        *expected_definition_revision = CatalogRevision::INITIAL.next()?;
    }
    super::super::super::operations::mutate(&registry.connection, &removal, &registry.root)?;
    assert_eq!(count(&registry.connection, "query_projects")?, 0);
    assert_eq!(count(&registry.connection, "index_definitions")?, 1);
    assert_eq!(count(&registry.connection, "index_sources")?, 1);
    Ok(())
}
