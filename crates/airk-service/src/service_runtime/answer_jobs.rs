// SPDX-License-Identifier: Apache-2.0
//! The service-owned Answer job table (plan section 12.4.1, step L2b): an
//! admitted Answer is a job of the service, not of the client's connection.
//! It keeps running when its caller times out or disconnects, a retry of the
//! same request in the same scope attaches instead of replaying, and its
//! result is kept for a bounded time. Admission degrades to fit the deadline
//! first and returns a ticket only when even the smallest disclosed answer
//! cannot fit. Contract: `answer_jobs/CONTRACT.md`.
//!
//! [`AnswerJobTable::call`] is the one narrow entry point for the service's
//! Answer path; the runner closure it is built with performs one Answer.
mod estimate;
mod schedule;
#[cfg(test)]
mod tests;

pub(crate) use estimate::{ContextRung, CostTable};
pub(crate) use schedule::{AnswerScope, JobOutcome, JobTableConfig, QueueStatus, StartedJob};

use airk_core::{
    arbiter::answer_loop::{AnswerTicket, ContextRequestV3, Degradation},
    identity::Sha256Digest,
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

/// One call's result.
#[derive(Debug)]
pub(crate) enum CallReply<R> {
    /// The job finished within the call; `attached` when another call made it.
    Finished {
        id: Sha256Digest,
        attached: bool,
        degradations: Vec<Degradation>,
        outcome: Arc<JobOutcome<R>>,
    },
    /// The call's deadline came first; the job keeps running. A queued job's
    /// reply is a ticket, a running job's a partial with this continuation.
    Pending { running: bool, ticket: AnswerTicket },
    /// Nothing could be queued; nothing was made.
    Busy { retry_after_ms: u64 },
}

struct Shared<R> {
    schedule: Mutex<Schedule<R>>,
    done: Mutex<HashMap<Sha256Digest, Signal<R>>>,
    clock: Arc<dyn Clock>,
    runner: Runner<R>,
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
    pub(crate) fn new(
        config: JobTableConfig,
        clock: Arc<dyn Clock>,
        runner: Runner<R>,
    ) -> Result<Self> {
        let mut salt = [0; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut salt)
            .map_err(|_| anyhow!("Answer job table salt unavailable"))?;
        Ok(Self::with_salt(config, clock, runner, salt))
    }
    pub(crate) fn with_salt(
        config: JobTableConfig,
        clock: Arc<dyn Clock>,
        runner: Runner<R>,
        salt: [u8; 32],
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                schedule: Mutex::new(Schedule::new(config, salt)),
                done: Mutex::new(HashMap::new()),
                clock,
                runner,
            }),
        }
    }

    fn schedule(&self) -> Result<std::sync::MutexGuard<'_, Schedule<R>>> {
        self.shared
            .schedule
            .lock()
            .map_err(|_| anyhow!("Answer job table unavailable"))
    }

    /// Submit or attach, then wait for the job until `call_deadline`.
    /// `scope` must come from the caller's own verified authority; a job is
    /// never visible outside it.
    pub(crate) async fn call(
        &self,
        scope: AnswerScope,
        request: ContextRequestV3,
        model: Option<ContextRung>,
        publication: Option<Sha256Digest>,
        call_deadline: tokio::time::Instant,
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
                Submitted::Attached(id) | Submitted::Admitted { id, .. } => *id,
                Submitted::Ticketed(ticket) => ticket.id,
            };
            // Under the schedule lock: a job not finished now signals the
            // receiver made here, because finishing takes this lock first.
            if let Some(JobView::Finished(outcome)) = schedule.view(&scope, &id, now) {
                return Ok(CallReply::Finished {
                    id,
                    attached: true,
                    degradations: vec![],
                    outcome,
                });
            }
            let receiver = self.receiver(&id)?;
            let started = schedule.dispatch(now);
            drop(schedule);
            self.spawn(started);
            (submitted, receiver)
        };
        let (id, attached, degradations) = match submitted {
            Submitted::Ticketed(ticket) => {
                return Ok(CallReply::Pending {
                    running: false,
                    ticket,
                });
            }
            Submitted::Attached(id) => (id, true, vec![]),
            Submitted::Admitted { id, degradations } => (id, false, degradations),
            Submitted::Busy { .. } => unreachable!("returned above"),
        };
        self.wait(scope, id, attached, degradations, receiver, call_deadline)
            .await
    }

    async fn wait(
        &self,
        scope: AnswerScope,
        id: Sha256Digest,
        attached: bool,
        degradations: Vec<Degradation>,
        mut receiver: Signalled<R>,
        call_deadline: tokio::time::Instant,
    ) -> Result<CallReply<R>> {
        let finished = |outcome| CallReply::Finished {
            id,
            attached,
            degradations: degradations.clone(),
            outcome,
        };
        let waited = tokio::time::timeout_at(
            call_deadline,
            receiver.wait_for(|outcome| outcome.is_some()),
        )
        .await;
        if let Ok(Ok(outcome)) = waited {
            let outcome = outcome.clone().expect("waited for an outcome");
            return Ok(finished(outcome));
        }
        match self.view(&scope, &id)? {
            Some(JobView::Finished(outcome)) => Ok(finished(outcome)),
            Some(JobView::Running(ticket)) => Ok(CallReply::Pending {
                running: true,
                ticket,
            }),
            Some(JobView::Queued(ticket)) => Ok(CallReply::Pending {
                running: false,
                ticket,
            }),
            None => Err(anyhow!("Answer job ended without a retained result")),
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
    /// never cancels the work; a panicking runner still frees its slot.
    fn spawn(&self, started: Vec<StartedJob>) {
        for job in started {
            let id = job.id;
            let run = tokio::spawn((self.shared.runner)(job));
            let table = self.clone();
            tokio::spawn(async move {
                let outcome = match run.await {
                    Ok(Ok(finished)) => JobOutcome::Done {
                        value: finished.value,
                        publication: finished.publication,
                        retained_bytes: finished.retained_bytes,
                    },
                    Ok(Err(error)) => JobOutcome::Failed(format!("{error:#}")),
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
    pub(crate) fn status(&self) -> Result<QueueStatus> {
        let now = self.shared.clock.now_ms();
        let mut schedule = self.schedule()?;
        schedule.purge(now);
        Ok(schedule.status(now))
    }

    /// L5b's runtime capacity figure; a larger limit starts queued jobs now.
    pub(crate) fn set_runtime_capacity(&self, capacity: Option<usize>) -> Result<()> {
        let mut schedule = self.schedule()?;
        schedule.set_runtime_capacity(capacity);
        let started = schedule.dispatch(self.shared.clock.now_ms());
        drop(schedule);
        self.spawn(started);
        Ok(())
    }

    /// Measured stage costs replacing the placeholders.
    pub(crate) fn set_costs(&self, costs: CostTable) -> Result<()> {
        self.schedule()?.set_costs(costs);
        Ok(())
    }

    /// A new publication of the scope's index drops older results at once.
    pub(crate) fn publication_changed(
        &self,
        scope: &AnswerScope,
        current: Sha256Digest,
    ) -> Result<()> {
        self.schedule()
            .context("Answer job table unavailable")?
            .publication_changed(scope.index(), current);
        Ok(())
    }
}
