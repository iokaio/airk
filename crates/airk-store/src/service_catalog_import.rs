// SPDX-License-Identifier: Apache-2.0
//! Exact, idempotent editor-context imports during the fenced upgrade.
use super::*;
use airk_core::index_catalog::{ContextId, ContextImport, ContextImportReceipt, ContextKind};
use rusqlite::OptionalExtension;

impl CatalogUpgrade {
    pub fn import_context(&self, request: &ContextImport) -> Result<ContextImportReceipt> {
        request.validate()?;
        let digest = request.identity()?;
        let receipt = self.receipt()?;
        ensure!(
            receipt.stage == UpgradeStage::Imports,
            "context import is not admitted at this upgrade stage"
        );
        let read = registry_read(&self.root)?;
        self.migration_state(&read, &receipt)?;
        drop(read);
        let mut connection = Connection::open_with_flags(
            self.root.join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        connection.busy_timeout(Duration::from_millis(100))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
        )?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String,String,Option<String>,i64)> = tx.query_row(
            "SELECT request_digest,context_identity,context_id,catalog_revision FROM index_context_imports WHERE import_key=?1",
            [request.key.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
        if let Some((saved, identity, current, revision)) = previous {
            ensure!(
                saved == digest.to_string() && identity == request.context.id.to_string(),
                "context import key was already used for another request"
            );
            return Ok(ContextImportReceipt {
                schema: 1,
                key: request.key,
                request_digest: digest,
                context_id: identity.parse::<ContextId>()?,
                catalog_revision: u64::try_from(revision)?.try_into()?,
                context_present: current.is_some(),
            });
        }
        let state = super::super::state(&tx)?;
        ensure!(
            state.revision == request.expected_catalog_revision,
            "context import review is stale"
        );
        let revision = state.revision.next()?;
        let context = &request.context;
        for project in &context.project_ids {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM query_projects WHERE id=?1)",
                [project.to_string()],
                |row| row.get(0),
            )?;
            ensure!(exists, "context import names an unavailable Project");
        }
        for index in &context.index_ids {
            let mut available = false;
            for project in &context.project_ids {
                available |= tx.query_row("SELECT EXISTS(SELECT 1 FROM project_index_memberships m JOIN index_definitions i ON i.id=m.index_id WHERE m.project_id=?1 AND m.index_id=?2 AND i.tombstone=0)", params![project.to_string(),index.to_string()], |row| row.get::<_,bool>(0))?;
            }
            ensure!(
                available,
                "context default is outside the selected Projects or unavailable"
            );
        }
        let kind = match context.kind {
            ContextKind::Folder => "folder",
            ContextKind::Workspace => "workspace",
            ContextKind::Empty => "empty",
            ContextKind::Untitled => "untitled",
            ContextKind::LegacyPending => "legacy_pending",
        };
        tx.execute(
            "INSERT INTO index_contexts VALUES(?1,?2,?3,?4,?5,1)",
            params![
                context.id.to_string(),
                context.host.to_string(),
                context.key.to_string(),
                kind,
                context.name
            ],
        )?;
        for project in &context.project_ids {
            tx.execute(
                "INSERT INTO context_projects VALUES(?1,?2)",
                params![context.id.to_string(), project.to_string()],
            )?;
        }
        for index in &context.index_ids {
            tx.execute(
                "INSERT INTO context_default_indexes VALUES(?1,?2)",
                params![context.id.to_string(), index.to_string()],
            )?;
        }
        tx.execute(
            "INSERT INTO index_context_imports VALUES(?1,?2,?3,?4,?4)",
            params![
                request.key.to_string(),
                digest.to_string(),
                i64::try_from(revision.get())?,
                context.id.to_string()
            ],
        )?;
        let changed = tx.execute(
            "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
            params![
                i64::try_from(revision.get())?,
                i64::try_from(request.expected_catalog_revision.get())?
            ],
        )?;
        ensure!(changed == 1, "context import revision changed");
        tx.commit()?;
        Ok(ContextImportReceipt {
            schema: 1,
            key: request.key,
            request_digest: digest,
            context_id: context.id,
            catalog_revision: revision,
            context_present: true,
        })
    }
}

#[cfg(test)]
#[path = "service_catalog_import_tests.rs"]
mod tests;
