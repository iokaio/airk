// SPDX-License-Identifier: Apache-2.0
//! Metadata-only authority/rebuild tests. The fixture explicitly substitutes the
//! held Current proof; real archive/process qualification remains a separate gate.
use super::*;
fn fixture() -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    NativeStandaloneAuthority,
)> {
    let (temp, registry, native, receipt) = setup::completed_authority_fixture()?;
    Ok((
        temp,
        registry,
        NativeStandaloneAuthority::from_completed_adoption(native, receipt, "airk".into(), false)?,
    ))
}
fn rows(db: &Connection, table: &str) -> Result<Vec<Vec<rusqlite::types::Value>>> {
    let mut q = db.prepare(&format!("SELECT * FROM {table} ORDER BY 1,2"))?;
    let count = q.column_count();
    Ok(q.query_map([], |r| {
        (0..count)
            .map(|i| r.get(i))
            .collect::<rusqlite::Result<Vec<_>>>()
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?)
}
#[test]
fn adopted_authority_preserves_exact_scope_retry_and_original_mapping() -> Result<()> {
    let (_temp, mut registry, native) = fixture()?;
    let host = Sha256Digest::from_bytes([91; 32]);
    let mut checks = 0;
    let grant = registry.import_standalone_authority_checked(&native, host, &mut || {
        checks += 1;
        Ok(())
    })?;
    assert_eq!(checks, 2);
    assert!(matches!(
        grant.provenance,
        GrantProvenance::ImportedStandaloneAdoption { .. }
    ));
    assert_eq!(
        registry.adopted_legacy_registration(
            &native.provenance.plan.original_registration,
            native.provenance.plan.config_sha256,
            host
        )?,
        Some(native.provenance.plan.setup.registration.clone())
    );
    assert!(
        registry
            .adopted_legacy_registration(
                &native.provenance.plan.original_registration,
                Sha256Digest::from_bytes([92; 32]),
                host
            )?
            .is_none()
    );
    assert_eq!(
        registry.adopted_legacy_registration_claim(
            &native.provenance.plan.original_registration,
            native.provenance.plan.config_sha256,
            host,
            Some(native.provenance.plan.origin_sha256)
        )?,
        Some(native.provenance.plan.setup.registration.clone())
    );
    assert!(
        registry
            .adopted_legacy_registration_claim(
                &native.provenance.plan.original_registration,
                native.provenance.plan.config_sha256,
                host,
                Some(Sha256Digest::from_bytes([199; 32]))
            )?
            .is_none()
    );
    let before = registry.catalog_state()?.unwrap().revision;
    assert_eq!(
        registry.import_standalone_authority_checked(&native, host, &mut || Ok(()))?,
        grant
    );
    assert_eq!(registry.catalog_state()?.unwrap().revision, before);
    assert_eq!(
        registry.legacy_compatibility_snapshot(
            native.provenance.plan.setup.registration.binding,
            host,
            before
        )?,
        grant
    );
    let mut selection = airk_core::service_http::NativeImportedHttpSelection {
        namespace: native.provenance.plan.setup.key.namespace,
        expected_catalog_revision: before,
        registration: native.provenance.plan.setup.registration.clone(),
        grant: grant.clone(),
        configuration_receipt: native.provenance.plan.config_sha256,
    };
    selection.validate()?;
    selection.configuration_receipt = Sha256Digest::from_bytes([93; 32]);
    assert!(selection.validate().is_err());
    registry.connection.execute(
        "UPDATE index_access_grants SET revoked=1,revision=revision+1 WHERE id=?1",
        [grant.grant.id.to_string()],
    )?;
    assert!(
        registry
            .legacy_compatibility_snapshot(
                native.provenance.plan.setup.registration.binding,
                host,
                before
            )
            .is_err()
    );
    assert!(
        registry
            .import_standalone_authority_checked(&native, host, &mut || Ok(()))
            .is_err()
    );
    Ok(())
}
#[test]
fn changed_origin_rolls_back_authority_and_physical_promotion() -> Result<()> {
    let (_temp, mut registry, native) = fixture()?;
    let before = super::super::schema_objects(&registry.connection)?;
    let revision = registry.catalog_state()?.unwrap().revision;
    let mut checks = 0;
    assert!(
        registry
            .import_standalone_authority_checked(
                &native,
                Sha256Digest::from_bytes([91; 32]),
                &mut || {
                    checks += 1;
                    ensure!(checks == 1, "origin bytes changed");
                    Ok(())
                }
            )
            .is_err()
    );
    assert_eq!(checks, 2);
    assert_eq!(super::super::schema_objects(&registry.connection)?, before);
    assert_eq!(registry.catalog_state()?.unwrap().revision, revision);
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    Ok(())
}
#[test]
fn adoption_layout_preserves_old_managed_and_reviewed_rows_and_all_policy_shapes() -> Result<()> {
    for policy in 0..=3 {
        let (_temp, mut registry, native) = fixture()?;
        let db = &mut registry.connection;
        let tx = db.transaction()?;
        match policy {
            1 => super::super::apply_policy_v1_layout(&tx)?,
            2 => super::super::apply_split_v2_layout(&tx)?,
            3 => super::super::apply_split_extraction_v3_layout(&tx)?,
            _ => {}
        }
        tx.commit()?;
        let db = &registry.connection;
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([90; 16]));
        db.execute(
            "INSERT INTO query_projects VALUES(?1,'original',1,1,1)",
            [project.to_string()],
        )?;
        db.execute(
            "INSERT INTO legacy_index_projects VALUES(?1,?2,?2)",
            params![
                native
                    .provenance
                    .plan
                    .setup
                    .registration
                    .binding
                    .to_string(),
                project.to_string()
            ],
        )?;
        let host = Sha256Digest::from_bytes([91; 32]);
        let state = registry.catalog_state()?.unwrap();
        let old = GrantProvenance::ImportedLegacySnapshot {
            upgrade_id: state.upgrade_id,
            plan_digest: Sha256Digest::from_bytes([92; 32]),
            legacy_binding: native.provenance.plan.setup.registration.binding,
            configuration_digest: Sha256Digest::from_bytes([93; 32]),
        };
        let imported = native_imported_grant_id(state.namespace, host, &old)?;
        let reviewed = airk_core::grant_provenance::native_reviewed_grant_id(
            Sha256Digest::from_bytes([94; 32]),
        );
        db.execute(
            "INSERT INTO index_access_grants VALUES(?1,?2,3,0,'imported_legacy',?3,?4,?5,?6)",
            params![
                imported.to_string(),
                host.to_string(),
                state.upgrade_id.to_string(),
                Sha256Digest::from_bytes([92; 32]).to_string(),
                native
                    .provenance
                    .plan
                    .setup
                    .registration
                    .binding
                    .to_string(),
                Sha256Digest::from_bytes([93; 32]).to_string()
            ],
        )?;
        db.execute(
            "INSERT INTO index_access_grants VALUES(?1,?2,5,1,'reviewed',NULL,NULL,NULL,NULL)",
            params![reviewed.to_string(), host.to_string()],
        )?;
        for id in [imported, reviewed] {
            db.execute(
                "INSERT INTO index_access_memberships VALUES(?1,?2,?3,7,1,0,NULL)",
                params![
                    id.to_string(),
                    project.to_string(),
                    native.receipt.setup.index_id.to_string()
                ],
            )?;
            db.execute(
                "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','old/path')",
                params![
                    id.to_string(),
                    project.to_string(),
                    native.receipt.setup.index_id.to_string(),
                    native.receipt.setup.source_ids[0].to_string()
                ],
            )?;
        }
        let tables = [
            "index_access_grants",
            "index_access_memberships",
            "index_access_scopes",
        ];
        let before = tables
            .iter()
            .map(|t| rows(db, t))
            .collect::<Result<Vec<_>>>()?;
        let tx = registry.connection.transaction()?;
        super::super::apply_adopted_authority_layout(&tx)?;
        for (table, expected) in tables.iter().zip(&before) {
            assert_eq!(rows(&tx, table)?, *expected);
        }
        let shape = super::super::schema_objects(&tx)?;
        super::super::apply_adopted_authority_layout(&tx)?;
        assert_eq!(super::super::schema_objects(&tx)?, shape);
        tx.commit()?;
        super::super::validate_existing(&registry.connection)?;
        registry
            .connection
            .pragma_update(None, "user_version", 12)?;
        assert!(super::super::validate_existing(&registry.connection).is_err());
        registry
            .connection
            .pragma_update(None, "user_version", 13)?;
        registry
            .connection
            .execute_batch("DROP TRIGGER index_adopted_authority_immutable")?;
        assert!(super::super::validate_existing(&registry.connection).is_err());
    }
    Ok(())
}

#[test]
fn imported_authority_scope_order_is_independent_of_setup_owner_alias_order() {
    use airk_core::index_catalog::SourceRootId;
    let first = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
    let last = SourceRootId::from_identity(Incarnation::from_bytes([254; 16]));
    let sorted = canonical_source_scope(&[first, last]);
    assert_eq!(canonical_source_scope(&[last, first]), sorted);
    assert_eq!(
        sorted.iter().map(|a| a.source_root_id).collect::<Vec<_>>(),
        vec![first, last]
    );
    assert!(
        sorted
            .iter()
            .all(|a| a.kind == PathScope::Subtree && a.path.is_empty())
    );
}

#[test]
fn native_batch_keeps_multiple_configurations_on_one_exact_index_and_grant() -> Result<()> {
    let (_temp, mut registry, mut native) = fixture()?;
    let mut secondary = native.provenance.configuration_claims[0].clone();
    secondary.origin_sha256 = Sha256Digest::from_bytes([102; 32]);
    secondary.config_sha256 = Sha256Digest::from_bytes([103; 32]);
    secondary.selected_entry_sha256 = Sha256Digest::from_bytes([104; 32]);
    secondary.container_path = vec![
        "projects".into(),
        native.provenance.plan.original_registration.root.clone(),
        "mcpServers".into(),
    ];
    secondary.original_server = "historical_alias".into();
    native
        .provenance
        .configuration_claims
        .push(secondary.clone());
    // A second alias in the same original file remains a distinct origin claim,
    // without manufacturing a second read authority for identical saved scope.
    let mut same_file = secondary.clone();
    same_file.origin_sha256 = Sha256Digest::from_bytes([105; 32]);
    same_file.original_server = "second_alias".into();
    native.provenance.configuration_claims.push(same_file);
    native
        .provenance
        .configuration_claims
        .sort_by_key(|c| c.origin_sha256);
    native.revalidate()?;
    let host = Sha256Digest::from_bytes([91; 32]);
    let grant = registry.import_standalone_authority_checked(&native, host, &mut || Ok(()))?;
    for config in [
        native.provenance.plan.config_sha256,
        secondary.config_sha256,
    ] {
        assert_eq!(
            registry.adopted_legacy_registration(
                &native.provenance.plan.original_registration,
                config,
                host
            )?,
            Some(native.provenance.plan.setup.registration.clone())
        );
        airk_core::service_http::NativeImportedHttpSelection {
            namespace: native.provenance.plan.setup.key.namespace,
            expected_catalog_revision: registry.catalog_state()?.unwrap().revision,
            registration: native.provenance.plan.setup.registration.clone(),
            grant: grant.clone(),
            configuration_receipt: config,
        }
        .validate()?;
    }
    let counts:(i64,i64,i64,i64)=registry.connection.query_row("SELECT (SELECT count(*) FROM index_definitions),(SELECT count(*) FROM query_projects),(SELECT count(*) FROM index_access_grants),(SELECT count(*) FROM index_adopted_configuration_claims)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    assert_eq!(counts, (1, 1, 1, 3));
    assert_eq!(grant.grant.memberships.len(), 1);
    assert_eq!(
        registry.adopted_legacy_registration_claim(
            &native.provenance.plan.original_registration,
            native.provenance.plan.config_sha256,
            host,
            Some(native.provenance.plan.origin_sha256)
        )?,
        Some(native.provenance.plan.setup.registration.clone())
    );
    assert!(
        registry
            .adopted_legacy_registration_claim(
                &native.provenance.plan.original_registration,
                native.provenance.plan.config_sha256,
                host,
                Some(Sha256Digest::from_bytes([199; 32]))
            )?
            .is_none()
    );
    let before = registry.catalog_state()?.unwrap().revision;
    assert_eq!(
        registry.import_standalone_authority_checked(&native, host, &mut || Ok(()))?,
        grant
    );
    let mut appended = secondary;
    appended.origin_sha256 = Sha256Digest::from_bytes([106; 32]);
    native.provenance.configuration_claims.push(appended);
    native
        .provenance
        .configuration_claims
        .sort_by_key(|c| c.origin_sha256);
    assert!(
        registry
            .import_standalone_authority_checked(&native, host, &mut || Ok(()))
            .is_err()
    );
    assert_eq!(registry.catalog_state()?.unwrap().revision, before);
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}
#[test]
fn standalone_claims_require_primary_identity_unique_roster_and_same_default() -> Result<()> {
    let (_temp, _registry, native) = fixture()?;
    let mut provenance = native.provenance.clone();
    provenance.configuration_claims[0].config_sha256 = Sha256Digest::from_bytes([106; 32]);
    assert!(provenance.validate().is_err());
    let mut provenance = native.provenance.clone();
    provenance
        .configuration_claims
        .push(provenance.configuration_claims[0].clone());
    assert!(provenance.validate().is_err());
    let mut provenance = native.provenance.clone();
    provenance.configuration_claims[0].no_default_project = !provenance.no_default_project;
    assert!(provenance.validate().is_err());
    let mut provenance = native.provenance.clone();
    provenance.operation_id = Incarnation::from_bytes([107; 16]);
    assert!(provenance.validate().is_err());
    let mut provenance = native.provenance.clone();
    provenance.configuration_claims.clear();
    assert!(provenance.validate().is_err());
    Ok(())
}

#[test]
fn exact_origin_disambiguates_same_file_same_registration_distinct_old_layouts() -> Result<()> {
    let (_temp, mut registry, first) = fixture()?;
    let host = Sha256Digest::from_bytes([91; 32]);
    let first_grant = registry.import_standalone_authority_checked(&first, host, &mut || Ok(()))?;
    let mut plan = (*first.provenance.plan).clone();
    plan.setup.key.nonce = Incarnation::from_bytes([111; 16]);
    plan.setup.expected_catalog_revision = registry.catalog_state()?.unwrap().revision;
    plan.setup.registration = airk_core::service::ProjectRegistration::new(
        plan.setup.registration.root.clone(),
        format!("{}-other-layout", plan.setup.registration.data_root),
        plan.setup.registration.collection.clone(),
    )?;
    plan.origin_sha256 = Sha256Digest::from_bytes([112; 32]);
    plan.original_publication_sha256 = Sha256Digest::from_bytes([113; 32]);
    plan.archive_inventory_sha256 = Sha256Digest::from_bytes([114; 32]);
    let (native, receipt) = setup::complete_metadata_authority_fixture(&mut registry, plan)?;
    let second = NativeStandaloneAuthority::from_completed_adoption(
        native,
        receipt,
        "different_helper_alias".into(),
        false,
    )?;
    let second_grant =
        registry.import_standalone_authority_checked(&second, host, &mut || Ok(()))?;
    assert_ne!(
        first_grant.grant.memberships[0].membership.index_id,
        second_grant.grant.memberships[0].membership.index_id
    );
    let original = &first.provenance.plan.original_registration;
    let config = first.provenance.plan.config_sha256;
    assert_eq!(second.provenance.plan.original_registration, *original);
    assert_eq!(second.provenance.plan.config_sha256, config);
    let error = registry
        .adopted_legacy_registration(original, config, host)
        .unwrap_err();
    assert!(error.downcast_ref::<AmbiguousStandaloneClaim>().is_some());
    for owner in [&first, &second] {
        assert_eq!(
            registry.adopted_legacy_registration_claim(
                original,
                config,
                host,
                Some(owner.provenance.plan.origin_sha256)
            )?,
            Some(owner.provenance.plan.setup.registration.clone())
        );
    }
    assert!(
        registry
            .adopted_legacy_registration_claim(
                original,
                config,
                host,
                Some(Sha256Digest::from_bytes([200; 32]))
            )?
            .is_none()
    );
    Ok(())
}
