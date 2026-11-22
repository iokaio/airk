// SPDX-License-Identifier: Apache-2.0
//! The job table on the live Answer path (CONTRACT.md, "Wiring"): the
//! service's table behind the `airk-mcp` seam its Answer handler calls, and
//! the lookup the project owners that run in this service find it by.
use super::{
    AnswerJobTable, AnswerScope, ContextRung, JobTableConfig, JobWork, RunFuture, Runner,
    StartedJob, TokioClock, Unretained,
    reply::{self, Delivered},
    schedule::publication_digest,
};
use airk_core::arbiter::{answer_loop::ContextResponseV3, device::SelectedModel};
use airk_mcp::answer_jobs::{AnswerAdmission, AnswerCall, AnswerDelivery, AnswerJobs, AnswerWork};
use anyhow::{Result, anyhow};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

/// The time a caller must still have for a job that did not run to be
/// submitted again for it.
const RESUBMIT_MIN_MS: u64 = 1_000;

/// The service's one Answer job table, as its Answer handlers reach it.
pub(crate) struct ServiceAnswerJobs {
    table: AnswerJobTable<ContextResponseV3>,
}

impl ServiceAnswerJobs {
    /// The service's table. Built inside the service's runtime.
    /// - The handler cannot change a job's Context rung, so every job is
    ///   fitted with its own rung and the no-model loop alone.
    /// - The stage costs are placeholders until measured ones are fed
    ///   (`set_costs`), so they do not reduce a request that did not wait.
    pub(crate) fn start() -> Result<Arc<Self>> {
        Self::with_config(JobTableConfig {
            configured_rung_only: true,
            costs_measured: false,
            ..JobTableConfig::default()
        })
    }

    pub(crate) fn with_config(config: JobTableConfig) -> Result<Arc<Self>> {
        // Every job on this path brings its own work.
        let runner: Runner<ContextResponseV3> = Arc::new(|_job: StartedJob| -> RunFuture<_> {
            Box::pin(async { Err(anyhow!("Answer job has no work")) })
        });
        Ok(Arc::new(Self {
            table: AnswerJobTable::new(config, Arc::new(TokioClock::new()), runner)?,
        }))
    }

    #[cfg(test)]
    pub(crate) fn table(&self) -> &AnswerJobTable<ContextResponseV3> {
        &self.table
    }
}

/// One caller's work as the table runs it.
fn job_work(work: AnswerWork) -> JobWork<ContextResponseV3> {
    Box::new(move |job: StartedJob| -> RunFuture<ContextResponseV3> {
        Box::pin(async move {
            let admission = AnswerAdmission {
                request: job.request,
                parts: job.shape.parts,
                sub_needs: job.shape.sub_needs,
                rounds: job.shape.rounds,
                context_model: job.shape.model.is_some(),
                degradations: job.degradations,
                run_ms: job.run_ms,
            };
            match work(admission).await {
                Ok(response) => reply::finished(response),
                // The payload is the handler's; the table keeps it verbatim.
                Err(failure) if failure.retain => Err(anyhow!(failure.payload)),
                Err(failure) => Err(Unretained {
                    message: failure.payload,
                    not_run: failure.not_run,
                }
                .into()),
            }
        })
    })
}

impl AnswerJobs for ServiceAnswerJobs {
    fn call(
        &self,
        call: AnswerCall,
    ) -> Pin<Box<dyn Future<Output = Result<AnswerDelivery>> + Send + '_>> {
        Box::pin(async move {
            let arrived = Instant::now();
            // The scope is the caller's verified binding, with the reserve
            // its reply is packed to; the publication is that binding's.
            let scope =
                AnswerScope::from_binding(&call.scope).with_reply_reserve(call.reply_reserve);
            let publication = publication_digest(&call.scope.publication)?;
            let model = match call.context_model {
                Some(SelectedModel::Arbiter) => Some(ContextRung::Compact),
                Some(SelectedModel::Focus) | None => None,
            };
            let deadline = tokio::time::Instant::from_std(call.deadline);
            let mut request = call.request.clone();
            let mut resubmitted = false;
            loop {
                let reply = self
                    .table
                    .call_with_work(
                        scope,
                        request.clone(),
                        model,
                        Some(publication),
                        deadline,
                        Some(job_work(call.work.clone())),
                    )
                    .await?;
                let elapsed_ms = u64::try_from(arrived.elapsed().as_millis()).unwrap_or(u64::MAX);
                return Ok(
                    match reply::deliver(reply, &call.request, &call.scope, elapsed_ms)? {
                        Delivered::Reply { response, attached } => {
                            AnswerDelivery::Reply { response, attached }
                        }
                        Delivered::Failed(message) => AnswerDelivery::Failed(message),
                        Delivered::Busy { retry_after_ms } => {
                            AnswerDelivery::Busy { retry_after_ms }
                        }
                        // The job this call waited for never ran (its window
                        // lay beyond the call that made it, or its owner
                        // stopped). With time left, this call makes its own
                        // job once; without, it is told to come back.
                        Delivered::Dropped { not_run: true, .. } => {
                            let left = deadline
                                .saturating_duration_since(tokio::time::Instant::now())
                                .as_millis();
                            if !resubmitted
                                && call.request.continuation.is_none()
                                && left >= u128::from(RESUBMIT_MIN_MS)
                            {
                                resubmitted = true;
                                // The new job gets the window this call
                                // still has, not the one it first asked for.
                                request.budget.deadline_ms = request
                                    .budget
                                    .deadline_ms
                                    .min(u64::try_from(left).unwrap_or(u64::MAX));
                                continue;
                            }
                            AnswerDelivery::Busy { retry_after_ms: 1 }
                        }
                        Delivered::Dropped { message, .. } => AnswerDelivery::Failed(message),
                    },
                );
            }
        })
    }
}

/// The tables of the services in this process, by control root. A project
/// owner reaches its service only through plain functions (the supervisor's
/// `Hooks`), so the table is found the way its other service functions are.
static INSTALLED: Mutex<Vec<(PathBuf, Weak<ServiceAnswerJobs>)>> = Mutex::new(Vec::new());

fn control_key(control: &Path) -> PathBuf {
    control.canonicalize().unwrap_or_else(|_| control.into())
}

/// Make `jobs` the table of the service at `control`, for as long as the
/// service keeps it.
pub(crate) fn install(control: &Path, jobs: &Arc<ServiceAnswerJobs>) {
    let key = control_key(control);
    if let Ok(mut installed) = INSTALLED.lock() {
        installed.retain(|(root, held)| *root != key && held.strong_count() > 0);
        installed.push((key, Arc::downgrade(jobs)));
    }
}

/// The table of the service at `control`: the supervisor hook
/// (`Hooks::with_answer_jobs`). `None` for an owner outside its service.
pub(crate) fn for_control(control: &Path) -> Option<Arc<dyn AnswerJobs>> {
    let key = control_key(control);
    let installed = INSTALLED.lock().ok()?;
    let jobs = installed
        .iter()
        .find(|(root, _)| *root == key)
        .and_then(|(_, held)| held.upgrade())?;
    Some(jobs)
}

#[cfg(test)]
mod tests;
