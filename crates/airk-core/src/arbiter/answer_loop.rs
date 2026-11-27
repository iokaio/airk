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
/// Sub-needs the kernel derives for one part, and the bound of the budget's
/// `max_sub_needs`. A reply with several parts holds at most this many each.
/// The name and value stay as the card policy sources use them: those
/// sources are hashed into every model profile's task policy identity.
pub const MAX_SUB_NEEDS: usize = 4;
/// Caller inputs of a request (CONTRACT "Optional caller inputs"). The single
/// part of a request with caller `needs` holds up to this many sub-needs.
pub const MAX_NEEDS: usize = 8;
pub const MAX_NEED_BYTES: usize = 400;
pub const MAX_ANCHORS: usize = 16;
pub const MAX_ANCHOR_BYTES: usize = 200;
pub const MAX_KNOWN: usize = 32;
pub const MAX_ROUNDS: usize = 3;
/// Round one of a sub-need: its search, then symbols, references, imports or
/// outline, the governing CONTRACT.md or README, and tests.
pub const FIRST_ROUND_READS_PER_SUB_NEED: usize = 6;
/// Each later round runs one targeted query or structural hop per gap.
pub const FOLLOW_UP_READS_PER_SUB_NEED_ROUND: usize = 1;
/// The mandatory search of a part's own text, in its first round.
pub const PART_SEARCHES: usize = 1;
/// Sub-needs the kernel adds by the request's `deliverable` beside the
/// caller's or the derived ones (kernel CONTRACT "Gathering rules"): project
/// rules, the test command, the build file and, for a code change, the error
/// path. They hold reply ids after the others, in a one-part reply only.
pub const MAX_STANDING_NEEDS: usize = 4;
/// Round-one searches of one sub-need beyond its own recipe: its source-code
/// lane and, for a standing sub-need, its configuration lane.
pub const SEARCH_LANES_PER_SUB_NEED: usize = 2;
/// Round-one reads of one part beyond its sub-needs' own recipes: project
/// guidance (listings and reads), files the evidence cites, and structure
/// following (private code-fact and reference lookups and their excerpts).
pub const GATHERING_READS_PER_PART: usize = 48;
/// A full loop: round one 4 x (1 + 4 x 6) = 100, later rounds 4 x 2 x 4 = 32.
pub const DEFAULT_CALLS: usize = 132;
pub const MAX_CALLS: usize = 160;
/// One candidate record per canonical read, before dedup.
pub const DEFAULT_CANDIDATES: usize = DEFAULT_CALLS;
pub const MAX_CANDIDATES: usize = MAX_CALLS;
pub const MAX_FANOUT: usize = 12;
pub const DEFAULT_FANOUT: usize = 8;
/// Model input tokens over the whole attempt: the user's decision of 23
/// November 2026 (change log entry 50). A four-part Answer gets about 95,000
/// a part, above the about 84,900 a part spends over three rounds with
/// evidence-carrying card calls, so it keeps its Context model through its
/// last round. Raised from 262,144.
pub const MAX_INPUT_TOKENS: usize = 380_000;
/// The default is the whole pool. It was 196,608 (two parts through three
/// rounds, change log entry 44) and 131,072 before that.
pub const DEFAULT_INPUT_TOKENS: usize = MAX_INPUT_TOKENS;
/// Model input tokens of one round of one part. At the gate's settings a
/// round with four open sub-needs spends about 30,200 of them: 16 Focus
/// pairs at 1,024 tokens, two batched Coverage calls and two batched
/// Follow-up calls with evidence (change log entry 44).
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
/// and round: 1 + 4 x (1 + 3 x 4 x 2) = 101. One part of eight caller needs
/// takes 1 + 1 + 3 x 8 x 2 = 50, inside the same bound.
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
    /// What the caller must find out, one fact each. When present these are
    /// the request's sub-needs, in this order, and the kernel derives none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    /// Exact strings from the caller's task. Each gets an exact lookup in
    /// round one. An anchor is data, never a pattern.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliverable: Option<Deliverable>,
    /// Source the caller already holds. Evidence wholly inside a known range
    /// is withheld and counted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<KnownRange>,
}

/// What the caller will produce from the gathered context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Deliverable {
    CodeChange,
    Test,
    Document,
    Analysis,
}

/// A repository-relative path, whole or with inclusive one-based lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnownRange {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}
impl KnownRange {
    fn validate(&self) -> Result<()> {
        ensure!(
            crate::index_catalog::relative_path(&self.path, false).is_ok()
                && match (self.start_line, self.end_line) {
                    (None, None) => true,
                    (Some(start), Some(end)) => start > 0 && start <= end,
                    _ => false,
                },
            "invalid Answer known range"
        );
        Ok(())
    }
}

fn caller_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
}

impl ContextRequestV3 {
    /// A request for `question` with no caller inputs.
    pub fn new(question: impl Into<String>, budget: BudgetV3) -> Self {
        Self {
            schema: REQUEST_SCHEMA_V3.into(),
            question: question.into(),
            budget,
            continuation: None,
            needs: Vec::new(),
            anchors: Vec::new(),
            deliverable: None,
            known: Vec::new(),
        }
    }
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
        ensure!(
            self.needs.len() <= MAX_NEEDS
                && self
                    .needs
                    .iter()
                    .all(|need| caller_text(need, MAX_NEED_BYTES))
                && self
                    .needs
                    .iter()
                    .enumerate()
                    .all(|(index, need)| !self.needs[..index].contains(need)),
            "invalid Answer needs"
        );
        ensure!(
            self.anchors.len() <= MAX_ANCHORS
                && self
                    .anchors
                    .iter()
                    .all(|anchor| caller_text(anchor, MAX_ANCHOR_BYTES))
                && self
                    .anchors
                    .iter()
                    .enumerate()
                    .all(|(index, anchor)| !self.anchors[..index].contains(anchor)),
            "invalid Answer anchors"
        );
        ensure!(self.known.len() <= MAX_KNOWN, "invalid Answer known ranges");
        self.known.iter().try_for_each(KnownRange::validate)?;
        Ok(())
    }
}

/// The public Answer call (CONTRACT "Public Answer call", user decision of
/// 25 November 2026): `task` alone, or the existing full request with
/// `schema`, `question` and `budget`. Every reader converts it with
/// [`AnswerCallV3::into_request`], so both forms give one canonical request.
/// The field comments are the advertised argument descriptions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerCallV3 {
    /// The whole task in your words; it becomes the question.
    // Absent is None without `default`, which would hide `required`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(required, with = "String")]
    pub task: Option<String>,
    /// Existing full request only; supplied by the service for task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub schema: Option<String>,
    /// Existing full request only, with schema and budget; never with task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub question: Option<String>,
    /// Optional with task: omitted uses the service default, a valid budget
    /// is used as sent. Required with question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "BudgetV3")]
    pub budget: Option<BudgetV3>,
    /// A ticket or continuation id, to attach to an existing job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<Sha256Digest>,
    /// Up to 8 statements of what you must learn, one each, at most 400
    /// bytes. Stating needs improves the result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    /// Up to 16 exact strings from the task (identifiers, routes, quoted
    /// text, numbers with units), at most 200 bytes each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    /// What you will produce. Checked; does not change gathering yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliverable: Option<Deliverable>,
    /// Up to 32 path ranges you already read. Checked; does not change
    /// gathering yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<KnownRange>,
}

/// Why a public Answer call has no canonical request. Each refusal carries a
/// stable code at the start of its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerCallError {
    /// `task` and `question` together.
    TaskAndQuestion,
    /// Neither `task` nor `question`.
    TaskRequired,
    /// The existing full request without `schema`.
    MissingSchema,
    /// The existing full request without `budget`.
    MissingBudget,
}
impl AnswerCallError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TaskAndQuestion => "answer_task_and_question",
            Self::TaskRequired => "answer_task_required",
            Self::MissingSchema | Self::MissingBudget => "answer_request_incomplete",
        }
    }
}
impl std::fmt::Display for AnswerCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = match self {
            Self::TaskAndQuestion => "send task or question, not both",
            Self::TaskRequired => "send task, or the full schema, question and budget request",
            // The existing full request keeps its decoding refusal text.
            Self::MissingSchema => "missing field `schema`",
            Self::MissingBudget => "missing field `budget`",
        };
        write!(formatter, "{}: {detail}", self.code())
    }
}
impl std::error::Error for AnswerCallError {}

impl AnswerCallV3 {
    /// The one conversion of a public Answer call to its canonical request.
    /// A `task` becomes the question verbatim, with the request schema and,
    /// when omitted, the complete default budget. The existing full request
    /// passes through unchanged. Bounds are checked by
    /// [`ContextRequestV3::validate`], as for every request.
    pub fn into_request(self) -> std::result::Result<ContextRequestV3, AnswerCallError> {
        let (schema, question, budget) = match (self.task, self.question) {
            (Some(_), Some(_)) => return Err(AnswerCallError::TaskAndQuestion),
            (None, None) => return Err(AnswerCallError::TaskRequired),
            (Some(task), None) => (
                self.schema.unwrap_or_else(|| REQUEST_SCHEMA_V3.into()),
                task,
                self.budget.unwrap_or_default(),
            ),
            (None, Some(question)) => (
                self.schema.ok_or(AnswerCallError::MissingSchema)?,
                question,
                self.budget.ok_or(AnswerCallError::MissingBudget)?,
            ),
        };
        Ok(ContextRequestV3 {
            schema,
            question,
            budget,
            continuation: self.continuation,
            needs: self.needs,
            anchors: self.anchors,
            deliverable: self.deliverable,
            known: self.known,
        })
    }
}
/// The longest Answer invocation key.
pub const MAX_INVOCATION_KEY_BYTES: usize = 128;

/// Job options beside an Answer /3 request, never inside it, so the request
/// digest is unchanged (CONTRACT "Answer job options and delivery record").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerJobOptions {
    /// Start an independent run; never reuse a job by digest.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fresh: bool,
    /// Exact retry recovery within the caller's own scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_key: Option<String>,
    /// How long this call waits; never shortens the job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
    /// The job may run to completion with no caller waiting. It never ranks
    /// a job ahead of an attended one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interactive: bool,
}
impl AnswerJobOptions {
    pub fn validate(&self, budget: &BudgetV3) -> Result<()> {
        if let Some(key) = &self.invocation_key {
            ensure!(
                (1..=MAX_INVOCATION_KEY_BYTES).contains(&key.len())
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)),
                "invalid Answer invocation key"
            );
        }
        if let Some(wait) = self.wait_ms {
            ensure!(
                (1..=budget.deadline_ms).contains(&wait),
                "invalid Answer wait"
            );
        }
        Ok(())
    }
}

/// What one delivery of a service-owned Answer job was: beside the reply,
/// never inside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerDeliveryRecord {
    /// The job's handle; it grants nothing.
    pub job: Sha256Digest,
    /// Submission to dispatch.
    pub queue_ms: u64,
    /// Dispatch to finish, or to now while the job runs.
    pub run_ms: u64,
    /// This call's own wait; equal to the reply's `elapsed_ms`.
    pub wait_ms: u64,
    /// This call did not make the work it received.
    pub reused: bool,
}
impl AnswerDeliveryRecord {
    pub fn validate(&self) -> Result<()> {
        let window = MAX_DEADLINE_MS.saturating_add(DEADLINE_TOLERANCE_MS);
        ensure!(
            self.queue_ms <= MAX_ESTIMATE_MS && self.run_ms <= window && self.wait_ms <= window,
            "invalid Answer delivery record"
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
    /// Who stated this sub-need. Absent means the kernel derived it.
    #[serde(default, skip_serializing_if = "SubNeedOrigin::is_derived")]
    pub origin: SubNeedOrigin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubNeedOrigin {
    /// The kernel derived it from the question.
    #[default]
    Derived,
    /// The caller stated it in the request's `needs`.
    Caller,
    /// The kernel added it by the request's `deliverable`: a need every piece
    /// of work of that kind has and callers seldom state.
    Standing,
}
impl SubNeedOrigin {
    pub fn is_derived(&self) -> bool {
        *self == Self::Derived
    }
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
    /// Spans of the retained records whose text the reply carries once, in
    /// another span: exact repeats, and spans that lie within a longer span
    /// of the same file. Each such span names the one that carries its text
    /// (`same_as`). Absent when there is none.
    #[serde(default, skip_serializing_if = "no_spans")]
    pub repeats_merged: usize,
    /// Spans trimmed of text that another span of the same file carries
    /// (`overlaps`). Absent when there is none.
    #[serde(default, skip_serializing_if = "no_spans")]
    pub overlaps_trimmed: usize,
}
fn no_spans(count: &usize) -> bool {
    *count == 0
}
/// The most spans one reply can merge or trim: every hit of every read.
pub const MAX_MERGED_SPANS: usize = 4096;

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
    /// No Context model decided: no card call was dispatched, and every card
    /// decision took the deterministic order.
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
    /// A stage's model did not run: `applied` is absent, the no-model loop.
    /// (A step down to a smaller model left with the Large model; the field
    /// stays so the wire shape is unchanged.)
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

/// A gathering rule of the kernel's Answer loop (kernel CONTRACT "Gathering
/// rules"), named in a reply's `gathering` when it was on for the request.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatheringRule {
    SearchLanes,
    StandingNeeds,
    ProjectGuidance,
    CitedFiles,
    StructureFollowing,
    EveryPassageToEveryNeed,
    DeliveryByNeed,
}

/// A stage of the kernel's gathering, as `gathering` reports it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatheringStageKind {
    /// Round one's searches and their recipe reads.
    Retrieve,
    /// Project guidance files: folder listings and reads.
    Guidance,
    /// Files the evidence names by path.
    Cited,
    /// Enclosing definitions, callees, call sites and users of inherited types.
    Structure,
    /// Round barriers: Focus, coverage and follow-up choices.
    Coverage,
    /// The reads of rounds two and later.
    FollowUp,
    /// Choosing each sub-need's passages, Compose and Union.
    Delivery,
}

/// Reads of one operation dispatched in a stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatheringReads {
    pub operation: LoopRead,
    pub count: usize,
}

/// What one gathering stage did and how long it took.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatheringStage {
    pub stage: GatheringStageKind,
    /// Evidence reads admitted in this stage, by operation.
    pub reads: Vec<GatheringReads>,
    /// Private index lookups charged as calls (code facts, listings,
    /// references); they choose reads and are never evidence themselves.
    pub lookups: usize,
    /// Hits of the records this stage retained.
    pub passages_fetched: usize,
    /// (sub-need, passage) pairs this stage put before a sub-need's ranking.
    pub passages_offered: usize,
    /// Passages from this stage's records the reply delivers to a sub-need.
    pub passages_delivered: usize,
    /// Focus pairs and card decisions taken in this stage.
    pub model_judgments: usize,
    /// Wall-clock time spent in this stage.
    pub elapsed_ms: u64,
}

/// Per-stage counts and timings of one Answer's gathering. Disclosure only:
/// no record id, digest or decision binds it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatheringReport {
    /// The gathering rules that were on for this request.
    pub rules: Vec<GatheringRule>,
    pub stages: Vec<GatheringStage>,
}
/// The most stages one report names: one per stage kind.
pub const MAX_GATHERING_STAGES: usize = 7;

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
    /// The per-read status (freshness and readiness observations) of the
    /// mandatory first search, carried once for the reply. Records never
    /// repeat it and no record id or digest binds it (change-log entry 54).
    /// Absent when the read carried none, or when the transport renders the
    /// reply without status (terse, `include_status` false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_status: Option<serde_json::Map<String, serde_json::Value>>,
    /// Per-stage counts and timings of the kernel's gathering. Absent on a
    /// ticket reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gathering: Option<GatheringReport>,
}

fn nullable_partial(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<PartialMarker>>()
}
fn nullable_ticket(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<Option<AnswerTicket>>()
}

/// Top-level fields of a canonical read result that are per-read status,
/// not evidence: they vary between two runs of one read at one publication
/// (the request identity, and the freshness and readiness observations the
/// transport attaches after the read). A loop record's id, `full_bytes`,
/// `full_digest` and retention accounting bind the result without them;
/// custody keeps the result as received (change-log entry 54).
pub const VOLATILE_READ_FIELDS: [&str; 6] = [
    "request_id",
    "freshness",
    "discovery_readiness",
    "semantic_readiness",
    "discovery_observation",
    "indexing",
];

/// The volatile fields a reply's one `read_status` may carry: every
/// volatile field except the inner read's own request identity.
pub const READ_STATUS_FIELDS: [&str; 5] = [
    "freshness",
    "discovery_readiness",
    "semantic_readiness",
    "discovery_observation",
    "indexing",
];

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
        self.validate_gathering(budget)?;
        ensure!(
            self.read_status.as_ref().is_none_or(|status| {
                !status.is_empty()
                    && status
                        .keys()
                        .all(|field| READ_STATUS_FIELDS.contains(&field.as_str()))
            }),
            "Answer read status carries a field that is not per-read status"
        );
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
                // No card call was dispatched: every decision is the
                // no-model path's or one whose call was not dispatched.
                Degradation::ContextAbsent => {
                    ensure!(
                        summary.count(None, SourceKind::Deterministic)
                            + summary.count(None, SourceKind::NotDispatched)
                            == summary.total,
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
                    // Each stage has one model, so the only step left is to
                    // no model (the no-model loop).
                    ensure!(
                        requested.stage() == *stage && applied.is_none(),
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
            // A part's own sub-needs are all derived or all the caller's.
            // Caller needs fill the single part of their request, up to
            // MAX_NEEDS; derived ones keep the budget's limit. Standing
            // sub-needs follow them, at most MAX_STANDING_NEEDS, in a
            // one-part reply only.
            let count = |origin: SubNeedOrigin| {
                part.sub_needs
                    .iter()
                    .filter(|need| need.origin == origin)
                    .count()
            };
            let stated = count(SubNeedOrigin::Caller);
            let standing = count(SubNeedOrigin::Standing);
            let own = part.sub_needs.len() - standing;
            ensure!(
                stated == 0 || stated == own,
                "Answer sub-need origins are mixed"
            );
            ensure!(
                standing <= MAX_STANDING_NEEDS
                    && (standing == 0 || self.parts.len() == 1)
                    && part.sub_needs[own..]
                        .iter()
                        .all(|need| need.origin == SubNeedOrigin::Standing),
                "Answer standing sub-needs follow the others, in a one-part reply"
            );
            ensure!(
                own > 0
                    && if stated == 0 {
                        own <= effective.sub_needs
                    } else {
                        self.parts.len() == 1 && own <= MAX_NEEDS
                    },
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
                // Round one also holds one exact lookup per caller anchor. A
                // reply does not carry its request, so the bound is the most
                // anchors a request may state. The gathering rules add each
                // sub-need's search lanes and the part's guidance, cited-file
                // and structure reads (kernel CONTRACT "Gathering rules").
                PART_SEARCHES
                    + MAX_ANCHORS
                    + GATHERING_READS_PER_PART
                    + needs * (FIRST_ROUND_READS_PER_SUB_NEED + SEARCH_LANES_PER_SUB_NEED)
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
                "Answer round token allowance exceeded: {} of {}",
                round.input_tokens,
                budget.round_input_tokens
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
            self.union.repeats_merged <= MAX_MERGED_SPANS
                && self.union.overlaps_trimmed <= MAX_MERGED_SPANS
                && (!self.records.is_empty()
                    || self.union.repeats_merged + self.union.overlaps_trimmed == 0),
            "Answer merged spans without records, or beyond every read's hits"
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
        // Decisions whose card call was dispatched: neither the no-model path
        // nor a call that was not dispatched holds a card slot.
        let scored_total = summary.total
            - summary.count(None, SourceKind::Deterministic)
            - summary.count(None, SourceKind::NotDispatched);
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
            "decision log card calls disagree with scored decisions or slots: {} card calls, {} reply slots, {scored_total} scored decisions, {} allowed",
            summary.card_calls,
            slots.len(),
            budget.context_calls
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
        // Focus pairs are pairs dispatched (log.rs), so every call has one.
        let (calls, pairs) = (summary.focus_calls, summary.focus_pairs);
        ensure!(
            calls <= budget.focus_calls,
            "Answer Focus work exceeds its budget: {calls} Focus calls of {}",
            budget.focus_calls
        );
        ensure!(
            pairs <= budget.focus_pairs,
            "Answer Focus work exceeds its budget: {pairs} Focus pairs of {}",
            budget.focus_pairs
        );
        ensure!(
            calls <= pairs,
            "Answer Focus work exceeds its budget: {calls} Focus calls dispatched only {pairs} pairs"
        );
        ensure!(
            pairs <= calls * MAX_FOCUS_BATCH_PAIRS,
            "Answer Focus work exceeds its budget: {pairs} Focus pairs over {calls} calls of at most {MAX_FOCUS_BATCH_PAIRS}"
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
            "Answer work does not retain its round, Split, Plan and Focus tokens: {} retained, {required} charged",
            self.work.input_tokens
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
            "Answer original work budget exceeded: calls {}/{}, rounds {}/{}, fanout {}/{}, input tokens {}/{}, work candidates {}/{}, candidate count {} (reads {}), bytes {}/{}",
            self.work.calls,
            budget.calls,
            self.work.rounds,
            budget.rounds,
            self.work.fanout,
            budget.fanout,
            self.work.input_tokens,
            budget.input_tokens,
            self.work.candidates,
            budget.candidates,
            self.candidate_count,
            self.work.calls,
            self.work.bytes,
            budget.max_bytes
        );
        Ok(())
    }

    /// A gathering report names each rule and stage once, and its reads and
    /// lookups are exactly the reply's calls. Its stages are disjoint spans
    /// of the one run, so together they take at most the deadline and its
    /// tolerance. They are not compared with the reply's `elapsed_ms`, which
    /// a service sets to the caller's own wait when a call attaches to a job
    /// that was already running.
    fn validate_gathering(&self, budget: &BudgetV3) -> Result<()> {
        let Some(report) = &self.gathering else {
            return Ok(());
        };
        ensure!(
            self.ticket.is_none(),
            "a ticket reply carries no gathering report"
        );
        ensure!(
            unique(&report.rules)
                && report.stages.len() <= MAX_GATHERING_STAGES
                && unique(report.stages.iter().map(|stage| stage.stage)),
            "Answer gathering report names a rule or stage twice"
        );
        let mut calls = 0usize;
        let mut elapsed = 0u64;
        for stage in &report.stages {
            ensure!(
                unique(stage.reads.iter().map(|reads| reads.operation))
                    && stage.reads.iter().all(|reads| reads.count > 0),
                "Answer gathering stage reads are not one count per operation"
            );
            elapsed = elapsed.saturating_add(stage.elapsed_ms);
            calls = stage
                .reads
                .iter()
                .map(|reads| reads.count)
                .chain([stage.lookups])
                .try_fold(calls, usize::checked_add)
                .ok_or_else(|| anyhow::anyhow!("Answer gathering accounting overflow"))?;
        }
        ensure!(
            elapsed <= budget.deadline_ms.saturating_add(DEADLINE_TOLERANCE_MS),
            "Answer gathering stages outlast the deadline and its tolerance"
        );
        ensure!(
            calls == self.work.calls,
            "Answer gathering reads and lookups differ from the reply's calls: {calls} of {}",
            self.work.calls
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
