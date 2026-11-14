// SPDX-License-Identifier: Apache-2.0
//! Inert whole-selection context allocations. These facts confer no authority.
use crate::{
    arbiter::Budget,
    identity::{Incarnation, Sha256Digest},
    index_catalog::{IndexId, MAX_SELECTION},
    read_attempt::ReadAttemptBudget,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ALLOWANCE_SCHEMA: &str = "airk.context-partition-allowance/1";
pub const SUMMARY_SCHEMA: &str = "airk.combined-context/1";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub calls: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub bytes: usize,
}
impl Resources {
    pub fn from_packet(packet: &crate::arbiter::context_action::ContextResponseV2) -> Result<Self> {
        Ok(Self {
            calls: packet.work.calls,
            input_tokens: packet.work.input_tokens,
            candidates: packet.candidate_count.max(packet.work.candidates),
            bytes: packet.work.bytes.max(serde_json::to_vec(packet)?.len()),
        })
    }
    pub fn checked_add(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            calls: self.calls.checked_add(other.calls).ok_or_else(overflow)?,
            input_tokens: self
                .input_tokens
                .checked_add(other.input_tokens)
                .ok_or_else(overflow)?,
            candidates: self
                .candidates
                .checked_add(other.candidates)
                .ok_or_else(overflow)?,
            bytes: self.bytes.checked_add(other.bytes).ok_or_else(overflow)?,
        })
    }
    pub fn fits(&self, ceiling: &Self) -> bool {
        self.calls <= ceiling.calls
            && self.input_tokens <= ceiling.input_tokens
            && self.candidates <= ceiling.candidates
            && self.bytes <= ceiling.bytes
    }
    pub fn budget(budget: &Budget) -> Self {
        Self {
            calls: budget.calls,
            input_tokens: budget.input_tokens,
            candidates: budget.candidates,
            bytes: budget.max_bytes,
        }
    }
}

pub fn known_budget_failure(packet: &crate::arbiter::context_action::ContextResponseV2) -> bool {
    use crate::arbiter::context_action::{
        AssessmentResult, PartitionCoverage, PartitionTermination,
    };
    packet.stop==PartitionTermination::Budget
        && matches!(packet.assessment.coverage,PartitionCoverage::Unassessed {..})
        && packet.assessment_history.last().is_some_and(|event|
            matches!(&event.result,AssessmentResult::Unavailable {advice:Some(advice),work_unknown:false,..}
                if advice.observed_input_tokens.is_some()))
}
fn overflow() -> anyhow::Error {
    anyhow::anyhow!("combined context accounting overflow")
}

/// Native adapter metadata, never public Answer arguments or a grant.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionAllowance {
    pub schema: String,
    pub attempt: Incarnation,
    pub selection: Sha256Digest,
    pub index_id: IndexId,
    pub parent_budget: Budget,
    pub resources: Resources,
    pub assessment_ordinals: Vec<u8>,
}
impl PartitionAllowance {
    pub fn validate(&self) -> Result<()> {
        self.parent_budget.validate()?;
        ensure!(
            self.schema == ALLOWANCE_SCHEMA,
            "invalid context allowance schema"
        );
        ensure!(
            self.resources.fits(&Resources::budget(&self.parent_budget))
                && self.resources.calls > 0
                && self.resources.candidates > 0
                && self.resources.bytes >= 1024,
            "invalid context partition allocation"
        );
        let maximum = self.parent_budget.rounds.min(3);
        ensure!(
            self.assessment_ordinals.len() <= maximum
                && self
                    .assessment_ordinals
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                && self
                    .assessment_ordinals
                    .iter()
                    .all(|ordinal| (1..=maximum).contains(&usize::from(*ordinal))),
            "invalid global assessment ordinals"
        );
        ensure!(
            self.assessment_ordinals.is_empty() == (self.resources.input_tokens == 0),
            "assessment allowance and token allocation disagree"
        );
        Ok(())
    }
    pub fn validate_binding(
        &self,
        attempt: &ReadAttemptBudget,
        selection: Sha256Digest,
        index: IndexId,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.attempt == attempt.request_id
                && self.selection == selection
                && self.index_id == index,
            "context partition binding mismatch"
        );
        Ok(())
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextDeliveryBinding {
    pub scope: crate::arbiter::ScopeBinding,
    #[serde(deserialize_with = "crate::arbiter::context_action::required_nullable")]
    pub profile: Option<Sha256Digest>,
}
impl ContextDeliveryBinding {
    pub fn from_packet(
        packet: &crate::arbiter::context_action::ContextResponseV2,
    ) -> Result<Option<Self>> {
        use crate::arbiter::context_action::AssessmentResult;
        let actual = packet.assessment_history.iter().any(has_native_receipt);
        if !actual {
            ensure!(
                packet.work.input_tokens == 0,
                "model work has no native receipt"
            );
            return Ok(None);
        }
        ensure!(
            packet.assessment.scope.configuration_revision > 0,
            "model receipt has no native policy revision"
        );
        let mut profiles = BTreeSet::new();
        for event in &packet.assessment_history {
            match &event.result {
                AssessmentResult::Selected { advice, .. }
                | AssessmentResult::Abstained { advice } => {
                    profiles.insert(Some(advice.profile));
                }
                AssessmentResult::Unavailable {
                    advice: Some(advice),
                    ..
                } => {
                    profiles.insert(Some(advice.profile));
                }
                AssessmentResult::Unavailable { advice: None, .. } => {}
            }
        }
        for application in &packet.applications {
            ensure!(
                application.scope == packet.assessment.scope,
                "foreign model application binding"
            );
            profiles.insert(application.profile);
        }
        ensure!(
            profiles.len() == 1,
            "inconsistent native model profile observations"
        );
        let profile = profiles.into_iter().next().unwrap();
        if profile.is_none() {
            use crate::arbiter::context_action::AssessmentUnavailable;
            ensure!(packet.work.input_tokens==0 && packet.assessment_history.iter().all(|event|
                matches!(&event.result,AssessmentResult::Unavailable {reason:AssessmentUnavailable::Disabled
                    |AssessmentUnavailable::NotEnrolled|AssessmentUnavailable::TaskUnsupported|AssessmentUnavailable::NoEvidence,
                    work,work_unknown:false,..} if work.input_tokens==0)),
                "absent profile is not a known no-model observation");
        }
        Ok(Some(Self {
            scope: packet.assessment.scope.clone(),
            profile,
        }))
    }
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
        let mut bound = BTreeSet::new();
        for binding in &self.delivery_bindings {
            ensure!(
                expected_members.contains(&binding.scope.index_id)
                    && bound.insert(binding.scope.index_id)
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
        let has_packets = rows.iter().any(|row| packet_body(&row["result"]).is_some());
        let Some(raw) = body.get("context").filter(|value| !value.is_null()) else {
            ensure!(!has_packets, "combined context summary missing");
            return Ok(Vec::new());
        };
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
                if let Some(binding) = ContextDeliveryBinding::from_packet(&typed)? {
                    actual.push(binding);
                }
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
                input_tokens: 2048,
                candidates: 64,
                max_bytes: 524288,
                deadline_ms: 30000,
            },
        )?;
        return Ok(ContextDeliveryBinding::from_packet(&typed)?
            .into_iter()
            .collect());
    }
    if let Some(content) = body
        .get("content")
        .and_then(Value::as_array)
        .filter(|items| items.len() == 1)
        && let Some(text) = content[0].get("text").and_then(Value::as_str)
    {
        let parsed = serde_json::from_str::<Value>(text);
        if let Ok(parsed) = parsed
            && matches!(
                parsed.get("schema").and_then(Value::as_str),
                Some(crate::arbiter::context_action::CONTEXT_SCHEMA_V2)
                    | Some(crate::combined_reads::COMBINED_READ_SCHEMA)
            )
        {
            return context_delivery_bindings(&parsed);
        }
    }
    Ok(Vec::new())
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
