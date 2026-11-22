// SPDX-License-Identifier: Apache-2.0
use super::*;

fn index(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}
fn attempt(byte: u8) -> Option<Incarnation> {
    Some(Incarnation::from_bytes([byte; 16]))
}
fn key(attempt_byte: Option<u8>, part: u8) -> LaneKey {
    LaneKey {
        attempt: attempt_byte.and_then(attempt),
        part,
    }
}

#[test]
fn parts_of_one_attempt_each_queue_one_job_up_to_four() {
    let one = index(1);
    let mut holders = Vec::new();
    for part in 1..=4 {
        let lane = key(Some(7), part);
        assert!(admits(holders.iter().copied(), holders.len(), one, lane));
        holders.push((one, lane));
    }
    // A fifth key of the same attempt (Split's part 0) waits for a slot.
    assert!(!admits(
        holders.iter().copied(),
        holders.len(),
        one,
        key(Some(7), 0)
    ));
    // On another index the same attempt starts afresh.
    assert!(admits(
        holders.iter().copied(),
        holders.len(),
        index(2),
        key(Some(7), 1)
    ));
}

#[test]
fn a_second_job_for_the_same_part_is_refused_even_when_running() {
    let one = index(1);
    let running = [(one, key(Some(7), 2))];
    assert!(!admits(running.iter().copied(), 0, one, key(Some(7), 2)));
    assert!(admits(running.iter().copied(), 0, one, key(Some(7), 3)));
}

#[test]
fn ordinary_jobs_keep_the_per_index_rule() {
    let one = index(1);
    let ordinary = key(None, 0);
    assert!(admits(std::iter::empty(), 0, one, ordinary));
    let held = [(one, ordinary)];
    assert!(admits(held.iter().copied(), 1, one, ordinary));
    let two = [(one, ordinary), (one, ordinary)];
    assert!(!admits(two.iter().copied(), 2, one, ordinary));
    // Answer jobs on the index count against ordinary admission, as every
    // job on an index did before.
    let answers = [(one, key(Some(7), 1)), (one, key(Some(7), 2))];
    assert!(!admits(answers.iter().copied(), 2, one, ordinary));
    // A second Answer is not blocked by the first one's parts.
    assert!(admits(answers.iter().copied(), 2, one, key(Some(8), 1)));
}

#[test]
fn the_queue_cap_of_eight_holds_for_every_lane() {
    let holders: Vec<_> = (1..=8).map(|byte| (index(byte), key(None, 0))).collect();
    assert!(!admits(holders.iter().copied(), 8, index(9), key(None, 0)));
    assert!(!admits(
        holders.iter().copied(),
        8,
        index(9),
        key(Some(7), 1)
    ));
}

#[test]
fn the_scheduler_alternates_attempts_on_one_index() {
    let one = index(1);
    let queued = [(one, attempt(7)), (one, attempt(7)), (one, attempt(8))];
    // After attempt 7 ran, attempt 8's job goes next although it is newest.
    assert_eq!(
        next_position(queued.iter().copied(), Some((one, attempt(7)))),
        2
    );
    // With no other key waiting, the oldest runs.
    let same = [(one, attempt(7)), (one, attempt(7))];
    assert_eq!(
        next_position(same.iter().copied(), Some((one, attempt(7)))),
        0
    );
    // Indexes still alternate as before.
    let indexes = [(one, None), (index(2), None)];
    assert_eq!(next_position(indexes.iter().copied(), Some((one, None))), 1);
    assert_eq!(next_position(indexes.iter().copied(), None), 0);
}

#[test]
fn card_tasks_run_under_the_context_enrollment() {
    for task in [
        Task::ContextSplit,
        Task::ContextPlan,
        Task::ContextCoverage,
        Task::ContextFollowUp,
    ] {
        assert_eq!(enrollment_task(task), Task::ContextAction);
    }
    assert_eq!(enrollment_task(Task::FocusRank), Task::FocusRank);
    assert_eq!(enrollment_task(Task::ContextAction), Task::ContextAction);
}
