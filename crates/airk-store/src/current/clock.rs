// SPDX-License-Identifier: Apache-2.0
//! Monotonic time for request-snapshot deadlines. Production readers use the
//! system clock. Tests inject a manual clock so deadline decisions are
//! deterministic and a long fixture cannot expire because the host is busy.
use std::sync::Arc;
use std::time::Instant;

/// The only time source `CurrentRead` consults for its request deadline.
pub(crate) trait RequestClock: Send + Sync {
    fn now(&self) -> Instant;
}

struct SystemClock;

impl RequestClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The production clock: `Instant::now()`.
pub(crate) fn system() -> Arc<dyn RequestClock> {
    Arc::new(SystemClock)
}

/// A clock that moves only when a test advances it.
#[cfg(test)]
pub(crate) struct ManualClock {
    start: Instant,
    elapsed: std::sync::Mutex<std::time::Duration>,
}

#[cfg(test)]
impl ManualClock {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            start: Instant::now(),
            elapsed: Default::default(),
        })
    }

    pub(crate) fn advance(&self, duration: std::time::Duration) {
        *self.elapsed.lock().expect("manual clock poisoned") += duration;
    }
}

#[cfg(test)]
impl RequestClock for ManualClock {
    fn now(&self) -> Instant {
        self.start + *self.elapsed.lock().expect("manual clock poisoned")
    }
}
