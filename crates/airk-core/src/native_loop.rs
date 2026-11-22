// SPDX-License-Identifier: Apache-2.0
//! Private native inputs of the Answer loop (version 3): batched card rosters
//! and batched Focus with per-candidate questions. Values cannot construct
//! service authority. These sit beside the /2 native requests, which stay
//! until the loop's consumers replace them; `native_focus.rs` is unchanged.
use crate::{
    Hit,
    arbiter::{
        ScopeBinding, Task,
        answer_loop::{
            BudgetV3, CARD_REQUEST_SCHEMA, CardAdvice, CardClass, CardRecord, FocusBatchReceipt,
            LoopStage, MAX_CARD_SEGMENTS, MAX_DEADLINE_MS, MAX_DECISIONS, MAX_FOCUS_BATCH_PAIRS,
            MAX_FOCUS_QUESTIONS, MAX_QUESTION_BYTES, MAX_SYMBOL_BYTES, MAX_TEXT_BYTES,
            QuestionSpan, TokenAttribution, card_task, questions_digest, roster_digest,
            scope_digest, unique, validate_attribution, validate_partition,
        },
        context_action::MAX_ACTION_CARDS,
        focus::MAX_RETURNED_CANDIDATES,
    },
    identity::Sha256Digest,
    native_focus::FocusSourceWindow,
    read_attempt::{ReadAttemptBudget, ReadClock},
    scope_records::ScopedReadRef,
    service_records::{MAX_TOOLS_CALL_REQUEST_BYTES, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Observation only, outside the loop semantics digest (it decides nothing).
mod card_model;
pub use card_model::NativeCardModel;

pub const ALLOWANCE_SCHEMA_V3: &str = "airk.context-partition-allowance/3";

/// What one Answer /3 partition may consume: the /2 counters plus Focus
/// pairs. Card slots and Focus calls are the lengths of the allowance's
/// ordinal prefixes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopResources {
    pub calls: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub bytes: usize,
    pub focus_pairs: usize,
}

/// Native adapter metadata for one Answer /3 partition, with a BudgetV3
/// parent. Never public Answer arguments or a grant. The /2 allowance is
/// unchanged and stays with the /2 requests.
///
/// Each member of a combined Answer runs its own Answer, whose card slots and
/// Focus call ordinals are numbered from 1. The ordinal lists are therefore
/// the prefixes `1..=k`: counts in effect, with no cross-member meaning
/// (recorded contract decision, Core arbiter CONTRACT.md, "Combined Answer
/// /3 allowance").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionAllowanceV3 {
    pub schema: String,
    pub attempt: crate::identity::Incarnation,
    pub selection: Sha256Digest,
    pub index_id: crate::index_catalog::IndexId,
    pub parent_budget: BudgetV3,
    pub resources: LoopResources,
    /// Card slots this partition may use: exactly `1..=card slots`.
    pub card_ordinals: Vec<u8>,
    /// Focus call ordinals this partition may use: exactly `1..=Focus calls`.
    pub focus_ordinals: Vec<u8>,
}
/// The ordinal prefix `1..=count`.
pub fn ordinal_prefix(count: usize) -> Result<Vec<u8>> {
    (1..=count)
        .map(|ordinal| Ok(u8::try_from(ordinal)?))
        .collect()
}
impl PartitionAllowanceV3 {
    pub fn card_slots(&self) -> usize {
        self.card_ordinals.len()
    }
    pub fn focus_calls(&self) -> usize {
        self.focus_ordinals.len()
    }
    pub fn validate(&self) -> Result<()> {
        self.parent_budget.validate()?;
        let budget = &self.parent_budget;
        ensure!(
            self.schema == ALLOWANCE_SCHEMA_V3,
            "invalid context allowance schema"
        );
        ensure!(
            self.resources.calls > 0
                && self.resources.candidates > 0
                && self.resources.bytes >= 1024
                && self.resources.calls <= budget.calls
                && self.resources.input_tokens <= budget.input_tokens
                && self.resources.candidates <= budget.candidates
                && self.resources.bytes <= budget.max_bytes
                && self.resources.focus_pairs <= budget.focus_pairs,
            "invalid context partition allocation"
        );
        let prefix = |ordinals: &[u8], maximum: usize| {
            ordinals.len() <= maximum
                && ordinals
                    .iter()
                    .enumerate()
                    .all(|(offset, ordinal)| usize::from(*ordinal) == offset + 1)
        };
        ensure!(
            prefix(&self.card_ordinals, budget.context_calls)
                && prefix(&self.focus_ordinals, budget.focus_calls),
            "invalid card slot or Focus ordinals"
        );
        ensure!(
            self.focus_ordinals.is_empty() == (self.resources.focus_pairs == 0)
                && self.focus_calls() <= self.resources.focus_pairs,
            "Focus calls and Focus pairs disagree"
        );
        ensure!(
            (self.card_ordinals.is_empty() && self.focus_ordinals.is_empty())
                == (self.resources.input_tokens == 0),
            "model allowance and token allocation disagree"
        );
        Ok(())
    }
    /// The member Answer's own budget: the request budget bounded by this
    /// allowance. Fields a BudgetV3 requires to be positive stay at least 1;
    /// card and Focus requests carry the allowance, which refuses any slot,
    /// ordinal or token it does not grant.
    pub fn clamp(&self, request: &BudgetV3) -> Result<BudgetV3> {
        self.validate()?;
        request.validate()?;
        let resources = &self.resources;
        let mut budget = request.clone();
        budget.calls = budget.calls.min(resources.calls);
        budget.candidates = budget.candidates.min(resources.candidates);
        budget.max_bytes = budget.max_bytes.min(resources.bytes);
        budget.input_tokens = budget.input_tokens.min(resources.input_tokens).max(1);
        budget.round_input_tokens = budget.round_input_tokens.min(budget.input_tokens);
        budget.context_calls = budget.context_calls.min(self.card_slots()).max(1);
        budget.focus_pairs = budget.focus_pairs.min(resources.focus_pairs).max(1);
        budget.focus_calls = budget
            .focus_calls
            .min(self.focus_calls())
            .max(1)
            .min(budget.focus_pairs);
        budget.validate()?;
        Ok(budget)
    }
    pub fn validate_binding(
        &self,
        attempt: &ReadAttemptBudget,
        selection: Sha256Digest,
        index: crate::index_catalog::IndexId,
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

/// A card as the model is shown it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopCard {
    pub symbol: String,
    pub class: CardClass,
    pub text: String,
    /// The ordered partition a Split card proposes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition: Vec<QuestionSpan>,
}
impl LoopCard {
    /// Covers the text and any Split partition, so a swapped range changes it.
    pub fn text_digest(&self) -> Result<Sha256Digest> {
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.loop-card.text/2",
            &[self.text.as_bytes(), &serde_json::to_vec(&self.partition)?],
        ))
    }
    /// The card as the decision log records it.
    pub fn record(&self) -> Result<CardRecord> {
        Ok(CardRecord {
            symbol: self.symbol.clone(),
            class: self.class,
            text_digest: self.text_digest()?,
            partition: self.partition.clone(),
        })
    }
}

/// Tokens of a card call's reservation kept for the model view around its
/// evidence: the prompt frame, the question, subjects, locations and cards.
/// A two-sub-need Coverage call with eight cards and eight located spans was
/// measured at about half of this.
pub const CARD_FRAME_TOKENS: usize = 2048;

/// The excerpt bytes one card call may present under its token reservation:
/// one byte for each reserved token above the frame. At the 8,192-token
/// reservation that is 6,144 bytes, 768 for each of eight spans. The Context
/// tokenizers are byte-level, so text needs at most one token a byte before
/// the model view quotes it; a call whose compiled input still passes its
/// reservation is refused by the native lane before any model work.
pub fn evidence_allowance(reserved_input_tokens: usize) -> usize {
    reserved_input_tokens.saturating_sub(CARD_FRAME_TOKENS)
}

/// One span gathered for a sub-need, as a Coverage roster shows it: its
/// location, its Focus relevance when Focus scored it, and the start of its
/// text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardEvidence {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    /// Absent when Focus did not score the span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<f32>,
    /// The first bytes of the span's text, cut on a character boundary.
    pub excerpt: String,
    /// Bytes of the span's text after the excerpt, which are not shown.
    pub cut_bytes: usize,
}
impl CardEvidence {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.path.is_empty()
                && self.path.len() <= 4096
                && self.start_line > 0
                && self.end_line >= self.start_line
                && !self.excerpt.is_empty()
                && self.cut_bytes <= crate::arbiter::answer_loop::MAX_RETENTION_BYTES
                && self.focus.is_none_or(|focus| (0.0..=1.0).contains(&focus)),
            "invalid card segment evidence"
        );
        Ok(())
    }
}

/// One roster of a batched card call, answering one decision.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSegment {
    /// The decision ordinal this segment answers.
    pub ordinal: u8,
    /// The card call's slot, the same for every segment of the call.
    pub card_slot: u8,
    pub stage: LoopStage,
    /// 0 for Split, which decides for the whole question.
    pub part: u8,
    /// 0 for Split and Plan.
    pub need: u8,
    /// 0 for Split and Plan.
    pub round: u8,
    /// The text this roster is about: the question, a part or a sub-need.
    pub subject: String,
    pub cards: Vec<LoopCard>,
    /// Several cards may be chosen (Plan only).
    pub multiple: bool,
    /// Excerpts of the spans gathered for the sub-need, in Focus rank order
    /// (Coverage only). Empty when the sub-need has no span, or when the
    /// call's token reservation leaves no room for one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<CardEvidence>,
}
impl CardSegment {
    /// The same roster digest the decision log recomputes from its records.
    pub fn roster_digest(&self) -> Result<Sha256Digest> {
        let cards = self
            .cards
            .iter()
            .map(LoopCard::record)
            .collect::<Result<Vec<_>>>()?;
        roster_digest(self.stage, &cards)
    }
    fn validate(&self, task: Task) -> Result<()> {
        ensure!(
            card_task(self.stage) == Some(task) && self.ordinal >= 1,
            "card segment stage differs from its call's task"
        );
        ensure!(
            match self.stage {
                LoopStage::Split => self.part == 0 && self.need == 0 && self.round == 0,
                LoopStage::Plan => self.part >= 1 && self.need == 0 && self.round == 0,
                _ => self.part >= 1 && self.need >= 1 && self.round >= 1,
            } && self.multiple == (self.stage == LoopStage::Plan),
            "card segment location differs from its stage"
        );
        ensure!(
            !self.subject.trim().is_empty() && self.subject.len() <= MAX_QUESTION_BYTES,
            "invalid card segment subject"
        );
        ensure!(
            (1..=MAX_ACTION_CARDS).contains(&self.cards.len())
                && unique(self.cards.iter().map(|card| &card.symbol))
                && unique(self.cards.iter().map(|card| &card.text))
                && self
                    .cards
                    .iter()
                    .all(|card| card.class.stage() == self.stage
                        && !card.symbol.is_empty()
                        && card.symbol.len() <= MAX_SYMBOL_BYTES
                        && !card.text.trim().is_empty()
                        && card.text.len() <= MAX_TEXT_BYTES),
            "invalid card segment roster"
        );
        for card in &self.cards {
            match card.class {
                CardClass::SplitAt | CardClass::KeepWhole => {
                    validate_partition(&self.subject, &card.partition)?;
                    ensure!(
                        (card.class == CardClass::KeepWhole) == (card.partition.len() == 1),
                        "a Split card is one whole partition"
                    );
                }
                _ => ensure!(
                    card.partition.is_empty(),
                    "only Split cards carry a partition"
                ),
            }
        }
        // Evidence is in Focus rank order: scored spans by falling relevance,
        // then the spans Focus did not score.
        ensure!(
            (self.stage == LoopStage::Coverage || self.evidence.is_empty())
                && self.evidence.len() <= MAX_ACTION_CARDS
                && self.evidence.windows(2).all(|pair| {
                    match (pair[0].focus, pair[1].focus) {
                        (Some(higher), Some(lower)) => higher >= lower,
                        (None, Some(_)) => false,
                        _ => true,
                    }
                }),
            "only Coverage segments carry evidence, at most eight spans in Focus rank order"
        );
        for evidence in &self.evidence {
            evidence.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCardRequest {
    pub schema: String,
    pub read_attempt: ReadAttemptBudget,
    pub parent_budget: BudgetV3,
    pub worker: WorkerIdentity,
    pub authority: ScopedReadRef,
    pub scope: ScopeBinding,
    pub task: Task,
    pub task_policy: Sha256Digest,
    pub segments: Vec<CardSegment>,
    /// Reserved tokens per part and round, summing to the reservation.
    pub attribution: Vec<TokenAttribution>,
    pub partition: Option<PartitionAllowanceV3>,
    pub remaining_ms: u64,
    pub remaining_input_tokens: usize,
}
impl NativeCardRequest {
    pub fn input_digest(&self) -> Result<Sha256Digest> {
        let mut semantic = self.clone();
        semantic.remaining_ms = 0;
        semantic.read_attempt.remaining_ns = 0;
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.loop-card.input/1",
            &[&serde_json::to_vec(&semantic)?],
        ))
    }
    /// Structural only: actual worker, grant, one-use slot ledger, live
    /// publication and original attempt are checked natively.
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
            self.schema == CARD_REQUEST_SCHEMA
                && self.worker == worker
                && self.worker.fencing_generation > 0
                && self.authority == authority
                && self.scope.index_id == authority.index_id
                && self.scope.authorization == authority.fingerprint
                && self.scope.configuration_revision > 0,
            "foreign native card binding"
        );
        ensure!(
            (1..=MAX_CARD_SEGMENTS).contains(&self.segments.len())
                && unique(self.segments.iter().map(|segment| segment.ordinal))
                && self.segments.iter().all(|segment| {
                    usize::from(segment.ordinal) <= MAX_DECISIONS
                        && segment.card_slot == self.segments[0].card_slot
                        && (1..=self.parent_budget.context_calls)
                            .contains(&usize::from(segment.card_slot))
                }),
            "invalid native card segments"
        );
        for segment in &self.segments {
            segment.validate(self.task)?;
        }
        let reserved = validate_attribution(&self.attribution)?;
        ensure!(
            reserved == self.remaining_input_tokens
                && (1..=crate::arbiter::answer_loop::CONTEXT_CALL_TOKENS)
                    .contains(&self.remaining_input_tokens)
                && self.remaining_input_tokens <= self.parent_budget.input_tokens
                && self.attribution.iter().all(|charge| charge.round == 0
                    || charge.tokens <= self.parent_budget.round_input_tokens)
                && self
                    .attribution
                    .iter()
                    .all(|charge| self.segments.iter().any(
                        |segment| segment.part == charge.part && segment.round == charge.round
                    ))
                && self.segments.iter().all(|segment| self
                    .attribution
                    .iter()
                    .any(|charge| segment.part == charge.part && segment.round == charge.round)),
            "native card token attribution differs from its segments or reservation"
        );
        let excerpt_bytes: usize = self
            .segments
            .iter()
            .flat_map(|segment| &segment.evidence)
            .map(|evidence| evidence.excerpt.len())
            .sum();
        ensure!(
            excerpt_bytes <= evidence_allowance(self.remaining_input_tokens),
            "native card evidence exceeds the excerpt allowance of its token reservation"
        );
        ensure!(
            (1..=MAX_DEADLINE_MS).contains(&self.remaining_ms)
                && self.remaining_ms <= self.parent_budget.deadline_ms
                && serde_json::to_vec(self)?.len() <= MAX_TOOLS_CALL_REQUEST_BYTES,
            "native card original bounds exceeded"
        );
        if let Some(partition) = &self.partition {
            partition.validate_binding(
                &self.read_attempt,
                partition.selection,
                self.scope.index_id,
            )?;
            ensure!(
                self.segments
                    .iter()
                    .all(|segment| partition.card_ordinals.contains(&segment.card_slot))
                    && self.remaining_input_tokens <= partition.resources.input_tokens,
                "native card call exceeds captured partition allowance"
            );
        }
        let remaining = self
            .read_attempt
            .remaining_at(clock, now_ns)?
            .min(Duration::from_millis(self.remaining_ms));
        ensure!(!remaining.is_zero(), "native card original attempt expired");
        Ok(remaining)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCardResponse {
    pub advice: CardAdvice,
}
impl NativeCardResponse {
    pub fn validate(&self, request: &NativeCardRequest) -> Result<()> {
        self.advice.validate()?;
        let ordinals: Vec<_> = request
            .segments
            .iter()
            .map(|segment| segment.ordinal)
            .collect();
        ensure!(
            self.advice.scope == request.scope
                && self.advice.task == request.task
                && self.advice.task_policy == request.task_policy
                && self.advice.input_digest == request.input_digest()?
                && self.advice.reserved_input_tokens == request.remaining_input_tokens
                && request
                    .segments
                    .first()
                    .is_some_and(|segment| segment.card_slot == self.advice.card_slot)
                && self
                    .advice
                    .segments
                    .iter()
                    .map(|segment| segment.ordinal)
                    .collect::<Vec<_>>()
                    == ordinals,
            "card advice differs from actual complete native request"
        );
        for (segment, outcome) in request.segments.iter().zip(&self.advice.segments) {
            ensure!(
                outcome.raw_scores.is_empty() || outcome.raw_scores.len() == segment.cards.len(),
                "card advice scores differ from the presented roster"
            );
            if let crate::arbiter::Outcome::Accepted { decision } = &outcome.outcome {
                let digests = segment
                    .cards
                    .iter()
                    .map(LoopCard::text_digest)
                    .collect::<Result<Vec<_>>>()?;
                let chosen = match decision {
                    crate::arbiter::Decision::LoopCard(card) => vec![*card],
                    crate::arbiter::Decision::LoopCards(cards) => cards.clone(),
                    _ => anyhow::bail!("card advice decision is not a loop card"),
                };
                ensure!(
                    (segment.multiple || chosen.len() == 1)
                        && chosen.iter().all(|card| digests.contains(card)),
                    "card advice chose a card outside its frozen roster"
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFocusCandidateV3 {
    pub id: Sha256Digest,
    /// Index into the request's questions.
    pub question: u8,
    pub hit: Hit,
    pub window: FocusSourceWindow,
}
impl NativeFocusCandidateV3 {
    pub fn new(
        scope: &ScopeBinding,
        questions: &[String],
        question: u8,
        ordinal: usize,
        hit: Hit,
        window: FocusSourceWindow,
    ) -> Result<Self> {
        window.text(&hit)?;
        let text = questions
            .get(usize::from(question))
            .ok_or_else(|| anyhow::anyhow!("Focus candidate names an unknown question"))?;
        let id = Self::identity(scope, text, ordinal, &hit, window)?;
        Ok(Self {
            id,
            question,
            hit,
            window,
        })
    }
    /// The identity hashes the candidate's own question text.
    fn identity(
        scope: &ScopeBinding,
        question: &str,
        ordinal: usize,
        hit: &Hit,
        window: FocusSourceWindow,
    ) -> Result<Sha256Digest> {
        let ordinal = u64::try_from(ordinal)?;
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.candidate/2",
            &[
                &serde_json::to_vec(scope)?,
                question.as_bytes(),
                &ordinal.to_be_bytes(),
                &serde_json::to_vec(hit)?,
                &serde_json::to_vec(&window)?,
            ],
        ))
    }
    fn validate(&self, scope: &ScopeBinding, questions: &[String], ordinal: usize) -> Result<()> {
        self.window.text(&self.hit)?;
        ensure!(
            !self.hit.citation.path.is_empty()
                && self.hit.citation.path.len() <= 4096
                && self.hit.citation.start_line > 0
                && self.hit.citation.end_line >= self.hit.citation.start_line,
            "invalid Focus candidate source citation"
        );
        let _: Sha256Digest = self.hit.citation.content_hash.parse()?;
        let text = questions
            .get(usize::from(self.question))
            .ok_or_else(|| anyhow::anyhow!("Focus candidate names an unknown question"))?;
        ensure!(
            self.id == Self::identity(scope, text, ordinal, &self.hit, self.window)?,
            "Focus candidate question/body/order/window identity changed"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFocusRequestV3 {
    pub read_attempt: ReadAttemptBudget,
    pub parent_budget: BudgetV3,
    pub worker: WorkerIdentity,
    pub authority: ScopedReadRef,
    pub scope: ScopeBinding,
    /// Distinct sub-need questions, at most 16.
    pub questions: Vec<String>,
    pub candidates: Vec<NativeFocusCandidateV3>,
    pub order_digest: Sha256Digest,
    pub score_count: usize,
    /// The Focus call ordinal over the whole Answer.
    pub ordinal: u8,
    pub attribution: Vec<TokenAttribution>,
    pub partition: Option<PartitionAllowanceV3>,
    pub remaining_ms: u64,
    pub remaining_input_tokens: usize,
}
impl NativeFocusRequestV3 {
    pub fn candidate_order(&self) -> Vec<Sha256Digest> {
        self.candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect()
    }
    pub fn candidate_order_digest(candidates: &[NativeFocusCandidateV3]) -> Result<Sha256Digest> {
        let order = candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.order/2",
            &[&serde_json::to_vec(&order)?],
        ))
    }
    pub fn input_digest(&self) -> Result<Sha256Digest> {
        let mut semantic = self.clone();
        semantic.remaining_ms = 0;
        semantic.read_attempt.remaining_ns = 0;
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.focus.input/3",
            &[&serde_json::to_vec(&semantic)?],
        ))
    }
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
        ensure!(
            (1..=MAX_FOCUS_QUESTIONS).contains(&self.questions.len())
                && unique(&self.questions)
                && self
                    .questions
                    .iter()
                    .all(|question| !question.trim().is_empty()
                        && question.len() <= MAX_QUESTION_BYTES),
            "invalid native Focus questions"
        );
        ensure!(
            self.candidates.len() <= MAX_RETURNED_CANDIDATES
                && self.score_count <= self.candidates.len()
                && self.score_count <= MAX_FOCUS_BATCH_PAIRS
                && self.score_count <= self.parent_budget.focus_pairs
                && (self.candidates.is_empty() == (self.score_count == 0))
                && self.ordinal >= 1
                && usize::from(self.ordinal) <= self.parent_budget.focus_calls
                && (1..=MAX_DEADLINE_MS).contains(&self.remaining_ms)
                && self.remaining_ms <= self.parent_budget.deadline_ms
                && (1..=self.parent_budget.round_input_tokens)
                    .contains(&self.remaining_input_tokens)
                && serde_json::to_vec(self)?.len() <= MAX_TOOLS_CALL_REQUEST_BYTES,
            "native Focus original bounds exceeded"
        );
        let reserved = validate_attribution(&self.attribution)?;
        ensure!(
            reserved == self.remaining_input_tokens
                && self
                    .attribution
                    .iter()
                    .all(|charge| charge.part >= 1 && charge.round >= 1),
            "native Focus token attribution differs from its reservation"
        );
        for (ordinal, candidate) in self.candidates.iter().enumerate() {
            candidate.validate(&self.scope, &self.questions, ordinal)?;
        }
        ensure!(
            unique(self.candidate_order())
                && self.order_digest == Self::candidate_order_digest(&self.candidates)?,
            "duplicate or foreign Focus candidate order"
        );
        if let Some(partition) = &self.partition {
            partition.validate_binding(
                &self.read_attempt,
                partition.selection,
                self.scope.index_id,
            )?;
            ensure!(
                self.remaining_input_tokens <= partition.resources.input_tokens
                    && self.score_count <= partition.resources.candidates
                    && self.score_count <= partition.resources.focus_pairs
                    && partition.focus_ordinals.contains(&self.ordinal),
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
pub struct NativeFocusResponseV3 {
    pub ranking: FocusBatchReceipt,
}
impl NativeFocusResponseV3 {
    pub fn validate(&self, request: &NativeFocusRequestV3) -> Result<()> {
        let mut remaining = request.parent_budget.clone();
        remaining.round_input_tokens = request.remaining_input_tokens;
        remaining.deadline_ms = request.remaining_ms;
        self.ranking.validate(&request.scope, &remaining)?;
        ensure!(
            self.ranking.scope_digest == scope_digest(&request.scope)?
                && self.ranking.input_digest == request.input_digest()?
                && self.ranking.order_digest == request.order_digest
                && self.ranking.questions_digest == questions_digest(&request.questions)?
                && usize::from(self.ranking.question_count) == request.questions.len()
                && self.ranking.candidate_count == request.candidates.len()
                && self.ranking.pairs.len() <= request.score_count
                && self.ranking.reserved_input_tokens == request.remaining_input_tokens
                && (!self.ranking.applied || self.ranking.pairs.len() == request.score_count),
            "Focus batch differs from actual complete native request"
        );
        for pair in &self.ranking.pairs {
            ensure!(
                usize::from(pair.candidate) < request.score_count
                    && request.candidates[usize::from(pair.candidate)].question == pair.question,
                "Focus batch scored a pair the request did not present"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
