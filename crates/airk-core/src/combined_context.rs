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
}

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
        for report in &self.members {
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
                && known
                    .checked_add(&held)?
                    .fits(&Resources::budget(&self.budget)),
            "combined context accounting mismatch"
        );
        ensure!(
            self.termination != Termination::Complete || complete,
            "incomplete partition roster cannot claim completion"
        );
        Ok(())
    }
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
