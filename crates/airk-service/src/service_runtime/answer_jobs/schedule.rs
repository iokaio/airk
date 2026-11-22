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
    identity::{Sha256Digest, framed_digest},
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
pub(crate) const DEFAULT_ACTIVE_PER_SCOPE: usize = 8;
/// Learned correction bounds, in thousandths of the cost table.
const CORRECTION_MIN: u64 = 250;
const CORRECTION_MAX: u64 = 4000;
const RECENT_SAMPLES: usize = 16;

/// Who may see a job. Two callers share a job only when their authority
/// digests are equal: the installation, catalog, index and the caller's
/// source-authority fingerprint all match. The publication is not part of
/// it; a finished result additionally requires the same publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct AnswerScope {
    authority: Sha256Digest,
    index: [u8; 16],
}
impl AnswerScope {
    pub(crate) fn from_binding(binding: &ScopeBinding) -> Self {
        let index = *binding.index_id.identity().as_bytes();
        Self {
            authority: framed_digest(
                b"airk.answer.job.scope/1",
                &[
                    binding.installation.as_bytes(),
                    binding.catalog.as_bytes(),
                    &index,
                    binding.authorization.as_bytes(),
                ],
            ),
            index,
        }
    }
    /// The publication digest a finished result is matched on.
    pub(crate) fn publication(binding: &ScopeBinding) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk.answer.job.publication/1",
            &[&serde_json::to_vec(&binding.publication)?],
        ))
    }
    pub(crate) fn index(&self) -> [u8; 16] {
        self.index
    }
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
    /// Concurrent gatherings the model lanes can serve, from L5b's runtime
    /// capacity rules. `None` until that figure exists.
    pub(crate) runtime_capacity: Option<usize>,
    /// The user's cap on the automatic figure; it never goes below the floor.
    pub(crate) user_cap: Option<usize>,
    /// Queued and running jobs one scope may hold.
    pub(crate) max_active_per_scope: usize,
    /// Queued and running jobs in the whole table.
    pub(crate) max_jobs: usize,
    /// Ageing weight: the EDF key is `due + ageing x enqueued`.
    pub(crate) ageing: u64,
    pub(crate) costs: CostTable,
}
impl Default for JobTableConfig {
    fn default() -> Self {
        Self {
            retention_ms: DEFAULT_RETENTION_MS,
            max_retained_bytes: DEFAULT_RETAINED_BYTES,
            runtime_capacity: None,
            user_cap: None,
            max_active_per_scope: DEFAULT_ACTIVE_PER_SCOPE,
            max_jobs: MAX_QUEUE_POSITION as usize,
            ageing: 1,
            costs: CostTable::default(),
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
}

/// What a dispatched job runs with.
#[derive(Clone, Debug)]
pub(crate) struct StartedJob {
    pub(crate) id: Sha256Digest,
    pub(crate) request: ContextRequestV3,
    pub(crate) shape: Shape,
    /// Admission's degrade-to-fit disclosures, for the reply.
    pub(crate) degradations: Vec<Degradation>,
    /// Milliseconds from dispatch to the job's own deadline.
    pub(crate) run_ms: u64,
    /// The job waited past its caller's deadline and runs on a fresh window.
    pub(crate) requeued: bool,
}

#[derive(Debug)]
pub(crate) enum Submitted {
    /// Joined an existing job with the same scope and request.
    Attached(Sha256Digest),
    /// A new job whose estimate fits the deadline, perhaps degraded.
    Admitted {
        id: Sha256Digest,
        degradations: Vec<Degradation>,
    },
    /// A new queued job that cannot fit even degraded; come back later.
    Ticketed(AnswerTicket),
    /// The scope or the table is full; no job was made.
    Busy { retry_after_ms: u64 },
}

/// A job as its caller sees it.
#[derive(Debug)]
pub(crate) enum JobView<R> {
    Queued(AnswerTicket),
    Running(AnswerTicket),
    Finished(Arc<JobOutcome<R>>),
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
        shape: Shape,
        estimated_ms: u64,
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
    /// Admitted without a fit; its window opens at dispatch.
    ticketed: bool,
    seq: u64,
    state: State<R>,
}
impl<R> Job<R> {
    fn order(&self, ageing: u64) -> (u64, u64) {
        (
            self.due_ms
                .saturating_add(ageing.saturating_mul(self.enqueued_ms)),
            self.seq,
        )
    }
    fn active(&self) -> bool {
        !matches!(self.state, State::Finished { .. })
    }
}

struct Planned {
    start_ms: u64,
    finish_ms: u64,
}

pub(crate) struct Schedule<R> {
    config: JobTableConfig,
    salt: [u8; 32],
    seq: u64,
    jobs: BTreeMap<Sha256Digest, Job<R>>,
    by_request: HashMap<(AnswerScope, Sha256Digest), Sha256Digest>,
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
    pub(crate) fn set_costs(&mut self, costs: CostTable) {
        self.config.costs = costs;
    }

    /// Admit, attach or ticket one request (plan section 12.4.1, ideas 1 to 3).
    /// `publication` is the caller's current publication when it is known;
    /// a finished result on another publication is never attached.
    pub(crate) fn submit(
        &mut self,
        scope: AnswerScope,
        request: ContextRequestV3,
        model: Option<ContextRung>,
        publication: Option<Sha256Digest>,
        now: u64,
    ) -> Result<Submitted> {
        request.validate()?;
        self.purge(now);
        let digest = request_digest(&request)?;
        if let Some(continuation) = request.continuation {
            // Unknown, expired and foreign ids are one indistinguishable
            // refusal: a digest never reveals another scope's job.
            let Some(job) = self
                .jobs
                .get(&continuation)
                .filter(|job| job.scope == scope)
            else {
                bail!("unknown or expired Answer continuation");
            };
            ensure!(
                job.request_digest == digest,
                "Answer continuation belongs to a different request"
            );
            return Ok(Submitted::Attached(continuation));
        }
        if let Some(id) = self.by_request.get(&(scope, digest)).copied()
            && let Some(job) = self.jobs.get(&id)
        {
            let reusable = match &job.state {
                State::Finished { outcome, .. } => match outcome.as_ref() {
                    JobOutcome::Done {
                        publication: ran_on,
                        ..
                    } => publication.is_none_or(|current| current == *ran_on),
                    JobOutcome::Failed(_) => true,
                },
                _ => true,
            };
            if reusable {
                return Ok(Submitted::Attached(id));
            }
        }
        let active = self.jobs.values().filter(|job| job.active());
        let (scope_active, total_active) = active.fold((0, 0), |(mine, all), job| {
            (mine + usize::from(job.scope == scope), all + 1)
        });
        if scope_active >= self.config.max_active_per_scope || total_active >= self.config.max_jobs
        {
            return Ok(Submitted::Busy {
                retry_after_ms: self.next_free_ms(now).clamp(1, MAX_DEADLINE_MS),
            });
        }

        let deadline_ms = request.budget.deadline_ms;
        let requested = Shape::requested(&request.budget, model);
        let due_ms = now.saturating_add(deadline_ms);
        self.seq += 1;
        let seq = self.seq;
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
        let mut job = Job {
            scope,
            request_digest: digest,
            request,
            requested,
            deadline_ms,
            enqueued_ms: now,
            due_ms,
            ticketed: false,
            seq,
            state: State::Queued,
        };
        let start_ms = self.start_for(&job, now);
        let available = due_ms.saturating_sub(start_ms);
        let costs = self.config.costs;
        let correction = self.correction_permille;
        let fitted =
            estimate::degrade_to_fit(&costs, correction, requested, available, deadline_ms);
        let smallest_fits_alone = costs
            .work_ms(&Shape::SMALLEST, correction)
            .is_some_and(|work| work <= deadline_ms);
        let submitted = match fitted {
            Some(fitted) => Submitted::Admitted {
                id,
                degradations: fitted.degradations,
            },
            None if !smallest_fits_alone => Submitted::Admitted {
                id,
                degradations: estimate::smallest(&costs, correction, requested, deadline_ms)
                    .degradations,
            },
            None => {
                // Degrade first, then a ticket: its window opens at its
                // estimated start, and it is ordered by that window.
                job.ticketed = true;
                job.due_ms = start_ms.saturating_add(deadline_ms);
                Submitted::Ticketed(AnswerTicket {
                    id,
                    queue_position: 1,
                    estimated_start_ms: 0,
                    estimated_finish_ms: 1,
                    retry_after_ms: 1,
                })
            }
        };
        self.by_request.insert((scope, digest), id);
        self.jobs.insert(id, job);
        Ok(match submitted {
            Submitted::Ticketed(_) => {
                Submitted::Ticketed(self.ticket(&id, now).expect("the job was just queued"))
            }
            other => other,
        })
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
            let costs = self.config.costs;
            let correction = self.correction_permille;
            let job = self.jobs.get_mut(&id).expect("selected job exists");
            let mut requeued = false;
            if job.ticketed {
                job.due_ms = now.saturating_add(job.deadline_ms);
            }
            let fitted = estimate::degrade_to_fit(
                &costs,
                correction,
                job.requested,
                job.due_ms.saturating_sub(now),
                job.deadline_ms,
            )
            .or_else(|| {
                // The wait outran the caller's deadline: the work outlives the
                // call, so it runs on a fresh window and the caller attaches.
                requeued = !job.ticketed;
                job.due_ms = now.saturating_add(job.deadline_ms);
                estimate::degrade_to_fit(
                    &costs,
                    correction,
                    job.requested,
                    job.deadline_ms,
                    job.deadline_ms,
                )
            })
            .unwrap_or_else(|| {
                estimate::smallest(&costs, correction, job.requested, job.deadline_ms)
            });
            let Fitted {
                shape,
                work_ms,
                degradations,
            } = fitted;
            job.state = State::Running {
                started_ms: now,
                shape,
                estimated_ms: work_ms,
            };
            started.push(StartedJob {
                id,
                request: job.request.clone(),
                shape,
                degradations,
                run_ms: job.due_ms.saturating_sub(now).max(1),
                requeued,
            });
        }
        started
    }

    /// Record a running job's outcome and learn from its elapsed time.
    pub(crate) fn finish(
        &mut self,
        id: &Sha256Digest,
        outcome: JobOutcome<R>,
        now: u64,
    ) -> Arc<JobOutcome<R>> {
        let outcome = Arc::new(outcome);
        if let Some(job) = self.jobs.get_mut(id) {
            if let State::Running {
                started_ms,
                shape,
                estimated_ms,
            } = job.state
            {
                let elapsed_ms = now.saturating_sub(started_ms);
                if matches!(outcome.as_ref(), JobOutcome::Done { .. }) && estimated_ms > 0 {
                    let ratio = (elapsed_ms.saturating_mul(1000) / estimated_ms)
                        .clamp(CORRECTION_MIN, CORRECTION_MAX);
                    self.correction_permille = ((self.correction_permille * 3 + ratio) / 4)
                        .clamp(CORRECTION_MIN, CORRECTION_MAX);
                }
                self.recent.push_back(CostSample {
                    model: shape.model,
                    estimated_ms,
                    elapsed_ms,
                });
                if self.recent.len() > RECENT_SAMPLES {
                    self.recent.pop_front();
                }
            }
            job.state = State::Finished {
                finished_ms: now,
                outcome: outcome.clone(),
            };
        }
        self.enforce_bytes();
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
    }

    /// A new publication of `index`: results of every other publication on
    /// it are dropped at once.
    pub(crate) fn publication_changed(&mut self, index: [u8; 16], current: Sha256Digest) {
        let stale: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, job)| {
                job.scope.index == index
                    && matches!(&job.state, State::Finished { outcome, .. }
                        if matches!(outcome.as_ref(), JobOutcome::Done { publication, .. } if *publication != current))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.remove(&id);
        }
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
            State::Finished { outcome, .. } => JobView::Finished(outcome.clone()),
            State::Running { .. } => JobView::Running(self.ticket(id, now)?),
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
                State::Running {
                    started_ms, shape, ..
                } => Some(StatusEntry {
                    state: EntryState::Running,
                    position: 0,
                    waited_ms: started_ms.saturating_sub(job.enqueued_ms),
                    estimated_start_ms: 0,
                    estimated_finish_ms: self.running_end(job, now).saturating_sub(now),
                    shape,
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
        let costs = &self.config.costs;
        let available = if job.ticketed {
            job.deadline_ms
        } else {
            job.due_ms.saturating_sub(start_ms)
        };
        estimate::degrade_to_fit(
            costs,
            self.correction_permille,
            job.requested,
            available,
            job.deadline_ms,
        )
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

    fn enforce_bytes(&mut self) {
        let limit = self.config.max_retained_bytes;
        loop {
            let finished = self.jobs.values().filter(|job| !job.active());
            if finished.clone().map(retained_bytes).sum::<usize>() <= limit {
                return;
            }
            let oldest = self
                .jobs
                .iter()
                .filter_map(|(id, job)| match job.state {
                    State::Finished { finished_ms, .. } if retained_bytes(job) > 0 => {
                        Some((finished_ms, job.seq, *id))
                    }
                    _ => None,
                })
                .min();
            let Some((_, _, id)) = oldest else { return };
            self.remove(&id);
        }
    }
}

fn retained_bytes<R>(job: &Job<R>) -> usize {
    match &job.state {
        State::Finished { outcome, .. } => match outcome.as_ref() {
            JobOutcome::Done { retained_bytes, .. } => *retained_bytes,
            JobOutcome::Failed(message) => message.len(),
        },
        _ => 0,
    }
}
