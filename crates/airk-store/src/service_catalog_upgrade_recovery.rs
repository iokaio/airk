// SPDX-License-Identifier: Apache-2.0
//! Installer-owned legacy retirement authorization. Ordinary reads and jobs
//! cannot construct the native lease proof or waive configuration uncertainty.
use super::*;
pub use airk_core::catalog_upgrade_recovery::{
    UpgradeRetirementAuthorization, UpgradeRetirementCleanup, UpgradeRetirementEffect,
    UpgradeRetirementPreview, UpgradeRetirementReceipt, UpgradeRetirementReply,
    UpgradeRetirementRequest, UpgradeRetirementWork,
};
use airk_core::service::ProjectBinding;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryJournal {
    /// Acceptance order is retained, so every prior-set CAS remains verifiable.
    pub authorizations: Vec<UpgradeRetirementAuthorization>,
}
fn set_digest(authorizations: &[UpgradeRetirementAuthorization]) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/upgrade-retirement-authorizations/1",
        &[&serde_json::to_vec(authorizations)?],
    ))
}
impl RecoveryJournal {
    pub(super) fn validate(&self, receipt: &UpgradeReceipt) -> Result<()> {
        ensure!(
            !self.authorizations.is_empty() && self.authorizations.len() <= MAX_DEFINITIONS,
            "invalid legacy retirement authorization capacity"
        );
        let mut indexes = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for (position, authorization) in self.authorizations.iter().enumerate() {
            authorization.validate()?;
            let request = &authorization.preview.request;
            ensure!(
                request.upgrade_id == receipt.plan.id
                    && request.plan_digest == receipt.plan_digest
                    && request.installer_identity == receipt.plan.installer_identity
                    && request.expected_authorizations_digest
                        == set_digest(&self.authorizations[..position])?
                    && indexes.insert(request.index_id)
                    && operations.insert(request.operation_id()?),
                "crossed legacy retirement authorization"
            );
            let index = receipt
                .plan
                .indexes
                .iter()
                .find(|index| index.index_id == request.index_id)
                .context("retirement index absent from original upgrade plan")?;
            ensure!(
                index.registration == request.registration
                    && index.source_import == SourceImportState::PendingMetadata
                    && index.configuration_digest.is_none()
                    && Some(authorization.preview.original_legacy_data_digest)
                        == receipt.legacy_data_digest
                    && receipt.backups.iter().any(|backup| backup.complete
                        && backup.sha256 == Some(authorization.preview.original_backup_digest)),
                "legacy retirement changed original unavailable identity or snapshot"
            );
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        set_digest(&self.authorizations)
    }
}

fn saved_authorization<'a>(
    journal: &'a RecoveryJournal,
    request: &UpgradeRetirementRequest,
) -> Result<Option<&'a UpgradeRetirementAuthorization>> {
    let operation = request.operation_id()?;
    for authorization in &journal.authorizations {
        if authorization.preview.request.operation_id()? == operation {
            return Ok(Some(authorization));
        }
    }
    Ok(None)
}

/// This value is neither serializable nor constructible by external callers.
/// Its lifetime borrows the CatalogUpgrade that actually holds every native
/// service/project/helper/watch lease. Wire authorizations are insufficient.
struct NativeLeaseRetirement<'a> {
    owner: &'a CatalogUpgrade,
    receipt: &'a UpgradeReceipt,
}
impl<'a> NativeLeaseRetirement<'a> {
    fn acquire(owner: &'a CatalogUpgrade, receipt: &'a UpgradeReceipt) -> Result<Self> {
        let actual = super::super::super::owner_record(&owner.root, true, owner.namespace)?;
        ensure!(
            actual.catalog_upgrade == Some(receipt.plan.id)
                && actual.incarnation == owner.namespace
                && receipt.plan.namespace == owner.namespace
                && receipt.plan.control == owner.root,
            "legacy retirement requires exact held native upgrade ownership"
        );
        let held = owner
            ._workers
            .iter()
            .map(|lease| lease.identity().0)
            .collect::<BTreeSet<_>>();
        ensure!(
            owner
                .registrations
                .keys()
                .all(|binding| held.contains(binding)),
            "native upgrade lacks an exact saved participant lease"
        );
        ensure!(
            owner.registrations
                == receipt
                    .plan
                    .indexes
                    .iter()
                    .map(|index| (index.registration.binding, index.registration.clone()))
                    .collect::<BTreeMap<_, _>>(),
            "held upgrade registration census changed"
        );
        owner.qualified_backup(receipt)?;
        Ok(Self { owner, receipt })
    }
    fn original(&self, read: &Connection) -> Result<()> {
        CatalogUpgrade::validate_backfill(
            &self.owner.root,
            self.owner.namespace,
            read,
            self.receipt,
        )
    }
}

fn work_census(read: &Connection, project: ProjectBinding) -> Result<UpgradeRetirementWork> {
    let mut statement = read.prepare("SELECT id FROM service_jobs WHERE project=?1 AND state IN('pending','running') ORDER BY id LIMIT 1025")?;
    let ids = statement
        .query_map([project.to_string()], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        ids.len() <= 1024,
        "legacy retirement work census exceeds bound"
    );
    let ids = ids
        .into_iter()
        .map(|id| id.parse::<Sha256Digest>())
        .collect::<Result<Vec<_>>>()?;
    Ok(UpgradeRetirementWork {
        jobs: u32::try_from(ids.len())?,
        job_ids_digest: framed_digest(
            b"airk/upgrade-retirement-job-ids/1",
            &[&serde_json::to_vec(&ids)?],
        ),
    })
}

fn require_unused_request_key(read: &Connection, request: &UpgradeRetirementRequest) -> Result<()> {
    let catalog_owned: bool = read.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1)",
        [request.operation_id()?.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        !catalog_owned,
        "retirement request key already belongs to a catalog operation"
    );
    let service_owned: bool = read.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4)",
        params![request.key.namespace.to_string(), i64::try_from(request.key.epoch)?,
            request.key.nonce.to_string(), request.caller.to_string()], |row| row.get(0))?;
    ensure!(
        !service_owned,
        "retirement request key already belongs to an ordinary service request"
    );
    Ok(())
}

fn validate_registration(read: &Connection, request: &UpgradeRetirementRequest) -> Result<()> {
    let (length, bytes, registered): (i64, Vec<u8>, bool) = read.query_row(
        "SELECT length(payload),substr(payload,1,20481),registered FROM projects WHERE binding=?1",
        [request.registration.binding.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(
        registered
            && usize::try_from(length)? == bytes.len()
            && bytes.len() <= airk_core::service::MAX_PROJECT_REGISTRATION_BYTES
            && serde_json::from_slice::<ProjectRegistration>(&bytes)? == request.registration,
        "legacy retirement saved registration changed"
    );
    Ok(())
}

impl CatalogUpgrade {
    /// Read-only under this already held maintenance owner. Does not initialize
    /// an absent data root, open source files, index, install policy, or grant access.
    pub fn preview_legacy_retirement(
        &self,
        request: &UpgradeRetirementRequest,
    ) -> Result<UpgradeRetirementPreview> {
        request.validate()?;
        let receipt = self.receipt()?;
        ensure!(
            matches!(
                receipt.stage,
                UpgradeStage::Imports | UpgradeStage::Readiness
            ) && receipt.authority_v1.is_none(),
            "legacy retirement review requires pre-seal inactive migration"
        );
        let authority = NativeLeaseRetirement::acquire(self, &receipt)?;
        let authorizations = receipt
            .retirement_recovery
            .as_ref()
            .map(|journal| journal.authorizations.as_slice())
            .unwrap_or(&[]);
        ensure!(
            request.upgrade_id == receipt.plan.id
                && request.plan_digest == receipt.plan_digest
                && request.installer_identity == receipt.plan.installer_identity
                && request.expected_authorizations_digest == set_digest(authorizations)?,
            "legacy retirement review lineage changed"
        );
        let index = receipt
            .plan
            .indexes
            .iter()
            .find(|index| index.index_id == request.index_id)
            .context("legacy retirement selected index is outside saved upgrade")?;
        ensure!(
            index.registration == request.registration
                && index.source_import == SourceImportState::PendingMetadata
                && index.configuration_digest.is_none(),
            "legacy retirement requires saved unavailable metadata"
        );
        let read = registry_read(&self.root)?;
        authority.original(&read)?;
        let state = super::super::state(&read)?;
        ensure!(
            !state.activated && !state.accepted_new_writes && state.authority_contract == 0,
            "legacy retirement cannot amend sealed or active authority"
        );
        let (namespace, epoch): (String, i64) = read.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        // This is the service RequestKey domain. The saved plan/catalog identity
        // is validated separately; epoch zero is a valid native initial epoch.
        ensure!(
            request.key.namespace.to_string() == namespace
                && request.key.epoch == u64::try_from(epoch)?,
            "legacy retirement request key expired"
        );
        require_unused_request_key(&read, request)?;
        validate_registration(&read, request)?;
        ensure!(
            source_configuration(&request.registration).source_import
                == SourceImportState::PendingMetadata,
            "legacy retirement configuration returned; review the actual recovered index"
        );
        let preview = UpgradeRetirementPreview {
            schema: 1,
            request: request.clone(),
            request_digest: request.identity()?,
            original_backup_digest: self
                .qualified_backup(&receipt)?
                .sha256
                .context("qualified backup digest missing")?,
            original_legacy_data_digest: receipt
                .legacy_data_digest
                .context("original legacy digest missing")?,
            source_configuration: SourceImportState::PendingMetadata,
            effect: UpgradeRetirementEffect::RetireAndCancelWithCleanupPending,
            affected_work: work_census(&read, request.registration.binding)?,
        };
        UpgradeRetirementReply::Review {
            review_digest: preview.identity()?,
            preview: preview.clone(),
        }
        .validate()?;
        Ok(preview)
    }

    /// One exact reviewed authorization. This writes only the existing upgrade
    /// journal; the original plan/backup/data digest and every registry row remain.
    /// Replay returns the exact saved authorization before stale review checks.
    pub fn authorize_legacy_retirement(
        &self,
        request: &UpgradeRetirementRequest,
        expected_review: Sha256Digest,
    ) -> Result<UpgradeRetirementAuthorization> {
        request.validate()?;
        let mut receipt = self.receipt()?;
        if let Some(journal) = &receipt.retirement_recovery {
            journal.validate(&receipt)?;
            if let Some(saved) = saved_authorization(journal, request)? {
                ensure!(
                    saved.preview.request == *request && saved.review_digest == expected_review,
                    "legacy retirement request key conflict"
                );
                return Ok(saved.clone());
            }
            ensure!(
                !journal
                    .authorizations
                    .iter()
                    .any(|saved| saved.preview.request.index_id == request.index_id),
                "legacy retirement index already has a different accepted authorization"
            );
        }
        let preview = self.preview_legacy_retirement(request)?;
        ensure!(
            preview.identity()? == expected_review,
            "legacy retirement review changed"
        );
        let authorization = UpgradeRetirementAuthorization {
            preview,
            review_digest: expected_review,
        };
        UpgradeRetirementReply::Authorized {
            authorization: authorization.clone(),
        }
        .validate()?;
        // Reserve enough wire capacity for the complete eventual outcome before
        // accepting authority. Use the longest supported revision encoding;
        // these prospective bytes are never published as a completion receipt.
        UpgradeRetirementReply::Applied {
            receipt: UpgradeRetirementReceipt {
                schema: 1,
                authorization: authorization.clone(),
                catalog_revision: (i64::MAX as u64).try_into()?,
                canceled_work: authorization.preview.affected_work.clone(),
                post_effect_legacy_data_digest: Sha256Digest::from_bytes([0; 32]),
                cleanup: UpgradeRetirementCleanup::Pending,
            },
        }
        .validate()?;

        receipt
            .retirement_recovery
            .get_or_insert_with(|| RecoveryJournal {
                authorizations: Vec::new(),
            })
            .authorizations
            .push(authorization.clone());
        self.save(&receipt)?;
        Ok(authorization)
    }

    pub fn prepare_legacy_retirement_request(
        &self,
        index_id: IndexId,
        caller: Incarnation,
        nonce: Incarnation,
    ) -> Result<UpgradeRetirementRequest> {
        let receipt = self.receipt()?;
        let index = receipt
            .plan
            .indexes
            .iter()
            .find(|index| index.index_id == index_id)
            .context("legacy retirement index absent from saved upgrade")?;
        let read = registry_read(self.control())?;
        let (namespace, epoch): (String, i64) = read.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let request = UpgradeRetirementRequest {
            schema: 1,
            caller,
            key: airk_core::service::RequestKey {
                namespace: namespace.parse()?,
                epoch: u64::try_from(epoch)?,
                nonce,
            },
            upgrade_id: receipt.plan.id,
            plan_digest: receipt.plan_digest,
            installer_identity: receipt.plan.installer_identity,
            index_id,
            registration: index.registration.clone(),
            expected_authorizations_digest: self.legacy_retirement_authorizations_digest()?,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn legacy_retirement_authorizations_digest(&self) -> Result<Sha256Digest> {
        let receipt = self.receipt()?;
        set_digest(
            receipt
                .retirement_recovery
                .as_ref()
                .map(|j| j.authorizations.as_slice())
                .unwrap_or(&[]),
        )
    }
}

/// Only this explicit path may omit an originally unknown index from imported
/// evidence authority. Its exact accepted authorization is part of the seal.
pub(super) fn authorized_index(
    receipt: &UpgradeReceipt,
    index: &BackfillIndex,
    read: &Connection,
) -> Result<bool> {
    let Some(journal) = &receipt.retirement_recovery else {
        return Ok(false);
    };
    journal.validate(receipt)?;
    let Some(authorization) = journal
        .authorizations
        .iter()
        .find(|authorization| authorization.preview.request.index_id == index.index_id)
    else {
        return Ok(false);
    };
    let request = &authorization.preview.request;
    ensure!(
        index.registration == request.registration,
        "authorized legacy index backing changed"
    );
    validate_registration(read, request)?;
    // Unknown was required at native preview/confirmation. After acceptance,
    // this is an immutable explicit logical retirement, even if storage returns.
    // Never grant A or reinterpret its missing original aliases as complete.
    ensure!(
        work_census(read, request.registration.binding)? == authorization.preview.affected_work,
        "authorized legacy work census changed"
    );
    Ok(true)
}
pub(super) fn sealed_imports_digest(
    receipt: &UpgradeReceipt,
    expected: &[airk_core::grant_provenance::PersistedGrant],
) -> Result<Sha256Digest> {
    if let Some(journal) = &receipt.retirement_recovery {
        journal.validate(receipt)?;
        Ok(framed_digest(
            b"airk/legacy-compatibility-recovery-seal/1",
            &[
                journal.identity()?.as_bytes(),
                &serde_json::to_vec(expected)?,
            ],
        ))
    } else {
        Ok(framed_digest(
            b"airk/legacy-compatibility-seal/1",
            &[&serde_json::to_vec(expected)?],
        ))
    }
}
pub(super) fn verify_authority(
    root: &Path,
    receipt: &UpgradeReceipt,
) -> Result<super::compatibility::LegacyPromotionObservation> {
    if receipt.retirement_recovery.is_some() {
        verify_recovered_authorized(root, receipt)
    } else {
        super::compatibility::verify_recovered_authority_v1(root, receipt)
    }
}

use super::compatibility::{
    CompatibilityRecoveryReason, CompatibilityRecoveryRequired, LegacyPromotionObservation,
};
pub(super) fn verify_recovered_authorized(
    root: &Path,
    receipt: &UpgradeReceipt,
) -> Result<LegacyPromotionObservation> {
    let native_host = receipt
        .authority_v1_host
        .context("owned compatibility host intent missing")?;
    let read = registry_read(root)?;
    super::super::validate_existing(&read)?;
    let state = super::super::state(&read)?;
    ensure!(
        state.namespace == receipt.plan.namespace
            && state.upgrade_id == receipt.plan.id
            && state.authority_contract == 1
            && state.policy_contract == 0,
        "recovered compatibility format/identity changed"
    );
    let prior = receipt
        .readiness
        .as_ref()
        .context("saved P5 readiness missing")?;
    prior.validate(receipt)?;
    ensure!(
        state.revision == prior.candidate.catalog.revision.next()? && !state.accepted_new_writes,
        "recovered compatibility revision/write fence changed"
    );
    if state.activated {
        let record = receipt
            .authority_v1
            .as_ref()
            .context("activated compatibility lacks native qualification")?;
        ensure!(
            record.phase == super::super::genesis::activation::AuthorityV1Phase::Activating,
            "activated compatibility phase changed"
        );
        super::super::genesis::activation::activated_identity(&state, record, false)?;
    }
    let attempt = receipt
        .backups
        .iter()
        .find(|b| b.complete)
        .context("qualified compatibility backup missing")?;
    super::super::backup::validate(root, receipt.plan.id, attempt)?;
    ensure!(
        Some(state.backup_digest) == attempt.sha256
            && Some(super::super::backup::legacy_data_digest(&read)?) == receipt.legacy_data_digest,
        "recovered compatibility original backup/data changed"
    );
    let mut expected = Vec::new();
    for index in &receipt.plan.indexes {
        let registered: bool = read.query_row(
            "SELECT registered FROM projects WHERE binding=?1",
            [index.registration.binding.to_string()],
            |r| r.get(0),
        )?;
        if !registered {
            continue;
        }
        if authorized_index(receipt, index, &read)? {
            continue;
        }
        let current = source_configuration(&index.registration);
        ensure!(
            current.source_import == SourceImportState::Complete
                && current.digest == index.configuration_digest,
            CompatibilityRecoveryRequired {
                binding: index.registration.binding,
                reason: CompatibilityRecoveryReason::ConfigurationChanged
            }
        );
        super::compatibility::verify_original_sources(&read, index)?;
        expected.push(super::compatibility::expected_grant(
            &receipt.plan,
            receipt.plan_digest,
            index,
            native_host,
        )?);
    }
    expected.sort_by_key(|g| g.grant.id);
    super::compatibility::verify_sealed(&read, &expected)?;
    Ok(LegacyPromotionObservation {
        schema: 1,
        namespace: receipt.plan.namespace,
        upgrade_id: receipt.plan.id,
        plan_digest: receipt.plan_digest,
        catalog_revision: state.revision,
        imported_grants: expected.len(),
        seal_digest: sealed_imports_digest(receipt, &expected)?,
    })
}

#[path = "service_catalog_upgrade_retirement_terminal.rs"]
mod terminal;
pub use terminal::{legacy_retirement_observation, legacy_retirement_receipt};
pub(super) use terminal::{recover_owned, settle_owned};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{
        collection::Collection,
        registry::{ServiceRegistry, catalog},
    };
    use airk_core::{
        index_catalog::{QueryProjectId, SourceRootId},
        service::{JobKind, RequestKey},
        source_policy::SourceRoot,
    };
    struct Fixture {
        owner: CatalogUpgrade,
        a: ProjectRegistration,
        b: ProjectRegistration,
        a_index: IndexId,
        a_job: Sha256Digest,
        b_job: Sha256Digest,
        b_sentinel: PathBuf,
        _temporary: tempfile::TempDir,
    }
    fn fixture() -> Result<Fixture> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let control = base.join("control");
        fs::create_dir(&control)?;
        // Actual native layout: disjoint source and storage siblings beneath
        // the one owned ancestor removed by this metadata-only fixture.
        let a_ancestor = base.join("removed-a");
        fs::create_dir(&a_ancestor)?;
        let a_source = a_ancestor.join("source");
        let a_data = a_ancestor.join("data");
        let b_source = base.join("source-b");
        let b_data = base.join("data-b");
        for path in [&a_source, &a_data, &b_source, &b_data] {
            fs::create_dir(path)?;
        }
        let b_sentinel = b_source.join("sentinel.txt");
        fs::write(&b_sentinel, b"healthy original B bytes")?;
        let registration = |source: &Path, data: &Path, name: &str| -> Result<_> {
            ProjectRegistration::new(
                source
                    .canonicalize()?
                    .to_str()
                    .context("source UTF8")?
                    .into(),
                data.canonicalize()?.to_str().context("data UTF8")?.into(),
                name.into(),
            )
        };
        let a = registration(&a_source, &a_data, "a")?;
        let b = registration(&b_source, &b_data, "b")?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([1; 16]))?;
        registry.register(&a)?;
        registry.register(&b)?;
        let (namespace, epoch) = registry.saved_request_epoch()?;
        let caller = Incarnation::from_bytes([2; 16]);
        let a_job = registry
            .enqueue(
                a.binding,
                caller,
                &RequestKey {
                    namespace,
                    epoch,
                    nonce: Incarnation::from_bytes([3; 16]),
                },
                JobKind::Inspect,
            )?
            .id;
        let b_job = registry
            .enqueue(
                b.binding,
                caller,
                &RequestKey {
                    namespace,
                    epoch,
                    nonce: Incarnation::from_bytes([4; 16]),
                },
                JobKind::Inspect,
            )?
            .id;
        drop(registry);
        let collection =
            Collection::initialize(&b_data, &b.root, "b", Incarnation::from_bytes([5; 16]))?;
        let selected = collection.select()?;
        let writer = selected.store.lock()?;
        let roots = crate::roots::RootConfig::load(selected.store.directory(), &b.root)?;
        roots.save(selected.store.directory())?;
        drop(writer);
        drop(selected);
        drop(collection);
        ensure!(
            a_ancestor.parent() == Some(base.as_path()),
            "owned fixture ancestor escaped"
        );
        fs::remove_dir_all(&a_ancestor)?;
        let original_registry = fs::read(control.join(DATABASE))?;
        let old_a = crate::service::ProjectLease::acquire(&control, a.binding)?;
        assert!(
            CatalogUpgrade::acquire(&control).is_err(),
            "held old A worker must block maintenance authority"
        );
        assert!(!control.join(INTENT).exists());
        assert_eq!(fs::read(control.join(DATABASE))?, original_registry);
        drop(old_a);
        let owner = CatalogUpgrade::acquire(&control)?;
        let a_index = IndexId::from_identity(Incarnation::from_bytes([6; 16]));
        let mut indexes = [
            (&a, a_index, 7),
            (
                &b,
                IndexId::from_identity(Incarnation::from_bytes([8; 16])),
                9,
            ),
        ]
        .into_iter()
        .map(|(registration, index_id, byte)| -> Result<_> {
            let config = owner.source_configuration(registration)?;
            Ok(BackfillIndex {
                registration: registration.clone(),
                index_id,
                project_id: QueryProjectId::from_identity(Incarnation::from_bytes([byte; 16])),
                sources: vec![SourceRoot {
                    id: SourceRootId::from_identity(Incarnation::from_bytes([byte + 10; 16])),
                    canonical_path: registration.root.clone(),
                    aliases: vec![],
                    owner: true,
                }],
                source_import: config.source_import,
                configuration_digest: config.digest,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        // Native plan admission requires binding order, independent of the
        // temporary source paths from which A/B bindings are derived.
        indexes.sort_by_key(|index| index.registration.binding);
        for (registration, expected) in [
            (&a, SourceImportState::PendingMetadata),
            (&b, SourceImportState::Complete),
        ] {
            let index = indexes
                .iter()
                .find(|index| index.registration.binding == registration.binding)
                .context("fixture role missing from canonical plan")?;
            assert_eq!(index.registration, *registration);
            assert_eq!(index.source_import, expected);
        }
        let plan = UpgradePlan {
            schema: 1,
            id: Incarnation::from_bytes([30; 16]),
            control: owner.control().into(),
            namespace: owner.namespace(),
            installer_identity: Sha256Digest::from_bytes([31; 32]),
            legacy_schema_digest: owner.source_schema_digest()?,
            registrations_digest: owner.registration_digest()?,
            indexes,
        };
        owner.admit(plan)?;
        owner.migrate()?;
        Ok(Fixture {
            owner,
            a,
            b,
            a_index,
            a_job,
            b_job,
            b_sentinel,
            _temporary: temporary,
        })
    }
    fn request(fixture: &Fixture) -> Result<UpgradeRetirementRequest> {
        let receipt = fixture.owner.receipt()?;
        let read = registry_read(fixture.owner.control())?;
        let epoch: i64 = read.query_row(
            "SELECT request_epoch FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Ok(UpgradeRetirementRequest {
            schema: 1,
            caller: Incarnation::from_bytes([40; 16]),
            key: RequestKey {
                namespace: receipt.plan.namespace,
                epoch: u64::try_from(epoch)?,
                nonce: Incarnation::from_bytes([41; 16]),
            },
            upgrade_id: receipt.plan.id,
            plan_digest: receipt.plan_digest,
            installer_identity: receipt.plan.installer_identity,
            index_id: fixture.a_index,
            registration: fixture.a.clone(),
            expected_authorizations_digest: fixture
                .owner
                .legacy_retirement_authorizations_digest()?,
        })
    }
    #[test]
    fn review_and_exact_authorization_do_not_change_registry_backup_b_or_recreate_a() -> Result<()>
    {
        let fixture = fixture()?;
        let request = request(&fixture)?;
        let original = fixture.owner.receipt()?;
        let backup = serde_json::to_value(fixture.owner.qualified_backup(&original)?)?;
        let before_db = fs::read(fixture.owner.control().join(DATABASE))?;
        let before_intent = fs::read(fixture.owner.control().join(INTENT))?;
        let preview = fixture.owner.preview_legacy_retirement(&request)?;
        assert_eq!(preview.affected_work.jobs, 1);
        assert_eq!(
            fs::read(fixture.owner.control().join(INTENT))?,
            before_intent
        );
        assert_eq!(fs::read(fixture.owner.control().join(DATABASE))?, before_db);
        let authorized = fixture
            .owner
            .authorize_legacy_retirement(&request, preview.identity()?)?;
        let after_intent = fs::read(fixture.owner.control().join(INTENT))?;
        assert_eq!(
            fixture
                .owner
                .authorize_legacy_retirement(&request, preview.identity()?)?,
            authorized
        );
        assert_eq!(
            fs::read(fixture.owner.control().join(INTENT))?,
            after_intent
        );
        let after = fixture.owner.receipt()?;
        assert_eq!(after.plan, original.plan);
        assert_eq!(after.plan_digest, original.plan_digest);
        assert_eq!(after.legacy_data_digest, original.legacy_data_digest);
        assert_eq!(
            serde_json::to_value(fixture.owner.qualified_backup(&after)?)?,
            backup
        );
        assert_eq!(fs::read(fixture.owner.control().join(DATABASE))?, before_db);
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        assert!(!Path::new(&fixture.a.root).exists() && !Path::new(&fixture.a.data_root).exists());
        let read = registry_read(fixture.owner.control())?;
        for (job, project) in [
            (fixture.a_job, fixture.a.binding),
            (fixture.b_job, fixture.b.binding),
        ] {
            assert_eq!(
                read.query_row(
                    "SELECT project,state FROM service_jobs WHERE id=?1",
                    [job.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                )?,
                (project.to_string(), "pending".into())
            );
        }
        assert!(
            ServiceRegistry::open_saved(fixture.owner.control()).is_err(),
            "authorization alone must not activate global admission"
        );
        Ok(())
    }
    #[test]
    fn stale_crossed_or_known_configuration_authorizations_refuse_without_effects() -> Result<()> {
        let fixture = fixture()?;
        let request = request(&fixture)?;
        let before = fs::read(fixture.owner.control().join(INTENT))?;
        let mut wrong = request.clone();
        wrong.installer_identity = Sha256Digest::from_bytes([99; 32]);
        assert!(fixture.owner.preview_legacy_retirement(&wrong).is_err());
        let mut wrong = request.clone();
        wrong.key.epoch += 1;
        assert!(fixture.owner.preview_legacy_retirement(&wrong).is_err());
        let receipt = fixture.owner.receipt()?;
        let known = receipt
            .plan
            .indexes
            .iter()
            .find(|index| index.registration == fixture.b)
            .context("known B missing")?;
        let mut wrong = request.clone();
        wrong.index_id = known.index_id;
        wrong.registration = fixture.b.clone();
        assert!(fixture.owner.preview_legacy_retirement(&wrong).is_err());
        let preview = fixture.owner.preview_legacy_retirement(&request)?;
        assert!(
            fixture
                .owner
                .authorize_legacy_retirement(&request, Sha256Digest::from_bytes([99; 32]))
                .is_err()
        );
        assert_eq!(fs::read(fixture.owner.control().join(INTENT))?, before);
        fixture
            .owner
            .authorize_legacy_retirement(&request, preview.identity()?)?;
        let before = fs::read(fixture.owner.control().join(INTENT))?;
        let mut conflict = request;
        conflict.registration = fixture.b.clone();
        assert!(
            fixture
                .owner
                .authorize_legacy_retirement(&conflict, preview.identity()?)
                .is_err()
        );
        assert_eq!(fs::read(fixture.owner.control().join(INTENT))?, before);
        Ok(())
    }

    // Store lifecycle producer only. These tests qualify real owned SQL/leases,
    // not artifact execution or predecessor binaries. The genuine installed
    // 1.8.7 two-index process fixture is a separate required later gate.
    fn refusal() -> RuntimeRefusal {
        RuntimeRefusal {
            payload_identity: Sha256Digest::from_bytes([70; 32]),
            read_refusal_digest: Sha256Digest::from_bytes([71; 32]),
            write_refusal_digest: Sha256Digest::from_bytes([72; 32]),
            native_provenance: None,
        }
    }
    struct Producer {
        root: PathBuf,
        upgrade: Incarnation,
        host: Sha256Digest,
    }
    impl crate::service::registry::catalog::genesis::activation::AuthorityV1ReadinessProducer
        for Producer
    {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(vec![refusal().payload_identity])
        }
        fn qualify(
            &self,
            expected: &crate::service::registry::catalog::genesis::activation::AuthorityV1CandidateObservation,
        ) -> Result<
            crate::service::registry::catalog::genesis::activation::AuthorityV1NativeQualification,
        > {
            Ok(crate::service::registry::catalog::genesis::activation::AuthorityV1NativeQualification {
                candidate: super::super::compatibility::activation::observe_authority_v1(
                    &self.root, self.upgrade, self.host, expected.nonce)?, predecessor_refusals: vec![refusal()],
            })
        }
    }
    fn authorize_and_qualify(
        fixture: &Fixture,
    ) -> Result<(UpgradeRetirementRequest, Sha256Digest)> {
        let request = request(fixture)?;
        let preview = fixture.owner.preview_legacy_retirement(&request)?;
        fixture
            .owner
            .authorize_legacy_retirement(&request, preview.identity()?)?;
        let host = qualify_authorized(fixture, &request)?;
        Ok((request, host))
    }
    fn qualify_authorized(
        fixture: &Fixture,
        request: &UpgradeRetirementRequest,
    ) -> Result<Sha256Digest> {
        let host = Sha256Digest::from_bytes([73; 32]);
        fixture.owner.bind_authority_v1_host(host)?;
        fixture.owner.record_readiness(ReadinessEvidence {
            schema: 1,
            candidate: super::super::observe(
                fixture.owner.control(),
                request.upgrade_id,
                Incarnation::from_bytes([74; 16]),
            )?,
            predecessor_refusals: vec![refusal()],
        })?;
        let sealed = fixture.owner.promote_authority_v1(host)?;
        assert_eq!(
            sealed.imported_grants, 1,
            "only actually known B receives imported authority"
        );
        let read = registry_read(fixture.owner.control())?;
        let a_grants: i64 = read.query_row(
            "SELECT count(*) FROM index_access_grants WHERE import_legacy_binding=?1",
            [fixture.a.binding.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(a_grants, 0);
        drop(read);
        fixture.owner.record_authority_v1_readiness(
            host,
            Incarnation::from_bytes([75; 16]),
            &Producer {
                root: fixture.owner.control().into(),
                upgrade: request.upgrade_id,
                host,
            },
        )?;
        Ok(host)
    }
    #[test]
    fn owned_terminal_rollback_then_resume_cancels_only_a_and_preserves_original_history()
    -> Result<()> {
        let mut fixture = fixture()?;
        let (request, host) = authorize_and_qualify(&fixture)?;
        let original = fixture.owner.receipt()?;
        let original_plan = serde_json::to_vec(&original.plan)?;
        let backup = serde_json::to_value(fixture.owner.qualified_backup(&original)?)?;
        let control = fixture.owner.control().to_path_buf();
        let reached = std::cell::Cell::new(false);
        let interrupted = terminal::settle_owned(&fixture.owner, host, |stage| {
            if stage == "before_recovery_terminal_commit" {
                reached.set(true);
            }
            ensure!(
                stage != "before_recovery_terminal_commit",
                "owned SQL rollback checkpoint"
            );
            Ok(())
        });
        assert!(
            reached.get(),
            "intended before_recovery_terminal_commit checkpoint not reached: {interrupted:?}"
        );
        assert!(interrupted.is_err());
        assert!(legacy_retirement_receipt(&control, &request)?.is_none());
        assert_eq!(
            catalog::backup::legacy_data_digest(&registry_read(&control)?)?,
            original
                .legacy_data_digest
                .context("original digest absent")?
        );
        drop(fixture.owner);
        fixture.owner = CatalogUpgrade::resume_for_installer(&control, request.installer_identity)?;
        let completed = terminal::settle_owned(&fixture.owner, host, |_| Ok(()))?;
        assert_eq!(completed.stage, UpgradeStage::Settled);
        assert_eq!(serde_json::to_vec(&completed.plan)?, original_plan);
        assert_eq!(completed.legacy_data_digest, original.legacy_data_digest);
        assert_eq!(
            serde_json::to_value(fixture.owner.qualified_backup(&completed)?)?,
            backup
        );
        let result =
            legacy_retirement_receipt(&control, &request)?.context("terminal receipt absent")?;
        assert_eq!(result.cleanup, UpgradeRetirementCleanup::Pending);
        assert_eq!(result.canceled_work.jobs, 1);
        let registry = ServiceRegistry::open_saved(&control)?;
        assert_eq!(
            registry.job(fixture.a_job)?.state,
            airk_core::service::JobState::Canceled
        );
        assert_eq!(
            registry.job(fixture.b_job)?.state,
            airk_core::service::JobState::Pending
        );
        assert_eq!(registry.get(fixture.b.binding)?.registration, fixture.b);
        assert!(registry.get(fixture.a.binding).is_err());
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        assert!(!Path::new(&fixture.a.root).exists() && !Path::new(&fixture.a.data_root).exists());
        assert!(
            registry
                .catalog_index_retirement_evidence(fixture.a_index)?
                .is_none(),
            "upgrade receipt must never be forged into ordinary worker-fence evidence"
        );
        Ok(())
    }
    #[test]
    fn committed_terminal_gap_recovers_exact_proof_and_forbids_original_snapshot_restore()
    -> Result<()> {
        let mut fixture = fixture()?;
        let (request, host) = authorize_and_qualify(&fixture)?;
        let control = fixture.owner.control().to_path_buf();
        let reached = std::cell::Cell::new(false);
        let interrupted = terminal::settle_owned(&fixture.owner, host, |stage| {
            if stage == "after_recovery_terminal_commit" {
                reached.set(true);
            }
            ensure!(
                stage != "after_recovery_terminal_commit",
                "committed terminal receipt gap"
            );
            Ok(())
        });
        assert!(
            reached.get(),
            "intended after_recovery_terminal_commit checkpoint not reached: {interrupted:?}"
        );
        assert!(interrupted.is_err());
        let result =
            legacy_retirement_receipt(&control, &request)?.context("committed receipt missing")?;
        assert!(
            ServiceRegistry::open_saved(&control).is_err(),
            "global admission must stay fenced until exact settlement"
        );
        assert!(
            fixture.owner.restore().is_err(),
            "accepted retirement writes prohibit legacy rollback"
        );
        drop(fixture.owner);
        fixture.owner = CatalogUpgrade::resume_for_installer(&control, request.installer_identity)?;
        terminal::settle_owned(&fixture.owner, host, |_| Ok(()))?;
        assert_eq!(legacy_retirement_receipt(&control, &request)?, Some(result));
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        Ok(())
    }
    #[test]
    fn postclear_replay_preserves_b_progress_and_applied_receipt_survives_a_physical_absence_cleanup()
    -> Result<()> {
        let mut fixture = fixture()?;
        let (request, host) = authorize_and_qualify(&fixture)?;
        let control = fixture.owner.control().to_path_buf();
        let reached = std::cell::Cell::new(false);
        let interrupted = terminal::settle_owned(&fixture.owner, host, |stage| {
            if stage == "after_recovery_admission_clear" {
                reached.set(true);
            }
            ensure!(
                stage != "after_recovery_admission_clear",
                "postclear receipt gap"
            );
            Ok(())
        });
        assert!(
            reached.get(),
            "intended after_recovery_admission_clear checkpoint not reached: {interrupted:?}"
        );
        assert!(interrupted.is_err());
        let result =
            legacy_retirement_receipt(&control, &request)?.context("committed outcome missing")?;
        let mut registry = ServiceRegistry::open(&control, fixture.owner.namespace())?;
        registry.pause(fixture.b.binding, true)?;
        drop(registry);
        drop(fixture.owner);
        fixture.owner = CatalogUpgrade::resume_for_installer(&control, request.installer_identity)?;
        terminal::settle_owned(&fixture.owner, host, |_| Ok(()))?;
        let namespace = fixture.owner.namespace();
        drop(fixture.owner); // Ordinary purge must acquire A's actual project lease.
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        assert!(registry.get(fixture.b.binding)?.paused);
        // Actual absence-proof cleanup retains the catalog's exact inactive
        // backing identity, without recreating A's data or scheduling more purge work.
        assert!(registry.purge_removed_project_turn(fixture.a.binding)?);
        assert_eq!(
            legacy_retirement_receipt(&control, &request)?,
            Some(result.clone())
        );
        let retained = registry.retained_project(fixture.a.binding)?;
        assert_eq!(retained.registration, fixture.a);
        assert!(retained.paused);
        assert!(!registry.is_registered(fixture.a.binding)?);
        assert!(registry.get(fixture.a.binding).is_err());
        let (tombstone, pointer, legacy): (bool, Option<String>, String) = registry
            .connection
            .query_row(
                "SELECT tombstone,active_registration,legacy_binding FROM index_definitions WHERE id=?1",
                [fixture.a_index.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        assert!(tombstone);
        assert_eq!(pointer, Some(fixture.a.binding.to_string()));
        assert_eq!(legacy, fixture.a.binding.to_string());
        let cleanup_guard: Option<String> = registry.connection.query_row(
            "SELECT cleanup_guard FROM projects WHERE binding=?1",
            [fixture.a.binding.to_string()],
            |row| row.get(0),
        )?;
        assert!(cleanup_guard.is_none());
        assert!(registry.pending_project_removals(None, 32)?.is_empty());
        let catalog_after_cleanup = registry.catalog_state()?;
        assert!(registry.purge_removed_project_turn(fixture.a.binding)?);
        assert_eq!(registry.catalog_state()?, catalog_after_cleanup);
        assert_eq!(legacy_retirement_receipt(&control, &request)?, Some(result));
        assert!(
            registry
                .catalog_index_retirement_evidence(fixture.a_index)?
                .is_none(),
            "upgrade receipt must not become ordinary worker-fence evidence"
        );
        assert!(registry.get(fixture.b.binding)?.paused);
        assert_eq!(registry.get(fixture.b.binding)?.registration, fixture.b);
        assert_eq!(
            registry.job(fixture.b_job)?.state,
            airk_core::service::JobState::Pending
        );
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        assert!(!Path::new(&fixture.a.root).exists() && !Path::new(&fixture.a.data_root).exists());
        Ok(())
    }
    #[test]
    fn corrupt_committed_terminal_marker_refuses_resume_without_unfencing_or_replaying()
    -> Result<()> {
        let fixture = fixture()?;
        let (request, host) = authorize_and_qualify(&fixture)?;
        let control = fixture.owner.control().to_path_buf();
        let reached = std::cell::Cell::new(false);
        let interrupted = terminal::settle_owned(&fixture.owner, host, |stage| {
            if stage == "after_recovery_terminal_commit" {
                reached.set(true);
            }
            ensure!(
                stage != "after_recovery_terminal_commit",
                "terminal receipt gap"
            );
            Ok(())
        });
        assert!(
            reached.get(),
            "intended after_recovery_terminal_commit checkpoint not reached: {interrupted:?}"
        );
        assert!(interrupted.is_err());
        let db = Connection::open(control.join(DATABASE))?;
        db.execute(
            "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
            params![
                br#"{"schema":999}"#.as_slice(),
                request.operation_id()?.to_string()
            ],
        )?;
        drop(db);
        drop(fixture.owner);
        assert!(
            CatalogUpgrade::resume_for_installer(&control, request.installer_identity).is_err()
        );
        assert!(ServiceRegistry::open_saved(&control).is_err());
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        Ok(())
    }

    fn restore_owned_a_metadata(fixture: &Fixture) -> Result<()> {
        let base = fixture._temporary.path().canonicalize()?;
        let source = Path::new(&fixture.a.root);
        let data = Path::new(&fixture.a.data_root);
        ensure!(
            source.starts_with(&base) && data.starts_with(&base),
            "fixture restoration escaped ownership"
        );
        fs::create_dir_all(source)?;
        fs::create_dir_all(data)?;
        let collection = Collection::initialize(
            data,
            &fixture.a.root,
            "a",
            Incarnation::from_bytes([90; 16]),
        )?;
        let selected = collection.select()?;
        let writer = selected.store.lock()?;
        crate::roots::RootConfig::load(selected.store.directory(), &fixture.a.root)?
            .save(selected.store.directory())?;
        drop(writer);
        drop(selected);
        drop(collection);
        assert_eq!(
            fixture
                .owner
                .source_configuration(&fixture.a)?
                .source_import,
            SourceImportState::Complete
        );
        Ok(())
    }
    #[test]
    fn returned_metadata_expires_unconfirmed_review_but_does_not_strand_accepted_retirement()
    -> Result<()> {
        {
            let fixture = fixture()?;
            let request = request(&fixture)?;
            let preview = fixture.owner.preview_legacy_retirement(&request)?;
            let before = fs::read(fixture.owner.control().join(INTENT))?;
            restore_owned_a_metadata(&fixture)?;
            assert!(
                fixture
                    .owner
                    .authorize_legacy_retirement(&request, preview.identity()?)
                    .is_err()
            );
            assert_eq!(fs::read(fixture.owner.control().join(INTENT))?, before);
            assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        }
        let fixture = fixture()?;
        let request = request(&fixture)?;
        let preview = fixture.owner.preview_legacy_retirement(&request)?;
        fixture
            .owner
            .authorize_legacy_retirement(&request, preview.identity()?)?;
        restore_owned_a_metadata(&fixture)?;
        let metadata =
            fs::read(Path::new(&fixture.a.data_root).join("collections-1.4/a/service-owner.json"))?;
        let host = qualify_authorized(&fixture, &request)?;
        terminal::settle_owned(&fixture.owner, host, |_| Ok(()))?;
        let result = legacy_retirement_receipt(fixture.owner.control(), &request)?
            .context("accepted retirement not completed")?;
        assert_eq!(
            result.authorization.preview.source_configuration,
            SourceImportState::PendingMetadata,
            "review observation is immutable history, not current metadata classification"
        );
        assert_eq!(result.cleanup, UpgradeRetirementCleanup::Pending);
        assert_eq!(
            fs::read(Path::new(&fixture.a.data_root).join("collections-1.4/a/service-owner.json"))?,
            metadata,
            "logical retirement never physically removes returned storage"
        );
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        let read = registry_read(fixture.owner.control())?;
        assert_eq!(
            read.query_row(
                "SELECT count(*) FROM index_access_grants WHERE import_legacy_binding=?1",
                [fixture.a.binding.to_string()],
                |row| row.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    }

    #[test]
    fn changed_native_qualification_omission_digest_never_authorizes_terminal_writes() -> Result<()>
    {
        let fixture = fixture()?;
        let (_request, host) = authorize_and_qualify(&fixture)?;
        let control = fixture.owner.control();
        let before = fs::read(control.join(DATABASE))?;
        let mut receipt = fixture.owner.receipt()?;
        let basis = &mut receipt
            .authority_v1
            .as_mut()
            .context("native proof missing")?
            .readiness
            .candidate
            .basis;
        let crate::service::registry::catalog::genesis::activation::AuthorityV1Basis::LegacyUpgrade { seal_digest, .. } = basis
            else { anyhow::bail!("legacy unit proof changed kind"); };
        *seal_digest = Sha256Digest::from_bytes([97; 32]);
        // Explicit private-metadata corruption fixture, not a supported mutation
        // or power-loss claim. A well-typed but crossed proof must fail closed.
        fs::write(control.join(INTENT), serde_json::to_vec(&receipt)?)?;
        assert!(terminal::settle_owned(&fixture.owner, host, |_| Ok(())).is_err());
        assert_eq!(fs::read(control.join(DATABASE))?, before);
        assert!(ServiceRegistry::open_saved(control).is_err());
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        Ok(())
    }
    #[test]
    fn actual_saved_service_request_key_cannot_be_adopted_by_legacy_retirement() -> Result<()> {
        let fixture = fixture()?;
        let clean = request(&fixture)?;
        let mut crossed = clean.clone();
        let read = registry_read(fixture.owner.control())?;
        let (namespace, epoch, nonce, caller): (String, i64, String, String) = read.query_row(
            "SELECT namespace,epoch,nonce,caller FROM service_requests WHERE job=?1",
            [fixture.a_job.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        crossed.caller = caller.parse()?;
        crossed.key = RequestKey {
            namespace: namespace.parse()?,
            epoch: u64::try_from(epoch)?,
            nonce: nonce.parse()?,
        };
        drop(read);
        let before_intent = fs::read(fixture.owner.control().join(INTENT))?;
        let before_db = fs::read(fixture.owner.control().join(DATABASE))?;
        assert!(fixture.owner.preview_legacy_retirement(&crossed).is_err());
        let mut forged_review = fixture.owner.preview_legacy_retirement(&clean)?;
        forged_review.request_digest = crossed.identity()?;
        forged_review.request = crossed.clone();
        assert!(
            fixture
                .owner
                .authorize_legacy_retirement(&crossed, forged_review.identity()?)
                .is_err()
        );
        assert_eq!(
            fs::read(fixture.owner.control().join(INTENT))?,
            before_intent
        );
        assert_eq!(fs::read(fixture.owner.control().join(DATABASE))?, before_db);
        let read = registry_read(fixture.owner.control())?;
        assert_eq!(
            read.query_row(
                "SELECT state FROM service_jobs WHERE id=?1",
                [fixture.a_job.to_string()],
                |row| row.get::<_, String>(0)
            )?,
            "pending"
        );
        assert_eq!(fs::read(&fixture.b_sentinel)?, b"healthy original B bytes");
        assert!(!Path::new(&fixture.a.data_root).exists());
        Ok(())
    }
}
