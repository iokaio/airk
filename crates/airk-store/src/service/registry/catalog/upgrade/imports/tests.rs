// SPDX-License-Identifier: Apache-2.0
use super::super::tests::{fixture, plan};
use super::*;
use airk_core::index_catalog::{CatalogRevision, ContextChoices};

fn request(plan: &UpgradePlan) -> ContextImport {
    ContextImport {
        schema: 1,
        key: Sha256Digest::from_bytes([10; 32]),
        expected_catalog_revision: CatalogRevision::INITIAL,
        context: ContextChoices {
            id: ContextId::from_identity(Incarnation::from_bytes([11; 16])),
            host: Sha256Digest::from_bytes([12; 32]),
            key: Sha256Digest::from_bytes([13; 32]),
            kind: ContextKind::Workspace,
            name: "Explicit workspace fixture".into(),
            revision: CatalogRevision::INITIAL,
            project_ids: vec![plan.indexes[0].project_id],
            index_ids: vec![plan.indexes[0].index_id],
        },
    }
}

#[test]
fn exact_import_replays_once_and_does_not_recreate_a_deleted_association() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan.clone())?;
    owner.migrate()?;
    let request = request(&plan);
    let applied = owner.import_context(&request)?;
    assert_eq!(applied.catalog_revision.get(), 2);
    assert_eq!(owner.import_context(&request)?, applied);
    let read = registry_read(root.path())?;
    assert_eq!(
        read.query_row("SELECT count(*) FROM index_contexts", [], |row| row
            .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        read.query_row("SELECT count(*) FROM index_access_grants", [], |row| row
            .get::<_, i64>(0))?,
        0,
        "preferences grant no access"
    );
    drop(read);
    let connection = Connection::open(root.path().join(DATABASE))?;
    connection.execute_batch("PRAGMA foreign_keys=ON; DELETE FROM index_contexts;")?;
    drop(connection);
    let replay = owner.import_context(&request)?;
    assert_eq!(replay.context_id, applied.context_id);
    assert_eq!(replay.catalog_revision, applied.catalog_revision);
    assert!(!replay.context_present);
    assert_eq!(
        registry_read(root.path())?.query_row(
            "SELECT count(*) FROM index_contexts",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    Ok(())
}

#[test]
fn reused_keys_stale_reviews_and_outside_defaults_fail_without_partial_imports() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan.clone())?;
    owner.migrate()?;
    let mut request = request(&plan);
    owner.import_context(&request)?;
    request.context.name.push_str(" changed");
    assert!(owner.import_context(&request).is_err());
    request.key = Sha256Digest::from_bytes([20; 32]);
    request.context.id = ContextId::from_identity(Incarnation::from_bytes([21; 16]));
    request.context.key = Sha256Digest::from_bytes([22; 32]);
    assert!(
        owner.import_context(&request).is_err(),
        "stale revision cannot be applied"
    );
    request.expected_catalog_revision = 2.try_into()?;
    request.context.index_ids = vec![IndexId::from_identity(Incarnation::from_bytes([23; 16]))];
    assert!(
        owner.import_context(&request).is_err(),
        "outside defaults cannot be substituted"
    );
    request.context.index_ids.clear();
    request.context.project_ids = vec![QueryProjectId::from_identity(Incarnation::from_bytes(
        [24; 16],
    ))];
    assert!(
        owner.import_context(&request).is_err(),
        "missing Project cannot be guessed"
    );
    let read = registry_read(root.path())?;
    assert_eq!(
        read.query_row("SELECT count(*) FROM index_context_imports", [], |row| row
            .get::<_, i64>(
            0
        ))?,
        1
    );
    assert_eq!(super::super::super::state(&read)?.revision.get(), 2);
    Ok(())
}

#[test]
fn missing_editor_state_stays_pending_without_inferred_workspace_grouping() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan.clone())?;
    owner.migrate()?;
    let mut request = request(&plan);
    request.context.kind = ContextKind::LegacyPending;
    assert!(owner.import_context(&request).is_err());
    request.context.project_ids.clear();
    request.context.index_ids.clear();
    owner.import_context(&request)?;
    let read = registry_read(root.path())?;
    assert_eq!(
        read.query_row("SELECT count(*) FROM context_projects", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        read.query_row("SELECT count(*) FROM context_default_indexes", [], |row| {
            row.get::<_, i64>(0)
        })?,
        0
    );
    assert_eq!(
        read.query_row("SELECT count(*) FROM index_access_grants", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}
