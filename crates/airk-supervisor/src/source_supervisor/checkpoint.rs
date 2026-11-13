// SPDX-License-Identifier: Apache-2.0
//! Actual watcher exit and retained canonical reconciliation cursors.
use super::*;
pub(crate) use airk_host::checkpoint_packet::SourceCheckpointObservation;
use airk_host::checkpoint_packet::{ExitBoundary, ReconcileReference};

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
