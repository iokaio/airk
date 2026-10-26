// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::ProjectLease;
use airk_core::{
    identity::Incarnation,
    service::{JobKind, JobOutcome, ProjectRegistration},
};

fn registration(name: &str) -> Result<ProjectRegistration> {
    ProjectRegistration::new(
        format!("/fictional-source-{name}"),
        format!("/fictional-data-{name}"),
        name.into(),
    )
}

#[test]
fn events_are_transactional_content_free_and_keep_terminal_receipts_after_job_rotation()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([7; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), instance)?;
    let project = registration("first")?;
    registry.register(&project)?;
    registry.register(&project)?;
    registry.pause(project.binding, false)?;
    registry.pause(project.binding, true)?;
    registry.pause(project.binding, false)?;
    registry.configure_watch(project.binding, true)?;
    let job = registry.enqueue_internal(
        project.binding,
        instance,
        JobKind::Index,
        Incarnation::from_bytes([8; 16]),
    )?;
    registry.enqueue_internal(
        project.binding,
        instance,
        JobKind::Index,
        Incarnation::from_bytes([8; 16]),
    )?;
    let mut lease = ProjectLease::acquire(owner.path(), project.binding)?;
    registry.admit_worker(&mut lease)?;
    registry.claim_job_id(&lease, job.id)?;
    registry.request_cancellation(job.id)?;
    registry.request_cancellation(job.id)?;
    registry.complete_job(
        &lease,
        job.id,
        JobOutcome::Failed {
            code: "source_unavailable".into(),
        },
    )?;
    let page = registry.events(None, None, 32)?;
    let kinds: Vec<_> = page.events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::Registered,
            EventKind::Paused,
            EventKind::Resumed,
            EventKind::WatchEnabled,
            EventKind::JobPending,
            EventKind::JobRunning,
            EventKind::CancellationRequested,
            EventKind::JobFailed
        ]
    );
    let failed = page.events.last().unwrap();
    assert_eq!(failed.job, Some(job.id));
    assert_eq!(failed.attempts, Some(1));
    assert_eq!(failed.code.as_deref(), Some("source_unavailable"));
    assert!(failed.observed_unix_ms > 0);
    let encoded = serde_json::to_string(&page)?;
    assert!(!encoded.contains("fictional-source") && !encoded.contains("fictional-data"));
    {
        let tx = registry.connection.transaction()?;
        tx.execute(
            "UPDATE projects SET paused=1 WHERE binding=?1",
            [project.binding.to_string()],
        )?;
        // Rollback must undo both the mutation and its event, including retention.
    }
    assert_eq!(
        registry.events(None, None, 32)?.events.len(),
        page.events.len()
    );
    registry
        .connection
        .execute("DELETE FROM service_jobs WHERE id=?1", [job.id.to_string()])?;
    assert_eq!(
        registry.events(None, None, 32)?.events.last().unwrap().code,
        failed.code
    );
    Ok(())
}

#[test]
fn event_pages_pin_their_snapshot_scope_and_restart_identity_and_tail_polls_advance() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([3; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), instance)?;
    let first = registration("first")?;
    let second = registration("second")?;
    registry.register(&first)?;
    registry.register(&second)?;
    registry.pause(first.binding, true)?;
    registry.pause(first.binding, false)?;
    let page = registry.events(Some(first.binding), None, 2)?;
    let cursor = page.next.as_ref().unwrap().clone();
    registry.pause(second.binding, true)?;
    registry.pause(first.binding, true)?;
    assert!(
        registry
            .events(Some(second.binding), Some(&cursor), 2)
            .is_err()
    );
    let mut foreign = cursor.clone();
    foreign.namespace = Incarnation::from_bytes([9; 16]);
    assert!(
        registry
            .events(Some(first.binding), Some(&foreign), 2)
            .is_err()
    );
    drop(registry);
    let registry = ServiceRegistry::open(owner.path(), Incarnation::from_bytes([4; 16]))?;
    let last = registry.events(Some(first.binding), Some(&cursor), 2)?;
    assert_eq!(last.namespace, instance);
    assert_eq!(last.snapshot_through, page.snapshot_through);
    assert_eq!(last.events.len(), 1);
    assert_eq!(last.events[0].kind, EventKind::Resumed);
    assert!(last.next.is_none());
    let poll = registry.events(Some(first.binding), Some(&last.tail), 32)?;
    assert_eq!(poll.events.len(), 1);
    assert_eq!(poll.events[0].kind, EventKind::Paused);
    assert!(poll.events[0].sequence > last.snapshot_through);
    let empty = registry.events(Some(first.binding), Some(&poll.tail), 32)?;
    assert!(empty.events.is_empty());
    let mut future = poll.tail;
    future.after = u64::MAX;
    assert!(
        registry
            .events(Some(first.binding), Some(&future), 32)
            .is_err()
    );
    assert!(registry.events(None, None, 0).is_err());
    assert!(registry.events(None, None, 33).is_err());
    Ok(())
}

#[test]
fn rotation_expires_unread_positions_and_preserves_the_exact_bound_through_rollback() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([5; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), instance)?;
    let project = registration("first")?;
    registry.register(&project)?;
    let old = registry.events(None, None, 32)?.tail;
    {
        let tx = registry.connection.transaction()?;
        for turn in 0..1030 {
            tx.execute(
                "UPDATE projects SET paused=?1 WHERE binding=?2",
                params![turn % 2, project.binding.to_string()],
            )?;
        }
        tx.commit()?;
    }
    let count: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_events", [], |row| row.get(0))?;
    assert_eq!(count, 1024);
    assert!(registry.events(None, Some(&old), 32).is_err());
    let page = registry.events(None, None, 32)?;
    assert!(page.retained_prefix_lost && page.events.len() == 32);
    let mut total = page.events.len();
    let mut next = page.next;
    while let Some(cursor) = next {
        let batch = registry.events(None, Some(&cursor), 32)?;
        assert_eq!(batch.snapshot_through, page.snapshot_through);
        total += batch.events.len();
        next = batch.next;
    }
    assert_eq!(total, 1024);
    {
        let tx = registry.connection.transaction()?;
        for turn in 0..1100 {
            tx.execute(
                "UPDATE projects SET paused=?1 WHERE binding=?2",
                params![turn % 2, project.binding.to_string()],
            )?;
        }
    }
    let unchanged = registry.events(None, None, 32)?;
    assert_eq!(unchanged.oldest_sequence, page.oldest_sequence);
    assert_eq!(unchanged.snapshot_through, page.snapshot_through);
    Ok(())
}
