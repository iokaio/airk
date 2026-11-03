// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::ProjectRegistration;
use rusqlite::params;

fn registry() -> Result<(tempfile::TempDir, ServiceRegistry)> {
    let root = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    Ok((root, registry))
}

#[test]
fn aggregate_catalog_capacity_failure_rolls_back_without_evicting_existing_metadata() -> Result<()>
{
    let (_root, mut registry) = registry()?;
    install(&mut registry)?;
    let original = insert_index(&registry)?;
    let tx = registry.connection.transaction()?;
    let path_suffix = "x".repeat(32700);
    let mut full = false;
    'indexes: for number in 0u128..128 {
        let index = format!(
            "idx_{}",
            Incarnation::from_bytes((1000 + number).to_be_bytes())
        );
        let backing = ProjectRegistration::new(
            format!("/fictional/{number}"),
            format!("/fictional/data/{number}"),
            "capacity".into(),
        )?;
        tx.execute("INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4',?5,'Capacity fixture',1,1,0,NULL,0,'pending_metadata')",params![index,backing.binding.to_string(),backing.root,backing.data_root,backing.collection])?;
        for root in 0u128..17 {
            let source = format!(
                "src_{}",
                Incarnation::from_bytes((1000 + number * 17 + root).to_be_bytes())
            );
            match tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,?4,1)",
                params![
                    source,
                    index,
                    format!("/fictional/{root}/{path_suffix}"),
                    root == 0
                ],
            ) {
                Ok(_) => {}
                Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull) => {
                    full = true;
                    break 'indexes;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    assert!(
        full,
        "fixture must reach the real aggregate 32 MiB registry limit"
    );
    drop(tx);
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_definitions", [], |row| row
                .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT id FROM index_definitions", [], |row| row
                .get::<_, String>(0))?,
        original
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM projects", [], |row| row
                .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_sources", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    let pages: i64 = registry
        .connection
        .pragma_query_value(None, "page_count", |row| row.get(0))?;
    assert!(pages <= 8192);
    backup::integrity(&registry.connection)?;
    Ok(())
}
fn install(registry: &mut ServiceRegistry) -> Result<()> {
    let old_digest = schema_digest(&registry.connection)?;
    let namespace = registry.incarnation()?;
    let tx = registry.connection.transaction()?;
    install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            old_digest.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    tx.pragma_update(None, "user_version", 12)?;
    tx.commit()?;
    validate_existing(&registry.connection)?;
    assert_eq!(state(&registry.connection)?.namespace, namespace);
    Ok(())
}
fn insert_index(registry: &ServiceRegistry) -> Result<String> {
    let backing = ProjectRegistration::new(
        "/fictional/unavailable".into(),
        "/fictional/storage".into(),
        "fixture".into(),
    )?;
    registry.connection.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,0,0,1)",
        params![backing.binding.to_string(), serde_json::to_vec(&backing)?],
    )?;
    let id = format!("idx_{}", Incarnation::from_bytes([4; 16]));
    registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Original',1,1,0,NULL,0,'complete')",params![id,backing.binding.to_string(),backing.root,backing.data_root,backing.collection])?;
    Ok(id)
}

#[test]
fn compiled_legacy_shape_matches_and_no_read_installs_the_new_catalog() -> Result<()> {
    let (root, registry) = registry()?;
    let actual = schema_objects(&registry.connection)?;
    let compiled = expected(11)?;
    assert_eq!(actual.len(), compiled.len());
    for (actual, compiled) in actual.iter().zip(compiled) {
        assert_eq!(actual, compiled, "compiled schema object {}", actual.name);
    }
    validate_shape(&registry.connection, 11)?;
    assert!(registry.catalog_state()?.is_none());
    assert!(
        ServiceRegistry::open_saved(root.path())?
            .catalog_state()?
            .is_none()
    );
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
        11
    );
    assert!(expected(12)?.len() <= MAX_SCHEMA_OBJECTS);
    Ok(())
}

#[test]
fn full_shape_rejects_missing_reverse_indexes_and_extra_triggers() -> Result<()> {
    let (_root, mut registry) = registry()?;
    install(&mut registry)?;
    registry
        .connection
        .execute_batch("DROP INDEX index_project_memberships;")?;
    assert!(validate_existing(&registry.connection).is_err());
    registry.connection.execute_batch(
        "CREATE INDEX index_project_memberships ON project_index_memberships(index_id,project_id);",
    )?;
    validate_existing(&registry.connection)?;
    registry.connection.execute_batch("CREATE TRIGGER unknown_catalog_trigger AFTER UPDATE ON query_projects BEGIN SELECT 1; END;")?;
    assert!(validate_existing(&registry.connection).is_err());
    Ok(())
}

#[test]
fn registration_purge_and_project_deletion_preserve_the_shared_index_identity() -> Result<()> {
    let (_root, mut registry) = registry()?;
    install(&mut registry)?;
    let index = insert_index(&registry)?;
    let mut projects = Vec::new();
    for n in [5, 6] {
        let project = format!("prj_{}", Incarnation::from_bytes([n; 16]));
        registry.connection.execute(
            "INSERT INTO query_projects VALUES(?1,'Project',1,1,1)",
            [&project],
        )?;
        registry.connection.execute(
            "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
            params![project, index],
        )?;
        projects.push(project);
    }
    registry
        .connection
        .execute("DELETE FROM query_projects WHERE id=?1", [&projects[0]])?;
    assert_eq!(
        registry.connection.query_row(
            "SELECT count(*) FROM project_index_memberships WHERE index_id=?1",
            [&index],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    registry.connection.execute("DELETE FROM projects", [])?;
    let (binding, active): (String, Option<String>) = registry.connection.query_row(
        "SELECT legacy_binding,active_registration FROM index_definitions WHERE id=?1",
        [&index],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(binding.len(), 64);
    assert!(active.is_none());
    assert!(
        registry
            .connection
            .execute(
                "UPDATE index_definitions SET collection='other' WHERE id=?1",
                [&index]
            )
            .is_err()
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_definitions", [], |row| row
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn first_settled_new_or_legacy_write_records_the_no_restore_barrier() -> Result<()> {
    for legacy in [false, true] {
        let (_root, mut registry) = registry()?;
        install(&mut registry)?;
        insert_index(&registry)?;
        assert!(!state(&registry.connection)?.accepted_new_writes);
        registry
            .connection
            .execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1", [])?;
        {
            let tx = registry.connection.transaction()?;
            tx.execute(
                if legacy {
                    "UPDATE projects SET paused=1"
                } else {
                    "UPDATE index_definitions SET name='Edited'"
                },
                [],
            )?;
            assert!(state(&tx)?.accepted_new_writes);
            tx.rollback()?;
        }
        assert!(
            !state(&registry.connection)?.accepted_new_writes,
            "rolled-back work must not settle the barrier"
        );
        registry.connection.execute(
            if legacy {
                "UPDATE projects SET paused=1"
            } else {
                "UPDATE index_definitions SET name='Edited'"
            },
            [],
        )?;
        assert!(state(&registry.connection)?.accepted_new_writes);
    }
    Ok(())
}

#[test]
fn unknown_authority_or_policy_contract_is_not_treated_as_compatible_schema_12() -> Result<()> {
    for column in ["authority_contract", "policy_contract"] {
        let (_root, mut registry) = registry()?;
        install(&mut registry)?;
        registry.connection.execute_batch(&format!(
            "UPDATE index_catalog_meta SET {column}=1 WHERE id=1"
        ))?;
        assert!(validate_existing(&registry.connection).is_err());
    }
    Ok(())
}

#[test]
fn ordinary_opens_refuse_pending_or_unknown_catalogs_without_mutating_them() -> Result<()> {
    let (root, mut registry) = registry()?;
    install(&mut registry)?;
    let namespace = registry.incarnation()?;
    let path = root.path().join("service-1.4.sqlite3");
    let before = std::fs::read(&path)?;
    assert!(ServiceRegistry::open_saved(root.path()).is_err());
    assert!(ServiceRegistry::open(root.path(), namespace).is_err());
    assert_eq!(std::fs::read(&path)?, before);
    registry
        .connection
        .execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1", [])?;
    assert!(
        ServiceRegistry::open_saved(root.path())?
            .catalog_state()?
            .unwrap()
            .activated
    );
    assert!(
        ServiceRegistry::open(root.path(), namespace)?
            .catalog_state()?
            .unwrap()
            .activated
    );
    registry.connection.execute(
        "UPDATE index_catalog_meta SET policy_contract=1 WHERE id=1",
        [],
    )?;
    let before = std::fs::read(&path)?;
    assert!(
        ServiceRegistry::open_saved(root.path())
            .err()
            .context("expected saved-reader refusal")?
            .to_string()
            .contains("upgrade required")
    );
    assert!(
        ServiceRegistry::open(root.path(), namespace)
            .err()
            .context("expected writable refusal")?
            .to_string()
            .contains("upgrade required")
    );
    assert_eq!(std::fs::read(&path)?, before);
    Ok(())
}
