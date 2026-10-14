// SPDX-License-Identifier: Apache-2.0
use airk_kernel::Kernel;
use airk_store::{FileStore, scheduler::Queue};
use std::{fs, process::Command};

#[test]
fn durable_watermarks_coalesce_clicks_preserve_new_events_and_survive_reopen() -> anyhow::Result<()>
{
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    let first = queue.enqueue("fixture", "fixture-root", false, true, true, 10000)?;
    assert_eq!(first.requested_revision, 1);
    assert_eq!(
        queue
            .enqueue("fixture", "fixture-root", false, true, true, 10001)?
            .requested_revision,
        1
    );
    let worker = queue.worker_lock()?.expect("worker acquired");
    assert!(Queue::new(directory.path()).worker_lock()?.is_none());
    let running = queue.claim(&worker, 10002)?.expect("due manual job");
    let latest = queue.enqueue("fixture", "fixture-root", true, true, true, 10003)?;
    assert_eq!(
        latest.requested_revision, 2,
        "full rebuild cannot join incremental work"
    );
    queue.finish(&worker, &running, 10004, Ok("{}"))?;
    let status = Queue::new(directory.path())
        .status("fixture", "fixture-root")?
        .expect("persisted");
    assert_eq!(
        (status.completed_revision, status.requested_revision),
        (1, 2)
    );
    let next = queue.claim(&worker, 10004)?.expect("new full job");
    assert!(next.full);
    queue.finish(&worker, &next, 10005, Err("disk full fixture"))?;
    assert!(
        queue.claim(&worker, 100000)?.is_none(),
        "failed work cannot busy-loop"
    );
    assert!(
        queue
            .status("fixture", "fixture-root")?
            .expect("blocked")
            .blocked
    );
    queue.pause("fixture", "fixture-root", false)?;
    assert!(
        queue.claim(&worker, 100000)?.is_some(),
        "resume unblocks retained work"
    );
    assert!(
        queue
            .enqueue("fixture", "other-root", false, true, false, 100000)
            .is_err()
    );
    Ok(())
}

#[test]
fn native_scheduler_persists_pause_and_manual_override_without_resetting_it() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("fixture.txt"), "original")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.schedule_pause(true)?;
    assert!(kernel.schedule_status()?.expect("registered pause").paused);
    assert!(kernel.scheduled_index(false, true).is_err());
    assert_eq!(kernel.run_scheduled_once()?.state, "idle");
    let indexed = kernel.scheduled_index(false, false)?;
    assert_eq!(indexed.indexed, 1);
    assert!(kernel.schedule_status()?.expect("still paused").paused);
    let reopened = Kernel::open(root.path(), Some(state.path().into()))?;
    assert_eq!(
        reopened
            .schedule_status()?
            .expect("persisted")
            .completed_revision,
        1
    );
    reopened.schedule_forget()?;
    assert!(reopened.schedule_status()?.is_none());
    Ok(())
}

#[test]
fn continuously_dirty_collection_yields_to_an_older_waiting_collection() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    queue.enqueue("busy", "busy-root", false, true, false, 10000)?;
    let worker = queue.worker_lock()?.expect("worker acquired");
    let running = queue.claim(&worker, 10000)?.expect("first collection");
    queue.enqueue("waiting", "waiting-root", false, true, false, 10001)?;
    let later = queue.enqueue("busy", "busy-root", false, false, true, 10002)?;
    assert_eq!(
        later.requested_revision, 2,
        "an automatic event cannot join a running scan that may already have read its source"
    );
    queue.finish(&worker, &running, 10003, Ok("{}"))?;
    let next = queue.claim(&worker, 10004)?.expect("next collection");
    assert_eq!(next.collection, "waiting");
    assert_eq!(
        queue
            .status("busy", "busy-root")?
            .expect("pending")
            .first_pending_unix_ms,
        10002
    );
    Ok(())
}

#[test]
fn crashed_worker_releases_native_locks_and_pending_revision_can_resume() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("fixture.txt"), "recoverable fixture")?;
    let canonical = root.path().canonicalize()?;
    let status = Command::new(std::env::current_exe()?)
        .args(["--exact", "scheduler_crash_child", "--nocapture"])
        .env("AIRK_SCHEDULER_CRASH_STATE", state.path())
        .env("AIRK_SCHEDULER_CRASH_ROOT", &canonical)
        .status()?;
    assert!(status.success());
    assert!(
        state
            .path()
            .join("collections/default/writer.lock")
            .exists()
    );
    let kernel = Kernel::open(&canonical, Some(state.path().into()))?;
    assert_eq!(kernel.run_scheduled_once()?.state, "indexed");
    assert_eq!(
        kernel
            .schedule_status()?
            .expect("recovered")
            .completed_revision,
        1
    );
    assert!(
        !state
            .path()
            .join("collections/default/writer.lock")
            .exists()
    );
    Ok(())
}

#[test]
fn scheduler_crash_child() -> anyhow::Result<()> {
    let Some(state) = std::env::var_os("AIRK_SCHEDULER_CRASH_STATE") else {
        return Ok(());
    };
    let root = std::env::var("AIRK_SCHEDULER_CRASH_ROOT")?;
    let state = std::path::PathBuf::from(state);
    let queue = Queue::new(&state);
    queue.enqueue("default", &root, false, true, true, 10000)?;
    let worker = queue.worker_lock()?.expect("crash worker acquired");
    queue.claim(&worker, 10000)?.expect("crash job acquired");
    let _writer = FileStore::new(state.join("collections/default")).lock()?;
    // Intentionally skip Rust destructors, matching a process crash while holding
    // both advisory locks and a durable running claim.
    std::process::exit(0);
}

#[test]
fn native_reconciliation_discovers_saved_edits_without_an_editor() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("fixture.txt"), "before")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.scheduled_index(false, false)?;
    fs::write(root.path().join("fixture.txt"), "after autonomous fixture")?;
    let result = kernel.reconcile_scheduled_once()?;
    assert_eq!(result.state, "queued");
    let job = kernel.schedule_status()?.expect("retained request");
    assert!(job.scan_started_unix_ms > 0 && job.scan_completed_unix_ms >= job.scan_started_unix_ms);
    assert!(job.requested_revision > job.completed_revision);
    assert_eq!(
        kernel.reconcile_scheduled_once()?.state,
        "idle",
        "pending work and scan interval prevent repeated hashing"
    );
    std::thread::sleep(std::time::Duration::from_millis(5100));
    assert_eq!(kernel.run_scheduled_once()?.state, "indexed");
    assert_eq!(kernel.coverage(true)?.percent, Some(100));
    kernel.schedule_pause(true)?;
    fs::write(root.path().join("fixture.txt"), "paused fixture")?;
    assert_eq!(kernel.reconcile_scheduled_once()?.state, "idle");
    Ok(())
}

#[cfg(unix)]
#[test]
fn registered_root_cannot_be_rebound_by_replacing_it_with_a_symlink() -> anyhow::Result<()> {
    let fixture = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let original = fixture.path().join("workspace");
    let replacement = fixture.path().join("other-workspace");
    fs::create_dir(&original)?;
    fs::create_dir(&replacement)?;
    fs::write(original.join("original.txt"), "original collection")?;
    fs::write(replacement.join("other.txt"), "outside collection")?;
    let kernel = Kernel::open(&original, Some(state.path().into()))?;
    kernel.schedule_enqueue(false)?;
    fs::rename(&original, fixture.path().join("moved-workspace"))?;
    std::os::unix::fs::symlink(&replacement, &original)?;
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let outcome = kernel.run_scheduled_once()?;
    assert_eq!(outcome.state, "failed");
    assert!(
        outcome
            .error
            .expect("binding error")
            .contains("root identity changed")
    );
    assert!(
        kernel.index().is_err(),
        "existing kernel cannot follow the swapped root either"
    );
    assert!(kernel.coverage(true).is_err());
    Ok(())
}
