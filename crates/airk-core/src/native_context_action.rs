// SPDX-License-Identifier: Apache-2.0
//! Private native joint-task inputs. Values cannot construct service authority.
use crate::{
    arbiter::{EvidenceRecord, ReadChoice, ScopeBinding, context_action::ContextActionRoster},
    combined_context::PartitionAllowance,
    read_attempt::{ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContextActionRequest {
    pub read_attempt: ReadAttemptBudget,
    pub parent_budget: crate::arbiter::Budget,
    pub worker: WorkerIdentity,
    pub authority: ScopedReadRef,
    pub scope: ScopeBinding,
    pub question: String,
    pub records: Vec<EvidenceRecord>,
    pub choices: Vec<ReadChoice>,
    pub roster: ContextActionRoster,
    pub ordinal: u8,
    pub partition: Option<PartitionAllowance>,
    pub remaining_ms: u64,
    pub remaining_input_tokens: usize,
}
impl NativeContextActionRequest {
    /// The adapter supplies the actual installed worker/authority and clock.
    /// This structural check is not a grant lookup or a one-use reservation.
    pub fn validate(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        clock: ReadClock,
        now_ns: u64,
    ) -> Result<Duration> {
        self.parent_budget.validate()?;
        self.authority.validate()?;
        ensure!(
            self.worker == worker
                && self.worker.fencing_generation > 0
                && self.authority == authority
                && self.scope.index_id == authority.index_id
                && self.scope.authorization == authority.fingerprint
                && self.scope == self.roster.scope,
            "foreign native joint-task binding"
        );
        ensure!(
            !self.question.trim().is_empty()
                && self.question.len() <= 16_384
                && self.records.len() <= 64
                && (1..=3).contains(&self.ordinal)
                && (1..=30_000).contains(&self.remaining_ms)
                && (1..=2048).contains(&self.remaining_input_tokens)
                && self.remaining_input_tokens <= self.parent_budget.input_tokens
                && usize::from(self.ordinal) <= self.parent_budget.rounds.min(3)
                && self.remaining_ms <= self.parent_budget.deadline_ms
                && serde_json::to_vec(self)?.len() <= 524_288,
            "native joint-task bounds exceeded"
        );
        self.roster.validate(&self.records)?;
        ensure!(
            self.records
                .iter()
                .map(|record| record.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.records.len(),
            "duplicate native joint-task record identity"
        );
        let ids: std::collections::BTreeSet<_> =
            self.choices.iter().map(|choice| choice.id).collect();
        ensure!(
            self.choices.len() <= 64
                && ids.len() == self.choices.len()
                && self.choices.len() == self.roster.basis.read_choices_available
                && self
                    .choices
                    .iter()
                    .all(|choice| !choice.target.is_empty() && choice.target.len() <= 16_384)
                && self
                    .roster
                    .basis
                    .read_choices_scored
                    .iter()
                    .all(|id| ids.contains(id)),
            "native joint-task verified choice universe mismatch"
        );
        ensure!(
            self.records.iter().all(|record| record.scope == self.scope),
            "foreign native joint-task evidence"
        );
        if let Some(partition) = &self.partition {
            ensure!(
                partition.parent_budget == self.parent_budget,
                "native joint-task parent budget differs from captured partition"
            );
            partition.validate_binding(
                &self.read_attempt,
                partition.selection,
                self.scope.index_id,
            )?;
            ensure!(
                partition.assessment_ordinals.contains(&self.ordinal)
                    && self.remaining_input_tokens <= partition.resources.input_tokens
                    && usize::from(self.ordinal) <= partition.parent_budget.rounds.min(3),
                "native joint-task exceeds captured partition allowance"
            );
        }
        let remaining = self
            .read_attempt
            .remaining_at(clock, now_ns)?
            .min(Duration::from_millis(self.remaining_ms));
        ensure!(
            !remaining.is_zero(),
            "native joint-task original attempt expired"
        );
        Ok(remaining)
    }
}

#[cfg(test)]
mod tests;
