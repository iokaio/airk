// SPDX-License-Identifier: Apache-2.0
//! Exact versioned schema identity and bounded schema-12 catalog metadata.
//! Ordinary registry opens never install this schema or migrate user data.
use super::ServiceRegistry;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::CatalogRevision,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(super) const SCHEMA: &str = include_str!("service_catalog_schema.sql");
const AUTHORITY_V1_SCHEMA: &str = include_str!("service_catalog_authority_v1.sql");
const SPLIT_V2_SCHEMA: &str = include_str!("service_catalog_split_v2.sql");
const POLICY_V1_SCHEMA: &str = include_str!("service_catalog_policy_v1.sql");
const LEGACY_SCHEMA: &str = include_str!("service_registry_schema11.sql");
const MAX_SCHEMA_OBJECTS: usize = 512;
const MAX_SCHEMA_BYTES: usize = 512 * 1024;
const MAX_OBJECT_SQL: usize = 16384;
// P6 and P9 must explicitly advance their own required contracts. Older schema-12
// readers must refuse them rather than silently ignore grants or source policy.
pub const SUPPORTED_AUTHORITY_CONTRACT: u32 = 1;
pub const SUPPORTED_POLICY_CONTRACT: u32 = 2;

#[derive(Clone, Copy, Debug)]
pub struct AdmissionPending;
impl std::fmt::Display for AdmissionPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("index catalog upgrade is pending; resume owned installation")
    }
}
impl std::error::Error for AdmissionPending {}

pub fn capabilities() -> airk_core::index_catalog::CatalogCapabilities {
    airk_core::index_catalog::CatalogCapabilities {
        maximum_registry_schema: 12,
        authority_contract: SUPPORTED_AUTHORITY_CONTRACT,
        policy_contract: SUPPORTED_POLICY_CONTRACT,
        production_activation_enabled: upgrade::PRODUCTION_ACTIVATION_ENABLED,
    }
}

#[path = "service_catalog_upgrade.rs"]
pub mod upgrade;

#[path = "service_catalog_genesis.rs"]
pub mod genesis;

#[path = "service_catalog_setup.rs"]
pub mod setup;

#[path = "service_catalog_source_change.rs"]
pub mod source_change;

#[path = "service_catalog_policy.rs"]
pub mod policy;

#[path = "service_catalog_split.rs"]
pub mod split;

#[path = "service_catalog_operations.rs"]
pub mod operations;

#[path = "service_catalog_binding_lookup.rs"]
mod binding_lookup;

#[path = "service_catalog_retirement_evidence.rs"]
mod retirement_evidence;
pub use retirement_evidence::NativeIndexRetirementEvidence;

#[path = "service_catalog_wal.rs"]
pub(crate) mod wal;

#[path = "service_catalog_backup.rs"]
mod backup;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaObject {
    kind: String,
    name: String,
    table: String,
    sql: String,
}

fn schema_objects(connection: &Connection) -> Result<Vec<SchemaObject>> {
    fn text(row: &rusqlite::Row<'_>, column: usize, maximum: usize) -> Result<String> {
        let bytes: Vec<u8> = row.get(column)?;
        ensure!(
            bytes.len() <= maximum,
            "registry schema field exceeds byte bound"
        );
        Ok(String::from_utf8(bytes)?)
    }
    // BLOB substr bounds UTF-8 bytes before allocating Rust strings, including
    // names from a corrupt/foreign schema. Character-count limits are insufficient.
    let mut statement=connection.prepare("SELECT substr(CAST(type AS BLOB),1,33),substr(CAST(name AS BLOB),1,257),substr(CAST(tbl_name AS BLOB),1,257),coalesce(substr(CAST(sql AS BLOB),1,16385),X'') FROM sqlite_schema ORDER BY type,name LIMIT 513")?;
    let mut rows = statement.query([])?;
    let mut objects = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = rows.next()? {
        let object = SchemaObject {
            kind: text(row, 0, 32)?,
            name: text(row, 1, 256)?,
            table: text(row, 2, 256)?,
            sql: text(row, 3, MAX_OBJECT_SQL)?.replace("\r\n", "\n"),
        };
        bytes = bytes
            .checked_add(
                object.sql.len() + object.name.len() + object.table.len() + object.kind.len(),
            )
            .context("registry schema size overflow")?;
        ensure!(
            objects.len() < MAX_SCHEMA_OBJECTS
                && object.sql.len() <= MAX_OBJECT_SQL
                && bytes <= MAX_SCHEMA_BYTES,
            "registry schema exceeds bounded format"
        );
        objects.push(object);
    }
    Ok(objects)
}

/// All interpolated names/conditions below are compiled constants, never input.
fn capacity_triggers(connection: &Connection) -> Result<()> {
    for table in [
        "index_definitions",
        "query_projects",
        "index_contexts",
        "index_access_grants",
        "index_context_imports",
        "index_operation_receipts",
    ] {
        connection.execute_batch(&format!("CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table})>=1024 BEGIN SELECT RAISE(ABORT,'index catalog capacity exceeded'); END;"))?;
    }
    for (table, condition, maximum) in [
        ("index_sources", "index_id=NEW.index_id", 17),
        ("index_source_aliases", "source_id=NEW.source_id", 16),
        ("project_index_memberships", "project_id=NEW.project_id", 32),
        (
            "project_index_scopes",
            "project_id=NEW.project_id AND index_id=NEW.index_id",
            128,
        ),
        ("context_projects", "context_id=NEW.context_id", 32),
        ("context_default_indexes", "context_id=NEW.context_id", 32),
        ("index_access_memberships", "grant_id=NEW.grant_id", 1024),
        (
            "index_access_scopes",
            "grant_id=NEW.grant_id AND project_id=NEW.project_id AND index_id=NEW.index_id",
            128,
        ),
    ] {
        connection.execute_batch(&format!("CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table} WHERE {condition})>={maximum} BEGIN SELECT RAISE(ABORT,'index catalog group capacity exceeded'); END;"))?;
    }
    for column in ["project_id", "index_id"] {
        connection.execute_batch(&format!("CREATE TRIGGER index_access_{column}_capacity BEFORE INSERT ON index_access_memberships WHEN NOT EXISTS(SELECT 1 FROM index_access_memberships WHERE grant_id=NEW.grant_id AND {column}=NEW.{column}) AND (SELECT count(DISTINCT {column}) FROM index_access_memberships WHERE grant_id=NEW.grant_id)>=32 BEGIN SELECT RAISE(ABORT,'grant selection capacity exceeded'); END;"))?;
    }
    Ok(())
}

fn write_barriers(connection: &Connection) -> Result<()> {
    let tables = schema_objects(connection)?
        .into_iter()
        .filter(|object| {
            object.kind == "table"
                && object.name != "index_catalog_meta"
                && !object.name.starts_with("sqlite_")
        })
        .map(|object| object.name)
        .collect::<Vec<_>>();
    for table in tables {
        // Compiled expected schema names only. Do not turn an arbitrary table
        // discovered in a foreign database into executable SQL.
        ensure!(
            table.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "invalid compiled registry table name"
        );
        for action in ["INSERT", "UPDATE", "DELETE"] {
            let suffix = action.to_ascii_lowercase();
            connection.execute_batch(&format!("CREATE TRIGGER catalog_write_{table}_{suffix} AFTER {action} ON {table} WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0) BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;"))?;
        }
    }
    Ok(())
}

pub(super) fn install_schema(connection: &Connection) -> Result<()> {
    validate_shape(connection, 11)?;
    connection.execute_batch(SCHEMA)?;
    capacity_triggers(connection)?;
    write_barriers(connection)
}

fn expected(version: u32) -> Result<&'static Vec<SchemaObject>> {
    static LEGACY: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    static CATALOG: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    ensure!(matches!(version, 11 | 12), "unsupported catalog schema");
    let slot = if version == 11 { &LEGACY } else { &CATALOG };
    slot.get_or_init(|| {
        (|| -> Result<_> {
            let connection = Connection::open_in_memory()?;
            connection.execute_batch(LEGACY_SCHEMA)?;
            if version == 12 {
                connection.execute_batch(SCHEMA)?;
                capacity_triggers(&connection)?;
                write_barriers(&connection)?;
            }
            schema_objects(&connection)
        })()
        .map_err(|error| format!("{error:#}"))
    })
    .as_ref()
    .map_err(|error| anyhow::anyhow!("compiled registry schema is invalid: {error}"))
}

/// Registry version and authority format jointly identify the complete physical
/// catalog layout. Authority 0 remains the exact P5 schema, not a loose subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogLayout {
    Schema11,
    Schema12Authority0,
    Schema12Authority1,
    Schema12Authority1Policy1,
    Schema12Authority1Policy2,
}

fn expected_layout(layout: CatalogLayout) -> Result<&'static Vec<SchemaObject>> {
    static AUTHORITY_V1: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    static POLICY_V1: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    static SPLIT_V2: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    match layout {
        CatalogLayout::Schema11 => expected(11),
        CatalogLayout::Schema12Authority0 => expected(12),
        CatalogLayout::Schema12Authority1
        | CatalogLayout::Schema12Authority1Policy1
        | CatalogLayout::Schema12Authority1Policy2 => {
            (if layout == CatalogLayout::Schema12Authority1 {
                &AUTHORITY_V1
            } else if layout == CatalogLayout::Schema12Authority1Policy1 {
                &POLICY_V1
            } else {
                &SPLIT_V2
            })
            .get_or_init(|| {
                (|| -> Result<_> {
                    let connection = Connection::open_in_memory()?;
                    connection.execute_batch(LEGACY_SCHEMA)?;
                    connection.execute_batch(SCHEMA)?;
                    capacity_triggers(&connection)?;
                    write_barriers(&connection)?;
                    // Exactly the DDL used by owned promotion; no alternative
                    // permissive schema or hand-maintained object roster.
                    connection.execute_batch(AUTHORITY_V1_SCHEMA)?;
                    if layout != CatalogLayout::Schema12Authority1 {
                        connection.execute_batch(POLICY_V1_SCHEMA)?;
                    }
                    if layout == CatalogLayout::Schema12Authority1Policy2 {
                        connection.execute_batch(SPLIT_V2_SCHEMA)?;
                    }
                    schema_objects(&connection)
                })()
                .map_err(|error| format!("{error:#}"))
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!("compiled registry schema is invalid: {error}"))
        }
    }
}

fn physical_catalog_layout(objects: &[SchemaObject]) -> Result<CatalogLayout> {
    if objects == expected_layout(CatalogLayout::Schema12Authority0)?.as_slice() {
        // Preserve the existing authority-0 path: do not build or inspect the
        // authority-1 expected shape until it is actually needed.
        Ok(CatalogLayout::Schema12Authority0)
    } else if objects == expected_layout(CatalogLayout::Schema12Authority1)?.as_slice() {
        Ok(CatalogLayout::Schema12Authority1)
    } else if objects == expected_layout(CatalogLayout::Schema12Authority1Policy1)?.as_slice() {
        Ok(CatalogLayout::Schema12Authority1Policy1)
    } else if objects == expected_layout(CatalogLayout::Schema12Authority1Policy2)?.as_slice() {
        Ok(CatalogLayout::Schema12Authority1Policy2)
    } else {
        anyhow::bail!("unsupported registry table, index, foreign-key or trigger shape")
    }
}

/// Match an entire compiled physical layout before trusting its authority
/// marker. A metadata edit cannot turn a foreign/partial schema into a format.
pub(super) fn validate_catalog_layout(connection: &Connection) -> Result<CatalogLayout> {
    let layout = physical_catalog_layout(&schema_objects(connection)?)?;
    let (authority, policy): (i64, i64) = connection.query_row(
        "SELECT authority_contract,policy_contract FROM index_catalog_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        authority <= i64::from(SUPPORTED_AUTHORITY_CONTRACT)
            && policy <= i64::from(SUPPORTED_POLICY_CONTRACT),
        "index catalog upgrade required: unsupported authority or policy contract"
    );
    ensure!(
        matches!(
            (layout, authority, policy),
            (CatalogLayout::Schema12Authority0, 0, 0)
                | (CatalogLayout::Schema12Authority1, 1, 0)
                | (CatalogLayout::Schema12Authority1Policy1, 1, 1)
                | (CatalogLayout::Schema12Authority1Policy2, 1, 2)
        ),
        "index authority marker does not match the compiled registry layout"
    );
    Ok(layout)
}

/// Policy installation is part of the already fenced catalog transaction. A
/// predecessor sees either the unchanged policy-0 shape or a format it refuses.
pub(super) fn apply_policy_v1_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "policy layout requires an explicit transaction"
    );
    match validate_catalog_layout(connection)? {
        CatalogLayout::Schema12Authority1 => {
            connection.execute_batch(POLICY_V1_SCHEMA)?;
            connection.execute(
                "UPDATE index_catalog_meta SET policy_contract=1,accepted_new_writes=1 WHERE id=1",
                [],
            )?;
        }
        CatalogLayout::Schema12Authority1Policy1 | CatalogLayout::Schema12Authority1Policy2 => {}
        _ => anyhow::bail!("policy installation requires qualified authority 1"),
    }
    validate_catalog_layout(connection)?;
    Ok(())
}

/// Pure DDL stage for an already owned/backed-up promotion transaction. This
/// performs no admission, snapshot sealing, metadata-marker update or activation.
/// The caller must seal and set authority 1 atomically before committing.
/// Install split semantics only inside an explicitly accepted native transaction.
pub(super) fn apply_split_v2_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "split layout requires explicit transaction"
    );
    apply_policy_v1_layout(connection)?;
    if validate_catalog_layout(connection)? == CatalogLayout::Schema12Authority1Policy1 {
        connection.execute_batch(SPLIT_V2_SCHEMA)?;
        connection.execute(
            "UPDATE index_catalog_meta SET policy_contract=2,accepted_new_writes=1 WHERE id=1",
            [],
        )?;
    }
    ensure!(
        validate_catalog_layout(connection)? == CatalogLayout::Schema12Authority1Policy2,
        "split capability layout differs"
    );
    Ok(())
}

pub(super) fn apply_authority_v1_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "index authority layout promotion requires an explicit transaction"
    );
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 12 && validate_catalog_layout(connection)? == CatalogLayout::Schema12Authority0,
        "index authority layout promotion requires exact schema-12 authority 0"
    );
    connection.execute_batch(AUTHORITY_V1_SCHEMA)?;
    ensure!(
        schema_objects(connection)? == *expected_layout(CatalogLayout::Schema12Authority1)?,
        "unsupported registry table, index, foreign-key or trigger shape"
    );
    Ok(())
}

pub(super) fn validate_shape(connection: &Connection, version: u32) -> Result<()> {
    ensure!(
        schema_objects(connection)? == *expected(version)?,
        "unsupported registry table, index, foreign-key or trigger shape"
    );
    Ok(())
}
pub(super) fn schema_digest(connection: &Connection) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/registry-ddl/1",
        &[&serde_json::to_vec(&schema_objects(connection)?)?],
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogState {
    pub schema: u32,
    pub namespace: Incarnation,
    pub revision: CatalogRevision,
    pub activated: bool,
    pub accepted_new_writes: bool,
    pub authority_contract: u32,
    pub policy_contract: u32,
    pub upgrade_id: Incarnation,
    pub legacy_schema_digest: Sha256Digest,
    pub backup_digest: Sha256Digest,
}

pub(super) fn state(connection: &Connection) -> Result<CatalogState> {
    let namespace: String = connection.query_row(
        "SELECT incarnation FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    let raw=connection.query_row("SELECT revision,activated,accepted_new_writes,authority_contract,policy_contract,upgrade_id,legacy_schema_digest,backup_digest FROM index_catalog_meta WHERE id=1",[],|row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,u32>(3)?,row.get::<_,u32>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?))).optional()?.context("catalog upgrade metadata missing")?;
    ensure!(
        matches!(raw.1, 0 | 1) && matches!(raw.2, 0 | 1) && (raw.2 == 0 || raw.1 == 1),
        "invalid catalog activation or write barrier"
    );
    let expected_legacy = framed_digest(
        b"airk/registry-ddl/1",
        &[&serde_json::to_vec(expected(11)?)?],
    );
    ensure!(
        raw.6 == expected_legacy.to_string(),
        "catalog legacy schema identity is unsupported"
    );
    ensure!(
        (0..=SUPPORTED_AUTHORITY_CONTRACT).contains(&raw.3)
            && (0..=SUPPORTED_POLICY_CONTRACT).contains(&raw.4),
        "upgrade required for index authority or source-policy contract"
    );
    Ok(CatalogState {
        schema: 12,
        namespace: namespace.parse()?,
        revision: u64::try_from(raw.0)?.try_into()?,
        activated: raw.1 == 1,
        accepted_new_writes: raw.2 == 1,
        authority_contract: raw.3,
        policy_contract: raw.4,
        upgrade_id: raw.5.parse()?,
        legacy_schema_digest: raw.6.parse()?,
        backup_digest: raw.7.parse()?,
    })
}

pub(super) fn validate_existing(connection: &Connection) -> Result<()> {
    let application: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        application == super::APPLICATION && version == 12,
        "unsupported index catalog format"
    );
    let pages: i64 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    ensure!(
        size == 4096 && pages > 0 && pages <= (super::MAX_DATABASE_BYTES / 4096) as i64,
        "index catalog exceeds registry bound"
    );
    validate_catalog_layout(connection)?;
    state(connection)?;
    Ok(())
}

impl ServiceRegistry {
    /// Administrative metadata only. Does not activate, import, grant or index.
    pub fn catalog_state(&self) -> Result<Option<CatalogState>> {
        let version: u32 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < 12 {
            return Ok(None);
        }
        validate_existing(&self.connection)?;
        Ok(Some(state(&self.connection)?))
    }
}

#[cfg(test)]
#[path = "service_catalog_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "service_catalog_layout_tests.rs"]
mod layout_tests;
