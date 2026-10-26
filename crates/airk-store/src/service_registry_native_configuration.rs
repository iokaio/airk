// SPDX-License-Identifier: Apache-2.0
//! Immutable typed configuration work and fenced durable result capture.
use super::{NativeJob, NativeServiceRegistry, State, enqueue_typed, observations::worker_epoch};
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobError, JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, RequestKey},
    service_cleanup::DerivedCleanupReport,
    service_configuration::{
        ConfigurationRequest, ConfigurationWork, MAX_CONFIGURATION_BYTES, ProjectConfiguration,
    },
    service_watch::WatchBackend,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    pub work: ConfigurationWork,
    pub result: Option<serde_json::Value>,
}
pub(super) fn validate(state: &State) -> Result<()> {
    let mut reserved = 0usize;
    for job in state.jobs.values() {
        ensure!(
            (job.record.kind == JobKind::Configure) == job.configuration.is_some(),
            "missing or foreign typed configuration"
        );
        if let Some(configuration) = &job.configuration {
            configuration.work.request.validate()?;
            ensure!(
                serde_json::to_vec(&configuration.work)?.len() <= MAX_CONFIGURATION_BYTES + 256,
                "configuration work exceeds job budget"
            );
            if let Some(result) = &configuration.result {
                ensure!(
                    serde_json::to_vec(result)?.len() <= MAX_CONFIGURATION_BYTES,
                    "configuration result exceeds budget"
                );
            } else {
                reserved = reserved
                    .checked_add(MAX_CONFIGURATION_BYTES)
                    .context("configuration reservation overflow")?;
            }
        }
    }
    ensure!(
        serde_json::to_vec(state)?.len().saturating_add(reserved) <= super::STATE_BYTES,
        "configuration result capacity exceeds metadata budget"
    );
    Ok(())
}
pub(super) fn derived(job: &NativeJob) -> bool {
    job.configuration.as_ref().is_some_and(|configuration| {
        configuration
            .work
            .request
            .configuration
            .is_derived_cleanup()
    })
}
pub(super) fn runs_while_paused(job: &NativeJob) -> bool {
    matches!(job.record.kind, JobKind::Inspect | JobKind::PlanInspect)
        || job.configuration.as_ref().is_some_and(|configuration| {
            configuration
                .work
                .request
                .configuration
                .is_derived_cleanup()
                || configuration.work.request.configuration == ProjectConfiguration::Unregister
        })
}
pub(super) fn completed(state: &mut State, id: Sha256Digest) -> Result<()> {
    let job = state
        .jobs
        .get(&id.to_string())
        .context(JobError::UnknownJob)?;
    if !matches!(job.record.outcome, Some(JobOutcome::Succeeded { .. })) {
        return Ok(());
    }
    let Some(configuration) = job.configuration.clone() else {
        return Ok(());
    };
    let project = job.record.project;
    match configuration.work.request.configuration {
        ProjectConfiguration::Unregister => {
            ensure!(
                configuration.result == Some(serde_json::json!({"unregistration_requested": true})),
                "unregister preparation receipt is missing or changed"
            );
            let entry = state
                .projects
                .get_mut(&project.to_string())
                .context("project missing")?;
            ensure!(entry.registered, "project is already unregistered");
            entry.registered = false;
            entry.paused = true;
            let mut canceled = Vec::new();
            for job in state.jobs.values_mut().filter(|job| {
                job.record.project == project
                    && job.record.id != id
                    && matches!(job.record.state, JobState::Pending | JobState::Running)
            }) {
                job.record.state = JobState::Canceled;
                job.record.outcome = Some(JobOutcome::Canceled);
                canceled.push(job.record.id);
            }
            for id in canceled {
                super::native_background::completed(state, id, None)?;
            }
            if let Some(watch) = state.watches.get_mut(&project.to_string()) {
                watch.enabled = false;
                watch.native_roots = 0;
                watch.backend = WatchBackend::Disabled;
            }
            super::native_background::set(state, project, JobKind::Index, false)?;
            super::native_background::set(state, project, JobKind::Vectors, false)?;
            super::native_maintenance::block_automatic(state, project);
            state.request_epoch = state
                .request_epoch
                .checked_add(1)
                .context("request epoch exhausted during unregister")?;
            state
                .jobs
                .get_mut(&id.to_string())
                .unwrap()
                .configuration
                .as_mut()
                .unwrap()
                .result = Some(
                serde_json::json!({"unregistered":true,"sources_preserved":true,"settings_preserved":true,"derived_state_retained":true,"worker_drain":"observed separately"}),
            );
        }
        ProjectConfiguration::DeleteDerived { preview } => {
            let report: DerivedCleanupReport = serde_json::from_value(
                configuration
                    .result
                    .context("cleanup completion receipt missing")?,
            )?;
            report.validate()?;
            ensure!(
                report.complete && report.guard == preview,
                "cleanup is not complete for this approval"
            );
            let entry = state
                .projects
                .get_mut(&project.to_string())
                .context("project missing")?;
            ensure!(
                !entry.registered && entry.cleanup_guard == Some(preview),
                "cleanup barrier differs from completion"
            );
            entry.cleanup_guard = None;
        }
        _ => {}
    }
    Ok(())
}
impl NativeServiceRegistry {
    pub fn enqueue_configuration(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        request: &ConfigurationRequest,
        engine: Sha256Digest,
    ) -> Result<JobRecord> {
        request.validate()?;
        let work = ConfigurationWork {
            engine,
            request: request.clone(),
        };
        self.prepare_job_request(caller, key, None)?;
        self.write(|state| {
            enqueue_typed(state, project, caller, key, JobKind::Configure, Some(&work))
        })
    }
    pub fn latest_configuration(&self, project: ProjectBinding) -> Result<Option<JobRecord>> {
        self.read(|state| {
            Ok(state
                .jobs
                .values()
                .filter(|job| {
                    job.record.project == project && job.record.kind == JobKind::Configure
                })
                .max_by_key(|job| job.sequence)
                .map(|job| job.record.clone()))
        })
    }
    pub fn configuration_result(&self, id: Sha256Digest) -> Result<Option<serde_json::Value>> {
        self.read(|state| {
            Ok(state
                .jobs
                .get(&id.to_string())
                .context(JobError::UnknownJob)?
                .configuration
                .as_ref()
                .context("configuration work missing")?
                .result
                .clone())
        })
    }
    pub fn configuration_work(&self, id: Sha256Digest) -> Result<Option<ConfigurationWork>> {
        self.read(|state| {
            Ok(state
                .jobs
                .get(&id.to_string())
                .and_then(|job| job.configuration.as_ref())
                .map(|configuration| configuration.work.clone()))
        })
    }
    pub fn record_configuration_result(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        result: &serde_json::Value,
    ) -> Result<()> {
        ensure!(
            serde_json::to_vec(result)?.len() <= MAX_CONFIGURATION_BYTES,
            "configuration result exceeds budget"
        );
        self.write(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            let job = state
                .jobs
                .get_mut(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == lease.binding
                    && job.record.kind == JobKind::Configure
                    && job.record.state == JobState::Running
                    && job.record.fencing_generation == epoch,
                JobError::StaleWorker
            );
            let configuration = job
                .configuration
                .as_mut()
                .context("configuration work missing")?;
            ensure!(
                configuration.result.is_none(),
                "configuration result was already recorded"
            );
            configuration.result = Some(result.clone());
            if configuration
                .work
                .request
                .configuration
                .requests_source_pass()
            {
                super::native_background::set(state, lease.binding, JobKind::Index, true)?;
            }
            state.bump()
        })
    }
    pub fn derived_work_pending(&self, project: ProjectBinding) -> Result<bool> {
        self.read(|state| {
            Ok(state.jobs.values().any(|job| {
                job.record.project == project
                    && matches!(job.record.state, JobState::Pending | JobState::Running)
                    && derived(job)
            }))
        })
    }
    pub fn validate_active_worker(
        &self,
        lease: &ProjectLease,
    ) -> Result<airk_core::service::ProjectRegistration> {
        ensure!(
            !lease.derived,
            "fresh setup requires registered worker ownership"
        );
        self.read(|state| {
            worker_epoch(state, &self.root, lease)?;
            let entry = state
                .projects
                .get(&lease.binding.to_string())
                .context("project missing")?;
            ensure!(entry.registered, "project is unregistered");
            Ok(entry.registration.clone())
        })
    }
    pub fn validate_inactive_worker(
        &self,
        lease: &ProjectLease,
    ) -> Result<airk_core::service::ProjectRegistration> {
        ensure!(
            lease.derived,
            "inactive collection access requires the private cleanup owner"
        );
        let registration = self.read(|state| {
            worker_epoch(state, &self.root, lease)?;
            let project = state
                .projects
                .get(&lease.binding.to_string())
                .context("project missing")?;
            ensure!(
                !project.registered,
                "derived cleanup requires an unregistered project"
            );
            Ok(project.registration.clone())
        })?;
        crate::service::data_scope::check(
            std::path::Path::new(&registration.data_root),
            &self.root,
            self.incarnation,
        )?;
        Ok(registration)
    }
    pub fn begin_derived_cleanup(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        guard: Sha256Digest,
    ) -> Result<()> {
        self.validate_inactive_worker(lease)?;
        self.write(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            let job = state
                .jobs
                .get(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == lease.binding
                    && job.record.fencing_generation == epoch
                    && job.record.state == JobState::Running,
                "cleanup job is not owned by this worker"
            );
            let configuration = job
                .configuration
                .as_ref()
                .context("cleanup configuration is missing")?;
            ensure!(
                configuration.work.request.configuration
                    == ProjectConfiguration::DeleteDerived { preview: guard },
                "cleanup approval differs from the accepted request"
            );
            let entry = state
                .projects
                .get_mut(&lease.binding.to_string())
                .context("project missing")?;
            ensure!(
                !entry.registered && entry.cleanup_guard.is_none_or(|prior| prior == guard),
                "another cleanup must finish before this approval can run"
            );
            entry.cleanup_guard = Some(guard);
            state.bump()
        })
    }
}
