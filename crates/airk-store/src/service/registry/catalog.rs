// SPDX-License-Identifier: Apache-2.0
//! Exact versioned schema identity and bounded schema-12/13 catalog metadata.
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

pub(super) const SCHEMA: &str = include_str!("../../service_catalog_schema.sql");
/// Frozen predecessor DDL, executed only in memory to recognize a layout the
/// upgrade step moves to the terminal one (change-log entry 68 part 3).
const V12_SCHEMA: &str = include_str!("../../service_catalog_schema_v12.sql");
const AUTHORITY_V1_SCHEMA: &str = include_str!("../../service_catalog_authority_v1.sql");
const SPLIT_V3_SCHEMA: &str = include_str!("../../service_catalog_split_extraction_v3.sql");
const SPLIT_V2_SCHEMA: &str = include_str!("../../service_catalog_split_v2.sql");
const POLICY_V1_SCHEMA: &str = include_str!("../../service_catalog_policy_v1.sql");
const LEGACY_SCHEMA: &str = include_str!("../../service_registry_schema11.sql");
const MAX_SCHEMA_OBJECTS: usize = 512;
const MAX_SCHEMA_BYTES: usize = 512 * 1024;
const MAX_OBJECT_SQL: usize = 16384;
// P6 and P9 must explicitly advance their own required contracts. Older schema-12
// readers must refuse them rather than silently ignore source policy.
pub const SUPPORTED_AUTHORITY_CONTRACT: u32 = 1;
pub const SUPPORTED_POLICY_CONTRACT: u32 = 4;

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
        maximum_registry_schema: airk_core::index_catalog::REGISTRY_SCHEMA,
        authority_contract: SUPPORTED_AUTHORITY_CONTRACT,
        policy_contract: SUPPORTED_POLICY_CONTRACT,
        reference_refresh_contract: Some(1),
        // Wire field: catalog activation is always available to this runtime.
        production_activation_enabled: true,
    }
}

pub mod upgrade;

pub mod genesis;

pub mod setup;

pub mod source_change;

pub mod policy;

pub mod split;

mod adopted_layout;
mod descriptions;
pub(super) mod grant_removal;

pub mod operations;

mod binding_lookup;

mod retirement_evidence;
pub use retirement_evidence::NativeIndexRetirementEvidence;
// Bounded operation records (record cleanups of 23 November 2026, items 1-3).
pub(super) mod record_pruning;

pub(crate) mod wal;

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
    capacity_triggers_for(connection, false)
}

/// The capacity triggers of the frozen predecessor layouts: the current set
/// plus those of the reviewed-grant and context-import tables.
fn predecessor_capacity_triggers(connection: &Connection) -> Result<()> {
    capacity_triggers_for(connection, true)
}

fn capacity_triggers_for(connection: &Connection, predecessor: bool) -> Result<()> {
    let mut tables = vec!["index_definitions", "query_projects", "index_contexts"];
    if predecessor {
        tables.extend(["index_access_grants", "index_context_imports"]);
    }
    tables.push("index_operation_receipts");
    for table in tables {
        connection.execute_batch(&format!("CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table})>=1024 BEGIN SELECT RAISE(ABORT,'index catalog capacity exceeded'); END;"))?;
    }
    let mut groups = vec![
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
    ];
    if predecessor {
        groups.extend([
            ("index_access_memberships", "grant_id=NEW.grant_id", 1024),
            (
                "index_access_scopes",
                "grant_id=NEW.grant_id AND project_id=NEW.project_id AND index_id=NEW.index_id",
                128,
            ),
        ]);
    }
    for (table, condition, maximum) in groups {
        connection.execute_batch(&format!("CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table} WHERE {condition})>={maximum} BEGIN SELECT RAISE(ABORT,'index catalog group capacity exceeded'); END;"))?;
    }
    if predecessor {
        for column in ["project_id", "index_id"] {
            connection.execute_batch(&format!("CREATE TRIGGER index_access_{column}_capacity BEFORE INSERT ON index_access_memberships WHEN NOT EXISTS(SELECT 1 FROM index_access_memberships WHERE grant_id=NEW.grant_id AND {column}=NEW.{column}) AND (SELECT count(DISTINCT {column}) FROM index_access_memberships WHERE grant_id=NEW.grant_id)>=32 BEGIN SELECT RAISE(ABORT,'grant selection capacity exceeded'); END;"))?;
        }
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

/// Installs the terminal layout directly: a fresh catalog never creates the
/// tables change-log entry 68 part 3 removed.
pub(super) fn install_schema(connection: &Connection) -> Result<()> {
    validate_shape(connection, 11)?;
    connection.execute_batch(SCHEMA)?;
    capacity_triggers(connection)?;
    write_barriers(connection)
}

fn expected(version: u32) -> Result<&'static Vec<SchemaObject>> {
    static LEGACY: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    ensure!(matches!(version, 11..=13), "unsupported catalog schema");
    if version == 11 {
        return LEGACY
            .get_or_init(|| {
                (|| -> Result<_> {
                    let connection = Connection::open_in_memory()?;
                    connection.execute_batch(LEGACY_SCHEMA)?;
                    schema_objects(&connection)
                })()
                .map_err(|error| format!("{error:#}"))
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!("compiled registry schema is invalid: {error}"));
    }
    expected_layout(CatalogLayout::Schema12)
}

/// The physical catalog layout. Registry version and the authority and
/// policy markers jointly identify the complete format; authority 0 and 1
/// share one physical shape since the grant tables were removed (change-log
/// entry 68 part 3), so the marker alone distinguishes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogLayout {
    Schema11,
    /// No source-policy layer: authority 0 (genesis) or authority 1.
    Schema12,
    Schema12Policy1,
    Schema12Policy2,
    Schema12Policy3,
}

/// The frozen layouts before change-log entry 68 part 3, recognized only so
/// that the upgrade step can move them to the terminal layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::service::registry) enum PredecessorLayout {
    Authority0,
    Authority1,
    Policy1,
    Policy2,
    Policy3,
}
impl PredecessorLayout {
    pub(super) const ALL: [Self; 5] = [
        Self::Authority0,
        Self::Authority1,
        Self::Policy1,
        Self::Policy2,
        Self::Policy3,
    ];
    /// The terminal layout the upgrade step produces from this one.
    pub(super) const fn terminal(self) -> CatalogLayout {
        match self {
            Self::Authority0 | Self::Authority1 => CatalogLayout::Schema12,
            Self::Policy1 => CatalogLayout::Schema12Policy1,
            Self::Policy2 => CatalogLayout::Schema12Policy2,
            Self::Policy3 => CatalogLayout::Schema12Policy3,
        }
    }
}

/// An existing catalog still in a layout before change-log entry 68 part 3.
/// Only the read-write `ServiceRegistry::open` upgrade step moves it to the
/// terminal layout; every other reader refuses it with this typed error and
/// never decodes its records.
#[derive(Clone, Copy, Debug)]
pub struct CatalogUpgradeRequired;
impl std::fmt::Display for CatalogUpgradeRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "index catalog upgrade required: open the registry read-write once to remove reviewed grants",
        )
    }
}
impl std::error::Error for CatalogUpgradeRequired {}

fn compiled(
    slot: &'static OnceLock<Result<Vec<SchemaObject>, String>>,
    build: impl FnOnce(&Connection) -> Result<()>,
) -> Result<&'static Vec<SchemaObject>> {
    slot.get_or_init(|| {
        (|| -> Result<_> {
            let connection = Connection::open_in_memory()?;
            connection.execute_batch(LEGACY_SCHEMA)?;
            build(&connection)?;
            schema_objects(&connection)
        })()
        .map_err(|error| format!("{error:#}"))
    })
    .as_ref()
    .map_err(|error| anyhow::anyhow!("compiled registry schema is invalid: {error}"))
}

/// The policy and split layers shared by the terminal and predecessor chains.
fn policy_layers(connection: &Connection, level: u8) -> Result<()> {
    if level >= 1 {
        connection.execute_batch(POLICY_V1_SCHEMA)?;
    }
    if level >= 2 {
        connection.execute_batch(SPLIT_V2_SCHEMA)?;
    }
    if level >= 3 {
        connection.execute_batch(SPLIT_V3_SCHEMA)?;
    }
    Ok(())
}

fn expected_layout(layout: CatalogLayout) -> Result<&'static Vec<SchemaObject>> {
    static SLOTS: [OnceLock<Result<Vec<SchemaObject>, String>>; 4] = [const { OnceLock::new() }; 4];
    let level = match layout {
        CatalogLayout::Schema11 => return expected(11),
        CatalogLayout::Schema12 => 0,
        CatalogLayout::Schema12Policy1 => 1,
        CatalogLayout::Schema12Policy2 => 2,
        CatalogLayout::Schema12Policy3 => 3,
    };
    // Exactly the DDL used by install and owned promotion; no alternative
    // permissive schema or hand-maintained object roster.
    compiled(&SLOTS[usize::from(level)], |connection| {
        connection.execute_batch(SCHEMA)?;
        capacity_triggers(connection)?;
        write_barriers(connection)?;
        policy_layers(connection, level)
    })
}

fn expected_predecessor(layout: PredecessorLayout) -> Result<&'static Vec<SchemaObject>> {
    static SLOTS: [OnceLock<Result<Vec<SchemaObject>, String>>; 5] = [const { OnceLock::new() }; 5];
    let (slot, level) = match layout {
        PredecessorLayout::Authority0 => (0, None),
        PredecessorLayout::Authority1 => (1, Some(0)),
        PredecessorLayout::Policy1 => (2, Some(1)),
        PredecessorLayout::Policy2 => (3, Some(2)),
        PredecessorLayout::Policy3 => (4, Some(3)),
    };
    compiled(&SLOTS[slot], |connection| {
        connection.execute_batch(V12_SCHEMA)?;
        predecessor_capacity_triggers(connection)?;
        write_barriers(connection)?;
        if let Some(level) = level {
            connection.execute_batch(AUTHORITY_V1_SCHEMA)?;
            policy_layers(connection, level)?;
        }
        Ok(())
    })
}

/// U105: the description layer sits over any authority-1 layout without a
/// marker change. Its exact compiled objects are removed before the base
/// layout is matched, so every base layout keeps one compiled identity.
fn physical_catalog_layout(objects: &[SchemaObject]) -> Result<CatalogLayout> {
    match descriptions::strip(objects)? {
        Some(base) => base_catalog_layout(&base),
        None => base_catalog_layout(objects),
    }
}

fn base_catalog_layout(objects: &[SchemaObject]) -> Result<CatalogLayout> {
    for layout in [
        CatalogLayout::Schema12,
        CatalogLayout::Schema12Policy1,
        CatalogLayout::Schema12Policy2,
        CatalogLayout::Schema12Policy3,
    ] {
        if objects == expected_layout(layout)?.as_slice() {
            return Ok(layout);
        }
    }
    if grant_removal::predecessor_layout(objects)?.is_some() {
        return Err(CatalogUpgradeRequired.into());
    }
    anyhow::bail!("unsupported registry table, index, foreign-key or trigger shape")
}

/// Match an entire compiled physical layout before trusting its authority
/// marker. A metadata edit cannot turn a foreign/partial schema into a format.
pub(super) fn validate_catalog_layout(connection: &Connection) -> Result<CatalogLayout> {
    let objects = schema_objects(connection)?;
    let layout = physical_catalog_layout(&objects)?;
    let (authority, policy) = markers(connection)?;
    ensure!(
        authority <= i64::from(SUPPORTED_AUTHORITY_CONTRACT)
            && policy <= i64::from(SUPPORTED_POLICY_CONTRACT),
        "index catalog upgrade required: unsupported authority or policy contract"
    );
    ensure!(
        matches!(
            (layout, authority, policy),
            (CatalogLayout::Schema12, 0 | 1, 0)
                | (CatalogLayout::Schema12Policy1, 1, 1)
                | (CatalogLayout::Schema12Policy2, 1, 2)
                | (CatalogLayout::Schema12Policy3, 1, 3 | 4)
        ),
        "index authority marker does not match the compiled registry layout"
    );
    // The description layer requires authority one (U105).
    ensure!(
        authority == 1
            || !objects
                .iter()
                .any(|object| object.kind == "table" && object.name == "catalog_descriptions"),
        "catalog descriptions require index authority one"
    );
    Ok(layout)
}

fn markers(connection: &Connection) -> Result<(i64, i64)> {
    Ok(connection.query_row(
        "SELECT authority_contract,policy_contract FROM index_catalog_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

/// Policy installation is part of the already fenced catalog transaction. A
/// predecessor sees either the unchanged policy-0 shape or a format it refuses.
pub(super) fn apply_policy_v1_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "policy layout requires an explicit transaction"
    );
    match validate_catalog_layout(connection)? {
        CatalogLayout::Schema12 => {
            ensure!(
                markers(connection)?.0 == 1,
                "policy installation requires qualified authority 1"
            );
            connection.execute_batch(POLICY_V1_SCHEMA)?;
            connection.execute(
                "UPDATE index_catalog_meta SET policy_contract=1,accepted_new_writes=1 WHERE id=1",
                [],
            )?;
        }
        CatalogLayout::Schema12Policy1
        | CatalogLayout::Schema12Policy2
        | CatalogLayout::Schema12Policy3 => {}
        CatalogLayout::Schema11 => {
            anyhow::bail!("policy installation requires qualified authority 1")
        }
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
    if validate_catalog_layout(connection)? == CatalogLayout::Schema12Policy1 {
        connection.execute_batch(SPLIT_V2_SCHEMA)?;
        connection.execute(
            "UPDATE index_catalog_meta SET policy_contract=2,accepted_new_writes=1 WHERE id=1",
            [],
        )?;
    }
    ensure!(
        matches!(
            validate_catalog_layout(connection)?,
            CatalogLayout::Schema12Policy2 | CatalogLayout::Schema12Policy3
        ),
        "split capability layout differs"
    );
    Ok(())
}

/// Only an already admitted exact private capture may request this within Accepted CAS.
pub(super) fn apply_split_extraction_v3_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "split policy inheritance requires owning transaction"
    );
    apply_split_v2_layout(connection)?;
    if validate_catalog_layout(connection)? == CatalogLayout::Schema12Policy2 {
        connection.execute_batch(SPLIT_V3_SCHEMA)?;
        connection.execute(
            "UPDATE index_catalog_meta SET policy_contract=3,accepted_new_writes=1 WHERE id=1",
            [],
        )?;
    }
    ensure!(
        validate_catalog_layout(connection)? == CatalogLayout::Schema12Policy3,
        "split extraction capability layout differs"
    );
    Ok(())
}

/// Authority 1 adds no physical object since the grant tables were removed
/// (change-log entry 68 part 3): its promotion is the marker update the
/// caller makes in the same transaction. This checks the exact authority-0
/// shape it starts from.
pub(super) fn apply_authority_v1_layout(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "index authority layout promotion requires an explicit transaction"
    );
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 12
            && validate_catalog_layout(connection)? == CatalogLayout::Schema12
            && markers(connection)?.0 == 0,
        "index authority layout promotion requires exact schema-12 authority 0"
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
        schema: connection.pragma_query_value(None, "user_version", |row| row.get(0))?,
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

/// [`validate_existing`], except that a predecessor layout the upgrade step
/// moves to the terminal one is accepted. Only `ServiceRegistry::open` uses it.
pub(super) fn validate_existing_or_predecessor(connection: &Connection) -> Result<()> {
    match validate_existing(connection) {
        Err(error) if error.is::<CatalogUpgradeRequired>() => {
            state(connection)?;
            Ok(())
        }
        result => result,
    }
}

pub(super) fn validate_existing(connection: &Connection) -> Result<()> {
    let application: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        application == super::APPLICATION && matches!(version, 12 | 13),
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
mod tests;

#[cfg(test)]
mod layout_tests;
