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
    use airk_core::{
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        index_catalog::*,
    };
    use rusqlite::{Connection, params};
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
    let digest = Sha256Digest::from_bytes([24; 32]);
    tx.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,1,0,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    let index = IndexId::from_identity(Incarnation::from_bytes([25; 16]));
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([26; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([27; 16]));
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
    tx.commit()?;
    drop(sql);
    Ok((
        owner,
        ServiceRegistry::open(&root, instance)?,
        registration,
        grant,
    ))
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
