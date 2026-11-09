// SPDX-License-Identifier: Apache-2.0
//! Durable index-owned deny admission. The deny and its exact native removal job
//! commit together under the existing catalog fence; failed cleanup never clears it.
use super::{ServiceRegistry, operations::index_record};
use airk_core::{
    catalog_management::{CatalogOperation, CatalogRequest},
    catalog_policy::{PolicyImpact, PolicySnapshot, SourcePolicyApplyReport},
    catalog_source_change::SourceChangeBusy,
    identity::{Incarnation, Sha256Digest},
    index_catalog::{IndexId, MAX_SOURCE_ROOTS, encoded_size},
    service::{JobKind, JobOutcome, JobState, RequestKey},
    service_configuration::{ConfigurationWork, ProjectConfiguration},
    source_policy::{MAX_POLICY_BYTES, PolicyChange, SourcePolicy, SourceRoot},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

/// Native scheduler selector, chosen before preparing SQL so legacy policy-0
/// registries never reference a policy-1 table. Pending source/vector jobs remain
/// queued without consuming attempts while the exact retained-removal job runs.
pub(in crate::service::registry) fn job_eligibility(db: &Connection) -> Result<&'static str> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        matches!(version, 11..=13),
        "unsupported source policy scheduler format"
    );
    if version == 11 || super::state(db)?.policy_contract == 0 {
        return Ok("1");
    }
    Ok(
        "NOT EXISTS(SELECT 1 FROM index_definitions policy_index JOIN index_source_policies policy_state ON policy_state.index_id=policy_index.id WHERE policy_index.active_registration=j.project AND json_extract(policy_state.payload,'$.application') IS NULL AND json_extract(policy_state.payload,'$.job')<>j.id)",
    )
}

pub(super) fn impact(db: &Connection, request: &CatalogRequest) -> Result<Option<PolicyImpact>> {
    let CatalogOperation::ChangeIndexPolicy { id, change, .. } = &request.operation else {
        return Ok(None);
    };
    let roots = source_roots(db, *id)?;
    let selector = match change {
        PolicyChange::SplitRemainder { .. } => return Ok(None),
        PolicyChange::Trim { selector, .. } => selector.clone(),
        PolicyChange::Unblock { rule_id } => snapshot(db, *id)?
            .context("unblock policy unavailable")?
            .policy
            .denies
            .iter()
            .find(|rule| rule.rule_id == *rule_id)
            .context("unblock rule unavailable")?
            .atom(),
    };
    let root = roots
        .iter()
        .find(|root| root.id == selector.source_root_id)
        .context("policy selector root unavailable")?;
    let target = std::path::Path::new(&root.canonical_path).join(&selector.path);
    let mut projects=db.prepare("SELECT project_id FROM project_index_memberships WHERE index_id=?1 ORDER BY project_id LIMIT 33")?;
    let projects = projects
        .query_map([id.to_string()], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        projects.len() <= 32,
        "affected Projects exceed bounded policy review capacity"
    );
    let affected_project_ids = projects
        .into_iter()
        .map(|value| value.parse())
        .collect::<Result<Vec<_>>>()?;
    let mut statement=db.prepare("SELECT s.index_id,substr(CAST(s.canonical_path AS BLOB),1,32769) FROM index_sources s JOIN index_definitions i ON i.id=s.index_id WHERE s.index_id<>?1 AND s.enabled=1 AND i.tombstone=0 ORDER BY s.index_id,s.id LIMIT 17409")?;
    let mut rows = statement.query([id.to_string()])?;
    let mut examined = 0usize;
    let mut bytes = 0usize;
    let mut covering = std::collections::BTreeSet::new();
    while let Some(row) = rows.next()? {
        examined += 1;
        let source: Vec<u8> = row.get(1)?;
        bytes = bytes
            .checked_add(source.len())
            .context("policy impact metadata byte overflow")?;
        ensure!(
            examined <= 17 * 1024 && source.len() <= 32768 && bytes <= 2 * 1024 * 1024,
            "policy overlap review exceeds bounded metadata allowance"
        );
        let source = String::from_utf8(source)?;
        let source = std::path::Path::new(&source);
        if target.starts_with(source)
            || selector.kind == airk_core::index_catalog::PathScope::Subtree
                && source.starts_with(&target)
        {
            let index: String = row.get(0)?;
            covering.insert(index.parse()?);
            ensure!(
                covering.len() <= 32,
                "other covering indexes exceed bounded policy review capacity"
            );
        }
    }
    Ok(Some(PolicyImpact {
        target:selector.clone(),canonical_root:root.canonical_path.clone(),
        affected_artifacts:None,affected_vectors:None,affected_facts:None,affected_relationships:None,
        counts_basis:"Not observed: preview reads bounded catalog metadata only, without opening sources or querying the evidence corpus.".into(),
        affected_source_root_ids:vec![selector.source_root_id],affected_project_ids,
        other_covering_index_ids:covering.into_iter().collect(),
        overlap_basis:"Configured canonical root/path overlap candidates only. Other indexes are unchanged; their include/deny eligibility, filesystem aliases and current evidence coverage were not observed.".into(),
    }))
}

pub(super) fn source_roots(db: &Connection, index: IndexId) -> Result<Vec<SourceRoot>> {
    let mut statement = db.prepare("SELECT id,canonical_path,owner FROM index_sources WHERE index_id=?1 AND enabled=1 ORDER BY id LIMIT 18")?;
    let rows = statement
        .query_map([index.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        !rows.is_empty() && rows.len() <= MAX_SOURCE_ROOTS,
        "policy source metadata unavailable"
    );
    rows.into_iter().map(|(id,canonical_path,owner)| {
        let mut aliases = db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias LIMIT 17")?;
        let aliases = aliases.query_map(params![index.to_string(),id], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let source = SourceRoot { id: id.parse()?, canonical_path, owner, aliases };
        source.validate()?;
        Ok(source)
    }).collect()
}

pub(super) fn snapshot(db: &Connection, id: IndexId) -> Result<Option<PolicySnapshot>> {
    let index = index_record(db, id)?;
    let Some(reference) = &index.policy else {
        return Ok(None);
    };
    ensure!(
        super::state(db)?.policy_contract >= 1,
        "source policy contract is unavailable"
    );
    let bytes: Vec<u8> = db.query_row(
        "SELECT substr(payload,1,1048577) FROM index_source_policies WHERE index_id=?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        bytes.len() <= MAX_POLICY_BYTES,
        "source policy snapshot exceeds byte budget"
    );
    let mut snapshot: PolicySnapshot = serde_json::from_slice(&bytes)?;
    snapshot.validate()?;
    ensure!(
        snapshot.policy.index_id == id
            && snapshot.policy.owner_binding.to_string() == index.backing.binding.to_string()
            && snapshot.policy.collection == index.backing.collection
            && snapshot.policy.epoch == reference.epoch
            && snapshot.policy.identity()? == reference.digest,
        "source policy identity differs from index backing"
    );
    // Policy roots are immutable canonical identities. Alias changes and source
    // retirement must use a separately qualified reconciliation, never dropping
    // or rebinding a policy's root as an incidental catalog mutation.
    for root in &snapshot.policy.roots {
        let bound:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM index_sources WHERE id=?1 AND index_id=?2 AND canonical_path=?3 AND owner=?4)",params![root.id.to_string(),id.to_string(),root.canonical_path,root.owner],|row|row.get(0))?;
        ensure!(bound, "source policy root binding changed");
    }
    if snapshot
        .application
        .as_ref()
        .is_none_or(|application| application.physical_cleanup_pending)
    {
        let job = super::super::jobs::record(db, snapshot.job)?;
        snapshot.last_error = match job.outcome {
            Some(JobOutcome::Failed { code }) => Some(code.chars().take(256).collect()),
            Some(JobOutcome::Canceled) => Some("policy_application_canceled".into()),
            _ => None,
        };
    }
    Ok(Some(snapshot))
}

pub(super) fn write_snapshot(db: &Connection, value: &PolicySnapshot) -> Result<()> {
    value.validate()?;
    encoded_size(value, MAX_POLICY_BYTES)?;
    let payload = serde_json::to_vec(value)?;
    let policy_bytes = encoded_size(&value.policy, MAX_POLICY_BYTES)?;
    let capacity = policy_bytes
        .checked_add(airk_core::catalog_management::MAX_RECEIPT_BYTES)
        .context("policy completion capacity overflow")?;
    ensure!(
        payload.len() <= capacity,
        "policy completion exceeds reserved receipt capacity"
    );
    // Keep fixed total record capacity through every pending logical/physical
    // stage. Growth for native reports consumes admitted reserve rather than
    // discovering a full registry after the deny or removal has committed.
    let reserve = if value
        .application
        .as_ref()
        .is_some_and(|report| !report.physical_cleanup_pending)
    {
        0
    } else {
        capacity - payload.len()
    };
    db.execute("INSERT INTO index_source_policies(index_id,payload,completion_reserve) VALUES(?1,?2,zeroblob(?3)) ON CONFLICT(index_id) DO UPDATE SET payload=excluded.payload,completion_reserve=excluded.completion_reserve",
        params![value.policy.index_id.to_string(),payload,i64::try_from(reserve)?])?;
    Ok(())
}

pub(super) fn mutate(
    db: &Connection,
    request: &CatalogRequest,
    context: super::operations::MutationContext,
) -> Result<()> {
    let CatalogOperation::ChangeIndexPolicy {
        id,
        expected_source_revision,
        expected_policy_epoch,
        expected_policy_digest,
        change,
        configuration,
        engine_identity,
    } = &request.operation
    else {
        anyhow::bail!("source policy operation required");
    };
    super::split::require_policy_change(db, *id, change)?;
    let index = index_record(db, *id)?;
    ensure!(
        !index.tombstone
            && index.source_import == airk_core::index_catalog::SourceImportState::Complete
            && index.source_revision == *expected_source_revision,
        "source policy index review stale"
    );
    ensure!(
        index.policy.as_ref().map_or(0, |p| p.epoch.get()) == *expected_policy_epoch
            && index.policy.as_ref().map(|p| p.digest) == *expected_policy_digest,
        "source policy review stale"
    );
    let registered:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1 AND cleanup_guard IS NULL)", [index.backing.binding.to_string()], |row| row.get(0))?;
    ensure!(registered, "source policy registration unavailable");
    let (pending,running):(u32,u32)=db.query_row("SELECT coalesce(sum(state='pending'),0),coalesce(sum(state='running'),0) FROM service_jobs WHERE project=?1 AND state IN('pending','running')",[index.backing.binding.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
    if pending != 0 || running != 0 {
        return Err(SourceChangeBusy {
            index_id: *id,
            pending_jobs: pending,
            running_jobs: running,
        }
        .into());
    }
    let after = if let PolicyChange::SplitRemainder {
        split_operation,
        expected_split_receipt,
    } = change
    {
        super::split::reviewed_remainder(db, *id, *split_operation, *expected_split_receipt)?
    } else if let Some(current) = snapshot(db, *id)? {
        if current.application.is_none() {
            // A terminal removal failure keeps its deny active. Only a newly
            // reviewed exact Unblock may supersede that failed operation; a new
            // trim, ordinary index or retry cannot silently discard its fence.
            ensure!(
                matches!(change, PolicyChange::Unblock { .. }),
                "previous policy removal remains pending"
            );
            let previous = super::super::jobs::record(db, current.job)?;
            ensure!(
                matches!(previous.state, JobState::Failed | JobState::Canceled),
                "unblock waits for the admitted policy removal job to settle"
            );
            ensure!(
                require_claim(db, current.job)?,
                "failed source policy operation authority unavailable"
            );
        }
        current
            .policy
            .preview_change(current.policy.identity()?, change.clone())?
            .after
    } else {
        let PolicyChange::Trim { rule_id, selector } = change else {
            anyhow::bail!("unblock requires an existing policy");
        };
        SourcePolicy::initial_trim(&index, source_roots(db, *id)?, *rule_id, selector.clone())?
    };
    let operation_id =
        airk_core::catalog_management::catalog_operation_id(request.caller, &request.key)?;
    let ProjectConfiguration::ApplySourcePolicy {
        policy,
        operation_id: work_operation,
    } = &configuration.configuration
    else {
        anyhow::bail!("source policy configuration missing");
    };
    ensure!(
        policy.as_ref() == &after && *work_operation == operation_id,
        "source policy job differs from reviewed canonical transition"
    );
    super::apply_policy_v1_layout(db)?;
    super::super::jobs::require_request_capacity(db)?;
    let work = ConfigurationWork {
        engine: *engine_identity,
        request: configuration.as_ref().clone(),
    };
    let job = super::super::jobs::insert_new_job(
        db,
        index.backing.binding,
        request.caller,
        &request.key,
        JobKind::Configure,
        Some(&work),
    )?;
    super::super::jobs::insert_request_receipt(db, request.caller, &request.key, job)?;
    let value = PolicySnapshot {
        schema: 1,
        operation_id,
        policy: after,
        job,
        application: None,
        application_job: None,
        last_error: None,
    };
    let digest = value.policy.identity()?;
    write_snapshot(db, &value)?;
    // The preview savepoint projects the same policy/job/index transition, but
    // cannot claim OriginalFenced before the native coordinator performs it.
    // Actual commits still require the exact durable drain/fence and owner seal.
    if context == super::operations::MutationContext::Commit
        && let PolicyChange::SplitRemainder {
            split_operation,
            expected_split_receipt,
        } = change
    {
        super::split::record_original_fence(
            db,
            request,
            *split_operation,
            *expected_split_receipt,
            job,
        )?;
    }
    ensure!(db.execute("UPDATE index_definitions SET policy_epoch=?1,policy_digest=?2 WHERE id=?3 AND source_revision=?4 AND policy_epoch=?5",
        params![i64::try_from(value.policy.epoch.get())?,digest.to_string(),id.to_string(),i64::try_from(expected_source_revision.get())?,i64::try_from(*expected_policy_epoch)?])? == 1,
        "source policy review changed");
    super::super::watch::ensure_row(db, index.backing.binding)?;
    ensure!(db.execute("UPDATE service_watch SET configuration_epoch=configuration_epoch+1,roots_observed=0,native_roots=0,backend=CASE WHEN enabled=1 THEN 'periodic_fallback' ELSE 'disabled' END,failure='metadata_unavailable' WHERE project=?1 AND configuration_epoch<9223372036854775807",[index.backing.binding.to_string()])? == 1,
        "source policy watcher revision exhausted");
    Ok(())
}

#[cfg(test)]
mod tests;

pub(in crate::service::registry) fn require_claim(
    db: &Connection,
    job: Sha256Digest,
) -> Result<bool> {
    let Some(work) = super::super::configuration::work(db, job)? else {
        return Ok(false);
    };
    let ProjectConfiguration::ApplySourcePolicy {
        policy,
        operation_id,
    } = &work.request.configuration
    else {
        return Ok(false);
    };
    let current =
        snapshot(db, policy.index_id)?.context("source policy job has no admitted policy")?;
    let record = super::super::jobs::record(db, job)?;
    let index = index_record(db, policy.index_id)?;
    ensure!(
        record.project == index.backing.binding
            && record.kind == JobKind::Configure
            && current.job == job
            && current.operation_id == *operation_id
            && current.policy == **policy,
        "source policy job differs from exact admitted operation"
    );
    let parent:Option<(String,Vec<u8>)> = db.query_row("SELECT stage,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management_fenced'", [operation_id.to_string()], |row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let (stage, bytes) = parent.context("source policy native fence unavailable")?;
    ensure!(
        stage == "metadata_applied"
            && bytes.len() <= airk_core::catalog_management::MAX_RECEIPT_BYTES,
        "source policy native fence remains pending"
    );
    let receipt: airk_core::catalog_fences::FencedReceipt = serde_json::from_slice(&bytes)?;
    receipt.validate()?;
    let CatalogOperation::ChangeIndexPolicy {
        id,
        configuration,
        engine_identity,
        ..
    } = &receipt.request.operation
    else {
        anyhow::bail!("source policy job parent differs");
    };
    ensure!(
        *id == policy.index_id
            && receipt.metadata.operation_id == *operation_id
            && work.request == **configuration
            && work.engine == *engine_identity,
        "source policy job lost its exact accepted engine/configuration lineage"
    );
    Ok(true)
}

pub(in crate::service::registry) fn validate_result(
    db: &Connection,
    job: Sha256Digest,
    result: &serde_json::Value,
) -> Result<()> {
    let Some(work) = super::super::configuration::work(db, job)? else {
        return Ok(());
    };
    if let ProjectConfiguration::ApplySourcePolicy {
        policy,
        operation_id,
    } = &work.request.configuration
    {
        ensure!(
            require_claim(db, job)?,
            "source policy job authority unavailable"
        );
        let report: SourcePolicyApplyReport = serde_json::from_value(result.clone())?;
        report.validate()?;
        ensure!(
            report.policy_digest == policy.identity()? && report.operation_id == *operation_id,
            "source policy native application identity differs"
        );
    }
    Ok(())
}

/// Selected retired backing only; no library scan and no legacy policy table.
pub(in crate::service::registry) fn retired_for_binding(
    db: &Connection,
    binding: airk_core::service::ProjectBinding,
) -> Result<Option<PolicySnapshot>> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 11 {
        return Ok(None);
    }
    super::validate_existing(db)?;
    if super::state(db)?.policy_contract == 0 {
        return Ok(None);
    }
    let id: Option<String> = db.query_row(
        "SELECT id FROM index_definitions WHERE legacy_binding=?1 AND active_registration=?1 AND tombstone=1",
        [binding.to_string()], |row| row.get(0),
    ).optional()?;
    id.map(|id| snapshot(db, id.parse()?))
        .transpose()
        .map(Option::flatten)
}

impl ServiceRegistry {
    /// A read-only exact-key lookup. Missing/pruned evidence is never proof of
    /// no effect; current policy state cannot authenticate a superseded retry.
    pub fn catalog_policy_resume_receipt(
        &self,
        id: IndexId,
        operation_id: Incarnation,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<(Option<airk_core::catalog_policy::PolicyResumeReceipt>, bool)> {
        let tx = self.connection.unchecked_transaction()?;
        super::validate_existing(&tx)?;
        let (job, expired) = self.request_receipt(caller, key)?;
        let Some(job) = job else {
            return Ok((None, expired));
        };
        let source: Option<String> = tx.query_row(
            "SELECT source_job FROM service_retry_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(), i64::try_from(key.epoch)?, key.nonce.to_string(), caller.to_string()],
            |row| row.get(0),
        ).optional()?;
        let source_job = source
            .context("policy receipt key belongs to a different native action")?
            .parse()?;
        let configuration = super::super::configuration::work(&tx, job.id)?
            .context("accepted policy retry configuration is not retained")?;
        configuration.request.validate()?;
        let ProjectConfiguration::ApplySourcePolicy {
            policy,
            operation_id: accepted_operation,
        } = &configuration.request.configuration
        else {
            anyhow::bail!("policy receipt key belongs to a different configuration");
        };
        ensure!(
            job.kind == JobKind::Configure
                && job.project == policy.owner_binding
                && policy.index_id == id
                && *accepted_operation == operation_id,
            "policy receipt differs from requested index or operation"
        );
        let result = super::super::configuration::result(&tx, job.id)?;
        if let Some(value) = &result {
            let report: SourcePolicyApplyReport = serde_json::from_value(value.clone())?;
            report.validate()?;
            ensure!(
                report.operation_id == operation_id && report.policy_digest == policy.identity()?,
                "policy receipt result differs from accepted configuration"
            );
        }
        Ok((
            Some(airk_core::catalog_policy::PolicyResumeReceipt {
                source_job,
                job,
                configuration,
                result,
            }),
            expired,
        ))
    }

    /// Existing native owner reconciliation only, never called by evidence or
    /// status reads. The finite pending journal is revisited on the next turn
    /// when the result reaches `limit`; completed rows leave this selection.
    pub fn catalog_reconcile_policy_completions(
        &mut self,
        limit: usize,
    ) -> Result<Vec<PolicySnapshot>> {
        ensure!(
            (1..=64).contains(&limit),
            "invalid policy completion reconciliation limit"
        );
        let version: u32 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 11 {
            return Ok(Vec::new());
        }
        super::validate_existing(&self.connection)?;
        if super::state(&self.connection)?.policy_contract == 0 {
            return Ok(Vec::new());
        }
        let mut statement=self.connection.prepare("SELECT p.index_id,json_extract(p.payload,'$.operation_id') FROM index_source_policies p JOIN service_jobs j ON j.id=json_extract(p.payload,'$.job') WHERE j.state='succeeded' AND EXISTS(SELECT 1 FROM service_configuration c WHERE c.job=j.id AND json_extract(c.payload,'$.request.configuration.operation')='apply_source_policy') AND (json_extract(p.payload,'$.application_job') IS NULL OR json_extract(p.payload,'$.application_job')<>json_extract(p.payload,'$.job')) ORDER BY p.index_id LIMIT ?1")?;
        let selected = statement
            .query_map([i64::try_from(limit)?], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        selected
            .into_iter()
            .map(|(id, operation)| self.catalog_record_policy_job(id.parse()?, operation.parse()?))
            .collect()
    }
    /// Deliberate recovery of the same policy operation. The new request key,
    /// exact configuration job and updated durable job binding commit together.
    pub fn catalog_resume_policy_job(
        &mut self,
        id: IndexId,
        operation_id: Incarnation,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<PolicySnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = resume_policy_job_in(&tx, id, operation_id, caller, key)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_policy_status(&self, id: IndexId) -> Result<Option<PolicySnapshot>> {
        let read = self.connection.unchecked_transaction()?;
        super::validate_existing(&read)?;
        let value = snapshot(&read, id)?;
        read.commit()?;
        Ok(value)
    }
    pub fn catalog_source_policy(&self, id: IndexId) -> Result<Option<SourcePolicy>> {
        Ok(self
            .catalog_policy_status(id)?
            .map(|snapshot| snapshot.policy))
    }
    /// Reconcile only the exact already accepted job. The caller does not supply
    /// a purported effect; native lease-checked result storage remains its owner.
    pub fn catalog_record_policy_job(
        &mut self,
        id: IndexId,
        operation_id: Incarnation,
    ) -> Result<PolicySnapshot> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::validate_existing(&tx)?;
        let mut current = snapshot(&tx, id)?.context("source policy unavailable")?;
        ensure!(
            current.operation_id == operation_id,
            "source policy operation changed"
        );
        if current
            .application
            .as_ref()
            .is_some_and(|application| !application.physical_cleanup_pending)
        {
            return Ok(current);
        }
        let job = super::super::jobs::record(&tx, current.job)?;
        ensure!(
            job.state == JobState::Succeeded,
            "source policy removal job is not complete"
        );
        let result = super::super::configuration::result(&tx, current.job)?
            .context("source policy native result unavailable")?;
        validate_result(&tx, current.job, &result)?;
        let report: SourcePolicyApplyReport = serde_json::from_value(result)?;
        if let Some(previous) = &current.application {
            ensure!(
                previous.removal_publication == report.removal_publication
                    && previous.no_index == report.no_index
                    && report.observed_unix_ms >= previous.observed_unix_ms
                    && previous.removed_artifacts == report.removed_artifacts
                    && previous.retired_components == report.retired_components
                    && report.reclaimed_files >= previous.reclaimed_files
                    && (previous.physical_cleanup_pending || !report.physical_cleanup_pending),
                "policy reclamation receipt regressed or changed its removal publication"
            );
            if let (Some(before), Some(after)) = (previous.reclaimed_bytes, report.reclaimed_bytes)
            {
                ensure!(after >= before, "policy reclamation byte counter regressed");
            }
        }
        current.application = Some(report);
        current.application_job = Some(current.job);
        current.last_error = None;
        write_snapshot(&tx, &current)?;
        tx.commit()?;
        Ok(current)
    }
}

pub(super) fn resume_policy_job_in(
    db: &Connection,
    id: IndexId,
    operation_id: Incarnation,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<PolicySnapshot> {
    super::validate_existing(db)?;
    let mut current = snapshot(db, id)?.context("source policy unavailable")?;
    ensure!(
        current.operation_id == operation_id,
        "source policy operation changed"
    );
    let accepted:Option<String>=db.query_row("SELECT job FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4", params![key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string(),caller.to_string()],|row|row.get(0)).optional()?;
    if let Some(accepted) = accepted {
        let retry_origin:Option<String>=db.query_row("SELECT source_job FROM service_retry_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",params![key.namespace.to_string(),i64::try_from(key.epoch)?,key.nonce.to_string(),caller.to_string()],|row|row.get(0)).optional()?;
        ensure!(
            retry_origin.is_some(),
            "policy retry key belongs to a different native action"
        );
        let work = super::super::configuration::work(db, accepted.parse()?)?
            .context("policy retry key belongs to another operation")?;
        ensure!(
            matches!(work.request.configuration,ProjectConfiguration::ApplySourcePolicy {ref policy,operation_id:accepted_operation} if accepted_operation==operation_id && policy.as_ref()==&current.policy),
            "policy retry key conflict"
        );
        return Ok(current);
    }
    ensure!(
        !index_record(db, id)?.tombstone,
        "retired index cannot admit source policy cleanup work"
    );
    let (namespace, epoch): (String, i64) = db.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        namespace == key.namespace.to_string() && u64::try_from(epoch)? == key.epoch,
        "policy retry request key expired"
    );
    let cleanup_pending = current
        .application
        .as_ref()
        .is_some_and(|application| application.physical_cleanup_pending);
    ensure!(
        current.application.is_none() || cleanup_pending,
        "source policy operation already completed"
    );
    let previous = super::super::jobs::record(db, current.job)?;
    ensure!(
        matches!(previous.state, JobState::Failed | JobState::Canceled)
            || cleanup_pending && previous.state == JobState::Succeeded,
        "source policy job is not retryable"
    );
    ensure!(
        require_claim(db, current.job)?,
        "source policy retry authority unavailable"
    );
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state IN('pending','running'))",[previous.project.to_string()],|row|row.get(0))?;
    ensure!(
        !busy,
        "source policy retry waits for accepted work to settle"
    );
    let work = super::super::configuration::work(db, current.job)?
        .context("accepted source policy configuration unavailable")?;
    ensure!(
        matches!(work.request.configuration,ProjectConfiguration::ApplySourcePolicy {ref policy,operation_id:accepted_operation} if accepted_operation==operation_id && policy.as_ref()==&current.policy),
        "source policy retry lineage changed"
    );
    super::super::jobs::require_request_capacity(db)?;
    let job = super::super::jobs::insert_new_job(
        db,
        previous.project,
        caller,
        key,
        JobKind::Configure,
        Some(&work),
    )?;
    super::super::jobs::insert_request_receipt(db, caller, key, job)?;
    db.execute(
        "INSERT INTO service_retry_requests VALUES(?1,?2,?3,?4,?5)",
        params![
            key.namespace.to_string(),
            epoch,
            key.nonce.to_string(),
            caller.to_string(),
            current.job.to_string()
        ],
    )?;
    current.job = job;
    current.last_error = None;
    write_snapshot(db, &current)?;
    Ok(current)
}
