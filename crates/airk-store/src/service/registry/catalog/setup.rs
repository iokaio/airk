// SPDX-License-Identifier: Apache-2.0
//! Normal reviewed index setup, distinct from upgrade imports. Native adapters
//! validate filesystem identity; this adapter owns bounded metadata and receipts.
use super::ServiceRegistry;
pub use airk_core::catalog_setup::{
    IndexSetupReceipt, IndexSetupRequest, PreparationEvidence, SetupCompletion,
    SetupPreparationReview, SetupSource, SetupStage,
};
use airk_core::{
    catalog_management::{MAX_RECEIPT_BYTES, catalog_operation_id},
    catalog_setup::{
        canonical_setup_sources, setup_index_id, setup_source_ids, validate_setup_sources,
    },
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, encoded_size},
    service::{ProjectBinding, ProjectRegistration, RequestKey},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

const KIND: &str = "catalog_index_setup";

mod checkpoint;
mod legacy_adoption;
mod metadata;
pub use checkpoint::legacy_adoption_checkpoint_history;
pub(crate) use checkpoint::uninitialized_legacy_adoption;
pub use legacy_adoption::{NativeLegacyAdoptionCompletion, SavedAdoptionSources};
#[cfg(test)]
pub(in crate::service::registry::catalog) use legacy_adoption::{
    complete_metadata_authority_fixture, completed_authority_fixture,
};

/// Read-only native encoding preflight. Its digest binds actual defaults and
/// individual native format budgets; syntax alone is not filesystem authority.
pub fn validate_setup_metadata_plan(
    control: &Path,
    request: &IndexSetupRequest,
) -> Result<Sha256Digest> {
    metadata::Metadata::from_request(control, request)?.digest(request)
}
pub fn validate_saved_adoption_metadata_plan(
    control: &Path,
    saved: &SavedAdoptionSources,
) -> Result<Sha256Digest> {
    metadata::Metadata::saved_plan(control, saved.plan())?.digest(&saved.plan().setup)
}

pub(crate) fn native_setup_scratch_name(name: &str, bytes: &[u8]) -> String {
    let digest = framed_digest(b"airk/catalog-setup-file/1", &[name.as_bytes(), bytes]);
    format!(".setup-write-{}", &digest.to_string()[..32])
}

#[cfg(any(unix, windows))]
#[path = "service_catalog_setup_storage.rs"]
pub mod storage;

/// This is deliberately not deserializable. Only the native setup owner may
/// construct it after validating reserved storage ownership and the exact saved
/// source configuration. Construction checks metadata syntax, not filesystem
/// truth; a frontend assertion or a digest alone is never preparation proof.
pub struct NativeIndexPreparation {
    registration: ProjectRegistration,
    sources: Vec<SetupSource>,
    storage_identity: Sha256Digest,
    source_configuration_digest: Sha256Digest,
    review: SetupPreparationReview,
    legacy_adoption: Option<Sha256Digest>,
}
impl NativeIndexPreparation {
    pub fn from_validated_native_review(
        registration: ProjectRegistration,
        sources: Vec<SetupSource>,
        storage_identity: Sha256Digest,
        source_configuration_digest: Sha256Digest,
        review: SetupPreparationReview,
    ) -> Result<Self> {
        registration.validate()?;
        validate_setup_sources(&registration, &sources)?;
        encoded_size(&(registration.clone(), sources.clone()), MAX_RECEIPT_BYTES)?;
        Ok(Self {
            registration,
            sources,
            storage_identity,
            source_configuration_digest,
            review,
            legacy_adoption: None,
        })
    }
    fn evidence(&self, request: &IndexSetupRequest) -> Result<PreparationEvidence> {
        ensure!(
            self.registration == request.registration
                && canonical_setup_sources(&self.sources)?
                    == canonical_setup_sources(&request.sources)?,
            "native preparation differs from reserved sources or storage"
        );
        let digest = framed_digest(
            b"airk/catalog-index-setup-prepared/1",
            &[&serde_json::to_vec(&(
                request.identity()?,
                &self.registration,
                canonical_setup_sources(&self.sources)?,
                self.storage_identity,
                self.source_configuration_digest,
                &self.review,
            ))?],
        );
        let digest = match self.legacy_adoption {
            None => digest,
            Some(plan) => framed_digest(
                b"airk/catalog-legacy-adoption-prepared/1",
                &[digest.as_bytes(), plan.as_bytes()],
            ),
        };
        Ok(PreparationEvidence {
            digest,
            storage_identity: self.storage_identity,
            source_configuration_digest: self.source_configuration_digest,
            review: self.review.clone(),
            legacy_adoption: self.legacy_adoption,
        })
    }
}

fn number(revision: CatalogRevision) -> i64 {
    i64::try_from(revision.get()).expect("validated catalog revision")
}
fn catalog(db: &Connection) -> Result<super::CatalogState> {
    super::validate_existing(db)?;
    let state = super::state(db)?;
    ensure!(
        state.activated,
        "index setup requires qualified catalog activation"
    );
    Ok(state)
}
fn no_pending_fence(db: &Connection) -> Result<()> {
    let pending: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage<>'metadata_applied')", [], |row| row.get(0))?;
    ensure!(
        !pending,
        "index setup blocked by pending native authority fence"
    );
    super::super::lifecycle::require_catalog_admission(db)
}
fn current_key(db: &Connection, key: &RequestKey) -> Result<()> {
    let (namespace, epoch): (String, i64) = db.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        key.namespace.to_string() == namespace && u64::try_from(epoch)? == key.epoch,
        "index setup request key expired"
    );
    Ok(())
}
fn saved(
    db: &Connection,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<Option<IndexSetupReceipt>> {
    let row: Option<(String,String,i64,Vec<u8>)> = db.query_row("SELECT kind,stage,revision,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1", [catalog_operation_id(caller,key)?.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    let Some((kind, stage, revision, bytes)) = row else {
        return Ok(None);
    };
    ensure!(
        kind == KIND && bytes.len() <= MAX_RECEIPT_BYTES,
        "index setup request key belongs to another operation or oversized receipt"
    );
    let receipt: IndexSetupReceipt = serde_json::from_slice(&bytes)?;
    receipt.validate()?;
    ensure!(
        receipt.request.caller == caller
            && receipt.request.key == *key
            && stage == receipt.stage.as_str()
            && revision == number(receipt.catalog_revision),
        "index setup receipt owner or stage conflict"
    );
    Ok(Some(receipt))
}
fn replace(db: &Connection, before: &IndexSetupReceipt, after: &IndexSetupReceipt) -> Result<()> {
    after.validate()?;
    ensure!(db.execute("UPDATE index_operation_receipts SET stage=?1,revision=?2,payload=?3 WHERE id=?4 AND kind=?5 AND stage=?6 AND payload=?7", params![after.stage.as_str(),number(after.catalog_revision),serde_json::to_vec(after)?,after.operation_id.to_string(),KIND,before.stage.as_str(),serde_json::to_vec(before)?])? == 1, "index setup receipt changed");
    Ok(())
}
fn additional(request: &IndexSetupRequest) -> Vec<String> {
    request
        .sources
        .iter()
        .filter(|source| !source.owner)
        .map(|source| source.canonical_path.clone())
        .collect()
}

/// Legacy registration/setup cannot activate an index whose normal reservation
/// has not completed native preparation. This guard is metadata-only.
pub(in crate::service::registry) fn require_registration_admission(
    db: &Connection,
    registration: &ProjectRegistration,
    admission: super::super::RegistrationAdmission<'_>,
) -> Result<()> {
    if let super::super::RegistrationAdmission::Split(state) = admission {
        return super::split::require_destination_registration(db, registration, state);
    }
    if let super::super::RegistrationAdmission::Reserved(receipt) = admission {
        let state = catalog(db)?;
        let stored = saved(db, receipt.request.caller, &receipt.request.key)?;
        ensure!(
            stored.as_ref() == Some(receipt)
                && receipt.stage == SetupStage::Reserved
                && receipt.request.registration == *registration
                && state.namespace == receipt.request.key.namespace
                && state.revision == receipt.request.expected_catalog_revision,
            "registration lacks its exact native setup reservation"
        );
        no_pending_fence(db)?;
        return Ok(());
    }
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version >= 12 {
        let authority: u32 = db.query_row(
            "SELECT authority_contract FROM index_catalog_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        if authority >= 1 {
            no_pending_fence(db)?;
            let exact:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions i JOIN projects p ON p.binding=i.active_registration WHERE i.legacy_binding=?1 AND i.active_registration=?1 AND i.backing_root=?2 AND i.data_root=?3 AND i.collection=?4 AND i.tombstone=0 AND i.source_import_state='complete' AND p.registered=1 AND p.cleanup_guard IS NULL)",params![registration.binding.to_string(),registration.root,registration.data_root,registration.collection],|row|row.get(0))?;
            ensure!(
                exact,
                "live catalog registration requires its ready canonical index mapping"
            );
        } else {
            // P5's authority-zero compatibility fixtures retain their original
            // behavior. A newly accepted normal setup still cannot be bypassed.
            let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind=?1 AND stage<>'ready' AND json_extract(payload,'$.request.registration.binding')=?2)",params![KIND,registration.binding.to_string()],|row|row.get(0))?;
            ensure!(!pending, "index setup awaits validated native preparation");
        }
        let collision: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions WHERE data_root=?1 AND collection=?2 AND legacy_binding<>?3)", params![registration.data_root, registration.collection, registration.binding.to_string()], |row| row.get(0))?;
        ensure!(
            !collision,
            "index backing storage belongs to another source binding"
        );
    }
    Ok(())
}

pub(super) fn reserved_sources(db: &Connection, receipt: &IndexSetupReceipt) -> Result<()> {
    let request = &receipt.request;
    let (payload, registered, cleanup): (Vec<u8>, bool, Option<String>) = db.query_row(
        "SELECT substr(payload,1,20481),registered,cleanup_guard FROM projects WHERE binding=?1",
        [request.registration.binding.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(
        payload == serde_json::to_vec(&request.registration)? && registered && cleanup.is_none(),
        "reserved registration changed or was removed"
    );
    let (binding, active, source, import, tombstone): (String,Option<String>,i64,String,bool) = db.query_row("SELECT legacy_binding,active_registration,source_revision,source_import_state,tombstone FROM index_definitions WHERE id=?1", [receipt.index_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    ensure!(
        binding == request.registration.binding.to_string()
            && active.as_deref() == Some(binding.as_str())
            && source == number(CatalogRevision::INITIAL)
            && import == "pending_metadata"
            && !tombstone,
        "reserved index source metadata changed"
    );
    let count: i64 = db.query_row(
        "SELECT count(*) FROM index_sources WHERE index_id=?1",
        [receipt.index_id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        usize::try_from(count)? == request.sources.len(),
        "reserved source set changed"
    );
    for (source, id) in request.sources.iter().zip(&receipt.source_ids) {
        let row: (String, bool, bool) = db.query_row(
            "SELECT canonical_path,owner,enabled FROM index_sources WHERE index_id=?1 AND id=?2",
            params![receipt.index_id.to_string(), id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        ensure!(
            row == (source.canonical_path.clone(), source.owner, false),
            "reserved source identity or state changed"
        );
        let mut statement = db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias LIMIT 17")?;
        let aliases = statement
            .query_map(
                params![receipt.index_id.to_string(), id.to_string()],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut expected = source.aliases.clone();
        expected.sort();
        ensure!(aliases == expected, "reserved source aliases changed");
    }
    Ok(())
}

impl ServiceRegistry {
    pub fn index_setup_receipt(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<IndexSetupReceipt>> {
        let state = catalog(&self.connection)?;
        ensure!(
            state.namespace == key.namespace,
            "index setup namespace changed"
        );
        let receipt = saved(&self.connection, caller, key)?;
        if receipt.is_none() {
            current_key(&self.connection, key)?;
        }
        Ok(receipt)
    }

    pub fn index_setup_pending(&self, binding: ProjectBinding) -> Result<bool> {
        catalog(&self.connection)?;
        self.connection.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions WHERE legacy_binding=?1 AND source_import_state='pending_metadata')", [binding.to_string()], |row|row.get(0)).map_err(Into::into)
    }

    /// Reserve both the legacy storage binding and its new library identity in
    /// one transaction. No watcher, model, indexing job, Project or grant starts.
    pub fn reserve_index_setup(
        &mut self,
        request: &IndexSetupRequest,
    ) -> Result<IndexSetupReceipt> {
        self.reserve_setup_inner(request, None)
    }
    fn reserve_setup_inner(
        &mut self,
        request: &IndexSetupRequest,
        legacy_adoption: Option<Sha256Digest>,
    ) -> Result<IndexSetupReceipt> {
        request.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = catalog(&tx)?;
        ensure!(
            state.namespace == request.key.namespace,
            "index setup namespace changed"
        );
        if let Some(previous) = saved(&tx, request.caller, &request.key)? {
            ensure!(
                previous.request_digest == request.identity()?
                    && previous.legacy_adoption == legacy_adoption,
                "index setup request key conflict"
            );
            return Ok(previous);
        }
        current_key(&tx, &request.key)?;
        no_pending_fence(&tx)?;
        ensure!(
            state.revision == request.expected_catalog_revision,
            "index setup review is stale"
        );
        let occupied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_definitions WHERE legacy_binding=?1 OR id=?2)",
            params![
                request.registration.binding.to_string(),
                setup_index_id(request)?.to_string()
            ],
            |row| row.get(0),
        )?;
        ensure!(
            !occupied,
            "index setup already has a library identity; review the existing index"
        );
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1)",
            [request.registration.binding.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !existing,
            "existing native registration requires reviewed metadata completion"
        );
        super::super::super::data_scope::check(
            Path::new(&request.registration.data_root),
            &self.root,
            state.namespace,
        )?;
        super::super::source_isolation::validate(
            &tx,
            &self.root,
            &request.registration,
            &additional(request),
        )?;
        let receipt = IndexSetupReceipt {
            schema: 1,
            operation_id: catalog_operation_id(request.caller, &request.key)?,
            request: request.clone(),
            request_digest: request.identity()?,
            index_id: setup_index_id(request)?,
            source_ids: setup_source_ids(request)?,
            stage: SetupStage::Reserved,
            catalog_revision: state.revision.next()?,
            source_revision: CatalogRevision::INITIAL,
            preparation: None,
            completion: None,
            legacy_adoption,
        };
        // Reserve enough encoded space for the largest terminal receipt before
        // accepting this metadata intent or authorizing any filesystem effects.
        let mut largest = receipt.clone();
        largest.preparation = Some(PreparationEvidence {
            digest: Sha256Digest::from_bytes([255; 32]),
            storage_identity: Sha256Digest::from_bytes([255; 32]),
            source_configuration_digest: Sha256Digest::from_bytes([255; 32]),
            review: SetupPreparationReview {
                expected_catalog_revision: u64::try_from(i64::MAX - 1)?.try_into()?,
                expected_source_revision: CatalogRevision::INITIAL,
            },
            legacy_adoption,
        });
        largest.completion = Some(SetupCompletion {
            prepared_digest: Sha256Digest::from_bytes([255; 32]),
            expected_catalog_revision: u64::try_from(i64::MAX - 1)?.try_into()?,
            expected_source_revision: CatalogRevision::INITIAL,
            legacy_adoption: legacy_adoption.map(|plan| {
                airk_core::catalog_legacy_adoption::LegacyAdoptionPublication {
                    schema: 1,
                    plan_digest: plan,
                    archive_inventory_sha256: Sha256Digest::from_bytes([255; 32]),
                    original_publication_sha256: Sha256Digest::from_bytes([255; 32]),
                    current: airk_core::identity::RevisionToken {
                        incarnation: Incarnation::from_bytes([255; 16]),
                        generation: u64::MAX,
                        revision: Sha256Digest::from_bytes([255; 32]),
                    },
                    resource_sha256: Sha256Digest::from_bytes([255; 32]),
                    compatibility_sha256: Sha256Digest::from_bytes([255; 32]),
                }
            }),
        });
        largest.stage = SetupStage::Ready;
        largest.catalog_revision = largest
            .completion
            .as_ref()
            .unwrap()
            .expected_catalog_revision
            .next()?;
        largest.source_revision = CatalogRevision::INITIAL.next()?;
        largest.validate()?;
        if let Some(plan_digest) = legacy_adoption {
            airk_core::catalog_legacy_adoption::LegacyAdoptionReceipt {
                schema: 1,
                plan_digest,
                setup: largest.clone(),
            }
            .validate()?;
        }
        receipt.validate()?;
        if legacy_adoption.is_some() {
            // New durable receipt semantics fence older managed readers before
            // any destination metadata or Current evidence can be written.
            super::super::runtime_contract::arm_in_transaction(&tx)?;
        }
        tx.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,?2,'reserved',?3,?4)",
            params![
                receipt.operation_id.to_string(),
                KIND,
                number(receipt.catalog_revision),
                serde_json::to_vec(&receipt)?
            ],
        )?;
        super::super::register_project_in(
            &tx,
            &self.root,
            &request.registration,
            None,
            false,
            super::super::RegistrationAdmission::Reserved(&receipt),
        )?;
        tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?6,1,1,0,NULL,0,'pending_metadata')",params![receipt.index_id.to_string(),request.registration.binding.to_string(),request.registration.root,request.registration.data_root,request.registration.collection,request.name])?;
        for (source, id) in request.sources.iter().zip(&receipt.source_ids) {
            tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,?4,0)",
                params![
                    id.to_string(),
                    receipt.index_id.to_string(),
                    source.canonical_path,
                    source.owner
                ],
            )?;
            for alias in &source.aliases {
                tx.execute(
                    "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
                    params![receipt.index_id.to_string(), id.to_string(), alias],
                )?;
            }
        }
        ensure!(
            tx.execute(
                "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
                params![number(receipt.catalog_revision), number(state.revision)]
            )? == 1,
            "index setup catalog revision changed"
        );
        tx.commit()?;
        Ok(receipt)
    }

    /// Native-only attestation after actual owned storage/source preparation.
    /// This retains pending_metadata; it cannot grant or claim indexed readiness.
    pub fn record_index_preparation(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        request_digest: Sha256Digest,
        native: &NativeIndexPreparation,
    ) -> Result<IndexSetupReceipt> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = catalog(&tx)?;
        ensure!(
            state.namespace == key.namespace,
            "index setup namespace changed"
        );
        let before = saved(&tx, caller, key)?.context("index setup reservation missing")?;
        ensure!(
            before.request_digest == request_digest,
            "index setup request digest changed"
        );
        let evidence = native.evidence(&before.request)?;
        ensure!(
            evidence.legacy_adoption == before.legacy_adoption,
            "native preparation changed reserved adoption intent"
        );
        if let Some(previous) = &before.preparation {
            ensure!(
                previous == &evidence,
                "native preparation changed after acknowledgement"
            );
            return Ok(before);
        }
        no_pending_fence(&tx)?;
        ensure!(
            state.revision == evidence.review.expected_catalog_revision
                && before.source_revision == evidence.review.expected_source_revision,
            "native preparation review is stale"
        );
        reserved_sources(&tx, &before)?;
        super::super::source_isolation::validate(
            &tx,
            &self.root,
            &before.request.registration,
            &additional(&before.request),
        )?;
        let mut after = before.clone();
        after.preparation = Some(evidence);
        after.stage = SetupStage::Prepared;
        replace(&tx, &before, &after)?;
        tx.commit()?;
        Ok(after)
    }

    /// Ready means native source/storage metadata is admitted. Publication and
    /// semantic readiness are separate; the ordinary saved setup jobs run later.
    pub fn complete_index_setup(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        request_digest: Sha256Digest,
        completion: &SetupCompletion,
    ) -> Result<IndexSetupReceipt> {
        ensure!(
            completion.legacy_adoption.is_none(),
            "ordinary setup cannot supply a legacy adoption publication"
        );
        self.complete_setup_inner(caller, key, request_digest, completion, None)
    }

    fn complete_setup_inner(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        request_digest: Sha256Digest,
        completion: &SetupCompletion,
        adoption: Option<&NativeLegacyAdoptionCompletion>,
    ) -> Result<IndexSetupReceipt> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = catalog(&tx)?;
        ensure!(
            state.namespace == key.namespace,
            "index setup namespace changed"
        );
        let before = saved(&tx, caller, key)?.context("index setup reservation missing")?;
        ensure!(
            before.request_digest == request_digest,
            "index setup request digest changed"
        );
        match adoption {
            None => ensure!(
                before
                    .preparation
                    .as_ref()
                    .is_none_or(|p| p.legacy_adoption.is_none()),
                "legacy adoption requires its native publication completion"
            ),
            Some(native) => native.check(&before, completion)?,
        }
        if let Some(previous) = &before.completion {
            ensure!(
                previous == completion,
                "index setup completion request changed"
            );
            return Ok(before);
        }
        no_pending_fence(&tx)?;
        ensure!(
            before.stage == SetupStage::Prepared
                && before
                    .preparation
                    .as_ref()
                    .is_some_and(|evidence| evidence.digest == completion.prepared_digest),
            "index setup lacks its actual native preparation"
        );
        ensure!(
            state.revision == completion.expected_catalog_revision
                && before.source_revision == completion.expected_source_revision,
            "index setup completion review is stale"
        );
        reserved_sources(&tx, &before)?;
        super::super::source_isolation::validate(
            &tx,
            &self.root,
            &before.request.registration,
            &additional(&before.request),
        )?;
        let mut after = before.clone();
        after.stage = SetupStage::Ready;
        after.completion = Some(completion.clone());
        after.catalog_revision = state.revision.next()?;
        after.source_revision = before.source_revision.next()?;
        after.validate()?;
        ensure!(tx.execute("UPDATE index_definitions SET source_import_state='complete',source_revision=?1 WHERE id=?2 AND source_revision=?3 AND source_import_state='pending_metadata' AND tombstone=0",params![number(after.source_revision),after.index_id.to_string(),number(before.source_revision)])? == 1, "index setup source revision changed");
        tx.execute(
            "UPDATE index_sources SET enabled=1 WHERE index_id=?1",
            [after.index_id.to_string()],
        )?;
        if adoption.is_none() {
            super::super::schedule_project_setup(
                &tx,
                before.request.registration.binding,
                before.request.setup_watch,
            )?;
        }
        replace(&tx, &before, &after)?;
        ensure!(
            tx.execute(
                "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
                params![number(after.catalog_revision), number(state.revision)]
            )? == 1,
            "index setup catalog revision changed"
        );
        tx.commit()?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests;
