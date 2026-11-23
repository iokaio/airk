// SPDX-License-Identifier: Apache-2.0
//! The decision log: service custody of every card decision's full roster,
//! scores, basis and receipts. The reply carries only its summary.
use super::*;
use crate::arbiter::context_action::EvidenceRef;
use crate::arbiter::device::SelectedModel;

pub const DECISION_LOG_SCHEMA: &str = "airk.arbiter.decision-log/1";

/// A presented card: decision symbol, class, text digest and, for Split
/// cards, the ordered partition of the question it proposes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardRecord {
    pub symbol: String,
    pub class: CardClass,
    pub text_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition: Vec<QuestionSpan>,
}

/// A partition is one to four contiguous, non-empty ranges that cover the
/// whole question, in order, on character boundaries.
pub fn validate_partition(question: &str, partition: &[QuestionSpan]) -> Result<()> {
    let mut next = 0;
    for span in partition {
        ensure!(
            span.utf8_start == next
                && span.utf8_start < span.utf8_end
                && span.utf8_end <= question.len()
                && question.is_char_boundary(span.utf8_end),
            "Split partition ranges must be contiguous and non-empty"
        );
        next = span.utf8_end;
    }
    ensure!(
        (1..=MAX_PARTS).contains(&partition.len()) && next == question.len(),
        "Split partition must cover the question in at most four ranges"
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionLogEntry {
    pub decision: DecisionRecord,
    /// The roster's task policy identity.
    pub policy: Sha256Digest,
    pub cards: Vec<CardRecord>,
    /// Raw scores in card order; empty when no model scored.
    pub scores: Vec<f32>,
    /// Indexes of the chosen cards.
    pub chosen: Vec<u8>,
    /// What the decision that stands rests on. When a model's choice
    /// stands: the byte ranges of the spans it was shown, then, as omitted,
    /// the ranges of those spans it was not shown (the text before and after
    /// an excerpt, and whole spans not shown). When the deterministic order
    /// decided: the whole spans it read, and nothing omitted.
    pub basis: Vec<EvidenceRef>,
    pub basis_omitted: Vec<EvidenceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionLog {
    pub schema: String,
    pub question: String,
    pub scope: ScopeBinding,
    pub entries: Vec<DecisionLogEntry>,
    pub receipts: Vec<CardAdvice>,
    pub focus_receipts: Vec<FocusBatchReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageSourceCount {
    pub stage: LoopStage,
    pub source: SourceKind,
    /// The scoring model; absent for deterministic decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<SelectedModel>,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopTimings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_us: Option<u64>,
}
impl LoopTimings {
    pub(crate) fn add(&mut self, work: &Work) -> Result<()> {
        for (total, measured) in [
            (&mut self.queue_us, work.queue_us),
            (&mut self.compile_us, work.compile_us),
            (&mut self.inference_us, work.inference_us),
            (&mut self.policy_us, work.policy_us),
        ] {
            if let Some(measured) = measured {
                *total = Some(
                    total
                        .unwrap_or(0)
                        .checked_add(measured)
                        .ok_or_else(|| anyhow::anyhow!("Answer timing accounting overflow"))?,
                );
            }
        }
        Ok(())
    }
    pub(crate) fn pairs(&self, work: &Work) -> [(Option<u64>, Option<u64>); 4] {
        [
            (self.queue_us, work.queue_us),
            (self.compile_us, work.compile_us),
            (self.inference_us, work.inference_us),
            (self.policy_us, work.policy_us),
        ]
    }
}

/// What the reply says about the whole decision log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionLogSummary {
    /// Digest of the complete custody log.
    pub digest: Sha256Digest,
    pub total: usize,
    pub included: usize,
    pub omitted: usize,
    /// Distinct card slots used.
    pub card_calls: usize,
    /// Decisions per stage and source, over the whole log.
    pub counts: Vec<StageSourceCount>,
    pub focus_calls: usize,
    pub focus_pairs: usize,
    /// Model tokens charged per part and round by every card and Focus receipt.
    pub attribution: Vec<TokenAttribution>,
    /// Measured model timings summed over every receipt.
    pub timings: LoopTimings,
    /// Decisions and Focus calls whose work is unknown.
    pub work_unknown: usize,
}
impl DecisionLogSummary {
    pub fn count(&self, stage: Option<LoopStage>, source: SourceKind) -> usize {
        self.counts
            .iter()
            .filter(|row| row.source == source && stage.is_none_or(|stage| row.stage == stage))
            .map(|row| row.count)
            .sum()
    }
}

pub fn basis_digest(presented: &[EvidenceRef], omitted: &[EvidenceRef]) -> Result<Sha256Digest> {
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.loop-card.basis/2",
        &[
            &serde_json::to_vec(presented)?,
            &serde_json::to_vec(omitted)?,
        ],
    ))
}

/// The digest of a presented roster: its stage and every card record, so a
/// swapped Split partition or text changes it.
pub fn roster_digest(stage: LoopStage, cards: &[CardRecord]) -> Result<Sha256Digest> {
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.loop-card.roster/2",
        &[&serde_json::to_vec(&stage)?, &serde_json::to_vec(cards)?],
    ))
}

/// The fixed digest of an empty decision log (no decisions, receipts or
/// Focus calls), whatever its question and scope.
pub fn empty_log_digest() -> Sha256Digest {
    crate::identity::framed_digest(b"airk.arbiter.decision-log.empty/1", &[])
}

impl DecisionLog {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.receipts.is_empty() && self.focus_receipts.is_empty()
    }
    pub fn digest(&self) -> Result<Sha256Digest> {
        if self.is_empty() {
            return Ok(empty_log_digest());
        }
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.decision-log/1",
            &[&serde_json::to_vec(self)?],
        ))
    }

    pub fn validate(&self, budget: &BudgetV3) -> Result<()> {
        budget.validate()?;
        validate_question(&self.question)?;
        ensure!(
            self.schema == DECISION_LOG_SCHEMA && self.entries.len() <= MAX_DECISIONS,
            "invalid decision log schema or size"
        );
        let mut receipts = BTreeMap::new();
        for receipt in &self.receipts {
            receipt.validate()?;
            ensure!(
                receipt.scope == self.scope
                    && receipts.insert(receipt.input_digest, receipt).is_none(),
                "invalid or duplicate decision log receipt"
            );
        }
        let mut slots = BTreeMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let decision = &entry.decision;
            ensure!(
                usize::from(decision.ordinal) == index + 1,
                "decision log ordinals are not contiguous"
            );
            decision.validate(budget.context_calls)?;
            self.validate_entry(entry)?;
            if let Some(slot) = decision.card_slot {
                let receipt = slots.entry(slot).or_insert(decision.receipt);
                ensure!(
                    *receipt == decision.receipt,
                    "decisions of one card slot name different receipts"
                );
            }
            if let Some(id) = decision.receipt {
                let receipt = receipts
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("decision names an unknown receipt"))?;
                self.validate_receipt_use(entry, receipt)?;
            }
        }
        ensure!(
            slots.len() <= budget.context_calls,
            "decision log card calls exceed their slots"
        );
        ensure!(
            self.entries
                .iter()
                .filter(|entry| entry.decision.stage == LoopStage::Split)
                .count()
                <= 1,
            "a decision log holds at most one Split decision"
        );
        for entry in &self.entries {
            let decision = &entry.decision;
            ensure!(
                decision.applicability != AssessmentApplicability::Current
                    || !self.entries.iter().any(|later| {
                        later.decision.ordinal > decision.ordinal
                            && later.decision.stage == decision.stage
                            && later.decision.part == decision.part
                            && later.decision.need == decision.need
                    }),
                "only the latest logged decision can remain current"
            );
        }
        for receipt in receipts.values() {
            let named: BTreeSet<_> = self
                .entries
                .iter()
                .filter(|entry| entry.decision.receipt == Some(receipt.input_digest))
                .map(|entry| entry.decision.ordinal)
                .collect();
            let segments: BTreeSet<_> = receipt.segments.iter().map(|s| s.ordinal).collect();
            ensure!(
                !named.is_empty() && named == segments,
                "receipt segments differ from the decisions naming it"
            );
        }
        let mut inputs = BTreeSet::new();
        let mut pairs = 0usize;
        for focus in &self.focus_receipts {
            focus.validate(&self.scope, budget)?;
            ensure!(
                inputs.insert(focus.input_digest),
                "repeated Focus batch receipt"
            );
            pairs += focus.pairs.len();
        }
        ensure!(
            self.focus_receipts.len() <= budget.focus_calls && pairs <= budget.focus_pairs,
            "decision log Focus work exceeds its budget"
        );
        Ok(())
    }

    fn validate_entry(&self, entry: &DecisionLogEntry) -> Result<()> {
        let decision = &entry.decision;
        ensure!(
            (1..=MAX_ACTION_CARDS).contains(&entry.cards.len())
                && unique(entry.cards.iter().map(|card| &card.symbol))
                && unique(entry.cards.iter().map(|card| card.text_digest))
                && entry
                    .cards
                    .iter()
                    .all(|card| card.class.stage() == decision.stage
                        && !card.symbol.is_empty()
                        && card.symbol.len() <= MAX_SYMBOL_BYTES),
            "invalid decision log roster"
        );
        for card in &entry.cards {
            match card.class {
                CardClass::SplitAt => {
                    validate_partition(&self.question, &card.partition)?;
                    ensure!(
                        card.partition.len() >= 2,
                        "SplitAt proposes two or more parts"
                    );
                }
                CardClass::KeepWhole => {
                    validate_partition(&self.question, &card.partition)?;
                    ensure!(card.partition.len() == 1, "KeepWhole is one range");
                }
                _ => ensure!(
                    card.partition.is_empty(),
                    "only Split cards carry a partition"
                ),
            }
        }
        ensure!(
            !entry.chosen.is_empty()
                && unique(&entry.chosen)
                && entry
                    .chosen
                    .iter()
                    .all(|index| usize::from(*index) < entry.cards.len())
                && entry.chosen.len() <= entry.cards.len(),
            "invalid decision log chosen cards"
        );
        let chosen: Vec<_> = entry
            .chosen
            .iter()
            .map(|index| {
                let card = &entry.cards[usize::from(*index)];
                ChosenCard {
                    symbol: card.symbol.clone(),
                    class: card.class,
                }
            })
            .collect();
        ensure!(
            chosen == decision.chosen,
            "reply decision differs from its logged chosen cards"
        );
        ensure!(
            entry.scores.iter().all(|score| score.is_finite())
                && (entry.scores.is_empty() || entry.scores.len() == entry.cards.len()),
            "invalid decision log scores"
        );
        let mut sorted = entry.scores.clone();
        sorted.sort_by(|left, right| right.total_cmp(left));
        ensure!(
            decision.top.map(f32::to_bits) == sorted.first().map(|score| score.to_bits())
                && decision.runner_up.map(f32::to_bits)
                    == sorted.get(1).map(|score| score.to_bits()),
            "reply top and runner-up differ from the logged scores"
        );
        ensure!(
            usize::from(decision.basis.presented) == entry.basis.len()
                && usize::from(decision.basis.omitted) == entry.basis_omitted.len()
                && decision.basis.digest == basis_digest(&entry.basis, &entry.basis_omitted)?,
            "reply basis differs from the logged basis"
        );
        ensure!(
            decision.roster_digest == roster_digest(decision.stage, &entry.cards)?,
            "logged roster differs from its digest"
        );
        Ok(())
    }

    fn validate_receipt_use(&self, entry: &DecisionLogEntry, receipt: &CardAdvice) -> Result<()> {
        let decision = &entry.decision;
        ensure!(
            Some(receipt.task) == card_task(decision.stage)
                && receipt.task_policy == entry.policy
                && Some(receipt.card_slot) == decision.card_slot,
            "decision and receipt task, policy or slot differ"
        );
        let charge = (decision.part.unwrap_or(0), decision.round.unwrap_or(0));
        ensure!(
            receipt
                .attribution
                .iter()
                .any(|item| (item.part, item.round) == charge),
            "a batched segment's part and round carry no charge"
        );
        let peers: BTreeSet<_> = self
            .entries
            .iter()
            .filter(|other| {
                other.decision.receipt == Some(receipt.input_digest)
                    && other.decision.ordinal != decision.ordinal
            })
            .map(|other| other.decision.ordinal)
            .collect();
        ensure!(
            peers == decision.batched_with.iter().copied().collect(),
            "decisions sharing a receipt must name each other"
        );
        let segment = receipt
            .segments
            .iter()
            .find(|segment| segment.ordinal == decision.ordinal)
            .ok_or_else(|| anyhow::anyhow!("receipt lacks the decision's segment"))?;
        ensure!(
            entry.scores.is_empty() || segment.raw_scores == entry.scores,
            "decision scores differ from the receipt"
        );
        let digests: Vec<_> = entry
            .chosen
            .iter()
            .map(|index| entry.cards[usize::from(*index)].text_digest)
            .collect();
        let consistent = match (&decision.source, &segment.outcome) {
            (DecisionSource::Model { rule, .. }, Outcome::Accepted { decision: accepted }) => {
                let chosen = match accepted {
                    Decision::LoopCard(card) => vec![*card],
                    Decision::LoopCards(cards) => cards.clone(),
                    _ => vec![],
                };
                chosen == digests
                    && segment.rule == *rule
                    && segment.margin.map(f32::to_bits) == decision.margin.map(f32::to_bits)
            }
            (DecisionSource::Abstained { .. }, Outcome::Abstained { .. }) => true,
            (
                DecisionSource::Unavailable { .. },
                Outcome::Unavailable { .. }
                | Outcome::Unsupported { .. }
                | Outcome::InvalidInput { .. },
            ) => true,
            _ => false,
        };
        ensure!(consistent, "decision differs from its receipt outcome");
        Ok(())
    }

    /// The reply's summary of this log, for the given included ordinals.
    pub fn summary(&self, included: usize) -> Result<DecisionLogSummary> {
        let mut counts: BTreeMap<(LoopStage, SourceKind, Option<SelectedModel>), usize> =
            BTreeMap::new();
        let mut slots = BTreeSet::new();
        let mut unknown = 0;
        for entry in &self.entries {
            *counts
                .entry((
                    entry.decision.stage,
                    entry.decision.source.kind(),
                    entry.decision.model,
                ))
                .or_default() += 1;
            slots.extend(entry.decision.card_slot);
            unknown += usize::from(entry.decision.work_unknown);
        }
        let mut charges: BTreeMap<(u8, u8), usize> = BTreeMap::new();
        let mut timings = LoopTimings::default();
        for receipt in &self.receipts {
            timings.add(&receipt.work)?;
            for charge in &receipt.attribution {
                *charges.entry((charge.part, charge.round)).or_default() += charge.tokens;
            }
        }
        for focus in &self.focus_receipts {
            timings.add(&focus.work)?;
            unknown += usize::from(focus.observed_input_tokens.is_none());
            for charge in &focus.attribution {
                *charges.entry((charge.part, charge.round)).or_default() += charge.tokens;
            }
        }
        ensure!(
            included <= self.entries.len(),
            "more decisions included than logged"
        );
        Ok(DecisionLogSummary {
            digest: self.digest()?,
            total: self.entries.len(),
            included,
            omitted: self.entries.len() - included,
            card_calls: slots.len(),
            counts: counts
                .into_iter()
                .map(|((stage, source, model), count)| StageSourceCount {
                    stage,
                    source,
                    model,
                    count,
                })
                .collect(),
            focus_calls: self.focus_receipts.len(),
            focus_pairs: self
                .focus_receipts
                .iter()
                .map(|focus| focus.pairs.len())
                .sum(),
            attribution: charges
                .into_iter()
                .map(|((part, round), tokens)| TokenAttribution {
                    part,
                    round,
                    tokens,
                })
                .collect(),
            timings,
            work_unknown: unknown,
        })
    }
}

impl ContextResponseV3 {
    /// The reply checked against its custody log. The kernel calls this
    /// before delivery; reply-only validation cannot see omitted decisions.
    pub fn validate_with_log(&self, log: &DecisionLog, budget: &BudgetV3) -> Result<()> {
        self.validate(budget)?;
        ensure!(
            log.question == self.question && log.scope == self.scope,
            "decision log question or scope differs from the reply"
        );
        log.validate(budget)?;
        let expected = log.summary(self.decisions.len())?;
        ensure!(
            self.decision_log.digest == log.digest()?,
            "reply decision log digest differs from the log"
        );
        ensure!(
            self.decision_log == expected,
            "reply decision log summary differs from the log"
        );
        let profiles: BTreeSet<Sha256Digest> = log
            .focus_receipts
            .iter()
            .filter_map(|receipt| receipt.identity.as_ref().map(|identity| identity.profile))
            .collect();
        ensure!(
            profiles.len() <= 1 && self.model_work.focus_profile == profiles.first().copied(),
            "reply Focus profile differs from the logged Focus receipts"
        );
        for entry in &log.entries {
            let decision = &entry.decision;
            let present = match (decision.part, decision.need, decision.round) {
                (None, None, None) => true,
                (Some(part), None, None) => self.part(part).is_some(),
                (Some(part), Some(need), Some(round)) => {
                    self.has_need(part, need) && self.has_round(part, round)
                }
                _ => false,
            };
            ensure!(
                present,
                "logged decision names a part, sub-need or round the reply lacks"
            );
        }
        for decision in &self.decisions {
            let entry = log
                .entries
                .get(usize::from(decision.ordinal).wrapping_sub(1))
                .ok_or_else(|| anyhow::anyhow!("reply decision is not in the log"))?;
            ensure!(
                &entry.decision == decision,
                "reply decision differs from its logged decision"
            );
        }
        // Union omits the oldest deterministic decisions first, then the
        // oldest model-scored ones.
        let included: BTreeSet<u8> = self.decisions.iter().map(|d| d.ordinal).collect();
        let (deterministic, scored): (Vec<_>, Vec<_>) = log
            .entries
            .iter()
            .map(|entry| &entry.decision)
            .partition(|decision| !decision.scored_by_model());
        let omitted = log.entries.len() - included.len();
        let expected: BTreeSet<u8> = deterministic
            .iter()
            .chain(&scored)
            .take(omitted)
            .map(|decision| decision.ordinal)
            .collect();
        let actual: BTreeSet<u8> = log
            .entries
            .iter()
            .map(|entry| entry.decision.ordinal)
            .filter(|ordinal| !included.contains(ordinal))
            .collect();
        ensure!(
            actual == expected,
            "Union must omit the oldest deterministic decisions first, then the oldest scored"
        );
        let parts_degraded = self
            .degradations
            .iter()
            .any(|degradation| matches!(degradation, Degradation::Parts { .. }));
        if let Some(split) = log
            .entries
            .iter()
            .find(|entry| entry.decision.stage == LoopStage::Split)
            && !parts_degraded
        {
            let chosen = &split.cards[usize::from(split.chosen[0])].partition;
            ensure!(
                chosen.len() == self.parts.len()
                    && chosen.iter().zip(&self.parts).all(|(range, part)| {
                        range.utf8_start <= part.span.utf8_start
                            && part.span.utf8_end <= range.utf8_end
                    }),
                "the chosen Split partition differs from the reply's parts"
            );
        }
        Ok(())
    }
}
