// SPDX-License-Identifier: Apache-2.0
//! Compact Focus batch receipt: one batched Focus call over several sub-need
//! questions, small enough for a 32-pair batch to fit about 12 KiB of custody.
use super::*;
use crate::arbiter::focus::{FocusIdentity, FocusScore, MAX_RETURNED_CANDIDATES};

pub const FOCUS_BATCH_SCHEMA: &str = "airk.arbiter.focus.batch/1";
pub const MAX_FOCUS_BATCH_PAIRS: usize = 32;
/// Distinct questions one batched Focus call may score against.
pub const MAX_FOCUS_QUESTIONS: usize = 16;
pub const MAX_COMPACT_REASON_BYTES: usize = 256;
/// The custody budget of a full 32-pair batch receipt.
pub const FOCUS_BATCH_CUSTODY_BYTES: usize = 12_288;

/// One scored (candidate, question) pair, by index into the call's request.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusPairScore {
    pub candidate: u16,
    pub question: u8,
    pub no_logit: f32,
    pub yes_logit: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusBatchReceipt {
    pub schema: String,
    /// Framed digest of the exact ScopeBinding; the record carries the scope.
    pub scope_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<FocusIdentity>,
    pub availability: Availability,
    pub reason: String,
    pub applied: bool,
    pub input_digest: Sha256Digest,
    /// Digest of the complete canonical candidate order, tail included.
    pub order_digest: Sha256Digest,
    /// Digest of the ordered distinct questions.
    pub questions_digest: Sha256Digest,
    pub question_count: u8,
    pub candidate_count: usize,
    /// The ranked permutation of candidate indexes.
    pub ranked: Vec<u16>,
    pub pairs: Vec<FocusPairScore>,
    pub reserved_input_tokens: usize,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_tokens")]
    pub observed_input_tokens: Option<usize>,
    pub attribution: Vec<TokenAttribution>,
    pub work: Work,
}
fn nullable_tokens(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<usize>>()
}

pub fn scope_digest(scope: &ScopeBinding) -> Result<Sha256Digest> {
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.focus.batch-scope/1",
        &[&serde_json::to_vec(scope)?],
    ))
}
pub fn questions_digest(questions: &[String]) -> Result<Sha256Digest> {
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.focus.batch-questions/1",
        &[&serde_json::to_vec(questions)?],
    ))
}

impl FocusBatchReceipt {
    /// Upper bound on the serialized receipt, from its counts alone.
    pub fn bound(candidates: usize, pairs: usize) -> usize {
        // Envelope with identity and a 256-byte escaped reason, then each
        // ranked index and each pair with two finite f32 logits.
        4096usize
            .saturating_add(candidates.saturating_mul(6))
            .saturating_add(pairs.saturating_mul(96))
    }
    pub fn relevance(&self, pair: &FocusPairScore, ids: &[Sha256Digest]) -> Result<FocusScore> {
        let candidate = ids
            .get(usize::from(pair.candidate))
            .ok_or_else(|| anyhow::anyhow!("Focus pair names an unknown candidate"))?;
        FocusScore::from_logits(*candidate, pair.no_logit, pair.yes_logit)
    }
    pub fn validate(&self, scope: &ScopeBinding, budget: &BudgetV3) -> Result<()> {
        budget.validate()?;
        ensure!(
            self.schema == FOCUS_BATCH_SCHEMA
                && self.scope_digest == scope_digest(scope)?
                && !self.reason.is_empty()
                && self.reason.len() <= MAX_COMPACT_REASON_BYTES,
            "invalid Focus batch schema, scope or reason"
        );
        ensure!(
            (1..=MAX_FOCUS_QUESTIONS).contains(&usize::from(self.question_count))
                && self.candidate_count <= MAX_RETURNED_CANDIDATES
                && self.pairs.len() <= MAX_FOCUS_BATCH_PAIRS
                && self.pairs.len() <= budget.focus_pairs,
            "invalid Focus batch accounting"
        );
        let mut ranked: Vec<_> = self
            .ranked
            .iter()
            .map(|index| usize::from(*index))
            .collect();
        ranked.sort_unstable();
        ensure!(
            ranked == (0..self.candidate_count).collect::<Vec<_>>(),
            "Focus batch must preserve the entire canonical candidate permutation"
        );
        ensure!(
            unique(
                self.pairs
                    .iter()
                    .map(|pair| (pair.candidate, pair.question))
            ) && self
                .pairs
                .iter()
                .all(|pair| usize::from(pair.candidate) < self.candidate_count
                    && pair.question < self.question_count
                    && pair.no_logit.is_finite()
                    && pair.yes_logit.is_finite()),
            "invalid Focus batch pair"
        );
        ensure!(
            self.reserved_input_tokens <= budget.round_input_tokens
                && self.work.input_tokens == self.observed_input_tokens.unwrap_or(0)
                && self.work.calls == 0
                && self.work.rounds == 0
                && self.work.fanout == 0
                && self.work.bytes == 0
                && self.work.candidates == self.pairs.len(),
            "Focus batch work exceeds its allocation or invents source reads"
        );
        let attributed = validate_attribution(&self.attribution)?;
        ensure!(
            self.attribution.iter().all(|charge| charge.round >= 1)
                && match self.observed_input_tokens {
                    Some(observed) => attributed == observed,
                    None => attributed <= self.reserved_input_tokens,
                },
            "Focus batch attribution differs from its observed tokens"
        );
        if self
            .observed_input_tokens
            .is_some_and(|tokens| tokens > self.reserved_input_tokens)
        {
            ensure!(
                !self.applied && self.availability == Availability::Failed,
                "observed Focus token violation cannot become a successful application"
            );
        }
        ensure!(
            !self.applied
                || (self.identity.is_some()
                    && !self.pairs.is_empty()
                    && self.observed_input_tokens.is_some_and(|tokens| tokens > 0)),
            "applied Focus batch lacks identity, pairs or tokens"
        );
        ensure!(
            self.applied
                || self
                    .ranked
                    .iter()
                    .enumerate()
                    .all(|(position, index)| usize::from(*index) == position),
            "unapplied Focus batch must keep the canonical order"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= Self::bound(self.candidate_count, self.pairs.len()),
            "Focus batch receipt exceeds its compact bound"
        );
        Ok(())
    }
}
