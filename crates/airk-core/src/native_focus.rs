// SPDX-License-Identifier: Apache-2.0
//! Private Focus inputs. Serialized identity never supplies service authority.
use crate::{
    Hit,
    arbiter::{Budget, FocusApplicationReceipt, ScopeBinding, focus::MAX_RETURNED_CANDIDATES},
    combined_context::PartitionAllowance,
    identity::Sha256Digest,
    read_attempt::{ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FocusSourceWindow {
    pub utf8_start: usize,
    pub utf8_end: usize,
}
impl FocusSourceWindow {
    pub fn text<'a>(&self, hit: &'a Hit) -> Result<&'a str> {
        ensure!(self.utf8_start < self.utf8_end, "empty Focus source window");
        hit.text
            .get(self.utf8_start..self.utf8_end)
            .ok_or_else(|| anyhow::anyhow!("invalid Focus source UTF-8 window"))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFocusCandidate {
    pub id: Sha256Digest,
    pub hit: Hit,
    pub window: FocusSourceWindow,
}
impl NativeFocusCandidate {
    pub fn new(
        scope: &ScopeBinding,
        question: &str,
        ordinal: usize,
        hit: Hit,
        window: FocusSourceWindow,
    ) -> Result<Self> {
        window.text(&hit)?;
        let id = Self::identity(scope, question, ordinal, &hit, window)?;
        Ok(Self { id, hit, window })
    }
    fn identity(
        scope: &ScopeBinding,
        question: &str,
        ordinal: usize,
        hit: &Hit,
        window: FocusSourceWindow,
    ) -> Result<Sha256Digest> {
        let ordinal = u64::try_from(ordinal)?;
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.candidate/1",
            &[
                &serde_json::to_vec(scope)?,
                question.as_bytes(),
                &ordinal.to_be_bytes(),
                &serde_json::to_vec(hit)?,
                &serde_json::to_vec(&window)?,
            ],
        ))
    }
    fn validate(&self, scope: &ScopeBinding, question: &str, ordinal: usize) -> Result<()> {
        self.window.text(&self.hit)?;
        ensure!(
            !self.hit.citation.path.is_empty()
                && self.hit.citation.path.len() <= 4096
                && self.hit.citation.start_line > 0
                && self.hit.citation.end_line >= self.hit.citation.start_line,
            "invalid Focus candidate source citation"
        );
        let _: Sha256Digest = self.hit.citation.content_hash.parse()?;
        ensure!(
            self.id == Self::identity(scope, question, ordinal, &self.hit, self.window)?,
            "Focus candidate full body/order/window identity changed"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFocusRequest {
    pub read_attempt: ReadAttemptBudget,
    pub parent_budget: Budget,
    pub worker: WorkerIdentity,
    pub authority: ScopedReadRef,
    pub scope: ScopeBinding,
    pub question: String,
    pub candidates: Vec<NativeFocusCandidate>,
    pub order_digest: Sha256Digest,
    pub score_count: usize,
    pub ordinal: u8,
    pub partition: Option<PartitionAllowance>,
    pub remaining_ms: u64,
    pub remaining_input_tokens: usize,
}
impl NativeFocusRequest {
    pub fn candidate_order(&self) -> Vec<Sha256Digest> {
        self.candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect()
    }
    pub fn candidate_order_digest(candidates: &[NativeFocusCandidate]) -> Result<Sha256Digest> {
        let order = candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.order/1",
            &[&serde_json::to_vec(&order)?],
        ))
    }
    pub fn input_digest(&self) -> Result<Sha256Digest> {
        let mut semantic = self.clone();
        semantic.remaining_ms = 0;
        semantic.read_attempt.remaining_ns = 0;
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.input/2",
            &[&serde_json::to_vec(&semantic)?],
        ))
    }
    /// Structural only: actual worker, grant, one-use ledger, live publication,
    /// selected stage and original attempt must be independently checked natively.
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
                && self.scope.configuration_revision > 0,
            "foreign native Focus binding"
        );
        let publication = &self.scope.publication;
        ensure!(
            !publication.collection.is_empty()
                && publication.collection.len() <= 4096
                && publication.incarnation == publication.token.incarnation
                && publication.generation == publication.token.generation
                && publication.revision == publication.token.revision,
            "invalid Focus publication binding"
        );
        ensure!(
            !self.question.trim().is_empty()
                && self.question.len() <= 16_384
                && self.candidates.len() <= MAX_RETURNED_CANDIDATES
                && self.score_count <= self.candidates.len()
                && self.score_count <= self.parent_budget.candidates
                && (self.candidates.is_empty() == (self.score_count == 0))
                && (1..=12).contains(&self.ordinal)
                && usize::from(self.ordinal) <= self.parent_budget.calls
                && (1..=120_000).contains(&self.remaining_ms)
                && self.remaining_ms <= self.parent_budget.deadline_ms
                && (1..=8192).contains(&self.remaining_input_tokens)
                && self.remaining_input_tokens <= self.parent_budget.input_tokens
                && serde_json::to_vec(self)?.len() <= crate::service::MAX_TOOLS_CALL_REQUEST_BYTES,
            "native Focus original bounds exceeded"
        );
        for (ordinal, candidate) in self.candidates.iter().enumerate() {
            candidate.validate(&self.scope, &self.question, ordinal)?;
        }
        ensure!(
            self.candidate_order()
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.candidates.len()
                && self.order_digest == Self::candidate_order_digest(&self.candidates)?,
            "duplicate or foreign Focus candidate order"
        );
        if let Some(partition) = &self.partition {
            ensure!(
                partition.parent_budget == self.parent_budget,
                "Focus captured parent budget changed"
            );
            partition.validate_binding(
                &self.read_attempt,
                partition.selection,
                self.scope.index_id,
            )?;
            ensure!(
                self.remaining_input_tokens <= partition.resources.input_tokens
                    && self.score_count <= partition.resources.candidates,
                "Focus exceeds captured partition allowance"
            );
        }
        let remaining = self
            .read_attempt
            .remaining_at(clock, now_ns)?
            .min(Duration::from_millis(self.remaining_ms));
        ensure!(!remaining.is_zero(), "Focus original attempt expired");
        Ok(remaining)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFocusResponse {
    pub ranking: FocusApplicationReceipt,
}
impl NativeFocusResponse {
    pub fn validate(&self, request: &NativeFocusRequest) -> Result<()> {
        let mut remaining = request.parent_budget.clone();
        remaining.input_tokens = request.remaining_input_tokens;
        remaining.deadline_ms = request.remaining_ms;
        self.ranking.validate(&remaining)?;
        ensure!(
            self.ranking.scope == request.scope
                && self.ranking.input_digest == request.input_digest()?
                && self.ranking.original_order == request.candidate_order()
                && self.ranking.presented_count <= request.score_count
                && self.ranking.scored_count <= request.score_count
                && (!self.ranking.applied || self.ranking.scored_count == request.score_count),
            "Focus response differs from actual complete native request"
        );
        for (presentation, candidate) in self.ranking.presentations.iter().zip(&request.candidates)
        {
            ensure!(
                presentation.candidate == candidate.id
                    && presentation.utf8_start == candidate.window.utf8_start
                    && presentation.utf8_end == candidate.window.utf8_end
                    && presentation.source_bytes == candidate.hit.text.len(),
                "Focus response source presentation differs from full canonical hit"
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
