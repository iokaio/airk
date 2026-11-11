// SPDX-License-Identifier: Apache-2.0
//! Bounded admission and complete blocking request lifetimes, including accounting.
use crate::AirkServer;
use airk_core::usage::UsageOperation;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EVIDENCE_WORKERS: u32 = 4;
const CONTROL_WORKERS: u32 = 2;
const MAX_QUEUED_REQUESTS: u64 = 16;
const QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

struct VerificationCancellation(Option<Arc<AtomicBool>>);
impl Drop for VerificationCancellation {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.0 {
            cancelled.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone)]
pub(crate) struct Admission(Arc<State>);

struct State {
    evidence: Arc<Semaphore>,
    control: Arc<Semaphore>,
    evidence_workers: u32,
    control_workers: u32,
    shared: bool,
    closing: AtomicBool,
    queued: AtomicU64,
    rejected: AtomicU64,
}

impl Default for Admission {
    fn default() -> Self {
        Self(Arc::new(State {
            evidence: Arc::new(Semaphore::new(EVIDENCE_WORKERS as usize)),
            control: Arc::new(Semaphore::new(CONTROL_WORKERS as usize)),
            evidence_workers: EVIDENCE_WORKERS,
            control_workers: CONTROL_WORKERS,
            shared: false,
            closing: AtomicBool::new(false),
            queued: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }))
    }
}

impl Admission {
    pub(crate) fn for_kernel(kernel: &airk_kernel::Kernel) -> Self {
        if !kernel.is_service_collection() {
            return Self::default();
        }
        // The inner permit survives transport cancellation and applies to status,
        // rendering and accounting too; an outer IPC grant cannot replace it.
        // The authenticated worker broker owns the live configured admission.
        // This inner lifetime gate retains accepted blocking work, including on cancellation.
        let capacity = airk_core::service_resources::MAX_QUERY_CONCURRENCY as u32 + 1;
        let materialization = Arc::new(Semaphore::new(capacity as usize));
        Self(Arc::new(State {
            evidence: materialization.clone(),
            control: materialization,
            evidence_workers: capacity,
            control_workers: capacity,
            shared: true,
            closing: AtomicBool::new(false),
            queued: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }))
    }
    pub(crate) async fn admit(
        &self,
        control: bool,
    ) -> Result<OwnedSemaphorePermit, CallToolResult> {
        let semaphore = if control {
            &self.0.control
        } else {
            &self.0.evidence
        };
        if !self.0.closing.load(Ordering::Acquire)
            && let Ok(permit) = semaphore.clone().try_acquire_owned()
            && !self.0.closing.load(Ordering::Acquire)
        {
            return Ok(permit);
        }
        if self.0.closing.load(Ordering::Acquire) {
            self.0.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(error("AIRK server is closing"));
        }
        let queued = self
            .0
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_QUEUED_REQUESTS).then_some(n + 1)
            });
        if queued.is_ok() {
            let waiting = Queued(&self.0.queued);
            let result =
                tokio::time::timeout(QUEUE_TIMEOUT, semaphore.clone().acquire_owned()).await;
            drop(waiting);
            if let Ok(Ok(permit)) = result
                && !self.0.closing.load(Ordering::Acquire)
            {
                return Ok(permit);
            }
        }
        self.0.rejected.fetch_add(1, Ordering::Relaxed);
        Err(error(
            "AIRK request queue is full or timed out; retry after outstanding requests complete",
        ))
    }

    pub(crate) fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "evidence_workers": self.0.evidence_workers,
            "control_workers": self.0.control_workers,
            "shared_execution_budget": self.0.shared,
            "queued_requests": self.0.queued.load(Ordering::Acquire),
            "max_queued_requests": MAX_QUEUED_REQUESTS,
            "queue_timeout_ms": QUEUE_TIMEOUT.as_millis(),
            "rejected_requests": self.0.rejected.load(Ordering::Relaxed),
            "cancellation": "Accepted blocking work completes and records once; its permit is retained until completion.",
            "shutdown": "Stop admission and drain accepted work before exit."
        })
    }

    pub(crate) fn close(&self) {
        self.0.closing.store(true, Ordering::Release);
    }

    pub(crate) async fn shutdown(&self) {
        self.close();
        // Accepted work owns permits even if its transport future was cancelled.
        // No blocking request is detached from shutdown accounting.
        let _evidence = self
            .0
            .evidence
            .clone()
            .acquire_many_owned(self.0.evidence_workers)
            .await;
        if !self.0.shared {
            let _control = self
                .0
                .control
                .clone()
                .acquire_many_owned(self.0.control_workers)
                .await;
        }
    }
}

struct Queued<'a>(&'a AtomicU64);

impl Drop for Queued<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

impl AirkServer {
    #[cfg(test)]
    pub(crate) async fn execute<T, F>(&self, operation: UsageOperation, work: F) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        self.execute_response(operation, Default::default(), work)
            .await
    }

    pub(crate) async fn execute_response<T, F>(
        &self,
        operation: UsageOperation,
        options: crate::response::ResponseArgs,
        work: F,
    ) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        self.execute_response_policy(operation, options, true, work)
            .await
    }

    pub(crate) async fn execute_response_policy<T, F>(
        &self,
        operation: UsageOperation,
        options: crate::response::ResponseArgs,
        allow_diagnostics: bool,
        work: F,
    ) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        self.execute_verified_policy(
            operation,
            options,
            allow_diagnostics,
            airk_core::verification::VerificationControls::off(),
            false,
            work,
        )
        .await
    }

    pub(crate) async fn execute_verified_response<T, F>(
        &self,
        operation: UsageOperation,
        options: crate::response::ResponseArgs,
        verification: airk_core::verification::VerificationControls,
        work: F,
    ) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        self.execute_verified_policy(operation, options, true, verification, false, work)
            .await
    }

    pub(crate) async fn execute_verification_aware<T, F>(
        &self,
        operation: UsageOperation,
        options: crate::response::ResponseArgs,
        track: bool,
        work: F,
    ) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        self.execute_verified_policy(
            operation,
            options,
            true,
            airk_core::verification::VerificationControls::off(),
            track,
            work,
        )
        .await
    }

    async fn execute_verified_policy<T, F>(
        &self,
        operation: UsageOperation,
        options: crate::response::ResponseArgs,
        allow_diagnostics: bool,
        verification: airk_core::verification::VerificationControls,
        track_verification: bool,
        work: F,
    ) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        let verification = match verification.resolve() {
            Ok(budget) => budget,
            Err(error) => return crate::response::error_result(&error),
        };
        if self.plan_read {
            // This runs on the outer parse's blocking worker, under its permit.
            // Reuse the exact handlers without recursive transport/admission or usage entries.
            let span = self.kernel.usage_span(operation, self.client);
            return crate::render(
                &self.kernel,
                (span, false),
                work(self.kernel.clone()),
                &self.responses,
                (
                    crate::response::ResponseMode::Full,
                    options.reply_detail,
                    verification,
                ),
                true,
                operation,
            );
        }
        let cancelled = (verification.is_some() || track_verification)
            .then(|| Arc::new(AtomicBool::new(false)));
        let _cancellation = VerificationCancellation(cancelled.clone());
        let responses = self.responses.clone();
        let mode = options.response_mode.unwrap_or(self.response_mode);
        let control = matches!(
            operation,
            UsageOperation::Status | UsageOperation::Capabilities
        );
        let permit = match self.admission.admit(control).await {
            Ok(permit) => permit,
            Err(result) => return result,
        };
        let kernel = self.kernel.clone();
        let client = self.client;
        let diagnostics = if allow_diagnostics && matches!(operation, UsageOperation::Status) {
            Some((
                self.runtime.read().ok().and_then(|value| value.clone()),
                self.work.read().ok().and_then(|value| value.clone()),
            ))
        } else {
            None
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut kernel = match kernel.refresh_service_snapshot() {
                Ok(kernel) => kernel,
                Err(failure) => {
                    // Contention is a safe-to-repeat read refusal, including for
                    // diagnostics with runtime context; it is not readiness evidence.
                    if airk_kernel::retrieval_retry::read_retry_hint(&failure).is_some() {
                        return crate::response::error_result(&failure);
                    }
                    if let Some((runtime, work)) = diagnostics && (runtime.is_some() || work.is_some()) {
                        let status = crate::unavailable_status(&kernel, runtime, work, "AIRK collection selection is busy or unavailable; retry against the current revision");
                        let span = kernel.usage_span(operation, client);
                        return crate::render(&kernel, (span, true), Ok(status), &responses, (mode,options.reply_detail,verification), true, operation);
                    }
                    // Other selection failures retain the path-free generic reply.
                    return error("AIRK collection selection is busy or unavailable; retry against the current revision");
                },
            };
            if let Some(cancelled)=cancelled {kernel=kernel.with_verification_cancellation(cancelled);}
            let span = kernel.usage_span(operation, client);
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(kernel.clone())))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("retrieval worker failed")));
            // Serialization and all optional usage I/O stay off the protocol executor.
            crate::render(&kernel, (span, true), result, &responses, (mode,options.reply_detail,verification), options.include_status || control, operation)
        })
        .await
        .unwrap_or_else(|_| error("retrieval worker failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_overflow_timeout_and_cancellation_release_admission_slots() {
        let admission = Admission::default();
        let _held = admission
            .0
            .evidence
            .clone()
            .acquire_many_owned(EVIDENCE_WORKERS)
            .await
            .unwrap();
        let mut waiting = Vec::new();
        for _ in 0..MAX_QUEUED_REQUESTS {
            let request = admission.clone();
            waiting.push(tokio::spawn(async move { request.admit(false).await }));
        }
        tokio::task::yield_now().await;
        assert_eq!(admission.status()["queued_requests"], MAX_QUEUED_REQUESTS);
        assert!(admission.admit(false).await.is_err());
        waiting.pop().unwrap().abort();
        tokio::task::yield_now().await;
        for task in waiting {
            assert!(task.await.unwrap().is_err());
        }
        assert_eq!(admission.status()["queued_requests"], 0);
        assert_eq!(admission.status()["rejected_requests"], MAX_QUEUED_REQUESTS);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn complete_pipeline_records_success_failure_and_panic_once() -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.txt"), "original request fixture")?;
        let kernel = airk_kernel::Kernel::open(&root, Some(temporary.path().join("state")))?;
        kernel.index()?;
        let server = AirkServer::new(kernel.clone());
        let good = server
            .execute(UsageOperation::Status, |kernel| kernel.status())
            .await;
        assert!(!good.is_error.unwrap_or(false));
        let bad = server
            .execute::<serde_json::Value, _>(UsageOperation::Status, |_| {
                anyhow::bail!("fixture failure")
            })
            .await;
        assert!(bad.is_error.unwrap_or(false));
        let panic = server
            .execute::<serde_json::Value, _>(UsageOperation::Status, |_| panic!("fixture panic"))
            .await;
        assert!(panic.is_error.unwrap_or(false));
        server.admission.shutdown().await;
        let report = kernel.usage_report(1)?;
        let summary = &report.report.operations[&UsageOperation::Status];
        assert_eq!(summary.calls, 3);
        assert_eq!(summary.failed, 2);
        let bytes = [&good, &bad, &panic]
            .iter()
            .map(|value| serde_json::to_vec(value).unwrap().len() as u128)
            .sum::<u128>();
        assert_eq!(summary.returned_utf8_bytes, bytes);
        let logical: serde_json::Value =
            serde_json::from_str(&good.content[0].as_text().unwrap().text)?;
        assert_eq!(logical["request_id"].as_str().unwrap().len(), 32);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saturation_queues_fifth_evidence_request_and_preserves_control_capacity() {
        let admission = Admission::default();
        let permits: Vec<_> = (0..EVIDENCE_WORKERS)
            .map(|_| admission.0.evidence.clone().try_acquire_owned().unwrap())
            .collect();
        let waiting = admission.clone();
        let fifth = tokio::spawn(async move { waiting.admit(false).await });
        tokio::task::yield_now().await;
        assert_eq!(admission.status()["queued_requests"], 1);
        assert!(admission.admit(true).await.is_ok());
        drop(permits);
        assert!(fifth.await.unwrap().is_ok());
        assert_eq!(admission.status()["queued_requests"], 0);
        assert_eq!(admission.status()["rejected_requests"], 0);
        admission.shutdown().await;
        assert!(admission.admit(false).await.is_err());
        assert!(admission.admit(true).await.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_waiter_keeps_worker_permit_and_shutdown_drains() {
        let admission = Admission::default();
        let permits: Vec<_> = (1..EVIDENCE_WORKERS)
            .map(|_| admission.0.evidence.clone().try_acquire_owned().unwrap())
            .collect();
        let permit = admission.admit(false).await.unwrap();
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, hold) = std::sync::mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            entered.send(()).unwrap();
            hold.recv().unwrap();
        });
        started.await.unwrap();
        worker.abort(); // A running blocking task cannot be forcibly cancelled.
        let waiting = admission.clone();
        let queued = tokio::spawn(async move { waiting.admit(false).await });
        tokio::task::yield_now().await;
        assert_eq!(admission.status()["queued_requests"], 1);
        queued.abort();
        let _ = queued.await;
        assert_eq!(admission.status()["queued_requests"], 0);
        let drain_admission = admission.clone();
        let drain = tokio::spawn(async move { drain_admission.shutdown().await });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished());
        // This current-thread executor remains available while the worker is held.
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::task::yield_now().await
        })
        .await
        .unwrap();
        drop(permits);
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), drain)
            .await
            .unwrap()
            .unwrap();
        assert!(worker.await.is_ok());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn service_materialization_survives_cancelled_transport_and_shares_status_and_evidence()
    -> anyhow::Result<()> {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&data)?;
        let kernel = airk_kernel::Kernel::open_service_collection(&source, Some(data), "fixture")?;
        let server = AirkServer::new(kernel);
        let admission = server.admission.clone();
        assert_eq!(admission.status()["shared_execution_budget"], true);
        assert_eq!(admission.status()["evidence_workers"], 65);
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, hold) = std::sync::mpsc::channel();
        let call = tokio::spawn(async move {
            server
                .execute(UsageOperation::Status, move |_| {
                    let _ = entered.send(());
                    hold.recv()?;
                    Ok(serde_json::json!({"fixture":"complete native work"}))
                })
                .await
        });
        started.await?;
        let remaining = admission.0.evidence.clone().acquire_many_owned(64).await?;
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        let waiting = admission.clone();
        let status = tokio::spawn(async move { waiting.admit(true).await });
        let waiting = admission.clone();
        let evidence = tokio::spawn(async move { waiting.admit(false).await });
        tokio::task::yield_now().await;
        assert_eq!(admission.status()["queued_requests"], 2);
        assert!(!status.is_finished() && !evidence.is_finished());
        let draining = admission.clone();
        let drain = tokio::spawn(async move { draining.shutdown().await });
        tokio::task::yield_now().await;
        assert!(
            !drain.is_finished(),
            "cancelled transport returned native memory credit"
        );
        drop(remaining);
        release.send(())?;
        tokio::time::timeout(Duration::from_secs(3), drain).await??;
        assert!(status.await?.is_err());
        assert!(evidence.await?.is_err());
        assert_eq!(admission.status()["queued_requests"], 0);
        Ok(())
    }
}
