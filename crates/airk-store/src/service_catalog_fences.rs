// SPDX-License-Identifier: Apache-2.0
//! Native durable catalog authority fences. Store owns transactions and opaque
//! absence observations; Core wire validation alone never authenticates workers.
use super::*;
pub use airk_core::catalog_fences::{
    CatalogFenceProof, FenceConfirmation, FenceConfirmationMethod, FenceIntent, FenceObservation,
    FencePlan, FencePreparation, FencePrepareRejected, FenceStage, FenceTarget, FencedReceipt,
    WorkerDrainObligation, WorkerFenceConfirmation, WorkerFenceConfirmationMethod,
    WorkerProviderHistory, WorkerProviderObligation,
};
use airk_core::catalog_fences::{validate_confirmations, validate_worker_confirmations};

const KIND: &str = "catalog_management_fenced";

#[cfg(test)]
#[path = "service_catalog_fence_tests.rs"]
mod tests;

/// A native read snapshot proving this exact non-evicting operation key has no
/// receipt and the reviewed catalog is unchanged. This is intentionally not a
/// wire type: deserializing JSON must never construct native absence authority.
#[derive(Clone, Debug)]
pub struct CatalogFenceAbsence {
    caller: Incarnation,
    key: RequestKey,
    request_digest: Sha256Digest,
    state: super::super::CatalogState,
}
impl CatalogFenceAbsence {
    pub fn validate_for(&self, request: &CatalogRequest) -> Result<()> {
        ensure!(
            self.caller == request.caller
                && self.key == request.key
                && self.request_digest == request.identity()?
                && self.state.namespace == request.key.namespace
                && self.state.revision == request.expected_catalog_revision,
            "catalog exact absence observation changed"
        );
        Ok(())
    }
    pub fn catalog_state(&self) -> &super::super::CatalogState {
        &self.state
    }
}

/// Exact unprepared-operation absence observed at a current catalog clock.
/// Independent native completion may advance that clock; this opaque read
/// never authorizes a stale request or establishes absence of native effects.
#[derive(Clone, Debug)]
pub struct CatalogFenceUnpreparedAbsence {
    caller: Incarnation,
    key: RequestKey,
    request_digest: Sha256Digest,
    state: super::super::CatalogState,
}
impl CatalogFenceUnpreparedAbsence {
    pub fn validate_for(&self, request: &CatalogRequest) -> Result<()> {
        ensure!(
            self.caller == request.caller
                && self.key == request.key
                && self.request_digest == request.identity()?
                && self.state.namespace == request.key.namespace
                && self.state.revision >= request.expected_catalog_revision
                && self.state.activated
                && self.state.authority_contract == 1
                && self.state.policy_contract <= super::super::SUPPORTED_POLICY_CONTRACT,
            "catalog unprepared absence observation changed"
        );
        Ok(())
    }
    pub fn catalog_state(&self) -> &super::super::CatalogState {
        &self.state
    }
}

/// Admission-free native diagnostic inspection. A metadata receipt which still
/// requires an authority fence is not evidence of completed native fencing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogSavedObservation {
    Fence(Box<FenceObservation>),
    Metadata(CatalogReceipt),
}

pub(super) fn observation(
    db: &Connection,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<Option<FenceObservation>> {
    let row: Option<(String, i64, Vec<u8>)> = db.query_row("SELECT stage,revision,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind=?2", params![operation_id(caller,key)?.to_string(),KIND], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    let Some((stage, revision, bytes)) = row else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "catalog fence receipt exceeds bound"
    );
    let result = if stage == "metadata_applied" {
        let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
        receipt.validate()?;
        ensure!(
            receipt.metadata.caller == caller
                && receipt.metadata.key == *key
                && number(receipt.metadata.catalog_revision) == revision,
            "catalog fence receipt owner conflict"
        );
        FenceObservation::Applied(receipt)
    } else {
        let intent: FenceIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        ensure!(
            intent.request.caller == caller
                && intent.request.key == *key
                && number(intent.request.expected_catalog_revision) == revision
                && stage == intent.stage.as_str(),
            "catalog fence intent owner/stage conflict"
        );
        FenceObservation::Pending(intent)
    };
    Ok(Some(result))
}
pub(super) fn require_no_pending(db: &Connection) -> Result<()> {
    let pending: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE kind=?1 AND stage<>'metadata_applied')", [KIND], |row| row.get(0))?;
    ensure!(
        !pending,
        "catalog mutation blocked by pending native authority fence"
    );
    Ok(())
}
fn require_exact_pending(db: &Connection, id: Incarnation) -> Result<()> {
    let (count,first):(i64,String) = db.query_row("SELECT count(*),coalesce(min(id),'') FROM index_operation_receipts WHERE kind=?1 AND stage<>'metadata_applied'",[KIND],|row|Ok((row.get(0)?,row.get(1)?)))?;
    ensure!(
        count == 1 && first == id.to_string(),
        "catalog pending fence set changed"
    );
    Ok(())
}
fn replace_intent(db: &Connection, before: &FenceIntent, after: &FenceIntent) -> Result<()> {
    after.validate()?;
    changed(db.execute("UPDATE index_operation_receipts SET stage=?1,payload=?2 WHERE id=?3 AND kind=?4 AND stage=?5 AND payload=?6", params![after.stage.as_str(),serde_json::to_vec(after)?,after.operation_id.to_string(),KIND,before.stage.as_str(),serde_json::to_vec(before)?])?)
}
fn confirmed_stage(intent: &FenceIntent) -> FenceStage {
    if intent.confirmations.len() == intent.plan.targets.len()
        && intent.worker_confirmations.len()
            == intent
                .plan
                .worker_obligation
                .as_ref()
                .map_or(0, |o| o.workers.len())
    {
        FenceStage::Fenced
    } else {
        FenceStage::FenceUnknown
    }
}
impl ServiceRegistry {
    /// Inspect the complete durable lane before reasoning about a retained
    /// never-prepared request. Old epochs may be observed, never newly admitted.
    /// Only a successful ANY-kind receipt lookup can establish exact absence;
    /// unsupported kinds, corrupt rows and database errors remain errors.
    pub fn catalog_saved_observation(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<CatalogSavedObservation>> {
        let tx = self.connection.unchecked_transaction()?;
        let current = ready(&tx)?;
        ensure!(
            current.namespace == key.namespace,
            "catalog saved observation namespace changed"
        );
        let kind: Option<String> = tx
            .query_row(
                "SELECT substr(kind,1,65) FROM index_operation_receipts WHERE id=?1",
                [operation_id(caller, key)?.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        match kind.as_deref() {
            None => Ok(None),
            Some(KIND) => Ok(Some(CatalogSavedObservation::Fence(Box::new(
                observation(&tx, caller, key)?.context("catalog fence receipt disappeared")?,
            )))),
            Some("catalog_management") => Ok(Some(CatalogSavedObservation::Metadata(
                saved(&tx, caller, key)?.context("catalog metadata receipt disappeared")?,
            ))),
            Some(_) => anyhow::bail!("catalog operation key belongs to another receipt kind"),
        }
    }
    /// Native reconciliation only. This checks exact receipt absence without
    /// admitting an expired request key or advancing its epoch. The accepted
    /// metadata actor must separately retain its closed gate and prove no native
    /// effects; neither a generic error nor this snapshot alone reopens issuance.
    pub fn catalog_fence_absence(&self, request: &CatalogRequest) -> Result<CatalogFenceAbsence> {
        request.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        let current = ready(&tx)?;
        ensure!(
            current.namespace == request.key.namespace
                && current.revision == request.expected_catalog_revision,
            "catalog preparation observation changed"
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1)",
            [operation_id(request.caller, &request.key)?.to_string()],
            |row| row.get(0),
        )?;
        ensure!(!exists, "catalog preparation receipt is not absent");
        require_no_pending(&tx)?;
        // An independent setup is not this operation's authority fence. Its
        // index remains unavailable for grants until ready, while exact absence
        // may safely reopen unaffected issuance after a zero-effect rejection.
        Ok(CatalogFenceAbsence {
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            state: current,
        })
    }
    /// Native reconciliation read for a never-prepared exact request whose
    /// reviewed clock may precede an independent native completion. Bypass of
    /// epoch admission applies only to this read; no request, receipt or epoch
    /// is written, and the native coordinator must retain its closed gate.
    pub fn catalog_fence_unprepared_absence(
        &self,
        request: &CatalogRequest,
    ) -> Result<CatalogFenceUnpreparedAbsence> {
        request.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        let current = ready(&tx)?;
        ensure!(
            current.namespace == request.key.namespace
                && current.revision >= request.expected_catalog_revision
                && current.authority_contract == 1
                && current.policy_contract <= super::super::SUPPORTED_POLICY_CONTRACT,
            "catalog unprepared absence namespace, revision or contract changed"
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1)",
            [operation_id(request.caller, &request.key)?.to_string()],
            |row| row.get(0),
        )?;
        ensure!(!exists, "catalog preparation receipt is not absent");
        require_no_pending(&tx)?;
        Ok(CatalogFenceUnpreparedAbsence {
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            state: current,
        })
    }
    /// Native-only admission. The coordinator must already have closed issuance
    /// and captured its complete exact-worker target census, including installs.
    pub fn catalog_prepare_fence(
        &mut self,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        plan: &FencePlan,
    ) -> Result<FencePreparation> {
        request.validate()?;
        plan.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx)?;
        if let Some(saved) = observation(&tx, request.caller, &request.key)? {
            return match saved {
                FenceObservation::Applied(receipt) => {
                    ensure!(
                        receipt.metadata.request_digest == request.identity()?
                            && receipt.preview_digest == preview_digest,
                        "catalog request key conflict"
                    );
                    Ok(FencePreparation::Replay(receipt))
                }
                FenceObservation::Pending(intent) => {
                    require_exact_pending(&tx, intent.operation_id)?;
                    ensure!(
                        intent.request == *request
                            && intent.preview_digest == preview_digest
                            && intent.plan == *plan,
                        "catalog request key conflict"
                    );
                    Ok(FencePreparation::Prepared(intent))
                }
            };
        }
        if let Some(receipt) = saved(&tx, request.caller, &request.key)? {
            ensure!(
                receipt.request_digest == request.identity()?,
                "catalog request key conflict"
            );
            anyhow::bail!("metadata-only catalog receipt has no verified native fence");
        }
        require_no_pending(&tx)?;
        plan.validate_for(request)?;
        if let Some(obligation) = &plan.worker_obligation {
            let CatalogItem::Index(index) =
                item(&tx, CatalogKind::Indexes, &obligation.index_id.to_string())?
            else {
                anyhow::bail!("catalog worker obligation index unavailable");
            };
            ensure!(
                index.backing.binding == obligation.backing,
                "catalog worker obligation backing changed"
            );
        }
        // This known rejection is safe only before the durable fence exists.
        // Reserved-apply failures must retain their outcome-unknown semantics.
        let reviewed = preview(&tx, request, &self.root).map_err(|error| {
            if error.is::<airk_core::catalog_source_change::SourceChangeBusy>() {
                error.context(FencePrepareRejected::SourceBusy)
            } else {
                error
            }
        })?;
        ensure!(
            reviewed.authority_fence_required,
            "catalog native fence unnecessary"
        );
        ensure!(
            reviewed.identity()? == preview_digest,
            FencePrepareRejected::StaleReview
        );
        let mut intent = FenceIntent {
            schema: 1,
            operation_id: operation_id(request.caller, &request.key)?,
            request: request.clone(),
            preview_digest,
            prepared_digest: Sha256Digest::from_bytes([0; 32]),
            plan: plan.clone(),
            stage: FenceStage::Prepared,
            confirmations: Vec::new(),
            worker_confirmations: Vec::new(),
        };
        intent.prepared_digest = intent.identity()?;
        ensure!(
            serde_json::to_vec(&intent)?.len() <= MAX_RECEIPT_BYTES,
            FencePrepareRejected::Capacity
        );
        intent.validate()?;
        // Reserve the complete longest-method confirmation state, retaining
        // every captured provider helper generation in its obligation, BEFORE
        // worker side effects; never discover a full receipt after revocation.
        let mut completed = intent.clone();
        completed.stage = FenceStage::Fenced;
        completed.confirmations = completed
            .plan
            .targets
            .iter()
            .map(|target| FenceConfirmation {
                target: target.clone(),
                method: FenceConfirmationMethod::NativeWorkerReap,
            })
            .collect();
        completed.worker_confirmations = completed
            .plan
            .worker_obligation
            .as_ref()
            .map(|obligation| {
                obligation
                    .workers
                    .iter()
                    .map(|worker| WorkerFenceConfirmation {
                        worker: *worker,
                        method: WorkerFenceConfirmationMethod::NativeWorkerReap,
                    })
                    .collect()
            })
            .unwrap_or_default();
        ensure!(
            serde_json::to_vec(&completed)?.len() <= MAX_RECEIPT_BYTES,
            FencePrepareRejected::Capacity
        );
        completed.validate()?;
        let terminal = FencedReceipt {
            request: request.clone(),
            metadata: CatalogReceipt {
                schema: 1,
                operation_id: intent.operation_id,
                caller: request.caller,
                key: request.key.clone(),
                request_digest: request.identity()?,
                catalog_revision: reviewed.next_revision,
                authority_fence_required: true,
            },
            preview_digest,
            prepared_digest: intent.prepared_digest,
            proof: CatalogFenceProof::from_intent(&completed)?,
        };
        ensure!(
            serde_json::to_vec(&terminal)?.len() <= MAX_RECEIPT_BYTES,
            FencePrepareRejected::Capacity
        );
        terminal.validate()?;
        super::super::source_change::reserve_completion_slot(&tx, request)?;
        tx.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,?2,'prepared',?3,?4)",
            params![
                intent.operation_id.to_string(),
                KIND,
                number(request.expected_catalog_revision),
                serde_json::to_vec(&intent)?
            ],
        )?;
        tx.commit()?;
        Ok(FencePreparation::Prepared(intent))
    }
    /// Reconciliation read preserves prepared/fenced/unknown stages; it never
    /// mistakes an intent for successful metadata or repeats a worker command.
    pub fn catalog_fence_observation(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<FenceObservation>> {
        ready(&self.connection)?;
        let result = observation(&self.connection, caller, key)?;
        if result.is_none() {
            current_key(&self.connection, key)?;
        }
        Ok(result)
    }
    /// At most one pending mutation is admitted. Startup uses this bounded read
    /// before exposing Health/Connect; none of these APIs starts normal admission.
    pub fn catalog_pending_fence(&self) -> Result<Option<FenceIntent>> {
        ready(&self.connection)?;
        let mut stmt = self.connection.prepare("SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE kind=?1 AND stage<>'metadata_applied' ORDER BY id LIMIT 2")?;
        let rows = stmt
            .query_map([KIND], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(rows.len() <= 1, "multiple pending catalog fences");
        let Some(bytes) = rows.into_iter().next() else {
            return Ok(None);
        };
        ensure!(
            bytes.len() <= MAX_RECEIPT_BYTES,
            "catalog fence intent exceeds bound"
        );
        let intent: FenceIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        let observed = observation(&self.connection, intent.request.caller, &intent.request.key)?;
        ensure!(
            observed == Some(FenceObservation::Pending(intent.clone())),
            "catalog pending fence identity conflict"
        );
        Ok(Some(intent))
    }
    /// Native coordinator records exact scope acknowledgements or observed native
    /// worker reaping. A timeout/EOF is NOT either proof; retain partial progress.
    pub fn catalog_record_fence_confirmations(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        prepared_digest: Sha256Digest,
        confirmations: &[FenceConfirmation],
    ) -> Result<FenceIntent> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx)?;
        let before = match observation(&tx, caller, key)? {
            Some(FenceObservation::Pending(intent)) => intent,
            _ => anyhow::bail!("catalog pending fence unavailable"),
        };
        ensure!(
            before.prepared_digest == prepared_digest,
            "catalog prepared fence changed"
        );
        require_exact_pending(&tx, before.operation_id)?;
        validate_confirmations(&before.plan, confirmations)?;
        ensure!(
            before
                .confirmations
                .iter()
                .all(|old| confirmations.iter().any(|new| new.target == old.target)),
            "catalog fence confirmations cannot regress"
        );
        let mut after = before.clone();
        after.confirmations = confirmations
            .iter()
            .map(|confirmation| {
                // A later process reap may confirm the same target already fenced by
                // an acknowledged scope command. Preserve the first actual method.
                before
                    .confirmations
                    .iter()
                    .find(|old| old.target == confirmation.target)
                    .unwrap_or(confirmation)
                    .clone()
            })
            .collect();
        after.stage = confirmed_stage(&after);
        replace_intent(&tx, &before, &after)?;
        tx.commit()?;
        Ok(after)
    }
    /// Native coordinator records actual mutating-worker reaps independently
    /// from scope revocation. Caller cancellation, EOF or a scope ACK is no reap.
    pub fn catalog_record_worker_fence_confirmations(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        prepared_digest: Sha256Digest,
        confirmations: &[WorkerFenceConfirmation],
    ) -> Result<FenceIntent> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx)?;
        let before = match observation(&tx, caller, key)? {
            Some(FenceObservation::Pending(intent)) => intent,
            _ => anyhow::bail!("catalog pending fence unavailable"),
        };
        ensure!(
            before.prepared_digest == prepared_digest,
            "catalog prepared fence changed"
        );
        require_exact_pending(&tx, before.operation_id)?;
        validate_worker_confirmations(&before.plan, confirmations)?;
        ensure!(
            before
                .worker_confirmations
                .iter()
                .all(|old| confirmations.iter().any(|new| new.worker == old.worker)),
            "catalog worker confirmations cannot regress"
        );
        let mut after = before.clone();
        after.worker_confirmations = confirmations
            .iter()
            .map(|confirmation| {
                before
                    .worker_confirmations
                    .iter()
                    .find(|old| old.worker == confirmation.worker)
                    .unwrap_or(confirmation)
                    .clone()
            })
            .collect();
        after.stage = confirmed_stage(&after);
        replace_intent(&tx, &before, &after)?;
        tx.commit()?;
        Ok(after)
    }
    /// Commit only after every captured exact-worker target has a confirmation.
    /// Metadata and terminal proof replace the one reserved receipt atomically.
    pub fn catalog_apply_fenced(
        &mut self,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
    ) -> Result<FencedReceipt> {
        request.validate()?;
        self.catalog_split_seal_pending_original(request)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx)?;
        let intent = match observation(&tx, request.caller, &request.key)? {
            Some(FenceObservation::Applied(receipt)) => {
                ensure!(
                    receipt.metadata.request_digest == request.identity()?
                        && receipt.preview_digest == preview_digest
                        && receipt.prepared_digest == prepared_digest,
                    "catalog request key conflict"
                );
                return Ok(receipt);
            }
            Some(FenceObservation::Pending(intent)) => intent,
            None => anyhow::bail!("catalog prepared fence unavailable"),
        };
        ensure!(
            intent.request == *request
                && intent.preview_digest == preview_digest
                && intent.prepared_digest == prepared_digest,
            "catalog request key conflict"
        );
        let proof = CatalogFenceProof::from_intent(&intent)?;
        require_exact_pending(&tx, intent.operation_id)?;
        let reviewed = preview_reserved(&tx, request, Some(intent.operation_id), &self.root)?;
        ensure!(
            reviewed.identity()? == preview_digest,
            "catalog fenced review changed"
        );
        let receipt = FencedReceipt {
            request: request.clone(),
            metadata: CatalogReceipt {
                schema: 1,
                operation_id: intent.operation_id,
                caller: request.caller,
                key: request.key.clone(),
                request_digest: request.identity()?,
                catalog_revision: reviewed.next_revision,
                authority_fence_required: true,
            },
            preview_digest,
            prepared_digest,
            proof,
        };
        receipt.validate_for(request, preview_digest, prepared_digest, &intent.plan)?;
        changed(tx.execute("UPDATE index_operation_receipts SET stage='metadata_applied',revision=?1,payload=?2 WHERE id=?3 AND kind=?4 AND stage='fenced' AND payload=?5", params![number(receipt.metadata.catalog_revision),serde_json::to_vec(&receipt)?,intent.operation_id.to_string(),KIND,serde_json::to_vec(&intent)?])?)?;
        mutate(&tx, request, &self.root)?;
        changed(tx.execute(
            "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
            params![
                number(receipt.metadata.catalog_revision),
                number(request.expected_catalog_revision)
            ],
        )?)?;
        tx.commit()?;
        Ok(receipt)
    }
}
