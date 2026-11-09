// SPDX-License-Identifier: Apache-2.0
// Native lifetime accounting. Wire observations are never completion authority.
use super::*;
use airk_core::{
    inference_provider::{CompletionObservation, CompletionState, ProviderIdentity},
    service::WorkerIdentity,
};

#[derive(Default)]
struct Counts {
    closed: bool,
    high_water: u64,
    queued: u64,
    running: u64,
    deliveries: u64,
    engine_barrier: bool,
    cached_model: bool,
    operation: Option<(Incarnation, Vec<WorkerIdentity>)>,
    workers: BTreeMap<Incarnation, (u64, u64, u64)>,
}
#[derive(Default)]
pub(super) struct Accounting(Mutex<Counts>);
impl Accounting {
    pub(super) fn status(
        &self,
        provider: ProviderIdentity,
    ) -> Result<airk_core::inference_provider::ProviderStatus> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        let status = airk_core::inference_provider::ProviderStatus {
            schema: 1,
            kind: airk_core::inference_provider::ProviderKind::InProcess,
            provider,
            admission_open: !state.closed,
            admitted_high_water: state.high_water,
            queued_outstanding: state.queued,
            running_outstanding: state.running,
            callback_outstanding: state.running,
            delivery_outstanding: state.deliveries,
            model_references_outstanding: state.running + u64::from(state.cached_model),
            failure: None,
        };
        status.validate()?;
        Ok(status)
    }
    pub(super) fn model_cached(&self, cached: bool) {
        if let Ok(mut state) = self.0.lock() {
            state.cached_model = cached;
        }
    }
    pub(super) fn admission_open(&self) -> Result<bool> {
        Ok(!self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?
            .closed)
    }
    pub(super) fn settled(&self) -> Result<bool> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        Ok(state.closed && state.queued == 0 && state.running == 0 && state.deliveries == 0)
    }
    pub(super) fn mark_engine_barrier(&self) {
        if let Ok(mut state) = self.0.lock()
            && state.closed
            && state.queued == 0
            && state.running == 0
            && state.deliveries == 0
        {
            state.engine_barrier = true;
        }
    }
    pub(super) fn worker_settled(&self, worker: Incarnation) -> Result<bool> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        Ok(state
            .workers
            .get(&worker)
            .is_none_or(|counts| *counts == (0, 0, 0)))
    }
    pub(super) fn forget_worker(&self, worker: Incarnation) -> Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        ensure!(
            state
                .workers
                .get(&worker)
                .is_none_or(|counts| *counts == (0, 0, 0)),
            "inference worker still charged"
        );
        state.workers.remove(&worker);
        Ok(())
    }
    pub(super) fn admit(
        self: &Arc<Self>,
        worker: Incarnation,
    ) -> Result<(ComputeCharge, DeliveryCharge)> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        ensure!(!state.closed, "inference admission closed");
        state.high_water = state
            .high_water
            .checked_add(1)
            .context("inference request counter overflow")?;
        state.queued += 1;
        state.deliveries += 1;
        let counts = state.workers.entry(worker).or_default();
        counts.0 += 1;
        counts.2 += 1;
        Ok((
            ComputeCharge {
                owner: self.clone(),
                running: false,
                worker,
            },
            DeliveryCharge(self.clone(), worker),
        ))
    }
    pub(super) fn operation(&self) -> Result<Option<Incarnation>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?
            .operation
            .as_ref()
            .map(|(operation, _)| *operation))
    }
    pub(super) fn close(&self, operation: Incarnation, workers: Vec<WorkerIdentity>) -> Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        if let Some(old) = &state.operation {
            ensure!(
                old == &(operation, workers),
                "inference checkpoint identity changed"
            );
        } else {
            state.operation = Some((operation, workers));
        }
        state.closed = true;
        Ok(())
    }
    pub(super) fn observation(
        &self,
        provider: ProviderIdentity,
        operation: Incarnation,
    ) -> Result<CompletionObservation> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("inference accounting unavailable"))?;
        let (owned_operation, workers) = state
            .operation
            .as_ref()
            .context("inference checkpoint not started")?;
        ensure!(
            *owned_operation == operation,
            "inference checkpoint operation changed"
        );
        let complete = state.closed
            && state.engine_barrier
            && state.queued == 0
            && state.running == 0
            && state.deliveries == 0
            && !state.cached_model;
        let observation = CompletionObservation {
            completion_basis: airk_core::inference_provider::CompletionBasis::NativeEngineBarrier,
            schema: 1,
            provider,
            operation_id: operation,
            captured_workers: workers.clone(),
            admitted_high_water: state.high_water,
            admission_closed: state.closed,
            queued_outstanding: state.queued,
            running_outstanding: state.running,
            // Engine callbacks and native model borrows are synchronous inside
            // the charged call; the separate barrier drops cached sessions.
            callback_outstanding: state.running,
            delivery_outstanding: state.deliveries,
            model_references_outstanding: state.running + u64::from(state.cached_model),
            completion_ledger: None,
            state: if complete {
                CompletionState::Complete
            } else {
                CompletionState::Pending
            },
        };
        observation.validate()?;
        Ok(observation)
    }
}
pub(super) struct ComputeCharge {
    owner: Arc<Accounting>,
    running: bool,
    worker: Incarnation,
}
impl ComputeCharge {
    pub(super) fn running(&mut self) {
        let mut state = self
            .owner
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.queued -= 1;
        state.running += 1;
        if let Some(counts) = state.workers.get_mut(&self.worker) {
            counts.0 -= 1;
            counts.1 += 1;
        }
        self.running = true;
    }
}
impl Drop for ComputeCharge {
    fn drop(&mut self) {
        let mut state = self
            .owner
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.running {
            state.running -= 1;
            if let Some(counts) = state.workers.get_mut(&self.worker) {
                counts.1 -= 1;
            }
        } else {
            state.queued -= 1;
            if let Some(counts) = state.workers.get_mut(&self.worker) {
                counts.0 -= 1;
            }
        }
    }
}
pub(super) struct DeliveryCharge(Arc<Accounting>, Incarnation);
impl Drop for DeliveryCharge {
    fn drop(&mut self) {
        let mut state = self
            .0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.deliveries -= 1;
        if let Some(counts) = state.workers.get_mut(&self.1) {
            counts.2 -= 1;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn provider() -> ProviderIdentity {
        ProviderIdentity {
            schema: 1,
            service_instance: Incarnation::from_bytes([1; 16]),
            approved_execution_sha256: Sha256Digest::from_bytes([2; 32]),
            provider_generation: Incarnation::from_bytes([3; 16]),
        }
    }
    #[test]
    fn completion_requires_actual_compute_delivery_and_cache_barriers() -> Result<()> {
        let accounting = Arc::new(Accounting::default());
        let worker = Incarnation::from_bytes([4; 16]);
        let operation = Incarnation::from_bytes([5; 16]);
        let (mut compute, delivery) = accounting.admit(worker)?;
        accounting.close(operation, Vec::new())?;
        assert!(accounting.admit(worker).is_err());
        let queued = accounting.observation(provider(), operation)?;
        assert_eq!(
            (queued.queued_outstanding, queued.delivery_outstanding),
            (1, 1)
        );
        compute.running();
        let running = accounting.observation(provider(), operation)?;
        assert_eq!(
            (
                running.running_outstanding,
                running.callback_outstanding,
                running.model_references_outstanding
            ),
            (1, 1, 1)
        );
        drop(compute);
        accounting.mark_engine_barrier();
        assert_eq!(
            accounting.observation(provider(), operation)?.state,
            CompletionState::Pending
        );
        assert!(!accounting.worker_settled(worker)?);
        drop(delivery);
        assert!(accounting.worker_settled(worker)?);
        assert_eq!(
            accounting.observation(provider(), operation)?.state,
            CompletionState::Pending
        );
        accounting.mark_engine_barrier();
        assert_eq!(
            accounting.observation(provider(), operation)?.state,
            CompletionState::Complete
        );
        assert!(
            accounting
                .observation(provider(), Incarnation::from_bytes([6; 16]))
                .is_err()
        );
        assert!(
            accounting
                .close(Incarnation::from_bytes([6; 16]), Vec::new())
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn canceled_queued_work_releases_only_its_own_worker_charges() -> Result<()> {
        let accounting = Arc::new(Accounting::default());
        let first = Incarnation::from_bytes([4; 16]);
        let second = Incarnation::from_bytes([5; 16]);
        let (compute_a, delivery_a) = accounting.admit(first)?;
        let (mut compute_b, delivery_b) = accounting.admit(second)?;
        compute_b.running();
        drop(compute_a);
        drop(delivery_a);
        assert!(accounting.worker_settled(first)?);
        assert!(!accounting.worker_settled(second)?);
        assert!(accounting.forget_worker(second).is_err());
        accounting.forget_worker(first)?;
        drop(compute_b);
        assert!(!accounting.worker_settled(second)?);
        drop(delivery_b);
        assert!(accounting.worker_settled(second)?);
        accounting.forget_worker(second)?;
        Ok(())
    }
}
