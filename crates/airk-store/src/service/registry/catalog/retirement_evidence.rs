// SPDX-License-Identifier: Apache-2.0
//! Explicit native recovery evidence. A tombstone or missing journal is never
//! equivalent to the full committed native retirement fence.
use super::{ServiceRegistry, operations::index_record, state, validate_existing};
pub use airk_core::catalog_fences::NativeIndexRetirementEvidence;
use airk_core::{
    catalog_fences::FencedReceipt,
    catalog_management::{CatalogOperation, MAX_RECEIPT_BYTES},
    index_catalog::IndexId,
    service::CatalogClock,
};
use anyhow::{Result, ensure};
use rusqlite::Connection;
use rusqlite::{OptionalExtension, params};

impl ServiceRegistry {
    /// Explicit selected-index recovery only. This scans at most the bounded
    /// 1024 native receipt rows, each capped before decoding to 16 KiB. There is
    /// no JSON expression index, source scan, mutation or retrieval-path work.
    pub fn catalog_index_retirement_evidence(
        &self,
        index_id: IndexId,
    ) -> Result<Option<NativeIndexRetirementEvidence>> {
        let read = self.connection.unchecked_transaction()?;
        validate_existing(&read)?;
        let catalog = state(&read)?;
        ensure!(
            catalog.activated
                && catalog.authority_contract == 1
                && catalog.policy_contract <= super::SUPPORTED_POLICY_CONTRACT,
            "native retirement evidence requires the active native catalog"
        );
        let row: Option<(String, Option<String>)> = read
            .query_row(
                "SELECT legacy_binding,active_registration FROM index_definitions WHERE id=?1",
                [index_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((binding, active)) = row else {
            return Ok(None);
        };
        let index = index_record(&read, index_id)?;
        if !index.tombstone {
            return Ok(None);
        }
        let mut statement = read.prepare("SELECT id,kind,stage,revision,length(payload),substr(payload,1,16385) FROM index_operation_receipts ORDER BY id LIMIT 1025")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        let mut retirement = None;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(count <= 1024, "native retirement receipt capacity exceeded");
            let length: i64 = row.get(4)?;
            let bytes: Vec<u8> = row.get(5)?;
            ensure!(
                length >= 0
                    && length <= i64::try_from(MAX_RECEIPT_BYTES)?
                    && usize::try_from(length)? == bytes.len(),
                "native retirement receipt exceeds bound"
            );
            let kind: String = row.get(1)?;
            let stage: String = row.get(2)?;
            if kind != "catalog_management_fenced" || stage != "metadata_applied" {
                continue;
            }
            let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
            receipt.validate()?;
            let operation: String = row.get(0)?;
            let revision: i64 = row.get(3)?;
            ensure!(
                receipt.metadata.operation_id.to_string() == operation
                    && i64::try_from(receipt.metadata.catalog_revision.get())? == revision,
                "native retirement saved receipt identity changed"
            );
            if let CatalogOperation::RetireIndex { id, .. } = &receipt.request.operation
                && *id == index_id
            {
                let evidence = NativeIndexRetirementEvidence {
                    catalog_clock: CatalogClock {
                        namespace: catalog.namespace,
                        revision: catalog.revision,
                    },
                    index: index.clone(),
                    retirement: receipt,
                };
                evidence.validate()?;
                ensure!(
                    retirement.replace(evidence).is_none(),
                    "ambiguous native retirement lineage"
                );
            }
        }
        let Some(evidence) = retirement else {
            return Ok(None);
        };
        ensure!(
            binding == index.backing.binding.to_string(),
            "native retirement backing identity changed"
        );
        let registration: Option<(Vec<u8>, bool, bool)> = read
            .query_row(
                "SELECT substr(payload,1,20481),registered,paused FROM projects WHERE binding=?1",
                [&binding],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match (active.as_deref(), registration) {
            (Some(pointer), Some((payload, registered, paused))) => ensure!(
                pointer == binding
                    && !registered
                    && paused
                    && payload == serde_json::to_vec(&index.backing)?,
                "native retirement retained registration changed"
            ),
            (None, None) => {
                // Physical cleanup drops the exact inactive registration and
                // its FK pointer together. Absence alone is not retirement
                // evidence: require the cleanup transaction's saved identity,
                // in addition to the complete canonical fence verified above.
                let removed: bool = read.query_row(
                    "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE binding=?1 AND root_identity=?2)",
                    params![
                        binding,
                        super::super::project_removal::removed_root_identity(&index.backing.root)
                            .to_string()
                    ],
                    |row| row.get(0),
                )?;
                ensure!(
                    removed,
                    "native retirement cleanup identity is not retained"
                );
            }
            _ => anyhow::bail!("native retirement backing pointer changed"),
        }
        let enabled: bool = read.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_sources WHERE index_id=?1 AND enabled<>0)",
            params![index_id.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !enabled,
            "native retirement still has enabled source authority"
        );
        Ok(Some(evidence))
    }
}

/// Delete the tombstoned definitions whose retirement cleanup is
/// acknowledged (the user's record cleanups of 23 November 2026, item 3).
/// A retired definition goes only when all of these hold: its complete
/// `RetireIndex` fence is applied, no fence is pending, the physical cleanup
/// committed (its registration is gone, or kept unregistered with its cleanup
/// guard cleared) and `service_removed_projects` holds its exact cleanup
/// identity. It keeps any definition still named by a membership, a context
/// default, a source policy, an active split participant or an enabled
/// source. Its sources, aliases and description go with it, and so does a
/// retained registration that nothing else refers to. Returns how many were
/// deleted.
pub(super) fn delete_retired_definitions(db: &Connection) -> Result<usize> {
    let pending: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage<>'metadata_applied')",
        [],
        |row| row.get(0),
    )?;
    let removal_history: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='service_removed_projects')",
        [],
        |row| row.get(0),
    )?;
    if pending || !removal_history {
        return Ok(0);
    }
    let optional = |table: &str| -> Result<bool> {
        Ok(db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )?)
    };
    let (policies, splits, descriptions) = (
        optional("index_source_policies")?,
        optional("index_split_participants")?,
        optional("catalog_descriptions")?,
    );
    let mut statement =
        db.prepare("SELECT id FROM index_definitions WHERE tombstone=1 ORDER BY id LIMIT 1025")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    ensure!(ids.len() <= 1024, "retired index roster exceeds capacity");
    let mut deleted = 0;
    for id in ids {
        let index_id: IndexId = id.parse()?;
        let referenced: bool = db.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM project_index_memberships WHERE index_id=?1) \
                 OR EXISTS(SELECT 1 FROM context_default_indexes WHERE index_id=?1) \
                 OR EXISTS(SELECT 1 FROM index_sources WHERE index_id=?1 AND enabled<>0){}{}",
                if policies {
                    " OR EXISTS(SELECT 1 FROM index_source_policies WHERE index_id=?1)"
                } else {
                    ""
                },
                if splits {
                    " OR EXISTS(SELECT 1 FROM index_split_participants WHERE index_id=?1 AND active=1)"
                } else {
                    ""
                }
            ),
            [&id],
            |row| row.get(0),
        )?;
        if referenced {
            continue;
        }
        let index = index_record(db, index_id)?;
        let binding = index.backing.binding.to_string();
        let identity =
            super::super::project_removal::removed_root_identity(&index.backing.root).to_string();
        let acknowledged: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE binding=?1 AND root_identity=?2) \
             AND NOT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND (registered<>0 OR cleanup_guard IS NOT NULL))",
            params![binding, identity],
            |row| row.get(0),
        )?;
        if !acknowledged || !retirement_fence_applied(db, index_id)? {
            continue;
        }
        db.execute_batch("SAVEPOINT retired_definition")?;
        let removed = (|| -> rusqlite::Result<()> {
            db.execute("DELETE FROM index_source_aliases WHERE index_id=?1", [&id])?;
            db.execute("DELETE FROM index_sources WHERE index_id=?1", [&id])?;
            if descriptions {
                db.execute(
                    "DELETE FROM catalog_descriptions WHERE kind='index' AND id=?1",
                    [&id],
                )?;
            }
            db.execute("DELETE FROM index_definitions WHERE id=?1", [&id])?;
            // The registration was kept only as this definition's backing
            // identity; anything that still refers to it keeps both.
            db.execute(
                "DELETE FROM projects WHERE binding=?1 AND registered=0 AND cleanup_guard IS NULL",
                [&binding],
            )?;
            Ok(())
        })();
        match removed {
            Ok(()) => {
                db.execute_batch("RELEASE retired_definition")?;
                deleted += 1;
            }
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                db.execute_batch("ROLLBACK TO retired_definition; RELEASE retired_definition")?;
            }
            Err(error) => {
                db.execute_batch("ROLLBACK TO retired_definition; RELEASE retired_definition")?;
                return Err(error.into());
            }
        }
    }
    Ok(deleted)
}

/// Whether the applied `RetireIndex` fence of `index_id` is retained, as the
/// retirement evidence requires it.
fn retirement_fence_applied(db: &Connection, index_id: IndexId) -> Result<bool> {
    let mut statement = db.prepare(
        "SELECT id,revision,substr(payload,1,16385) FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage='metadata_applied' AND json_extract(payload,'$.request.operation.operation')='retire_index' AND json_extract(payload,'$.request.operation.id')=?1 LIMIT 2",
    )?;
    let rows = statement
        .query_map([index_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1 {
        return Ok(false);
    }
    let (operation, revision, bytes) = &rows[0];
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "native retirement receipt exceeds bound"
    );
    let receipt: FencedReceipt = serde_json::from_slice(bytes)?;
    receipt.validate()?;
    Ok(receipt.metadata.operation_id.to_string() == *operation
        && i64::try_from(receipt.metadata.catalog_revision.get())? == *revision
        && matches!(&receipt.request.operation, CatalogOperation::RetireIndex { id, .. } if *id == index_id))
}
