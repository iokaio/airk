// SPDX-License-Identifier: Apache-2.0
//! Bounded relevance-only reordering. It never asserts context sufficiency.
use super::*;

pub const RANKING_SCHEMA: &str = "airk.arbiter.focus.ranking/1";
pub const MAX_RETURNED_CANDIDATES: usize = 100;

/// Owner-defined task semantics include ranking, accounting, source-window and
/// immutable native input identity rules. No cross-crate source mount.
pub fn semantic_policy_digest() -> Result<Sha256Digest> {
    fn source(bytes: &[u8]) -> Result<Sha256Digest> {
        let canonical = std::str::from_utf8(bytes)?.replace("\r\n", "\n");
        ensure!(
            !canonical.contains('\r'),
            "lone CR in Focus native semantics"
        );
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus-core-source-lf/1",
            &[canonical.as_bytes()],
        ))
    }
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.focus-core-semantics/1",
        &[
            source(include_bytes!("focus.rs"))?.as_bytes(),
            source(include_bytes!("../native_focus.rs"))?.as_bytes(),
        ],
    ))
}

/// Facts from an actual authenticated qualified profile, never caller authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusProfileLimits {
    pub max_candidates: usize,
    pub pair_input_tokens: usize,
    pub max_input_tokens: usize,
}
impl FocusProfileLimits {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=64).contains(&self.max_candidates)
                && (1..=8192).contains(&self.pair_input_tokens)
                && (1..=8192).contains(&self.max_input_tokens)
                && self.pair_input_tokens <= self.max_input_tokens,
            "invalid qualified Focus profile limits"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusApplication {
    pub application: IndexApplication,
    pub identity: Option<FocusIdentity>,
    pub limits: Option<FocusProfileLimits>,
}
impl FocusApplication {
    pub fn validate(&self) -> Result<()> {
        self.application.validate()?;
        if self.application.availability == Availability::Ready {
            ensure!(
                self.identity
                    .as_ref()
                    .is_some_and(
                        |identity| Some(identity.profile) == self.application.profile
                            && identity.runtime == RUNTIME
                    ),
                "ready Focus profile identity absent or mismatched"
            );
            self.limits
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("ready Focus profile limits absent"))?
                .validate()?;
        } else {
            ensure!(
                self.limits.is_none() && self.identity.is_none(),
                "unavailable Focus cannot grant a scoring allocation"
            );
        }
        Ok(())
    }
}
/// Whole-pair Focus capacity under the actual original local allocation.
/// Initial Context readiness is captured once before reads, never reset per round.
/// Cumulative Focus work includes outstanding/unknown reservations; the caller
/// separately enforces unknown-work refusal and the common native total ledger.
pub fn focus_token_allowance(
    local_parent_tokens: usize,
    initial_context_ready: bool,
    cumulative_focus_tokens: usize,
    shared_remaining_tokens: usize,
    limits: &FocusProfileLimits,
) -> Result<usize> {
    limits.validate()?;
    ensure!(
        (1..=8192).contains(&local_parent_tokens),
        "invalid original local Focus token allocation"
    );
    let ceiling = if initial_context_ready {
        local_parent_tokens / 2
    } else {
        local_parent_tokens
    };
    let available = ceiling
        .saturating_sub(cumulative_focus_tokens)
        .min(shared_remaining_tokens)
        .min(limits.max_input_tokens);
    Ok(available / limits.pair_input_tokens * limits.pair_input_tokens)
}

/// Ordinary *single-index* Search only. The caller supplies the actual Host
/// attempt/frame allowance. Nested Answer supplies its captured parent budget
/// instead; an unpartitioned mixed-index request must never call this helper.
pub fn ordinary_search_allocation(
    attempt: crate::read_attempt::ReadAttemptBudget,
    clock: crate::read_attempt::ReadClock,
    now_ns: u64,
    returned_count: usize,
    requested_limit: usize,
    limits: &FocusProfileLimits,
    remaining_frame_bytes: usize,
) -> Result<Option<Budget>> {
    limits.validate()?;
    crate::validate_budget(requested_limit, 1)?;
    ensure!(
        returned_count <= requested_limit && returned_count <= MAX_RETURNED_CANDIDATES,
        "Focus actual returned count exceeds canonical Search limits"
    );
    let deadline_ms = attempt
        .remaining_at(clock, now_ns)?
        .as_millis()
        .min(120_000) as u64;
    let candidates = returned_count
        .min(requested_limit)
        .min(64)
        .min(limits.max_candidates)
        .min(limits.max_input_tokens / limits.pair_input_tokens);
    let max_bytes = remaining_frame_bytes.min(65_536);
    if deadline_ms == 0 || candidates == 0 || max_bytes < 1024 {
        return Ok(None);
    }
    let budget = Budget {
        calls: 1,
        rounds: 1,
        fanout: 1,
        input_tokens: limits.max_input_tokens,
        candidates,
        max_bytes,
        deadline_ms,
    };
    budget.validate()?;
    Ok(Some(budget))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusScore {
    pub candidate: Sha256Digest,
    pub no_logit: f32,
    pub yes_logit: f32,
    /// Publisher two-logit normalization; not calibrated sufficiency/confidence.
    pub relevance: f32,
}
impl FocusScore {
    pub fn from_logits(candidate: Sha256Digest, no_logit: f32, yes_logit: f32) -> Result<Self> {
        ensure!(
            no_logit.is_finite() && yes_logit.is_finite(),
            "nonfinite Focus logits"
        );
        let peak = no_logit.max(yes_logit);
        let no = (no_logit - peak).exp();
        let yes = (yes_logit - peak).exp();
        let relevance = yes / (no + yes);
        ensure!(relevance.is_finite(), "nonfinite Focus relevance");
        Ok(Self {
            candidate,
            no_logit,
            yes_logit,
            relevance,
        })
    }
    pub fn validate(&self) -> Result<()> {
        let expected = Self::from_logits(self.candidate, self.no_logit, self.yes_logit)?;
        ensure!(
            self.relevance.to_bits() == expected.relevance.to_bits(),
            "Focus score differs from exact two-logit normalization"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusPresentation {
    pub candidate: Sha256Digest,
    pub utf8_start: usize,
    pub utf8_end: usize,
    pub source_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusIdentity {
    pub profile: Sha256Digest,
    pub runtime: String,
    pub compiler: Sha256Digest,
    pub calibration: Sha256Digest,
    pub task_policy: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusApplicationReceipt {
    pub schema: String,
    pub scope: ScopeBinding,
    pub identity: Option<FocusIdentity>,
    pub availability: Availability,
    pub reason: String,
    pub applied: bool,
    pub input_digest: Sha256Digest,
    /// Complete canonical returned candidate order, including the unscored tail.
    pub original_order: Vec<Sha256Digest>,
    pub ranked_order: Vec<Sha256Digest>,
    /// Scored candidates MUST be the exact original prefix, in original order.
    pub scores: Vec<FocusScore>,
    /// Exact source windows prepared for scoring, not proof of successful FFI.
    pub presentations: Vec<FocusPresentation>,
    pub presented_count: usize,
    pub candidate_count: usize,
    pub scored_count: usize,
    pub omitted_count: usize,
    pub reserved_input_tokens: usize,
    pub observed_input_tokens: Option<usize>,
    pub work: Work,
}
impl FocusApplicationReceipt {
    pub fn validate(&self, budget: &Budget) -> Result<()> {
        budget.validate()?;
        ensure!(
            self.schema == RANKING_SCHEMA && !self.reason.is_empty() && self.reason.len() <= 1024,
            "invalid Focus ranking schema/reason"
        );
        ensure!(
            self.original_order.len() <= MAX_RETURNED_CANDIDATES
                && self.original_order.len() == self.candidate_count
                && self.scores.len() == self.scored_count
                && self.presentations.len() == self.presented_count
                && self.scored_count <= self.presented_count
                && self.presented_count <= budget.candidates
                && self.presented_count.checked_add(self.omitted_count)
                    == Some(self.candidate_count),
            "invalid Focus candidate/scoring accounting"
        );
        let original: BTreeSet<_> = self.original_order.iter().copied().collect();
        let ranked: BTreeSet<_> = self.ranked_order.iter().copied().collect();
        ensure!(
            original.len() == self.original_order.len()
                && self.ranked_order.len() == self.original_order.len()
                && ranked == original,
            "Focus must preserve the entire canonical candidate permutation"
        );
        ensure!(
            self.reserved_input_tokens <= budget.input_tokens
                && self.work.input_tokens == self.observed_input_tokens.unwrap_or(0)
                && self.work.calls == 0
                && self.work.rounds == 0
                && self.work.fanout == 0
                && self.work.bytes == 0
                && self.work.candidates == self.scored_count,
            "Focus scoring work exceeds original allocation or invents source reads"
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
        let publication = &self.scope.publication;
        ensure!(
            !publication.collection.is_empty()
                && publication.collection.len() <= 4096
                && publication.incarnation == publication.token.incarnation
                && publication.generation == publication.token.generation
                && publication.revision == publication.token.revision,
            "invalid Focus receipt publication"
        );
        if !self.scores.is_empty()
            || self.observed_input_tokens.is_none()
            || self.reserved_input_tokens > 0
            || self.observed_input_tokens.is_some_and(|tokens| tokens > 0)
        {
            ensure!(
                self.scope.configuration_revision > 0
                    && self
                        .identity
                        .as_ref()
                        .is_some_and(|identity| identity.runtime == RUNTIME),
                "observed Focus work requires its current profile identity"
            );
        }
        for (presentation, expected) in self.presentations.iter().zip(&self.original_order) {
            ensure!(
                presentation.candidate == *expected
                    && presentation.utf8_start < presentation.utf8_end
                    && presentation.utf8_end <= presentation.source_bytes,
                "Focus source-window disclosure differs"
            );
        }
        for (score, expected) in self.scores.iter().zip(&self.original_order) {
            score.validate()?;
            ensure!(
                score.candidate == *expected,
                "Focus scored a foreign or non-prefix candidate"
            );
        }
        if self.applied {
            let identity = self
                .identity
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("applied Focus identity absent"))?;
            ensure!(
                identity.runtime == RUNTIME
                    && self.availability == Availability::Ready
                    && self.scope.configuration_revision > 0
                    && !self.scores.is_empty()
                    && self.scored_count == self.presented_count
                    && self.observed_input_tokens.is_some_and(|tokens| tokens > 0)
                    && self.work.compile_us.is_some()
                    && self.work.inference_us.is_some(),
                "applied Focus lacks current complete observed scoring"
            );
            let mut ordered = self.scores.iter().enumerate().collect::<Vec<_>>();
            ordered.sort_by(|(left_index, left), (right_index, right)| {
                right
                    .relevance
                    .total_cmp(&left.relevance)
                    .then(left_index.cmp(right_index))
            });
            let expected = ordered
                .into_iter()
                .map(|(_, score)| score.candidate)
                .chain(self.original_order[self.scored_count..].iter().copied())
                .collect::<Vec<_>>();
            ensure!(
                self.ranked_order == expected,
                "Focus ordering or stable tie/tail preservation differs"
            );
            let observed_us = [
                self.work.queue_us,
                self.work.compile_us,
                self.work.inference_us,
                self.work.policy_us,
            ]
            .into_iter()
            .flatten()
            .try_fold(0_u64, |total, value| total.checked_add(value))
            .ok_or_else(|| anyhow::anyhow!("Focus timing overflow"))?;
            ensure!(
                observed_us <= budget.deadline_ms * 1000,
                "successful Focus exceeded original deadline"
            );
        } else {
            ensure!(
                self.ranked_order == self.original_order
                    && self.availability != Availability::Ready,
                "unapplied Focus cannot expose partial ranked output"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= budget.max_bytes,
            "Focus receipt exceeds original byte budget"
        );
        Ok(())
    }
    pub fn validate_search_response(
        &self,
        response: &crate::SearchResponse,
        budget: &Budget,
    ) -> Result<()> {
        self.validate(budget)?;
        ensure!(
            response.publication.as_ref() == Some(&self.scope.publication)
                && response.collection == self.scope.publication.collection
                && response.generation == self.scope.publication.generation
                && response.hits.len() == self.candidate_count,
            "Focus canonical Search source/count changed"
        );
        for presentation in &self.presentations {
            let position = self
                .ranked_order
                .iter()
                .position(|id| *id == presentation.candidate)
                .ok_or_else(|| anyhow::anyhow!("Focus presentation candidate not returned"))?;
            let hit = &response.hits[position];
            ensure!(
                presentation.source_bytes == hit.text.len()
                    && hit
                        .text
                        .get(presentation.utf8_start..presentation.utf8_end)
                        .is_some(),
                "Focus presented window differs from complete returned source"
            );
        }
        Ok(())
    }
    /// The canonical owner supplies actual identities and retains every original
    /// value, including future metadata; this helper has no source authority.
    pub fn reorder<T: Clone>(
        &self,
        original: &[T],
        ids: &[Sha256Digest],
        budget: &Budget,
    ) -> Result<Vec<T>> {
        self.validate(budget)?;
        ensure!(
            ids == self.original_order && original.len() == ids.len(),
            "Focus receipt does not bind the actual canonical result"
        );
        self.ranked_order
            .iter()
            .map(|id| {
                let position = ids
                    .iter()
                    .position(|actual| actual == id)
                    .ok_or_else(|| anyhow::anyhow!("foreign Focus result identity"))?;
                Ok(original[position].clone())
            })
            .collect()
    }
}

/// Only the canonical Search field carries this observation. A compact notice
/// is known pre-dispatch absence, never a substitute for unknown native work.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FocusObservation {
    Receipt {
        receipt: Box<FocusApplicationReceipt>,
    },
    NotApplied {
        availability: Availability,
        reason: String,
        candidate_count: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order_digest: Option<Sha256Digest>,
        work_unknown: bool,
    },
}
impl FocusObservation {
    pub fn receipt(&self) -> Option<&FocusApplicationReceipt> {
        match self {
            Self::Receipt { receipt } => Some(receipt),
            Self::NotApplied { .. } => None,
        }
    }
    pub fn validate(&self, budget: &Budget) -> Result<()> {
        match self {
            Self::Receipt { receipt } => receipt.validate(budget),
            Self::NotApplied {
                availability,
                reason,
                candidate_count,
                work_unknown,
                ..
            } => {
                ensure!(
                    !work_unknown
                        && !reason.is_empty()
                        && reason.len() <= 1024
                        && *candidate_count <= MAX_RETURNED_CANDIDATES
                        && !matches!(
                            availability,
                            Availability::Ready
                                | Availability::Disabled
                                | Availability::IndexDisabled
                        ),
                    "compact Focus notice cannot claim execution, readiness or off-state effects"
                );
                Ok(())
            }
        }
    }
    pub fn not_applied_if_fits(
        availability: Availability,
        reason: String,
        candidate_count: usize,
        order_digest: Option<Sha256Digest>,
        remaining_frame_bytes: usize,
    ) -> Result<Option<Self>> {
        let notice = Self::NotApplied {
            availability,
            reason,
            candidate_count,
            order_digest,
            work_unknown: false,
        };
        notice.validate(&Budget::default())?;
        // Existing Search object has other fields: key + colon + separating comma.
        let bytes = serde_json::to_vec(&notice)?
            .len()
            .checked_add(9)
            .ok_or_else(|| anyhow::anyhow!("Focus notice byte accounting overflow"))?;
        Ok((bytes <= remaining_frame_bytes).then_some(notice))
    }
}

pub type FocusRanking = FocusApplicationReceipt;

#[cfg(test)]
pub(crate) mod tests;
/// Returns only the native canonical Search field of an actual Search record.
/// Source text, future metadata and other read-operation payloads are not scanned.
pub fn search_record_observation(record: &EvidenceRecord) -> Result<Option<FocusObservation>> {
    if record.operation != NativeRead::Search
        || record
            .canonical_result
            .get("focus")
            .is_none_or(serde_json::Value::is_null)
    {
        return Ok(None);
    }
    let response: crate::SearchResponse = serde_json::from_value(record.canonical_result.clone())?;
    let observation = response
        .focus
        .clone()
        .ok_or_else(|| anyhow::anyhow!("canonical Focus observation missing"))?;
    observation.validate(&validation_budget())?;
    match &observation {
        FocusObservation::Receipt { receipt } => {
            receipt.validate_search_response(&response, &validation_budget())?;
            ensure!(
                receipt.scope == record.scope
                    && response.publication.as_ref() == Some(&record.scope.publication)
                    && response.collection == record.scope.publication.collection
                    && response.generation == record.scope.publication.generation
                    && response.hits.len() == receipt.candidate_count,
                "Focus receipt is not bound to its canonical Search record"
            );
        }
        FocusObservation::NotApplied {
            candidate_count, ..
        } => {
            ensure!(
                *candidate_count == response.hits.len(),
                "compact Focus candidate count differs from original Search"
            );
        }
    }
    Ok(Some(observation))
}
/// Structural outer delivery validation ceiling, never a dispatch allocation.
pub fn validation_budget() -> Budget {
    Budget {
        calls: 12,
        rounds: 4,
        fanout: 12,
        input_tokens: 8192,
        candidates: 64,
        max_bytes: 65_536,
        deadline_ms: 120_000,
    }
}
#[derive(Clone, Debug, Default)]
pub struct FocusWorkSummary {
    pub work: Work,
    pub work_unknown: bool,
    pub failed_token_overrun: bool,
    pub has_native_receipt: bool,
}
/// Known scoring work is additive across distinct actual Search receipts.
/// A partial/unknown native observation is not a fresh allowance or zero cost.
pub fn observed_focus_work(
    records: &[EvidenceRecord],
    budget: &Budget,
) -> Result<FocusWorkSummary> {
    let mut summary = FocusWorkSummary::default();
    let mut inputs = BTreeSet::new();
    for record in records {
        let Some(FocusObservation::Receipt { receipt }) = search_record_observation(record)? else {
            continue;
        };
        receipt.validate(budget)?;
        if receipt.identity.is_some()
            && (receipt.reserved_input_tokens > 0
                || receipt.observed_input_tokens.is_none()
                || receipt.work.input_tokens > 0
                || !receipt.scores.is_empty())
        {
            ensure!(
                inputs.insert(receipt.input_digest),
                "repeated Focus native input receipt"
            );
            summary.has_native_receipt = true;
        }
        summary.work_unknown |= receipt.observed_input_tokens.is_none();
        summary.failed_token_overrun |= receipt
            .observed_input_tokens
            .is_some_and(|observed| observed > receipt.reserved_input_tokens);
        summary.work.input_tokens = summary
            .work
            .input_tokens
            .checked_add(receipt.work.input_tokens)
            .ok_or_else(|| anyhow::anyhow!("Focus aggregate token overflow"))?;
        summary.work.candidates = summary.work.candidates.max(receipt.work.candidates);
        for (total, value) in [
            (&mut summary.work.queue_us, receipt.work.queue_us),
            (&mut summary.work.compile_us, receipt.work.compile_us),
            (&mut summary.work.inference_us, receipt.work.inference_us),
            (&mut summary.work.policy_us, receipt.work.policy_us),
        ] {
            if let Some(value) = value {
                *total = Some(
                    total
                        .unwrap_or(0)
                        .checked_add(value)
                        .ok_or_else(|| anyhow::anyhow!("Focus aggregate timing overflow"))?,
                );
            }
        }
    }
    Ok(summary)
}
