// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest};

fn grant(identity: WorkerIdentity, turn: u8) -> WorkAllowance {
    WorkAllowance {
        id: Incarnation::from_bytes([turn; 16]),
        identity,
        job: Sha256Digest::from_bytes([turn; 32]),
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 256 << 20,
        background_io_bytes_per_second: 32 << 20,
        ttl_ms: 300_000,
    }
}

fn on_helper<T: Send>(inherited: &Inherited, probe: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let _bound = inherited.bind();
                probe()
            })
            .join()
            .expect("helper probe panicked")
    })
}

/// A helper thread of accepted work (graph insertion beside inference) must
/// stop at its own checkpoints as the grant's thread does, and a paused grant
/// must be observable without parking at a batch boundary.
#[test]
fn helper_threads_inherit_the_grant_and_pause_is_observable_without_waiting() -> Result<()> {
    let root = tempfile::tempdir()?;
    let identity = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([2; 16]),
        project: Sha256Digest::from_bytes([3; 32]),
        fencing_generation: 1,
    };
    let scope = ExecutionScope::new();
    scope.install(root.path(), identity)?;
    let _bound = scope.bind_thread();
    let outside = Inherited::capture();
    assert!(on_helper(&outside, || checkpoint().is_ok() && !paused_at_boundary()));

    let canceled = grant(identity, 4);
    activate(&canceled)?;
    run(canceled.clone(), || {
        let inherited = Inherited::capture();
        assert!(on_helper(&inherited, || checkpoint().is_ok()));
        control(identity, canceled.id, true, true, false)?;
        let stop = on_helper(&inherited, checkpoint).expect_err("helper ignored cancellation");
        assert!(stop.is::<Canceled>(), "{stop:#}");
        assert!(
            !paused_at_boundary(),
            "cancellation must take precedence over a pause boundary"
        );
        Ok(())
    })?;
    // The helper binding is restored when the helper's guard drops.
    assert!(on_helper(&outside, || checkpoint().is_ok()));

    let paused = grant(identity, 5);
    activate(&paused)?;
    run(paused.clone(), || {
        assert!(!paused_at_boundary());
        control(identity, paused.id, true, false, false)?;
        assert!(paused_at_boundary());
        let inherited = Inherited::capture();
        assert!(on_helper(&inherited, paused_at_boundary));
        // A turn that ended on the pause finishes its bookkeeping without parking,
        // and still stops for cancellation.
        {
            let _turn = pause_turn();
            release_pause_wait();
            assert!(pause_released());
            checkpoint()?;
        }
        // The release ends with its turn: a later turn of the same grant parks
        // on the pause again instead of running batches while paused.
        assert!(!pause_released());
        assert!(paused_at_boundary());
        control(identity, paused.id, false, false, false)?;
        assert!(!paused_at_boundary());
        assert!(on_helper(&inherited, || checkpoint().is_ok()));
        Ok(())
    })?;
    assert!(!paused_at_boundary(), "no grant outside accepted work");
    Ok(())
}
