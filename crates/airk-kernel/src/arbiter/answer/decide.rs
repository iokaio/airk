// SPDX-License-Identifier: Apache-2.0
//! Decide: every card decision of the loop, as the decision log records it.
//! A decision is taken only at a round barrier, from state ordered by
//! dispatch, so concurrency cannot change it. With no model, or when the
//! model abstains or is unavailable, the deterministic order of section 12.2
//! decides and the record says which rule did.
use super::*;
use airk_core::arbiter::context_action::{EvidenceRef, SourceUnitKind};
use airk_core::native_loop::LoopCard;

/// Where a decision applies: Split has no part; Plan has no sub-need or
/// round; coverage and follow-up decisions have all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) stage: v3::LoopStage,
    pub(crate) part: Option<u8>,
    pub(crate) need: Option<u8>,
    pub(crate) round: Option<u8>,
}
impl At {
    pub(crate) fn split() -> Self {
        Self {
            stage: v3::LoopStage::Split,
            part: None,
            need: None,
            round: None,
        }
    }
    pub(crate) fn plan(part: u8) -> Self {
        Self {
            stage: v3::LoopStage::Plan,
            part: Some(part),
            need: None,
            round: None,
        }
    }
    pub(crate) fn need(stage: v3::LoopStage, part: u8, need: u8, round: u8) -> Self {
        Self {
            stage,
            part: Some(part),
            need: Some(need),
            round: Some(round),
        }
    }
}

/// The roster policy identity of a card stage. The kernel builds these
/// rosters; L4's task policies in airk-retrieval replace this identity with
/// their own hashed policies when the model path is admitted.
pub(crate) fn policy(stage: v3::LoopStage) -> Sha256Digest {
    framed_digest(
        b"airk.arbiter.loop-kernel-roster/1",
        &[format!("{stage:?}").as_bytes()],
    )
}

/// A span shown to a decision, as the log's basis records it.
pub(crate) fn basis(record: Sha256Digest, pointer: &str, text: &str) -> EvidenceRef {
    EvidenceRef {
        record,
        kind: SourceUnitKind::Hit,
        result_pointer: pointer.to_owned(),
        utf8_start: 0,
        utf8_end: text.len(),
    }
}

/// The decision log under construction.
#[derive(Default)]
pub(crate) struct Ledger {
    pub(crate) entries: Vec<v3::DecisionLogEntry>,
    pub(crate) receipts: Vec<v3::CardAdvice>,
    pub(crate) focus: Vec<v3::FocusBatchReceipt>,
}

impl Ledger {
    pub(crate) fn next_ordinal(&self) -> Result<u8> {
        let next = self.entries.len() + 1;
        ensure!(next <= v3::MAX_DECISIONS, "Answer decision log is full");
        Ok(next as u8)
    }

    /// Earlier decisions of the same stage, part and sub-need are no longer
    /// current once a later one is taken.
    fn supersede(&mut self, at: At) {
        for entry in &mut self.entries {
            let decision = &mut entry.decision;
            if decision.applicability == v3::AssessmentApplicability::Current
                && decision.stage == at.stage
                && decision.part == at.part
                && decision.need == at.need
            {
                decision.applicability = v3::AssessmentApplicability::HistoricalAfterRead;
            }
        }
    }

    /// A configuration change makes every current decision historical.
    pub(crate) fn configuration_changed(&mut self) {
        for entry in &mut self.entries {
            if entry.decision.applicability == v3::AssessmentApplicability::Current {
                entry.decision.applicability =
                    v3::AssessmentApplicability::HistoricalAfterConfigurationChange;
            }
        }
    }

    /// Whether any decision carries native model work.
    #[cfg_attr(not(test), allow(dead_code))] // the model path (L3-8)
    pub(crate) fn has_native_work(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.decision.scored_by_model())
            || !self.focus.is_empty()
    }

    /// Record a decision the deterministic order took.
    pub(crate) fn deterministic(
        &mut self,
        at: At,
        cards: &[LoopCard],
        chosen: &[usize],
        rule: v3::DeterministicRule,
        basis: Vec<EvidenceRef>,
    ) -> Result<u8> {
        self.record(
            at,
            cards,
            chosen,
            v3::DecisionSource::Deterministic { rule },
            basis,
            None,
        )
    }

    /// Record one decision. `model` carries the facts of a model-scored one.
    pub(crate) fn record(
        &mut self,
        at: At,
        cards: &[LoopCard],
        chosen: &[usize],
        source: v3::DecisionSource,
        basis: Vec<EvidenceRef>,
        model: Option<ModelFacts>,
    ) -> Result<u8> {
        let ordinal = self.next_ordinal()?;
        ensure!(
            !cards.is_empty()
                && !chosen.is_empty()
                && chosen.iter().all(|index| *index < cards.len()),
            "Answer decision chose outside its roster"
        );
        let records = cards
            .iter()
            .map(LoopCard::record)
            .collect::<Result<Vec<_>>>()?;
        let basis_summary = v3::BasisSummary {
            presented: u16::try_from(basis.len())?,
            omitted: 0,
            digest: v3::basis_digest(&basis, &[])?,
        };
        let facts = model.clone().unwrap_or_default();
        let mut sorted = facts.scores.clone();
        sorted.sort_by(|left, right| right.total_cmp(left));
        let top = sorted.first().copied();
        let runner_up = sorted.get(1).copied();
        let decision = v3::DecisionRecord {
            ordinal,
            stage: at.stage,
            part: at.part,
            need: at.need,
            round: at.round,
            roster_digest: v3::roster_digest(at.stage, &records)?,
            card_slot: facts.card_slot,
            chosen: chosen
                .iter()
                .map(|index| v3::ChosenCard {
                    symbol: cards[*index].symbol.clone(),
                    class: cards[*index].class,
                })
                .collect(),
            source,
            top,
            runner_up,
            margin: facts.margin,
            model: facts.model,
            device: facts.device,
            precision: facts.precision,
            batched_with: facts.batched_with.clone(),
            applicability: v3::AssessmentApplicability::Current,
            receipt: facts.receipt,
            basis: basis_summary,
            work_unknown: facts.work_unknown,
        };
        self.supersede(at);
        self.entries.push(v3::DecisionLogEntry {
            decision,
            policy: policy(at.stage),
            cards: records,
            scores: facts.scores,
            chosen: chosen.iter().map(|index| *index as u8).collect(),
            basis,
            basis_omitted: vec![],
        });
        Ok(ordinal)
    }

    pub(crate) fn log(&self, question: &str, scope: &ScopeBinding) -> v3::DecisionLog {
        v3::DecisionLog {
            schema: v3::DECISION_LOG_SCHEMA.into(),
            question: question.to_owned(),
            scope: scope.clone(),
            entries: self.entries.clone(),
            receipts: self.receipts.clone(),
            focus_receipts: self.focus.clone(),
        }
    }
}

/// The model facts of a model-scored decision.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModelFacts {
    pub(crate) card_slot: Option<u8>,
    pub(crate) scores: Vec<f32>,
    pub(crate) margin: Option<f32>,
    pub(crate) model: Option<device::SelectedModel>,
    pub(crate) device: Option<v3::ExecutionDevice>,
    pub(crate) precision: Option<device::DevicePrecision>,
    pub(crate) batched_with: Vec<u8>,
    pub(crate) receipt: Option<Sha256Digest>,
    pub(crate) work_unknown: bool,
}

#[cfg(test)]
mod tests;
