// SPDX-License-Identifier: Apache-2.0
//! An Invoke's opaque original read allowance; never mutation/replay authority.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    read_attempt::{ReadAttemptBudget, ReadClock},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// In-process Invoke custody. Public only because Io crosses the thin CLI
/// crate boundary; construction/consumption stay with the owning CLI/session.
#[derive(Clone)]
pub struct InvocationReadContext {
    original: Result<ReadAttemptBudget, String>,
    cancelled: Arc<AtomicBool>,
    clock: Arc<dyn super::session::Clock>,
}
fn native_clock() -> ReadClock {
    if cfg!(windows) {
        ReadClock::WindowsPerformanceCounter
    } else {
        ReadClock::UnixMonotonic
    }
}
impl InvocationReadContext {
    pub(crate) fn new(
        id: Incarnation,
        received_ns: Option<u64>,
        duration_ms: u64,
        cancelled: Arc<AtomicBool>,
        clock: Arc<dyn super::session::Clock>,
    ) -> Self {
        let original = (|| {
            let received = received_ns
                .ok_or_else(|| "original native receipt clock unavailable".to_owned())?;
            let duration = duration_ms
                .checked_mul(1_000_000)
                .ok_or_else(|| "original read duration overflow".to_owned())?;
            let expires = received
                .checked_add(duration)
                .ok_or_else(|| "original read expiry overflow".to_owned())?;
            Ok(ReadAttemptBudget {
                schema: 1,
                request_id: id,
                clock: native_clock(),
                expires_monotonic_ns: expires,
                remaining_ns: duration,
                native_retry_expires_monotonic_ns: None,
            })
        })();
        Self {
            original,
            cancelled,
            clock,
        }
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub(crate) fn live_budget(&self) -> Result<ReadAttemptBudget, ErrorLine> {
        let failure = |message: &str| {
            ErrorLine::new(
                ExitClass::Refused,
                "read_attempt_unavailable",
                false,
                message,
            )
        };
        if self.cancelled() {
            return Err(failure(
                "Original Invoke was cancelled before canonical read submission.",
            ));
        }
        let mut budget = *self.original.as_ref().map_err(|error| failure(error))?;
        let now = self.clock.now_ns().map_err(|error| failure(&error))?;
        let remaining = budget
            .remaining_at(native_clock(), now)
            .map_err(|error| failure(&error.to_string()))?;
        if remaining.is_zero() {
            return Err(failure(
                "Original Invoke deadline expired before canonical read submission.",
            ));
        }
        budget.remaining_ns = u64::try_from(remaining.as_nanos())
            .map_err(|_| failure("Original read remainder overflow"))?;
        Ok(budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    struct Clock(AtomicU64);
    impl super::super::session::Clock for Clock {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        fn real_wait(&self, _ms: u64) -> Option<std::time::Duration> {
            None
        }
        fn attach(&self, _wake: super::super::session::Wake) {}
    }
    #[test]
    fn original_read_expiry_survives_queue_progress_and_cancellation() {
        let clock = Arc::new(Clock(AtomicU64::new(100)));
        let cancel = Arc::new(AtomicBool::new(false));
        let context = InvocationReadContext::new(
            Incarnation::from_bytes([7; 16]),
            Some(100_000_000),
            2000,
            cancel.clone(),
            clock.clone(),
        );
        clock.0.store(600, Ordering::SeqCst);
        let first = context.live_budget().unwrap();
        assert_eq!(first.remaining_ns, 1_500_000_000);
        clock.0.store(1600, Ordering::SeqCst);
        let retried = context.clone().live_budget().unwrap();
        assert_eq!(first.request_id, retried.request_id);
        assert_eq!(first.expires_monotonic_ns, retried.expires_monotonic_ns);
        assert_eq!(retried.remaining_ns, 500_000_000);
        clock.0.store(2100, Ordering::SeqCst);
        assert!(context.live_budget().is_err());
        clock.0.store(600, Ordering::SeqCst);
        cancel.store(true, Ordering::SeqCst);
        assert!(context.live_budget().is_err());
    }
    #[test]
    fn native_clock_failure_is_lazy_and_cannot_create_a_read_allowance() {
        let context = InvocationReadContext::new(
            Incarnation::from_bytes([8; 16]),
            None,
            2000,
            Arc::new(AtomicBool::new(false)),
            Arc::new(Clock(AtomicU64::new(0))),
        );
        assert!(!context.cancelled());
        assert!(context.live_budget().is_err());
    }
}
