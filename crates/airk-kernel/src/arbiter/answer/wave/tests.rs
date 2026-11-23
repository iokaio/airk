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

/// A connector a read can recognize: the real thread-local lookup
/// (`embedding_worker::connector`) must return exactly this one.
struct Marked;
impl airk_retrieval::embedding_worker::Connector for Marked {
    fn open(
        &self,
        _location: airk_retrieval::embedding_worker::ModelLocation,
        _identity: &str,
    ) -> anyhow::Result<std::sync::Arc<dyn airk_retrieval::embedding_worker::RemoteSession>> {
        anyhow::bail!("test connector opens nothing")
    }
}

fn marked_scope() -> (
    airk_retrieval::embedding_worker::ConnectorScope,
    std::sync::Arc<dyn airk_retrieval::embedding_worker::Connector>,
) {
    let connector: std::sync::Arc<dyn airk_retrieval::embedding_worker::Connector> =
        std::sync::Arc::new(Marked);
    let scope = airk_retrieval::embedding_worker::ConnectorScope::new();
    scope.install(connector.clone()).unwrap();
    (scope, connector)
}

/// The thread's connector is exactly `expected` (not a process fallback).
fn sees(expected: &std::sync::Arc<dyn airk_retrieval::embedding_worker::Connector>) -> bool {
    airk_retrieval::embedding_worker::connector()
        .is_ok_and(|found| std::sync::Arc::ptr_eq(&found, expected))
}

fn worker_identity(byte: u8) -> airk_core::service::WorkerIdentity {
    use airk_core::identity::{Incarnation, Sha256Digest};
    airk_core::service::WorkerIdentity {
        service: Incarnation::from_bytes([byte; 16]),
        worker: Incarnation::from_bytes([byte.wrapping_add(1); 16]),
        project: Sha256Digest::from_bytes([byte; 32]),
        fencing_generation: 1,
    }
}

/// A wave of two and of four reads: every read, on a worker thread, sees the
/// caller's connector, the caller's execution scope and the caller's accepted
/// grant, so a canceled grant stops the read's own checkpoint. On the old
/// runner the workers saw none of them: the connector lookup failed with
/// "embedding worker is unavailable" and the checkpoint passed.
#[test]
fn every_wave_worker_reads_under_the_callers_scopes() -> anyhow::Result<()> {
    use crate::service_execution as execution;
    let root = tempfile::tempdir()?;
    let identity = worker_identity(9);
    let scope = execution::ExecutionScope::new();
    scope.install(root.path(), identity)?;
    let (connector_scope, connector) = marked_scope();
    let _execution = scope.bind_thread();
    let _connector = connector_scope.bind_thread();
    let caller = std::thread::current().id();
    for (turn, count) in [(30u8, 2usize), (31, 4)] {
        let grant = airk_core::service_resources::WorkAllowance {
            id: airk_core::identity::Incarnation::from_bytes([turn; 16]),
            identity,
            job: airk_core::identity::Sha256Digest::from_bytes([turn; 32]),
            extraction_threads: 1,
            inference_threads: 1,
            working_memory_bytes: 128 << 20,
            background_io_bytes_per_second: 1 << 20,
            ttl_ms: 60_000,
        };
        execution::activate(&grant)?;
        execution::run(grant.clone(), || {
            execution::control(identity, grant.id, false, true, false)?;
            let seen = run(
                count,
                &|| false,
                &|_| {
                    let off_caller = std::thread::current().id() != caller;
                    let stopped = execution::checkpoint()
                        .err()
                        .is_some_and(|stop| stop.is::<execution::Canceled>());
                    (
                        off_caller,
                        sees(&connector),
                        execution::ExecutionScope::current().is_some(),
                        stopped,
                    )
                },
                &|_| false,
            );
            assert_eq!(
                seen,
                vec![(true, true, true, true); count],
                "wave of {count}: (off the caller, connector, execution scope, canceled grant)"
            );
            Ok(())
        })?;
    }
    Ok(())
}

/// A binding a read leaves behind on its worker never reaches the worker's
/// next read, and nothing a worker bound reaches the caller.
#[test]
fn a_binding_left_on_a_worker_reaches_neither_its_next_read_nor_the_caller() {
    let (caller_scope, caller_connector) = marked_scope();
    let (stray_scope, stray_connector) = marked_scope();
    let reads = 2 * READS_IN_FLIGHT;
    let runs = Mutex::new(Vec::new());
    {
        let _caller = caller_scope.bind_thread();
        let seen = run(
            reads,
            &|| false,
            &|_| {
                let before = sees(&caller_connector);
                // A read that leaks its own binding on the worker thread.
                std::mem::forget(stray_scope.bind_thread());
                runs.lock().unwrap().push(std::thread::current().id());
                before
            },
            &|_| false,
        );
        assert_eq!(seen, vec![true; reads], "a read saw another read's binding");
        // Eight reads on four workers: some worker ran two or more, so the
        // check above covered a read after a leak on its own thread.
        let runs = runs.into_inner().unwrap();
        assert!(
            runs.iter()
                .any(|thread| runs.iter().filter(|other| *other == thread).count() > 1),
            "no worker ran a second read: {runs:?}"
        );
        assert!(sees(&caller_connector), "the caller lost its binding");
        assert!(!sees(&stray_connector));
    }
    // The caller's own guard restores it to unbound; a wave from an unbound
    // caller binds nothing on its workers.
    assert!(airk_retrieval::embedding_worker::ConnectorScope::current().is_none());
    let unbound = run(
        2,
        &|| false,
        &|_| {
            airk_retrieval::embedding_worker::ConnectorScope::current().is_none()
                && crate::service_execution::ExecutionScope::current().is_none()
        },
        &|_| false,
    );
    assert_eq!(unbound, vec![true, true]);
}

#[test]
fn a_panicking_read_halts_the_wave_and_the_lowest_panic_is_raised() {
    let calls = AtomicUsize::new(0);
    let raised = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(
            40,
            &|| false,
            &|index| {
                calls.fetch_add(1, Ordering::AcqRel);
                if index == 1 || index == 2 {
                    panic!("read {index} failed");
                }
                index
            },
            &|_| false,
        )
    }))
    .unwrap_err();
    assert_eq!(
        raised.downcast_ref::<String>().map(String::as_str),
        Some("read 1 failed")
    );
    assert!(calls.load(Ordering::Acquire) >= 3, "reads 0 to 2 ran");
}
