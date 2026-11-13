// SPDX-License-Identifier: Apache-2.0
//! Inert native coordinator, called synchronously by the accepted metadata actor.
//! Caller cancellation drops only its waiter; the actor retains this operation.
//! Store owns transactions; Supervisor alone owns private child exchanges.
#![allow(dead_code)]
use crate::catalog_admission::{FenceKey, FenceTicket, NativeCatalogAdmission};
use airk_core::{
    catalog_management::CatalogRequest, identity::Sha256Digest, service::WorkerIdentity,
};
use airk_store::service::registry::{
    ServiceRegistry,
    catalog::operations::{
        CatalogFenceAbsence, CatalogFenceUnpreparedAbsence, FenceIntent, FenceObservation,
        FencePreparation, FenceTarget, FencedReceipt,
    },
};
use anyhow::{Result, ensure};

/// Native control port; no method consumes a frontend assertion of revocation.
pub trait CatalogControl {
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool;
    fn revoke(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()>;
    /// `RevokeScope` for a selection scope the fence revoked locally (§3.3
    /// step 4). Never a durable target; no `FenceTicket`.
    fn revoke_selection(
        &self,
        _: WorkerIdentity,
        _: airk_core::service::ScopedReadRef,
    ) -> Result<()> {
        anyhow::bail!("native selection revocation unavailable")
    }
    fn confirm_reaped(&self, worker: WorkerIdentity) -> Result<bool>;
    fn capture_workers(
        &self,
        _: &FenceTicket,
        _: airk_core::service::ProjectBinding,
    ) -> Result<crate::catalog_admission::FencePlan> {
        anyhow::bail!("native worker census unavailable")
    }
    fn drain_worker(&self, _: &FenceTicket, _: WorkerIdentity) -> Result<()> {
        anyhow::bail!("native worker drain unavailable")
    }
    fn finish_worker_drains(&self, _: &FencedReceipt) -> Result<()> {
        Ok(())
    }
    fn fence_source_policy(
        &self,
        _: airk_core::service::ProjectBinding,
        _: airk_core::identity::Incarnation,
    ) -> Result<()> {
        anyhow::bail!("native source policy watcher fence unavailable")
    }
    fn release_source_policy(
        &self,
        _: airk_core::service::ProjectBinding,
        _: airk_core::identity::Incarnation,
    ) -> Result<()> {
        anyhow::bail!("native source policy watcher release unavailable")
    }
}
#[derive(Debug)]
pub struct CatalogRevisionChanged;
impl std::fmt::Display for CatalogRevisionChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("catalog_revision_changed")
    }
}
impl std::error::Error for CatalogRevisionChanged {}
#[derive(Debug)]
pub struct CatalogClockPending;
impl std::fmt::Display for CatalogClockPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("catalog_clock_pending")
    }
}
impl std::error::Error for CatalogClockPending {}
/// Bounded native startup/configuration-completion boundary, never Health or a
/// read/query loop. Observes metadata only and publishes its cached clock.
pub fn refresh_catalog_clock(
    gate: &NativeCatalogAdmission,
    registry: &ServiceRegistry,
) -> Result<bool> {
    let state = registry
        .catalog_state()
        .map_err(|e| e.context(CatalogClockPending))?
        .ok_or(CatalogClockPending)?;
    ensure!(
        state.activated
            && state.authority_contract == 1
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
        CatalogClockPending
    );
    let before = gate.catalog_clock().ok_or(CatalogClockPending)?;
    gate.reconcile_catalog_snapshot(state.namespace, state.revision)
        .map_err(|e| e.context(CatalogClockPending))?;
    Ok(before.namespace != state.namespace || before.revision != state.revision)
}

/// Constructed only after exact native Store observations, never from a wire bool.
pub struct KnownUnprepared {
    key: FenceKey,
    absence: CatalogFenceAbsence,
}
impl KnownUnprepared {
    pub(crate) fn matches(&self, key: &FenceKey) -> bool {
        self.key == *key
            && self.absence.catalog_state().namespace == key.key.namespace
            && self.absence.catalog_state().revision == key.expected_revision
    }
    fn observe(registry: &ServiceRegistry, request: &CatalogRequest) -> Result<Self> {
        // This native-only snapshot proves exact saved absence even after the
        // accepted key's epoch rotates. It never relaxes new-write admission.
        let absence = registry.catalog_fence_absence(request)?;
        absence.validate_for(request)?;
        let state = absence.catalog_state();
        ensure!(
            state.namespace == request.key.namespace
                && state.revision == request.expected_catalog_revision
                && state.activated
                && state.authority_contract == 1
                && state.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
            "catalog preparation observation changed"
        );
        Ok(Self {
            key: FenceKey::new(request)?,
            absence,
        })
    }
}

/// Current same-namespace saved absence, usable only to release an already
/// retained ephemeral close. It never changes original mutation admission.
pub struct ReconciledUnprepared {
    key: FenceKey,
    absence: CatalogFenceUnpreparedAbsence,
}
impl ReconciledUnprepared {
    fn observe(registry: &ServiceRegistry, request: &CatalogRequest) -> Result<Self> {
        let absence = registry.catalog_fence_unprepared_absence(request)?;
        absence.validate_for(request)?;
        Ok(Self {
            key: FenceKey::new(request)?,
            absence,
        })
    }
    pub(crate) fn matches(&self, key: &FenceKey) -> bool {
        self.key == *key
            && self.absence.catalog_state().namespace == key.key.namespace
            && self.absence.catalog_state().revision >= key.expected_revision
    }
    pub(crate) fn clock(&self) -> airk_core::service::CatalogClock {
        let state = self.absence.catalog_state();
        airk_core::service::CatalogClock {
            namespace: state.namespace,
            revision: state.revision,
        }
    }
}

pub struct CatalogCoordinator<C: CatalogControl> {
    gate: NativeCatalogAdmission,
    control: C,
    cleanup: Option<FencedReceipt>,
    // Explicit observation can know a durable outcome while this service's
    // conflicting/uncertain ephemeral gate remains closed. Never hide it.
    receipt_reconciliation_pending: bool,
}
/// A native accepted operation may have durable/fence effects. Keep the exact
/// journal identity and complete known record; never report a no-write refusal.
pub struct CatalogApplyOutcomeUnknown {
    request: CatalogRequest,
    observation: Option<FenceObservation>,
}
#[derive(Debug)]
pub struct CatalogApplyRejectedNoEffects;
impl std::fmt::Display for CatalogApplyRejectedNoEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("catalog_preparation_rejected")
    }
}
impl std::error::Error for CatalogApplyRejectedNoEffects {}
impl std::fmt::Debug for CatalogApplyOutcomeUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogApplyOutcomeUnknown")
    }
}
impl std::fmt::Display for CatalogApplyOutcomeUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("catalog_apply_outcome_unknown")
    }
}
impl std::error::Error for CatalogApplyOutcomeUnknown {}
impl CatalogApplyOutcomeUnknown {
    pub fn capture(
        registry: &ServiceRegistry,
        request: &CatalogRequest,
        error: anyhow::Error,
    ) -> anyhow::Error {
        if request.validate().is_err()
            || error.downcast_ref::<Self>().is_some()
            || error
                .downcast_ref::<CatalogApplyRejectedNoEffects>()
                .is_some()
        {
            return error;
        }
        let observation = registry
            .catalog_fence_observation(request.caller, &request.key)
            .unwrap_or_default()
            .filter(|record| match record {
                FenceObservation::Pending(intent) => intent.request == *request,
                FenceObservation::Applied(receipt) => receipt.request == *request,
            });
        error.context(Self {
            request: request.clone(),
            observation,
        })
    }
    pub fn response(&self) -> airk_core::service::ControlResponse {
        airk_core::service::ControlResponse::CatalogOutcomeUnknown {
            code: "catalog_apply_outcome_unknown".into(),
            caller: self.request.caller,
            key: self.request.key.clone(),
            request_digest: self
                .request
                .identity()
                .expect("validated native catalog request"),
            observation: self.observation.clone().map(|fence| {
                airk_core::service::CatalogOutcomeObservation::AuthorityFence {
                    fence: Box::new(fence),
                }
            }),
        }
    }
}

/// Called by the authenticated metadata actor. Context defaults remain
/// preferences; the persisted reviewed grant is the evidence authority.
pub fn native_scoped_selection(
    registry: &mut ServiceRegistry,
    request: &airk_core::service::ScopedConnectRequest,
) -> Result<crate::catalog_authority::NativeCatalogAuthoritySelection> {
    use airk_core::{
        catalog_management::{CatalogEntity, CatalogItem},
        scoped_reads::ScopeReadRefused,
    };
    request.validate()?;
    let context = crate::catalog_host_context::NativeHostContext::load(
        registry,
        request.context_key,
        Some(request.expected_catalog_revision),
    )?;
    let descriptor = context.descriptor();
    ensure!(
        descriptor.namespace == request.namespace
            && descriptor.catalog_revision == request.expected_catalog_revision
            && descriptor.key == request.context_key,
        ScopeReadRefused
    );
    let grant = registry.catalog_detail(
        &CatalogEntity::Grant {
            id: request.grant_id,
        },
        Some(request.expected_catalog_revision),
    )?;
    ensure!(
        grant.namespace == descriptor.namespace && grant.revision == descriptor.catalog_revision,
        ScopeReadRefused
    );
    let Some(CatalogItem::Grant(persisted)) = grant.item else {
        anyhow::bail!(ScopeReadRefused)
    };
    persisted.validate()?;
    ensure!(
        persisted.grant.id == request.grant_id
            && persisted.grant.host == descriptor.host
            && !persisted.grant.revoked
            && persisted.grant.revision == request.expected_grant_revision,
        ScopeReadRefused
    );
    let detail = registry.catalog_detail(
        &CatalogEntity::Index {
            id: request.index_id,
        },
        Some(request.expected_catalog_revision),
    )?;
    ensure!(
        detail.namespace == descriptor.namespace && detail.revision == descriptor.catalog_revision,
        ScopeReadRefused
    );
    let Some(CatalogItem::Index(index)) = detail.item else {
        anyhow::bail!(ScopeReadRefused)
    };
    index.validate()?;
    ensure!(
        !index.tombstone && index.id == request.index_id,
        ScopeReadRefused
    );
    Ok(crate::catalog_authority::NativeCatalogAuthoritySelection {
        namespace: descriptor.namespace,
        host: descriptor.host,
        catalog_revision: descriptor.catalog_revision,
        grant_id: request.grant_id,
        grant_revision: request.expected_grant_revision,
        index_id: request.index_id,
        project_ids: request.project_ids.clone(),
        backing: index.backing.binding,
    })
}
impl<C: CatalogControl> CatalogCoordinator<C> {
    pub fn new(gate: NativeCatalogAdmission, control: C) -> Result<Self> {
        ensure!(
            control.uses_gate(&gate),
            "foreign native catalog control binding"
        );
        Ok(Self {
            gate,
            control,
            cleanup: None,
            receipt_reconciliation_pending: false,
        })
    }
    pub fn has_pending_cleanup(&self) -> bool {
        self.cleanup.is_some()
    }
    fn finish_provider_cleanup(&mut self, receipt: &FencedReceipt) -> Result<()> {
        self.cleanup = Some(receipt.clone());
        self.control.finish_worker_drains(receipt)?;
        if matches!(
            receipt.request.operation,
            airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
        ) {
            let obligation = receipt
                .proof
                .worker_obligation
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("source policy worker census missing"))?;
            self.control
                .release_source_policy(obligation.backing, receipt.metadata.operation_id)?;
        }
        self.cleanup = None;
        Ok(())
    }
    /// The existing metadata actor serializes calls with its own registry. Never
    /// move this operation into a caller-owned cancellable transport future.
    pub fn apply_fenced(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
    ) -> Result<FencedReceipt> {
        self.apply_fenced_after_prepare(registry, request, preview_digest, || {})
    }
    /// Qualification can retain the actual accepted owner at the durable
    /// prepare boundary. It uses the same classification/control/commit path.
    #[cfg(any(test, feature = "test-support"))]
    pub fn apply_fenced_with_prepared_test_hook(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        after_prepare: impl FnOnce(),
    ) -> Result<FencedReceipt> {
        self.apply_fenced_after_prepare(registry, request, preview_digest, after_prepare)
    }

    fn apply_fenced_after_prepare(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        after_prepare: impl FnOnce(),
    ) -> Result<FencedReceipt> {
        if let Some(receipt) = self.cleanup.clone() {
            self.finish_provider_cleanup(&receipt)?;
        }
        let result = self.apply_with_hooks(registry, request, preview_digest, || {}, after_prepare);
        match result {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                if request.validate().is_err() {
                    return Err(error);
                }
                if error
                    .downcast_ref::<CatalogApplyRejectedNoEffects>()
                    .is_some()
                {
                    return Err(error);
                }
                if error.is::<crate::catalog_admission::NativeCatalogRetentionFull>()
                    && matches!(
                        registry.catalog_saved_observation(request.caller, &request.key),
                        Ok(None)
                    )
                {
                    // Typed native memory refusal occurred before dispatch.
                    // Successful saved absence is required; read errors cannot
                    // become a known-no-effects result or silently clear close.
                    let safe = match self.gate.matching_plan(request) {
                        Ok(Some(_)) => self.reconcile_unprepared(registry, request).is_ok(),
                        Ok(None) => self
                            .gate
                            .pending_fence_key()
                            .is_ok_and(|pending| pending.is_none()),
                        Err(_) => false,
                    };
                    if safe {
                        return Err(error.context(CatalogApplyRejectedNoEffects));
                    }
                }
                // A failed observation never proves saved absence. Preserve an
                // unknown outcome even if every reconciliation read also fails.
                let observation = registry
                    .catalog_fence_observation(request.caller, &request.key)
                    .unwrap_or_default()
                    .filter(|record| match record {
                        FenceObservation::Pending(intent) => intent.request == *request,
                        FenceObservation::Applied(receipt) => receipt.request == *request,
                    });
                Err(error.context(CatalogApplyOutcomeUnknown {
                    request: request.clone(),
                    observation,
                }))
            }
        }
    }
    fn apply_with_hook(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        before_prepare: impl FnOnce(),
    ) -> Result<FencedReceipt> {
        self.apply_with_hooks(registry, request, preview_digest, before_prepare, || {})
    }
    fn apply_with_hooks(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        before_prepare: impl FnOnce(),
        after_prepare: impl FnOnce(),
    ) -> Result<FencedReceipt> {
        request.validate()?;
        // Durable exact replay comes before catalog/key staleness. Metadata-only
        // receipts do not acquire a native proof merely because they were saved.
        // Read errors / expired-key mutation attempts never implicitly reopen
        // an existing close. Only explicit readonly receipt reconciliation or
        // a typed initial preinsert rejection consumes native absence proof.
        let observed = registry.catalog_fence_observation(request.caller, &request.key)?;
        let pending = match observed {
            Some(FenceObservation::Applied(receipt)) => {
                ensure!(
                    receipt.request == *request && receipt.preview_digest == preview_digest,
                    "catalog request key conflict"
                );
                self.gate.reconcile_terminal(request, &receipt)?;
                self.finish_provider_cleanup(&receipt)?;
                return Ok(receipt);
            }
            Some(FenceObservation::Pending(intent)) => {
                ensure!(
                    intent.request == *request && intent.preview_digest == preview_digest,
                    "catalog request key conflict"
                );
                Some(intent)
            }
            None => {
                if let Some(receipt) = registry.catalog_receipt(request.caller, &request.key)? {
                    ensure!(
                        receipt.request_digest == request.identity()?,
                        "catalog request key conflict"
                    );
                    anyhow::bail!("catalog metadata receipt lacks native authority proof");
                }
                None
            }
        };
        let current = registry
            .catalog_state()?
            .ok_or_else(|| anyhow::anyhow!("native catalog authority unavailable"))?;
        ensure!(
            current.activated
                && current.authority_contract == 1
                && current.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
                && current.namespace == request.key.namespace,
            "native catalog authority contract unavailable"
        );
        // A native source completion may have advanced metadata since the last
        // host delivery. Refresh only before a new fence; never advance around
        // an existing captured operation or close an obsolete view's clock.
        if self.gate.pending_fence_key()?.is_none() {
            self.gate
                .reconcile_catalog_snapshot(current.namespace, current.revision)?;
        }
        let retirement_backing = if pending.is_none() && request.operation.requires_worker_drain() {
            let id = match request.operation {
                airk_core::catalog_management::CatalogOperation::RetireIndex { id, .. }
                | airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy {
                    id, ..
                } => id,
                _ => unreachable!("worker-draining operation lacks an index"),
            };
            let detail = registry.catalog_detail(
                &airk_core::catalog_management::CatalogEntity::Index { id },
                Some(request.expected_catalog_revision),
            )?;
            let Some(airk_core::catalog_management::CatalogItem::Index(index)) = detail.item else {
                anyhow::bail!("native retirement index unavailable");
            };
            Some((id, index.backing.binding))
        } else {
            None
        };
        let mut plan = match (&pending, self.gate.matching_plan(request)?) {
            (_, Some(plan)) => plan,
            (Some(intent), None) => self.gate.restore_pending_fence(intent)?,
            (None, None) => match retirement_backing {
                Some((_, backing)) => self.gate.close_retirement_admission(request, backing)?,
                None => self.gate.close_request_admission(request)?,
            },
        };
        if let Some((id, backing)) = retirement_backing {
            self.gate
                .bind_retirement_backing(&plan.ticket, id, backing)?;
            if matches!(
                request.operation,
                airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
            ) {
                self.control.fence_source_policy(
                    backing,
                    airk_core::catalog_management::catalog_operation_id(
                        request.caller,
                        &request.key,
                    )?,
                )?;
            }
            plan = self.control.capture_workers(&plan.ticket, backing)?;
        }
        if pending.is_some()
            && matches!(
                request.operation,
                airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
            )
        {
            let obligation = plan
                .durable
                .worker_obligation
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("source policy worker census missing"))?;
            self.control.fence_source_policy(
                obligation.backing,
                airk_core::catalog_management::catalog_operation_id(request.caller, &request.key)?,
            )?;
        }
        before_prepare();
        let prepared = registry.catalog_prepare_fence(request, preview_digest, &plan.durable);
        let intent = match prepared {
            Ok(FencePreparation::Replay(receipt)) => {
                self.gate.reconcile_terminal(request, &receipt)?;
                self.finish_provider_cleanup(&receipt)?;
                return Ok(receipt);
            }
            Ok(FencePreparation::Prepared(intent)) => intent,
            Err(error) => {
                // A synchronous, known rejection may reopen only after exact
                // durable absence and unchanged native revision are observed.
                // Read/commit uncertainty or any native effect leaves it closed.
                if error.downcast_ref::<airk_store::service::registry::catalog::operations::FencePrepareRejected>().is_some()
                    && let Ok(proof) = ReconciledUnprepared::observe(registry, request)
                    && self.gate.abort_reconciled_unprepared(&plan.ticket, proof).is_ok()
                {
                    if matches!(request.operation, airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }) {
                        let obligation = plan.durable.worker_obligation.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("source policy worker census missing"))?;
                        self.control.release_source_policy(obligation.backing, airk_core::catalog_management::catalog_operation_id(request.caller,&request.key)?)?;
                    }
                    return Err(error.context(CatalogApplyRejectedNoEffects));
                }
                return Err(error);
            }
        };
        after_prepare();
        self.gate.begin_revocations(&plan.ticket)?;
        for worker in &plan.drain_workers {
            let result = self.control.drain_worker(&plan.ticket, *worker);
            let confirmations = self.gate.worker_confirmations(&plan.ticket)?;
            registry.catalog_record_worker_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &confirmations,
            )?;
            result?;
        }
        for target in &plan.targets {
            if self
                .gate
                .confirmations(&plan.ticket)?
                .iter()
                .any(|confirmation| confirmation.target == *target)
            {
                continue;
            }
            let result = self.control.revoke(&plan.ticket, target);
            // Record real late/partial progress even if the waiter timed out.
            let confirmations = self.gate.confirmations(&plan.ticket)?;
            registry.catalog_record_fence_confirmations(
                request.caller,
                &request.key,
                intent.prepared_digest,
                &confirmations,
            )?;
            result?;
        }
        // Local selection revocations hold the commit until each worker
        // acknowledges or is reaped; nothing about them is recorded durably.
        for (worker, scope) in self.gate.unsettled_selection_revocations(&plan.ticket)? {
            self.control.revoke_selection(worker, scope)?;
        }
        self.gate.require_commit_ready(&plan.ticket)?;
        let confirmations = self.gate.confirmations(&plan.ticket)?;
        registry.catalog_record_fence_confirmations(
            request.caller,
            &request.key,
            intent.prepared_digest,
            &confirmations,
        )?;
        let receipt =
            registry.catalog_apply_fenced(request, preview_digest, intent.prepared_digest)?;
        self.gate.finish_commit(
            &plan.ticket,
            request,
            preview_digest,
            intent.prepared_digest,
            &receipt,
        )?;
        self.finish_provider_cleanup(&receipt)?;
        Ok(receipt)
    }
    /// Explicit same-request reconciliation after an uncertain Store outcome.
    /// Native observations, not the original transport error, prove safe absence.
    pub fn reconcile_unprepared(
        &mut self,
        registry: &ServiceRegistry,
        request: &CatalogRequest,
    ) -> Result<()> {
        let plan = self
            .gate
            .matching_plan(request)?
            .ok_or_else(|| anyhow::anyhow!("native pending catalog operation unavailable"))?;
        let proof = ReconciledUnprepared::observe(registry, request)?;
        self.gate.abort_reconciled_unprepared(&plan.ticket, proof)?;
        Ok(())
    }
    /// Called only by the readonly receipt handler after native saved-record
    /// inspection. Full original identity comes from this same service's gate.
    pub fn reconcile_receipt(
        &mut self,
        registry: &ServiceRegistry,
        caller: airk_core::identity::Incarnation,
        key: &airk_core::service::RequestKey,
    ) -> Result<Option<airk_core::service::ControlResponse>> {
        let Some(request) = self.gate.retained_request(caller, key)? else {
            return Ok(None);
        };
        let result = (|| -> Result<_> {
            let proof = ReconciledUnprepared::observe(registry, &request)?;
            let clock = match self.gate.matching_plan(&request)? {
                Some(plan) => self.gate.abort_reconciled_unprepared(&plan.ticket, proof)?,
                None => self.gate.reobserve_unprepared(&request, proof)?,
            };
            Ok(
                airk_core::service::ControlResponse::CatalogReceiptUnprepared {
                    request_digest: request.identity()?,
                    request: request.clone(),
                    catalog_clock: clock,
                },
            )
        })();
        result
            .map(Some)
            .map_err(|error| CatalogApplyOutcomeUnknown::capture(registry, &request, error))
    }
    /// Canonical native readonly receipt route. Every authority-fence insertion
    /// is serialized by this same accepted Metadata actor through proof read
    /// and Gate abort. Independent workers may advance only catalog metadata;
    /// fresh grant issuance still revalidates Store revision and issue ticket.
    pub fn observe_receipt(
        registry: &ServiceRegistry,
        mut coordinator: Option<&mut Self>,
        caller: airk_core::identity::Incarnation,
        key: &airk_core::service::RequestKey,
    ) -> Result<airk_core::service::ControlResponse> {
        use airk_core::service::ControlResponse;
        use airk_store::service::registry::catalog::operations::CatalogSavedObservation;
        let unknown = |error: anyhow::Error, native: Option<&Self>| -> anyhow::Error {
            if let Some(native) = native
                && let Ok(Some(request)) = native.gate.retained_request(caller, key)
            {
                return CatalogApplyOutcomeUnknown::capture(registry, &request, error);
            }
            error
        };
        match registry.catalog_saved_observation(caller, key) {
            Ok(Some(CatalogSavedObservation::Fence(fence))) => match *fence {
                FenceObservation::Applied(receipt) => {
                    if let Some(native) = coordinator.as_deref_mut() {
                        native.receipt_reconciliation_pending =
                            match native.gate.matching_plan(&receipt.request) {
                                Ok(Some(_)) => {
                                    // Releasing the gate must not orphan its
                                    // provider/watcher cleanup. Receipt reads
                                    // retain that exact work for the existing
                                    // metadata-owner resume turn, without RPC.
                                    if native
                                        .cleanup
                                        .as_ref()
                                        .is_some_and(|owned| owned != &receipt)
                                        || native
                                            .gate
                                            .reconcile_terminal(&receipt.request, &receipt)
                                            .is_err()
                                    {
                                        true
                                    } else {
                                        native.cleanup = Some(receipt.clone());
                                        false
                                    }
                                }
                                Ok(None) => native
                                    .gate
                                    .pending_fence_key()
                                    .map_or(true, |pending| pending.is_some()),
                                Err(_) => true,
                            };
                        // The canonical durable terminal record wins even when an
                        // unrelated or uncertain native gate cannot reconcile.
                        // Retain the diagnostic and closed gate; do not erase or
                        // downgrade the actual saved observation.
                    }
                    Ok(ControlResponse::CatalogReceipt {
                        receipt: Some(receipt.metadata.clone()),
                        fence: Some(FenceObservation::Applied(receipt)),
                    })
                }
                FenceObservation::Pending(intent) => Ok(ControlResponse::CatalogReceipt {
                    receipt: None,
                    fence: Some(FenceObservation::Pending(intent)),
                }),
            },
            Ok(Some(CatalogSavedObservation::Metadata(receipt))) => {
                if receipt.authority_fence_required {
                    let error = unknown(
                        anyhow::anyhow!("catalog_authority_fence_unavailable"),
                        coordinator.as_deref(),
                    );
                    if error.is::<CatalogApplyOutcomeUnknown>() {
                        return Err(error);
                    }
                    return Ok(ControlResponse::Failure {
                        code: "catalog_receipt_outcome_unknown".into(),
                    });
                }
                Ok(ControlResponse::CatalogReceipt {
                    receipt: Some(receipt),
                    fence: None,
                })
            }
            Ok(None) => {
                if let Some(native) = coordinator {
                    match native.reconcile_receipt(registry, caller, key) {
                        Ok(Some(response)) => return Ok(response),
                        Ok(None) => {}
                        Err(error) if error.is::<CatalogApplyOutcomeUnknown>() => {
                            return Err(error);
                        }
                        Err(_) => {
                            return Ok(ControlResponse::Failure {
                                code: "catalog_receipt_outcome_unknown".into(),
                            });
                        }
                    }
                }
                Ok(ControlResponse::CatalogReceipt {
                    receipt: None,
                    fence: None,
                })
            }
            Err(error) => {
                let error = unknown(error, coordinator.as_deref());
                if error.is::<CatalogApplyOutcomeUnknown>() {
                    return Err(error);
                }
                Ok(ControlResponse::Failure {
                    code: "catalog_receipt_outcome_unknown".into(),
                })
            }
        }
    }
    /// Continue only the one durable operation already accepted by this native
    /// actor. This is not frontend replay or a new request/key allocation.
    pub fn resume_pending(
        &mut self,
        registry: &mut ServiceRegistry,
    ) -> Result<Option<FencedReceipt>> {
        if let Some(receipt) = self.cleanup.clone() {
            self.finish_provider_cleanup(&receipt)?;
            return Ok(Some(receipt));
        }
        let Some(key) = self.gate.pending_fence_key()? else {
            return Ok(None);
        };
        let Some(intent) = registry.catalog_pending_fence()? else {
            if let Some(FenceObservation::Applied(receipt)) =
                registry.catalog_fence_observation(key.caller, &key.key)?
            {
                return self
                    .apply_fenced(registry, &receipt.request, receipt.preview_digest)
                    .map(Some);
            }
            // Absence never reopens ephemeral admission or invents no effects.
            return Ok(None);
        };
        let memory = self
            .gate
            .matching_plan(&intent.request)?
            .ok_or_else(|| anyhow::anyhow!("durable catalog fence changed native owner"))?;
        ensure!(
            memory.durable == intent.plan,
            "durable catalog fence plan changed"
        );
        self.apply_fenced(registry, &intent.request, intent.preview_digest)
            .map(Some)
    }
    /// Startup calls this before publishing native admission. No old worker is
    /// considered gone just because this process lacks its child pipe.
    pub fn restore_pending(&mut self, registry: &ServiceRegistry) -> Result<Option<FenceIntent>> {
        let Some(intent) = registry.catalog_pending_fence()? else {
            return Ok(None);
        };
        let plan = match self.gate.matching_plan(&intent.request)? {
            Some(plan) => plan,
            None => self.gate.restore_pending_fence(&intent)?,
        };
        if matches!(
            intent.request.operation,
            airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
        ) {
            let obligation = plan
                .durable
                .worker_obligation
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("source policy worker census missing"))?;
            self.control
                .fence_source_policy(obligation.backing, intent.operation_id)?;
        }
        for target in &plan.targets {
            if self.control.confirm_reaped(target.worker)? {
                self.gate.observe_worker_reaped(target.worker)?;
            }
        }
        for worker in &plan.drain_workers {
            if self.control.confirm_reaped(*worker)? {
                self.gate.observe_worker_reaped(*worker)?;
            }
        }
        Ok(Some(intent))
    }
}

#[cfg(test)]
mod tests;
