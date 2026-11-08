// SPDX-License-Identifier: Apache-2.0
//! Native, transactional physical adoption-authority promotion. Reads only recognize it.
use super::*;
const SCHEMA: &str = include_str!("service_catalog_adopted_authority.sql");
const LAYOUTS: [CatalogLayout; 4] = [
    CatalogLayout::Schema12Authority1,
    CatalogLayout::Schema12Authority1Policy1,
    CatalogLayout::Schema12Authority1Policy2,
    CatalogLayout::Schema12Authority1Policy3,
];
fn apply_raw(db: &Connection, layout: CatalogLayout) -> Result<()> {
    // Restore only compiled triggers lost with the rebuilt tables. Never execute
    // schema text supplied by a database, even after a shape check.
    let compiled = expected_layout(layout)?;
    db.execute_batch(SCHEMA)?;
    for object in compiled.iter().filter(|o| {
        o.kind == "trigger"
            && matches!(
                o.table.as_str(),
                "index_access_grants" | "index_access_memberships" | "index_access_scopes"
            )
    }) {
        let present: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND name=?1)",
            [&object.name],
            |r| r.get(0),
        )?;
        if !present {
            db.execute_batch(&object.sql)?;
        }
    }
    db.execute_batch("CREATE TRIGGER index_adopted_authorities_capacity BEFORE INSERT ON index_adopted_authorities WHEN (SELECT count(*) FROM index_adopted_authorities)>=1024 BEGIN SELECT RAISE(ABORT,'adopted authority capacity exceeded'); END;")?;
    for table in [
        "index_adopted_authorities",
        "index_adopted_configuration_claims",
    ] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            let suffix = action.to_ascii_lowercase();
            db.execute_batch(&format!("CREATE TRIGGER catalog_write_{table}_{suffix} AFTER {action} ON {table} WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0) BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;"))?;
        }
    }
    Ok(())
}
fn expected_adopted(layout: CatalogLayout) -> Result<&'static Vec<SchemaObject>> {
    static SLOTS: [OnceLock<Result<Vec<SchemaObject>, String>>; 4] = [const { OnceLock::new() }; 4];
    let n = LAYOUTS
        .iter()
        .position(|v| *v == layout)
        .context("adoption requires authority one")?;
    SLOTS[n]
        .get_or_init(|| {
            (|| -> Result<_> {
                let db = Connection::open_in_memory()?;
                db.execute_batch(LEGACY_SCHEMA)?;
                db.execute_batch(super::SCHEMA)?;
                capacity_triggers(&db)?;
                write_barriers(&db)?;
                db.execute_batch(AUTHORITY_V1_SCHEMA)?;
                if n >= 1 {
                    db.execute_batch(POLICY_V1_SCHEMA)?;
                }
                if n >= 2 {
                    db.execute_batch(SPLIT_V2_SCHEMA)?;
                }
                if n >= 3 {
                    db.execute_batch(SPLIT_V3_SCHEMA)?;
                }
                apply_raw(&db, layout)?;
                schema_objects(&db)
            })()
            .map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("compiled adopted schema is invalid: {e}"))
}
pub(super) fn recognize(objects: &[SchemaObject]) -> Result<Option<CatalogLayout>> {
    if !objects
        .iter()
        .any(|o| o.kind == "table" && o.name == "index_adopted_authorities")
    {
        return Ok(None);
    }
    for layout in LAYOUTS {
        if objects == expected_adopted(layout)?.as_slice() {
            return Ok(Some(layout));
        }
    }
    Ok(None)
}
pub(super) fn apply_adopted_authority_layout(db: &Connection) -> Result<()> {
    ensure!(
        !db.is_autocommit(),
        "adoption authority layout requires native transaction"
    );
    let layout = validate_catalog_layout(db)?;
    super::super::runtime_contract::arm_in_transaction(db)?;
    if recognize(&schema_objects(db)?)?.is_some() {
        return Ok(());
    }
    ensure!(LAYOUTS.contains(&layout), "adoption requires authority one");
    apply_raw(db, layout)?;
    ensure!(
        schema_objects(db)? == *expected_adopted(layout)?,
        "adoption layout differs from compiled identity"
    );
    let foreign_key_failure: bool = db
        .prepare("PRAGMA foreign_key_check")?
        .query([])?
        .next()?
        .is_some();
    ensure!(
        !foreign_key_failure,
        "adoption authority foreign key validation failed"
    );
    validate_catalog_layout(db)?;
    Ok(())
}
