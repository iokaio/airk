// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::ProjectRegistration;

#[test]
fn clean_break_retry_successors_retain_exact_manual_lineage_and_original_failure() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let caller = Incarnation::from_bytes([51; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), caller)?;
    let project = ProjectRegistration::new(
        "/original".into(),
        "/derived".into(),
        "retry-lineage".into(),
    )?;
    registry.register(&project)?;
    registry.pause(project.binding, true)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: caller,
    };
    let original = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
    assert!(registry.retry_successors(original.id)?.is_empty());
    let failure = JobOutcome::Failed {
        code: "source_unavailable".into(),
    };
    registry.connection.execute(
        "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
        params![serde_json::to_vec(&failure)?, original.id.to_string()],
    )?;
    let retry = registry.retry_job(
        original.id,
        caller,
        &RequestKey {
            nonce: Incarnation::from_bytes([52; 16]),
            ..key
        },
    )?;
    let before: i64 = registry
        .connection
        .query_row("SELECT total_changes()", [], |row| row.get(0))?;
    let children = registry.retry_successors(original.id)?;
    assert_eq!(children.len(), 1);
    assert_eq!(
        serde_json::to_value(&children[0])?,
        serde_json::to_value(&retry)?
    );
    assert!(registry.retry_successors(retry.id)?.is_empty());
    assert_eq!(
        registry
            .connection
            .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))?,
        before
    );
    assert_eq!(registry.job(original.id)?.outcome, Some(failure));
    registry.connection.execute(
        "UPDATE service_jobs SET kind='reconcile' WHERE id=?1",
        [retry.id.to_string()],
    )?;
    assert!(
        registry.retry_successors(original.id).is_err(),
        "kind changes cannot become exact retry lineage"
    );
    Ok(())
}

#[test]
fn clean_break_successful_retry_follows_transitive_branches_and_refuses_backward_authority()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let caller = Incarnation::from_bytes([61; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), caller)?;
    let project = ProjectRegistration::new(
        "/original".into(),
        "/derived".into(),
        "retry-branches".into(),
    )?;
    registry.register(&project)?;
    registry.pause(project.binding, true)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: caller,
    };
    let root = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
    let failure = JobOutcome::Failed {
        code: "source_unavailable".into(),
    };
    let failed = serde_json::to_vec(&failure)?;
    let succeeded = serde_json::to_vec(&JobOutcome::Succeeded { revision: None })?;
    registry.connection.execute(
        "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
        params![failed, root.id.to_string()],
    )?;
    let first = registry.retry_job(
        root.id,
        caller,
        &RequestKey {
            nonce: Incarnation::from_bytes([62; 16]),
            ..key
        },
    )?;
    registry.connection.execute(
        "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
        params![failed, first.id.to_string()],
    )?;
    let second = registry.retry_job(
        first.id,
        caller,
        &RequestKey {
            nonce: Incarnation::from_bytes([63; 16]),
            ..key
        },
    )?;
    registry.connection.execute(
        "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
        params![succeeded, second.id.to_string()],
    )?;
    assert_eq!(registry.successful_retry(root.id)?.unwrap().id, second.id);
    let branch = registry.retry_job(
        root.id,
        caller,
        &RequestKey {
            nonce: Incarnation::from_bytes([64; 16]),
            ..key
        },
    )?;
    registry.connection.execute(
        "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
        params![succeeded, branch.id.to_string()],
    )?;
    assert_eq!(
        registry.successful_retry(root.id)?.unwrap().id,
        branch.id,
        "newest actual successful accepted retry wins by native creation sequence"
    );
    assert_eq!(registry.job(root.id)?.outcome, Some(failure));
    registry.connection.execute("INSERT INTO service_retry_requests(namespace,epoch,nonce,caller,source_job) VALUES(?1,?2,?3,?4,?5)",
        params![key.namespace.to_string(), i64::try_from(key.epoch)?, key.nonce.to_string(), caller.to_string(), second.id.to_string()])?;
    assert!(
        registry.successful_retry(root.id).is_err(),
        "backward/cyclic lineage cannot certify a successful rebuild"
    );
    Ok(())
}

#[test]
fn rebuild_configuration_guard_includes_paused_running_and_setup_continuations() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let caller = Incarnation::from_bytes([41; 16]);
    let mut registry = ServiceRegistry::open(owner.path(), caller)?;
    let project = ProjectRegistration::new(
        "/saved-source".into(),
        "/saved-state".into(),
        "fixture".into(),
    )?;
    let sibling = ProjectRegistration::new(
        "/other-source".into(),
        "/other-state".into(),
        "sibling".into(),
    )?;
    registry.register(&project)?;
    registry.register(&sibling)?;
    registry.pause(project.binding, true)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let job = registry.enqueue(
        project.binding,
        caller,
        &RequestKey {
            namespace,
            epoch,
            nonce: caller,
        },
        JobKind::Index,
    )?;
    assert!(!registry.has_unsettled_configuration(project.binding)?);
    for kind in [JobKind::Configure, JobKind::ModelSetup] {
        for state in ["pending", "running", "succeeded", "failed", "canceled"] {
            // Terminal rows must carry the canonical outcome: source-job
            // history triggers retain it even when this fixture changes kind.
            let outcome = match state {
                "succeeded" => Some(JobOutcome::Succeeded { revision: None }),
                "failed" => Some(JobOutcome::Failed {
                    code: "fictional_configuration_failure".into(),
                }),
                "canceled" => Some(JobOutcome::Canceled),
                _ => None,
            }
            .map(|outcome| serde_json::to_vec(&outcome))
            .transpose()?;
            registry.connection.execute(
                "UPDATE service_jobs SET kind=?1,state=?2,outcome=?3 WHERE id=?4",
                params![kind.code(), state, outcome, job.id.to_string()],
            )?;
            assert_eq!(
                registry.has_unsettled_configuration(project.binding)?,
                matches!(state, "pending" | "running"),
                "{kind:?}/{state}"
            );
            assert!(!registry.has_unsettled_configuration(sibling.binding)?);
        }
    }
    registry.connection.execute(
        "INSERT INTO service_setup_pending VALUES(?1)",
        [project.binding.to_string()],
    )?;
    assert!(registry.has_unsettled_configuration(project.binding)?);
    assert!(!registry.has_unsettled_configuration(sibling.binding)?);
    registry.connection.execute(
        "DELETE FROM service_setup_pending WHERE project=?1",
        [project.binding.to_string()],
    )?;
    assert!(!registry.has_unsettled_configuration(project.binding)?);
    Ok(())
}
