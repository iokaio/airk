// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ProjectRegistration, WorkerIdentity},
};
fn tracker(now: Instant) -> Result<Tracker, anyhow::Error> {
    let identity = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([2; 16]),
        project: ProjectRegistration::new("/source".into(), "/derived".into(), "fixture".into())?
            .binding,
        fencing_generation: 1,
    };
    Ok(Tracker::new(
        &WorkAllowance {
            id: Incarnation::from_bytes([3; 16]),
            identity,
            job: Sha256Digest::from_bytes([4; 32]),
            extraction_threads: 1,
            inference_threads: 1,
            working_memory_bytes: 768 << 20,
            background_io_bytes_per_second: 8 << 20,
            ttl_ms: 300_000,
        },
        now,
    ))
}
#[test]
fn long_semantic_preparation_advances_without_claiming_committed_coverage() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut tracker = tracker(start)?;
    for seconds in [120, 400, 700, 1000] {
        let now = start + Duration::from_secs(seconds);
        tracker.semantic_prepared(now);
        assert_eq!(tracker.watchdog(now), (false, false));
    }
    let snapshot = tracker.snapshot(start + Duration::from_secs(1001));
    assert_eq!(snapshot.last_advance_age_ms, Some(1000));
    assert_eq!(snapshot.publications, 0);
    assert_eq!(snapshot.committed_changes, 0);
    assert_eq!(snapshot.completed_semantic_units, None);
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(1600)),
        (true, false)
    );
    Ok(())
}

#[test]
fn resource_wait_and_overlapping_pause_do_not_trigger_stall_or_invent_progress()
-> anyhow::Result<()> {
    let start = Instant::now();
    let mut tracker = tracker(start)?;
    tracker.wait(true, start + Duration::from_secs(10));
    tracker.pause(true, start + Duration::from_secs(20));
    tracker.wait(false, start + Duration::from_secs(1000));
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(2000)),
        (false, false)
    );
    tracker.pause(false, start + Duration::from_secs(2010));
    let observation = tracker.snapshot(start + Duration::from_secs(2011));
    assert_eq!(observation.unpaused_idle_ms, 11_000);
    assert_eq!(observation.last_advance_age_ms, None);
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(2600)),
        (true, false)
    );
    Ok(())
}
#[test]
fn heartbeat_and_phase_changes_do_not_extend_real_progress_deadline() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut tracker = tracker(start)?;
    tracker.phase(WorkPhase::Discovering, start + Duration::from_secs(100));
    tracker.phase(WorkPhase::Preparing, start + Duration::from_secs(590));
    let snapshot = tracker.snapshot(start + Duration::from_secs(599));
    snapshot.validate()?;
    assert_eq!(snapshot.last_advance_age_ms, None);
    assert_eq!(snapshot.unpaused_idle_ms, 599_000);
    assert_eq!(snapshot.phase_totals_ms.values().sum::<u64>(), 599_000);
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(599)),
        (false, false)
    );
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(600)),
        (true, false)
    );
    assert!(tracker.snapshot(start + Duration::from_secs(600)).stalled);
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(629)),
        (true, false)
    );
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(630)),
        (true, true)
    );
    tracker.phase(WorkPhase::Finished, start + Duration::from_secs(631));
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(632)),
        (false, false)
    );
    Ok(())
}
#[test]
fn pause_excludes_idle_time_and_only_actual_boundaries_advance_counts() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut tracker = tracker(start)?;
    tracker.capture(start + Duration::from_secs(10));
    tracker.extracted(start + Duration::from_secs(20));
    tracker.pause(true, start + Duration::from_secs(30));
    assert_eq!(
        tracker.watchdog(start + Duration::from_secs(3600)),
        (false, false)
    );
    tracker.pause(false, start + Duration::from_secs(3630));
    let snapshot = tracker.snapshot(start + Duration::from_secs(3640));
    snapshot.validate()?;
    assert_eq!(snapshot.unpaused_idle_ms, 20_000);
    assert_eq!(snapshot.last_advance_age_ms, Some(3_620_000));
    assert_eq!(snapshot.total_files, None);
    assert_eq!(
        (
            snapshot.captured_files,
            snapshot.extracted_files,
            snapshot.committed_changes
        ),
        (1, 1, 0)
    );
    tracker.prepared(1, start + Duration::from_secs(3650));
    assert_eq!(
        tracker
            .snapshot(start + Duration::from_secs(3651))
            .committed_changes,
        0
    );
    tracker.committed(1, start + Duration::from_secs(3660));
    let snapshot = tracker.snapshot(start + Duration::from_secs(3661));
    snapshot.validate()?;
    assert_eq!(
        (
            snapshot.prepared_changes,
            snapshot.committed_changes,
            snapshot.publications
        ),
        (1, 1, 1)
    );
    assert_eq!(snapshot.last_advance_age_ms, Some(1000));
    tracker.phase(WorkPhase::Finished, start + Duration::from_secs(3662));
    let terminal = tracker.snapshot(start + Duration::from_secs(3662));
    tracker.pause(true, start + Duration::from_secs(4000));
    let later = tracker.snapshot(start + Duration::from_secs(8000));
    later.validate()?;
    assert_eq!(terminal.elapsed_ms, later.elapsed_ms);
    assert_eq!(terminal.last_advance_age_ms, later.last_advance_age_ms);
    assert!(
        !later.paused,
        "a late control pulse changed a terminal work observation"
    );
    Ok(())
}
