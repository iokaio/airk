// SPDX-License-Identifier: Apache-2.0
//! One-use Answer /3 model accounting across the actual original read
//! attempt: card slots up to the parent's `context_calls`, Focus calls up to
//! its `focus_calls`, one attempt token pool up to `input_tokens`, and a
//! per-part, per-round allowance of `round_input_tokens`. A batched call
//! charges each part and round for its own candidates. A lost outcome never
//! refunds its reservation; only observed completion re-attributes it.
//!
//! This is the /3 sibling of `joint_ledger.rs`, which keeps serving Answer
//! /2 until it is deleted (L3-10); the two never share a slot or a pool.
use airk_core::{
    arbiter::answer_loop::{BudgetV3, CONTEXT_CALL_TOKENS, MAX_DEADLINE_MS, TokenAttribution},
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    native_loop::PartitionAllowanceV3,
    read_attempt::{ReadAttemptBudget, ReadClock},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

const MAX_ACTIVE_ATTEMPTS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Card(u8),
    Focus(IndexId, u8),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Ticket {
    attempt: Incarnation,
    slot: Slot,
    expires_ns: u64,
}
impl Ticket {
    pub(super) fn remaining_ns(self, now_ns: u64) -> u64 {
        self.expires_ns.saturating_sub(now_ns)
    }
}

struct Charge {
    /// The current per-(part, round) charge: the reservation until observed
    /// completion re-attributes it, zero after a known refusal.
    attribution: Vec<TokenAttribution>,
    compiled: Option<usize>,
    partition: Option<IndexId>,
}
impl Charge {
    fn tokens(&self) -> usize {
        self.attribution.iter().map(|charge| charge.tokens).sum()
    }
}

struct Attempt {
    clock: ReadClock,
    original_expiry: u64,
    expiry: u64,
    selection: Option<Sha256Digest>,
    parent_budget: Vec<u8>,
    charges: BTreeMap<Slot, Charge>,
    partitions: BTreeMap<IndexId, Vec<u8>>,
}
impl Attempt {
    fn used_for(&self, part: u8, round: u8) -> usize {
        self.charges
            .values()
            .flat_map(|charge| &charge.attribution)
            .filter(|charge| (charge.part, charge.round) == (part, round))
            .map(|charge| charge.tokens)
            .sum()
    }
}

/// Distribute `tokens` over `weights` in proportion, the remainder to the
/// first (lowest part and round). The same rule the kernel reserves by.
pub(super) fn attribute(tokens: usize, weights: &[TokenAttribution]) -> Vec<TokenAttribution> {
    let mut sorted = weights.to_vec();
    sorted.sort_by_key(|charge| (charge.part, charge.round));
    let total: usize = sorted.iter().map(|charge| charge.tokens).sum();
    let mut out: Vec<TokenAttribution> = sorted
        .iter()
        .map(|charge| TokenAttribution {
            tokens: if total == 0 {
                0
            } else {
                // u128 keeps the product exact for any token count.
                (tokens as u128 * charge.tokens as u128 / total as u128) as usize
            },
            ..*charge
        })
        .collect();
    let given: usize = out.iter().map(|charge| charge.tokens).sum();
    if let Some(first) = out.first_mut() {
        first.tokens += tokens - given.min(tokens);
    }
    out
}

#[derive(Default)]
pub(super) struct Ledger {
    attempts: BTreeMap<Incarnation, Attempt>,
}
impl Ledger {
    /// Reserve one card call's slot and tokens. Only the owner-authenticated
    /// caller may reserve; a slot is never reusable.
    pub(super) fn reserve_card(
        &mut self,
        attempt: ReadAttemptBudget,
        parent: &BudgetV3,
        card_slot: u8,
        attribution: &[TokenAttribution],
        partition: Option<&PartitionAllowanceV3>,
        now_ns: u64,
    ) -> Result<Ticket> {
        self.reserve(
            attempt,
            parent,
            Slot::Card(card_slot),
            attribution,
            partition,
            now_ns,
        )
    }

    /// Reserve one batched Focus call's ordinal and tokens.
    pub(super) fn reserve_focus(
        &mut self,
        attempt: ReadAttemptBudget,
        parent: &BudgetV3,
        index_id: IndexId,
        ordinal: u8,
        attribution: &[TokenAttribution],
        partition: Option<&PartitionAllowanceV3>,
        now_ns: u64,
    ) -> Result<Ticket> {
        if let Some(part) = partition {
            ensure!(
                part.index_id == index_id,
                "Focus reservation uses a foreign partition"
            );
        }
        self.reserve(
            attempt,
            parent,
            Slot::Focus(index_id, ordinal),
            attribution,
            partition,
            now_ns,
        )
    }

    fn reserve(
        &mut self,
        attempt: ReadAttemptBudget,
        parent: &BudgetV3,
        slot: Slot,
        attribution: &[TokenAttribution],
        partition: Option<&PartitionAllowanceV3>,
        now_ns: u64,
    ) -> Result<Ticket> {
        parent.validate()?;
        let parent_bytes = serde_json::to_vec(parent)?;
        let window_ns = parent
            .deadline_ms
            .min(MAX_DEADLINE_MS)
            .saturating_mul(1_000_000);
        ensure!(
            !attribution.is_empty()
                && attribution.windows(2).all(|pair| {
                    (pair[0].part, pair[0].round) < (pair[1].part, pair[1].round)
                }),
            "loop reservation attribution must be ordered and unique per part and round"
        );
        let requested: usize = attribution.iter().map(|charge| charge.tokens).sum();
        let (valid_slot, call_ceiling) = match slot {
            Slot::Card(card) => (
                (1..=parent.context_calls).contains(&usize::from(card)),
                CONTEXT_CALL_TOKENS,
            ),
            Slot::Focus(_, ordinal) => (
                (1..=parent.focus_calls).contains(&usize::from(ordinal))
                    && attribution
                        .iter()
                        .all(|charge| charge.part >= 1 && charge.round >= 1),
                parent.round_input_tokens,
            ),
        };
        ensure!(
            valid_slot && (1..=call_ceiling.min(parent.input_tokens)).contains(&requested),
            "invalid loop model reservation"
        );
        ensure!(
            !attempt.remaining_at(attempt.clock, now_ns)?.is_zero(),
            "loop model attempt expired"
        );
        let selection = partition.map(|part| part.selection);
        if let Some(part) = partition {
            part.validate_binding(&attempt, part.selection, part.index_id)?;
            ensure!(
                serde_json::to_vec(&part.parent_budget)? == parent_bytes,
                "loop partition parent budget changed"
            );
            ensure!(
                (match slot {
                    Slot::Card(card) => part.card_ordinals.contains(&card),
                    Slot::Focus(_, ordinal) => part.focus_ordinals.contains(&ordinal),
                }) && requested <= part.resources.input_tokens,
                "loop partition allowance exceeded"
            );
        }
        // The tombstone lives until the ORIGINAL parent expires, so waiting
        // out a window never renews a consumed slot.
        self.attempts
            .retain(|_, state| state.original_expiry > now_ns);
        if !self.attempts.contains_key(&attempt.request_id) {
            ensure!(
                self.attempts.len() < MAX_ACTIVE_ATTEMPTS,
                "loop model ledger at capacity"
            );
            self.attempts.insert(
                attempt.request_id,
                Attempt {
                    clock: attempt.clock,
                    original_expiry: attempt.expires_monotonic_ns,
                    expiry: attempt
                        .expires_monotonic_ns
                        .min(now_ns.saturating_add(attempt.remaining_ns.min(window_ns))),
                    selection,
                    parent_budget: parent_bytes.clone(),
                    charges: BTreeMap::new(),
                    partitions: BTreeMap::new(),
                },
            );
        }
        let state = self
            .attempts
            .get_mut(&attempt.request_id)
            .context("loop model attempt unavailable")?;
        ensure!(
            state.clock == attempt.clock
                && state.original_expiry == attempt.expires_monotonic_ns
                && state.selection == selection
                && state.parent_budget == parent_bytes
                && now_ns < state.expiry,
            "loop model parent identity changed"
        );
        state.expiry = state
            .expiry
            .min(now_ns.saturating_add(attempt.remaining_ns.min(window_ns)));
        ensure!(
            !state.charges.contains_key(&slot),
            "loop model slot already used"
        );
        let used: usize = state.charges.values().map(Charge::tokens).sum();
        ensure!(
            used.checked_add(requested)
                .is_some_and(|sum| sum <= parent.input_tokens),
            "loop model token pool exhausted"
        );
        for charge in attribution {
            ensure!(
                charge.round == 0
                    || state.used_for(charge.part, charge.round) + charge.tokens
                        <= parent.round_input_tokens,
                "loop part-round token allowance exhausted"
            );
        }
        if let Some(part) = partition {
            let bytes = serde_json::to_vec(part)?;
            if let Some(captured) = state.partitions.get(&part.index_id) {
                ensure!(captured == &bytes, "loop partition changed");
            }
            let used: usize = state
                .charges
                .values()
                .filter(|charge| charge.partition == Some(part.index_id))
                .map(Charge::tokens)
                .sum();
            ensure!(
                used.checked_add(requested)
                    .is_some_and(|sum| sum <= part.resources.input_tokens),
                "loop partition tokens exhausted"
            );
            state.partitions.entry(part.index_id).or_insert(bytes);
        }
        state.charges.insert(
            slot,
            Charge {
                attribution: attribution.to_vec(),
                compiled: None,
                partition: partition.map(|part| part.index_id),
            },
        );
        Ok(Ticket {
            attempt: attempt.request_id,
            slot,
            expires_ns: state.expiry,
        })
    }

    fn charge(&mut self, ticket: Ticket) -> Result<&mut Charge> {
        self.attempts
            .get_mut(&ticket.attempt)
            .and_then(|state| state.charges.get_mut(&ticket.slot))
            .context("loop model reservation unavailable")
    }

    /// Record an actual compile receipt; unknown native work keeps its hold.
    pub(super) fn compiled(&mut self, ticket: Ticket, tokens: usize) -> Result<()> {
        let charge = self.charge(ticket)?;
        ensure!(
            tokens > 0 && tokens <= charge.tokens(),
            "loop compiled token count exceeds reservation"
        );
        ensure!(
            charge.compiled.is_none_or(|previous| previous == tokens),
            "loop compiled receipt changed"
        );
        charge.compiled = Some(tokens);
        Ok(())
    }

    /// The lane refused a card input before any model work: nothing was
    /// consumed, so every part-round charge is released. The slot stays used.
    pub(super) fn refused(&mut self, ticket: Ticket) -> Result<()> {
        ensure!(
            matches!(ticket.slot, Slot::Card(_)),
            "Only a card call has an input-ceiling refusal"
        );
        let charge = self.charge(ticket)?;
        ensure!(
            charge.compiled.is_none(),
            "loop refusal contradicts its compile receipt"
        );
        for part in &mut charge.attribution {
            part.tokens = 0;
        }
        Ok(())
    }

    /// Only observed completion may refund unused reservation. Returns the
    /// observed per-part, per-round attribution, which sums to `tokens`.
    pub(super) fn completed(
        &mut self,
        ticket: Ticket,
        tokens: usize,
    ) -> Result<Vec<TokenAttribution>> {
        self.compiled(ticket, tokens)?;
        let charge = self.charge(ticket)?;
        charge.attribution = attribute(tokens, &charge.attribution);
        Ok(charge.attribution.clone())
    }

    /// A Focus batch reports monotonic aggregate receipts for its pairs.
    pub(super) fn compiled_prefix(&mut self, ticket: Ticket, tokens: usize) -> Result<()> {
        ensure!(
            matches!(ticket.slot, Slot::Focus(_, _)),
            "Only Focus has pair-prefix accounting"
        );
        let charge = self.charge(ticket)?;
        ensure!(
            tokens > 0
                && tokens <= charge.tokens()
                && charge.compiled.is_none_or(|previous| previous <= tokens),
            "Focus pair receipts exceed or regress the reservation"
        );
        charge.compiled = Some(tokens);
        Ok(())
    }

    /// Settle a Focus batch at its observed total, attributed by `weights`
    /// (each part-round's scored pairs). Returns the observed attribution.
    pub(super) fn completed_prefix(
        &mut self,
        ticket: Ticket,
        tokens: usize,
        weights: &[TokenAttribution],
    ) -> Result<Vec<TokenAttribution>> {
        self.compiled_prefix(ticket, tokens)?;
        let charge = self.charge(ticket)?;
        let reserved: Vec<(u8, u8)> = charge
            .attribution
            .iter()
            .map(|charge| (charge.part, charge.round))
            .collect();
        ensure!(
            !weights.is_empty()
                && weights
                    .iter()
                    .all(|weight| reserved.contains(&(weight.part, weight.round))),
            "Focus attribution names a part-round it did not reserve"
        );
        charge.attribution = attribute(tokens, weights);
        Ok(charge.attribution.clone())
    }
}

#[cfg(test)]
mod tests;
