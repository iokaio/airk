// SPDX-License-Identifier: Apache-2.0
//! Answer loop wire contract, version 3 (final plan section 12). Inert
//! request and response shapes with their bounds; no execution, model,
//! scheduling or source authority. Outside the Context and Focus semantic
//! digest closures until the loop's consumers adopt it (CONTRACT.md, L1).
use super::context_action::required_nullable;
use super::device::SelectedModel;
use super::*;

mod calibration;
mod cards;
mod focus_batch;
mod log;
mod semantics;
pub use calibration::*;
pub use cards::*;
pub use focus_batch::*;
pub use log::*;
pub use semantics::loop_semantic_policy_digest;

pub const REQUEST_SCHEMA_V3: &str = "airk.arbiter.context.request/3";
pub const CONTEXT_SCHEMA_V3: &str = "airk.arbiter.context/3";
pub const PACKET_SCHEMA_V2: &str = "airk.arbiter.packet/2";

/// Same question bound as requests /1 and /2.
pub const MAX_QUESTION_BYTES: usize = 16_384;
pub const MAX_PARTS: usize = 4;
pub const MAX_SUB_NEEDS: usize = 4;
pub const MAX_ROUNDS: usize = 3;
/// Round one of a sub-need: its search, then symbols, references, imports or
/// outline, the governing CONTRACT.md or README, and tests.
pub const FIRST_ROUND_READS_PER_SUB_NEED: usize = 6;
/// Each later round runs one targeted query or structural hop per gap.
pub const FOLLOW_UP_READS_PER_SUB_NEED_ROUND: usize = 1;
/// The mandatory search of a part's own text, in its first round.
pub const PART_SEARCHES: usize = 1;
/// A full loop: round one 4 x (1 + 4 x 6) = 100, later rounds 4 x 2 x 4 = 32.
pub const DEFAULT_CALLS: usize = 132;
pub const MAX_CALLS: usize = 160;
/// One candidate record per canonical read, before dedup.
pub const DEFAULT_CANDIDATES: usize = DEFAULT_CALLS;
pub const MAX_CANDIDATES: usize = MAX_CALLS;
pub const MAX_FANOUT: usize = 12;
pub const DEFAULT_FANOUT: usize = 8;
/// Model input tokens over the whole attempt.
pub const MAX_INPUT_TOKENS: usize = 262_144;
pub const DEFAULT_INPUT_TOKENS: usize = 131_072;
/// Model input tokens of one round of one part: two batched coverage calls
/// and one follow-up call at 8,192, plus 16 Focus pairs at 512.
pub const MAX_ROUND_INPUT_TOKENS: usize = 32_768;
/// Context card calls (slots). A full loop with two-sub-need batching is
/// 1 Split + 4 Plan + 24 coverage + 16 follow-up = 45.
pub const MAX_CONTEXT_CALLS: usize = 64;
pub const DEFAULT_CONTEXT_CALLS: usize = 48;
/// Unbatched Focus: 4 parts x 3 rounds x 4 sub-needs.
pub const MAX_FOCUS_CALLS: usize = 48;
pub const DEFAULT_FOCUS_CALLS: usize = 24;
/// 12 part-rounds x 32 pairs, the batch ceiling; default 16 pairs a part-round.
pub const MAX_FOCUS_PAIRS: usize = 384;
pub const DEFAULT_FOCUS_PAIRS: usize = 192;
pub const MIN_RETENTION_BYTES: usize = 1024;
pub const MAX_RETENTION_BYTES: usize = 524_288;
pub const DEFAULT_RETENTION_BYTES: usize = 262_144;
/// The native reply frame of every other tool and of Answer /2.
pub const MAX_RESPONSE_BYTES: usize = 65_536;
/// The Answer /3 reply frame (user decision, 22 November 2026). A reply is
/// bounded by the smaller of this and the request's `max_bytes`.
pub const MAX_ANSWER_RESPONSE_BYTES: usize = 131_072;
/// The smallest reply around a question: scope, one part, one sub-need,
/// one round, the decision log summary and the work counters.
pub const MIN_REPLY_SKELETON_BYTES: usize = 4096;
pub const MAX_DEADLINE_MS: u64 = 600_000;
/// Time allowed past the deadline to compose, union and write the reply.
pub const DEADLINE_TOLERANCE_MS: u64 = 1_000;
pub const DEFAULT_DEADLINE_MS: u64 = 300_000;
/// One Context card call at the Compact and Large profile ceiling.
pub const CONTEXT_CALL_TOKENS: usize = 8192;
/// Split once, Plan once per part, then coverage and follow-up per sub-need
/// and round: 1 + 4 x (1 + 3 x 4 x 2) = 101.
pub const MAX_DECISIONS: usize = 1 + MAX_PARTS * (1 + MAX_ROUNDS * MAX_SUB_NEEDS * 2);
pub const MAX_TEXT_BYTES: usize = 1024;
pub const MAX_UNRUN_READS: usize = 4;
pub const MAX_CONFLICTS: usize = 16;
pub const MAX_DEGRADATIONS: usize = 32;
pub const MAX_RECORD_USES: usize = 512;
pub const MAX_GAPS: usize = 8;
pub const MAX_QUEUE_POSITION: u32 = 4096;
pub const MAX_ESTIMATE_MS: u64 = 86_400_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BudgetV3 {
    /// Whole Answer, Answer only.
    pub deadline_ms: u64,
    /// Bytes retained while gathering; the reply takes at most the smaller
    /// of this and 131,072 bytes.
    pub max_bytes: usize,
    pub max_parts: usize,
    /// Per part.
    pub max_sub_needs: usize,
    /// Per part.
    pub rounds: usize,
    /// Canonical reads over the whole Answer.
    pub calls: usize,
    /// Peak concurrent reads in one round of one part.
    pub fanout: usize,
    /// Candidate records over the whole Answer, before dedup.
    pub candidates: usize,
    /// Model input token pool of the whole attempt.
    pub input_tokens: usize,
    /// Model input tokens of one round of one part.
    pub round_input_tokens: usize,
    /// Context card calls (slots).
    pub context_calls: usize,
    pub focus_calls: usize,
    pub focus_pairs: usize,
}
impl Default for BudgetV3 {
    fn default() -> Self {
        Self {
            deadline_ms: DEFAULT_DEADLINE_MS,
            max_bytes: DEFAULT_RETENTION_BYTES,
            max_parts: MAX_PARTS,
            max_sub_needs: MAX_SUB_NEEDS,
            rounds: MAX_ROUNDS,
            calls: DEFAULT_CALLS,
            fanout: DEFAULT_FANOUT,
            candidates: DEFAULT_CANDIDATES,
            input_tokens: DEFAULT_INPUT_TOKENS,
            round_input_tokens: MAX_ROUND_INPUT_TOKENS,
            context_calls: DEFAULT_CONTEXT_CALLS,
            focus_calls: DEFAULT_FOCUS_CALLS,
            focus_pairs: DEFAULT_FOCUS_PAIRS,
        }
    }
}
impl BudgetV3 {
    pub fn validate(&self) -> Result<()> {
        let bounded = |value: usize, maximum: usize, message: &'static str| -> Result<()> {
            ensure!((1..=maximum).contains(&value), "{message}");
            Ok(())
        };
        ensure!(
            (1..=MAX_DEADLINE_MS).contains(&self.deadline_ms),
            "invalid Answer deadline"
        );
        ensure!(
            (MIN_RETENTION_BYTES..=MAX_RETENTION_BYTES).contains(&self.max_bytes),
            "invalid Answer retention bytes"
        );
        bounded(self.max_parts, MAX_PARTS, "invalid Answer part limit")?;
        bounded(
            self.max_sub_needs,
            MAX_SUB_NEEDS,
            "invalid Answer sub-need limit",
        )?;
        bounded(self.rounds, MAX_ROUNDS, "invalid Answer round limit")?;
        bounded(self.calls, MAX_CALLS, "invalid Answer read limit")?;
        bounded(self.fanout, MAX_FANOUT, "invalid Answer fanout")?;
        bounded(
            self.candidates,
            MAX_CANDIDATES,
            "invalid Answer candidate limit",
        )?;
        bounded(
            self.input_tokens,
            MAX_INPUT_TOKENS,
            "invalid Answer token pool",
        )?;
        bounded(
            self.round_input_tokens,
            MAX_ROUND_INPUT_TOKENS,
            "invalid Answer round token allowance",
        )?;
        ensure!(
            self.round_input_tokens <= self.input_tokens,
            "Answer round allowance exceeds the token pool"
        );
        bounded(
            self.context_calls,
            MAX_CONTEXT_CALLS,
            "invalid Answer card slots",
        )?;
        bounded(
            self.focus_calls,
            MAX_FOCUS_CALLS,
            "invalid Answer Focus calls",
        )?;
        bounded(
            self.focus_pairs,
            MAX_FOCUS_PAIRS,
            "invalid Answer Focus pairs",
        )?;
        ensure!(
            self.focus_calls <= self.focus_pairs,
            "Answer Focus calls exceed Focus pairs"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextRequestV3 {
    pub schema: String,
    pub question: String,
    pub budget: BudgetV3,
    /// A ticket or continuation id, to attach to an existing job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<Sha256Digest>,
}
impl ContextRequestV3 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == REQUEST_SCHEMA_V3,
            "invalid Answer request schema"
        );
        validate_question(&self.question)?;
        self.budget.validate()?;
        ensure!(
            self.question.len() + MIN_REPLY_SKELETON_BYTES <= self.budget.max_bytes,
            "Answer retention bytes cannot hold the question and a minimal reply"
        );
        Ok(())
    }
}
pub(crate) fn validate_question(question: &str) -> Result<()> {
    ensure!(
        !question.trim().is_empty() && question.len() <= MAX_QUESTION_BYTES,
        "invalid Answer question"
    );
    Ok(())
}

/// The loop's stages, step 0 to step 6, with admission before them.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LoopStage {
    Admission,
    Split,
    Plan,
    Retrieve,
    Read,
    Coverage,
    FollowUp,
    Compose,
    Union,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubNeedKind {
    Definition,
    Callers,
    Tests,
    Contract,
    Documentation,
    Configuration,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Covered,
    Partial,
    Missing,
}

/// Canonical reads the loop may dispatch or propose.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LoopRead {
    Search,
    Symbols,
    Definitions,
    References,
    Imports,
    Outline,
    ReadFile,
    DocumentSections,
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FollowUp {
    pub operation: LoopRead,
    /// Query wording, identifier, path or name, from what was read.
    pub target: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum UnrunReason {
    Deadline,
    Budget,
    RoundLimit,
    NotChosen,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnrunRead {
    pub operation: LoopRead,
    pub target: String,
    pub reason: UnrunReason,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubNeed {
    /// Numbered from 1 within its part.
    pub id: u8,
    /// The sub-need's own query wording.
    pub text: String,
    pub kind: SubNeedKind,
    pub coverage: Coverage,
    /// Record ids this sub-need cites, retained or omitted by the frame.
    pub evidence: Vec<Sha256Digest>,
    /// How many cited records the frame omitted.
    pub evidence_omitted: u16,
    /// Reads chosen or proposed but not run, for the agent to run itself.
    pub unrun_reads: Vec<UnrunRead>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuestionSpan {
    pub utf8_start: usize,
    pub utf8_end: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundProvenance {
    /// Numbered from 1, consecutive within its part.
    pub round: u8,
    pub calls: usize,
    pub fanout: usize,
    /// Observed model input tokens charged to this part in this round.
    pub input_tokens: usize,
    /// Candidate records first found by this part in this round, retained or not.
    pub new_evidence: usize,
    /// Follow-ups chosen at the end of the previous round and run in this one.
    pub follow_ups_run: Vec<FollowUp>,
    /// Sub-needs judged covered at the end of this round.
    pub sub_needs_covered: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartStop {
    /// Every sub-need is covered.
    Covered,
    /// The question needs clarification (the /2 clarification terminal).
    ClarificationRequired,
    /// No proposed read is justified (the /2 no-read terminal).
    NoJustifiedRead,
    /// A round added nothing new.
    NoProgress,
    /// The round limit was reached.
    Rounds,
    Budget,
    Deadline,
    Stale,
    Refused,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerPart {
    /// Numbered from 1 in question order.
    pub id: u8,
    pub span: QuestionSpan,
    /// This part's share of the deadline, after any early finisher's return.
    pub deadline_share_ms: u64,
    pub sub_needs: Vec<SubNeed>,
    /// The /2 gap kinds that remain for this part, beside the sub-need kinds.
    pub gaps: Vec<Gap>,
    pub rounds: Vec<RoundProvenance>,
    pub stop: PartStop,
    /// The part was shortened to fit the shared reply frame.
    pub shortened: bool,
}

/// One deduplicated canonical read, merged by location and content digest.
/// Its scope is the reply's `scope`; a v3 record never repeats it. A loop
/// search defers in-search Focus, so it carries no Focus receipt.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopEvidence {
    pub id: Sha256Digest,
    pub operation: LoopRead,
    pub arguments: BTreeMap<String, serde_json::Value>,
    pub role: EvidenceRole,
    pub canonical_result: serde_json::Value,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum UseCause {
    /// The part's own search, in round one.
    PartSearch,
    /// A sub-need's search, in round one.
    NeedSearch,
    /// A structural read of round one.
    StructuralRead,
    /// A follow-up read of a later round.
    FollowUp,
    /// Served from the shared read cache.
    SharedRead,
}

/// One finding of a retained record: provenance naming every part and sub-need.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordUse {
    pub record: Sha256Digest,
    pub part: u8,
    /// Absent for a part's own search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<u8>,
    pub round: u8,
    pub cause: UseCause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Parts found different definitions for one name: two or more records.
    DivergentDefinition,
    /// Parts judged the same evidence differently: one record or more.
    DivergentVerdict,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConflictNote {
    pub kind: ConflictKind,
    pub parts: Vec<u8>,
    pub evidence: Vec<Sha256Digest>,
    pub note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnionSummary {
    /// Findings merged into a record another part also found.
    pub dedup_count: usize,
    /// Noted, never silently resolved.
    pub conflicts: Vec<ConflictNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartialReason {
    Queued,
    Deadline,
    Budget,
    Bytes,
    Stale,
    Refused,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartialMarker {
    pub stage: LoopStage,
    pub reason: PartialReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u8>,
    /// The still-running job to attach to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<Sha256Digest>,
}

/// Times are milliseconds from the reply.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerTicket {
    pub id: Sha256Digest,
    pub queue_position: u32,
    pub estimated_start_ms: u64,
    pub estimated_finish_ms: u64,
    pub retry_after_ms: u64,
}
impl AnswerTicket {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_QUEUE_POSITION).contains(&self.queue_position)
                && self.estimated_start_ms <= self.estimated_finish_ms
                && self.estimated_finish_ms <= MAX_ESTIMATE_MS
                && (1..=MAX_DEADLINE_MS).contains(&self.retry_after_ms)
                && self.retry_after_ms <= self.estimated_finish_ms,
            "invalid Answer ticket"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DegradationReason {
    DeadlineEstimate,
    QueueWait,
    DeviceUnavailable,
    MemoryUnavailable,
    ModelUnavailable,
}

/// What was reduced or left out, and why.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Degradation {
    /// No Context model: every card decision took the deterministic order.
    ContextAbsent,
    /// Context decisions that abstained and fell back.
    ContextAbstained { count: usize },
    /// No Focus model ran: candidates kept their canonical order.
    FocusAbsent,
    /// A part received less than its share of a model stage.
    ModelShare { part: u8, stage: LoopStage },
    /// A submitted model call's outcome is unknown; its slot stays charged.
    ModelOutcomeUnknown,
    /// The configuration changed during the Answer; later decisions are historical.
    ConfigurationChanged,
    /// A batched model call was split to fit.
    BatchSplit,
    /// Focus over the search results was deferred.
    SearchFocusDeferred,
    /// Admission planned fewer parts (section 12.4.1).
    Parts {
        requested: usize,
        applied: usize,
        reason: DegradationReason,
    },
    SubNeeds {
        requested: usize,
        applied: usize,
        reason: DegradationReason,
    },
    Rounds {
        requested: usize,
        applied: usize,
        reason: DegradationReason,
    },
    /// Down the model ladder; no applied model is the no-model loop.
    Model {
        stage: Stage,
        requested: SelectedModel,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        applied: Option<SelectedModel>,
        reason: DegradationReason,
    },
    /// Down the device ladder, from a GPU to the CPU.
    Device {
        stage: Stage,
        requested: ExecutionDevice,
        applied: ExecutionDevice,
        reason: DegradationReason,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopModelWork {
    /// Card calls (slots) used.
    pub context_calls: usize,
    pub focus_calls: usize,
    pub focus_pairs: usize,
    /// The Focus profile every identified batched Focus receipt ran on, so a
    /// delivery owner can bind the Focus stage from the reply alone (the
    /// receipts stay in custody). Null when no Focus receipt is identified.
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_profile")]
    pub focus_profile: Option<Sha256Digest>,
}
fn nullable_profile(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<Sha256Digest>>()
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextResponseV3 {
    pub schema: String,
    pub packet_schema: String,
    pub question: String,
    /// The one index partition this Answer ran on.
    pub scope: ScopeBinding,
    pub parts: Vec<AnswerPart>,
    pub records: Vec<LoopEvidence>,
    /// Ids of cited records the reply frame omitted.
    pub omitted_evidence: Vec<Sha256Digest>,
    pub record_uses: Vec<RecordUse>,
    pub paragraphs: Vec<GroundedParagraph>,
    pub union: UnionSummary,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_partial")]
    pub partial: Option<PartialMarker>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_ticket")]
    pub ticket: Option<AnswerTicket>,
    pub degradations: Vec<Degradation>,
    /// Compact card decisions, oldest deterministic ones omitted first.
    pub decisions: Vec<DecisionRecord>,
    pub decision_log: DecisionLogSummary,
    pub work: Work,
    pub model_work: LoopModelWork,
    /// Wall-clock milliseconds from admission to the reply, set by the kernel.
    pub elapsed_ms: u64,
    pub candidate_count: usize,
    pub omitted_count: usize,
    pub applications: Vec<IndexApplication>,
}

fn nullable_partial(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<PartialMarker>>()
}
fn nullable_ticket(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<AnswerTicket>>()
}

/// The bytes one Answer /3 reply may take, as transported.
pub fn reply_limit(budget: &BudgetV3) -> usize {
    budget.max_bytes.min(MAX_ANSWER_RESPONSE_BYTES)
}

pub(crate) fn validate_text(text: &str, message: &'static str) -> Result<()> {
    ensure!(
        !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES,
        "{message}"
    );
    Ok(())
}
pub(crate) fn unique<T: Ord>(items: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    items.into_iter().all(|item| seen.insert(item))
}
fn part_stop_reason(stop: PartStop) -> Option<PartialReason> {
    match stop {
        PartStop::Budget => Some(PartialReason::Budget),
        PartStop::Deadline => Some(PartialReason::Deadline),
        PartStop::Stale => Some(PartialReason::Stale),
        PartStop::Refused => Some(PartialReason::Refused),
        PartStop::Covered
        | PartStop::ClarificationRequired
        | PartStop::NoJustifiedRead
        | PartStop::NoProgress
        | PartStop::Rounds
        | PartStop::Unsupported => None,
    }
}
fn gap_rank(gap: Gap) -> u8 {
    gap as u8
}

/// Limits in force after admission's disclosed degradations.
struct Effective {
    parts: usize,
    sub_needs: usize,
    rounds: usize,
}

/// Retained and frame-omitted record ids.
struct Evidence {
    retained: BTreeSet<Sha256Digest>,
    omitted: BTreeSet<Sha256Digest>,
}

impl ContextResponseV3 {
    /// Typed accounting view of this exact native schema inside the canonical
    /// read envelope. Callers retain the ORIGINAL complete value for delivery.
    /// The frame is measured on that whole envelope as transported.
    pub fn from_canonical(value: &serde_json::Value, budget: &BudgetV3) -> Result<Self> {
        ensure!(
            serde_json::to_vec(value)?.len() <= reply_limit(budget),
            "Answer envelope exceeds the reply frame"
        );
        let object = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Answer packet is not an object"))?;
        let schema = schemars::schema_for!(ContextResponseV3);
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("Answer native schema lacks properties"))?;
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
                publication == typed.scope.publication,
                "Answer outer publication mismatch"
            );
        }
        Ok(typed)
    }

    pub fn validate(&self, budget: &BudgetV3) -> Result<()> {
        budget.validate()?;
        validate_question(&self.question)?;
        ensure!(
            self.schema == CONTEXT_SCHEMA_V3 && self.packet_schema == PACKET_SCHEMA_V2,
            "invalid Answer response schema"
        );
        let effective = self.validate_degradations(budget)?;
        let evidence = self.validate_records()?;
        self.validate_parts(budget, &effective, &evidence)?;
        self.validate_uses(&evidence.retained)?;
        self.validate_union(&evidence)?;
        for paragraph in &self.paragraphs {
            ensure!(
                !paragraph.text.is_empty()
                    && !paragraph.evidence.is_empty()
                    && paragraph
                        .evidence
                        .iter()
                        .all(|id| evidence.retained.contains(id)),
                "ungrounded Answer paragraph"
            );
        }
        self.validate_decisions(budget)?;
        self.validate_log_summary(budget)?;
        self.validate_partial()?;
        self.validate_applications()?;
        self.validate_work(budget)?;
        let bytes = serde_json::to_vec(self)?.len();
        ensure!(
            bytes <= reply_limit(budget),
            "Answer response exceeds the reply frame"
        );
        Ok(())
    }

    fn part(&self, id: u8) -> Option<&AnswerPart> {
        self.parts.iter().find(|part| part.id == id)
    }
    fn has_need(&self, part: u8, need: u8) -> bool {
        self.part(part)
            .is_some_and(|part| part.sub_needs.iter().any(|item| item.id == need))
    }
    fn has_round(&self, part: u8, round: u8) -> bool {
        self.part(part)
            .is_some_and(|part| (1..=part.rounds.len()).contains(&usize::from(round)))
    }

    fn validate_degradations(&self, budget: &BudgetV3) -> Result<Effective> {
        ensure!(
            self.degradations.len() <= MAX_DEGRADATIONS,
            "Answer degradation limit"
        );
        let summary = &self.decision_log;
        let mut effective = Effective {
            parts: budget.max_parts,
            sub_needs: budget.max_sub_needs,
            rounds: budget.rounds,
        };
        let mut kinds = BTreeSet::new();
        let mut focus_absent = false;
        for degradation in &self.degradations {
            let kind: (u8, u8, u8) = match degradation {
                Degradation::ContextAbsent => {
                    ensure!(
                        summary.count(None, SourceKind::Deterministic) == summary.total,
                        "absent Context model cannot have scored a card"
                    );
                    (0, 0, 0)
                }
                Degradation::ContextAbstained { count } => {
                    ensure!(
                        *count > 0 && *count == summary.count(None, SourceKind::Abstained),
                        "Answer abstention count differs from the decision log"
                    );
                    (1, 0, 0)
                }
                Degradation::FocusAbsent => {
                    focus_absent = true;
                    (2, 0, 0)
                }
                Degradation::ModelShare { part, stage } => {
                    ensure!(
                        self.part(*part).is_some()
                            && matches!(
                                stage,
                                LoopStage::Plan
                                    | LoopStage::Read
                                    | LoopStage::Coverage
                                    | LoopStage::FollowUp
                            ),
                        "invalid Answer model share disclosure"
                    );
                    (3, *part, *stage as u8)
                }
                Degradation::ModelOutcomeUnknown => {
                    ensure!(
                        summary.work_unknown > 0,
                        "unknown model outcome without unknown logged work"
                    );
                    (4, 0, 0)
                }
                Degradation::ConfigurationChanged => (5, 0, 0),
                Degradation::BatchSplit => (6, 0, 0),
                Degradation::SearchFocusDeferred => (7, 0, 0),
                Degradation::Parts {
                    requested, applied, ..
                } => {
                    ensure!(
                        *requested <= budget.max_parts && (1..*requested).contains(applied),
                        "invalid Answer part degradation"
                    );
                    effective.parts = *applied;
                    (8, 0, 0)
                }
                Degradation::SubNeeds {
                    requested, applied, ..
                } => {
                    ensure!(
                        *requested <= budget.max_sub_needs && (1..*requested).contains(applied),
                        "invalid Answer sub-need degradation"
                    );
                    effective.sub_needs = *applied;
                    (9, 0, 0)
                }
                Degradation::Rounds {
                    requested, applied, ..
                } => {
                    ensure!(
                        *requested <= budget.rounds && (1..*requested).contains(applied),
                        "invalid Answer round degradation"
                    );
                    effective.rounds = *applied;
                    (10, 0, 0)
                }
                Degradation::Model {
                    stage,
                    requested,
                    applied,
                    ..
                } => {
                    ensure!(
                        requested.stage() == *stage
                            && matches!(
                                (requested, applied),
                                (
                                    SelectedModel::ContextLarge,
                                    Some(SelectedModel::ContextCompact)
                                ) | (_, None)
                            ),
                        "invalid Answer model degradation"
                    );
                    ensure!(
                        self.decisions
                            .iter()
                            .all(|decision| decision.model != Some(*requested))
                            && summary
                                .counts
                                .iter()
                                .all(|row| row.model != Some(*requested)),
                        "degraded Answer model still decided"
                    );
                    (11, u8::from(stage.is_context()), 0)
                }
                Degradation::Device {
                    stage,
                    requested,
                    applied,
                    ..
                } => {
                    ensure!(
                        *requested != ExecutionDevice::Cpu && *applied == ExecutionDevice::Cpu,
                        "invalid Answer device degradation"
                    );
                    (12, u8::from(stage.is_context()), 0)
                }
            };
            ensure!(kinds.insert(kind), "duplicate Answer degradation");
        }
        ensure!(
            focus_absent == (summary.focus_calls == 0) || self.ticket.is_some(),
            "FocusAbsent is disclosed exactly when no Focus call ran"
        );
        Ok(effective)
    }

    fn validate_records(&self) -> Result<Evidence> {
        let retained: BTreeSet<_> = self.records.iter().map(|record| record.id).collect();
        ensure!(
            retained.len() == self.records.len(),
            "Answer evidence is not one exact deduplicated partition"
        );
        let search = self.search_focus()?;
        ensure!(
            !search.has_native_receipt
                && !search.work_unknown
                && !search.failed_token_overrun
                && search.work == Work::default(),
            "Answer loop searches defer in-search Focus"
        );
        ensure!(
            self.records.len().checked_add(self.omitted_count) == Some(self.candidate_count),
            "invalid Answer omission accounting"
        );
        let omitted: BTreeSet<_> = self.omitted_evidence.iter().copied().collect();
        ensure!(
            omitted.len() == self.omitted_evidence.len()
                && omitted.len() <= self.omitted_count
                && omitted.is_disjoint(&retained),
            "invalid Answer omitted evidence list"
        );
        Ok(Evidence { retained, omitted })
    }

    fn validate_parts(
        &self,
        budget: &BudgetV3,
        effective: &Effective,
        evidence: &Evidence,
    ) -> Result<()> {
        ensure!(
            self.parts.len() <= effective.parts,
            "Answer part limit exceeded"
        );
        let mut previous_end = 0;
        let mut shares = 0u64;
        for (index, part) in self.parts.iter().enumerate() {
            ensure!(
                usize::from(part.id) == index + 1,
                "noncanonical Answer part id"
            );
            let span = part.span;
            ensure!(
                previous_end <= span.utf8_start
                    && span.utf8_start < span.utf8_end
                    && span.utf8_end <= self.question.len()
                    && self.question.is_char_boundary(span.utf8_start)
                    && self.question.is_char_boundary(span.utf8_end),
                "invalid Answer part span"
            );
            previous_end = span.utf8_end;
            shares = shares
                .checked_add(part.deadline_share_ms)
                .ok_or_else(|| anyhow::anyhow!("Answer deadline share overflow"))?;
            ensure!(
                !part.sub_needs.is_empty() && part.sub_needs.len() <= effective.sub_needs,
                "Answer sub-need limit"
            );
            for (position, need) in part.sub_needs.iter().enumerate() {
                ensure!(
                    usize::from(need.id) == position + 1,
                    "noncanonical Answer sub-need id"
                );
                validate_text(&need.text, "invalid Answer sub-need text")?;
                let omitted = need
                    .evidence
                    .iter()
                    .filter(|id| evidence.omitted.contains(id))
                    .count();
                ensure!(
                    unique(&need.evidence)
                        && need
                            .evidence
                            .iter()
                            .all(|id| evidence.retained.contains(id)
                                || evidence.omitted.contains(id))
                        && usize::from(need.evidence_omitted) == omitted,
                    "Answer sub-need cites unknown or uncounted evidence"
                );
                ensure!(
                    match need.coverage {
                        Coverage::Covered | Coverage::Partial => !need.evidence.is_empty(),
                        Coverage::Missing => need.evidence.is_empty(),
                    },
                    "Answer coverage disagrees with cited evidence"
                );
                ensure!(
                    need.unrun_reads.len() <= MAX_UNRUN_READS
                        && unique(
                            need.unrun_reads
                                .iter()
                                .map(|read| (read.operation, &read.target))
                        )
                        && (need.coverage != Coverage::Covered || need.unrun_reads.is_empty()),
                    "invalid Answer unrun reads"
                );
                for read in &need.unrun_reads {
                    validate_text(&read.target, "invalid Answer unrun read target")?;
                    ensure!(
                        match read.reason {
                            UnrunReason::Deadline => part.stop == PartStop::Deadline,
                            UnrunReason::Budget => part.stop == PartStop::Budget,
                            UnrunReason::RoundLimit => part.stop == PartStop::Rounds,
                            UnrunReason::NotChosen => true,
                        },
                        "Answer unrun read reason differs from its part's stop"
                    );
                }
            }
            self.validate_gaps(part)?;
            self.validate_rounds(part, budget, effective)?;
        }
        ensure!(
            shares <= budget.deadline_ms,
            "Answer deadline shares exceed the deadline"
        );
        Ok(())
    }

    /// The /2 gap rules, per part.
    fn validate_gaps(&self, part: &AnswerPart) -> Result<()> {
        ensure!(
            part.gaps.len() <= MAX_GAPS && unique(part.gaps.iter().map(|gap| gap_rank(*gap))),
            "invalid Answer part gaps"
        );
        let consistent = match part.stop {
            PartStop::Covered => part.gaps == [Gap::Sufficient],
            PartStop::ClarificationRequired => {
                part.gaps.contains(&Gap::Ambiguous) && !part.gaps.contains(&Gap::Sufficient)
            }
            PartStop::NoJustifiedRead => {
                !part.gaps.is_empty() && !part.gaps.contains(&Gap::Sufficient)
            }
            _ => !part.gaps.contains(&Gap::Sufficient),
        };
        ensure!(
            consistent,
            "Answer part gaps disagree with its stop (sufficient requires covered)"
        );
        Ok(())
    }

    fn validate_rounds(
        &self,
        part: &AnswerPart,
        budget: &BudgetV3,
        effective: &Effective,
    ) -> Result<()> {
        let needs = part.sub_needs.len();
        ensure!(
            part.rounds.len() <= effective.rounds,
            "Answer round limit exceeded"
        );
        for (index, round) in part.rounds.iter().enumerate() {
            ensure!(
                usize::from(round.round) == index + 1,
                "noncanonical Answer round number"
            );
            let reads = if index == 0 {
                PART_SEARCHES + needs * FIRST_ROUND_READS_PER_SUB_NEED
            } else {
                needs * FOLLOW_UP_READS_PER_SUB_NEED_ROUND
            };
            ensure!(
                round.calls >= 1 && round.calls <= reads,
                "Answer round reads exceed its structural limit"
            );
            ensure!(
                round.fanout >= 1 && round.fanout <= round.calls && round.fanout <= budget.fanout,
                "Answer round fanout exceeds its reads or budget"
            );
            ensure!(
                round.input_tokens <= budget.round_input_tokens,
                "Answer round token allowance exceeded"
            );
            ensure!(
                if index == 0 {
                    round.follow_ups_run.is_empty()
                } else {
                    !round.follow_ups_run.is_empty()
                        && round.follow_ups_run.len() <= needs
                        && round.follow_ups_run.len() <= round.calls
                },
                "Answer follow-ups run outside a follow-up round"
            );
            for follow_up in &round.follow_ups_run {
                validate_text(&follow_up.target, "invalid Answer follow-up target")?;
            }
            ensure!(
                round.sub_needs_covered <= needs,
                "Answer round coverage count"
            );
        }
        let covered = part
            .sub_needs
            .iter()
            .filter(|need| need.coverage == Coverage::Covered)
            .count();
        ensure!(
            part.rounds
                .last()
                .is_none_or(|last| last.sub_needs_covered == covered),
            "Answer final round coverage differs from sub-needs"
        );
        let consistent = match part.stop {
            PartStop::Covered => covered == needs && !part.rounds.is_empty(),
            PartStop::NoProgress => part
                .rounds
                .last()
                .is_some_and(|last| part.shortened || last.new_evidence == 0),
            PartStop::Rounds => part.rounds.len() == effective.rounds,
            _ => true,
        };
        ensure!(consistent, "Answer part stop disagrees with its rounds");
        ensure!(
            covered < needs
                || matches!(
                    part.stop,
                    PartStop::Covered | PartStop::ClarificationRequired | PartStop::NoJustifiedRead
                )
                || part_stop_reason(part.stop).is_some(),
            "Answer part with every sub-need covered must stop covered"
        );
        Ok(())
    }

    fn validate_uses(&self, ids: &BTreeSet<Sha256Digest>) -> Result<()> {
        ensure!(
            self.record_uses.len() <= MAX_RECORD_USES
                && unique(self.record_uses.iter().map(|item| (
                    item.record,
                    item.part,
                    item.need,
                    item.round
                ))),
            "invalid Answer record uses"
        );
        for item in &self.record_uses {
            ensure!(
                ids.contains(&item.record),
                "Answer record use names an unknown record"
            );
            ensure!(
                self.has_round(item.part, item.round)
                    && item.need.is_none_or(|need| self.has_need(item.part, need)),
                "Answer record use names an unknown part, sub-need or round"
            );
            ensure!(
                match item.cause {
                    UseCause::PartSearch => item.need.is_none() && item.round == 1,
                    UseCause::NeedSearch | UseCause::StructuralRead =>
                        item.need.is_some() && item.round == 1,
                    UseCause::FollowUp => item.need.is_some() && item.round >= 2,
                    UseCause::SharedRead => true,
                },
                "Answer record use cause differs from its round"
            );
        }
        for record in &self.records {
            ensure!(
                self.record_uses.iter().any(|item| item.record == record.id),
                "Answer record without provenance"
            );
        }
        Ok(())
    }

    fn validate_union(&self, evidence: &Evidence) -> Result<()> {
        let mut merged = 0usize;
        for record in &self.records {
            let parts: BTreeSet<_> = self
                .record_uses
                .iter()
                .filter(|item| item.record == record.id)
                .map(|item| item.part)
                .collect();
            merged += parts.len().saturating_sub(1);
        }
        // Omitted records take their provenance with them: the retained part
        // of the count is exact, the rest can only be more.
        ensure!(
            if self.omitted_count == 0 {
                self.union.dedup_count == merged
            } else {
                self.union.dedup_count >= merged
            },
            "Answer dedup count differs from record provenance"
        );
        ensure!(
            self.union.conflicts.len() <= MAX_CONFLICTS,
            "Answer conflict limit"
        );
        for conflict in &self.union.conflicts {
            validate_text(&conflict.note, "invalid Answer conflict note")?;
            let minimum = match conflict.kind {
                ConflictKind::DivergentDefinition => 2,
                ConflictKind::DivergentVerdict => 1,
            };
            ensure!(
                conflict.parts.len() >= 2
                    && unique(&conflict.parts)
                    && conflict.parts.iter().all(|id| self.part(*id).is_some())
                    && conflict.evidence.len() >= minimum
                    && unique(&conflict.evidence)
                    && conflict
                        .evidence
                        .iter()
                        .all(|id| evidence.retained.contains(id)),
                "invalid Answer conflict note"
            );
            for part in &conflict.parts {
                ensure!(
                    self.record_uses
                        .iter()
                        .any(|item| item.part == *part && conflict.evidence.contains(&item.record)),
                    "Answer conflict part found none of its evidence"
                );
            }
        }
        Ok(())
    }

    fn validate_decisions(&self, budget: &BudgetV3) -> Result<()> {
        let summary = &self.decision_log;
        let mut previous = 0u8;
        for decision in &self.decisions {
            ensure!(
                decision.ordinal > previous && usize::from(decision.ordinal) <= summary.total,
                "Answer decision ordinals are not increasing within the log"
            );
            previous = decision.ordinal;
            decision.validate(budget.context_calls)?;
            match decision.stage {
                LoopStage::Split => {}
                LoopStage::Plan => ensure!(
                    decision.part.is_some_and(|part| self.part(part).is_some()),
                    "Plan names an unknown part"
                ),
                _ => ensure!(
                    match (decision.part, decision.need, decision.round) {
                        (Some(part), Some(need), Some(round)) =>
                            self.has_need(part, need) && self.has_round(part, round),
                        _ => false,
                    },
                    "Answer decision names an unknown part, sub-need or round"
                ),
            }
            for other in &decision.batched_with {
                ensure!(
                    usize::from(*other) <= summary.total,
                    "Answer batch names an unknown decision"
                );
                if let Some(peer) = self.decisions.iter().find(|d| d.ordinal == *other) {
                    ensure!(
                        peer.card_slot == decision.card_slot
                            && peer.receipt == decision.receipt
                            && peer.batched_with.contains(&decision.ordinal),
                        "Answer batch peers do not share one card call"
                    );
                }
            }
        }
        // Only the latest local decision per (part, need, stage) is current.
        for decision in &self.decisions {
            if decision.applicability == AssessmentApplicability::Current {
                ensure!(
                    !self
                        .decisions
                        .iter()
                        .any(|later| later.ordinal > decision.ordinal
                            && later.stage == decision.stage
                            && later.part == decision.part
                            && later.need == decision.need),
                    "only the latest local decision can remain current"
                );
            }
        }
        Ok(())
    }

    fn validate_log_summary(&self, budget: &BudgetV3) -> Result<()> {
        let summary = &self.decision_log;
        ensure!(
            summary.total <= MAX_DECISIONS
                && summary.included == self.decisions.len()
                && summary.included.checked_add(summary.omitted) == Some(summary.total),
            "decision log summary counts disagree"
        );
        ensure!(
            unique(
                summary
                    .counts
                    .iter()
                    .map(|row| (row.stage, row.source, row.model))
            ) && summary.counts.iter().all(|row| row.count > 0
                && card_task(row.stage).is_some()
                && (row.source == SourceKind::Deterministic) == row.model.is_none())
                && summary.counts.iter().map(|row| row.count).sum::<usize>() == summary.total,
            "invalid decision log stage and source counts"
        );
        ensure!(
            summary.digest != empty_log_digest()
                || (summary.total == 0
                    && summary.focus_calls == 0
                    && summary.attribution.is_empty()),
            "only an empty decision log has the empty log digest"
        );
        ensure!(
            summary.digest == empty_log_digest()
                || summary.total > 0
                || summary.focus_calls > 0
                || !summary.attribution.is_empty(),
            "an empty decision log has the fixed empty log digest"
        );
        for row in &summary.counts {
            let included = self
                .decisions
                .iter()
                .filter(|d| {
                    d.stage == row.stage && d.source.kind() == row.source && d.model == row.model
                })
                .count();
            ensure!(
                included <= row.count,
                "reply includes more decisions than the log counts"
            );
        }
        ensure!(
            self.decisions.iter().all(|decision| summary
                .counts
                .iter()
                .any(|row| row.stage == decision.stage
                    && row.source == decision.source.kind()
                    && row.model == decision.model)),
            "reply decision of an uncounted stage and source"
        );
        let scored_total = summary.total - summary.count(None, SourceKind::Deterministic);
        let scored_included = self
            .decisions
            .iter()
            .filter(|d| d.scored_by_model())
            .count();
        ensure!(
            scored_included == scored_total
                || self
                    .decisions
                    .iter()
                    .all(|decision| decision.scored_by_model()),
            "Union omits deterministic decisions before model-scored ones"
        );
        let slots: BTreeSet<_> = self.decisions.iter().filter_map(|d| d.card_slot).collect();
        ensure!(
            slots.len() <= summary.card_calls
                && summary.card_calls <= scored_total
                && (scored_total == 0) == (summary.card_calls == 0)
                && summary.card_calls <= budget.context_calls,
            "decision log card calls disagree with scored decisions or slots"
        );
        ensure!(
            self.decisions.iter().filter(|d| d.work_unknown).count() <= summary.work_unknown,
            "reply has more unknown decisions than the log"
        );
        ensure!(
            self.model_work.context_calls == summary.card_calls
                && self.model_work.focus_calls == summary.focus_calls
                && self.model_work.focus_pairs == summary.focus_pairs,
            "Answer model work differs from the decision log"
        );
        ensure!(
            self.model_work.focus_profile.is_none() || summary.focus_calls > 0,
            "a Focus profile without Focus calls"
        );
        ensure!(
            summary.focus_calls <= budget.focus_calls
                && summary.focus_pairs <= budget.focus_pairs
                && summary.focus_calls <= summary.focus_pairs
                && summary.focus_pairs <= summary.focus_calls * MAX_FOCUS_BATCH_PAIRS,
            "Answer Focus work exceeds its budget"
        );
        validate_charges(&summary.attribution, MAX_ATTRIBUTIONS)?;
        for charge in &summary.attribution {
            if charge.round == 0 {
                ensure!(
                    charge.part == 0 || self.part(charge.part).is_some(),
                    "decision log charges an unknown part"
                );
                continue;
            }
            let recorded = self
                .part(charge.part)
                .and_then(|part| part.rounds.get(usize::from(charge.round) - 1))
                .map(|round| round.input_tokens)
                .ok_or_else(|| anyhow::anyhow!("decision log charges an unknown part or round"))?;
            ensure!(
                charge.tokens <= recorded,
                "attributed round tokens are not retained by the round"
            );
        }
        Ok(())
    }

    fn validate_partial(&self) -> Result<()> {
        if let Some(ticket) = &self.ticket {
            ticket.validate()?;
            ensure!(
                self.parts.is_empty()
                    && self.records.is_empty()
                    && self.record_uses.is_empty()
                    && self.paragraphs.is_empty()
                    && self.decisions.is_empty()
                    && self.decision_log.total == 0
                    && self.candidate_count == 0
                    && self.work.calls == 0
                    && self.work.input_tokens == 0
                    && self.model_work == LoopModelWork::default()
                    && self.partial.as_ref().is_some_and(|partial| partial.stage
                        == LoopStage::Admission
                        && partial.reason == PartialReason::Queued
                        && partial.part.is_none()
                        && partial.continuation.is_none()),
                "a ticket reply carries no work and is marked queued"
            );
            return Ok(());
        }
        ensure!(!self.parts.is_empty(), "Answer reply without parts");
        let early: BTreeSet<_> = self
            .parts
            .iter()
            .filter_map(|part| part_stop_reason(part.stop))
            .map(|reason| reason as u8)
            .collect();
        ensure!(
            self.parts.iter().any(|part| part.shortened) == (self.omitted_count > 0),
            "Answer shortened parts disagree with omissions"
        );
        match &self.partial {
            None => ensure!(
                early.is_empty() && self.omitted_count == 0,
                "an unfinished Answer must be marked partial"
            ),
            Some(partial) => {
                let late = matches!(partial.stage, LoopStage::Compose | LoopStage::Union);
                ensure!(
                    partial.stage != LoopStage::Admission
                        && match partial.reason {
                            PartialReason::Queued => false,
                            PartialReason::Bytes => self.omitted_count > 0,
                            reason @ (PartialReason::Deadline | PartialReason::Budget) if late => {
                                let _ = reason;
                                true
                            }
                            reason => early.contains(&(reason as u8)),
                        },
                    "Answer partial reason is not observed"
                );
                ensure!(
                    partial.part.is_none_or(|id| self.part(id).is_some()),
                    "Answer partial marker names an unknown part"
                );
            }
        }
        Ok(())
    }

    fn validate_applications(&self) -> Result<()> {
        let scope = &self.scope;
        let changed = self
            .degradations
            .iter()
            .any(|degradation| matches!(degradation, Degradation::ConfigurationChanged));
        for application in &self.applications {
            application.validate()?;
            let observed = &application.scope;
            ensure!(
                observed.installation == scope.installation
                    && observed.catalog == scope.catalog
                    && observed.index_id == scope.index_id
                    && observed.authorization == scope.authorization
                    && observed.publication == scope.publication
                    && (observed.configuration_revision == scope.configuration_revision || changed),
                "Answer application scope mismatch"
            );
        }
        Ok(())
    }

    /// Search-embedded Focus receipts, validated as /2 does.
    fn search_focus(&self) -> Result<focus::FocusWorkSummary> {
        let searches: Vec<EvidenceRecord> = self
            .records
            .iter()
            .filter(|record| record.operation == LoopRead::Search)
            .map(|record| EvidenceRecord {
                id: record.id,
                scope: self.scope.clone(),
                operation: NativeRead::Search,
                arguments: record.arguments.clone(),
                role: record.role,
                canonical_result: record.canonical_result.clone(),
            })
            .collect();
        focus::observed_focus_work(&searches, &focus::validation_budget())
    }

    fn validate_work(&self, budget: &BudgetV3) -> Result<()> {
        let mut calls = 0usize;
        let mut fanout = 0usize;
        let mut tokens = 0usize;
        for round in self.parts.iter().flat_map(|part| &part.rounds) {
            calls = calls
                .checked_add(round.calls)
                .ok_or_else(|| anyhow::anyhow!("Answer read accounting overflow"))?;
            fanout = fanout.max(round.fanout);
            tokens = tokens
                .checked_add(round.input_tokens)
                .ok_or_else(|| anyhow::anyhow!("Answer token accounting overflow"))?;
        }
        let before_rounds: usize = self
            .decision_log
            .attribution
            .iter()
            .filter(|charge| charge.round == 0)
            .map(|charge| charge.tokens)
            .sum();
        let required = tokens
            .checked_add(before_rounds)
            .ok_or_else(|| anyhow::anyhow!("Answer token accounting overflow"))?;
        let peak_rounds = self
            .parts
            .iter()
            .map(|part| part.rounds.len())
            .max()
            .unwrap_or(0);
        ensure!(
            self.work.calls == calls
                && self.work.fanout == fanout
                && self.work.rounds == peak_rounds,
            "Answer work differs from its rounds"
        );
        ensure!(
            self.work.input_tokens >= required,
            "Answer work does not retain its round, Split, Plan and Focus tokens"
        );
        // Summed model timings are disclosure: they overlap in batches and
        // across devices, so the deadline is judged on wall-clock elapsed.
        for (measured, retained) in self.decision_log.timings.pairs(&self.work) {
            if let Some(measured) = measured {
                ensure!(
                    retained.is_some_and(|retained| measured <= retained),
                    "Answer measured model timing not retained"
                );
            }
        }
        ensure!(
            self.elapsed_ms <= budget.deadline_ms.saturating_add(DEADLINE_TOLERANCE_MS),
            "Answer elapsed time exceeds its deadline and tolerance"
        );
        ensure!(
            self.work.calls <= budget.calls
                && self.work.rounds <= budget.rounds
                && self.work.fanout <= budget.fanout
                && self.work.input_tokens <= budget.input_tokens
                && self.work.candidates <= budget.candidates
                && self.candidate_count <= budget.candidates
                && self.candidate_count <= self.work.calls
                && self.work.bytes <= budget.max_bytes,
            "Answer original work budget exceeded"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
