// SPDX-License-Identifier: Apache-2.0
//! Coherent bounded source-intent/job observations, independent of indexed evidence.
use super::ServiceRegistry;
use airk_core::{
    changes::{FreshnessObservation, FreshnessState},
    service::{JobOutcome, ProjectBinding},
};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;

impl ServiceRegistry {
    /// The supervisor forwards this content-free sample through fenced worker
    /// control. Retrieval never opens the service registry or the legacy queue.
    pub fn source_freshness(
        &self,
        project: ProjectBinding,
        now_ms: u64,
    ) -> Result<FreshnessObservation> {
        let tx = self.connection.unchecked_transaction()?;
        let watched = self.watch(project)?;
        let predicate =
            "(j.kind IN('index','reconcile','rebuild') OR (j.kind='repair' AND s.job IS NOT NULL))";
        // Registry admission already bounds all jobs to 1,024. Only one bounded
        // latest outcome and scalar aggregates cross this adapter boundary.
        let latest: Option<(String, String, Option<Vec<u8>>)> = tx.query_row(
            &format!("SELECT job,state,outcome FROM (SELECT j.seq,substr(j.id,1,65) AS job,substr(j.state,1,33) AS state,substr(j.outcome,1,2049) AS outcome FROM service_jobs j LEFT JOIN service_job_source s ON s.job=j.id WHERE j.project=?1 AND {predicate} UNION ALL SELECT seq,substr(job,1,65),substr(state,1,33),substr(outcome,1,2049) FROM service_source_outcomes WHERE project=?1) ORDER BY seq DESC LIMIT 1"),
            [project.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
        let (pending, running, running_revision): (bool, bool, Option<i64>) = tx.query_row(
            &format!("SELECT coalesce(max(j.state='pending'),0),coalesce(max(j.state='running'),0),max(CASE WHEN j.state='running' THEN s.revision END) FROM service_jobs j LEFT JOIN service_job_source s ON s.job=j.id WHERE j.project=?1 AND {predicate}"),
            [project.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut error = None;
        let terminal = latest
            .as_ref()
            .map(|(_, state, outcome)| -> Result<_> {
                if let Some(outcome) = outcome {
                    ensure!(outcome.len() <= 2048, "oversized source outcome");
                    let outcome: JobOutcome = serde_json::from_slice(outcome)?;
                    ensure!(
                        matches!(
                            (state.as_str(), &outcome),
                            ("succeeded", JobOutcome::Succeeded { .. })
                                | ("failed", JobOutcome::Failed { .. })
                                | ("canceled", JobOutcome::Canceled)
                        ),
                        "source outcome disagrees with state"
                    );
                    if let JobOutcome::Failed { code } = outcome {
                        ensure!(code.len() <= 384, "oversized source failure code");
                        error = Some(code);
                    }
                } else {
                    ensure!(
                        matches!(state.as_str(), "pending" | "running"),
                        "missing source outcome"
                    );
                }
                Ok(state.as_str())
            })
            .transpose()?;
        let watch = watched.observation;
        if terminal == Some("canceled") {
            error.get_or_insert_with(|| "source_job_canceled".into());
        }
        let watch_error = watch
            .failure
            .map(|failure| -> Result<String> {
                Ok(format!(
                    "source_watch_{}",
                    serde_json::to_value(failure)?
                        .as_str()
                        .context("invalid watch failure")?
                ))
            })
            .transpose()?;
        if let Some(watch_error) = &watch_error {
            error = Some(error.map_or_else(
                || watch_error.clone(),
                |code| format!("{code};{watch_error}"),
            ));
        }
        let state = if watched.project.paused {
            FreshnessState::Paused
        } else if watch.automatic_retries_exhausted || watch.automatic_retry_canceled {
            error.get_or_insert_with(|| {
                if watch.automatic_retry_canceled {
                    "source_retry_canceled"
                } else {
                    "source_retries_exhausted"
                }
                .into()
            });
            FreshnessState::Blocked
        } else if running {
            FreshnessState::Running
        } else if pending {
            FreshnessState::Pending
        } else if terminal == Some("failed") {
            FreshnessState::Failed
        } else if terminal == Some("canceled") {
            error.get_or_insert_with(|| "source_job_canceled".into());
            FreshnessState::Blocked
        } else if watch_error.is_some() {
            FreshnessState::Unavailable
        } else if watch.requested_revision > watch.completed_revision {
            FreshnessState::Pending
        } else if watch.enabled || latest.is_some() {
            FreshnessState::Idle
        } else {
            FreshnessState::Unregistered
        };
        let observation = FreshnessObservation {
            source_job: latest.as_ref().map(|row| row.0.parse()).transpose()?,
            observed_unix_ms: now_ms,
            state,
            requested_watermark: Some(watch.requested_revision),
            completed_watermark: Some(watch.completed_revision),
            running_watermark: running_revision.map(u64::try_from).transpose()?,
            last_success_unix_ms: watch.last_reconciled_unix_ms,
            error,
            live_source_verified: false,
            last_inventory_unix_ms: None,
            last_index: None,
        };
        tx.commit()?;
        Ok(observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ProjectLease;
    use airk_core::{
        identity::Incarnation,
        service::{JobKind, ProjectRegistration},
    };
    fn nonce(value: u8) -> Incarnation {
        Incarnation::from_bytes([value; 16])
    }

    #[test]
    fn source_failures_survive_unrelated_jobs_receipt_rotation_and_registry_restart() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), nonce(1))?;
        let registration =
            ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
        registry.register(&registration)?;
        let project = registration.binding;
        assert_eq!(
            registry.source_freshness(project, 100)?.state,
            FreshnessState::Unregistered
        );
        let mut lease = ProjectLease::acquire(directory.path(), project)?;
        registry.admit_worker(&mut lease)?;
        let job = registry.enqueue_internal(project, nonce(1), JobKind::Index, nonce(2))?;
        assert_eq!(
            registry.source_freshness(project, 101)?.state,
            FreshnessState::Pending
        );
        registry.pause(project, true)?;
        assert_eq!(
            registry.source_freshness(project, 101)?.state,
            FreshnessState::Paused
        );
        registry.pause(project, false)?;
        registry
            .claim_job_id(&lease, job.id)?
            .context("source not claimable")?;
        assert_eq!(
            registry.source_freshness(project, 102)?.state,
            FreshnessState::Running
        );
        registry.complete_job_at(
            &lease,
            job.id,
            JobOutcome::Failed {
                code: "source_unavailable".into(),
            },
            103,
        )?;
        let failed = registry.source_freshness(project, 104)?;
        assert_eq!(failed.state, FreshnessState::Failed);
        assert_eq!(failed.error.as_deref(), Some("source_unavailable"));
        let verify = registry.enqueue_internal(project, nonce(1), JobKind::Verify, nonce(3))?;
        registry
            .claim_job_id(&lease, verify.id)?
            .context("verify not claimable")?;
        registry.complete_job_at(
            &lease,
            verify.id,
            JobOutcome::Succeeded { revision: None },
            105,
        )?;
        assert_eq!(
            registry.source_freshness(project, 106)?.state,
            FreshnessState::Failed
        );
        // Model exact terminal receipt rotation without publishing more evidence.
        registry
            .connection
            .execute("DELETE FROM service_jobs WHERE id=?1", [job.id.to_string()])?;
        assert_eq!(
            registry.source_freshness(project, 107)?.state,
            FreshnessState::Failed
        );
        drop(registry);
        let mut registry = ServiceRegistry::open(directory.path(), nonce(1))?;
        assert_eq!(
            registry.source_freshness(project, 108)?.state,
            FreshnessState::Failed
        );
        let replacement =
            registry.enqueue_internal(project, nonce(1), JobKind::Reconcile, nonce(4))?;
        registry
            .claim_job_id(&lease, replacement.id)?
            .context("replacement not claimable")?;
        registry.complete_job_at(
            &lease,
            replacement.id,
            JobOutcome::Succeeded { revision: None },
            109,
        )?;
        let success = registry.source_freshness(project, 110)?;
        assert_eq!(success.state, FreshnessState::Idle);
        assert_eq!(success.last_success_unix_ms, Some(109));
        assert!(!success.live_source_verified && success.last_inventory_unix_ms.is_none());
        assert!(serde_json::to_vec(&success)?.len() < 4096);
        let repeated = registry.enqueue_internal(project, nonce(1), JobKind::Index, nonce(5))?;
        registry
            .claim_job_id(&lease, repeated.id)?
            .context("repeated source not claimable")?;
        registry.complete_job_at(
            &lease,
            repeated.id,
            JobOutcome::Failed {
                code: "source_unavailable".into(),
            },
            111,
        )?;
        let repeated_failure = registry.source_freshness(project, 112)?;
        assert_eq!(repeated_failure.error, failed.error);
        assert_ne!(
            repeated_failure.source_job, failed.source_job,
            "same error code must not hide a new source attempt"
        );
        Ok(())
    }

    #[test]
    fn canceled_and_corrupt_source_outcomes_are_never_idle_or_cross_project() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), nonce(1))?;
        let project =
            ProjectRegistration::new("/original".into(), "/derived".into(), "one".into())?;
        let other = ProjectRegistration::new("/other".into(), "/elsewhere".into(), "two".into())?;
        registry.register(&project)?;
        registry.register(&other)?;
        let job = registry.enqueue_internal(project.binding, nonce(1), JobKind::Index, nonce(2))?;
        registry.cancel_pending(job.id)?;
        assert_eq!(
            registry.source_freshness(project.binding, 100)?.state,
            FreshnessState::Blocked
        );
        assert_eq!(
            registry.source_freshness(other.binding, 100)?.state,
            FreshnessState::Unregistered
        );
        registry.connection.execute(
            "UPDATE service_source_outcomes SET state='succeeded' WHERE project=?1",
            [project.binding.to_string()],
        )?;
        // Drop the matching old job to ensure the persistent summary is tested.
        registry
            .connection
            .execute("DELETE FROM service_jobs WHERE id=?1", [job.id.to_string()])?;
        assert!(registry.source_freshness(project.binding, 101).is_err());
        Ok(())
    }

    #[test]
    fn existing_terminal_source_jobs_seed_only_the_latest_source_summary() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), nonce(1))?;
        let project =
            ProjectRegistration::new("/original".into(), "/derived".into(), "one".into())?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(directory.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let mut latest = None;
        for (sequence, kind, outcome) in [
            (2, JobKind::Index, JobOutcome::Succeeded { revision: None }),
            (
                3,
                JobKind::Reconcile,
                JobOutcome::Failed {
                    code: "source_unavailable".into(),
                },
            ),
            (4, JobKind::Verify, JobOutcome::Succeeded { revision: None }),
        ] {
            let job =
                registry.enqueue_internal(project.binding, nonce(1), kind, nonce(sequence))?;
            registry
                .claim_job_id(&lease, job.id)?
                .context("fixture job not claimable")?;
            registry.complete_job_at(&lease, job.id, outcome, 1000 + u64::from(sequence))?;
            if kind == JobKind::Reconcile {
                latest = Some(job.id);
            }
        }
        registry
            .connection
            .execute("DELETE FROM service_source_outcomes", [])?;
        drop(registry);
        let registry = ServiceRegistry::open(directory.path(), nonce(1))?;
        let observed = registry.source_freshness(project.binding, 1005)?;
        assert_eq!(observed.state, FreshnessState::Failed);
        assert_eq!(observed.source_job, latest);
        assert_eq!(observed.last_success_unix_ms, Some(1002));
        assert_eq!(
            registry.connection.query_row(
                "SELECT count(*) FROM service_source_outcomes",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    }
}
