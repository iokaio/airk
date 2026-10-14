// SPDX-License-Identifier: Apache-2.0
use airk_store::{quota::Quota, scheduler::Queue};
use rusqlite::Connection;
use std::fs;

#[test]
fn full_quota_rejects_queue_writes_but_preserves_readable_pending_work() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let queue = Queue::new(directory.path());
    queue.enqueue("fixture", "workspace", false, true, false, 1)?;
    let quota = Quota::open(directory.path())?;
    let mut policy = quota.policy()?;
    policy.root_bytes = quota.status()?.used_bytes;
    quota.configure(policy)?;
    let before = fs::read(directory.path().join("scheduler.sqlite3"))?;
    assert!(
        queue
            .enqueue("fixture", "workspace", true, true, false, 2)
            .is_err()
    );
    let pending = queue.status("fixture", "workspace")?.expect("pending work");
    assert_eq!(pending.requested_revision, 1);
    assert!(!pending.full);
    assert_eq!(
        fs::read(directory.path().join("scheduler.sqlite3"))?,
        before
    );
    assert_eq!(quota.status()?.active_reservations, 0);
    Ok(())
}

#[test]
fn legacy_status_is_read_only_and_corrupt_negative_revisions_are_rejected() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("scheduler.sqlite3");
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
    assert!(!directory.path().join(".quota").exists());
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
    assert!(queue.worker_lock().is_err());
    Ok(())
}
