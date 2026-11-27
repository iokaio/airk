// SPDX-License-Identifier: Apache-2.0
//! Job table tests. Schedule tests drive the pure state with explicit `now`
//! values; table tests run on paused Tokio time with a runner the test
//! releases through events, waiting on them under deadlines, never sleeping.
use super::estimate::{Shape, StageCosts};
use super::schedule::{
    ENTRY_OVERHEAD_BYTES, EntryState, JobOptions, JobView, Schedule, Submitted, gathering_limit,
    publication_digest, request_digest,
};
use super::*;
use airk_core::{
    arbiter::{
        ScopeBinding,
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
const RETENTION: u64 = schedule::DEFAULT_RETENTION_MS;

fn index() -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([3; 16]))
}
fn envelope(generation: u8) -> RevisionEnvelope {
    RevisionEnvelope::new(
        "fixture".into(),
        RevisionToken {
            incarnation: Incarnation::from_bytes([5; 16]),
            generation: u64::from(generation),
            revision: Sha256Digest::from_bytes([generation; 32]),
        },
    )
}
fn binding(authorization: u8, configuration_revision: u64) -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: index(),
        authorization: Sha256Digest::from_bytes([authorization; 32]),
        configuration_revision,
        publication: envelope(1),
    }
}
fn scope(authorization: u8) -> AnswerScope {
    AnswerScope::from_binding(&binding(authorization, 1))
}
fn publication(generation: u8) -> Sha256Digest {
    publication_digest(&envelope(generation)).unwrap()
}

fn request(question: &str, deadline_ms: u64) -> ContextRequestV3 {
    ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
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
fn continued(mut request: ContextRequestV3, id: Sha256Digest) -> ContextRequestV3 {
    request.continuation = Some(id);
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
    }
}
/// The Arbiter priced as the retired Large was (8 s fixed, 4 s a part, 8 s a
/// sub-need round), so the arithmetic in these tests keeps its round numbers.
fn arbiter_costs() -> CostTable {
    CostTable {
        compact: Some(StageCosts {
            fixed_ms: 8_000,
            per_part_ms: 4_000,
            per_sub_need_round_ms: 8_000,
        }),
        ..CostTable::default()
    }
}
fn schedule(config: JobTableConfig) -> Schedule<&'static str> {
    Schedule::new(config, [9; 32])
}
fn flat_schedule(ms: u64) -> Schedule<&'static str> {
    schedule(JobTableConfig {
        costs: flat(ms),
        ..JobTableConfig::default()
    })
}
fn done_on(value: &'static str, bytes: usize, generation: u8) -> JobOutcome<&'static str> {
    JobOutcome::Done {
        value,
        publication: publication(generation),
        retained_bytes: bytes,
        cut: false,
    }
}
fn done(value: &'static str, bytes: usize) -> JobOutcome<&'static str> {
    done_on(value, bytes, 1)
}
fn admitted(submitted: Submitted) -> Sha256Digest {
    match submitted {
        Submitted::Admitted(id) => id,
        other => panic!("expected admission, got {other:?}"),
    }
}
fn submit(table: &mut Schedule<&'static str>, request: ContextRequestV3, now: u64) -> Sha256Digest {
    admitted(table.submit(scope(4), request, None, None, now).unwrap())
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
fn is_finished(view: Option<JobView<&'static str>>) -> bool {
    matches!(view, Some(JobView::Finished { .. }))
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
    let costs = arbiter_costs();
    let requested = Shape {
        parts: 4,
        sub_needs: 4,
        rounds: 3,
        model: Some(ContextRung::Compact),
    };
    // Arbiter at full shape: 8 + 4 x (4 + 12 x 8) = 408 s; at two rounds
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
    // In 200 s, one round fits: 8 + 4 x (4 + 4 x 8) = 152 s. With one model
    // there is no model step; the model stays.
    let fitted = estimate::degrade_to_fit(&costs, 1000, requested, 200 * SECOND, 200 * SECOND)
        .expect("fits degraded");
    assert_eq!(
        fitted.shape,
        Shape {
            rounds: 1,
            ..requested
        }
    );
    assert_eq!(fitted.work_ms, 152 * SECOND);
    assert!(matches!(
        fitted.degradations.as_slice(),
        [Degradation::Rounds {
            requested: 3,
            applied: 1,
            reason: DegradationReason::DeadlineEstimate
        }]
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
        compact: None,
        ..CostTable::default()
    };
    let requested = Shape {
        parts: 1,
        sub_needs: 1,
        rounds: 1,
        model: Some(ContextRung::Compact),
    };
    let fitted =
        estimate::degrade_to_fit(&costs, 1000, requested, 600 * SECOND, 600 * SECOND).unwrap();
    assert_eq!(fitted.shape.model, None);
    assert!(matches!(
        fitted.degradations.as_slice(),
        [Degradation::Model {
            reason: DegradationReason::ModelUnavailable,
            requested: SelectedModel::Arbiter,
            applied: None,
            ..
        }]
    ));
}

#[test]
fn the_same_request_in_the_same_scope_attaches_and_never_across_scopes() {
    let mut table = schedule(JobTableConfig::default());
    let id = submit(&mut table, request("where is admission", 300_000), 0);
    // A retry with the same deadline is the same request and loses nothing
    // (a call with another deadline runs on its own: CONTRACT "Reuse").
    assert!(matches!(
        table.submit(scope(4), request("where is admission", 300_000), None, None, 10),
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
    // Review S2: a new configuration revision is another scope.
    let reconfigured = AnswerScope::from_binding(&binding(4, 2));
    assert_ne!(reconfigured, scope(4));
    assert!(table.view(&reconfigured, &id, 20).is_none());
    // A foreign continuation is refused exactly like an unknown one.
    let refused = table
        .submit(
            scope(7),
            continued(request("where is admission", 300_000), id),
            None,
            None,
            30,
        )
        .unwrap_err()
        .to_string();
    let missing = table
        .submit(
            scope(4),
            continued(
                request("where is admission", 300_000),
                Sha256Digest::from_bytes([0xee; 32]),
            ),
            None,
            None,
            30,
        )
        .unwrap_err()
        .to_string();
    assert_eq!(refused, missing);
    // Within the scope, a continuation must name the same request.
    assert!(
        table
            .submit(
                scope(4),
                continued(request("something else", 300_000), id),
                None,
                None,
                30
            )
            .is_err()
    );
    assert!(matches!(
        table.submit(scope(4), continued(request("where is admission", 300_000), id), None, None, 30),
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

/// The reviewer's note: a retry that would get more on its own does not
/// receive a shorter run's more degraded answer; it runs separately.
#[test]
fn a_retry_wanting_more_runs_separately_and_one_wanting_no_more_attaches() {
    let mut table = schedule(JobTableConfig::default());
    let model = Some(ContextRung::Compact);
    let short = admitted(
        table
            .submit(scope(4), request("q", 200_000), model, None, 0)
            .unwrap(),
    );
    let [started] = <[StartedJob; 1]>::try_from(table.dispatch(0)).unwrap();
    // The Arbiter at full shape is 4 + 4 x (2 + 12 x 4) = 204 s; two rounds
    // (140 s) fit 200 s.
    assert_eq!(started.shape.rounds, 2);
    // 600 s would admit the full shape: a separate job.
    let long = admitted(
        table
            .submit(scope(4), request("q", 600_000), model, None, 1)
            .unwrap(),
    );
    assert_ne!(long, short);
    // The latest job now answers the digest; a call wanting no more than
    // what it applies attaches to it.
    assert!(matches!(
        table.submit(scope(4), request("q", 600_000), model, None, 1),
        Ok(Submitted::Attached(attached)) if attached == long
    ));
    // The short job's own continuation still reaches it, with the
    // disclosures fitted at its dispatch for every caller.
    table.finish(&short, done("short", 10), 50 * SECOND);
    let Some(JobView::Finished { degradations, .. }) = table.view(&scope(4), &short, 50 * SECOND)
    else {
        panic!("the short job is retained");
    };
    assert_eq!(degradations.len(), started.degradations.len());
    assert!(!degradations.is_empty());
}

#[test]
fn dispatch_takes_the_earliest_deadline_within_the_gathering_limit() {
    let mut table = flat_schedule(10 * SECOND);
    let late = submit(&mut table, small("late", 300_000), 0);
    let early = submit(&mut table, small("early", 60_000), 0);
    let middle = submit(&mut table, small("middle", 120_000), 0);
    let started: Vec<_> = table.dispatch(0).into_iter().map(|job| job.id).collect();
    assert_eq!(started, vec![early, middle]);
    assert!(
        matches!(table.view(&scope(4), &late, 0), Some(JobView::Queued(ticket, _)) if ticket.queue_position == 1)
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
    let long = submit(&mut table, small("long", 600_000), 0);
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
            submit(&mut table, small(&question, 20_000), now);
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
    let mut table = flat_schedule(100 * SECOND);
    let running: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|question| submit(&mut table, small(question, 300_000), 0))
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
    let hopeless = admitted(
        table
            .submit(scope(5), request("d", 50_000), None, None, 0)
            .unwrap(),
    );
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
    assert_eq!(late.degradations.len(), 3);
    assert!(late.degradations.iter().all(|degradation| matches!(
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
}

/// Review C4: a smallest-disclosed job dispatched at once never waited.
#[test]
fn a_job_that_never_waited_is_not_requeued() {
    let mut table = flat_schedule(100 * SECOND);
    let id = submit(&mut table, request("d", 50_000), 0);
    let [job] = <[StartedJob; 1]>::try_from(table.dispatch(0)).unwrap();
    assert_eq!(job.id, id);
    assert!(!job.requeued);
    assert_eq!((job.shape, job.run_ms), (Shape::SMALLEST, 50_000));
}

#[test]
fn a_wait_past_the_deadline_requeues_on_a_fresh_window() {
    let mut table = flat_schedule(10 * SECOND);
    let first = submit(&mut table, small("a", 300_000), 0);
    let second = submit(&mut table, small("b", 300_000), 0);
    assert_eq!(table.dispatch(0).len(), 2);
    // Estimated to start at 10 s and finish at 20 s: admitted.
    let late = submit(&mut table, small("c", 30_000), 0);
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
fn results_are_kept_ten_minutes_then_dropped() {
    let finished_at = 5 * SECOND;
    let mut table = schedule(JobTableConfig::default());
    let id = submit(&mut table, small("kept", 300_000), 0);
    table.dispatch(0);
    table.finish(&id, done("kept", 100), finished_at);
    assert!(matches!(
        table.submit(scope(4), small("kept", 300_000), None, Some(publication(1)), finished_at + RETENTION - 1),
        Ok(Submitted::Attached(attached)) if attached == id
    ));
    // On another publication the finished result is not attached.
    let fresh = admitted(
        table
            .submit(
                scope(4),
                small("kept", 300_000),
                None,
                Some(publication(2)),
                finished_at + 1,
            )
            .unwrap(),
    );
    assert_ne!(fresh, id);
    table.purge(finished_at + RETENTION);
    assert!(
        table
            .view(&scope(4), &id, finished_at + RETENTION)
            .is_none()
    );
}

/// An invocation key recovers its own job only on the publication that job
/// ran on: after a new publication the same key and request run again, as a
/// call by digest or by continuation would (plan 12.4.2, "invalidated by a
/// new publication").
#[test]
fn an_invocation_key_never_recovers_a_result_of_an_older_publication() {
    let mut table = schedule(JobTableConfig::default());
    let keyed = JobOptions {
        key: Some("turn-1".into()),
        ..JobOptions::default()
    };
    let submit_keyed = |table: &mut Schedule<&'static str>, generation: u8, now: u64| {
        table
            .submit_with(
                scope(4),
                small("kept", 300_000),
                None,
                Some(publication(generation)),
                now,
                &keyed,
            )
            .unwrap()
    };
    let id = admitted(submit_keyed(&mut table, 1, 0));
    table.dispatch(0);
    table.finish(&id, done_on("kept", 100, 1), 5 * SECOND);
    // Same publication: the key recovers the job.
    assert!(matches!(
        submit_keyed(&mut table, 1, 6 * SECOND),
        Submitted::Attached(attached) if attached == id
    ));
    // A newer publication: the key runs the request again.
    let again = admitted(submit_keyed(&mut table, 2, 7 * SECOND));
    assert_ne!(again, id);
    // And from then on the key names the new job.
    assert!(matches!(
        submit_keyed(&mut table, 2, 8 * SECOND),
        Submitted::Attached(attached) if attached == again
    ));
}

/// Review C1 and S4: results, failures and running jobs of an older
/// publication are never attached once a newer one is observed, and an
/// out-of-order older notification changes nothing.
#[test]
fn a_new_publication_supersedes_every_older_result() {
    let mut table = flat_schedule(10 * SECOND);
    let finished = submit(&mut table, small("finished", 300_000), 0);
    let failed = submit(&mut table, small("failed", 300_000), 0);
    table.dispatch(0);
    table.finish(&finished, done("finished", 10), SECOND);
    table.finish(
        &failed,
        JobOutcome::Failed("index mid-publication".into()),
        SECOND,
    );
    // Running on publication 1, as the caller knew it.
    let running = admitted(
        table
            .submit(
                scope(4),
                small("running", 300_000),
                None,
                Some(publication(1)),
                SECOND,
            )
            .unwrap(),
    );
    let [started] = <[StartedJob; 1]>::try_from(table.dispatch(SECOND)).unwrap();
    assert_eq!(started.publication, Some(publication(1)));
    // A caller already on publication 2 does not attach to it.
    let separate = admitted(
        table
            .submit(
                scope(4),
                small("running", 300_000),
                None,
                Some(publication(2)),
                SECOND,
            )
            .unwrap(),
    );
    assert_ne!(separate, running);

    assert!(table.publication_changed(index(), &envelope(2)).unwrap());
    assert!(table.view(&scope(4), &finished, 2 * SECOND).is_none());
    assert!(table.view(&scope(4), &failed, 2 * SECOND).is_none());
    // The running job is superseded: its continuation is refused, and a
    // retry without a known publication makes a new job.
    let superseded = table
        .submit(
            scope(4),
            continued(small("running", 300_000), running),
            None,
            None,
            2 * SECOND,
        )
        .unwrap_err()
        .to_string();
    assert!(superseded.contains("superseded"), "{superseded}");
    // It finishes later on publication 1 and is not kept.
    table.finish(&running, done_on("running", 10, 1), 3 * SECOND);
    assert!(table.view(&scope(4), &running, 3 * SECOND).is_none());
    // A result that ran on publication 2 is kept and attached.
    let current = submit(&mut table, small("current", 300_000), 3 * SECOND);
    table.dispatch(3 * SECOND);
    table.finish(&current, done_on("current", 10, 2), 4 * SECOND);
    // An older notification arriving late is ignored.
    assert!(!table.publication_changed(index(), &envelope(1)).unwrap());
    assert!(is_finished(table.view(&scope(4), &current, 4 * SECOND)));
    assert!(matches!(
        table.submit(scope(4), small("current", 300_000), None, None, 5 * SECOND),
        Ok(Submitted::Attached(attached)) if attached == current
    ));
    // A result finishing on an older publication than the observed one is
    // delivered but not kept.
    let lagging = submit(&mut table, small("lagging", 300_000), 5 * SECOND);
    table.dispatch(5 * SECOND);
    table.finish(&lagging, done_on("lagging", 10, 1), 6 * SECOND);
    assert!(table.view(&scope(4), &lagging, 6 * SECOND).is_none());
}

/// Review S1: every entry is charged its question and an overhead, so
/// zero-byte results and failures are evicted too, and the count is capped.
#[test]
fn retention_charges_every_entry_and_caps_the_count() {
    let entry = ENTRY_OVERHEAD_BYTES + 3;
    let config = JobTableConfig {
        max_retained_bytes: 2 * entry + 10,
        costs: flat(SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let ids: Vec<_> = ["one", "two", "six"]
        .into_iter()
        .map(|question| submit(&mut table, small(question, 300_000), 0))
        .collect();
    table.dispatch(0);
    table.finish(&ids[0], done("one", 0), 1);
    table.finish(&ids[1], JobOutcome::Failed(String::new()), 2);
    table.dispatch(2);
    table.finish(&ids[2], done("six", 0), 3);
    assert!(table.view(&scope(4), &ids[0], 3).is_none());
    assert!(is_finished(table.view(&scope(4), &ids[1], 3)));
    assert_eq!(table.status(3).retained_bytes, 2 * entry);
    // A failure belongs to its own invocation (review M5): a call by digest
    // runs a new job, and the failed job's own continuation still reaches it.
    assert!(matches!(
        table.submit(scope(4), small("two", 300_000), None, None, 5),
        Ok(Submitted::Admitted(id)) if id != ids[1]
    ));
    let mut own = small("two", 300_000);
    own.continuation = Some(ids[1]);
    assert!(matches!(
        table.submit(scope(4), own, None, None, 5),
        Ok(Submitted::Attached(attached)) if attached == ids[1]
    ));

    let config = JobTableConfig {
        max_retained_results: 1,
        costs: flat(SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let first = submit(&mut table, small("first", 300_000), 0);
    let second = submit(&mut table, small("second", 300_000), 0);
    table.dispatch(0);
    table.finish(&first, done("first", 0), 1);
    table.finish(&second, done("second", 0), 2);
    assert!(table.view(&scope(4), &first, 2).is_none());
    assert_eq!(table.status(2).retained_results, 1);
}

#[test]
fn one_scope_or_one_principal_cannot_fill_the_table() {
    let config = JobTableConfig {
        max_active_per_scope: 2,
        max_active_per_principal: 3,
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    submit(&mut table, small("a", 300_000), 0);
    submit(&mut table, small("b", 300_000), 0);
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
    // Review S3: the same principal in another scope (a new configuration
    // revision here) shares its principal's cap.
    let same_principal = AnswerScope::from_binding(&binding(4, 2));
    admitted(
        table
            .submit(same_principal, small("d", 300_000), None, None, 0)
            .unwrap(),
    );
    let third = AnswerScope::from_binding(&binding(4, 3));
    assert!(matches!(
        table.submit(third, small("e", 300_000), None, None, 0),
        Ok(Submitted::Busy { .. })
    ));
}

#[test]
fn status_carries_no_identity_and_learns_only_from_uncut_runs() {
    let config = JobTableConfig {
        costs: flat(10 * SECOND),
        runtime_capacity: Some(1),
        ..JobTableConfig::default()
    };
    let mut table = schedule(config);
    let ids: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|question| submit(&mut table, small(question, 300_000), 0))
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
    let learned = table.status(20 * SECOND).correction_permille;
    assert!(learned > 1000);
    // Review C5: a run that reached its deadline teaches nothing.
    table.finish(&ids[1], done("b", 1), 300 * SECOND);
    let status = table.status(300 * SECOND);
    assert_eq!(status.correction_permille, learned);
    assert_eq!(status.recent.len(), 2);
    assert!(status.recent[1].cut && !status.recent[0].cut);
    table.set_runtime_capacity(Some(4));
    assert_eq!(table.status(300 * SECOND).gathering_limit, 4);
    table.set_costs(CostTable::default());
}

#[test]
fn the_request_digest_ignores_the_deadline_and_the_continuation() {
    let a = request("q", 300_000);
    let b = continued(request("q", 10_000), Sha256Digest::from_bytes([1; 32]));
    assert_eq!(request_digest(&a).unwrap(), request_digest(&b).unwrap());
    let mut c = request("q", 300_000);
    c.budget.rounds = 2;
    assert_ne!(request_digest(&a).unwrap(), request_digest(&c).unwrap());
}

/// Found on 26 November 2026: the digest left the caller inputs out, so a
/// call with needs was answered from the stored job of the bare question.
#[test]
fn the_request_digest_covers_caller_inputs_and_is_unchanged_without_them() {
    let bare = request("q", 300_000);
    let mut budget = bare.budget.clone();
    budget.deadline_ms = 0;
    let before = airk_core::identity::framed_digest(
        b"airk.answer.job.request/1",
        &[
            bare.schema.as_bytes(),
            b"q",
            &serde_json::to_vec(&budget).unwrap(),
        ],
    );
    assert_eq!(request_digest(&bare).unwrap(), before);
    let mut needs = request("q", 300_000);
    needs.needs = vec!["where the limit is defined".into()];
    let mut anchors = request("q", 300_000);
    anchors.anchors = vec!["MAX_TOKENS".into()];
    let mut both = needs.clone();
    both.anchors = anchors.anchors.clone();
    let digests = [&bare, &needs, &anchors, &both].map(|item| request_digest(item).unwrap());
    for (index, left) in digests.iter().enumerate() {
        for right in &digests[index + 1..] {
            assert_ne!(left, right);
        }
    }
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
                    cut: false,
                })
            })
        })
    };
    Harness {
        table: AnswerJobTable::with_salt(config, Arc::new(TokioClock::new()), runner, [3; 32])
            .unwrap(),
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
    async fn waiting(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(60), self.table.waiting_at_least(count))
            .await
            .expect("the call is waiting");
    }
}

fn deadline_in(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now() + duration
}

/// Review T2: the disconnecting call and the retry are both observed
/// waiting on the running job before it is released.
#[tokio::test(start_paused = true)]
async fn a_retry_after_a_client_timeout_attaches_to_the_job() {
    let mut harness = harness(JobTableConfig::default());
    let table = harness.table.clone();
    let call = |deadline_ms, wait| {
        let table = table.clone();
        tokio::spawn(async move {
            table
                .call(
                    scope(4),
                    request("long", deadline_ms),
                    None,
                    None,
                    deadline_in(wait),
                )
                .await
        })
    };
    // The client gives up after 5 s; the job is the service's and keeps going.
    let first = call(300_000, STEP);
    let (job, release) = harness.next_start().await;
    let CallReply::Pending {
        running,
        ticket,
        degradations,
        ..
    } = first.await.unwrap().unwrap()
    else {
        panic!("the client timed out first");
    };
    assert!(running);
    // No-model 4 x 4 x 3 is 100 s, inside 300 s: nothing was reduced.
    assert!(degradations.is_empty());
    assert_eq!(ticket.id, job.id);
    check_ticket(&ticket);
    // A second client attaches, then disconnects outright.
    let dropped = call(300_000, STEP * 100);
    harness.waiting(2).await;
    dropped.abort();
    assert!(dropped.await.unwrap_err().is_cancelled());
    // The retry (the same arguments, so the same deadline) attaches to the
    // running job and is notified when it ends.
    let retry = call(300_000, STEP * 10);
    harness.waiting(3).await;
    assert_eq!(harness.table.status().unwrap().running, 1);
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
    assert!(matches!(
        table
            .call(
                scope(4),
                continued(request("long", 300_000), ticket.id),
                None,
                Some(publication(1)),
                deadline_in(STEP)
            )
            .await
            .unwrap(),
        CallReply::Finished { attached: true, .. }
    ));
    // Another caller cannot use the continuation.
    assert!(
        table
            .call(
                scope(9),
                continued(request("long", 300_000), ticket.id),
                None,
                None,
                deadline_in(STEP)
            )
            .await
            .is_err()
    );
    assert_eq!(harness.runs.load(Ordering::SeqCst), 1);
}

/// Review T1: later arrivals have earlier deadlines, so the order shows
/// earliest-deadline dispatch rather than arrival order.
#[tokio::test(start_paused = true)]
async fn several_concurrent_answers_run_earliest_deadline_first_within_the_limit() {
    let mut harness = harness(JobTableConfig {
        costs: flat(5 * SECOND),
        ..JobTableConfig::default()
    });
    let calls: Vec<_> = (0..6u8)
        .map(|index| {
            let table = harness.table.clone();
            let deadline_ms = 110_000 - u64::from(index) * 10_000;
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
    let expected: Vec<_> = [0, 1, 5, 4, 3, 2]
        .into_iter()
        .map(|index| format!("question {index}"))
        .collect();
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
            ..arbiter_costs()
        },
        ..JobTableConfig::default()
    };
    let mut harness = harness(config);
    let table = harness.table.clone();
    // Two Arbiter Answers fill the lane; each degrades to fit 300 s
    // (4 x 4 x 3 is 408 s; two rounds are 280 s).
    let mut held = vec![];
    for question in ["first", "second"] {
        let table = table.clone();
        let call = tokio::spawn(async move {
            table
                .call(
                    scope(4),
                    request(question, 300_000),
                    Some(ContextRung::Compact),
                    None,
                    deadline_in(Duration::from_secs(300)),
                )
                .await
        });
        let (job, release) = harness.next_start().await;
        assert_eq!(
            (job.shape.rounds, job.shape.model),
            (2, Some(ContextRung::Compact))
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
        ..
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
    let CallReply::Finished { outcome, .. } = table
        .call(
            scope(5),
            continued(small("third", 100_000), ticket.id),
            None,
            None,
            deadline_in(STEP),
        )
        .await
        .unwrap()
    else {
        panic!("the ticket's job finished");
    };
    assert!(matches!(outcome.as_ref(), JobOutcome::Failed(message) if message == "worker gone"));
    table.set_costs(CostTable::default()).unwrap();
}

/// Review C1 through the table: a waiter still receives the outcome of a
/// job superseded while it ran, which the table then does not keep.
#[tokio::test(start_paused = true)]
async fn a_waiter_receives_a_superseded_outcome_that_is_not_kept() {
    let mut harness = harness(JobTableConfig::default());
    let table = harness.table.clone();
    let call = tokio::spawn({
        let table = table.clone();
        async move {
            table
                .call(
                    scope(4),
                    small("q", 300_000),
                    None,
                    Some(publication(1)),
                    deadline_in(STEP * 100),
                )
                .await
        }
    });
    let (_job, release) = harness.next_start().await;
    harness.waiting(1).await;
    assert!(table.publication_changed(index(), &envelope(2)).unwrap());
    release.send(Ok("old")).unwrap();
    assert!(matches!(
        call.await.unwrap().unwrap(),
        CallReply::Finished { outcome, .. }
            if matches!(outcome.as_ref(), JobOutcome::Done { value: "old", .. })
    ));
    assert_eq!(table.status().unwrap().retained_results, 0);
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

/// Review T3 and C2: a runner that panics before returning its future still
/// finishes its job as failed and frees its slot.
#[tokio::test(start_paused = true)]
async fn a_synchronous_runner_panic_frees_its_slot() {
    let started = Arc::new(AtomicUsize::new(0));
    let runner: Runner<&'static str> = {
        let started = started.clone();
        Arc::new(move |_job| -> RunFuture<&'static str> {
            started.fetch_add(1, Ordering::SeqCst);
            panic!("runner fault before its future")
        })
    };
    let config = JobTableConfig {
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    };
    let table =
        AnswerJobTable::with_salt(config, Arc::new(TokioClock::new()), runner, [3; 32]).unwrap();
    for question in ["a", "b", "c"] {
        let reply = table
            .call(
                scope(4),
                small(question, 60_000),
                None,
                None,
                deadline_in(STEP),
            )
            .await
            .unwrap();
        assert!(matches!(
            reply,
            CallReply::Finished { outcome, .. }
                if matches!(outcome.as_ref(), JobOutcome::Failed(message) if message.starts_with("Answer job task failed"))
        ));
    }
    assert_eq!(table.status().unwrap().running, 0);
    assert_eq!(started.load(Ordering::SeqCst), 3);
}

/// Review C2: a capacity observer on a plain thread, outside the runtime,
/// starts a queued job on the table's kept handle.
#[tokio::test(start_paused = true)]
async fn a_dispatch_from_a_foreign_thread_starts_the_job() {
    let mut harness = harness(JobTableConfig {
        costs: flat(10 * SECOND),
        ..JobTableConfig::default()
    });
    let table = harness.table.clone();
    let calls: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|question| {
            let table = table.clone();
            tokio::spawn(async move {
                table
                    .call(
                        scope(4),
                        small(question, 300_000),
                        None,
                        None,
                        deadline_in(STEP * 50),
                    )
                    .await
            })
        })
        .collect();
    let mut held = vec![harness.next_start().await, harness.next_start().await];
    harness.waiting(3).await;
    assert_eq!(table.status().unwrap().queued, 1);
    let observer = std::thread::spawn({
        let table = table.clone();
        move || table.set_runtime_capacity(Some(3))
    });
    observer.join().unwrap().unwrap();
    held.push(harness.next_start().await);
    for (_, release) in held {
        release.send(Ok("done")).unwrap();
    }
    for call in calls {
        assert!(matches!(
            call.await.unwrap().unwrap(),
            CallReply::Finished { .. }
        ));
    }
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

// --- Replies the table produces, and delivery (wiring preparation). ---

fn ticket_for(id: Sha256Digest, running: bool) -> AnswerTicket {
    AnswerTicket {
        id,
        queue_position: if running { 1 } else { 3 },
        estimated_start_ms: if running { 0 } else { 40_000 },
        estimated_finish_ms: 90_000,
        retry_after_ms: 90_000,
    }
}

#[test]
fn a_queued_or_running_job_has_a_valid_ticket_shaped_reply() {
    let request = request("where is admission", 300_000);
    let degradations = vec![Degradation::Rounds {
        requested: 3,
        applied: 2,
        reason: DegradationReason::QueueWait,
    }];
    for (running, degradations) in [(false, vec![]), (true, degradations)] {
        let reply = reply::pending_reply(
            &request,
            binding(4, 1),
            ticket_for(Sha256Digest::from_bytes([7; 32]), running),
            degradations.clone(),
            12,
        )
        .unwrap();
        // Validated on build; serialized, it still parses and validates.
        let bytes = serde_json::to_vec(&reply).unwrap();
        let parsed: airk_core::arbiter::answer_loop::ContextResponseV3 =
            serde_json::from_slice(&bytes).unwrap();
        parsed.validate(&request.budget).unwrap();
        assert!(parsed.parts.is_empty() && parsed.records.is_empty());
        assert_eq!(parsed.degradations.len(), degradations.len());
        assert_eq!(parsed.ticket.unwrap().id, Sha256Digest::from_bytes([7; 32]));
    }
    // A ticket outside the Core bounds is refused, never delivered.
    let mut bad = ticket_for(Sha256Digest::from_bytes([7; 32]), true);
    bad.retry_after_ms = 0;
    assert!(reply::pending_reply(&request, binding(4, 1), bad, vec![], 1).is_err());
}

#[test]
fn a_finished_reply_is_retained_on_its_own_publication_and_size() {
    let request = request("q", 300_000);
    let response = reply::pending_reply(
        &request,
        binding(4, 1),
        ticket_for(Sha256Digest::from_bytes([7; 32]), true),
        vec![],
        1,
    )
    .unwrap();
    let size = serde_json::to_vec(&response).unwrap().len();
    let finished = reply::finished(response).unwrap();
    assert_eq!(finished.publication, publication(1));
    assert_eq!(finished.retained_bytes, size);
}

/// Through the real table: a call that times out is delivered a ticket
/// reply naming its job, the retry by continuation is delivered the
/// finished reply, a reply naming another authority is refused, and Busy
/// and failures map through unchanged.
#[tokio::test(start_paused = true)]
async fn delivery_maps_every_table_reply_and_checks_the_scope() {
    let request = request("delivered", 300_000);
    let answer = reply::pending_reply(
        &request,
        binding(4, 1),
        ticket_for(Sha256Digest::from_bytes([9; 32]), true),
        vec![],
        1,
    )
    .unwrap();
    let (release, released) = oneshot::channel::<()>();
    let released = Arc::new(std::sync::Mutex::new(Some(released)));
    let runner: Runner<airk_core::arbiter::answer_loop::ContextResponseV3> = {
        let answer = answer.clone();
        Arc::new(
            move |_job| -> RunFuture<airk_core::arbiter::answer_loop::ContextResponseV3> {
                let answer = answer.clone();
                let released = released.lock().unwrap().take();
                Box::pin(async move {
                    if let Some(released) = released {
                        released.await.ok();
                    }
                    reply::finished(answer)
                })
            },
        )
    };
    let table = AnswerJobTable::with_salt(
        JobTableConfig::default(),
        Arc::new(TokioClock::new()),
        runner,
        [5; 32],
    )
    .unwrap();
    let pending = table
        .call(scope(4), request.clone(), None, None, deadline_in(STEP))
        .await
        .unwrap();
    let reply::Delivered::Reply { response, record } =
        reply::deliver(pending, &request, &binding(4, 1), 5_000).unwrap()
    else {
        panic!("a timed-out call is delivered a reply");
    };
    assert!(!record.reused);
    assert_eq!((record.job, record.wait_ms), (response.ticket.as_ref().unwrap().id, 5_000));
    let ticket = response.ticket.clone().expect("a ticket-shaped reply");
    response.validate(&request.budget).unwrap();
    release.send(()).unwrap();
    let finished = table
        .call(
            scope(4),
            continued(request.clone(), ticket.id),
            None,
            None,
            deadline_in(STEP),
        )
        .await
        .unwrap();
    // The finished reply ran under authority 4; a caller bound to another
    // authority is refused even if a table reply reached it.
    let other = reply::deliver(
        table
            .call(
                scope(4),
                continued(request.clone(), ticket.id),
                None,
                None,
                deadline_in(STEP),
            )
            .await
            .unwrap(),
        &request,
        &binding(5, 1),
        1,
    );
    assert!(other.is_err());
    let reply::Delivered::Reply { response, record } =
        reply::deliver(finished, &request, &binding(4, 1), 1).unwrap()
    else {
        panic!("the finished reply is delivered");
    };
    assert!(record.reused);
    // Rendered for this call: its own wait is the reply's elapsed time.
    let mut expected = answer.clone();
    expected.elapsed_ms = 1;
    assert_eq!(
        serde_json::to_value(&*response).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert!(matches!(
        reply::deliver(
            CallReply::Busy { retry_after_ms: 7 },
            &request,
            &binding(4, 1),
            1
        )
        .unwrap(),
        reply::Delivered::Busy { retry_after_ms: 7 }
    ));
    assert!(matches!(
        reply::deliver(
            CallReply::Finished {
                id: ticket.id,
                attached: true,
                degradations: vec![],
                outcome: Arc::new(JobOutcome::Failed("worker gone".into())),
                timing: Default::default(),
            },
            &request,
            &binding(4, 1),
            1
        )
        .unwrap(),
        reply::Delivered::Failed(message) if message == "worker gone"
    ));
}

/// Each caller's own work runs its job; an attaching call's work is dropped
/// unused, so the work runs once whatever the number of callers.
#[tokio::test(start_paused = true)]
async fn the_work_of_the_call_that_made_the_job_runs_it_once() {
    fn fail() -> Finished<&'static str> {
        panic!("the table runner must not run a job that has its own work")
    }
    let runner: Runner<&'static str> =
        Arc::new(|_job| -> RunFuture<&'static str> { Box::pin(async { Ok(fail()) }) });
    let table = AnswerJobTable::with_salt(
        JobTableConfig::default(),
        Arc::new(TokioClock::new()),
        runner,
        [6; 32],
    )
    .unwrap();
    let ran = Arc::new(AtomicUsize::new(0));
    let (release, released) = oneshot::channel::<()>();
    let work =
        |label: &'static str, gate: Option<oneshot::Receiver<()>>| -> JobWork<&'static str> {
            let ran = ran.clone();
            Box::new(move |job: StartedJob| -> RunFuture<&'static str> {
                Box::pin(async move {
                    ran.fetch_add(1, Ordering::SeqCst);
                    if let Some(gate) = gate {
                        gate.await.ok();
                    }
                    assert!(job.run_ms > 0);
                    Ok(Finished {
                        value: label,
                        publication: publication(1),
                        retained_bytes: 1,
                        cut: false,
                    })
                })
            })
        };
    let first = tokio::spawn({
        let table = table.clone();
        let work = work("first", Some(released));
        async move {
            table
                .call_with_work(
                    scope(4),
                    small("q", 60_000),
                    None,
                    None,
                    deadline_in(STEP * 10),
                    Some(work),
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(60), table.waiting_at_least(1))
        .await
        .unwrap();
    let second = tokio::spawn({
        let table = table.clone();
        let work = work("second", None);
        async move {
            table
                .call_with_work(
                    scope(4),
                    small("q", 60_000),
                    None,
                    None,
                    deadline_in(STEP * 10),
                    Some(work),
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(60), table.waiting_at_least(2))
        .await
        .unwrap();
    release.send(()).unwrap();
    for call in [first, second] {
        assert!(matches!(
            call.await.unwrap().unwrap(),
            CallReply::Finished { outcome, .. }
                if matches!(outcome.as_ref(), JobOutcome::Done { value: "first", .. })
        ));
    }
    assert_eq!(ran.load(Ordering::SeqCst), 1);
}
