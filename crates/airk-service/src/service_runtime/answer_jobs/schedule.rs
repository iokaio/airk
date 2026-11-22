// SPDX-License-Identifier: Apache-2.0
//! The job table's pure state: admission, attach-by-digest, earliest-deadline
//! dispatch with ageing, the gathering limit, retention and queue status.
//! Every operation takes the injected clock's `now` in milliseconds; nothing
//! here sleeps, spawns or reads a clock.
use super::estimate::{self, ContextRung, CostTable, Fitted, Shape};
use airk_core::{
    arbiter::{
        ScopeBinding,
        answer_loop::{
            AnswerTicket, ContextRequestV3, DEADLINE_TOLERANCE_MS, Degradation, MAX_DEADLINE_MS,
            MAX_ESTIMATE_MS, MAX_QUEUE_POSITION,
        },
    },
    evidence::RevisionEnvelope,
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::IndexId,
};
use anyhow::{Result, bail, ensure};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, VecDeque},
    sync::Arc,
};

/// The floor of the global gathering limit (plan section 12.4.1).
pub(crate) const GATHERING_FLOOR: usize = 2;
/// Finished results are kept this long after they finish.
pub(crate) const DEFAULT_RETENTION_MS: u64 = 600_000;
pub(crate) const DEFAULT_RETAINED_BYTES: usize = 64 << 20;
pub(crate) const DEFAULT_RETAINED_RESULTS: usize = 1024;
pub(crate) const DEFAULT_ACTIVE_PER_SCOPE: usize = 8;
pub(crate) const DEFAULT_ACTIVE_PER_PRINCIPAL: usize = 32;
/// Bytes every retained entry is charged beyond its question and result.
pub(crate) const ENTRY_OVERHEAD_BYTES: usize = 1024;
/// Learned correction bounds, in thousandths of the cost table.
const CORRECTION_MIN: u64 = 250;
const CORRECTION_MAX: u64 = 4000;
const RECENT_SAMPLES: usize = 16;

/// Who may see a job. Two callers share a job only when their authority
/// digests are equal: the installation, catalog, index, the caller's
/// source-authority fingerprint and the configuration revision all match.
/// The principal (installation and fingerprint) bounds one caller across its
/// scopes. The publication is not part of either; results are matched on it
/// separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct AnswerScope {
    authority: Sha256Digest,
    principal: Sha256Digest,
    index: [u8; 16],
}
impl AnswerScope {
    pub(crate) fn from_binding(binding: &ScopeBinding) -> Self {
        let index = *binding.index_id.identity().as_bytes();
        Self {
            authority: framed_digest(
                b"airk.answer.job.scope/2",
                &[
                    binding.installation.as_bytes(),
                    binding.catalog.as_bytes(),
                    &index,
                    binding.authorization.as_bytes(),
                    &binding.configuration_revision.to_be_bytes(),
                ],
            ),
            principal: framed_digest(
                b"airk.answer.job.principal/1",
                &[
                    binding.installation.as_bytes(),
                    binding.authorization.as_bytes(),
                ],
            ),
            index,
        }
    }
    /// This scope for replies packed to `reserve` bytes of the caller's own
    /// metadata: calls with different reserves never share a job, while the
    /// principal and the index, and so every limit, stay the same.
    pub(crate) fn with_reply_reserve(mut self, reserve: usize) -> Self {
        self.authority = framed_digest(
            b"airk.answer.job.scope.reserve/1",
            &[self.authority.as_bytes(), &(reserve as u64).to_be_bytes()],
        );
        self
    }
}

/// The digest a publication is matched on.
pub(crate) fn publication_digest(publication: &RevisionEnvelope) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk.answer.job.publication/1",
        &[&serde_json::to_vec(publication)?],
    ))
}

/// The digest a retry attaches by: the question and every budget field but
/// the deadline. The continuation is never part of it.
pub(crate) fn request_digest(request: &ContextRequestV3) -> Result<Sha256Digest> {
    let mut budget = request.budget.clone();
    budget.deadline_ms = 0;
    Ok(framed_digest(
        b"airk.answer.job.request/1",
        &[
            request.schema.as_bytes(),
            request.question.as_bytes(),
            &serde_json::to_vec(&budget)?,
        ],
    ))
}

#[derive(Clone, Debug)]
pub(crate) struct JobTableConfig {
    pub(crate) retention_ms: u64,
    pub(crate) max_retained_bytes: usize,
    pub(crate) max_retained_results: usize,
    /// Concurrent gatherings the model lanes can serve, from L5b's runtime
    /// capacity rules. `None` until that figure exists.
    pub(crate) runtime_capacity: Option<usize>,
    /// The user's cap on the automatic figure; it never goes below the floor.
    pub(crate) user_cap: Option<usize>,
    /// Queued and running jobs one scope may hold.
    pub(crate) max_active_per_scope: usize,
    /// Queued and running jobs one principal may hold across its scopes.
    pub(crate) max_active_per_principal: usize,
    /// Queued and running jobs in the whole table.
    pub(crate) max_jobs: usize,
    /// Ageing weight: the EDF key is `due + ageing x enqueued`.
    pub(crate) ageing: u64,
    pub(crate) costs: CostTable,
    /// False while `costs` are placeholders. Unmeasured costs still order
    /// and bound the queue, but they never reduce a job that has its whole
    /// window: a request that did not wait runs as asked. `set_costs` makes
    /// them measured.
    pub(crate) costs_measured: bool,
    /// The runner cannot change a job's Context rung: each job is fitted
    /// with its own rung and the no-model loop alone.
    pub(crate) configured_rung_only: bool,
}
impl Default for JobTableConfig {
    fn default() -> Self {
        Self {
            retention_ms: DEFAULT_RETENTION_MS,
            max_retained_bytes: DEFAULT_RETAINED_BYTES,
            max_retained_results: DEFAULT_RETAINED_RESULTS,
            runtime_capacity: None,
            user_cap: None,
            max_active_per_scope: DEFAULT_ACTIVE_PER_SCOPE,
            max_active_per_principal: DEFAULT_ACTIVE_PER_PRINCIPAL,
            max_jobs: MAX_QUEUE_POSITION as usize,
            ageing: 1,
            costs: CostTable::default(),
            costs_measured: true,
            configured_rung_only: false,
        }
    }
}

/// `max(2, min(capacity, cap))`, with capacity 2 until L5b supplies it.
pub(crate) fn gathering_limit(runtime_capacity: Option<usize>, user_cap: Option<usize>) -> usize {
    let automatic = runtime_capacity.unwrap_or(GATHERING_FLOOR);
    user_cap
        .map_or(automatic, |cap| automatic.min(cap))
        .max(GATHERING_FLOOR)
}

/// What a finished job holds.
#[derive(Debug)]
pub(crate) enum JobOutcome<R> {
    Done {
        value: R,
        /// The publication the job ran on.
        publication: Sha256Digest,
        /// Bytes counted against the retention budget.
        retained_bytes: usize,
    },
    Failed(String),
    /// The job did nothing a retry could repeat: it never started
    /// (`not_run`), or its refusal is safe to repeat. Delivered to its
    /// waiters, never kept and never learned from.
    Dropped {
        message: String,
        not_run: bool,
    },
}

/// What a dispatched job runs with.
#[derive(Clone, Debug)]
pub(crate) struct StartedJob {
    pub(crate) id: Sha256Digest,
    pub(crate) request: ContextRequestV3,
    pub(crate) shape: Shape,
    /// Degrade-to-fit disclosures, fitted at dispatch, for the reply.
    pub(crate) degradations: Vec<Degradation>,
    /// Milliseconds from dispatch to the job's own deadline.
    pub(crate) run_ms: u64,
    /// The job waited past its caller's deadline and runs on a fresh window.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the live work judges its window by `run_ms` alone"
        )
    )]
    pub(crate) requeued: bool,
    /// The publication the table expects the job to run on, when known.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the live work runs on its caller's verified publication"
        )
    )]
    pub(crate) publication: Option<Sha256Digest>,
}

#[derive(Debug)]
pub(crate) enum Submitted {
    /// Joined an existing job with the same scope and request.
    Attached(Sha256Digest),
    /// A new job whose estimate fits the deadline, perhaps degraded.
    Admitted(Sha256Digest),
    /// A new queued job that cannot fit even degraded; come back later.
    Ticketed(AnswerTicket),
    /// The scope, the principal or the table is full; no job was made.
    Busy { retry_after_ms: u64 },
}

/// A job as its caller sees it. Degradations are the dispatched ones, the
/// same for the caller that made the job and every caller that attached.
#[derive(Debug)]
pub(crate) enum JobView<R> {
    Queued(AnswerTicket),
    Running {
        ticket: AnswerTicket,
        degradations: Vec<Degradation>,
    },
    Finished {
        outcome: Arc<JobOutcome<R>>,
        degradations: Vec<Degradation>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryState {
    Queued,
    Running,
}

/// One queued or running job in status. No id, digest, scope or question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusEntry {
    pub(crate) state: EntryState,
    pub(crate) position: u32,
    pub(crate) waited_ms: u64,
    pub(crate) estimated_start_ms: u64,
    pub(crate) estimated_finish_ms: u64,
    pub(crate) shape: Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CostSample {
    pub(crate) model: Option<ContextRung>,
    pub(crate) estimated_ms: u64,
    pub(crate) elapsed_ms: u64,
    /// The run reached its deadline, so it taught the correction nothing.
    pub(crate) cut: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueueStatus {
    pub(crate) gathering_limit: usize,
    pub(crate) running: usize,
    pub(crate) queued: usize,
    pub(crate) retained_results: usize,
    pub(crate) retained_bytes: usize,
    pub(crate) entries: Vec<StatusEntry>,
    pub(crate) correction_permille: u64,
    pub(crate) recent: Vec<CostSample>,
}

enum State<R> {
    Queued,
    Running {
        started_ms: u64,
        estimated_ms: u64,
        run_ms: u64,
        /// Admitted at the smallest shape although it fits no window: its
        /// elapsed time says nothing about the cost table.
        forced: bool,
    },
    Finished {
        finished_ms: u64,
        outcome: Arc<JobOutcome<R>>,
    },
}

struct Job<R> {
    scope: AnswerScope,
    request_digest: Sha256Digest,
    request: ContextRequestV3,
    requested: Shape,
    deadline_ms: u64,
    enqueued_ms: u64,
    /// Absolute deadline of the job's run.
    due_ms: u64,
    /// The deadline the job is ordered by: its own, or the one of the job
    /// whose place in line it took over (a claim).
    order_due_ms: u64,
    /// Admitted without a fit; its window opens at dispatch.
    ticketed: bool,
    seq: u64,
    /// Expected at dispatch, then the one the runner reports.
    publication: Option<Sha256Digest>,
    /// A newer publication arrived while it ran: never attached, not kept.
    superseded: bool,
    /// The shape and disclosures fitted at dispatch.
    applied: Option<Fitted>,
    state: State<R>,
}
impl<R> Job<R> {
    fn order(&self, ageing: u64) -> (u64, u64) {
        (
            self.order_due_ms
                .saturating_add(ageing.saturating_mul(self.enqueued_ms)),
            self.seq,
        )
    }
    fn active(&self) -> bool {
        !matches!(self.state, State::Finished { .. })
    }
    fn degradations(&self) -> Vec<Degradation> {
        self.applied
            .as_ref()
            .map(|fitted| fitted.degradations.clone())
            .unwrap_or_default()
    }
}

struct Planned {
    start_ms: u64,
    finish_ms: u64,
}

#[derive(Clone, Copy)]
struct Observed {
    incarnation: Incarnation,
    generation: u64,
    digest: Sha256Digest,
}

/// The place in line of a job that was dropped without running. A retry of
/// the same request in the same scope, by digest or by the job's id as its
/// continuation, is admitted afresh in that place.
struct Claim {
    id: Sha256Digest,
    order_due_ms: u64,
    enqueued_ms: u64,
    expires_ms: u64,
}

pub(crate) struct Schedule<R> {
    config: JobTableConfig,
    salt: [u8; 32],
    seq: u64,
    jobs: BTreeMap<Sha256Digest, Job<R>>,
    by_request: HashMap<(AnswerScope, Sha256Digest), Sha256Digest>,
    /// Kept for the retention time, at most one per scope and request.
    claims: HashMap<(AnswerScope, Sha256Digest), Claim>,
    /// The newest publication observed per index.
    current: HashMap<[u8; 16], Observed>,
    correction_permille: u64,
    recent: VecDeque<CostSample>,
}

impl<R> Schedule<R> {
    pub(crate) fn new(config: JobTableConfig, salt: [u8; 32]) -> Self {
        Self {
            config,
            salt,
            seq: 0,
            jobs: BTreeMap::new(),
            by_request: HashMap::new(),
            claims: HashMap::new(),
            current: HashMap::new(),
            correction_permille: 1000,
            recent: VecDeque::new(),
        }
    }

    pub(crate) fn gathering_limit(&self) -> usize {
        gathering_limit(self.config.runtime_capacity, self.config.user_cap)
    }
    /// L5b's runtime capacity figure, injected when it changes.
    pub(crate) fn set_runtime_capacity(&mut self, capacity: Option<usize>) {
        self.config.runtime_capacity = capacity;
    }
    /// Measured costs: from here on they may also reduce a job that has its
    /// whole window.
    pub(crate) fn set_costs(&mut self, costs: CostTable) {
        self.config.costs = costs;
        self.config.costs_measured = true;
    }
    /// The costs a job asking for `model` is fitted with.
    fn costs_for(&self, model: Option<ContextRung>) -> CostTable {
        if self.config.configured_rung_only {
            self.config.costs.only(model)
        } else {
            self.config.costs
        }
    }
    /// The first shape of `requested`'s ladder that fits `available_ms`.
    /// Costs that were not measured only order and bound the queue: they
    /// never reduce a job that still has its whole window.
    fn fit(&self, requested: Shape, available_ms: u64, deadline_ms: u64) -> Option<Fitted> {
        let costs = self.costs_for(requested.model);
        if !self.config.costs_measured && available_ms >= deadline_ms {
            return Some(estimate::unreduced(
                &costs,
                self.correction_permille,
                requested,
                deadline_ms,
            ));
        }
        estimate::degrade_to_fit(
            &costs,
            self.correction_permille,
            requested,
            available_ms,
            deadline_ms,
        )
    }

    fn current_of(&self, index: &[u8; 16]) -> Option<Sha256Digest> {
        self.current.get(index).map(|observed| observed.digest)
    }

    /// A started or finished job may serve a caller on `caller`'s
    /// publication (or, when the caller does not know it, the newest one
    /// observed). Queued jobs have not run yet and always may.
    fn publication_ok(&self, job: &Job<R>, caller: Option<Sha256Digest>) -> bool {
        if matches!(job.state, State::Queued) {
            return true;
        }
        if job.superseded {
            return false;
        }
        match (
            caller.or(self.current_of(&job.scope.index)),
            job.publication,
        ) {
            (Some(known), Some(ran_on)) => known == ran_on,
            _ => true,
        }
    }

    /// Admit, attach or ticket one request (plan section 12.4.1, ideas 1 to 3).
    /// `publication` is the caller's current publication when it is known.
    pub(crate) fn submit(
        &mut self,
        scope: AnswerScope,
        mut request: ContextRequestV3,
        model: Option<ContextRung>,
        publication: Option<Sha256Digest>,
        now: u64,
    ) -> Result<Submitted> {
        request.validate()?;
        self.purge(now);
        let digest = request_digest(&request)?;
        if let Some(continuation) = request.continuation {
            match self
                .jobs
                .get(&continuation)
                .filter(|job| job.scope == scope)
            {
                Some(job) => {
                    ensure!(
                        job.request_digest == digest,
                        "Answer continuation belongs to a different request"
                    );
                    ensure!(
                        self.publication_ok(job, publication),
                        "Answer continuation superseded by a newer publication"
                    );
                    return Ok(Submitted::Attached(continuation));
                }
                // The ticket of a job that was dropped without running still
                // holds its place for this scope and request: the call is
                // admitted afresh below, in that place.
                None if self
                    .claims
                    .get(&(scope, digest))
                    .is_some_and(|claim| claim.id == continuation) =>
                {
                    request.continuation = None;
                }
                // Unknown, expired and foreign ids are one indistinguishable
                // refusal: a digest never reveals another scope's job.
                None => bail!("unknown or expired Answer continuation"),
            }
        }

        // What this call would get on its own, to decide whether attaching
        // to an existing job would give it less.
        let deadline_ms = request.budget.deadline_ms;
        let requested = Shape::requested(&request.budget, model);
        let due_ms = now.saturating_add(deadline_ms);
        self.seq += 1;
        let seq = self.seq;
        // A retry of a job that was dropped without running is ordered as
        // that job was: it takes its place in line, with its own window.
        let place = self
            .claims
            .get(&(scope, digest))
            .map(|claim| (claim.order_due_ms, claim.enqueued_ms.min(now)));
        let mut job = Job {
            scope,
            request_digest: digest,
            request,
            requested,
            deadline_ms,
            enqueued_ms: place.map_or(now, |(_, enqueued_ms)| enqueued_ms),
            due_ms,
            order_due_ms: place.map_or(due_ms, |(order_due_ms, _)| order_due_ms),
            ticketed: false,
            seq,
            publication,
            superseded: false,
            applied: None,
            state: State::Queued,
        };
        let start_ms = self.start_for(&job, now);
        let costs = self.costs_for(requested.model);
        let correction = self.correction_permille;
        let fitted = self.fit(requested, due_ms.saturating_sub(start_ms), deadline_ms);
        let smallest_fits_alone = costs
            .work_ms(&Shape::SMALLEST, correction)
            .is_some_and(|work| work <= deadline_ms);
        let own: Option<Shape> = match &fitted {
            Some(fitted) => Some(fitted.shape),
            None if !smallest_fits_alone => Some(Shape::SMALLEST),
            None => None,
        };

        if let Some(id) = self.by_request.get(&(scope, digest)).copied()
            && let Some(existing) = self.jobs.get(&id)
            && self.attachable(existing, publication, own, due_ms, deadline_ms)
        {
            return Ok(Submitted::Attached(id));
        }

        let (mut scope_active, mut principal_active, mut total_active) = (0, 0, 0);
        for active in self.jobs.values().filter(|job| job.active()) {
            scope_active += usize::from(active.scope == scope);
            principal_active += usize::from(active.scope.principal == scope.principal);
            total_active += 1;
        }
        if scope_active >= self.config.max_active_per_scope
            || principal_active >= self.config.max_active_per_principal
            || total_active >= self.config.max_jobs
        {
            return Ok(Submitted::Busy {
                retry_after_ms: self.next_free_ms(now).clamp(1, MAX_DEADLINE_MS),
            });
        }

        let id = framed_digest(
            b"airk.answer.job.id/1",
            &[
                &self.salt,
                scope.authority.as_bytes(),
                &scope.index,
                digest.as_bytes(),
                &seq.to_be_bytes(),
            ],
        );
        let ticketed = own.is_none();
        if ticketed {
            // Degrade first, then a ticket: its window opens at its
            // estimated start, and it is ordered by that window.
            job.ticketed = true;
            job.due_ms = start_ms.saturating_add(deadline_ms);
            if place.is_none() {
                job.order_due_ms = job.due_ms;
            }
        }
        // The place is taken over once, by the job made here.
        self.claims.remove(&(scope, digest));
        self.by_request.insert((scope, digest), id);
        self.jobs.insert(id, job);
        Ok(if ticketed {
            Submitted::Ticketed(self.ticket(&id, now).expect("the job was just queued"))
        } else {
            Submitted::Admitted(id)
        })
    }

    /// Attach by digest only when the existing job gives this call no less
    /// than its own admission would: a retry with a longer deadline never
    /// silently receives a shorter run's more degraded answer.
    fn attachable(
        &self,
        existing: &Job<R>,
        publication: Option<Sha256Digest>,
        own: Option<Shape>,
        due_ms: u64,
        deadline_ms: u64,
    ) -> bool {
        if !self.publication_ok(existing, publication) {
            return false;
        }
        match &existing.state {
            State::Finished { outcome, .. }
                if matches!(outcome.as_ref(), JobOutcome::Failed(_)) =>
            {
                // Uncertain work is never replayed on the same publication.
                true
            }
            State::Queued if existing.ticketed => existing.deadline_ms >= deadline_ms,
            State::Queued => existing.due_ms >= due_ms,
            _ => match (existing.applied.as_ref(), own) {
                (Some(applied), Some(own)) => applied.shape.covers(&own),
                // This call could only have a ticket: the running or
                // finished job is sooner and discloses what it reduced.
                (_, None) => true,
                (None, Some(_)) => false,
            },
        }
    }

    /// Start queued jobs, earliest key first, while the gathering limit allows.
    pub(crate) fn dispatch(&mut self, now: u64) -> Vec<StartedJob> {
        let limit = self.gathering_limit();
        let mut started = vec![];
        while self.running() < limit {
            let ageing = self.config.ageing;
            let Some(id) = self
                .jobs
                .iter()
                .filter(|(_, job)| matches!(job.state, State::Queued))
                .min_by_key(|(_, job)| job.order(ageing))
                .map(|(id, _)| *id)
            else {
                break;
            };
            let (requested, deadline_ms, ticketed, mut due_ms) = {
                let job = &self.jobs[&id];
                (job.requested, job.deadline_ms, job.ticketed, job.due_ms)
            };
            let costs = self.costs_for(requested.model);
            let correction = self.correction_permille;
            let current = self.current_of(&self.jobs[&id].scope.index);
            if ticketed {
                due_ms = now.saturating_add(deadline_ms);
            }
            let remaining = due_ms.saturating_sub(now);
            let mut requeued = false;
            let mut fitted = self.fit(requested, remaining, deadline_ms);
            if fitted.is_none() && remaining < deadline_ms {
                // The wait consumed the caller's window: the work outlives
                // the call, so it runs on a fresh window and callers attach.
                requeued = !ticketed;
                due_ms = now.saturating_add(deadline_ms);
                fitted = self.fit(requested, deadline_ms, deadline_ms);
            }
            let forced = fitted.is_none();
            let fitted = fitted
                .unwrap_or_else(|| estimate::smallest(&costs, correction, requested, deadline_ms));
            let job = self.jobs.get_mut(&id).expect("selected job exists");
            job.due_ms = due_ms;
            let run_ms = job.due_ms.saturating_sub(now).max(1);
            job.publication = current.or(job.publication);
            job.state = State::Running {
                started_ms: now,
                estimated_ms: fitted.work_ms,
                run_ms,
                forced,
            };
            started.push(StartedJob {
                id,
                request: job.request.clone(),
                shape: fitted.shape,
                degradations: fitted.degradations.clone(),
                run_ms,
                requeued,
                publication: job.publication,
            });
            job.applied = Some(fitted);
        }
        started
    }

    /// Record a running job's outcome and learn from its elapsed time. A
    /// result of a superseded or no longer current publication goes to its
    /// waiters but is not kept.
    pub(crate) fn finish(
        &mut self,
        id: &Sha256Digest,
        outcome: JobOutcome<R>,
        now: u64,
    ) -> Arc<JobOutcome<R>> {
        let outcome = Arc::new(outcome);
        let dropped = matches!(outcome.as_ref(), JobOutcome::Dropped { .. });
        let mut stale = false;
        let current = self
            .jobs
            .get(id)
            .and_then(|job| self.current_of(&job.scope.index));
        if let Some(job) = self.jobs.get(id)
            && matches!(outcome.as_ref(), JobOutcome::Dropped { not_run: true, .. })
        {
            // The job never ran: its place in line is kept for a retry.
            let key = (job.scope, job.request_digest);
            let claim = Claim {
                id: *id,
                order_due_ms: job.order_due_ms,
                enqueued_ms: job.enqueued_ms,
                expires_ms: now.saturating_add(self.config.retention_ms),
            };
            if self.claims.len() >= self.config.max_jobs && !self.claims.contains_key(&key) {
                let oldest = self
                    .claims
                    .iter()
                    .min_by_key(|(_, claim)| claim.expires_ms)
                    .map(|(key, _)| *key);
                if let Some(oldest) = oldest {
                    self.claims.remove(&oldest);
                }
            }
            self.claims.insert(key, claim);
        }
        if let Some(job) = self.jobs.get_mut(id) {
            if let State::Running {
                started_ms,
                estimated_ms,
                run_ms,
                forced,
            } = job.state
                && !dropped
            {
                let elapsed_ms = now.saturating_sub(started_ms);
                // A run that reached its deadline, or one admitted smallest
                // beyond it, says only that the work was at least that long.
                let cut = forced || elapsed_ms.saturating_add(DEADLINE_TOLERANCE_MS) >= run_ms;
                if matches!(outcome.as_ref(), JobOutcome::Done { .. }) && estimated_ms > 0 && !cut {
                    let ratio = (elapsed_ms.saturating_mul(1000) / estimated_ms)
                        .clamp(CORRECTION_MIN, CORRECTION_MAX);
                    self.correction_permille = ((self.correction_permille * 3 + ratio) / 4)
                        .clamp(CORRECTION_MIN, CORRECTION_MAX);
                }
                self.recent.push_back(CostSample {
                    model: job.applied.as_ref().and_then(|fitted| fitted.shape.model),
                    estimated_ms,
                    elapsed_ms,
                    cut,
                });
                if self.recent.len() > RECENT_SAMPLES {
                    self.recent.pop_front();
                }
            }
            if let JobOutcome::Done { publication, .. } = outcome.as_ref() {
                job.publication = Some(*publication);
            }
            stale = dropped
                || job.superseded
                || current.is_some_and(|current| job.publication != Some(current));
            job.state = State::Finished {
                finished_ms: now,
                outcome: outcome.clone(),
            };
        }
        if stale {
            self.remove(id);
        }
        self.enforce_limits();
        outcome
    }

    /// Drop finished results older than the retention window.
    pub(crate) fn purge(&mut self, now: u64) {
        let retention = self.config.retention_ms;
        let expired: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, job)| match job.state {
                State::Finished { finished_ms, .. } => finished_ms.saturating_add(retention) <= now,
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.remove(&id);
        }
        self.claims.retain(|_, claim| claim.expires_ms > now);
    }

    /// The service's publication observer reports `publication` for `index`.
    /// An older generation of the same incarnation is ignored (`false`).
    /// Otherwise every finished result of another publication, failures
    /// included, is dropped, and jobs running on another one are superseded.
    pub(crate) fn publication_changed(
        &mut self,
        index: IndexId,
        publication: &RevisionEnvelope,
    ) -> Result<bool> {
        let index = *index.identity().as_bytes();
        let observed = Observed {
            incarnation: publication.incarnation,
            generation: publication.generation,
            digest: publication_digest(publication)?,
        };
        if let Some(last) = self.current.get(&index)
            && last.incarnation == observed.incarnation
            && observed.generation < last.generation
        {
            return Ok(false);
        }
        self.current.insert(index, observed);
        let mut stale = vec![];
        for (id, job) in self.jobs.iter_mut() {
            if job.scope.index != index || job.publication == Some(observed.digest) {
                continue;
            }
            match job.state {
                State::Finished { .. } => stale.push(*id),
                State::Running { .. } => job.superseded = true,
                State::Queued => {}
            }
        }
        for id in stale {
            self.remove(&id);
        }
        self.by_request
            .retain(|_, id| self.jobs.get(id).is_some_and(|job| !job.superseded));
        Ok(true)
    }

    /// The caller's view of one job, only within its own scope.
    pub(crate) fn view(
        &self,
        scope: &AnswerScope,
        id: &Sha256Digest,
        now: u64,
    ) -> Option<JobView<R>> {
        let job = self.jobs.get(id).filter(|job| job.scope == *scope)?;
        Some(match &job.state {
            State::Finished { outcome, .. } => JobView::Finished {
                outcome: outcome.clone(),
                degradations: job.degradations(),
            },
            State::Running { .. } => JobView::Running {
                ticket: self.ticket(id, now)?,
                degradations: job.degradations(),
            },
            State::Queued => JobView::Queued(self.ticket(id, now)?),
        })
    }

    /// The ticket of a queued or running job, times from `now`.
    pub(crate) fn ticket(&self, id: &Sha256Digest, now: u64) -> Option<AnswerTicket> {
        let job = self.jobs.get(id)?;
        let (position, start_ms, finish_ms) = match job.state {
            State::Running { .. } => (1, now, self.running_end(job, now)),
            State::Queued => {
                let plan = self.plan(now);
                let (position, planned) = plan
                    .iter()
                    .enumerate()
                    .find(|(_, (queued, _))| queued == id)
                    .map(|(index, (_, planned))| (index + 1, planned))?;
                (position, planned.start_ms, planned.finish_ms)
            }
            State::Finished { .. } => return None,
        };
        let estimated_finish_ms = finish_ms.saturating_sub(now).clamp(1, MAX_ESTIMATE_MS);
        Some(AnswerTicket {
            id: *id,
            queue_position: u32::try_from(position)
                .unwrap_or(MAX_QUEUE_POSITION)
                .clamp(1, MAX_QUEUE_POSITION),
            estimated_start_ms: start_ms.saturating_sub(now).min(estimated_finish_ms),
            estimated_finish_ms,
            retry_after_ms: estimated_finish_ms.min(MAX_DEADLINE_MS),
        })
    }

    /// Aggregate queue status: counts, positions, estimates and recent costs.
    pub(crate) fn status(&self, now: u64) -> QueueStatus {
        let mut entries: Vec<_> = self
            .jobs
            .values()
            .filter_map(|job| match job.state {
                State::Running { started_ms, .. } => Some(StatusEntry {
                    state: EntryState::Running,
                    position: 0,
                    waited_ms: started_ms.saturating_sub(job.enqueued_ms),
                    estimated_start_ms: 0,
                    estimated_finish_ms: self.running_end(job, now).saturating_sub(now),
                    shape: job
                        .applied
                        .as_ref()
                        .map_or(job.requested, |fitted| fitted.shape),
                }),
                _ => None,
            })
            .collect();
        for (index, (id, planned)) in self.plan(now).into_iter().enumerate() {
            let job = &self.jobs[&id];
            entries.push(StatusEntry {
                state: EntryState::Queued,
                position: u32::try_from(index + 1).unwrap_or(u32::MAX),
                waited_ms: now.saturating_sub(job.enqueued_ms),
                estimated_start_ms: planned.start_ms.saturating_sub(now),
                estimated_finish_ms: planned.finish_ms.saturating_sub(now),
                shape: job.requested,
            });
        }
        let finished = self.jobs.values().filter(|job| !job.active());
        QueueStatus {
            gathering_limit: self.gathering_limit(),
            running: self.running(),
            queued: self
                .jobs
                .values()
                .filter(|job| matches!(job.state, State::Queued))
                .count(),
            retained_results: finished.clone().count(),
            retained_bytes: finished.map(retained_bytes).sum(),
            entries,
            correction_permille: self.correction_permille,
            recent: self.recent.iter().copied().collect(),
        }
    }

    fn running(&self) -> usize {
        self.jobs
            .values()
            .filter(|job| matches!(job.state, State::Running { .. }))
            .count()
    }

    fn running_end(&self, job: &Job<R>, now: u64) -> u64 {
        match job.state {
            State::Running {
                started_ms,
                estimated_ms,
                ..
            } => started_ms
                .saturating_add(estimated_ms)
                .min(job.due_ms.saturating_add(DEADLINE_TOLERANCE_MS))
                .max(now),
            _ => now,
        }
    }

    /// The slot-free times of the running jobs, as a min-heap.
    fn slots(&self, now: u64) -> BinaryHeap<Reverse<u64>> {
        self.jobs
            .values()
            .filter(|job| matches!(job.state, State::Running { .. }))
            .map(|job| Reverse(self.running_end(job, now)))
            .collect()
    }

    fn take_slot(slots: &mut BinaryHeap<Reverse<u64>>, limit: usize, now: u64) -> u64 {
        let mut free = now;
        while slots.len() >= limit {
            let Reverse(end) = slots.pop().expect("limit is at least the floor");
            free = free.max(end);
        }
        free
    }

    /// Queued jobs in dispatch order with their simulated start and finish.
    fn plan(&self, now: u64) -> Vec<(Sha256Digest, Planned)> {
        let ageing = self.config.ageing;
        let limit = self.gathering_limit();
        let mut queued: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, job)| matches!(job.state, State::Queued))
            .collect();
        queued.sort_by_key(|(_, job)| job.order(ageing));
        let mut slots = self.slots(now);
        queued
            .into_iter()
            .map(|(id, job)| {
                let start_ms = Self::take_slot(&mut slots, limit, now);
                let finish_ms = start_ms.saturating_add(self.work_at(job, start_ms));
                slots.push(Reverse(finish_ms));
                (
                    *id,
                    Planned {
                        start_ms,
                        finish_ms,
                    },
                )
            })
            .collect()
    }

    /// The estimated work of `job` if it started at `start_ms`.
    fn work_at(&self, job: &Job<R>, start_ms: u64) -> u64 {
        let costs = &self.costs_for(job.requested.model);
        let available = if job.ticketed {
            job.deadline_ms
        } else {
            job.due_ms.saturating_sub(start_ms)
        };
        self.fit(job.requested, available, job.deadline_ms)
            .unwrap_or_else(|| {
                estimate::smallest(
                    costs,
                    self.correction_permille,
                    job.requested,
                    job.deadline_ms,
                )
            })
            .work_ms
    }

    /// The simulated start of a job not yet in the table.
    fn start_for(&self, job: &Job<R>, now: u64) -> u64 {
        let ageing = self.config.ageing;
        let limit = self.gathering_limit();
        let order = job.order(ageing);
        let mut ahead: Vec<_> = self
            .jobs
            .values()
            .filter(|queued| matches!(queued.state, State::Queued) && queued.order(ageing) < order)
            .collect();
        ahead.sort_by_key(|queued| queued.order(ageing));
        let mut slots = self.slots(now);
        for queued in ahead {
            let start_ms = Self::take_slot(&mut slots, limit, now);
            slots.push(Reverse(
                start_ms.saturating_add(self.work_at(queued, start_ms)),
            ));
        }
        Self::take_slot(&mut slots, limit, now)
    }

    fn next_free_ms(&self, now: u64) -> u64 {
        self.slots(now)
            .peek()
            .map_or(1, |Reverse(end)| end.saturating_sub(now))
    }

    fn remove(&mut self, id: &Sha256Digest) {
        if let Some(job) = self.jobs.remove(id) {
            let key = (job.scope, job.request_digest);
            if self.by_request.get(&key) == Some(id) {
                self.by_request.remove(&key);
            }
        }
    }

    /// Keep retained results within the byte and count caps, oldest first.
    fn enforce_limits(&mut self) {
        loop {
            let finished = self.jobs.values().filter(|job| !job.active());
            let count = finished.clone().count();
            let bytes: usize = finished.map(retained_bytes).sum();
            if bytes <= self.config.max_retained_bytes && count <= self.config.max_retained_results
            {
                return;
            }
            let oldest = self
                .jobs
                .iter()
                .filter_map(|(id, job)| match job.state {
                    State::Finished { finished_ms, .. } => Some((finished_ms, job.seq, *id)),
                    _ => None,
                })
                .min();
            let Some((_, _, id)) = oldest else { return };
            self.remove(&id);
        }
    }
}

/// A finished entry's charge: its result or failure, its question and a
/// fixed overhead, so no entry is ever free.
fn retained_bytes<R>(job: &Job<R>) -> usize {
    match &job.state {
        State::Finished { outcome, .. } => {
            let result = match outcome.as_ref() {
                JobOutcome::Done { retained_bytes, .. } => *retained_bytes,
                JobOutcome::Failed(message) | JobOutcome::Dropped { message, .. } => message.len(),
            };
            result
                .saturating_add(job.request.question.len())
                .saturating_add(ENTRY_OVERHEAD_BYTES)
        }
        _ => 0,
    }
}
