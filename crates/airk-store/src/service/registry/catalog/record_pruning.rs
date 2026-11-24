// SPDX-License-Identifier: Apache-2.0
//! Bounded catalog operation records: the user's record cleanups of
//! 23 November 2026, items 1 to 3 (local trust plan, decision 3).
//!
//! `index_operation_receipts` admits at most 1,024 rows and finished splits at
//! most 64, and nothing evicted either, so the catalog refused every change
//! after about 1,024 changes. Like the job receipts (`jobs::prune_job_receipts`),
//! these records are now pruned under pressure inside the caller's immediate
//! transaction, and the caller rotates the request epoch whenever a retry key
//! was deleted, so an old key expires explicitly instead of replaying.
//!
//! A receipt is kept when it is unfinished (the checkpoint's terminal
//! classifier, the one owner of receipt stages) or when a live reader still
//! needs it: a `RetireIndex` fence while its tombstone is retained (retirement
//! evidence), a source-change begin until its completion is applied, the
//! current source policy's fence, and a fence or final receipt of a split that
//! is still retained. Of the other terminal receipts the newest
//! `KEPT_TERMINAL_RECEIPTS` stay.
//!
//! Regressions: `operations::tests::more_than_1024_catalog_mutations_succeed_and_unfinished_receipts_survive`,
//! `split::tests::more_than_64_splits_succeed_and_an_unfinished_split_survives` and
//! `source_change::tests::retirement_evidence::acknowledged_retirement_cleanup_deletes_the_tombstone_and_frees_its_fence`.
use super::super::checkpoint::receipts::terminal;
use anyhow::{Result, ensure};
use rusqlite::Connection;
use std::collections::BTreeSet;

/// The admission bound every receipt writer checks.
const RECEIPT_CAPACITY: i64 = 1024;
/// Pruning starts at this many receipts, leaving room for the begin and
/// completion pair of a source change and other multi-row operations.
const RECEIPT_HIGH_WATER: i64 = RECEIPT_CAPACITY - 128;
/// Terminal receipts kept after pruning. A retry of an uncertain catalog
/// request comes from its own caller within seconds; 256 completed operations
/// is far beyond any real retry window, and it leaves at least 512 free rows
/// under the cap when every other row is a protected one of a bounded owner.
pub(in crate::service::registry) const KEPT_TERMINAL_RECEIPTS: usize = 256;
/// Finished splits are pruned at this many retained splits (cap 64).
const SPLIT_HIGH_WATER: i64 = 48;
/// Finished splits kept: enough for the recent history Admin shows.
pub(in crate::service::registry) const KEPT_FINISHED_SPLITS: usize = 8;

/// Kinds whose terminal receipts are pruned by age. Split final receipts go
/// with their split, never alone.
const PRUNABLE_KINDS: &[&str] = &[
    "catalog_management",
    "catalog_management_fenced",
    "catalog_source_completion",
    "catalog_index_setup",
];

fn catalog_present(db: &Connection) -> Result<bool> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !matches!(version, 12 | 13) || !table(db, "index_catalog_meta")? {
        return Ok(false);
    }
    // Pruning is housekeeping: a catalog its owners would refuse is theirs
    // to report, never a reason to fail a request epoch.
    Ok(super::state(db).is_ok_and(|state| state.activated))
}

fn table(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

fn count(db: &Connection, name: &str) -> Result<i64> {
    if !table(db, name)? {
        return Ok(0);
    }
    Ok(
        db.query_row(&format!("SELECT count(*) FROM {name}"), [], |row| {
            row.get(0)
        })?,
    )
}

/// Whether a pruning pass has anything to do. Read-only.
pub(in crate::service::registry) fn pressure(db: &Connection) -> Result<bool> {
    if !catalog_present(db)? {
        return Ok(false);
    }
    // A retired definition whose cleanup identity is recorded and that no
    // source policy keeps; the pass checks the rest. Tombstones that must stay
    // do not hold the writer lock on every observation.
    let policies = table(db, "index_source_policies")?;
    let retired: bool = table(db, "service_removed_projects")?
        && db.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM index_definitions i JOIN service_removed_projects r ON r.binding=i.legacy_binding WHERE i.tombstone=1{})",
                if policies {
                    " AND NOT EXISTS(SELECT 1 FROM index_source_policies p WHERE p.index_id=i.id)"
                } else {
                    ""
                }
            ),
            [],
            |row| row.get(0),
        )?;
    Ok(retired
        || count(db, "index_operation_receipts")? >= RECEIPT_HIGH_WATER
        || count(db, "index_split_operations")? >= SPLIT_HIGH_WATER)
}

/// One pruning pass inside the caller's immediate transaction. Returns
/// whether a retry key was deleted, so the caller must rotate the epoch.
pub(in crate::service::registry) fn prune(db: &Connection) -> Result<bool> {
    ensure!(
        !db.is_autocommit(),
        "catalog record pruning requires the caller's transaction"
    );
    if !catalog_present(db)? {
        return Ok(false);
    }
    // Retired definitions first: their retirement fences become prunable.
    super::retirement_evidence::delete_retired_definitions(db)?;
    let mut keys_deleted = false;
    if count(db, "index_split_operations")? >= SPLIT_HIGH_WATER {
        keys_deleted |= super::split::prune_finished_splits(db, KEPT_FINISHED_SPLITS)? > 0;
    }
    if count(db, "index_operation_receipts")? >= RECEIPT_HIGH_WATER {
        keys_deleted |= prune_receipts(db, KEPT_TERMINAL_RECEIPTS)? > 0;
    }
    Ok(keys_deleted)
}

/// Delete all but the newest `keep` prunable terminal receipts.
pub(in crate::service::registry) fn prune_receipts(db: &Connection, keep: usize) -> Result<usize> {
    let protected = protected_by_readers(db)?;
    let mut statement = db.prepare(
        "SELECT id,kind,stage,revision,length(payload),substr(payload,1,16385) FROM index_operation_receipts ORDER BY revision DESC,id DESC LIMIT 1025",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    ensure!(
        rows.len() as i64 <= RECEIPT_CAPACITY,
        "catalog receipt roster exceeds capacity"
    );
    let mut classifier = super::split::CheckpointReceiptClassifier::default();
    let mut kept = 0;
    let mut doomed = Vec::new();
    for (id, kind, stage, revision, length, payload) in rows {
        ensure!(
            usize::try_from(length)? == payload.len(),
            "catalog receipt exceeds bound"
        );
        if !PRUNABLE_KINDS.contains(&kind.as_str())
            || protected.contains(&id)
            || !terminal(&mut classifier, db, &id, &kind, &stage, revision, &payload)?
        {
            continue;
        }
        if kept < keep {
            kept += 1;
        } else {
            doomed.push(id);
        }
    }
    for id in &doomed {
        db.execute("DELETE FROM index_operation_receipts WHERE id=?1", [id])?;
    }
    Ok(doomed.len())
}

/// The terminal receipts a live reader still needs.
fn protected_by_readers(db: &Connection) -> Result<BTreeSet<String>> {
    let mut protected = BTreeSet::new();
    let mut add = |sql: &str| -> Result<()> {
        let mut statement = db.prepare(sql)?;
        for id in statement.query_map([], |row| row.get::<_, String>(0))? {
            protected.insert(id?);
        }
        Ok(())
    };
    // Retirement evidence reads the fence while its tombstone is retained.
    add(
        "SELECT r.id FROM index_operation_receipts r JOIN index_definitions i ON i.id=json_extract(r.payload,'$.request.operation.id') \
         WHERE r.kind='catalog_management_fenced' AND json_extract(r.payload,'$.request.operation.operation')='retire_index'",
    )?;
    // A source-change begin stays until its completion is applied (checked
    // below), and the current policy keeps its fence.
    if table(db, "index_source_policies")? {
        add(
            "SELECT json_extract(payload,'$.operation_id') FROM index_source_policies WHERE json_extract(payload,'$.operation_id') IS NOT NULL",
        )?;
    }
    if table(db, "index_split_operations")? {
        add(
            "SELECT r.id FROM index_operation_receipts r JOIN index_split_operations s ON s.operation_id=json_extract(r.payload,'$.request.operation.change.split_operation') \
             WHERE r.kind='catalog_management_fenced'",
        )?;
    }
    let mut statement = db.prepare(
        "SELECT id,substr(payload,1,16385) FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage='metadata_applied' \
         AND json_extract(payload,'$.request.operation.operation')='begin_index_source_change'",
    )?;
    let begins = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (id, bytes) in begins {
        let receipt: airk_core::catalog_fences::FencedReceipt = serde_json::from_slice(&bytes)?;
        let completion = super::source_change::completion_id(&receipt.request)?.to_string();
        let applied: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1 AND kind='catalog_source_completion' AND stage='metadata_applied')",
            [&completion],
            |row| row.get(0),
        )?;
        if !applied {
            protected.insert(id);
        }
    }
    Ok(protected)
}
