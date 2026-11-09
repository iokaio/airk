// SPDX-License-Identifier: Apache-2.0
//! Private terminal transition for exactly authorized unavailable legacy indexes.
//! The ordinary activation verifier and normal fence receipts are not weakened.
use super::*;
use crate::service::registry::{catalog, owner_record};
use airk_core::{
    catalog_management::{CatalogOperation, CatalogRequest, MAX_RECEIPT_BYTES},
    catalog_source_change::IndexRetirementReason,
};
use catalog::genesis::activation::{AuthorityV1Phase, activated_identity};
use rusqlite::OptionalExtension;
const KIND: &str = "catalog_upgrade_retirement";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitMarker {
    schema: u32,
    authorization_digest: Sha256Digest,
    receipt_digest: Sha256Digest,
    catalog_revision: airk_core::index_catalog::CatalogRevision,
    post_effect_legacy_data_digest: Sha256Digest,
}
fn full_receipt(
    authorization: &UpgradeRetirementAuthorization,
    marker: &CommitMarker,
) -> Result<UpgradeRetirementReceipt> {
    let receipt = UpgradeRetirementReceipt {
        schema: 1,
        authorization: authorization.clone(),
        catalog_revision: marker.catalog_revision,
        canceled_work: authorization.preview.affected_work.clone(),
        post_effect_legacy_data_digest: marker.post_effect_legacy_data_digest,
        cleanup: UpgradeRetirementCleanup::Pending,
    };
    UpgradeRetirementReply::Applied {
        receipt: receipt.clone(),
    }
    .validate()?;
    ensure!(
        marker.schema == 1
            && marker.authorization_digest == authorization.identity()?
            && receipt.identity()? == marker.receipt_digest,
        "committed upgrade retirement proof changed"
    );
    Ok(receipt)
}
fn marker(
    read: &Connection,
    authorization: &UpgradeRetirementAuthorization,
) -> Result<Option<CommitMarker>> {
    let operation = authorization.preview.request.operation_id()?;
    let row: Option<(String, String, i64, i64, Vec<u8>)> = read.query_row(
        "SELECT kind,stage,revision,length(payload),substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1",
        [operation.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let Some((kind, stage, revision, length, bytes)) = row else {
        return Ok(None);
    };
    ensure!(
        kind == KIND
            && stage == "metadata_applied"
            && bytes.len() <= MAX_RECEIPT_BYTES
            && usize::try_from(length)? == bytes.len(),
        "foreign or malformed upgrade retirement commit marker"
    );
    let marker: CommitMarker = serde_json::from_slice(&bytes)?;
    ensure!(
        i64::try_from(marker.catalog_revision.get())? == revision,
        "upgrade retirement commit revision changed"
    );
    full_receipt(authorization, &marker)?;
    Ok(Some(marker))
}

/// Immutable outcome lookup only. No ordinary service open, process election,
/// source/data path access, lease creation, mutation, or request replay occurs.
/// Pending cleanup in this historical receipt is not a current cleanup census.
pub fn legacy_retirement_receipt(
    root: &Path,
    request: &UpgradeRetirementRequest,
) -> Result<Option<UpgradeRetirementReceipt>> {
    request.validate()?;
    let root = root.canonicalize()?;
    let receipt = pending(&root)?.context("saved legacy upgrade recovery missing")?;
    let Some(journal) = &receipt.retirement_recovery else {
        return Ok(None);
    };
    journal.validate(&receipt)?;
    let Some(authorization) = saved_authorization(journal, request)? else {
        return Ok(None);
    };
    ensure!(
        authorization.preview.request == *request,
        "legacy recovery request key conflict"
    );
    let read = registry_read(&root)?;
    marker(&read, authorization)?
        .map(|marker| full_receipt(authorization, &marker))
        .transpose()
}

pub fn legacy_retirement_observation(
    root: &Path,
    request: &UpgradeRetirementRequest,
) -> Result<Option<UpgradeRetirementReply>> {
    if let Some(receipt) = legacy_retirement_receipt(root, request)? {
        return Ok(Some(UpgradeRetirementReply::Applied { receipt }));
    }
    let receipt = pending(root)?.context("saved legacy upgrade recovery missing")?;
    let Some(journal) = &receipt.retirement_recovery else {
        return Ok(None);
    };
    journal.validate(&receipt)?;
    let operation = request.operation_id()?;
    for authorization in &journal.authorizations {
        if authorization.preview.request.operation_id()? == operation {
            ensure!(
                authorization.preview.request == *request,
                "legacy recovery request key conflict"
            );
            let reply = UpgradeRetirementReply::Authorized {
                authorization: authorization.clone(),
            };
            reply.validate()?;
            return Ok(Some(reply));
        }
    }
    Ok(None)
}

fn qualified_original_imports(
    receipt: &UpgradeReceipt,
) -> Result<Vec<airk_core::grant_provenance::PersistedGrant>> {
    let journal = receipt
        .retirement_recovery
        .as_ref()
        .context("retirement omission set missing")?;
    journal.validate(receipt)?;
    let backup = receipt
        .backups
        .iter()
        .find(|backup| backup.complete)
        .context("original qualified backup missing")?;
    let path = catalog::backup::validate(&receipt.plan.control, receipt.plan.id, backup)?;
    let read = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    ensure!(
        Some(catalog::backup::legacy_data_digest(&read)?) == receipt.legacy_data_digest,
        "terminal proof no longer names the original qualified legacy rows"
    );
    let host = receipt
        .authority_v1_host
        .context("native qualified host missing")?;
    let mut expected = Vec::new();
    for index in &receipt.plan.indexes {
        let registered: bool = read.query_row(
            "SELECT registered FROM projects WHERE binding=?1",
            [index.registration.binding.to_string()],
            |row| row.get(0),
        )?;
        let authorized = journal
            .authorizations
            .iter()
            .any(|authorization| authorization.preview.request.index_id == index.index_id);
        ensure!(
            !authorized || registered,
            "retirement authority crossed original inactive registration"
        );
        if registered && !authorized {
            expected.push(super::super::compatibility::expected_grant(
                &receipt.plan,
                receipt.plan_digest,
                index,
                host,
            )?);
        }
    }
    expected.sort_by_key(|grant| grant.grant.id);
    let record = receipt
        .authority_v1
        .as_ref()
        .context("native qualified recovery proof missing")?;
    let catalog::genesis::activation::AuthorityV1Basis::LegacyUpgrade { seal_digest, .. } =
        &record.readiness.candidate.basis
    else {
        anyhow::bail!("recovery terminal cannot consume genesis qualification");
    };
    ensure!(
        *seal_digest == sealed_imports_digest(receipt, &expected)?,
        "native qualification no longer binds exact retirement omissions and imported B authority"
    );
    Ok(expected)
}

fn verify_terminal(
    read: &Connection,
    receipt: &UpgradeReceipt,
    fence_held: bool,
) -> Result<Vec<UpgradeRetirementReceipt>> {
    let journal = receipt
        .retirement_recovery
        .as_ref()
        .context("explicit recovery authorization missing")?;
    journal.validate(receipt)?;
    let state = catalog::state(read)?;
    let record = receipt
        .authority_v1
        .as_ref()
        .context("native recovery qualification missing")?;
    activated_identity(&state, record, true)?;
    let expected_imports = qualified_original_imports(receipt)?;
    if fence_held {
        ensure!(
            state.policy_contract == record.readiness.candidate.catalog.policy_contract,
            "recovery policy authority changed before admission clear"
        );
        super::super::compatibility::verify_sealed(read, &expected_imports)?;
        for index in &receipt.plan.indexes {
            if expected_imports.iter().any(|grant| matches!(&grant.provenance,
                airk_core::grant_provenance::GrantProvenance::ImportedLegacySnapshot { legacy_binding, .. }
                if *legacy_binding == index.registration.binding)) {
                super::super::compatibility::verify_original_sources(read, index)?;
            }
        }
    }

    ensure!(
        state.accepted_new_writes,
        "retirement terminal transition did not commit owned writes"
    );
    let expected_revision = journal.authorizations.iter().try_fold(
        record.readiness.candidate.catalog.revision,
        |revision, _| revision.next(),
    )?;
    let actual_legacy = if fence_held {
        Some(catalog::backup::legacy_data_digest(read)?)
    } else {
        None
    };
    let mut results = Vec::new();
    for authorization in &journal.authorizations {
        let marker =
            marker(read, authorization)?.context("upgrade retirement terminal receipt missing")?;
        ensure!(
            marker.catalog_revision == expected_revision,
            "upgrade retirement terminal census changed"
        );
        if fence_held {
            catalog::source_change::require_no_retired_references(
                read,
                authorization.preview.request.index_id,
            )?;
            ensure!(
                actual_legacy == Some(marker.post_effect_legacy_data_digest),
                "committed retirement legacy rows changed before admission clear"
            );
            let request = &authorization.preview.request;
            let (payload, registered, paused): (Vec<u8>, bool, bool) = read.query_row(
                "SELECT substr(payload,1,20481),registered,paused FROM projects WHERE binding=?1",
                [request.registration.binding.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            ensure!(
                !registered
                    && paused
                    && serde_json::from_slice::<ProjectRegistration>(&payload)?
                        == request.registration,
                "terminal retirement operational registration changed"
            );
            let active: bool = read.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state IN('pending','running'))",
                [request.registration.binding.to_string()], |row| row.get(0))?;
            ensure!(!active, "terminal retirement retained runnable work");
        }
        let (backing, tombstone, enabled): (String, bool, i64) = read.query_row(
            "SELECT i.legacy_binding,i.tombstone,(SELECT count(*) FROM index_sources s WHERE s.index_id=i.id AND s.enabled<>0) FROM index_definitions i WHERE i.id=?1",
            [authorization.preview.request.index_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        ensure!(
            backing
                == authorization
                    .preview
                    .request
                    .registration
                    .binding
                    .to_string()
                && tombstone
                && enabled == 0,
            "upgrade retirement canonical source authority is not retired"
        );
        results.push(full_receipt(authorization, &marker)?);
    }
    ensure!(
        state.revision >= expected_revision && (!fence_held || state.revision == expected_revision),
        "upgrade recovery catalog revision changed before admission clear"
    );
    Ok(results)
}

pub(in crate::service::registry::catalog::upgrade) fn settle_owned(
    owner: &CatalogUpgrade,
    native_host: Sha256Digest,
    checkpoint: impl Fn(&str) -> Result<()>,
) -> Result<UpgradeReceipt> {
    let mut receipt =
        pending(owner.control())?.context("legacy recovery upgrade intent missing")?;
    let journal = receipt
        .retirement_recovery
        .as_ref()
        .context("legacy recovery authorization missing")?
        .clone();
    journal.validate(&receipt)?;
    ensure!(
        receipt.plan.namespace == owner.namespace
            && receipt.authority_v1_host == Some(native_host)
            && matches!(
                receipt.stage,
                UpgradeStage::Readiness | UpgradeStage::Settled
            ),
        "legacy retirement settlement identity changed"
    );
    let mut record = receipt
        .authority_v1
        .clone()
        .context("legacy retirement lacks native qualification")?;
    record.validate()?;
    let mut native_owner = owner_record(owner.control(), true, owner.namespace)?;
    let read = registry_read(owner.control())?;
    if native_owner.catalog_upgrade.is_none() {
        ensure!(
            matches!(
                record.phase,
                AuthorityV1Phase::Activating | AuthorityV1Phase::Settled
            ),
            "legacy retirement admission cleared outside its terminal intent"
        );
        verify_terminal(&read, &receipt, false)?;
        record.phase = AuthorityV1Phase::Settled;
        receipt.authority_v1 = Some(record);
        receipt.stage = UpgradeStage::Settled;
        owner.save(&receipt)?;
        return Ok(receipt);
    }
    let _authority = NativeLeaseRetirement::acquire(owner, &receipt)?;
    ensure!(
        record.phase != AuthorityV1Phase::Settled,
        "settled recovery retains native admission fence"
    );
    let state = catalog::state(&read)?;
    if !state.activated {
        let qualified = super::super::compatibility::activation::observe_authority_v1(
            owner.control(),
            receipt.plan.id,
            native_host,
            record.readiness.candidate.nonce,
        )?;
        ensure!(
            qualified == record.readiness.candidate,
            "recovery native qualification changed"
        );
        // This is already sealed authority1; P5's zero-grant backfill checker
        // must never be reused here. The exact sealed native observation above
        // authenticates unchanged original rows and the authorized omission set.
        catalog::backup::integrity(&read)?;
        ensure!(
            Some(catalog::backup::legacy_data_digest(&read)?) == receipt.legacy_data_digest,
            "original legacy rows changed before retirement terminal transition"
        );
        for authorization in &journal.authorizations {
            let index = receipt
                .plan
                .indexes
                .iter()
                .find(|index| index.index_id == authorization.preview.request.index_id)
                .context("authorized index missing")?;
            ensure!(
                authorized_index(&receipt, index, &read)?,
                "explicit retirement authorization lost"
            );
            ensure!(
                marker(&read, authorization)?.is_none(),
                "retirement marker exists before terminal activation"
            );
        }
        // Budget the COMPLETE public result wrapper, not merely individual
        // authorizations or compact SQL rows. No physical-data allowance is implied.
        let revision = journal
            .authorizations
            .iter()
            .try_fold(state.revision, |revision, _| revision.next())?;
        let prospective = journal
            .authorizations
            .iter()
            .map(|authorization| UpgradeRetirementReceipt {
                schema: 1,
                authorization: authorization.clone(),
                catalog_revision: revision,
                canceled_work: authorization.preview.affected_work.clone(),
                post_effect_legacy_data_digest: Sha256Digest::from_bytes([0; 32]),
                cleanup: UpgradeRetirementCleanup::Pending,
            })
            .collect::<Vec<_>>();
        for result in &prospective {
            result.validate()?;
        }
        for result in prospective {
            UpgradeRetirementReply::Applied { receipt: result }.validate()?;
        }
        drop(read);
        record.phase = AuthorityV1Phase::Activating;
        receipt.authority_v1 = Some(record.clone());
        owner.save(&receipt)?;
        crate::capacity::require_growth(
            owner.control(),
            crate::service::registry::MAX_DATABASE_BYTES * 2,
        )?;
        let mut db = Connection::open_with_flags(
            owner.control().join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        db.busy_timeout(Duration::from_millis(100))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
        )?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            catalog::state(&tx)? == state
                && owner_record(owner.control(), true, owner.namespace)?.catalog_upgrade
                    == Some(receipt.plan.id),
            "native retirement terminal owner or qualified catalog changed"
        );
        let count: i64 =
            tx.query_row("SELECT count(*) FROM index_operation_receipts", [], |row| {
                row.get(0)
            })?;
        ensure!(
            usize::try_from(count)?
                .checked_add(journal.authorizations.len())
                .is_some_and(|sum| sum <= 1024),
            "native retirement receipt capacity exhausted"
        );
        // No worker can observe this activated marker before commit: the global
        // native owner fence and every project OS lease remain held throughout.
        ensure!(tx.execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1 AND activated=0 AND accepted_new_writes=0", [])? == 1,
            "legacy recovery terminal activation changed");
        let mut current_revision = state.revision;
        for authorization in &journal.authorizations {
            let request = &authorization.preview.request;
            ensure!(
                work_census(&tx, request.registration.binding)?
                    == authorization.preview.affected_work,
                "reviewed retirement work changed at terminal boundary"
            );
            let mutation = CatalogRequest {
                caller: request.caller,
                key: request.key.clone(),
                expected_catalog_revision: current_revision,
                operation: CatalogOperation::RetireIndex {
                    id: request.index_id,
                    expected_definition_revision:
                        airk_core::index_catalog::CatalogRevision::INITIAL,
                    expected_source_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
                    reason: IndexRetirementReason::Explicit,
                },
            };
            // NativeLeaseRetirement above is the owner-specific authority. This
            // shares canonical logical effects, never mints an ordinary FenceProof.
            catalog::source_change::mutate(&tx, &mutation, owner.control())?;
            current_revision = current_revision.next()?;
        }
        let post_digest = catalog::backup::legacy_data_digest(&tx)?;
        for authorization in &journal.authorizations {
            let result = UpgradeRetirementReceipt {
                schema: 1,
                authorization: authorization.clone(),
                catalog_revision: revision,
                canceled_work: authorization.preview.affected_work.clone(),
                post_effect_legacy_data_digest: post_digest,
                cleanup: UpgradeRetirementCleanup::Pending,
            };
            result.validate()?;
            let marker = CommitMarker {
                schema: 1,
                authorization_digest: authorization.identity()?,
                receipt_digest: result.identity()?,
                catalog_revision: revision,
                post_effect_legacy_data_digest: post_digest,
            };
            airk_core::index_catalog::encoded_size(&marker, MAX_RECEIPT_BYTES)?;
            tx.execute(
                "INSERT INTO index_operation_receipts VALUES(?1,?2,'metadata_applied',?3,?4)",
                params![
                    authorization.preview.request.operation_id()?.to_string(),
                    KIND,
                    i64::try_from(revision.get())?,
                    serde_json::to_vec(&marker)?
                ],
            )?;
        }
        ensure!(
            current_revision == revision,
            "retirement revision census changed"
        );
        // Canonical retirement updates ordinary registry/catalog rows, whose
        // existing catalog_write triggers set accepted_new_writes once activated.
        // These are this transaction's authorized writes under all native leases;
        // never reset that durable marker or mistake it for a competing writer.
        ensure!(tx.execute("UPDATE index_catalog_meta SET revision=?1,accepted_new_writes=1 WHERE id=1 AND activated=1 AND accepted_new_writes=1 AND revision=?2",
            params![i64::try_from(revision.get())?, i64::try_from(state.revision.get())?])? == 1,
            "retirement terminal new-write marker changed");
        catalog::validate_existing(&tx)?;
        verify_terminal(&tx, &receipt, true)?;
        checkpoint("before_recovery_terminal_commit")?;
        tx.commit()?;
    } else {
        ensure!(
            record.phase == AuthorityV1Phase::Activating,
            "recovery catalog activated outside exact durable terminal intent"
        );
        verify_terminal(&read, &receipt, true)?;
    }
    checkpoint("after_recovery_terminal_commit")?;
    native_owner.catalog_upgrade = None;
    native_owner.complete(owner.control())?;
    checkpoint("after_recovery_admission_clear")?;
    record.phase = AuthorityV1Phase::Settled;
    receipt.authority_v1 = Some(record);
    receipt.stage = UpgradeStage::Settled;
    owner.save(&receipt)?;
    Ok(receipt)
}

/// This is called only with the already constructed actual all-lease owner.
/// SQLite performs rollback; no sidecar is manually deleted or replayed. The
/// ordinary original-row recovery remains unchanged for all other upgrades.
pub(in crate::service::registry::catalog::upgrade) fn recover_owned(
    owner: &CatalogUpgrade,
    receipt: &UpgradeReceipt,
) -> Result<()> {
    let authority = NativeLeaseRetirement::acquire(owner, receipt)?;
    let original = receipt
        .legacy_data_digest
        .context("recovery original legacy digest missing")?;
    let journal = owner.control().join(format!("{DATABASE}-journal"));
    let recover = match fs::symlink_metadata(&journal) {
        Ok(metadata) => {
            crate::managed_collection::reject_file_aliases(&journal, &metadata)?;
            metadata.len() > 0
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if recover {
        let attempt = owner.qualified_backup(receipt)?;
        let saved = catalog::backup::validate(owner.control(), receipt.plan.id, attempt)?;
        let backup = Connection::open_with_flags(saved, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let namespace: String = backup.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            namespace == owner.namespace.to_string()
                && catalog::backup::legacy_data_digest(&backup)? == original,
            "retirement recovery original snapshot identity changed"
        );
        let path = super::super::validate_registry_files(owner.control())?;
        ensure!(
            !owner
                .control()
                .join(format!("{DATABASE}-wal"))
                .try_exists()?
                && !owner
                    .control()
                    .join(format!("{DATABASE}-shm"))
                    .try_exists()?,
            "owned retirement journal recovery refuses WAL overlays"
        );
        let mut header = [0; 100];
        let mut file = fs::File::open(&path)?;
        crate::managed_collection::reject_file_aliases(&path, &file.metadata()?)?;
        file.read_exact(&mut header)?;
        ensure!(
            &header[..16] == b"SQLite format 3\0"
                && u32::from_be_bytes(header[68..72].try_into()?)
                    == crate::service::registry::APPLICATION as u32
                && matches!(u32::from_be_bytes(header[60..64].try_into()?), 12 | 13),
            "foreign owned retirement recovery registry"
        );
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_millis(100))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
        )?;
        let application: i64 =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
        let pages: i64 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
        ensure!(
            application == crate::service::registry::APPLICATION
                && matches!(version, 12 | 13)
                && page_size == 4096
                && pages > 0
                && pages <= i64::try_from(crate::service::registry::MAX_DATABASE_BYTES / 4096)?,
            "owned retirement recovery physical format or budget changed"
        );
        catalog::validate_catalog_layout(&connection)?;
        catalog::backup::integrity(&connection)?;
        let namespace: String = connection.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            namespace == owner.namespace.to_string(),
            "owned retirement recovery namespace changed"
        );
        if catalog::backup::legacy_data_digest(&connection)? != original {
            // This accepts only the exact committed native terminal branch,
            // including full compact-marker/body hashes and its post-effect digest.
            // Unknown or corrupt saved proof refuses while retaining the fence.
            verify_terminal(&connection, receipt, true)?;
        }
    }
    let current = registry_read(owner.control())?;
    let state = catalog::state(&current)?;
    if state.activated {
        verify_terminal(&current, receipt, true)?;
    } else if state.authority_contract == 1 {
        super::verify_recovered_authorized(owner.control(), receipt)?;
    } else {
        authority.original(&current)?;
    }
    Ok(())
}
