// SPDX-License-Identifier: Apache-2.0
//! Durable source/semantic continuations and captured source-watermark receipts.
use super::{NativeServiceRegistry, State, enqueue_record, request_identity};
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, RequestKey},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Background {
    revision: u64,
    source: bool,
    semantic: bool,
}
pub(super) fn source_kind(kind: JobKind) -> bool {
    matches!(kind, JobKind::Index | JobKind::Reconcile | JobKind::Rebuild)
}
pub(super) fn validate(state: &State) -> Result<()> {
    ensure!(
        state.background.len() <= super::MAX_PROJECTS
            && state.source_outcomes.len() <= super::MAX_PROJECTS,
        "background metadata count exceeded"
    );
    for project in state.background.keys().chain(state.source_outcomes.keys()) {
        ensure!(
            state.projects.contains_key(project),
            "background project missing"
        );
    }
    for (project, (sequence, record)) in &state.source_outcomes {
        ensure!(
            project == &record.project.to_string()
                && *sequence <= state.next_job_sequence
                && record.outcome.is_some()
                && matches!(
                    record.state,
                    JobState::Succeeded | JobState::Failed | JobState::Canceled
                ),
            "invalid retained source outcome"
        );
    }
    Ok(())
}
pub(super) fn set(
    state: &mut State,
    project: ProjectBinding,
    kind: JobKind,
    pending: bool,
) -> Result<()> {
    ensure!(
        matches!(kind, JobKind::Index | JobKind::Vectors),
        "unsupported background continuation"
    );
    let background = state.background.entry(project.to_string()).or_default();
    let current = match kind {
        JobKind::Index => &mut background.source,
        _ => &mut background.semantic,
    };
    if *current != pending {
        *current = pending;
        background.revision = background
            .revision
            .checked_add(1)
            .context("background continuation revision exhausted")?;
    }
    Ok(())
}
pub(super) fn completed(state: &mut State, id: Sha256Digest, now_ms: Option<u64>) -> Result<()> {
    let job = state
        .jobs
        .get(&id.to_string())
        .context("completed job missing")?;
    let record = job.record.clone();
    if matches!(record.state, JobState::Pending | JobState::Running) {
        return Ok(());
    }
    let canceled = job.cancellation_requested;
    if let Some(revision) = job.source_revision {
        let watch = state
            .watches
            .get_mut(&record.project.to_string())
            .context("source watch missing")?;
        ensure!(
            revision <= watch.requested,
            "source completion revision exceeds desired state"
        );
        match &record.outcome {
            Some(JobOutcome::Succeeded { .. }) => {
                watch.completed = watch.completed.max(revision);
                if now_ms.is_some() {
                    watch.last_reconciled_ms = now_ms;
                }
            }
            Some(JobOutcome::Canceled) => {
                watch.blocked_revision = Some(watch.blocked_revision.unwrap_or(0).max(revision));
            }
            _ => {}
        }
        if state
            .source_outcomes
            .get(&record.project.to_string())
            .is_none_or(|(sequence, _)| *sequence < job.sequence)
        {
            state
                .source_outcomes
                .insert(record.project.to_string(), (job.sequence, record.clone()));
        }
    }
    if canceled {
        let next = match record.kind {
            JobKind::ModelSetup => Some(JobKind::Index),
            JobKind::Index
            | JobKind::Reconcile
            | JobKind::Rebuild
            | JobKind::Repair
            | JobKind::Recover
            | JobKind::Vectors => Some(JobKind::Vectors),
            _ => None,
        };
        if let Some(kind) = next {
            set(state, record.project, kind, false)?;
        }
    }
    Ok(())
}
fn idle(state: &State, project: ProjectBinding) -> bool {
    !state.jobs.values().any(|job| {
        job.record.project == project
            && matches!(job.record.state, JobState::Pending | JobState::Running)
    })
}
impl NativeServiceRegistry {
    pub fn set_background_pending(
        &mut self,
        lease: &ProjectLease,
        kind: JobKind,
        pending: bool,
    ) -> Result<()> {
        self.write(|state| {
            super::observations::worker_epoch(state, &self.root, lease)?;
            set(state, lease.binding, kind, pending)?;
            state.bump()
        })
    }
    pub fn due_background_jobs(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64, JobKind)>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid background continuation page bound"
        );
        self.read(|state| {
            let mut due = Vec::new();
            for (id, background) in &state.background {
                let project: ProjectBinding = id.parse()?;
                let registered = state
                    .projects
                    .get(id)
                    .context("background project missing")?;
                if after.is_none_or(|after| project > after)
                    && registered.registered
                    && !registered.paused
                    && (background.source || background.semantic)
                    && idle(state, project)
                {
                    due.push((
                        project,
                        background.revision,
                        if background.source {
                            JobKind::Index
                        } else {
                            JobKind::Vectors
                        },
                    ));
                    if due.len() == limit {
                        break;
                    }
                }
            }
            Ok(due)
        })
    }
    pub fn has_background_work(&self) -> Result<bool> {
        self.read(|state| {
            Ok(state.background.iter().any(|(id, background)| {
                state
                    .projects
                    .get(id)
                    .is_some_and(|project| project.registered && !project.paused)
                    && (background.source || background.semantic)
            }))
        })
    }
    pub fn enqueue_background_job(
        &mut self,
        project: ProjectBinding,
        revision: u64,
        kind: JobKind,
        caller: Incarnation,
        nonce: Incarnation,
    ) -> Result<JobRecord> {
        ensure!(
            matches!(kind, JobKind::Index | JobKind::Vectors),
            "unsupported background continuation"
        );
        let (namespace, epoch) = self.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce,
        };
        self.prepare_job_request(caller, &key, None)?;
        self.write(|state| {
            if state
                .accepted_requests
                .contains_key(&request_identity(caller, &key))
            {
                return enqueue_record(state, project, caller, &key, kind);
            }
            let registered = state
                .projects
                .get(&project.to_string())
                .context("background project missing")?;
            let background = state
                .background
                .get(&project.to_string())
                .context("background continuation missing")?;
            ensure!(
                registered.registered
                    && !registered.paused
                    && background.revision == revision
                    && match kind {
                        JobKind::Index => background.source,
                        _ => background.semantic,
                    }
                    && idle(state, project),
                "background continuation is no longer eligible"
            );
            let record = enqueue_record(state, project, caller, &key, kind)?;
            set(state, project, kind, false)?;
            Ok(record)
        })
    }
    pub fn enqueue_source_reconcile(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        nonce: Incarnation,
        revision: u64,
        now_ms: u64,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce,
        };
        self.prepare_job_request(caller, &key, None)?;
        self.write(|state| {
            if state
                .accepted_requests
                .contains_key(&request_identity(caller, &key))
            {
                return enqueue_record(state, project, caller, &key, JobKind::Reconcile);
            }
            let registered = state
                .projects
                .get(&project.to_string())
                .context("source project missing")?;
            let watch = state
                .watches
                .get(&project.to_string())
                .context("source watch missing")?;
            ensure!(
                registered.registered
                    && !registered.paused
                    && watch.enabled
                    && watch.requested == revision
                    && revision > watch.completed
                    && watch.attempts < 3
                    && watch
                        .blocked_revision
                        .is_none_or(|blocked| blocked < revision)
                    && watch.next_ms <= now_ms
                    && idle(state, project),
                "source reconciliation is no longer eligible"
            );
            let next = now_ms
                .checked_add((1u64 << (watch.attempts + 1)) * 1000)
                .context("source retry deadline exhausted")?;
            let record = enqueue_record(state, project, caller, &key, JobKind::Reconcile)?;
            let watch = state.watches.get_mut(&project.to_string()).unwrap();
            watch.attempts += 1;
            watch.next_ms = next;
            Ok(record)
        })
    }
    pub fn due_source_projects(
        &self,
        after: Option<ProjectBinding>,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64)>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid source admission page limit"
        );
        self.read(|state| {
            let mut due = Vec::new();
            for (id, watch) in &state.watches {
                let project = id.parse()?;
                let registered = state.projects.get(id).context("source project missing")?;
                if after.is_none_or(|after| project > after)
                    && registered.registered
                    && !registered.paused
                    && watch.enabled
                    && watch.requested > watch.completed
                    && watch.attempts < 3
                    && watch
                        .blocked_revision
                        .is_none_or(|blocked| blocked < watch.requested)
                    && watch.next_ms <= now_ms
                    && idle(state, project)
                {
                    due.push((project, watch.requested));
                    if due.len() == limit {
                        break;
                    }
                }
            }
            Ok(due)
        })
    }
}
