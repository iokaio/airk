// SPDX-License-Identifier: Apache-2.0
//! One-use joint NN accounting across the actual original read attempt.
use airk_core::{
    arbiter::Budget,
    combined_context::PartitionAllowance,
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    read_attempt::{ReadAttemptBudget, ReadClock},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

const MAX_ACTIVE_ATTEMPTS: usize = 1024;
const MAX_INPUT_TOKENS: usize = 2048;

#[derive(Clone, Copy)]
pub(super) struct Ticket {
    attempt: Incarnation,
    ordinal: u8,
    expires_ns: u64,
}
impl Ticket {
    pub(super) fn remaining_ns(self, now_ns: u64) -> u64 {
        self.expires_ns.saturating_sub(now_ns)
    }
}
struct Charge {
    tokens: usize,
    compiled: Option<usize>,
    partition: Option<IndexId>,
}
struct Attempt {
    clock: ReadClock,
    original_expiry: u64,
    expiry: u64,
    selection: Option<Sha256Digest>,
    ceiling: usize,
    parent_budget: Vec<u8>,
    charges: BTreeMap<u8, Charge>,
    partitions: BTreeMap<IndexId, (Vec<u8>, usize)>,
}
#[derive(Default)]
pub(super) struct Ledger {
    attempts: BTreeMap<Incarnation, Attempt>,
}
impl Ledger {
    /// Only the owner-authenticated caller may reserve. A lost outcome never
    /// refunds this reservation, and a different partition cannot reuse a slot.
    pub(super) fn reserve(
        &mut self,
        attempt: ReadAttemptBudget,
        parent_budget: &Budget,
        ordinal: u8,
        requested: usize,
        partition: Option<&PartitionAllowance>,
        now_ns: u64,
    ) -> Result<Ticket> {
        parent_budget.validate()?;
        let parent_bytes = serde_json::to_vec(parent_budget)?;
        let budget_window_ns = parent_budget
            .deadline_ms
            .min(30_000)
            .saturating_mul(1_000_000);
        ensure!(
            (1..=parent_budget.rounds.min(3)).contains(&usize::from(ordinal))
                && (1..=parent_budget.input_tokens.min(MAX_INPUT_TOKENS)).contains(&requested),
            "invalid joint NN reservation"
        );
        ensure!(
            !attempt.remaining_at(attempt.clock, now_ns)?.is_zero(),
            "joint NN attempt expired"
        );
        let selection = partition.map(|part| part.selection);
        let ceiling = parent_budget.input_tokens;
        ensure!(ceiling <= MAX_INPUT_TOKENS, "joint NN parent token limit");
        if let Some(part) = partition {
            part.validate_binding(&attempt, part.selection, part.index_id)?;
            ensure!(
                serde_json::to_vec(&part.parent_budget)? == parent_bytes,
                "joint NN partition parent budget changed"
            );
            ensure!(
                part.assessment_ordinals.contains(&ordinal)
                    && requested <= part.resources.input_tokens,
                "joint NN partition allowance exceeded"
            );
        }
        // Keep the tombstone until the ORIGINAL parent expires: a 125s parent
        // cannot renew a consumed 30s assessment window by waiting out our cap.
        self.attempts
            .retain(|_, state| state.original_expiry > now_ns);
        if !self.attempts.contains_key(&attempt.request_id) {
            ensure!(
                self.attempts.len() < MAX_ACTIVE_ATTEMPTS,
                "joint NN ledger at capacity"
            );
            self.attempts.insert(
                attempt.request_id,
                Attempt {
                    clock: attempt.clock,
                    original_expiry: attempt.expires_monotonic_ns,
                    expiry: attempt
                        .expires_monotonic_ns
                        .min(now_ns.saturating_add(attempt.remaining_ns.min(budget_window_ns))),
                    selection,
                    ceiling,
                    parent_budget: parent_bytes.clone(),
                    charges: BTreeMap::new(),
                    partitions: BTreeMap::new(),
                },
            );
        }
        let state = self
            .attempts
            .get_mut(&attempt.request_id)
            .context("joint NN attempt unavailable")?;
        ensure!(
            state.clock == attempt.clock
                && state.original_expiry == attempt.expires_monotonic_ns
                && state.selection == selection
                && state.ceiling == ceiling
                && state.parent_budget == parent_bytes
                && now_ns < state.expiry,
            "joint NN parent identity changed"
        );
        state.expiry = state
            .expiry
            .min(now_ns.saturating_add(attempt.remaining_ns.min(budget_window_ns)));
        ensure!(
            !state.charges.contains_key(&ordinal),
            "joint NN ordinal already used"
        );
        let used: usize = state.charges.values().map(|charge| charge.tokens).sum();
        ensure!(
            used.checked_add(requested)
                .is_some_and(|sum| sum <= ceiling),
            "joint NN token budget exhausted"
        );
        if let Some(part) = partition {
            let bytes = serde_json::to_vec(part)?;
            if let Some((captured, tokens)) = state.partitions.get(&part.index_id) {
                ensure!(
                    captured == &bytes && *tokens == part.resources.input_tokens,
                    "joint NN partition changed"
                );
            }
            let used: usize = state
                .charges
                .values()
                .filter(|charge| charge.partition == Some(part.index_id))
                .map(|charge| charge.tokens)
                .sum();
            ensure!(
                used.checked_add(requested)
                    .is_some_and(|sum| sum <= part.resources.input_tokens),
                "joint NN partition tokens exhausted"
            );
            state
                .partitions
                .entry(part.index_id)
                .or_insert((bytes, part.resources.input_tokens));
        }
        state.charges.insert(
            ordinal,
            Charge {
                tokens: requested,
                compiled: None,
                partition: partition.map(|part| part.index_id),
            },
        );
        Ok(Ticket {
            attempt: attempt.request_id,
            ordinal,
            expires_ns: state.expiry,
        })
    }
    /// Record an actual compile receipt without refunding unknown native work.
    pub(super) fn compiled(&mut self, ticket: Ticket, tokens: usize) -> Result<()> {
        let charge = self
            .attempts
            .get_mut(&ticket.attempt)
            .and_then(|state| state.charges.get_mut(&ticket.ordinal))
            .context("joint NN reservation unavailable")?;
        ensure!(
            tokens > 0 && tokens <= charge.tokens,
            "joint NN compiled token count exceeds reservation"
        );
        ensure!(
            charge.compiled.is_none_or(|previous| previous == tokens),
            "joint NN compiled receipt changed"
        );
        charge.compiled = Some(tokens);
        Ok(())
    }
    /// Only observed native completion may refund unused token reservation.
    pub(super) fn completed(&mut self, ticket: Ticket, tokens: usize) -> Result<()> {
        self.compiled(ticket, tokens)?;
        self.attempts
            .get_mut(&ticket.attempt)
            .and_then(|state| state.charges.get_mut(&ticket.ordinal))
            .context("joint NN reservation unavailable")?
            .tokens = tokens;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
