// SPDX-License-Identifier: Apache-2.0
//! Combined Answer /3: whole-selection allowances, the `airk.combined-context/2`
//! summary and the delivery bindings of an Answer /3 reply. These facts confer
//! no authority. Each member runs its own Answer under a
//! `PartitionAllowanceV3`; unused work is handed back by count.
use super::{
    ContextDeliveryBinding, MAX_DELIVERY_BINDINGS, Termination, UnqueriedReason, share,
    validate_members, validate_stage_roster,
};
use crate::{
    arbiter::{
        Stage,
        answer_loop::{self as v3, BudgetV3, ContextResponseV3},
    },
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    native_loop::{ALLOWANCE_SCHEMA_V3, LoopResources, PartitionAllowanceV3, ordinal_prefix},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SUMMARY_SCHEMA_V2: &str = "airk.combined-context/2";

/// The counters of a member's Answer /3 work, or of what may be spent: the
/// allowance's resources plus its card slots and Focus calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopUse {
    pub calls: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub bytes: usize,
    pub card_slots: usize,
    pub focus_calls: usize,
    pub focus_pairs: usize,
}
fn overflow() -> anyhow::Error {
    anyhow::anyhow!("combined Answer accounting overflow")
}
impl LoopUse {
    /// What a member reply used, by count: reads, tokens, candidates and
    /// bytes from its work, card slots and Focus calls and pairs from its
    /// decision log summary.
    pub fn of_reply(reply: &ContextResponseV3) -> Result<Self> {
        Ok(Self {
            calls: reply.work.calls,
            input_tokens: reply.work.input_tokens,
            candidates: reply.candidate_count.max(reply.work.candidates),
            bytes: reply.work.bytes.max(serde_json::to_vec(reply)?.len()),
            card_slots: reply.decision_log.card_calls,
            focus_calls: reply.decision_log.focus_calls,
            focus_pairs: reply.decision_log.focus_pairs,
        })
    }
    pub fn allowance(allowance: &PartitionAllowanceV3) -> Self {
        let resources = &allowance.resources;
        Self {
            calls: resources.calls,
            input_tokens: resources.input_tokens,
            candidates: resources.candidates,
            bytes: resources.bytes,
            card_slots: allowance.card_slots(),
            focus_calls: allowance.focus_calls(),
            focus_pairs: resources.focus_pairs,
        }
    }
    pub fn budget(budget: &BudgetV3) -> Self {
        Self {
            calls: budget.calls,
            input_tokens: budget.input_tokens,
            candidates: budget.candidates,
            bytes: budget.max_bytes,
            card_slots: budget.context_calls,
            focus_calls: budget.focus_calls,
            focus_pairs: budget.focus_pairs,
        }
    }
    fn fields(&self) -> [usize; 7] {
        [
            self.calls,
            self.input_tokens,
            self.candidates,
            self.bytes,
            self.card_slots,
            self.focus_calls,
            self.focus_pairs,
        ]
    }
    fn from_fields(fields: [usize; 7]) -> Self {
        let [
            calls,
            input_tokens,
            candidates,
            bytes,
            card_slots,
            focus_calls,
            focus_pairs,
        ] = fields;
        Self {
            calls,
            input_tokens,
            candidates,
            bytes,
            card_slots,
            focus_calls,
            focus_pairs,
        }
    }
    pub fn checked_add(&self, other: &Self) -> Result<Self> {
        let (a, b) = (self.fields(), other.fields());
        let mut sum = [0; 7];
        for index in 0..7 {
            sum[index] = a[index].checked_add(b[index]).ok_or_else(overflow)?;
        }
        Ok(Self::from_fields(sum))
    }
    /// What is left of `self` after `used`; an overrun is an error.
    pub fn checked_sub(&self, used: &Self) -> Result<Self> {
        let (a, b) = (self.fields(), used.fields());
        let mut left = [0; 7];
        for index in 0..7 {
            left[index] = a[index]
                .checked_sub(b[index])
                .ok_or_else(|| anyhow::anyhow!("combined Answer member overran its allowance"))?;
        }
        Ok(Self::from_fields(left))
    }
    pub fn fits(&self, ceiling: &Self) -> bool {
        self.fields()
            .iter()
            .zip(ceiling.fields())
            .all(|(value, ceiling)| *value <= ceiling)
    }
}

/// A member allowance grown by work an earlier member handed back. Card
/// slots and Focus calls stay the prefixes `1..=k`.
pub fn grow_allowance(allowance: &mut PartitionAllowanceV3, returned: &LoopUse) -> Result<()> {
    let total = LoopUse::allowance(allowance).checked_add(returned)?;
    set_counts(allowance, &total)
}
/// An allowance made only of handed-back work, for a member the initial
/// allocation could not admit. None when it cannot carry one read.
pub fn returned_allowance(
    attempt: Incarnation,
    selection: Sha256Digest,
    index_id: IndexId,
    budget: &BudgetV3,
    returned: &LoopUse,
    minimum_bytes: usize,
) -> Result<Option<PartitionAllowanceV3>> {
    // A member needs room for the question and a minimal reply (review
    // finding 3 of L3-10 (a)); `minimum_bytes` is the caller's
    // `question.len() + MIN_REPLY_SKELETON_BYTES`.
    if returned.calls == 0 || returned.candidates == 0 || returned.bytes < minimum_bytes.max(1024)
    {
        return Ok(None);
    }
    let mut allowance = PartitionAllowanceV3 {
        schema: ALLOWANCE_SCHEMA_V3.into(),
        attempt,
        selection,
        index_id,
        parent_budget: budget.clone(),
        resources: LoopResources::default(),
        card_ordinals: vec![],
        focus_ordinals: vec![],
    };
    set_counts(&mut allowance, returned)?;
    Ok(Some(allowance))
}
/// No model work: after unknown model work no later member may spend the
/// token pool, so its slots, Focus calls, pairs and tokens are withdrawn.
pub fn without_models(allowance: &mut PartitionAllowanceV3) {
    allowance.card_ordinals.clear();
    allowance.focus_ordinals.clear();
    allowance.resources.focus_pairs = 0;
    allowance.resources.input_tokens = 0;
}
fn set_counts(allowance: &mut PartitionAllowanceV3, total: &LoopUse) -> Result<()> {
    allowance.resources = LoopResources {
        calls: total.calls,
        input_tokens: total.input_tokens,
        candidates: total.candidates,
        bytes: total.bytes,
        focus_pairs: total.focus_pairs,
    };
    allowance.card_ordinals = ordinal_prefix(total.card_slots)?;
    allowance.focus_ordinals = ordinal_prefix(total.focus_calls)?;
    // A Focus call needs a pair, and model work needs tokens: counts that
    // cannot be spent are dropped, never minted elsewhere.
    if allowance.resources.focus_pairs < allowance.focus_ordinals.len() {
        allowance
            .focus_ordinals
            .truncate(allowance.resources.focus_pairs);
    }
    if allowance.focus_ordinals.is_empty() {
        allowance.resources.focus_pairs = 0;
    }
    if allowance.resources.input_tokens == 0 {
        without_models(allowance);
    } else if allowance.card_ordinals.is_empty() && allowance.focus_ordinals.is_empty() {
        allowance.resources.input_tokens = 0;
    }
    Ok(())
}

/// Pure initial reservations of a combined Answer /3, not execution. Reads,
/// candidates and bytes are shared over the admitted members; card slots,
/// Focus calls and Focus pairs over the members that can carry model work,
/// with the token pool shared over the same members.
pub fn initial_allocations_v3(
    attempt: Incarnation,
    selection: Sha256Digest,
    members: &[IndexId],
    budget: &BudgetV3,
) -> Result<Vec<Option<PartitionAllowanceV3>>> {
    budget.validate()?;
    validate_members(members)?;
    let admitted = members
        .len()
        .min(budget.calls)
        .min(budget.candidates)
        .min(budget.max_bytes / 1024);
    let model_members = admitted
        .min(budget.context_calls.max(budget.focus_calls))
        .min(budget.input_tokens);
    // Model work is shared over the members that can carry it, and Focus
    // pairs over the members that get a Focus call (review finding 6 of
    // L3-10 (a): sharing over every admitted member stranded the rest).
    let focus_members = model_members.min(budget.focus_calls);
    let mut result = Vec::with_capacity(members.len());
    for (offset, index_id) in members.iter().copied().enumerate() {
        if offset >= admitted {
            result.push(None);
            continue;
        }
        let model = offset < model_members;
        let slots = if model {
            share(budget.context_calls, model_members, offset)
        } else {
            0
        };
        let focus_calls = if model {
            share(budget.focus_calls, model_members, offset)
        } else {
            0
        };
        let focus_pairs = if focus_calls == 0 {
            0
        } else {
            share(budget.focus_pairs, focus_members, offset)
        };
        let allowance = PartitionAllowanceV3 {
            schema: ALLOWANCE_SCHEMA_V3.into(),
            attempt,
            selection,
            index_id,
            parent_budget: budget.clone(),
            resources: LoopResources {
                calls: share(budget.calls, admitted, offset),
                input_tokens: if slots > 0 || focus_calls > 0 {
                    share(budget.input_tokens, model_members, offset)
                } else {
                    0
                },
                candidates: share(budget.candidates, admitted, offset),
                bytes: share(budget.max_bytes, admitted, offset),
                focus_pairs,
            },
            card_ordinals: ordinal_prefix(slots)?,
            focus_ordinals: ordinal_prefix(focus_calls)?,
        };
        allowance.validate()?;
        result.push(Some(allowance));
    }
    // Independent of individual validation: prove nothing was minted.
    let mut total = LoopUse::default();
    for allowance in result.iter().flatten() {
        total = total.checked_add(&LoopUse::allowance(allowance))?;
    }
    ensure!(
        total.fits(&LoopUse::budget(budget)),
        "initial combined Answer allocation exceeds the budget"
    );
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartitionOutcomeV3 {
    Unqueried {
        reason: UnqueriedReason,
    },
    Known {
        used: LoopUse,
        read_depth: usize,
        peak_fanout: usize,
    },
    /// The entire allowance stays charged; no zero-work or replay inference.
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionReportV3 {
    pub index_id: IndexId,
    pub allowance: Option<PartitionAllowanceV3>,
    pub outcome: PartitionOutcomeV3,
}

/// The combined Answer /3 summary beside the members' full replies.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedContextSummaryV2 {
    pub schema: String,
    pub attempt: Incarnation,
    pub selection: Sha256Digest,
    pub budget: BudgetV3,
    pub members: Vec<PartitionReportV3>,
    pub known_used: LoopUse,
    pub unknown_held: LoopUse,
    pub termination: Termination,
    pub delivery_bindings: Vec<ContextDeliveryBinding>,
}
impl CombinedContextSummaryV2 {
    pub fn validate(&self, expected_members: &[IndexId]) -> Result<()> {
        self.budget.validate()?;
        ensure!(
            self.schema == SUMMARY_SCHEMA_V2,
            "invalid combined context schema"
        );
        validate_members(expected_members)?;
        ensure!(
            self.members
                .iter()
                .map(|member| member.index_id)
                .eq(expected_members.iter().copied()),
            "combined context roster mismatch"
        );
        let mut known = LoopUse::default();
        let mut held = LoopUse::default();
        let mut complete = true;
        // Members run in roster order. After unknown model work no later
        // member may hold model work (review finding 7 of L3-10 (a)): an
        // unknown member's token use is not bounded by its accounting.
        let mut unknown_seen = false;
        for report in &self.members {
            if unknown_seen && let Some(allowance) = &report.allowance {
                ensure!(
                    allowance.card_ordinals.is_empty()
                        && allowance.focus_ordinals.is_empty()
                        && allowance.resources.input_tokens == 0
                        && allowance.resources.focus_pairs == 0,
                    "model work allowed after unknown model work"
                );
            }
            unknown_seen |= matches!(report.outcome, PartitionOutcomeV3::Unknown);
            match (&report.allowance, &report.outcome) {
                (None, PartitionOutcomeV3::Unqueried { reason }) => {
                    ensure!(
                        *reason != UnqueriedReason::AccountingViolation,
                        "an Answer /3 member is never refused for a known violation"
                    );
                    complete = false;
                }
                (Some(allowance), outcome) => {
                    allowance.validate()?;
                    ensure!(
                        allowance.attempt == self.attempt
                            && allowance.selection == self.selection
                            && allowance.index_id == report.index_id
                            && allowance.parent_budget == self.budget,
                        "foreign context partition allocation"
                    );
                    match outcome {
                        PartitionOutcomeV3::Known {
                            used,
                            read_depth,
                            peak_fanout,
                        } => {
                            ensure!(
                                used.fits(&LoopUse::allowance(allowance))
                                    && *read_depth <= self.budget.rounds
                                    && *peak_fanout <= self.budget.fanout,
                                "partition work exceeds allocation"
                            );
                            ensure!(
                                (used.card_slots == 0 && used.focus_calls == 0)
                                    || used.input_tokens > 0,
                                "model work has no actual input charge"
                            );
                            known = known.checked_add(used)?;
                        }
                        PartitionOutcomeV3::Unknown => {
                            held = held.checked_add(&LoopUse::allowance(allowance))?;
                            complete = false;
                        }
                        PartitionOutcomeV3::Unqueried { .. } => {
                            anyhow::bail!("unqueried member retains dispatched allocation")
                        }
                    }
                }
                _ => anyhow::bail!("dispatched partition has no allocation"),
            }
        }
        ensure!(
            known == self.known_used
                && held == self.unknown_held
                && known
                    .checked_add(&held)?
                    .fits(&LoopUse::budget(&self.budget)),
            "combined context accounting mismatch"
        );
        ensure!(
            self.termination != Termination::AccountingViolation,
            "an Answer /3 summary has no known-violation terminal"
        );
        ensure!(
            self.termination != Termination::Complete || complete,
            "incomplete partition roster cannot claim completion"
        );
        ensure!(
            self.delivery_bindings.len() <= MAX_DELIVERY_BINDINGS,
            "context stage delivery roster limit"
        );
        let mut bound = BTreeSet::new();
        for binding in &self.delivery_bindings {
            ensure!(
                expected_members.contains(&binding.scope.index_id)
                    && bound.insert((binding.stage, binding.scope.index_id))
                    && binding.scope.configuration_revision > 0,
                "invalid context delivery policy binding"
            );
        }
        Ok(())
    }
}

/// The summary a combined context wrapper carries: `airk.combined-context/2`
/// for Answer /3, or `/1` for the Answer /2 path until it is deleted.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CombinedContext {
    Answer(CombinedContextSummaryV2),
    Context(super::CombinedContextSummary),
}
impl From<CombinedContextSummaryV2> for CombinedContext {
    fn from(summary: CombinedContextSummaryV2) -> Self {
        Self::Answer(summary)
    }
}
impl From<super::CombinedContextSummary> for CombinedContext {
    fn from(summary: super::CombinedContextSummary) -> Self {
        Self::Context(summary)
    }
}
impl CombinedContext {
    pub fn validate(&self, expected_members: &[IndexId]) -> Result<()> {
        match self {
            Self::Answer(summary) => summary.validate(expected_members),
            Self::Context(summary) => summary.validate(expected_members),
        }
    }
    pub fn selection(&self) -> Sha256Digest {
        match self {
            Self::Answer(summary) => summary.selection,
            Self::Context(summary) => summary.selection,
        }
    }
    pub fn deadline_ms(&self) -> u64 {
        match self {
            Self::Answer(summary) => summary.budget.deadline_ms,
            Self::Context(summary) => summary.budget.deadline_ms,
        }
    }
    /// The whole wrapper's byte ceiling: Answer /3 takes the 131,072-byte
    /// Answer frame, Answer /2 the 65,536 bytes of every other tool.
    pub fn reply_bytes(&self) -> usize {
        match self {
            Self::Answer(summary) => v3::reply_limit(&summary.budget),
            Self::Context(summary) => summary.budget.max_bytes.min(crate::MAX_RESPONSE_BYTES),
        }
    }
    /// Each member's allocated bytes, None when it was not dispatched.
    pub fn member_bytes(&self) -> Vec<(IndexId, Option<usize>)> {
        match self {
            Self::Answer(summary) => summary
                .members
                .iter()
                .map(|report| {
                    (
                        report.index_id,
                        report.allowance.as_ref().map(|a| a.resources.bytes),
                    )
                })
                .collect(),
            Self::Context(summary) => summary
                .members
                .iter()
                .map(|report| {
                    (
                        report.index_id,
                        report.allowance.as_ref().map(|a| a.resources.bytes),
                    )
                })
                .collect(),
        }
    }
    pub fn termination(&self) -> Termination {
        match self {
            Self::Answer(summary) => summary.termination,
            Self::Context(summary) => summary.termination,
        }
    }
    /// Members with a known accounting violation (Answer /2 only).
    pub fn violation_members(&self) -> Vec<IndexId> {
        match self {
            Self::Answer(_) => vec![],
            Self::Context(summary) => summary
                .members
                .iter()
                .filter(|member| {
                    matches!(
                        &member.outcome,
                        super::PartitionOutcome::KnownViolation { .. }
                    )
                })
                .map(|member| member.index_id)
                .collect(),
        }
    }
    pub fn delivery_bindings(&self) -> &[ContextDeliveryBinding] {
        match self {
            Self::Answer(summary) => &summary.delivery_bindings,
            Self::Context(summary) => &summary.delivery_bindings,
        }
    }
}

/// Unknown model work in a member reply keeps its whole allowance charged.
pub fn reply_work_unknown(reply: &ContextResponseV3) -> bool {
    reply.decision_log.work_unknown > 0
}

/// The policy bindings an Answer /3 reply's model work must still match at
/// delivery: Focus when a batched Focus receipt identified its profile
/// (`model_work.focus_profile`), Context when any card call was made, with
/// the profile of the reply's application. Both bind the reply's scope.
pub fn reply_delivery_bindings(reply: &ContextResponseV3) -> Result<Vec<ContextDeliveryBinding>> {
    let mut bindings = Vec::new();
    if let Some(profile) = reply.model_work.focus_profile {
        ensure!(
            reply.decision_log.focus_calls > 0 && reply.scope.configuration_revision > 0,
            "a Focus profile without Focus work"
        );
        bindings.push(ContextDeliveryBinding {
            stage: Stage::Focus,
            scope: reply.scope.clone(),
            profile: Some(profile),
        });
    }
    if reply.decision_log.card_calls > 0 {
        let application = reply
            .applications
            .last()
            .ok_or_else(|| anyhow::anyhow!("card work without an application"))?;
        ensure!(
            application.scope == reply.scope && application.profile.is_some(),
            "card work without the reply's own application profile"
        );
        bindings.push(ContextDeliveryBinding {
            stage: Stage::Context,
            scope: reply.scope.clone(),
            profile: application.profile,
        });
    }
    validate_stage_roster(&bindings)?;
    Ok(bindings)
}

/// The widest Answer /3 budget, for validating a reply whose request budget
/// the delivery owner does not hold (a singleton reply).
pub fn widest_budget() -> BudgetV3 {
    BudgetV3 {
        deadline_ms: v3::MAX_DEADLINE_MS,
        max_bytes: v3::MAX_RETENTION_BYTES,
        max_parts: v3::MAX_PARTS,
        max_sub_needs: v3::MAX_SUB_NEEDS,
        rounds: v3::MAX_ROUNDS,
        calls: v3::MAX_CALLS,
        fanout: v3::MAX_FANOUT,
        candidates: v3::MAX_CANDIDATES,
        input_tokens: v3::MAX_INPUT_TOKENS,
        round_input_tokens: v3::MAX_ROUND_INPUT_TOKENS,
        context_calls: v3::MAX_CONTEXT_CALLS,
        focus_calls: v3::MAX_FOCUS_CALLS,
        focus_pairs: v3::MAX_FOCUS_PAIRS,
    }
}

pub(super) fn reply_body(value: &serde_json::Value) -> Option<&serde_json::Value> {
    let body = value
        .get("structuredContent")
        .filter(|body| body.is_object())
        .unwrap_or(value);
    (body.get("schema").and_then(serde_json::Value::as_str) == Some(v3::CONTEXT_SCHEMA_V3))
        .then_some(body)
}

/// Delivery bindings of a combined Answer /3 wrapper: every member reply is
/// re-validated under its own clamped budget and its counts and bindings
/// must equal the summary's.
pub(super) fn combined_bindings(
    body: &serde_json::Value,
    raw: &serde_json::Value,
    ids: &[IndexId],
) -> Result<Vec<ContextDeliveryBinding>> {
    let summary: CombinedContextSummaryV2 = serde_json::from_value(raw.clone())?;
    summary.validate(ids)?;
    let mut actual = Vec::new();
    for report in &summary.members {
        let full = crate::combined_reads::shared_values::materialize_combined_member_result(
            body,
            report.index_id,
        )?;
        if let Some(packet) = reply_body(&full) {
            let allowance = report
                .allowance
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("reply without native allocation"))?;
            let budget = allowance.clamp(&summary.budget)?;
            let typed = ContextResponseV3::from_canonical(packet, &budget)?;
            ensure!(
                typed.scope.index_id == report.index_id,
                "foreign delivery reply index"
            );
            if let PartitionOutcomeV3::Known { used, .. } = &report.outcome {
                ensure!(
                    LoopUse::of_reply(&typed)? == *used && !reply_work_unknown(&typed),
                    "known context observation mismatch"
                );
            }
            actual.extend(reply_delivery_bindings(&typed)?);
        } else {
            ensure!(
                !matches!(&report.outcome, PartitionOutcomeV3::Known { .. }),
                "known context work has no complete native reply"
            );
        }
    }
    ensure!(
        actual == summary.delivery_bindings,
        "combined model delivery binding roster mismatch"
    );
    Ok(actual)
}

#[cfg(test)]
mod tests;
