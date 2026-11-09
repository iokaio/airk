// SPDX-License-Identifier: Apache-2.0
//! Bounded native split intent, exact jobs and resumable restrictive cutover.
//! Registry reads never create storage, capture sources, start jobs or migrate.
use super::{ServiceRegistry, operations::index_record};
use airk_core::{
    catalog_management::{CatalogOperation, CatalogRequest, MAX_PAGE_BYTES, MAX_RECEIPT_BYTES},
    catalog_source_change::SourceConfigurationSnapshot,
    catalog_split::*,
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, IndexId, SourceImportState, encoded_size},
    service::{JobKind, JobState, ProjectRegistration, RequestKey},
    service_configuration::{ConfigurationRequest, ConfigurationWork, ProjectConfiguration},
    source_policy::{POLICY_SCHEMA, PolicyChange, SourcePolicy, SplitPolicyPlan},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

mod finalization;
pub(crate) use finalization::CheckpointReceiptClassifier;
const RESERVED_COMPLETION_BYTES: usize = 3 * 1024 * 1024;
const MAX_ACTIVE: i64 = 8;
/// Conservative native format capacity, independent of selected document size.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NativeSplitMetadataCapacity {
    pub snapshot_upper_bound: u64,
    pub per_participant: u64,
    pub total: u64,
}
pub fn native_metadata_capacity(
    request: &SplitRequest,
    baseline: &SourcePolicy,
    plan: &SplitPolicyPlan,
) -> Result<NativeSplitMetadataCapacity> {
    validate_snapshot_admission(request, baseline, plan)?;
    let immutable=serde_json::to_vec(&serde_json::json!({"request":request,"baseline":baseline,"plan":plan,"receipt":null,"receipt_digest":"0".repeat(64)}))?.len();
    let snapshot = immutable
        .checked_sub(4)
        .and_then(|n| n.checked_add(MAX_SPLIT_RECEIPT_BYTES + MAX_SPLIT_FINALIZATION_BYTES + 512))
        .context("split metadata capacity overflow")? as u64;
    // Each real job may publish its own immutable phase capsule. Fixed cutover,
    // activation and interrupted-prefix checkpoints have four additional slots.
    let capsules = snapshot
        .checked_mul((MAX_SPLIT_JOBS + 4) as u64)
        .context("split capsule capacity overflow")?;
    // StoredPolicy includes an escaped owning root and the parsed body. Reserve
    // its full bounded envelope plus one replacement/staging copy, not only the
    // compact public body's bytes. This is native metadata capacity, not a claim
    // that selected source payloads fit the separately reviewed build budget.
    let inherited = if request.extraction_policy.is_some() {
        2 * 160 * 1024
    } else {
        0
    };
    let fixed = (MAX_PAGE_BYTES + 16384 + 16384 + 65536 + 2 * 16384) as u64 + inherited;
    let required = capsules
        .checked_add(fixed)
        .context("split metadata capacity overflow")?;
    let per_participant = required
        .checked_add(65535)
        .context("split metadata capacity overflow")?
        / 65536
        * 65536;
    Ok(NativeSplitMetadataCapacity {
        snapshot_upper_bound: snapshot,
        per_participant,
        total: per_participant
            .checked_mul(2)
            .context("split metadata capacity overflow")?,
    })
}

fn available(db: &Connection) -> Result<bool> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        matches!(version, 11..=13),
        "unsupported native split scheduler format"
    );
    if version == 11 {
        return Ok(false);
    }
    super::validate_existing(db)?;
    Ok(super::state(db)?.policy_contract >= SPLIT_CONTRACT)
}
fn key_current(db: &Connection, key: &RequestKey) -> Result<()> {
    let (namespace, epoch): (String, i64) = db.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        namespace == key.namespace.to_string() && u64::try_from(epoch)? == key.epoch,
        "split request key expired"
    );
    Ok(())
}
fn require_unused_action_key(
    db: &Connection,
    caller: Incarnation,
    key: &RequestKey,
    accepted_job: Option<Sha256Digest>,
) -> Result<()> {
    let catalog: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1)",
        [airk_core::catalog_management::catalog_operation_id(caller, key)?.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        !catalog,
        "split request key belongs to another native catalog/setup action"
    );
    let job:Option<String>=db.query_row("SELECT job FROM service_requests WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|row.get(0)).optional()?;
    ensure!(
        job.as_deref() == accepted_job.as_ref().map(|job| job.to_string()).as_deref(),
        "split request key belongs to another native job"
    );
    let plan:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM service_plan_requests WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4)",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|row.get(0))?;
    ensure!(
        !plan,
        "split request key belongs to a native maintenance plan"
    );
    if available(db)? {
        let split:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_split_operations WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4) OR EXISTS(SELECT 1 FROM index_split_recoveries WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4)",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|row.get(0))?;
        ensure!(!split, "split request key belongs to another split action");
    }
    Ok(())
}

fn number(value: CatalogRevision) -> i64 {
    value.get() as i64
}
fn get(db: &Connection, operation: Incarnation) -> Result<Option<SplitSnapshot>> {
    if !available(db)? {
        return Ok(None);
    }
    let row: Option<(Vec<u8>, i64, String)> = db.query_row("SELECT substr(payload,1,49153),revision,receipt_digest FROM index_split_operations WHERE operation_id=?1", [operation.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    row.map(|(bytes, revision, digest)| {
        ensure!(
            bytes.len() <= MAX_PAGE_BYTES,
            "split snapshot exceeds bounded metadata"
        );
        let mut value: SplitSnapshot = serde_json::from_slice(&bytes)?;
        value.finalization = finalization::observation(db, &value)?;
        value.validate()?;
        ensure!(
            value.request.operation_id()? == operation
                && number(value.receipt.revision) == revision
                && value.receipt.identity(&value.request)?.to_string() == digest,
            "split receipt identity differs"
        );
        Ok(value)
    })
    .transpose()
}
fn source_state(
    db: &Connection,
    id: IndexId,
    expected: CatalogRevision,
) -> Result<SplitSourceState> {
    super::validate_existing(db)?;
    let state = super::state(db)?;
    ensure!(
        state.activated && state.authority_contract == 1 && state.revision == expected,
        "split catalog review stale"
    );
    let index = index_record(db, id)?;
    ensure!(
        !index.tombstone && index.source_import == SourceImportState::Complete,
        "split index sources unavailable"
    );
    let registered: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1 AND cleanup_guard IS NULL)", [index.backing.binding.to_string()], |row| row.get(0))?;
    ensure!(registered, "split registration unavailable");
    let roots = super::policy::source_roots(db, id)?;
    let (policy, baseline) = if let Some(snapshot) = super::policy::snapshot(db, id)? {
        ensure!(
            snapshot.application.is_some(),
            "split waits for existing policy removal"
        );
        (
            snapshot.policy,
            SplitBaseline::Persisted {
                reference: index
                    .policy
                    .clone()
                    .context("split policy reference absent")?,
            },
        )
    } else {
        (
            SourcePolicy {
                schema: POLICY_SCHEMA.into(),
                index_id: id,
                owner_binding: index.backing.binding,
                namespace: "collections-1.4".into(),
                collection: index.backing.collection.clone(),
                epoch: CatalogRevision::INITIAL,
                roots: roots.clone(),
                positive_includes: None,
                denies: vec![],
            },
            SplitBaseline::VirtualUnrestricted,
        )
    };
    ensure!(
        policy.roots == roots,
        "split source descriptors differ from policy"
    );
    policy.validate()?;
    let mut aliases = BTreeMap::new();
    let mut primary = None;
    for root in &roots {
        if root.owner {
            ensure!(
                primary.replace(root.canonical_path.clone()).is_none() && root.aliases.is_empty(),
                "split primary source differs"
            );
        } else {
            ensure!(
                root.aliases.len() == 1
                    && aliases
                        .insert(root.aliases[0].clone(), root.canonical_path.clone())
                        .is_none(),
                "split source aliases differ"
            );
        }
    }
    let source_configuration = SourceConfigurationSnapshot {
        primary_root: primary.context("split primary missing")?,
        aliases,
    };
    source_configuration.validate()?;
    let paused: bool = db.query_row(
        "SELECT paused FROM projects WHERE binding=?1",
        [index.backing.binding.to_string()],
        |row| row.get(0),
    )?;
    let watch: Option<(bool, i64)> = db
        .query_row(
            "SELECT enabled,configuration_epoch FROM service_watch WHERE project=?1",
            [index.backing.binding.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let watch_digest = framed_digest(
        b"airk/split-watch-policy/1",
        &[&serde_json::to_vec(&(paused, watch))?],
    );
    let mut statement = db.prepare("SELECT p.id,p.scope_revision,m.revision FROM query_projects p JOIN project_index_memberships m ON m.project_id=p.id WHERE m.index_id=?1 ORDER BY p.id LIMIT 33")?;
    let rows = statement
        .query_map([id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= MAX_SPLIT_PROJECTS,
        "split affected Project census exceeds capacity"
    );
    let affected_projects = rows
        .into_iter()
        .map(|(id, scope, member)| {
            Ok(SplitProjectEffect {
                project_id: id.parse()?,
                expected_scope_revision: u64::try_from(scope)?.try_into()?,
                expected_original_membership: Some(u64::try_from(member)?.try_into()?),
                retain_original: true,
                destination_scope: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let result = SplitSourceState {
        catalog_revision: state.revision,
        index,
        policy,
        baseline,
        source_configuration,
        watch_digest,
        affected_projects,
    };
    encoded_size(&result, MAX_PAGE_BYTES)?;
    Ok(result)
}
fn projects_current(db: &Connection, request: &SplitRequest) -> Result<()> {
    let mut statement = db.prepare("SELECT project_id FROM project_index_memberships WHERE index_id=?1 ORDER BY project_id LIMIT 33")?;
    let actual = statement
        .query_map([request.original_index.to_string()], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        actual.len() <= MAX_SPLIT_PROJECTS,
        "split affected Project census exceeds capacity"
    );
    let reviewed = request
        .projects
        .iter()
        .filter(|effect| effect.expected_original_membership.is_some())
        .map(|effect| effect.project_id.to_string())
        .collect::<BTreeSet<_>>();
    ensure!(
        actual.into_iter().collect::<BTreeSet<_>>() == reviewed,
        "split affected Project census changed"
    );
    for effect in &request.projects {
        let current: i64 = db.query_row(
            "SELECT scope_revision FROM query_projects WHERE id=?1",
            [effect.project_id.to_string()],
            |row| row.get(0),
        )?;
        let membership: Option<i64> = db.query_row("SELECT revision FROM project_index_memberships WHERE project_id=?1 AND index_id=?2", params![effect.project_id.to_string(),request.original_index.to_string()], |row| row.get(0)).optional()?;
        ensure!(
            current == number(effect.expected_scope_revision)
                && membership == effect.expected_original_membership.map(number),
            "split Project review stale"
        );
    }
    Ok(())
}
fn baseline_current(db: &Connection, value: &SplitSnapshot) -> Result<()> {
    let state = source_state(db, value.request.original_index, super::state(db)?.revision)?;
    ensure!(
        state.index.backing == value.request.original_registration
            && state.index.definition_revision == value.request.expected_definition_revision
            && state.index.source_revision == value.request.expected_source_revision
            && state.policy == value.baseline
            && state.baseline == value.request.baseline
            && state.source_configuration == value.request.source_configuration
            && state.watch_digest == value.request.original_watch_digest,
        "split original state changed; review again"
    );
    projects_current(db, &value.request)
}
fn replace(db: &Connection, old: &SplitSnapshot, mut next: SplitReceipt) -> Result<SplitSnapshot> {
    ensure!(
        old.finalization.is_none(),
        "frozen terminal plan permits only its exact finalization recovery"
    );
    next.revision = old.receipt.revision.next()?;
    next.previous_receipt_digest = Some(old.receipt.identity(&old.request)?);
    next.catalog_revision = next.catalog_revision.max(super::state(db)?.revision);
    let value = SplitSnapshot {
        request: old.request.clone(),
        baseline: old.baseline.clone(),
        plan: old.plan.clone(),
        receipt_digest: next.identity(&old.request)?,
        receipt: next,
        finalization: None,
    };
    value.validate()?;
    let bytes = serde_json::to_vec(&value.receipt)?;
    let charge = bytes
        .len()
        .checked_mul(2)
        .and_then(|v| v.checked_add(256))
        .context("split receipt capacity overflow")?;
    let remaining: i64 = db.query_row(
        "SELECT length(completion_reserve) FROM index_split_operations WHERE operation_id=?1",
        [value.receipt.operation_id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        remaining >= i64::try_from(charge)?,
        "split reserved completion capacity exhausted"
    );
    db.execute(
        "INSERT INTO index_split_history VALUES(?1,?2,?3,?4)",
        params![
            value.receipt.operation_id.to_string(),
            number(value.receipt.revision),
            value.receipt.identity(&value.request)?.to_string(),
            bytes
        ],
    )?;
    ensure!(db.execute("UPDATE index_split_operations SET revision=?1,receipt_digest=?2,payload=?3,completion_reserve=zeroblob(?4) WHERE operation_id=?5 AND revision=?6 AND receipt_digest=?7", params![number(value.receipt.revision),value.receipt.identity(&value.request)?.to_string(),serde_json::to_vec(&value)?,remaining-i64::try_from(charge)?,value.receipt.operation_id.to_string(),number(old.receipt.revision),old.receipt.identity(&old.request)?.to_string()])? == 1, "split receipt CAS changed");
    if matches!(
        value.receipt.state,
        SplitRunState::Complete
            | SplitRunState::CanceledBeforeFence {
                cleanup_pending: false
            }
    ) {
        db.execute(
            "UPDATE index_split_participants SET active=0 WHERE operation_id=?1",
            [value.receipt.operation_id.to_string()],
        )?;
        db.execute("UPDATE index_split_operations SET completion_reserve=zeroblob(0) WHERE operation_id=?1", [value.receipt.operation_id.to_string()])?;
    }
    Ok(value)
}
fn exact(db: &Connection, operation: Incarnation, expected: Sha256Digest) -> Result<SplitSnapshot> {
    let value = get(db, operation)?.context("split operation not retained")?;
    ensure!(
        value.receipt.identity(&value.request)? == expected,
        "split lifecycle review stale"
    );
    Ok(value)
}

/// A live native registry observation, not a serializable authority claim.
#[derive(Clone, Debug)]
pub struct NativeSplitState(pub(crate) SplitSnapshot);
impl NativeSplitState {
    pub fn snapshot(&self) -> &SplitSnapshot {
        &self.0
    }
}

impl ServiceRegistry {
    pub fn catalog_split_source_state(
        &self,
        id: IndexId,
        expected: CatalogRevision,
    ) -> Result<SplitSourceState> {
        let tx = self.connection.unchecked_transaction()?;
        source_state(&tx, id, expected)
    }
    pub fn catalog_split_status(&self, operation: Incarnation) -> Result<Option<SplitSnapshot>> {
        let tx = self.connection.unchecked_transaction()?;
        get(&tx, operation)
    }
    pub fn catalog_split_receipt(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<SplitSnapshot>> {
        let tx = self.connection.unchecked_transaction()?;
        if !available(&tx)? {
            return Ok(None);
        }
        let operation: Option<String> = tx.query_row("SELECT operation_id FROM index_split_operations WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|row.get(0)).optional()?;
        operation
            .map(|value| {
                get(&tx, value.parse()?)
                    .and_then(|value| value.context("split receipt disappeared"))
            })
            .transpose()
    }
    pub fn catalog_split_state(&self, operation: Incarnation) -> Result<NativeSplitState> {
        Ok(NativeSplitState(
            self.catalog_split_status(operation)?
                .context("split operation not retained")?,
        ))
    }
    /// Save the exact operation before creating a destination shell or funding
    /// either owner. Accepted never authorizes source capture or build work.
    pub fn catalog_reserve_split(
        &mut self,
        request: &SplitRequest,
        baseline: &SourcePolicy,
        plan: &SplitPolicyPlan,
    ) -> Result<SplitSnapshot> {
        self.catalog_reserve_split_owned(request, baseline, plan, None)
    }
    pub fn catalog_reserve_split_with_extraction(
        &mut self,
        request: &SplitRequest,
        baseline: &SourcePolicy,
        plan: &SplitPolicyPlan,
        captured: &crate::service::collection::CapturedSplitExtractionPolicy,
    ) -> Result<SplitSnapshot> {
        self.catalog_reserve_split_owned(request, baseline, plan, Some(captured))
    }
    fn catalog_reserve_split_owned(
        &mut self,
        request: &SplitRequest,
        baseline: &SourcePolicy,
        plan: &SplitPolicyPlan,
        captured: Option<&crate::service::collection::CapturedSplitExtractionPolicy>,
    ) -> Result<SplitSnapshot> {
        request.validate_plan(baseline, plan)?;
        if let Some(saved) = self.catalog_split_receipt(request.caller, &request.key)? {
            ensure!(
                saved.request == *request && saved.baseline == *baseline && saved.plan == *plan,
                "split request key conflict"
            );
            return Ok(saved);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::validate_existing(&tx)?;
        if available(&tx)?
            && let Some(saved) = get(&tx, request.operation_id()?)?
        {
            ensure!(
                saved.request == *request && saved.baseline == *baseline && saved.plan == *plan,
                "split request key conflict"
            );
            return Ok(saved);
        }
        let inherited = match (&request.extraction_policy, captured) {
            (None, None) => None,
            (Some(_), Some(captured)) => Some(captured.validate_for(request)?),
            _ => anyhow::bail!("split retained policy requires exact native capture"),
        };
        let metadata = native_metadata_capacity(request, baseline, plan)?;
        ensure!(
            request.capacity.cleanup_allowance >= metadata.total,
            "split cleanup/preparation capacity is below native minimum {} bytes ({} per participant); document workload capacity remains separately reviewed",
            metadata.total,
            metadata.per_participant
        );
        key_current(&tx, &request.key)?;
        require_unused_action_key(&tx, request.caller, &request.key, None)?;
        let source = source_state(
            &tx,
            request.original_index,
            request.expected_catalog_revision,
        )?;
        ensure!(
            source.index.backing == request.original_registration
                && source.index.definition_revision == request.expected_definition_revision
                && source.index.source_revision == request.expected_source_revision
                && source.policy == *baseline
                && source.baseline == request.baseline
                && source.source_configuration == request.source_configuration
                && source.watch_digest == request.original_watch_digest,
            "split draft source state changed"
        );
        projects_current(&tx, request)?;
        let pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND stage<>'metadata_applied')",[],|row|row.get(0))?;
        ensure!(!pending, "split waits for existing native catalog fence");
        if inherited.is_some() {
            super::apply_split_extraction_v3_layout(&tx)?;
        } else {
            super::apply_split_v2_layout(&tx)?;
        }
        let active: i64 = tx.query_row(
            "SELECT count(DISTINCT operation_id) FROM index_split_participants WHERE active=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            active < MAX_ACTIVE,
            "active split admission capacity exceeded"
        );
        for id in [request.original_index, request.destination.index_id] {
            let busy: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM index_split_participants WHERE index_id=?1 AND active=1)",[id.to_string()],|row|row.get(0))?;
            ensure!(!busy, "index already participates in active split");
        }
        let occupied: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions WHERE id=?1 OR legacy_binding=?2 OR (data_root=?3 AND collection=?4))",params![request.destination.index_id.to_string(),request.destination_registration.binding.to_string(),request.destination_registration.data_root,request.destination_registration.collection],|row|row.get(0))?;
        ensure!(!occupied, "split destination already has owned backing");
        let receipt = SplitReceipt {
            schema: 1,
            operation_id: request.operation_id()?,
            request_digest: request.identity()?,
            revision: CatalogRevision::INITIAL,
            previous_receipt_digest: None,
            catalog_revision: request.expected_catalog_revision,
            stage: SplitStage::Accepted,
            state: SplitRunState::Pending,
            cleanup_attention: None,
            terminal_publication_slots: BTreeMap::new(),
            capacity_receipt: None,
            funding_receipts: BTreeMap::new(),
            destination_storage: None,
            destination_publication: None,
            destination_validation: None,
            remainder_witness: None,
            original_fence: None,
            remainder_publication: None,
            membership_receipt: None,
            physical_cleanup_authorization: None,
            funding_release_receipts: BTreeMap::new(),
            cleanup_receipt: None,
            cancellation_cleanup_receipt: None,
            jobs: vec![],
        };
        let value = SplitSnapshot {
            request: request.clone(),
            baseline: baseline.clone(),
            plan: plan.clone(),
            receipt_digest: receipt.identity(request)?,
            receipt,
            finalization: None,
        };
        value.validate()?;
        tx.execute(
            "INSERT INTO index_split_operations VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9,zeroblob(?10))",
            params![
                request.operation_id()?.to_string(),
                request.caller.to_string(),
                request.key.namespace.to_string(),
                i64::try_from(request.key.epoch)?,
                request.key.nonce.to_string(),
                request.original_index.to_string(),
                request.destination.index_id.to_string(),
                value.receipt.identity(request)?.to_string(),
                serde_json::to_vec(&value)?,
                i64::try_from(RESERVED_COMPLETION_BYTES)?
            ],
        )?;
        if let Some(body) = inherited {
            tx.execute(
                "INSERT INTO index_split_extraction_policies VALUES(?1,?2)",
                params![request.operation_id()?.to_string(), body],
            )?;
        }
        for (id, role) in [
            (request.original_index, "original_remainder"),
            (request.destination.index_id, "destination"),
        ] {
            tx.execute("INSERT INTO index_split_participants VALUES(?1,?2,?3,1) ON CONFLICT(index_id) DO UPDATE SET operation_id=excluded.operation_id,role=excluded.role,active=1 WHERE active=0",params![id.to_string(),request.operation_id()?.to_string(),role])?;
        }
        tx.execute(
            "INSERT INTO index_split_history VALUES(?1,1,?2,?3)",
            params![
                request.operation_id()?.to_string(),
                value.receipt.identity(request)?.to_string(),
                serde_json::to_vec(&value.receipt)?
            ],
        )?;
        tx.commit()?;
        Ok(value)
    }
}

pub(in crate::service::registry) fn require_destination_registration(
    db: &Connection,
    registration: &ProjectRegistration,
    state: &NativeSplitState,
) -> Result<()> {
    let value = get(db, state.0.request.operation_id()?)?.context("split reservation missing")?;
    ensure!(
        value == state.0
            && value.receipt.stage == SplitStage::Accepted
            && value.request.destination_registration == *registration,
        "split registration lacks exact accepted reservation"
    );
    Ok(())
}

#[cfg(test)]
pub(in crate::service::registry::catalog) mod tests;
// This type has no Deserialize implementation: only protected owned storage
// preparation can construct the proof used by the native registry.
pub struct NativeSplitPreparation {
    operation: Incarnation,
    expected: Sha256Digest,
    storage: Sha256Digest,
}
impl NativeSplitPreparation {
    pub(crate) fn new(state: &NativeSplitState, storage: Sha256Digest) -> Self {
        Self {
            operation: state.0.receipt.operation_id,
            expected: state
                .0
                .receipt
                .identity(&state.0.request)
                .expect("validated split snapshot"),
            storage,
        }
    }
}

impl ServiceRegistry {
    /// Reserve only native registration/source rows, leaving all source rows
    /// disabled and every normal/legacy grant route unavailable.
    pub fn catalog_reserve_split_destination(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        ensure!(
            old.receipt.stage == SplitStage::Accepted,
            "split destination reservation stage changed"
        );
        let registration = &old.request.destination_registration;
        let present: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_definitions WHERE id=?1)",
            [old.request.destination.index_id.to_string()],
            |row| row.get(0),
        )?;
        if present {
            let index = index_record(&tx, old.request.destination.index_id)?;
            ensure!(
                index.backing == *registration
                    && index.source_import == SourceImportState::PendingMetadata
                    && !index.tombstone,
                "split destination reservation differs"
            );
            return Ok(old);
        }
        super::super::source_isolation::validate(
            &tx,
            &self.root,
            registration,
            &old.request
                .source_configuration
                .aliases
                .values()
                .cloned()
                .collect::<Vec<_>>(),
        )?;
        super::super::register_project_in(
            &tx,
            &self.root,
            registration,
            None,
            false,
            super::super::RegistrationAdmission::Split(&NativeSplitState(old.clone())),
        )?;
        tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?6,1,1,0,NULL,0,'pending_metadata')",params![old.request.destination.index_id.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection,old.request.destination_name])?;
        for source in &old.plan.destination.roots {
            tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,?4,0)",
                params![
                    source.id.to_string(),
                    old.request.destination.index_id.to_string(),
                    source.canonical_path,
                    source.owner
                ],
            )?;
            for alias in &source.aliases {
                tx.execute(
                    "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
                    params![
                        old.request.destination.index_id.to_string(),
                        source.id.to_string(),
                        alias
                    ],
                )?;
            }
        }
        let revision = super::state(&tx)?.revision.next()?;
        tx.execute(
            "UPDATE index_catalog_meta SET revision=?1 WHERE id=1",
            [number(revision)],
        )?;
        let mut next = old.receipt.clone();
        next.catalog_revision = revision;
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_record_split_prepared(
        &mut self,
        proof: NativeSplitPreparation,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, proof.operation, proof.expected)?;
        ensure!(
            old.receipt.stage == SplitStage::Accepted,
            "split metadata preparation stage changed"
        );
        let index = index_record(&tx, old.request.destination.index_id)?;
        ensure!(
            index.backing == old.request.destination_registration
                && index.source_import == SourceImportState::PendingMetadata
                && !index.tombstone,
            "split reserved destination differs"
        );
        let mut next = old.receipt.clone();
        next.destination_storage = Some(proof.storage);
        if next.capacity_receipt.is_some() {
            next.stage = SplitStage::DestinationPrepared;
        }
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    /// Record one ACTUAL allocation. Partial cross-root funding is durable and
    /// resumable under Accepted; it never grants build/capture admission.
    pub fn catalog_record_split_funding(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        receipt: &crate::quota::funding::FundingReceipt,
    ) -> Result<SplitSnapshot> {
        let observed = self
            .catalog_split_status(operation)?
            .context("split operation not retained")?;
        receipt.validate_for(&observed.request)?;
        crate::quota::Quota::open_service(&receipt.data_root)?.validate_funding(receipt)?;
        let root = receipt
            .data_root
            .to_str()
            .context("split funding owner must be UTF-8")?
            .to_owned();
        let identity = receipt.identity()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        ensure!(
            old.receipt.stage == SplitStage::Accepted,
            "split funding admission stage changed"
        );
        let roots = [
            old.request.original_registration.data_root.clone(),
            old.request.destination_registration.data_root.clone(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        ensure!(
            roots.contains(&root),
            "split funding belongs to another quota root"
        );
        if let Some(previous) = old.receipt.funding_receipts.get(&root) {
            ensure!(*previous == identity, "split funding identity changed");
            return Ok(old);
        }
        let mut next = old.receipt.clone();
        next.funding_receipts.insert(root, identity);
        if next.funding_receipts.len() == roots.len() {
            next.capacity_receipt = Some(framed_digest(
                b"airk/split-capacity/1",
                &[&serde_json::to_vec(&next.funding_receipts)?],
            ));
            if next.destination_storage.is_some() {
                next.stage = SplitStage::DestinationPrepared;
            }
        }
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    /// This is the only public producer of SplitWork jobs. Ordinary Configure
    /// admission rejects this compact reference, and each claim rechecks it.
    pub fn enqueue_split_job(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        caller: Incarnation,
        key: &RequestKey,
        purpose: SplitJobPurpose,
        work: &ConfigurationWork,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = enqueue_split_job_in(&tx, operation, expected, caller, key, purpose, work)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn claimed_split_work(
        &self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        engine: Sha256Digest,
    ) -> Result<ClaimedSplitWork> {
        let tx = self.connection.unchecked_transaction()?;
        let (state, reference) =
            admitted_job(&tx, job)?.context("job has no admitted split operation")?;
        let epoch = super::super::jobs::worker_epoch(&tx, &self.root, lease)?;
        let record = super::super::jobs::record(&tx, job)?;
        ensure!(
            record.project == lease.identity().0
                && record.state == JobState::Running
                && record.fencing_generation == epoch
                && reference.engine_identity == engine,
            "split worker claim differs"
        );
        let mut permit = crate::service::collection::SplitBuildPermit::claimed(
            &NativeSplitState(state.clone()),
            &reference,
            lease,
        )?;
        let position = state
            .receipt
            .jobs
            .iter()
            .position(|job| job == &reference)
            .context("split claimed job position missing")?;
        if let Some(previous) = state.receipt.jobs[..position]
            .iter()
            .rev()
            .find(|job| job.purpose == reference.purpose)
        {
            let record = super::super::jobs::record(&tx, previous.job)?;
            ensure!(
                matches!(record.state, JobState::Failed | JobState::Canceled)
                    || reference.purpose == SplitJobPurpose::OriginalWitness
                        && record.state == JobState::Succeeded,
                "split retry predecessor is not terminal/reviewable"
            );
            let mut before = super::super::configuration::work(&tx, previous.job)?
                .context("split predecessor work not retained")?;
            let after = super::super::configuration::work(&tx, reference.job)?
                .context("split claimed work not retained")?;
            if let (
                ProjectConfiguration::SplitWork {
                    expected_receipt_digest: before_digest,
                    ..
                },
                ProjectConfiguration::SplitWork {
                    expected_receipt_digest: after_digest,
                    ..
                },
            ) = (
                &mut before.request.configuration,
                &after.request.configuration,
            ) {
                *before_digest = *after_digest;
            }
            ensure!(
                before == after && previous.engine_identity == reference.engine_identity,
                "split claimed retry reinterpreted saved work"
            );
            if reference.purpose != SplitJobPurpose::OriginalWitness {
                permit = permit.with_native_retry_predecessor(previous.clone())?;
            }
        }
        let extraction_policy = if reference.purpose == SplitJobPurpose::DestinationBuild
            && state.request.extraction_policy.is_some()
        {
            let body: Vec<u8> = tx.query_row(
                "SELECT substr(body,1,131073) FROM index_split_extraction_policies WHERE operation_id=?1",
                [state.receipt.operation_id.to_string()],
                |row| row.get(0),
            )?;
            Some(
                crate::service::collection::SavedSplitExtractionPolicy::from_record(
                    &state.request,
                    body,
                )?,
            )
        } else {
            None
        };
        Ok(ClaimedSplitWork {
            snapshot: state,
            job: reference,
            permit,
            extraction_policy,
        })
    }
}

pub struct ClaimedSplitWork {
    pub extraction_policy: Option<crate::service::collection::SavedSplitExtractionPolicy>,
    pub snapshot: SplitSnapshot,
    pub job: SplitJobRef,
    pub permit: crate::service::collection::SplitBuildPermit,
}
fn admitted_job(
    db: &Connection,
    job: Sha256Digest,
) -> Result<Option<(SplitSnapshot, SplitJobRef)>> {
    if !available(db)? {
        return Ok(None);
    }
    let row: Option<(String, Vec<u8>)> = db
        .query_row(
            "SELECT operation_id,substr(payload,1,2049) FROM index_split_jobs WHERE job=?1",
            [job.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((operation, bytes)) = row else {
        return Ok(None);
    };
    ensure!(bytes.len() <= 2048, "split job reference exceeds bound");
    let reference: SplitJobRef = serde_json::from_slice(&bytes)?;
    let state = get(db, operation.parse()?)?.context("split job lost admitted operation")?;
    ensure!(
        reference.job == job && state.receipt.jobs.contains(&reference),
        "split job reference differs"
    );
    let work = super::super::configuration::work(db, job)?
        .context("split configuration is not retained")?;
    ensure!(
        work.engine == reference.engine_identity
            && framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?])
                == reference.configuration_digest,
        "split immutable work lineage differs"
    );
    match &work.request.configuration {
        ProjectConfiguration::SplitWork {
            operation_id,
            request_digest,
            expected_receipt_digest,
            purpose,
            require_semantic_ready,
        } => {
            ensure!(
                *operation_id == state.receipt.operation_id
                    && *request_digest == state.receipt.request_digest
                    && *purpose == reference.purpose
                    && *require_semantic_ready
                        == (*purpose == SplitJobPurpose::DestinationBuild
                            && state.request.require_semantic_ready),
                "split claim configuration identity differs"
            );
            let prior:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_split_history WHERE operation_id=?1 AND digest=?2)",params![operation_id.to_string(),expected_receipt_digest.to_string()],|row|row.get(0))?;
            ensure!(prior, "split job lacks exact accepted phase receipt");
        }
        ProjectConfiguration::ApplySourcePolicy { policy, .. } => {
            ensure!(
                reference.purpose == SplitJobPurpose::OriginalRemoval
                    && **policy == state.plan.remainder,
                "split original removal policy differs"
            );
            ensure!(
                super::policy::require_claim(db, job)?,
                "split original policy claim unavailable"
            );
        }
        _ => anyhow::bail!("split job cannot reinterpret ordinary configuration"),
    }
    let allowed = match reference.purpose {
        SplitJobPurpose::DestinationBuild => {
            state.receipt.stage == SplitStage::Building
                && matches!(
                    state.receipt.state,
                    SplitRunState::Pending | SplitRunState::Running
                )
        }
        SplitJobPurpose::OriginalWitness => {
            state.receipt.stage == SplitStage::DestinationValidated
                && !matches!(
                    state.receipt.state,
                    SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
                )
        }
        SplitJobPurpose::OriginalRemoval => state.receipt.stage == SplitStage::OriginalFenced,
        SplitJobPurpose::DestinationCleanup => matches!(
            state.receipt.state,
            SplitRunState::CanceledBeforeFence {
                cleanup_pending: true
            }
        ),
        SplitJobPurpose::OriginalCleanup => {
            state.receipt.stage >= SplitStage::MembershipApplied
                && state.receipt.stage < SplitStage::Complete
        }
    };
    ensure!(allowed, "split job authority was canceled or superseded");
    Ok(Some((state, reference)))
}
pub(in crate::service::registry) fn require_claim(
    db: &Connection,
    job: Sha256Digest,
) -> Result<bool> {
    Ok(admitted_job(db, job)?.is_some())
}
pub(in crate::service::registry) fn validate_result(
    db: &Connection,
    job: Sha256Digest,
    result: &serde_json::Value,
) -> Result<()> {
    let Some((state, reference)) = admitted_job(db, job)? else {
        return Ok(());
    };
    if reference.purpose == SplitJobPurpose::OriginalRemoval {
        return Ok(());
    }
    let result: SplitJobResult = serde_json::from_value(result.clone())?;
    result.validate_for(&state.request, &state.plan)?;
    ensure!(
        result.purpose == reference.purpose,
        "split job result purpose differs"
    );
    if reference.purpose == SplitJobPurpose::OriginalWitness {
        ensure!(
            result
                .remainder_witness
                .as_ref()
                .is_some_and(|witness| witness.job == Some(job)),
            "split witness result accepted job differs"
        );
    }
    Ok(())
}
/// Complete result qualified by the actual native collection reader. No serde
/// constructor exists; public control payloads cannot assert this proof.
pub struct NativeSplitJobResult {
    operation: Incarnation,
    job: Sha256Digest,
    result: SplitJobResult,
}
impl NativeSplitJobResult {
    pub fn from_current(
        work: &ClaimedSplitWork,
        store: &crate::FileStore,
        mut result: SplitJobResult,
    ) -> Result<Self> {
        result.validate_for(&work.snapshot.request, &work.snapshot.plan)?;
        ensure!(
            result.purpose != SplitJobPurpose::OriginalWitness,
            "source witness requires its opaque live-source proof"
        );
        ensure!(
            result.purpose == work.job.purpose,
            "native split result purpose differs"
        );
        work.permit.validate_store(store)?;
        let registration = match work.job.purpose {
            SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup => {
                &work.snapshot.request.destination_registration
            }
            _ => &work.snapshot.request.original_registration,
        };
        let scope = store
            .current_scope(&registration.root, &registration.collection)?
            .context("split result has no native current publication")?;
        let read = store.current_read(&scope)?;
        let token = read.token()?;
        let publication = airk_core::identity::RevisionToken {
            incarnation: token.incarnation,
            generation: token.sequence,
            revision: token.digest,
        };
        let state = read
            .collection_state()?
            .context("split result lacks current collection state")?;
        ensure!(
            result.publication == Some(publication)
                && state.configuration.source_policy_digest == Some(result.policy_digest),
            "split result publication or policy differs from native current state"
        );
        if result.purpose == SplitJobPurpose::DestinationBuild {
            ensure!(
                state.counts.files > 0 && result.selected_sources == state.counts.files,
                "split destination native evidence is empty or unaccounted"
            );
        }
        let capsule = crate::current::policy_capsule::PolicyCapsule::load(
            store.managed_collection_directory()?,
        )?
        .context("split result lacks retained policy capsule")?;
        ensure!(
            capsule.policy.identity()? == result.policy_digest,
            "split result retained policy differs"
        );
        result.policy_application = capsule.application;
        if result.cleanup_complete {
            ensure!(
                result
                    .policy_application
                    .as_ref()
                    .is_some_and(|report| !report.physical_cleanup_pending),
                "split cleanup has pending retained policy work"
            );
        }
        Ok(Self {
            operation: work.snapshot.receipt.operation_id,
            job: work.job.job,
            result,
        })
    }
    pub fn from_witness(
        work: &ClaimedSplitWork,
        proof: NativeSplitRemainderWitness,
    ) -> Result<Self> {
        ensure!(
            work.job.purpose == SplitJobPurpose::OriginalWitness
                && proof.operation == work.snapshot.receipt.operation_id
                && proof.expected == work.snapshot.receipt_digest
                && proof.witness.job == Some(work.job.job),
            "split witness proof crossed accepted job/operation"
        );
        let result = SplitJobResult {
            schema: 1,
            operation_id: proof.operation,
            request_digest: work.snapshot.receipt.request_digest,
            purpose: SplitJobPurpose::OriginalWitness,
            publication: Some(proof.witness.publication),
            policy_digest: work.snapshot.request.before_policy_digest,
            selected_sources: 1,
            actual_bytes: Some(proof.witness.bytes_read),
            semantic_ready: false,
            cleanup_complete: false,
            policy_application: None,
            remainder_witness: Some(proof.witness),
        };
        result.validate_for(&work.snapshot.request, &work.snapshot.plan)?;
        Ok(Self {
            operation: proof.operation,
            job: work.job.job,
            result,
        })
    }
    pub fn result(&self) -> &SplitJobResult {
        &self.result
    }
}
impl ServiceRegistry {
    pub fn record_split_job_result(
        &mut self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        proof: NativeSplitJobResult,
    ) -> Result<()> {
        ensure!(proof.job == job, "native split result job differs");
        let claimed = self.claimed_split_work(
            lease,
            job,
            self.configuration_work(job)?
                .context("split configuration missing")?
                .engine,
        )?;
        ensure!(
            claimed.snapshot.receipt.operation_id == proof.operation
                && claimed.job.purpose == proof.result.purpose,
            "native split result operation differs"
        );
        self.record_configuration_result_inner(
            lease,
            job,
            &serde_json::to_value(&proof.result)?,
            true,
        )
    }
}

/// Pre-terminal authority comes from persisted physical cleanup, not a Core
/// caller assertion or an already Complete receipt that still holds resources.
#[derive(Clone, Debug)]
pub struct FundingTerminalPermit {
    snapshot: SplitSnapshot,
}
impl FundingTerminalPermit {
    pub(crate) fn validate_acknowledgment_root(
        &self,
        root: &Path,
        operation: Incarnation,
    ) -> Result<Incarnation> {
        let value = &self.snapshot;
        value.validate()?;
        ensure!(
            value.receipt.operation_id == operation
                && (value.receipt.stage == SplitStage::Complete
                    || matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: false
                        }
                    )),
            "funding acknowledgment requires exact actual terminal receipt"
        );
        let root = root.to_str().context("funding acknowledgment root UTF-8")?;
        ensure!(
            value.receipt.funding_receipts.contains_key(root)
                && value.receipt.funding_release_receipts.contains_key(root),
            "funding acknowledgment root lacks actual saved release"
        );
        Ok(value.request.key.namespace)
    }
    pub(crate) fn validate(&self, receipt: &crate::quota::funding::FundingReceipt) -> Result<()> {
        let value = &self.snapshot;
        value.validate()?;
        ensure!(
            value.receipt.physical_cleanup_authorization.is_some()
                && (value.receipt.stage >= SplitStage::CleanupPending
                    || matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence { .. }
                    )),
            "split physical cleanup has not authorized funding release"
        );
        receipt.validate_for(&value.request)?;
        let root = receipt
            .data_root
            .to_str()
            .context("split funding root must be UTF-8")?;
        ensure!(
            value.receipt.funding_receipts.get(root) == Some(&receipt.identity()?),
            "split terminal funding owner differs"
        );
        Ok(())
    }
    pub(crate) fn validate_acknowledged(
        &self,
        receipt: &crate::quota::funding::FundingReceipt,
    ) -> Result<()> {
        self.validate(receipt)?;
        ensure!(
            matches!(
                self.snapshot.receipt.state,
                SplitRunState::Complete
                    | SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
            ) && self.snapshot.receipt.funding_release_receipts.get(
                receipt
                    .data_root
                    .to_str()
                    .context("split funding root UTF-8")?
            ) == Some(&receipt.release_identity()?),
            "split funding release is not durably acknowledged"
        );
        Ok(())
    }
}
impl ServiceRegistry {
    pub fn catalog_split_funding_terminal(
        &self,
        operation: Incarnation,
    ) -> Result<FundingTerminalPermit> {
        let snapshot = self
            .catalog_split_status(operation)?
            .context("split operation not retained")?;
        ensure!(
            snapshot.receipt.physical_cleanup_authorization.is_some()
                && (snapshot.receipt.stage >= SplitStage::CleanupPending
                    || matches!(
                        snapshot.receipt.state,
                        SplitRunState::CanceledBeforeFence { .. }
                    )),
            "split physical cleanup authorization missing"
        );
        Ok(FundingTerminalPermit { snapshot })
    }
    pub fn catalog_record_split_funding_release(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        receipt: &crate::quota::funding::FundingReceipt,
    ) -> Result<SplitSnapshot> {
        let observed = self
            .catalog_split_status(operation)?
            .context("split operation not retained")?;
        let permit = FundingTerminalPermit { snapshot: observed };
        permit.validate(receipt)?;
        ensure!(
            receipt.complete,
            "split funding owner has not completed native release"
        );
        crate::quota::Quota::open_service(&receipt.data_root)?.validate_funding(receipt)?;
        let root = receipt
            .data_root
            .to_str()
            .context("split funding root UTF-8")?
            .to_owned();
        let digest = receipt.release_identity()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        if let Some(previous) = old.receipt.funding_release_receipts.get(&root) {
            ensure!(*previous == digest, "split release proof changed");
            return Ok(old);
        }
        let mut next = old.receipt.clone();
        next.funding_release_receipts.insert(root, digest);
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
}
fn fence_parts(
    db: &Connection,
    request: &CatalogRequest,
) -> Result<airk_core::catalog_fences::FencedReceipt> {
    let row:Option<(String,Vec<u8>)>=db.query_row("SELECT stage,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management_fenced'",[airk_core::catalog_management::catalog_operation_id(request.caller,&request.key)?.to_string()],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let (stage, bytes) = row.context("split original cutover lacks native fence")?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "split native fence exceeds bound"
    );
    let value = if stage == "metadata_applied" {
        serde_json::from_slice::<airk_core::catalog_fences::FencedReceipt>(&bytes)?
    } else {
        let intent: airk_core::catalog_fences::FenceIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        ensure!(intent.request == *request, "split pending fence differs");
        airk_core::catalog_fences::FencedReceipt {
            request: request.clone(),
            metadata: airk_core::catalog_management::CatalogReceipt {
                schema: 1,
                operation_id: intent.operation_id,
                caller: request.caller,
                key: request.key.clone(),
                request_digest: request.identity()?,
                catalog_revision: request.expected_catalog_revision.next()?,
                authority_fence_required: true,
            },
            preview_digest: intent.preview_digest,
            prepared_digest: intent.prepared_digest,
            proof: airk_core::catalog_fences::CatalogFenceProof::from_intent(&intent)?,
        }
    };
    value.validate()?;
    ensure!(value.request == *request, "split fenced request differs");
    Ok(value)
}
fn original_fenced_snapshot(
    db: &Connection,
    old: &SplitSnapshot,
    request: &CatalogRequest,
    job: Sha256Digest,
) -> Result<SplitSnapshot> {
    let fence = fence_parts(db, request)?;
    let CatalogOperation::ChangeIndexPolicy {
        configuration,
        engine_identity,
        change:
            PolicyChange::SplitRemainder {
                split_operation,
                expected_split_receipt,
            },
        ..
    } = &request.operation
    else {
        anyhow::bail!("split native cutover request required")
    };
    ensure!(
        *split_operation == old.receipt.operation_id
            && *expected_split_receipt == old.receipt.identity(&old.request)?,
        "split cutover receipt differs"
    );
    let work = ConfigurationWork {
        engine: *engine_identity,
        request: configuration.as_ref().clone(),
    };
    let reference = SplitJobRef {
        purpose: SplitJobPurpose::OriginalRemoval,
        caller: request.caller,
        key: request.key.clone(),
        backing: old.request.original_registration.binding,
        job,
        engine_identity: *engine_identity,
        configuration_digest: framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?]),
    };
    let mut next = old.receipt.clone();
    next.stage = SplitStage::OriginalFenced;
    next.state = SplitRunState::Pending;
    next.original_fence = Some(framed_digest(
        b"airk/split-original-fence/1",
        &[
            old.receipt.operation_id.as_bytes(),
            old.receipt.request_digest.as_bytes(),
            job.as_bytes(),
            &serde_json::to_vec(&fence)?,
        ],
    ));
    next.jobs.push(reference);
    next.revision = old.receipt.revision.next()?;
    next.previous_receipt_digest = Some(old.receipt.identity(&old.request)?);
    next.catalog_revision = request.expected_catalog_revision.next()?;
    let value = SplitSnapshot {
        request: old.request.clone(),
        baseline: old.baseline.clone(),
        plan: old.plan.clone(),
        receipt_digest: next.identity(&old.request)?,
        receipt: next,
        finalization: None,
    };
    value.validate()?;
    Ok(value)
}
fn verify_fenced_original_configuration(
    store: &crate::FileStore,
    request: &SplitRequest,
    lease: &crate::service::ProjectLease,
) -> Result<()> {
    ensure!(
        lease.identity().0 == request.original_registration.binding
            && store.service_incarnation()? == Some(request.original_incarnation),
        "split original native owner/incarnation changed at fence"
    );
    ensure!(
        crate::service::collection::inspect_settings_digest(
            store,
            &request.original_registration.root
        )? == request.original_settings_digest,
        "split original settings changed before restrictive fence"
    );
    let roots =
        crate::roots::RootConfig::load(store.directory(), &request.original_registration.root)?;
    ensure!(
        SourceConfigurationSnapshot {
            primary_root: request.original_registration.root.clone(),
            aliases: roots.roots
        } == request.source_configuration,
        "split original source configuration changed before restrictive fence"
    );
    Ok(())
}

pub(super) fn reviewed_remainder(
    db: &Connection,
    index: IndexId,
    operation: Incarnation,
    expected: Sha256Digest,
) -> Result<SourcePolicy> {
    let value = exact(db, operation, expected)?;
    if let Some(reference) = value
        .receipt
        .jobs
        .iter()
        .rev()
        .find(|job| job.purpose == SplitJobPurpose::OriginalWitness)
    {
        let witness = value
            .receipt
            .remainder_witness
            .as_ref()
            .context("split latest witness result is pending")?;
        ensure!(
            witness.job == Some(reference.job)
                && super::super::jobs::record(db, reference.job)?.state == JobState::Succeeded,
            "split latest witness child is not successfully qualified"
        );
        let result: SplitJobResult = serde_json::from_value(
            super::super::configuration::result(db, reference.job)?
                .context("split witness result not retained")?,
        )?;
        ensure!(
            result.remainder_witness.as_ref() == Some(witness),
            "split latest witness proof differs from actual saved result"
        );
    }
    ensure!(
        value.request.original_index == index
            && value.receipt.stage == SplitStage::DestinationValidated
            && value.receipt.destination_validation.is_some()
            && value.receipt.capacity_receipt.is_some(),
        "split destination has not validated"
    );
    baseline_current(db, &value)?;
    Ok(value.plan.remainder)
}
pub(super) fn record_original_fence(
    db: &Connection,
    request: &CatalogRequest,
    operation: Incarnation,
    expected: Sha256Digest,
    job: Sha256Digest,
) -> Result<()> {
    let old = exact(db, operation, expected)?;
    let next = original_fenced_snapshot(db, &old, request, job)?;
    let reference = next
        .receipt
        .jobs
        .last()
        .context("split original removal job missing")?;
    db.execute(
        "INSERT INTO index_split_jobs VALUES(?1,?2,?3)",
        params![
            job.to_string(),
            operation.to_string(),
            serde_json::to_vec(reference)?
        ],
    )?;
    let saved = replace(db, &old, next.receipt)?;
    ensure!(
        saved.receipt_digest == next.receipt_digest,
        "split source owner seal and committed receipt differ"
    );
    Ok(())
}
impl ServiceRegistry {
    /// Runs only for the already durable, fully acknowledged native fence, and
    /// before starting its metadata transaction. Failure keeps cutover sealed;
    /// exact native reconciliation resumes it. Preview never calls this method.
    pub fn catalog_split_seal_pending_original(&self, request: &CatalogRequest) -> Result<()> {
        let CatalogOperation::ChangeIndexPolicy {
            change:
                PolicyChange::SplitRemainder {
                    split_operation,
                    expected_split_receipt,
                },
            ..
        } = &request.operation
        else {
            return Ok(());
        };
        let old = self
            .catalog_split_status(*split_operation)?
            .context("split operation not retained")?;
        if old.receipt.stage >= SplitStage::OriginalFenced {
            return Ok(());
        }
        ensure!(
            old.receipt_digest == *expected_split_receipt,
            "split cutover review stale"
        );
        let next = {
            let tx = self.connection.unchecked_transaction()?;
            baseline_current(&tx, &old)?;
            let job = airk_core::catalog_source_change::source_job_id(
                old.request.original_registration.binding,
                request.caller,
                &request.key,
            )?;
            original_fenced_snapshot(&tx, &old, request, job)?
        };
        let registration = &next.request.original_registration;
        // The live witness is dated, not an atomic filesystem promise. Fence
        // persistence must still name the exact original publication/hash.
        let witness = next
            .receipt
            .remainder_witness
            .as_ref()
            .context("split remainder witness unavailable")?;
        let collection = crate::service::collection::Collection::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        let reconciliation_lease =
            crate::service::ProjectLease::acquire(&self.root, registration.binding)?;
        let selected = match collection.select() {
            Ok(value) => value,
            Err(_) => {
                // An earlier exact attempt may already have published this
                // restrictive owner capsule before its SQL commit failed.
                let work = next
                    .receipt
                    .jobs
                    .last()
                    .context("split fenced work missing")?;
                let permit = crate::service::collection::SplitBuildPermit::claimed(
                    &NativeSplitState(next.clone()),
                    work,
                    &reconciliation_lease,
                )?;
                collection.with_split_access(permit)?.select()?
            }
        };
        verify_fenced_original_configuration(
            &selected.store,
            &next.request,
            &reconciliation_lease,
        )?;
        let scope = selected
            .store
            .current_scope(&registration.root, &registration.collection)?
            .context("split original publication disappeared")?;
        let read = selected.store.current_read(&scope)?;
        let token = read.token()?;
        ensure!(
            witness.publication
                == airk_core::identity::RevisionToken {
                    incarnation: token.incarnation,
                    generation: token.sequence,
                    revision: token.digest
                }
                && read
                    .artifact(&witness.logical_path)?
                    .is_some_and(
                        |artifact| artifact.content_hash == witness.expected_hash.to_string()
                    ),
            "split original publication changed since remainder verification"
        );
        drop(read);
        verify_fenced_original_configuration(
            &selected.store,
            &next.request,
            &reconciliation_lease,
        )?;
        drop(selected);
        let quota = crate::quota::Quota::open_service(Path::new(&registration.data_root))?;
        let funding = quota.observe_funding(*split_operation)?;
        funding.validate_for(&next.request)?;
        let job = next
            .receipt
            .jobs
            .last()
            .context("split cutover job missing")?;
        let phase = crate::quota::funding::FundingPhase {
            revision: next.receipt.revision.get(),
            purpose: SplitJobPurpose::OriginalRemoval,
            job: job.job,
        };
        let owner = if funding
            .participants
            .iter()
            .find(|participant| participant.index_id == next.request.original_index)
            .is_some_and(|participant| participant.phase.as_ref() == Some(&phase))
        {
            quota.adopt_funding_phase(&funding, next.request.original_index, &phase)?
        } else {
            quota.begin_funding_phase(&funding, next.request.original_index, phase)?
        };
        let quota = quota.with_funding(owner)?;
        crate::service::collection::Collection::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?
        .retain_split_state(&NativeSplitState(next), &quota)
    }
    /// A native completed builder report is retained separately from current
    /// policy observations, and cannot substitute a publication from another job.
    pub fn catalog_complete_split_job(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        job: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        // A worker can die after Succeeded but before its ordinary P9 policy
        // completion tail. Reconcile only this exact accepted result before the
        // split advertises RemainderPublished; never depend on another actor's
        // scan/timer ordering. The canonical P9 owner validates the real result.
        let policy_completion = {
            let tx = self.connection.unchecked_transaction()?;
            let old = exact(&tx, operation, expected)?;
            let (admitted, reference) =
                admitted_job(&tx, job)?.context("split job not admitted")?;
            ensure!(
                admitted.receipt_digest == expected,
                "split completion lifecycle changed"
            );
            if reference.purpose == SplitJobPurpose::OriginalRemoval {
                ensure!(
                    super::super::jobs::record(&tx, job)?.state == JobState::Succeeded,
                    "split job is not successfully complete"
                );
                let work = super::super::configuration::work(&tx, job)?
                    .context("split original policy configuration missing")?;
                let ProjectConfiguration::ApplySourcePolicy {
                    policy,
                    operation_id,
                } = work.request.configuration
                else {
                    anyhow::bail!("split original removal is not accepted policy work");
                };
                let current = super::policy::snapshot(&tx, old.request.original_index)?
                    .context("split original policy missing")?;
                ensure!(
                    current.job == job
                        && current.operation_id == operation_id
                        && current.policy == *policy
                        && current.policy == old.plan.remainder,
                    "split original policy completion lineage differs"
                );
                Some((old.request.original_index, operation_id))
            } else {
                None
            }
        };
        if let Some((index, policy_operation)) = policy_completion {
            self.catalog_record_policy_job(index, policy_operation)?;
        }
        // Recheck the exact receipt, native work lineage, and Succeeded result
        // under the existing write transaction after the P9 owner's commit.
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        let (admitted, reference) = admitted_job(&tx, job)?.context("split job not admitted")?;
        ensure!(
            admitted.receipt_digest == expected,
            "split completion lifecycle changed"
        );
        let record = super::super::jobs::record(&tx, job)?;
        ensure!(
            record.state == JobState::Succeeded,
            "split job is not successfully complete"
        );
        let raw = super::super::configuration::result(&tx, job)?
            .context("split native result not retained")?;
        let mut next = old.receipt.clone();
        match reference.purpose {
            SplitJobPurpose::DestinationBuild => {
                let result: SplitJobResult = serde_json::from_value(raw.clone())?;
                result.validate_for(&old.request, &old.plan)?;
                baseline_current(&tx, &old)?;
                let report = result
                    .policy_application
                    .clone()
                    .context("split destination lacks native policy attestation")?;
                report.validate()?;
                ensure!(
                    report.policy_digest == old.plan.destination.identity()?
                        && report.operation_id == operation,
                    "split destination policy attestation differs"
                );
                let policy = airk_core::catalog_policy::PolicySnapshot {
                    schema: 1,
                    operation_id: operation,
                    policy: old.plan.destination.clone(),
                    job,
                    application: Some(report),
                    application_job: Some(job),
                    last_error: None,
                };
                super::policy::write_snapshot(&tx, &policy)?;
                tx.execute("UPDATE index_definitions SET policy_epoch=?1,policy_digest=?2 WHERE id=?3 AND policy_epoch=0",params![number(policy.policy.epoch),policy.policy.identity()?.to_string(),old.request.destination.index_id.to_string()])?;
                next.stage = SplitStage::DestinationValidated;
                next.state = SplitRunState::Pending;
                next.destination_publication = result.publication;
                next.destination_validation = Some(framed_digest(
                    b"airk/split-destination-validation/1",
                    &[job.as_bytes(), &serde_json::to_vec(&result)?],
                ));
            }
            SplitJobPurpose::OriginalWitness => {
                let result: SplitJobResult = serde_json::from_value(raw.clone())?;
                result.validate_for(&old.request, &old.plan)?;
                baseline_current(&tx, &old)?;
                next.remainder_witness = result.remainder_witness;
                next.state = SplitRunState::Pending;
            }
            SplitJobPurpose::OriginalRemoval => {
                let report: airk_core::catalog_policy::SourcePolicyApplyReport =
                    serde_json::from_value(raw)?;
                report.validate()?;
                ensure!(
                    report.policy_digest == old.plan.remainder.identity()?
                        && report.removal_publication.is_some()
                        && !report.no_index,
                    "split remainder has no removal publication"
                );
                next.stage = SplitStage::RemainderPublished;
                next.remainder_publication = report.removal_publication;
                next.state = SplitRunState::Pending;
            }
            SplitJobPurpose::DestinationCleanup | SplitJobPurpose::OriginalCleanup => {
                let result: SplitJobResult = serde_json::from_value(raw.clone())?;
                result.validate_for(&old.request, &old.plan)?;
                ensure!(
                    result.cleanup_complete,
                    "split native physical cleanup is pending"
                );
                next.physical_cleanup_authorization = Some(framed_digest(
                    b"airk/split-physical-cleanup/1",
                    &[job.as_bytes(), &serde_json::to_vec(&result)?],
                ));
                if reference.purpose == SplitJobPurpose::OriginalCleanup {
                    next.stage = SplitStage::CleanupPending;
                    next.state = SplitRunState::Pending;
                }
            }
        }
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
}
/// Dated positive saved-source check qualified against an actual original
/// current reader. Contains no source text and has no Deserialize constructor.
pub struct NativeSplitRemainderWitness {
    operation: Incarnation,
    expected: Sha256Digest,
    witness: SplitRemainderWitness,
}
impl NativeSplitRemainderWitness {
    pub fn from_current(
        state: &NativeSplitState,
        store: &crate::FileStore,
        logical_path: &str,
        check: &airk_core::verification::SourceCheck,
        observed_from: u64,
        observed_to: u64,
    ) -> Result<Self> {
        use airk_core::verification::SourceCheckState;
        let value = &state.0;
        value.validate()?;
        ensure!(
            value.receipt.stage == SplitStage::DestinationValidated,
            "split remainder witness requires validated destination"
        );
        let registration = &value.request.original_registration;
        let managed = store.managed_path()?;
        ensure!(
            managed.root == Path::new(&registration.data_root)
                && managed.collection == registration.collection,
            "split witness backing differs"
        );
        let scope = store
            .current_scope(&registration.root, &registration.collection)?
            .context("split original has no current publication")?;
        let read = store.current_read(&scope)?;
        let token = read.token()?;
        let artifact = read
            .artifact(logical_path)?
            .context("split witness source not retained")?;
        let location = crate::roots::source_location(
            &registration.root,
            &value.request.source_configuration.aliases,
            logical_path,
        )?;
        let root = value
            .baseline
            .roots
            .iter()
            .find(|root| root.canonical_path == location.root)
            .context("split witness source binding differs")?;
        ensure!(
            value
                .plan
                .remainder
                .allows_canonical(root.id, &location.relative_path),
            "split witness is outside remainder"
        );
        let expected_hash: Sha256Digest = artifact.content_hash.parse()?;
        ensure!(
            check.state == SourceCheckState::Matched
                && check.observed_hash == Some(expected_hash)
                && check.bytes_read == artifact.bytes as u64
                && observed_from > 0
                && observed_to >= observed_from,
            "split witness lacks positive full-source verification"
        );
        let witness = SplitRemainderWitness {
            job: value
                .receipt
                .jobs
                .iter()
                .rev()
                .find(|job| job.purpose == SplitJobPurpose::OriginalWitness)
                .map(|job| job.job),
            index_id: value.request.original_index,
            before_policy_digest: value.request.before_policy_digest,
            publication: airk_core::identity::RevisionToken {
                incarnation: token.incarnation,
                generation: token.sequence,
                revision: token.digest,
            },
            source_root_id: root.id,
            logical_path: logical_path.into(),
            source_relative_path: location.relative_path,
            expected_hash,
            observed_from_unix_ms: observed_from,
            observed_to_unix_ms: observed_to,
            bytes_read: check.bytes_read,
        };
        Ok(Self {
            operation: value.receipt.operation_id,
            expected: value.receipt_digest,
            witness,
        })
    }
    pub fn observation(&self) -> &SplitRemainderWitness {
        &self.witness
    }
}
impl ServiceRegistry {
    pub fn catalog_record_split_remainder_witness(
        &mut self,
        lease: &crate::service::ProjectLease,
        proof: NativeSplitRemainderWitness,
    ) -> Result<SplitSnapshot> {
        super::super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let state = self.catalog_split_state(proof.operation)?;
        ensure!(
            lease.identity().0 == state.0.request.original_registration.binding,
            "split witness native owner differs"
        );
        self.catalog_record_split_admin_remainder_witness(proof)
    }
    /// Consumes an opaque actual Kernel/Store observation; the administrative
    /// reader needs no exclusive worker lease. Source/current handles are closed
    /// before this bounded registry transaction. Fence admission rechecks current.
    pub fn catalog_record_split_admin_remainder_witness(
        &mut self,
        proof: NativeSplitRemainderWitness,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, proof.operation, proof.expected)?;
        ensure!(
            old.receipt.stage == SplitStage::DestinationValidated,
            "split witness phase differs"
        );
        baseline_current(&tx, &old)?;
        let mut next = old.receipt.clone();
        next.remainder_witness = Some(proof.witness);
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
}
impl ServiceRegistry {
    /// The existing P9 index fence already revoked/drained original evidence
    /// authority. Destination memberships remain choices, never automatic grants.
    pub fn catalog_apply_split_memberships(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        ensure!(
            old.receipt.stage == SplitStage::RemainderPublished,
            "split remainder publication is pending"
        );
        projects_current(&tx, &old.request)?;
        let source = super::policy::snapshot(&tx, old.request.original_index)?
            .context("split original policy missing")?;
        ensure!(
            source.policy == old.plan.remainder
                && source.application.as_ref().is_some_and(
                    |report| report.removal_publication == old.receipt.remainder_publication
                ),
            "split original removal attestation differs"
        );
        let destination = super::policy::snapshot(&tx, old.request.destination.index_id)?
            .context("split destination policy missing")?;
        ensure!(
            destination.policy == old.plan.destination && destination.application.is_some(),
            "split destination is not validated"
        );
        tx.execute("UPDATE index_definitions SET source_import_state='complete',source_revision=source_revision+1 WHERE id=?1 AND source_import_state='pending_metadata' AND tombstone=0",[old.request.destination.index_id.to_string()])?;
        tx.execute(
            "UPDATE index_sources SET enabled=1 WHERE index_id=?1",
            [old.request.destination.index_id.to_string()],
        )?;
        for effect in &old.request.projects {
            let mut scope = effect.expected_scope_revision;
            if let Some(atoms) = &effect.destination_scope {
                let change = CatalogRequest {
                    caller: old.request.caller,
                    key: old.request.key.clone(),
                    expected_catalog_revision: super::state(&tx)?.revision,
                    operation: CatalogOperation::SetMembership {
                        membership: airk_core::index_catalog::Membership {
                            project_id: effect.project_id,
                            index_id: old.request.destination.index_id,
                            revision: CatalogRevision::INITIAL,
                            scope: atoms.clone(),
                        },
                        expected_scope_revision: scope,
                        expected_membership_revision: None,
                    },
                };
                super::operations::mutate(&tx, &change, &self.root)?;
                scope = scope.next()?;
            }
            if !effect.retain_original
                && let Some(revision) = effect.expected_original_membership
            {
                let change = CatalogRequest {
                    caller: old.request.caller,
                    key: old.request.key.clone(),
                    expected_catalog_revision: super::state(&tx)?.revision,
                    operation: CatalogOperation::RemoveMembership {
                        project_id: effect.project_id,
                        index_id: old.request.original_index,
                        expected_scope_revision: scope,
                        expected_membership_revision: revision,
                    },
                };
                super::operations::mutate(&tx, &change, &self.root)?;
            }
        }
        super::super::watch::ensure_row(&tx, old.request.destination_registration.binding)?;
        tx.execute("UPDATE service_watch SET enabled=?1,configuration_epoch=configuration_epoch+1,backend=CASE WHEN ?1 THEN 'periodic_fallback' ELSE 'disabled' END WHERE project=?2 AND configuration_epoch=0",params![old.request.destination_watch_enabled,old.request.destination_registration.binding.to_string()])?;
        let revision = super::state(&tx)?.revision.next()?;
        tx.execute(
            "UPDATE index_catalog_meta SET revision=?1 WHERE id=1",
            [number(revision)],
        )?;
        let mut next = old.receipt.clone();
        next.stage = SplitStage::MembershipApplied;
        next.catalog_revision = revision;
        next.membership_receipt = Some(framed_digest(
            b"airk/split-reviewed-memberships/1",
            &[
                old.receipt.request_digest.as_bytes(),
                &serde_json::to_vec(&old.request.projects)?,
                &revision.get().to_le_bytes(),
            ],
        ));
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_authorize_split_cleanup(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        ensure!(
            old.receipt.stage == SplitStage::MembershipApplied,
            "split cutover is not complete"
        );
        let reference = old
            .receipt
            .jobs
            .iter()
            .rev()
            .find(|job| job.purpose == SplitJobPurpose::OriginalRemoval)
            .context("split removal job missing")?;
        let report: airk_core::catalog_policy::SourcePolicyApplyReport = serde_json::from_value(
            super::super::configuration::result(&tx, reference.job)?
                .context("split removal result not retained")?,
        )?;
        report.validate()?;
        ensure!(
            !report.physical_cleanup_pending
                && report.policy_digest == old.plan.remainder.identity()?,
            "split retained physical cleanup still pending"
        );
        let mut next = old.receipt.clone();
        next.stage = SplitStage::CleanupPending;
        next.physical_cleanup_authorization = Some(framed_digest(
            b"airk/split-physical-cleanup/1",
            &[reference.job.as_bytes(), &serde_json::to_vec(&report)?],
        ));
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_finish_split(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        // Compatibility adapter remains bounded; the metadata actor uses the
        // one-action advance API directly.
        let mut value = self
            .catalog_split_status(operation)?
            .context("split operation not retained")?;
        ensure!(
            value.receipt_digest == expected,
            "split finalization review stale"
        );
        for _ in 0..12 {
            value = self.catalog_advance_split_finalization(operation, value.receipt_digest)?;
            if (value.receipt.stage == SplitStage::Complete
                || matches!(
                    value.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
                ))
                && value
                    .finalization
                    .as_ref()
                    .is_some_and(|state| state.progress == SplitFinalizationProgress::Complete)
            {
                return Ok(value);
            }
        }
        anyhow::bail!("split terminal metadata continuation budget exhausted")
    }
    pub fn catalog_cancel_split(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = cancel_split_in(&tx, operation, expected)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_note_split_failure(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        code: &str,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, operation, expected)?;
        if let Some(value) = finalization::latch(&tx, &old, code)? {
            tx.commit()?;
            return Ok(value);
        }
        ensure!(
            old.receipt.stage != SplitStage::Complete
                && !matches!(
                    old.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
                ),
            "split terminal state cannot be rewritten as failed"
        );
        let mut next = old.receipt.clone();
        if matches!(
            old.receipt.state,
            SplitRunState::CanceledBeforeFence {
                cleanup_pending: true
            }
        ) {
            next.cleanup_attention = Some(code.into());
        } else {
            next.state = SplitRunState::NeedsAttention { code: code.into() };
        }
        let value = replace(&tx, &old, next)?;
        tx.commit()?;
        Ok(value)
    }
    /// Read accepted recovery independently of later current operation state.
    pub fn catalog_split_recovery_receipt(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<SplitRecoveryReceipt>> {
        let tx = self.connection.unchecked_transaction()?;
        if !available(&tx)? {
            return Ok(None);
        }
        let bytes:Option<Vec<u8>>=tx.query_row("SELECT substr(payload,1,16385) FROM index_split_recoveries WHERE caller=?1 AND namespace=?2 AND epoch=?3 AND nonce=?4",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|row.get(0)).optional()?;
        bytes.map(|bytes|{ensure!(bytes.len()<=MAX_RECEIPT_BYTES,"split recovery receipt exceeds bound");let value:SplitRecoveryReceipt=serde_json::from_slice(&bytes)?;
            ensure!(value.request.caller==caller && value.request.key==*key,"split recovery receipt identity differs");
            let current=get(&tx,value.request.operation_id)?.context("split recovery operation missing")?;
            let prior:Vec<u8>=tx.query_row("SELECT substr(payload,1,16385) FROM index_split_history WHERE operation_id=?1 AND digest=?2",params![value.request.operation_id.to_string(),value.request.expected_receipt_digest.to_string()],|row|row.get(0))?;
            let reviewed:SplitReceipt=serde_json::from_slice(&prior)?;value.validate_for(&current.request,&reviewed)?;Ok(value)}).transpose()
    }
    /// Only native owners can acknowledge effects already recorded by exact CAS.
    /// This method never resubmits the requested Resume/Cancel action.
    pub fn catalog_record_split_recovery(
        &mut self,
        receipt: &SplitRecoveryReceipt,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) =
            self.catalog_split_recovery_receipt(receipt.request.caller, &receipt.request.key)?
        {
            ensure!(
                saved.request == receipt.request,
                "split recovery request key conflict"
            );
            return Ok(saved);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current =
            get(&tx, receipt.request.operation_id)?.context("split operation not retained")?;
        key_current(&tx, &receipt.request.key)?;
        let prior:Vec<u8>=tx.query_row("SELECT substr(payload,1,16385) FROM index_split_history WHERE operation_id=?1 AND digest=?2",params![receipt.request.operation_id.to_string(),receipt.request.expected_receipt_digest.to_string()],|row|row.get(0))?;
        let reviewed: SplitReceipt = serde_json::from_slice(&prior)?;
        receipt.validate_for(&current.request, &reviewed)?;
        let accepted_job = match &receipt.outcome {
            SplitRecoveryOutcome::ResumeAccepted { accepted_job, .. } => Some(*accepted_job),
            SplitRecoveryOutcome::CancelBeforeFenceAccepted {
                resulting_receipt, ..
            } => resulting_receipt.jobs[reviewed.jobs.len()..]
                .first()
                .map(|work| work.job),
            _ => None,
        };
        require_unused_action_key(
            &tx,
            receipt.request.caller,
            &receipt.request.key,
            accepted_job,
        )?;
        match &receipt.outcome {
            SplitRecoveryOutcome::FinalizationAccepted { .. } => {
                anyhow::bail!("frozen finalization acknowledgment requires its atomic native owner")
            }
            SplitRecoveryOutcome::ResumeAccepted {
                resulting_receipt, ..
            }
            | SplitRecoveryOutcome::ContinuationAccepted {
                resulting_receipt, ..
            }
            | SplitRecoveryOutcome::CancelBeforeFenceAccepted {
                resulting_receipt, ..
            } => {
                let found:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM index_split_history WHERE operation_id=?1 AND digest=?2)",params![current.receipt.operation_id.to_string(),resulting_receipt.identity(&current.request)?.to_string()],|row|row.get(0))?;
                ensure!(
                    found,
                    "split recovery successor is not a native committed receipt"
                );
            }
            _ => (),
        }
        encoded_size(receipt, MAX_RECEIPT_BYTES)?;
        tx.execute(
            "INSERT INTO index_split_recoveries VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                receipt.request.caller.to_string(),
                receipt.request.key.namespace.to_string(),
                i64::try_from(receipt.request.key.epoch)?,
                receipt.request.key.nonce.to_string(),
                receipt.request.operation_id.to_string(),
                serde_json::to_vec(receipt)?
            ],
        )?;
        tx.commit()?;
        Ok(receipt.clone())
    }
}

// Generic retirement must not strand an admitted split's paid children or
// destroy either participant's recovery authority, including before cutover.
pub(super) fn require_retirement(db: &Connection, index: IndexId) -> Result<()> {
    if !available(db)? {
        return Ok(());
    }
    let active: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_split_participants WHERE index_id=?1 AND active=1)",
        [index.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        !active,
        "active split owns this participant; finish exact split recovery before retirement"
    );
    Ok(())
}

pub(super) fn require_policy_change(
    db: &Connection,
    index: IndexId,
    change: &PolicyChange,
) -> Result<()> {
    if !available(db)? || matches!(change, PolicyChange::SplitRemainder { .. }) {
        return Ok(());
    }
    let operation: Option<String> = db
        .query_row(
            "SELECT operation_id FROM index_split_participants WHERE index_id=?1 AND active=1",
            [index.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(operation) = operation {
        let state = get(db, operation.parse()?)?.context("active split journal missing")?;
        ensure!(
            state.receipt.stage < SplitStage::OriginalFenced
                && index == state.request.original_index,
            "active split cutover owns this policy; finish exact recovery first"
        );
    }
    Ok(())
}

impl ServiceRegistry {
    /// One bounded native destination purge turn. Retains stable capability and
    /// lifecycle metadata; source directories are never in this deletion scope.
    /// A completed proof is recorded only through the exact claimed cleanup job.
    pub fn cleanup_split_destination(
        &self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        engine: Sha256Digest,
        funding: Option<&crate::quota::funding::FundingUse>,
    ) -> Result<Option<NativeSplitJobResult>> {
        let work = self.claimed_split_work(lease, job, engine)?;
        ensure!(
            work.job.purpose == SplitJobPurpose::DestinationCleanup,
            "split destination cleanup job required"
        );
        let registration = &work.snapshot.request.destination_registration;
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        let store = crate::FileStore::new(&directory);
        work.permit.validate_store(&store)?;
        if let Some(funding) = funding {
            work.permit.validate_funding_use(funding)?;
        } else {
            ensure!(
                !work
                    .snapshot
                    .receipt
                    .funding_receipts
                    .contains_key(&registration.data_root),
                "funded split cleanup requires exact adopted funding use"
            );
        }
        if !crate::service::collection::Collection::purge_removed_turn_inner(
            registration,
            lease,
            Some(&work.permit),
            funding,
        )? {
            return Ok(None);
        }
        // The purge inventories the entire owned scope before any deletion and
        // returns true only after incarnation bytes and current grants retire.
        if let Some(funding) = funding {
            let receipt = crate::quota::Quota::open_service(&funding.data_root)?
                .observe_funding(work.snapshot.receipt.operation_id)?;
            receipt.validate_for(&work.snapshot.request)?;
            ensure!(
                work.snapshot
                    .receipt
                    .funding_receipts
                    .get(&registration.data_root)
                    == Some(&receipt.identity()?),
                "split cleanup funding owner differs"
            );
            let participant = receipt
                .participants
                .iter()
                .find(|participant| participant.index_id == funding.index_id)
                .context("split cleanup participant funding missing")?;
            ensure!(
                participant.phase.as_ref() == Some(&funding.phase)
                    && participant.settled_revision == Some(funding.phase.revision)
                    && receipt
                        .children
                        .iter()
                        .filter(|child| child.index_id == funding.index_id)
                        .all(|child| child.closed),
                "split destination funding disposal remains unsettled"
            );
        }
        let result = SplitJobResult {
            schema: 1,
            operation_id: work.snapshot.receipt.operation_id,
            request_digest: work.snapshot.receipt.request_digest,
            purpose: SplitJobPurpose::DestinationCleanup,
            publication: None,
            policy_digest: work.snapshot.plan.destination.identity()?,
            selected_sources: 0,
            actual_bytes: Some(0),
            semantic_ready: false,
            cleanup_complete: true,
            policy_application: None,
            remainder_witness: None,
        };
        result.validate_for(&work.snapshot.request, &work.snapshot.plan)?;
        Ok(Some(NativeSplitJobResult {
            operation: work.snapshot.receipt.operation_id,
            job,
            result,
        }))
    }
}

impl ServiceRegistry {
    /// Attach only an actually accepted P9 policy retry, preserving its exact
    /// saved engine/configuration and original fenced operation lineage.
    pub fn catalog_attach_split_policy_retry(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = attach_split_policy_retry_in(&tx, operation, expected, caller, key)?;
        tx.commit()?;
        Ok(value)
    }
}

/// Actual held-ancestor proof that an Accepted destination never acquired an
/// owned collection shell. This is intentionally neither serde nor Clone.
pub struct NativeSplitAbsentDestination {
    operation: Incarnation,
    request: Sha256Digest,
    expected: Sha256Digest,
    directory: std::path::PathBuf,
    ancestor_path: std::path::PathBuf,
    ancestor: super::setup::storage::SetupDirectory,
    missing: std::ffi::OsString,
}
impl NativeSplitAbsentDestination {
    pub fn capture(state: &NativeSplitState) -> Result<Self> {
        let value = &state.0;
        value.validate()?;
        ensure!(
            value.receipt.stage == SplitStage::Accepted
                && value.receipt.destination_storage.is_none()
                && value.receipt.jobs.is_empty()
                && matches!(
                    value.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ),
            "split absence requires canceled, never-prepared destination"
        );
        let directory = Path::new(&value.request.destination_registration.data_root)
            .join("collections-1.4")
            .join(&value.request.destination_registration.collection);
        let mut selected = None;
        for path in directory.ancestors() {
            match std::fs::symlink_metadata(path) {
                Ok(metadata) => {
                    ensure!(
                        path != directory
                            && metadata.is_dir()
                            && !metadata.file_type().is_symlink(),
                        "unexpected/unowned split destination exists; cleanup requires attention"
                    );
                    selected = Some(path.to_path_buf());
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        let ancestor_path =
            selected.context("split destination has no protected existing ancestor")?;
        let ancestor = super::setup::storage::SetupDirectory::open_canonical(&ancestor_path)?;
        let relative = directory.strip_prefix(&ancestor_path)?;
        let missing = match relative
            .components()
            .next()
            .context("split destination absence has no missing child")?
        {
            std::path::Component::Normal(name) => name.to_os_string(),
            _ => anyhow::bail!("split destination absence has invalid path"),
        };
        let proof = Self {
            operation: value.receipt.operation_id,
            request: value.receipt.request_digest,
            expected: value.receipt_digest,
            directory,
            ancestor_path,
            ancestor,
            missing,
        };
        proof.revalidate()?;
        Ok(proof)
    }
    fn revalidate(&self) -> Result<()> {
        ensure!(
            super::setup::storage::SetupDirectory::open_canonical(&self.ancestor_path)?
                .identity()?
                == self.ancestor.identity()?,
            "split absence ancestor changed"
        );
        match self.ancestor.open_dir(&self.missing) {
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
            Ok(_) => anyhow::bail!("split destination appeared; cleanup remains pending"),
        }
    }
}
impl ServiceRegistry {
    pub fn catalog_authorize_split_absent_cleanup(
        &mut self,
        lease: &crate::service::ProjectLease,
        proof: NativeSplitAbsentDestination,
    ) -> Result<SplitSnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, proof.operation, proof.expected)?;
        ensure!(
            lease.identity().0 == old.request.destination_registration.binding
                && lease.root == self.root
                && old.receipt.request_digest == proof.request
                && old.receipt.stage == SplitStage::Accepted
                && old.receipt.destination_storage.is_none()
                && old.receipt.jobs.is_empty()
                && matches!(
                    old.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ),
            "split absence cleanup authority differs"
        );
        ensure!(
            proof.directory
                == Path::new(&old.request.destination_registration.data_root)
                    .join("collections-1.4")
                    .join(&old.request.destination_registration.collection),
            "split absence destination differs"
        );
        proof.revalidate()?;
        let mut next = old.receipt.clone();
        next.physical_cleanup_authorization = Some(framed_digest(
            b"airk/split-never-prepared-cleanup/1",
            &[
                proof.request.as_bytes(),
                proof.expected.as_bytes(),
                &serde_json::to_vec(&(
                    proof.ancestor.identity()?,
                    proof
                        .directory
                        .to_str()
                        .context("split destination UTF-8")?,
                ))?,
            ],
        ));
        let value = replace(&tx, &old, next)?;
        proof.revalidate()?;
        tx.commit()?;
        Ok(value)
    }
}

impl ServiceRegistry {
    /// Atomically saves the exact explicit metadata resume acknowledgment with
    /// its successor. A lost reply never turns a later status into retry power.
    /// Read-only routing observation, never retry authority. Both native worker
    /// recovery owners recheck it before accepting an exact metadata continuation.
    pub fn catalog_split_resume_requires_continuation(
        &self,
        request: &SplitRecoveryRequest,
    ) -> Result<bool> {
        let tx = self.connection.unchecked_transaction()?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        ensure!(
            request.action == SplitRecoveryAction::Resume,
            "split Resume action required"
        );
        let purpose = match old.receipt.stage {
            SplitStage::OriginalFenced => SplitJobPurpose::OriginalRemoval,
            SplitStage::DestinationValidated => SplitJobPurpose::OriginalWitness,
            _ => return Ok(false),
        };
        let Some(reference) = old
            .receipt
            .jobs
            .iter()
            .rev()
            .find(|job| job.purpose == purpose)
        else {
            return Ok(false);
        };
        if super::super::jobs::record(&tx, reference.job)?.state != JobState::Succeeded {
            return Ok(false);
        }
        let (admitted, admitted_reference) = admitted_job(&tx, reference.job)?
            .context("split successful child authority not retained; outcome remains unknown")?;
        ensure!(
            admitted.receipt_digest == old.receipt_digest && admitted_reference == *reference,
            "split successful child lineage differs"
        );
        let raw = super::super::configuration::result(&tx, reference.job)?.context(
            "split successful child result proof is not retained; outcome remains unknown",
        )?;
        if purpose == SplitJobPurpose::OriginalRemoval {
            super::policy::validate_result(&tx, reference.job, &raw)?;
            let result: airk_core::catalog_policy::SourcePolicyApplyReport =
                serde_json::from_value(raw)?;
            ensure!(
                result.policy_digest == old.plan.remainder.identity()?
                    && result.removal_publication.is_some()
                    && !result.no_index,
                "split successful original removal result differs"
            );
            // Physical reclamation is a later OriginalCleanup purpose. Never
            // rerun a successful restrictive publication to repair bookkeeping.
            return Ok(true);
        }
        let result: SplitJobResult = serde_json::from_value(raw)?;
        result.validate_for(&old.request, &old.plan)?;
        let witness = result
            .remainder_witness
            .as_ref()
            .context("successful split witness result lacks its proof")?;
        ensure!(
            witness.job == Some(reference.job),
            "split successful witness job differs"
        );
        if old
            .receipt
            .remainder_witness
            .as_ref()
            .is_some_and(|saved| saved.job == Some(reference.job))
        {
            ensure!(
                old.receipt.remainder_witness == result.remainder_witness,
                "split retained witness differs from successful result"
            );
            // The exact witness was already consumed. Explicit Resume may
            // refresh it when cutover detects a changed original publication.
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub fn catalog_resume_split_continuation(
        &mut self,
        request: &SplitRecoveryRequest,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) = self.catalog_split_recovery_receipt(request.caller, &request.key)? {
            ensure!(saved.request == *request, "split continuation key conflict");
            return Ok(saved);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        key_current(&tx, &request.key)?;
        ensure!(
            request.action == SplitRecoveryAction::Resume
                && old.receipt.supports_metadata_continuation(),
            "split phase requires its exact worker recovery or is terminal"
        );
        require_unused_action_key(&tx, request.caller, &request.key, None)?;
        let running:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM index_split_jobs s JOIN service_jobs j ON j.id=s.job WHERE s.operation_id=?1 AND j.state IN('pending','running'))",[request.operation_id.to_string()],|row|row.get(0))?;
        ensure!(
            !running,
            "split metadata continuation waits for accepted child work"
        );
        if let Some(last) = old.receipt.jobs.last() {
            let record = super::super::jobs::record(&tx, last.job)?;
            ensure!(
                record.state == JobState::Succeeded,
                "split failed/canceled child requires exact worker recovery"
            );
            ensure!(
                super::super::configuration::result(&tx, last.job)?.is_some(),
                "split successful child result proof is not retained; outcome remains unknown"
            );
        }
        let mut next = old.receipt.clone();
        next.cleanup_attention = None;
        if !matches!(
            next.state,
            SplitRunState::CanceledBeforeFence {
                cleanup_pending: true
            }
        ) {
            next.state = SplitRunState::Pending;
        }
        let value = replace(&tx, &old, next)?;
        let receipt = SplitRecoveryReceipt {
            request: request.clone(),
            request_digest: framed_digest(
                b"airk/catalog-split-recovery/1",
                &[&serde_json::to_vec(request)?],
            ),
            outcome: SplitRecoveryOutcome::ContinuationAccepted {
                receipt_digest: value.receipt_digest,
                resulting_receipt: Box::new(value.receipt),
            },
        };
        receipt.validate_for(&old.request, &old.receipt)?;
        tx.execute(
            "INSERT INTO index_split_recoveries VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                request.caller.to_string(),
                request.key.namespace.to_string(),
                i64::try_from(request.key.epoch)?,
                request.key.nonce.to_string(),
                request.operation_id.to_string(),
                serde_json::to_vec(&receipt)?
            ],
        )?;
        tx.commit()?;
        Ok(receipt)
    }
}

fn enqueue_split_job_in(
    db: &Connection,
    operation: Incarnation,
    expected: Sha256Digest,
    caller: Incarnation,
    key: &RequestKey,
    purpose: SplitJobPurpose,
    work: &ConfigurationWork,
) -> Result<SplitSnapshot> {
    let engine = work.engine;
    let configuration = &work.request;
    configuration.validate()?;
    let observed = get(db, operation)?.context("split operation not retained")?;
    let existing = observed
        .receipt
        .jobs
        .iter()
        .find(|job| job.caller == caller && job.key == *key);
    if let Some(existing) = existing {
        let work = ConfigurationWork {
            engine,
            request: configuration.clone(),
        };
        ensure!(
            existing.purpose == purpose
                && existing.engine_identity == engine
                && existing.configuration_digest
                    == framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?]),
            "split request key conflict"
        );
        return Ok(observed);
    }
    let old = exact(db, operation, expected)?;
    key_current(db, key)?;
    ensure!(
        old.finalization.is_none(),
        "split finalization is frozen; use its exact finalization recovery"
    );
    require_unused_action_key(db, caller, key, None)?;
    ensure!(
        purpose == SplitJobPurpose::DestinationCleanup
            || !matches!(
                old.receipt.state,
                SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
            ),
        "only destination cleanup may continue a canceled split",
    );
    let ProjectConfiguration::SplitWork {
        operation_id,
        request_digest,
        expected_receipt_digest,
        purpose: work_purpose,
        require_semantic_ready,
    } = &configuration.configuration
    else {
        anyhow::bail!("exact native split configuration required")
    };
    ensure!(
        *operation_id == operation
            && *request_digest == old.request.identity()?
            && *expected_receipt_digest == expected
            && *work_purpose == purpose
            && *require_semantic_ready
                == (purpose == SplitJobPurpose::DestinationBuild
                    && old.request.require_semantic_ready),
        "split job differs from admitted operation"
    );
    let backing = match purpose {
        SplitJobPurpose::DestinationBuild => {
            ensure!(
                matches!(
                    old.receipt.stage,
                    SplitStage::DestinationPrepared | SplitStage::Building
                ) && old.receipt.capacity_receipt.is_some(),
                "split destination is not funded/prepared"
            );
            baseline_current(db, &old)?;
            old.request.destination_registration.binding
        }
        SplitJobPurpose::OriginalWitness => {
            ensure!(
                old.receipt.stage == SplitStage::DestinationValidated
                    && !matches!(
                        old.receipt.state,
                        SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
                    ),
                "split witness phase differs"
            );
            baseline_current(db, &old)?;
            old.request.original_registration.binding
        }
        SplitJobPurpose::OriginalRemoval => {
            anyhow::bail!("split original removal requires the existing native source policy fence")
        }
        SplitJobPurpose::DestinationCleanup => {
            ensure!(
                matches!(
                    old.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ),
                "split destination cleanup is not pending"
            );
            old.request.destination_registration.binding
        }
        SplitJobPurpose::OriginalCleanup => {
            ensure!(
                old.receipt.stage >= SplitStage::MembershipApplied
                    && old.receipt.stage < SplitStage::Complete,
                "split original cleanup stage differs"
            );
            old.request.original_registration.binding
        }
    };
    let (pending_jobs,running_jobs):(u32,u32)=db.query_row("SELECT coalesce(sum(state='pending'),0),coalesce(sum(state='running'),0) FROM service_jobs WHERE project=?1 AND state IN('pending','running')",[backing.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
    if pending_jobs > 0 || running_jobs > 0 {
        return Err(airk_core::catalog_source_change::SourceChangeBusy {
            index_id: if backing == old.request.destination_registration.binding {
                old.request.destination.index_id
            } else {
                old.request.original_index
            },
            pending_jobs,
            running_jobs,
        }
        .into());
    }
    super::super::jobs::require_request_capacity(db)?;
    let work = ConfigurationWork {
        engine,
        request: configuration.clone(),
    };
    let job = super::super::jobs::insert_new_job(
        db,
        backing,
        caller,
        key,
        JobKind::Configure,
        Some(&work),
    )?;
    super::super::jobs::insert_request_receipt(db, caller, key, job)?;
    let reference = SplitJobRef {
        purpose,
        caller,
        key: key.clone(),
        backing,
        job,
        engine_identity: engine,
        configuration_digest: framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?]),
    };
    db.execute(
        "INSERT INTO index_split_jobs VALUES(?1,?2,?3)",
        params![
            job.to_string(),
            operation.to_string(),
            serde_json::to_vec(&reference)?
        ],
    )?;
    let mut next = old.receipt.clone();
    next.cleanup_attention = None;
    next.jobs.push(reference);
    if purpose == SplitJobPurpose::DestinationBuild {
        next.stage = SplitStage::Building;
    }
    // Exact accepted work clears the attention latch for every runnable purpose,
    // including a fresh retained-witness job and an original cleanup retry.
    // Cancellation remains restrictive throughout destination-only cleanup.
    if purpose != SplitJobPurpose::DestinationCleanup {
        next.state = SplitRunState::Pending;
    }
    replace(db, &old, next)
}

fn cancel_split_in(
    db: &Connection,
    operation: Incarnation,
    expected: Sha256Digest,
) -> Result<SplitSnapshot> {
    let old = exact(db, operation, expected)?;
    ensure!(
        old.finalization.is_none(),
        "split finalization is frozen; use its exact finalization recovery"
    );
    ensure!(
        !matches!(
            old.receipt.state,
            SplitRunState::Complete
                | SplitRunState::CanceledBeforeFence {
                    cleanup_pending: false
                }
        ),
        "terminal split cannot be reopened by cancellation"
    );
    let cutover:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind='catalog_management_fenced' AND json_extract(payload,'$.request.operation.change.split_operation')=?1)",[operation.to_string()],|row|row.get(0))?;
    ensure!(
        !cutover && old.receipt.stage < SplitStage::OriginalFenced,
        "split restrictive cutover must finish; cancellation cannot undo its fence"
    );
    let mut next = old.receipt.clone();
    next.cleanup_attention = None;
    next.state = SplitRunState::CanceledBeforeFence {
        cleanup_pending: true,
    };
    // Cancel only this operation's recorded build/witness jobs. Preserve all
    // unrelated original and destination work.
    for reference in &old.receipt.jobs {
        if matches!(
            reference.purpose,
            SplitJobPurpose::DestinationBuild | SplitJobPurpose::OriginalWitness
        ) {
            super::super::jobs::request_cancellation_in(db, reference.job)?;
        }
    }
    replace(db, &old, next)
}

fn save_action_ack(
    db: &Connection,
    request: &SplitRecoveryRequest,
    old: &SplitSnapshot,
    outcome: SplitRecoveryOutcome,
) -> Result<SplitRecoveryReceipt> {
    let receipt = SplitRecoveryReceipt {
        request: request.clone(),
        request_digest: framed_digest(
            b"airk/catalog-split-recovery/1",
            &[&serde_json::to_vec(request)?],
        ),
        outcome,
    };
    receipt.validate_for(&old.request, &old.receipt)?;
    db.execute(
        "INSERT INTO index_split_recoveries VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            request.caller.to_string(),
            request.key.namespace.to_string(),
            i64::try_from(request.key.epoch)?,
            request.key.nonce.to_string(),
            request.operation_id.to_string(),
            serde_json::to_vec(&receipt)?
        ],
    )?;
    Ok(receipt)
}
impl ServiceRegistry {
    pub fn catalog_cancel_split_action(
        &mut self,
        request: &SplitRecoveryRequest,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) = self.catalog_split_recovery_receipt(request.caller, &request.key)? {
            ensure!(saved.request == *request, "split cancellation key conflict");
            return Ok(saved);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        key_current(&tx, &request.key)?;
        ensure!(
            request.action == SplitRecoveryAction::Cancel,
            "split Cancel action required"
        );
        require_unused_action_key(&tx, request.caller, &request.key, None)?;
        let value = cancel_split_in(&tx, request.operation_id, request.expected_receipt_digest)?;
        let receipt = save_action_ack(
            &tx,
            request,
            &old,
            SplitRecoveryOutcome::CancelBeforeFenceAccepted {
                receipt_digest: value.receipt_digest,
                cleanup_pending: true,
                resulting_receipt: Box::new(value.receipt),
            },
        )?;
        tx.commit()?;
        Ok(receipt)
    }
    pub fn catalog_resume_split_job_action(
        &mut self,
        request: &SplitRecoveryRequest,
        engine: Sha256Digest,
        configuration: &ConfigurationRequest,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) = self.catalog_split_recovery_receipt(request.caller, &request.key)? {
            ensure!(
                saved.request == *request,
                "split worker recovery key conflict"
            );
            return Ok(saved);
        }
        if self.catalog_split_resume_requires_continuation(request)? {
            return self.catalog_resume_split_continuation(request);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        ensure!(
            request.action == SplitRecoveryAction::Resume,
            "split Resume action required"
        );
        let ProjectConfiguration::SplitWork { purpose, .. } = &configuration.configuration else {
            anyhow::bail!("exact saved split worker configuration required")
        };
        if let Some(previous) = old
            .receipt
            .jobs
            .iter()
            .rev()
            .find(|job| job.purpose == *purpose)
        {
            let record = super::super::jobs::record(&tx, previous.job)?;
            ensure!(
                matches!(record.state, JobState::Failed | JobState::Canceled)
                    || *purpose == SplitJobPurpose::OriginalWitness
                        && record.state == JobState::Succeeded,
                "split worker retry requires terminal/reviewable work"
            );
            let mut work = super::super::configuration::work(&tx, previous.job)?
                .context("split retry saved configuration not retained")?;
            if let ProjectConfiguration::SplitWork {
                expected_receipt_digest,
                ..
            } = &mut work.request.configuration
            {
                *expected_receipt_digest = request.expected_receipt_digest;
            }
            ensure!(
                work.engine == engine && work.request == *configuration,
                "split retry must preserve saved engine/configuration"
            );
        }
        let value = enqueue_split_job_in(
            &tx,
            request.operation_id,
            request.expected_receipt_digest,
            request.caller,
            &request.key,
            *purpose,
            &ConfigurationWork {
                engine,
                request: configuration.clone(),
            },
        )?;
        let job = value
            .receipt
            .jobs
            .last()
            .context("split worker recovery lacks accepted child")?
            .job;
        let receipt = save_action_ack(
            &tx,
            request,
            &old,
            SplitRecoveryOutcome::ResumeAccepted {
                accepted_job: job,
                receipt_digest: value.receipt_digest,
                resulting_receipt: Box::new(value.receipt),
            },
        )?;
        tx.commit()?;
        Ok(receipt)
    }
    pub fn catalog_pending_splits(&self, limit: usize) -> Result<Vec<SplitSnapshot>> {
        ensure!(
            (1..=8).contains(&limit),
            "split metadata wake limit must be 1..8"
        );
        let tx = self.connection.unchecked_transaction()?;
        if !available(&tx)? {
            return Ok(vec![]);
        }
        let mut query=tx.prepare("SELECT DISTINCT operation_id FROM index_split_participants WHERE active=1 ORDER BY operation_id LIMIT 9")?;
        let ids = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            ids.len() <= 8,
            "split active metadata wake census exceeds bound"
        );
        let mut result = Vec::new();
        for id in ids {
            let value = get(&tx, id.parse()?)?.context("active split metadata journal missing")?;
            if value.receipt.cleanup_attention.is_some()
                || value
                    .finalization
                    .as_ref()
                    .is_some_and(|state| state.attention.is_some())
            {
                continue;
            }
            let unfinished_finalization = value
                .finalization
                .as_ref()
                .is_some_and(|state| state.progress != SplitFinalizationProgress::Complete);
            let eligible = unfinished_finalization
                || match value.receipt.state {
                    SplitRunState::Pending | SplitRunState::Running => true,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true,
                    } => {
                        if let Some(job) = value
                            .receipt
                            .jobs
                            .iter()
                            .rev()
                            .find(|job| job.purpose == SplitJobPurpose::DestinationCleanup)
                        {
                            !matches!(
                                super::super::jobs::record(&tx, job.job)?.state,
                                JobState::Failed | JobState::Canceled
                            )
                        } else {
                            true
                        }
                    }
                    _ => false,
                };
            if eligible {
                result.push(value);
                if result.len() == limit {
                    break;
                }
            }
        }
        Ok(result)
    }
}

fn attach_split_policy_retry_in(
    db: &Connection,
    operation: Incarnation,
    expected: Sha256Digest,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<SplitSnapshot> {
    let current = get(db, operation)?.context("split operation not retained")?;
    if current
        .receipt
        .jobs
        .iter()
        .any(|job| job.caller == caller && job.key == *key)
    {
        return Ok(current);
    }
    let old = exact(db, operation, expected)?;
    ensure!(
        old.receipt.stage == SplitStage::OriginalFenced,
        "split original retry stage differs"
    );
    let (source,accepted):(String,String)=db.query_row("SELECT r.source_job,q.job FROM service_retry_requests r JOIN service_requests q ON q.caller=r.caller AND q.namespace=r.namespace AND q.epoch=r.epoch AND q.nonce=r.nonce WHERE r.caller=?1 AND r.namespace=?2 AND r.epoch=?3 AND r.nonce=?4",params![caller.to_string(),key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
    let source: Sha256Digest = source.parse()?;
    let accepted: Sha256Digest = accepted.parse()?;
    let predecessor = old
        .receipt
        .jobs
        .iter()
        .find(|job| job.job == source && job.purpose == SplitJobPurpose::OriginalRemoval)
        .context("split retry lost original job lineage")?;
    ensure!(
        super::policy::require_claim(db, accepted)?,
        "split policy retry authority unavailable"
    );
    let work = super::super::configuration::work(db, accepted)?
        .context("split retry work not retained")?;
    let digest = framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?]);
    ensure!(
        work.engine == predecessor.engine_identity && digest == predecessor.configuration_digest,
        "split retry changed saved work"
    );
    let reference = SplitJobRef {
        purpose: SplitJobPurpose::OriginalRemoval,
        caller,
        key: key.clone(),
        backing: predecessor.backing,
        job: accepted,
        engine_identity: work.engine,
        configuration_digest: digest,
    };
    db.execute(
        "INSERT INTO index_split_jobs VALUES(?1,?2,?3)",
        params![
            accepted.to_string(),
            operation.to_string(),
            serde_json::to_vec(&reference)?
        ],
    )?;
    let mut next = old.receipt.clone();
    next.cleanup_attention = None;
    next.jobs.push(reference);
    next.state = SplitRunState::Pending;
    replace(db, &old, next)
}

impl ServiceRegistry {
    /// Exact original-removal retry, split successor and outer recovery ack are
    /// one registry transaction; native owner/funding publication follows it.
    pub fn catalog_resume_split_policy_action(
        &mut self,
        request: &SplitRecoveryRequest,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) = self.catalog_split_recovery_receipt(request.caller, &request.key)? {
            ensure!(
                saved.request == *request,
                "split policy recovery key conflict"
            );
            return Ok(saved);
        }
        if self.catalog_split_resume_requires_continuation(request)? {
            return self.catalog_resume_split_continuation(request);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        key_current(&tx, &request.key)?;
        ensure!(
            request.action == SplitRecoveryAction::Resume
                && old.receipt.stage == SplitStage::OriginalFenced,
            "split original removal recovery phase differs"
        );
        require_unused_action_key(&tx, request.caller, &request.key, None)?;
        let policy = super::policy::snapshot(&tx, old.request.original_index)?
            .context("split original policy missing")?;
        ensure!(
            policy.policy == old.plan.remainder
                && old
                    .receipt
                    .jobs
                    .iter()
                    .any(|job| job.purpose == SplitJobPurpose::OriginalRemoval
                        && job.job == policy.job),
            "split original policy retry lineage differs"
        );
        super::policy::resume_policy_job_in(
            &tx,
            old.request.original_index,
            policy.operation_id,
            request.caller,
            &request.key,
        )?;
        let value = attach_split_policy_retry_in(
            &tx,
            request.operation_id,
            request.expected_receipt_digest,
            request.caller,
            &request.key,
        )?;
        let job = value
            .receipt
            .jobs
            .last()
            .context("split policy recovery child missing")?
            .job;
        let receipt = save_action_ack(
            &tx,
            request,
            &old,
            SplitRecoveryOutcome::ResumeAccepted {
                accepted_job: job,
                receipt_digest: value.receipt_digest,
                resulting_receipt: Box::new(value.receipt),
            },
        )?;
        tx.commit()?;
        Ok(receipt)
    }
}

impl ServiceRegistry {
    /// Saved immutable worker lineage for an explicit native retry, including
    /// already removed cleanup slots. Only the new exact phase reference changes.
    pub fn catalog_split_saved_work_configuration(
        &self,
        operation: Incarnation,
        purpose: SplitJobPurpose,
    ) -> Result<Option<ConfigurationWork>> {
        let tx = self.connection.unchecked_transaction()?;
        let value = get(&tx, operation)?.context("split operation not retained")?;
        let Some(reference) = value
            .receipt
            .jobs
            .iter()
            .rev()
            .find(|job| job.purpose == purpose)
        else {
            return Ok(None);
        };
        let mut work = super::super::configuration::work(&tx, reference.job)?
            .context("split saved configuration not retained; outcome unknown")?;
        ensure!(
            work.engine == reference.engine_identity
                && framed_digest(b"airk/split-work/1", &[&serde_json::to_vec(&work)?])
                    == reference.configuration_digest,
            "split saved configuration lineage differs"
        );
        if let ProjectConfiguration::SplitWork {
            expected_receipt_digest,
            ..
        } = &mut work.request.configuration
        {
            *expected_receipt_digest = value.receipt_digest;
        }
        Ok(Some(work))
    }
}

impl ServiceRegistry {
    pub fn catalog_active_split_for_binding(
        &self,
        binding: airk_core::service::ProjectBinding,
    ) -> Result<Option<SplitSnapshot>> {
        let tx = self.connection.unchecked_transaction()?;
        if !available(&tx)? {
            return Ok(None);
        }
        let mut query=tx.prepare("SELECT DISTINCT o.operation_id FROM index_split_operations o JOIN index_split_participants p ON p.operation_id=o.operation_id AND p.active=1 WHERE json_extract(o.payload,'$.request.original_registration.binding')=?1 OR json_extract(o.payload,'$.request.destination_registration.binding')=?1 LIMIT 2")?;
        let ids = query
            .query_map([binding.to_string()], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            ids.len() <= 1,
            "backing has conflicting active split operations"
        );
        ids.first()
            .map(|id| {
                get(&tx, id.parse()?)
                    .and_then(|value| value.context("active split journal missing"))
            })
            .transpose()
    }
}

fn write_fence_select(binding: &str) -> String {
    format!(
        "SELECT 1 FROM index_split_operations o JOIN index_split_participants p ON p.operation_id=o.operation_id AND p.active=1 WHERE (p.role='destination' AND json_extract(o.payload,'$.request.destination_registration.binding')={binding}) OR (p.role='original_remainder' AND json_extract(o.payload,'$.request.original_registration.binding')={binding} AND (json_extract(o.payload,'$.receipt.stage') IN('original_fenced','remainder_published','membership_applied','cleanup_pending','complete') OR EXISTS(SELECT 1 FROM index_operation_receipts f WHERE f.kind='catalog_management_fenced' AND f.stage<>'metadata_applied' AND json_extract(f.payload,'$.request.operation.operation')='change_index_policy' AND json_extract(f.payload,'$.request.operation.id')=p.index_id AND json_extract(f.payload,'$.request.operation.change.split_operation')=o.operation_id)))"
    )
}
fn binding_write_fenced(
    db: &Connection,
    binding: airk_core::service::ProjectBinding,
) -> Result<bool> {
    if !available(db)? {
        return Ok(false);
    }
    db.query_row(
        &format!("SELECT EXISTS({})", write_fence_select("?1")),
        [binding.to_string()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
pub(in crate::service::registry) fn watch_eligibility(db: &Connection) -> Result<String> {
    if !available(db)? {
        return Ok("1".into());
    }
    Ok(format!(
        "NOT EXISTS({})",
        write_fence_select("projects.binding")
    ))
}
pub(in crate::service::registry) fn job_eligibility(db: &Connection) -> Result<String> {
    if !available(db)? {
        return Ok("1".into());
    }
    Ok(format!(
        "(NOT EXISTS({}) OR EXISTS(SELECT 1 FROM index_split_jobs s WHERE s.job=j.id))",
        write_fence_select("j.project")
    ))
}

impl ServiceRegistry {
    pub fn catalog_split_write_fenced(
        &self,
        binding: airk_core::service::ProjectBinding,
    ) -> Result<bool> {
        let tx = self.connection.unchecked_transaction()?;
        binding_write_fenced(&tx, binding)
    }
}
