// SPDX-License-Identifier: Apache-2.0
//! The upgrade step of change-log entry 68 part 3 (local trust plan, decision
//! 3; user decisions of 23 November 2026). Reviewed grants were removed
//! entirely and this build was never deployed beyond its author's machine, so
//! no backward compatibility is owed for their application records. In one
//! transaction of the read-write `ServiceRegistry::open`, before any stored
//! record is decoded, the step:
//! - deletes the saved records that only a grant-bearing build can decode
//!   ([`RECORD_REMOVALS`]);
//! - drops the removed tables, whose indexes and triggers SQLite drops with
//!   them ([`REMOVED_TABLES`]);
//! - and leaves exactly the terminal compiled layout of the same policy level.
//!
//! A second open finds the terminal layout and does nothing. Read-only opens
//! never run it: they refuse a predecessor layout with
//! [`super::CatalogUpgradeRequired`]. Another removal is one more entry in
//! either list, plus its regression in `grant_removal/tests.rs`.
use super::*;
use rusqlite::TransactionBehavior;

/// Removed tables, children first: a `DROP TABLE` runs an implicit `DELETE`
/// that would otherwise meet a surviving `ON DELETE RESTRICT` reference.
pub(super) const REMOVED_TABLES: [&str; 7] = [
    // Standalone-adoption authority (schema 13): only standalone adoption by
    // releases before the service wrote it; adoption is now refused.
    "index_adopted_configuration_claims",
    "index_adopted_authorities",
    // Reviewed and imported catalog grants.
    "index_access_scopes",
    "index_access_memberships",
    "index_access_grants",
    // Pre-catalog project identities, referenced only by imported grants.
    "legacy_index_projects",
    // Context imports: no writer or reader; only its capacity trigger named it.
    "index_context_imports",
];

/// One class of saved records the terminal build can no longer decode.
pub(super) struct RecordRemoval {
    pub(super) name: &'static str,
    /// Exact compiled SQL; it names no input.
    pub(super) delete: &'static str,
}

/// Saved records deleted before the tables are dropped. The operation tag is
/// serialized first inside `"operation":{...}` (`CatalogOperation` is
/// internally tagged by `operation`), and `instr` matches bytes literally, so
/// no other operation's record matches. A `CreateGrant` wrote an unfenced
/// `catalog_management` receipt that stores no operation; it decodes and stays.
pub(super) const RECORD_REMOVALS: &[RecordRemoval] = &[
    RecordRemoval {
        name: "grant_operation_receipts",
        delete: "DELETE FROM index_operation_receipts WHERE kind='catalog_management_fenced' \
            AND (instr(CAST(payload AS TEXT),'\"operation\":{\"operation\":\"revoke_grant\"')>0 \
            OR instr(CAST(payload AS TEXT),'\"operation\":{\"operation\":\"create_grant\"')>0)",
    },
    // A completed authority fence of another operation that confirmed an
    // installed catalog-grant scope: its proof holds a grant-scoped
    // `ScopedReadRef`, which no longer decodes. Its plan digest covers those
    // targets, so the proof cannot be narrowed; the completed record goes.
    // Unfinished ones refuse the upgrade instead (`grant_scoped_pending`).
    RecordRemoval {
        name: "grant_scoped_completed_fences",
        delete: "DELETE FROM index_operation_receipts WHERE kind='catalog_management_fenced' \
            AND stage='metadata_applied' \
            AND instr(CAST(payload AS TEXT),'\"grant_id\":\"grt_')>0",
    },
    // Reference refreshes: admitted only through a reviewed-grant catalog
    // session, so the feature was removed with grants. Deleting the job
    // cascades to its configuration work and result, request key, progress,
    // cancellation and recovery rows.
    RecordRemoval {
        name: "reference_refresh_jobs",
        delete: "DELETE FROM service_jobs WHERE id IN(SELECT job FROM service_configuration \
            WHERE json_extract(CAST(payload AS TEXT),'$.request.configuration.operation')='refresh_references')",
    },
];

/// An unfinished authority fence still waits on an installed catalog-grant
/// scope. It cannot complete under this build and is never silently
/// abandoned: finish it with the previous build first.
#[derive(Clone, Copy, Debug)]
pub struct CatalogUpgradeBlocked;
impl std::fmt::Display for CatalogUpgradeBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "index catalog upgrade blocked: an unfinished catalog operation waits on a removed reviewed-grant connection; finish it with the previous build",
        )
    }
}
impl std::error::Error for CatalogUpgradeBlocked {}

fn grant_scoped_pending(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' \
            AND stage<>'metadata_applied' AND instr(CAST(payload AS TEXT),'\"grant_id\":\"grt_')>0)",
        [],
        |row| row.get(0),
    )?)
}

/// The predecessor layout these compiled objects are, if any.
pub(super) fn predecessor_layout(objects: &[SchemaObject]) -> Result<Option<PredecessorLayout>> {
    let stripped = descriptions::strip(objects)?;
    let base = stripped.as_deref().unwrap_or(objects);
    for layout in PredecessorLayout::ALL {
        if base == expected_predecessor(layout)?.as_slice() {
            return Ok(Some(layout));
        }
    }
    adopted_layout::recognize(base)
}

/// What the step did; `Terminal` when the layout already was terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::service::registry) enum GrantRemoval {
    Terminal,
    Upgraded {
        from: PredecessorLayout,
        records_deleted: usize,
    },
}

/// The upgrade step. The caller has checked the application id, the
/// registry version, the page budget and the namespace owner.
pub(in crate::service::registry) fn upgrade(connection: &mut Connection) -> Result<GrantRemoval> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let objects = schema_objects(&tx)?;
    let Some(layout) = predecessor_layout(&objects)? else {
        return Ok(GrantRemoval::Terminal);
    };
    let (authority, policy) = markers(&tx)?;
    ensure!(
        matches!(
            (layout, authority, policy),
            (PredecessorLayout::Authority0, 0, 0)
                | (PredecessorLayout::Authority1, 1, 0)
                | (PredecessorLayout::Policy1, 1, 1)
                | (PredecessorLayout::Policy2, 1, 2)
                | (PredecessorLayout::Policy3, 1, 3 | 4)
        ),
        "index authority marker does not match the compiled registry layout"
    );
    // The same metadata identity checks as an ordinary open.
    let current = state(&tx)?;
    ensure!(!grant_scoped_pending(&tx)?, CatalogUpgradeBlocked);
    let mut records_deleted = 0;
    for removal in RECORD_REMOVALS {
        records_deleted += tx
            .execute(removal.delete, [])
            .with_context(|| format!("remove saved {}", removal.name))?;
    }
    for table in REMOVED_TABLES {
        tx.execute_batch(&format!("DROP TABLE IF EXISTS {table};"))?;
    }
    // The write barrier records that an activated catalog accepted a write
    // since its schema-11 backup; this step is such a write. The receipt
    // deletes above already set it through their triggers when any row went;
    // the drops do not fire triggers, so set it for them too. A predecessor
    // refuses the terminal shape whatever the flag says.
    if current.activated {
        tx.execute(
            "UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1 AND activated=1",
            [],
        )?;
    }
    ensure!(
        validate_catalog_layout(&tx)? == layout.terminal(),
        "grant removal produced an unexpected registry layout"
    );
    tx.commit()?;
    Ok(GrantRemoval::Upgraded {
        from: layout,
        records_deleted,
    })
}

#[cfg(test)]
mod tests;
