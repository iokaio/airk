// SPDX-License-Identifier: Apache-2.0
//! Selected old collection reset; no old database content is opened.
use super::*;
use crate::current::legacy_retirement::{Witness, witness};
use crate::service::{
    ProjectLease,
    registry::{ServiceRegistry, fresh_setup_namespaces},
};
use std::collections::BTreeMap;
const RECEIPT: &str = "fresh-storage-reset.json";
const OLD_FILES: &[&str] = &[
    "current.sqlite3-wal",
    "current.sqlite3-shm",
    "current.sqlite3-journal",
    "current.sqlite3",
    "current-owner.json",
    "current-bootstrap.json",
    "current-runtime-bootstrap.json",
    "fts5-rebuild.json",
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reset {
    version: u32,
    owner: Sha256Digest,
    slot: Incarnation,
    namespace: Incarnation,
    scope: crate::current::CurrentScope,
    files: BTreeMap<String, Witness>,
    complete: bool,
}
impl Collection {
    /// Requires the currently admitted project process and a completed service reset.
    /// Separately stored settings, objects and unknown files remain in place.
    pub fn reset_preceding_storage(&self, lease: &ProjectLease) -> Result<bool> {
        let registry = ServiceRegistry::open_saved(&lease.root)?;
        let registration = registry.validate_active_worker(lease)?;
        ensure!(
            registration.root == self.owner.root
                && registration.collection == self.owner.collection
                && registration.data_root == self.owner.data_root,
            "fresh setup project differs"
        );
        let Some((ownership, namespace)) = fresh_setup_namespaces(&lease.root)? else {
            return Ok(false);
        };
        ensure!(
            registry.incarnation()? == ownership && registry.saved_request_epoch()?.0 == namespace,
            "fresh setup registry changed"
        );
        let _replacement = acquire_file(&self.directory, "replacement.lock")?;
        let _manifest = manifest_lock(&self.directory)?;
        let manifest = self.manifest()?;
        ensure!(
            !self.directory.join("replacement.json").try_exists()?,
            "pending rebuild must settle before fresh setup"
        );
        let store = FileStore::new(self.slot(manifest.active));
        let writer = store.lock()?;
        let _gate = Gate::exclusive(&store, &writer)?;
        let receipt_path = self.directory.join(RECEIPT);
        let mut reset: Reset = if receipt_path.try_exists()? {
            let value: Reset = json(&receipt_path)?;
            ensure!(
                value.version == 1
                    && value.owner == self.binding
                    && value.slot == manifest.active
                    && value.namespace == namespace,
                "fresh collection reset authority changed"
            );
            if value.complete {
                return Ok(false);
            }
            value
        } else {
            if !store.directory().join("current-owner.json").try_exists()? {
                return Ok(false);
            }
            if crate::current::database::redb_owner(store.directory())? {
                return Ok(false);
            }
            let scope = crate::current::database::legacy_retirement_scope(store.directory())?;
            ensure!(
                scope.incarnation == manifest.active
                    && scope.root == self.owner.root
                    && scope.collection == self.owner.collection,
                "foreign old collection ownership"
            );
            ensure!(
                !store.directory().join("current.airk").try_exists()?,
                "native storage cannot be reset as old storage"
            );
            let mut files = BTreeMap::new();
            for name in OLD_FILES {
                let path = store.directory().join(name);
                if path.try_exists()? {
                    files.insert((*name).into(), witness(&path)?);
                }
            }
            for name in [
                "usage.sqlite3-wal",
                "usage.sqlite3-shm",
                "usage.sqlite3-journal",
                "usage.sqlite3",
            ] {
                let path = self.directory.join(name);
                if path.try_exists()? {
                    files.insert(format!("usage/{name}"), witness(&path)?);
                }
            }
            let value = Reset {
                version: 1,
                owner: self.binding,
                slot: manifest.active,
                namespace,
                scope,
                files,
                complete: false,
            };
            write(self, RECEIPT, &value)?;
            value
        };
        let resolve = |name: &str| -> Result<PathBuf> {
            let path = if let Some(name) = name.strip_prefix("usage/") {
                ensure!(
                    [
                        "usage.sqlite3",
                        "usage.sqlite3-wal",
                        "usage.sqlite3-shm",
                        "usage.sqlite3-journal"
                    ]
                    .contains(&name),
                    "unknown usage reset target"
                );
                self.directory.join(name)
            } else {
                ensure!(OLD_FILES.contains(&name), "unknown collection reset target");
                store.directory().join(name)
            };
            ensure!(
                path.is_absolute()
                    && (path.parent() == Some(store.directory())
                        || path.parent() == Some(self.directory.as_path())),
                "fresh reset target escaped selected collection"
            );
            Ok(path)
        };
        let mut present = Vec::new();
        for (name, expected) in &reset.files {
            let path = resolve(name)?;
            if path.try_exists()? {
                ensure!(
                    witness(&path)? == *expected,
                    "old derived file changed after reset receipt"
                );
                present.push(path);
            }
        }
        for path in present {
            fs::remove_file(path)?;
            crate::maintenance::sync_parent(store.directory())?;
        }
        let quota = Quota::open_service(Path::new(&self.owner.data_root))?;
        for grant in quota.current_grants(&writer)? {
            ensure!(
                grant.scope().incarnation == reset.scope.incarnation,
                "foreign old collection capacity grant"
            );
            grant.release(&writer)?;
        }
        reset.complete = true;
        write(self, RECEIPT, &reset)?;
        Ok(true)
    }
}
