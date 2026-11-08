// SPDX-License-Identifier: Apache-2.0
//! Empty-registry genesis and separately qualified authority-1 activation.
//! This never fabricates P5 predecessor readiness, grants access or scans sources.
use super::{backup, upgrade};
use crate::service::{EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::encoded_size,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const DATABASE: &str = "service-1.4.sqlite3";
const INTENT: &str = "index-catalog-genesis.json";
const MAX_RECEIPT_BYTES: usize = 16384;

#[path = "service_catalog_activation.rs"]
pub mod activation;
pub use activation::observe_authority_v1;
#[path = "service_catalog_genesis_origin.rs"]
mod origin;
pub use origin::{ManagedGenesisOrigin, preflight_managed_origin};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenesisStage {
    Admitted,
    BackedUp,
    Migrating,
    Migrated,
    Promoting,
    Promoted,
    Qualified,
    Activating,
    Settled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisReceipt {
    pub schema: u32,
    pub root: PathBuf,
    pub namespace: Incarnation,
    pub operation: Incarnation,
    pub installer_identity: Sha256Digest,
    pub legacy_schema_digest: Sha256Digest,
    pub legacy_data_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_origin: Option<ManagedGenesisOrigin>,
    pub stage: GenesisStage,
    pub backups: Vec<backup::BackupAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_v1: Option<activation::AuthorityV1ActivationRecord>,
}

impl GenesisReceipt {
    fn validate(&self, root: &Path) -> Result<()> {
        if let Some(origin) = &self.coordinator_origin {
            origin.require_binding(
                root,
                self.namespace,
                self.installer_identity,
                self.operation,
            )?;
        }
        ensure!(
            self.schema == 1 && self.root == root && root.is_absolute(),
            "foreign catalog genesis intent"
        );
        ensure!(
            self.backups.len() <= backup::MAX_ATTEMPTS,
            "genesis backup capacity exceeded"
        );
        let mut complete = 0;
        for (slot, attempt) in self.backups.iter().enumerate() {
            attempt.validate()?;
            ensure!(
                usize::from(attempt.slot) == slot,
                "noncanonical genesis backup slot"
            );
            complete += usize::from(attempt.complete);
        }
        ensure!(complete <= 1, "ambiguous genesis backup selection");
        ensure!(
            self.stage == GenesisStage::Admitted || complete == 1,
            "genesis stage lacks a qualified backup"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        if let Some(record) = &self.authority_v1 {
            activation::validate_genesis_record(self, record)?;
        }
        ensure!(
            !matches!(
                self.stage,
                GenesisStage::Qualified | GenesisStage::Activating | GenesisStage::Settled
            ) || self.authority_v1.is_some(),
            "genesis authority-1 qualification missing"
        );
        Ok(())
    }

    fn same_intent(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.root == other.root
            && self.namespace == other.namespace
            && self.operation == other.operation
            && self.installer_identity == other.installer_identity
            && self.legacy_schema_digest == other.legacy_schema_digest
            && self.legacy_data_digest == other.legacy_data_digest
            && self.coordinator_origin == other.coordinator_origin
    }
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Positive native read of the exact completed managed operation. A saved
/// `Settled` label alone never permits normal service startup.
pub fn require_managed_settled(root: &Path, origin: &ManagedGenesisOrigin) -> Result<()> {
    let receipt = pending(root)?.context("managed catalog admission must be resumed")?;
    ensure!(
        receipt.coordinator_origin.as_ref() == Some(origin)
            && receipt.stage == GenesisStage::Settled,
        "managed catalog operation is not settled"
    );
    activation::validate_live_completed_genesis(root, &receipt)
}

/// Read only the bounded saved intent. This neither acquires ownership nor
/// opens/initializes SQLite; callers still need the owned genesis adapter.
pub fn pending(root: &Path) -> Result<Option<GenesisReceipt>> {
    let root = root.canonicalize()?;
    let path = root.join(INTENT);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() <= MAX_RECEIPT_BYTES as u64,
        "genesis intent exceeds bound"
    );
    let file = fs::File::open(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &file.metadata()?)?;
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "genesis intent grew beyond bound"
    );
    let receipt: GenesisReceipt = serde_json::from_slice(&bytes)?;
    receipt.validate(&root)?;
    Ok(Some(receipt))
}

fn persist(receipt: &GenesisReceipt, initial: bool) -> Result<()> {
    receipt.validate(&receipt.root)?;
    ensure!(
        receipt.root.canonicalize()? == receipt.root,
        "redirected genesis root"
    );
    let path = receipt.root.join(INTENT);
    if !initial {
        let previous = pending(&receipt.root)?.context("genesis intent disappeared")?;
        ensure!(
            receipt.same_intent(&previous),
            "genesis intent was replaced"
        );
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&receipt.root)?;
    temporary.write_all(&serde_json::to_vec(receipt)?)?;
    if initial {
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(path)?;
        crate::maintenance::sync_parent(&receipt.root)
    } else {
        crate::durability::persist_synced(temporary, &path)
    }
}

fn no_competing_upgrade(root: &Path) -> Result<()> {
    ensure!(
        !exists(&root.join("index-catalog-upgrade.json"))?,
        "existing catalog upgrade requires its original recovery path"
    );
    super::super::data_relocation::require_settled(root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GenesisAdmission {
    Fenced,
    Committed,
}

fn admission(root: &Path, receipt: &GenesisReceipt) -> Result<GenesisAdmission> {
    no_competing_upgrade(root)?;
    let owner = super::super::owner_record(root, true, receipt.namespace)?;
    ensure!(
        owner.ready && owner.format == 2 && owner.incarnation == receipt.namespace,
        "genesis native owner identity changed"
    );
    if owner.catalog_upgrade == Some(receipt.operation) {
        return Ok(GenesisAdmission::Fenced);
    }
    if owner.catalog_upgrade.is_none()
        && matches!(
            receipt.stage,
            GenesisStage::Activating | GenesisStage::Settled
        )
    {
        activation::validate_completed_genesis(root, receipt)?;
        return Ok(GenesisAdmission::Committed);
    }
    anyhow::bail!("genesis admission fence changed")
}

fn require_fence(root: &Path, receipt: &GenesisReceipt) -> Result<()> {
    ensure!(
        admission(root, receipt)? == GenesisAdmission::Fenced,
        "genesis mutation/recovery requires its exact pending native fence"
    );
    Ok(())
}

fn refuse_overlays(root: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        ensure!(
            !exists(&root.join(format!("{DATABASE}{suffix}")))?,
            "unrecovered genesis registry overlay"
        );
    }
    Ok(())
}

fn registry(root: &Path, write: bool) -> Result<Connection> {
    refuse_overlays(root)?;
    let path = upgrade::validate_registry_files(root)?;
    let connection = Connection::open_with_flags(
        path,
        if write {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        },
    )?;
    connection.busy_timeout(Duration::from_millis(100))?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA cache_size=-8192;")?;
    if write {
        connection.execute_batch("PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;")?;
    } else {
        connection.execute_batch("PRAGMA query_only=ON;")?;
    }
    let application: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    ensure!(
        application == super::super::APPLICATION,
        "foreign genesis registry"
    );
    Ok(connection)
}

/// An empty catalog is not inferred from an empty projects list: history,
/// requests, lifecycle changes and consumed AUTOINCREMENT sequences also refuse.
fn empty_legacy(connection: &Connection, namespace: Incarnation) -> Result<()> {
    for object in super::expected(11)?
        .iter()
        .filter(|object| object.kind == "table")
    {
        if matches!(
            object.name.as_str(),
            "service_meta" | "service_lifecycle_state"
        ) {
            continue;
        }
        let empty: bool = connection.query_row(
            &format!(
                "SELECT NOT EXISTS(SELECT 1 FROM \"{}\" LIMIT 1)",
                object.name
            ),
            [],
            |row| row.get(0),
        )?;
        ensure!(
            empty,
            "populated registry requires the real catalog upgrade"
        );
    }
    let meta: (i64, String, i64, i64) = connection.query_row(
        "SELECT id,incarnation,revision,request_epoch FROM service_meta",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let lifecycle: (i64, i64, Option<String>) = connection.query_row(
        "SELECT id,stopped,instance FROM service_lifecycle_state",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let singleton_counts: (i64, i64) = connection.query_row(
        "SELECT (SELECT count(*) FROM service_meta),(SELECT count(*) FROM service_lifecycle_state)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let sequences: i64 =
        connection.query_row("SELECT count(*) FROM sqlite_sequence", [], |row| row.get(0))?;
    ensure!(
        meta == (1, namespace.to_string(), 0, 0)
            && lifecycle == (1, 0, None)
            && singleton_counts == (1, 1)
            && sequences == 0,
        "changed registry singleton/history requires the real catalog upgrade"
    );
    Ok(())
}

fn verify_legacy(connection: &Connection, receipt: &GenesisReceipt) -> Result<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 11
            && matches!(
                receipt.stage,
                GenesisStage::Admitted | GenesisStage::BackedUp | GenesisStage::Migrating
            ),
        "genesis requires its canonical schema 11 source"
    );
    super::validate_shape(connection, 11)?;
    empty_legacy(connection, receipt.namespace)?;
    ensure!(
        super::schema_digest(connection)? == receipt.legacy_schema_digest
            && backup::legacy_data_digest(connection)? == receipt.legacy_data_digest,
        "genesis source changed after admission"
    );
    Ok(())
}

/// Owns the same exclusion order as legacy upgrades. Dropping this adapter does
/// not clear its durable admission fence. Ordinary runtime admission requires
/// the separate native producer qualification and authority-1 settlement.
pub struct CatalogGenesis {
    root: PathBuf,
    intent: GenesisReceipt,
    _workers: Vec<ProjectLease>,
    _watcher: SourceWatchLease,
    _embedding: EmbeddingLease,
    service: ServiceLease,
}

impl CatalogGenesis {
    pub fn begin(
        service: ServiceLease,
        installer: Sha256Digest,
        operation: Incarnation,
    ) -> Result<Self> {
        Self::begin_with_origin(service, installer, operation, None)
    }

    /// Native managed admission. Origin bytes identify the exact reservation;
    /// the caller still owns payload approval and actual process quiescence.
    pub fn begin_managed(service: ServiceLease, origin: &ManagedGenesisOrigin) -> Result<Self> {
        origin.require_binding(
            service.root(),
            origin.namespace,
            origin.installer_identity,
            origin.operation,
        )?;
        Self::begin_with_origin(
            service,
            origin.installer_identity,
            origin.operation,
            Some(origin.clone()),
        )
    }

    fn begin_with_origin(
        service: ServiceLease,
        installer: Sha256Digest,
        operation: Incarnation,
        coordinator_origin: Option<ManagedGenesisOrigin>,
    ) -> Result<Self> {
        let root = service.root().to_path_buf();
        ensure!(
            root.canonicalize()? == root,
            "redirected genesis control root"
        );
        no_competing_upgrade(&root)?;
        let embedding = EmbeddingLease::acquire(&root)?;
        let watcher = SourceWatchLease::acquire(&root)?;
        let workers = upgrade::genesis_worker_leases(&root)?;
        let saved = pending(&root)?;
        let intent = if let Some(receipt) = saved {
            ensure!(
                receipt.operation == operation && receipt.installer_identity == installer,
                "genesis belongs to another operation or installer"
            );
            ensure!(
                receipt.coordinator_origin == coordinator_origin,
                "genesis coordinator origin changed"
            );
            receipt
        } else {
            refuse_overlays(&root)?;
            let existing = exists(&root.join(DATABASE))?;
            let registry = if existing {
                // Never migrate an existing registry merely to ask if it is empty.
                super::super::ServiceRegistry::open_saved(&root)?
            } else {
                // Existing ready/protected owner with a missing database refuses
                // inside the canonical initializer; it is never recreated here.
                super::super::ServiceRegistry::open(&root, operation)?
            };
            let namespace = registry.incarnation()?;
            if let Some(origin) = &coordinator_origin {
                origin.require_binding(&root, namespace, installer, operation)?;
            }
            let version: u32 =
                registry
                    .connection
                    .pragma_query_value(None, "user_version", |row| row.get(0))?;
            ensure!(
                version == 11,
                "fresh genesis refuses an existing catalog or earlier registry format"
            );
            super::validate_shape(&registry.connection, 11)?;
            empty_legacy(&registry.connection, namespace)?;
            let owner = super::super::owner_record(&root, true, namespace)?;
            ensure!(
                owner.ready && owner.format == 2 && owner.catalog_upgrade.is_none(),
                "unexplained genesis admission fence"
            );
            let receipt = GenesisReceipt {
                schema: 1,
                root: root.clone(),
                namespace,
                operation,
                installer_identity: installer,
                legacy_schema_digest: super::schema_digest(&registry.connection)?,
                legacy_data_digest: backup::legacy_data_digest(&registry.connection)?,
                coordinator_origin,
                stage: GenesisStage::Admitted,
                backups: Vec::new(),
                authority_v1: None,
            };
            persist(&receipt, true)?;
            receipt
        };
        let mut owner = super::super::owner_record(&root, true, intent.namespace)?;
        ensure!(
            owner.ready && owner.format == 2 && owner.incarnation == intent.namespace,
            "genesis namespace changed"
        );
        if owner.catalog_upgrade.is_none() && intent.stage == GenesisStage::Admitted {
            let read = registry(&root, false)?;
            verify_legacy(&read, &intent)?;
            owner.catalog_upgrade = Some(intent.operation);
            owner.complete(&root)?;
        }
        let admitted = admission(&root, &intent)?;
        if admitted == GenesisAdmission::Fenced
            && matches!(
                intent.stage,
                GenesisStage::Promoting
                    | GenesisStage::Promoted
                    | GenesisStage::Qualified
                    | GenesisStage::Activating
                    | GenesisStage::Settled
            )
        {
            activation::recover_genesis_v1(&root, &intent)?;
        } else if admitted == GenesisAdmission::Fenced {
            upgrade::recover_owned_registry(
                &root,
                intent.operation,
                intent.namespace,
                Some(intent.legacy_data_digest),
                &intent.backups,
                intent.stage == GenesisStage::Migrating,
            )?;
        }
        let result = Self {
            root,
            intent,
            _workers: workers,
            _watcher: watcher,
            _embedding: embedding,
            service,
        };
        result.validate_current()?;
        Ok(result)
    }

    pub fn receipt(&self) -> Result<GenesisReceipt> {
        let receipt = pending(&self.root)?.context("genesis intent missing")?;
        ensure!(receipt.same_intent(&self.intent), "genesis intent changed");
        admission(&self.root, &receipt)?;
        Ok(receipt)
    }

    fn validate_current(&self) -> Result<()> {
        let receipt = self.receipt()?;
        if matches!(
            receipt.stage,
            GenesisStage::Promoting
                | GenesisStage::Promoted
                | GenesisStage::Qualified
                | GenesisStage::Activating
                | GenesisStage::Settled
        ) {
            return activation::validate_owned_genesis(&self.root, &receipt);
        }
        let read = registry(&self.root, false)?;
        let version: u32 = read.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 11 {
            verify_legacy(&read, &receipt)?;
        } else {
            self.verify_migrated(&read, &receipt)?;
        }
        if let Some(attempt) = receipt.backups.iter().find(|attempt| attempt.complete) {
            backup::validate(&self.root, receipt.operation, attempt)?;
        }
        Ok(())
    }

    fn verify_migrated(&self, read: &Connection, receipt: &GenesisReceipt) -> Result<()> {
        ensure!(
            matches!(
                receipt.stage,
                GenesisStage::Migrating | GenesisStage::Migrated | GenesisStage::Promoting
            ),
            "unexpected catalog before genesis migration"
        );
        super::validate_existing(read)?;
        let state = super::state(read)?;
        let attempt = receipt
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified genesis backup missing")?;
        ensure!(
            state.namespace == receipt.namespace
                && state.upgrade_id == receipt.operation
                && state.revision == airk_core::index_catalog::CatalogRevision::INITIAL
                && !state.activated
                && !state.accepted_new_writes
                && state.authority_contract == 0
                && state.policy_contract == 0
                && state.legacy_schema_digest == receipt.legacy_schema_digest
                && Some(state.backup_digest) == attempt.sha256,
            "genesis catalog identity or admission changed"
        );
        empty_legacy(read, receipt.namespace)?;
        ensure!(
            backup::legacy_data_digest(read)? == receipt.legacy_data_digest,
            "genesis changed legacy rows"
        );
        for object in super::expected(12)?
            .iter()
            .filter(|object| object.kind == "table")
        {
            if object.name == "index_catalog_meta"
                || super::expected(11)?
                    .iter()
                    .any(|old| old.kind == "table" && old.name == object.name)
            {
                continue;
            }
            let empty: bool = read.query_row(
                &format!(
                    "SELECT NOT EXISTS(SELECT 1 FROM \"{}\" LIMIT 1)",
                    object.name
                ),
                [],
                |row| row.get(0),
            )?;
            ensure!(empty, "genesis catalog has accepted user metadata");
        }
        Ok(())
    }

    pub fn backup(&mut self) -> Result<GenesisReceipt> {
        self.validate_current()?;
        let mut receipt = self.receipt()?;
        if receipt.backups.iter().any(|attempt| attempt.complete) {
            if receipt.stage == GenesisStage::Admitted {
                receipt.stage = GenesisStage::BackedUp;
                persist(&receipt, false)?;
            }
            return Ok(receipt);
        }
        ensure!(
            receipt.stage == GenesisStage::Admitted,
            "genesis backup is not admitted"
        );
        let read = registry(&self.root, false)?;
        read.execute_batch("BEGIN;")?;
        verify_legacy(&read, &receipt)?;
        loop {
            let slot = if let Some(slot) = receipt
                .backups
                .iter()
                .position(|attempt| !attempt.abandoned)
            {
                slot
            } else {
                ensure!(
                    receipt.backups.len() < backup::MAX_ATTEMPTS,
                    "genesis backup attempts exhausted; unknown files preserved"
                );
                receipt
                    .backups
                    .push(backup::BackupAttempt::reserved(receipt.backups.len() as u8));
                persist(&receipt, false)?;
                receipt.backups.len() - 1
            };
            let mut attempt = receipt.backups[slot].clone();
            let captured = backup::capture(
                &self.root,
                receipt.operation,
                &read,
                receipt.legacy_data_digest,
                &mut attempt,
                |current| {
                    receipt.backups[slot] = current.clone();
                    persist(&receipt, false)
                },
            )?;
            if captured {
                receipt.stage = GenesisStage::BackedUp;
                persist(&receipt, false)?;
                return Ok(receipt);
            }
            receipt.backups[slot].abandoned = true;
            persist(&receipt, false)?;
        }
    }

    pub fn migrate(&mut self) -> Result<GenesisReceipt> {
        self.migrate_at(|_, _| Ok(()))
    }

    fn migrate_at(
        &mut self,
        mut checkpoint: impl FnMut(&Connection, &str) -> Result<()>,
    ) -> Result<GenesisReceipt> {
        self.validate_current()?;
        let mut receipt = self.receipt()?;
        let mut write = registry(&self.root, true)?;
        let version: u32 = write.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if matches!(version, 12 | 13) {
            self.verify_migrated(&write, &receipt)?;
            receipt.stage = GenesisStage::Migrated;
            persist(&receipt, false)?;
            return Ok(receipt);
        }
        ensure!(
            matches!(
                receipt.stage,
                GenesisStage::BackedUp | GenesisStage::Migrating
            ),
            "genesis migration requires its qualified backup"
        );
        let attempt = receipt
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified genesis backup missing")?;
        backup::validate(&self.root, receipt.operation, attempt)?;
        let digest = attempt.sha256.context("genesis backup digest missing")?;
        receipt.stage = GenesisStage::Migrating;
        persist(&receipt, false)?;
        checkpoint(&write, "before_transaction")?;
        let tx = write.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_fence(&self.root, &receipt)?;
        verify_legacy(&tx, &receipt)?;
        super::install_schema(&tx)?;
        tx.execute(
            "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
            params![
                receipt.operation.to_string(),
                receipt.legacy_schema_digest.to_string(),
                digest.to_string()
            ],
        )?;
        tx.pragma_update(None, "user_version", 12)?;
        checkpoint(&tx, "before_commit")?;
        require_fence(&self.root, &receipt)?;
        ensure!(
            backup::legacy_data_digest(&tx)? == receipt.legacy_data_digest,
            "genesis transaction changed legacy rows"
        );
        self.verify_migrated(&tx, &receipt)?;
        tx.commit()?;
        checkpoint(&write, "after_commit")?;
        receipt.stage = GenesisStage::Migrated;
        persist(&receipt, false)?;
        Ok(receipt)
    }

    /// This transfers only process ownership. Any pending fence remains
    /// authoritative; ordinary supervisors and connections require completed
    /// candidate readiness, authority activation and durable settlement.
    pub fn into_service_lease(self) -> ServiceLease {
        self.service
    }
}

#[cfg(test)]
#[path = "service_catalog_genesis_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "service_catalog_genesis_managed_tests.rs"]
mod managed_tests;
