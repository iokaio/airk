// SPDX-License-Identifier: Apache-2.0
//! Inert whole-selection context allocations. These facts confer no authority.
use crate::context_records::overflow;
pub use crate::context_records::{
    ALLOWANCE_SCHEMA, ContextDeliveryBinding, PartitionAllowance, Resources,
};
use crate::{
    arbiter::Budget,
    identity::{Incarnation, Sha256Digest},
    index_catalog::{IndexId, MAX_SELECTION},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod answer;
pub use answer::{
    CombinedContext, CombinedContextSummaryV2, LoopUse, PartitionOutcomeV3, PartitionReportV3,
    SUMMARY_SCHEMA_V2, grow_allowance, initial_allocations_v3, reply_delivery_bindings,
    reply_work_unknown, returned_allowance, widest_budget, without_models,
};

pub const SUMMARY_SCHEMA: &str = "airk.combined-context/1";
pub const MAX_DELIVERY_BINDINGS: usize = 2 * MAX_SELECTION;

pub fn known_budget_failure(packet: &crate::arbiter::context_action::ContextResponseV2) -> bool {
    use crate::arbiter::context_action::{
        AssessmentResult, PartitionCoverage, PartitionTermination,
    };
    let focus_violation = crate::arbiter::focus::observed_focus_work(
        &packet.records,
        &crate::arbiter::focus::validation_budget(),
    )
    .is_ok_and(|focus| focus.failed_token_overrun && !focus.work_unknown);
    packet.stop == PartitionTermination::Budget
        && matches!(packet.assessment.coverage,PartitionCoverage::Unassessed {..})
        && (focus_violation || packet.assessment_history.last().is_some_and(|event|
            matches!(&event.result,AssessmentResult::Unavailable {advice:Some(advice),work_unknown:false,..}
                if advice.observed_input_tokens.is_some())))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnqueriedReason {
    Budget,
    Deadline,
    Canceled,
    RequiredMemberFailure,
    AccountingViolation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartitionOutcome {
    Unqueried {
        reason: UnqueriedReason,
    },
    Known {
        used: Resources,
        assessment_ordinals: Vec<u8>,
        read_depth: usize,
        peak_fanout: usize,
    },
    /// Truthfully observed failed work, never permission to enlarge a lease.
    KnownViolation {
        observed: Resources,
        assessment_ordinals: Vec<u8>,
        read_depth: usize,
        peak_fanout: usize,
    },
    /// The entire reservation stays charged; no zero-work or replay inference.
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionReport {
    pub index_id: IndexId,
    pub allowance: Option<PartitionAllowance>,
    pub outcome: PartitionOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Complete,
    Budget,
    Deadline,
    Canceled,
    Partial,
    AccountingViolation,
}

pub use crate::arbiter::context_action::has_native_receipt;

/// Optional addition to the existing combined wrapper; member bodies stay full.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedContextSummary {
    pub schema: String,
    pub attempt: Incarnation,
    pub selection: Sha256Digest,
    pub budget: Budget,
    pub members: Vec<PartitionReport>,
    pub known_used: Resources,
    pub unknown_held: Resources,
    pub termination: Termination,
    pub delivery_bindings: Vec<ContextDeliveryBinding>,
}
impl CombinedContextSummary {
    pub fn validate(&self, expected_members: &[IndexId]) -> Result<()> {
        self.budget.validate()?;
        ensure!(
            self.schema == SUMMARY_SCHEMA,
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
        let mut known = Resources::default();
        let mut held = Resources::default();
        let mut last_ordinal = 0;
        let mut charged_ordinals = BTreeSet::new();
        let mut complete = true;
        let mut violation = false;
        for report in &self.members {
            ensure!(
                !violation
                    || matches!(
                        (&report.allowance, &report.outcome),
                        (
                            None,
                            PartitionOutcome::Unqueried {
                                reason: UnqueriedReason::AccountingViolation
                            }
                        )
                    ),
                "work dispatched after known accounting violation"
            );
            match (&report.allowance, &report.outcome) {
                (None, PartitionOutcome::Unqueried { .. }) => {
                    complete = false;
                }
                (Some(allowance), outcome) => {
                    allowance.validate()?;
                    ensure!(
                        allowance.attempt == self.attempt
                            && allowance.selection == self.selection
                            && allowance.index_id == report.index_id
                            && same_budget(&allowance.parent_budget, &self.budget),
                        "foreign context partition allocation"
                    );
                    match outcome {
                        PartitionOutcome::Known {
                            used,
                            assessment_ordinals,
                            read_depth,
                            peak_fanout,
                        } => {
                            ensure!(
                                assessment_ordinals.is_empty() || used.input_tokens > 0,
                                "completed assessment has no actual input charge"
                            );
                            ensure!(
                                used.fits(&allowance.resources)
                                    && *read_depth <= self.budget.rounds
                                    && *peak_fanout <= self.budget.fanout,
                                "partition work exceeds allocation"
                            );
                            for ordinal in assessment_ordinals {
                                ensure!(
                                    *ordinal > last_ordinal
                                        && allowance.assessment_ordinals.contains(ordinal),
                                    "repeated or foreign global assessment ordinal"
                                );
                                ensure!(
                                    charged_ordinals.insert(*ordinal),
                                    "assessment ordinal remains reserved by unknown work"
                                );
                                last_ordinal = *ordinal;
                            }
                            known = known.checked_add(used)?;
                        }
                        PartitionOutcome::Unknown => {
                            for ordinal in &allowance.assessment_ordinals {
                                ensure!(
                                    charged_ordinals.insert(*ordinal),
                                    "unknown assessment reservation reused"
                                );
                            }
                            held = held.checked_add(&allowance.resources)?;
                            complete = false;
                        }
                        PartitionOutcome::KnownViolation {
                            observed,
                            assessment_ordinals,
                            read_depth,
                            peak_fanout,
                        } => {
                            ensure!(
                                !observed.fits(&allowance.resources),
                                "known violation has no observed overrun"
                            );
                            ensure!(
                                assessment_ordinals.is_empty() || observed.input_tokens > 0,
                                "known violation has uncharged assessment"
                            );
                            ensure!(
                                *read_depth <= self.budget.rounds
                                    && *peak_fanout <= self.budget.fanout,
                                "invalid failed context depth or fanout"
                            );
                            for ordinal in assessment_ordinals {
                                ensure!(
                                    *ordinal > last_ordinal
                                        && allowance.assessment_ordinals.contains(ordinal)
                                        && charged_ordinals.insert(*ordinal),
                                    "foreign failed assessment ordinal"
                                );
                                last_ordinal = *ordinal;
                            }
                            known = known.checked_add(observed)?;
                            violation = true;
                            complete = false;
                        }
                        PartitionOutcome::Unqueried { .. } => {
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
                && (known
                    .checked_add(&held)?
                    .fits(&Resources::budget(&self.budget))
                    || violation),
            "combined context accounting mismatch"
        );
        ensure!(
            (self.termination == Termination::AccountingViolation) == violation,
            "known accounting violation requires explicit failed terminal"
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

/// Exact native model-output custody facts, not a scan of arbitrary payloads.
/// Transport owners unwrap their own envelope before calling this function.
pub fn context_delivery_bindings(value: &serde_json::Value) -> Result<Vec<ContextDeliveryBinding>> {
    use serde_json::Value;
    let body = value
        .get("structuredContent")
        .filter(|body| body.is_object())
        .unwrap_or(value);
    if body.get("schema").and_then(Value::as_str)
        == Some(crate::combined_reads::COMBINED_READ_SCHEMA)
    {
        let rows = body["members"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("combined members missing"))?;
        let mut ids = Vec::with_capacity(rows.len());
        for row in rows {
            ids.push(serde_json::from_value::<IndexId>(row["index_id"].clone())?);
        }
        let has_packets = rows.iter().any(|row| {
            packet_body(&row["result"]).is_some() || answer::reply_body(&row["result"]).is_some()
        });
        let Some(raw) = body.get("context").filter(|value| !value.is_null()) else {
            ensure!(!has_packets, "combined context summary missing");
            let mut actual = Vec::new();
            for id in &ids {
                let full =
                    crate::combined_reads::shared_values::materialize_combined_member_result(
                        body, *id,
                    )?;
                if let Some(binding) = canonical_search_focus_binding(&full)? {
                    ensure!(
                        binding.scope.index_id == *id,
                        "foreign combined Focus source index"
                    );
                    actual.push(binding);
                }
            }
            validate_stage_roster(&actual)?;
            return Ok(actual);
        };
        if raw.get("schema").and_then(Value::as_str) == Some(SUMMARY_SCHEMA_V2) {
            return answer::combined_bindings(body, raw, &ids);
        }
        let summary: CombinedContextSummary = serde_json::from_value(raw.clone())?;
        summary.validate(&ids)?;
        let mut actual = Vec::new();
        let mut used_ordinals = BTreeSet::new();
        for report in &summary.members {
            let full = crate::combined_reads::shared_values::materialize_combined_member_result(
                body,
                report.index_id,
            )?;
            if let Some(packet) = packet_body(&full) {
                let allowance = report
                    .allowance
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("packet without native allocation"))?;
                let mut budget = summary.budget.clone();
                budget.calls = allowance.resources.calls;
                budget.candidates = allowance.resources.candidates;
                budget.input_tokens = allowance.resources.input_tokens.max(1);
                budget.max_bytes = allowance.resources.bytes;
                let typed = crate::arbiter::context_action::ContextResponseV2::from_canonical(
                    packet, &budget,
                )?;
                ensure!(
                    typed.assessment.scope.index_id == report.index_id,
                    "foreign delivery packet index"
                );
                for event in typed
                    .assessment_history
                    .iter()
                    .filter(|event| has_native_receipt(event))
                {
                    ensure!(
                        allowance.assessment_ordinals.contains(&event.ordinal)
                            && used_ordinals.insert(event.ordinal),
                        "unallocated or repeated actual native assessment ordinal"
                    );
                }
                match &report.outcome {
                    PartitionOutcome::KnownViolation {
                        observed,
                        assessment_ordinals,
                        ..
                    } => ensure!(
                        known_budget_failure(&typed)
                            && Resources::from_packet(&typed)? == *observed
                            && *assessment_ordinals == packet_charged_ordinals(&typed),
                        "accounting violation lacks exact failed native observation"
                    ),
                    PartitionOutcome::Known {
                        used,
                        assessment_ordinals,
                        ..
                    } => ensure!(
                        Resources::from_packet(&typed)? == *used
                            && !crate::arbiter::focus::observed_focus_work(
                                &typed.records,
                                &budget
                            )?
                            .work_unknown
                            && !typed.assessment_history.iter().any(|event| matches!(
                                &event.result,
                                crate::arbiter::context_action::AssessmentResult::Unavailable {
                                    work_unknown: true,
                                    ..
                                }
                            ))
                            && *assessment_ordinals == packet_charged_ordinals(&typed),
                        "known context observation mismatch"
                    ),
                    _ => {}
                }
                actual.extend(packet_delivery_bindings(&typed)?);
            } else {
                ensure!(
                    !matches!(
                        &report.outcome,
                        PartitionOutcome::Known { .. } | PartitionOutcome::KnownViolation { .. }
                    ),
                    "known context work has no complete native packet"
                );
            }
        }
        ensure!(
            actual == summary.delivery_bindings,
            "combined model delivery binding roster mismatch"
        );
        return Ok(actual);
    }
    if let Some(packet) = packet_body(body) {
        let typed = crate::arbiter::context_action::ContextResponseV2::from_canonical(
            packet,
            &Budget {
                calls: 12,
                rounds: 4,
                fanout: 12,
                input_tokens: 8192,
                candidates: 64,
                max_bytes: 524288,
                deadline_ms: 120000,
            },
        )?;
        return packet_delivery_bindings(&typed);
    }
    if let Some(reply) = answer::reply_body(body) {
        let typed = crate::arbiter::answer_loop::ContextResponseV3::from_canonical(
            reply,
            &widest_budget(),
        )?;
        return reply_delivery_bindings(&typed);
    }
    if let Some(binding) = canonical_search_focus_binding(body)? {
        return Ok(vec![binding]);
    }
    if let Some(content) = body
        .get("content")
        .and_then(Value::as_array)
        .filter(|items| items.len() == 1)
        && let Some(text) = content[0].get("text").and_then(Value::as_str)
    {
        let parsed = serde_json::from_str::<Value>(text);
        if let Ok(parsed) = parsed
            && (matches!(
                parsed.get("schema").and_then(Value::as_str),
                Some(crate::arbiter::context_action::CONTEXT_SCHEMA_V2)
                    | Some(crate::arbiter::answer_loop::CONTEXT_SCHEMA_V3)
                    | Some(crate::combined_reads::COMBINED_READ_SCHEMA)
            ) || (parsed.get("focus").is_some()
                && parsed.get("hits").is_some()
                && parsed.get("generation").is_some()
                && parsed.get("collection").is_some()))
        {
            return context_delivery_bindings(&parsed);
        }
    }
    Ok(Vec::new())
}
/// Exact stage/index roster, preserving native encounter order and retaining no
/// duplicate observations with conflicting source/config/profile identities.
fn validate_stage_roster(bindings: &[ContextDeliveryBinding]) -> Result<()> {
    ensure!(
        bindings.len() <= MAX_DELIVERY_BINDINGS,
        "native stage delivery roster limit"
    );
    let mut keys = BTreeSet::new();
    for binding in bindings {
        ensure!(
            binding.scope.configuration_revision > 0
                && keys.insert((binding.stage, binding.scope.index_id)),
            "duplicate or unobserved native stage delivery binding"
        );
    }
    Ok(())
}
fn focus_receipt_binding(
    receipt: &crate::arbiter::FocusApplicationReceipt,
) -> Result<Option<ContextDeliveryBinding>> {
    receipt.validate(&crate::arbiter::focus::validation_budget())?;
    let actual = receipt.reserved_input_tokens > 0
        || receipt.observed_input_tokens.is_none()
        || receipt.work.input_tokens > 0
        || !receipt.scores.is_empty();
    if !actual {
        return Ok(None);
    }
    let identity = receipt
        .identity
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("actual Focus work identity missing"))?;
    ensure!(
        receipt.scope.configuration_revision > 0,
        "actual Focus work configuration missing"
    );
    Ok(Some(ContextDeliveryBinding {
        stage: crate::arbiter::Stage::Focus,
        scope: receipt.scope.clone(),
        profile: Some(identity.profile),
    }))
}
fn canonical_search_focus_binding(
    value: &serde_json::Value,
) -> Result<Option<ContextDeliveryBinding>> {
    let body = value
        .get("structuredContent")
        .filter(|body| body.is_object())
        .unwrap_or(value);
    if body.get("focus").is_none_or(serde_json::Value::is_null)
        || !body.get("hits").is_some_and(serde_json::Value::is_array)
        || !body
            .get("generation")
            .is_some_and(serde_json::Value::is_u64)
        || !body
            .get("collection")
            .is_some_and(serde_json::Value::is_string)
        || !body
            .get("truncated")
            .is_some_and(serde_json::Value::is_boolean)
    {
        return Ok(None);
    }
    let response: crate::SearchResponse = serde_json::from_value(body.clone())?;
    let Some(observation) = response.focus.clone() else {
        return Ok(None);
    };
    observation.validate(&crate::arbiter::focus::validation_budget())?;
    let Some(receipt) = observation.receipt() else {
        return Ok(None);
    };
    receipt.validate_search_response(&response, &crate::arbiter::focus::validation_budget())?;
    ensure!(
        response.publication.as_ref() == Some(&receipt.scope.publication)
            && response.collection == receipt.scope.publication.collection
            && response.generation == receipt.scope.publication.generation
            && response.hits.len() == receipt.candidate_count,
        "Focus source result publication/count differs"
    );
    focus_receipt_binding(receipt)
}
/// One actual partition may carry both stage bindings. Only canonical Search
/// records provide Focus facts; arbitrary nested payload fields are ignored.
pub fn packet_delivery_bindings(
    packet: &crate::arbiter::context_action::ContextResponseV2,
) -> Result<Vec<ContextDeliveryBinding>> {
    let mut bindings = Vec::new();
    for record in &packet.records {
        let Some(crate::arbiter::FocusObservation::Receipt { receipt }) =
            crate::arbiter::focus::search_record_observation(record)?
        else {
            continue;
        };
        if let Some(binding) = focus_receipt_binding(&receipt)? {
            if let Some(prior) = bindings.iter().find(|prior: &&ContextDeliveryBinding| {
                prior.stage == binding.stage && prior.scope.index_id == binding.scope.index_id
            }) {
                ensure!(
                    prior == &binding,
                    "Focus policy/profile changed within one partition"
                );
            } else {
                bindings.push(binding);
            }
        }
    }
    if let Some(context) = ContextDeliveryBinding::from_packet(packet)? {
        bindings.push(context);
    }
    validate_stage_roster(&bindings)?;
    Ok(bindings)
}

fn packet_charged_ordinals(packet: &crate::arbiter::context_action::ContextResponseV2) -> Vec<u8> {
    use crate::arbiter::context_action::AssessmentResult;
    packet
        .assessment_history
        .iter()
        .filter_map(|event| {
            let tokens = match &event.result {
                AssessmentResult::Selected { advice, .. }
                | AssessmentResult::Abstained { advice } => advice.work.input_tokens,
                AssessmentResult::Unavailable { work, .. } => work.input_tokens,
            };
            (tokens > 0).then_some(event.ordinal)
        })
        .collect()
}
fn packet_body(value: &serde_json::Value) -> Option<&serde_json::Value> {
    let body = value
        .get("structuredContent")
        .filter(|body| body.is_object())
        .unwrap_or(value);
    if body.get("schema").and_then(serde_json::Value::as_str)
        == Some(crate::arbiter::context_action::CONTEXT_SCHEMA_V2)
    {
        return Some(body);
    }
    if body.get("error").is_some_and(|error| !error.is_null()) {
        let evidence = body.get("evidence")?;
        if evidence.get("schema").and_then(serde_json::Value::as_str)
            == Some(crate::arbiter::context_action::CONTEXT_SCHEMA_V2)
        {
            return Some(evidence);
        }
    }
    None
}
fn same_budget(a: &Budget, b: &Budget) -> bool {
    a.calls == b.calls
        && a.rounds == b.rounds
        && a.fanout == b.fanout
        && a.input_tokens == b.input_tokens
        && a.candidates == b.candidates
        && a.max_bytes == b.max_bytes
        && a.deadline_ms == b.deadline_ms
}
fn validate_members(members: &[IndexId]) -> Result<()> {
    ensure!(
        !members.is_empty()
            && members.len() <= MAX_SELECTION
            && members.windows(2).all(|pair| pair[0] < pair[1]),
        "invalid context member roster"
    );
    Ok(())
}

/// Pure initial reservations, not execution. Later known-completion reclaim is
/// owned by the native frontend ledger; a member is never submitted twice.
pub fn initial_allocations(
    attempt: Incarnation,
    selection: Sha256Digest,
    members: &[IndexId],
    budget: &Budget,
) -> Result<Vec<Option<PartitionAllowance>>> {
    budget.validate()?;
    validate_members(members)?;
    let admitted = members
        .len()
        .min(budget.calls)
        .min(budget.candidates)
        .min(budget.max_bytes / 1024);
    let assessments = budget.rounds.min(3).min(budget.input_tokens);
    let assessment_members = admitted.min(assessments);
    let mut ordinal = 1u8;
    let mut result = Vec::with_capacity(members.len());
    for (offset, index_id) in members.iter().copied().enumerate() {
        if offset >= admitted {
            result.push(None);
            continue;
        }
        let count = share(assessments, admitted, offset);
        let mut ordinals = Vec::with_capacity(count);
        for _ in 0..count {
            ordinals.push(ordinal);
            ordinal = ordinal.checked_add(1).ok_or_else(overflow)?;
        }
        let resources = Resources {
            calls: share(budget.calls, admitted, offset),
            candidates: share(budget.candidates, admitted, offset),
            bytes: share(budget.max_bytes, admitted, offset),
            input_tokens: if count == 0 {
                0
            } else {
                share(budget.input_tokens, assessment_members, offset)
            },
        };
        let allowance = PartitionAllowance {
            schema: ALLOWANCE_SCHEMA.into(),
            attempt,
            selection,
            index_id,
            parent_budget: budget.clone(),
            resources,
            assessment_ordinals: ordinals,
        };
        allowance.validate()?;
        result.push(Some(allowance));
    }
    // Independent of individual validation: prove no resource was minted.
    let mut total = Resources::default();
    let mut ordinals = BTreeSet::new();
    for allowance in result.iter().flatten() {
        total = total.checked_add(&allowance.resources)?;
        for ordinal in &allowance.assessment_ordinals {
            ensure!(
                ordinals.insert(*ordinal),
                "duplicate initial assessment ordinal"
            );
        }
    }
    ensure!(
        total.fits(&Resources::budget(budget)),
        "initial context allocation exceeds budget"
    );
    Ok(result)
}
fn share(total: usize, members: usize, offset: usize) -> usize {
    total / members + usize::from(offset < total % members)
}

#[cfg(test)]
mod tests;
