// SPDX-License-Identifier: Apache-2.0
//! Fixture-owned source work observations, separate from scoped evidence reads.
use airk_core::service::{ControlMethod, ControlResponse, ProjectBinding};
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};

fn settled(
    progress: &ControlResponse,
    watch: &ControlResponse,
    project: ProjectBinding,
    paused: bool,
) -> Result<bool> {
    let ControlResponse::Progress {
        project: seen,
        pending_job,
        queued_jobs,
        queue_truncated,
        project_paused,
        observation,
        ..
    } = progress
    else {
        anyhow::bail!(
            "source progress refused: {}",
            serde_json::to_string(progress)?
        );
    };
    ensure!(
        *seen == project
            && pending_job
                .as_ref()
                .is_none_or(|job| job.project == project)
            && queued_jobs.iter().all(|job| job.project == project)
            && observation.as_ref().is_none_or(
                |sample| sample.worker.project == project && sample.job.project == project
            ),
        "crossed source progress project"
    );
    let ControlResponse::Watch {
        project: seen,
        observation: watch,
    } = watch
    else {
        anyhow::bail!("source watch refused: {}", serde_json::to_string(watch)?);
    };
    ensure!(*seen == project, "crossed source watch project");
    watch.validate()?;
    Ok(pending_job.is_none()
        && queued_jobs.is_empty()
        && *queue_truncated == Some(false)
        && *project_paused == Some(paused)
        && watch.completed_revision == watch.requested_revision)
}

pub(super) fn wait(service: &crate::common::Service, state: &str) -> Result<()> {
    wait_until(service, state, Instant::now() + Duration::from_secs(30))
}
fn wait_until(service: &crate::common::Service, state: &str, until: Instant) -> Result<()> {
    ensure!(
        matches!(state, "idle" | "paused"),
        "unsupported fixture source state"
    );
    loop {
        // The owner-authenticated management path observes actual accepted jobs
        // and source-intent watermarks. Scoped evidence freshness deliberately
        // cannot expose this whole-project scheduler state, even in Full mode.
        let project = service.project.binding;
        let progress = service.request_until(ControlMethod::Progress { project }, until)?;
        let watch = service.request_until(ControlMethod::Watch { project }, until)?;
        if settled(&progress, &watch, project, state == "paused")?
            && !service.background_pending_until(project, until)?
        {
            return Ok(());
        }
        ensure!(
            Instant::now() < until,
            "fixture source work did not settle to {state}: progress={}, watch={}",
            serde_json::to_string(&progress)?,
            serde_json::to_string(&watch)?
        );
        std::thread::sleep(
            Duration::from_millis(25).min(until.saturating_duration_since(Instant::now())),
        );
    }
}

/// Pin the fixture's admitted publication work across sequential transport
/// comparisons. An idle sample alone is not a lease: automatic semantic work
/// may publish after it. Pause is native project policy, and the following
/// observations still require all previously admitted work to finish.
fn set_pause(service: &crate::common::Service, paused: bool, until: Instant) -> Result<()> {
    let project = service.project.binding;
    let response = service.request_until(ControlMethod::Pause { project, paused }, until)?;
    ensure!(
        matches!(&response, ControlResponse::Registered { project: saved }
        if saved.registration.binding == project && saved.paused == paused),
        "fixture pause refused or crossed project: {}",
        serde_json::to_string(&response)?
    );
    Ok(())
}
pub(super) fn pause_and_wait(service: &crate::common::Service) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        wait_until(service, "idle", until)?;
        set_pause(service, true, until)?;
        let project = service.project.binding;
        let progress = service.request_until(ControlMethod::Progress { project }, until)?;
        let watch = service.request_until(ControlMethod::Watch { project }, until)?;
        if settled(&progress, &watch, project, true)?
            && !service.background_pending_until(project, until)?
        {
            return Ok(());
        }
        // Accepted work can race the idle sample before Pause. Resume only this
        // owned fixture's project so that work can actually finish; never hide
        // its queue or classify a paused unfinished job as settled.
        set_pause(service, false, until)?;
        ensure!(
            Instant::now() < until,
            "fixture publication did not settle before pause: progress={}, watch={}",
            serde_json::to_string(&progress)?,
            serde_json::to_string(&watch)?
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service_watch::{WatchBackend, WatchObservation};
    fn progress(
        project: ProjectBinding,
        paused: Option<bool>,
        truncated: Option<bool>,
    ) -> ControlResponse {
        ControlResponse::Progress {
            project,
            observation: None,
            pending_job: None,
            queued_jobs: vec![],
            queue_truncated: truncated,
            project_paused: paused,
            pending_paused: Some(false),
            observed_unix_ms: Some(1),
        }
    }
    fn watch(project: ProjectBinding, requested: u64, completed: u64) -> ControlResponse {
        ControlResponse::Watch {
            project,
            observation: WatchObservation {
                enabled: false,
                requested_revision: requested,
                completed_revision: completed,
                automatic_retries_exhausted: false,
                automatic_retry_canceled: false,
                configuration_observed: true,
                configured_roots: 1,
                native_roots: 0,
                backend: WatchBackend::Disabled,
                failure: None,
                observed_unix_ms: Some(1),
                last_reconciled_unix_ms: Some(1),
                traversal: None,
                traversal_observed_unix_ms: None,
            },
        }
    }
    #[test]
    fn source_settlement_requires_exact_project_complete_queue_watermarks_and_pause_state()
    -> Result<()> {
        let project = ProjectBinding::from_bytes([1; 32]);
        let observed = watch(project, 2, 2);
        assert!(settled(
            &progress(project, Some(false), Some(false)),
            &observed,
            project,
            false
        )?);
        assert!(!settled(
            &progress(project, None, Some(false)),
            &observed,
            project,
            false
        )?);
        assert!(!settled(
            &progress(project, Some(false), None),
            &observed,
            project,
            false
        )?);
        assert!(!settled(
            &progress(project, Some(false), Some(true)),
            &observed,
            project,
            false
        )?);
        assert!(!settled(
            &progress(project, Some(false), Some(false)),
            &watch(project, 3, 2),
            project,
            false
        )?);
        assert!(!settled(
            &progress(project, Some(false), Some(false)),
            &observed,
            project,
            true
        )?);
        assert!(settled(
            &progress(project, Some(true), Some(false)),
            &observed,
            project,
            true
        )?);
        for state in [
            airk_core::service::JobState::Pending,
            airk_core::service::JobState::Running,
        ] {
            let job = airk_core::service::JobRecord {
                id: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
                project,
                kind: airk_core::service::JobKind::Index,
                state,
                fencing_generation: 1,
                attempts: 1,
                outcome: None,
            };
            let mut active = progress(project, Some(false), Some(false));
            if let ControlResponse::Progress { pending_job, .. } = &mut active {
                *pending_job = Some(job.clone());
            }
            assert!(!settled(&active, &observed, project, false)?);
            let mut queued = progress(project, Some(false), Some(false));
            if let ControlResponse::Progress { queued_jobs, .. } = &mut queued {
                queued_jobs.push(job);
            }
            assert!(!settled(&queued, &observed, project, false)?);
        }
        assert!(
            settled(
                &progress(
                    ProjectBinding::from_bytes([2; 32]),
                    Some(false),
                    Some(false)
                ),
                &observed,
                project,
                false
            )
            .is_err()
        );
        assert!(
            settled(
                &progress(project, Some(false), Some(false)),
                &watch(ProjectBinding::from_bytes([2; 32]), 2, 2),
                project,
                false
            )
            .is_err()
        );
        Ok(())
    }
}
