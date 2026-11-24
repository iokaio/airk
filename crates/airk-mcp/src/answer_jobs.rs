// SPDX-License-Identifier: Apache-2.0
//! The seam between the Answer handler and the service's Answer job table
//! (final plan sections 12.4.1 and 12.4.2, steps L2b and L2c). This crate
//! cannot depend on the service, so the service implements [`AnswerJobs`]
//! for its table, and the owner that runs a session implements
//! [`AnswerJobAuthority`] for that session's selection or open read. A server without both
//! (stdio, a child owner, a local session) calls the kernel directly.
//!
//! With a table, an admitted Answer is a job of the service. At submission
//! the handler derives the job's own kernel (`Kernel::answer_job_kernel`: the
//! caller's frozen authority with a revocation flag of its own) and has the
//! owner bind it to the selection or open read its session redeemed. At dispatch the job mints its own native
//! attempt and read context and runs on its owner's blocking pool under a
//! supervised deadline. A caller that leaves ends only its own wait; the
//! maker's session teardown does not reach the job (review H1).
use crate::{AirkServer, read_attempt::ReadAttemptContext};
use airk_core::arbiter::{
    ScopeBinding,
    answer_loop::{
        AnswerDeliveryRecord, AnswerJobOptions, ContextRequestV3, ContextResponseV3, Degradation,
    },
    device::SelectedModel,
};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The most a call's wait ends before its own end, to deliver a ticket.
const TICKET_MARGIN_MS: u64 = 2_000;
/// How long past its deadline a job's blocking work may take before its
/// owner is quarantined (CONTRACT "Supervised deadline").
pub const JOB_GRACE: Duration = Duration::from_secs(5);

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

/// Work still settling after its job was finished for its callers: the
/// table keeps the job's capacity held until it resolves.
pub type Settling = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A job that produced no reply.
pub struct AnswerFailure {
    /// The handler's own encoding of the failure, opaque to the table and
    /// delivered unchanged to the calls the failure belongs to.
    pub payload: String,
    /// False when a retry could repeat nothing: the job never started, its
    /// owner was stopping, or its refusal is typed safe to repeat. The table
    /// then keeps nothing.
    pub retain: bool,
    /// The job never started.
    pub not_run: bool,
    /// The job's blocking work passed its supervised deadline and is still
    /// running: its capacity is not free until this resolves.
    pub settling: Option<Settling>,
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
    /// The request as asked: its `deadline_ms` is the job's work budget.
    pub request: ContextRequestV3,
    /// The caller's validated job options.
    pub options: AnswerJobOptions,
    /// The Context model the caller's card calls would run on; `None` is the
    /// no-model loop.
    pub context_model: Option<SelectedModel>,
    /// The bytes this call's reply keeps back for its own metadata. A reply
    /// is packed to it, so calls with different reserves never share a job.
    pub reply_reserve: usize,
    /// The reply is packed with the call's status headroom (review M3), so
    /// calls that differ in it never share a job.
    pub include_status: bool,
    /// When this call's wait must end; never the job's deadline.
    pub deadline: Instant,
    /// Cancelled when the maker's session ends: a diagnostic seam for tests
    /// and records, never authority.
    pub maker_session: CancellationToken,
    pub work: AnswerWork,
}

/// What the table delivers for one call.
pub enum AnswerDelivery {
    /// The answer, or a ticket-shaped reply for a job to come back for,
    /// rendered for this call, with this delivery's record.
    Reply {
        response: Box<ContextResponseV3>,
        record: AnswerDeliveryRecord,
    },
    /// The job failed; the payload is the one its work returned.
    Failed(String),
    /// Nothing was queued; retry after this long.
    Busy { retry_after_ms: u64 },
    /// The invocation key names another request in this authority.
    KeyConflict,
}

/// The service's Answer job table, as the handler sees it.
pub trait AnswerJobs: Send + Sync {
    fn call(
        &self,
        call: AnswerCall,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<AnswerDelivery>> + Send + '_>>;
}

/// What the owner returns when it binds a job's kernel to its read authority.
pub struct AnswerJobGrant {
    /// Keeps the job's kernel registered with that authority; dropping it removes
    /// and revokes that kernel.
    pub guard: Box<dyn std::any::Any + Send + Sync>,
    /// Cancelled when the maker's session ends.
    pub maker_session: CancellationToken,
}

/// The owner's authority for the jobs of one session (supervisor CONTRACT,
/// "Answer job authority").
pub trait AnswerJobAuthority: Send + Sync {
    /// Register `kernel` (a job kernel) with the live selection or open read
    /// the session redeemed, so that its revocation, lease and owner close end it
    /// and the session's own teardown does not.
    fn bind(&self, kernel: &airk_kernel::Kernel) -> anyhow::Result<AnswerJobGrant>;
    /// Mint the job's own native attempt, ending at `deadline`.
    fn attempt(
        &self,
        deadline: Instant,
    ) -> anyhow::Result<airk_core::read_attempt::ReadAttemptBudget>;
    /// A job passed its deadline and grace: quarantine the owner.
    fn stuck(&self);
}

/// A server's table and its session's job authority, set once by its owner
/// and shared by its clones. One is never set without the other.
pub(crate) type Slot = Arc<OnceLock<(Arc<dyn AnswerJobs>, Arc<dyn AnswerJobAuthority>)>>;

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

/// A typed refusal that belongs to one Answer job invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerJobRefusal {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    /// The job's work had started reading when it was refused: its reads
    /// ran, and a retry repeats them. False only when nothing was read.
    pub query_submitted: bool,
}
impl std::fmt::Display for AnswerJobRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for AnswerJobRefusal {}
impl AnswerJobRefusal {
    fn new(code: &str, message: &str, retryable: bool, query_submitted: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            query_submitted,
        }
    }
    /// The same key in this authority already names another request; no job
    /// was made and nothing was read.
    pub fn key_conflict() -> Self {
        Self::new(
            "answer_invocation_conflict",
            "this invocation key already names a different Answer request; use a new key",
            false,
            false,
        )
    }
    /// The job's selection, lease or owner ended before or while it ran.
    /// `query_submitted`: whether the job had started its reads.
    pub fn authority_ended(query_submitted: bool) -> Self {
        Self::new(
            "answer_authority_ended",
            "the Answer job's read authority ended before it finished; send the request again on a live connection",
            true,
            query_submitted,
        )
    }
    /// The job passed its deadline and grace; its owner is quarantined. Its
    /// reads ran, and may still be running.
    pub fn stuck() -> Self {
        Self::new(
            "answer_job_stuck",
            "the Answer job did not finish within its deadline; its owner stopped admitting work",
            true,
            true,
        )
    }
    pub(crate) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({"error":{
            "code":self.code,"message":self.message,"retryable":self.retryable,
            "query_submitted":self.query_submitted
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
    /// A refusal of the job path itself.
    #[serde(default)]
    refusal: Option<AnswerJobRefusal>,
}

fn encode(error: &anyhow::Error, refusal: Option<AnswerJobRefusal>) -> String {
    let payload = Payload {
        airk_answer_failure: 1,
        message: error.to_string(),
        receipt: crate::response::PreservedReadFailure::from_error(error)
            .and_then(|preserved| preserved.receipt()),
        refusal,
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| error.to_string())
}

fn failure(error: &anyhow::Error) -> AnswerFailure {
    let unsettled = error.is::<OwnerStopping>()
        || error.is::<crate::read_attempt::ReadAttemptClosed>()
        || error
            .chain()
            .any(|cause| cause.is::<airk_kernel::service_execution::Draining>());
    let repeatable = airk_kernel::retrieval_retry::read_retry_hint(error).is_some();
    let refused = error
        .chain()
        .any(|cause| cause.is::<airk_core::scoped_reads::ScopeReadRefused>());
    AnswerFailure {
        payload: encode(error, None),
        retain: !(unsettled || repeatable || refused),
        not_run: false,
        settling: None,
    }
}

fn refused(refusal: AnswerJobRefusal, not_run: bool, settling: Option<Settling>) -> AnswerFailure {
    AnswerFailure {
        payload: encode(&anyhow::Error::new(refusal.clone()), Some(refusal)),
        retain: false,
        not_run,
        settling,
    }
}

fn not_run(message: &str) -> AnswerFailure {
    AnswerFailure {
        payload: message.into(),
        retain: false,
        not_run: true,
        settling: None,
    }
}

/// The error a delivered failure renders as: its typed refusal or receipt
/// when the job kept one, its message otherwise.
fn delivered_failure(payload: String) -> anyhow::Error {
    match serde_json::from_str::<Payload>(&payload) {
        Ok(Payload {
            refusal: Some(refusal),
            ..
        }) => anyhow::Error::new(refusal),
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

/// The table and job authority for this call, if its server has them. A
/// combined member runs under its parent's allowance and is never a job.
pub(crate) fn for_call(
    server: &AirkServer,
) -> Option<(Arc<dyn AnswerJobs>, Arc<dyn AnswerJobAuthority>)> {
    if server
        .read_attempt
        .as_ref()
        .is_some_and(ReadAttemptContext::is_combined_member)
    {
        return None;
    }
    server.answer_jobs.get().cloned()
}

/// The reply a handler renders: the typed reply and, for a job's delivery,
/// its record beside it.
#[derive(Debug, Serialize)]
pub(crate) struct Answered {
    #[serde(flatten)]
    pub(crate) response: ContextResponseV3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivery: Option<AnswerDeliveryRecord>,
}

/// One admitted Answer call, on its blocking worker.
pub(crate) struct Caller {
    pub(crate) jobs: Arc<dyn AnswerJobs>,
    pub(crate) authority: Arc<dyn AnswerJobAuthority>,
    /// The handler's reader: this call's server with plan and Answer reads on.
    pub(crate) reader: AirkServer,
    pub(crate) kernel: airk_kernel::Kernel,
    pub(crate) request: ContextRequestV3,
    pub(crate) options: AnswerJobOptions,
    pub(crate) reply_reserve: usize,
    pub(crate) include_status: bool,
    /// The call's own end: the request's deadline, bounded by its read
    /// attempt.
    pub(crate) deadline: Instant,
    pub(crate) handle: tokio::runtime::Handle,
    pub(crate) sink: crate::progress::AnswerProgressSink,
}

/// Hand one call to the table and wait for its delivery. Returns what the
/// direct path returns: the reply to render, or the error to render.
pub(crate) fn run(caller: Caller) -> anyhow::Result<Answered> {
    caller.request.validate()?;
    caller.options.validate(&caller.request.budget)?;
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
    // The wait: the call's own end, shortened by `wait_ms`. It never
    // shortens the job, whose work budget is the request's deadline.
    let end = caller.options.wait_ms.map_or(caller.deadline, |wait| {
        caller
            .deadline
            .min(now.checked_add(Duration::from_millis(wait)).unwrap_or(caller.deadline))
    });
    let remaining = u64::try_from(end.saturating_duration_since(now).as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    // The wait ends a little before the end, so that a ticket-shaped reply
    // for a job still queued or running is delivered inside it.
    let margin = Duration::from_millis((remaining / 10).min(TICKET_MARGIN_MS));
    // The job's own authority, bound to the caller's selection now: the maker's session
    // may end while the job is queued.
    let job_kernel = caller.kernel.answer_job_kernel()?;
    let grant = caller.authority.bind(&job_kernel)?;
    let call = AnswerCall {
        scope,
        request: caller.request.clone(),
        options: caller.options.clone(),
        context_model,
        reply_reserve: caller.reply_reserve,
        include_status: caller.include_status,
        deadline: end.checked_sub(margin).unwrap_or(end),
        maker_session: grant.maker_session,
        work: work(&caller, job_kernel, Arc::from(grant.guard)),
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
        AnswerDelivery::Reply { response, record } => Ok(Answered {
            response: *response,
            delivery: Some(record),
        }),
        AnswerDelivery::Failed(payload) => Err(delivered_failure(payload)),
        AnswerDelivery::Busy { retry_after_ms } => Err(AnswerQueueBusy { retry_after_ms }.into()),
        AnswerDelivery::KeyConflict => Err(AnswerJobRefusal::key_conflict().into()),
    }
}

/// This caller's work for a job it makes: the job's own kernel and grant,
/// and none of the maker's transport guards or custody.
fn work(
    caller: &Caller,
    job_kernel: airk_kernel::Kernel,
    grant: Arc<dyn std::any::Any + Send + Sync>,
) -> AnswerWork {
    let mut reader = caller.reader.clone();
    reader.read_attempt = None;
    let (authority, handle, sink) = (
        caller.authority.clone(),
        caller.handle.clone(),
        caller.sink.clone(),
    );
    // The owner counts the job from the moment it is made, not only while
    // it runs: a queued job may have no caller and no connection left, and
    // its owner must not leave as idle under it (plan 12.4.2, "What a
    // ticket means"). The count ends when the job starts (the running
    // guard below takes over) or when the table drops this work unrun.
    let queued = Arc::new(std::sync::Mutex::new(Some(
        caller.reader.admission.job_started(),
    )));
    Arc::new(move |admission: AnswerAdmission| -> AnswerWorkFuture {
        let (mut reader, kernel, authority, handle, sink, grant) = (
            reader.clone(),
            job_kernel.clone(),
            authority.clone(),
            handle.clone(),
            sink.clone(),
            grant.clone(),
        );
        let queued = queued.lock().ok().and_then(|mut queued| queued.take());
        Box::pin(async move {
            let _grant = grant;
            if reader.admission.closed().is_cancelled() {
                return Err(not_run("the Answer job's owner is stopping"));
            }
            // Its selection, lease or owner may have ended while it was queued:
            // that refusal is this invocation's alone, and nothing was read.
            if kernel.scoped_read_fingerprint().is_err() {
                return Err(refused(AnswerJobRefusal::authority_ended(false), false, None));
            }
            let job_deadline = Instant::now()
                .checked_add(Duration::from_millis(admission.run_ms.max(1)))
                .ok_or_else(|| failure(&anyhow::anyhow!("Answer job deadline overflow")))?;
            let prepared = (|| -> anyhow::Result<airk_kernel::Kernel> {
                let attempt = authority.attempt(job_deadline)?;
                kernel
                    .with_optional_read_attempt_for("airk_answer", attempt, job_deadline)?
                    .with_fresh_answer_accounting()
            })();
            let kernel = match prepared {
                Ok(kernel) => kernel,
                Err(error) if kernel.scoped_read_fingerprint().is_err() => {
                    // Ended while its attempt was minted: still nothing read.
                    let _ = error;
                    return Err(refused(AnswerJobRefusal::authority_ended(false), false, None));
                }
                Err(error) => return Err(failure(&error)),
            };
            let context = ReadAttemptContext::new(job_deadline, CancellationToken::new())
                .with_read_ceiling(airk_core::read_attempt::MAX_ANSWER_READ_BUDGET);
            context.mark_answer_call();
            reader.read_attempt = Some(context);
            reader.kernel = kernel.clone();
            // Counted as the owner's while it runs, until its deadline and
            // grace: a stuck job never holds the owner's idle exit open.
            let running = queued.unwrap_or_else(|| reader.admission.job_started());
            let reads = handle.clone();
            let mut job = handle.spawn_blocking(move || {
                run_job(reader, kernel, admission, job_deadline, reads, sink)
            });
            let supervised = tokio::time::Instant::from_std(job_deadline + JOB_GRACE);
            let result = match tokio::time::timeout_at(supervised, &mut job).await {
                Ok(Ok(result)) => result,
                Ok(Err(error)) if error.is_cancelled() => {
                    Err(not_run("the Answer job's owner stopped before it ran"))
                }
                Ok(Err(_)) => Err(failure(&anyhow::anyhow!("retrieval worker failed"))),
                Err(_) => {
                    authority.stuck();
                    drop(running);
                    let settling: Settling = Box::pin(async move {
                        let _ = job.await;
                    });
                    return Err(refused(AnswerJobRefusal::stuck(), false, Some(settling)));
                }
            };
            drop(running);
            result
        })
    })
}

/// One job, on its owner's blocking pool, to its own deadline.
fn run_job(
    reader: AirkServer,
    kernel: airk_kernel::Kernel,
    admission: AnswerAdmission,
    deadline: Instant,
    handle: tokio::runtime::Handle,
    sink: crate::progress::AnswerProgressSink,
) -> Result<ContextResponseV3, AnswerFailure> {
    let run = || -> anyhow::Result<ContextResponseV3> {
        #[cfg(any(test, feature = "test-support"))]
        test_hooks::observe_job_start();
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
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("retrieval worker failed")))
        .map_err(|error| {
            // The loop had started: its reads ran.
            if kernel.scoped_read_fingerprint().is_err() {
                refused(AnswerJobRefusal::authority_ended(true), false, None)
            } else {
                failure(&error)
            }
        })
}

/// Test hooks for the job's blocking work (tests and `test-support` only).
#[cfg(any(test, feature = "test-support"))]
pub mod test_hooks {
    use std::sync::{Arc, Mutex};
    pub type Hook = Arc<dyn Fn() + Send + Sync>;
    static START: Mutex<Option<Hook>> = Mutex::new(None);
    /// Run `hook` on the job's blocking thread just before its loop starts
    /// (a stuck-job test holds it there). Process-wide; tests that use it
    /// serialise themselves.
    pub fn on_job_start(hook: Option<Hook>) {
        if let Ok(mut slot) = START.lock() {
            *slot = hook;
        }
    }
    pub(super) fn observe_job_start() {
        let hook = START.lock().ok().and_then(|slot| slot.clone());
        if let Some(hook) = hook {
            hook();
        }
    }
}
