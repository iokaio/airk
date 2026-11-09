// SPDX-License-Identifier: Apache-2.0
//! Required persistent runtime semantics. The existing SQLite version marker
//! rejects old canonical Store access before actual source/index work. An old
//! main process may remain available in diagnostic-only mode.
use super::{ServiceRegistry, catalog};
use airk_core::{
    catalog_fences::{FenceIntent, FencedReceipt, WorkerProviderHistory},
    identity::Incarnation,
    index_catalog::CatalogCapabilities,
};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

pub const IN_PROCESS_SCHEMA: u32 = 13;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredCapabilities {
    pub minimum_registry_schema: u32,
    pub authority_contract: u32,
    pub policy_contract: u32,
}
#[derive(Clone, Debug)]
pub struct RuntimeContractUnsupported {
    pub required: RequiredCapabilities,
    pub supported: CatalogCapabilities,
}
impl std::fmt::Display for RuntimeContractUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "runtime_state_contract_unsupported: stored state requires registry {}, authority {}, policy {}; candidate supports {}, {}, {}; preserve the current runtime and use a compatible payload or explicit validated snapshot restoration",
            self.required.minimum_registry_schema,
            self.required.authority_contract,
            self.required.policy_contract,
            self.supported.maximum_registry_schema,
            self.supported.authority_contract,
            self.supported.policy_contract
        )
    }
}
impl std::error::Error for RuntimeContractUnsupported {}
impl RequiredCapabilities {
    pub fn require_supported(&self, candidate: &CatalogCapabilities) -> Result<()> {
        if !candidate.supports(
            self.minimum_registry_schema,
            self.authority_contract,
            self.policy_contract,
        ) {
            return Err(RuntimeContractUnsupported {
                required: self.clone(),
                supported: candidate.clone(),
            }
            .into());
        }
        Ok(())
    }
}

/// Bounded complete read, including terminal fence receipts. No model files,
/// source paths, state repair, history rewriting or engine migration occurs.
pub fn required_capabilities(control: &Path) -> Result<RequiredCapabilities> {
    let control = control.canonicalize()?;
    let path = control.join("service-1.4.sqlite3");
    if !path.try_exists()? {
        ensure!(
            !control.join("service-1.4.owner.json").try_exists()?,
            "runtime contract registry missing with retained owner"
        );
        return Ok(RequiredCapabilities {
            minimum_registry_schema: 0,
            authority_contract: 0,
            policy_contract: 0,
        });
    }
    let registry = ServiceRegistry::open_saved(&control)?;
    let tx = registry.connection.unchecked_transaction()?;
    let mut required = RequiredCapabilities {
        minimum_registry_schema: tx.pragma_query_value(None, "user_version", |row| row.get(0))?,
        authority_contract: 0,
        policy_contract: 0,
    };
    if let Some(state) = registry.catalog_state()? {
        required.authority_contract = state.authority_contract;
        required.policy_contract = state.policy_contract;
        let mut statement=tx.prepare("SELECT stage,substr(payload,1,16385) FROM index_operation_receipts WHERE kind='catalog_management_fenced' ORDER BY id LIMIT 4097")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(
                count <= 4096,
                "runtime contract complete fence roster exceeds bound"
            );
            let stage: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            ensure!(
                bytes.len() <= 16384,
                "runtime contract fence receipt exceeds bound"
            );
            let obligation = if stage == "metadata_applied" {
                let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
                receipt.validate()?;
                receipt.proof.worker_obligation
            } else {
                let intent: FenceIntent = serde_json::from_slice(&bytes)?;
                intent.validate()?;
                ensure!(
                    intent.stage.as_str() == stage,
                    "runtime contract fence stage changed"
                );
                intent.plan.worker_obligation
            };
            if obligation.is_some_and(|o| {
                o.providers
                    .iter()
                    .any(|p| matches!(&p.history, WorkerProviderHistory::InProcess { .. }))
            }) {
                required.minimum_registry_schema =
                    required.minimum_registry_schema.max(IN_PROCESS_SCHEMA);
            }
        }
    }
    tx.commit()?;
    Ok(required)
}

/// The caller owns this same existing transaction. Promotion and the first
/// new-format fence row either commit together or both roll back. Never restore
/// schema 12 by relabeling new history as helper-owned state.
pub(crate) fn arm_in_transaction(connection: &Connection) -> Result<()> {
    ensure!(
        !connection.is_autocommit(),
        "runtime contract promotion requires an explicit native transaction"
    );
    catalog::validate_existing(connection)?;
    let state = catalog::state(connection)?;
    ensure!(
        state.activated && state.authority_contract == 1,
        "runtime contract promotion requires qualified active authority 1"
    );
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        matches!(version, 12 | 13),
        "runtime contract promotion requires catalog schema 12 or 13"
    );
    if version == 12 {
        connection.pragma_update(None, "user_version", IN_PROCESS_SCHEMA)?;
        connection.execute(
            "UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1",
            [],
        )?;
    }
    Ok(())
}

/// Mutation-only prewrite guard for a real native model/default publication.
/// This does not create a registry, migrate a predecessor, provision/warm a model
/// or open source/index data. A read-only embedded binding must never call it.
pub fn require_embedded_default_write(control: &Path) -> Result<()> {
    require_embedded_default_write_for_namespace(control, super::retained_namespace(control)?)
}

pub fn require_embedded_default_write_for_namespace(
    control: &Path,
    expected: Incarnation,
) -> Result<()> {
    let control = control.canonicalize()?;
    // Independent projects can publish their first embedded model concurrently.
    // Wait only for this existing compatibility fence, before opening a write
    // transaction; never replay a model job or create missing ownership state.
    let _initialization = super::super::acquire_existing_file_for(
        &control,
        "service-1.4.initialize.lock",
        Duration::from_secs(2),
    )?;
    // Reuse the canonical completed-owner, overlay, privacy and layout checks.
    // Drop this read connection before taking write admission.
    let saved = ServiceRegistry::open_saved(&control)?;
    ensure!(
        saved.incarnation()? == expected,
        "embedded model publication namespace changed"
    );
    drop(saved);
    let mut connection = Connection::open_with_flags(
        control.join("service-1.4.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    connection.busy_timeout(Duration::from_secs(2))?;
    connection.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let namespace: String = tx.query_row(
        "SELECT incarnation FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        namespace.parse::<Incarnation>()? == expected,
        "embedded model publication namespace changed inside native transaction"
    );
    arm_in_transaction(&tx)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;
