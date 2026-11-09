// SPDX-License-Identifier: Apache-2.0
//! Explicit offline catalog upgrade admission. No ordinary read calls this adapter.
use super::{schema_digest, validate_shape};
use crate::service::{EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease};
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{
        IndexId, MAX_DEFINITIONS, MAX_SOURCE_ROOTS, QueryProjectId, SourceImportState,
    },
    service::ProjectRegistration,
    source_policy::SourceRoot,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub use super::backup::{BackupAttempt, FileIdentity};
pub use super::wal::WalObservation;

const DATABASE: &str = "service-1.4.sqlite3";
const INTENT: &str = "index-catalog-upgrade.json";
const MAX_INTENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 16384;
const MAX_WORKER_LOCKS: usize = 4096;

/// A recognized registry application advertises a schema outside the catalog
/// migration reader's supported range. This is a diagnostic, not rebuild
/// authority: the version marker does not authenticate a predecessor's complete
/// schema, source definitions, settings or process ownership. Recovery must
/// establish those independently before accepting any replacement state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogMigrationSourceUnsupported {
    pub observed_registry_schema: u32,
    pub minimum_registry_schema: u32,
    pub maximum_registry_schema: u32,
}

impl std::fmt::Display for CatalogMigrationSourceUnsupported {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("unsupported catalog upgrade source format")
    }
}

impl std::error::Error for CatalogMigrationSourceUnsupported {}

#[cfg(test)]
#[path = "service_catalog_upgrade_tests.rs"]
mod tests;

#[path = "service_catalog_import.rs"]
mod imports;

#[path = "service_catalog_settlement.rs"]
mod settlement;
pub use settlement::{
    NativeReadRefusalEntrypoint, NativeRefusalDiagnostic, NativeRefusalEntrypoints,
    NativeRefusalProvenance, NativeWriteRefusalEntrypoint, ReadinessEvidence, RuntimeRefusal,
};

#[path = "service_catalog_compatibility.rs"]
pub mod compatibility;
pub use compatibility::activation as authority_v1;

#[path = "service_catalog_upgrade_recovery.rs"]
pub mod recovery;

#[path = "service_catalog_exclusion.rs"]
mod exclusion;
pub use exclusion::PendingCatalogExclusion;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackfillIndex {
    pub registration: ProjectRegistration,
    pub index_id: IndexId,
    pub project_id: QueryProjectId,
    pub sources: Vec<SourceRoot>,
    pub source_import: SourceImportState,
    pub configuration_digest: Option<Sha256Digest>,
}
impl BackfillIndex {
    pub fn validate(&self) -> Result<()> {
        self.registration.validate()?;
        ensure!(
            !self.sources.is_empty() && self.sources.len() <= MAX_SOURCE_ROOTS,
            "invalid upgrade source capacity"
        );
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        let mut owners = 0;
        for source in &self.sources {
            source.validate()?;
            ensure!(
                ids.insert(source.id) && paths.insert(&source.canonical_path),
                "duplicate upgrade source identity"
            );
            for alias in &source.aliases {
                ensure!(aliases.insert(alias), "ambiguous upgrade source alias");
            }
            if source.owner {
                owners += 1;
                ensure!(
                    source.canonical_path == self.registration.root,
                    "upgrade changed primary source identity"
                );
            }
        }
        ensure!(owners == 1, "upgrade requires the original primary source");
        ensure!(
            (self.source_import == SourceImportState::Complete)
                == self.configuration_digest.is_some(),
            "upgrade source completeness and configuration digest disagree"
        );
        if self.source_import == SourceImportState::PendingMetadata {
            ensure!(
                self.sources.len() == 1
                    && self.sources[0].owner
                    && self.sources[0].aliases.is_empty(),
                "unknown source configuration cannot infer extra roots"
            );
        } else {
            ensure!(
                self.sources
                    .iter()
                    .all(|source| source.owner || !source.aliases.is_empty()),
                "legacy additional source has no configured alias"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradePlan {
    pub schema: u32,
    pub id: Incarnation,
    pub control: PathBuf,
    pub namespace: Incarnation,
    pub installer_identity: Sha256Digest,
    pub legacy_schema_digest: Sha256Digest,
    pub registrations_digest: Sha256Digest,
    pub indexes: Vec<BackfillIndex>,
}
impl UpgradePlan {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.control.is_absolute()
                && self.control.to_str().is_some_and(|path| path.len() <= 4096),
            "unsupported catalog upgrade intent"
        );
        ensure!(
            self.indexes.len() <= MAX_DEFINITIONS,
            "upgrade registration capacity exceeded"
        );
        let mut bindings = BTreeSet::new();
        let mut indexes = BTreeSet::new();
        let mut projects = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let mut previous = None;
        for index in &self.indexes {
            index.validate()?;
            let binding = index.registration.binding;
            ensure!(
                previous.is_none_or(|old| old < binding),
                "upgrade registrations must be in canonical order"
            );
            previous = Some(binding);
            ensure!(
                bindings.insert(binding)
                    && indexes.insert(index.index_id)
                    && projects.insert(index.project_id),
                "duplicate upgrade identity"
            );
            for source in &index.sources {
                ensure!(
                    sources.insert(source.id),
                    "source identity belongs to multiple indexes"
                );
            }
        }
        airk_core::index_catalog::encoded_size(self, MAX_INTENT_BYTES - 16384)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/index-catalog-upgrade-plan/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStage {
    Admission,
    Backup,
    Migration,
    Imports,
    Readiness,
    Settled,
    Restoring,
    Restored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfiguration {
    pub source_import: SourceImportState,
    pub aliases: BTreeMap<String, String>,
    pub digest: Option<Sha256Digest>,
}

fn source_configuration(registration: &ProjectRegistration) -> SourceConfiguration {
    let read = || -> Result<_> {
        let data = Path::new(&registration.data_root);
        ensure!(
            data.canonicalize()? == data,
            "index storage location changed"
        );
        let collection = crate::service::collection::Collection::existing(
            data,
            &registration.root,
            &registration.collection,
        )?;
        let selected = collection.select_existing_locked()?;
        let config =
            crate::roots::RootConfig::load(selected.store.directory(), &registration.root)?;
        let digest = framed_digest(
            b"airk/catalog-source-configuration/1",
            &[
                registration.root.as_bytes(),
                &serde_json::to_vec(&config.roots)?,
            ],
        );
        Ok((config.roots, digest))
    };
    match read() {
        Ok((aliases, digest)) => SourceConfiguration {
            source_import: SourceImportState::Complete,
            aliases,
            digest: Some(digest),
        },
        Err(_) => SourceConfiguration {
            source_import: SourceImportState::PendingMetadata,
            aliases: BTreeMap::new(),
            digest: None,
        },
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeReceipt {
    pub schema: u32,
    pub plan: UpgradePlan,
    pub plan_digest: Sha256Digest,
    pub stage: UpgradeStage,
    pub legacy_data_digest: Option<Sha256Digest>,
    pub wal: Option<WalObservation>,
    pub backups: Vec<BackupAttempt>,
    pub readiness: Option<ReadinessEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_v1_host: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_v1: Option<super::genesis::activation::AuthorityV1ActivationRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retirement_recovery: Option<recovery::RecoveryJournal>,
}
impl UpgradeReceipt {
    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.plan.control == root
                && self.plan.identity()? == self.plan_digest,
            "foreign or changed catalog upgrade intent"
        );
        ensure!(
            self.backups.len() <= super::backup::MAX_ATTEMPTS,
            "catalog backup attempt capacity exceeded"
        );
        if let Some(recovery) = &self.retirement_recovery {
            recovery.validate(self)?;
        }
        for (slot, attempt) in self.backups.iter().enumerate() {
            attempt.validate()?;
            ensure!(
                usize::from(attempt.slot) == slot,
                "noncanonical catalog backup slot"
            );
        }
        ensure!(
            self.backups
                .iter()
                .filter(|attempt| attempt.complete)
                .count()
                <= 1,
            "multiple qualified catalog snapshots"
        );
        if let Some(evidence) = &self.readiness {
            evidence.validate(self)?;
        }
        if let Some(record) = &self.authority_v1 {
            record.validate()?;
            use super::genesis::activation::AuthorityV1Phase;
            ensure!(
                matches!(
                    (self.stage, record.phase),
                    (
                        UpgradeStage::Readiness,
                        AuthorityV1Phase::Qualified | AuthorityV1Phase::Activating
                    ) | (UpgradeStage::Settled, AuthorityV1Phase::Settled)
                ),
                "authority-1 durable settlement phase changed"
            );
            let candidate = &record.readiness.candidate;
            ensure!(
                candidate.catalog.namespace == self.plan.namespace
                    && candidate.catalog.upgrade_id == self.plan.id
                    && candidate.installer_identity == self.plan.installer_identity,
                "authority-1 qualification crossed upgrade identity"
            );
            let super::genesis::activation::AuthorityV1Basis::LegacyUpgrade {
                upgrade_id,
                plan_digest,
                native_host,
                ..
            } = &candidate.basis
            else {
                anyhow::bail!("legacy activation cannot use genesis evidence");
            };
            ensure!(
                *upgrade_id == self.plan.id
                    && *plan_digest == self.plan_digest
                    && Some(*native_host) == self.authority_v1_host,
                "authority-1 seal lineage changed"
            );
            let old = self
                .readiness
                .as_ref()
                .context("authority-1 upgrade lacks P5 producer evidence")?;
            let required = old
                .predecessor_refusals
                .iter()
                .map(|r| r.payload_identity)
                .collect::<BTreeSet<_>>();
            ensure!(
                !required.is_empty()
                    && required
                        == record
                            .readiness
                            .required_predecessors
                            .iter()
                            .copied()
                            .collect()
                    && candidate.catalog.revision == old.candidate.catalog.revision.next()?
                    && candidate.catalog.backup_digest == old.candidate.catalog.backup_digest
                    && candidate.catalog.legacy_schema_digest
                        == old.candidate.catalog.legacy_schema_digest,
                "authority-1 proof no longer matches saved P5 qualification"
            );
        }
        ensure!(
            !matches!(self.stage, UpgradeStage::Readiness | UpgradeStage::Settled)
                || self.readiness.is_some(),
            "catalog readiness evidence missing"
        );
        Ok(())
    }
}

fn bounded_read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= maximum as u64,
        "catalog metadata exceeds byte bound"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum,
        "catalog metadata grew beyond byte bound"
    );
    Ok(bytes)
}
fn persist(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("catalog metadata parent missing")?;
    ensure!(
        parent.canonicalize()? == parent,
        "redirected catalog metadata ancestry"
    );
    match fs::symlink_metadata(path) {
        Ok(metadata) => crate::managed_collection::reject_file_aliases(path, &metadata)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    crate::durability::persist_synced(temporary, path)
}

pub fn pending(root: &Path) -> Result<Option<UpgradeReceipt>> {
    let root = root.canonicalize()?;
    let path = root.join(INTENT);
    if !path.try_exists()? {
        return Ok(None);
    }
    let receipt: UpgradeReceipt = serde_json::from_slice(&bounded_read(&path, MAX_INTENT_BYTES)?)?;
    receipt.validate(&root)?;
    Ok(Some(receipt))
}

/// P5 only prepares and qualifies isolated upgrades. P6 must enable this together
/// with native grant enforcement and the reviewed editor/explicit-setup cutover.
pub const PRODUCTION_ACTIVATION_ENABLED: bool = true;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogUpgradeObservation {
    pub schema: u32,
    pub nonce: Incarnation,
    pub plan_digest: Sha256Digest,
    pub installer_identity: Sha256Digest,
    pub catalog: super::CatalogState,
    pub indexes: usize,
    pub pending_source_imports: usize,
    pub contexts: usize,
    pub pending_context_imports: usize,
    pub production_activation_enabled: bool,
}

/// Installer-only bounded metadata probe. It does not acquire an owner lease
/// because the coordinating installer holds exclusion while probing the actual
/// candidate process. It cannot initialize, migrate, activate or grant access.
pub fn observe(
    root: &Path,
    upgrade: Incarnation,
    nonce: Incarnation,
) -> Result<CatalogUpgradeObservation> {
    let root = root.canonicalize()?;
    super::super::data_relocation::require_settled(&root)?;
    let owner = super::super::owner_record(&root, true, Incarnation::from_bytes([0; 16]))?;
    ensure!(
        owner.ready && owner.format == 2 && owner.catalog_upgrade == Some(upgrade),
        "catalog probe requires a matching pending owner fence"
    );
    let receipt = pending(&root)?.context("catalog probe intent missing")?;
    ensure!(
        receipt.plan.id == upgrade
            && receipt.plan.namespace == owner.incarnation
            && matches!(
                receipt.stage,
                UpgradeStage::Imports | UpgradeStage::Readiness
            ),
        "catalog probe intent is not ready for inspection"
    );
    let connection = registry_read(&root)?;
    super::super::lifecycle::require_catalog_admission(&connection)?;
    CatalogUpgrade::validate_backfill(&root, owner.incarnation, &connection, &receipt)?;
    let catalog = super::state(&connection)?;
    ensure!(
        !catalog.activated,
        "catalog probe requires an inactive upgrade"
    );
    let contexts: i64 =
        connection.query_row("SELECT count(*) FROM index_contexts", [], |row| row.get(0))?;
    let pending_contexts: i64 = connection.query_row(
        "SELECT count(*) FROM index_contexts WHERE kind='legacy_pending'",
        [],
        |row| row.get(0),
    )?;
    // Recheck the fence after the read. A superseded upgrade cannot certify the
    // current candidate, even when it happened to observe a consistent snapshot.
    let current_owner = super::super::owner_record(&root, true, owner.incarnation)?;
    let current = pending(&root)?.context("catalog probe intent disappeared")?;
    ensure!(
        current_owner.catalog_upgrade == Some(upgrade)
            && current.plan_digest == receipt.plan_digest
            && current.stage == receipt.stage,
        "catalog probe was superseded"
    );
    Ok(CatalogUpgradeObservation {
        schema: 1,
        nonce,
        plan_digest: receipt.plan_digest,
        installer_identity: receipt.plan.installer_identity,
        catalog,
        indexes: receipt.plan.indexes.len(),
        pending_source_imports: receipt
            .plan
            .indexes
            .iter()
            .filter(|index| index.source_import == SourceImportState::PendingMetadata)
            .count(),
        contexts: usize::try_from(contexts)?,
        pending_context_imports: usize::try_from(pending_contexts)?,
        production_activation_enabled: PRODUCTION_ACTIVATION_ENABLED,
    })
}

pub(super) fn validate_registry_files(root: &Path) -> Result<PathBuf> {
    let path = root.join(DATABASE);
    let metadata = fs::symlink_metadata(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() > 0 && metadata.len() <= super::super::MAX_DATABASE_BYTES,
        "invalid catalog source registry size"
    );
    // This explicit owned adapter may observe committed WAL. Ordinary opens
    // continue to reject overlays. No source/collection files are opened here.
    for (suffix, maximum) in [
        ("-wal", 64 * 1024 * 1024),
        ("-shm", 1024 * 1024),
        ("-journal", 64 * 1024 * 1024),
    ] {
        let overlay = root.join(format!("{DATABASE}{suffix}"));
        match fs::symlink_metadata(&overlay) {
            Ok(metadata) => {
                crate::managed_collection::reject_file_aliases(&overlay, &metadata)?;
                ensure!(
                    metadata.len() <= maximum,
                    "registry overlay exceeds upgrade bound"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let wal = super::wal::observe(&root.join(format!("{DATABASE}-wal")))?;
    ensure!(
        wal.is_some() || !root.join(format!("{DATABASE}-shm")).try_exists()?,
        "registry shared-memory overlay has no recognized WAL"
    );
    Ok(path)
}

fn registry_read(root: &Path) -> Result<Connection> {
    let path = validate_registry_files(root)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    connection.execute_batch(
        "PRAGMA query_only=ON; PRAGMA foreign_keys=ON; PRAGMA cache_size=-8192; BEGIN;",
    )?;
    let application: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        application == super::super::APPLICATION,
        "unsupported catalog upgrade source format"
    );
    ensure!(
        matches!(version, 11..=13),
        CatalogMigrationSourceUnsupported {
            observed_registry_schema: version,
            minimum_registry_schema: 11,
            maximum_registry_schema: 13,
        }
    );
    if matches!(version, 12 | 13) {
        super::validate_catalog_layout(&connection)?;
    } else {
        validate_shape(&connection, 11)?;
    }
    Ok(connection)
}

/// Only the owned migration stage may ask SQLite to recover a rollback journal.
/// This runs after every saved and discovered worker lease has been acquired.
/// SQLite validates/replays its journal; this adapter never copies pages or
/// deletes sidecars. Unknown or unfenced input stays on the read-only path.
fn recover_owned_migration(root: &Path, receipt: &UpgradeReceipt) -> Result<()> {
    if receipt.stage == UpgradeStage::Readiness && receipt.authority_v1_host.is_some() {
        let original = receipt
            .legacy_data_digest
            .context("authority-1 recovery source digest missing")?;
        super::genesis::activation::recover_owned_v1(
            root,
            receipt.plan.id,
            receipt.plan.namespace,
            original,
            &receipt.backups,
        )?;
        let current = registry_read(root)?;
        if super::state(&current)?.authority_contract == 1 {
            recovery::verify_authority(root, receipt)?;
        } else {
            CatalogUpgrade::validate_backfill(root, receipt.plan.namespace, &current, receipt)?;
        }
        return Ok(());
    }
    recover_owned_registry(
        root,
        receipt.plan.id,
        receipt.plan.namespace,
        receipt.legacy_data_digest,
        &receipt.backups,
        matches!(
            receipt.stage,
            UpgradeStage::Migration | UpgradeStage::Restoring
        ),
    )
}

/// Shared only by owned offline adapters after exact intent/fence and every
/// service/helper/watcher/worker lease have been checked. This is not admission.
pub(super) fn recover_owned_registry(
    root: &Path,
    operation: Incarnation,
    expected_namespace: Incarnation,
    expected_rows: Option<Sha256Digest>,
    backups: &[BackupAttempt],
    migration_admitted: bool,
) -> Result<()> {
    let journal = root.join(format!("{DATABASE}-journal"));
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(metadata) if metadata.len() > 0 => metadata,
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        migration_admitted,
        "rollback journal requires owned migration recovery"
    );
    crate::managed_collection::reject_file_aliases(&journal, &metadata)?;
    let path = validate_registry_files(root)?;
    ensure!(
        !root.join(format!("{DATABASE}-wal")).try_exists()?,
        "conflicting migration journal and WAL"
    );
    let attempt = backups
        .iter()
        .find(|attempt| attempt.complete)
        .context("migration recovery requires a qualified snapshot")?;
    let snapshot = super::backup::validate(root, operation, attempt)?;
    let backup = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let namespace: String = backup.query_row(
        "SELECT incarnation FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        namespace == expected_namespace.to_string()
            && Some(super::backup::legacy_data_digest(&backup)?) == expected_rows,
        "migration recovery snapshot identity changed"
    );
    let mut header = [0; 100];
    std::io::Read::read_exact(&mut fs::File::open(&path)?, &mut header)?;
    ensure!(
        &header[..16] == b"SQLite format 3\0"
            && u32::from_be_bytes(header[68..72].try_into()?) == super::super::APPLICATION as u32
            && matches!(u32::from_be_bytes(header[60..64].try_into()?), 11..=13),
        "migration recovery registry header is foreign or damaged"
    );
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
    )?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        matches!(version, 11..=13),
        "migration recovery returned an unsupported registry"
    );
    validate_shape(&connection, version)?;
    super::backup::integrity(&connection)?;
    ensure!(
        Some(super::backup::legacy_data_digest(&connection)?) == expected_rows,
        "migration recovery changed legacy rows; snapshot retained for explicit repair"
    );
    Ok(())
}

/// Fresh genesis has no registered projects, but still fences orphan worker
/// lock entries through the same bounded native ownership census as upgrades.
pub(super) fn genesis_worker_leases(root: &Path) -> Result<Vec<ProjectLease>> {
    exclusion::worker_leases(root, std::iter::empty())
}

fn registrations(connection: &Connection) -> Result<Vec<ProjectRegistration>> {
    let mut statement=connection.prepare("SELECT substr(CAST(binding AS BLOB),1,65),substr(payload,1,20481) FROM projects ORDER BY binding LIMIT 1025")?;
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        ensure!(
            result.len() < MAX_DEFINITIONS,
            "catalog registration capacity exceeded"
        );
        let binding: Vec<u8> = row.get(0)?;
        ensure!(binding.len() == 64, "invalid upgrade registration binding");
        let bytes: Vec<u8> = row.get(1)?;
        ensure!(
            bytes.len() <= airk_core::service::MAX_PROJECT_REGISTRATION_BYTES,
            "oversized upgrade registration"
        );
        let registration: ProjectRegistration = serde_json::from_slice(&bytes)?;
        registration.validate()?;
        ensure!(
            registration.binding.to_string().as_bytes() == binding,
            "crossed catalog registration binding"
        );
        result.push(registration);
    }
    Ok(result)
}
fn registrations_digest(registrations: &[ProjectRegistration]) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/catalog-upgrade-registrations/1",
        &[&serde_json::to_vec(registrations)?],
    ))
}

/// The native installer first persists its activation intent and drains owned
/// processes. These leases then prove exclusion; no PID guessing or termination.
/// The caller establishes owner-only directory access, as for ServiceRegistry.
pub struct CatalogUpgrade {
    root: PathBuf,
    namespace: Incarnation,
    registrations: BTreeMap<Sha256Digest, ProjectRegistration>,
    _workers: Vec<ProjectLease>,
    _watcher: SourceWatchLease,
    _embedding: EmbeddingLease,
    _service: ServiceLease,
}
impl CatalogUpgrade {
    fn qualified_backup<'a>(&self, receipt: &'a UpgradeReceipt) -> Result<&'a BackupAttempt> {
        let attempt = receipt
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified catalog backup missing")?;
        super::backup::validate(&self.root, receipt.plan.id, attempt)?;
        Ok(attempt)
    }

    fn migration_state(&self, connection: &Connection, receipt: &UpgradeReceipt) -> Result<()> {
        Self::validate_backfill(&self.root, self.namespace, connection, receipt)?;
        ensure!(
            !super::state(connection)?.activated,
            "catalog migration is already activated"
        );
        Ok(())
    }

    fn validate_backfill(
        root: &Path,
        namespace: Incarnation,
        connection: &Connection,
        receipt: &UpgradeReceipt,
    ) -> Result<()> {
        super::validate_existing(connection)?;
        super::backup::integrity(connection)?;
        let state = super::state(connection)?;
        ensure!(
            state.authority_contract == 0,
            "P5 backfill/readiness requires authority contract 0"
        );
        let backup = receipt
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified catalog backup missing")?;
        super::backup::validate(root, receipt.plan.id, backup)?;
        ensure!(
            state.namespace == namespace
                && state.upgrade_id == receipt.plan.id
                && state.legacy_schema_digest == receipt.plan.legacy_schema_digest
                && Some(state.backup_digest) == backup.sha256,
            "catalog migration crossed its owned snapshot identity"
        );
        ensure!(
            !state.accepted_new_writes,
            "catalog migration has accepted new writes; forward repair required"
        );
        ensure!(
            Some(super::backup::legacy_data_digest(connection)?) == receipt.legacy_data_digest,
            "catalog migration changed legacy registry rows"
        );
        let count: i64 =
            connection.query_row("SELECT count(*) FROM index_definitions", [], |row| {
                row.get(0)
            })?;
        ensure!(
            count == receipt.plan.indexes.len() as i64,
            "catalog backfill definition count changed"
        );
        let source_count: usize = receipt
            .plan
            .indexes
            .iter()
            .map(|index| index.sources.len())
            .sum();
        let alias_count: usize = receipt
            .plan
            .indexes
            .iter()
            .flat_map(|index| &index.sources)
            .map(|source| source.aliases.len())
            .sum();
        for (table, expected) in [
            ("query_projects", receipt.plan.indexes.len()),
            ("legacy_index_projects", receipt.plan.indexes.len()),
            ("project_index_memberships", receipt.plan.indexes.len()),
            ("project_default_indexes", receipt.plan.indexes.len()),
            ("index_sources", source_count),
            ("project_index_scopes", source_count),
            ("index_source_aliases", alias_count),
            ("index_access_grants", 0),
            ("index_operation_receipts", 0),
        ] {
            let count: i64 =
                connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            ensure!(
                count == expected as i64,
                "catalog backfill row count changed: {table}"
            );
        }
        for index in &receipt.plan.indexes {
            let (binding,root,data,collection,source_state):(String,String,String,String,String)=connection.query_row(
                "SELECT legacy_binding,backing_root,data_root,collection,source_import_state FROM index_definitions WHERE id=?1",
                [index.index_id.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
            ensure!(
                binding == index.registration.binding.to_string()
                    && root == index.registration.root
                    && data == index.registration.data_root
                    && collection == index.registration.collection,
                "catalog backfill identity changed"
            );
            let expected = match index.source_import {
                SourceImportState::Complete => "complete",
                SourceImportState::PendingMetadata => "pending_metadata",
            };
            ensure!(
                source_state == expected,
                "catalog source-import state changed"
            );
            let registered: bool = connection.query_row(
                "SELECT registered FROM projects WHERE binding=?1",
                [index.registration.binding.to_string()],
                |row| row.get(0),
            )?;
            let definition: (Option<String>,String,i64,i64,i64,Option<String>,bool) = connection.query_row(
                "SELECT active_registration,name,definition_revision,source_revision,policy_epoch,policy_digest,tombstone FROM index_definitions WHERE id=?1", [index.index_id.to_string()],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
            ensure!(
                definition
                    == (
                        Some(index.registration.binding.to_string()),
                        index.registration.collection.clone(),
                        1,
                        1,
                        0,
                        None,
                        !registered
                    ),
                "catalog backfill definition metadata changed"
            );
            let project: (String,i64,i64,i64) = connection.query_row("SELECT name,definition_revision,scope_revision,defaults_revision FROM query_projects WHERE id=?1", [index.project_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
            ensure!(
                project
                    == (
                        format!("Legacy collection: {}", index.registration.collection),
                        1,
                        1,
                        1
                    ),
                "catalog backfill Project changed"
            );
            let membership: i64 = connection.query_row("SELECT revision FROM project_index_memberships WHERE project_id=?1 AND index_id=?2", params![index.project_id.to_string(),index.index_id.to_string()], |row| row.get(0))?;
            ensure!(membership == 1, "catalog backfill membership changed");
            let default: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM project_default_indexes WHERE project_id=?1 AND index_id=?2)", params![index.project_id.to_string(),index.index_id.to_string()], |row| row.get(0))?;
            ensure!(default, "catalog backfill default changed");
            let mapped: String = connection.query_row(
                "SELECT project_identity FROM legacy_index_projects WHERE legacy_binding=?1",
                [index.registration.binding.to_string()],
                |row| row.get(0),
            )?;
            ensure!(
                mapped == index.project_id.to_string(),
                "catalog legacy Project mapping changed"
            );
            let current_mapping: Option<String> = connection.query_row(
                "SELECT project_id FROM legacy_index_projects WHERE legacy_binding=?1",
                [index.registration.binding.to_string()],
                |row| row.get(0),
            )?;
            ensure!(
                current_mapping.as_deref() == Some(mapped.as_str()),
                "catalog legacy Project mapping was removed"
            );
            let sources: i64 = connection.query_row(
                "SELECT count(*) FROM index_sources WHERE index_id=?1",
                [index.index_id.to_string()],
                |row| row.get(0),
            )?;
            ensure!(
                sources == index.sources.len() as i64,
                "catalog source backfill count changed"
            );
            for source in &index.sources {
                let stored: (String, String, bool, bool) = connection.query_row(
                    "SELECT index_id,canonical_path,owner,enabled FROM index_sources WHERE id=?1",
                    [source.id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                ensure!(
                    stored
                        == (
                            index.index_id.to_string(),
                            source.canonical_path.clone(),
                            source.owner,
                            true
                        ),
                    "catalog source backfill changed"
                );
                let mut statement = connection.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias")?;
                let aliases = statement
                    .query_map(
                        params![index.index_id.to_string(), source.id.to_string()],
                        |row| row.get::<_, String>(0),
                    )?
                    .collect::<rusqlite::Result<BTreeSet<_>>>()?;
                ensure!(
                    aliases == source.aliases.iter().cloned().collect(),
                    "catalog source aliases changed"
                );
                let scope: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM project_index_scopes WHERE project_id=?1 AND index_id=?2 AND source_id=?3 AND kind='subtree' AND path='')", params![index.project_id.to_string(),index.index_id.to_string(),source.id.to_string()], |row| row.get(0))?;
                ensure!(scope, "catalog backfill scope changed");
            }
        }
        Ok(())
    }

    pub fn migrate(&self) -> Result<UpgradeReceipt> {
        self.migrate_at(|_, _| Ok(()))
    }

    fn migrate_at(
        &self,
        checkpoint: impl Fn(&str, Option<&Connection>) -> Result<()>,
    ) -> Result<UpgradeReceipt> {
        let mut receipt = self.receipt()?;
        if receipt.stage == UpgradeStage::Admission {
            receipt = self.backup()?;
        }
        ensure!(
            matches!(
                receipt.stage,
                UpgradeStage::Backup
                    | UpgradeStage::Migration
                    | UpgradeStage::Imports
                    | UpgradeStage::Readiness
            ),
            "catalog migration is not admitted in this stage"
        );
        let backup_digest = self
            .qualified_backup(&receipt)?
            .sha256
            .context("catalog backup digest missing")?;
        let read = registry_read(&self.root)?;
        let version: u32 = read.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if matches!(version, 12 | 13) {
            self.migration_state(&read, &receipt)?;
            if receipt.stage == UpgradeStage::Migration {
                receipt.stage = UpgradeStage::Imports;
                self.save(&receipt)?;
            }
            return Ok(receipt);
        }
        ensure!(
            matches!(
                receipt.stage,
                UpgradeStage::Backup | UpgradeStage::Migration
            ),
            "catalog schema regressed during upgrade"
        );
        ensure!(
            Some(super::backup::legacy_data_digest(&read)?) == receipt.legacy_data_digest,
            "legacy registry changed before migration"
        );
        drop(read);
        receipt.stage = UpgradeStage::Migration;
        self.save(&receipt)?;
        checkpoint("migration_intent", None)?;
        let mut connection = Connection::open_with_flags(
            self.root.join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        connection.busy_timeout(Duration::from_millis(100))?;
        // The qualified snapshot includes committed WAL before this explicit
        // owned checkpoint. SQLite alone settles its sidecars; no manual deletion.
        let checkpoint_result: (i64, i64, i64) =
            connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        ensure!(
            checkpoint_result.0 == 0,
            "registry WAL checkpoint is busy; owned snapshot preserved"
        );
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA max_page_count=8192; PRAGMA cache_size=-8192;")?;
        validate_shape(&connection, 11)?;
        super::backup::integrity(&connection)?;
        ensure!(
            Some(super::backup::legacy_data_digest(&connection)?) == receipt.legacy_data_digest,
            "checkpoint changed committed legacy rows"
        );
        for index in &receipt.plan.indexes {
            if index.source_import == SourceImportState::Complete {
                let current = source_configuration(&index.registration);
                ensure!(
                    current.digest == index.configuration_digest,
                    "configured sources changed before migration"
                );
            }
        }
        crate::capacity::require_growth(&self.root, super::super::MAX_DATABASE_BYTES * 2)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::install_schema(&transaction)?;
        transaction.execute(
            "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
            params![
                receipt.plan.id.to_string(),
                receipt.plan.legacy_schema_digest.to_string(),
                backup_digest.to_string()
            ],
        )?;
        for index in &receipt.plan.indexes {
            let binding = index.registration.binding.to_string();
            let registered: bool = transaction.query_row(
                "SELECT registered FROM projects WHERE binding=?1",
                [&binding],
                |row| row.get(0),
            )?;
            let source_state = match index.source_import {
                SourceImportState::Complete => "complete",
                SourceImportState::PendingMetadata => "pending_metadata",
            };
            transaction.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?5,1,1,0,NULL,?6,?7)",params![index.index_id.to_string(),binding,index.registration.root,index.registration.data_root,index.registration.collection,!registered,source_state])?;
            transaction.execute(
                "INSERT INTO query_projects VALUES(?1,?2,1,1,1)",
                params![
                    index.project_id.to_string(),
                    format!("Legacy collection: {}", index.registration.collection)
                ],
            )?;
            transaction.execute(
                "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
                params![index.project_id.to_string(), index.index_id.to_string()],
            )?;
            transaction.execute(
                "INSERT INTO project_default_indexes VALUES(?1,?2)",
                params![index.project_id.to_string(), index.index_id.to_string()],
            )?;
            transaction.execute(
                "INSERT INTO legacy_index_projects VALUES(?1,?2,?2)",
                params![binding, index.project_id.to_string()],
            )?;
            for source in &index.sources {
                transaction.execute(
                    "INSERT INTO index_sources VALUES(?1,?2,?3,?4,1)",
                    params![
                        source.id.to_string(),
                        index.index_id.to_string(),
                        source.canonical_path,
                        source.owner
                    ],
                )?;
                for alias in &source.aliases {
                    transaction.execute(
                        "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
                        params![index.index_id.to_string(), source.id.to_string(), alias],
                    )?;
                }
                transaction.execute(
                    "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
                    params![
                        index.project_id.to_string(),
                        index.index_id.to_string(),
                        source.id.to_string()
                    ],
                )?;
            }
        }
        transaction.pragma_update(None, "user_version", 12)?;
        super::backup::integrity(&transaction)?;
        self.migration_state(&transaction, &receipt)?;
        checkpoint("before_migration_commit", Some(&transaction))?;
        transaction.commit()?;
        checkpoint("after_migration_commit", Some(&connection))?;
        receipt.stage = UpgradeStage::Imports;
        self.save(&receipt)?;
        Ok(receipt)
    }

    fn receipt(&self) -> Result<UpgradeReceipt> {
        let receipt = pending(&self.root)?.context("catalog upgrade intent missing")?;
        ensure!(
            receipt.plan.namespace == self.namespace,
            "catalog upgrade namespace changed"
        );
        let owner = super::super::owner_record(&self.root, true, self.namespace)?;
        ensure!(
            owner.catalog_upgrade == Some(receipt.plan.id),
            "catalog upgrade admission fence missing"
        );
        Ok(receipt)
    }

    fn save(&self, receipt: &UpgradeReceipt) -> Result<()> {
        receipt.validate(&self.root)?;
        let previous = pending(&self.root)?.context("catalog upgrade intent missing")?;
        ensure!(
            previous.plan_digest == receipt.plan_digest && previous.plan == receipt.plan,
            "catalog upgrade intent changed"
        );
        if let Some(saved) = &previous.retirement_recovery {
            ensure!(
                receipt
                    .retirement_recovery
                    .as_ref()
                    .is_some_and(|next| next.authorizations.starts_with(&saved.authorizations)),
                "accepted retirement authorizations cannot be removed or rewritten"
            );
        }
        ensure!(
            previous.authority_v1_host.is_none()
                || previous.authority_v1_host == receipt.authority_v1_host,
            "catalog upgrade native host intent changed"
        );
        airk_core::index_catalog::encoded_size(receipt, MAX_INTENT_BYTES)?;
        persist(&self.root.join(INTENT), &serde_json::to_vec(receipt)?)
    }

    pub fn backup(&self) -> Result<UpgradeReceipt> {
        let mut receipt = self.receipt()?;
        if !matches!(
            receipt.stage,
            UpgradeStage::Admission | UpgradeStage::Backup
        ) {
            let attempt = receipt
                .backups
                .iter()
                .find(|attempt| attempt.complete)
                .context("qualified catalog snapshot missing")?;
            super::backup::validate(&self.root, receipt.plan.id, attempt)?;
            return Ok(receipt);
        }
        let source = registry_read(&self.root)?;
        let version: u32 = source.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(version == 11, "catalog backup requires original schema 11");
        super::backup::integrity(&source)?;
        let rows = super::backup::legacy_data_digest(&source)?;
        if let Some(expected) = receipt.legacy_data_digest {
            ensure!(
                rows == expected,
                "legacy registry changed after backup admission"
            );
        } else {
            receipt.legacy_data_digest = Some(rows);
            receipt.wal = super::wal::observe(&self.root.join(format!("{DATABASE}-wal")))?;
        }
        receipt.stage = UpgradeStage::Backup;
        self.save(&receipt)?;
        loop {
            if let Some(attempt) = receipt.backups.iter().find(|attempt| attempt.complete) {
                let path = super::backup::validate(&self.root, receipt.plan.id, attempt)?;
                let snapshot = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                ensure!(
                    super::backup::legacy_data_digest(&snapshot)? == rows,
                    "catalog snapshot rows changed"
                );
                return Ok(receipt);
            }
            let slot = if let Some(slot) = receipt
                .backups
                .iter()
                .position(|attempt| !attempt.abandoned)
            {
                slot
            } else {
                ensure!(
                    receipt.backups.len() < super::backup::MAX_ATTEMPTS,
                    "catalog backup attempts exhausted; incomplete files preserved for review"
                );
                let slot = receipt.backups.len();
                receipt.backups.push(BackupAttempt::reserved(slot as u8));
                self.save(&receipt)?;
                slot
            };
            let mut attempt = receipt.backups[slot].clone();
            let captured = super::backup::capture(
                &self.root,
                receipt.plan.id,
                &source,
                rows,
                &mut attempt,
                |attempt| {
                    receipt.backups[slot] = attempt.clone();
                    self.save(&receipt)
                },
            )?;
            receipt.backups[slot] = attempt;
            if captured {
                self.save(&receipt)?;
                return Ok(receipt);
            }
            receipt.backups[slot].abandoned = true;
            self.save(&receipt)?;
        }
    }

    pub fn acquire(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        // Refuse foreign/unsettled control state before creating a lock file.
        super::super::data_relocation::require_settled(&root)?;
        let owner = super::super::owner_record(&root, true, Incarnation::from_bytes([0; 16]))?;
        ensure!(
            owner.ready && owner.format == 2,
            "catalog upgrade requires a completed owned registry"
        );
        let service = ServiceLease::acquire(&root)?;
        Self::acquire_with_service(service)
    }

    /// Service startup can transfer its already held admission lease to the
    /// upgrade owner before any watcher/helper/project supervisor starts. There
    /// is no release/reacquire gap and no second service owner.
    pub fn acquire_with_service(service: ServiceLease) -> Result<Self> {
        Self::acquire_with_identity(service, None, false)
    }

    /// Resume an existing intent, including the gaps before the admission fence
    /// and after its settlement clear. Verify identity before SQLite recovery.
    pub fn resume_for_installer(root: &Path, installer: Sha256Digest) -> Result<Self> {
        let root = root.canonicalize()?;
        Self::check_installer_resume(&root, installer)?;
        let service = ServiceLease::acquire(&root)?;
        Self::acquire_with_identity(service, Some(installer), true)
    }

    /// Read-only preflight before process effects; acquisition rechecks this
    /// under ownership before granting exclusion or database recovery authority.
    pub fn check_installer_resume(root: &Path, installer: Sha256Digest) -> Result<Incarnation> {
        let root = root.canonicalize()?;
        super::super::data_relocation::require_settled(&root)?;
        let owner = super::super::owner_record(&root, true, Incarnation::from_bytes([0; 16]))?;
        ensure!(
            owner.ready && owner.format == 2,
            "catalog resume requires an owned initialized registry"
        );
        Self::validate_installer_intent(&root, &owner, installer, true)?;
        Ok(owner.incarnation)
    }

    /// Fresh admission or service-owned resume under an already held lease.
    pub fn acquire_with_service_for_installer(
        service: ServiceLease,
        installer: Sha256Digest,
    ) -> Result<Self> {
        Self::acquire_with_identity(service, Some(installer), false)
    }

    fn validate_installer_intent(
        root: &Path,
        owner: &super::super::Owner,
        installer: Sha256Digest,
        required: bool,
    ) -> Result<()> {
        let saved = pending(root)?;
        if let Some(receipt) = saved {
            ensure!(
                receipt.plan.installer_identity == installer
                    && receipt.plan.namespace == owner.incarnation,
                "catalog upgrade belongs to another verified installer payload"
            );
            ensure!(
                owner.catalog_upgrade == Some(receipt.plan.id)
                    || (owner.catalog_upgrade.is_none()
                        && matches!(
                            receipt.stage,
                            UpgradeStage::Admission
                                | UpgradeStage::Readiness
                                | UpgradeStage::Settled
                                | UpgradeStage::Restoring
                                | UpgradeStage::Restored
                        )),
                "catalog installer intent has no matching admission or settlement boundary"
            );
        } else {
            ensure!(
                !required && owner.catalog_upgrade.is_none(),
                "catalog installer resume intent missing"
            );
        }
        Ok(())
    }

    fn acquire_with_identity(
        service: ServiceLease,
        installer: Option<Sha256Digest>,
        required_intent: bool,
    ) -> Result<Self> {
        let root = service.root().to_path_buf();
        super::super::data_relocation::require_settled(&root)?;
        let owner = super::super::owner_record(&root, true, Incarnation::from_bytes([0; 16]))?;
        ensure!(
            owner.ready && owner.format == 2,
            "catalog upgrade requires a completed owned registry"
        );
        if let Some(installer) = installer {
            Self::validate_installer_intent(&root, &owner, installer, required_intent)?;
        }
        let embedding = EmbeddingLease::acquire(&root)?;
        let watcher = SourceWatchLease::acquire(&root)?;
        let intent = if let Some(id) = owner.catalog_upgrade {
            let receipt =
                pending(&root)?.context("catalog admission fence has no matching owned intent")?;
            ensure!(
                receipt.plan.id == id && receipt.plan.namespace == owner.incarnation,
                "catalog admission fence has no matching owned intent"
            );
            if let Some(installer) = installer {
                ensure!(
                    receipt.plan.installer_identity == installer,
                    "catalog upgrade belongs to another verified installer payload"
                );
            }
            Some(receipt)
        } else {
            None
        };
        let registered = if let Some(receipt) = &intent {
            receipt
                .plan
                .indexes
                .iter()
                .map(|index| index.registration.clone())
                .collect()
        } else {
            registrations(&registry_read(&root)?)?
        };
        let workers = exclusion::worker_leases(
            &root,
            registered.iter().map(|registration| registration.binding),
        )?;
        if let Some(receipt) = &intent
            && receipt.retirement_recovery.is_some()
            && receipt.authority_v1_host.is_some()
        {
            // The recovery path borrows this actual all-lease owner; a serialized
            // authorization cannot independently trigger SQLite journal recovery.
            let native = Self {
                root: root.clone(),
                namespace: owner.incarnation,
                registrations: registered.iter().cloned().map(|r| (r.binding, r)).collect(),
                _workers: workers,
                _watcher: watcher,
                _embedding: embedding,
                _service: service,
            };
            recovery::recover_owned(&native, receipt)?;
            let current = registry_read(&root)?;
            let namespace: String = current.query_row(
                "SELECT incarnation FROM service_meta WHERE id=1",
                [],
                |row| row.get(0),
            )?;
            ensure!(
                namespace.parse::<Incarnation>()? == owner.incarnation
                    && registrations_digest(&registered)?
                        == registrations_digest(&registrations(&current)?)?,
                "registry changed during owned retirement recovery"
            );
            return Ok(native);
        }
        if let Some(receipt) = &intent {
            recover_owned_migration(&root, receipt)?;
        }
        // Recheck after all worker leases: an interrupted drain may have settled
        // metadata between the initial observation and exclusion.
        let current = registry_read(&root)?;
        let namespace: String = current.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            namespace.parse::<Incarnation>()? == owner.incarnation,
            "catalog upgrade namespace mismatch"
        );
        ensure!(
            registrations_digest(&registered)? == registrations_digest(&registrations(&current)?)?,
            "registry changed during upgrade admission"
        );
        Ok(Self {
            root,
            namespace: owner.incarnation,
            registrations: registered
                .into_iter()
                .map(|registration| (registration.binding, registration))
                .collect(),
            _workers: workers,
            _watcher: watcher,
            _embedding: embedding,
            _service: service,
        })
    }
    /// Preserve service ownership while releasing migration-only worker/helper
    /// exclusion. Ordinary supervisors may start only after qualified settlement.
    pub fn into_service_lease(self) -> ServiceLease {
        self._service
    }
    pub fn control(&self) -> &Path {
        &self.root
    }
    pub fn namespace(&self) -> Incarnation {
        self.namespace
    }

    /// Complete owned metadata observation for choosing migration versus
    /// settlement resume. This never recovers SQLite or starts normal admission.
    pub fn catalog_state(&self) -> Result<Option<super::CatalogState>> {
        let read = registry_read(&self.root)?;
        let version: u32 = read.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 11 {
            return Ok(None);
        }
        super::validate_existing(&read)?;
        let state = super::state(&read)?;
        let receipt = pending(&self.root)?.context("owned catalog intent missing")?;
        ensure!(
            state.namespace == self.namespace && state.upgrade_id == receipt.plan.id,
            "owned catalog state belongs to another upgrade"
        );
        Ok(Some(state))
    }
    pub fn registrations(&self) -> Vec<ProjectRegistration> {
        self.registrations.values().cloned().collect()
    }
    pub fn source_schema_digest(&self) -> Result<Sha256Digest> {
        schema_digest(&registry_read(&self.root)?)
    }
    pub fn registration_digest(&self) -> Result<Sha256Digest> {
        registrations_digest(&self.registrations())
    }

    /// Read private configuration only: no slot initialization, source-directory
    /// traversal, source contents or extraction/model work.
    pub fn source_configuration(
        &self,
        registration: &ProjectRegistration,
    ) -> Result<SourceConfiguration> {
        ensure!(
            self.registrations.get(&registration.binding) == Some(registration),
            "source configuration is outside upgrade scope"
        );
        Ok(source_configuration(registration))
    }

    fn arm(&self, receipt: &UpgradeReceipt) -> Result<()> {
        let mut owner = super::super::owner_record(&self.root, true, self.namespace)?;
        if owner.catalog_upgrade == Some(receipt.plan.id) {
            return Ok(());
        }
        ensure!(
            owner.catalog_upgrade.is_none() && receipt.stage == UpgradeStage::Admission,
            "catalog admission fence changed"
        );
        let current = registry_read(&self.root)?;
        let version: u32 = current.pragma_query_value(None, "user_version", |row| row.get(0))?;
        super::super::lifecycle::require_catalog_admission(&current)?;
        ensure!(
            version == 11
                && schema_digest(&current)? == receipt.plan.legacy_schema_digest
                && registrations_digest(&registrations(&current)?)?
                    == receipt.plan.registrations_digest,
            "catalog preflight expired before admission fence"
        );
        for index in &receipt.plan.indexes {
            if index.source_import == SourceImportState::Complete {
                let current = source_configuration(&index.registration);
                let aliases = index
                    .sources
                    .iter()
                    .flat_map(|source| {
                        source
                            .aliases
                            .iter()
                            .map(|alias| (alias.clone(), source.canonical_path.clone()))
                    })
                    .collect::<BTreeMap<_, _>>();
                ensure!(
                    current.digest == index.configuration_digest && current.aliases == aliases,
                    "configured sources changed before catalog admission"
                );
            }
        }
        owner.catalog_upgrade = Some(receipt.plan.id);
        persist(
            &self.root.join("service-1.4.owner.json"),
            &serde_json::to_vec(&owner)?,
        )
    }

    pub fn admit(&self, plan: UpgradePlan) -> Result<UpgradeReceipt> {
        plan.validate()?;
        ensure!(
            plan.control == self.root && plan.namespace == self.namespace,
            "crossed catalog upgrade owner"
        );
        if let Some(receipt) = pending(&self.root)? {
            ensure!(receipt.plan == plan, "another catalog upgrade is pending");
            if !matches!(
                receipt.stage,
                UpgradeStage::Settled | UpgradeStage::Restored
            ) {
                self.arm(&receipt)?;
            }
            return Ok(receipt);
        }
        let current = registry_read(&self.root)?;
        let version: u32 = current.pragma_query_value(None, "user_version", |row| row.get(0))?;
        super::super::lifecycle::require_catalog_admission(&current)?;
        ensure!(
            version == 11,
            "catalog upgrade admission requires schema 11"
        );
        ensure!(
            schema_digest(&current)? == plan.legacy_schema_digest
                && registrations_digest(&registrations(&current)?)? == plan.registrations_digest,
            "catalog upgrade preflight expired"
        );
        let expected = plan
            .indexes
            .iter()
            .map(|index| index.registration.clone())
            .collect::<Vec<_>>();
        ensure!(
            expected == registrations(&current)?,
            "catalog backfill is not the exact legacy registration set"
        );
        crate::capacity::require_growth(
            &self.root,
            // Four preserved backup slots, bounded migration growth/journal,
            // the durable intent and the installer's full native receipt output.
            super::super::MAX_DATABASE_BYTES * 6 + MAX_INTENT_BYTES as u64 * 2 + 65536,
        )?;
        let receipt = UpgradeReceipt {
            schema: 1,
            plan_digest: plan.identity()?,
            plan,
            stage: UpgradeStage::Admission,
            legacy_data_digest: None,
            wal: None,
            backups: vec![],
            readiness: None,
            authority_v1_host: None,
            authority_v1: None,
            retirement_recovery: None,
        };
        persist(&self.root.join(INTENT), &serde_json::to_vec(&receipt)?)?;
        self.arm(&receipt)?;
        Ok(receipt)
    }
}
