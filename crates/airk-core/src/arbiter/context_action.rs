// SPDX-License-Identifier: Apache-2.0
//! Inert, local-partition joint action contracts. No execution or model authority.
use super::*;

pub const REQUEST_SCHEMA_V2: &str = "airk.arbiter.context.request/2";
pub const CONTEXT_SCHEMA_V2: &str = "airk.arbiter.context/2";
pub const ADVICE_SCHEMA_V2: &str = "airk.arbiter.advice/2";
pub const ACTION_ROSTER_SCHEMA: &str = "airk.arbiter.context-action-roster/1";
pub const MAX_ACTION_CARDS: usize = 8;
pub const MAX_ASSESSMENTS: usize = 3;
pub mod calibration;

/// Normal dependency edge for the native compiler's NEW task policy. Core
/// hashes its own structural/acceptance sources; no cross-crate source mounts.
pub fn semantic_policy_digest() -> Result<Sha256Digest> {
    fn source(bytes: &[u8]) -> Result<Sha256Digest> {
        let canonical = std::str::from_utf8(bytes)?.replace("\r\n", "\n");
        ensure!(!canonical.contains('\r'), "lone CR in joint Core semantics");
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.context-action-core-source-lf/1",
            &[canonical.as_bytes()],
        ))
    }
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.context-action-core-semantics/1",
        &[
            source(include_bytes!("context_action.rs"))?.as_bytes(),
            source(include_bytes!("context_action/calibration.rs"))?.as_bytes(),
        ],
    ))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextRequestV2 {
    pub schema: String,
    pub question: String,
    pub budget: Budget,
}
impl ContextRequestV2 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == REQUEST_SCHEMA_V2,
            "invalid joint context request schema"
        );
        ensure!(
            !self.question.trim().is_empty() && self.question.len() <= 16_384,
            "invalid joint context question"
        );
        self.budget.validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextAction {
    Read { choice: Sha256Digest },
    Sufficient,
    Clarify,
    NoJustifiedRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextReason {
    AdditionalEvidenceMayHelp,
    RetainedEvidenceCoversQuestion,
    MissingDefinition,
    MissingConstraint,
    MissingExample,
    MissingTest,
    MissingCounterevidence,
    ConflictingEvidence,
    AmbiguousQuestion,
    NoVerifiedUsefulRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelevanceHypothesis {
    Useful,
    Complementary,
    Redundant,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceUnitKind {
    Hit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub record: Sha256Digest,
    pub kind: SourceUnitKind,
    pub result_pointer: String,
    pub utf8_start: usize,
    pub utf8_end: usize,
}
impl EvidenceRef {
    /// Structural source-unit validation only. The canonical read owner must
    /// establish the record's actual authority/publication before presenting it.
    pub fn validate(&self, records: &[EvidenceRecord], scope: &ScopeBinding) -> Result<bool> {
        ensure!(
            self.result_pointer.starts_with('/') && self.result_pointer.len() <= 256,
            "invalid joint evidence pointer"
        );
        let record = records
            .iter()
            .find(|record| record.id == self.record)
            .ok_or_else(|| anyhow::anyhow!("joint evidence record missing"))?;
        ensure!(&record.scope == scope, "joint evidence scope mismatch");
        ensure!(
            matches!(
                record.operation,
                NativeRead::Search | NativeRead::Context | NativeRead::ReadEvidence
            ),
            "joint source unit is not from a supported hit-producing read"
        );
        let index = self
            .result_pointer
            .strip_prefix("/hits/")
            .and_then(|index| index.parse::<usize>().ok())
            .ok_or_else(|| anyhow::anyhow!("joint source pointer is not a canonical hit"))?;
        ensure!(
            self.result_pointer == format!("/hits/{index}"),
            "joint source pointer is not a canonical hit index"
        );
        let value = record
            .canonical_result
            .pointer(&self.result_pointer)
            .ok_or_else(|| anyhow::anyhow!("joint source unit missing"))?;
        let hit: crate::Hit = serde_json::from_value(value.clone())?;
        ensure!(
            !hit.citation.path.is_empty()
                && hit.citation.content_hash.parse::<Sha256Digest>().is_ok()
                && hit.citation.start_line > 0
                && hit.citation.end_line >= hit.citation.start_line,
            "invalid joint source citation"
        );
        ensure!(
            self.utf8_start < self.utf8_end
                && self.utf8_end <= hit.text.len()
                && hit.text.is_char_boundary(self.utf8_start)
                && hit.text.is_char_boundary(self.utf8_end),
            "invalid joint source window"
        );
        Ok(self.utf8_start != 0 || self.utf8_end != hit.text.len())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceAssessment {
    pub evidence: EvidenceRef,
    pub role: EvidenceRole,
    pub relevance: RelevanceHypothesis,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssessmentBasis {
    pub source_units_available: usize,
    pub source_units_presented: Vec<EvidenceRef>,
    pub source_units_omitted: usize,
    pub source_units_partial: usize,
    pub read_choices_available: usize,
    pub read_choices_scored: Vec<Sha256Digest>,
    pub read_choices_omitted: usize,
}
impl AssessmentBasis {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.source_units_presented.len() <= MAX_ACTION_CARDS
                && self.read_choices_scored.len() <= MAX_ACTION_CARDS
                && self
                    .source_units_presented
                    .len()
                    .checked_add(self.source_units_omitted)
                    == Some(self.source_units_available)
                && self
                    .read_choices_scored
                    .len()
                    .checked_add(self.read_choices_omitted)
                    == Some(self.read_choices_available)
                && self.source_units_partial <= self.source_units_presented.len(),
            "invalid joint assessment omission accounting"
        );
        for (index, unit) in self.source_units_presented.iter().enumerate() {
            ensure!(
                !self.source_units_presented[..index].contains(unit),
                "duplicate joint source unit"
            );
        }
        ensure!(
            self.read_choices_scored
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == self.read_choices_scored.len(),
            "duplicate joint read choice"
        );
        Ok(())
    }
    fn validate_units(&self, records: &[EvidenceRecord], scope: &ScopeBinding) -> Result<()> {
        self.validate()?;
        let mut partial = 0;
        for unit in &self.source_units_presented {
            partial += usize::from(unit.validate(records, scope)?);
        }
        ensure!(
            partial == self.source_units_partial,
            "incorrect joint source window accounting"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextActionCard {
    pub id: Sha256Digest,
    pub action: ContextAction,
    pub gap: Gap,
    pub reason: ContextReason,
    pub assessments: Vec<EvidenceAssessment>,
}
impl ContextActionCard {
    pub fn validate(&self, basis: &AssessmentBasis) -> Result<()> {
        basis.validate()?;
        ensure!(
            self.assessments.len() <= MAX_ACTION_CARDS,
            "joint assessment unit limit"
        );
        for (index, assessment) in self.assessments.iter().enumerate() {
            ensure!(
                basis.source_units_presented.contains(&assessment.evidence),
                "joint hypothesis was not presented"
            );
            ensure!(
                !self.assessments[..index]
                    .iter()
                    .any(|previous| previous.evidence == assessment.evidence),
                "duplicate joint unit hypothesis"
            );
        }
        match self.action {
            ContextAction::Read { choice } => {
                ensure!(
                    basis.read_choices_scored.contains(&choice),
                    "joint read choice was not scored"
                );
                ensure!(
                    matches!(
                        (self.gap, self.reason),
                        (Gap::Unsupported, ContextReason::AdditionalEvidenceMayHelp)
                            | (Gap::Definition, ContextReason::MissingDefinition)
                            | (Gap::Constraint, ContextReason::MissingConstraint)
                            | (Gap::Example, ContextReason::MissingExample)
                            | (Gap::Test, ContextReason::MissingTest)
                            | (Gap::Counterevidence, ContextReason::MissingCounterevidence)
                    ),
                    "inconsistent joint read need"
                );
            }
            ContextAction::Sufficient => ensure!(
                self.gap == Gap::Sufficient
                    && self.reason == ContextReason::RetainedEvidenceCoversQuestion
                    && !self.assessments.is_empty()
                    && self
                        .assessments
                        .iter()
                        .all(|unit| unit.role != EvidenceRole::Unknown
                            && matches!(
                                unit.relevance,
                                RelevanceHypothesis::Useful | RelevanceHypothesis::Complementary
                            )),
                "inconsistent joint sufficiency support"
            ),
            ContextAction::Clarify => ensure!(
                self.gap == Gap::Ambiguous
                    && matches!(
                        self.reason,
                        ContextReason::AmbiguousQuestion | ContextReason::ConflictingEvidence
                    )
                    && (self.reason != ContextReason::ConflictingEvidence
                        || self.assessments.len() >= 2),
                "inconsistent joint clarification"
            ),
            ContextAction::NoJustifiedRead => ensure!(
                self.gap != Gap::Sufficient
                    && self.reason == ContextReason::NoVerifiedUsefulRead
                    && basis.read_choices_omitted == 0,
                "joint terminal cannot dismiss unscored reads"
            ),
        }
        Ok(())
    }
}

/// Frozen native alternatives, not an arbitrary model-supplied operation list.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextActionRoster {
    pub schema: String,
    pub scope: ScopeBinding,
    pub input_digest: Sha256Digest,
    pub policy: Sha256Digest,
    pub basis: AssessmentBasis,
    pub cards: Vec<ContextActionCard>,
}
impl ContextActionRoster {
    pub fn validate(&self, records: &[EvidenceRecord]) -> Result<()> {
        ensure!(
            self.schema == ACTION_ROSTER_SCHEMA
                && self.scope.configuration_revision > 0
                && !self.cards.is_empty()
                && self.cards.len() <= MAX_ACTION_CARDS,
            "invalid joint action roster"
        );
        self.basis.validate_units(records, &self.scope)?;
        let mut ids = BTreeSet::new();
        let mut reads = BTreeSet::new();
        let mut terminals = BTreeSet::new();
        for card in &self.cards {
            ensure!(ids.insert(card.id), "duplicate joint action card");
            card.validate(&self.basis)?;
            if let ContextAction::Read { choice } = card.action {
                ensure!(reads.insert(choice), "duplicate joint read alternative");
            } else {
                let terminal = match card.action {
                    ContextAction::Sufficient => 0,
                    ContextAction::Clarify => 1,
                    ContextAction::NoJustifiedRead => 2,
                    ContextAction::Read { .. } => unreachable!(),
                };
                ensure!(
                    terminals.insert(terminal),
                    "duplicate joint terminal alternative"
                );
            }
        }
        ensure!(
            reads == self.basis.read_choices_scored.iter().copied().collect(),
            "joint scored-read roster mismatch"
        );
        Ok(())
    }
    pub fn selected(&self, id: Sha256Digest) -> Result<&ContextActionCard> {
        self.cards
            .iter()
            .find(|card| card.id == id)
            .ok_or_else(|| anyhow::anyhow!("joint decision not in frozen native roster"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentUnavailable {
    Disabled,
    NotEnrolled,
    TaskUnsupported,
    NoEvidence,
    Abstained,
    Unavailable,
    TokenBudget,
    Deadline,
    NewEvidence,
    BasisOmitted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentApplicability {
    Current,
    HistoricalAfterRead,
    HistoricalAfterConfigurationChange,
    BasisOmitted,
}

/// Same immutable native bindings as Advice, with a distinct joint-task schema.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextActionAdvice {
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
    pub evidence: Vec<Sha256Digest>,
    pub reserved_input_tokens: usize,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_tokens_schema")]
    pub observed_input_tokens: Option<usize>,
    pub work: Work,
    pub outcome: Outcome,
}
impl ContextActionAdvice {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == ADVICE_SCHEMA_V2
                && self.task == Task::ContextAction
                && self.runtime == RUNTIME
                && self.scope.configuration_revision > 0
                && (1..=2048).contains(&self.reserved_input_tokens)
                && self.work.input_tokens == self.observed_input_tokens.unwrap_or(0)
                && self.work.calls == 0
                && self.work.rounds == 0
                && self.work.fanout == 0
                && self.work.candidates == 0
                && self.work.bytes == 0
                && self.evidence.len() <= 64
                && self.evidence.iter().collect::<BTreeSet<_>>().len() == self.evidence.len(),
            "invalid joint advice binding"
        );
        match &self.outcome {
            Outcome::Accepted { decision } => ensure!(
                matches!(decision, Decision::ContextAction(_))
                    && self
                        .observed_input_tokens
                        .is_some_and(|tokens| tokens > 0 && tokens <= self.reserved_input_tokens)
                    && self.work.compile_us.is_some()
                    && self.work.inference_us.is_some(),
                "joint advice decision mismatch"
            ),
            Outcome::Abstained { reason }
            | Outcome::Unsupported { reason }
            | Outcome::InvalidInput { reason }
            | Outcome::Unavailable { reason } => ensure!(
                !reason.is_empty() && reason.len() <= 1024,
                "invalid joint advice reason"
            ),
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssessmentResult {
    Selected {
        card: ContextActionCard,
        advice: ContextActionAdvice,
    },
    Abstained {
        advice: ContextActionAdvice,
    },
    Unavailable {
        reason: AssessmentUnavailable,
        work: Work,
        work_unknown: bool,
        #[serde(deserialize_with = "required_nullable")]
        #[schemars(required, schema_with = "nullable_advice_schema")]
        advice: Option<ContextActionAdvice>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssessmentEvent {
    pub ordinal: u8,
    pub partition: ScopeBinding,
    pub basis: AssessmentBasis,
    pub result: AssessmentResult,
    pub applicability: AssessmentApplicability,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartitionCoverage {
    Unassessed { reason: AssessmentUnavailable },
    Assessed { ordinal: u8 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartitionTermination {
    Pending,
    Covered,
    ClarificationRequired,
    NoJustifiedRead,
    NoProgress,
    Budget,
    Deadline,
    Refused,
    Stale,
    Unsupported,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartitionAssessment {
    pub scope: ScopeBinding,
    pub coverage: PartitionCoverage,
    pub termination: PartitionTermination,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextResponseV2 {
    pub schema: String,
    pub packet_schema: String,
    pub question: String,
    pub records: Vec<EvidenceRecord>,
    pub paragraphs: Vec<GroundedParagraph>,
    pub gaps: Vec<Gap>,
    pub stop: PartitionTermination,
    pub work: Work,
    pub candidate_count: usize,
    pub omitted_count: usize,
    pub assessment_history: Vec<AssessmentEvent>,
    pub assessment: PartitionAssessment,
    pub applications: Vec<IndexApplication>,
}
impl ContextResponseV2 {
    /// Typed accounting view of this exact native schema inside the canonical
    /// read envelope. Callers retain the ORIGINAL complete value for delivery.
    pub fn from_canonical(value: &serde_json::Value, budget: &Budget) -> Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("joint packet is not an object"))?;
        let schema = schemars::schema_for!(ContextResponseV2);
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("joint native schema lacks properties"))?;
        let fields: serde_json::Map<String, serde_json::Value> = object
            .iter()
            .filter(|(key, _)| properties.contains_key(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let typed: Self = serde_json::from_value(serde_json::Value::Object(fields))?;
        typed.validate(budget)?;
        if let Some(publication) = object.get("publication") {
            let publication: crate::evidence::RevisionEnvelope =
                serde_json::from_value(publication.clone())?;
            ensure!(
                publication == typed.assessment.scope.publication,
                "joint outer publication mismatch"
            );
        }
        Ok(typed)
    }
    pub fn validate(&self, budget: &Budget) -> Result<()> {
        ContextRequestV2 {
            schema: REQUEST_SCHEMA_V2.into(),
            question: self.question.clone(),
            budget: budget.clone(),
        }
        .validate()?;
        ensure!(
            self.schema == CONTEXT_SCHEMA_V2
                && self.packet_schema == PACKET_SCHEMA
                && self.stop == self.assessment.termination
                && self.records.len().checked_add(self.omitted_count) == Some(self.candidate_count)
                && self.candidate_count <= budget.candidates,
            "invalid joint context response"
        );
        let scope = &self.assessment.scope;
        let ids: BTreeSet<_> = self.records.iter().map(|record| record.id).collect();
        ensure!(
            ids.len() == self.records.len()
                && self.records.iter().all(|record| &record.scope == scope),
            "joint response is not one exact partition"
        );
        for paragraph in &self.paragraphs {
            ensure!(
                !paragraph.text.is_empty()
                    && !paragraph.evidence.is_empty()
                    && paragraph.evidence.iter().all(|id| ids.contains(id)),
                "ungrounded joint paragraph"
            );
        }
        ensure!(
            self.assessment_history.len() <= MAX_ASSESSMENTS,
            "joint assessment history limit"
        );
        let mut previous = 0;
        let mut tokens = 0usize;
        let mut timings = [None; 4];
        let mut unknown = false;
        let mut task_policy = None;
        let failed_token_overrun = self.stop == PartitionTermination::Budget
            && matches!(self.assessment.coverage, PartitionCoverage::Unassessed { .. })
            && self.assessment_history.last().is_some_and(|event| matches!(&event.result,
                AssessmentResult::Unavailable { reason: AssessmentUnavailable::TokenBudget | AssessmentUnavailable::Unavailable,
                    advice: Some(advice), .. } if advice.observed_input_tokens.is_some()));
        for (position, event) in self.assessment_history.iter().enumerate() {
            ensure!(
                !unknown
                    && event.ordinal > previous
                    && usize::from(event.ordinal) <= MAX_ASSESSMENTS.min(budget.rounds)
                    && &event.partition == scope,
                "invalid joint assessment ordinal/scope"
            );
            previous = event.ordinal;
            ensure!(
                event.applicability != AssessmentApplicability::Current
                    || position + 1 == self.assessment_history.len(),
                "only latest local assessment can remain current"
            );
            event.basis.validate()?;
            if event.applicability != AssessmentApplicability::BasisOmitted {
                event.basis.validate_units(&self.records, scope)?;
            }
            let work = match &event.result {
                AssessmentResult::Selected { card, advice } => {
                    card.validate(&event.basis)?;
                    advice.validate()?;
                    pin_task_policy(&mut task_policy, advice.task_policy)?;
                    ensure!(
                        &advice.scope == scope
                            && matches!(advice.outcome, Outcome::Accepted {
                            decision: Decision::ContextAction(id) } if id == card.id),
                        "joint selected card/advice mismatch"
                    );
                    validate_advice_basis(advice, &event.basis)?;
                    ensure!(
                        advice.reserved_input_tokens <= budget.input_tokens.saturating_sub(tokens),
                        "joint reservation exceeds original remaining tokens"
                    );
                    unknown |= advice.observed_input_tokens.is_none();
                    &advice.work
                }
                AssessmentResult::Abstained { advice } => {
                    advice.validate()?;
                    pin_task_policy(&mut task_policy, advice.task_policy)?;
                    ensure!(
                        &advice.scope == scope
                            && matches!(advice.outcome, Outcome::Abstained { .. }),
                        "joint abstention/advice mismatch"
                    );
                    validate_advice_basis(advice, &event.basis)?;
                    ensure!(
                        advice.reserved_input_tokens <= budget.input_tokens.saturating_sub(tokens),
                        "joint reservation exceeds original remaining tokens"
                    );
                    unknown |= advice.observed_input_tokens.is_none();
                    &advice.work
                }
                AssessmentResult::Unavailable {
                    reason,
                    work,
                    work_unknown,
                    advice,
                } => {
                    if let Some(advice) = advice {
                        advice.validate()?;
                        pin_task_policy(&mut task_policy, advice.task_policy)?;
                        ensure!(
                            &advice.scope == scope
                                && &advice.work == work
                                && !matches!(
                                    advice.outcome,
                                    Outcome::Accepted { .. } | Outcome::Abstained { .. }
                                )
                                && advice.reserved_input_tokens
                                    <= budget.input_tokens.saturating_sub(tokens)
                                && (advice.observed_input_tokens.is_some() || *work_unknown),
                            "joint unavailable reply/binding/uncertainty mismatch"
                        );
                        validate_advice_basis(advice, &event.basis)?;
                    } else if !work_unknown {
                        ensure!(
                            matches!(
                                reason,
                                AssessmentUnavailable::Disabled
                                    | AssessmentUnavailable::NotEnrolled
                                    | AssessmentUnavailable::TaskUnsupported
                                    | AssessmentUnavailable::NoEvidence
                                    | AssessmentUnavailable::TokenBudget
                            ) && work == &Work::default(),
                            "lost submitted reply cannot become known zero work"
                        );
                    }
                    unknown |= *work_unknown;
                    work
                }
            };
            tokens = tokens
                .checked_add(work.input_tokens)
                .ok_or_else(|| anyhow::anyhow!("joint token accounting overflow"))?;
            // Scoring cannot perform canonical reads. Their additive calls/bytes
            // and peak rounds/fanout/candidates are accounted once by Kernel.
            ensure!(
                work.calls == 0
                    && work.rounds == 0
                    && work.fanout == 0
                    && work.candidates == 0
                    && work.bytes == 0
                    && (work.input_tokens <= budget.input_tokens || failed_token_overrun),
                "joint scorer work contains canonical work or exceeds token budget"
            );
            for (total, measured) in timings.iter_mut().zip([
                work.queue_us,
                work.compile_us,
                work.inference_us,
                work.policy_us,
            ]) {
                if let Some(measured) = measured {
                    *total = Some(
                        total
                            .unwrap_or(0u64)
                            .checked_add(measured)
                            .ok_or_else(|| anyhow::anyhow!("joint timing accounting overflow"))?,
                    );
                }
            }
            ensure!(
                tokens <= budget.input_tokens
                    || (failed_token_overrun && position + 1 == self.assessment_history.len()),
                "joint measured token violation cannot permit later assessment"
            );
        }
        ensure!(
            tokens <= self.work.input_tokens,
            "joint assessment tokens not retained"
        );
        let learned_terminal = matches!(
            self.stop,
            PartitionTermination::Covered
                | PartitionTermination::ClarificationRequired
                | PartitionTermination::NoJustifiedRead
        );
        for (known, retained) in timings.into_iter().zip([
            self.work.queue_us,
            self.work.compile_us,
            self.work.inference_us,
            self.work.policy_us,
        ]) {
            if let Some(known) = known {
                ensure!(
                    retained.is_some_and(|retained| known <= retained),
                    "joint measured timing not retained"
                );
            }
            // Failed/expired work may have actually overrun. Keep that measured
            // cost; only a successful learned terminal must fit the deadline.
            ensure!(
                !learned_terminal
                    || retained
                        .is_none_or(|retained| retained <= budget.deadline_ms.saturating_mul(1000)),
                "joint learned terminal exceeds original deadline"
            );
        }
        let selected = match self.assessment.coverage {
            PartitionCoverage::Unassessed { .. } => None,
            PartitionCoverage::Assessed { ordinal } => {
                let event = self
                    .assessment_history
                    .iter()
                    .find(|event| event.ordinal == ordinal)
                    .ok_or_else(|| anyhow::anyhow!("joint coverage ordinal missing"))?;
                ensure!(
                    event.applicability == AssessmentApplicability::Current
                        && self
                            .assessment_history
                            .last()
                            .is_some_and(|latest| latest.ordinal == ordinal),
                    "historical joint assessment cannot establish current coverage"
                );
                let AssessmentResult::Selected { card, .. } = &event.result else {
                    anyhow::bail!("unselected joint assessment cannot establish coverage");
                };
                Some((card.action, card.gap))
            }
        };
        ensure!(
            self.stop == PartitionTermination::Covered || !self.gaps.contains(&Gap::Sufficient),
            "joint sufficient gap requires covered terminal"
        );
        match self.stop {
            PartitionTermination::Covered => ensure!(
                !unknown
                    && selected == Some((ContextAction::Sufficient, Gap::Sufficient))
                    && self.gaps == [Gap::Sufficient],
                "joint coverage lacks current sufficient support"
            ),
            PartitionTermination::ClarificationRequired => ensure!(
                !unknown
                    && selected == Some((ContextAction::Clarify, Gap::Ambiguous))
                    && self.gaps == [Gap::Ambiguous],
                "joint clarification lacks current assessment"
            ),
            PartitionTermination::NoJustifiedRead => ensure!(
                !unknown
                    && selected
                        .is_some_and(|(action, gap)| action == ContextAction::NoJustifiedRead
                            && self.gaps == [gap]),
                "joint no-read terminal lacks current assessment"
            ),
            _ => ensure!(
                !self.gaps.contains(&Gap::Sufficient),
                "joint sufficient gap requires covered terminal"
            ),
        }
        for application in &self.applications {
            application.validate()?;
            let observed = &application.scope;
            let historical_configuration = observed.installation == scope.installation
                && observed.catalog == scope.catalog
                && observed.index_id == scope.index_id
                && observed.authorization == scope.authorization
                && observed.publication == scope.publication
                && matches!(
                    self.assessment.coverage,
                    PartitionCoverage::Unassessed { .. }
                )
                && self
                    .assessment_history
                    .iter()
                    .all(|event| event.applicability != AssessmentApplicability::Current)
                && !matches!(
                    self.stop,
                    PartitionTermination::Covered
                        | PartitionTermination::ClarificationRequired
                        | PartitionTermination::NoJustifiedRead
                );
            ensure!(
                observed == scope || historical_configuration,
                "joint application scope mismatch"
            );
        }
        ensure!(
            self.work.calls <= budget.calls
                && self.work.rounds <= budget.rounds
                && self.work.fanout <= budget.fanout
                && (self.work.input_tokens <= budget.input_tokens || failed_token_overrun)
                && self.work.candidates <= budget.candidates
                && self.work.bytes <= budget.max_bytes,
            "joint original work budget exceeded"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= budget.max_bytes,
            "joint response byte budget"
        );
        Ok(())
    }
}

fn validate_advice_basis(advice: &ContextActionAdvice, basis: &AssessmentBasis) -> Result<()> {
    let records: BTreeSet<_> = basis
        .source_units_presented
        .iter()
        .map(|unit| unit.record)
        .collect();
    ensure!(
        advice.evidence.iter().all(|id| records.contains(id))
            && records.iter().all(|id| advice.evidence.contains(id)),
        "joint advice evidence differs from presented basis"
    );
    Ok(())
}
fn pin_task_policy(current: &mut Option<Sha256Digest>, observed: Sha256Digest) -> Result<()> {
    ensure!(
        current.is_none_or(|expected| expected == observed),
        "joint history task policy changed"
    );
    *current = Some(observed);
    Ok(())
}
/// A synthetic known-zero preflight note is not a native NN receipt. Unknown
/// submission keeps its slot even without an observed reply or positive count.
pub fn has_native_receipt(event: &AssessmentEvent) -> bool {
    !matches!(
        &event.result,
        AssessmentResult::Unavailable {
            advice: None,
            work_unknown: false,
            ..
        }
    )
}

/// Serde normally treats an absent Option as None. Current native contracts
/// instead require the field to be present, with explicit null when unknown.
pub(crate) fn required_nullable<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn nullable_tokens_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<usize>>()
}
fn nullable_advice_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<ContextActionAdvice>>()
}

#[cfg(test)]
mod tests;
