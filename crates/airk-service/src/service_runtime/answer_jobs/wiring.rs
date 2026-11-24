// SPDX-License-Identifier: Apache-2.0
//! The job table on the live Answer path (CONTRACT.md, "Wiring"): the
//! service's table behind the `airk-mcp` seam its Answer handler calls, and
//! the lookup the project owners that run in this service find it by.
use super::{
    AnswerJobTable, AnswerScope, ContextRung, JobTableConfig, JobWork, RunFuture, Runner,
    StartedJob, TokioClock, Unretained,
    reply::{self, Delivered},
    schedule::{JobOptions, publication_digest},
};
use airk_core::arbiter::{answer_loop::ContextResponseV3, device::SelectedModel};
use airk_mcp::answer_jobs::{AnswerAdmission, AnswerCall, AnswerDelivery, AnswerJobs, AnswerWork};
use anyhow::{Result, anyhow};
use tokio_util::sync::CancellationToken;
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
fn job_work(work: AnswerWork, maker_session: CancellationToken) -> JobWork<ContextResponseV3> {
    Box::new(move |job: StartedJob| -> RunFuture<ContextResponseV3> {
        Box::pin(async move {
            #[cfg(feature = "test-support")]
            test_gate::hold(job.id, maker_session).await;
            #[cfg(not(feature = "test-support"))]
            drop(maker_session);
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
                    settling: std::sync::Mutex::new(failure.settling),
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
            // and status headroom its reply is packed to; the publication is
            // that binding's.
            let scope = AnswerScope::from_binding(&call.scope)
                .with_reply_variant(call.reply_reserve, call.include_status);
            let publication = publication_digest(&call.scope.publication)?;
            let model = match call.context_model {
                Some(SelectedModel::Arbiter) => Some(ContextRung::Compact),
                Some(SelectedModel::Focus) | None => None,
            };
            let options = JobOptions {
                fresh: call.options.fresh,
                key: call.options.invocation_key.clone(),
                interactive: call.options.interactive,
            };
            let deadline = tokio::time::Instant::from_std(call.deadline);
            let mut request = call.request.clone();
            let mut resubmitted = false;
            loop {
                let reply = self
                    .table
                    .call_job(
                        scope,
                        request.clone(),
                        model,
                        Some(publication),
                        deadline,
                        &options,
                        Some(job_work(call.work.clone(), call.maker_session.clone())),
                    )
                    .await?;
                let waited_ms = u64::try_from(arrived.elapsed().as_millis()).unwrap_or(u64::MAX);
                return Ok(match reply::deliver(reply, &call.request, &call.scope, waited_ms)? {
                    Delivered::Reply { response, record } => {
                        AnswerDelivery::Reply { response, record }
                    }
                    Delivered::Failed(message) => AnswerDelivery::Failed(message),
                    Delivered::Busy { retry_after_ms } => AnswerDelivery::Busy { retry_after_ms },
                    Delivered::KeyConflict => AnswerDelivery::KeyConflict,
                    // The job this call waited for never ran (its owner
                    // stopped, or it expired unattended). With time left,
                    // this call makes its own job once; without, it is told
                    // to come back.
                    Delivered::Dropped { not_run: true, .. } => {
                        let left = deadline
                            .saturating_duration_since(tokio::time::Instant::now())
                            .as_millis();
                        if !resubmitted
                            && call.request.continuation.is_none()
                            && left >= u128::from(RESUBMIT_MIN_MS)
                        {
                            resubmitted = true;
                            request.continuation = None;
                            continue;
                        }
                        AnswerDelivery::Busy { retry_after_ms: 1 }
                    }
                    Delivered::Dropped { message, .. } => AnswerDelivery::Failed(message),
                });
            }
        })
    }
}

/// Test support only (`test-support`; never compiled into a product or
/// release build, which do not enable that feature): with
/// `AIRK_TEST_ANSWER_JOB_GATE` naming a loopback `ip:port` the test listens
/// on, each job connects out to it before its work starts, sends
/// `started <job>`, and then obeys one-byte commands: `m` waits (at most 120
/// s) for its maker's session to end and answers `maker-closed`, `r`
/// releases the job. It gates; it never changes what the job does. Without
/// the variable it does nothing. Every wait has a deadline, so a stuck test
/// cannot hold a job for ever.
#[cfg(feature = "test-support")]
pub(crate) mod test_gate {
    use airk_core::identity::Sha256Digest;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    pub(crate) const VARIABLE: &str = "AIRK_TEST_ANSWER_JOB_GATE";
    const DEADLINE: Duration = Duration::from_secs(120);

    pub(crate) async fn hold(job: Sha256Digest, maker_session: CancellationToken) {
        let Some(address) = std::env::var_os(VARIABLE) else {
            return;
        };
        let Some(address) = address
            .to_str()
            .and_then(|text| text.parse::<std::net::SocketAddr>().ok())
            .filter(|address| address.ip().is_loopback())
        else {
            return;
        };
        let _ = tokio::time::timeout(DEADLINE, async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut stream = tokio::net::TcpStream::connect(address).await?;
            stream
                .write_all(format!("started {job}\n").as_bytes())
                .await?;
            loop {
                let mut command = [0u8; 1];
                stream.read_exact(&mut command).await?;
                match command[0] {
                    b'm' => {
                        maker_session.cancelled().await;
                        stream.write_all(b"maker-closed\n").await?;
                    }
                    _ => return Ok::<(), std::io::Error>(()),
                }
            }
        })
        .await;
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
