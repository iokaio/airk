// SPDX-License-Identifier: Apache-2.0
use super::*;

fn index(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}
fn attempt(byte: u8) -> Option<Incarnation> {
    Some(Incarnation::from_bytes([byte; 16]))
}
fn answer(attempt_byte: u8, parts: &[u8]) -> LaneKey {
    LaneKey::answer(
        Incarnation::from_bytes([attempt_byte; 16]),
        parts.iter().copied(),
    )
}
fn ordinary() -> LaneKey {
    LaneKey::default()
}

#[test]
fn parts_of_one_attempt_each_queue_one_job_up_to_the_index_ceiling() {
    let one = index(1);
    let mut holders = Vec::new();
    for part in 1..=4 {
        let lane = answer(7, &[part]);
        assert!(admits(holders.iter().copied(), holders.len(), one, lane));
        holders.push((one, lane));
    }
    // A fifth job on the index waits, whatever attempt or part it serves.
    assert!(!admits(
        holders.iter().copied(),
        holders.len(),
        one,
        answer(7, &[0])
    ));
    assert!(!admits(
        holders.iter().copied(),
        holders.len(),
        one,
        answer(8, &[1])
    ));
    // On another index the same attempt starts afresh.
    assert!(admits(
        holders.iter().copied(),
        holders.len(),
        index(2),
        answer(7, &[1])
    ));
}

#[test]
fn several_attempts_share_one_index_ceiling_and_a_second_index_keeps_room() {
    let (a, b) = (index(1), index(2));
    // Two compound Answers on index A: three parts of one, one of the other.
    let holders = [
        (a, answer(7, &[1])),
        (a, answer(7, &[2])),
        (a, answer(7, &[3])),
        (a, answer(8, &[1])),
    ];
    assert!(!admits(holders.iter().copied(), 4, a, answer(8, &[2])));
    assert!(!admits(holders.iter().copied(), 4, a, answer(9, &[1])));
    assert!(!admits(holders.iter().copied(), 4, a, ordinary()));
    // Index B is untouched by A's jobs: Answers and Search Focus both queue.
    assert!(admits(holders.iter().copied(), 4, b, answer(9, &[1])));
    assert!(admits(holders.iter().copied(), 4, b, ordinary()));
    // Below the ceiling a second Answer on A is admitted beside the first.
    assert!(admits(holders[..3].iter().copied(), 3, a, answer(8, &[1])));
}

#[test]
fn a_job_may_not_share_a_part_with_another_job_of_its_attempt() {
    let one = index(1);
    // A batched call over parts 1 and 2 is running.
    let running = [(one, answer(7, &[1, 2]))];
    assert!(!admits(running.iter().copied(), 0, one, answer(7, &[2])));
    assert!(!admits(running.iter().copied(), 0, one, answer(7, &[2, 3])));
    assert!(admits(running.iter().copied(), 0, one, answer(7, &[3])));
    // Another attempt's part 2 is a different key.
    assert!(admits(running.iter().copied(), 0, one, answer(8, &[2])));
    // An Answer job must name at least one part.
    assert!(!admits(std::iter::empty(), 0, one, answer(7, &[])));
}

#[test]
fn ordinary_jobs_keep_the_per_index_rule() {
    let one = index(1);
    assert!(admits(std::iter::empty(), 0, one, ordinary()));
    let held = [(one, ordinary())];
    assert!(admits(held.iter().copied(), 1, one, ordinary()));
    let two = [(one, ordinary()), (one, ordinary())];
    assert!(!admits(two.iter().copied(), 2, one, ordinary()));
    // Answer jobs on the index count against ordinary admission.
    let answers = [(one, answer(7, &[1])), (one, answer(7, &[2]))];
    assert!(!admits(answers.iter().copied(), 2, one, ordinary()));
}

#[test]
fn the_queue_cap_of_eight_holds_for_every_lane() {
    let holders: Vec<_> = (1..=8).map(|byte| (index(byte), ordinary())).collect();
    assert!(!admits(holders.iter().copied(), 8, index(9), ordinary()));
    assert!(!admits(
        holders.iter().copied(),
        8,
        index(9),
        answer(7, &[1])
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
fn part_masks_name_every_served_part() {
    let key = answer(7, &[0, 2, 4]);
    assert_eq!(key.parts, 0b1_0101);
    assert_eq!(LaneKey::default().parts, 0);
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
