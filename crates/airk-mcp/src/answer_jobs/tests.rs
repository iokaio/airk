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
fn refused_failure(refusal: AnswerJobRefusal) -> AnswerFailure {
    refused(refusal, false, None)
}

/// An owner's job authority without a supervisor: it counts its binds,
/// attempts and quarantines, and mints attempts with a fixed clock reading.
#[derive(Default)]
struct Authority {
    binds: std::sync::atomic::AtomicUsize,
    attempts: std::sync::Mutex<Vec<Duration>>,
    stuck: std::sync::atomic::AtomicUsize,
}
impl AnswerJobAuthority for Authority {
    fn bind(&self, _: &airk_kernel::Kernel) -> anyhow::Result<AnswerJobGrant> {
        self.binds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(AnswerJobGrant {
            guard: Box::new(()),
            maker_session: CancellationToken::new(),
        })
    }
    fn attempt(
        &self,
        deadline: Instant,
    ) -> anyhow::Result<airk_core::read_attempt::ReadAttemptBudget> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.push(remaining);
        }
        let count = self.attempts.lock().map(|attempts| attempts.len()).unwrap_or(0);
        Ok(airk_core::read_attempt::ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([u8::try_from(count).unwrap_or(u8::MAX); 16]),
            clock: if cfg!(windows) {
                airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
            } else {
                airk_core::read_attempt::ReadClock::UnixMonotonic
            },
            expires_monotonic_ns: u64::MAX / 2,
            remaining_ns: u64::try_from(remaining.as_nanos()).unwrap_or(u64::MAX).max(1),
            native_retry_expires_monotonic_ns: None,
        })
    }
    fn stuck(&self) {
        self.stuck.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn a_failure_keeps_the_receipt_the_direct_path_renders_and_says_what_a_retry_may_repeat() {
    // A scope refusal belongs to its own invocation (review H1): it is never
    // kept, and its caller renders exactly the receipt the direct path renders.
    let refused = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused);
    let own = failure(&refused);
    assert!(!own.retain && !own.not_run);
    assert_eq!(rendered(&delivered_failure(own.payload)), rendered(&refused));
    // A refusal of the job path itself renders as its typed receipt.
    // Each says whether the job's reads ran (independent review, item 3): a
    // client told nothing was submitted may replay work that did run.
    for (refusal, submitted) in [
        (AnswerJobRefusal::authority_ended(false), false),
        (AnswerJobRefusal::authority_ended(true), true),
        (AnswerJobRefusal::stuck(), true),
        (AnswerJobRefusal::key_conflict(), false),
    ] {
        let carried = refused_failure(refusal.clone());
        assert!(!carried.retain);
        let body = rendered(&delivered_failure(carried.payload));
        let text = body["content"][0]["text"].as_str().expect("a text block");
        let receipt: serde_json::Value = serde_json::from_str(text).expect("a JSON receipt");
        assert_eq!(receipt["error"]["code"], refusal.code.as_str(), "{body}");
        assert_eq!(receipt["error"]["retryable"], refusal.retryable, "{body}");
        assert_eq!(receipt["error"]["query_submitted"], submitted, "{body}");
    }

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
        authority: Arc::new(Authority::default()),
        reader,
        kernel,
        request: request(u64::try_from(window.as_millis()).unwrap_or(u64::MAX)),
        options: Default::default(),
        reply_reserve: 4096,
        include_status: false,
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

/// Review item 2: a job reply carries its delivery record beside the reply
/// Union packed. A reply packed to its limit (`reply_limit - reserve`,
/// measured as Union measures it, escaped), with the largest valid delivery
/// record and the caller's status, still renders within the 131,072-byte
/// Answer limit through the scoped handler path, with and without saved-source
/// verification. Above it the handler would refuse the whole reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_packed_to_its_limit_with_status_and_a_delivery_record_fits_the_answer_limit()
-> Result<()> {
    use airk_core::arbiter::answer_loop::{
        DEADLINE_TOLERANCE_MS, MAX_ANSWER_RESPONSE_BYTES, MAX_DEADLINE_MS, MAX_ESTIMATE_MS,
    };
    let scoped = scoped()?;
    let delivery = AnswerDeliveryRecord {
        job: airk_core::identity::Sha256Digest::from_bytes([0xff; 32]),
        queue_ms: MAX_ESTIMATE_MS,
        run_ms: MAX_DEADLINE_MS + DEADLINE_TOLERANCE_MS,
        wait_ms: MAX_DEADLINE_MS + DEADLINE_TOLERANCE_MS,
        reused: false,
    };
    delivery.validate()?;
    for mode in [None, Some(airk_core::verification::VerificationMode::Off)] {
        let verification = crate::response::VerificationArgs {
            verification: mode,
            ..Default::default()
        };
        let reserve = crate::answer_reply_reserve(verification.controls())?;
        let limit = MAX_ANSWER_RESPONSE_BYTES - reserve;
        // A stand-in with exactly the bytes Union may pack.
        let mut packed = serde_json::json!({"schema":CONTEXT_SCHEMA_V3,"padding":""});
        let room = limit - airk_core::verification::quoted_len(&packed)?;
        packed["padding"] = "x".repeat(room).into();
        assert_eq!(airk_core::verification::quoted_len(&packed)?, limit);
        let mut answered = packed;
        answered["delivery"] = serde_json::to_value(&delivery)?;
        let server = AirkServer::new(scoped.kernel.clone());
        let rendered = server
            .execute_verified_response(
                airk_core::usage::UsageOperation::Answer,
                crate::response::ResponseArgs {
                    include_status: true,
                    ..Default::default()
                },
                verification.controls(),
                move |_| Ok(answered),
            )
            .await;
        let bytes = serde_json::to_vec(&rendered)?.len();
        let text = rendered.content[0]
            .as_text()
            .map(|text| text.text.clone())
            .unwrap_or_default();
        assert_ne!(
            rendered.is_error,
            Some(true),
            "{mode:?}: {}",
            &text[..text.len().min(600)]
        );
        assert!(bytes <= MAX_ANSWER_RESPONSE_BYTES, "{mode:?}: {bytes}");
        let body: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(body["delivery"], serde_json::to_value(&delivery)?, "{mode:?}");
    }
    Ok(())
}

fn admission(run_ms: u64, rounds: usize) -> AnswerAdmission {
    let request = request(run_ms);
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

/// A maker with a counting authority, its job kernel and the job's work.
fn job(
    scoped: &Scoped,
    window: Duration,
) -> Result<(Caller, Arc<Authority>, airk_kernel::Kernel, AnswerWork)> {
    let silent = Arc::new(Silent {
        calls: watch::channel(0).0,
    });
    let mut maker = caller(scoped, silent, window, CancellationToken::new())?;
    let authority = Arc::new(Authority::default());
    maker.authority = authority.clone();
    let job_kernel = maker.kernel.answer_job_kernel()?;
    let work = work(&maker, job_kernel.clone(), Arc::new(()));
    Ok((maker, authority, job_kernel, work))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_jobs_work_runs_on_its_own_kernel_and_attempt_after_its_maker_is_gone() -> Result<()> {
    let scoped = scoped()?;
    let (mut maker, authority, job_kernel, work) = job(&scoped, Duration::from_secs(60))?;
    let owner = maker.reader.admission.clone();
    // Each progress report notes how many jobs its owner counts as running.
    let counted = Arc::new(std::sync::Mutex::new(Vec::new()));
    maker.sink = {
        let (counted, owner) = (counted.clone(), owner.clone());
        Arc::new(move |_| {
            if let Ok(mut counted) = counted.lock() {
                counted.push(owner.jobs_in_flight());
            }
        })
    };
    let work = {
        let job_kernel = job_kernel.clone();
        // The first work is replaced unrun; the owner stops counting it.
        drop(work);
        super::work(&maker, job_kernel, Arc::new(()))
    };
    // The maker's session ends (H1): its kernel is revoked, its transport
    // context cancelled, and it leaves.
    maker.kernel.revoke_scoped_reads();
    if let Some(context) = &maker.reader.read_attempt {
        context.cancellation().cancel();
    }
    drop(maker);
    assert!(job_kernel.scoped_read_fingerprint().is_ok());

    // A window longer than the maker's 60 s runs on the job's own attempt,
    // under the plan, and the reply says what the plan took away.
    let reply = tokio::time::timeout(WAIT, work(admission(120_000, 1)))
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
    let attempts = authority.attempts.lock().map(|a| a.clone()).unwrap_or_default();
    assert_eq!(attempts.len(), 1, "one attempt of the job's own");
    assert!(
        attempts[0] > Duration::from_secs(60) && attempts[0] <= Duration::from_secs(120),
        "{attempts:?}"
    );

    // The same work answers a second job: fresh accounting, a fresh attempt.
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
    assert_eq!(authority.attempts.lock().map(|a| a.len()).unwrap_or(0), 2);

    // While a job ran, its owner counted it; afterwards it counts none.
    let counted = counted.lock().map(|c| c.clone()).unwrap_or_default();
    assert!(
        !counted.is_empty() && counted.iter().all(|running| *running == 1),
        "{counted:?}"
    );
    assert_eq!(owner.jobs_in_flight(), 0);

    // The grant ends (its job kernel is revoked): the job does not run, and
    // the refusal is its own, never kept for others.
    job_kernel.revoke_scoped_reads();
    let ended = tokio::time::timeout(WAIT, work(admission(30_000, 3)))
        .await?
        .map(|reply| reply.elapsed_ms)
        .expect_err("a revoked job kernel reads nothing");
    assert!(!ended.not_run && !ended.retain);
    let ended = delivered_failure(ended.payload);
    let refusal = ended
        .downcast_ref::<AnswerJobRefusal>()
        .expect("a typed job refusal");
    // Refused before its loop started: nothing was read.
    assert!(!refusal.query_submitted, "{refusal:?}");

    // Once the owner stops, a job does not start. (A fresh session: the
    // maker above shared, and revoked, the fixture's session scope.)
    let other = self::scoped()?;
    let (_maker, _, _, work) = job(&other, Duration::from_secs(60))?;
    _maker.reader.admission.close();
    let stopped = tokio::time::timeout(WAIT, work(admission(30_000, 3)))
        .await?
        .map(|reply| reply.elapsed_ms)
        .expect_err("a stopping owner starts no job");
    assert!(stopped.not_run && !stopped.retain);
    assert_eq!(authority.stuck.load(std::sync::atomic::Ordering::SeqCst), 0);
    Ok(())
}

/// A job counts for its owner from the moment it is made: queued, with its
/// maker gone and no connection left, it keeps the owner from leaving as
/// idle (the owner's 15 s idle exit asks `jobs_in_flight`). The count ends
/// when the job's work returns, or when the table drops the work unrun.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_job_keeps_its_owner_from_leaving_as_idle_until_its_work_ends() -> Result<()> {
    let scoped = scoped()?;
    let (maker, _, _, work) = job(&scoped, Duration::from_secs(60))?;
    let owner = maker.reader.admission.clone();
    drop(maker);
    assert_eq!(owner.jobs_in_flight(), 1, "a queued job whose maker left");
    let reply = tokio::time::timeout(WAIT, work(admission(30_000, 3)))
        .await?
        .map_err(|failure| anyhow::anyhow!(failure.payload))?;
    assert!(!reply.parts.is_empty());
    assert_eq!(owner.jobs_in_flight(), 0, "a finished job");

    let (maker, _, _, unrun) = job(&scoped, Duration::from_secs(60))?;
    let owner = maker.reader.admission.clone();
    drop(maker);
    assert_eq!(owner.jobs_in_flight(), 1);
    drop(unrun);
    assert_eq!(owner.jobs_in_flight(), 0, "a job dropped without running");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_past_its_deadline_quarantines_its_owner_and_holds_capacity_until_it_settles()
-> Result<()> {
    let scoped = scoped()?;
    let (maker, authority, _, work) = job(&scoped, Duration::from_secs(60))?;
    let owner = maker.reader.admission.clone();
    // The job's blocking work is held before its loop starts, past its
    // deadline and grace, until the test releases it.
    let (entered, started) = std::sync::mpsc::channel::<()>();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let held = Arc::new(std::sync::Mutex::new(held));
    test_hooks::on_job_start(Some(Arc::new(move || {
        let _ = entered.send(());
        if let Ok(held) = held.lock() {
            let _ = held.recv_timeout(WAIT);
        }
    })));
    let running = tokio::spawn(work(admission(50, 1)));
    tokio::task::spawn_blocking(move || started.recv_timeout(WAIT)).await??;
    assert_eq!(owner.jobs_in_flight(), 1);
    let outcome = tokio::time::timeout(WAIT, running).await??;
    test_hooks::on_job_start(None);
    let failure = outcome
        .map(|reply| reply.elapsed_ms)
        .expect_err("a stuck job delivers no reply");
    assert!(!failure.retain && !failure.not_run);
    assert_eq!(authority.stuck.load(std::sync::atomic::Ordering::SeqCst), 1);
    // The owner no longer counts it: a stuck job cannot hold the idle exit.
    assert_eq!(owner.jobs_in_flight(), 0);
    let receipt = rendered(&delivered_failure(failure.payload));
    assert!(receipt.to_string().contains("answer_job_stuck"), "{receipt}");
    // Its reads ran (and may still run): the receipt says so.
    let text: serde_json::Value = serde_json::from_str(
        receipt["content"][0]["text"].as_str().expect("a text block"),
    )?;
    assert_eq!(text["error"]["query_submitted"], true, "{receipt}");
    // Its capacity is not free until the blocking work has returned.
    let mut settling = failure.settling.expect("a stuck job is still settling");
    let early = tokio::select! {
        biased;
        _ = &mut settling => true,
        _ = std::future::ready(()) => false,
    };
    assert!(!early, "a stuck job settled while its work still ran");
    release.send(())?;
    tokio::time::timeout(WAIT, settling).await?;
    Ok(())
}
