// SPDX-License-Identifier: Apache-2.0
//! Answer /3: Split, then per part Plan, Retrieve, Read and check coverage,
//! Follow up and Compose, then Union (final plan section 12). Every read is a
//! canonical dispatch through the one shared read owner (`gather`); models
//! only choose among cards deterministic code proposes.
//!
//! The parts run in lockstep rounds. Within a round, reads go out in waves:
//! a wave's reads run concurrently (at most four in flight, `wave`), then
//! are observed in dispatch order, and the
//! structural reads they plan join the next wave. Decisions are taken only
//! at a round barrier, from state ordered by dispatch, so neither read
//! completion order nor concurrency can change the reply.
use super::*;
use airk_core::arbiter::answer_loop as v3;
use airk_core::native_loop::LoopCard;
use gather::{Cause, Gathering, GlobalStop, Observed, Offer, ReadKey, Step};
use retrieve::Probe;
use state::{NeedState, PartState, RoundState};
use std::collections::{BTreeMap, VecDeque};

mod admission;
pub use admission::AdmissionPlan;
mod compose;
mod coverage;
mod decide;
mod deliver;
mod extend;
mod follow;
mod gather;
mod model;
mod nominate;
mod plan;
mod repeats;
mod report;
mod retrieve;
mod rules;
pub use rules::GatheringRules;
#[cfg(feature = "test-support")]
pub use rules::bind_gathering_rules;
mod shares;
mod split;
pub(crate) use split::independent_parts;
mod state;
mod union;
mod volatile;
mod wave;

/// A canonical read the loop may call from several threads.
pub type CanonicalReadFn<'a> =
    dyn Fn(&str, &Map<String, Value>, Option<&str>) -> Result<Value> + Sync + 'a;

/// One progress observation: a part reached a stage of a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnswerProgress {
    pub part: u8,
    pub parts: u8,
    pub round: u8,
    pub stage: v3::LoopStage,
    pub sub_needs: usize,
    pub covered: usize,
}

/// The caller's environment: the original deadline, the liveness fence,
/// the canonical reader, and an injectable clock and progress observer.
pub struct AnswerEnv<'a> {
    pub deadline: Instant,
    pub require_live: &'a (dyn Fn() -> Result<()> + Sync),
    pub reader: &'a CanonicalReadFn<'a>,
    pub clock: &'a (dyn Fn() -> Instant + Sync),
    pub progress: &'a (dyn Fn(AnswerProgress) + Sync),
}

/// A retained canonical read, intact. The reply carries its compact spans.
#[derive(Clone, Debug)]
pub struct CustodyRecord {
    /// Binds the dispatch key and `canonical_result` without its volatile
    /// per-read status (Core `VOLATILE_READ_FIELDS`, change-log entry 54).
    pub id: Sha256Digest,
    pub tool: String,
    pub arguments: Map<String, Value>,
    /// The complete result as received, status included.
    pub canonical_result: Value,
}

/// What stays in service custody beside the reply: the complete decision
/// log and every intact canonical result. The service keeps it in its
/// Answer job table, keyed by the log digest, for the result retention of
/// plan section 12.4.1 (L2b).
#[derive(Clone, Debug)]
pub struct AnswerCustody {
    pub log: v3::DecisionLog,
    pub records: Vec<CustodyRecord>,
    pub nominations: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct AnswerOutcome {
    pub response: v3::ContextResponseV3,
    pub custody: AnswerCustody,
}

#[derive(Clone, Debug)]
struct NominationObservation {
    ordinal: usize,
    tool: &'static str,
    arguments: Map<String, Value>,
    basis: Vec<Sha256Digest>,
    outcome: Value,
}

fn nomination_arguments(read: &nominate::Read) -> Result<(&'static str, Map<String, Value>)> {
    Ok(match read {
        nominate::Read::Literal { literal, prefix } => (
            "literal_scan",
            json!({"literal": literal, "prefix": prefix})
                .as_object()
                .context("literal scan arguments")?
                .clone(),
        ),
        nominate::Read::Canonical { tool, arguments } => (*tool, arguments.clone()),
    })
}

const SYMBOLS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// Round-one reads of one sub-need (search, symbols, references, imports or
/// outline, governing document, tests).
const FIRST_ROUND_READS: usize = v3::FIRST_ROUND_READS_PER_SUB_NEED;
const PROBES_PER_ANSWER: usize = 24;
/// Repeats of one canonical read whose owner answered "repeat the same
/// request" (`publication_busy` and its kin), each after the owner's own
/// delay of at most one second.
const BUSY_REPEATS: usize = 3;

/// The owner's delay before a canonical read may be repeated unchanged, if
/// its failure is a retryable `repeat_same_request`.
fn busy_retry(result: &Result<Value>) -> Option<Duration> {
    let failure = result
        .as_ref()
        .err()?
        .downcast_ref::<crate::workbench::ReadFailure>()?;
    let body = match failure
        .0
        .get("structuredContent")
        .filter(|body| body.is_object())
    {
        Some(body) => body.clone(),
        None => failure
            .0
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_else(|| failure.0.clone()),
    };
    let hint = body
        .get("error")
        .filter(|error| error.is_object())
        .unwrap_or(&body);
    (hint["action"] == "repeat_same_request" && hint["retryable"] == true)
        .then(|| hint["retry_after_ms"].as_u64())
        .flatten()
        .filter(|delay| (1..=1000).contains(delay))
        .map(Duration::from_millis)
}

impl crate::Kernel {
    /// Answer /3 with the system clock and no progress observer.
    pub fn answer(
        &self,
        request: v3::ContextRequestV3,
        deadline: Instant,
        require_live: &(dyn Fn() -> Result<()> + Sync),
        reader: &CanonicalReadFn<'_>,
    ) -> Result<AnswerOutcome> {
        self.answer_with(
            request,
            AnswerEnv {
                deadline,
                require_live,
                reader,
                clock: &Instant::now,
                progress: &|_| {},
            },
        )
    }

    /// Answer /3. The reply is validated against its custody log before it
    /// is returned, on every path.
    pub fn answer_with(
        &self,
        request: v3::ContextRequestV3,
        env: AnswerEnv<'_>,
    ) -> Result<AnswerOutcome> {
        self.answer_run(request, env, None)
    }

    /// Answer /3 under one set of gathering rules instead of the process's.
    #[cfg(test)]
    pub(crate) fn answer_with_rules(
        &self,
        request: v3::ContextRequestV3,
        env: AnswerEnv<'_>,
        rules: GatheringRules,
    ) -> Result<AnswerOutcome> {
        self.answer_ruled(request, env, None, rules)
    }

    /// The loop behind `answer_with` and `answer_admitted`; `plan` is the
    /// service's admission plan (`admission`), when there is one.
    fn answer_run(
        &self,
        request: v3::ContextRequestV3,
        env: AnswerEnv<'_>,
        plan: Option<&AdmissionPlan>,
    ) -> Result<AnswerOutcome> {
        self.answer_ruled(request, env, plan, rules::gathering_rules())
    }

    fn answer_ruled(
        &self,
        mut request: v3::ContextRequestV3,
        env: AnswerEnv<'_>,
        plan: Option<&AdmissionPlan>,
        wanted: GatheringRules,
    ) -> Result<AnswerOutcome> {
        request.validate()?;
        (env.require_live)()?;
        let started = (env.clock)();
        let context = self
            .arbiter_context
            .as_ref()
            .context("Answer requires a native admitted index connection")?;
        // A combined member never spends more than its allowance, whatever
        // budget its request names.
        if let Some(partition) = &context.answer_partition {
            ensure!(
                partition.index_id == context.authority.index_id,
                "Answer partition names another index"
            );
            request.budget = partition.clamp(&request.budget)?;
            request.validate()?;
        }
        // The service's admission plan bounds the loop from here on; the
        // reply is validated against the budget as asked.
        let admitted = admission::apply(plan, &mut request.budget)?;
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "Answer source authority expired"
        );
        let publication = self.arbiter_publication()?;
        let application = observed_application(context, &publication, None)?;
        let scope = application.scope.clone();
        ensure!(
            scope.index_id == context.authority.index_id
                && scope.authorization == context.authority.fingerprint
                && scope.publication == publication,
            "Answer owner supplied a foreign binding"
        );
        let deadline = env.deadline.min(
            started
                .checked_add(Duration::from_millis(request.budget.deadline_ms))
                .context("Answer deadline overflow")?,
        );
        ensure!(
            deadline > started,
            "Answer original deadline exhausted before dispatch"
        );
        self.bind_answer_loop(deadline)?;
        let gather = Gathering::new(request.budget.clone(), scope.clone())?;
        let publication_for_model = publication.clone();
        let mut run = Loop {
            kernel: self,
            context,
            env: &env,
            request: &request,
            scope,
            publication,
            application,
            started,
            deadline,
            gather,
            parts: vec![],
            shares: shares::Shares::allocate(&request.budget, 1, 0, 0),
            ledger: decide::Ledger::default(),
            configuration_changed: false,
            probes: vec![],
            probes_run: 0,
            nomination_queue: VecDeque::new(),
            nominations: BTreeMap::new(),
            governing: BTreeMap::new(),
            searched: BTreeSet::new(),
            waiting: BTreeMap::new(),
            used_calls: vec![],
            used_slots: vec![],
            used_pairs: vec![],
            rr: 0,
            model: context
                .owner
                .card_model(context.worker, context.authority, &publication_for_model)
                .ok()
                .flatten()
                .filter(|_| admitted.context_model),
            model_blocked: false,
            focus_blocked: false,
            cost: shares::CostModel::default(),
            card_slots: 0,
            focus_pairs: 0,
            used_tokens: BTreeMap::new(),
            unknown_tokens: BTreeMap::new(),
            model_share: vec![],
            batch_split: false,
            admitted,
            rules: GatheringRules::none(),
            wanted,
            report: report::Report::new(vec![]),
            lookups: BTreeMap::new(),
            gathered: vec![],
            extension: 0,
            stage_floor: 0,
            starved: None,
            idle: v3::GatheringSkip::NothingFound,
        };
        run.begin_stage(v3::GatheringStageKind::Retrieve);
        run.split()?;
        run.plan()?;
        run.rounds()?;
        run.finish()
    }
}

struct Loop<'a> {
    kernel: &'a crate::Kernel,
    context: &'a NativeContext,
    env: &'a AnswerEnv<'a>,
    request: &'a v3::ContextRequestV3,
    scope: ScopeBinding,
    publication: airk_core::evidence::RevisionEnvelope,
    application: IndexApplication,
    started: Instant,
    deadline: Instant,
    gather: Gathering,
    parts: Vec<PartState>,
    shares: shares::Shares,
    ledger: decide::Ledger,
    configuration_changed: bool,
    probes: Vec<Probe>,
    probes_run: usize,
    nomination_queue: VecDeque<nominate::Probe>,
    nominations: BTreeMap<ReadKey, NominationObservation>,
    governing: BTreeMap<(u8, u8), usize>,
    /// Search queries already dispatched, lower case.
    searched: BTreeSet<String>,
    /// Recipe steps waiting on a read another dispatch already queued.
    waiting: BTreeMap<ReadKey, Vec<(u8, u8, Step)>>,
    used_calls: Vec<usize>,
    /// Card slots charged to each part (by index) and Focus pairs it used.
    used_slots: Vec<usize>,
    used_pairs: Vec<usize>,
    rr: usize,
    model: Option<LoopModel>,
    model_blocked: bool,
    focus_blocked: bool,
    cost: shares::CostModel,
    card_slots: usize,
    focus_pairs: usize,
    /// Model tokens charged per part and round, reservations included.
    used_tokens: BTreeMap<(u8, u8), usize>,
    /// Reservations of calls whose outcome is unknown, still charged.
    unknown_tokens: BTreeMap<(u8, u8), usize>,
    model_share: Vec<(u8, v3::LoopStage)>,
    batch_split: bool,
    /// The budget as asked and what the service's admission plan took away.
    admitted: admission::Admitted,
    /// The gathering rules in force for this request (kernel CONTRACT
    /// "Gathering rules"): the bound switches when the request carries
    /// caller inputs that select them, otherwise none.
    rules: GatheringRules,
    /// The rules bound for the process (or given by a test).
    wanted: GatheringRules,
    report: report::Report,
    /// Private lookups of the gathering rules, kept in custody beside the
    /// nominations.
    lookups: BTreeMap<ReadKey, NominationObservation>,
    /// Gathering reads and lookups charged to each part (by index).
    gathered: Vec<usize>,
    /// The next gathering phase of round one (`extend`).
    extension: u8,
    /// Retention bytes the open gathering stage leaves free (its share).
    stage_floor: usize,
    /// What refused the open gathering stage a lookup or read, if anything.
    starved: Option<v3::GatheringSkip>,
    /// The open stage's reason if it reads nothing and was not starved.
    idle: v3::GatheringSkip,
}

struct Assessed {
    spans: Vec<coverage::Span>,
    coverage: v3::Coverage,
    evidence: Vec<usize>,
}

impl Loop<'_> {
    fn now(&self) -> Instant {
        (self.env.clock)()
    }
    fn elapsed_ms(&self) -> u64 {
        self.now()
            .saturating_duration_since(self.started)
            .as_millis() as u64
    }
    /// No read or model call starts inside the Compose, Union and dispatch reserve.
    fn stop_due(&self) -> bool {
        let reserve = Duration::from_millis(self.shares.reserve.total_ms());
        self.now() + reserve >= self.deadline
    }
    /// Model calls end before the Compose and Union reserve.
    fn model_deadline(&self) -> Instant {
        self.deadline
            .checked_sub(Duration::from_millis(self.shares.reserve.compose_union_ms))
            .unwrap_or(self.started)
            .max(self.started)
    }
    fn part_index(&self, id: u8) -> Option<usize> {
        self.parts.iter().position(|part| part.id == id)
    }
    /// Model tokens charged to a part over every round.
    fn part_tokens(&self, id: u8) -> usize {
        self.used_tokens
            .iter()
            .filter(|((part, _), _)| *part == id)
            .map(|(_, tokens)| *tokens)
            .sum()
    }
    fn progress(&self, part: usize, stage: v3::LoopStage) {
        let part = &self.parts[part];
        (self.env.progress)(AnswerProgress {
            part: part.id,
            parts: self.parts.len() as u8,
            round: part.round(),
            stage,
            sub_needs: part.needs.len(),
            covered: part.needs.iter().filter(|need| need.covered()).count(),
        });
    }

    /// The liveness, authority and configuration fences of every turn.
    fn fence(&mut self) -> Result<()> {
        (self.env.require_live)()?;
        ensure!(
            self.kernel.scoped_read_fingerprint()? == Some(self.context.authority.fingerprint),
            "Answer source authority expired"
        );
        let current =
            observed_application(self.context, &self.publication, Some(&self.application))?;
        let observed = &current.scope;
        ensure!(
            observed.installation == self.scope.installation
                && observed.catalog == self.scope.catalog
                && observed.index_id == self.scope.index_id
                && observed.authorization == self.scope.authorization
                && observed.publication == self.scope.publication,
            "Answer application authority changed"
        );
        if observed.configuration_revision != self.scope.configuration_revision
            && !self.configuration_changed
        {
            self.configuration_changed = true;
            self.ledger.configuration_changed();
            self.model_blocked = true;
            self.focus_blocked = true;
        }
        self.application = current;
        Ok(())
    }

    fn split(&mut self) -> Result<()> {
        self.fence()?;
        let question = self.request.question.as_str();
        if !self.request.needs.is_empty() {
            // Caller needs make the request one part, with no Split decision
            // (Core CONTRACT "Optional caller inputs").
            let span = split::part_span(question, (0, question.len()));
            self.parts
                .push(PartState::new(1, span, &question[span.0..span.1]));
            return self.cut_shares();
        }
        let plan = split::propose(question, self.request.budget.max_parts);
        let cards: Vec<LoopCard> = plan
            .cards
            .iter()
            .enumerate()
            .map(|(index, partition)| split::card(question, partition, SYMBOLS[index]))
            .collect();
        let choice = self.decide(
            vec![model::Seg {
                at: decide::At::split(),
                cards,
                subject: question.to_owned(),
                basis: vec![],
                spans: vec![],
                verdict: None,
                fallback: vec![plan.deterministic],
                rule: v3::DeterministicRule::ExplicitBreaks,
            }],
            1,
        )?;
        let chosen = choice[0]
            .chosen
            .first()
            .copied()
            .filter(|index| *index < plan.cards.len())
            .unwrap_or(plan.deterministic);
        for (index, range) in plan.cards[chosen].0.iter().enumerate() {
            let span = split::part_span(question, *range);
            self.parts.push(PartState::new(
                index as u8 + 1,
                span,
                &question[span.0..span.1],
            ));
        }
        self.cut_shares()
    }

    /// Cut the per-part shares once the parts are known.
    fn cut_shares(&mut self) -> Result<()> {
        // The Split call is charged before the shares are cut.
        let split_tokens: usize = self.used_tokens.values().sum();
        self.shares = shares::Shares::allocate(
            &self.request.budget,
            self.parts.len(),
            self.request
                .budget
                .context_calls
                .saturating_sub(self.card_slots),
            self.request
                .budget
                .input_tokens
                .saturating_sub(split_tokens),
        );
        self.used_calls = vec![0; self.parts.len()];
        self.used_slots = vec![0; self.parts.len()];
        self.used_pairs = vec![0; self.parts.len()];
        self.gathered = vec![0; self.parts.len()];
        for (part, share) in self.parts.iter_mut().zip(&self.shares.parts) {
            part.share_ms = share.deadline_ms;
        }
        Ok(())
    }

    /// The gathering rules apply to a one-part request whose caller inputs
    /// select them: caller needs, or a deliverable that brings standing
    /// sub-needs. Every other request keeps the loop's own gathering.
    fn choose_rules(&mut self) {
        let wanted = self.wanted;
        let caller = !self.request.needs.is_empty();
        let standing = !rules::standing(self.request.deliverable, caller).is_empty();
        if self.parts.len() == 1 && (caller || (standing && wanted.standing)) {
            self.rules = wanted;
        }
        // Rules 3 and 4 plan from what round one retained: its recipe reads
        // leave them, and the later rounds, half the retention bytes.
        if self.rules.guidance || self.rules.cited || self.rules.structure {
            self.gather.recipe_floor =
                Some(self.request.budget.max_bytes / extend::ROUND_ONE_KEEPS);
        }
        self.report.set_rules(self.rules.wire());
    }

    /// Rule 2: the standing sub-needs of the request's deliverable follow
    /// the part's own, as overview sub-needs searched in their own words.
    fn add_standing(&mut self, index: usize) {
        if !self.rules.standing || self.parts.len() != 1 {
            return;
        }
        let caller = !self.request.needs.is_empty();
        for which in rules::standing(self.request.deliverable, caller) {
            let part = &mut self.parts[index];
            let id = part.needs.len() as u8 + 1;
            part.needs.push(NeedState::standing(
                retrieve::Need {
                    id,
                    kind: retrieve::NeedKind::Overview,
                    subject: None,
                    query: which.text().to_owned(),
                },
                *which,
            ));
        }
    }

    fn plan(&mut self) -> Result<()> {
        let question = self.request.question.clone();
        self.choose_rules();
        for index in 0..self.parts.len() {
            if !self.request.needs.is_empty() {
                // The caller's statements are the sub-needs, in order; the
                // kernel proposes none and no Plan decision is taken.
                self.parts[index].needs = self
                    .request
                    .needs
                    .iter()
                    .enumerate()
                    .map(|(position, text)| {
                        NeedState::stated(retrieve::Need {
                            id: position as u8 + 1,
                            kind: retrieve::NeedKind::Overview,
                            subject: None,
                            query: text.clone(),
                        })
                    })
                    .collect();
                self.add_standing(index);
                self.progress(index, v3::LoopStage::Plan);
                continue;
            }
            let proposals = plan::propose(&self.parts[index].text, &question);
            let cards = plan::cards(&proposals);
            let fallback = plan::deterministic(&proposals, self.request.budget.max_sub_needs);
            let choice = self.decide(
                vec![model::Seg {
                    at: decide::At::plan(self.parts[index].id),
                    cards,
                    subject: self.parts[index].text.clone(),
                    basis: vec![],
                    spans: vec![],
                    verdict: None,
                    fallback: fallback.clone(),
                    rule: v3::DeterministicRule::EveryProposedNeed,
                }],
                1,
            )?;
            let mut chosen: Vec<usize> = choice[0]
                .chosen
                .iter()
                .copied()
                .filter(|index| *index < proposals.len())
                .take(self.request.budget.max_sub_needs)
                .collect();
            if chosen.is_empty() {
                chosen = fallback;
            }
            self.parts[index].needs = plan::needs(&proposals, &chosen)
                .into_iter()
                .map(NeedState::new)
                .collect();
            self.add_standing(index);
            self.progress(index, v3::LoopStage::Plan);
        }
        if self.gather.recipe_floor.is_some() {
            self.gather.recipe_calls = extend::reserved_calls(self.request, &self.parts);
        }
        Ok(())
    }

    /// Offer a read, within its sub-need's round-one cap.
    fn offer(&mut self, part: usize, dispatch: gather::Dispatch) -> Result<()> {
        self.offer_as(part, dispatch, true)
    }

    /// Offer a read; `capped` holds a round-one read to its sub-need's
    /// recipe cap. The gathering rules' reads are counted against the part's
    /// gathering room instead (`extend::gathering_room`).
    fn offer_as(&mut self, part: usize, dispatch: gather::Dispatch, capped: bool) -> Result<()> {
        let id = self.parts[part].id;
        if let Some(need) = dispatch.need {
            let state = self.parts[part]
                .need(need)
                .context("Answer read for an unknown sub-need")?;
            if capped && dispatch.round == 1 && state.reads >= FIRST_ROUND_READS {
                return Ok(());
            }
            state.tried.insert(dispatch.key);
        }
        if dispatch.tool == "search"
            && let Some(query) = dispatch.arguments.get("query").and_then(Value::as_str)
        {
            self.searched.insert(query.to_lowercase());
        }
        let (key, step, round, need) = (dispatch.key, dispatch.step, dispatch.round, dispatch.need);
        let mandatory = dispatch.cause == Cause::PartSearch;
        match self.gather.offer(dispatch) {
            // Only a new dispatch counts against the round-one cap.
            Offer::Queued => {
                if let Some(need) = need
                    && capped
                    && round == 1
                    && let Some(state) = self.parts[part].need(need)
                {
                    state.reads += 1;
                }
            }
            Offer::Waiting => {
                if let Some(need) = need
                    && round == 1
                    && step != Step::Leaf
                {
                    self.waiting.entry(key).or_default().push((id, need, step));
                }
            }
            Offer::Cached(record) => {
                if let Some(need) = need
                    && round == 1
                {
                    self.react(part, need, step, record)?;
                }
            }
            Offer::Failed(_) => {
                if mandatory && self.parts[part].active() {
                    self.stop_part(part, v3::PartStop::Refused, round);
                }
            }
            Offer::Unavailable => {}
        }
        Ok(())
    }

    /// The structural reads a round-one recipe step plans from its record.
    fn react(&mut self, part: usize, need: u8, step: Step, record: usize) -> Result<()> {
        // A stopped part plans nothing more: its reads would never run.
        if !self.parts[part].active() || step == Step::Leaf || self.gather.records[record].failure {
            return Ok(());
        }
        let Some(state) = self.parts[part]
            .needs
            .iter()
            .find(|state| state.need.id == need)
        else {
            return Ok(());
        };
        let target = state.need.clone();
        let record = self.gather.records[record].clone();
        for read in nominate::from_record(&target, step, &record) {
            self.nomination_queue.push_back(nominate::Probe {
                part: self.parts[part].id,
                need,
                basis: record.id,
                read,
            });
        }
        let reaction = retrieve::react(
            &mut self.gather,
            self.parts[part].id,
            &target,
            step,
            &record,
        )?;
        for read in reaction.reads {
            self.offer(part, read)?;
        }
        self.probes.extend(reaction.probes);
        Ok(())
    }

    /// Governing documents are read only after `file_info` shows them indexed.
    fn resolve_probes(&mut self) -> Result<()> {
        let shape = software::Shape::PRODUCT;
        for probe in std::mem::take(&mut self.probes) {
            let Some(part) = self
                .parts
                .iter()
                .position(|part| part.id == probe.part && part.active())
            else {
                continue;
            };
            let found = self
                .governing
                .get(&(probe.part, probe.need))
                .copied()
                .unwrap_or(0);
            if found >= shape.governing_documents
                || self.probes_run >= PROBES_PER_ANSWER
                || self.stop_due()
            {
                continue;
            }
            self.probes_run += 1;
            let arguments = json!({"path":probe.path})
                .as_object()
                .cloned()
                .unwrap_or_default();
            let Ok(info) = (self.env.reader)("file_info", &arguments, Some(&self.gather.revision))
            else {
                continue;
            };
            let lines = info
                .get("indexed_lines")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if token(&info) != Some(self.gather.revision.as_str()) || lines == 0 {
                continue;
            }
            let read = retrieve::governing_read(&mut self.gather, &probe, lines)?;
            if self.gather.known(&read.key) {
                continue;
            }
            *self.governing.entry((probe.part, probe.need)).or_default() += 1;
            self.offer(part, read)?;
        }
        Ok(())
    }

    /// Resolve private nominations. Their locations become evidence only
    /// through an ordinary hash-bound read on the same publication.
    fn resolve_nominations(&mut self) -> Result<()> {
        while let Some(probe) = self.nomination_queue.pop_front() {
            if self.gather.stop.is_some() || self.stop_due() {
                break;
            }
            let Some(part) = self
                .parts
                .iter()
                .position(|part| part.id == probe.part && part.active())
            else {
                continue;
            };
            let Some(state) = self.parts[part].need(probe.need) else {
                continue;
            };
            let need = state.need.clone();
            let room = FIRST_ROUND_READS.saturating_sub(state.reads);
            let part_calls = self.shares.parts[part]
                .calls
                .saturating_sub(self.used_calls[part]);
            if room == 0 || part_calls == 0 {
                continue;
            }
            let (tool, mut arguments) = nomination_arguments(&probe.read)?;
            let key = gather::read_key(tool, &arguments, &self.gather.revision)?;
            let observed = match self.nominations.get_mut(&key) {
                Some(observed) => {
                    if !observed.basis.contains(&probe.basis) {
                        observed.basis.push(probe.basis);
                    }
                    observed.clone()
                }
                None => {
                    if self.nominations.len() >= PROBES_PER_ANSWER
                        || room < 2
                        || part_calls < 2
                        || !self.gather.recipe_call_left()
                    {
                        continue;
                    }
                    // Under rules 3 and 4 a nomination, like a recipe read,
                    // leaves the recipe floor free.
                    let free = self
                        .gather
                        .remaining_bytes()
                        .saturating_sub(self.gather.recipe_floor.unwrap_or(0));
                    let byte_allowance = (free / 4).min(24_576);
                    if byte_allowance < 512 || !self.gather.reserve_nomination() {
                        continue;
                    }
                    self.report.lookup(self.gather.stage);
                    self.parts[part].need(probe.need).expect("known need").reads += 1;
                    self.used_calls[part] += 1;
                    let round = self.parts[part].current();
                    round.fanout = round.fanout.max(1);
                    round.calls += 1;
                    match probe.read {
                        nominate::Read::Literal { .. } => {
                            arguments.insert("max_files".into(), json!(16));
                            arguments.insert("max_bytes".into(), json!(byte_allowance));
                            arguments.insert("max_hits".into(), json!(8));
                        }
                        nominate::Read::Canonical { .. } => {
                            arguments.insert("max_bytes".into(), json!(byte_allowance.min(4_096)));
                        }
                    }
                    let outcome =
                        self.run_nomination(&probe.read, tool, &arguments, byte_allowance)?;
                    let observed = NominationObservation {
                        ordinal: self.nominations.len() + self.lookups.len() + 1,
                        tool,
                        arguments,
                        basis: vec![probe.basis],
                        outcome,
                    };
                    self.nominations.insert(key, observed.clone());
                    observed
                }
            };
            let Some(body) = observed.outcome.get("body") else {
                continue;
            };
            if tool == "code_facts" {
                for read in nominate::from_facts(&need, body) {
                    self.nomination_queue.push_back(nominate::Probe {
                        part: probe.part,
                        need: probe.need,
                        basis: key,
                        read,
                    });
                }
            }
            for location in nominate::locations(tool, body).into_iter().take(1) {
                let hash = location.known_hash.to_string();
                let read = retrieve::excerpt(
                    &mut self.gather,
                    (probe.part, Some(probe.need), 1),
                    (&location.path, location.start, location.end, Some(&hash)),
                    150,
                    (software::FollowUp::Excerpt, Step::Leaf, Cause::Structural),
                    &location.path,
                )?;
                self.offer(part, read)?;
            }
        }
        Ok(())
    }

    fn run_nomination(
        &mut self,
        read: &nominate::Read,
        tool: &str,
        arguments: &Map<String, Value>,
        byte_allowance: usize,
    ) -> Result<Value> {
        use crate::current::literal::{LiteralLimits, LiteralStop};

        (self.env.require_live)()?;
        let revision = self.gather.revision.as_str();
        let observed = match read {
            nominate::Read::Literal { literal, prefix } => self
                .kernel
                .scan_retained_literal(
                    literal,
                    prefix,
                    revision,
                    LiteralLimits {
                        max_files: 16,
                        max_bytes: byte_allowance,
                        max_hits: 8,
                    },
                    || {
                        (self.env.require_live)()?;
                        Ok(!self.stop_due())
                    },
                )
                .map(|scan| {
                    let matches: Vec<_> = scan
                        .hits
                        .iter()
                        .map(|hit| {
                            json!({"citation":{"path":hit.path,"start_line":hit.start_line,
                                "end_line":hit.end_line,"content_hash":hit.content_hash},
                                "start_byte":hit.start_byte,"end_byte":hit.end_byte})
                        })
                        .collect();
                    (
                        json!({"matches": matches, "files_examined": scan.files_examined,
                            "bytes_examined": scan.bytes_examined, "files_excluded": scan.files_excluded,
                            "retained_bytes_loaded": scan.retained_bytes_loaded,
                            "stop": format!("{:?}", scan.stop)}),
                        scan.retained_bytes_loaded,
                        scan.stop == LiteralStop::Stale,
                    )
                }),
            nominate::Read::Canonical { .. } => {
                (self.env.reader)(tool, arguments, Some(revision))
                    .map(|body| {
                        let stale = token(&body) != Some(revision);
                        (body, 0, stale)
                    })
            }
        };
        let (body, scanned, stale) = match observed {
            Ok(result) => result,
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            Err(_) => return Ok(json!({"disposition": "unavailable"})),
        };
        if stale {
            self.gather.stop = Some(GlobalStop::Stale);
            return Ok(json!({"disposition": "stale"}));
        }
        let retained = serde_json::to_vec(&body)?.len();
        let allowance = self.gather.remaining_bytes();
        let cost = scanned.saturating_add(retained);
        self.gather.charge_nomination_bytes(cost.min(allowance));
        if cost > allowance {
            return Ok(json!({"disposition": "retention_limit", "scanned_bytes": scanned}));
        }
        Ok(json!({"disposition": "observed", "body": body, "charged_bytes": cost}))
    }

    fn rounds(&mut self) -> Result<()> {
        for part in 0..self.parts.len() {
            self.parts[part].rounds.push(RoundState::default());
            let id = self.parts[part].id;
            let text = self.parts[part].text.clone();
            let search = retrieve::part_search(&mut self.gather, id, &text)?;
            self.offer(part, search)?;
            // Every caller anchor gets an exact lookup in round one.
            for anchor in &self.request.anchors {
                let lookup = retrieve::anchor_search(&mut self.gather, id, anchor)?;
                self.offer(part, lookup)?;
            }
            let needs: Vec<_> = self.parts[part]
                .needs
                .iter()
                .map(|state| (state.need.clone(), state.standing))
                .collect();
            for (need, standing) in needs {
                for read in retrieve::initial(&mut self.gather, id, &need)? {
                    self.offer(part, read)?;
                }
                // Rule 1: the same search on source code only, and for a
                // standing sub-need about tests or builds on configuration
                // only, beside the sub-need's recipe.
                if self.rules.lanes {
                    use airk_core::search_filters::EvidenceKind;
                    let lanes = [
                        Some(EvidenceKind::Code),
                        standing
                            .filter(|which| which.configuration_lane())
                            .map(|_| EvidenceKind::Configuration),
                    ];
                    for kind in lanes.into_iter().flatten() {
                        let read = retrieve::lane_search(&mut self.gather, id, &need, kind)?;
                        self.offer_as(part, read, false)?;
                    }
                }
            }
        }
        let mut round = 1u8;
        loop {
            if round > 1 {
                self.begin_stage(v3::GatheringStageKind::FollowUp);
            }
            if let Some(stop) = self.gather_round(round)? {
                self.stop_all(stop, round);
                break;
            }
            self.begin_stage(v3::GatheringStageKind::Coverage);
            self.barrier(round)?;
            if !self.parts.iter().any(PartState::active) {
                break;
            }
            round += 1;
        }
        Ok(())
    }

    /// Run a round's reads in waves until no active part has a queued read.
    fn gather_round(&mut self, round: u8) -> Result<Option<GlobalStop>> {
        loop {
            self.fence()?;
            if let Some(stop) = self.gather.stop {
                return Ok(Some(stop));
            }
            if self.stop_due() {
                return Ok(Some(GlobalStop::Deadline));
            }
            let mut wave = Vec::new();
            let count = self.parts.len();
            let before: Vec<usize> = self
                .parts
                .iter()
                .map(|part| part.rounds.last().map_or(0, |round| round.fanout))
                .collect();
            for offset in 0..count {
                let part = (self.rr + offset) % count;
                if !self.parts[part].active() {
                    continue;
                }
                let left = self.shares.parts[part]
                    .calls
                    .saturating_sub(self.used_calls[part]);
                let limit = self.request.budget.fanout.min(left);
                if limit == 0 && self.gather.queued_for(self.parts[part].id) > 0 {
                    self.stop_part(part, v3::PartStop::Budget, round);
                    continue;
                }
                let taken = self.gather.take(self.parts[part].id, limit);
                self.used_calls[part] += taken.len();
                self.parts[part].current().calls += taken.len();
                for read in &taken {
                    if read.cause != Cause::FollowUp {
                        continue;
                    }
                    let wire = self.parts[part]
                        .chosen
                        .iter()
                        .find(|(_, candidate)| candidate.key == read.key)
                        .map(|(_, candidate)| candidate.wire());
                    if let Some(wire) = wire {
                        self.parts[part].current().follow_ups_run.push(wire);
                    }
                }
                wave.extend(taken);
            }
            self.rr += 1;
            if wave.is_empty() {
                // Round one's searches and recipes have run: the gathering
                // rules' next phase plans from what they retained.
                if round == 1 && self.gather.stop.is_none() && self.extend()? {
                    continue;
                }
                return Ok(self.gather.stop);
            }
            wave.sort_by_key(|read| read.seq);
            let results = self.read_wave(&wave)?;
            // Reads the reserve stopped before they started go back unrun.
            for read in wave[results.len()..].iter().rev() {
                self.gather.release(read.seq);
                if let Some(part) = self.parts.iter().position(|part| part.id == read.part) {
                    self.used_calls[part] -= 1;
                    let state = self.parts[part].current();
                    state.calls -= 1;
                    state.follow_ups_run.retain(|item| {
                        item.target != read.target() || read.cause != Cause::FollowUp
                    });
                }
            }
            // Fanout is the reads of this wave each part actually started.
            for (index, part) in self.parts.iter_mut().enumerate() {
                let started = wave[..results.len()]
                    .iter()
                    .filter(|read| read.part == part.id)
                    .count();
                if started > 0 {
                    let round = part.current();
                    round.fanout = before[index].max(started);
                }
            }
            wave.truncate(results.len());
            for (read, result) in wave.into_iter().zip(results) {
                let Some(part) = self.parts.iter().position(|part| part.id == read.part) else {
                    continue;
                };
                match self.gather.observe(read.seq, result)? {
                    Observed::Retained(record) => {
                        let waiters = self.waiting.remove(&read.key).unwrap_or_default();
                        // A read re-tagged to a waiter reacts once, with the
                        // waiter's own step.
                        let owner_waits = waiters.iter().any(|(waiter, need, _)| {
                            *waiter == read.part && Some(*need) == read.need
                        });
                        if let Some(need) = read.need
                            && read.round == 1
                            && !owner_waits
                        {
                            self.react(part, need, read.step, record)?;
                        }
                        for (waiter, need, step) in waiters {
                            if let Some(index) =
                                self.parts.iter().position(|part| part.id == waiter)
                            {
                                self.react(index, need, step, record)?;
                            }
                        }
                    }
                    Observed::Failed { refused, .. } => {
                        for id in refused {
                            if let Some(index) = self.parts.iter().position(|part| part.id == id)
                                && self.parts[index].active()
                            {
                                self.stop_part(index, v3::PartStop::Refused, round);
                            }
                        }
                    }
                    Observed::Omitted => self.parts[part].shortened = true,
                    Observed::Stale => {}
                }
            }
            self.resolve_nominations()?;
            self.resolve_probes()?;
        }
    }

    /// A wave's canonical reads, up to `wave::READS_IN_FLIGHT` at once, in
    /// dispatch order. No read starts inside the reserve, so the result is a
    /// prefix of the wave; the caller releases the rest. A scope refusal
    /// stops further reads and is returned for the whole Answer; every other
    /// read error is that read's own outcome. Source-reply accounting runs
    /// here, in dispatch order, after the wave.
    fn read_wave(&self, wave: &[gather::Dispatch]) -> Result<Vec<Result<Value>>> {
        let reader = self.env.reader;
        let clock = self.env.clock;
        let reserve = Duration::from_millis(self.shares.reserve.total_ms());
        let deadline = self.deadline;
        let refused = |result: &Result<Value>| {
            result
                .as_ref()
                .is_err_and(|error| error.is::<airk_core::scoped_reads::ScopeReadRefused>())
        };
        let results = wave::run(
            wave.len(),
            &|| clock() + reserve >= deadline,
            &|index| {
                let read = &wave[index];
                let mut result = reader(read.tool, &read.arguments, Some(&read.revision));
                // A read the owner asks to repeat (a publication briefly
                // busy) is repeated a few times within the reserve, so a
                // transient owner state never becomes the read's evidence.
                for _ in 0..BUSY_REPEATS {
                    let Some(wait) = busy_retry(&result) else {
                        break;
                    };
                    if clock() + wait + reserve >= deadline {
                        break;
                    }
                    std::thread::sleep(wait);
                    result = reader(read.tool, &read.arguments, Some(&read.revision));
                }
                result
            },
            &refused,
        );
        for (read, result) in wave.iter().zip(&results) {
            match result {
                Ok(body) => self.kernel.observe_optional_source_reply(read.tool, body)?,
                Err(_) if refused(result) => {
                    return Err(anyhow::anyhow!(airk_core::scoped_reads::ScopeReadRefused));
                }
                Err(_) => {}
            }
        }
        Ok(results)
    }

    fn stage_of(round: u8) -> v3::LoopStage {
        if round <= 1 {
            v3::LoopStage::Retrieve
        } else {
            v3::LoopStage::FollowUp
        }
    }

    /// Stop one part early; its queued reads become its unrun reads.
    fn stop_part(&mut self, part: usize, stop: v3::PartStop, round: u8) {
        let reason = match stop {
            v3::PartStop::Deadline => Some(v3::UnrunReason::Deadline),
            v3::PartStop::Budget => Some(v3::UnrunReason::Budget),
            _ => None,
        };
        let drained = self.gather.drain_part(self.parts[part].id);
        if let Some(reason) = reason {
            for read in drained {
                let Some(need) = read.need else { continue };
                let unrun = v3::UnrunRead {
                    operation: read.operation,
                    target: read.target(),
                    reason,
                };
                let Some(state) = self.parts[part].need(need) else {
                    continue;
                };
                if state.unrun.len() < v3::MAX_UNRUN_READS
                    && !state
                        .unrun
                        .iter()
                        .any(|old| (old.operation, &old.target) == (unrun.operation, &unrun.target))
                {
                    state.unrun.push(unrun);
                }
            }
        }
        self.assess_final(part, round);
        let state = &mut self.parts[part];
        state.stop = Some(stop);
        state.stopped_in = Some(Self::stage_of(round));
        self.finish_part(part);
    }

    fn stop_all(&mut self, stop: GlobalStop, round: u8) {
        let stop = match stop {
            GlobalStop::Stale => v3::PartStop::Stale,
            GlobalStop::Budget => v3::PartStop::Budget,
            GlobalStop::Deadline => v3::PartStop::Deadline,
        };
        for part in 0..self.parts.len() {
            if self.parts[part].active() {
                self.stop_part(part, stop, round);
            }
        }
    }

    /// The deterministic coverage of what a part read before it stopped
    /// early; no decision is logged for it. A round with no read is dropped.
    fn assess_final(&mut self, part: usize, round: u8) {
        let id = self.parts[part].id;
        if self.parts[part]
            .rounds
            .last()
            .is_some_and(|last| last.calls == 0)
        {
            self.parts[part].rounds.pop();
        }
        if self.parts[part].rounds.is_empty() {
            return;
        }
        let round = round.min(self.parts[part].round());
        for index in 0..self.parts[part].needs.len() {
            if self.parts[part].needs[index].covered() {
                continue;
            }
            let mut spans = self.need_spans(part, index, round);
            coverage::rank(&mut spans);
            let assessed = self.assess(id, &self.parts[part].needs[index], spans, round);
            let state = &mut self.parts[part].needs[index];
            state.history.extend(assessed.spans.iter().cloned());
            state.coverage = assessed.coverage;
            state.coverage_by_model = false;
            state.evidence = assessed.evidence;
            state.spans = assessed.spans;
        }
        let covered = self.parts[part]
            .needs
            .iter()
            .filter(|need| need.covered())
            .count();
        let new_evidence = self.gather.new_evidence(id, round);
        let last = self.parts[part].current();
        last.covered = covered;
        last.new_evidence = new_evidence;
    }

    /// A sub-need's spans of this round: its own and, round one, the part's
    /// (`coverage::spans`), then with rule 5 the cheap first cut of the
    /// other sub-needs' passages, which Focus scores with the rest.
    fn need_spans(&self, part: usize, index: usize, round: u8) -> Vec<coverage::Span> {
        let id = self.parts[part].id;
        let state = &self.parts[part].needs[index];
        let mut spans = coverage::spans(&self.gather, id, state.need.id, round);
        if self.rules.cross {
            let cross = deliver::cross_spans(
                &self.gather,
                id,
                state.need.id,
                &state.need.query,
                round,
                &spans,
                deliver::CROSS_PER_NEED,
            );
            spans.extend(cross);
        }
        spans
    }

    /// Rule 6: each sub-need's best passages inside one byte budget, half
    /// the reply frame. A sub-need that receives passages cites their
    /// records beside its structural ones, so it is at least partly covered.
    fn deliver(&mut self, limit: usize) {
        let budget = limit / 2 / self.parts.len().max(1);
        for part in 0..self.parts.len() {
            let id = self.parts[part].id;
            let ranked: Vec<Vec<deliver::Candidate>> = self.parts[part]
                .needs
                .iter()
                .map(|state| {
                    let scored: Vec<coverage::Span> =
                        state.history.iter().chain(&state.spans).cloned().collect();
                    deliver::candidates(
                        &self.gather,
                        id,
                        state.need.id,
                        &state.need.query,
                        &scored,
                        self.rules.cross,
                    )
                })
                .collect();
            for candidates in &ranked {
                let spans: Vec<coverage::Span> =
                    candidates.iter().map(|item| item.span.clone()).collect();
                self.report.offered(&spans, &self.gather);
            }
            let chosen = deliver::deliver(&ranked, budget);
            for (state, delivered) in self.parts[part].needs.iter_mut().zip(chosen) {
                if delivered.is_empty() {
                    continue;
                }
                let mut evidence: Vec<usize> = Vec::new();
                for span in &delivered {
                    if !evidence.contains(&span.record) {
                        evidence.push(span.record);
                    }
                }
                for index in &state.evidence {
                    let structural = coverage::record_hits(&self.gather.records[*index]).is_empty();
                    if structural && !evidence.contains(index) {
                        evidence.push(*index);
                    }
                }
                state.evidence = evidence;
                if state.coverage == v3::Coverage::Missing {
                    state.coverage = v3::Coverage::Partial;
                }
                state.delivered = delivered;
            }
        }
    }

    fn assess(
        &self,
        part: u8,
        state: &NeedState,
        spans: Vec<coverage::Span>,
        round: u8,
    ) -> Assessed {
        if follow::only_join_discovery(&self.gather, part, state.need.id, round) {
            return Assessed {
                spans,
                coverage: state.coverage,
                evidence: state.evidence.clone(),
            };
        }
        let structural: Vec<usize> = self
            .gather
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                !record.failure
                    && !follow::is_join_search(record)
                    && coverage::record_hits(record).is_empty()
                    && record
                        .uses
                        .iter()
                        .any(|item| item.part == part && item.need == Some(state.need.id))
            })
            .map(|(index, _)| index)
            .collect();
        let verdict = coverage::verdict(&self.gather, part, &state.need, &spans, structural.len());
        let mut evidence: Vec<usize> = state.evidence.clone();
        for index in spans.iter().map(|span| span.record).chain(structural) {
            if !evidence.contains(&index) {
                evidence.push(index);
            }
        }
        let coverage = if evidence.is_empty() {
            v3::Coverage::Missing
        } else {
            verdict
        };
        Assessed {
            spans,
            coverage,
            evidence,
        }
    }

    fn finish_part(&mut self, part: usize) {
        if self.shares.parts[part].done {
            return;
        }
        let elapsed = self.elapsed_ms();
        self.shares.finish(
            part,
            shares::Share {
                calls: self.used_calls[part],
                card_slots: self.used_slots[part],
                focus_pairs: self.used_pairs[part],
                tokens: self.part_tokens(self.parts[part].id),
                ..shares::Share::default()
            },
            elapsed,
        );
        for (state, share) in self.parts.iter_mut().zip(&self.shares.parts) {
            state.share_ms = share.deadline_ms;
        }
    }

    fn alternatives(need: u8, candidates: &[follow::Candidate]) -> Vec<coverage::Alternative> {
        candidates
            .iter()
            .take(3)
            .map(|candidate| {
                if candidate.operation == v3::LoopRead::Search {
                    coverage::Alternative {
                        class: v3::CardClass::SearchForTerm,
                        text: format!("s{need} search: {}", candidate.term),
                    }
                } else {
                    coverage::Alternative {
                        class: v3::CardClass::ReadMoreHere,
                        text: format!("s{need} read more: {}", candidate.term),
                    }
                }
            })
            .collect()
    }

    fn spans_basis(
        &self,
        spans: &[coverage::Span],
    ) -> Vec<airk_core::arbiter::context_action::EvidenceRef> {
        spans
            .iter()
            .map(|span| {
                decide::basis(
                    self.gather.records[span.record].id,
                    &span.pointer,
                    &span.hit.text,
                )
            })
            .collect()
    }

    /// The round barrier, common to every active part: batched Focus over
    /// all their spans, coverage decisions, then each part's stop or its
    /// follow-up decisions. Rosters of different gaps never share a read.
    fn barrier(&mut self, round: u8) -> Result<()> {
        self.fence()?;
        let limit = self.request.budget.rounds;
        let mut entries: Vec<(usize, usize, Vec<coverage::Span>)> = Vec::new();
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let id = self.parts[part].id;
            let new_evidence = self.gather.new_evidence(id, round);
            self.parts[part].current().new_evidence = new_evidence;
            self.progress(part, v3::LoopStage::Read);
            for index in 0..self.parts[part].needs.len() {
                if !self.parts[part].needs[index].covered() {
                    let spans = self.need_spans(part, index, round);
                    self.report.offered(&spans, &self.gather);
                    entries.push((part, index, spans));
                }
            }
        }
        if let Err(error) = self.focus(&mut entries, round) {
            // Focus went stale after native work: the Answer's own
            // nonreplayable refusal, with the work so far kept in custody.
            return Err(if error.is::<super::focus::FocusBatchStale>() {
                AnswerPostExecutionStale {
                    _reply: None,
                    _custody: Box::new(
                        self.custody(self.ledger.log(&self.request.question, &self.scope)),
                    ),
                    _cause: Some(error),
                }
                .into()
            } else {
                error
            });
        }
        // Coverage: one roster per uncovered sub-need, two per card call.
        let mut segs = Vec::new();
        let mut pending = Vec::new();
        for (part, index, mut spans) in entries {
            coverage::rank(&mut spans);
            let id = self.parts[part].id;
            let assessed = self.assess(id, &self.parts[part].needs[index], spans, round);
            let state = &self.parts[part].needs[index];
            let candidates = follow::candidates(
                &self.gather,
                id,
                &state.need,
                &assessed.spans,
                &self.searched,
                &state.tried,
            )?;
            let need = state.need.id;
            let covered = assessed.coverage == v3::Coverage::Covered;
            let roster = (!follow::only_join_discovery(&self.gather, id, need, round)
                && coverage::may_offer_covered(state.need.kind, assessed.coverage))
            .then(|| {
                coverage::cards(
                    need,
                    &assessed.spans,
                    &Self::alternatives(need, &candidates),
                )
            })
            .filter(|roster| roster.len() >= 2 || covered);
            let logged = roster.is_some();
            if let Some(roster) = roster {
                segs.push(model::Seg {
                    at: decide::At::need(v3::LoopStage::Coverage, id, need, round),
                    cards: roster,
                    subject: plan::wording(
                        state.need.kind,
                        state.need.subject.as_deref(),
                        &state.need.query,
                    ),
                    basis: self.spans_basis(&assessed.spans),
                    spans: assessed.spans.clone(),
                    verdict: None,
                    fallback: vec![usize::from(!covered)],
                    rule: v3::DeterministicRule::TopFocusSpan,
                });
            }
            pending.push((part, index, assessed, candidates, logged));
        }
        let mut choices = self.decide(segs, 2)?.into_iter();
        for (part, index, assessed, candidates, logged) in pending {
            let mut coverage = assessed.coverage;
            let mut by_model = false;
            if logged
                && let Some(choice) = choices.next()
                && choice.model
            {
                coverage = coverage::from_card(
                    choice.chosen.first() == Some(&0),
                    &assessed.spans,
                    assessed.evidence.len(),
                );
                by_model = true;
            }
            let state = &mut self.parts[part].needs[index];
            state.history.extend(assessed.spans.iter().cloned());
            state.coverage = coverage;
            state.coverage_by_model = by_model;
            state.evidence = assessed.evidence;
            state.spans = assessed.spans;
            state.candidates = candidates;
        }
        // Stops, then follow-up rosters of the parts that continue.
        let mut segs = Vec::new();
        let mut planned: Vec<(usize, u8, Vec<follow::Candidate>)> = Vec::new();
        let mut shown: BTreeSet<ReadKey> = BTreeSet::new();
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let id = self.parts[part].id;
            let covered = self.parts[part]
                .needs
                .iter()
                .filter(|need| need.covered())
                .count();
            self.parts[part].current().covered = covered;
            self.progress(part, v3::LoopStage::Coverage);
            let new_evidence = self.parts[part].current().new_evidence;
            if covered == self.parts[part].needs.len() {
                self.parts[part].stop = Some(v3::PartStop::Covered);
            } else if usize::from(round) >= limit {
                self.unrun(part, v3::UnrunReason::RoundLimit);
                self.parts[part].stop = Some(v3::PartStop::Rounds);
            } else if round > 1 && new_evidence == 0 {
                self.unrun(part, v3::UnrunReason::NotChosen);
                self.parts[part].stop = Some(v3::PartStop::NoProgress);
            } else {
                let gaps = self.parts[part]
                    .needs
                    .iter()
                    .filter(|need| !need.covered())
                    .count();
                let size = if gaps == 1 { follow::MAX_CANDIDATES } else { 4 };
                let mut any = false;
                for state in &self.parts[part].needs {
                    if state.covered() {
                        continue;
                    }
                    let candidates: Vec<follow::Candidate> = state
                        .candidates
                        .iter()
                        .filter(|candidate| {
                            !self.gather.known(&candidate.key) && !shown.contains(&candidate.key)
                        })
                        .take(size)
                        .cloned()
                        .collect();
                    if candidates.is_empty() {
                        continue;
                    }
                    shown.extend(candidates.iter().map(|candidate| candidate.key));
                    any = true;
                    let need = state.need.id;
                    segs.push(model::Seg {
                        at: decide::At::need(v3::LoopStage::FollowUp, id, need, round),
                        cards: follow::cards(need, &candidates, size),
                        // The roster says what the read is for: the
                        // sub-need with its kind, how far this round's spans
                        // cover it and who judged that, and those spans.
                        subject: plan::wording(
                            state.need.kind,
                            state.need.subject.as_deref(),
                            &state.need.query,
                        ),
                        basis: self.spans_basis(&state.spans),
                        spans: state.spans.clone(),
                        verdict: Some(airk_core::native_loop::CardVerdict {
                            coverage: state.coverage,
                            by_model: state.coverage_by_model,
                        }),
                        fallback: vec![0],
                        rule: v3::DeterministicRule::FirstExtractedTerm,
                    });
                    planned.push((part, need, candidates));
                }
                if any {
                    self.parts[part].chosen.clear();
                    self.parts[part].rounds.push(RoundState::default());
                } else {
                    self.parts[part].stop = Some(v3::PartStop::NoJustifiedRead);
                }
            }
            if !self.parts[part].active() {
                self.finish_part(part);
            }
        }
        let choices = self.decide(segs, 2)?;
        for ((part, need, candidates), choice) in planned.into_iter().zip(choices) {
            let pick = choice
                .chosen
                .first()
                .copied()
                .unwrap_or(0)
                .min(candidates.len() - 1);
            let chosen = candidates[pick].clone();
            let id = self.parts[part].id;
            let read = self.gather.dispatch(
                (chosen.tool, chosen.operation),
                Value::Object(chosen.arguments.clone()),
                (id, Some(need), round + 1),
                Cause::FollowUp,
                chosen.kind,
                chosen.step,
                &chosen.term,
            )?;
            self.parts[part].chosen.push((need, chosen));
            self.offer(part, read)?;
        }
        for part in 0..self.parts.len() {
            if self.parts[part].active() {
                self.progress(part, v3::LoopStage::FollowUp);
            }
        }
        Ok(())
    }

    /// The candidates a gap would read next, listed for the agent to run.
    fn unrun(&mut self, part: usize, reason: v3::UnrunReason) {
        for state in &mut self.parts[part].needs {
            if state.covered() {
                continue;
            }
            for candidate in &state.candidates {
                let read = candidate.unrun(reason);
                if state.unrun.len() < v3::MAX_UNRUN_READS
                    && !state
                        .unrun
                        .iter()
                        .any(|old| (old.operation, &old.target) == (read.operation, &read.target))
                {
                    state.unrun.push(read);
                }
            }
        }
    }

    fn degradations(&self) -> Vec<v3::Degradation> {
        let mut out = Vec::new();
        let entries = &self.ledger.entries;
        // No card call was dispatched: every logged decision took the
        // deterministic order, for whatever reason (no model, or a model
        // that was blocked, declined or out of slots, tokens or time).
        if entries
            .iter()
            .all(|entry| !entry.decision.scored_by_model())
        {
            out.push(v3::Degradation::ContextAbsent);
        }
        let abstained = entries
            .iter()
            .filter(|entry| entry.decision.source.kind() == v3::SourceKind::Abstained)
            .count();
        if abstained > 0 {
            out.push(v3::Degradation::ContextAbstained { count: abstained });
        }
        if self.ledger.focus.is_empty() {
            out.push(v3::Degradation::FocusAbsent);
        }
        for (part, stage) in &self.model_share {
            out.push(v3::Degradation::ModelShare {
                part: *part,
                stage: *stage,
            });
        }
        if entries.iter().any(|entry| entry.decision.work_unknown)
            || self
                .ledger
                .focus
                .iter()
                .any(|receipt| receipt.observed_input_tokens.is_none())
        {
            out.push(v3::Degradation::ModelOutcomeUnknown);
        }
        if self.configuration_changed {
            out.push(v3::Degradation::ConfigurationChanged);
        }
        if self.batch_split {
            out.push(v3::Degradation::BatchSplit);
        }
        if self
            .gather
            .records
            .iter()
            .any(|record| record.operation == v3::LoopRead::Search)
        {
            out.push(v3::Degradation::SearchFocusDeferred);
        }
        out.extend(self.admitted.degradations.iter().cloned());
        out
    }

    /// The decision log and every intact canonical result as received,
    /// status included; the id binds its stable form (entry 54).
    fn custody(&self, log: v3::DecisionLog) -> AnswerCustody {
        let records = self
            .gather
            .records
            .iter()
            .map(|record| CustodyRecord {
                id: record.id,
                tool: record.tool.to_owned(),
                arguments: record.arguments.clone(),
                canonical_result: record.received.clone(),
            })
            .collect();
        let mut nominations: Vec<_> = self
            .nominations
            .iter()
            .chain(&self.lookups)
            .map(|(id, item)| {
                json!({"id": id, "ordinal": item.ordinal, "tool": item.tool, "arguments": item.arguments,
                    "revision": self.gather.revision, "basis": item.basis, "outcome": item.outcome})
            })
            .collect();
        nominations.sort_by_key(|row| row["ordinal"].as_u64());
        AnswerCustody {
            log,
            records,
            nominations,
        }
    }

    fn finish(mut self) -> Result<AnswerOutcome> {
        for part in 0..self.parts.len() {
            if self.parts[part].active() {
                let round = self.parts[part].round();
                self.assess_final(part, round);
                self.parts[part].stop = Some(v3::PartStop::NoProgress);
                self.finish_part(part);
            }
        }
        for part in 0..self.parts.len() {
            self.progress(part, v3::LoopStage::Compose);
        }
        self.report_unbegun();
        self.begin_stage(v3::GatheringStageKind::Delivery);
        (self.env.require_live)()?;
        self.kernel
            .require_delivery_fence(self.context.authority.fingerprint, &self.publication)?;
        self.fence()?;
        let reserve = self.context.context_reply_reserve;
        let limit = v3::reply_limit(&self.request.budget).saturating_sub(reserve);
        if self.rules.delivery {
            self.deliver(limit);
        }
        // Every stage closes before the reply's elapsed time is taken, so no
        // stage outlasts it.
        let (now, judgments) = (self.now(), self.judgments());
        self.report.end(now, judgments);
        let elapsed_ms = self.elapsed_ms();
        let delivery = self.rules.delivery;
        let gathering = self.report.build(&self.gather, &self.parts, |need| {
            if delivery && !need.delivered.is_empty() {
                need.delivered.clone()
            } else {
                need.history
                    .iter()
                    .filter(|span| need.evidence.contains(&span.record))
                    .cloned()
                    .collect()
            }
        });
        let assembly = union::Assembly {
            gathering: Some(gathering),
            delivered_only: delivery,
            question: &self.request.question,
            scope: &self.scope,
            parts: &self.parts,
            gather: &self.gather,
            ledger: &self.ledger,
            degradations: self.degradations(),
            application: self.application.clone(),
            elapsed_ms,
            limit,
            unknown_tokens: self.unknown_tokens.clone(),
        };
        let (response, log) = union::assemble(&assembly)?;
        let custody = self.custody(log);
        // Native work under a configuration that is no longer current is
        // never delivered: a content-free, nonreplayable refusal instead.
        if self.configuration_changed && self.ledger.has_native_work() {
            return Err(AnswerPostExecutionStale {
                _reply: Some(Box::new(response)),
                _custody: Box::new(custody),
                _cause: None,
            }
            .into());
        }
        response.validate_with_log(&custody.log, &self.admitted.asked)?;
        if !self.parts.is_empty() {
            self.progress(0, v3::LoopStage::Union);
        }
        Ok(AnswerOutcome { response, custody })
    }
}

#[cfg(test)]
mod tests;

/// Native work became stale after a configuration change, or a Focus batch
/// went stale mid-loop: the reply (if one was assembled), its custody and
/// the cause stay private, and the caller receives a nonreplayable refusal.
pub struct AnswerPostExecutionStale {
    _reply: Option<Box<v3::ContextResponseV3>>,
    _custody: Box<AnswerCustody>,
    _cause: Option<anyhow::Error>,
}
impl std::fmt::Debug for AnswerPostExecutionStale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnswerPostExecutionStale")
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for AnswerPostExecutionStale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Optional read policy changed after native work; the stale answer cannot be replayed.",
        )
    }
}
impl std::error::Error for AnswerPostExecutionStale {}
impl AnswerPostExecutionStale {
    pub fn receipt(&self) -> Value {
        json!({"error":{"code":"arbiter_context_action_stale","retryable":false,"query_replay_safe":false,
            "message":self.to_string()}})
    }
}
