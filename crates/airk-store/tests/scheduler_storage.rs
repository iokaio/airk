// SPDX-License-Identifier: Apache-2.0
use airk_store::{quota::Quota, scheduler::Queue};
use rusqlite::Connection;
use std::fs;

#[test]
fn collection_workers_preserve_foreign_claims_and_reject_foreign_mutations() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    queue.enqueue("a", "root-a", false, true, false, 1)?;
    queue.enqueue("b", "root-b", false, true, false, 2)?;
    let a = queue.worker_lock("a")?.unwrap();
    let running_a = queue.claim(&a, 3)?.unwrap();
    assert!(queue.worker_lock("a")?.is_none());
    let b = queue.worker_lock("b")?.unwrap();
    let running_b = queue.claim(&b, 3)?.unwrap();
    assert_eq!(running_b.collection, "b");
    assert_eq!(queue.status("a", "root-a")?.unwrap().running_revision, 1);
    assert!(queue.finish(&b, &running_a, 4, Ok("{}")).is_err());
    assert!(queue.defer(&b, &running_a).is_err());
    assert!(queue.block_semantic(&b, &running_a, "foreign").is_err());
    assert!(queue.finish_scan(&b, &running_a, 4, None).is_err());
    assert!(queue.forget_locked(&b, "a", "root-a").is_err());
    queue.pause("a", "root-a", true)?;
    queue.finish(&b, &running_b, 4, Ok("{}"))?;
    drop(a); // Simulate an abandoned claim; only A may recover it.
    let a = queue.worker_lock("a")?.unwrap();
    assert!(queue.claim(&a, 5)?.is_none());
    assert_eq!(queue.status("a", "root-a")?.unwrap().running_revision, 0);
    assert_eq!(queue.status("b", "root-b")?.unwrap().completed_revision, 1);
    queue.pause("a", "root-a", false)?;
    assert_eq!(queue.claim(&a, 10000)?.unwrap().collection, "a");
    assert!(queue.worker_lock("../escape").is_err());
    Ok(())
}

#[test]
fn legacy_worker_and_collection_workers_exclude_each_other() -> anyhow::Result<()> {
    use std::fs::OpenOptions;
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    let old = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.path().join("scheduler-1.2-worker.lock"))?;
    old.try_lock()?;
    let error = queue
        .worker_lock("a")
        .err()
        .expect("old worker excludes new workers");
    assert!(error.to_string().contains("older AIRK indexing worker"));
    old.unlock()?;
    let a = queue.worker_lock("a")?.unwrap();
    let b = queue.worker_lock("b")?.unwrap();
    assert!(matches!(
        old.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(a);
    assert!(matches!(
        old.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(b);
    old.try_lock()?;
    Ok(())
}

#[test]
fn completion_history_binds_coalesced_revisions_and_expires_without_misreporting()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    let first = queue.enqueue("fixture", "workspace", true, true, false, 1)?;
    let second = queue.enqueue("fixture", "workspace", false, true, false, 2)?;
    let worker = queue.worker_lock("fixture")?.unwrap();
    let full = queue.claim(&worker, 2)?.unwrap();
    assert!(full.full);
    queue.finish(&worker, &full, 3, Ok(r#"{"path_scoped":false}"#))?;
    for revision in [first.requested_revision, second.requested_revision] {
        assert_eq!(
            queue
                .completed_report("fixture", "workspace", revision)?
                .as_deref(),
            Some(r#"{"path_scoped":false}"#)
        );
        assert!(
            queue
                .completed_report("fixture", "other-root", revision)?
                .is_none()
        );
    }
    for pass in 0..4 {
        queue.enqueue_selected(
            "fixture",
            "workspace",
            false,
            true,
            false,
            4 + pass * 2,
            Some(&["changed.rs".into()]),
            true,
        )?;
        let scoped = queue.claim(&worker, 4 + pass * 2)?.unwrap();
        queue.finish(
            &worker,
            &scoped,
            5 + pass * 2,
            Ok(r#"{"path_scoped":true}"#),
        )?;
        if pass == 0 {
            assert_eq!(
                queue
                    .completed_report("fixture", "workspace", first.requested_revision)?
                    .as_deref(),
                Some(r#"{"path_scoped":false}"#)
            );
        }
    }
    assert!(
        queue
            .completed_report("fixture", "workspace", first.requested_revision)?
            .is_none()
    );
    let latest = queue
        .status("fixture", "workspace")?
        .unwrap()
        .completed_revision;
    assert_eq!(
        queue
            .completed_report("fixture", "workspace", latest)?
            .as_deref(),
        Some(r#"{"path_scoped":true}"#)
    );
    queue.forget_locked(&worker, "fixture", "workspace")?;
    assert!(
        queue
            .completed_report("fixture", "workspace", latest)?
            .is_none()
    );
    Ok(())
}

#[test]
fn hinted_queue_preserves_concurrent_events_and_bypasses_only_settled_debounce()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    let first = queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        true,
        10,
        Some(&["a.rs".into()]),
        true,
    )?;
    assert_eq!(first.paths, Some(vec!["a.rs".into()]));
    let worker = queue.worker_lock("fixture")?.unwrap();
    let running = queue
        .claim(&worker, 10)?
        .expect("settled request is immediately due");
    let next = queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        true,
        11,
        Some(&["b.rs".into()]),
        true,
    )?;
    assert!(next.requested_revision > running.requested_revision);
    queue.finish(&worker, &running, 12, Ok(r#"{"path_scoped":true}"#))?;
    let pending = queue.status("fixture", "workspace")?.unwrap();
    assert_eq!(pending.paths, Some(vec!["b.rs".into()]));
    assert_eq!(pending.recent_paths, vec!["a.rs"]);
    assert_eq!(pending.inventory_completed_unix_ms, 0);
    queue.pause("fixture", "workspace", true)?;
    assert!(queue.claim(&worker, 12)?.is_none());
    queue.pause("fixture", "workspace", false)?;
    let running = queue
        .claim(&worker, 12)?
        .expect("settled edits bypass the five second floor");
    assert_eq!(running.paths, Some(vec!["b.rs".into()]));
    queue.finish(&worker, &running, 13, Ok(r#"{"path_scoped":true}"#))?;
    let pending = queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        false,
        14,
        Some(&["c.rs".into()]),
        false,
    )?;
    assert!(!pending.debounced);
    assert!(queue.claim(&worker, 14)?.is_none());
    assert!(
        queue
            .enqueue_selected(
                "fixture",
                "workspace",
                false,
                true,
                false,
                14,
                Some(&["../outside".into()]),
                true
            )
            .is_err()
    );
    queue.enqueue("fixture", "workspace", false, true, false, 15)?;
    let running = queue.claim(&worker, 15)?.unwrap();
    assert!(
        running.paths.is_none(),
        "complete inventory supersedes all hints"
    );
    queue.finish(&worker, &running, 16, Ok(r#"{"path_scoped":false}"#))?;
    assert_eq!(
        queue
            .status("fixture", "workspace")?
            .unwrap()
            .inventory_completed_unix_ms,
        16
    );
    Ok(())
}

#[test]
fn hint_overflow_promotes_to_complete_inventory_without_losing_the_revision() -> anyhow::Result<()>
{
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    let paths: Vec<_> = (0..1024).map(|index| format!("file_{index}.rs")).collect();
    let first = queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        false,
        1,
        Some(&paths),
        true,
    )?;
    assert_eq!(first.paths.as_ref().unwrap().len(), 1024);
    let next = queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        false,
        2,
        Some(&["overflow.rs".into()]),
        true,
    )?;
    assert!(next.requested_revision > first.requested_revision);
    assert!(next.paths.is_none());
    Ok(())
}

#[test]
fn full_quota_rejects_queue_writes_but_preserves_readable_pending_work() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    queue.enqueue("fixture", "workspace", false, true, false, 1)?;
    let quota = Quota::open(directory.path())?;
    let mut policy = quota.policy()?;
    policy.root_bytes = quota.status()?.used_bytes;
    quota.configure(policy)?;
    let before = fs::read(directory.path().join("scheduler-1.2.sqlite3"))?;
    assert!(
        queue
            .enqueue("fixture", "workspace", true, true, false, 2)
            .is_err()
    );
    let pending = queue.status("fixture", "workspace")?.expect("pending work");
    assert_eq!(pending.requested_revision, 1);
    assert!(!pending.full);
    assert_eq!(
        fs::read(directory.path().join("scheduler-1.2.sqlite3"))?,
        before
    );
    assert_eq!(quota.status()?.active_reservations, 0);
    Ok(())
}

#[test]
fn legacy_status_is_read_only_and_corrupt_negative_revisions_are_rejected() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("scheduler-1.2.sqlite3");
    let database = Connection::open(&path)?;
    database.execute_batch("CREATE TABLE jobs(collection TEXT PRIMARY KEY, root TEXT, revision INTEGER, completed INTEGER, running INTEGER, paused INTEGER, blocked INTEGER, full_revision INTEGER, manual_revision INTEGER, first_pending INTEGER, last_event INTEGER, last_completed INTEGER, error TEXT, report TEXT);
        INSERT INTO jobs VALUES('fixture','workspace',3,1,0,0,0,0,3,10,20,0,NULL,NULL);")?;
    drop(database);
    let before = fs::read(&path)?;
    let queue = Queue::new(directory.path());
    let status = queue.status("fixture", "workspace")?.expect("legacy queue");
    assert_eq!(status.requested_revision, 3);
    assert_eq!(status.scan_started_unix_ms, 0);
    assert_eq!(fs::read(&path)?, before);
    assert!(!directory.path().join(".quota-1.2").exists());
    Connection::open(path)?.execute("UPDATE jobs SET revision=-1", [])?;
    assert!(queue.status("fixture", "workspace").is_err());
    Ok(())
}

#[test]
fn retired_root_rejects_queue_access_even_without_a_database() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join(".relocated-to.json"),
        br#"{"destination":"new-root"}"#,
    )?;
    let queue = Queue::new(directory.path());
    assert!(queue.status("fixture", "workspace").is_err());
    assert!(
        queue
            .enqueue("fixture", "workspace", false, true, false, 1)
            .is_err()
    );
    assert!(queue.worker_lock("fixture").is_err());
    Ok(())
}

#[test]
fn manual_inventory_during_a_running_hint_pass_cannot_complete_with_that_pass() -> anyhow::Result<()>
{
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    queue.enqueue_selected(
        "fixture",
        "workspace",
        false,
        false,
        true,
        1,
        Some(&["one.rs".into()]),
        true,
    )?;
    let worker = queue.worker_lock("fixture")?.unwrap();
    let running = queue.claim(&worker, 1)?.unwrap();
    let requested =
        queue.enqueue_selected("fixture", "workspace", false, true, true, 2, None, true)?;
    assert!(requested.requested_revision > running.requested_revision);
    queue.finish(&worker, &running, 3, Ok(r#"{"path_scoped":true}"#))?;
    let next = queue.claim(&worker, 3)?.unwrap();
    assert_eq!(next.requested_revision, requested.requested_revision);
    assert!(next.paths.is_none());
    Ok(())
}
