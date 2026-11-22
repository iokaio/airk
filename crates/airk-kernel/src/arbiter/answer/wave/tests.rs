// SPDX-License-Identifier: Apache-2.0
//! Wave reads: results by index whatever the completion order, at most four
//! in flight, and the reads that ran are always a prefix. Reads wait on
//! events under deadlines; nothing sleeps.
use super::*;
use std::sync::Condvar;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(20);

/// Shared state the test reads wait on.
#[derive(Default)]
struct Board {
    entered: usize,
    inside: usize,
    most_inside: usize,
    done: Vec<usize>,
}

struct Gate {
    board: Mutex<Board>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Self {
        Self {
            board: Mutex::new(Board::default()),
            changed: Condvar::new(),
        }
    }
    /// Enter, wait until `ready` holds (or fail at the deadline), then leave.
    fn read(&self, index: usize, ready: impl Fn(&Board) -> bool) -> usize {
        let mut board = self.board.lock().unwrap();
        board.entered += 1;
        board.inside += 1;
        board.most_inside = board.most_inside.max(board.inside);
        self.changed.notify_all();
        let until = Instant::now() + DEADLINE;
        while !ready(&board) {
            let left = until.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "read {index} waited past the deadline");
            board = self.changed.wait_timeout(board, left).unwrap().0;
        }
        board.inside -= 1;
        board.done.push(index);
        self.changed.notify_all();
        index
    }
}

#[test]
fn results_come_back_by_index_whatever_the_completion_order() {
    let gate = Gate::new();
    // Read i finishes only after read i + 1 has, so all four must be in
    // flight together and they complete in reverse.
    let results = run(
        4,
        &|| false,
        &|index| {
            gate.read(index, |board| {
                index == 3 || board.done.contains(&(index + 1))
            })
        },
        &|_| false,
    );
    assert_eq!(results, vec![0, 1, 2, 3]);
    let board = gate.board.lock().unwrap();
    assert_eq!(board.done, vec![3, 2, 1, 0]);
    assert_eq!(board.most_inside, 4);
}

#[test]
fn at_most_four_reads_are_in_flight() {
    let gate = Gate::new();
    // Each group of four waits for its whole group to enter, so a fifth
    // read in flight would show in `most_inside`, and fewer than four
    // workers would never release the first group.
    let results = run(
        10,
        &|| false,
        &|index| {
            let group_end = ((index / READS_IN_FLIGHT + 1) * READS_IN_FLIGHT).min(10);
            gate.read(index, move |board| board.entered >= group_end)
        },
        &|_| false,
    );
    assert_eq!(results, (0..10).collect::<Vec<_>>());
    let board = gate.board.lock().unwrap();
    assert_eq!(board.most_inside, READS_IN_FLIGHT);
    assert_eq!(board.done.len(), 10);
}

#[test]
fn no_read_is_claimed_after_the_reserve_or_a_halting_read() {
    // The reserve is reached once three reads have finished.
    let finished = AtomicUsize::new(0);
    let calls = AtomicUsize::new(0);
    let results = run(
        12,
        &|| finished.load(Ordering::Acquire) >= 3,
        &|index| {
            calls.fetch_add(1, Ordering::AcqRel);
            finished.fetch_add(1, Ordering::AcqRel);
            index
        },
        &|_| false,
    );
    assert!(results.len() >= 3, "{results:?}");
    assert_eq!(results, (0..results.len()).collect::<Vec<_>>());
    assert_eq!(calls.load(Ordering::Acquire), results.len());

    // A read that must stop the Answer stops further claims.
    let calls = AtomicUsize::new(0);
    let results = run(
        12,
        &|| false,
        &|index| {
            calls.fetch_add(1, Ordering::AcqRel);
            index
        },
        &|index| *index == 1,
    );
    assert!(results.len() >= 2, "{results:?}");
    assert_eq!(results, (0..results.len()).collect::<Vec<_>>());
    assert_eq!(calls.load(Ordering::Acquire), results.len());

    // One read runs inline; a reserve already reached runs none.
    assert_eq!(run(1, &|| false, &|index| index, &|_| false), vec![0]);
    assert!(run(5, &|| true, &|index| index, &|_| false).is_empty());
    assert!(run(0, &|| false, &|index| index, &|_| false).is_empty());
}
