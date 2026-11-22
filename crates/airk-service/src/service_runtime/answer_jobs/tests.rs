// SPDX-License-Identifier: Apache-2.0
//! Job table tests. Schedule tests drive the pure state with explicit `now`
//! values; table tests run on paused Tokio time with a runner the test
//! releases through events, waiting on them under deadlines, never sleeping.
use super::estimate::{Shape, StageCosts};
use super::schedule::{EntryState, JobView, Schedule, Submitted, gathering_limit, request_digest};
use super::*;
use airk_core::{
    arbiter::{
        ScopeBinding, Stage,
        answer_loop::{BudgetV3, DegradationReason, MAX_QUEUE_POSITION, REQUEST_SCHEMA_V3},
        device::SelectedModel,
    },
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken},
    index_catalog::IndexId,
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const SECOND: u64 = 1000;
const STEP: Duration = Duration::from_secs(5);

fn binding(authorization: u8, publication: u8) -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([authorization; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: u64::from(publication),
                revision: Sha256Digest::from_bytes([publication; 32]),
            },
        ),
    }
}
fn scope(authorization: u8) -> AnswerScope {
    AnswerScope::from_binding(&binding(authorization, 1))
}
fn publication(generation: u8) -> Sha256Digest {
    AnswerScope::publication(&binding(4, generation)).unwrap()
}

fn request(question: &str, deadline_ms: u64) -> ContextRequestV3 {
    ContextRequestV3 {
        schema: REQUEST_SCHEMA_V3.into(),
        question: question.into(),
        budget: BudgetV3 {
            deadline_ms,
            ..BudgetV3::default()
        },
        continuation: None,
    }
}
fn small(question: &str, deadline_ms: u64) -> ContextRequestV3 {
    let mut request = request(question, deadline_ms);
    request.budget.max_parts = 1;
    request.budget.max_sub_needs = 1;
    request.budget.rounds = 1;
    request
}

/// A table whose every job costs exactly `ms` on the no-model rung.
fn flat(ms: u64) -> CostTable {
    CostTable {
        no_model: StageCosts {
            fixed_ms: ms,
            per_part_ms: 0,
            per_sub_need_round_ms: 0,
        },
        compact: None,
        large: None,
    }
}
fn schedule(config: JobTableConfig) -> Schedule<&'static str> {
    Schedule::new(config, [9; 32])
}
fn done(value: &'static str, bytes: usize) -> JobOutcome<&'static str> {
    JobOutcome::Done {
        value,
        publication: publication(1),
        retained_bytes: bytes,
    }
}
fn admitted(submitted: Submitted) -> Sha256Digest {
    match submitted {
        Submitted::Admitted { id, .. } => id,
        other => panic!("expected admission, got {other:?}"),
    }
}
fn check_ticket(ticket: &AnswerTicket) {
    assert!((1..=MAX_QUEUE_POSITION).contains(&ticket.queue_position));
    assert!(ticket.estimated_start_ms <= ticket.estimated_finish_ms);
    assert!(ticket.estimated_finish_ms <= airk_core::arbiter::answer_loop::MAX_ESTIMATE_MS);
    assert!(
        (1..=airk_core::arbiter::answer_loop::MAX_DEADLINE_MS).contains(&ticket.retry_after_ms)
    );
    assert!(ticket.retry_after_ms <= ticket.estimated_finish_ms);
}

#[test]
fn gathering_limit_has_a_floor_of_two_and_a_user_cap() {
    assert_eq!(gathering_limit(None, None), 2);
    assert_eq!(gathering_limit(Some(1), None), 2);
    assert_eq!(gathering_limit(Some(6), None), 6);
    assert_eq!(gathering_limit(Some(6), Some(3)), 3);
    assert_eq!(gathering_limit(Some(6), Some(1)), 2);
    assert_eq!(gathering_limit(None, Some(8)), 2);
}

#[test]
fn degrade_to_fit_discloses_each_reduction_cheapest_loss_first() {
    let costs = CostTable::default();
    let requested = Shape {
        parts: 4,
        sub_needs: 4,
        rounds: 3,
        model: Some(ContextRung::Large),
    };
    // Large at full shape: 8 + 4 x (4 + 12 x 8) = 408 s; at two rounds
    // 280 s, which fits 300 s alone.
    let fitted = estimate::degrade_to_fit(&costs, 1000, requested, 300 * SECOND, 300 * SECOND)
        .expect("fits degraded");
    assert_eq!(
        fitted.shape,
        Shape {
            rounds: 2,
            ..requested
        }
    );
    assert_eq!(fitted.degradations.len(), 1);
    // In 200 s, Compact at two rounds fits: 4 + 4 x (2 + 8 x 4) = 140 s.
    let fitted = estimate::degrade_to_fit(&costs, 1000, requested, 200 * SECOND, 200 * SECOND)
        .expect("fits degraded");
    assert_eq!(
        fitted.shape,
        Shape {
            rounds: 2,
            model: Some(ContextRung::Compact),
            ..requested
        }
    );
    assert_eq!(fitted.work_ms, 140 * SECOND);
    assert!(matches!(
        fitted.degradations.as_slice(),
        [
            Degradation::Rounds {
                requested: 3,
                applied: 2,
                reason: DegradationReason::DeadlineEstimate
            },
            Degradation::Model {
                stage: Stage::Context,
                requested: SelectedModel::ContextLarge,
                applied: Some(SelectedModel::ContextCompact),
                reason: DegradationReason::DeadlineEstimate
            }
        ]
    ));
    // The work alone fits; only a queue wait forces the cut.
    let fitted =
        estimate::degrade_to_fit(&costs, 1000, requested, 300 * SECOND, 600 * SECOND).unwrap();
    assert!(fitted.degradations.iter().all(|degradation| matches!(
        degradation,
        Degradation::Rounds {
            reason: DegradationReason::QueueWait,
            ..
        } | Degradation::Model {
            reason: DegradationReason::QueueWait,
            ..
        }
    )));
    // Nothing fits: below the smallest disclosed answer (4.5 s no-model).
    assert!(estimate::degrade_to_fit(&costs, 1000, requested, 4 * SECOND, 600 * SECOND).is_none());
    let smallest = estimate::degrade_to_fit(&costs, 1000, requested, 4_500, 600 * SECOND).unwrap();
    assert_eq!(smallest.shape, Shape::SMALLEST);
    assert_eq!(smallest.degradations.len(), 4);
    assert!(matches!(
        smallest.degradations.last(),
        Some(Degradation::Model { applied: None, .. })
    ));
}

#[test]
fn an_unavailable_rung_is_disclosed_as_model_unavailable() {
    let costs = CostTable {
        large: None,
        ..CostTable::default()
    };
    let requested = Shape {
        parts: 1,
        sub_needs: 1,
        rounds: 1,
        model: Some(ContextRung::Large),
    };
    let fitted =
        estimate::degrade_to_fit(&costs, 1000, requested, 600 * SECOND, 600 * SECOND).unwrap();
    assert_eq!(fitted.shape.model, Some(ContextRung::Compact));
    assert!(matches!(
        fitted.degradations.as_slice(),
        [Degradation::Model {
            reason: DegradationReason::ModelUnavailable,
            applied: Some(SelectedModel::ContextCompact),
            ..
        }]
    ));
}

#[test]
fn the_same_request_in_the_same_scope_attaches_and_never_across_scopes() {
    let mut table = schedule(JobTableConfig::default());
    let id = admitted(
        table
            .submit(
                scope(4),
                request("where is admission", 300_000),
                None,
                None,
                0,
            )
            .unwrap(),
    );
    // A retry with another deadline is the same request.
    assert!(matches!(
        table.submit(scope(4), request("where is admission", 120_000), None, None, 10),
        Ok(Submitted::Attached(attached)) if attached == id
    ));
    // Another caller's authority never attaches, even to the same question.
    let other = admitted(
        table
            .submit(
                scope(7),
                request("where is admission", 300_000),
                None,
                None,
                20,
            )
            .unwrap(),
    );
    assert_ne!(other, id);
    assert!(table.view(&scope(7), &id, 20).is_none());
    // A foreign continuation is refused exactly like an unknown one.
    let mut foreign = request("where is admission", 300_000);
    foreign.continuation = Some(id);
    let refused = table
        .submit(scope(7), foreign, None, None, 30)
        .unwrap_err()
        .to_string();
    let mut unknown = request("where is admission", 300_000);
    unknown.continuation = Some(Sha256Digest::from_bytes([0xee; 32]));
    let missing = table
        .submit(scope(4), unknown, None, None, 30)
        .unwrap_err()
        .to_string();
    assert_eq!(refused, missing);
    // Within the scope, a continuation must name the same request.
    let mut mismatched = request("something else", 300_000);
    mismatched.continuation = Some(id);
    assert!(table.submit(scope(4), mismatched, None, None, 30).is_err());
    let mut own = request("where is admission", 300_000);
    own.continuation = Some(id);
    assert!(matches!(
        table.submit(scope(4), own, None, None, 30),
        Ok(Submitted::Attached(attached)) if attached == id
    ));
    // Ids are salted: the same table inputs with another salt differ.
    let mut salted = Schedule::<&str>::new(JobTableConfig::default(), [8; 32]);
    assert_ne!(
        admitted(
            salted
                .submit(
                    scope(4),
                    request("where is admission", 300_000),
                    None,
                    None,
                    0
                )
                .unwrap()
        ),
        id
    );
}

#[test]
fn dispatch_takes_the_earliest_deadline_within_the_gathering_limit() {
    let config = JobTableConfig {
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let late = admitted(
        table
            .submit(scope(4), small("late", 300_000), None, None, 0)
            .unwrap(),
    );
    let early = admitted(
        table
            .submit(scope(4), small("early", 60_000), None, None, 0)
            .unwrap(),
    );
    let middle = admitted(
        table
            .submit(scope(4), small("middle", 120_000), None, None, 0)
            .unwrap(),
    );
    let started: Vec<_> = table.dispatch(0).into_iter().map(|job| job.id).collect();
    assert_eq!(started, vec![early, middle]);
    assert!(
        matches!(table.view(&scope(4), &late, 0), Some(JobView::Queued(ticket)) if ticket.queue_position == 1)
    );
    table.finish(&early, done("early", 10), 10 * SECOND);
    let started: Vec<_> = table
        .dispatch(10 * SECOND)
        .into_iter()
        .map(|job| job.id)
        .collect();
    assert_eq!(started, vec![late]);
    let status = table.status(10 * SECOND);
    assert_eq!(
        (status.running, status.queued, status.retained_results),
        (2, 0, 1)
    );
    assert!(
        status
            .entries
            .iter()
            .all(|entry| entry.state == EntryState::Running)
    );
}

/// A long-deadline Answer under a stream of short ones: ageing bounds its wait.
fn long_answer_start(ageing: u64) -> u64 {
    let config = JobTableConfig {
        costs: flat(5 * SECOND),
        ageing,
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let long = admitted(
        table
            .submit(scope(4), small("long", 600_000), None, None, 0)
            .unwrap(),
    );
    let mut running: Vec<Sha256Digest> = vec![];
    let mut sequence = 0;
    for tick in 0..200 {
        let now = tick * 5 * SECOND;
        for id in running.drain(..) {
            table.finish(&id, done("short", 1), now);
        }
        for _ in 0..2 {
            sequence += 1;
            let question = format!("short {sequence}");
            let short = small(&question, 20_000);
            let submitted = table.submit(scope(4), short, None, None, now).unwrap();
            assert!(matches!(submitted, Submitted::Admitted { .. }));
        }
        for job in table.dispatch(now) {
            if job.id == long {
                return now;
            }
            running.push(job.id);
        }
    }
    panic!("the long Answer starved");
}

#[test]
fn ageing_guards_a_long_deadline_against_starvation() {
    let aged = long_answer_start(1);
    let pure = long_answer_start(0);
    // Overtaking stops once 2t + 20 s > 600 s.
    assert!(aged <= 295 * SECOND, "aged start {aged}");
    assert!(pure > aged, "pure EDF {pure}, aged {aged}");
}

#[test]
fn a_ticket_when_the_smallest_answer_cannot_fit_after_the_wait() {
    let config = JobTableConfig {
        costs: flat(100 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let running: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|question| {
            admitted(
                table
                    .submit(scope(4), small(question, 300_000), None, None, 0)
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(table.dispatch(0).len(), 2);
    // Both slots are busy for 100 s; 150 s cannot hold a 100 s wait and 100 s.
    let Submitted::Ticketed(ticket) = table
        .submit(scope(5), small("c", 150_000), None, None, 0)
        .unwrap()
    else {
        panic!("expected a ticket");
    };
    check_ticket(&ticket);
    assert_eq!(ticket.queue_position, 1);
    assert_eq!(ticket.estimated_start_ms, 100 * SECOND);
    assert_eq!(ticket.estimated_finish_ms, 200 * SECOND);
    assert_eq!(ticket.retry_after_ms, 200 * SECOND);
    // A request whose work alone exceeds its deadline is admitted smallest
    // and disclosed, since a ticket would never help it.
    let Submitted::Admitted {
        id: hopeless,
        degradations,
    } = table
        .submit(scope(5), request("d", 50_000), None, None, 0)
        .unwrap()
    else {
        panic!("expected a disclosed admission");
    };
    assert_eq!(degradations.len(), 3);
    assert!(degradations.iter().all(|degradation| matches!(
        degradation,
        Degradation::Parts {
            reason: DegradationReason::DeadlineEstimate,
            ..
        } | Degradation::SubNeeds {
            reason: DegradationReason::DeadlineEstimate,
            ..
        } | Degradation::Rounds {
            reason: DegradationReason::DeadlineEstimate,
            ..
        }
    )));
    // The ticketed job's window opens at dispatch with its whole deadline;
    // the hopeless one, past its deadline, runs smallest on a fresh window.
    for id in &running {
        table.finish(id, done("ran", 1), 100 * SECOND);
    }
    let started = table.dispatch(100 * SECOND);
    let ticketed = started.iter().find(|job| job.id == ticket.id).unwrap();
    assert_eq!(ticketed.run_ms, 150_000);
    assert!(!ticketed.requeued);
    let late = started.iter().find(|job| job.id == hopeless).unwrap();
    assert!(late.requeued);
    assert_eq!((late.shape, late.run_ms), (Shape::SMALLEST, 50_000));
}

#[test]
fn a_wait_past_the_deadline_requeues_on_a_fresh_window() {
    let config = JobTableConfig {
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let first = admitted(
        table
            .submit(scope(4), small("a", 300_000), None, None, 0)
            .unwrap(),
    );
    let second = admitted(
        table
            .submit(scope(4), small("b", 300_000), None, None, 0)
            .unwrap(),
    );
    assert_eq!(table.dispatch(0).len(), 2);
    // Estimated to start at 10 s and finish at 20 s: admitted.
    let late = admitted(
        table
            .submit(scope(4), small("c", 30_000), None, None, 0)
            .unwrap(),
    );
    // The estimates were wrong: both slots stayed busy for 60 s.
    table.finish(&first, done("a", 1), 60 * SECOND);
    let started = table.dispatch(60 * SECOND);
    let [job] = started.as_slice() else {
        panic!("one slot freed")
    };
    assert_eq!(job.id, late);
    assert!(job.requeued);
    assert_eq!(job.run_ms, 30_000);
    table.finish(&second, done("b", 1), 61 * SECOND);
}

#[test]
fn results_are_kept_ten_minutes_and_dropped_on_a_new_publication() {
    let mut table = schedule(JobTableConfig::default());
    let id = admitted(
        table
            .submit(scope(4), small("kept", 300_000), None, None, 0)
            .unwrap(),
    );
    table.dispatch(0);
    table.finish(&id, done("kept", 100), 5 * SECOND);
    let finished_at = 5 * SECOND;
    let retry = |table: &mut Schedule<&'static str>, now, current| {
        table.submit(scope(4), small("kept", 300_000), None, current, now)
    };
    assert!(matches!(
        retry(&mut table, finished_at + DEFAULT_RETENTION - 1, Some(publication(1))),
        Ok(Submitted::Attached(attached)) if attached == id
    ));
    // On another publication the finished result is not attached.
    let fresh = admitted(retry(&mut table, finished_at + 1, Some(publication(2))).unwrap());
    assert_ne!(fresh, id);
    // At ten minutes the result is gone, and so is its continuation.
    let mut table = schedule(JobTableConfig::default());
    let id = admitted(
        table
            .submit(scope(4), small("kept", 300_000), None, None, 0)
            .unwrap(),
    );
    table.dispatch(0);
    table.finish(&id, done("kept", 100), finished_at);
    table.purge(finished_at + DEFAULT_RETENTION);
    assert!(
        table
            .view(&scope(4), &id, finished_at + DEFAULT_RETENTION)
            .is_none()
    );
    assert_eq!(
        table
            .status(finished_at + DEFAULT_RETENTION)
            .retained_results,
        0
    );
    // A new publication drops the older result at once.
    let mut table = schedule(JobTableConfig::default());
    let id = admitted(
        table
            .submit(scope(4), small("kept", 300_000), None, None, 0)
            .unwrap(),
    );
    table.dispatch(0);
    table.finish(&id, done("kept", 100), finished_at);
    table.publication_changed(scope(4).index(), publication(1));
    assert!(table.view(&scope(4), &id, finished_at).is_some());
    table.publication_changed(scope(4).index(), publication(2));
    assert!(table.view(&scope(4), &id, finished_at).is_none());
}
const DEFAULT_RETENTION: u64 = schedule::DEFAULT_RETENTION_MS;

#[test]
fn retained_bytes_are_bounded_oldest_first_and_failures_are_kept() {
    let config = JobTableConfig {
        max_retained_bytes: 250,
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let ids: Vec<_> = ["one", "two", "three"]
        .into_iter()
        .map(|question| {
            admitted(
                table
                    .submit(scope(4), small(question, 300_000), None, None, 0)
                    .unwrap(),
            )
        })
        .collect();
    let failed = admitted(
        table
            .submit(scope(4), small("four", 300_000), None, None, 0)
            .unwrap(),
    );
    table.dispatch(0);
    table.finish(&ids[0], done("one", 100), 1);
    table.finish(&ids[1], done("two", 100), 2);
    table.dispatch(2);
    table.finish(&ids[2], done("three", 100), 3);
    table.dispatch(3);
    table.finish(&failed, JobOutcome::Failed("worker gone".into()), 4);
    assert!(table.view(&scope(4), &ids[0], 4).is_none());
    assert!(matches!(
        table.view(&scope(4), &ids[2], 4),
        Some(JobView::Finished(_))
    ));
    assert!(table.status(4).retained_bytes <= 250);
    // A failed job is not replayed: the retry attaches to the failure.
    assert!(matches!(
        table.submit(scope(4), small("four", 300_000), None, None, 5),
        Ok(Submitted::Attached(attached)) if attached == failed
    ));
}

#[test]
fn one_scope_cannot_fill_the_table() {
    let config = JobTableConfig {
        max_active_per_scope: 2,
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    admitted(
        table
            .submit(scope(4), small("a", 300_000), None, None, 0)
            .unwrap(),
    );
    admitted(
        table
            .submit(scope(4), small("b", 300_000), None, None, 0)
            .unwrap(),
    );
    table.dispatch(0);
    assert!(matches!(
        table.submit(scope(4), small("c", 300_000), None, None, 0),
        Ok(Submitted::Busy {
            retry_after_ms: 10_000
        })
    ));
    admitted(
        table
            .submit(scope(5), small("c", 300_000), None, None, 0)
            .unwrap(),
    );
}

#[test]
fn status_carries_no_identity_and_learns_measured_cost() {
    let config = JobTableConfig {
        costs: flat(10 * SECOND),
        runtime_capacity: Some(1),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let ids: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|question| {
            admitted(
                table
                    .submit(scope(4), small(question, 300_000), None, None, 0)
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(table.dispatch(0).len(), 2);
    let status = table.status(0);
    assert_eq!(status.gathering_limit, 2);
    let queued: Vec<_> = status
        .entries
        .iter()
        .filter(|entry| entry.state == EntryState::Queued)
        .collect();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        (
            queued[0].position,
            queued[0].estimated_start_ms,
            queued[0].estimated_finish_ms
        ),
        (1, 10 * SECOND, 20 * SECOND)
    );
    // Status is aggregate: its debug form names no job id or question.
    let rendered = format!("{status:?}");
    assert!(ids.iter().all(|id| !rendered.contains(&id.to_string())));
    assert!(!rendered.contains("\"a\""));
    // Jobs ran twice as long as estimated: the correction rises.
    table.finish(&ids[0], done("a", 1), 20 * SECOND);
    assert!(table.status(20 * SECOND).correction_permille > 1000);
    assert_eq!(table.status(20 * SECOND).recent.len(), 1);
    table.set_runtime_capacity(Some(4));
    assert_eq!(table.status(20 * SECOND).gathering_limit, 4);
    table.set_costs(CostTable::default());
}

#[test]
fn the_request_digest_ignores_the_deadline_and_the_continuation() {
    let a = request("q", 300_000);
    let mut b = request("q", 10_000);
    b.continuation = Some(Sha256Digest::from_bytes([1; 32]));
    assert_eq!(request_digest(&a).unwrap(), request_digest(&b).unwrap());
    let mut c = request("q", 300_000);
    c.budget.rounds = 2;
    assert_ne!(request_digest(&a).unwrap(), request_digest(&c).unwrap());
}

// --- The async table on paused Tokio time. -----------------------------

type Start = (StartedJob, oneshot::Sender<Result<&'static str>>);

struct Harness {
    table: AnswerJobTable<&'static str>,
    starts: mpsc::UnboundedReceiver<Start>,
    runs: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

fn harness(config: JobTableConfig) -> Harness {
    let (sender, starts) = mpsc::unbounded_channel();
    let runs = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let runner: Runner<&'static str> = {
        let (runs, active, peak) = (runs.clone(), active.clone(), peak.clone());
        Arc::new(move |job: StartedJob| -> RunFuture<&'static str> {
            let (sender, runs, active, peak) =
                (sender.clone(), runs.clone(), active.clone(), peak.clone());
            Box::pin(async move {
                runs.fetch_add(1, Ordering::SeqCst);
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                let (release, released) = oneshot::channel();
                sender.send((job, release)).ok();
                let result = released.await.unwrap_or_else(|_| Err(anyhow!("dropped")));
                active.fetch_sub(1, Ordering::SeqCst);
                result.map(|value| Finished {
                    value,
                    publication: publication(1),
                    retained_bytes: 64,
                })
            })
        })
    };
    Harness {
        table: AnswerJobTable::with_salt(config, Arc::new(TokioClock::new()), runner, [3; 32]),
        starts,
        runs,
        peak,
    }
}

impl Harness {
    async fn next_start(&mut self) -> Start {
        tokio::time::timeout(Duration::from_secs(60), self.starts.recv())
            .await
            .expect("a job starts")
            .expect("runner alive")
    }
}

fn deadline_in(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now() + duration
}

#[tokio::test(start_paused = true)]
async fn a_retry_after_a_client_timeout_attaches_to_the_job() {
    let mut harness = harness(JobTableConfig::default());
    let table = harness.table.clone();
    // The client gives up after 5 s; the job is the service's and keeps going.
    let first = tokio::spawn({
        let table = table.clone();
        async move {
            table
                .call(
                    scope(4),
                    request("long", 300_000),
                    None,
                    None,
                    deadline_in(STEP),
                )
                .await
        }
    });
    let (job, release) = harness.next_start().await;
    let CallReply::Pending { running, ticket } = first.await.unwrap().unwrap() else {
        panic!("the client timed out first");
    };
    assert!(running);
    assert_eq!(ticket.id, job.id);
    check_ticket(&ticket);
    // A second client disconnects outright.
    let dropped = tokio::spawn({
        let table = table.clone();
        async move {
            table
                .call(
                    scope(4),
                    request("long", 300_000),
                    None,
                    None,
                    deadline_in(STEP * 100),
                )
                .await
        }
    });
    dropped.abort();
    assert!(dropped.await.unwrap_err().is_cancelled());
    // The retry attaches and receives the one run's result.
    let retry = tokio::spawn({
        let table = table.clone();
        async move {
            table
                .call(
                    scope(4),
                    request("long", 120_000),
                    None,
                    None,
                    deadline_in(STEP * 10),
                )
                .await
        }
    });
    tokio::task::yield_now().await;
    release.send(Ok("answer")).unwrap();
    let CallReply::Finished {
        id,
        attached,
        outcome,
        ..
    } = retry.await.unwrap().unwrap()
    else {
        panic!("attached to the finished job");
    };
    assert!(attached);
    assert_eq!(id, job.id);
    assert!(matches!(
        outcome.as_ref(),
        JobOutcome::Done {
            value: "answer",
            ..
        }
    ));
    // The kept ticket's continuation attaches after the finish too.
    let mut continued = request("long", 300_000);
    continued.continuation = Some(ticket.id);
    assert!(matches!(
        table
            .call(
                scope(4),
                continued,
                None,
                Some(publication(1)),
                deadline_in(STEP)
            )
            .await
            .unwrap(),
        CallReply::Finished { attached: true, .. }
    ));
    // Another caller cannot use the continuation.
    let mut foreign = request("long", 300_000);
    foreign.continuation = Some(ticket.id);
    assert!(
        table
            .call(scope(9), foreign, None, None, deadline_in(STEP))
            .await
            .is_err()
    );
    assert_eq!(harness.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn several_concurrent_answers_all_finish_within_the_limit() {
    let config = JobTableConfig {
        costs: flat(5 * SECOND),
        ..JobTableConfig::default()
    };
    let mut harness = harness(config);
    let calls: Vec<_> = (0..6u8)
        .map(|index| {
            let table = harness.table.clone();
            let deadline_ms = 60_000 + u64::from(index) * 10_000;
            tokio::spawn(async move {
                let question = format!("question {index}");
                table
                    .call(
                        scope(10 + index),
                        small(&question, deadline_ms),
                        None,
                        None,
                        deadline_in(Duration::from_millis(deadline_ms)),
                    )
                    .await
            })
        })
        .collect();
    // Two gather at once; the other four wait without any reservation.
    let first = harness.next_start().await;
    let second = harness.next_start().await;
    let status = harness.table.status().unwrap();
    assert_eq!((status.running, status.queued), (2, 4));
    let mut order = vec![first.0.request.question, second.0.request.question];
    first.1.send(Ok("done")).unwrap();
    second.1.send(Ok("done")).unwrap();
    for _ in 0..4 {
        let (job, release) = harness.next_start().await;
        assert!(job.run_ms > 0);
        order.push(job.request.question);
        release.send(Ok("done")).unwrap();
    }
    // Earliest deadline first, and every Answer ran.
    let expected: Vec<_> = (0..6).map(|index| format!("question {index}")).collect();
    assert_eq!(order, expected);
    for call in calls {
        assert!(matches!(
            call.await.unwrap().unwrap(),
            CallReply::Finished {
                attached: false,
                ..
            }
        ));
    }
    assert_eq!(harness.runs.load(Ordering::SeqCst), 6);
    assert_eq!(harness.peak.load(Ordering::SeqCst), 2);
    let status = harness.table.status().unwrap();
    assert_eq!(
        (status.running, status.queued, status.retained_results),
        (0, 0, 6)
    );
}

#[tokio::test(start_paused = true)]
async fn a_ticket_or_a_disclosed_degradation_when_the_deadline_cannot_be_met() {
    let config = JobTableConfig {
        costs: CostTable {
            no_model: StageCosts {
                fixed_ms: 50 * SECOND,
                per_part_ms: 0,
                per_sub_need_round_ms: 0,
            },
            ..CostTable::default()
        },
        ..JobTableConfig::default()
    };
    let mut harness = harness(config);
    let table = harness.table.clone();
    // Two Large Answers fill the lane; each degrades to fit 300 s
    // (Large 4 x 4 x 3 is 408 s; two rounds are 280 s).
    let mut held = vec![];
    for question in ["first", "second"] {
        let table = table.clone();
        let call = tokio::spawn(async move {
            table
                .call(
                    scope(4),
                    request(question, 300_000),
                    Some(ContextRung::Large),
                    None,
                    deadline_in(Duration::from_secs(300)),
                )
                .await
        });
        let (job, release) = harness.next_start().await;
        assert_eq!(
            (job.shape.rounds, job.shape.model),
            (2, Some(ContextRung::Large))
        );
        assert!(matches!(
            job.degradations.as_slice(),
            [Degradation::Rounds {
                requested: 3,
                applied: 2,
                reason: DegradationReason::DeadlineEstimate
            }]
        ));
        held.push((call, release));
    }
    // A 100 s request behind them cannot fit even the smallest answer: ticket.
    let reply = table
        .call(
            scope(5),
            small("third", 100_000),
            None,
            None,
            deadline_in(Duration::from_secs(100)),
        )
        .await
        .unwrap();
    let CallReply::Pending {
        running: false,
        ticket,
    } = reply
    else {
        panic!("expected a ticket, got {reply:?}");
    };
    check_ticket(&ticket);
    for (call, release) in held {
        release.send(Ok("done")).unwrap();
        let CallReply::Finished { degradations, .. } = call.await.unwrap().unwrap() else {
            panic!("the held Answers finish");
        };
        assert!(
            degradations
                .iter()
                .any(|degradation| matches!(degradation, Degradation::Rounds { .. }))
        );
    }
    // The ticketed job starts once a slot frees, with its full deadline.
    let (job, release) = harness.next_start().await;
    assert_eq!(job.id, ticket.id);
    assert_eq!(job.run_ms, 100_000);
    release.send(Err(anyhow!("worker gone"))).unwrap();
    let mut retry = small("third", 100_000);
    retry.continuation = Some(ticket.id);
    let CallReply::Finished { outcome, .. } = table
        .call(scope(5), retry, None, None, deadline_in(STEP))
        .await
        .unwrap()
    else {
        panic!("the ticket's job finished");
    };
    assert!(matches!(outcome.as_ref(), JobOutcome::Failed(message) if message == "worker gone"));
    // Raising the capacity is accepted at runtime; costs can be replaced.
    table.set_runtime_capacity(Some(3)).unwrap();
    table.set_costs(CostTable::default()).unwrap();
    table
        .publication_changed(&scope(5), publication(2))
        .unwrap();
    assert_eq!(table.status().unwrap().gathering_limit, 3);
}

#[tokio::test(start_paused = true)]
async fn a_panicking_runner_frees_its_slot() {
    fn fault() -> Finished<&'static str> {
        panic!("runner fault")
    }
    let runner: Runner<&'static str> =
        Arc::new(|_job| -> RunFuture<&'static str> { Box::pin(async { Ok(fault()) }) });
    let table = AnswerJobTable::new(
        JobTableConfig::default(),
        Arc::new(TokioClock::new()),
        runner,
    )
    .unwrap();
    let reply = table
        .call(scope(4), small("q", 60_000), None, None, deadline_in(STEP))
        .await
        .unwrap();
    assert!(matches!(
        reply,
        CallReply::Finished { outcome, .. }
            if matches!(outcome.as_ref(), JobOutcome::Failed(message) if message.starts_with("Answer job task failed"))
    ));
    assert_eq!(table.status().unwrap().running, 0);
}

#[tokio::test(start_paused = true)]
async fn a_full_scope_is_told_when_to_retry_and_makes_no_job() {
    let config = JobTableConfig {
        max_active_per_scope: 1,
        costs: flat(30 * SECOND),
        ..JobTableConfig::default()
    };
    let mut harness = harness(config);
    let table = harness.table.clone();
    let held = tokio::spawn({
        let table = table.clone();
        async move {
            table
                .call(
                    scope(4),
                    small("a", 60_000),
                    None,
                    None,
                    deadline_in(STEP * 12),
                )
                .await
        }
    });
    let (_job, release) = harness.next_start().await;
    let reply = table
        .call(scope(4), small("b", 60_000), None, None, deadline_in(STEP))
        .await
        .unwrap();
    assert!(matches!(
        reply,
        CallReply::Busy {
            retry_after_ms: 30_000
        }
    ));
    release.send(Ok("a")).unwrap();
    assert!(matches!(
        held.await.unwrap().unwrap(),
        CallReply::Finished { .. }
    ));
    assert_eq!(harness.runs.load(Ordering::SeqCst), 1);
}
