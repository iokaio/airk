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
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EVIDENCE_WORKERS: u32 = 4;
const CONTROL_WORKERS: u32 = 2;

#[derive(Clone)]
pub(crate) struct Admission(Arc<State>);

struct State {
    evidence: Arc<Semaphore>,
    control: Arc<Semaphore>,
    closing: AtomicBool,
    rejected: AtomicU64,
}

impl Default for Admission {
    fn default() -> Self {
        Self(Arc::new(State {
            evidence: Arc::new(Semaphore::new(EVIDENCE_WORKERS as usize)),
            control: Arc::new(Semaphore::new(CONTROL_WORKERS as usize)),
            closing: AtomicBool::new(false),
            rejected: AtomicU64::new(0),
        }))
    }
}

impl Admission {
    fn admit(&self, control: bool) -> Result<OwnedSemaphorePermit, CallToolResult> {
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
        self.0.rejected.fetch_add(1, Ordering::Relaxed);
        Err(error(
            "AIRK request capacity unavailable; retry after outstanding requests complete",
        ))
    }

    pub(crate) fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "evidence_workers": EVIDENCE_WORKERS,
            "control_workers": CONTROL_WORKERS,
            "queued_requests": 0,
            "rejected_requests": self.0.rejected.load(Ordering::Relaxed),
            "cancellation": "Accepted blocking work completes and records once; its permit is retained until completion.",
            "shutdown": "Stop admission and drain accepted work before exit."
        })
    }

    pub(crate) async fn shutdown(&self) {
        self.0.closing.store(true, Ordering::Release);
        // Accepted work owns permits even if its transport future was cancelled.
        // No blocking request is detached from shutdown accounting.
        let _evidence = self
            .0
            .evidence
            .clone()
            .acquire_many_owned(EVIDENCE_WORKERS)
            .await;
        let _control = self
            .0
            .control
            .clone()
            .acquire_many_owned(CONTROL_WORKERS)
            .await;
    }
}

fn error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

impl AirkServer {
    pub(crate) async fn execute<T, F>(&self, operation: UsageOperation, work: F) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(airk_kernel::Kernel) -> anyhow::Result<T> + Send + 'static,
    {
        let control = matches!(
            operation,
            UsageOperation::Status | UsageOperation::Capabilities
        );
        let permit = match self.admission.admit(control) {
            Ok(permit) => permit,
            Err(result) => return result,
        };
        let kernel = self.kernel.clone();
        let client = self.client;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let span = kernel.usage_span(operation, client);
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(kernel.clone())))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("retrieval worker failed")));
            // Serialization and all optional usage I/O stay off the protocol executor.
            crate::render(&kernel, span, result)
        })
        .await
        .unwrap_or_else(|_| error("retrieval worker failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn saturation_preserves_control_capacity_and_counts_rejection() {
        let admission = Admission::default();
        let permits: Vec<_> = (0..EVIDENCE_WORKERS)
            .map(|_| admission.admit(false).unwrap())
            .collect();
        assert!(admission.admit(false).is_err());
        assert_eq!(admission.status()["rejected_requests"], 1);
        assert!(admission.admit(true).is_ok());
        drop(permits);
        admission.shutdown().await;
        assert!(admission.admit(false).is_err());
        assert!(admission.admit(true).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_waiter_keeps_worker_permit_and_shutdown_drains() {
        let admission = Admission::default();
        let permits: Vec<_> = (1..EVIDENCE_WORKERS)
            .map(|_| admission.admit(false).unwrap())
            .collect();
        let permit = admission.admit(false).unwrap();
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, hold) = std::sync::mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            entered.send(()).unwrap();
            hold.recv().unwrap();
        });
        started.await.unwrap();
        worker.abort(); // A running blocking task cannot be forcibly cancelled.
        assert!(admission.admit(false).is_err());
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
}
