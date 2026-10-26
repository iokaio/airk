// SPDX-License-Identifier: Apache-2.0
//! Source observation rosters and freshness from one bounded native snapshot.
use super::{NativeServiceRegistry, observations::worker_epoch, watch_project};
use crate::service::ProjectLease;
use airk_core::{
    changes::{FreshnessObservation, FreshnessState},
    service::{JobOutcome, JobState, ProjectBinding},
    service_watch::{WatchBackend, WatchFailure},
};
use anyhow::{Context, Result, ensure};

impl NativeServiceRegistry {
    pub fn reset_native_watch_observations(&mut self) -> Result<()> {
        self.write(|state| {
            for watch in state.watches.values_mut() {
                watch.native_roots = 0;
                watch.backend = if watch.enabled {
                    WatchBackend::PeriodicFallback
                } else {
                    WatchBackend::Disabled
                };
                if watch.failure != Some(WatchFailure::MetadataUnavailable) {
                    watch.failure = watch.enabled.then_some(WatchFailure::BackendUnavailable);
                }
                watch.observed_ms = None;
            }
            state.bump()
        })
    }
    pub fn watch_projects(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<super::super::WatchProject>> {
        ensure!((1..=32).contains(&limit), "invalid watch roster page limit");
        self.read(|state| {
            let mut page = Vec::new();
            let mut bytes = 0;
            for entry in state
                .projects
                .values()
                .filter(|entry| {
                    entry.registered && after.is_none_or(|after| entry.registration.binding > after)
                })
                .take(limit)
            {
                let project = watch_project(state, entry.registration.binding)?;
                let weight = project
                    .additional_roots
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    + serde_json::to_vec(&project.project)?.len()
                    + 512;
                if bytes + weight > 512 * 1024 {
                    break;
                }
                bytes += weight;
                page.push(project);
            }
            Ok(page)
        })
    }
    pub fn invalidate_watch_roots(&mut self, lease: &ProjectLease) -> Result<()> {
        self.write(|state| {
            worker_epoch(state, &self.root, lease)?;
            let watch = state.watches.entry(lease.binding.to_string()).or_default();
            if watch.roots_observed || watch.failure != Some(WatchFailure::MetadataUnavailable) {
                watch.roots_observed = false;
                watch.native_roots = 0;
                watch.backend = if watch.enabled {
                    WatchBackend::PeriodicFallback
                } else {
                    WatchBackend::Disabled
                };
                watch.failure = Some(WatchFailure::MetadataUnavailable);
                watch.configuration_epoch = watch
                    .configuration_epoch
                    .checked_add(1)
                    .context("source configuration epoch exhausted")?;
                if watch.enabled {
                    watch.requested = watch
                        .requested
                        .checked_add(1)
                        .context("source revision exhausted")?;
                }
                state.bump()?;
            }
            Ok(())
        })
    }
    pub fn record_watch_observation(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
        native_roots: u16,
        backend: WatchBackend,
        failure: Option<WatchFailure>,
        now_ms: u64,
    ) -> Result<()> {
        self.write(|state| {
            let watch = state
                .watches
                .get_mut(&project.to_string())
                .context("source watch missing")?;
            ensure!(
                watch.configuration_epoch == configuration_epoch,
                "source watcher configuration changed"
            );
            watch.native_roots = native_roots;
            watch.backend = backend;
            watch.failure = failure;
            watch.observed_ms = Some(now_ms);
            watch_project(state, project)?.observation.validate()?;
            Ok(())
        })
    }
    pub fn source_freshness(
        &self,
        project: ProjectBinding,
        now_ms: u64,
    ) -> Result<FreshnessObservation> {
        self.read(|state| {
            let watched = watch_project(state, project)?;
            let source = state
                .jobs
                .values()
                .filter(|job| job.record.project == project && job.source_revision.is_some());
            let mut latest = state
                .source_outcomes
                .get(&project.to_string())
                .map(|(sequence, record)| (*sequence, record));
            let mut pending = false;
            let mut running = false;
            let mut running_revision = None;
            for job in source {
                if latest.is_none_or(|(sequence, _)| sequence < job.sequence) {
                    latest = Some((job.sequence, &job.record));
                }
                pending |= job.record.state == JobState::Pending;
                if job.record.state == JobState::Running {
                    running = true;
                    running_revision = running_revision.max(job.source_revision);
                }
            }
            let terminal = latest.map(|(_, record)| record.state);
            let mut error = latest.and_then(|(_, record)| match &record.outcome {
                Some(JobOutcome::Failed { code }) => Some(code.clone()),
                Some(JobOutcome::Canceled) => Some("source_job_canceled".into()),
                _ => None,
            });
            let watch = watched.observation;
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
                        "source_retry_canceled".into()
                    } else {
                        "source_retries_exhausted".into()
                    }
                });
                FreshnessState::Blocked
            } else if running {
                FreshnessState::Running
            } else if pending {
                FreshnessState::Pending
            } else if terminal == Some(JobState::Failed) {
                FreshnessState::Failed
            } else if terminal == Some(JobState::Canceled) {
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
            Ok(FreshnessObservation {
                source_job: latest.map(|(_, record)| record.id),
                observed_unix_ms: now_ms,
                state,
                requested_watermark: Some(watch.requested_revision),
                completed_watermark: Some(watch.completed_revision),
                running_watermark: running_revision,
                last_success_unix_ms: watch.last_reconciled_unix_ms,
                error,
                live_source_verified: false,
                last_inventory_unix_ms: None,
                last_index: None,
            })
        })
    }
}
