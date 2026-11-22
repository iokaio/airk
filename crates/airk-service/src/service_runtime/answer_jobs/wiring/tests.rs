// SPDX-License-Identifier: Apache-2.0
//! The wired Answer path: the real MCP `airk_answer` handler, the real kernel
//! loop over a real index and the real job table. A gate at the `AnswerJobs`
//! seam holds each job's work until the test releases it, so the tests wait
//! on events under deadlines and never sleep. The native controller is a
//! disabled one: these tests claim nothing about models.
use super::*;
use crate::service_runtime::answer_jobs::{
    CostTable, JobOutcome,
    estimate::StageCosts,
    schedule::{EntryState, Schedule, Submitted},
};
use airk_core::{
    arbiter::{
        Availability, IndexApplication, ScopeBinding,
        answer_loop::{BudgetV3, CONTEXT_SCHEMA_V3, ContextRequestV3, REQUEST_SCHEMA_V3},
    },
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    scoped_reads::SourceDescriptor,
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::Kernel;
use airk_mcp::{answer_jobs::AnswerFailure, local::LocalTools, read_attempt::ReadAttemptContext};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Every wait in these tests ends on its event or fails here.
const WAIT: Duration = Duration::from_secs(120);

struct Disabled;
impl airk_kernel::arbiter::NativeController for Disabled {
    fn application(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        Ok(IndexApplication {
            scope: ScopeBinding {
                installation: Sha256Digest::from_bytes([7; 32]),
                catalog: Incarnation::from_bytes([8; 16]),
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: 1,
                publication: publication.clone(),
            },
            profile: None,
            availability: Availability::Disabled,
            reason: "Explicitly disabled test owner".into(),
        })
    }
}

/// A scoped, natively admitted kernel over a small indexed source.
struct Indexed {
    kernel: Kernel,
    _fixture: tempfile::TempDir,
}
fn indexed() -> Result<Indexed> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    for index in 0..3 {
        std::fs::write(
            root.join(format!("public/policy-{index}.txt")),
            format!(
                "amber_queue owns indexed policy {index}.\nThe amber_queue deadline is original.\n"
            ),
        )?;
    }
    let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let text = |path: &std::path::Path| path.to_str().map(str::to_owned);
    let authority = airk_kernel::scoped_reads::ScopedReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            text(base.root()).ok_or_else(|| anyhow!("fixture root"))?,
            text(base.data_dir()).ok_or_else(|| anyhow!("fixture data"))?,
            base.collection().into(),
        )?
        .binding,
        index_id: index,
        project_ids: vec![project],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
            host: Sha256Digest::from_bytes([5; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: "public".into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: text(base.root()).ok_or_else(|| anyhow!("fixture root"))?,
        }],
    };
    let kernel = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: kernel
            .scoped_read_fingerprint()?
            .ok_or_else(|| anyhow!("fixture fingerprint"))?,
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    Ok(Indexed {
        kernel: kernel.with_native_arbiter(worker, reference, Arc::new(Disabled))?,
        _fixture: fixture,
    })
}

/// Holds every job's work at the seam until the test opens it.
struct Gate {
    open: watch::Sender<bool>,
    /// Works that have started, as the table's jobs.
    started: watch::Sender<usize>,
    running: AtomicUsize,
    peak: AtomicUsize,
}
impl Gate {
    fn new(open: bool) -> Arc<Self> {
        Arc::new(Self {
            open: watch::channel(open).0,
            started: watch::channel(0).0,
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        })
    }
    async fn started_at_least(&self, count: usize) {
        let mut started = self.started.subscribe();
        tokio::time::timeout(WAIT, started.wait_for(|started| *started >= count))
            .await
            .expect("the jobs start under the deadline")
            .expect("the gate outlives its jobs");
    }
    fn started(&self) -> usize {
        *self.started.borrow()
    }
}
struct Gated {
    inner: Arc<ServiceAnswerJobs>,
    gate: Arc<Gate>,
}
impl AnswerJobs for Gated {
    fn call(
        &self,
        mut call: AnswerCall,
    ) -> Pin<Box<dyn Future<Output = Result<AnswerDelivery>> + Send + '_>> {
        let (work, gate) = (call.work.clone(), self.gate.clone());
        call.work = Arc::new(move |mut admission: AnswerAdmission| {
            let (work, gate) = (work.clone(), gate.clone());
            Box::pin(async move {
                let arrived = Instant::now();
                let running = gate.running.fetch_add(1, Ordering::SeqCst) + 1;
                gate.peak.fetch_max(running, Ordering::SeqCst);
                gate.started.send_modify(|started| *started += 1);
                let mut open = gate.open.subscribe();
                let _ = open.wait_for(|open| *open).await;
                // `run_ms` is the time from now to the job's deadline.
                admission.run_ms = admission.run_ms.saturating_sub(
                    u64::try_from(arrived.elapsed().as_millis()).unwrap_or(u64::MAX),
                );
                let result = work(admission).await;
                gate.running.fetch_sub(1, Ordering::SeqCst);
                result
            })
        });
        self.inner.call(call)
    }
}

/// Every job costs exactly `ms`, whatever its shape.
fn flat(ms: u64) -> CostTable {
    CostTable {
        no_model: StageCosts {
            fixed_ms: ms,
            per_part_ms: 0,
            per_sub_need_round_ms: 0,
        },
        compact: None,
    }
}

struct Wired {
    tools: Arc<LocalTools>,
    jobs: Arc<ServiceAnswerJobs>,
    gate: Arc<Gate>,
    _indexed: Indexed,
}
async fn wired(config: JobTableConfig, open: bool) -> Result<Wired> {
    let indexed = indexed()?;
    let jobs = ServiceAnswerJobs::with_config(config)?;
    let gate = Gate::new(open);
    let tools = LocalTools::new(indexed.kernel.clone()).await?;
    tools.set_answer_jobs(Arc::new(Gated {
        inner: jobs.clone(),
        gate: gate.clone(),
    }));
    Ok(Wired {
        tools: Arc::new(tools),
        jobs,
        gate,
        _indexed: indexed,
    })
}

fn arguments(question: &str, deadline_ms: u64) -> Result<serde_json::Map<String, Value>> {
    let request = ContextRequestV3 {
        schema: REQUEST_SCHEMA_V3.into(),
        question: question.into(),
        budget: BudgetV3 {
            deadline_ms,
            ..BudgetV3::default()
        },
        continuation: None,
    };
    request.validate()?;
    let mut arguments = serde_json::to_value(&request)?;
    arguments["response_mode"] = json!("full");
    arguments
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("Answer arguments"))
}

/// One `airk_answer` call through the real handler, as a transport makes it.
fn answer(
    wired: &Wired,
    question: &str,
    deadline_ms: u64,
    canceled: CancellationToken,
) -> tokio::task::JoinHandle<Result<Value>> {
    let tools = wired.tools.clone();
    let question = question.to_owned();
    tokio::spawn(async move {
        let context = ReadAttemptContext::new(
            Instant::now() + Duration::from_millis(deadline_ms),
            canceled,
        );
        let reply = tools
            .call_with_read_context(
                "airk_answer".into(),
                arguments(&question, deadline_ms)?,
                context,
            )
            .await?;
        let text = reply
            .content
            .first()
            .and_then(|content| content.as_text())
            .ok_or_else(|| anyhow!("the reply has no JSON text"))?;
        let mut body: Value = serde_json::from_str(&text.text)?;
        body["is_error"] = json!(reply.is_error == Some(true));
        Ok(body)
    })
}
async fn delivered(call: tokio::task::JoinHandle<Result<Value>>) -> Result<Value> {
    tokio::time::timeout(WAIT, call)
        .await
        .map_err(|_| anyhow!("the call did not return under the deadline"))??
}

/// A complete answer: the /3 schema, at least one part, and no ticket.
fn assert_complete(body: &Value) {
    assert_eq!(body["is_error"], false, "{body}");
    assert_eq!(body["schema"], CONTEXT_SCHEMA_V3, "{body}");
    assert!(body["ticket"].is_null(), "{body}");
    assert!(
        body["parts"]
            .as_array()
            .is_some_and(|parts| !parts.is_empty()),
        "{body}"
    );
    assert!(
        body["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty()),
        "{body}"
    );
    assert_deliverable(body);
}
/// The check the delivery planes make on a singleton Answer before it leaves
/// the service: Core validation under the widest budget. A reply without
/// model work binds no stage.
fn assert_deliverable(body: &Value) {
    let bindings = airk_core::combined_context::context_delivery_bindings(body);
    assert!(
        bindings.as_ref().is_ok_and(Vec::is_empty),
        "{bindings:?}: {body}"
    );
}
fn admission_kinds(body: &Value) -> Vec<&str> {
    body["degradations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["kind"].as_str())
        .filter(|kind| matches!(*kind, "parts" | "sub_needs" | "rounds" | "model"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_and_four_concurrent_answers_all_complete_within_the_gathering_limit() -> Result<()> {
    for count in [2usize, 4] {
        let wired = wired(
            JobTableConfig {
                costs: flat(100),
                configured_rung_only: true,
                ..JobTableConfig::default()
            },
            false,
        )
        .await?;
        let calls: Vec<_> = (0..count)
            .map(|index| {
                answer(
                    &wired,
                    &format!("which policy number {index} does amber_queue own"),
                    120_000,
                    CancellationToken::new(),
                )
            })
            .collect();
        // Every call is waiting in the table, and no more jobs run than the
        // gathering limit of two; the others are queued, holding nothing.
        tokio::time::timeout(WAIT, wired.jobs.table().waiting_at_least(count))
            .await
            .map_err(|_| anyhow!("the calls did not reach the table"))?;
        wired.gate.started_at_least(2).await;
        let status = wired.jobs.table().status()?;
        assert_eq!(status.gathering_limit, 2);
        assert_eq!(
            (status.running, status.queued),
            (2, count - 2),
            "{status:?}"
        );
        assert_eq!(wired.gate.started(), 2);
        wired.gate.open.send_replace(true);
        for call in calls {
            let body = delivered(call).await?;
            assert_complete(&body);
            assert!(admission_kinds(&body).is_empty(), "{body}");
        }
        // None starved and none ran twice: one job per call, two at a time.
        assert_eq!(wired.gate.started(), count);
        assert_eq!(wired.gate.peak.load(Ordering::SeqCst), 2);
        let status = wired.jobs.table().status()?;
        assert_eq!(
            (status.running, status.queued, status.retained_results),
            (0, 0, count),
            "{status:?}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_after_the_client_left_attaches_to_the_job_that_kept_running() -> Result<()> {
    let wired = wired(
        JobTableConfig {
            costs: flat(100),
            configured_rung_only: true,
            ..JobTableConfig::default()
        },
        false,
    )
    .await?;
    let question = "which policy does amber_queue own";
    // The first client gives up while its job is in flight: its transport
    // cancels the call. Only its wait ends.
    let client = CancellationToken::new();
    let first = answer(&wired, question, 120_000, client.clone());
    wired.gate.started_at_least(1).await;
    client.cancel();
    let left = delivered(first).await?;
    assert_eq!(left["is_error"], true, "{left}");
    let status = wired.jobs.table().status()?;
    assert_eq!((status.running, status.queued), (1, 0), "{status:?}");
    assert!(
        status
            .entries
            .iter()
            .all(|entry| entry.state == EntryState::Running),
        "{status:?}"
    );

    // The retry is the same request in the same scope: it joins the job.
    let retry = answer(&wired, question, 120_000, CancellationToken::new());
    tokio::time::timeout(WAIT, wired.jobs.table().waiting_at_least(2))
        .await
        .map_err(|_| anyhow!("the retry did not reach the table"))?;
    let status = wired.jobs.table().status()?;
    assert_eq!((status.running, status.queued), (1, 0), "{status:?}");
    assert_eq!(wired.gate.started(), 1);
    wired.gate.open.send_replace(true);
    let answered = delivered(retry).await?;
    assert_complete(&answered);

    // One job ran, once, and its result is kept: a later call gets the same
    // answer without another run.
    let again = delivered(answer(&wired, question, 120_000, CancellationToken::new())).await?;
    assert_complete(&again);
    assert_eq!(again["decision_log"], answered["decision_log"]);
    assert_eq!(again["records"], answered["records"]);
    assert_eq!(wired.gate.started(), 1);
    let status = wired.jobs.table().status()?;
    assert_eq!(
        (
            status.running,
            status.queued,
            status.retained_results,
            status.recent.len()
        ),
        (0, 0, 1, 1),
        "{status:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_whose_work_cannot_fit_its_deadline_is_reduced_and_says_so() -> Result<()> {
    // 2 s for each sub-need and round: four parts of four sub-needs over
    // three rounds are 96 s, two rounds 64 s, one round 32 s. Only one round
    // fits a 60 s deadline.
    let wired = wired(
        JobTableConfig {
            costs: CostTable {
                no_model: StageCosts {
                    fixed_ms: 0,
                    per_part_ms: 0,
                    per_sub_need_round_ms: 2_000,
                },
                compact: None,
            },
            configured_rung_only: true,
            ..JobTableConfig::default()
        },
        true,
    )
    .await?;
    let body = delivered(answer(
        &wired,
        "1. which policy does amber_queue own\n2. where is the amber_queue deadline?",
        60_000,
        CancellationToken::new(),
    ))
    .await?;
    assert_complete(&body);
    let rounds: Vec<&Value> = body["degradations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["kind"] == "rounds")
        .collect();
    assert_eq!(
        rounds,
        [&json!({"kind":"rounds","requested":3,"applied":1,"reason":"deadline_estimate"})],
        "{body}"
    );
    assert_eq!(admission_kinds(&body), ["rounds"], "{body}");
    // The loop really ran inside the plan: no part went past one round.
    for part in body["parts"].as_array().into_iter().flatten() {
        assert!(
            part["rounds"]
                .as_array()
                .is_some_and(|rounds| rounds.len() <= 1),
            "{body}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_that_cannot_start_in_time_gets_a_ticket_at_once_and_an_answer_on_its_retry()
-> Result<()> {
    // Every job is estimated at 50 s. Two fit their 60 s deadlines and run;
    // a third would start after them with 10 s left, less than any answer.
    let wired = wired(
        JobTableConfig {
            costs: flat(50_000),
            configured_rung_only: true,
            ..JobTableConfig::default()
        },
        false,
    )
    .await?;
    let running: Vec<_> = (0..2)
        .map(|index| {
            answer(
                &wired,
                &format!("which policy number {index} does amber_queue own"),
                60_000,
                CancellationToken::new(),
            )
        })
        .collect();
    wired.gate.started_at_least(2).await;
    let question = "where is the amber_queue deadline";
    // The third call returns while both jobs are still held: a ticket, not a
    // wait to its deadline.
    let ticketed = delivered(answer(&wired, question, 60_000, CancellationToken::new())).await?;
    assert_eq!(wired.gate.started(), 2);
    assert_eq!(ticketed["is_error"], false, "{ticketed}");
    assert_eq!(ticketed["schema"], CONTEXT_SCHEMA_V3);
    assert_eq!(
        ticketed["partial"],
        json!({"stage":"admission","reason":"queued"}),
        "{ticketed}"
    );
    assert!(ticketed["parts"].as_array().is_some_and(Vec::is_empty));
    let ticket = &ticketed["ticket"];
    assert_eq!(ticket["queue_position"], 1, "{ticketed}");
    let (start, finish, retry) = (
        ticket["estimated_start_ms"].as_u64(),
        ticket["estimated_finish_ms"].as_u64(),
        ticket["retry_after_ms"].as_u64(),
    );
    assert!(
        start.is_some_and(|start| (1..=50_000).contains(&start))
            && finish > start
            && retry.is_some_and(|retry| retry >= 1),
        "{ticketed}"
    );
    assert_eq!(ticket["id"].as_str().map(str::len), Some(64), "{ticketed}");
    // A ticket-shaped reply passes the delivery planes' check as well.
    assert_deliverable(&ticketed);

    wired.gate.open.send_replace(true);
    for call in running {
        assert_complete(&delivered(call).await?);
    }
    // The retry of the ticketed request gets a complete answer, whether its
    // queued job ran inside its maker's window or was dropped unrun.
    let answered = delivered(answer(&wired, question, 60_000, CancellationToken::new())).await?;
    assert_complete(&answered);
    let status = wired.jobs.table().status()?;
    assert_eq!((status.running, status.queued), (0, 0), "{status:?}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_scope_is_refused_with_a_typed_retry_and_nothing_is_queued() -> Result<()> {
    let wired = wired(
        JobTableConfig {
            costs: flat(100),
            max_active_per_scope: 1,
            configured_rung_only: true,
            ..JobTableConfig::default()
        },
        false,
    )
    .await?;
    let held = answer(
        &wired,
        "which policy does amber_queue own",
        120_000,
        CancellationToken::new(),
    );
    wired.gate.started_at_least(1).await;
    let refused = delivered(answer(
        &wired,
        "where is the amber_queue deadline",
        120_000,
        CancellationToken::new(),
    ))
    .await?;
    assert_eq!(refused["is_error"], true, "{refused}");
    assert_eq!(refused["error"]["code"], "answer_queue_busy", "{refused}");
    assert_eq!(refused["error"]["retryable"], true, "{refused}");
    assert_eq!(refused["error"]["query_submitted"], false, "{refused}");
    assert!(
        refused["error"]["retry_after_ms"]
            .as_u64()
            .is_some_and(|retry| retry >= 1),
        "{refused}"
    );
    let status = wired.jobs.table().status()?;
    assert_eq!((status.running, status.queued), (1, 0), "{status:?}");
    wired.gate.open.send_replace(true);
    assert_complete(&delivered(held).await?);
    Ok(())
}

/// A call at the seam with a work the test supplies; no handler and no
/// kernel.
fn seam_call(question: &str, deadline: Duration, work: AnswerWork) -> AnswerCall {
    let scope = ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 1,
        publication: airk_core::evidence::RevisionEnvelope::new(
            "fixture".into(),
            airk_core::identity::RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([6; 32]),
            },
        ),
    };
    AnswerCall {
        scope,
        request: ContextRequestV3 {
            schema: REQUEST_SCHEMA_V3.into(),
            question: question.into(),
            budget: BudgetV3 {
                deadline_ms: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
                ..BudgetV3::default()
            },
            continuation: None,
        },
        context_model: Some(SelectedModel::Arbiter),
        reply_reserve: 4096,
        deadline: Instant::now() + deadline,
        work,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_that_did_not_run_is_dropped_and_its_waiter_gets_a_job_of_its_own() -> Result<()> {
    let jobs = ServiceAnswerJobs::with_config(JobTableConfig {
        costs: flat(100),
        ..JobTableConfig::default()
    })?;
    let (release, held) = watch::channel(false);
    let runs = Arc::new(AtomicUsize::new(0));
    // The first caller's work holds its job, then reports it never ran (its
    // maker's window had ended).
    let stale: AnswerWork = {
        let runs = runs.clone();
        Arc::new(move |_| {
            let (mut held, runs) = (held.clone(), runs.clone());
            Box::pin(async move {
                runs.fetch_add(1, Ordering::SeqCst);
                let _ = held.wait_for(|released| *released).await;
                Err(AnswerFailure {
                    payload: "not run".into(),
                    retain: false,
                    not_run: true,
                })
            })
        })
    };
    // A work that answers: a valid, ticket-shaped value stands for the reply.
    let fresh_runs = Arc::new(AtomicUsize::new(0));
    let fresh: AnswerWork = {
        let fresh_runs = fresh_runs.clone();
        Arc::new(move |admission: AnswerAdmission| {
            let fresh_runs = fresh_runs.clone();
            Box::pin(async move {
                fresh_runs.fetch_add(1, Ordering::SeqCst);
                let call = seam_call(&admission.request.question, Duration::from_secs(60), {
                    Arc::new(|_| Box::pin(async { unreachable!("never run") }))
                });
                reply::pending_reply(
                    &admission.request,
                    call.scope,
                    airk_core::arbiter::answer_loop::AnswerTicket {
                        id: Sha256Digest::from_bytes([9; 32]),
                        queue_position: 1,
                        estimated_start_ms: 0,
                        estimated_finish_ms: 1,
                        retry_after_ms: 1,
                    },
                    vec![],
                    0,
                )
                .map_err(|error| AnswerFailure {
                    payload: error.to_string(),
                    retain: true,
                    not_run: false,
                })
            })
        })
    };
    let question = "which policy does amber_queue own";
    let first = {
        let jobs = jobs.clone();
        let call = seam_call(question, Duration::from_secs(60), stale);
        tokio::spawn(async move { jobs.call(call).await })
    };
    tokio::time::timeout(WAIT, jobs.table().waiting_at_least(1))
        .await
        .map_err(|_| anyhow!("the first call did not reach the table"))?;
    let second = {
        let jobs = jobs.clone();
        let call = seam_call(question, Duration::from_secs(60), fresh);
        tokio::spawn(async move { jobs.call(call).await })
    };
    // Both wait on the one job; the second attached and brought no job.
    tokio::time::timeout(WAIT, jobs.table().waiting_at_least(2))
        .await
        .map_err(|_| anyhow!("the calls did not reach the table"))?;
    let status = jobs.table().status()?;
    assert_eq!((status.running, status.queued), (1, 0), "{status:?}");
    release.send_replace(true);
    // Each waiter is submitted again once with its own work. Whichever call
    // makes the new job, the fresh work answers or the stale one is dropped
    // again; neither call is left with a kept failure.
    let outcomes = [
        tokio::time::timeout(WAIT, first).await???,
        tokio::time::timeout(WAIT, second).await???,
    ];
    for outcome in &outcomes {
        assert!(
            matches!(
                outcome,
                AnswerDelivery::Reply { .. } | AnswerDelivery::Busy { retry_after_ms: 1 }
            ),
            "a job that never ran is never delivered as a failure"
        );
    }
    assert!(
        matches!(outcomes[1], AnswerDelivery::Reply { .. })
            || fresh_runs.load(Ordering::SeqCst) == 0,
        "the caller with a live work is answered whenever its work runs"
    );
    let status = jobs.table().status()?;
    assert_eq!((status.running, status.queued), (0, 0), "{status:?}");
    // Nothing of an unrun job is kept or learned from.
    assert!(
        status.recent.len() == fresh_runs.load(Ordering::SeqCst)
            && status.retained_results == fresh_runs.load(Ordering::SeqCst),
        "{status:?}"
    );
    // A later call is admitted afresh, never attached to a failure.
    let later = tokio::time::timeout(
        WAIT,
        jobs.call(seam_call(
            "where is the amber_queue deadline",
            Duration::from_secs(60),
            Arc::new(|_| {
                Box::pin(async {
                    Err(AnswerFailure {
                        payload: "safe to repeat".into(),
                        retain: false,
                        not_run: false,
                    })
                })
            }),
        )),
    )
    .await??;
    assert!(matches!(later, AnswerDelivery::Failed(payload) if payload == "safe to repeat"));
    assert_eq!(
        jobs.table().status()?.retained_results,
        fresh_runs.load(Ordering::SeqCst)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_is_fitted_on_its_own_rung_and_only_drops_to_the_no_model_loop() -> Result<()> {
    // The Arbiter is priced past the deadline and the no-model loop well
    // inside it. The handler cannot change rung, so the plan keeps no other
    // model: it drops to the no-model loop and discloses that. (Before Large
    // was retired this also showed a Large job never moved to Compact.)
    let costly = StageCosts {
        fixed_ms: 500_000,
        per_part_ms: 0,
        per_sub_need_round_ms: 0,
    };
    let cheap = StageCosts {
        fixed_ms: 100,
        per_part_ms: 0,
        per_sub_need_round_ms: 0,
    };
    let jobs = ServiceAnswerJobs::with_config(JobTableConfig {
        costs: CostTable {
            no_model: cheap,
            compact: Some(costly),
        },
        configured_rung_only: true,
        ..JobTableConfig::default()
    })?;
    let (seen, mut admitted) = watch::channel(None);
    let work: AnswerWork = Arc::new(move |admission: AnswerAdmission| {
        seen.send_replace(Some((
            admission.context_model,
            serde_json::to_value(&admission.degradations).unwrap_or_default(),
        )));
        Box::pin(async {
            Err(AnswerFailure {
                payload: "observed".into(),
                retain: false,
                not_run: false,
            })
        })
    });
    let _ = tokio::time::timeout(
        WAIT,
        jobs.call(seam_call(
            "which policy does amber_queue own",
            Duration::from_secs(60),
            work,
        )),
    )
    .await??;
    let observed = tokio::time::timeout(WAIT, admitted.wait_for(Option::is_some))
        .await??
        .clone();
    let (context_model, degradations) = observed.ok_or_else(|| anyhow!("no admission"))?;
    assert!(!context_model);
    let models: Vec<&Value> = degradations
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["kind"] == "model")
        .collect();
    assert_eq!(models.len(), 1, "{degradations}");
    assert_eq!(models[0]["requested"], "arbiter", "{degradations}");
    assert!(models[0].get("applied").is_none(), "{degradations}");
    Ok(())
}

fn seam_scope() -> AnswerScope {
    AnswerScope::from_binding(
        &seam_call(
            "scope",
            Duration::from_secs(1),
            Arc::new(|_| Box::pin(async { unreachable!("never run") })),
        )
        .scope,
    )
}
fn asked(question: &str, deadline_ms: u64) -> ContextRequestV3 {
    seam_call(
        question,
        Duration::from_millis(deadline_ms),
        Arc::new(|_| Box::pin(async { unreachable!("never run") })),
    )
    .request
}
fn admitted_id(submitted: Submitted) -> Sha256Digest {
    match submitted {
        Submitted::Admitted(id) => id,
        other => panic!("expected an admitted job, got {other:?}"),
    }
}
fn finished_ok(text: &'static str) -> JobOutcome<&'static str> {
    JobOutcome::Done {
        value: text,
        publication: Sha256Digest::from_bytes([1; 32]),
        retained_bytes: 1,
        cut: false,
    }
}

#[test]
fn a_result_its_deadline_cut_never_answers_a_call_with_more_time() -> Result<()> {
    const SECOND: u64 = 1_000;
    // Unmeasured costs: a request that did not wait runs at its full shape
    // whatever its deadline, so the shape alone cannot tell a 60 s run from a
    // 300 s one.
    let config = JobTableConfig {
        costs_measured: false,
        configured_rung_only: true,
        ..JobTableConfig::default()
    };
    let mut table: Schedule<&'static str> = Schedule::new(config, [9; 32]);
    let scope = seam_scope();
    let short = admitted_id(table.submit(scope, asked("q", 60 * SECOND), None, None, 0)?);
    assert_eq!(table.dispatch(0)[0].run_ms, 60 * SECOND);
    // While it runs, a retry attaches to it whatever its own deadline: the
    // work is under way and is not replayed.
    assert!(matches!(
        table.submit(scope, asked("q", 300 * SECOND), None, None, SECOND)?,
        Submitted::Attached(id) if id == short
    ));
    // Its loop stops at the reserve and returns what it has, 50 s into a
    // 60 s window: the runner reports the cut.
    table.finish(
        &short,
        JobOutcome::Done {
            value: "what it had after 50 s",
            publication: Sha256Digest::from_bytes([1; 32]),
            retained_bytes: 1,
            cut: true,
        },
        50 * SECOND,
    );
    // A cut run teaches the cost correction nothing.
    let status = table.status(50 * SECOND);
    assert!(status.recent[0].cut, "{:?}", status.recent);
    assert_eq!(status.correction_permille, 1_000);
    // A call with no more time than that job had gets its result.
    assert!(matches!(
        table.submit(scope, asked("q", 60 * SECOND), None, None, 51 * SECOND)?,
        Submitted::Attached(id) if id == short
    ));
    assert!(matches!(
        table.submit(scope, asked("q", 30 * SECOND), None, None, 51 * SECOND)?,
        Submitted::Attached(id) if id == short
    ));
    // A call with more time runs on its own, and its result is then the one
    // the request's digest names.
    let long =
        admitted_id(table.submit(scope, asked("q", 300 * SECOND), None, None, 52 * SECOND)?);
    assert_ne!(long, short);
    assert_eq!(table.dispatch(52 * SECOND)[0].run_ms, 300 * SECOND);
    table.finish(&long, finished_ok("the whole answer"), 100 * SECOND);
    // An uncut result answers a call with any deadline.
    for deadline in [30, 300, 600] {
        assert!(matches!(
            table.submit(scope, asked("q", deadline * SECOND), None, None, 101 * SECOND)?,
            Submitted::Attached(id) if id == long
        ));
    }
    // The cut result's own ticket still reaches it.
    let mut kept = asked("q", 300 * SECOND);
    kept.continuation = Some(short);
    assert!(matches!(
        table.submit(scope, kept, None, None, 102 * SECOND)?,
        Submitted::Attached(id) if id == short
    ));
    Ok(())
}

#[test]
fn unmeasured_costs_never_reduce_a_request_that_did_not_wait() -> Result<()> {
    const SECOND: u64 = 1_000;
    // The Arbiter priced as the retired Large was (8 s fixed, 4 s a part, 8 s
    // a sub-need round) puts the full shape at 408 s, past a 300 s deadline.
    // Measured, that estimate costs the request a round.
    let arbiter = Some(ContextRung::Compact);
    let priced = CostTable {
        compact: Some(StageCosts {
            fixed_ms: 8_000,
            per_part_ms: 4_000,
            per_sub_need_round_ms: 8_000,
        }),
        ..CostTable::default()
    };
    let mut measured: Schedule<&'static str> = Schedule::new(
        JobTableConfig {
            costs: priced,
            ..JobTableConfig::default()
        },
        [9; 32],
    );
    measured.submit(seam_scope(), asked("alone", 300 * SECOND), arbiter, None, 0)?;
    let reduced = measured.dispatch(0);
    assert_eq!(reduced[0].shape.rounds, 2);
    assert!(!reduced[0].degradations.is_empty());

    // Unmeasured, a request with its whole window runs as asked and
    // discloses nothing.
    let config = JobTableConfig {
        costs: priced,
        costs_measured: false,
        configured_rung_only: true,
        ..JobTableConfig::default()
    };
    let mut table: Schedule<&'static str> = Schedule::new(config, [9; 32]);
    let first =
        admitted_id(table.submit(seam_scope(), asked("first", 300 * SECOND), arbiter, None, 0)?);
    admitted_id(table.submit(seam_scope(), asked("second", 300 * SECOND), arbiter, None, 0)?);
    let started = table.dispatch(0);
    assert_eq!(started.len(), 2);
    for job in &started {
        assert_eq!(
            (
                job.shape.parts,
                job.shape.sub_needs,
                job.shape.rounds,
                job.shape.model
            ),
            (4, 4, 3, arbiter)
        );
        assert!(job.degradations.is_empty(), "{:?}", job.degradations);
        assert_eq!(job.run_ms, 300 * SECOND);
    }
    // A third request, with 600 s, waits for a slot. The same costs still
    // bound the queue: it is admitted, and when its turn comes with 310 s of
    // its window left it is reduced to fit them, and says so. (The first
    // run took 290 of its estimated 408 s, so the correction is 927/1000:
    // the full shape is then 378 s, two rounds 259 s.)
    let third =
        admitted_id(table.submit(seam_scope(), asked("third", 600 * SECOND), arbiter, None, 0)?);
    table.finish(&first, finished_ok("first"), 290 * SECOND);
    let waited = table.dispatch(290 * SECOND);
    assert_eq!(waited.len(), 1);
    assert_eq!(waited[0].id, third);
    assert_eq!(waited[0].run_ms, 310 * SECOND);
    assert_eq!(
        (
            waited[0].shape.parts,
            waited[0].shape.sub_needs,
            waited[0].shape.rounds
        ),
        (4, 4, 2)
    );
    assert_eq!(
        serde_json::to_value(&waited[0].degradations)?,
        json!([{"kind":"rounds","requested":3,"applied":2,"reason":"queue_wait"}])
    );
    // The first run finished inside its window, so it taught the correction
    // although its estimate (408 s) was longer than the window (300 s).
    let status = table.status(290 * SECOND);
    assert!(!status.recent[0].cut, "{:?}", status.recent);
    assert_eq!(status.correction_permille, 927, "{status:?}");

    // Measured costs are trusted again for a request that did not wait.
    let mut trusted: Schedule<&'static str> = Schedule::new(
        JobTableConfig {
            costs_measured: false,
            ..JobTableConfig::default()
        },
        [9; 32],
    );
    trusted.set_costs(priced);
    trusted.submit(seam_scope(), asked("alone", 300 * SECOND), arbiter, None, 0)?;
    assert_eq!(trusted.dispatch(0)[0].shape.rounds, 2);
    Ok(())
}

#[test]
fn a_job_dropped_without_running_keeps_its_place_in_line_for_the_retry() -> Result<()> {
    const SECOND: u64 = 1_000;
    let config = JobTableConfig {
        costs: flat(100 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table: Schedule<&'static str> = Schedule::new(config, [9; 32]);
    let scope = seam_scope();
    let submit = |table: &mut Schedule<&'static str>, question: &str, now: u64| {
        table
            .submit(scope, asked(question, 600 * SECOND), None, None, now)
            .map(admitted_id)
    };
    let (a, b) = (submit(&mut table, "a", 0)?, submit(&mut table, "b", 0)?);
    assert_eq!(table.dispatch(0).len(), 2);
    // Three wait, in this order.
    let x = submit(&mut table, "x", SECOND)?;
    let w = submit(&mut table, "w", 6 * SECOND)?;
    let z = submit(&mut table, "z", 7 * SECOND)?;
    // x starts first and reports it never ran.
    table.finish(&a, finished_ok("a"), 10 * SECOND);
    assert_eq!(table.dispatch(10 * SECOND)[0].id, x);
    table.finish(
        &x,
        JobOutcome::Dropped {
            message: "not run".into(),
            not_run: true,
        },
        10 * SECOND,
    );
    assert!(table.view(&scope, &x, 10 * SECOND).is_none());
    assert_eq!(table.dispatch(10 * SECOND)[0].id, w);
    let status = table.status(10 * SECOND);
    assert_eq!(
        (status.retained_results, status.recent.len()),
        (1, 1),
        "{status:?}"
    );

    // Another scope cannot use x's ticket, and an unknown id is refused the
    // same way.
    let mut foreign = seam_call(
        "x",
        Duration::from_secs(600),
        Arc::new(|_| Box::pin(async { unreachable!("never run") })),
    );
    foreign.scope.authorization = Sha256Digest::from_bytes([44; 32]);
    foreign.request.continuation = Some(x);
    let refusal = |error: anyhow::Error| error.to_string();
    let foreign_refusal = table
        .submit(
            AnswerScope::from_binding(&foreign.scope),
            foreign.request,
            None,
            None,
            11 * SECOND,
        )
        .map_err(refusal)
        .expect_err("a foreign continuation is refused");
    let mut unknown = asked("x", 600 * SECOND);
    unknown.continuation = Some(Sha256Digest::from_bytes([77; 32]));
    let unknown_refusal = table
        .submit(scope, unknown, None, None, 11 * SECOND)
        .map_err(refusal)
        .expect_err("an unknown continuation is refused");
    assert_eq!(foreign_refusal, unknown_refusal);

    // The retry of x, by its ticket, is admitted afresh in x's place: it
    // goes before z, which arrived before the retry but after x.
    let mut retry = asked("x", 600 * SECOND);
    retry.continuation = Some(x);
    let again = admitted_id(table.submit(scope, retry, None, None, 11 * SECOND)?);
    assert_ne!(again, x);
    table.finish(&b, finished_ok("b"), 20 * SECOND);
    let next = table.dispatch(20 * SECOND);
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].id, again);
    assert!(next[0].request.continuation.is_none());
    assert_eq!(next[0].run_ms, 591 * SECOND);

    // The place was taken once: the old ticket is now unknown, and a later
    // request for x is ordered by its own arrival, after z.
    let mut spent = asked("x", 600 * SECOND);
    spent.continuation = Some(x);
    // (The retry is running, so the same request by digest would attach;
    // the spent ticket names no job and no place.)
    assert_eq!(
        table
            .submit(scope, spent, None, None, 21 * SECOND)
            .map_err(refusal)
            .expect_err("a spent ticket is refused"),
        unknown_refusal
    );
    table.finish(&w, finished_ok("w"), 30 * SECOND);
    assert_eq!(table.dispatch(30 * SECOND)[0].id, z);
    Ok(())
}

#[test]
fn a_place_in_line_expires_with_the_retention_time() -> Result<()> {
    const SECOND: u64 = 1_000;
    let config = JobTableConfig {
        costs: flat(SECOND),
        ..JobTableConfig::default()
    };
    let retention = config.retention_ms;
    let mut table: Schedule<&'static str> = Schedule::new(config, [9; 32]);
    let scope = seam_scope();
    let x = admitted_id(table.submit(scope, asked("x", 60 * SECOND), None, None, 0)?);
    table.dispatch(0);
    table.finish(
        &x,
        JobOutcome::Dropped {
            message: "not run".into(),
            not_run: true,
        },
        SECOND,
    );
    let mut late = asked("x", 60 * SECOND);
    late.continuation = Some(x);
    assert!(
        table
            .submit(scope, late, None, None, SECOND + retention)
            .is_err(),
        "an expired place is unknown"
    );
    // A refusal that is safe to repeat keeps no place at all.
    let later = SECOND + retention;
    let y = admitted_id(table.submit(scope, asked("y", 60 * SECOND), None, None, later)?);
    table.dispatch(later);
    table.finish(
        &y,
        JobOutcome::Dropped {
            message: "safe to repeat".into(),
            not_run: false,
        },
        later + SECOND,
    );
    let mut repeated = asked("y", 60 * SECOND);
    repeated.continuation = Some(y);
    assert!(
        table
            .submit(scope, repeated, None, None, later + 2 * SECOND)
            .is_err()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_owner_finds_only_the_table_of_its_own_service() -> Result<()> {
    let (first, second, absent) = (
        tempfile::tempdir()?,
        tempfile::tempdir()?,
        tempfile::tempdir()?,
    );
    let jobs = ServiceAnswerJobs::start()?;
    let other = ServiceAnswerJobs::start()?;
    install(first.path(), &jobs);
    install(second.path(), &other);
    assert!(for_control(first.path()).is_some());
    assert!(for_control(second.path()).is_some());
    assert!(for_control(absent.path()).is_none());
    // A service that stopped no longer offers its table.
    drop(jobs);
    assert!(for_control(first.path()).is_none());
    assert!(for_control(second.path()).is_some());
    Ok(())
}
