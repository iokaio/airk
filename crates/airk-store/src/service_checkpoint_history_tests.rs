// SPDX-License-Identifier: Apache-2.0
//! History capacity must not consume the expanded accepted-work roster.
use super::*;
use airk_core::service::{JobOutcome, ProjectRegistration};
use rusqlite::params;

#[test]
fn completed_job_and_request_capacity_retains_full_history_without_expansion() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let control = directory.path().join("control");
    std::fs::create_dir(&control)?;
    let namespace = Incarnation::from_bytes([3; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let project = ProjectRegistration::new(
        directory
            .path()
            .join("missing-source")
            .to_string_lossy()
            .into_owned(),
        directory
            .path()
            .join("missing-storage")
            .to_string_lossy()
            .into_owned(),
        "history-fixture".into(),
    )?;
    registry.register(&project)?;
    // Exact canonical capacities; this fixture does not claim mutation admission.
    let tx = registry.connection.transaction()?;
    for n in 0u128..1024 {
        let id = framed_digest(b"checkpoint-history-fixture", &[&n.to_be_bytes()]);
        tx.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts,outcome) VALUES(?1,?2,'index','succeeded',0,1,?3)",
            params![id.to_string(),project.binding.to_string(),serde_json::to_vec(&JobOutcome::Succeeded { revision: None })?])?;
        for receipt in 0u128..4 {
            tx.execute(
                "INSERT INTO service_requests VALUES(?1,0,?2,?3,?4)",
                params![
                    namespace.to_string(),
                    Incarnation::from_bytes((n * 4 + receipt).to_be_bytes()).to_string(),
                    namespace.to_string(),
                    id.to_string()
                ],
            )?;
        }
    }
    tx.commit()?;
    let changes = registry.connection.total_changes();
    preflight_capacity(&control)?;
    let before = snapshot(&control)?;
    assert!(before.jobs.is_empty());
    assert!(before.requests.is_empty());
    assert!(before.unfinished.is_empty());
    for (name, count) in [("service_jobs", 1024), ("service_requests", 4096)] {
        assert_eq!(
            before.tables.iter().find(|t| t.name == name).unwrap().rows,
            Some(count)
        );
    }
    assert_eq!(changes, registry.connection.total_changes());
    let id = framed_digest(b"checkpoint-history-fixture", &[&0u128.to_be_bytes()]);
    registry.connection.execute(
        "UPDATE service_jobs SET attempts=2 WHERE id=?1",
        [id.to_string()],
    )?;
    assert_ne!(
        before.digest()?,
        snapshot(&control)?.digest()?,
        "terminal history remains authenticated"
    );
    registry.connection.execute(
        "UPDATE service_jobs SET state='pending',outcome=NULL WHERE id=?1",
        [id.to_string()],
    )?;
    let pending = snapshot(&control)?;
    assert_eq!(pending.jobs.len(), 1);
    assert_eq!(pending.jobs[0].record.id, id);
    assert_eq!(pending.requests.len(), 4);
    assert!(pending.requests.iter().all(|request| request.job == id));
    assert_eq!(pending.unfinished.len(), 1);
    assert!(
        pending
            .require_compatible(
                Some(Sha256Digest::from_bytes([1; 32])),
                Sha256Digest::from_bytes([2; 32])
            )
            .is_err()
    );
    assert!(
        preflight_snapshot(&control)?
            .require_compatible(
                Some(Sha256Digest::from_bytes([1; 32])),
                Sha256Digest::from_bytes([2; 32])
            )
            .is_err(),
        "registry-only preflight must still refuse incompatible accepted work"
    );
    registry.connection.execute(
        "UPDATE service_jobs SET state='succeeded',outcome=X'7b7d' WHERE id=?1",
        [id.to_string()],
    )?;
    assert!(
        snapshot(&control).is_err(),
        "terminal records must still validate before compaction"
    );
    Ok(())
}
