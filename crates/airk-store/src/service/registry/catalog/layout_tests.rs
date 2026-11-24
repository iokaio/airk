// SPDX-License-Identifier: Apache-2.0
//! Physical-format fixtures only; these do not qualify native activation or
//! substitute for held-owner backup, migration, readiness or sealing evidence.
use super::*;
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
    assert_eq!(validate_catalog_layout(&tx)?, CatalogLayout::Schema12);
    tx.commit()?;
    Ok(())
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
    assert_eq!(before, *expected_layout(CatalogLayout::Schema12)?);
    assert_eq!(
        validate_catalog_layout(&registry.connection)?,
        CatalogLayout::Schema12
    );
    validate_shape(&registry.connection, 12)?;
    validate_existing(&registry.connection)?;
    assert!(apply_authority_v1_layout(&registry.connection).is_err());
    assert_eq!(schema_objects(&registry.connection)?, before);
    Ok(())
}

/// Change-log entry 68 part 3: a fresh catalog never creates the removed
/// tables, and authority 1 adds no physical object.
#[test]
fn fresh_terminal_layout_never_creates_the_removed_tables() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let tables = |connection: &Connection| -> Result<Vec<String>> {
        Ok(schema_objects(connection)?
            .into_iter()
            .filter(|object| object.kind == "table")
            .map(|object| object.name)
            .collect())
    };
    for removed in grant_removal::REMOVED_TABLES {
        assert!(!tables(&registry.connection)?.contains(&removed.to_owned()));
    }
    let before = schema_objects(&registry.connection)?;
    promote_layout(&mut registry)?;
    assert_eq!(schema_objects(&registry.connection)?, before);
    for removed in grant_removal::REMOVED_TABLES {
        assert!(!tables(&registry.connection)?.contains(&removed.to_owned()));
        assert!(!SCHEMA.contains(&format!("CREATE TABLE {removed}(")));
    }
    Ok(())
}

#[test]
fn authority_promotion_marker_rolls_back_atomically() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let before = schema_objects(&registry.connection)?;
    {
        let tx = registry.connection.transaction()?;
        apply_authority_v1_layout(&tx)?;
        tx.execute(
            "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
            [],
        )?;
        assert_eq!(validate_catalog_layout(&tx)?, CatalogLayout::Schema12);
        assert_eq!(markers(&tx)?.0, 1);
        // Simulate failure before commit: rollback the marker.
    }
    assert_eq!(schema_objects(&registry.connection)?, before);
    assert_eq!(markers(&registry.connection)?.0, 0);
    Ok(())
}

#[test]
fn authority_layout_requires_matching_marker_and_rejects_foreign_objects() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    registry.connection.execute(
        "UPDATE index_catalog_meta SET policy_contract=1 WHERE id=1",
        [],
    )?;
    assert!(validate_catalog_layout(&registry.connection).is_err());
    registry.connection.execute(
        "UPDATE index_catalog_meta SET policy_contract=0 WHERE id=1",
        [],
    )?;
    registry.connection.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    {
        let tx = registry.connection.transaction()?;
        assert!(
            apply_authority_v1_layout(&tx).is_err(),
            "promotion starts only from exact authority 0"
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
        CatalogLayout::Schema12
    );
    assert_eq!(
        validate_existing(&registry.connection).is_ok(),
        SUPPORTED_AUTHORITY_CONTRACT >= 1
    );
    registry
        .connection
        .execute_batch("CREATE INDEX foreign_catalog_index ON index_definitions(name);")?;
    assert!(validate_catalog_layout(&registry.connection).is_err());
    assert!(validate_existing(&registry.connection).is_err());
    Ok(())
}
