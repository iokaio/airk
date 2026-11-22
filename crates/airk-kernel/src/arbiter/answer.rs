// SPDX-License-Identifier: Apache-2.0
//! Answer /3: Split, then per part Plan, Retrieve, Read and check coverage,
//! Follow up and Compose, then Union (final plan section 12). Every read is a
//! canonical dispatch through the one shared read owner (`gather`); models
//! only choose among cards deterministic code proposes.
//!
//! The parts run in lockstep rounds. Within a round, reads go out in waves:
//! a wave's reads are dispatched, then observed in dispatch order, and the
//! structural reads they plan join the next wave. Decisions are taken only
//! at a round barrier, from state ordered by dispatch, so neither read
//! completion order nor concurrency can change the reply.
use super::*;
use airk_core::arbiter::answer_loop as v3;
use airk_core::native_loop::LoopCard;
use gather::{Cause, Gathering, GlobalStop, Observed, Offer, ReadKey, Step};
use retrieve::Probe;
use state::{NeedState, PartState, RoundState};
use std::collections::BTreeMap;

mod compose;
mod coverage;
mod decide;
mod follow;
mod gather;
mod plan;
mod retrieve;
mod shares;
mod split;
mod state;
mod union;

/// A canonical read the loop may call from several threads.
pub type CanonicalReadFn<'a> =
    dyn Fn(&str, &Map<String, Value>, Option<&str>) -> Result<Value> + Sync + 'a;

/// One progress observation: a part reached a stage of a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnswerProgress {
    pub part: u8,
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
    pub id: Sha256Digest,
    pub tool: String,
    pub arguments: Map<String, Value>,
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
}

#[derive(Clone, Debug)]
pub struct AnswerOutcome {
    pub response: v3::ContextResponseV3,
    pub custody: AnswerCustody,
}

const SYMBOLS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// Round-one reads of one sub-need (search, symbols, references, imports or
/// outline, governing document, tests).
const FIRST_ROUND_READS: usize = v3::FIRST_ROUND_READS_PER_SUB_NEED;
const PROBES_PER_ANSWER: usize = 24;

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
        request.validate()?;
        (env.require_live)()?;
        let started = (env.clock)();
        let context = self
            .arbiter_context
            .as_ref()
            .context("Answer requires a native admitted index connection")?;
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
            governing: BTreeMap::new(),
            searched: BTreeSet::new(),
            waiting: BTreeMap::new(),
            used_calls: vec![],
            rr: 0,
        };
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
    governing: BTreeMap<(u8, u8), usize>,
    /// Search queries already dispatched, lower case.
    searched: BTreeSet<String>,
    /// Recipe steps waiting on a read another dispatch already queued.
    waiting: BTreeMap<ReadKey, Vec<(u8, u8, Step)>>,
    used_calls: Vec<usize>,
    rr: usize,
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
    fn progress(&self, part: usize, stage: v3::LoopStage) {
        let part = &self.parts[part];
        (self.env.progress)(AnswerProgress {
            part: part.id,
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
        }
        self.application = current;
        Ok(())
    }

    fn split(&mut self) -> Result<()> {
        self.fence()?;
        let question = self.request.question.as_str();
        let plan = split::propose(question, self.request.budget.max_parts);
        let cards: Vec<LoopCard> = plan
            .cards
            .iter()
            .enumerate()
            .map(|(index, partition)| split::card(question, partition, SYMBOLS[index]))
            .collect();
        let chosen = plan.deterministic;
        self.ledger.deterministic(
            decide::At::split(),
            &cards,
            &[chosen],
            v3::DeterministicRule::ExplicitBreaks,
            vec![],
        )?;
        for (index, range) in plan.cards[chosen].0.iter().enumerate() {
            let span = split::part_span(question, *range);
            self.parts.push(PartState::new(
                index as u8 + 1,
                span,
                &question[span.0..span.1],
            ));
        }
        self.shares = shares::Shares::allocate(
            &self.request.budget,
            self.parts.len(),
            self.request.budget.context_calls,
            self.request.budget.input_tokens,
        );
        self.used_calls = vec![0; self.parts.len()];
        for (part, share) in self.parts.iter_mut().zip(&self.shares.parts) {
            part.share_ms = share.deadline_ms;
        }
        Ok(())
    }

    fn plan(&mut self) -> Result<()> {
        let question = self.request.question.clone();
        for index in 0..self.parts.len() {
            let proposals = plan::propose(&self.parts[index].text, &question);
            let cards = plan::cards(&proposals);
            let chosen = plan::deterministic(&proposals, self.request.budget.max_sub_needs);
            self.ledger.deterministic(
                decide::At::plan(self.parts[index].id),
                &cards,
                &chosen,
                v3::DeterministicRule::EveryProposedNeed,
                vec![],
            )?;
            self.parts[index].needs = plan::needs(&proposals, &chosen)
                .into_iter()
                .map(NeedState::new)
                .collect();
            self.progress(index, v3::LoopStage::Plan);
        }
        Ok(())
    }

    /// Offer a read, within its sub-need's round-one cap.
    fn offer(&mut self, part: usize, dispatch: gather::Dispatch) -> Result<()> {
        let id = self.parts[part].id;
        if let Some(need) = dispatch.need {
            let state = self.parts[part]
                .need(need)
                .context("Answer read for an unknown sub-need")?;
            if dispatch.round == 1 {
                if state.reads >= FIRST_ROUND_READS {
                    return Ok(());
                }
                state.reads += 1;
            }
            state.tried.insert(dispatch.key);
        }
        if dispatch.tool == "search"
            && let Some(query) = dispatch.arguments.get("query").and_then(Value::as_str)
        {
            self.searched.insert(query.to_lowercase());
        }
        let (key, step, round, need) = (dispatch.key, dispatch.step, dispatch.round, dispatch.need);
        match self.gather.offer(dispatch) {
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
            Offer::Queued | Offer::Unavailable => {}
        }
        Ok(())
    }

    /// The structural reads a round-one recipe step plans from its record.
    fn react(&mut self, part: usize, need: u8, step: Step, record: usize) -> Result<()> {
        if step == Step::Leaf || self.gather.records[record].failure {
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

    fn rounds(&mut self) -> Result<()> {
        for part in 0..self.parts.len() {
            self.parts[part].rounds.push(RoundState::default());
            let id = self.parts[part].id;
            let text = self.parts[part].text.clone();
            let search = retrieve::part_search(&mut self.gather, id, &text)?;
            self.offer(part, search)?;
            let needs: Vec<_> = self.parts[part]
                .needs
                .iter()
                .map(|state| state.need.clone())
                .collect();
            for need in needs {
                for read in retrieve::initial(&mut self.gather, id, &need)? {
                    self.offer(part, read)?;
                }
            }
        }
        let mut round = 1u8;
        loop {
            if let Some(stop) = self.gather_round(round)? {
                self.stop_all(stop, round);
                break;
            }
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
                return Ok(self.gather.stop);
            }
            wave.sort_by_key(|read| read.seq);
            let mut results = Vec::with_capacity(wave.len());
            for read in &wave {
                if self.stop_due() {
                    break;
                }
                results.push(self.read(read)?);
            }
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
                        if let Some(need) = read.need
                            && read.round == 1
                        {
                            self.react(part, need, read.step, record)?;
                        }
                        for (waiter, need, step) in
                            self.waiting.remove(&read.key).unwrap_or_default()
                        {
                            if let Some(index) =
                                self.parts.iter().position(|part| part.id == waiter)
                            {
                                self.react(index, need, step, record)?;
                            }
                        }
                    }
                    Observed::Refused(_) => {
                        if self.parts[part].active() {
                            self.stop_part(part, v3::PartStop::Refused, round);
                        }
                    }
                    Observed::Omitted => self.parts[part].shortened = true,
                    Observed::Failed(_) | Observed::Stale => {}
                }
            }
            self.resolve_probes()?;
        }
    }

    /// One canonical read. The outer `Result` is a refusal the whole Answer
    /// must propagate; the inner one is the read's own outcome.
    fn read(&self, read: &gather::Dispatch) -> Result<Result<Value>> {
        let result = (self.env.reader)(read.tool, &read.arguments, Some(&read.revision));
        match &result {
            Ok(body) => self.kernel.observe_optional_source_reply(read.tool, body)?,
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(anyhow::anyhow!(airk_core::scoped_reads::ScopeReadRefused));
            }
            Err(_) => {}
        }
        Ok(result)
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
            let assessed = self.assess(id, &self.parts[part].needs[index], round);
            let state = &mut self.parts[part].needs[index];
            state.history.extend(assessed.spans.iter().cloned());
            state.coverage = assessed.coverage;
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

    fn assess(&self, part: u8, state: &NeedState, round: u8) -> Assessed {
        let mut spans = coverage::spans(&self.gather, part, state.need.id, round);
        coverage::rank(&mut spans);
        let structural: Vec<usize> = self
            .gather
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                !record.failure
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

    /// The round barrier: coverage, then stop or follow-ups, per part.
    fn barrier(&mut self, round: u8) -> Result<()> {
        self.fence()?;
        let limit = self.request.budget.rounds;
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let id = self.parts[part].id;
            let new_evidence = self.gather.new_evidence(id, round);
            self.parts[part].current().new_evidence = new_evidence;
            for index in 0..self.parts[part].needs.len() {
                if self.parts[part].needs[index].covered() {
                    continue;
                }
                let assessed = self.assess(id, &self.parts[part].needs[index], round);
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
                let roster = coverage::cards(
                    need,
                    &assessed.spans,
                    &Self::alternatives(need, &candidates),
                );
                let covered = assessed.coverage == v3::Coverage::Covered;
                if roster.len() >= 2 || covered {
                    let basis = self.spans_basis(&assessed.spans);
                    self.ledger.deterministic(
                        decide::At::need(v3::LoopStage::Coverage, id, need, round),
                        &roster,
                        &[usize::from(!covered)],
                        v3::DeterministicRule::TopFocusSpan,
                        basis,
                    )?;
                }
                let state = &mut self.parts[part].needs[index];
                state.history.extend(assessed.spans.iter().cloned());
                state.coverage = assessed.coverage;
                state.evidence = assessed.evidence;
                state.spans = assessed.spans;
                state.candidates = candidates;
            }
            let covered = self.parts[part]
                .needs
                .iter()
                .filter(|need| need.covered())
                .count();
            self.parts[part].current().covered = covered;
            self.progress(part, v3::LoopStage::Coverage);
            if covered == self.parts[part].needs.len() {
                self.parts[part].stop = Some(v3::PartStop::Covered);
            } else if usize::from(round) >= limit {
                self.unrun(part, v3::UnrunReason::RoundLimit);
                self.parts[part].stop = Some(v3::PartStop::Rounds);
            } else if round > 1 && new_evidence == 0 {
                self.unrun(part, v3::UnrunReason::NotChosen);
                self.parts[part].stop = Some(v3::PartStop::NoProgress);
            } else if !self.follow_up(part, round)? {
                self.parts[part].stop = Some(v3::PartStop::NoJustifiedRead);
            }
            if !self.parts[part].active() {
                self.finish_part(part);
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

    /// Choose one follow-up read per gap; false when no gap has one.
    fn follow_up(&mut self, part: usize, round: u8) -> Result<bool> {
        let id = self.parts[part].id;
        let mut chosen = Vec::new();
        for index in 0..self.parts[part].needs.len() {
            let state = &self.parts[part].needs[index];
            if state.covered() {
                continue;
            }
            let candidates: Vec<follow::Candidate> = state
                .candidates
                .iter()
                .filter(|candidate| !self.gather.known(&candidate.key))
                .cloned()
                .collect();
            let Some(first) = candidates.first().cloned() else {
                continue;
            };
            let need = state.need.id;
            let roster = follow::cards(need, &candidates, 4);
            let basis = self.spans_basis(&state.spans);
            self.ledger.deterministic(
                decide::At::need(v3::LoopStage::FollowUp, id, need, round),
                &roster,
                &[0],
                v3::DeterministicRule::FirstExtractedTerm,
                basis,
            )?;
            let read = self.gather.dispatch(
                (first.tool, first.operation),
                Value::Object(first.arguments.clone()),
                (id, Some(need), round + 1),
                Cause::FollowUp,
                first.kind,
                first.step,
                &first.term,
            )?;
            chosen.push((need, first));
            self.offer(part, read)?;
        }
        if chosen.is_empty() {
            return Ok(false);
        }
        self.parts[part].chosen = chosen;
        self.parts[part].rounds.push(RoundState::default());
        Ok(true)
    }

    fn degradations(&self) -> Vec<v3::Degradation> {
        let mut out = Vec::new();
        if self
            .ledger
            .entries
            .iter()
            .all(|entry| !entry.decision.scored_by_model())
        {
            out.push(v3::Degradation::ContextAbsent);
        }
        if self.ledger.focus.is_empty() {
            out.push(v3::Degradation::FocusAbsent);
        }
        if self.configuration_changed {
            out.push(v3::Degradation::ConfigurationChanged);
        }
        if self
            .gather
            .records
            .iter()
            .any(|record| record.operation == v3::LoopRead::Search)
        {
            out.push(v3::Degradation::SearchFocusDeferred);
        }
        out
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
        (self.env.require_live)()?;
        ensure!(
            self.kernel.scoped_read_fingerprint()? == Some(self.context.authority.fingerprint)
                && self.kernel.arbiter_publication()? == self.publication,
            "Answer publication or source authority changed before delivery"
        );
        self.fence()?;
        let reserve = self.context.context_reply_reserve;
        let limit = v3::reply_limit(&self.request.budget).saturating_sub(reserve);
        let assembly = union::Assembly {
            question: &self.request.question,
            scope: &self.scope,
            parts: &self.parts,
            gather: &self.gather,
            ledger: &self.ledger,
            degradations: self.degradations(),
            application: self.application.clone(),
            elapsed_ms: self.elapsed_ms(),
            limit,
        };
        let (response, log) = union::assemble(&assembly)?;
        response.validate_with_log(&log, &self.request.budget)?;
        let records = self
            .gather
            .records
            .iter()
            .map(|record| CustodyRecord {
                id: record.id,
                tool: record.tool.to_owned(),
                arguments: record.arguments.clone(),
                canonical_result: record.body.clone(),
            })
            .collect();
        Ok(AnswerOutcome {
            response,
            custody: AnswerCustody { log, records },
        })
    }
}

#[cfg(test)]
mod tests;
