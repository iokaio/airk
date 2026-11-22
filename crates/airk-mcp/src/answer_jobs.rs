// SPDX-License-Identifier: Apache-2.0
//! The seam between the Answer handler and the service's Answer job table
//! (final plan section 12.4.1, step L2b). This crate cannot depend on the
//! service, so the service implements [`AnswerJobs`] for its table and
//! supplies it to the project owners that run in it. A server without one
//! (stdio, a child owner, a local session) calls the kernel directly.
//!
//! With a table, an admitted Answer is a job of the service: the handler
//! hands over the work, waits for the job until its own deadline, and renders
//! whatever the table delivers. The job's work runs under the kernel and the
//! native attempt of the call that made it, on that owner's blocking pool,
//! with a read context of its own, so a caller that leaves cancels only its
//! own wait. A job never runs outside its maker's window: the native attempt
//! ends there.
use crate::{AirkServer, read_attempt::ReadAttemptContext};
use airk_core::arbiter::{
    ScopeBinding,
    answer_loop::{ContextRequestV3, ContextResponseV3, DEADLINE_TOLERANCE_MS, Degradation},
    device::SelectedModel,
};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

/// The most a call's wait ends before its deadline, to deliver a ticket.
const TICKET_MARGIN_MS: u64 = 2_000;

/// What the table admitted for one job, at its dispatch.
pub struct AnswerAdmission {
    /// The job's own request.
    pub request: ContextRequestV3,
    pub parts: usize,
    pub sub_needs: usize,
    pub rounds: usize,
    /// False runs the no-model loop.
    pub context_model: bool,
    /// The reductions the reply must disclose.
    pub degradations: Vec<Degradation>,
    /// Milliseconds from now to the job's own deadline.
    pub run_ms: u64,
}

/// A job that produced no reply.
pub struct AnswerFailure {
    /// The handler's own encoding of the failure, opaque to the table and
    /// delivered unchanged to every caller of the job.
    pub payload: String,
    /// False when a retry could repeat nothing: the job never started, its
    /// owner was stopping, or its refusal is typed safe to repeat. The table
    /// then keeps nothing, so a retry is admitted afresh.
    pub retain: bool,
    /// The job never started.
    pub not_run: bool,
}

pub type AnswerWorkFuture =
    Pin<Box<dyn Future<Output = Result<ContextResponseV3, AnswerFailure>> + Send>>;
/// One caller's work for a job it makes. The table runs it at most once per
/// job, on the job's own task; a call that attaches never runs its own.
pub type AnswerWork = Arc<dyn Fn(AnswerAdmission) -> AnswerWorkFuture + Send + Sync>;

/// One Answer call, as the handler hands it to the table.
pub struct AnswerCall {
    /// The caller's verified binding (`Kernel::answer_scope`), never client
    /// input: a job is visible only within it.
    pub scope: ScopeBinding,
    pub request: ContextRequestV3,
    /// The Context model the caller's card calls would run on; `None` is the
    /// no-model loop.
    pub context_model: Option<SelectedModel>,
    /// The bytes this call's reply keeps back for its own metadata. A reply
    /// is packed to it, so calls with different reserves never share a job.
    pub reply_reserve: usize,
    /// When this call must return.
    pub deadline: Instant,
    pub work: AnswerWork,
}

/// What the table delivers for one call.
pub enum AnswerDelivery {
    /// The finished answer, or a ticket-shaped reply for a job to come back
    /// for; `attached` when another call made the job.
    Reply {
        response: Box<ContextResponseV3>,
        attached: bool,
    },
    /// The job failed; the payload is the one its work returned.
    Failed(String),
    /// Nothing was queued; retry after this long.
    Busy { retry_after_ms: u64 },
}

/// The service's Answer job table, as the handler sees it.
pub trait AnswerJobs: Send + Sync {
    fn call(
        &self,
        call: AnswerCall,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<AnswerDelivery>> + Send + '_>>;
}

/// A server's table, set once by its owner and shared by its clones.
pub(crate) type Slot = Arc<OnceLock<Arc<dyn AnswerJobs>>>;

/// The typed refusal of a call the table could not queue.
#[derive(Debug)]
pub struct AnswerQueueBusy {
    pub retry_after_ms: u64,
}
impl std::fmt::Display for AnswerQueueBusy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "the Answer queue is full; send the same request again after the stated time",
        )
    }
}
impl std::error::Error for AnswerQueueBusy {}
impl AnswerQueueBusy {
    pub(crate) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"error":{
            "code":"answer_queue_busy","reason":"busy","message":self.to_string(),
            "query_submitted":false,"retryable":true,"retry_after_ms":self.retry_after_ms
        }})
    }
}

/// The owner of the job's server is stopping.
#[derive(Debug)]
struct OwnerStopping;
impl std::fmt::Display for OwnerStopping {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AIRK server is closing")
    }
}
impl std::error::Error for OwnerStopping {}

/// A failure as the table carries it between the job and its callers. It is
/// written and read only here and held only in service memory.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    airk_answer_failure: u8,
    message: String,
    /// The typed receipt today's direct path would have rendered.
    receipt: Option<serde_json::Value>,
}

fn failure(error: &anyhow::Error) -> AnswerFailure {
    let unsettled = error.is::<OwnerStopping>()
        || error.is::<crate::read_attempt::ReadAttemptClosed>()
        || error
            .chain()
            .any(|cause| cause.is::<airk_kernel::service_execution::Draining>());
    let repeatable = airk_kernel::retrieval_retry::read_retry_hint(error).is_some();
    let payload = Payload {
        airk_answer_failure: 1,
        message: error.to_string(),
        receipt: crate::response::PreservedReadFailure::from_error(error)
            .and_then(|preserved| preserved.receipt()),
    };
    AnswerFailure {
        payload: serde_json::to_string(&payload).unwrap_or_else(|_| error.to_string()),
        retain: !(unsettled || repeatable),
        not_run: false,
    }
}

fn not_run(message: &str) -> AnswerFailure {
    AnswerFailure {
        payload: message.into(),
        retain: false,
        not_run: true,
    }
}

/// The error a delivered failure renders as: its typed receipt when the job
/// kept one, its message otherwise.
fn delivered_failure(payload: String) -> anyhow::Error {
    match serde_json::from_str::<Payload>(&payload) {
        Ok(Payload {
            receipt: Some(receipt),
            message,
            ..
        }) => match crate::response::PreservedReadFailure::from_receipt(receipt) {
            Some(preserved) => anyhow::Error::new(preserved),
            None => anyhow::anyhow!(message),
        },
        Ok(Payload { message, .. }) => anyhow::anyhow!(message),
        Err(_) => anyhow::anyhow!(payload),
    }
}

/// The table for this call, if its server has one. A combined member runs
/// under its parent's allowance and is never a job of its own.
pub(crate) fn for_call(server: &AirkServer) -> Option<Arc<dyn AnswerJobs>> {
    if server
        .read_attempt
        .as_ref()
        .is_some_and(ReadAttemptContext::is_combined_member)
    {
        return None;
    }
    server.answer_jobs.get().cloned()
}

/// One admitted Answer call, on its blocking worker.
pub(crate) struct Caller {
    pub(crate) jobs: Arc<dyn AnswerJobs>,
    /// The handler's reader: this call's server with plan and Answer reads on.
    pub(crate) reader: AirkServer,
    pub(crate) kernel: airk_kernel::Kernel,
    pub(crate) request: ContextRequestV3,
    pub(crate) reply_reserve: usize,
    /// The call's original deadline: the request's, bounded by its read
    /// attempt.
    pub(crate) deadline: Instant,
    pub(crate) handle: tokio::runtime::Handle,
    pub(crate) sink: crate::progress::AnswerProgressSink,
}

/// Hand one call to the table and wait for its delivery. Returns what the
/// direct path returns: the reply to render, or the error to render.
pub(crate) fn run(caller: Caller) -> anyhow::Result<ContextResponseV3> {
    caller.request.validate()?;
    if let Some(context) = &caller.reader.read_attempt {
        context.require_live()?;
    }
    let scope = caller.kernel.answer_scope()?;
    let context_model = caller.kernel.answer_context_model()?;
    let now = Instant::now();
    anyhow::ensure!(
        caller.deadline > now,
        "Answer original deadline exhausted before dispatch"
    );
    // The table sees the window this call really has, so a job that never
    // waited ends exactly with its maker's native attempt.
    let mut request = caller.request.clone();
    let remaining = u64::try_from(caller.deadline.saturating_duration_since(now).as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    request.budget.deadline_ms = request.budget.deadline_ms.min(remaining);
    // The wait ends a little before the deadline, so that a ticket-shaped
    // reply for a job still queued or running is delivered inside it. A job
    // that runs to its own deadline has returned its partial before then.
    let margin = Duration::from_millis((remaining / 10).min(TICKET_MARGIN_MS));
    let call = AnswerCall {
        scope,
        request,
        context_model,
        reply_reserve: caller.reply_reserve,
        deadline: caller
            .deadline
            .checked_sub(margin)
            .unwrap_or(caller.deadline),
        work: work(&caller),
    };
    let closed = caller.reader.admission.closed();
    let canceled = caller
        .reader
        .read_attempt
        .as_ref()
        .map(|context| context.cancellation().clone());
    let jobs = caller.jobs.clone();
    // The wait ends with the delivery, with this caller's own cancellation,
    // or when its owner stops; the job itself is the service's.
    let delivery = caller.handle.block_on(async move {
        tokio::select! {
            biased;
            _ = closed.cancelled() => Err(anyhow::Error::new(OwnerStopping)),
            _ = async {
                match &canceled {
                    Some(canceled) => canceled.cancelled().await,
                    None => std::future::pending().await,
                }
            } => Err(crate::read_attempt::ReadAttemptClosed.into()),
            delivery = jobs.call(call) => delivery,
        }
    })?;
    match delivery {
        AnswerDelivery::Reply { response, .. } => Ok(*response),
        AnswerDelivery::Failed(payload) => Err(delivered_failure(payload)),
        AnswerDelivery::Busy { retry_after_ms } => Err(AnswerQueueBusy { retry_after_ms }.into()),
    }
}

/// This caller's work for a job it makes.
fn work(caller: &Caller) -> AnswerWork {
    let (reader, kernel, handle, sink, deadline) = (
        caller.reader.clone(),
        caller.kernel.clone(),
        caller.handle.clone(),
        caller.sink.clone(),
        caller.deadline,
    );
    Arc::new(move |admission: AnswerAdmission| -> AnswerWorkFuture {
        let (mut reader, kernel, handle, sink) =
            (reader.clone(), kernel.clone(), handle.clone(), sink.clone());
        Box::pin(async move {
            let now = Instant::now();
            let window = now
                .checked_add(Duration::from_millis(admission.run_ms))
                .unwrap_or(deadline);
            // A window the table opened beyond its maker's (a ticketed or
            // requeued job) cannot be served: the maker's native attempt
            // ends with its own deadline.
            if now >= deadline || window > deadline + Duration::from_millis(DEADLINE_TOLERANCE_MS) {
                return Err(not_run(
                    "the Answer job's window lies beyond the call that made it",
                ));
            }
            if reader.admission.closed().is_cancelled() {
                return Err(not_run("the Answer job's owner is stopping"));
            }
            let job_deadline = window.min(deadline);
            reader.read_attempt = Some(match &reader.read_attempt {
                Some(context) => context.job_owned(job_deadline),
                None => ReadAttemptContext::new(
                    job_deadline,
                    tokio_util::sync::CancellationToken::new(),
                ),
            });
            // The owner's blocking pool: its threads carry the owner's
            // execution scope, and its reads block on the owner's runtime.
            // The job is counted as the owner's while it runs, because its
            // maker's connection may be gone.
            let running = reader.admission.job_started();
            let reads = handle.clone();
            let job = handle.spawn_blocking(move || {
                let _running = running;
                run_job(reader, kernel, admission, job_deadline, reads, sink)
            });
            match job.await {
                Ok(result) => result,
                Err(error) if error.is_cancelled() => {
                    Err(not_run("the Answer job's owner stopped before it ran"))
                }
                Err(_) => Err(failure(&anyhow::anyhow!("retrieval worker failed"))),
            }
        })
    })
}

/// One job, on its owner's blocking pool, to its own deadline.
fn run_job(
    mut reader: AirkServer,
    kernel: airk_kernel::Kernel,
    admission: AnswerAdmission,
    deadline: Instant,
    handle: tokio::runtime::Handle,
    sink: crate::progress::AnswerProgressSink,
) -> Result<ContextResponseV3, AnswerFailure> {
    let run = || -> anyhow::Result<ContextResponseV3> {
        // Loop accounting binds once per Answer, and a caller's work may be
        // asked for more than one job.
        let kernel = kernel.with_fresh_answer_accounting()?;
        reader.kernel = kernel.clone();
        let plan = airk_kernel::arbiter::AdmissionPlan {
            parts: admission.parts,
            sub_needs: admission.sub_needs,
            rounds: admission.rounds,
            context_model: admission.context_model,
            degradations: admission.degradations,
        };
        let mut request = admission.request;
        request.budget.deadline_ms = request.budget.deadline_ms.min(admission.run_ms.max(1));
        let closed = reader.admission.closed();
        let outcome = kernel.answer_admitted(
            request,
            airk_kernel::arbiter::AnswerEnv {
                deadline,
                require_live: &|| {
                    anyhow::ensure!(!closed.is_cancelled(), OwnerStopping);
                    if let Some(context) = &reader.read_attempt {
                        context.require_live()?;
                    }
                    Ok(())
                },
                reader: &|tool, args, revision| {
                    handle.block_on(reader.workbench_read(tool, args, revision))
                },
                clock: &Instant::now,
                progress: &|item| sink(crate::answer_progress(item)),
            },
            &plan,
        )?;
        Ok(outcome.response)
    };
    run().map_err(|error| failure(&error))
}
