// SPDX-License-Identifier: Apache-2.0
//! Actual watcher exit and retained canonical reconciliation cursors.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileReference {
    pub project: ProjectBinding,
    pub configuration_epoch: u64,
    pub requested_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExitBoundary {
    GracefulChildExit,
    NoActiveWatcherAfterNativeExclusion,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceCheckpointObservation {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub service: Incarnation,
    pub worker: Option<Incarnation>,
    pub process_id: Option<u32>,
    pub exit_boundary: ExitBoundary,
    pub source_lease_excluded: bool,
    pub targets: Vec<protocol::CheckpointTarget>,
    pub retained_reconciliations: Vec<ReconcileReference>,
}
impl SourceCheckpointObservation {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.source_lease_excluded
                && self.targets.len() <= 1024
                && self.retained_reconciliations.len() <= 1024,
            "source checkpoint barrier incomplete or oversized"
        );
        ensure!(
            match self.exit_boundary {
                ExitBoundary::GracefulChildExit =>
                    self.worker.is_some() && self.process_id.is_some_and(|pid| pid > 0),
                ExitBoundary::NoActiveWatcherAfterNativeExclusion =>
                    self.worker.is_none() && self.process_id.is_none() && self.targets.is_empty(),
            },
            "source checkpoint exit identity changed"
        );
        let mut projects = BTreeSet::new();
        for target in &self.targets {
            ensure!(
                target.sequence > 0
                    && target.sequence <= i64::MAX as u64
                    && projects.insert(target.project),
                "source checkpoint target duplicated or invalid"
            );
        }
        projects.clear();
        for target in &self.retained_reconciliations {
            ensure!(
                projects.insert(target.project),
                "source checkpoint reconciliation duplicated"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 256 * 1024,
            "source checkpoint receipt capacity exceeded"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Barrier {
    pub operation_id: Incarnation,
    pub targets: Vec<protocol::CheckpointTarget>,
}
#[derive(Default)]
pub(super) struct Progress {
    sent: bool,
    completed: Option<SourceCheckpointObservation>,
    retained: BTreeMap<ProjectBinding, ReconcileReference>,
    exited: Option<(Incarnation, u32, Vec<protocol::CheckpointTarget>)>,
}
pub(super) enum Step {
    Pending,
    ContinueAcceptedCleanup,
    Complete(SourceCheckpointObservation),
}

impl WatchChild {
    fn checkpoint_exit(&mut self, operation: Incarnation) -> Result<Option<Barrier>> {
        let Some(status) = self.process.try_wait()? else {
            return Ok(None);
        };
        ensure!(
            status.success(),
            "source checkpoint watcher exited unsuccessfully"
        );
        self.sender.take();
        if self
            .writer
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            || self
                .reader
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
        {
            return Ok(None);
        }
        if let Some(writer) = self.writer.take() {
            writer
                .join()
                .map_err(|_| anyhow::anyhow!("source checkpoint writer panicked"))?;
        }
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("source checkpoint reader panicked"))?;
        }
        let replies = self
            .replies
            .lock()
            .map_err(|_| anyhow::anyhow!("source checkpoint replies unavailable"))?;
        let barrier = replies
            .checkpoint
            .as_ref()
            .context("source watcher exited without checkpoint barrier")?;
        ensure!(
            barrier.operation_id == operation && barrier.targets.len() <= 1024,
            "source checkpoint operation crossed"
        );
        Ok(Some(barrier.clone()))
    }
}

fn retain_reconciliations(
    registry: &mut ServiceRegistry,
    progress: &mut Progress,
) -> Result<Vec<ReconcileReference>> {
    let mut retained = Vec::new();
    for project in registry.bindings()? {
        if !registry.is_registered(project)? {
            continue;
        }
        let watch = registry.watch(project)?;
        if !watch.observation.enabled {
            continue;
        }
        if let Some(saved) = progress.retained.get(&project)
            && saved.configuration_epoch == watch.configuration_epoch
        {
            retained.push(saved.clone());
            continue;
        }
        // Reinstallation can reset a watcher's local sequence while its SQL
        // duplicate cursor remains higher. Retain an explicit upgrade catch-up
        // intent for every enabled scope, independently of notification dedup.
        let requested = watch
            .observation
            .requested_revision
            .checked_add(1)
            .context("source checkpoint revision exhausted")?;
        registry.source_changed(project, watch.configuration_epoch)?;
        let reference = ReconcileReference {
            project,
            configuration_epoch: watch.configuration_epoch,
            requested_revision: requested,
        };
        progress.retained.insert(project, reference.clone());
        retained.push(reference);
    }
    Ok(retained)
}

/// Never force-kill a watcher or infer its exit from a timeout. The normal
/// service teardown remains available for an explicitly aborted upgrade.
pub(super) fn step(
    root: &Path,
    service: Incarnation,
    operation: Incarnation,
    child: &mut Option<WatchChild>,
    registry: &mut Option<ServiceRegistry>,
    progress: &mut Progress,
) -> Result<Step> {
    if let Some(completed) = &progress.completed {
        drop(SourceWatchLease::acquire(root)?);
        return Ok(Step::Complete(completed.clone()));
    }
    if registry.is_none() {
        *registry = Some(ServiceRegistry::open(root, service)?);
    }
    let registry = registry
        .as_mut()
        .context("source checkpoint registry unavailable")?;
    if !registry.pending_project_removals(None, 1)?.is_empty() {
        return Ok(Step::ContinueAcceptedCleanup);
    }
    let mut targets = Vec::new();
    let (worker, process_id, exit_boundary) = if let Some(active) = child.as_mut() {
        if !progress.sent {
            // Mark uncertainty before the writer can consume any command bytes.
            progress.sent = true;
            active.send(vec![Command::Checkpoint {
                service,
                worker: active.worker,
                operation_id: operation,
            }])?;
            return Ok(Step::Pending);
        }
        let Some(barrier) = active.checkpoint_exit(operation)? else {
            return Ok(Step::Pending);
        };
        let worker = active.worker;
        let pid = active.host_process_id();
        targets = barrier.targets;
        let replies = active
            .replies
            .lock()
            .map_err(|_| anyhow::anyhow!("source checkpoint replies unavailable"))?;
        for target in &targets {
            ensure!(
                replies
                    .events
                    .get(&target.project)
                    .is_some_and(|(epoch, sequence)| *epoch == target.configuration_epoch
                        && *sequence >= target.sequence),
                "source checkpoint lost a forced reconciliation notice"
            );
        }
        // Exact native watcher identity and existing epoch/duplicate checks.
        for (project, (epoch, sequence)) in &replies.events {
            registry.source_event(*project, *epoch, worker, *sequence)?;
        }
        drop(replies);
        // Native exit and both pipe-thread joins have already completed.
        progress.exited = Some((worker, pid, targets.clone()));
        drop(child.take());
        (Some(worker), Some(pid), ExitBoundary::GracefulChildExit)
    } else if let Some((worker, pid, saved_targets)) = &progress.exited {
        targets = saved_targets.clone();
        (Some(*worker), Some(*pid), ExitBoundary::GracefulChildExit)
    } else {
        (
            None,
            None,
            ExitBoundary::NoActiveWatcherAfterNativeExclusion,
        )
    };
    let exclusion = SourceWatchLease::acquire(root)?;
    let retained_reconciliations = retain_reconciliations(registry, progress)?;
    let completed = SourceCheckpointObservation {
        schema: 1,
        operation_id: operation,
        service,
        worker,
        process_id,
        exit_boundary,
        source_lease_excluded: true,
        targets,
        retained_reconciliations,
    };
    completed.validate()?;
    progress.completed = Some(completed.clone());
    drop(exclusion);
    Ok(Step::Complete(completed))
}
