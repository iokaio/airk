// SPDX-License-Identifier: Apache-2.0
//! One logical read's original deadline, cancellation and selection retry credit.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const SELECTION_WAIT: Duration = Duration::from_millis(50);
pub type WorkerReapObserver =
    Arc<dyn Fn(airk_core::service::WorkerIdentity) -> anyhow::Result<bool> + Send + Sync>;
pub type WorkerSettlementRequester =
    Arc<dyn Fn(airk_core::service::WorkerIdentity) -> anyhow::Result<()> + Send + Sync>;

#[derive(Default)]
struct LifetimeGuards(std::sync::Mutex<Vec<Arc<dyn std::any::Any + Send + Sync>>>);

#[derive(Debug)]
pub(crate) struct ReadAttemptClosed;
impl std::fmt::Display for ReadAttemptClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native read allowance expired before handler admission")
    }
}
impl std::error::Error for ReadAttemptClosed {}

#[derive(Clone)]
pub struct ReadAttemptContext {
    deadline: Instant,
    canceled: CancellationToken,
    retry_spent: Arc<AtomicBool>,
    remote_admission: bool,
    selection_refused: Arc<AtomicBool>,
    handler_started: Arc<AtomicBool>,
    native_retry_deadline: Option<Instant>,
    lifetime_guards: Arc<LifetimeGuards>,
    worker_reap_observer: Option<WorkerReapObserver>,
    worker_settlement_requester: Option<WorkerSettlementRequester>,
    native_unsettled: Arc<AtomicBool>,
}

impl ReadAttemptContext {
    pub fn new(deadline: Instant, canceled: CancellationToken) -> Self {
        Self {
            deadline,
            canceled,
            retry_spent: Arc::new(AtomicBool::new(false)),
            remote_admission: false,
            selection_refused: Arc::new(AtomicBool::new(false)),
            handler_started: Arc::new(AtomicBool::new(false)),
            native_retry_deadline: None,
            lifetime_guards: Arc::new(LifetimeGuards::default()),
            worker_reap_observer: None,
            worker_settlement_requester: None,
            native_unsettled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.canceled
    }

    pub fn with_native_retry_deadline(mut self, deadline: Instant) -> Self {
        self.native_retry_deadline = Some(
            self.native_retry_deadline
                .map_or(deadline, |old| old.min(deadline))
                .min(self.deadline),
        );
        self
    }

    pub fn native_retry_deadline(&self) -> Option<Instant> {
        self.native_retry_deadline
    }

    pub fn with_lifetime_guard(mut self, guard: Arc<dyn std::any::Any + Send + Sync>) -> Self {
        self.lifetime_guards = Arc::new(LifetimeGuards(std::sync::Mutex::new(vec![guard])));
        self
    }

    pub fn lifetime_guard(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        let guards = self
            .lifetime_guards
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (!guards.is_empty())
            .then(|| self.lifetime_guards.clone() as Arc<dyn std::any::Any + Send + Sync>)
    }

    /// Retain permit-only owners in the shared container visible to delivery
    /// and native tasks. A guard must never own this container or its context.
    pub fn retain_lifetime_guard(
        &self,
        guard: Arc<dyn std::any::Any + Send + Sync>,
    ) -> anyhow::Result<()> {
        let mut guards = self
            .lifetime_guards
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        anyhow::ensure!(guards.len() < 4, "read lifetime guard bound exceeded");
        guards.push(guard);
        Ok(())
    }

    pub fn with_worker_reap_observer(mut self, observer: WorkerReapObserver) -> Self {
        self.worker_reap_observer = Some(observer);
        self
    }

    pub fn native_completion_unsettled(&self) -> bool {
        self.native_unsettled.load(Ordering::Acquire)
    }

    pub fn with_worker_settlement_requester(
        mut self,
        requester: WorkerSettlementRequester,
    ) -> Self {
        self.worker_settlement_requester = Some(requester);
        self
    }

    /// Unknown outcomes retain bounded accepted capacity until the supervisor
    /// confirms this exact worker's reap. Cancellation cannot return credits.
    pub async fn wait_for_worker_reap(&self, identity: airk_core::service::WorkerIdentity) -> bool {
        self.native_unsettled.store(true, Ordering::Release);
        let mut requested = false;
        loop {
            if let Some(observer) = &self.worker_reap_observer {
                if observer(identity).unwrap_or(false) {
                    self.native_unsettled.store(false, Ordering::Release);
                    return true;
                }
            } else if self.lifetime_guard().is_none() {
                // Legacy one-shot callers transfer no HTTP capacity. Their
                // native supervisor still owns charges; this is NOT settlement.
                return false;
            }
            if !requested && let Some(requester) = &self.worker_settlement_requester {
                // Queue admission is not completion. A full/closed queue keeps
                // capacity charged and may be retried without replaying work.
                requested = requester(identity).is_ok();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Member/verification budgets may narrow the original allowance, never
    /// replace it. All members retain the same single logical retry credit.
    pub fn narrowed(&self, deadline: Instant) -> Self {
        Self {
            deadline: self.deadline.min(deadline),
            ..self.clone()
        }
    }

    /// A native worker returns a private admission proof; the original frontend
    /// owns the shared aggregate credit and async wait, not each worker process.
    pub fn remote_admission(mut self) -> Self {
        self.remote_admission = true;
        self
    }

    pub fn selection_admission_refused(&self) -> bool {
        self.selection_refused.load(Ordering::Acquire)
            && !self.handler_started.load(Ordering::Acquire)
    }

    pub(crate) fn execution_child(&self) -> Self {
        Self {
            canceled: self.canceled.child_token(),
            selection_refused: Arc::new(AtomicBool::new(false)),
            handler_started: Arc::new(AtomicBool::new(false)),
            ..self.clone()
        }
    }

    pub(crate) fn mark_handler_started(&self) {
        self.handler_started.store(true, Ordering::Release);
    }
    pub(crate) fn mark_selection_refused(&self) {
        self.selection_refused.store(true, Ordering::Release);
    }
    pub(crate) fn uses_remote_admission(&self) -> bool {
        self.remote_admission
    }
    pub(crate) fn is_live(&self) -> bool {
        !self.canceled.is_cancelled() && Instant::now() < self.deadline
    }
    pub(crate) fn require_live(&self) -> anyhow::Result<()> {
        if self.is_live() {
            Ok(())
        } else {
            Err(ReadAttemptClosed.into())
        }
    }

    fn reserve_at(&self, now: Instant) -> bool {
        !self.canceled.is_cancelled()
            && now
                .checked_add(SELECTION_WAIT)
                .is_some_and(|after| after < self.selection_deadline())
            && self
                .retry_spent
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    fn selection_deadline(&self) -> Instant {
        self.native_retry_deadline
            .map_or(self.deadline, |deadline| self.deadline.min(deadline))
    }

    /// Call only for the native executor's authenticated pre-handler selection
    /// refusal, never a public JSON hint or an uncertain transport error.
    pub async fn wait_for_selection_admission(&self) -> bool {
        if !self.reserve_at(Instant::now()) {
            return false;
        }
        tokio::select! {
            biased;
            _ = self.canceled.cancelled() => false,
            _ = tokio::time::sleep(SELECTION_WAIT) => {
                !self.canceled.is_cancelled() && Instant::now() < self.selection_deadline()
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifetime_tests;
