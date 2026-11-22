// SPDX-License-Identifier: Apache-2.0
//! The handler's side of the Answer job seam, without the service's table:
//! how a failure travels, when a caller's wait ends, and when a job's work
//! runs. The table itself and the whole wired path are tested in
//! `airk-service` (`answer_jobs::wiring`). No model is involved.
use super::*;
use airk_core::arbiter::answer_loop::{
    BudgetV3, CONTEXT_SCHEMA_V3, DegradationReason, REQUEST_SCHEMA_V3,
};
use airk_core::{
    identity::Incarnation,
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::Result;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(120);

fn rendered(error: &anyhow::Error) -> serde_json::Value {
    serde_json::to_value(crate::response::error_result(error)).expect("a tool result is JSON")
}

#[test]
fn a_failure_keeps_the_receipt_the_direct_path_renders_and_says_what_a_retry_may_repeat() {
    // A typed refusal is kept, and every caller of the job renders exactly
    // the receipt the direct path renders.
    let refused = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused);
    let kept = failure(&refused);
    assert!(kept.retain && !kept.not_run);
    assert_eq!(
        rendered(&delivered_failure(kept.payload)),
        rendered(&refused)
    );

    // An untyped failure is kept with its message, and renders as before.
    let plain = anyhow::anyhow!("index mid-publication");
    let kept = failure(&plain);
    assert!(kept.retain && !kept.not_run);
    assert_eq!(rendered(&delivered_failure(kept.payload)), rendered(&plain));

    // A refusal typed safe to repeat is delivered typed and never kept.
    let busy = anyhow::Error::new(airk_kernel::retrieval_retry::SelectionReadAdmissionContention)
        .context("read request");
    let repeatable = failure(&busy);
    assert!(!repeatable.retain && !repeatable.not_run);
    assert_eq!(
        rendered(&delivered_failure(repeatable.payload)),
        rendered(&busy)
    );

    // A job ended by its own closed context or by its stopping owner did
    // nothing a retry could not repeat.
    for unsettled in [
        anyhow::Error::new(crate::read_attempt::ReadAttemptClosed),
        anyhow::Error::new(OwnerStopping),
        anyhow::Error::new(airk_kernel::service_execution::Draining).context("canonical read"),
    ] {
        let dropped = failure(&unsettled);
        assert!(!dropped.retain && !dropped.not_run, "{unsettled:#}");
    }

    // A job that never ran says so in plain words.
    let unrun = not_run("the Answer job's owner is stopping");
    assert!(!unrun.retain && unrun.not_run);
    assert_eq!(
        delivered_failure(unrun.payload).to_string(),
        "the Answer job's owner is stopping"
    );
}

#[test]
fn a_full_queue_is_a_typed_retryable_refusal() {
    let error = anyhow::Error::new(AnswerQueueBusy {
        retry_after_ms: 250,
    });
    let body = rendered(&error);
    assert_eq!(body["isError"], true, "{body}");
    let text = body["content"][0]["text"].as_str().expect("a text block");
    let receipt: serde_json::Value = serde_json::from_str(text).expect("a JSON receipt");
    assert_eq!(receipt["error"]["code"], "answer_queue_busy");
    assert_eq!(receipt["error"]["retryable"], true);
    assert_eq!(receipt["error"]["query_submitted"], false);
    assert_eq!(receipt["error"]["retry_after_ms"], 250);
}

struct Scoped {
    kernel: airk_kernel::Kernel,
    _fixture: tempfile::TempDir,
}
fn scoped() -> Result<Scoped> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::write(
        root.join("public/policy.txt"),
        "amber_seam owns the indexed policy.\nThe amber_seam deadline is original.\n",
    )?;
    let base = airk_kernel::Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let kernel = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: kernel.scoped_read_fingerprint()?.expect("a scoped kernel"),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    Ok(Scoped {
        kernel: kernel.with_native_arbiter(
            worker,
            reference,
            Arc::new(crate::arbiter_handler_tests::Disabled),
        )?,
        _fixture: fixture,
    })
}

fn request(deadline_ms: u64) -> ContextRequestV3 {
    ContextRequestV3 {
        schema: REQUEST_SCHEMA_V3.into(),
        question: "which policy does amber_seam own".into(),
        budget: BudgetV3 {
            deadline_ms,
            ..BudgetV3::default()
        },
        continuation: None,
    }
}

/// One call as the handler builds it after admission, with `jobs` as its
/// server's table.
fn caller(
    scoped: &Scoped,
    jobs: Arc<dyn AnswerJobs>,
    window: Duration,
    canceled: CancellationToken,
) -> Result<Caller> {
    let deadline = Instant::now() + window;
    let kernel = scoped
        .kernel
        .with_context_reply_reserve(4096)?
        .with_fresh_answer_accounting()?;
    let mut reader = AirkServer::new(kernel.clone());
    reader.plan_read = true;
    reader.answer_read = true;
    reader.read_attempt = Some(ReadAttemptContext::new(deadline, canceled));
    Ok(Caller {
        jobs,
        reader,
        kernel,
        request: request(u64::try_from(window.as_millis()).unwrap_or(u64::MAX)),
        reply_reserve: 4096,
        deadline,
        handle: tokio::runtime::Handle::current(),
        sink: crate::progress::no_progress(),
    })
}

/// A table that takes a call and never answers it.
struct Silent {
    calls: watch::Sender<usize>,
}
impl AnswerJobs for Silent {
    fn call(
        &self,
        _: AnswerCall,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<AnswerDelivery>> + Send + '_>> {
        self.calls.send_modify(|calls| *calls += 1);
        Box::pin(std::future::pending())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_caller_that_leaves_or_whose_owner_stops_ends_only_its_own_wait() -> Result<()> {
    let scoped = scoped()?;
    let silent = Arc::new(Silent {
        calls: watch::channel(0).0,
    });
    let mut calls = silent.calls.subscribe();

    // The client's transport cancels the call while it waits on the table.
    let client = CancellationToken::new();
    let waiting = caller(
        &scoped,
        silent.clone(),
        Duration::from_secs(60),
        client.clone(),
    )?;
    let waited = tokio::task::spawn_blocking(move || run(waiting));
    tokio::time::timeout(WAIT, calls.wait_for(|calls| *calls == 1)).await??;
    client.cancel();
    let error = tokio::time::timeout(WAIT, waited)
        .await??
        .expect_err("a cancelled caller has no reply");
    assert!(
        error.is::<crate::read_attempt::ReadAttemptClosed>(),
        "{error:#}"
    );

    // The owner stops: admission closes under a caller that is waiting.
    let waiting = caller(
        &scoped,
        silent.clone(),
        Duration::from_secs(60),
        CancellationToken::new(),
    )?;
    let admission = waiting.reader.admission.clone();
    let waited = tokio::task::spawn_blocking(move || run(waiting));
    tokio::time::timeout(WAIT, calls.wait_for(|calls| *calls == 2)).await??;
    admission.close();
    let error = tokio::time::timeout(WAIT, waited)
        .await??
        .expect_err("a stopping owner delivers no reply");
    assert!(error.is::<OwnerStopping>(), "{error:#}");
    Ok(())
}

fn admission(run_ms: u64, rounds: usize) -> AnswerAdmission {
    let request = request(60_000);
    AnswerAdmission {
        parts: request.budget.max_parts,
        sub_needs: request.budget.max_sub_needs,
        rounds,
        context_model: false,
        degradations: if rounds < request.budget.rounds {
            vec![Degradation::Rounds {
                requested: request.budget.rounds,
                applied: rounds,
                reason: DegradationReason::QueueWait,
            }]
        } else {
            vec![]
        },
        request,
        run_ms,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_jobs_work_runs_the_admitted_plan_and_only_inside_its_makers_window() -> Result<()> {
    let scoped = scoped()?;
    let silent = Arc::new(Silent {
        calls: watch::channel(0).0,
    });
    let mut maker = caller(
        &scoped,
        silent,
        Duration::from_secs(60),
        CancellationToken::new(),
    )?;
    let admission_gate = maker.reader.admission.clone();
    // Each progress report notes how many jobs its owner counts as running.
    let counted = Arc::new(std::sync::Mutex::new(Vec::new()));
    maker.sink = {
        let (counted, owner) = (counted.clone(), admission_gate.clone());
        Arc::new(move |_| {
            if let Ok(mut counted) = counted.lock() {
                counted.push(owner.jobs_in_flight());
            }
        })
    };
    let work = work(&maker);
    // The maker leaves; its work is the job's now.
    drop(maker);

    // A window the table opened beyond the maker's is not run.
    let beyond = tokio::time::timeout(WAIT, work(admission(120_000, 3)))
        .await?
        .map(|reply| reply.elapsed_ms)
        .expect_err("a window beyond the maker's is not served");
    assert!(beyond.not_run && !beyond.retain);
    assert_eq!(admission_gate.jobs_in_flight(), 0);

    // Inside the window the work runs the real loop under the plan, and the
    // reply says what the plan took away.
    let reply = tokio::time::timeout(WAIT, work(admission(30_000, 1)))
        .await?
        .map_err(|failure| anyhow::anyhow!(failure.payload))?;
    assert_eq!(reply.schema, CONTEXT_SCHEMA_V3);
    assert!(!reply.parts.is_empty() && !reply.records.is_empty());
    assert!(reply.parts.iter().all(|part| part.rounds.len() <= 1));
    let rounds: Vec<_> = serde_json::to_value(&reply.degradations)?
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["kind"] == "rounds")
        .cloned()
        .collect();
    assert_eq!(
        rounds,
        [serde_json::json!({"kind":"rounds","requested":3,"applied":1,"reason":"queue_wait"})]
    );
    // The same work answers a second job: its accounting is fresh each time.
    let again = tokio::time::timeout(WAIT, work(admission(30_000, 3)))
        .await?
        .map_err(|failure| anyhow::anyhow!(failure.payload))?;
    assert!(
        serde_json::to_value(&again.degradations)?
            .as_array()
            .into_iter()
            .flatten()
            .all(|item| item["kind"] != "rounds")
    );

    // While a job ran, its owner counted it, although no call was waiting:
    // an owner with a running job is not idle. Afterwards it counts none.
    let counted = counted
        .lock()
        .map(|counted| counted.clone())
        .unwrap_or_default();
    assert!(
        !counted.is_empty() && counted.iter().all(|running| *running == 1),
        "{counted:?}"
    );
    assert_eq!(admission_gate.jobs_in_flight(), 0);

    // Once the owner stops, a job does not start.
    admission_gate.close();
    let stopped = tokio::time::timeout(WAIT, work(admission(30_000, 3)))
        .await?
        .map(|reply| reply.elapsed_ms)
        .expect_err("a stopping owner starts no job");
    assert!(stopped.not_run && !stopped.retain);
    Ok(())
}
