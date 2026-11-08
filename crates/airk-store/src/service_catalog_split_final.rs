// SPDX-License-Identifier: Apache-2.0
//! Durable fixed-capacity native owner finalization, separate from source work.
use super::*;
use crate::service::collection::{Collection, NativeTerminalPublication, TerminalSlot};
const PLAN_KIND: &str = "catalog_split_final_plan";
const SLOT_KIND: &str = "catalog_split_final_slot";
const ERROR_KIND: &str = "catalog_split_final_error";
const ACK_KIND: &str = "catalog_split_final_ack";
#[cfg(test)]
#[path = "service_catalog_split_checkpoint_tests.rs"]
mod checkpoint_tests;

/// Classify immutable final-publication evidence only through its complete
/// native parent. A slot's retained `prepared` stage is not a running plan.
#[derive(Default)]
pub(crate) struct CheckpointReceiptClassifier {
    ack_operations: Option<BTreeMap<String, Incarnation>>,
}
impl CheckpointReceiptClassifier {
    pub(crate) fn terminal(
        &mut self,
        db: &Connection,
        id: &str,
        kind: &str,
        stage: &str,
        revision: i64,
        payload: &[u8],
    ) -> Result<bool> {
        if !matches!(kind, PLAN_KIND | SLOT_KIND | ACK_KIND) {
            return Ok(false);
        }
        let operation = match kind {
            PLAN_KIND => serde_json::from_slice::<SplitReceipt>(payload)?.operation_id,
            SLOT_KIND => serde_json::from_slice::<TerminalSlot>(payload)?.operation,
            ACK_KIND => {
                // Ack bytes contain an exact funding census, not a caller-supplied
                // operation. Resolve only its canonical native row identity.
                self.ack_operation(db, id)?
            }
            _ => unreachable!(),
        };
        let value =
            get(db, operation)?.context("checkpoint split evidence has no native parent")?;
        let state = NativeSplitState(value.clone());
        match kind {
            PLAN_KIND => {
                let terminal: SplitReceipt = serde_json::from_slice(payload)?;
                terminal.validate_for(&value.request)?;
                let (observed_revision, observed_stage, saved) =
                    plan(db, operation)?.context("checkpoint split plan missing")?;
                ensure!(
                    identity(operation, b"plan")?.to_string() == id
                        && observed_stage == stage
                        && i64::try_from(observed_revision)? == revision
                        && saved == terminal,
                    "checkpoint split plan identity changed"
                );
            }
            SLOT_KIND => {
                let slot: TerminalSlot = serde_json::from_slice(payload)?;
                slot.validate_for(&state)?;
                let digest = slot.identity()?;
                ensure!(
                    stage == "prepared"
                        && revision == 1
                        && slot_row(digest)?.to_string() == id
                        && value.receipt.terminal_publication_slots.get(&slot.index)
                            == Some(&digest),
                    "checkpoint split slot identity changed"
                );
            }
            ACK_KIND => {
                let acknowledged: BTreeSet<Sha256Digest> = serde_json::from_slice(payload)?;
                ensure!(
                    stage == "acknowledged"
                        && revision > 0
                        && value
                            .finalization
                            .as_ref()
                            .is_some_and(|status| status.funding_acknowledged == acknowledged),
                    "checkpoint split acknowledgment identity changed"
                );
            }
            _ => unreachable!(),
        }
        Ok(value.finalization.as_ref().is_some_and(|status| {
            status.progress == SplitFinalizationProgress::Complete && status.attention.is_none()
        }))
    }
    fn ack_operation(&mut self, db: &Connection, id: &str) -> Result<Incarnation> {
        if self.ack_operations.is_none() {
            let maximum = super::super::super::checkpoint::MAX_ROWS;
            let mut statement = db.prepare(&format!(
                "SELECT operation_id FROM index_split_operations ORDER BY operation_id LIMIT {}",
                maximum + 1
            ))?;
            let operations = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                operations.len() <= maximum,
                "checkpoint split roster exceeds capacity"
            );
            let mut mapping = BTreeMap::new();
            for operation in operations {
                let operation = operation.parse()?;
                ensure!(
                    mapping
                        .insert(identity(operation, b"ack")?.to_string(), operation)
                        .is_none(),
                    "checkpoint split ack ambiguous"
                );
            }
            self.ack_operations = Some(mapping);
        }
        Ok(*self
            .ack_operations
            .as_ref()
            .and_then(|mapping| mapping.get(id))
            .context("checkpoint split ack has no native parent")?)
    }
}
fn identity(operation: Incarnation, part: &[u8]) -> Result<Incarnation> {
    framed_digest(b"airk/split-final-record/1", &[operation.as_bytes(), part]).to_string()[..32]
        .parse()
}
fn slot_row(digest: Sha256Digest) -> Result<Incarnation> {
    digest.to_string()[..32].parse()
}
fn plan(db: &Connection, operation: Incarnation) -> Result<Option<(u64, String, SplitReceipt)>> {
    let row:Option<(i64,String,Vec<u8>)>=db.query_row("SELECT revision,stage,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind=?2",params![identity(operation,b"plan")?.to_string(),PLAN_KIND],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    row.map(|(revision, stage, bytes)| {
        ensure!(
            bytes.len() <= MAX_RECEIPT_BYTES
                && matches!(
                    stage.as_str(),
                    "prepared" | "publishing" | "acknowledging" | "complete"
                ),
            "invalid bounded finalization plan"
        );
        Ok((
            u64::try_from(revision)?,
            stage,
            serde_json::from_slice(&bytes)?,
        ))
    })
    .transpose()
}
pub(super) fn observation(
    db: &Connection,
    value: &SplitSnapshot,
) -> Result<Option<SplitFinalizationStatus>> {
    let Some((revision, stage, terminal)) = plan(db, value.receipt.operation_id)? else {
        return Ok(None);
    };
    terminal.validate_for(&value.request)?;
    ensure!(
        terminal.operation_id == value.receipt.operation_id
            && terminal.terminal_publication_slots == value.receipt.terminal_publication_slots
            && terminal.funding_release_receipts == value.receipt.funding_release_receipts,
        "frozen split terminal plan lineage differs"
    );
    let bytes: Option<Vec<u8>> = db
        .query_row(
            "SELECT substr(payload,1,513) FROM index_operation_receipts WHERE id=?1 AND kind=?2",
            params![
                identity(value.receipt.operation_id, b"error")?.to_string(),
                ERROR_KIND
            ],
            |row| row.get(0),
        )
        .optional()?;
    let attention = bytes
        .map(|bytes| {
            ensure!(bytes.len() <= 512, "finalization attention exceeds bound");
            serde_json::from_slice::<String>(&bytes).map_err(Into::into)
        })
        .transpose()?;
    let progress = match stage.as_str() {
        "prepared" => SplitFinalizationProgress::Prepared,
        "publishing" => SplitFinalizationProgress::Publishing,
        "acknowledging" => SplitFinalizationProgress::Acknowledging,
        "complete" => SplitFinalizationProgress::Complete,
        _ => unreachable!(),
    };
    if matches!(
        progress,
        SplitFinalizationProgress::Prepared | SplitFinalizationProgress::Publishing
    ) {
        ensure!(
            terminal.previous_receipt_digest == Some(value.receipt_digest)
                && terminal.revision == value.receipt.revision.next()?,
            "frozen finalization predecessor changed"
        );
    }
    let acked: Option<Vec<u8>> = db
        .query_row(
            "SELECT substr(payload,1,513) FROM index_operation_receipts WHERE id=?1 AND kind=?2",
            params![
                identity(value.receipt.operation_id, b"ack")?.to_string(),
                ACK_KIND
            ],
            |row| row.get(0),
        )
        .optional()?;
    let funding_acknowledged = acked
        .map(|bytes| {
            ensure!(
                bytes.len() <= 512,
                "funding acknowledgment census exceeds bound"
            );
            serde_json::from_slice::<BTreeSet<Sha256Digest>>(&bytes).map_err(Into::into)
        })
        .transpose()?
        .unwrap_or_default();
    let status = SplitFinalizationStatus {
        schema: 1,
        revision,
        terminal_receipt_digest: terminal.identity(&value.request)?,
        terminal_receipt_revision: terminal.revision,
        slots: terminal.terminal_publication_slots.clone(),
        progress,
        attention,
        funding_acknowledged,
    };
    encoded_size(&status, MAX_SPLIT_FINALIZATION_BYTES)?;
    Ok(Some(status))
}
fn owners(value: &SplitSnapshot) -> Result<Vec<Collection>> {
    let mut result = Vec::new();
    for (original, registration) in [
        (true, &value.request.original_registration),
        (false, &value.request.destination_registration),
    ] {
        let path = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        crate::managed_collection::validate_ancestors(&path)?;
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure!(
                    matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence { .. }
                    ) && (original || value.receipt.destination_storage.is_none()),
                    "split terminal owned collection disappeared"
                );
                continue;
            }
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        let collection = Collection::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        match collection.required_split()? {
            Some(reference)
                if reference.operation_id == value.receipt.operation_id
                    && reference.request_digest == value.receipt.request_digest =>
            {
                result.push(collection)
            }
            _ => ensure!(
                original && value.receipt.stage < SplitStage::OriginalFenced,
                "split terminal owner is unexplained or belongs to another operation"
            ),
        }
    }
    Ok(result)
}
fn next_terminal(old: &SplitSnapshot) -> Result<SplitReceipt> {
    ensure!(
        old.receipt.physical_cleanup_authorization.is_some()
            && old.receipt.funding_release_receipts.len() == old.receipt.funding_receipts.len(),
        "split resource release remains pending"
    );
    let mut next = old.receipt.clone();
    next.cleanup_attention = None;
    if matches!(
        old.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: true
        }
    ) {
        next.state = SplitRunState::CanceledBeforeFence {
            cleanup_pending: false,
        };
        next.cancellation_cleanup_receipt = Some(framed_digest(
            b"airk/split-canceled-cleanup/1",
            &[
                old.receipt.request_digest.as_bytes(),
                &serde_json::to_vec(&old.receipt.funding_release_receipts)?,
                old.receipt
                    .physical_cleanup_authorization
                    .unwrap()
                    .as_bytes(),
            ],
        ));
    } else {
        ensure!(
            old.receipt.stage == SplitStage::CleanupPending,
            "split terminal phase differs"
        );
        next.stage = SplitStage::Complete;
        next.state = SplitRunState::Complete;
        next.cleanup_receipt = Some(framed_digest(
            b"airk/split-complete-cleanup/1",
            &[
                old.receipt.request_digest.as_bytes(),
                &serde_json::to_vec(&old.receipt.funding_release_receipts)?,
                old.receipt
                    .physical_cleanup_authorization
                    .unwrap()
                    .as_bytes(),
            ],
        ));
    }
    next.revision = old.receipt.revision.next()?;
    next.previous_receipt_digest = Some(old.receipt_digest);
    next.validate_for(&old.request)?;
    Ok(next)
}
fn saved_slot(
    db: &Connection,
    digest: Sha256Digest,
    state: &NativeSplitState,
) -> Result<TerminalSlot> {
    let bytes: Vec<u8> = db.query_row(
        "SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind=?2",
        params![slot_row(digest)?.to_string(), SLOT_KIND],
        |row| row.get(0),
    )?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "terminal slot intent exceeds bound"
    );
    let slot: TerminalSlot = serde_json::from_slice(&bytes)?;
    slot.validate_for(state)?;
    ensure!(
        slot.identity()? == digest,
        "terminal slot native allocation identity differs"
    );
    Ok(slot)
}
impl ServiceRegistry {
    /// A bounded metadata action: one slot, one release, or at most two owner
    /// publications followed by their actual durable terminal receipt.
    pub fn catalog_advance_split_finalization(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
    ) -> Result<SplitSnapshot> {
        let old = self
            .catalog_split_status(operation)?
            .context("split operation not retained")?;
        ensure!(
            old.receipt_digest == expected,
            "split finalization review stale"
        );
        if old.receipt.stage == SplitStage::Complete
            || matches!(
                old.receipt.state,
                SplitRunState::CanceledBeforeFence {
                    cleanup_pending: false
                }
            )
        {
            return self.catalog_acknowledge_split_finalization(old);
        }
        ensure!(
            old.receipt.physical_cleanup_authorization.is_some()
                && old.receipt.cleanup_attention.is_none(),
            "split native cleanup/attention prevents finalization"
        );
        ensure!(
            old.finalization
                .as_ref()
                .is_none_or(|state| state.attention.is_none()),
            "split finalization requires explicit exact resume"
        );
        // Retire only the never-activated canceled destination before freezing
        // the terminal receipt. Later final publication changes no catalog scope.
        if old.finalization.is_none()
            && matches!(
                old.receipt.state,
                SplitRunState::CanceledBeforeFence {
                    cleanup_pending: true
                }
            )
        {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = exact(&tx, operation, expected)?;
            let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM index_split_jobs s JOIN service_jobs j ON j.id=s.job WHERE s.operation_id=?1 AND j.state IN('pending','running'))",[operation.to_string()],|row|row.get(0))?;
            ensure!(
                !active,
                "split accepted work must settle before canceled finalization"
            );
            let memberships: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM project_index_memberships WHERE index_id=?1)",
                [current.request.destination.index_id.to_string()],
                |row| row.get(0),
            )?;
            ensure!(
                !memberships,
                "canceled staged destination has unexpected Project membership"
            );
            let changed=tx.execute("UPDATE index_definitions SET tombstone=1,active_registration=NULL,definition_revision=definition_revision+1 WHERE id=?1 AND tombstone=0 AND definition_revision<9223372036854775807",[current.request.destination.index_id.to_string()])?;
            if changed > 0 {
                tx.execute(
                    "UPDATE index_sources SET enabled=0 WHERE index_id=?1",
                    [current.request.destination.index_id.to_string()],
                )?;
                tx.execute(
                    "UPDATE projects SET registered=0,paused=1 WHERE binding=?1",
                    [current.request.destination_registration.binding.to_string()],
                )?;
                tx.execute(
                    "UPDATE service_watch SET enabled=0,backend='disabled' WHERE project=?1",
                    [current.request.destination_registration.binding.to_string()],
                )?;
                let revision = super::super::state(&tx)?.revision.next()?;
                tx.execute(
                    "UPDATE index_catalog_meta SET revision=?1 WHERE id=1",
                    [number(revision)],
                )?;
                let mut next = current.receipt.clone();
                next.catalog_revision = revision;
                let value = replace(&tx, &current, next)?;
                tx.commit()?;
                return Ok(value);
            }
        }
        let collections = owners(&old)?;
        if old.finalization.is_none() {
            for collection in &collections {
                let reference = collection
                    .required_split()?
                    .context("terminal participant marker missing")?;
                if old
                    .receipt
                    .terminal_publication_slots
                    .contains_key(&reference.index_id)
                {
                    continue;
                }
                let registration = if reference.index_id == old.request.original_index {
                    &old.request.original_registration
                } else {
                    &old.request.destination_registration
                };
                let quota = crate::quota::Quota::open_service(Path::new(&registration.data_root))?;
                let funding = quota.observe_funding(operation)?;
                let (slot, updated) = collection
                    .prepare_split_terminal_slot(&NativeSplitState(old.clone()), &funding)?;
                updated.validate_for(&old.request)?;
                quota.validate_funding(&updated)?;
                let digest = slot.identity()?;
                let bytes = serde_json::to_vec(&slot)?;
                ensure!(
                    bytes.len() <= MAX_RECEIPT_BYTES,
                    "terminal slot intent exceeds bound"
                );
                let tx = self
                    .connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)?;
                let current = exact(&tx, operation, expected)?;
                tx.execute(
                    "INSERT INTO index_operation_receipts VALUES(?1,?2,'prepared',1,?3)",
                    params![slot_row(digest)?.to_string(), SLOT_KIND, bytes],
                )?;
                let mut next = current.receipt.clone();
                next.terminal_publication_slots.insert(slot.index, digest);
                let value = replace(&tx, &current, next)?;
                tx.commit()?;
                return Ok(value);
            }
            for root in old.receipt.funding_receipts.keys() {
                if old.receipt.funding_release_receipts.contains_key(root) {
                    continue;
                }
                let quota = crate::quota::Quota::open_service(Path::new(root))?;
                let funding = quota.observe_funding(operation)?;
                let permit = self.catalog_split_funding_terminal(operation)?;
                let released = quota.finish_funding(&funding, &permit)?;
                return self.catalog_record_split_funding_release(operation, expected, &released);
            }
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = exact(&tx, operation, expected)?;
            let terminal = next_terminal(&current)?;
            tx.execute(
                "INSERT INTO index_operation_receipts VALUES(?1,?2,'prepared',1,?3)",
                params![
                    identity(operation, b"plan")?.to_string(),
                    PLAN_KIND,
                    serde_json::to_vec(&terminal)?
                ],
            )?;
            let value = get(&tx, operation)?.context("split terminal freeze disappeared")?;
            tx.commit()?;
            return Ok(value);
        }
        let (_, _, terminal) =
            plan(&self.connection, operation)?.context("frozen split terminal plan disappeared")?;
        ensure!(
            terminal.previous_receipt_digest == Some(expected),
            "frozen terminal plan predecessor differs"
        );
        let target = SplitSnapshot {
            request: old.request.clone(),
            baseline: old.baseline.clone(),
            plan: old.plan.clone(),
            receipt_digest: terminal.identity(&old.request)?,
            receipt: terminal,
            finalization: None,
        };
        target.validate()?;
        {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            exact(&tx, operation, expected)?;
            tx.execute("UPDATE index_operation_receipts SET stage='publishing',revision=revision+1 WHERE id=?1 AND kind=?2 AND revision<9223372036854775807",params![identity(operation,b"plan")?.to_string(),PLAN_KIND])?;
            tx.commit()?;
        }
        let mut proofs: Vec<NativeTerminalPublication> = Vec::new();
        for collection in &collections {
            let reference = collection
                .required_split()?
                .context("terminal owner marker disappeared")?;
            let digest = *target
                .receipt
                .terminal_publication_slots
                .get(&reference.index_id)
                .context("terminal owner capacity was not prepared")?;
            let slot = saved_slot(&self.connection, digest, &NativeSplitState(target.clone()))?;
            proofs.push(
                collection.publish_split_terminal_slot(&NativeSplitState(target.clone()), &slot)?,
            );
        }
        ensure!(
            proofs.len() == target.receipt.terminal_publication_slots.len(),
            "terminal publication participant census changed"
        );
        for proof in &proofs {
            ensure!(
                proof.receipt == target.receipt_digest,
                "terminal owner receipt differs"
            );
            proof.revalidate()?;
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = exact(&tx, operation, expected)?;
        let (_, _, saved) = plan(&tx, operation)?.context("frozen terminal receipt missing")?;
        ensure!(saved == target.receipt, "frozen terminal receipt changed");
        let bytes = serde_json::to_vec(&target.receipt)?;
        let charge = bytes
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(256))
            .context("terminal receipt charge overflow")?;
        let remaining: i64 = tx.query_row(
            "SELECT length(completion_reserve) FROM index_split_operations WHERE operation_id=?1",
            [operation.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            remaining >= i64::try_from(charge)?,
            "reserved terminal catalog capacity exhausted"
        );
        tx.execute(
            "INSERT INTO index_split_history VALUES(?1,?2,?3,?4)",
            params![
                operation.to_string(),
                number(target.receipt.revision),
                target.receipt_digest.to_string(),
                bytes
            ],
        )?;
        ensure!(tx.execute("UPDATE index_split_operations SET revision=?1,receipt_digest=?2,payload=?3,completion_reserve=zeroblob(0) WHERE operation_id=?4 AND receipt_digest=?5",params![number(target.receipt.revision),target.receipt_digest.to_string(),serde_json::to_vec(&target)?,operation.to_string(),current.receipt_digest.to_string()])?==1,"terminal split CAS changed");
        // Native current credit is already released, but retained funding
        // acknowledgments remain explicit work. Keep participants write-fenced.
        tx.execute("UPDATE index_operation_receipts SET stage='acknowledging',revision=revision+1 WHERE id=?1 AND kind=?2 AND revision<9223372036854775807",params![identity(operation,b"plan")?.to_string(),PLAN_KIND])?;
        tx.execute(
            "DELETE FROM index_operation_receipts WHERE id=?1 AND kind=?2",
            params![identity(operation, b"error")?.to_string(), ERROR_KIND],
        )?;
        let value = get(&tx, operation)?.context("terminal split receipt disappeared")?;
        tx.commit()?;
        drop(proofs);
        Ok(value)
    }
    fn catalog_acknowledge_split_finalization(
        &mut self,
        old: SplitSnapshot,
    ) -> Result<SplitSnapshot> {
        let state = old
            .finalization
            .as_ref()
            .context("terminal split lacks native finalization plan")?;
        if state.progress == SplitFinalizationProgress::Complete {
            return Ok(old);
        }
        ensure!(
            state.progress == SplitFinalizationProgress::Acknowledging && state.attention.is_none(),
            "terminal funding acknowledgment requires exact resume"
        );
        let operation = old.receipt.operation_id;
        for (root, release) in &old.receipt.funding_release_receipts {
            if state.funding_acknowledged.contains(release) {
                continue;
            }
            let permit = self.catalog_split_funding_terminal(operation)?;
            crate::quota::Quota::acknowledge_service_funding(Path::new(root), operation, &permit)?;
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = exact(&tx, operation, old.receipt_digest)?;
            let observed = current
                .finalization
                .as_ref()
                .context("funding ack plan disappeared")?;
            ensure!(
                observed.revision == state.revision && observed.attention.is_none(),
                "funding ack metadata review changed"
            );
            let mut acked = observed.funding_acknowledged.clone();
            acked.insert(*release);
            tx.execute("INSERT INTO index_operation_receipts VALUES(?1,?2,'acknowledged',?3,?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,payload=excluded.payload WHERE kind=excluded.kind",params![identity(operation,b"ack")?.to_string(),ACK_KIND,i64::try_from(state.revision)?,serde_json::to_vec(&acked)?])?;
            tx.execute("UPDATE index_operation_receipts SET revision=revision+1 WHERE id=?1 AND kind=?2 AND revision=?3 AND revision<9223372036854775807",params![identity(operation,b"plan")?.to_string(),PLAN_KIND,i64::try_from(state.revision)?])?;
            let value =
                get(&tx, operation)?.context("funding acknowledgment receipt disappeared")?;
            tx.commit()?;
            return Ok(value);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = exact(&tx, operation, old.receipt_digest)?;
        ensure!(
            current.finalization.as_ref().is_some_and(
                |latest| latest.revision == state.revision && latest.attention.is_none()
            ),
            "funding ack completion review changed"
        );
        tx.execute(
            "UPDATE index_split_participants SET active=0 WHERE operation_id=?1",
            [operation.to_string()],
        )?;
        tx.execute("UPDATE index_operation_receipts SET stage='complete',revision=revision+1 WHERE id=?1 AND kind=?2 AND revision=?3 AND revision<9223372036854775807",params![identity(operation,b"plan")?.to_string(),PLAN_KIND,i64::try_from(state.revision)?])?;
        let value = get(&tx, operation)?.context("terminal acknowledgment receipt disappeared")?;
        tx.commit()?;
        Ok(value)
    }
    pub fn catalog_resume_split_finalization(
        &mut self,
        request: &SplitRecoveryRequest,
    ) -> Result<SplitRecoveryReceipt> {
        if let Some(saved) = self.catalog_split_recovery_receipt(request.caller, &request.key)? {
            ensure!(saved.request == *request, "split finalization key conflict");
            return Ok(saved);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        key_current(&tx, &request.key)?;
        require_unused_action_key(&tx, request.caller, &request.key, None)?;
        let state = old
            .finalization
            .as_ref()
            .context("split finalization plan is not frozen")?;
        ensure!(
            request.action == SplitRecoveryAction::Resume
                && state.progress != SplitFinalizationProgress::Complete
                && request.expected_finalization_revision == Some(state.revision),
            "split finalization attention review stale"
        );
        let (_, _, terminal) =
            plan(&tx, request.operation_id)?.context("frozen split terminal plan missing")?;
        let revision = state
            .revision
            .checked_add(1)
            .context("split finalization revision overflow")?;
        ensure!(tx.execute("UPDATE index_operation_receipts SET revision=?1 WHERE id=?2 AND kind=?3 AND revision=?4",params![i64::try_from(revision)?,identity(request.operation_id,b"plan")?.to_string(),PLAN_KIND,i64::try_from(state.revision)?])?==1,"split finalization CAS changed");
        tx.execute(
            "DELETE FROM index_operation_receipts WHERE id=?1 AND kind=?2",
            params![
                identity(request.operation_id, b"error")?.to_string(),
                ERROR_KIND
            ],
        )?;
        let receipt = save_action_ack(
            &tx,
            request,
            &old,
            SplitRecoveryOutcome::FinalizationAccepted {
                receipt_digest: old.receipt_digest,
                terminal_receipt_digest: terminal.identity(&old.request)?,
                terminal_receipt: Box::new(terminal),
                finalization_revision: revision,
            },
        )?;
        tx.commit()?;
        Ok(receipt)
    }
}
pub(super) fn latch(
    db: &Connection,
    old: &SplitSnapshot,
    code: &str,
) -> Result<Option<SplitSnapshot>> {
    let Some(state) = &old.finalization else {
        return Ok(None);
    };
    ensure!(
        state.progress != SplitFinalizationProgress::Complete,
        "terminal split cannot fail again"
    );
    ensure!(
        !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "invalid finalization attention code"
    );
    let revision = state
        .revision
        .checked_add(1)
        .context("finalization revision overflow")?;
    db.execute(
        "UPDATE index_operation_receipts SET revision=?1 WHERE id=?2 AND kind=?3 AND revision=?4",
        params![
            i64::try_from(revision)?,
            identity(old.receipt.operation_id, b"plan")?.to_string(),
            PLAN_KIND,
            i64::try_from(state.revision)?
        ],
    )?;
    db.execute("INSERT INTO index_operation_receipts VALUES(?1,?2,'attention',?3,?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,payload=excluded.payload WHERE kind=excluded.kind",params![identity(old.receipt.operation_id,b"error")?.to_string(),ERROR_KIND,i64::try_from(revision)?,serde_json::to_vec(code)?])?;
    get(db, old.receipt.operation_id)
}
