// SPDX-License-Identifier: Apache-2.0
//! Loop card decisions in their compact reply form, and card advice
//! receipts. Deterministic code proposes the cards; a scoring model may
//! choose. Full rosters and scores live in the decision log (service custody).
use super::*;
pub use crate::arbiter::context_action::{
    AssessmentApplicability, AssessmentUnavailable, MAX_ACTION_CARDS,
};
use crate::arbiter::device::DevicePrecision;

pub const CARD_REQUEST_SCHEMA: &str = "airk.arbiter.advice.request/3";
pub const CARD_ADVICE_SCHEMA: &str = "airk.arbiter.advice/3";
/// One batched call holds at most one roster per sub-need of every part.
pub const MAX_CARD_SEGMENTS: usize = MAX_PARTS * MAX_SUB_NEEDS;
pub const MAX_RULES: u8 = 32;
pub const MAX_SYMBOL_BYTES: usize = 16;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionDevice {
    Cpu,
    Cuda,
    DirectMl,
    CoreMl,
}

/// What a card proposes. Each class belongs to exactly one stage.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CardClass {
    /// A whole partition of the request into two to four ordered ranges.
    SplitAt,
    /// The partition of the request into one range.
    KeepWhole,
    /// Include a proposed sub-need.
    PlanNeed,
    Covered,
    ReadMoreHere,
    SearchForTerm,
    FollowUpQuery,
    FollowUpHop,
}
impl CardClass {
    pub fn stage(self) -> LoopStage {
        match self {
            Self::SplitAt | Self::KeepWhole => LoopStage::Split,
            Self::PlanNeed => LoopStage::Plan,
            Self::Covered | Self::ReadMoreHere | Self::SearchForTerm => LoopStage::Coverage,
            Self::FollowUpQuery | Self::FollowUpHop => LoopStage::FollowUp,
        }
    }
}

/// The task policy that owns a card stage's roster.
pub fn card_task(stage: LoopStage) -> Option<Task> {
    match stage {
        LoopStage::Split => Some(Task::ContextSplit),
        LoopStage::Plan => Some(Task::ContextPlan),
        LoopStage::Coverage => Some(Task::ContextCoverage),
        LoopStage::FollowUp => Some(Task::ContextFollowUp),
        _ => None,
    }
}
pub fn is_card_task(task: Task) -> bool {
    matches!(
        task,
        Task::ContextSplit | Task::ContextPlan | Task::ContextCoverage | Task::ContextFollowUp
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelMode {
    /// An accepted calibration rule admitted the top card.
    Rule,
    /// The top score, with no calibration rule in force.
    TopScore,
}

/// The deterministic order of section 12.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeterministicRule {
    /// Split only on explicit enumerations and separate questions.
    ExplicitBreaks,
    EveryProposedNeed,
    TopFocusSpan,
    FirstExtractedTerm,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Model,
    Abstained,
    Unavailable,
    NotDispatched,
    Deterministic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionSource {
    Model {
        mode: ModelMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<u8>,
    },
    Abstained {
        fallback: DeterministicRule,
        reason: AssessmentUnavailable,
    },
    Unavailable {
        fallback: DeterministicRule,
        reason: AssessmentUnavailable,
    },
    /// A Context model was present, but no card call was dispatched for this
    /// roster: the call had no slot, token share, excerpt room or time left,
    /// or the model was not usable. The deterministic order decided. The
    /// reason is `token_budget`, `deadline` or `unavailable`.
    NotDispatched {
        fallback: DeterministicRule,
        reason: AssessmentUnavailable,
    },
    /// No Context model takes part, or the roster holds one card; the
    /// deterministic order decided.
    Deterministic { rule: DeterministicRule },
}
impl DecisionSource {
    pub fn kind(&self) -> SourceKind {
        match self {
            Self::Model { .. } => SourceKind::Model,
            Self::Abstained { .. } => SourceKind::Abstained,
            Self::Unavailable { .. } => SourceKind::Unavailable,
            Self::NotDispatched { .. } => SourceKind::NotDispatched,
            Self::Deterministic { .. } => SourceKind::Deterministic,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChosenCard {
    pub symbol: String,
    pub class: CardClass,
}

/// The source units the decision that stands rests on; the full EvidenceRef
/// list is in the log. When a model's choice stands, `presented` counts the
/// byte ranges the model was shown and `omitted` the ranges of the same
/// spans it was not shown. When the deterministic order decided (no model,
/// a call not dispatched, refused or lost, or an abstention), `presented`
/// counts the whole spans that order read and `omitted` is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BasisSummary {
    pub presented: u16,
    pub omitted: u16,
    pub digest: Sha256Digest,
}

/// The compact reply form of one card decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    /// Position in the whole decision log, from 1.
    pub ordinal: u8,
    pub stage: LoopStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u8>,
    /// Digest of the presented roster (stage and every card record).
    pub roster_digest: Sha256Digest,
    /// The card call this decision was scored in; batched segments share it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_slot: Option<u8>,
    pub chosen: Vec<ChosenCard>,
    pub source: DecisionSource,
    /// Top and runner-up raw scores and their margin, in native units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_up: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<SelectedModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<ExecutionDevice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<DevicePrecision>,
    /// Ordinals of the other decisions scored in the same card call.
    pub batched_with: Vec<u8>,
    pub applicability: AssessmentApplicability,
    /// The card advice receipt's input digest, held in the decision log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Sha256Digest>,
    pub basis: BasisSummary,
    /// A submitted call whose reply was lost: its work stays charged, unknown.
    pub work_unknown: bool,
}
impl DecisionRecord {
    /// A card call was dispatched for this decision: it holds a card slot.
    pub fn scored_by_model(&self) -> bool {
        !matches!(
            self.source.kind(),
            SourceKind::Deterministic | SourceKind::NotDispatched
        )
    }
    /// Structural rules that need no other part of the reply.
    pub fn validate(&self, context_calls: usize) -> Result<()> {
        ensure!(
            card_task(self.stage).is_some() && self.ordinal >= 1,
            "Answer decision stage makes no card decision"
        );
        ensure!(
            match self.stage {
                LoopStage::Split =>
                    self.part.is_none() && self.need.is_none() && self.round.is_none(),
                LoopStage::Plan =>
                    self.part.is_some() && self.need.is_none() && self.round.is_none(),
                _ => self.part.is_some() && self.need.is_some() && self.round.is_some(),
            },
            "Answer decision location differs from its stage"
        );
        ensure!(
            !self.chosen.is_empty()
                && unique(self.chosen.iter().map(|card| &card.symbol))
                && self
                    .chosen
                    .iter()
                    .all(|card| card.class.stage() == self.stage
                        && !card.symbol.is_empty()
                        && card.symbol.len() <= MAX_SYMBOL_BYTES)
                && if self.stage == LoopStage::Plan {
                    self.chosen.len() <= MAX_SUB_NEEDS
                } else {
                    self.chosen.len() == 1
                },
            "invalid Answer chosen cards"
        );
        ensure!(
            [self.top, self.runner_up, self.margin]
                .iter()
                .all(|score| score.is_none_or(f32::is_finite))
                && usize::from(self.basis.presented) <= MAX_ACTION_CARDS,
            "invalid Answer decision scores or basis"
        );
        ensure!(
            unique(&self.batched_with)
                && !self.batched_with.contains(&self.ordinal)
                && self.batched_with.len() < MAX_CARD_SEGMENTS
                && (self.batched_with.is_empty() || self.card_slot.is_some()),
            "invalid Answer batch peers"
        );
        ensure!(
            self.precision.is_none()
                || self
                    .device
                    .is_some_and(|device| device != ExecutionDevice::Cpu),
            "Answer precision is stated for GPU decisions only"
        );
        let margin_ok = match (self.top, self.runner_up, self.margin) {
            (Some(top), Some(runner_up), Some(margin)) => {
                top >= runner_up && margin.to_bits() == (top - runner_up).to_bits()
            }
            (_, _, None) => true,
            _ => false,
        };
        ensure!(
            margin_ok,
            "Answer decision margin differs from its top and runner-up"
        );
        if !self.scored_by_model() {
            ensure!(
                self.card_slot.is_none()
                    && self.device.is_none()
                    && self.top.is_none()
                    && self.runner_up.is_none()
                    && self.margin.is_none()
                    && self.batched_with.is_empty()
                    && self.receipt.is_none()
                    && !self.work_unknown,
                "deterministic Answer decision carries model facts"
            );
            // A call that was not dispatched names the model that was
            // present and why it did not run; the no-model path names none.
            match self.source {
                DecisionSource::NotDispatched { reason, .. } => ensure!(
                    matches!(
                        self.model,
                        Some(SelectedModel::ContextCompact | SelectedModel::ContextLarge)
                    ) && matches!(
                        reason,
                        AssessmentUnavailable::TokenBudget
                            | AssessmentUnavailable::Deadline
                            | AssessmentUnavailable::Unavailable
                    ),
                    "an Answer decision not dispatched lacks its model or reason"
                ),
                _ => ensure!(
                    self.model.is_none(),
                    "deterministic Answer decision carries model facts"
                ),
            }
            return Ok(());
        }
        ensure!(
            self.card_slot
                .is_some_and(|slot| (1..=context_calls).contains(&usize::from(slot))),
            "Answer model decision lacks a card slot within its budget"
        );
        let model = self
            .model
            .ok_or_else(|| anyhow::anyhow!("model-scored Answer decision lacks its model"))?;
        ensure!(
            matches!(
                model,
                SelectedModel::ContextCompact | SelectedModel::ContextLarge
            ),
            "only a Context model scores loop cards"
        );
        ensure!(
            !(model == SelectedModel::ContextLarge && self.device == Some(ExecutionDevice::Cpu)),
            "Large never runs on the CPU"
        );
        ensure!(
            self.device
                .is_none_or(|device| (device == ExecutionDevice::Cpu) == self.precision.is_none()),
            "Answer GPU decision precision must be stated"
        );
        match self.source {
            DecisionSource::Model { mode, rule } => ensure!(
                self.device.is_some()
                    && self.receipt.is_some()
                    && self.margin.is_some()
                    && !self.work_unknown
                    && match mode {
                        ModelMode::Rule => rule.is_some_and(|rule| rule < MAX_RULES),
                        ModelMode::TopScore => rule.is_none(),
                    },
                "accepted Answer decision lacks its device, receipt, margin or rule"
            ),
            DecisionSource::Abstained { reason, .. } => ensure!(
                self.device.is_some()
                    && self.receipt.is_some()
                    && !self.work_unknown
                    && reason == AssessmentUnavailable::Abstained,
                "abstained Answer decision lacks its device, receipt or reason"
            ),
            DecisionSource::Unavailable { reason, .. } => ensure!(
                reason != AssessmentUnavailable::Abstained
                    && (self.receipt.is_some() || self.work_unknown),
                "unavailable Answer decision names an abstention or loses its receipt"
            ),
            DecisionSource::Deterministic { .. } | DecisionSource::NotDispatched { .. } => {
                unreachable!()
            }
        }
        Ok(())
    }
}

/// Model tokens charged to one part and round. Part 0 is the whole question
/// (Split); round 0 is before the rounds (Split and Plan).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct TokenAttribution {
    pub part: u8,
    pub round: u8,
    pub tokens: usize,
}
/// At most this many distinct (part, round) charges exist in one Answer.
pub const MAX_ATTRIBUTIONS: usize = 1 + MAX_PARTS * (MAX_ROUNDS + 1);
pub(crate) fn validate_charges(charges: &[TokenAttribution], limit: usize) -> Result<usize> {
    ensure!(
        charges.len() <= limit
            && unique(charges.iter().map(|charge| (charge.part, charge.round)))
            && charges
                .iter()
                .all(|charge| usize::from(charge.part) <= MAX_PARTS
                    && usize::from(charge.round) <= MAX_ROUNDS
                    && (charge.part != 0 || charge.round == 0)),
        "invalid model token attribution"
    );
    charges.iter().try_fold(0usize, |total, charge| {
        total
            .checked_add(charge.tokens)
            .ok_or_else(|| anyhow::anyhow!("model token attribution overflow"))
    })
}
pub(crate) fn validate_attribution(charges: &[TokenAttribution]) -> Result<usize> {
    ensure!(!charges.is_empty(), "invalid model token attribution");
    validate_charges(charges, MAX_CARD_SEGMENTS)
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SegmentOutcome {
    /// The decision ordinal this roster segment answers.
    pub ordinal: u8,
    /// Raw scores in card order and native units.
    pub raw_scores: Vec<f32>,
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<f32>,
}

/// Same immutable native bindings as advice /2, for one batched card call.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardAdvice {
    pub schema: String,
    pub task: Task,
    pub profile: Sha256Digest,
    pub runtime: String,
    pub compiler: Sha256Digest,
    pub calibration: Sha256Digest,
    pub policy: Sha256Digest,
    pub task_policy: Sha256Digest,
    pub scope: ScopeBinding,
    pub input_digest: Sha256Digest,
    /// The card call's slot, shared by all of its segments.
    pub card_slot: u8,
    pub reserved_input_tokens: usize,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_tokens")]
    pub observed_input_tokens: Option<usize>,
    pub attribution: Vec<TokenAttribution>,
    pub work: Work,
    pub segments: Vec<SegmentOutcome>,
}
fn nullable_tokens(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<usize>>()
}
impl CardAdvice {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == CARD_ADVICE_SCHEMA
                && is_card_task(self.task)
                && device::valid_advice_runtime(&self.runtime)
                && self.scope.configuration_revision > 0
                && (1..=MAX_CONTEXT_CALLS).contains(&usize::from(self.card_slot))
                && (1..=CONTEXT_CALL_TOKENS).contains(&self.reserved_input_tokens),
            "invalid card advice binding"
        );
        ensure!(
            self.work.input_tokens == self.observed_input_tokens.unwrap_or(0)
                && self.work.calls == 0
                && self.work.rounds == 0
                && self.work.fanout == 0
                && self.work.candidates == 0
                && self.work.bytes == 0
                && self
                    .observed_input_tokens
                    .is_none_or(|tokens| tokens <= self.reserved_input_tokens),
            "card advice work contains reads or exceeds its reservation"
        );
        let attributed = validate_attribution(&self.attribution)?;
        ensure!(
            match self.observed_input_tokens {
                Some(observed) => attributed == observed,
                None => attributed <= self.reserved_input_tokens,
            },
            "card advice attribution differs from its observed tokens"
        );
        ensure!(
            (1..=MAX_CARD_SEGMENTS).contains(&self.segments.len())
                && unique(self.segments.iter().map(|segment| segment.ordinal))
                && self.segments.iter().all(|segment| {
                    segment.ordinal >= 1 && usize::from(segment.ordinal) <= MAX_DECISIONS
                }),
            "invalid card advice segments"
        );
        for segment in &self.segments {
            ensure!(
                segment.raw_scores.len() <= MAX_ACTION_CARDS
                    && segment.raw_scores.iter().all(|score| score.is_finite()),
                "invalid card advice scores"
            );
            match &segment.outcome {
                Outcome::Accepted { decision } => {
                    let cards = match decision {
                        Decision::LoopCard(_) => 1,
                        Decision::LoopCards(cards) => {
                            ensure!(unique(cards), "duplicate accepted loop cards");
                            cards.len()
                        }
                        _ => anyhow::bail!("card advice decision is not a loop card"),
                    };
                    ensure!(
                        (1..=segment.raw_scores.len()).contains(&cards)
                            && self.observed_input_tokens.is_some_and(|tokens| tokens > 0)
                            && self.work.compile_us.is_some()
                            && self.work.inference_us.is_some()
                            && segment.rule.is_none_or(|rule| rule < MAX_RULES)
                            && segment
                                .margin
                                .is_some_and(|margin| margin.is_finite() && margin >= 0.0),
                        "accepted card advice lacks its scores, tokens or margin"
                    );
                }
                Outcome::Abstained { reason }
                | Outcome::Unsupported { reason }
                | Outcome::InvalidInput { reason }
                | Outcome::Unavailable { reason } => ensure!(
                    !reason.is_empty()
                        && reason.len() <= 1024
                        && segment.rule.is_none()
                        && segment.margin.is_none(),
                    "invalid card advice reason"
                ),
            }
        }
        Ok(())
    }
}
