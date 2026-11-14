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

/// Opaque native policy custody. Implementations fence the ACTUAL nonblocking
/// delivery poll, not a preceding boolean probe. Never own the context/container.
pub trait ReadDeliveryGuard: Send + Sync {
    fn bindings_digest(&self) -> airk_core::identity::Sha256Digest;
    /// Final raw-I/O cut only: MUST NOT acquire Store, native gates or lifecycle.
    fn check_live(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn poll(
        &self,
        effect: &mut dyn FnMut() -> std::task::Poll<std::io::Result<usize>>,
    ) -> std::task::Poll<std::io::Result<usize>>;
}
pub type ReadDeliveryGuardFactory = Arc<
    dyn Fn(
            &[airk_core::combined_context::ContextDeliveryBinding],
            &ReadAttemptContext,
        ) -> anyhow::Result<Arc<dyn ReadDeliveryGuard>>
        + Send
        + Sync,
>;
#[derive(Default)]
struct DeliveryCustody(std::sync::Mutex<Option<Arc<dyn ReadDeliveryGuard>>>);
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReadSourceGuardKind {
    Epoch,
    Selection,
}
/// Minted only by a native admission owner from actual opaque custody.
pub trait ReadSourceDeliveryGuard: Send + Sync {
    fn kind(&self) -> ReadSourceGuardKind;
    fn identity(&self) -> airk_core::identity::Sha256Digest;
    fn check(&self) -> std::io::Result<()>;
    fn poll(
        &self,
        effect: &mut dyn FnMut() -> std::task::Poll<std::io::Result<usize>>,
    ) -> std::task::Poll<std::io::Result<usize>>;
}
#[derive(Default)]
struct SourceCustody(std::sync::Mutex<Vec<Arc<dyn ReadSourceDeliveryGuard>>>);

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
    context_answer_started: Arc<AtomicBool>,
    request_id: Option<airk_core::identity::Incarnation>,
    wire_attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
    wire_deadline: Option<Instant>,
    context_partition: Option<airk_core::combined_context::PartitionAllowance>,
    deadline: Instant,
    canceled: CancellationToken,
    retry_spent: Arc<AtomicBool>,
    remote_admission: bool,
    selection_refused: Arc<AtomicBool>,
    handler_started: Arc<AtomicBool>,
    native_retry_deadline: Option<Instant>,
    lifetime_guards: Arc<LifetimeGuards>,
    delivery_custody: Arc<DeliveryCustody>,
    delivery_factory: Option<ReadDeliveryGuardFactory>,
    source_custody: Arc<SourceCustody>,
    worker_reap_observer: Option<WorkerReapObserver>,
    worker_settlement_requester: Option<WorkerSettlementRequester>,
    native_unsettled: Arc<AtomicBool>,
}

impl ReadAttemptContext {
    pub fn new(deadline: Instant, canceled: CancellationToken) -> Self {
        Self {
            context_answer_started: Arc::new(AtomicBool::new(false)),
            request_id: None,
            wire_attempt: None,
            wire_deadline: None,
            context_partition: None,
            deadline,
            canceled,
            retry_spent: Arc::new(AtomicBool::new(false)),
            remote_admission: false,
            selection_refused: Arc::new(AtomicBool::new(false)),
            handler_started: Arc::new(AtomicBool::new(false)),
            native_retry_deadline: None,
            lifetime_guards: Arc::new(LifetimeGuards::default()),
            delivery_custody: Arc::new(DeliveryCustody::default()),
            delivery_factory: None,
            source_custody: Arc::new(SourceCustody::default()),
            worker_reap_observer: None,
            worker_settlement_requester: None,
            native_unsettled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn begin_context_answer(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.context_answer_started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "original context attempt was already submitted"
        );
        Ok(())
    }
    pub fn lifetime_guard_of<T: std::any::Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.lifetime_guards
            .0
            .lock()
            .ok()?
            .iter()
            .find_map(|guard| guard.clone().downcast::<T>().ok())
    }

    pub fn request_id(&self) -> Option<airk_core::identity::Incarnation> {
        self.request_id
    }
    pub fn with_request_id(mut self, id: airk_core::identity::Incarnation) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.request_id.is_none_or(|prior| prior == id),
            "original read attempt identity cannot be rebound"
        );
        self.request_id = Some(id);
        Ok(self)
    }
    pub fn wire_attempt(&self) -> Option<airk_core::read_attempt::ReadAttemptBudget> {
        self.wire_attempt
    }
    pub fn with_wire_attempt(
        mut self,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.request_id
                .is_none_or(|prior| prior == attempt.request_id),
            "original read attempt identity cannot be rebound"
        );
        anyhow::ensure!(
            self.wire_attempt.is_none(),
            "original native read allowance already captured"
        );
        self.request_id = Some(attempt.request_id);
        self.wire_attempt = Some(attempt);
        self.wire_deadline = Some(self.deadline);
        Ok(self)
    }
    pub fn captured_transport_attempt(&self) -> Option<airk_core::read_attempt::ReadAttemptBudget> {
        (self.wire_deadline == Some(self.deadline))
            .then_some(self.wire_attempt)
            .flatten()
    }
    pub fn capture_transport_attempt(
        mut self,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.request_id == Some(attempt.request_id),
            "transport attempt identity mismatch"
        );
        if let Some(prior) = self.wire_attempt {
            anyhow::ensure!(
                attempt.clock == prior.clock
                    && attempt.schema == prior.schema
                    && attempt.expires_monotonic_ns <= prior.expires_monotonic_ns
                    && attempt.remaining_ns <= prior.remaining_ns,
                "transport attempt cannot enlarge original allowance"
            );
        }
        self.wire_attempt = Some(attempt);
        self.wire_deadline = Some(self.deadline);
        Ok(self)
    }
    pub fn context_partition(&self) -> Option<&airk_core::combined_context::PartitionAllowance> {
        self.context_partition.as_ref()
    }
    pub fn with_context_partition(
        mut self,
        allowance: airk_core::combined_context::PartitionAllowance,
    ) -> anyhow::Result<Self> {
        allowance.validate()?;
        anyhow::ensure!(
            self.request_id == Some(allowance.attempt),
            "partition differs from original read attempt"
        );
        anyhow::ensure!(
            self.context_partition.is_none(),
            "read attempt already has a partition allocation"
        );
        self.context_partition = Some(allowance);
        Ok(self)
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

    pub fn retain_delivery_guard(&self, guard: Arc<dyn ReadDeliveryGuard>) -> anyhow::Result<()> {
        let mut held = self
            .delivery_custody
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("native delivery custody unavailable"))?;
        if let Some(prior) = held.as_ref() {
            anyhow::ensure!(
                prior.bindings_digest() == guard.bindings_digest(),
                "native delivery binding changed"
            );
        } else {
            *held = Some(guard);
        }
        Ok(())
    }
    pub fn with_delivery_guard_factory(mut self, factory: ReadDeliveryGuardFactory) -> Self {
        self.delivery_factory = Some(factory);
        self
    }
    /// Called on the complete known Answer value, before transport rendering
    /// or SSE/chunk buffering. Ordinary external clients retain their existing
    /// native handoff boundary; native server planes install the owning factory.
    pub fn retain_delivery_for(&self, value: &serde_json::Value) -> anyhow::Result<()> {
        // A captured private member is not the whole reply owner. Its complete
        // native body is reconciled by the aggregate parent, which registers
        // exactly one full binding roster on the shared outer custody.
        if self.context_partition.is_some() {
            return Ok(());
        }
        if let Some(factory) = &self.delivery_factory {
            let expected = airk_core::combined_context::context_delivery_bindings(value)?;
            self.retain_delivery_guard(factory(&expected, self)?)?;
        }
        Ok(())
    }
    /// Copy the opaque owner out before entering policy/native locks. Pending
    /// releases every policy lock; later polls repeat the same original fence.
    pub fn poll_delivery(
        &self,
        effect: &mut dyn FnMut() -> std::task::Poll<std::io::Result<usize>>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.canceled.is_cancelled() || Instant::now() >= self.deadline {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "original native delivery deadline or cancellation",
            )));
        }
        let guard = match self.delivery_custody.0.lock() {
            Ok(held) => held.clone(),
            Err(_) => {
                return std::task::Poll::Ready(Err(std::io::Error::other(
                    "native delivery custody unavailable",
                )));
            }
        };
        match guard {
            Some(guard) => guard.poll(effect),
            None => effect(),
        }
    }
    pub fn check_delivery_live(&self) -> std::io::Result<()> {
        if self.canceled.is_cancelled() || Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "original native delivery deadline or cancellation",
            ));
        }
        let guard = self
            .delivery_custody
            .0
            .lock()
            .map_err(|_| std::io::Error::other("native delivery custody unavailable"))?
            .clone();
        if let Some(guard) = guard {
            guard.check_live()?;
        }
        Ok(())
    }
    pub fn retain_source_delivery_guard(
        &self,
        guard: Arc<dyn ReadSourceDeliveryGuard>,
    ) -> anyhow::Result<()> {
        let mut guards = self
            .source_custody
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("native source custody unavailable"))?;
        if let Some(prior) = guards.iter().find(|prior| prior.kind() == guard.kind()) {
            anyhow::ensure!(
                prior.identity() == guard.identity(),
                "native source custody changed"
            );
        } else {
            anyhow::ensure!(guards.len() < 2, "native source custody bound");
            guards.push(guard);
        }
        Ok(())
    }
    pub fn has_source_delivery(&self) -> bool {
        self.source_custody
            .0
            .lock()
            .map_or(true, |guards| !guards.is_empty())
    }
    pub fn poll_source_delivery(
        &self,
        effect: &mut dyn FnMut() -> std::task::Poll<std::io::Result<usize>>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        let guards = match self.source_custody.0.lock() {
            Ok(guards) => guards.clone(),
            Err(_) => {
                return std::task::Poll::Ready(Err(std::io::Error::other(
                    "native source custody unavailable",
                )));
            }
        };
        let epoch = guards
            .iter()
            .find(|guard| guard.kind() == ReadSourceGuardKind::Epoch);
        let selected = guards
            .iter()
            .find(|guard| guard.kind() == ReadSourceGuardKind::Selection);
        if let Some(selected) = selected {
            // The extra epoch is fixed before work, never refreshed. The final
            // selected guard holds its own fixed issue+nonce through the poll;
            // a generation transition invalidates both, avoiding nested gates.
            if let Some(epoch) = epoch
                && let Err(error) = epoch.check()
            {
                return std::task::Poll::Ready(Err(error));
            }
            selected.poll(effect)
        } else if let Some(epoch) = epoch {
            epoch.poll(effect)
        } else {
            effect()
        }
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

/// Anchor a validated Answer ceiling before scope/setup/member work. Invalid
/// arguments remain for the canonical router's existing exact error projection.
pub fn admit_tool_context(
    tool: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
    context: ReadAttemptContext,
) -> ReadAttemptContext {
    admit_tool_context_at(tool, arguments, context, Instant::now())
}
fn admit_tool_context_at(
    tool: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
    context: ReadAttemptContext,
    admitted: Instant,
) -> ReadAttemptContext {
    if !matches!(tool, "airk_answer" | "answer") {
        return context;
    }
    let Some(payload) = crate::combined::native_admission_arguments(tool, arguments) else {
        return context;
    };
    let Ok(args) = serde_json::from_value::<crate::AnswerArgs>(serde_json::Value::Object(payload))
    else {
        return context;
    };
    if args.request.validate().is_err() {
        return context;
    }
    let Some(deadline) =
        admitted.checked_add(Duration::from_millis(args.request.budget.deadline_ms))
    else {
        return context;
    };
    context.narrowed(deadline)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifetime_tests;

#[cfg(test)]
mod admission_tests;

#[cfg(test)]
mod context_identity_tests;
#[cfg(test)]
mod delivery_tests;
