// SPDX-License-Identifier: Apache-2.0
//! Runtime capacity of one stage on one device (plan section 12.5.1).
//!
//! One loaded copy of a model only; throughput beyond one run comes from
//! concurrent runs on that one session. This is the rule that sets how many
//! runs a stage may hold, with the clock and every observation supplied by the
//! caller. It is advisory and grants nothing: the owner of the session admits
//! runs and still checks each run's own budget.
//!
//! - **Cap.** Auto allows at most [`AUTO_MAX_RUNS`] runs per stage until L7
//!   measures more; a user cap may lower it.
//! - **Memory.** A run is added only if the device's free memory after the
//!   run's measured working memory stays at or above the reserve floor, the
//!   larger of 1.5 GB and 10% of the device. Stages sharing a device pass
//!   their combined pending working memory.
//! - **Need.** More than one job has waited for a sustained period, or a
//!   deadline is at risk.
//! - **Proof.** After a run is added, throughput must rise by at least
//!   [`MIN_GAIN`] with every run within budget, or the run is removed.
//! - **Shed fast, recover slowly.** Below the floor, or under another
//!   application's rising load, runs drop to one at once; they come back one
//!   at a time, and only after a quiet period. Shedding stops new admissions
//!   to the extra run and lets running calls finish.

use std::time::{Duration, Instant};

/// Auto capacity per stage until L7 measures more.
pub const AUTO_MAX_RUNS: usize = 2;
/// Minimum throughput gain that keeps an added run.
pub const MIN_GAIN: f64 = 1.3;
/// The fixed part of the reserve floor: 1.5 GB.
pub const RESERVE_FLOOR_BYTES: u64 = 1_500_000_000;
/// Quiet time after a shed or a failed proof before a run is added again.
pub const RECOVERY_QUIET: Duration = Duration::from_secs(120);

/// The reserve floor of a device: the larger of 1.5 GB and 10% of it.
pub fn reserve_floor(device_total_bytes: u64) -> u64 {
    RESERVE_FLOOR_BYTES.max(device_total_bytes / 10)
}

/// What the device and queue show now.
#[derive(Clone, Copy, Debug)]
pub struct CapacityObservation {
    pub now: Instant,
    pub device_total_bytes: u64,
    pub device_free_bytes: u64,
    /// Measured working memory of one more run at the stage's largest input.
    pub run_working_bytes: u64,
    /// Working memory other stages on this device have admitted but not yet
    /// allocated.
    pub pending_other_bytes: u64,
    /// More than one job waited for a sustained period, or a deadline is at risk.
    pub sustained_need: bool,
    /// Another application's device load is rising.
    pub external_pressure: bool,
    /// User cap on runs for this stage, if any.
    pub user_cap: Option<usize>,
}

/// The measured throughput before and after the last added run, once the
/// trial window closes.
#[derive(Clone, Copy, Debug)]
pub struct GainProof {
    pub before_per_second: f64,
    pub after_per_second: f64,
    pub runs_within_budget: bool,
}

/// Why the stage holds the runs it holds. Status shows it per stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapacityReason {
    Single,
    Added,
    KeptProven,
    RemovedNoGain,
    ShedMemory,
    ShedPressure,
    Capped,
    Quiet,
    NoNeed,
    MemoryFloor,
    AwaitingProof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityDecision {
    pub runs: usize,
    pub reason: CapacityReason,
}

/// Per-stage capacity state. Starts at one run.
#[derive(Clone, Copy, Debug)]
pub struct Capacity {
    runs: usize,
    /// The last added run awaits its gain proof.
    trial: bool,
    quiet_until: Option<Instant>,
}
impl Default for Capacity {
    fn default() -> Self {
        Self {
            runs: 1,
            trial: false,
            quiet_until: None,
        }
    }
}
impl Capacity {
    pub fn runs(&self) -> usize {
        self.runs
    }

    /// Applies one observation and, when a trial window has closed, its proof.
    pub fn step(
        &mut self,
        observed: &CapacityObservation,
        proof: Option<GainProof>,
    ) -> CapacityDecision {
        let floor = reserve_floor(observed.device_total_bytes);
        let free = observed
            .device_free_bytes
            .saturating_sub(observed.pending_other_bytes);
        // Shed first and at once: below the floor, or under external load.
        if self.runs > 1 && (free < floor || observed.external_pressure) {
            self.runs = 1;
            self.trial = false;
            self.quiet_until = Some(observed.now + RECOVERY_QUIET);
            let reason = if free < floor {
                CapacityReason::ShedMemory
            } else {
                CapacityReason::ShedPressure
            };
            return self.decision(reason);
        }
        // A lowered user cap applies at once, before any pending trial.
        let cap = observed
            .user_cap
            .unwrap_or(AUTO_MAX_RUNS)
            .clamp(1, AUTO_MAX_RUNS);
        if self.runs > cap {
            self.runs = cap;
            self.trial = false;
            return self.decision(CapacityReason::Capped);
        }
        if self.trial {
            let Some(proof) = proof else {
                return self.decision(CapacityReason::AwaitingProof);
            };
            self.trial = false;
            let gained = proof.before_per_second > 0.0
                && proof.after_per_second >= proof.before_per_second * MIN_GAIN;
            if gained && proof.runs_within_budget {
                return self.decision(CapacityReason::KeptProven);
            }
            self.runs -= 1;
            self.quiet_until = Some(observed.now + RECOVERY_QUIET);
            return self.decision(CapacityReason::RemovedNoGain);
        }
        if self.runs == cap {
            return self.decision(if cap == 1 {
                CapacityReason::Single
            } else {
                CapacityReason::Capped
            });
        }
        if self.quiet_until.is_some_and(|until| observed.now < until) {
            return self.decision(CapacityReason::Quiet);
        }
        if !observed.sustained_need {
            return self.decision(CapacityReason::NoNeed);
        }
        if free.saturating_sub(observed.run_working_bytes) < floor {
            return self.decision(CapacityReason::MemoryFloor);
        }
        // Recover slowly: one run at a time, each on trial.
        self.runs += 1;
        self.trial = true;
        self.decision(CapacityReason::Added)
    }

    fn decision(&self, reason: CapacityReason) -> CapacityDecision {
        CapacityDecision {
            runs: self.runs,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;
    // RTX A6000 physical memory, as observed.
    const A6000: u64 = 51_526_500_352;

    fn observed(now: Instant, free: u64) -> CapacityObservation {
        CapacityObservation {
            now,
            device_total_bytes: A6000,
            device_free_bytes: free,
            run_working_bytes: 2 * GIB,
            pending_other_bytes: 0,
            sustained_need: true,
            external_pressure: false,
            user_cap: None,
        }
    }
    fn proof(gain: f64) -> GainProof {
        GainProof {
            before_per_second: 10.0,
            after_per_second: 10.0 * gain,
            runs_within_budget: true,
        }
    }

    #[test]
    fn the_floor_is_the_larger_of_one_and_a_half_gb_and_ten_percent() {
        assert_eq!(reserve_floor(8 * GIB), RESERVE_FLOOR_BYTES);
        assert_eq!(reserve_floor(A6000), A6000 / 10);
        assert_eq!(reserve_floor(0), RESERVE_FLOOR_BYTES);
    }

    #[test]
    fn a_run_is_added_only_with_need_and_memory_above_the_floor() {
        let now = Instant::now();
        let mut capacity = Capacity::default();
        let mut idle = observed(now, 30 * GIB);
        idle.sustained_need = false;
        assert_eq!(capacity.step(&idle, None).reason, CapacityReason::NoNeed);
        // 6 GiB free leaves 4 GiB after the run: below the A6000's 4.8 GB floor.
        assert_eq!(
            capacity.step(&observed(now, 6 * GIB), None),
            CapacityDecision {
                runs: 1,
                reason: CapacityReason::MemoryFloor
            }
        );
        // Another stage's admitted memory on the same device counts too.
        let mut shared = observed(now, 12 * GIB);
        shared.pending_other_bytes = 6 * GIB;
        assert_eq!(
            capacity.step(&shared, None).reason,
            CapacityReason::MemoryFloor
        );
        assert_eq!(
            capacity.step(&observed(now, 30 * GIB), None),
            CapacityDecision {
                runs: 2,
                reason: CapacityReason::Added
            }
        );
    }

    #[test]
    fn an_added_run_stays_only_with_a_proven_gain() {
        let now = Instant::now();
        let mut kept = Capacity::default();
        kept.step(&observed(now, 30 * GIB), None);
        assert_eq!(
            kept.step(&observed(now, 30 * GIB), None).reason,
            CapacityReason::AwaitingProof
        );
        assert_eq!(
            kept.step(&observed(now, 30 * GIB), Some(proof(1.3))).reason,
            CapacityReason::KeptProven
        );
        assert_eq!(kept.runs(), 2);

        let mut removed = Capacity::default();
        removed.step(&observed(now, 30 * GIB), None);
        assert_eq!(
            removed.step(&observed(now, 30 * GIB), Some(proof(1.29))),
            CapacityDecision {
                runs: 1,
                reason: CapacityReason::RemovedNoGain
            }
        );
        // A run over its budget is removed even with a gain.
        let mut over = Capacity::default();
        over.step(&observed(now, 30 * GIB), None);
        let mut late = proof(2.0);
        late.runs_within_budget = false;
        assert_eq!(
            over.step(&observed(now, 30 * GIB), Some(late)).reason,
            CapacityReason::RemovedNoGain
        );
    }

    #[test]
    fn auto_caps_at_two_and_a_user_cap_lowers_it() {
        let now = Instant::now();
        let mut capacity = Capacity::default();
        capacity.step(&observed(now, 30 * GIB), None);
        capacity.step(&observed(now, 30 * GIB), Some(proof(1.5)));
        assert_eq!(
            capacity.step(&observed(now, 30 * GIB), None),
            CapacityDecision {
                runs: 2,
                reason: CapacityReason::Capped
            }
        );
        let mut one = observed(now, 30 * GIB);
        one.user_cap = Some(1);
        assert_eq!(capacity.step(&one, None).runs, 1);
        assert_eq!(capacity.step(&one, None).reason, CapacityReason::Single);
    }

    #[test]
    fn a_lowered_user_cap_ends_a_pending_trial() {
        let now = Instant::now();
        let mut capacity = Capacity::default();
        capacity.step(&observed(now, 30 * GIB), None);
        assert_eq!(capacity.runs(), 2);
        let mut one = observed(now, 30 * GIB);
        one.user_cap = Some(1);
        // Even with a passing proof in hand, the cap wins.
        assert_eq!(
            capacity.step(&one, Some(proof(2.0))),
            CapacityDecision {
                runs: 1,
                reason: CapacityReason::Capped
            }
        );
        assert_eq!(capacity.step(&one, None).reason, CapacityReason::Single);
    }

    #[test]
    fn injected_pressure_sheds_at_once_and_recovery_waits_for_quiet() {
        let now = Instant::now();
        let mut capacity = Capacity::default();
        capacity.step(&observed(now, 30 * GIB), None);
        capacity.step(&observed(now, 30 * GIB), Some(proof(1.6)));
        assert_eq!(capacity.runs(), 2);
        // Another application takes memory: free falls below the floor.
        assert_eq!(
            capacity.step(&observed(now, 4 * GIB), None),
            CapacityDecision {
                runs: 1,
                reason: CapacityReason::ShedMemory
            }
        );
        // Memory is back, but the quiet period holds the count at one.
        let soon = now + RECOVERY_QUIET - Duration::from_secs(1);
        assert_eq!(
            capacity.step(&observed(soon, 30 * GIB), None).reason,
            CapacityReason::Quiet
        );
        let later = now + RECOVERY_QUIET;
        assert_eq!(capacity.step(&observed(later, 30 * GIB), None).runs, 2);
        // External load sheds too, with its own reason.
        capacity.step(&observed(later, 30 * GIB), Some(proof(1.6)));
        let mut busy = observed(later, 30 * GIB);
        busy.external_pressure = true;
        assert_eq!(
            capacity.step(&busy, None).reason,
            CapacityReason::ShedPressure
        );
    }
}
