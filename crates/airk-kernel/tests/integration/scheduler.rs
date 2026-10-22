// SPDX-License-Identifier: Apache-2.0
use airk_kernel::Kernel;
use airk_store::{FileStore, scheduler::Queue};
use std::{fs, process::Command};

#[test]
fn different_project_publishes_while_another_process_is_scanning() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let other_root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    for file in 0..40 {
        fs::write(
            root.path().join(format!("a_{file:02}.txt")),
            "project a evidence",
        )?;
    }
    fs::write(other_root.path().join("b.txt"), "project b evidence")?;
    let signal = tempfile::tempdir()?;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "scheduler::parallel_project_child",
            "--nocapture",
        ])
        .env("AIRK_PARALLEL_STATE", state.path())
        .env("AIRK_PARALLEL_ROOT", root.path())
        .env("AIRK_PARALLEL_SIGNAL", signal.path())
        .spawn()?;
    let result = (|| -> anyhow::Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !signal.path().join("scanning").exists() {
            anyhow::ensure!(child.try_wait()?.is_none(), "child exited before its scan");
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "child scan did not start"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let queue = Queue::new(state.path());
        assert!(
            queue.worker_lock("a")?.is_none(),
            "same collection cannot run twice"
        );
        let a_root = root.path().canonicalize()?.to_string_lossy().into_owned();
        let before = queue.status("a", &a_root)?.unwrap();
        assert!(before.running_revision > 0);
        let other = Kernel::open_collection(other_root.path(), Some(state.path().into()), "b")?;
        other.configure_vectors(false)?;
        assert_eq!(other.scheduled_index(false, false)?.indexed, 1);
        assert_eq!(other.status()?.files, 1);
        assert!(
            !signal.path().join("timed-out").exists(),
            "B waited for A instead of publishing concurrently"
        );
        assert_eq!(
            queue.status("a", &a_root)?.unwrap().running_revision,
            before.running_revision
        );
        // Deleting B must not require stopping A's worker.
        airk_store::remove_collection(state.path(), "b")?;
        assert!(queue.status("a", &a_root)?.unwrap().running_revision > 0);
        Ok(())
    })();
    fs::write(signal.path().join("release"), "release")?;
    let status = child.wait()?;
    result?;
    assert!(status.success());
    let kernel = Kernel::open_collection(root.path(), Some(state.path().into()), "a")?;
    assert_eq!(kernel.status()?.files, 40);
    Ok(())
}

#[test]
fn parallel_project_child() -> anyhow::Result<()> {
    let Some(state) = std::env::var_os("AIRK_PARALLEL_STATE") else {
        return Ok(());
    };
    let root = std::env::var_os("AIRK_PARALLEL_ROOT").unwrap();
    let signal = std::path::PathBuf::from(std::env::var_os("AIRK_PARALLEL_SIGNAL").unwrap());
    let kernel = Kernel::open_collection(std::path::Path::new(&root), Some(state.into()), "a")?
        .with_index_progress(move |event| {
            if event.phase == "base_committed" && !signal.join("scanning").exists() {
                fs::write(signal.join("scanning"), "scanning").unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
                while !signal.join("release").exists() {
                    if std::time::Instant::now() >= deadline {
                        fs::write(signal.join("timed-out"), "timed out").unwrap();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        });
    kernel.configure_vectors(false)?;
    kernel.scheduled_index(false, false)?;
    Ok(())
}

#[test]
fn selected_join_does_not_execute_an_older_foreign_request() -> anyhow::Result<()> {
    let a = tempfile::tempdir()?;
    let b = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(a.path().join("a.txt"), "a")?;
    fs::write(b.path().join("b.txt"), "b")?;
    let foreign = Kernel::open_collection(a.path(), Some(data.path().into()), "a")?;
    foreign.configure_vectors(false)?;
    let pending = foreign.schedule_enqueue(false)?;
    Queue::new(data.path()).enqueue("a", &pending.root, false, true, true, 0)?;
    let selected = Kernel::open_collection(b.path(), Some(data.path().into()), "b")?;
    selected.configure_vectors(false)?;
    selected.scheduled_index(false, false)?;
    assert_eq!(foreign.schedule_status()?.unwrap().completed_revision, 0);
    assert_eq!(foreign.schedule_status()?.unwrap().running_revision, 0);
    assert!(foreign.status().is_err());
    // Root-wide workers skip a held collection rather than stopping at it.
    let held = Queue::new(data.path()).worker_lock("a")?.unwrap();
    let request = selected.schedule_enqueue(false)?;
    Queue::new(data.path()).enqueue("b", &request.root, false, true, true, 0)?;
    let outcome = selected.run_scheduled_once()?;
    assert_eq!(outcome.state, "indexed");
    assert_eq!(outcome.collection.as_deref(), Some("b"));
    assert_eq!(foreign.schedule_status()?.unwrap().completed_revision, 0);
    assert_eq!(selected.run_scheduled_once()?.state, "busy");
    drop(held);
    assert_eq!(
        selected.run_scheduled_once()?.collection.as_deref(),
        Some("a")
    );
    Ok(())
}

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
    let worker = queue.worker_lock("fixture")?.expect("worker acquired");
    assert!(
        Queue::new(directory.path())
            .worker_lock("fixture")?
            .is_none()
    );
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
    let worker = queue.worker_lock("busy")?.expect("worker acquired");
    let running = queue.claim(&worker, 10000)?.expect("first collection");
    queue.enqueue("waiting", "waiting-root", false, true, false, 10001)?;
    let later = queue.enqueue("busy", "busy-root", false, false, true, 10002)?;
    assert_eq!(
        later.requested_revision, 2,
        "an automatic event cannot join a running scan that may already have read its source"
    );
    queue.finish(&worker, &running, 10003, Ok("{}"))?;
    let waiting_worker = queue.worker_lock("waiting")?.expect("independent worker");
    let next = queue
        .claim(&waiting_worker, 10004)?
        .expect("next collection");
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
        .args(["--exact", "scheduler::scheduler_crash_child", "--nocapture"])
        .env("AIRK_SCHEDULER_CRASH_STATE", state.path())
        .env("AIRK_SCHEDULER_CRASH_ROOT", &canonical)
        .status()?;
    assert!(status.success());
    assert!(
        state
            .path()
            .join("collections-1.2/default/writer.lock")
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
            .join("collections-1.2/default/writer.lock")
            .exists()
    );
    Ok(())
}

#[test]
fn live_collection_writer_defers_scheduled_index_without_blocking_queue() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("fixture.txt"), "original fixture evidence")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    let requested = kernel.schedule_enqueue(false)?;
    // Coverage refresh, retention or another editor can briefly own this lock
    // independently of the data-root scheduler worker.
    let writer = FileStore::new(state.path().join("collections-1.2/default")).lock()?;
    // Manual enqueue is immediately due, without a timing-dependent sleep.
    Queue::new(state.path()).enqueue("default", &requested.root, false, true, true, 0)?;
    assert_eq!(kernel.run_scheduled_once()?.state, "busy");
    let pending = kernel.schedule_status()?.expect("queued request");
    assert!(!pending.blocked);
    assert_eq!(pending.running_revision, 0);
    assert_eq!(pending.completed_revision, 0);
    assert!(pending.error.is_none());
    drop(writer);
    let completed = kernel.scheduled_index(false, false)?;
    assert_eq!(completed.indexed, 1);
    assert!(
        !kernel
            .schedule_status()?
            .expect("completed request")
            .blocked
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
    let worker = queue
        .worker_lock("default")?
        .expect("crash worker acquired");
    queue.claim(&worker, 10000)?.expect("crash job acquired");
    let _writer = FileStore::new(state.join("collections-1.2/default")).lock()?;
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
