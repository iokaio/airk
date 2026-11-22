// SPDX-License-Identifier: Apache-2.0
//! The service-owned Answer job table (plan section 12.4.1, step L2b): an
//! admitted Answer is a job of the service, not of the client's connection.
//! It keeps running when its caller times out or disconnects, a retry of the
//! same request in the same scope attaches instead of replaying, and its
//! result is kept for a bounded time. Admission degrades to fit the deadline
//! first and returns a ticket only when even the smallest disclosed answer
//! cannot fit. Contract: `answer_jobs/CONTRACT.md`.
//!
//! [`AnswerJobTable::call_with_work`] is the one narrow entry point for the
//! service's Answer path: `wiring` puts the table behind the `airk-mcp` seam
//! the Answer handler calls, and each call brings the work of the job it
//! makes.
mod estimate;
mod reply;
mod schedule;
#[cfg(test)]
mod tests;
mod wiring;
pub(crate) use wiring::{ServiceAnswerJobs, for_control, install};

pub(crate) use estimate::{ContextRung, CostTable};
pub(crate) use schedule::{AnswerScope, JobOutcome, JobTableConfig, QueueStatus, StartedJob};

use airk_core::{
    arbiter::answer_loop::{AnswerTicket, ContextRequestV3, Degradation},
    evidence::RevisionEnvelope,
    identity::Sha256Digest,
    index_catalog::IndexId,
};
use anyhow::{Context, Result, anyhow};
use schedule::{JobView, Schedule, Submitted};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

type Signal<R> = watch::Sender<Option<Arc<JobOutcome<R>>>>;
type Signalled<R> = watch::Receiver<Option<Arc<JobOutcome<R>>>>;

/// The table's injected clock, in milliseconds from an arbitrary origin.
pub(crate) trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Production clock on Tokio's time, so paused-time tests drive it too.
pub(crate) struct TokioClock {
    origin: tokio::time::Instant,
}
impl TokioClock {
    pub(crate) fn new() -> Self {
        Self {
            origin: tokio::time::Instant::now(),
        }
    }
}
impl Clock for TokioClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// What the runner returns for one finished Answer.
pub(crate) struct Finished<R> {
    pub(crate) value: R,
    pub(crate) publication: Sha256Digest,
    pub(crate) retained_bytes: usize,
}

pub(crate) type RunFuture<R> = Pin<Box<dyn Future<Output = Result<Finished<R>>> + Send>>;
/// Performs one dispatched Answer to its own deadline (`StartedJob::run_ms`)
/// with its applied shape and disclosed degradations.
pub(crate) type Runner<R> = Arc<dyn Fn(StartedJob) -> RunFuture<R> + Send + Sync>;
/// One caller's own work for the job it makes: the service's Answer path
/// supplies the kernel call with a job-owned read context, so the work does
/// not depend on the calling connection. A call that attaches, or is told
/// `Busy`, drops its work unused.
pub(crate) type JobWork<R> = Box<dyn FnOnce(StartedJob) -> RunFuture<R> + Send>;

/// A runner's or a work's error for a job that did nothing a retry could
/// repeat: the table delivers it to the job's waiters as
/// [`JobOutcome::Dropped`] and keeps nothing, so a retry is admitted afresh
/// instead of attaching to a failure.
#[derive(Debug)]
pub(crate) struct Unretained {
    pub(crate) message: String,
    /// The job never started.
    pub(crate) not_run: bool,
}
impl std::fmt::Display for Unretained {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for Unretained {}

/// One call's result. Degradations are the ones fitted at dispatch, the same
/// for the caller that made the job and for every caller that attached.
#[derive(Debug)]
pub(crate) enum CallReply<R> {
    /// The job finished within the call; `attached` when another call made it.
    Finished {
        #[cfg_attr(
            not(test),
            expect(dead_code, reason = "a finished reply names no job; tests read it")
        )]
        id: Sha256Digest,
        attached: bool,
        #[cfg_attr(
            not(test),
            expect(dead_code, reason = "a finished reply carries its own; tests read it")
        )]
        degradations: Vec<Degradation>,
        outcome: Arc<JobOutcome<R>>,
    },
    /// The call's deadline came first; the job keeps running. A queued job's
    /// reply is a ticket, a running job's a partial with this continuation.
    Pending {
        #[cfg_attr(
            not(test),
            expect(dead_code, reason = "both replies are ticket-shaped; tests read it")
        )]
        running: bool,
        ticket: AnswerTicket,
        degradations: Vec<Degradation>,
    },
    /// Nothing could be queued; nothing was made.
    Busy { retry_after_ms: u64 },
}

struct Shared<R> {
    schedule: Mutex<Schedule<R>>,
    done: Mutex<HashMap<Sha256Digest, Signal<R>>>,
    clock: Arc<dyn Clock>,
    runner: Runner<R>,
    /// Work supplied by the call that made a job, taken when it starts.
    /// Locked only after the schedule lock, never the other way round.
    work: Mutex<HashMap<Sha256Digest, JobWork<R>>>,
    /// Jobs are spawned here, whatever thread dispatches them.
    runtime: tokio::runtime::Handle,
    /// Test hook: the number of calls that have started waiting.
    #[cfg(test)]
    waiting: watch::Sender<usize>,
}

pub(crate) struct AnswerJobTable<R> {
    shared: Arc<Shared<R>>,
}
impl<R> Clone for AnswerJobTable<R> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<R: Send + Sync + 'static> AnswerJobTable<R> {
    /// Built inside the service's Tokio runtime, whose handle it keeps.
    pub(crate) fn new(
        config: JobTableConfig,
        clock: Arc<dyn Clock>,
        runner: Runner<R>,
    ) -> Result<Self> {
        let mut salt = [0; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut salt)
            .map_err(|_| anyhow!("Answer job table salt unavailable"))?;
        Self::with_salt(config, clock, runner, salt)
    }
    pub(crate) fn with_salt(
        config: JobTableConfig,
        clock: Arc<dyn Clock>,
        runner: Runner<R>,
        salt: [u8; 32],
    ) -> Result<Self> {
        let runtime = tokio::runtime::Handle::try_current()
            .context("the Answer job table needs the service's Tokio runtime")?;
        Ok(Self {
            shared: Arc::new(Shared {
                schedule: Mutex::new(Schedule::new(config, salt)),
                done: Mutex::new(HashMap::new()),
                clock,
                runner,
                work: Mutex::new(HashMap::new()),
                runtime,
                #[cfg(test)]
                waiting: watch::channel(0).0,
            }),
        })
    }

    fn schedule(&self) -> Result<std::sync::MutexGuard<'_, Schedule<R>>> {
        self.shared
            .schedule
            .lock()
            .map_err(|_| anyhow!("Answer job table unavailable"))
    }

    /// Submit or attach, then wait for the job until `call_deadline`.
    /// `scope` must come from the caller's own verified authority; a job is
    /// never visible outside it. The live path always brings its work
    /// ([`Self::call_with_work`]); the table's own tests use this.
    #[cfg(test)]
    pub(crate) async fn call(
        &self,
        scope: AnswerScope,
        request: ContextRequestV3,
        model: Option<ContextRung>,
        publication: Option<Sha256Digest>,
        call_deadline: tokio::time::Instant,
    ) -> Result<CallReply<R>> {
        self.call_with_work(scope, request, model, publication, call_deadline, None)
            .await
    }

    /// [`Self::call`] with this caller's own `work` for a job it makes; a job
    /// made without work runs the table's runner.
    pub(crate) async fn call_with_work(
        &self,
        scope: AnswerScope,
        request: ContextRequestV3,
        model: Option<ContextRung>,
        publication: Option<Sha256Digest>,
        call_deadline: tokio::time::Instant,
        work: Option<JobWork<R>>,
    ) -> Result<CallReply<R>> {
        let (submitted, receiver) = {
            let mut schedule = self.schedule()?;
            let now = self.shared.clock.now_ms();
            let submitted = schedule.submit(scope, request, model, publication, now)?;
            let id = match &submitted {
                Submitted::Busy { retry_after_ms } => {
                    return Ok(CallReply::Busy {
                        retry_after_ms: *retry_after_ms,
                    });
                }
                Submitted::Attached(id) | Submitted::Admitted(id) => *id,
                Submitted::Ticketed(ticket) => ticket.id,
            };
            if let (Submitted::Admitted(_) | Submitted::Ticketed(_), Some(work)) =
                (&submitted, work)
            {
                self.shared
                    .work
                    .lock()
                    .map_err(|_| anyhow!("Answer job work unavailable"))?
                    .insert(id, work);
            }
            // Under the schedule lock: a job not finished now signals the
            // receiver made here, because finishing takes this lock first.
            if let Some(JobView::Finished {
                outcome,
                degradations,
            }) = schedule.view(&scope, &id, now)
            {
                return Ok(CallReply::Finished {
                    id,
                    attached: true,
                    degradations,
                    outcome,
                });
            }
            let receiver = self.receiver(&id)?;
            let started = schedule.dispatch(now);
            drop(schedule);
            self.spawn(started);
            (submitted, receiver)
        };
        let (id, attached) = match submitted {
            Submitted::Ticketed(ticket) => {
                return Ok(CallReply::Pending {
                    running: false,
                    ticket,
                    degradations: vec![],
                });
            }
            Submitted::Attached(id) => (id, true),
            Submitted::Admitted(id) => (id, false),
            Submitted::Busy { .. } => unreachable!("returned above"),
        };
        self.wait(scope, id, attached, receiver, call_deadline)
            .await
    }

    async fn wait(
        &self,
        scope: AnswerScope,
        id: Sha256Digest,
        attached: bool,
        mut receiver: Signalled<R>,
        call_deadline: tokio::time::Instant,
    ) -> Result<CallReply<R>> {
        #[cfg(test)]
        self.shared.waiting.send_modify(|count| *count += 1);
        let waited = match tokio::time::timeout_at(
            call_deadline,
            receiver.wait_for(|outcome| outcome.is_some()),
        )
        .await
        {
            Ok(Ok(outcome)) => outcome.clone(),
            _ => None,
        };
        // An outcome sent just after the timeout is still this call's, even
        // if the table has dropped the entry since (review C6).
        let signalled = waited.or_else(|| receiver.borrow().clone());
        match (signalled, self.view(&scope, &id)?) {
            (
                _,
                Some(JobView::Finished {
                    outcome,
                    degradations,
                }),
            ) => Ok(CallReply::Finished {
                id,
                attached,
                degradations,
                outcome,
            }),
            // Finished and no longer retained (a stale publication, the
            // caps): the outcome is delivered; its disclosures went with it.
            (Some(outcome), _) => Ok(CallReply::Finished {
                id,
                attached,
                degradations: vec![],
                outcome,
            }),
            (
                None,
                Some(JobView::Running {
                    ticket,
                    degradations,
                }),
            ) => Ok(CallReply::Pending {
                running: true,
                ticket,
                degradations,
            }),
            (None, Some(JobView::Queued(ticket))) => Ok(CallReply::Pending {
                running: false,
                ticket,
                degradations: vec![],
            }),
            (None, None) => Err(anyhow!("Answer job ended without a retained result")),
        }
    }

    fn view(&self, scope: &AnswerScope, id: &Sha256Digest) -> Result<Option<JobView<R>>> {
        let now = self.shared.clock.now_ms();
        Ok(self.schedule()?.view(scope, id, now))
    }

    fn receiver(&self, id: &Sha256Digest) -> Result<Signalled<R>> {
        let mut done = self
            .shared
            .done
            .lock()
            .map_err(|_| anyhow!("Answer job signals unavailable"))?;
        Ok(done
            .entry(*id)
            .or_insert_with(|| watch::channel(None).0)
            .subscribe())
    }

    /// Run each started job on its own task, so a caller dropping its call
    /// never cancels the work. The runner is called inside the task, and the
    /// task is spawned on the kept handle, so neither a panicking runner nor
    /// a dispatch from a non-Tokio thread can strand a slot (review C2).
    fn spawn(&self, started: Vec<StartedJob>) {
        for job in started {
            let id = job.id;
            let runner = self.shared.runner.clone();
            let work = self
                .shared
                .work
                .lock()
                .ok()
                .and_then(|mut work| work.remove(&id));
            let run = self.shared.runtime.spawn(async move {
                match work {
                    Some(work) => work(job).await,
                    None => runner(job).await,
                }
            });
            let table = self.clone();
            self.shared.runtime.spawn(async move {
                let outcome = match run.await {
                    Ok(Ok(finished)) => JobOutcome::Done {
                        value: finished.value,
                        publication: finished.publication,
                        retained_bytes: finished.retained_bytes,
                    },
                    Ok(Err(error)) => match error.downcast::<Unretained>() {
                        Ok(Unretained { message, not_run }) => {
                            JobOutcome::Dropped { message, not_run }
                        }
                        Err(error) => JobOutcome::Failed(format!("{error:#}")),
                    },
                    Err(error) => JobOutcome::Failed(format!("Answer job task failed: {error}")),
                };
                table.complete(id, outcome);
            });
        }
    }

    fn complete(&self, id: Sha256Digest, outcome: JobOutcome<R>) {
        let Ok(mut schedule) = self.schedule() else {
            return;
        };
        let now = self.shared.clock.now_ms();
        let outcome = schedule.finish(&id, outcome, now);
        let started = schedule.dispatch(now);
        drop(schedule);
        if let Ok(mut done) = self.shared.done.lock()
            && let Some(sender) = done.remove(&id)
        {
            sender.send_replace(Some(outcome));
        }
        self.spawn(started);
    }

    /// Aggregate queue status for the status surface.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "queue status on the status surface is deferred until after LG"
        )
    )]
    pub(crate) fn status(&self) -> Result<QueueStatus> {
        let now = self.shared.clock.now_ms();
        let mut schedule = self.schedule()?;
        schedule.purge(now);
        Ok(schedule.status(now))
    }

    /// L5b's runtime capacity figure; a larger limit starts queued jobs now.
    /// Safe from any thread.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "L5b's runtime capacity is not fed to the table yet"
        )
    )]
    pub(crate) fn set_runtime_capacity(&self, capacity: Option<usize>) -> Result<()> {
        let mut schedule = self.schedule()?;
        schedule.set_runtime_capacity(capacity);
        let started = schedule.dispatch(self.shared.clock.now_ms());
        drop(schedule);
        self.spawn(started);
        Ok(())
    }

    /// Measured stage costs replacing the placeholders.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "measured stage costs (L5b, L7) are not fed to the table yet"
        )
    )]
    pub(crate) fn set_costs(&self, costs: CostTable) -> Result<()> {
        self.schedule()?.set_costs(costs);
        Ok(())
    }

    /// For the service's own publication observer only, never a caller: a
    /// new publication of `index` drops other publications' results,
    /// failures included, and supersedes jobs running on them. An older
    /// generation of the same incarnation is ignored (`Ok(false)`).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the service's publication observer is not wired to the table yet"
        )
    )]
    pub(crate) fn publication_changed(
        &self,
        index: IndexId,
        publication: &RevisionEnvelope,
    ) -> Result<bool> {
        self.schedule()?.publication_changed(index, publication)
    }

    /// Test hook: resolves once `count` calls have started waiting.
    #[cfg(test)]
    pub(crate) async fn waiting_at_least(&self, count: usize) {
        let mut waiting = self.shared.waiting.subscribe();
        waiting
            .wait_for(|waiting| *waiting >= count)
            .await
            .expect("the table outlives its waiters");
    }
}
