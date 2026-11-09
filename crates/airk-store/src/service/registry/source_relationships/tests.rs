// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, Vec<ProjectRegistration>)> {
    let owner = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(owner.path(), Incarnation::from_bytes([1; 16]))?;
    let mut registrations = Vec::new();
    for (number, root) in [
        "/fictional/repo",
        "/fictional/repo/src",
        "/fictional/repo-other",
        "/fictional/removed",
    ]
    .into_iter()
    .enumerate()
    {
        let registration = ProjectRegistration::new(
            root.into(),
            format!("/fictional/private/{number}"),
            format!("fixture-{number}"),
        )?;
        registry.connection.execute(
            "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,0,0,?3)",
            params![
                registration.binding.to_string(),
                serde_json::to_vec(&registration)?,
                number != 3
            ],
        )?;
        registrations.push(registration);
    }
    Ok((owner, registry, registrations))
}
fn request(root: &str) -> SourceRelationshipsRequest {
    SourceRelationshipsRequest {
        root: root.into(),
        bindings: None,
        after: None,
        limit: 64,
        max_bytes: MAX_RELATIONSHIP_BYTES,
    }
}

fn install_catalog(
    registry: &mut ServiceRegistry,
    registrations: &[ProjectRegistration],
) -> Result<()> {
    let legacy = super::super::catalog::schema_digest(&registry.connection)?;
    let tx = registry.connection.transaction()?;
    super::super::catalog::install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            legacy.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    for (number, registration) in registrations.iter().enumerate() {
        let index = IndexId::from_identity(Incarnation::from_bytes([10 + number as u8; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([30 + number as u8; 16]));
        tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Fixture',1,1,0,NULL,0,?6)",params![index.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection,if number==1 {"pending_metadata"} else {"complete"}])?;
        tx.execute(
            "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
            params![source.to_string(), index.to_string(), registration.root],
        )?;
        if number == 0 {
            tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,'/fictional/shared',0,0)",
                params![
                    SourceRootId::from_identity(Incarnation::from_bytes([60; 16])).to_string(),
                    index.to_string()
                ],
            )?;
        }
    }
    tx.execute("UPDATE index_catalog_meta SET activated=1", [])?;
    tx.pragma_update(None, "user_version", 12)?;
    tx.commit()?;
    Ok(())
}

#[test]
fn catalog_sources_keep_root_ids_disabled_retention_and_pending_imports_explicit() -> Result<()> {
    let (_owner, mut registry, registrations) = fixture()?;
    install_catalog(&mut registry, &registrations)?;
    let page = registry.source_relationships(&request("/fictional"))?;
    assert_eq!(page.basis, RootObservationBasis::CatalogSources);
    assert_eq!(page.inspected_roots, 5);
    assert_eq!(page.unknown_assignment_indexes, 1);
    assert!(
        page.relationships
            .iter()
            .all(|row| row.index_id.is_some() && row.source_root_id.is_some())
    );
    let disabled = page
        .relationships
        .iter()
        .find(|row| row.root == "/fictional/shared")
        .unwrap();
    assert_eq!(disabled.enabled, Some(false));
    let removed = page
        .relationships
        .iter()
        .find(|row| row.binding == registrations[3].binding)
        .unwrap();
    assert!(!removed.registered);
    registry.connection.execute(
        "DELETE FROM index_sources WHERE index_id=?1",
        [disabled.index_id.unwrap().to_string()],
    )?;
    registry.connection.execute(
        "DELETE FROM index_definitions WHERE id=?1",
        [disabled.index_id.unwrap().to_string()],
    )?;
    assert!(
        registry
            .source_relationships(&request("/fictional"))
            .is_err(),
        "unmapped legacy registrations cannot be hidden by an incomplete catalog"
    );
    Ok(())
}

#[test]
fn root_relationships_use_components_include_retained_roots_and_never_open_sources() -> Result<()> {
    let (owner, registry, registrations) = fixture()?;
    let before = fs::read(owner.path().join("service-1.4.sqlite3"))?;
    let page = registry.source_relationships(&request("/fictional/repo"))?;
    assert_eq!(page.matching_roots, 2);
    assert_eq!(page.inspected_roots, 4);
    assert_eq!(page.unknown_assignment_indexes, 4);
    assert_eq!(page.basis, RootObservationBasis::LegacyOwnerAndWatchCache);
    assert!(
        page.relationships
            .iter()
            .any(|row| row.binding == registrations[0].binding
                && row.relation == SourceRelation::Same)
    );
    assert!(
        page.relationships
            .iter()
            .any(|row| row.binding == registrations[1].binding
                && row.relation == SourceRelation::Contains)
    );
    let child = registry.source_relationships(&request("/fictional/repo/src/deeper"))?;
    assert!(
        child
            .relationships
            .iter()
            .all(|row| row.relation == SourceRelation::ContainedBy)
    );
    let removed = registry.source_relationships(&request("/fictional/removed"))?;
    assert_eq!(removed.matching_roots, 1);
    assert!(!removed.relationships[0].registered);
    assert_eq!(fs::read(owner.path().join("service-1.4.sqlite3"))?, before);
    Ok(())
}

#[test]
fn scope_and_cursor_changes_refuse_instead_of_expanding_or_restarting_a_page() -> Result<()> {
    let (_owner, registry, registrations) = fixture()?;
    let mut input = request("/fictional");
    input.limit = 1;
    let first = registry.source_relationships(&input)?;
    assert!(first.truncated);
    assert_eq!(first.matching_roots, 4);
    input.after = first.after;
    let second = registry.source_relationships(&input)?;
    assert_ne!(
        first.relationships[0].binding,
        second.relationships[0].binding
    );
    let mut changed = input.clone();
    changed.root = "/fictional/repo".into();
    assert!(registry.source_relationships(&changed).is_err());
    changed = input.clone();
    changed.bindings = Some(vec![registrations[0].binding]);
    assert!(registry.source_relationships(&changed).is_err());
    let selected = registry.source_relationships(&SourceRelationshipsRequest {
        bindings: Some(vec![registrations[0].binding]),
        after: None,
        ..input.clone()
    })?;
    assert_eq!(selected.inspected_roots, 1);
    assert_eq!(selected.matching_roots, 1);
    registry.connection.execute(
        "UPDATE projects SET registered=0 WHERE binding=?1",
        [registrations[0].binding.to_string()],
    )?;
    assert!(registry.source_relationships(&input).is_err());
    changed = input;
    changed.after = None;
    changed.bindings = Some(vec![]);
    assert!(registry.source_relationships(&changed).is_err());
    changed.bindings = Some(vec![Sha256Digest::from_bytes([99; 32])]);
    assert!(registry.source_relationships(&changed).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn source_relationships_preserve_native_ordinal_case_and_path_component_rules() -> Result<()> {
    assert_eq!(
        source_relation(
            Path::new(r"C:\Copper\SRC"),
            Path::new(r"c:\copper\src\file")
        )?,
        Some(SourceRelation::Contains)
    );
    assert_eq!(
        source_relation(
            Path::new(r"C:\Copper\src"),
            Path::new(r"c:\copper\src-other")
        )?,
        None
    );
    assert_eq!(
        source_relation(Path::new(r"C:\Örebro"), Path::new(r"c:\örebro"))?,
        Some(SourceRelation::Same)
    );
    Ok(())
}
