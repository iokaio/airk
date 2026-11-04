// SPDX-License-Identifier: Apache-2.0
//! Complete physical-format component fixtures, NOT managed activation proofs.
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_store::service::registry::{ServiceRegistry, catalog::upgrade::CatalogUpgrade};
use anyhow::Result;
use std::path::Path;

pub(crate) fn authority_one(root: &Path, instance: Incarnation) -> Result<ServiceRegistry> {
    crate::local_ipc::prepare_root(root)?;
    drop(ServiceRegistry::open(root, instance)?);
    let upgrade = CatalogUpgrade::acquire(root)?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, Sha256Digest::from_bytes([2; 32]))?;
    upgrade.migrate()?;
    drop(upgrade);
    // Real owned migration installs ALL compiled capacity/write barriers.
    // Apply the complete compiled authority-1 physical format exactly once;
    // setting its marker alone would (correctly) fail strict layout validation.
    let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute_batch(include_str!(
        "../../airk-store/src/service_catalog_authority_v1.sql"
    ))?;
    tx.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    tx.commit()?;
    drop(sql);
    // Explicit isolated component settlement, identical to the original
    // coordinator fixture. This does NOT qualify installer/native readiness,
    // lineage promotion or managed activation. Never repair file permissions.
    let marker = root.join("service-1.4.owner.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&marker)?)?;
    record.as_object_mut().unwrap().remove("catalog_upgrade");
    std::fs::write(marker, serde_json::to_vec(&record)?)?;
    ServiceRegistry::open(root, instance)
}
