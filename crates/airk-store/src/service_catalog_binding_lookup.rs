// SPDX-License-Identifier: Apache-2.0
//! Exact owning binding lookup for native observations; never a library walk.
use super::{ServiceRegistry, operations::index_record, state, validate_existing};
use airk_core::{index_catalog::IndexDefinition, service::ProjectBinding};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
impl ServiceRegistry {
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
