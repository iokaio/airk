// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    catalog_management::*,
    identity::Sha256Digest,
    index_catalog::*,
    service::{ProjectRegistration, RequestKey},
};
use rusqlite::params;

/// Activated schema-12 authority-1 catalog with one index and one set.
fn fixture(authority: bool) -> Result<(tempfile::TempDir, ServiceRegistry)> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let legacy = schema_digest(&registry.connection)?;
    let tx = registry.connection.transaction()?;
    install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,1,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            legacy.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    tx.pragma_update(None, "user_version", 12)?;
    if authority {
        apply_authority_v1_layout(&tx)?;
        tx.execute(
            "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
            [],
        )?;
    }
    let backing = ProjectRegistration::new(
        "/fixture/source".into(),
        "/fixture/data".into(),
        "described".into(),
    )?;
    tx.execute("INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4',?5,'Fixture',1,1,0,NULL,0,'complete')",
        params![index().to_string(),backing.binding.to_string(),backing.root,backing.data_root,backing.collection])?;
    tx.execute(
        "INSERT INTO query_projects VALUES(?1,'Fixture set',1,1,1)",
        [project().to_string()],
    )?;
    tx.commit()?;
    Ok((root, registry))
}
fn index() -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([20; 16]))
}
fn project() -> QueryProjectId {
    QueryProjectId::from_identity(Incarnation::from_bytes([6; 16]))
}
fn request(
    registry: &ServiceRegistry,
    nonce: u8,
    operation: CatalogOperation,
) -> Result<CatalogRequest> {
    let (namespace, epoch): (String, i64) = registry.connection.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(CatalogRequest {
        caller: Incarnation::from_bytes([5; 16]),
        key: RequestKey {
            namespace: namespace.parse()?,
            epoch: u64::try_from(epoch)?,
            nonce: Incarnation::from_bytes([nonce; 16]),
        },
        expected_catalog_revision: state(&registry.connection)?.revision,
        operation,
    })
}
fn apply(registry: &mut ServiceRegistry, request: &CatalogRequest) -> Result<CatalogReceipt> {
    let preview = registry.catalog_preview(request)?;
    registry.catalog_apply(request, preview.identity()?)
}
fn run(
    registry: &mut ServiceRegistry,
    nonce: u8,
    operation: CatalogOperation,
) -> Result<CatalogReceipt> {
    let request = request(registry, nonce, operation)?;
    apply(registry, &request)
}
fn set_index(revision: u64, description: Option<&str>) -> Result<CatalogOperation> {
    Ok(CatalogOperation::SetIndexDescription {
        id: index(),
        expected_definition_revision: revision.try_into()?,
        description: description.map(Into::into),
    })
}
fn set_project(revision: u64, description: Option<&str>) -> Result<CatalogOperation> {
    Ok(CatalogOperation::SetProjectDescription {
        id: project(),
        expected_definition_revision: revision.try_into()?,
        description: description.map(Into::into),
    })
}
fn described(registry: &mut ServiceRegistry) -> Result<(IndexDefinition, QueryProject)> {
    let Some(CatalogItem::Index(index)) = registry
        .catalog_detail(&CatalogEntity::Index { id: index() }, None)?
        .item
    else {
        anyhow::bail!("index detail missing");
    };
    let Some(CatalogItem::Project(project)) = registry
        .catalog_detail(&CatalogEntity::Project { id: project() }, None)?
        .item
    else {
        anyhow::bail!("project detail missing");
    };
    Ok((index, project))
}

#[test]
fn previews_and_clears_never_install_the_layer() -> Result<()> {
    let (_root, mut registry) = fixture(true)?;
    let before = schema_objects(&registry.connection)?;
    let set = request(&registry, 7, set_index(1, Some("Docs\nand code"))?)?;
    let preview = registry.catalog_preview(&set)?;
    let [CatalogItem::Index(after)] = preview.after.as_slice() else {
        anyhow::bail!("preview target missing");
    };
    assert_eq!(after.description.as_deref(), Some("Docs\nand code"));
    assert_eq!(after.definition_revision.get(), 2);
    assert_eq!(schema_objects(&registry.connection)?, before);
    // A clear on an untouched installation fences and bumps the revision
    // but keeps the exact physical identity.
    run(&mut registry, 8, set_index(1, None)?)?;
    run(&mut registry, 9, set_project(1, None)?)?;
    assert_eq!(schema_objects(&registry.connection)?, before);
    let (saved_index, saved_set) = described(&mut registry)?;
    assert_eq!(
        (
            saved_index.description.clone(),
            saved_index.definition_revision.get()
        ),
        (None, 2)
    );
    assert_eq!(
        (saved_set.description, saved_set.definition_revision.get()),
        (None, 2)
    );
    // Encodings of undescribed items carry no description field.
    assert!(!serde_json::to_string(&saved_index)?.contains("description"));
    Ok(())
}

#[test]
fn first_write_installs_the_layer_and_null_clears() -> Result<()> {
    let (_root, mut registry) = fixture(true)?;
    let base = schema_objects(&registry.connection)?;
    let receipt = run(&mut registry, 7, set_index(1, Some("Index notes"))?)?;
    assert!(!receipt.authority_fence_required);
    let installed = schema_objects(&registry.connection)?;
    assert_eq!(strip(&installed)?.as_ref(), Some(&base));
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12Authority1
    );
    validate_existing(&registry.connection)?;
    run(&mut registry, 8, set_project(1, Some("Set notes"))?)?;
    let (saved_index, saved_set) = described(&mut registry)?;
    assert_eq!(saved_index.description.as_deref(), Some("Index notes"));
    assert_eq!(saved_set.description.as_deref(), Some("Set notes"));
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 8,
        max_bytes: 4096,
    })?;
    assert_eq!(page.items, vec![CatalogItem::Index(saved_index.clone())]);
    // Replace, then clear with null; the layer stays installed.
    run(&mut registry, 9, set_index(2, Some("Replaced"))?)?;
    assert_eq!(
        described(&mut registry)?.0.description.as_deref(),
        Some("Replaced")
    );
    run(&mut registry, 10, set_index(3, None)?)?;
    let (saved_index, _) = described(&mut registry)?;
    assert_eq!(
        (
            saved_index.description.clone(),
            saved_index.definition_revision.get()
        ),
        (None, 4)
    );
    assert_eq!(schema_objects(&registry.connection)?, installed);
    // Deleting a set removes its description with it.
    run(
        &mut registry,
        11,
        CatalogOperation::DeleteProject {
            id: project(),
            expected_definition_revision: 2u64.try_into()?,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
    )?;
    let rows: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM catalog_descriptions", [], |r| {
                r.get(0)
            })?;
    assert_eq!(rows, 0);
    Ok(())
}

#[test]
fn stale_tombstoned_and_invalid_edits_refuse_without_effect() -> Result<()> {
    let (_root, mut registry) = fixture(true)?;
    run(&mut registry, 7, set_index(1, Some("First"))?)?;
    let installed = schema_objects(&registry.connection)?;
    let before = state(&registry.connection)?;
    // The definition revision fences the edit.
    let stale = request(&registry, 8, set_index(1, Some("Stale"))?)?;
    assert!(registry.catalog_preview(&stale).is_err());
    let stale = request(&registry, 9, set_project(2, Some("Stale"))?)?;
    assert!(registry.catalog_preview(&stale).is_err());
    // The core bound refuses before the store: blank, control, oversize.
    for text in ["  ", "tab\there", &"x".repeat(MAX_DESCRIPTION_BYTES + 1)] {
        assert!(
            request(&registry, 10, set_index(2, Some(text))?)?
                .validate()
                .is_err()
        );
    }
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=1 WHERE id=?1",
        [index().to_string()],
    )?;
    let retired = request(&registry, 11, set_index(2, Some("Retired"))?)?;
    assert!(registry.catalog_preview(&retired).is_err());
    assert_eq!(state(&registry.connection)?, before);
    assert_eq!(schema_objects(&registry.connection)?, installed);
    Ok(())
}

#[test]
fn authority_zero_refuses_and_a_partial_layer_is_unsupported() -> Result<()> {
    let (_root, mut registry) = fixture(false)?;
    let before = schema_objects(&registry.connection)?;
    let set = request(&registry, 7, set_index(1, Some("Notes"))?)?;
    assert!(registry.catalog_preview(&set).is_err());
    assert_eq!(schema_objects(&registry.connection)?, before);
    // A layer missing any compiled object is not a recognized format; the
    // same check makes a predecessor refuse the complete layer.
    let (_root, registry) = fixture(true)?;
    let tx = registry.connection.unchecked_transaction()?;
    apply_layout(&tx)?;
    let installed = schema_objects(&tx)?;
    for object in layer()? {
        let partial = installed
            .iter()
            .filter(|candidate| *candidate != object)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            physical_catalog_layout(&partial).is_err(),
            "{}",
            object.name
        );
    }
    let foreign = installed
        .iter()
        .cloned()
        .map(|mut object| {
            if object.name == "catalog_descriptions" {
                object.sql = object.sql.replace("1024", "4096");
            }
            object
        })
        .collect::<Vec<_>>();
    assert!(physical_catalog_layout(&foreign).is_err());
    Ok(())
}
