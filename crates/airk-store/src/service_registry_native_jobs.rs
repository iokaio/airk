// SPDX-License-Identifier: Apache-2.0
//! Bounded job observations and explicit fenced interruption/cancellation.
use super::{NativeServiceRegistry, State, observations::worker_epoch};
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, RevisionToken, Sha256Digest},
    service::{JobError, JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, RequestKey},
};
use anyhow::{Context, Result, ensure};

fn observation(
    state: &State,
    project: ProjectBinding,
    limit: usize,
    eligible_only: bool,
) -> Result<Vec<JobRecord>> {
    let project_entry = state
        .projects
        .get(&project.to_string())
        .context("service project metadata is unavailable")?;
    let mut jobs: Vec<_> = state
        .jobs
        .values()
        .filter(|job| {
            job.record.project == project
                && (project_entry.registered || super::native_configuration::derived(job))
                && job.record.state == JobState::Pending
                && (!eligible_only || super::native_maintenance::eligible(state, job))
                && (!eligible_only
                    || !project_entry.paused
                    || super::native_configuration::runs_while_paused(job))
        })
        .collect();
    jobs.sort_by_key(|job| job.sequence);
    Ok(jobs
        .into_iter()
        .take(limit)
        .map(|job| job.record.clone())
        .collect())
}
impl NativeServiceRegistry {
    pub fn retry_job(
        &mut self,
        id: Sha256Digest,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<JobRecord> {
        self.prepare_job_request(caller, key, Some(id))?;
        self.write(|state| {
            let identity = super::request_identity(caller, key);
            if let Some(accepted) = state.accepted_requests.get(&identity) {
                ensure!(
                    state.retry_origins.get(&identity) == Some(&id),
                    JobError::KeyConflict
                );
                return Ok(state
                    .jobs
                    .get(accepted)
                    .context(JobError::UnknownJob)?
                    .record
                    .clone());
            }
            ensure!(
                key.namespace == state.request_namespace && key.epoch == state.request_epoch,
                JobError::KeyExpired
            );
            let original = state
                .jobs
                .get(&id.to_string())
                .context(JobError::UnknownJob)?
                .clone();
            ensure!(
                matches!(original.record.state, JobState::Failed | JobState::Canceled),
                JobError::NotRetryable
            );
            ensure!(
                !matches!(original.record.kind, JobKind::PlanInspect | JobKind::Repair),
                airk_core::service_maintenance::PlanError::InvalidTransition
            );
            let result = super::enqueue_request(
                state,
                original.record.project,
                caller,
                key,
                original.record.kind,
                original
                    .configuration
                    .as_ref()
                    .map(|configuration| &configuration.work),
                Some(id),
            )?;
            if original.record.kind == JobKind::Maintain {
                super::native_maintenance::reset_automatic(state, original.record.project);
            }
            if super::native_background::source_kind(original.record.kind) {
                let watch = state
                    .watches
                    .entry(original.record.project.to_string())
                    .or_default();
                watch.attempts = 0;
                watch.next_ms = 0;
                watch.blocked_revision = None;
            }
            Ok(result)
        })
    }
    pub fn enqueue_internal(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        kind: JobKind,
        nonce: Incarnation,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        self.enqueue(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            kind,
        )
    }
    pub fn job_runs_while_paused(&self, id: Sha256Digest) -> Result<bool> {
        self.read(|state| {
            Ok(super::native_configuration::runs_while_paused(
                state
                    .jobs
                    .get(&id.to_string())
                    .context(JobError::UnknownJob)?,
            ))
        })
    }
    pub fn pending_project_page(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        self.read(|state| {
            let mut projects = std::collections::BTreeSet::new();
            for job in state.jobs.values() {
                let entry = state
                    .projects
                    .get(&job.record.project.to_string())
                    .context("job project missing")?;
                if after.is_none_or(|after| job.record.project > after)
                    && (entry.registered || super::native_configuration::derived(job))
                    && (job.record.state == JobState::Running
                        || job.record.state == JobState::Pending
                            && super::native_maintenance::eligible(state, job)
                            && (!entry.paused
                                || super::native_configuration::runs_while_paused(job)))
                {
                    projects.insert(job.record.project);
                }
            }
            Ok(projects.into_iter().take(limit).collect())
        })
    }
    pub fn next_pending_job(&self, project: ProjectBinding) -> Result<Option<JobRecord>> {
        self.read(|state| Ok(observation(state, project, 1, true)?.into_iter().next()))
    }
    pub fn pending_job_observation(&self, project: ProjectBinding) -> Result<Option<JobRecord>> {
        self.read(|state| Ok(observation(state, project, 1, false)?.into_iter().next()))
    }
    pub fn queued_jobs_observation(
        &self,
        project: ProjectBinding,
        limit: usize,
    ) -> Result<Vec<JobRecord>> {
        ensure!(
            (1..=32).contains(&limit),
            "queued job observation limit must be 1..32"
        );
        self.read(|state| observation(state, project, limit, false))
    }
    pub fn cancellation_requested(&self, id: Sha256Digest) -> Result<bool> {
        self.read(|state| {
            Ok(state
                .jobs
                .get(&id.to_string())
                .is_some_and(|job| job.cancellation_requested))
        })
    }
    pub fn request_cancellation(&mut self, id: Sha256Digest) -> Result<JobRecord> {
        self.cancel_job(id, false)
    }
    pub fn cancel_pending(&mut self, id: Sha256Digest) -> Result<JobRecord> {
        self.cancel_job(id, true)
    }
    fn cancel_job(&self, id: Sha256Digest, pending_only: bool) -> Result<JobRecord> {
        self.write(|state| {
            let job = state
                .jobs
                .get_mut(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                !pending_only || job.record.state == JobState::Pending,
                "running or completed job requires worker-aware cancellation"
            );
            if job.record.state == JobState::Pending {
                job.record.state = JobState::Canceled;
                job.record.outcome = Some(JobOutcome::Canceled);
            } else if job.record.state == JobState::Running {
                job.cancellation_requested = true;
            }
            let result = job.record.clone();
            let source_revision = job.source_revision;
            if result.kind == JobKind::Maintain
                && matches!(
                    result.state,
                    JobState::Pending | JobState::Running | JobState::Canceled
                )
            {
                super::native_maintenance::block_automatic(state, result.project);
            }
            if let Some(revision) = source_revision {
                let watch = state
                    .watches
                    .get_mut(&result.project.to_string())
                    .context("source watch missing")?;
                watch.blocked_revision = Some(watch.blocked_revision.unwrap_or(0).max(revision));
            }
            if result.state == JobState::Canceled {
                super::native_background::completed(state, id, None)?;
                super::native_maintenance::completed(
                    state,
                    id,
                    super::native_maintenance::clock_ms()?,
                    None,
                )?;
            }
            state.bump()?;
            Ok(result)
        })
    }
    pub fn fail_pending_start(&mut self, lease: &ProjectLease, code: &str) -> Result<usize> {
        ensure!(
            lease.root == self.root && lease.epoch.is_none(),
            "startup failure requires unadmitted project ownership"
        );
        ensure!(
            !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid job failure code"
        );
        self.write(|state| {
            ensure!(!state.jobs.values().any(|job| job.record.project == lease.binding && job.record.state == JobState::Running), "interrupted running work requires fenced recovery before startup failure settlement");
            let mut count = 0;
            for job in state.jobs.values_mut().filter(|job| {
                job.record.project == lease.binding && job.record.state == JobState::Pending
            }) {
                job.record.state = JobState::Failed;
                job.record.outcome = Some(JobOutcome::Failed {
                    code: code.to_owned(),
                });
                count += 1;
            }
            if count > 0 {
                state.bump()?;
            }
            Ok(count)
        })
    }
    pub fn recover_jobs(&mut self, lease: &ProjectLease) -> Result<()> {
        self.recover_jobs_with_rebuild_receipt(lease, None)
    }
    pub fn recover_jobs_with_rebuild_receipt(
        &mut self,
        lease: &ProjectLease,
        receipt: Option<&crate::service::collection::RebuildReceipt>,
    ) -> Result<()> {
        self.write(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            let mut completed = Vec::new();
            let mut rebuilt = false;
            for job in state.jobs.values_mut().filter(|job| {
                job.record.project == lease.binding
                    && job.record.state == JobState::Running
                    && job.record.fencing_generation != epoch
            }) {
                if let Some(receipt) = receipt.filter(|receipt| {
                    receipt.job == job.record.id
                        && matches!(job.record.kind, JobKind::Rebuild | JobKind::Repair)
                }) {
                    job.record.state = JobState::Succeeded;
                    job.record.outcome = Some(JobOutcome::Succeeded {
                        revision: Some(RevisionToken {
                            incarnation: receipt.token.incarnation,
                            generation: receipt.token.sequence,
                            revision: receipt.token.digest,
                        }),
                    });
                    rebuilt = true;
                } else if job.record.kind == JobKind::Configure {
                    if job
                        .configuration
                        .as_ref()
                        .is_some_and(|configuration| configuration.result.is_some())
                    {
                        job.record.state = JobState::Succeeded;
                        job.record.outcome = Some(JobOutcome::Succeeded { revision: None });
                    } else if job.record.attempts < 3 && super::native_configuration::derived(job) {
                        job.record.state = JobState::Pending;
                    } else {
                        job.record.state = JobState::Failed;
                        job.record.outcome = Some(JobOutcome::Failed {
                            code: "configuration_outcome_unknown_reinspect".into(),
                        });
                    }
                } else if matches!(job.record.kind, JobKind::Rebuild | JobKind::Repair)
                    || job.record.attempts >= 3
                {
                    job.record.state = JobState::Failed;
                    job.record.outcome = Some(JobOutcome::Failed {
                        code: "interrupted_maintenance_or_retry_exhausted".into(),
                    });
                } else {
                    job.record.state = JobState::Pending;
                }
                if job.record.state != JobState::Pending {
                    completed.push(job.record.id);
                }
            }
            if rebuilt {
                super::native_background::set(state, lease.binding, JobKind::Vectors, true)?;
            }
            for id in completed {
                super::native_background::completed(state, id, None)?;
                super::native_configuration::completed(state, id)?;
                super::native_maintenance::completed(
                    state,
                    id,
                    super::native_maintenance::clock_ms()?,
                    None,
                )?;
            }
            state.bump()
        })
    }
}
