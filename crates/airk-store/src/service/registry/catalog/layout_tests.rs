// SPDX-License-Identifier: Apache-2.0
//! Physical-format fixtures only; these do not qualify native activation or
//! substitute for held-owner backup, migration, readiness or sealing evidence.
use super::*;
use airk_core::service::ProjectRegistration;
use rusqlite::params;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry)> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let legacy = schema_digest(&registry.connection)?;
    let tx = registry.connection.transaction()?;
    install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            legacy.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    tx.pragma_update(None, "user_version", 12)?;
    tx.commit()?;
    Ok((root, registry))
}

fn promote_layout(registry: &mut ServiceRegistry) -> Result<()> {
    let tx = registry.connection.transaction()?;
    apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    assert_eq!(
        validate_catalog_layout(&tx)?,
        CatalogLayout::Schema12Authority1
    );
    tx.commit()?;
    Ok(())
}

fn insert_binding(connection: &Connection, number: u128) -> Result<(String, String)> {
    let registration = ProjectRegistration::new(
        format!("/layout/source-{number}"),
        format!("/layout/data-{number}"),
        "layout".into(),
    )?;
    let index = format!(
        "idx_{}",
        Incarnation::from_bytes((number + 1).to_be_bytes())
    );
    let project = format!(
        "prj_{}",
        Incarnation::from_bytes((number + 1).to_be_bytes())
    );
    connection.execute(
        "INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4','layout','Layout fixture',1,1,0,NULL,0,'complete')",
        params![index, registration.binding.to_string(), registration.root, registration.data_root],
    )?;
    connection.execute(
        "INSERT INTO legacy_index_projects VALUES(?1,NULL,?2)",
        params![registration.binding.to_string(), project],
    )?;
    Ok((index, registration.binding.to_string()))
}

fn imported(connection: &Connection, number: u128, binding: &str) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest)
        VALUES(?1,?2,1,0,'imported_legacy',?3,?4,?5,?6)",
        params![format!("grt_8{number:031x}"), Sha256Digest::from_bytes([4;32]).to_string(), Incarnation::from_bytes([2;16]).to_string(), Sha256Digest::from_bytes([5;32]).to_string(), binding, Sha256Digest::from_bytes([6;32]).to_string()],
    )
}

fn reviewed(connection: &Connection, number: u128) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked) VALUES(?1,?2,1,0)",
        params![
            format!("grt_{number:032x}"),
            Sha256Digest::from_bytes([4; 32]).to_string()
        ],
    )
}

#[test]
fn authority_layout_preserves_exact_legacy_shapes_and_never_promotes_on_read() -> Result<()> {
    let root = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    assert_eq!(
        schema_objects(&registry.connection)?,
        *expected_layout(CatalogLayout::Schema11)?
    );
    assert!(validate_existing(&registry.connection).is_err());
    assert!(registry.catalog_state()?.is_none());
    let (_root, registry) = fixture()?;
    let before = schema_objects(&registry.connection)?;
    assert_eq!(before, *expected(12)?);
    assert_eq!(before, *expected_layout(CatalogLayout::Schema12Authority0)?);
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12Authority0
    );
    validate_shape(&registry.connection, 12)?;
    validate_existing(&registry.connection)?;
    assert!(apply_authority_v1_layout(&registry.connection).is_err());
    assert_eq!(schema_objects(&registry.connection)?, before);
    Ok(())
}

#[test]
fn authority_promotion_ddl_and_marker_rollback_atomically() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let before = schema_objects(&registry.connection)?;
    {
        let tx = registry.connection.transaction()?;
        apply_authority_v1_layout(&tx)?;
        assert!(
            validate_shape(&tx, 12).is_err(),
            "P5 validation remains exact authority 0"
        );
        assert!(
            validate_catalog_layout(&tx).is_err(),
            "marker must not lag the physical layout at admission"
        );
        tx.execute(
            "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
            [],
        )?;
        assert_eq!(
            validate_catalog_layout(&tx)?,
            CatalogLayout::Schema12Authority1
        );
        // Simulate failure before commit: rollback both schema and marker.
    }
    assert_eq!(schema_objects(&registry.connection)?, before);
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12Authority0
    );
    Ok(())
}

#[test]
fn authority_layout_requires_matching_marker_and_rejects_foreign_objects() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    registry.connection.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    assert!(validate_catalog_layout(&registry.connection).is_err());
    {
        let tx = registry.connection.transaction()?;
        assert!(
            apply_authority_v1_layout(&tx).is_err(),
            "promotion cannot repair a forged marker by accepting a different layout"
        );
        assert_eq!(schema_objects(&tx)?, *expected(12)?);
    }
    registry.connection.execute(
        "UPDATE index_catalog_meta SET authority_contract=0 WHERE id=1",
        [],
    )?;
    promote_layout(&mut registry)?;
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12Authority1
    );
    assert_eq!(
        validate_existing(&registry.connection).is_ok(),
        SUPPORTED_AUTHORITY_CONTRACT >= 1
    );
    registry
        .connection
        .execute_batch("CREATE INDEX foreign_catalog_index ON index_access_grants(revoked);")?;
    assert!(validate_catalog_layout(&registry.connection).is_err());
    assert!(validate_existing(&registry.connection).is_err());
    Ok(())
}

#[test]
fn authority_grant_provenance_is_complete_reserved_and_immutable() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    promote_layout(&mut registry)?;
    let (_index, binding) = insert_binding(&registry.connection, 0)?;
    assert!(reviewed(&registry.connection, 1u128 << 127).is_err());
    assert!(registry.connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind) VALUES(?1,?2,1,0,'imported_legacy')",
        params![format!("grt_8{:031x}", 0), Sha256Digest::from_bytes([4;32]).to_string()],
    ).is_err());
    assert!(registry.connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest)
        VALUES(?1,?2,1,0,'imported_legacy',?3,?4,?5,?6)",
        params![format!("grt_{:032x}", 2), Sha256Digest::from_bytes([4;32]).to_string(), Incarnation::from_bytes([2;16]).to_string(), Sha256Digest::from_bytes([5;32]).to_string(), binding, Sha256Digest::from_bytes([6;32]).to_string()],
    ).is_err(), "complete imported provenance still requires its reserved ID partition");
    assert!(registry.connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest)
        VALUES(?1,?2,1,0,'imported_legacy',?3,?4,?5,?6)",
        params![format!("grt_8{:031x}", 2), Sha256Digest::from_bytes([4;32]).to_string(), Incarnation::from_bytes([2;16]).to_string(), Sha256Digest::from_bytes([5;32]).to_string(), binding, "g".repeat(64)],
    ).is_err(), "invalid configuration digest is not an import seal");
    imported(&registry.connection, 0, &binding)?;
    reviewed(&registry.connection, 1)?;
    assert!(
        imported(&registry.connection, 1, &binding).is_err(),
        "one sealed record per original binding"
    );
    for column in ["host", "provenance_kind", "import_configuration_digest"] {
        assert!(
            registry
                .connection
                .execute(
                    &format!("UPDATE index_access_grants SET {column}={column} WHERE id=?1"),
                    [format!("grt_8{:031x}", 0)],
                )
                .is_err(),
            "{column} cannot be rewritten even to the same value"
        );
    }
    assert_eq!(
        registry.connection.execute(
            "UPDATE index_access_grants SET revision=2,revoked=1 WHERE id=?1",
            [format!("grt_8{:031x}", 0)],
        )?,
        1
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |row| row
                .get::<_, i64>(0))?,
        2
    );
    Ok(())
}

#[test]
fn authority_grant_classes_have_separate_caps_and_retain_real_aggregate_quota() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    promote_layout(&mut registry)?;
    let mut indexes = Vec::new();
    {
        let tx = registry.connection.transaction()?;
        for number in 0u128..1024 {
            let (index, binding) = insert_binding(&tx, number)?;
            imported(&tx, number, &binding)?;
            reviewed(&tx, number + 1)?;
            indexes.push(index);
        }
        let imported_error = imported(
            &tx,
            1024,
            &ProjectRegistration::new(
                "/layout/source-0".into(),
                "/layout/data-0".into(),
                "layout".into(),
            )?
            .binding
            .to_string(),
        )
        .unwrap_err();
        assert!(
            imported_error
                .to_string()
                .contains("imported compatibility grant capacity exceeded")
        );
        let reviewed_error = reviewed(&tx, 1025).unwrap_err();
        assert!(
            reviewed_error
                .to_string()
                .contains("reviewed project grant capacity exceeded")
        );
        tx.commit()?;
    }
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |row| row
                .get::<_, i64>(0))?,
        2048
    );
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "max_page_count", |row| row.get::<_, i64>(0))?,
        8192
    );
    let mut full = false;
    {
        let tx = registry.connection.transaction()?;
        let suffix = "x".repeat(32700);
        'fill: for (number, index) in indexes.iter().enumerate() {
            for source in 0..17 {
                let id = format!(
                    "src_{}",
                    Incarnation::from_bytes(
                        (1_000_000u128 + (number * 17 + source) as u128).to_be_bytes()
                    )
                );
                match tx.execute(
                    "INSERT INTO index_sources VALUES(?1,?2,?3,?4,1)",
                    params![
                        id,
                        index,
                        format!("/layout/fill/{number}/{source}/{suffix}"),
                        source == 0
                    ],
                ) {
                    Ok(_) => {}
                    Err(error)
                        if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull) =>
                    {
                        full = true;
                        break 'fill;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        assert!(
            full,
            "authority-1 fixtures must reach the actual unchanged 32 MiB quota"
        );
    }
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_sources", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |row| row
                .get::<_, i64>(0))?,
        2048
    );
    assert!(
        registry
            .connection
            .pragma_query_value(None, "page_count", |row| row.get::<_, i64>(0))?
            <= 8192
    );
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12Authority1
    );
    backup::integrity(&registry.connection)?;
    Ok(())
}
