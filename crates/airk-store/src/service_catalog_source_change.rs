// SPDX-License-Identifier: Apache-2.0
//! Native source mutation bridge. Authorization uses the existing catalog fence;
//! filesystem effects use the existing configuration job and its saved result.
use super::{ServiceRegistry, operations::catalog_operation_id};
use airk_core::{
    catalog_fences::FencedReceipt,
    catalog_management::{CatalogOperation, CatalogReceipt, CatalogRequest, MAX_RECEIPT_BYTES},
    catalog_source_change::{
        AppliedSourceChange, IndexRetirementReason, SourceChangeBusy, SourceChangeObservation,
        SourceChangePlan, SourceConfigurationSnapshot, new_source_id, source_change_job_id,
    },
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, IndexId, encoded_size},
    service::{
        JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, ProjectRegistration, RequestKey,
    },
    service_configuration::{ConfigurationRequest, ConfigurationWork, ProjectConfiguration},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const COMPLETION_KIND: &str = "catalog_source_completion";
const EFFECT_FIELD: &str = "catalog_source_change";
const RESOLUTION_FIELD: &str = "catalog_native_desired_resolution";

#[path = "service_catalog_source_discovery.rs"]
mod discovery;

/// Legacy source mutation admission requires an explicit native catalog review.
/// This instructional error does not establish whether any earlier work ran.
#[derive(Clone, Copy, Debug)]
pub struct SourceChangeReviewRequired;

impl std::fmt::Display for SourceChangeReviewRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "source changes and index retirement require reviewed native catalog coordination",
        )
    }
}

impl std::error::Error for SourceChangeReviewRequired {}

fn number(value: CatalogRevision) -> i64 {
    i64::try_from(value.get()).expect("bounded catalog revision")
}
fn changed(count: usize) -> Result<()> {
    ensure!(count == 1, "source mutation review changed");
    Ok(())
}
fn live(db: &Connection) -> Result<bool> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 11 {
        let application: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
        ensure!(
            application == super::super::APPLICATION,
            "unsupported source registry format"
        );
        super::validate_shape(db, 11)?;
        return Ok(false);
    }
    // Never infer legacy permission from an absent/damaged catalog table or an
    // unknown authority. Registry opens and accepted work share this format check.
    super::validate_existing(db)?;
    let state = super::state(db)?;
    ensure!(state.activated, super::AdmissionPending);
    Ok(state.authority_contract == 1)
}
fn completion_key(request: &CatalogRequest) -> Result<RequestKey> {
    let operation = catalog_operation_id(request.caller, &request.key)?;
    let digest = framed_digest(
        b"airk/catalog-source-completion-key/1",
        &[operation.as_bytes()],
    )
    .to_string();
    Ok(RequestKey {
        namespace: request.key.namespace,
        epoch: request.key.epoch,
        nonce: digest[..32].parse()?,
    })
}
fn completion_id(request: &CatalogRequest) -> Result<Incarnation> {
    catalog_operation_id(request.caller, &completion_key(request)?)
}

fn fence_request(db: &Connection, id: Incarnation) -> Result<Option<CatalogRequest>> {
    let row:Option<(String,Vec<u8>)>=db.query_row("SELECT stage,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management_fenced'",[id.to_string()],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let Some((stage, bytes)) = row else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "source parent receipt exceeds bound"
    );
    let request = if stage == "metadata_applied" {
        let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
        receipt.validate()?;
        receipt.request
    } else {
        let intent: airk_core::catalog_fences::FenceIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        intent.request
    };
    ensure!(
        catalog_operation_id(request.caller, &request.key)? == id,
        "source parent identity conflict"
    );
    Ok(Some(request))
}
fn begin_receipt(db: &Connection, id: Incarnation) -> Result<FencedReceipt> {
    let bytes:Vec<u8>=db.query_row("SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management_fenced' AND stage='metadata_applied'",[id.to_string()],|row|row.get(0))?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "source begin exceeds receipt bound"
    );
    let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
    receipt.validate()?;
    ensure!(
        receipt.metadata.operation_id == id
            && matches!(
                receipt.request.operation,
                CatalogOperation::BeginIndexSourceChange { .. }
            ),
        "source begin receipt mismatch"
    );
    Ok(receipt)
}
fn completed(db: &Connection, begin: &CatalogRequest) -> Result<Option<CatalogReceipt>> {
    let row:Option<(String,Vec<u8>)>=db.query_row("SELECT stage,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind=?2",params![completion_id(begin)?.to_string(),COMPLETION_KIND],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let Some((stage, bytes)) = row else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "source completion exceeds bound"
    );
    if stage == "reserved" {
        ensure!(
            bytes.len() == MAX_RECEIPT_BYTES && bytes.iter().all(|byte| *byte == 0),
            "source completion reservation changed"
        );
        return Ok(None);
    }
    ensure!(
        stage == "metadata_applied",
        "source completion outcome unknown"
    );
    let receipt: CatalogReceipt = serde_json::from_slice(&bytes)?;
    ensure!(
        receipt.schema == 1
            && receipt.operation_id == completion_id(begin)?
            && receipt.caller == begin.caller
            && receipt.key == completion_key(begin)?
            && !receipt.authority_fence_required,
        "source completion identity conflict"
    );
    let CatalogOperation::BeginIndexSourceChange {
        id,
        expected_source_revision,
        plan,
    } = &begin.operation
    else {
        anyhow::bail!("source begin required");
    };
    let effect = desired_effect(begin)?;
    let expected = CatalogRequest {
        caller: begin.caller,
        key: completion_key(begin)?,
        expected_catalog_revision: receipt
            .catalog_revision
            .get()
            .checked_sub(1)
            .context("source completion revision underflow")?
            .try_into()?,
        operation: CatalogOperation::CompleteIndexSourceChange {
            id: *id,
            begin_operation: catalog_operation_id(begin.caller, &begin.key)?,
            expected_source_revision: expected_source_revision.next()?,
            effect_digest: effect.digest,
        },
    };
    ensure!(
        receipt.request_digest == expected.identity()? && effect.job == plan.job,
        "source completion saved request digest mismatch"
    );
    Ok(Some(receipt))
}

/// Reserve terminal bytes before any native fence or RootConfig effect. This
/// row is work-result capacity, not another authorization or confirmation ledger.
pub(in crate::service::registry) fn reserve_completion_slot(
    db: &Connection,
    request: &CatalogRequest,
) -> Result<()> {
    if !matches!(
        request.operation,
        CatalogOperation::BeginIndexSourceChange { .. }
    ) {
        return Ok(());
    }
    let id = completion_id(request)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    if exists {
        ensure!(
            fence_request(db, catalog_operation_id(request.caller, &request.key)?)?.as_ref()
                == Some(request),
            "unclaimed source completion reservation"
        );
        ensure!(
            completed(db, request)?.is_none(),
            "source completion already applied"
        );
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1 AND kind=?2 AND stage='reserved' AND length(payload)=?3)",params![id.to_string(),COMPLETION_KIND,i64::try_from(MAX_RECEIPT_BYTES)?],|row|row.get(0))?;
        ensure!(valid, "source completion capacity unavailable");
        return Ok(());
    }
    let count: i64 = db.query_row("SELECT count(*) FROM index_operation_receipts", [], |row| {
        row.get(0)
    })?;
    let parent = fence_request(db, catalog_operation_id(request.caller, &request.key)?)?.is_some();
    ensure!(
        count < if parent { 1024 } else { 1023 },
        "source mutation requires reserved begin/completion capacity"
    );
    db.execute(
        "INSERT INTO index_operation_receipts VALUES(?1,?2,'reserved',?3,zeroblob(?4))",
        params![
            id.to_string(),
            COMPLETION_KIND,
            number(request.expected_catalog_revision),
            i64::try_from(MAX_RECEIPT_BYTES)?
        ],
    )?;
    Ok(())
}

fn registration(
    db: &Connection,
    id: IndexId,
) -> Result<(
    ProjectRegistration,
    CatalogRevision,
    CatalogRevision,
    bool,
    String,
)> {
    let (root,data,collection,definition,source,tombstone,import):(String,String,String,i64,i64,bool,String)=db.query_row("SELECT backing_root,data_root,collection,definition_revision,source_revision,tombstone,source_import_state FROM index_definitions WHERE id=?1",[id.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
    let registration = ProjectRegistration::new(root, data, collection)?;
    let policy: i64 = db.query_row(
        "SELECT policy_epoch FROM index_definitions WHERE id=?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        policy == 0,
        "source policy mutation requires its qualified policy adapter"
    );
    Ok((
        registration,
        u64::try_from(definition)?.try_into()?,
        u64::try_from(source)?.try_into()?,
        tombstone,
        import,
    ))
}
fn catalog_snapshot(
    db: &Connection,
    id: IndexId,
    primary: &str,
) -> Result<SourceConfigurationSnapshot> {
    let mut stmt=db.prepare("SELECT id,canonical_path,owner FROM index_sources WHERE index_id=?1 AND enabled=1 ORDER BY id LIMIT 18")?;
    let rows = stmt
        .query_map([id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        !rows.is_empty() && rows.len() <= 17,
        "catalog source set unavailable"
    );
    let mut aliases = BTreeMap::new();
    let mut owners = 0;
    for (source, path, owner) in rows {
        if owner {
            ensure!(path == primary, "catalog primary identity mismatch");
            owners += 1;
        }
        let mut stmt=db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias LIMIT 17")?;
        let names = stmt
            .query_map(params![id.to_string(), source], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            if owner {
                names.is_empty()
            } else {
                names.len() == 1
            },
            "unsupported catalog source aliases"
        );
        for alias in names {
            ensure!(
                aliases.insert(alias, path.clone()).is_none(),
                "duplicate catalog source alias"
            );
        }
    }
    ensure!(owners == 1, "catalog primary source unavailable");
    let snapshot = SourceConfigurationSnapshot {
        primary_root: primary.into(),
        aliases,
    };
    snapshot.validate()?;
    Ok(snapshot)
}
fn validate_target_ids(
    db: &Connection,
    request: &CatalogRequest,
    id: IndexId,
    plan: &SourceChangePlan,
) -> Result<()> {
    let operation = catalog_operation_id(request.caller, &request.key)?;
    for source in &plan.sources {
        let existing: Option<(String, bool)> = db
            .query_row(
                "SELECT id,owner FROM index_sources WHERE index_id=?1 AND canonical_path=?2",
                params![id.to_string(), source.canonical_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((saved, owner)) = existing {
            ensure!(
                saved == source.id.to_string() && owner == source.owner,
                "source identity cannot be rebound"
            );
        } else {
            ensure!(
                source.id == new_source_id(id, operation, &source.canonical_path)?,
                "new source identity is not bound to native operation"
            );
        }
    }
    Ok(())
}

fn for_job(db: &Connection, job: Sha256Digest) -> Result<Option<FencedReceipt>> {
    let mut stmt=db.prepare("SELECT id FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage='metadata_applied' AND json_extract(payload,'$.request.operation.operation')='begin_index_source_change' AND json_extract(payload,'$.request.operation.plan.job')=?1 ORDER BY id LIMIT 2")?;
    let ids = stmt
        .query_map([job.to_string()], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(ids.len() <= 1, "ambiguous source configuration job lineage");
    ids.first()
        .map(|id| begin_receipt(db, id.parse()?))
        .transpose()
}
fn pending_index(db: &Connection, begin: &CatalogRequest) -> Result<()> {
    let CatalogOperation::BeginIndexSourceChange {
        id,
        expected_source_revision,
        plan,
    } = &begin.operation
    else {
        anyhow::bail!("source begin required");
    };
    let (registration, _, revision, tombstone, import) = registration(db, *id)?;
    ensure!(
        registration == plan.registration
            && !tombstone
            && import == "pending_metadata"
            && revision == expected_source_revision.next()?,
        "source mutation is no longer pending at its admitted revision"
    );
    let enabled: i64 = db.query_row(
        "SELECT count(*) FROM index_sources WHERE index_id=?1 AND enabled<>0",
        [id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        enabled == 0,
        "pending source mutation has admitted active sources"
    );
    for source in &plan.sources {
        let exact:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_sources WHERE id=?1 AND index_id=?2 AND canonical_path=?3 AND owner=?4 AND enabled=0)",params![source.id.to_string(),id.to_string(),source.canonical_path,source.owner],|row|row.get(0))?;
        ensure!(exact, "pending source identity changed");
        let mut stmt=db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias LIMIT 17")?;
        let aliases = stmt
            .query_map(params![id.to_string(), source.id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut expected = source.aliases.clone();
        expected.sort();
        ensure!(aliases == expected, "pending source aliases changed");
    }
    let aliases: i64 = db.query_row(
        "SELECT count(*) FROM index_source_aliases WHERE index_id=?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        usize::try_from(aliases)? == plan.after.aliases.len(),
        "pending source alias set changed"
    );
    Ok(())
}
fn desired_effect(begin: &CatalogRequest) -> Result<AppliedSourceChange> {
    let CatalogOperation::BeginIndexSourceChange {
        expected_source_revision,
        plan,
        ..
    } = &begin.operation
    else {
        anyhow::bail!("source begin required");
    };
    let mut effect = AppliedSourceChange {
        schema: 1,
        begin_operation: catalog_operation_id(begin.caller, &begin.key)?,
        begin_request_digest: begin.identity()?,
        job: plan.job,
        source_revision: expected_source_revision.next()?,
        actual: plan.after.clone(),
        digest: Sha256Digest::from_bytes([0; 32]),
    };
    effect.digest = effect.identity()?;
    effect.validate()?;
    Ok(effect)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DesiredSourceResolution {
    schema: u32,
    begin_request_digest: Sha256Digest,
    failed_job: JobRecord,
    effect_digest: Sha256Digest,
    collection_incarnation: Incarnation,
    settings_digest: Sha256Digest,
}

fn effect_from_result(
    begin: &CatalogRequest,
    result: &serde_json::Value,
) -> Result<AppliedSourceChange> {
    let CatalogOperation::BeginIndexSourceChange {
        expected_source_revision,
        plan,
        ..
    } = &begin.operation
    else {
        anyhow::bail!("source begin required");
    };
    let effect: AppliedSourceChange = serde_json::from_value(
        result
            .get(EFFECT_FIELD)
            .context("actual native source effect is not recorded")?
            .clone(),
    )?;
    effect.validate()?;
    ensure!(
        effect.begin_operation == catalog_operation_id(begin.caller, &begin.key)?
            && effect.begin_request_digest == begin.identity()?
            && effect.job == plan.job
            && effect.source_revision == expected_source_revision.next()?
            && effect.actual == plan.after,
        "native source effect differs from accepted work"
    );
    Ok(effect)
}
fn validated_effect(db: &Connection, begin: &CatalogRequest) -> Result<AppliedSourceChange> {
    pending_index(db, begin)?;
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &begin.operation else {
        unreachable!()
    };
    let job = super::super::jobs::record(db, plan.job)?;
    ensure!(
        job.project == plan.registration.binding
            && job.kind == JobKind::Configure
            && matches!(job.state, JobState::Succeeded | JobState::Failed),
        "source configuration job is not successfully completed"
    );
    let work = super::super::configuration::work(db, plan.job)?
        .context("source configuration work missing")?;
    ensure!(
        work.engine == plan.engine_identity && work.request == plan.configuration,
        "source configuration work changed"
    );
    let result = super::super::configuration::result(db, plan.job)?
        .context("source configuration result missing")?;
    let effect = effect_from_result(begin, &result)?;
    if job.state == JobState::Failed {
        let resolution: DesiredSourceResolution = serde_json::from_value(
            result
                .get(RESOLUTION_FIELD)
                .context("failed source work requires native desired-state resolution")?
                .clone(),
        )?;
        ensure!(
            resolution.schema == 1
                && serde_json::to_vec(&resolution.failed_job)? == serde_json::to_vec(&job)?
                && matches!(job.outcome, Some(JobOutcome::Failed { .. }))
                && resolution.begin_request_digest == begin.identity()?
                && resolution.effect_digest == effect.digest
                && resolution.collection_incarnation == plan.configuration.incarnation,
            "native failed source resolution lineage changed"
        );
    } else {
        ensure!(
            result.get(RESOLUTION_FIELD).is_none(),
            "successful job cannot adopt failed source resolution"
        );
    }
    Ok(effect)
}

/// Not deserializable: the native worker constructs this only after reading its
/// actual owned RootConfig. These structural checks do not establish FS truth.
pub struct NativeSourceConfiguration {
    registration: ProjectRegistration,
    engine: Sha256Digest,
    actual: SourceConfigurationSnapshot,
}
impl NativeSourceConfiguration {
    pub fn from_validated_native_owner(
        registration: ProjectRegistration,
        engine: Sha256Digest,
        actual: SourceConfigurationSnapshot,
    ) -> Result<Self> {
        registration.validate()?;
        actual.validate()?;
        ensure!(
            registration.root == actual.primary_root,
            "native source owner differs from root snapshot"
        );
        Ok(Self {
            registration,
            engine,
            actual,
        })
    }
}

/// Native-only, non-deserializable actual desired-state evidence. References
/// retain the exact writer through reconciliation; no index/source scan occurs.
pub struct NativeDesiredSourceResolution<'a> {
    native: &'a NativeSourceConfiguration,
    source: &'a crate::FileStore,
    writer: &'a crate::WriterLock,
    incarnation: Incarnation,
    settings: Sha256Digest,
    _selection: super::super::super::collection::Selection,
}
impl<'a> NativeDesiredSourceResolution<'a> {
    pub fn from_validated_native_owner(
        native: &'a NativeSourceConfiguration,
        source: &'a crate::FileStore,
        writer: &'a crate::WriterLock,
    ) -> Result<Self> {
        super::super::super::collection::validate_location(
            source.directory(),
            Path::new(&native.registration.data_root),
            &native.registration.collection,
        )?;
        let selected = super::super::super::collection::Collection::existing(
            Path::new(&native.registration.data_root),
            &native.registration.root,
            &native.registration.collection,
        )?
        .select()?;
        ensure!(
            selected.store.directory().canonicalize()? == source.directory().canonicalize()?,
            "native desired source is no longer the selected incarnation"
        );
        let settings = super::super::super::collection::settings_digest_with_writer(
            source,
            &native.registration.root,
            writer,
        )?;
        let incarnation = source
            .service_incarnation()?
            .context("native desired source collection incarnation missing")?;
        let roots = crate::roots::RootConfig::load(source.directory(), &native.registration.root)?;
        ensure!(
            roots.roots == native.actual.aliases,
            "native desired source configuration differs from actual saved roots"
        );
        Ok(Self {
            native,
            source,
            writer,
            incarnation,
            settings,
            _selection: selected,
        })
    }
    fn recheck(&self) -> Result<()> {
        let fresh = Self::from_validated_native_owner(self.native, self.source, self.writer)?;
        ensure!(
            fresh.incarnation == self.incarnation && fresh.settings == self.settings,
            "native desired source proof changed under held writer"
        );
        Ok(())
    }
}

/// Return the exact native source plan for an owned running configuration job.
/// A caller must compare actual RootConfig with before/after before applying it:
/// before permits the saved change, after reconciles without a second write,
/// any other configuration leaves the mutation pending for explicit recovery.
pub struct SourceChangeWork {
    pub begin: CatalogRequest,
    pub job: Sha256Digest,
}

pub(in crate::service::registry) fn require_configuration_admission(
    db: &Connection,
    request: &ConfigurationRequest,
) -> Result<()> {
    if live(db)?
        && matches!(
            request.configuration,
            ProjectConfiguration::AddRoot { .. }
                | ProjectConfiguration::RemoveRoot { .. }
                | ProjectConfiguration::Unregister
        )
    {
        return Err(SourceChangeReviewRequired.into());
    }
    Ok(())
}
pub(in crate::service::registry) fn require_claim(
    db: &Connection,
    job: Sha256Digest,
) -> Result<()> {
    if !live(db)? {
        return Ok(());
    }
    if let Some(work) = super::super::configuration::work(db, job)? {
        match work.request.configuration {
            ProjectConfiguration::AddRoot { .. } | ProjectConfiguration::RemoveRoot { .. } => {
                let begin = for_job(db, job)?
                    .context("accepted source work requires native catalog reconciliation")?;
                require_no_native_fence(db)?;
                pending_index(db, &begin.request)?;
                let CatalogOperation::BeginIndexSourceChange { plan, .. } =
                    &begin.request.operation
                else {
                    unreachable!()
                };
                ensure!(
                    work.engine == plan.engine_identity && work.request == plan.configuration,
                    "source configuration differs from native begin"
                );
                return Ok(());
            }
            ProjectConfiguration::Unregister => {
                anyhow::bail!("index retirement requires its native catalog fence")
            }
            ref configuration if configuration.is_derived_cleanup() => {
                let record = super::super::jobs::record(db, job)?;
                return require_retired(db, record.project);
            }
            _ => {}
        }
    }
    let record = super::super::jobs::record(db, job)?;
    require_ready_registration(db, record.project)
}
pub(in crate::service::registry) fn validate_configuration_result(
    db: &Connection,
    job: Sha256Digest,
    result: &serde_json::Value,
) -> Result<()> {
    ensure!(
        result.get(RESOLUTION_FIELD).is_none(),
        "reserved native source resolution cannot be supplied as ordinary job JSON"
    );
    if !live(db)? {
        return Ok(());
    }
    let Some(work) = super::super::configuration::work(db, job)? else {
        return Ok(());
    };
    if matches!(
        work.request.configuration,
        ProjectConfiguration::AddRoot { .. } | ProjectConfiguration::RemoveRoot { .. }
    ) {
        let begin = for_job(db, job)?.context("source result lacks native begin")?;
        require_claim(db, job)?;
        pending_index(db, &begin.request)?;
        effect_from_result(&begin.request, result)?;
    }
    Ok(())
}

/// Pin only still-pending accepted source jobs. Called by bounded job receipt
/// pruning, never by evidence reads or a source-directory scanner.
pub(in crate::service::registry) fn pinned_jobs(db: &Connection) -> Result<BTreeSet<Sha256Digest>> {
    if !live(db)? {
        return Ok(BTreeSet::new());
    }
    let mut stmt=db.prepare("SELECT json_extract(r.payload,'$.request.operation.plan.job') FROM index_operation_receipts r JOIN index_definitions i ON i.id=json_extract(r.payload,'$.request.operation.id') WHERE r.kind='catalog_management_fenced' AND r.stage='metadata_applied' AND json_extract(r.payload,'$.request.operation.operation')='begin_index_source_change' AND i.source_import_state='pending_metadata' AND i.tombstone=0 AND i.source_revision=json_extract(r.payload,'$.request.operation.expected_source_revision')+1 LIMIT 1025")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(rows.len() <= 1024, "pending source work capacity exceeded");
    rows.into_iter().map(|row| row.parse()).collect()
}

fn require_no_native_fence(db: &Connection) -> Result<()> {
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage<>'metadata_applied')",[],|row|row.get(0))?;
    ensure!(
        !pending,
        "source metadata mutation blocked by pending native authority fence"
    );
    Ok(())
}
pub(in crate::service::registry) fn require_success_result(
    db: &Connection,
    job: Sha256Digest,
) -> Result<()> {
    if !live(db)? {
        return Ok(());
    }
    let Some(work) = super::super::configuration::work(db, job)? else {
        return Ok(());
    };
    if matches!(
        work.request.configuration,
        ProjectConfiguration::AddRoot { .. }
            | ProjectConfiguration::RemoveRoot { .. }
            | ProjectConfiguration::Unregister
    ) {
        let result = super::super::configuration::result(db, job)?
            .context("native source result must be saved before successful completion")?;
        require_claim(db, job)?;
        validate_configuration_result(db, job, &result)?;
    }
    Ok(())
}

/// Legacy side effects cannot bypass a live canonical pending source or native fence.
pub(in crate::service::registry) fn require_ready_registration(
    db: &Connection,
    project: ProjectBinding,
) -> Result<()> {
    if !live(db)? {
        return Ok(());
    }
    require_no_native_fence(db)?;
    let ready:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions WHERE active_registration=?1 AND legacy_binding=?1 AND tombstone=0 AND source_import_state='complete')",[project.to_string()],|row|row.get(0))?;
    ensure!(
        ready,
        "registration requires ready canonical source authority"
    );
    Ok(())
}
/// Called before legacy retirement or derived purge; the catalog mutation must
/// already have disabled and tombstoned its canonical mapping in this transaction.
pub(in crate::service::registry) fn require_retired(
    db: &Connection,
    project: ProjectBinding,
) -> Result<()> {
    if !live(db)? {
        return Ok(());
    }
    let retired:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions i WHERE i.legacy_binding=?1 AND i.tombstone=1 AND NOT EXISTS(SELECT 1 FROM index_sources s WHERE s.index_id=i.id AND s.enabled<>0))",[project.to_string()],|row|row.get(0))?;
    ensure!(
        retired,
        "index retirement requires its committed native catalog fence"
    );
    Ok(())
}
/// Requeue only accepted source work with exact lineage. The native worker must
/// reconcile actual before/after RootConfig, never blindly repeat the effect.
pub(in crate::service::registry) fn recover_pending_jobs(
    db: &Connection,
    lease: &crate::service::ProjectLease,
    epoch: u64,
) -> Result<()> {
    for job in pinned_jobs(db)? {
        let record = super::super::jobs::record(db, job)?;
        if record.project == lease.binding
            && record.state == JobState::Running
            && record.fencing_generation != epoch
        {
            require_claim(db, job)?;
            if record.attempts < 3 {
                db.execute("UPDATE service_jobs SET state='pending' WHERE id=?1 AND state='running' AND epoch<>?2",params![job.to_string(),i64::try_from(epoch)?])?;
            }
        }
    }
    Ok(())
}

fn complete_native(
    db: &Connection,
    runtime: &Path,
    begin: &FencedReceipt,
    native: &NativeSourceConfiguration,
) -> Result<CatalogReceipt> {
    require_no_native_fence(db)?;
    let CatalogOperation::BeginIndexSourceChange {
        id,
        expected_source_revision,
        plan,
    } = &begin.request.operation
    else {
        unreachable!()
    };
    ensure!(
        native.registration == plan.registration
            && native.engine == plan.engine_identity
            && native.actual == plan.after,
        "source completion requires actual current native RootConfig"
    );
    let effect = validated_effect(db, &begin.request)?;
    let state = super::state(db)?;
    ensure!(
        state.namespace == begin.request.key.namespace && state.activated,
        "source completion namespace changed"
    );
    let request = CatalogRequest {
        caller: begin.request.caller,
        key: completion_key(&begin.request)?,
        expected_catalog_revision: state.revision,
        operation: CatalogOperation::CompleteIndexSourceChange {
            id: *id,
            begin_operation: begin.metadata.operation_id,
            expected_source_revision: expected_source_revision.next()?,
            effect_digest: effect.digest,
        },
    };
    request.validate()?;
    let receipt = CatalogReceipt {
        schema: 1,
        operation_id: completion_id(&begin.request)?,
        caller: request.caller,
        key: request.key.clone(),
        request_digest: request.identity()?,
        catalog_revision: state.revision.next()?,
        authority_fence_required: false,
    };
    encoded_size(&receipt, MAX_RECEIPT_BYTES)?;
    // Release only this reserved native slot before the small final metadata
    // writes, so reserved pages remain available at the database size bound.
    changed(db.execute("UPDATE index_operation_receipts SET stage='metadata_applied',revision=?1,payload=?2 WHERE id=?3 AND kind=?4 AND stage='reserved' AND length(payload)=?5",params![number(receipt.catalog_revision),serde_json::to_vec(&receipt)?,receipt.operation_id.to_string(),COMPLETION_KIND,i64::try_from(MAX_RECEIPT_BYTES)?])?)?;
    mutate(db, &request, runtime)?;
    changed(db.execute(
        "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
        params![number(receipt.catalog_revision), number(state.revision)],
    )?)?;
    Ok(receipt)
}

impl ServiceRegistry {
    /// Bounded owning-worker startup reconciliation only. No filesystem reads,
    /// writes or per-query scans; actual current RootConfig proof remains native.
    pub fn pending_source_completions(
        &self,
        lease: &crate::service::ProjectLease,
    ) -> Result<Vec<SourceChangeWork>> {
        super::super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        ensure!(
            !lease.derived,
            "derived worker cannot complete source authority"
        );
        if !live(&self.connection)? {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for job in pinned_jobs(&self.connection)? {
            let record = super::super::jobs::record(&self.connection, job)?;
            if record.project != lease.binding
                || record.kind != JobKind::Configure
                || record.state != JobState::Succeeded
            {
                continue;
            }
            let begin = for_job(&self.connection, job)?
                .context("pending source completion lost its exact begin")?;
            validated_effect(&self.connection, &begin.request)?;
            ensure!(
                result.len() < 1024,
                "source completion recovery capacity exceeded"
            );
            result.push(SourceChangeWork {
                begin: begin.request,
                job,
            });
        }
        Ok(result)
    }
    pub fn catalog_source_work(
        &self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
    ) -> Result<Option<SourceChangeWork>> {
        super::super::owner(&self.root, true, self.incarnation()?)?;
        if !live(&self.connection)? {
            return Ok(None);
        }
        let Some(begin) = for_job(&self.connection, job)? else {
            require_claim(&self.connection, job)?;
            return Ok(None);
        };
        let epoch = super::super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let record = super::super::jobs::record(&self.connection, job)?;
        ensure!(
            record.project == lease.binding
                && record.fencing_generation == epoch
                && record.state == JobState::Running,
            "source configuration is not owned by this worker"
        );
        require_claim(&self.connection, job)?;
        Ok(Some(SourceChangeWork {
            begin: begin.request,
            job,
        }))
    }
    pub fn qualify_catalog_source_result(
        &self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        native: &NativeSourceConfiguration,
        result: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure!(
            result.get(RESOLUTION_FIELD).is_none(),
            "ordinary source result cannot assert reserved native desired-state resolution"
        );
        let work = self
            .catalog_source_work(lease, job)?
            .context("source mutation native job required")?;
        let CatalogOperation::BeginIndexSourceChange {
            expected_source_revision,
            plan,
            ..
        } = &work.begin.operation
        else {
            unreachable!()
        };
        ensure!(
            native.registration == plan.registration
                && native.engine == plan.engine_identity
                && native.actual == plan.after,
            "actual native RootConfig does not match desired source mutation"
        );
        let mut effect = AppliedSourceChange {
            schema: 1,
            begin_operation: catalog_operation_id(work.begin.caller, &work.begin.key)?,
            begin_request_digest: work.begin.identity()?,
            job,
            source_revision: expected_source_revision.next()?,
            actual: native.actual.clone(),
            digest: Sha256Digest::from_bytes([0; 32]),
        };
        effect.digest = effect.identity()?;
        effect.validate()?;
        let mut value = result.clone();
        value
            .as_object_mut()
            .context("configuration result must be structured")?
            .insert(EFFECT_FIELD.into(), serde_json::to_value(effect)?);
        encoded_size(&value, MAX_RECEIPT_BYTES)?;
        Ok(value)
    }

    /// Native-only finalization; public CatalogApply cannot promote a supplied
    /// digest. Actual current ownership plus the exact saved successful job and
    /// qualified native result are all required. Replays precede fresh reviews.
    pub fn complete_catalog_source_change(
        &mut self,
        begin_operation: Incarnation,
        native: &NativeSourceConfiguration,
    ) -> Result<CatalogReceipt> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let begin = begin_receipt(&tx, begin_operation)?;
        if let Some(receipt) = completed(&tx, &begin.request)? {
            return Ok(receipt);
        }
        let CatalogOperation::BeginIndexSourceChange { plan, .. } = &begin.request.operation else {
            unreachable!()
        };
        ensure!(
            super::super::jobs::record(&tx, plan.job)?.state == JobState::Succeeded,
            "failed source work requires its native desired-state resolution adapter"
        );
        let receipt = complete_native(&tx, &self.root, &begin, native)?;
        tx.commit()?;
        Ok(receipt)
    }
    /// Explicit desired-only recovery. Preserve the accepted failed job and its
    /// complete outcome; only existing result/metadata reservations are settled.
    pub fn reconcile_failed_source_change(
        &mut self,
        lease: &crate::service::ProjectLease,
        begin_operation: Incarnation,
        proof: &NativeDesiredSourceResolution<'_>,
    ) -> Result<CatalogReceipt> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let begin = begin_receipt(&tx, begin_operation)?;
        if let Some(receipt) = completed(&tx, &begin.request)? {
            return Ok(receipt);
        }
        super::super::jobs::worker_epoch(&tx, &self.root, lease)?;
        ensure!(
            !lease.derived,
            "derived worker cannot resolve source authority"
        );
        require_no_native_fence(&tx)?;
        pending_index(&tx, &begin.request)?;
        let CatalogOperation::BeginIndexSourceChange { plan, .. } = &begin.request.operation else {
            unreachable!()
        };
        ensure!(
            lease.binding == plan.registration.binding
                && proof.native.registration == plan.registration
                && proof.native.engine == plan.engine_identity
                && proof.native.actual == plan.after
                && proof.incarnation == plan.configuration.incarnation,
            "native failed source resolution owner differs"
        );
        proof.recheck()?;
        let job = super::super::jobs::record(&tx, plan.job)?;
        ensure!(
            job.state == JobState::Failed && matches!(job.outcome, Some(JobOutcome::Failed { .. })),
            "native desired-state resolution requires the saved failed job"
        );
        let work = super::super::configuration::work(&tx, plan.job)?
            .context("failed source configuration missing")?;
        ensure!(
            work.engine == plan.engine_identity && work.request == plan.configuration,
            "failed source work changed"
        );
        let effect = desired_effect(&begin.request)?;
        let tag = DesiredSourceResolution {
            schema: 1,
            begin_request_digest: begin.request.identity()?,
            failed_job: job,
            effect_digest: effect.digest,
            collection_incarnation: proof.incarnation,
            settings_digest: proof.settings,
        };
        let mut result = super::super::configuration::result(&tx, plan.job)?
            .unwrap_or_else(|| serde_json::json!({}));
        let object = result
            .as_object_mut()
            .context("failed source result cannot be safely extended")?;
        let effect_json = serde_json::to_value(&effect)?;
        let tag_json = serde_json::to_value(&tag)?;
        if let Some(old) = object.get(EFFECT_FIELD) {
            ensure!(
                old == &effect_json,
                "existing source effect evidence differs"
            );
        }
        if let Some(old) = object.get(RESOLUTION_FIELD) {
            ensure!(
                old == &tag_json,
                "existing native resolution evidence differs"
            );
        }
        object.insert(EFFECT_FIELD.into(), effect_json);
        object.insert(RESOLUTION_FIELD.into(), tag_json);
        encoded_size(&result, MAX_RECEIPT_BYTES)?;
        super::super::configuration::write_result(&tx, plan.job, &serde_json::to_vec(&result)?)?;
        super::super::background::set(&tx, plan.registration.binding, JobKind::Index, true)?;
        let receipt = complete_native(&tx, &self.root, &begin, proof.native)?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn catalog_source_change_observation(
        &self,
        begin_operation: Incarnation,
    ) -> Result<SourceChangeObservation> {
        discovery::source_observation(&self.connection, begin_operation)
    }
}

fn historical_inactive_retirement(
    db: &Connection,
    id: IndexId,
    saved: &ProjectRegistration,
    source: CatalogRevision,
) -> Result<bool> {
    if source != CatalogRevision::INITIAL {
        return Ok(false);
    }
    type HistoricalRegistrationRow = (Vec<u8>, i64, bool, bool, Option<String>);
    let row:Option<HistoricalRegistrationRow>=db.query_row("SELECT substr(payload,1,20481),epoch,paused,registered,cleanup_guard FROM projects WHERE binding=?1",[saved.binding.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    let Some((payload, epoch, paused, registered, cleanup)) = row else {
        return Ok(false);
    };
    if registered || !paused {
        return Ok(false);
    }
    let retained = super::super::decode(&payload, epoch, paused)?;
    ensure!(
        retained.registration == *saved,
        "historical inactive retained identity differs"
    );
    if let Some(guard) = cleanup {
        let _: Sha256Digest = guard.parse()?;
    }
    let imported: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM legacy_index_projects WHERE legacy_binding=?1)",
        [saved.binding.to_string()],
        |r| r.get(0),
    )?;
    if !imported {
        return Ok(false);
    }
    let active: Option<String> = db.query_row(
        "SELECT active_registration FROM index_definitions WHERE id=?1",
        [id.to_string()],
        |r| r.get(0),
    )?;
    ensure!(
        active.as_deref() == Some(saved.binding.to_string().as_str()),
        "historical inactive backing pointer differs"
    );
    let mut statement=db.prepare("SELECT id,canonical_path,owner,enabled FROM index_sources WHERE index_id=?1 ORDER BY id LIMIT 18")?;
    let rows = statement
        .query_map([id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() || rows.len() > 17 || rows.iter().any(|r| !r.3) {
        return Ok(false);
    }
    let mut owners = 0;
    let mut aliases_total = 0;
    for (source, path, owner, _) in rows {
        let mut aliases=db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias LIMIT 17")?;
        let aliases = aliases
            .query_map(params![id.to_string(), source], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        aliases_total += aliases.len();
        let root = airk_core::source_policy::SourceRoot {
            id: source.parse()?,
            canonical_path: path,
            aliases,
            owner,
        };
        root.validate()?;
        if owner {
            owners += 1;
            ensure!(
                root.canonical_path == saved.root,
                "historical inactive primary root differs"
            );
        }
    }
    let total: i64 = db.query_row(
        "SELECT count(*) FROM index_source_aliases WHERE index_id=?1",
        [id.to_string()],
        |r| r.get(0),
    )?;
    ensure!(
        owners == 1 && usize::try_from(total)? == aliases_total,
        "historical inactive source roster differs"
    );
    Ok(true)
}

pub(in crate::service::registry) fn mutate(
    db: &Connection,
    request: &CatalogRequest,
    runtime: &Path,
) -> Result<()> {
    match &request.operation {
        CatalogOperation::BeginIndexSourceChange {
            id,
            expected_source_revision,
            plan,
        } => {
            plan.validate()?;
            let (saved, _, revision, tombstone, import) = registration(db, *id)?;
            ensure!(
                !tombstone
                    && import == "complete"
                    && revision == *expected_source_revision
                    && saved == plan.registration,
                "source change index review stale"
            );
            ensure!(
                catalog_snapshot(db, *id, &saved.root)? == plan.before,
                "catalog/native source configuration review differs"
            );
            let registered:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1 AND cleanup_guard IS NULL)",[saved.binding.to_string()],|row|row.get(0))?;
            ensure!(registered, "source change registration unavailable");
            let (pending,running):(u32,u32)=db.query_row("SELECT coalesce(sum(state='pending'),0),coalesce(sum(state='running'),0) FROM service_jobs WHERE project=?1 AND state IN('pending','running')",[saved.binding.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
            if pending != 0 || running != 0 {
                return Err(anyhow::Error::new(SourceChangeBusy {
                    index_id: *id,
                    pending_jobs: pending,
                    running_jobs: running,
                }));
            }
            validate_target_ids(db, request, *id, plan)?;
            super::super::source_isolation::validate(
                db,
                runtime,
                &saved,
                &plan.after.aliases.values().cloned().collect::<Vec<_>>(),
            )?;
            reserve_completion_slot(db, request)?;
            super::super::jobs::require_request_capacity(db)?;
            let work = ConfigurationWork {
                engine: plan.engine_identity,
                request: plan.configuration.clone(),
            };
            let job = super::super::jobs::insert_new_job(
                db,
                saved.binding,
                request.caller,
                &request.key,
                JobKind::Configure,
                Some(&work),
            )?;
            ensure!(
                job == plan.job && job == source_change_job_id(request)?,
                "source configuration job identity changed"
            );
            super::super::jobs::insert_request_receipt(db, request.caller, &request.key, job)?;
            db.execute(
                "UPDATE index_sources SET enabled=0 WHERE index_id=?1",
                [id.to_string()],
            )?;
            db.execute(
                "DELETE FROM index_source_aliases WHERE index_id=?1",
                [id.to_string()],
            )?;
            for source in &plan.sources {
                db.execute("INSERT INTO index_sources(id,index_id,canonical_path,owner,enabled) SELECT ?1,?2,?3,?4,0 WHERE NOT EXISTS(SELECT 1 FROM index_sources WHERE id=?1)",params![source.id.to_string(),id.to_string(),source.canonical_path,source.owner])?;
                for alias in &source.aliases {
                    db.execute(
                        "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
                        params![id.to_string(), source.id.to_string(), alias],
                    )?;
                }
            }
            changed(db.execute("UPDATE index_definitions SET source_import_state='pending_metadata',source_revision=?1 WHERE id=?2 AND source_revision=?3 AND source_import_state='complete' AND tombstone=0",params![number(revision.next()?),id.to_string(),number(revision)])?)?;
        }
        CatalogOperation::CompleteIndexSourceChange {
            id,
            begin_operation,
            expected_source_revision,
            effect_digest,
        } => {
            let begin = begin_receipt(db, *begin_operation)?;
            let CatalogOperation::BeginIndexSourceChange {
                id: source,
                expected_source_revision: before,
                plan,
            } = &begin.request.operation
            else {
                unreachable!()
            };
            let (saved, _, revision, tombstone, import) = registration(db, *id)?;
            ensure!(
                source == id
                    && !tombstone
                    && import == "pending_metadata"
                    && saved == plan.registration
                    && revision == *expected_source_revision
                    && revision == before.next()?,
                "source completion scope changed"
            );
            let effect = validated_effect(db, &begin.request)?;
            ensure!(
                effect.digest == *effect_digest && effect.actual == plan.after,
                "source completion lacks actual native effect"
            );
            for source in &plan.sources {
                changed(db.execute("UPDATE index_sources SET enabled=1 WHERE id=?1 AND index_id=?2 AND canonical_path=?3 AND owner=?4",params![source.id.to_string(),id.to_string(),source.canonical_path,source.owner])?)?;
            }
            changed(db.execute("UPDATE index_definitions SET source_import_state='complete' WHERE id=?1 AND source_revision=?2 AND source_import_state='pending_metadata' AND tombstone=0",params![id.to_string(),number(revision)])?)?;
        }
        CatalogOperation::RetireIndex {
            id,
            expected_definition_revision,
            expected_source_revision,
            reason,
        } => {
            let (saved, definition, source, tombstone, _) = registration(db, *id)?;
            ensure!(
                definition == *expected_definition_revision && source == *expected_source_revision,
                "index retirement review stale"
            );
            let historical = if tombstone {
                historical_inactive_retirement(db, *id, &saved, source)?
            } else {
                false
            };
            ensure!(
                !tombstone || historical,
                "index already retired or inactive backfill is not intact"
            );
            if let IndexRetirementReason::ConfirmedMissingSource {
                registration_epoch,
                configuration_epoch,
            } = reason
            {
                let observed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM projects p LEFT JOIN service_watch w ON w.project=p.binding WHERE p.binding=?1 AND p.registered=1 AND p.epoch=?2 AND coalesce(w.configuration_epoch,0)=?3)",params![saved.binding.to_string(),i64::try_from(*registration_epoch)?,i64::try_from(*configuration_epoch)?],|row|row.get(0))?;
                ensure!(observed, "missing-source retirement observation expired");
            }
            changed(db.execute("UPDATE index_definitions SET tombstone=1,definition_revision=?1,source_revision=?2 WHERE id=?3 AND definition_revision=?4 AND source_revision=?5 AND tombstone=?6",params![number(definition.next()?),number(source.next()?),id.to_string(),number(definition),number(source),tombstone])?)?;
            db.execute(
                "UPDATE index_sources SET enabled=0 WHERE index_id=?1",
                [id.to_string()],
            )?;
            if !historical {
                super::super::unregistration::retire(db, saved.binding, None)?;
            }
            // The validated P5 inactive case preserves its legacy registration,
            // watcher/cleanup state and accepted jobs byte-for-byte. Only the
            // canonical source authority is finalized after the native fence.
        }
        _ => anyhow::bail!("source mutation operation required"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "service_catalog_source_change_tests.rs"]
mod tests;
