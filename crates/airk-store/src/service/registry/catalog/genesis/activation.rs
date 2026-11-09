// SPDX-License-Identifier: Apache-2.0
//! Native producer port and durable, exact authority-1 activation evidence.
use super::*;
use airk_core::index_catalog::CatalogRevision;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorityV1Basis {
    FreshGenesis {
        operation: Incarnation,
        legacy_data_digest: Sha256Digest,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        coordinator_origin_digest: Option<Sha256Digest>,
    },
    LegacyUpgrade {
        upgrade_id: Incarnation,
        plan_digest: Sha256Digest,
        seal_digest: Sha256Digest,
        native_host: Sha256Digest,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityV1CandidateObservation {
    pub schema: u32,
    pub nonce: Incarnation,
    pub installer_identity: Sha256Digest,
    pub catalog: super::super::CatalogState,
    pub layout_digest: Sha256Digest,
    pub basis: AuthorityV1Basis,
}
pub struct AuthorityV1NativeQualification {
    pub candidate: AuthorityV1CandidateObservation,
    pub predecessor_refusals: Vec<upgrade::RuntimeRefusal>,
}
/// Implemented by the owning native installer, which pins actual artifact
/// handles and executes nonce-bound candidate and old-artifact read/write probes.
/// An empty predecessor set is a native proved fresh installation, never a wire
/// flag. This port has no serde representation and is not an MCP input contract.
pub trait AuthorityV1ReadinessProducer {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>>;
    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityV1ReadinessEvidence {
    pub candidate: AuthorityV1CandidateObservation,
    pub required_predecessors: Vec<Sha256Digest>,
    pub predecessor_refusals: Vec<upgrade::RuntimeRefusal>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityV1Phase {
    Qualified,
    Activating,
    Settled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityV1ActivationRecord {
    pub readiness: AuthorityV1ReadinessEvidence,
    pub phase: AuthorityV1Phase,
}
impl AuthorityV1ActivationRecord {
    pub fn validate(&self) -> Result<()> {
        self.readiness.validate()
    }
}
impl AuthorityV1ReadinessEvidence {
    pub fn validate(&self) -> Result<()> {
        let candidate = &self.candidate;
        for refusal in &self.predecessor_refusals {
            refusal.validate()?;
        }
        ensure!(
            candidate.schema == 1
                && candidate.catalog.schema == 12
                && candidate.catalog.authority_contract == 1
                && candidate.catalog.policy_contract == 0
                && !candidate.catalog.activated
                && !candidate.catalog.accepted_new_writes,
            "authority-1 candidate is not an exact inactive catalog"
        );
        let required = self
            .required_predecessors
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let actual = self
            .predecessor_refusals
            .iter()
            .map(|r| r.payload_identity)
            .collect::<BTreeSet<_>>();
        ensure!(
            required.len() == self.required_predecessors.len()
                && required.len() <= 8
                && actual.len() == self.predecessor_refusals.len()
                && actual == required
                && !required.contains(&candidate.installer_identity),
            "authority-1 artifact refusal coverage changed"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}
pub(crate) fn collect(
    producer: &impl AuthorityV1ReadinessProducer,
    expected: AuthorityV1CandidateObservation,
    legacy_required: Option<&[Sha256Digest]>,
) -> Result<AuthorityV1ReadinessEvidence> {
    let mut required = producer.required_predecessor_identities()?;
    ensure!(
        required.len() <= 8,
        "authority-1 predecessor capacity exceeded"
    );
    required.sort();
    if let Some(previous) = legacy_required {
        let old = previous.iter().copied().collect::<BTreeSet<_>>();
        ensure!(
            !old.is_empty() && old == required.iter().copied().collect(),
            "authority-1 producer must cover exactly the saved P5 predecessors"
        );
    }
    let produced = producer.qualify(&expected)?;
    ensure!(
        produced.candidate == expected,
        "native candidate probe did not return the exact owned observation"
    );
    let evidence = AuthorityV1ReadinessEvidence {
        candidate: expected,
        required_predecessors: required,
        predecessor_refusals: produced.predecessor_refusals,
    };
    evidence.validate()?;
    Ok(evidence)
}
pub(crate) fn activated_identity(
    current: &super::super::CatalogState,
    record: &AuthorityV1ActivationRecord,
    fence_cleared: bool,
) -> Result<()> {
    record.validate()?;
    let original = &record.readiness.candidate.catalog;
    ensure!(
        current.activated
            && (current.schema == original.schema
                || (fence_cleared
                    && current.accepted_new_writes
                    && original.schema == 12
                    && current.schema
                        == crate::service::registry::runtime_contract::IN_PROCESS_SCHEMA))
            && current.namespace == original.namespace
            && current.upgrade_id == original.upgrade_id
            && current.authority_contract == 1
            && current.policy_contract <= super::super::SUPPORTED_POLICY_CONTRACT
            && (fence_cleared || current.policy_contract == 0)
            && current.legacy_schema_digest == original.legacy_schema_digest
            && current.backup_digest == original.backup_digest
            && current.revision >= original.revision,
        "authority-1 committed activation identity changed"
    );
    if !fence_cleared {
        ensure!(
            !current.accepted_new_writes && current.revision == original.revision,
            "new writes occurred before authority-1 admission settlement"
        );
    }
    Ok(())
}

pub(super) fn validate_genesis_record(
    receipt: &GenesisReceipt,
    record: &AuthorityV1ActivationRecord,
) -> Result<()> {
    record.validate()?;
    let candidate = &record.readiness.candidate;
    ensure!(
        candidate.catalog.namespace == receipt.namespace
            && candidate.catalog.upgrade_id == receipt.operation
            && candidate.installer_identity == receipt.installer_identity
            && candidate.basis
                == (AuthorityV1Basis::FreshGenesis {
                    operation: receipt.operation,
                    legacy_data_digest: receipt.legacy_data_digest,
                    coordinator_origin_digest: receipt
                        .coordinator_origin
                        .as_ref()
                        .map(ManagedGenesisOrigin::digest)
                        .transpose()?,
                })
            && candidate.catalog.revision == CatalogRevision::INITIAL.next()?
            && candidate.catalog.legacy_schema_digest == receipt.legacy_schema_digest,
        "authority-1 evidence crossed genesis identity"
    );
    ensure!(
        matches!(
            (receipt.stage, record.phase),
            (GenesisStage::Qualified, AuthorityV1Phase::Qualified)
                | (GenesisStage::Activating, AuthorityV1Phase::Activating)
                | (GenesisStage::Settled, AuthorityV1Phase::Settled)
        ),
        "genesis authority-1 activation phase changed"
    );
    Ok(())
}
fn qualified_backup(receipt: &GenesisReceipt) -> Result<()> {
    let attempt = receipt
        .backups
        .iter()
        .find(|b| b.complete)
        .context("qualified genesis backup missing")?;
    backup::validate(&receipt.root, receipt.operation, attempt)?;
    Ok(())
}
fn empty_catalog(read: &Connection) -> Result<()> {
    for object in super::super::expected(12)?
        .iter()
        .filter(|object| object.kind == "table")
    {
        if object.name == "index_catalog_meta"
            || super::super::expected(11)?
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
            |r| r.get(0),
        )?;
        ensure!(
            empty,
            "fresh authority-1 catalog contains user metadata or grants"
        );
    }
    Ok(())
}
fn verify_fresh_inactive(
    read: &Connection,
    receipt: &GenesisReceipt,
    authority: u32,
) -> Result<super::super::CatalogState> {
    super::super::validate_existing(read)?;
    let current = super::super::state(read)?;
    let backup = receipt
        .backups
        .iter()
        .find(|b| b.complete)
        .context("qualified genesis backup missing")?;
    let revision = if authority == 0 {
        CatalogRevision::INITIAL
    } else {
        CatalogRevision::INITIAL.next()?
    };
    ensure!(
        current.namespace == receipt.namespace
            && current.upgrade_id == receipt.operation
            && current.authority_contract == authority
            && current.policy_contract == 0
            && current.revision == revision
            && !current.activated
            && !current.accepted_new_writes
            && current.legacy_schema_digest == receipt.legacy_schema_digest
            && Some(current.backup_digest) == backup.sha256,
        "fresh authority format/identity changed"
    );
    empty_legacy(read, receipt.namespace)?;
    empty_catalog(read)?;
    ensure!(
        backup::legacy_data_digest(read)? == receipt.legacy_data_digest,
        "genesis original data changed"
    );
    Ok(current)
}
fn completed_genesis_record(receipt: &GenesisReceipt) -> Result<&AuthorityV1ActivationRecord> {
    ensure!(
        matches!(
            receipt.stage,
            GenesisStage::Activating | GenesisStage::Settled
        ),
        "genesis lost its fence before qualified activation"
    );
    let record = receipt
        .authority_v1
        .as_ref()
        .context("committed genesis activation lacks qualification")?;
    validate_genesis_record(receipt, record)?;
    Ok(record)
}
pub(super) fn validate_completed_genesis(root: &Path, receipt: &GenesisReceipt) -> Result<()> {
    let record = completed_genesis_record(receipt)?;
    let read = registry(root, false)?;
    super::super::validate_existing(&read)?;
    activated_identity(&super::super::state(&read)?, record, true)
}
pub(super) fn validate_live_completed_genesis(root: &Path, receipt: &GenesisReceipt) -> Result<()> {
    let record = completed_genesis_record(receipt)?;
    let current = match crate::service::ServiceLease::acquire(root) {
        Ok(_excluded) => {
            // With no live owner, every unexplained overlay still requires its
            // original recovery lane. This reader never replays a journal.
            let read = registry(root, false)?;
            super::super::validate_existing(&read)?;
            super::super::state(&read)?
        }
        Err(error)
            if matches!(
                error.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            ) =>
        {
            // An actual native owner can be committing an ordinary DELETE-mode
            // transaction. Reuse the canonical read-only saved-registry reader,
            // which retains owner/namespace/schema/authority checks and rejects
            // WAL/SHM, instead of calling that live rollback journal corruption.
            super::super::super::ServiceRegistry::open_saved(root)?
                .catalog_state()?
                .context("completed genesis catalog missing")?
        }
        Err(error) => return Err(error),
    };
    activated_identity(&current, record, true)
}
pub(super) fn validate_owned_genesis(root: &Path, receipt: &GenesisReceipt) -> Result<()> {
    if admission(root, receipt)? == GenesisAdmission::Committed {
        return Ok(()); // Qualified identity already checked; no old empty-row proof.
    }
    require_fence(root, receipt)?;
    qualified_backup(receipt)?;
    let read = registry(root, false)?;
    super::super::validate_existing(&read)?;
    let current = super::super::state(&read)?;
    if current.authority_contract == 0 {
        ensure!(
            receipt.stage == GenesisStage::Promoting,
            "genesis authority regressed after promotion"
        );
        verify_fresh_inactive(&read, receipt, 0)?;
    } else if current.activated {
        let record = receipt
            .authority_v1
            .as_ref()
            .context("genesis activation lacks durable qualification")?;
        ensure!(
            record.phase == AuthorityV1Phase::Activating,
            "genesis activated outside its durable intent"
        );
        activated_identity(&current, record, false)?;
        empty_legacy(&read, receipt.namespace)?;
        empty_catalog(&read)?;
    } else {
        verify_fresh_inactive(&read, receipt, 1)?;
    }
    Ok(())
}

/// Candidate process probe. It acquires no competing lease, writes nothing and
/// never creates an empty predecessor record or a P5 migration plan.
pub fn observe_authority_v1(
    root: &Path,
    operation: Incarnation,
    nonce: Incarnation,
) -> Result<AuthorityV1CandidateObservation> {
    let root = root.canonicalize()?;
    let receipt = pending(&root)?.context("genesis intent missing")?;
    ensure!(
        receipt.operation == operation
            && matches!(
                receipt.stage,
                GenesisStage::Promoted | GenesisStage::Qualified | GenesisStage::Activating
            ),
        "genesis candidate probe is outside its owned promoted intent"
    );
    require_fence(&root, &receipt)?;
    qualified_backup(&receipt)?;
    let read = registry(&root, false)?;
    let catalog = verify_fresh_inactive(&read, &receipt, 1)?;
    Ok(AuthorityV1CandidateObservation {
        schema: 1,
        nonce,
        installer_identity: receipt.installer_identity,
        catalog,
        layout_digest: super::super::schema_digest(&read)?,
        basis: AuthorityV1Basis::FreshGenesis {
            operation,
            legacy_data_digest: receipt.legacy_data_digest,
            coordinator_origin_digest: receipt
                .coordinator_origin
                .as_ref()
                .map(ManagedGenesisOrigin::digest)
                .transpose()?,
        },
    })
}

/// Only owned offline v1 transitions call this after all leases have been
/// acquired. A qualified original snapshot and exact owner fence precede RW
/// SQLite journal recovery. SQLite alone rolls back; no sidecar is deleted.
pub(crate) fn recover_owned_v1(
    root: &Path,
    operation: Incarnation,
    namespace: Incarnation,
    original_rows: Sha256Digest,
    backups: &[backup::BackupAttempt],
) -> Result<()> {
    let journal = root.join(format!("{DATABASE}-journal"));
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(meta) if meta.len() > 0 => meta,
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    crate::managed_collection::reject_file_aliases(&journal, &metadata)?;
    let owner = super::super::super::owner_record(root, true, namespace)?;
    ensure!(
        owner.ready
            && owner.format == 2
            && owner.incarnation == namespace
            && owner.catalog_upgrade == Some(operation),
        "authority-1 rollback recovery requires exact native ownership"
    );
    let attempt = backups
        .iter()
        .find(|b| b.complete)
        .context("authority-1 recovery backup missing")?;
    let saved = backup::validate(root, operation, attempt)?;
    let snapshot = Connection::open_with_flags(saved, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let saved_namespace: String =
        snapshot.query_row("SELECT incarnation FROM service_meta WHERE id=1", [], |r| {
            r.get(0)
        })?;
    ensure!(
        saved_namespace == namespace.to_string()
            && backup::legacy_data_digest(&snapshot)? == original_rows,
        "authority-1 recovery backup identity changed"
    );
    let path = upgrade::validate_registry_files(root)?;
    ensure!(
        !root.join(format!("{DATABASE}-wal")).try_exists()?
            && !root.join(format!("{DATABASE}-shm")).try_exists()?,
        "authority-1 journal recovery refuses WAL overlays"
    );
    let mut header = [0; 100];
    let mut file = fs::File::open(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &file.metadata()?)?;
    file.read_exact(&mut header)?;
    ensure!(
        &header[..16] == b"SQLite format 3\0"
            && u32::from_be_bytes(header[68..72].try_into()?)
                == super::super::super::APPLICATION as u32
            && matches!(u32::from_be_bytes(header[60..64].try_into()?), 12 | 13),
        "foreign authority-1 recovery registry"
    );
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
    )?;
    let application: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let page_size: i64 = connection.pragma_query_value(None, "page_size", |r| r.get(0))?;
    let pages: i64 = connection.pragma_query_value(None, "page_count", |r| r.get(0))?;
    ensure!(
        application == super::super::super::APPLICATION
            && matches!(version, 12 | 13)
            && page_size == 4096
            && pages > 0
            && pages <= i64::try_from(super::super::super::MAX_DATABASE_BYTES / 4096)?,
        "authority-1 recovery format/physical budget changed"
    );
    super::super::validate_catalog_layout(&connection)?;
    backup::integrity(&connection)?;
    let actual_namespace: String =
        connection.query_row("SELECT incarnation FROM service_meta WHERE id=1", [], |r| {
            r.get(0)
        })?;
    ensure!(
        actual_namespace == namespace.to_string()
            && backup::legacy_data_digest(&connection)? == original_rows,
        "authority-1 journal recovery changed original registry data"
    );
    Ok(())
}
pub(super) fn recover_genesis_v1(root: &Path, receipt: &GenesisReceipt) -> Result<()> {
    recover_owned_v1(
        root,
        receipt.operation,
        receipt.namespace,
        receipt.legacy_data_digest,
        &receipt.backups,
    )
}

impl CatalogGenesis {
    pub fn promote_authority_v1(&mut self) -> Result<GenesisReceipt> {
        ensure!(
            super::super::capabilities().supports(12, 1, 0),
            "native authority contract 1 is not qualified"
        );
        self.promote_authority_v1_at(|_, _| Ok(()))
    }
    fn promote_authority_v1_at(
        &mut self,
        checkpoint: impl Fn(&Connection, &str) -> Result<()>,
    ) -> Result<GenesisReceipt> {
        let mut receipt = self.receipt()?;
        ensure!(
            matches!(
                receipt.stage,
                GenesisStage::Migrated | GenesisStage::Promoting | GenesisStage::Promoted
            ),
            "genesis promotion requires actual empty migration"
        );
        qualified_backup(&receipt)?;
        let read = registry(&self.root, false)?;
        let layout = super::super::validate_catalog_layout(&read)?;
        if layout == super::super::CatalogLayout::Schema12Authority1 {
            verify_fresh_inactive(&read, &receipt, 1)?;
            receipt.stage = GenesisStage::Promoted;
            persist(&receipt, false)?;
            return Ok(receipt);
        }
        verify_fresh_inactive(&read, &receipt, 0)?;
        drop(read);
        crate::capacity::require_growth(&self.root, super::super::super::MAX_DATABASE_BYTES)?;
        receipt.stage = GenesisStage::Promoting;
        persist(&receipt, false)?;
        let mut write = registry(&self.root, true)?;
        let tx = write.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_fence(&self.root, &receipt)?;
        verify_fresh_inactive(&tx, &receipt, 0)?;
        super::super::apply_authority_v1_layout(&tx)?;
        ensure!(tx.execute("UPDATE index_catalog_meta SET authority_contract=1,revision=2 WHERE id=1 AND authority_contract=0 AND activated=0 AND accepted_new_writes=0",[])?==1,
            "fresh authority promotion marker changed");
        verify_fresh_inactive(&tx, &receipt, 1)?;
        checkpoint(&tx, "before_authority_commit")?;
        tx.commit()?;
        checkpoint(&write, "after_authority_commit")?;
        receipt.stage = GenesisStage::Promoted;
        persist(&receipt, false)?;
        Ok(receipt)
    }
    pub fn record_authority_v1_readiness(
        &self,
        nonce: Incarnation,
        producer: &impl AuthorityV1ReadinessProducer,
    ) -> Result<GenesisReceipt> {
        let mut receipt = self.receipt()?;
        ensure!(
            matches!(
                receipt.stage,
                GenesisStage::Promoted | GenesisStage::Qualified
            ),
            "genesis is not ready for native qualification"
        );
        let expected = observe_authority_v1(&self.root, receipt.operation, nonce)?;
        if let Some(record) = &receipt.authority_v1 {
            ensure!(
                record.readiness.candidate == expected,
                "genesis qualification identity changed"
            );
            return Ok(receipt);
        }
        let readiness = collect(producer, expected, None)?;
        ensure!(
            observe_authority_v1(&self.root, receipt.operation, nonce)? == readiness.candidate,
            "genesis changed during native qualification"
        );
        receipt.authority_v1 = Some(AuthorityV1ActivationRecord {
            readiness,
            phase: AuthorityV1Phase::Qualified,
        });
        receipt.stage = GenesisStage::Qualified;
        persist(&receipt, false)?;
        Ok(receipt)
    }
    pub fn settle_authority_v1(&self) -> Result<GenesisReceipt> {
        ensure!(
            upgrade::PRODUCTION_ACTIVATION_ENABLED,
            "catalog activation remains disabled"
        );
        self.settle_authority_v1_at(|_| Ok(()))
    }
    fn settle_authority_v1_at(
        &self,
        checkpoint: impl Fn(&str) -> Result<()>,
    ) -> Result<GenesisReceipt> {
        let mut receipt = self.receipt()?;
        let mut record = receipt
            .authority_v1
            .clone()
            .context("genesis native qualification missing")?;
        validate_genesis_record(&receipt, &record)?;
        let mut owner = super::super::super::owner_record(&self.root, true, receipt.namespace)?;
        if owner.catalog_upgrade.is_none() {
            validate_completed_genesis(&self.root, &receipt)?;
            record.phase = AuthorityV1Phase::Settled;
            receipt.authority_v1 = Some(record);
            receipt.stage = GenesisStage::Settled;
            persist(&receipt, false)?;
            return Ok(receipt);
        }
        require_fence(&self.root, &receipt)?;
        qualified_backup(&receipt)?;
        let read = registry(&self.root, false)?;
        let state = super::super::state(&read)?;
        if !state.activated {
            ensure!(
                observe_authority_v1(
                    &self.root,
                    receipt.operation,
                    record.readiness.candidate.nonce
                )? == record.readiness.candidate,
                "qualified genesis observation changed before activation"
            );
            record.phase = AuthorityV1Phase::Activating;
            receipt.authority_v1 = Some(record.clone());
            receipt.stage = GenesisStage::Activating;
            persist(&receipt, false)?;
            drop(read);
            let mut write = registry(&self.root, true)?;
            let tx = write.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_fence(&self.root, &receipt)?;
            ensure!(
                super::super::state(&tx)? == record.readiness.candidate.catalog,
                "genesis changed during activation"
            );
            ensure!(tx.execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1 AND activated=0 AND authority_contract=1 AND accepted_new_writes=0",[])?==1,
                "genesis activation marker changed");
            activated_identity(&super::super::state(&tx)?, &record, false)?;
            tx.commit()?;
        } else {
            ensure!(
                record.phase == AuthorityV1Phase::Activating,
                "genesis activated without intent"
            );
            activated_identity(&state, &record, false)?;
        }
        checkpoint("after_catalog_activation")?;
        owner.catalog_upgrade = None;
        owner.complete(&self.root)?;
        checkpoint("after_admission_clear")?;
        record.phase = AuthorityV1Phase::Settled;
        receipt.authority_v1 = Some(record);
        receipt.stage = GenesisStage::Settled;
        persist(&receipt, false)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct OwnedGenesisProducer {
        root: PathBuf,
        operation: Incarnation,
    }
    // Store lifecycle fixture only: observes the real owned registry, but is
    // not evidence that an installed candidate artifact was executed.
    impl AuthorityV1ReadinessProducer for OwnedGenesisProducer {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(Vec::new())
        }
        fn qualify(
            &self,
            expected: &AuthorityV1CandidateObservation,
        ) -> Result<AuthorityV1NativeQualification> {
            Ok(AuthorityV1NativeQualification {
                candidate: observe_authority_v1(&self.root, self.operation, expected.nonce)?,
                predecessor_refusals: Vec::new(),
            })
        }
    }
    fn begin_genesis(root: &Path) -> Result<CatalogGenesis> {
        ensure!(
            super::super::super::capabilities().supports(12, 1, 0),
            "owned v1 qualification requires actual native contract-1 support"
        );
        CatalogGenesis::begin(
            ServiceLease::acquire(root)?,
            Sha256Digest::from_bytes([117; 32]),
            Incarnation::from_bytes([118; 16]),
        )
    }
    fn qualified_genesis(root: &Path) -> Result<CatalogGenesis> {
        let mut owner = begin_genesis(root)?;
        owner.backup()?;
        owner.migrate()?;
        owner.promote_authority_v1()?;
        let receipt = owner.receipt()?;
        owner.record_authority_v1_readiness(
            Incarnation::from_bytes([119; 16]),
            &OwnedGenesisProducer {
                root: receipt.root,
                operation: receipt.operation,
            },
        )?;
        Ok(owner)
    }
    #[test]
    fn owned_fresh_v1_settlement_recovers_both_fence_boundaries_without_reverting_new_writes()
    -> Result<()> {
        for fault in ["after_catalog_activation", "after_admission_clear"] {
            let root = tempfile::tempdir()?;
            let owner = qualified_genesis(root.path())?;
            assert!(
                owner
                    .settle_authority_v1_at(|stage| {
                        ensure!(stage != fault, "injected v1 settlement fault");
                        Ok(())
                    })
                    .is_err()
            );
            if fault == "after_admission_clear" {
                let cleared = owner.receipt()?;
                assert_eq!(
                    admission(&cleared.root, &cleared)?,
                    GenesisAdmission::Committed
                );
                assert!(require_fence(&cleared.root, &cleared).is_err());
                let registry = super::super::super::super::ServiceRegistry::open(
                    root.path(),
                    owner.receipt()?.namespace,
                )?;
                registry
                    .connection
                    .execute("UPDATE service_meta SET revision=revision+1", [])?;
                assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
                assert_eq!(
                    registry.connection.query_row(
                        "SELECT revision FROM service_meta",
                        [],
                        |r| r.get::<_, i64>(0)
                    )?,
                    1
                );
                owner.validate_current()?;
            }
            drop(owner);
            let resumed = begin_genesis(root.path())?;
            assert_eq!(
                resumed.settle_authority_v1_at(|_| Ok(()))?.stage,
                GenesisStage::Settled
            );
            let registry = super::super::super::super::ServiceRegistry::open_saved(root.path())?;
            assert!(registry.catalog_state()?.unwrap().activated);
            if fault == "after_admission_clear" {
                assert_eq!(
                    registry.connection.query_row(
                        "SELECT revision FROM service_meta",
                        [],
                        |r| r.get::<_, i64>(0)
                    )?,
                    1
                );
            }
        }
        Ok(())
    }
    #[test]
    fn completed_genesis_reads_live_owner_transaction_but_refuses_orphan_overlay() -> Result<()> {
        let root = tempfile::tempdir()?;
        let owner = qualified_genesis(root.path())?;
        let receipt = owner.settle_authority_v1_at(|_| Ok(()))?;
        drop(owner);
        let native_owner = crate::service::ServiceLease::acquire(root.path())?;
        let writer = Connection::open(root.path().join(DATABASE))?;
        writer.execute_batch("BEGIN IMMEDIATE; UPDATE service_meta SET revision=revision+1;")?;
        let journal = root.path().join(format!("{DATABASE}-journal"));
        assert!(
            journal.try_exists()?,
            "live native transaction did not create its journal"
        );
        assert!(
            validate_completed_genesis(root.path(), &receipt).is_err(),
            "owned recovery accepted a live journal"
        );
        validate_live_completed_genesis(root.path(), &receipt)?;
        writer.execute_batch("ROLLBACK;")?;
        drop(writer);
        drop(native_owner);
        let before = fs::read(root.path().join(DATABASE))?;
        fs::write(&journal, b"unknown stopped-owner journal")?;
        assert!(validate_live_completed_genesis(root.path(), &receipt).is_err());
        assert_eq!(fs::read(&journal)?, b"unknown stopped-owner journal");
        assert_eq!(fs::read(root.path().join(DATABASE))?, before);
        Ok(())
    }

    #[test]
    fn committed_genesis_never_recovers_an_unknown_postclear_journal() -> Result<()> {
        let root = tempfile::tempdir()?;
        let owner = qualified_genesis(root.path())?;
        assert!(
            owner
                .settle_authority_v1_at(|stage| {
                    ensure!(stage != "after_admission_clear", "injected postclear gap");
                    Ok(())
                })
                .is_err()
        );
        drop(owner);
        let journal = root.path().join(format!("{DATABASE}-journal"));
        fs::write(&journal, b"unknown post-admission journal")?;
        let before = fs::read(root.path().join(DATABASE))?;
        assert!(begin_genesis(root.path()).is_err());
        assert_eq!(fs::read(root.path().join(DATABASE))?, before);
        assert_eq!(fs::read(&journal)?, b"unknown post-admission journal");
        Ok(())
    }
    #[test]
    fn owned_genesis_post_commit_promotion_gap_resumes_without_reapplying_ddl() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut owner = begin_genesis(root.path())?;
        owner.backup()?;
        owner.migrate()?;
        assert!(
            owner
                .promote_authority_v1_at(|_, stage| {
                    ensure!(stage != "after_authority_commit", "injected receipt gap");
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(owner.receipt()?.stage, GenesisStage::Promoting);
        drop(owner);
        let mut resumed = begin_genesis(root.path())?;
        let before = fs::read(root.path().join(DATABASE))?;
        assert_eq!(
            resumed.promote_authority_v1()?.stage,
            GenesisStage::Promoted
        );
        assert_eq!(fs::read(root.path().join(DATABASE))?, before);
        Ok(())
    }
    #[test]
    fn genesis_v1_crash_fixture_child() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_GENESIS_V1_CRASH_FIXTURE") else {
            return Ok(());
        };
        let mut owner = begin_genesis(Path::new(&root))?;
        owner.backup()?;
        owner.migrate()?;
        owner.promote_authority_v1_at(|connection, stage| {
            if stage == "before_authority_commit" {
                connection.cache_flush()?;
                std::process::exit(74);
            }
            Ok(())
        })?;
        anyhow::bail!("genesis v1 crash checkpoint was not reached")
    }
    #[test]
    fn owned_genesis_hot_journal_v1_promotion_rolls_back_then_retries_exactly() -> Result<()> {
        let root = tempfile::tempdir()?;
        let child = concat!(module_path!(), "::genesis_v1_crash_fixture_child");
        let child = child
            .split_once("::")
            .context("test module name missing")?
            .1;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", child, "--nocapture"])
            .env("AIRK_GENESIS_V1_CRASH_FIXTURE", root.path())
            .output()?;
        ensure!(
            output.status.code() == Some(74),
            "v1 fixture child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            root.path()
                .join(format!("{DATABASE}-journal"))
                .try_exists()?
        );
        let mut resumed = begin_genesis(root.path())?;
        resumed.promote_authority_v1()?;
        let receipt = resumed.receipt()?;
        let read = registry(root.path(), false)?;
        verify_fresh_inactive(&read, &receipt, 1)?;
        Ok(())
    }
    fn candidate() -> AuthorityV1CandidateObservation {
        let id = Incarnation::from_bytes([1; 16]);
        let digest = Sha256Digest::from_bytes([2; 32]);
        AuthorityV1CandidateObservation {
            schema: 1,
            nonce: id,
            installer_identity: digest,
            catalog: super::super::super::CatalogState {
                schema: 12,
                namespace: id,
                revision: CatalogRevision::INITIAL.next().expect("fixture revision"),
                activated: false,
                accepted_new_writes: false,
                authority_contract: 1,
                policy_contract: 0,
                upgrade_id: id,
                legacy_schema_digest: digest,
                backup_digest: digest,
            },
            layout_digest: digest,
            basis: AuthorityV1Basis::FreshGenesis {
                operation: id,
                legacy_data_digest: digest,
                coordinator_origin_digest: None,
            },
        }
    }
    struct PureProducer {
        candidate: AuthorityV1CandidateObservation,
        required: Vec<Sha256Digest>,
        refusals: Vec<upgrade::RuntimeRefusal>,
    }
    impl AuthorityV1ReadinessProducer for PureProducer {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(self.required.clone())
        }
        fn qualify(
            &self,
            _: &AuthorityV1CandidateObservation,
        ) -> Result<AuthorityV1NativeQualification> {
            Ok(AuthorityV1NativeQualification {
                candidate: self.candidate.clone(),
                predecessor_refusals: self.refusals.clone(),
            })
        }
    }
    #[test]
    fn pure_native_port_binds_nonce_and_exact_required_artifacts() -> Result<()> {
        let candidate = candidate();
        let old = Sha256Digest::from_bytes([3; 32]);
        let refusal = upgrade::RuntimeRefusal {
            payload_identity: old,
            read_refusal_digest: old,
            write_refusal_digest: old,
            native_provenance: None,
        };
        let mut producer = PureProducer {
            candidate: candidate.clone(),
            required: vec![old],
            refusals: vec![refusal],
        };
        collect(&producer, candidate.clone(), Some(&[old]))?;
        producer.refusals[0] = upgrade::RuntimeRefusal::from_native(
            old,
            upgrade::NativeRefusalProvenance {
                native_entrypoints: upgrade::NativeRefusalEntrypoints {
                    read: upgrade::NativeReadRefusalEntrypoint::ServiceRequestsList,
                    write: upgrade::NativeWriteRefusalEntrypoint::HiddenUpgradeMigrate,
                },
                read: upgrade::NativeRefusalDiagnostic {
                    initial_admission_pending_digest: None,
                    final_classified_diagnostic_digest: old,
                },
                write: upgrade::NativeRefusalDiagnostic {
                    initial_admission_pending_digest: Some(Sha256Digest::from_bytes([5; 32])),
                    final_classified_diagnostic_digest: old,
                },
            },
        )?;
        collect(&producer, candidate.clone(), Some(&[old]))?;
        let valid = producer.refusals[0].clone();
        producer.refusals[0].write_refusal_digest = old;
        assert!(collect(&producer, candidate.clone(), Some(&[old])).is_err());
        producer.refusals[0] = valid;
        producer.candidate.nonce = Incarnation::from_bytes([4; 16]);
        assert!(collect(&producer, candidate.clone(), Some(&[old])).is_err());
        producer.candidate = candidate.clone();
        producer.refusals.clear();
        assert!(collect(&producer, candidate, Some(&[old])).is_err());
        Ok(())
    }
    #[test]
    fn settled_identity_allows_legitimate_later_writes_only_after_fence_clear() -> Result<()> {
        let candidate = candidate();
        let mut current = candidate.catalog.clone();
        let record = AuthorityV1ActivationRecord {
            readiness: AuthorityV1ReadinessEvidence {
                candidate,
                required_predecessors: Vec::new(),
                predecessor_refusals: Vec::new(),
            },
            phase: AuthorityV1Phase::Activating,
        };
        current.activated = true;
        activated_identity(&current, &record, false)?;
        current.revision = current.revision.next()?;
        current.accepted_new_writes = true;
        activated_identity(&current, &record, true)?;
        assert!(activated_identity(&current, &record, false).is_err());
        // Schema 13 is an owned post-settlement write capability, not a new
        // namespace or a replacement for the original readiness receipt.
        current.schema = crate::service::registry::runtime_contract::IN_PROCESS_SCHEMA;
        activated_identity(&current, &record, true)?;
        assert!(activated_identity(&current, &record, false).is_err());
        current.accepted_new_writes = false;
        assert!(activated_identity(&current, &record, true).is_err());
        current.accepted_new_writes = true;
        current.schema = 14;
        assert!(activated_identity(&current, &record, true).is_err());
        current.schema = 11;
        assert!(activated_identity(&current, &record, true).is_err());
        current.schema = 13;
        current.namespace = Incarnation::from_bytes([99; 16]);
        assert!(activated_identity(&current, &record, true).is_err());
        Ok(())
    }
}
