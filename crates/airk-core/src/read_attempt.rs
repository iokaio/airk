// SPDX-License-Identifier: Apache-2.0
//! Private same-machine read budgets. Wall-clock time never grants extra time.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The read attempt of every tool except Answer: the 120 s operation plus 5 s
/// for delivery. Transports mint at most this for a non-Answer tool.
pub const MAX_READ_BUDGET: Duration = Duration::from_secs(125);
/// Answer /3's operation ceiling (`arbiter::answer_loop::MAX_DEADLINE_MS`).
pub const MAX_ANSWER_DEADLINE: Duration =
    Duration::from_millis(crate::arbiter::answer_loop::MAX_DEADLINE_MS);
/// Answer's read attempt: its 600 s operation ceiling plus the same 5 s for
/// delivery that [`MAX_READ_BUDGET`] keeps over 120 s (final plan 12.4).
pub const MAX_ANSWER_READ_BUDGET: Duration =
    Duration::from_millis(crate::arbiter::answer_loop::MAX_DEADLINE_MS + 5_000);
/// The Arbiter IPC remaining-time ceiling of Answer-only methods (Context
/// actions and the loop's card scoring). Focus methods keep 120,000 ms.
pub const MAX_ANSWER_ARBITER_REMAINING_MS: u64 = crate::arbiter::answer_loop::MAX_DEADLINE_MS;

/// Whether a public or native tool name is Answer.
pub fn is_answer_tool(tool: &str) -> bool {
    matches!(tool, "airk_answer" | "answer")
}

/// The read attempt a transport mints, admits or reaps for one tool call:
/// [`MAX_ANSWER_READ_BUDGET`] for Answer, [`MAX_READ_BUDGET`] for every other
/// tool. Every entry point applies it, so other tools keep their 125 s.
pub fn read_budget_for_tool(tool: &str) -> Duration {
    if is_answer_tool(tool) {
        MAX_ANSWER_READ_BUDGET
    } else {
        MAX_READ_BUDGET
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadClock {
    UnixMonotonic,
    WindowsPerformanceCounter,
}

/// Worker Query budgets use their existing peer feature-version gate. M3
/// ToolsCall budgets instead require strict refusal by older closed readers,
/// with no missing-budget fallback after sending a supplied parent. Existing
/// service/grant identities still fence the same-machine, same-boot connection.
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
    /// The remaining time of an attempt nested in any tool call. A nested read
    /// (a search, Focus or card call inside an Answer) carries its parent's
    /// attempt, so this accepts up to [`MAX_ANSWER_READ_BUDGET`]; the nested
    /// operation still applies its own limit, and entry points that know the
    /// tool admit with [`Self::remaining_for_tool`].
    pub fn remaining_at(self, clock: ReadClock, now_ns: u64) -> Result<Duration> {
        self.remaining_within(clock, now_ns, MAX_ANSWER_READ_BUDGET)
    }

    /// The remaining time when the tool is known: refuses an attempt longer
    /// than that tool's [`read_budget_for_tool`].
    pub fn remaining_for_tool(self, tool: &str, clock: ReadClock, now_ns: u64) -> Result<Duration> {
        self.remaining_within(clock, now_ns, read_budget_for_tool(tool))
    }

    fn remaining_within(
        self,
        clock: ReadClock,
        now_ns: u64,
        ceiling: Duration,
    ) -> Result<Duration> {
        ensure!(
            self.schema == 1 && self.clock == clock,
            "unsupported native read clock"
        );
        ensure!(
            self.remaining_ns <= ceiling.as_nanos() as u64,
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

#[cfg(test)]
mod tools_call_tests;
