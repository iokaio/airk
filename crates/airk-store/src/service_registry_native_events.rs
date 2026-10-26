// SPDX-License-Identifier: Apache-2.0
//! Content-free events derived from typed metadata transitions in the commit.
use super::{NativeServiceRegistry, State};
use airk_core::{
    service::{JobRecord, JobState, ProjectBinding},
    service_events::{EventCursor, EventError, EventKind, EventPage, ServiceEvent},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Events {
    sequence: u64,
    records: VecDeque<ServiceEvent>,
}
impl Events {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.records.len() <= 1024,
            "service event retention bound exceeded"
        );
        let mut previous = 0;
        for event in &self.records {
            ensure!(
                event.sequence > previous
                    && event.sequence <= self.sequence
                    && serde_json::to_vec(event)?.len() <= 1024,
                "invalid service event record"
            );
            previous = event.sequence;
        }
        Ok(())
    }
    fn push(&mut self, mut event: ServiceEvent) -> Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("service event sequence exhausted")?;
        event.sequence = self.sequence;
        self.records.push_back(event);
        while self.records.len() > 1024 {
            self.records.pop_front();
        }
        Ok(())
    }
}
pub(super) struct Baseline {
    retry_receipts: std::collections::BTreeSet<String>,
    projects: BTreeMap<String, (bool, bool)>,
    watches: BTreeMap<String, bool>,
    jobs: BTreeMap<String, (JobRecord, bool)>,
}
impl Baseline {
    pub(super) fn capture(state: &State) -> Self {
        Self {
            retry_receipts: state.retry_origins.keys().cloned().collect(),
            projects: state
                .projects
                .iter()
                .map(|(key, value)| (key.clone(), (value.registered, value.paused)))
                .collect(),
            watches: state
                .watches
                .iter()
                .map(|(key, value)| (key.clone(), value.enabled))
                .collect(),
            jobs: state
                .jobs
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        (value.record.clone(), value.cancellation_requested),
                    )
                })
                .collect(),
        }
    }
}
fn event(
    project: ProjectBinding,
    kind: EventKind,
    now_ms: u64,
    job: Option<&JobRecord>,
) -> ServiceEvent {
    ServiceEvent {
        sequence: 0,
        observed_unix_ms: now_ms,
        project,
        kind,
        job: job.map(|job| job.id),
        job_kind: job.map(|job| job.kind),
        fencing_generation: job.map(|job| job.fencing_generation),
        attempts: job.map(|job| job.attempts),
        code: job.and_then(|job| match &job.outcome {
            Some(airk_core::service::JobOutcome::Failed { code }) => Some(code.clone()),
            _ => None,
        }),
    }
}
pub(super) fn record_changes(prior: Baseline, state: &mut State) -> Result<()> {
    let now_ms = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?;
    for (id, project) in &state.projects {
        let prior = prior.projects.get(id);
        if project.registered && prior.is_none_or(|(registered, _)| !registered) {
            state.events.push(event(
                project.registration.binding,
                EventKind::Registered,
                now_ms,
                None,
            ))?;
        }
        if let Some((_, paused)) = prior
            && *paused != project.paused
        {
            state.events.push(event(
                project.registration.binding,
                if project.paused {
                    EventKind::Paused
                } else {
                    EventKind::Resumed
                },
                now_ms,
                None,
            ))?;
        }
    }
    for (id, watch) in &state.watches {
        if prior.watches.get(id).copied().unwrap_or(false) != watch.enabled {
            state.events.push(event(
                id.parse()?,
                if watch.enabled {
                    EventKind::WatchEnabled
                } else {
                    EventKind::WatchDisabled
                },
                now_ms,
                None,
            ))?;
        }
    }
    for receipt in state
        .retry_origins
        .keys()
        .filter(|key| !prior.retry_receipts.contains(*key))
    {
        let job = state
            .accepted_requests
            .get(receipt)
            .and_then(|id| state.jobs.get(id))
            .context("retry receipt lost its accepted job")?;
        state.events.push(event(
            job.record.project,
            EventKind::JobRetryRequested,
            now_ms,
            Some(&job.record),
        ))?;
    }
    // Sequence order, rather than hash-key order, preserves FIFO event order
    // when one recovery transaction settles multiple interrupted jobs.
    let mut jobs: Vec<_> = state.jobs.iter().collect();
    jobs.sort_by_key(|(_, job)| job.sequence);
    for (id, job) in jobs {
        let previous = prior.jobs.get(id);
        if previous.is_none_or(|(old, _)| {
            old.state != job.record.state
                || old.fencing_generation != job.record.fencing_generation
                || old.attempts != job.record.attempts
        }) {
            let kind = match job.record.state {
                JobState::Pending => EventKind::JobPending,
                JobState::Running => EventKind::JobRunning,
                JobState::Succeeded => EventKind::JobSucceeded,
                JobState::Failed => EventKind::JobFailed,
                JobState::Canceled => EventKind::JobCanceled,
            };
            state
                .events
                .push(event(job.record.project, kind, now_ms, Some(&job.record)))?;
        }
        if job.cancellation_requested && previous.is_none_or(|(_, canceled)| !canceled) {
            state.events.push(event(
                job.record.project,
                EventKind::CancellationRequested,
                now_ms,
                Some(&job.record),
            ))?;
        }
    }
    Ok(())
}
impl NativeServiceRegistry {
    pub fn events(
        &self,
        project: Option<ProjectBinding>,
        cursor: Option<&EventCursor>,
        limit: usize,
    ) -> Result<EventPage> {
        ensure!((1..=32).contains(&limit), EventError::InvalidLimit);
        self.read(|state| {
            if let Some(project) = project {
                ensure!(
                    state
                        .projects
                        .get(&project.to_string())
                        .is_some_and(|project| project.registered),
                    "service project is not registered"
                );
            }
            let namespace = state.request_namespace;
            let oldest = state.events.records.front().map(|event| event.sequence);
            let newest = state.events.sequence;
            let (after, through) = match cursor {
                Some(cursor) => {
                    ensure!(
                        cursor.namespace == namespace && cursor.project == project,
                        EventError::ScopeChanged
                    );
                    let through = cursor.through.unwrap_or(newest);
                    ensure!(
                        cursor.after <= through && through <= newest,
                        EventError::InvalidCursor
                    );
                    ensure!(
                        oldest.is_none_or(|floor| cursor.after >= floor.saturating_sub(1)),
                        EventError::CursorExpired
                    );
                    (cursor.after, through)
                }
                None => (oldest.unwrap_or(1).saturating_sub(1), newest),
            };
            let mut events: Vec<_> = state
                .events
                .records
                .iter()
                .filter(|event| {
                    event.sequence > after
                        && event.sequence <= through
                        && project.is_none_or(|project| project == event.project)
                })
                .take(limit + 1)
                .cloned()
                .collect();
            let next = if events.len() > limit {
                events.pop();
                Some(EventCursor {
                    namespace,
                    project,
                    after: events.last().context("empty event page")?.sequence,
                    through: Some(through),
                })
            } else {
                None
            };
            let page = EventPage {
                namespace,
                oldest_sequence: oldest,
                snapshot_through: through,
                retained_prefix_lost: cursor.is_none() && oldest.is_some_and(|floor| floor > 1),
                events,
                next,
                tail: EventCursor {
                    namespace,
                    project,
                    after: through,
                    through: None,
                },
            };
            ensure!(
                serde_json::to_vec(&page)?.len() <= 48 * 1024,
                "service event page exceeds byte bound"
            );
            Ok(page)
        })
    }
}
