// SPDX-License-Identifier: Apache-2.0
//! Upgrade intent construction only; normal query/setup paths do not call this.
use airk_core::{
    identity::Sha256Digest,
    index_catalog::{IndexId, QueryProjectId, SourceRootId},
    source_policy::SourceRoot,
};
use airk_store::service::registry::catalog::upgrade::{
    self, BackfillIndex, CatalogUpgrade, UpgradePlan, UpgradeReceipt,
};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

/// Random identities are persisted under exclusive admission before any database
/// backfill. Reopening the same intent reuses every identity, including roots.
pub fn prepare(owner: &CatalogUpgrade, installer_identity: Sha256Digest) -> Result<UpgradeReceipt> {
    if let Some(receipt) = upgrade::pending(owner.control())? {
        ensure!(
            receipt.plan.installer_identity == installer_identity,
            "catalog upgrade belongs to another verified installer payload"
        );
        return owner.admit(receipt.plan);
    }
    let mut indexes = Vec::new();
    for registration in owner.registrations() {
        let configuration = owner.source_configuration(&registration)?;
        // Multiple aliases of exactly the same configured canonical path share
        // one immutable source identity. Never canonicalize or scan source roots.
        let mut paths = BTreeMap::<String, Vec<String>>::new();
        paths.insert(registration.root.clone(), Vec::new());
        for (alias, path) in configuration.aliases {
            paths.entry(path).or_default().push(alias);
        }
        let mut sources = Vec::new();
        for (canonical_path, aliases) in paths {
            sources.push(SourceRoot {
                id: SourceRootId::from_identity(crate::service_broker::new_instance_id()?),
                owner: canonical_path == registration.root,
                canonical_path,
                aliases,
            });
        }
        indexes.push(BackfillIndex {
            registration,
            index_id: IndexId::from_identity(crate::service_broker::new_instance_id()?),
            project_id: QueryProjectId::from_identity(crate::service_broker::new_instance_id()?),
            sources,
            source_import: configuration.source_import,
            configuration_digest: configuration.digest,
        });
    }
    let plan = UpgradePlan {
        schema: 1,
        id: crate::service_broker::new_instance_id()?,
        control: owner.control().into(),
        namespace: owner.namespace(),
        installer_identity,
        legacy_schema_digest: owner.source_schema_digest()?,
        registrations_digest: owner.registration_digest()?,
        indexes,
    };
    owner.admit(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation, index_catalog::SourceImportState, service::ProjectRegistration,
    };
    use airk_store::service::registry::ServiceRegistry;
    use rusqlite::{Connection, params};

    #[test]
    fn saved_random_ids_survive_restart_without_inventing_missing_source_configuration()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        drop(ServiceRegistry::open(
            root.path(),
            Incarnation::from_bytes([1; 16]),
        )?);
        let connection = Connection::open(root.path().join("service-1.4.sqlite3"))?;
        for name in ["one", "two"] {
            let registration = ProjectRegistration::new(
                format!("/fictional/{name}"),
                format!("/fictional/private/{name}"),
                name.into(),
            )?;
            connection.execute(
                "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,0,0,1)",
                params![
                    registration.binding.to_string(),
                    serde_json::to_vec(&registration)?
                ],
            )?;
        }
        drop(connection);
        let owner = CatalogUpgrade::acquire(root.path())?;
        let payload = Sha256Digest::from_bytes([2; 32]);
        let receipt = prepare(&owner, payload)?;
        assert_eq!(receipt.plan.indexes.len(), 2);
        assert_ne!(
            receipt.plan.indexes[0].index_id,
            receipt.plan.indexes[1].index_id
        );
        for index in &receipt.plan.indexes {
            assert_eq!(index.source_import, SourceImportState::PendingMetadata);
            assert_eq!(index.sources.len(), 1);
            assert_eq!(index.sources[0].canonical_path, index.registration.root);
        }
        drop(owner);
        let owner = CatalogUpgrade::acquire(root.path())?;
        assert_eq!(prepare(&owner, payload)?.plan, receipt.plan);
        assert!(prepare(&owner, Sha256Digest::from_bytes([3; 32])).is_err());
        assert!(ServiceRegistry::open_saved(root.path()).is_err());
        Ok(())
    }
}
