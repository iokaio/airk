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
fn one_attempt_holds_at_most_three_jobs_so_another_always_has_a_slot() {
    let (a, b) = (index(1), index(2));
    let mut holders = Vec::new();
    for part in 1..=3 {
        let lane = answer(7, &[part]);
        assert!(admits(holders.iter().copied(), holders.len(), a, lane));
        holders.push((a, lane));
    }
    // A 4-part Answer holds 3 of index A's 4 slots; its 4th part waits.
    assert!(!admits(holders.iter().copied(), 3, a, answer(7, &[4])));
    // A second Answer, or an ordinary job, takes the remaining slot.
    assert!(admits(holders.iter().copied(), 3, a, answer(8, &[1])));
    assert!(admits(holders.iter().copied(), 3, a, ordinary()));
    holders.push((a, answer(8, &[1])));
    // Now the index is full for everyone.
    assert!(!admits(holders.iter().copied(), 4, a, answer(9, &[1])));
    assert!(!admits(holders.iter().copied(), 4, a, ordinary()));
    // Index B is untouched by A's jobs.
    assert!(admits(holders.iter().copied(), 4, b, answer(7, &[1])));
    assert!(admits(holders.iter().copied(), 4, b, ordinary()));
    // Once the first Answer's part 1 completes and the second's slot frees,
    // its 4th part is admitted.
    let freed: Vec<_> = holders[1..3].to_vec();
    assert!(admits(freed.iter().copied(), 2, a, answer(7, &[4])));
}

#[test]
fn a_second_answer_is_admitted_while_the_first_holds_three() {
    let a = index(1);
    let first = [
        (a, answer(7, &[1])),
        (a, answer(7, &[2])),
        (a, answer(7, &[3])),
    ];
    assert!(admits(first.iter().copied(), 3, a, answer(8, &[1])));
    // After one of the first Answer's jobs finishes, its 4th part fits again.
    assert!(admits(first[..2].iter().copied(), 2, a, answer(7, &[4])));
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
    // Answer jobs on the index count against the index ceiling, not the
    // ordinary limit: two Answer parts leave an ordinary job its slot.
    let answers = [(one, answer(7, &[1])), (one, answer(7, &[2]))];
    assert!(admits(answers.iter().copied(), 2, one, ordinary()));
    let full = [
        (one, answer(7, &[1])),
        (one, answer(7, &[2])),
        (one, answer(8, &[1])),
        (one, ordinary()),
    ];
    assert!(!admits(full.iter().copied(), 4, one, ordinary()));
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
