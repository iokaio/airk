// SPDX-License-Identifier: Apache-2.0
//! Native compatibility metadata handler tests; no installer qualification.
use super::*;
use airk_core::{catalog_host::NativeLegacyBindingEligibility, service::ProjectRegistration};

fn imported_fixture(
    foreign_host: bool,
) -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    ProjectRegistration,
    airk_core::grant_provenance::PersistedGrant,
)> {
    use rusqlite::Connection;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    crate::local_ipc::prepare_root(&root)?;
    let instance = Incarnation::from_bytes([22; 16]);
    let registry = crate::catalog_test_fixture::authority_one(&root, instance)?;
    let namespace = registry.incarnation()?;
    let registration = ProjectRegistration::new(
        owner.path().join("source").to_str().unwrap().into(),
        owner.path().join("data").to_str().unwrap().into(),
        "imported-observation".into(),
    )?;
    drop(registry);
    // Honest physical-format component fixture. It does not claim to exercise
    // native promotion readiness, the installer or a real managed activation.
    let sql = Connection::open(root.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    let upgrade: String = tx.query_row(
        "SELECT upgrade_id FROM index_catalog_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    let upgrade: Incarnation = upgrade.parse()?;
    let grant = insert_imported_fixture(&tx, namespace, upgrade, &registration, foreign_host, 0)?;
    tx.commit()?;
    drop(sql);
    Ok((
        owner,
        ServiceRegistry::open(&root, instance)?,
        registration,
        grant,
    ))
}

fn insert_imported_fixture(
    tx: &rusqlite::Transaction<'_>,
    namespace: Incarnation,
    upgrade: Incarnation,
    registration: &ProjectRegistration,
    foreign_host: bool,
    offset: u8,
) -> Result<airk_core::grant_provenance::PersistedGrant> {
    use airk_core::{
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        index_catalog::*,
    };
    use rusqlite::params;
    let digest = Sha256Digest::from_bytes([24; 32]);
    tx.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,1,0,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    let index = IndexId::from_identity(Incarnation::from_bytes([25 + offset; 16]));
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([26 + offset; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([27 + offset; 16]));
    tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?5,1,1,0,NULL,0,'complete')",params![index.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
    tx.execute(
        "INSERT INTO query_projects VALUES(?1,'Imported native handler fixture',1,1,1)",
        [project.to_string()],
    )?;
    tx.execute(
        "INSERT INTO legacy_index_projects VALUES(?1,?2,?2)",
        params![registration.binding.to_string(), project.to_string()],
    )?;
    tx.execute(
        "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
        params![project.to_string(), index.to_string()],
    )?;
    tx.execute(
        "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
        params![source.to_string(), index.to_string(), registration.root],
    )?;
    tx.execute(
        "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
        params![project.to_string(), index.to_string(), source.to_string()],
    )?;
    let host = if foreign_host {
        Sha256Digest::from_bytes([28; 32])
    } else {
        current_native_host(namespace)?
    };
    let provenance = GrantProvenance::ImportedLegacySnapshot {
        upgrade_id: upgrade,
        plan_digest: digest,
        legacy_binding: registration.binding,
        configuration_digest: digest,
    };
    let grant = PersistedGrant {
        grant: ReviewedGrant {
            id: native_imported_grant_id(namespace, host, &provenance)?,
            host,
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        provenance,
    };
    grant.validate()?;
    tx.execute(
        "INSERT INTO index_access_grants VALUES(?1,?2,1,0,'imported_legacy',?3,?4,?5,?4)",
        params![
            grant.grant.id.to_string(),
            host.to_string(),
            upgrade.to_string(),
            digest.to_string(),
            registration.binding.to_string()
        ],
    )?;
    tx.execute(
        "INSERT INTO index_access_memberships VALUES(?1,?2,?3,1,1,0,NULL)",
        params![
            grant.grant.id.to_string(),
            project.to_string(),
            index.to_string()
        ],
    )?;
    tx.execute(
        "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','')",
        params![
            grant.grant.id.to_string(),
            project.to_string(),
            index.to_string(),
            source.to_string()
        ],
    )?;
    Ok(grant)
}

#[tokio::test]
async fn imported_http_union_preserves_all_bindings_and_requires_explicit_project() -> Result<()> {
    use crate::runtime_install::http_legacy::{LegacyMigrationResolution, resolve};
    use airk_core::{
        grant_provenance::GrantProvenance,
        service_http::{HttpSelection, NativeImportedHttpUnion},
    };
    use airk_mcp::read_attempt::ReadAttemptContext;
    use std::time::{Duration, Instant};
    let (owner, mut registry, first, grant) = imported_fixture(false)?;
    let control = owner.path().join("runtime");
    let second = ProjectRegistration::new(
        owner
            .path()
            .join("second-source")
            .to_string_lossy()
            .into_owned(),
        owner
            .path()
            .join("second-data")
            .to_string_lossy()
            .into_owned(),
        "second".into(),
    )?;
    let GrantProvenance::ImportedLegacySnapshot { upgrade_id, .. } = grant.provenance else {
        panic!("import fixture");
    };
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    let second_grant =
        insert_imported_fixture(&tx, registry.incarnation()?, upgrade_id, &second, false, 30)?;
    tx.commit()?;
    let mut selections = Vec::new();
    for (offset, registration) in [first.clone(), second.clone()].into_iter().enumerate() {
        let LegacyMigrationResolution::Eligible { selection } = resolve(
            &mut registry,
            registration,
            Sha256Digest::from_bytes([90 + offset as u8; 32]),
        )?
        else {
            panic!("exact imported fixture");
        };
        selections.push(*selection);
    }
    let union = NativeImportedHttpUnion {
        namespace: registry.incarnation()?,
        selections,
        no_default_project: true,
    };
    union.validate()?;
    let mut duplicate = union.clone();
    duplicate.selections[1] = duplicate.selections[0].clone();
    assert!(duplicate.validate().is_err());
    let mut crossed = union.clone();
    crossed.selections[1].namespace = Incarnation::from_bytes([99; 16]);
    assert!(crossed.validate().is_err());
    let mut narrowed = union.clone();
    narrowed.no_default_project = false;
    assert!(narrowed.validate().is_err());
    let selection = HttpSelection::ImportedLegacyUnion { selection: union };
    let context =
        || ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    let factory =
        crate::http_backend::factory(control.clone(), selection.clone(), context()).await?;
    let backend = factory()?;
    let projects = backend.call("projects".into(), Default::default()).await?;
    let projects: serde_json::Value =
        serde_json::from_str(projects["content"][0]["text"].as_str().unwrap())?;
    assert!(projects["default_project"].is_null());
    let bindings = projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["project"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let first_id = first.binding.to_string();
    let second_id = second.binding.to_string();
    assert_eq!(
        bindings,
        std::collections::BTreeSet::from([first_id.as_str(), second_id.as_str()])
    );
    let ambiguous = backend
        .call(
            "airk_search".into(),
            serde_json::json!({"query":"original"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(ambiguous["isError"], true);
    assert!(ambiguous.to_string().contains("PROJECT_REQUIRED"));
    let foreign = backend
        .call(
            "airk_search".into(),
            serde_json::json!({"query":"original","project":"ff".repeat(32)})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(foreign["isError"], true);
    sql.execute(
        "UPDATE index_access_grants SET revoked=1,revision=2 WHERE id=?1",
        [second_grant.grant.id.to_string()],
    )?;
    assert!(
        crate::http_backend::validate(&control, &selection, &context())
            .await
            .is_err(),
        "one revoked member refuses the entire original group, not a narrower first-root credential"
    );
    backend.shutdown().await?;
    Ok(())
}

#[test]
fn http_migration_retains_imported_scope_and_never_mints_reviewed_authority() -> Result<()> {
    use crate::runtime_install::http_legacy::{LegacyMigrationResolution, resolve, validate};
    for foreign in [false, true] {
        let (owner, mut registry, registration, original) = imported_fixture(foreign)?;
        let receipt = Sha256Digest::from_bytes([93; 32]);
        let resolution = resolve(&mut registry, registration.clone(), receipt)?;
        if foreign {
            assert!(matches!(
                resolution,
                LegacyMigrationResolution::ReviewRequired { .. }
            ));
        } else {
            let LegacyMigrationResolution::Eligible { selection } = resolution else {
                panic!("existing exact imported authority must remain eligible");
            };
            assert_eq!(selection.registration, registration);
            assert_eq!(selection.grant, original);
            assert_eq!(selection.configuration_receipt, receipt);
            assert!(
                matches!(
                    crate::runtime_install::http_legacy::resolve_claim(
                        &mut registry,
                        registration.clone(),
                        receipt,
                        Some(Sha256Digest::from_bytes([95; 32]))
                    )?,
                    LegacyMigrationResolution::ReviewRequired { .. }
                ),
                "unknown explicit standalone claim fell back to unrelated existing imported authority"
            );
            let control = owner.path().join("runtime");
            validate(&control, &selection)?;
            let mut crossed = selection.clone();
            crossed.namespace = Incarnation::from_bytes([94; 16]);
            assert!(validate(&control, &crossed).is_err());
            crossed = selection.clone();
            crossed.grant.provenance =
                airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant;
            assert!(crossed.validate().is_err());
            let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
            sql.execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
            assert!(validate(&control, &selection).is_err());
            assert!(matches!(
                resolve(&mut registry, registration, receipt)?,
                LegacyMigrationResolution::ReviewRequired { .. }
            ));
            assert_eq!(
                sql.query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        }
    }
    Ok(())
}

#[test]
fn imported_native_handler_preserves_full_snapshot_and_refuses_changed_authority() -> Result<()> {
    for change in ["none", "membership", "revoked", "foreign_host"] {
        let (owner, mut registry, registration, grant) =
            imported_fixture(change == "foreign_host")?;
        let sql = rusqlite::Connection::open(owner.path().join("runtime/service-1.4.sqlite3"))?;
        match change {
            "membership" => {
                sql.execute("UPDATE project_index_memberships SET revision=2", [])?;
            }
            "revoked" => {
                sql.execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
            }
            _ => {}
        }
        let observation = legacy_binding(
            &mut registry,
            registration.binding,
            Some(CatalogRevision::INITIAL),
        )?;
        if change == "none" {
            #[derive(serde::Serialize)]
            #[serde(tag = "kind", rename_all = "snake_case")]
            enum UnboxedEligibility<'a> {
                ImportedSnapshot {
                    registration: &'a ProjectRegistration,
                    grant: &'a airk_core::grant_provenance::PersistedGrant,
                },
            }
            assert_eq!(
                serde_json::to_vec(&observation.eligibility)?,
                serde_json::to_vec(&UnboxedEligibility::ImportedSnapshot {
                    registration: &registration,
                    grant: &grant,
                })?,
            );
            assert_eq!(
                observation.eligibility,
                NativeLegacyBindingEligibility::ImportedSnapshot {
                    registration,
                    grant: Box::new(grant)
                }
            );
            let response =
                airk_core::service::ControlResponse::CatalogLegacyBinding { observation };
            let bytes = serde_json::to_vec(&response)?;
            let restored: airk_core::service::ControlResponse = serde_json::from_slice(&bytes)?;
            assert_eq!(
                serde_json::to_value(restored)?,
                serde_json::to_value(response)?
            );
        } else {
            assert_eq!(
                observation.eligibility,
                NativeLegacyBindingEligibility::Unavailable
            );
        }
        assert_eq!(
            sql.query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                .get::<_, i64>(0))?,
            1,
            "observation must not create or reseal grants"
        );
    }
    Ok(())
}

#[tokio::test]
async fn imported_http_factory_is_lazy_and_revalidates_revocation() -> Result<()> {
    use crate::runtime_install::http_legacy::{LegacyMigrationResolution, resolve};
    use airk_core::service_http::HttpSelection;
    use airk_mcp::read_attempt::ReadAttemptContext;
    use std::time::{Duration, Instant};
    let (owner, mut registry, registration, _) = imported_fixture(false)?;
    let LegacyMigrationResolution::Eligible { selection } = resolve(
        &mut registry,
        registration,
        Sha256Digest::from_bytes([93; 32]),
    )?
    else {
        panic!("expected imported scope");
    };
    let selection = HttpSelection::ImportedLegacy { selection };
    let control = owner.path().join("runtime");
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    let factory = crate::http_backend::factory(control.clone(), selection.clone(), context).await?;
    let first = factory()?;
    let second = factory()?;
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert!(!first.catalog().is_empty());
    first.close();
    first.shutdown().await?;
    // Factory/catalog creation needs no live service or worker and changes no grant.
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    assert!(
        crate::http_backend::validate(&control, &selection, &context)
            .await
            .is_err()
    );
    let context = ReadAttemptContext::new(Instant::now(), Default::default());
    assert!(
        crate::http_backend::validate(&control, &selection, &context)
            .await
            .is_err()
    );
    assert_eq!(
        sql.query_row("SELECT count(*) FROM index_access_grants", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    second.shutdown().await?;
    Ok(())
}

#[test]
fn legacy_eligibility_observes_exact_registration_without_initializing_catalog() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    crate::local_ipc::prepare_root(&root)?;
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([19; 16]))?;
    // These paths intentionally do not exist: this observation is metadata only.
    let registration = ProjectRegistration::new(
        owner.path().join("source").to_str().unwrap().into(),
        owner.path().join("data").to_str().unwrap().into(),
        "legacy-observation".into(),
    )?;
    registry.register(&registration)?;
    let migration = crate::runtime_install::http_legacy::resolve(
        &mut registry,
        registration.clone(),
        Sha256Digest::from_bytes([93; 32]),
    )?;
    assert!(matches!(
        migration,
        crate::runtime_install::http_legacy::LegacyMigrationResolution::ReviewRequired { .. }
    ));
    let observed = legacy_binding(&mut registry, registration.binding, None)?;
    observed.validate()?;
    assert_eq!(observed.namespace, registry.incarnation()?);
    assert_eq!(observed.catalog_revision, None);
    assert_eq!(
        observed.eligibility,
        NativeLegacyBindingEligibility::AuthorityZero {
            registration: registration.clone()
        }
    );
    let stale = legacy_binding(
        &mut registry,
        registration.binding,
        Some(CatalogRevision::INITIAL),
    )?;
    assert_eq!(
        stale.eligibility,
        NativeLegacyBindingEligibility::Unavailable
    );
    let missing = legacy_binding(&mut registry, Sha256Digest::from_bytes([20; 32]), None)?;
    assert_eq!(
        missing.eligibility,
        NativeLegacyBindingEligibility::Unavailable
    );
    assert!(registry.catalog_state()?.is_none());
    assert!(!owner.path().join("source").exists());
    Ok(())
}

#[test]
fn corrupt_native_registration_is_not_negative_permission_evidence() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    crate::local_ipc::prepare_root(&root)?;
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([21; 16]))?;
    let registration = ProjectRegistration::new(
        owner.path().join("source").to_str().unwrap().into(),
        owner.path().join("data").to_str().unwrap().into(),
        "legacy-corrupt".into(),
    )?;
    registry.register(&registration)?;
    let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
    sql.execute(
        "UPDATE projects SET payload=?1 WHERE binding=?2",
        rusqlite::params![b"{".as_slice(), registration.binding.to_string()],
    )?;
    assert!(legacy_binding(&mut registry, registration.binding, None).is_err());
    Ok(())
}
