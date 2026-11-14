// SPDX-License-Identifier: Apache-2.0
//! Original-named grouping belongs to the existing split publication transaction.
use super::*;
use airk_core::catalog_split::{SplitGrouping, SplitGroupingMode};
#[cfg(test)]
mod tests;

fn resolve(
    db: &Connection,
    original: IndexId,
    mut grouping: SplitGrouping,
) -> Result<SplitGrouping> {
    let mut statement = db.prepare("SELECT id,name,definition_revision,scope_revision FROM query_projects WHERE name=?1 COLLATE NOCASE ORDER BY id LIMIT 2")?;
    let rows = statement
        .query_map([&grouping.name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= 1,
        "multiple same-name sets make split grouping ambiguous; rename them and review again"
    );
    if let Some((id, name, definition, scope)) = rows.into_iter().next() {
        ensure!(
            name == grouping.name,
            "split original-name set conflicts with an unrelated name"
        );
        let member = super::super::operations::membership(db, &id, &original.to_string()).context(
            "same-name set has no reviewed original membership; rename that set and review again",
        )?;
        ensure!(
            member.scope == grouping.original_scope,
            "same-name set has a restricted original scope; review grouping explicitly before splitting"
        );
        grouping.project_id = id.parse()?;
        grouping.mode = SplitGroupingMode::ExistingNamedSet {
            definition_revision: u64::try_from(definition)?.try_into()?,
            scope_revision: u64::try_from(scope)?.try_into()?,
            original_membership_revision: member.revision,
        };
    } else {
        let conflict: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM query_projects WHERE id=?1)",
            [grouping.project_id.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !conflict,
            "split grouping identity conflicts with another set"
        );
        grouping.mode = SplitGroupingMode::Create;
    }
    Ok(grouping)
}

pub(super) fn review(db: &Connection, request: &SplitRequest) -> Result<()> {
    let Some(grouping) = &request.grouping else {
        return Ok(());
    };
    let original = index_record(db, request.original_index)?;
    ensure!(
        original.name == grouping.name
            && original.definition_revision == request.expected_definition_revision
            && !original.tombstone,
        "split original name changed; review again"
    );
    let expected = resolve(db, request.original_index, grouping.clone())?;
    ensure!(
        expected == *grouping,
        "split grouping review changed; review again"
    );
    Ok(())
}

pub(super) fn publish(db: &Connection, request: &SplitRequest, root: &Path) -> Result<()> {
    let Some(grouping) = &request.grouping else {
        return Ok(());
    };
    review(db, request)?;
    let mutate = |operation| {
        super::super::operations::mutate(
            db,
            &CatalogRequest {
                caller: request.caller,
                key: request.key.clone(),
                expected_catalog_revision: super::super::state(db)?.revision,
                operation,
            },
            root,
        )
    };
    let mut scope_revision = match grouping.mode {
        SplitGroupingMode::Create => {
            mutate(CatalogOperation::CreateProject {
                id: grouping.project_id,
                name: grouping.name.clone(),
            })?;
            mutate(CatalogOperation::SetMembership {
                membership: airk_core::index_catalog::Membership {
                    project_id: grouping.project_id,
                    index_id: request.original_index,
                    revision: CatalogRevision::INITIAL,
                    scope: grouping.original_scope.clone(),
                },
                expected_scope_revision: CatalogRevision::INITIAL,
                expected_membership_revision: None,
            })?;
            CatalogRevision::INITIAL.next()?
        }
        SplitGroupingMode::ExistingNamedSet { scope_revision, .. } => scope_revision,
    };
    mutate(CatalogOperation::SetMembership {
        membership: airk_core::index_catalog::Membership {
            project_id: grouping.project_id,
            index_id: request.destination.index_id,
            revision: CatalogRevision::INITIAL,
            scope: grouping.destination_scope.clone(),
        },
        expected_scope_revision: scope_revision,
        expected_membership_revision: None,
    })?;
    scope_revision = scope_revision.next()?;
    let members:u32=db.query_row("SELECT count(*) FROM project_index_memberships WHERE project_id=?1 AND index_id IN (?2,?3)",params![grouping.project_id.to_string(),request.original_index.to_string(),request.destination.index_id.to_string()],|row|row.get(0))?;
    let saved_scope: i64 = db.query_row(
        "SELECT scope_revision FROM query_projects WHERE id=?1",
        [grouping.project_id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        members == 2 && u64::try_from(saved_scope)? == scope_revision.get(),
        "split grouping memberships are incomplete"
    );
    Ok(())
}

impl ServiceRegistry {
    /// Preview pins a single source-associated set or a fresh operation identity.
    pub fn catalog_review_split_grouping(
        &self,
        original: IndexId,
        grouping: SplitGrouping,
    ) -> Result<SplitGrouping> {
        super::super::validate_existing(&self.connection)?;
        resolve(&self.connection, original, grouping)
    }
}
