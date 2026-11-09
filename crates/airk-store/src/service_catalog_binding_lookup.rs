// SPDX-License-Identifier: Apache-2.0
//! Exact owning binding lookup for native observations; never a library walk.
use super::{ServiceRegistry, operations::index_record, state, validate_existing};
use airk_core::{
    index_catalog::{CatalogRevision, IndexDefinition, SourceImportState},
    service::ProjectBinding,
    source_policy::{SourcePolicy, SourceRoot},
};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
pub type CleanBreakSource = (IndexDefinition, Vec<SourceRoot>, Option<SourcePolicy>);
impl ServiceRegistry {
    /// Complete metadata-only source census for a reviewed clean break. Old
    /// indexes are never opened, and retired registrations are not resurrected.
    pub fn catalog_clean_break_sources(
        &self,
        expected: CatalogRevision,
    ) -> Result<Vec<CleanBreakSource>> {
        let read = self.connection.unchecked_transaction()?;
        validate_existing(&read)?;
        let catalog = state(&read)?;
        ensure!(
            catalog.activated && catalog.authority_contract == 1 && catalog.revision == expected,
            "clean-break source catalog changed or is unavailable"
        );
        let mut statement = read.prepare("SELECT i.id FROM index_definitions i JOIN projects p ON p.binding=i.active_registration WHERE i.tombstone=0 AND p.registered=1 ORDER BY i.id LIMIT 65")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        ensure!(
            ids.len() <= 64,
            "clean-break complete index roster exceeds 64 indexes"
        );
        let mut result = Vec::with_capacity(ids.len());
        let mut bindings = std::collections::BTreeSet::new();
        for id in ids {
            let index = index_record(&read, id.parse()?)?;
            ensure!(
                bindings.insert(index.backing.binding),
                "clean-break logical indexes share an unsupported physical backing"
            );
            ensure!(
                index.source_import == SourceImportState::Complete,
                "clean-break saved source selection is incomplete"
            );
            let roots = super::policy::source_roots(&read, index.id)?;
            let policy = super::policy::snapshot(&read, index.id)?.map(|saved| saved.policy);
            if let Some(policy) = &policy {
                ensure!(
                    policy.roots == roots,
                    "clean-break source selection differs from saved policy"
                );
            }
            result.push((index, roots, policy));
        }
        read.commit()?;
        Ok(result)
    }

    /// Current native backing only. Administrative inactive/tombstoned entries
    /// remain available through the canonical catalog detail contract.
    pub fn catalog_index_for_binding(
        &self,
        binding: ProjectBinding,
    ) -> Result<Option<IndexDefinition>> {
        let read = self.connection.unchecked_transaction()?;
        validate_existing(&read)?;
        let catalog = state(&read)?;
        ensure!(
            catalog.activated
                && catalog.authority_contract == 1
                && catalog.namespace == self.incarnation()?,
            "binding lookup requires the active native catalog namespace"
        );
        let row:Option<(String,Option<String>,bool)>=read.query_row("SELECT i.id,i.active_registration,p.registered FROM index_definitions i LEFT JOIN projects p ON p.binding=i.active_registration WHERE i.legacy_binding=?1 AND i.tombstone=0",[binding.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        let result = match row {
            None => None,
            Some((id, active, registered)) => {
                ensure!(
                    active.as_deref() == Some(binding.to_string().as_str()),
                    "index active backing binding differs"
                );
                let index = index_record(&read, id.parse()?)?;
                ensure!(
                    index.backing.binding == binding,
                    "index immutable backing identity differs"
                );
                let retained = self.retained_project(binding)?;
                ensure!(
                    retained.registration == index.backing,
                    "native retained registration differs from index backing"
                );
                registered.then_some(index)
            }
        };
        read.commit()?;
        Ok(result)
    }
}
