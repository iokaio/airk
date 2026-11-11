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
        FenceIntent, FenceObservation, FencePreparation, FenceTarget, FencedReceipt,
    },
};
use anyhow::{Result, ensure};

/// Native control port; no method consumes a frontend assertion of revocation.
pub(crate) trait CatalogControl {
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool;
    fn revoke(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()>;
    fn confirm_reaped(&self, worker: WorkerIdentity) -> Result<bool>;
}

/// Constructed only after exact native Store observations, never from a wire bool.
pub(crate) struct KnownUnprepared {
    key: FenceKey,
}
impl KnownUnprepared {
    pub(crate) fn matches(&self, key: &FenceKey) -> bool {
        self.key == *key
    }
    fn observe(registry: &ServiceRegistry, request: &CatalogRequest) -> Result<Self> {
        ensure!(
            registry
                .catalog_fence_observation(request.caller, &request.key)?
                .is_none()
                && registry
                    .catalog_receipt(request.caller, &request.key)?
                    .is_none()
                && registry.catalog_pending_fence()?.is_none(),
            "catalog preparation outcome unknown"
        );
        let state = registry
            .catalog_state()?
            .ok_or_else(|| anyhow::anyhow!("catalog preparation outcome unknown"))?;
        ensure!(
            state.namespace == request.key.namespace
                && state.revision == request.expected_catalog_revision
                && state.activated
                && state.authority_contract == 1
                && state.policy_contract == 0,
            "catalog preparation observation changed"
        );
        Ok(Self {
            key: FenceKey::new(request)?,
        })
    }
}

pub(crate) struct CatalogCoordinator<C: CatalogControl> {
    gate: NativeCatalogAdmission,
    control: C,
}
pub(crate) type NativeCatalogCoordinator =
    CatalogCoordinator<crate::project_supervisor::CatalogControlHandle>;
impl<C: CatalogControl> CatalogCoordinator<C> {
    pub fn new(gate: NativeCatalogAdmission, control: C) -> Result<Self> {
        ensure!(
            control.uses_gate(&gate),
            "foreign native catalog control binding"
        );
        Ok(Self { gate, control })
    }
    /// The existing metadata actor serializes calls with its own registry. Never
    /// move this operation into a caller-owned cancellable transport future.
    pub fn apply_fenced(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
    ) -> Result<FencedReceipt> {
        self.apply_with_hook(registry, request, preview_digest, || {})
    }
    fn apply_with_hook(
        &mut self,
        registry: &mut ServiceRegistry,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        before_prepare: impl FnOnce(),
    ) -> Result<FencedReceipt> {
        request.validate()?;
        // Durable exact replay comes before catalog/key staleness. Metadata-only
        // receipts do not acquire a native proof merely because they were saved.
        let observed = registry.catalog_fence_observation(request.caller, &request.key)?;
        let pending = match observed {
            Some(FenceObservation::Applied(receipt)) => {
                ensure!(
                    receipt.request == *request && receipt.preview_digest == preview_digest,
                    "catalog request key conflict"
                );
                self.gate.reconcile_terminal(request, &receipt)?;
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
                && current.policy_contract == 0
                && current.namespace == request.key.namespace,
            "native catalog authority contract unavailable"
        );
        let plan = match (&pending, self.gate.matching_plan(request)?) {
            (_, Some(plan)) => plan,
            (Some(intent), None) => self.gate.restore_pending_fence(intent)?,
            (None, None) => self.gate.close_request_admission(request)?,
        };
        before_prepare();
        let prepared = registry.catalog_prepare_fence(request, preview_digest, &plan.durable);
        let intent = match prepared {
            Ok(FencePreparation::Replay(receipt)) => {
                self.gate.reconcile_terminal(request, &receipt)?;
                return Ok(receipt);
            }
            Ok(FencePreparation::Prepared(intent)) => intent,
            Err(error) => {
                // A synchronous, known rejection may reopen only after exact
                // durable absence and unchanged native revision are observed.
                // Read/commit uncertainty or any native effect leaves it closed.
                if error.downcast_ref::<airk_store::service::registry::catalog::operations::FencePrepareRejected>().is_some()
                    && let Ok(proof) = KnownUnprepared::observe(registry, request) {
                    let _ = self.gate.abort_known_unprepared(&plan.ticket, &proof);
                }
                return Err(error);
            }
        };
        self.gate.begin_revocations(&plan.ticket)?;
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
        let proof = KnownUnprepared::observe(registry, request)?;
        self.gate.abort_known_unprepared(&plan.ticket, &proof)
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
        for target in &plan.targets {
            if self.control.confirm_reaped(target.worker)? {
                self.gate.observe_worker_reaped(target.worker)?;
            }
        }
        Ok(Some(intent))
    }
}

#[cfg(test)]
#[path = "catalog_coordinator_tests.rs"]
mod tests;
