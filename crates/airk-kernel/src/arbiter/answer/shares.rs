// SPDX-License-Identifier: Apache-2.0
//! Budget shares: each part gets an equal share of the Answer's reads and
//! model resources, with the remainder to the lowest part; a part that
//! finishes early hands its unused share back to the parts still running.
//! A reserve keeps time for Compose and Union. Time is not cut into fixed
//! slices, because reads overlap: model time is re-divided every turn.
use super::*;

/// Time kept back from gathering for Compose, Union and the last dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reserve {
    pub(crate) compose_union_ms: u64,
    pub(crate) dispatch_ms: u64,
}
impl Reserve {
    /// `max(3 s, 3%) + 1 s + 250 ms a part`, and a 5 s dispatch reserve, never
    /// more than a quarter of the deadline together.
    pub(crate) fn new(deadline_ms: u64, parts: usize) -> Self {
        let compose = (deadline_ms * 3 / 100).max(3_000) + 1_000 + 250 * parts as u64;
        let dispatch = 5_000;
        let ceiling = deadline_ms / 4;
        if compose + dispatch <= ceiling {
            return Self {
                compose_union_ms: compose,
                dispatch_ms: dispatch,
            };
        }
        // Scale both down to the ceiling, keeping their proportion.
        let total = compose + dispatch;
        Self {
            compose_union_ms: ceiling * compose / total,
            dispatch_ms: ceiling * dispatch / total,
        }
    }
    pub(crate) fn total_ms(self) -> u64 {
        self.compose_union_ms + self.dispatch_ms
    }
}

/// One part's allowance of the countable resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Share {
    pub(crate) calls: usize,
    pub(crate) card_slots: usize,
    pub(crate) focus_pairs: usize,
    pub(crate) tokens: usize,
    pub(crate) deadline_ms: u64,
    pub(crate) done: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Shares {
    pub(crate) reserve: Reserve,
    pub(crate) parts: Vec<Share>,
}

fn split(total: usize, parts: usize) -> Vec<usize> {
    let each = total / parts;
    let mut out = vec![each; parts];
    out[0] += total - each * parts;
    out
}

impl Shares {
    /// Totals minus the reserve, divided equally, the remainder to part 1.
    /// The Split call is charged before the shares, so `card_slots` and
    /// `tokens` are what is left after it.
    pub(crate) fn allocate(
        budget: &v3::BudgetV3,
        parts: usize,
        card_slots: usize,
        tokens: usize,
    ) -> Self {
        let parts = parts.max(1);
        let reserve = Reserve::new(budget.deadline_ms, parts);
        let time = budget.deadline_ms.saturating_sub(reserve.total_ms());
        let calls = split(budget.calls, parts);
        let slots = split(card_slots, parts);
        let pairs = split(budget.focus_pairs, parts);
        let tokens = split(tokens, parts);
        let times = split(time as usize, parts);
        Self {
            reserve,
            parts: (0..parts)
                .map(|index| Share {
                    calls: calls[index],
                    card_slots: slots[index],
                    focus_pairs: pairs[index],
                    tokens: tokens[index],
                    deadline_ms: times[index] as u64,
                    done: false,
                })
                .collect(),
        }
    }

    pub(crate) fn active(&self) -> usize {
        self.parts.iter().filter(|share| !share.done).count()
    }

    /// Part `index` (0-based) is done: what it did not use goes, in equal
    /// shares with the remainder to the lowest, to the parts still active.
    /// `used` is what it consumed of each counter, `elapsed_ms` the time it
    /// ran; its deadline share keeps only what it used.
    pub(crate) fn finish(&mut self, index: usize, used: Share, elapsed_ms: u64) {
        let share = self.parts[index];
        if share.done {
            return;
        }
        let unused = Share {
            calls: share.calls.saturating_sub(used.calls),
            card_slots: share.card_slots.saturating_sub(used.card_slots),
            focus_pairs: share.focus_pairs.saturating_sub(used.focus_pairs),
            tokens: share.tokens.saturating_sub(used.tokens),
            deadline_ms: share.deadline_ms.saturating_sub(elapsed_ms),
            done: true,
        };
        self.parts[index] = Share {
            calls: share.calls - unused.calls,
            card_slots: share.card_slots - unused.card_slots,
            focus_pairs: share.focus_pairs - unused.focus_pairs,
            tokens: share.tokens - unused.tokens,
            deadline_ms: share.deadline_ms - unused.deadline_ms,
            done: true,
        };
        let active: Vec<usize> = (0..self.parts.len())
            .filter(|other| !self.parts[*other].done)
            .collect();
        if active.is_empty() {
            return;
        }
        let give = |total: usize| split(total, active.len());
        let calls = give(unused.calls);
        let slots = give(unused.card_slots);
        let pairs = give(unused.focus_pairs);
        let tokens = give(unused.tokens);
        let times = give(unused.deadline_ms as usize);
        for (position, other) in active.into_iter().enumerate() {
            let target = &mut self.parts[other];
            target.calls += calls[position];
            target.card_slots += slots[position];
            target.focus_pairs += pairs[position];
            target.tokens += tokens[position];
            target.deadline_ms += times[position] as u64;
        }
    }

    /// A part's model allowance for this turn: the time left before the
    /// reserve, divided among the active parts, then by its rounds left.
    pub(crate) fn round_model_ms(&self, left_ms: u64, rounds_left: usize) -> u64 {
        left_ms.saturating_sub(self.reserve.compose_union_ms)
            / self.active().max(1) as u64
            / rounds_left.max(1) as u64
    }
}

/// Measured model cost per card task, an exponentially weighted mean that
/// starts at the first-Context estimate.
#[derive(Clone, Debug, Default)]
pub(crate) struct CostModel {
    measured: BTreeMap<u8, u64>,
}
impl CostModel {
    pub(crate) fn estimate(&self, stage: v3::LoopStage) -> u64 {
        self.measured
            .get(&(stage as u8))
            .copied()
            .unwrap_or(software::FIRST_CONTEXT_ESTIMATE_MS)
    }
    pub(crate) fn observe(&mut self, stage: v3::LoopStage, elapsed_ms: u64) {
        let entry = self.measured.entry(stage as u8).or_insert(elapsed_ms);
        *entry = (*entry * 3 + elapsed_ms) / 4;
    }
}

use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
