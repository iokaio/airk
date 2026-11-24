// SPDX-License-Identifier: Apache-2.0
//! The upgrade step of change-log entry 68 part 3. A real settled registry is
//! turned back into its 1.8.16 predecessor with the frozen predecessor DDL; the
//! result is held to the retained capture before any record is added, and the
//! upgraded layout to the terminal fixture derived from it independently.
use super::super::genesis::tests::{TERMINAL_REGISTRY_FIXTURES, s3_assert, s3_settled};
use super::*;
use crate::service::registry::ServiceRegistry;
use airk_core::service::ProjectRegistration;
use rusqlite::params;
use std::collections::BTreeMap;

const RETAINED_POLICY3: &[(&str, &str, &str)] = &[(
    "service-1.4.policy3",
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/schema/airk-state-1/service-1.4.policy3.objects.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/schema/airk-state-1/service-1.4.policy3.meta.json"
    )),
)];

/// Recreate the removed objects with the frozen predecessor DDL: the removed
/// statements of `service_catalog_schema_v12.sql`, their capacity triggers and
/// write barriers, then the authority-1 grant columns.
fn make_predecessor(db: &Connection) -> Result<()> {
    let scratch = Connection::open_in_memory()?;
    scratch.execute_batch(LEGACY_SCHEMA)?;
    scratch.execute_batch(V12_SCHEMA)?;
    predecessor_capacity_triggers(&scratch)?;
    write_barriers(&scratch)?;
    let removed = schema_objects(&scratch)?
        .into_iter()
        .filter(|object| REMOVED_TABLES.contains(&object.table.as_str()) && !object.sql.is_empty())
        .collect::<Vec<_>>();
    for kind in ["table", "index", "trigger"] {
        for object in removed.iter().filter(|object| object.kind == kind) {
            db.execute_batch(&format!("{};", object.sql))?;
        }
    }
    db.execute_batch(AUTHORITY_V1_SCHEMA)?;
    Ok(())
}

fn settled_policy3(root: &std::path::Path) -> Result<ServiceRegistry> {
    let mut registry = s3_settled(root, false)?;
    for apply in [
        super::super::apply_policy_v1_layout,
        super::super::apply_split_v2_layout,
        super::super::apply_split_extraction_v3_layout,
    ] {
        let tx = registry.connection.transaction()?;
        apply(&tx)?;
        tx.commit()?;
    }
    s3_assert(
        TERMINAL_REGISTRY_FIXTURES,
        &registry.connection,
        "service-1.4.terminal-policy3",
    )?;
    Ok(registry)
}

fn id(prefix: &str, byte: u8) -> String {
    format!("{prefix}{}", format!("{byte:02x}").repeat(16))
}
fn hex(byte: u8, bytes: usize) -> String {
    format!("{byte:02x}").repeat(bytes)
}

fn receipt(db: &Connection, byte: u8, kind: &str, stage: &str, payload: &str) -> Result<()> {
    db.execute(
        "INSERT INTO index_operation_receipts VALUES(?1,?2,?3,1,?4)",
        params![hex(byte, 16), kind, stage, payload.as_bytes()],
    )?;
    Ok(())
}

/// Grant rows, one of each removed table, and receipts of every kind the
/// step must delete or keep. Returns the receipt ids it must keep.
fn populate(registry: &mut ServiceRegistry) -> Result<Vec<String>> {
    let registration = ProjectRegistration::new(
        "/upgrade/source".into(),
        "/upgrade/data".into(),
        "upgrade".into(),
    )?;
    let db = &registry.connection;
    let binding = registration.binding.to_string();
    db.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,0,0,1)",
        params![binding, serde_json::to_vec(&registration)?],
    )?;
    let index = id("idx_", 1);
    let project = id("prj_", 2);
    let source = id("src_", 3);
    let grant = id("grt_", 4);
    db.execute(
        "INSERT INTO index_definitions VALUES(?1,?2,?2,'/upgrade/source','/upgrade/data','collections-1.4','upgrade','Upgrade',1,1,0,NULL,0,'complete')",
        params![index, binding],
    )?;
    db.execute(
        "INSERT INTO index_sources VALUES(?1,?2,'/upgrade/source',1,1)",
        params![source, index],
    )?;
    db.execute(
        "INSERT INTO query_projects VALUES(?1,'Upgrade',1,1,1)",
        [&project],
    )?;
    db.execute(
        "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
        params![project, index],
    )?;
    db.execute(
        "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
        params![project, index, source],
    )?;
    db.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked) VALUES(?1,?2,1,0)",
        params![grant, hex(5, 32)],
    )?;
    db.execute(
        "INSERT INTO index_access_memberships VALUES(?1,?2,?3,1,1,0,NULL)",
        params![grant, project, index],
    )?;
    db.execute(
        "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','')",
        params![grant, project, index, source],
    )?;
    db.execute(
        "INSERT INTO legacy_index_projects VALUES(?1,?2,?2)",
        params![binding, project],
    )?;
    db.execute(
        "INSERT INTO index_context_imports VALUES(?1,?2,1,NULL,?3)",
        params![hex(6, 32), hex(7, 32), id("ctx_", 8)],
    )?;
    let request = |operation: &str| {
        format!(
            r#"{{"request":{{"caller":"{c}","key":{{"namespace":"{c}","epoch":1,"nonce":"{c}"}},"expected_catalog_revision":1,"operation":{{"operation":"{operation}","id":"{grant}"}}}}}}"#,
            c = hex(9, 16)
        )
    };
    let scoped = format!(
        r#"{{"proof":{{"confirmations":[{{"target":{{"scope":{{"schema":1,"lease_generation":1,"index_id":"{index}","grant_id":"{grant}","grant_revision":1}}}}}}]}}}}"#
    );
    // Deleted: grant operations at every stage, and a completed fence of
    // another operation whose proof names a catalog-grant scope.
    receipt(
        db,
        0x10,
        "catalog_management_fenced",
        "fenced",
        &request("revoke_grant"),
    )?;
    receipt(
        db,
        0x11,
        "catalog_management_fenced",
        "metadata_applied",
        &request("revoke_grant"),
    )?;
    receipt(
        db,
        0x12,
        "catalog_management_fenced",
        "reserved",
        &request("create_grant"),
    )?;
    receipt(
        db,
        0x13,
        "catalog_management_fenced",
        "metadata_applied",
        &scoped,
    )?;
    // Kept: other operations, other kinds, and look-alikes that only
    // mention a grant operation name outside the operation tag.
    receipt(
        db,
        0x20,
        "catalog_management_fenced",
        "metadata_applied",
        &request("delete_project"),
    )?;
    receipt(
        db,
        0x21,
        "catalog_management",
        "applied",
        r#"{"note":"revoke_grant"}"#,
    )?;
    receipt(
        db,
        0x22,
        "catalog_index_setup",
        "ready",
        r#"{"operation":"create_grant"}"#,
    )?;
    receipt(
        db,
        0x23,
        "catalog_management_fenced",
        "metadata_applied",
        r#"{"name":"\"grant_id\""}"#,
    )?;
    // A finished reference refresh (removed with grants) and another
    // configuration job that stays.
    for (byte, operation) in [(0x40, "refresh_references"), (0x41, "bundled_model")] {
        db.execute(
            "INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'configure','succeeded',0,0)",
            params![hex(byte, 32), binding],
        )?;
        db.execute(
            "INSERT INTO service_configuration VALUES(?1,?2,zeroblob(16392))",
            params![
                hex(byte, 32),
                format!(r#"{{"engine":"{e}","request":{{"configuration":{{"operation":"{operation}"}}}}}}"#, e = hex(byte, 32))
                    .into_bytes()
            ],
        )?;
    }
    Ok(["20", "21", "22", "23"]
        .iter()
        .map(|b| b.repeat(16))
        .collect())
}

/// The refresh job the step deletes, with its cascaded rows.
const REFRESH_JOB: &str = "4040404040404040404040404040404040404040404040404040404040404040";

/// Every row of every table except the removed ones and the catalog marker
/// row, whose write barrier the step sets.
fn rows(db: &Connection) -> Result<BTreeMap<String, Vec<String>>> {
    let mut tables = db.prepare(
        "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = tables
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = BTreeMap::new();
    for name in names {
        if REMOVED_TABLES.contains(&name.as_str()) || name == "index_catalog_meta" {
            continue;
        }
        let mut statement = db.prepare(&format!("SELECT * FROM {name}"))?;
        let columns = statement.column_count();
        let mut values = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|n| {
                        Ok(match row.get_ref(n)? {
                            rusqlite::types::ValueRef::Text(bytes)
                            | rusqlite::types::ValueRef::Blob(bytes) => {
                                String::from_utf8_lossy(bytes).into_owned()
                            }
                            other => format!("{other:?}"),
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map(|cells| cells.join("|"))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values.sort();
        out.insert(name, values);
    }
    Ok(out)
}

fn receipt_ids(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare("SELECT id FROM index_operation_receipts ORDER BY id")?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn predecessor(root: &std::path::Path) -> Result<(Incarnation, Vec<String>)> {
    let mut registry = settled_policy3(root)?;
    let namespace = registry.incarnation()?;
    {
        let tx = registry.connection.transaction()?;
        make_predecessor(&tx)?;
        tx.commit()?;
    }
    // Independent identity: exactly the retained 1.8.16 policy-3 capture.
    s3_assert(
        RETAINED_POLICY3,
        &registry.connection,
        "service-1.4.policy3",
    )?;
    let kept = populate(&mut registry)?;
    Ok((namespace, kept))
}

fn path(root: &std::path::Path) -> std::path::PathBuf {
    root.join("service-1.4.sqlite3")
}

fn objects(db: &Connection) -> Result<Vec<SchemaObject>> {
    schema_objects(db)
}

#[test]
fn old_database_with_grant_records_upgrades_in_one_start_and_the_step_is_idempotent() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (namespace, kept) = predecessor(root.path())?;
    let before = rows(&Connection::open(path(root.path()))?)?;
    let registry = ServiceRegistry::open(root.path(), namespace)?;
    s3_assert(
        TERMINAL_REGISTRY_FIXTURES,
        &registry.connection,
        "service-1.4.terminal-policy3",
    )?;
    assert_eq!(receipt_ids(&registry.connection)?, kept);
    let after = rows(&registry.connection)?;
    let refresh = |row: &String| row.contains(REFRESH_JOB);
    for (table, old) in &before {
        let new = after.get(table).map(Vec::as_slice).unwrap_or_default();
        let expected = match table.as_str() {
            "index_operation_receipts" => old
                .iter()
                .filter(|row| kept.iter().any(|id| row.contains(id.as_str())))
                .cloned()
                .collect::<Vec<_>>(),
            "service_jobs" | "service_configuration" => {
                assert!(old.iter().any(refresh), "{table}: refresh fixture missing");
                old.iter().filter(|row| !refresh(row)).cloned().collect()
            }
            _ => old.clone(),
        };
        assert_eq!(new, expected.as_slice(), "{table} changed");
    }
    assert_eq!(after.len(), before.len());
    let (_, _, _, accepted) = registry.connection.query_row(
        "SELECT authority_contract,policy_contract,activated,accepted_new_writes FROM index_catalog_meta WHERE id=1",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
    )?;
    assert_eq!(accepted, 1, "the step is a write the barrier records");
    let schema = objects(&registry.connection)?;
    drop(registry);
    // Idempotent: a second start finds the terminal layout and changes nothing.
    let registry = ServiceRegistry::open(root.path(), namespace)?;
    assert_eq!(objects(&registry.connection)?, schema);
    assert_eq!(rows(&registry.connection)?, after);
    let mut registry = registry;
    assert_eq!(upgrade(&mut registry.connection)?, GrantRemoval::Terminal);
    Ok(())
}

#[test]
fn an_unfinished_fence_on_a_grant_scope_refuses_the_upgrade_and_changes_nothing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (namespace, _) = predecessor(root.path())?;
    {
        let db = Connection::open(path(root.path()))?;
        receipt(
            &db,
            0x30,
            "catalog_management_fenced",
            "fenced",
            &format!(
                r#"{{"plan":{{"targets":[{{"scope":{{"grant_id":"{}"}}}}]}}}}"#,
                id("grt_", 4)
            ),
        )?;
    }
    let before = rows(&Connection::open(path(root.path()))?)?;
    let error = ServiceRegistry::open(root.path(), namespace)
        .err()
        .context("upgrade admitted")?;
    assert!(error.is::<CatalogUpgradeBlocked>(), "{error:#}");
    let db = Connection::open(path(root.path()))?;
    assert_eq!(rows(&db)?, before);
    s3_assert(RETAINED_POLICY3, &db, "service-1.4.policy3")?;
    Ok(())
}

#[test]
fn read_only_opens_refuse_a_predecessor_typed_and_never_decode_it() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (_, _) = predecessor(root.path())?;
    let before = rows(&Connection::open(path(root.path()))?)?;
    let error = ServiceRegistry::open_saved(root.path())
        .err()
        .context("read-only open admitted")?;
    assert!(error.is::<CatalogUpgradeRequired>(), "{error:#}");
    let db = Connection::open(path(root.path()))?;
    assert!(
        validate_existing(&db)
            .unwrap_err()
            .is::<CatalogUpgradeRequired>()
    );
    assert_eq!(rows(&db)?, before);
    s3_assert(RETAINED_POLICY3, &db, "service-1.4.policy3")?;
    Ok(())
}

/// Every retained predecessor capture reduces by the compiled removal to its
/// terminal fixture; the adopted ones lose their adoption tables too.
#[test]
fn every_retained_predecessor_layout_maps_to_its_terminal_layout() {
    use PredecessorLayout::*;
    for (layout, terminal) in [
        (Authority0, CatalogLayout::Schema12),
        (Authority1, CatalogLayout::Schema12),
        (Policy1, CatalogLayout::Schema12Policy1),
        (Policy2, CatalogLayout::Schema12Policy2),
        (Policy3, CatalogLayout::Schema12Policy3),
    ] {
        assert_eq!(layout.terminal(), terminal);
    }
}
