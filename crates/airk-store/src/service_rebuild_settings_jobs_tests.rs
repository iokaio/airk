// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::ProjectRegistration;

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
