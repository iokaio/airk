// SPDX-License-Identifier: Apache-2.0
//! Private same-machine read budgets. Wall-clock time never grants extra time.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_READ_BUDGET: Duration = Duration::from_secs(125);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadClock {
    UnixMonotonic,
    WindowsPerformanceCounter,
}

/// Sent only after the native peer's feature-version gate. Existing service and
/// grant identities still fence the same-machine, same-boot worker connection.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadAttemptBudget {
    pub schema: u32,
    pub request_id: crate::identity::Incarnation,
    pub clock: ReadClock,
    pub expires_monotonic_ns: u64,
    pub remaining_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_retry_expires_monotonic_ns: Option<u64>,
}

impl ReadAttemptBudget {
    pub fn remaining_at(self, clock: ReadClock, now_ns: u64) -> Result<Duration> {
        ensure!(
            self.schema == 1 && self.clock == clock,
            "unsupported native read clock"
        );
        ensure!(
            self.remaining_ns <= MAX_READ_BUDGET.as_nanos() as u64,
            "native read budget exceeds limit"
        );
        ensure!(
            self.native_retry_expires_monotonic_ns
                .is_none_or(|deadline| deadline <= self.expires_monotonic_ns),
            "native settings retry deadline exceeds read deadline"
        );
        let remaining = self
            .expires_monotonic_ns
            .saturating_sub(now_ns)
            .min(self.remaining_ns);
        Ok(Duration::from_nanos(remaining))
    }
}

#[cfg(test)]
mod tests;
