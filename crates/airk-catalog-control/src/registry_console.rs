// SPDX-License-Identifier: Apache-2.0
//! Bounded console pages of cached registry observations (moved from
//! `service_management`); a damaged observation is reported, never fatal.
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;

fn observed<T>(
    value: Result<T>,
    kind: airk_core::service_console::ObservationKind,
    failed: &mut Vec<airk_core::service_console::ObservationKind>,
) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(_) => {
            failed.push(kind);
            None
        }
    }
}
pub fn console_page(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
) -> Result<airk_core::service_console::ConsolePage> {
    console_page_role(registry, after, revision, limit, now_ms, false)
}
pub fn console_page_role(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
    inactive: bool,
) -> Result<airk_core::service_console::ConsolePage> {
    use airk_core::service_console::{
        ConsolePage, MAX_PAGE_BYTES, ObservationKind as Kind, ProjectObservation,
    };
    let source = if inactive {
        registry.list_inactive(after, revision, limit)?
    } else {
        registry.list(after, revision, limit)?
    };
    let mut page = ConsolePage {
        revision: source.revision,
        observed_unix_ms: now_ms,
        cached: true,
        projects: Vec::new(),
        after: source.after,
    };
    let source_after = source.after;
    let mut overflow = false;
    for project in source.projects {
        let binding = project.registration.binding;
        let mut failed = Vec::new();
        let pending_job = observed(
            registry.pending_job_observation(binding),
            Kind::PendingJob,
            &mut failed,
        )
        .flatten();
        let pending_paused = project.paused
            && pending_job.as_ref().is_some_and(|job| {
                !observed(
                    registry.job_runs_while_paused(job.id),
                    Kind::PendingJob,
                    &mut failed,
                )
                .unwrap_or(true)
            });
        let configuration = observed(
            registry.latest_configuration(binding),
            Kind::Configuration,
            &mut failed,
        )
        .flatten();
        let preference = configuration
            .as_ref()
            .filter(|job| {
                matches!(
                    job.state,
                    airk_core::service::JobState::Pending | airk_core::service::JobState::Running
                )
            })
            .and_then(|job| {
                let work = observed(
                    registry.configuration_work(job.id),
                    Kind::Configuration,
                    &mut failed,
                )
                .flatten()?;
                use airk_core::service_configuration::ProjectConfiguration;
                let (vectors_enabled, detail) = match work.request.configuration {
                    ProjectConfiguration::Vectors { enabled } => (Some(enabled), None),
                    ProjectConfiguration::Detail { level } => (None, Some(level)),
                    _ => return None,
                };
                Some(airk_core::service_console::IndexingPreference {
                    job: job.id,
                    vectors_enabled,
                    detail,
                })
            });
        let row = ProjectObservation {
            pending_gate: observed(
                registry.pending_job_gate(binding),
                Kind::PendingJob,
                &mut failed,
            )
            .flatten(),
            preference,
            recovery: observed(
                registry.recovery_observations(binding),
                Kind::Recovery,
                &mut failed,
            )
            .unwrap_or_default(),
            pending_job,
            pending_paused,
            configuration,
            watch: observed(
                registry.watch(binding).map(|watch| watch.observation),
                Kind::Watch,
                &mut failed,
            ),
            progress: observed(
                registry.progress_observation(binding),
                Kind::Progress,
                &mut failed,
            )
            .flatten(),
            inspection: observed(registry.inspection(binding), Kind::Inspection, &mut failed)
                .flatten(),
            maintenance: observed(
                registry.maintenance_observation(binding),
                Kind::Maintenance,
                &mut failed,
            )
            .flatten(),
            maintenance_retrying: observed(
                registry.maintenance_retrying(binding),
                Kind::MaintenanceRetry,
                &mut failed,
            ),
            failed_observations: failed,
            project,
        };
        page.projects.push(row);
        page.after = Some(binding);
        if serde_json::to_vec(&page)?.len() > MAX_PAGE_BYTES {
            page.projects.pop();
            overflow = true;
            break;
        }
    }
    anyhow::ensure!(
        !overflow || !page.projects.is_empty(),
        "console project observation exceeds page budget"
    );
    page.after = if overflow || source_after.is_some() {
        page.projects
            .last()
            .map(|row| row.project.registration.binding)
    } else {
        None
    };
    Ok(page)
}
