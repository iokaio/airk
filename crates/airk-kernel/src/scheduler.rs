// SPDX-License-Identifier: Apache-2.0
//! Native scheduling shared by extension hosts and explicit command-line workers.
use crate::Kernel;
use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
use airk_ingest::IndexReport;
use airk_store::scheduler::{Queue, ScheduledJob, WorkerLock};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
pub struct RunOutcome {
    pub state: &'static str,
    pub collection: Option<String>,
    pub revision: Option<u64>,
    pub report: Option<IndexReport>,
    pub error: Option<String>,
    pub maintenance: Option<airk_store::retention::MaintenanceRun>,
}

#[derive(Serialize)]
pub struct ReconcileOutcome {
    pub state: &'static str,
    pub collection: Option<String>,
    pub inventory_revision: Option<u64>,
    pub requested_revision: Option<u64>,
    pub error: Option<String>,
}

fn now() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

#[derive(Debug)]
pub struct ScanPaused;
impl std::fmt::Display for ScanPaused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("scan paused; committed batches remain available")
    }
}
impl std::error::Error for ScanPaused {}

/// The caller exhausted its join budget; durable indexing has not failed.
#[derive(Debug)]
pub struct ScheduledIndexPending {
    pub revision: u64,
}
impl std::fmt::Display for ScheduledIndexPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "scheduled indexing wait ended; request {} remains queued; resume with schedule wait {}",
            self.revision, self.revision
        )
    }
}
impl std::error::Error for ScheduledIndexPending {}

impl Kernel {
    /// Advance one semantic batch for this collection only. Editor hosts use
    /// this after base indexing and on reopen, without installing a service or
    /// executing queued source scans for other workspace roots.
    pub fn run_scheduled_enrichment(&self) -> Result<RunOutcome> {
        self.run_scheduled_enrichment_bounded(1, None)
    }

    /// Reuse one model and worker for several durable batches, yielding after
    /// the time budget so other collections can acquire the scheduler worker.
    pub fn run_scheduled_enrichment_bounded(
        &self,
        max_batches: usize,
        budget: Option<std::time::Duration>,
    ) -> Result<RunOutcome> {
        ensure!(
            (1..=8).contains(&max_batches),
            "semantic batch limit exceeded"
        );
        ensure!(
            budget.is_none_or(|duration| duration <= Duration::from_secs(120)),
            "semantic time budget exceeded"
        );
        let queue = self.queue()?;
        let outcome = |state, error| RunOutcome {
            state,
            collection: Some(self.collection.clone()),
            revision: None,
            report: None,
            error,
            maintenance: None,
        };
        let Some(worker) = queue.worker_lock()? else {
            return Ok(outcome("busy", None));
        };
        let Some(job) = queue.status(&self.collection, &self.root_id())? else {
            return Ok(outcome("unregistered", None));
        };
        if job.paused {
            return Ok(outcome("paused", None));
        }
        if job.blocked {
            return Ok(outcome("blocked", job.error));
        }
        if job.requested_revision > job.completed_revision {
            return Ok(outcome("busy", None));
        }
        // Unlike a root-wide worker searching for eligible collections, an
        // editor request must surface a failure to read its selected index.
        self.status()?;
        Ok(self
            .continue_semantic(&queue, &worker, &[job], max_batches, budget)?
            .unwrap_or_else(|| outcome("idle", None)))
    }

    fn continue_semantic(
        &self,
        queue: &Queue,
        worker: &WorkerLock,
        jobs: &[ScheduledJob],
        max_batches: usize,
        budget: Option<std::time::Duration>,
    ) -> Result<Option<RunOutcome>> {
        use airk_core::readiness::SemanticState;
        for job in jobs.iter().filter(|job| {
            !job.paused && !job.blocked && job.requested_revision == job.completed_revision
        }) {
            let mut kernel =
                Kernel::open_collection(&job.root, Some(self.data_root.clone()), &job.collection)?;
            kernel.progress = self.progress.clone();
            let Ok(status) = kernel.status() else {
                continue;
            };
            if !matches!(
                status.semantic_readiness.state,
                SemanticState::Pending | SemanticState::Partial | SemanticState::Failed
            ) {
                continue;
            }
            let span = kernel.usage_span(UsageOperation::Index, ClientFamily::NativeScheduler);
            let result = kernel.resume_current_enrichment_batches_bounded(max_batches, budget);
            let logical = result
                .as_ref()
                .ok()
                .and_then(|(_, report)| serde_json::to_value(report).ok());
            let bytes = logical
                .as_ref()
                .and_then(|value| serde_json::to_vec(value).ok())
                .map_or(0, |bytes| bytes.len());
            kernel.finish_usage(
                span,
                logical.as_ref(),
                bytes,
                result.is_err(),
                ResponseRepresentation::LogicalResultJson,
            );
            return Ok(Some(match result {
                Ok((generation, report)) => RunOutcome {
                    state: "enriched",
                    collection: Some(job.collection.clone()),
                    revision: generation,
                    report: Some(report),
                    error: None,
                    maintenance: None,
                },
                Err(error)
                    if matches!(
                        error.downcast_ref::<std::fs::TryLockError>(),
                        Some(std::fs::TryLockError::WouldBlock)
                    ) =>
                {
                    RunOutcome {
                        state: "busy",
                        collection: Some(job.collection.clone()),
                        revision: None,
                        report: None,
                        error: None,
                        maintenance: None,
                    }
                }
                Err(error) => {
                    let message: String = format!("{error:#}").chars().take(4096).collect();
                    queue.block_semantic(worker, job, &message)?;
                    RunOutcome {
                        state: "semantic_failed",
                        collection: Some(job.collection.clone()),
                        revision: None,
                        report: None,
                        error: Some(message),
                        maintenance: None,
                    }
                }
            }));
        }
        Ok(None)
    }

    fn queue(&self) -> Result<Queue> {
        std::fs::create_dir_all(&self.data_root)?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        // Queue intents do not read current evidence or acquire its request
        // gate. The claimed worker validates ownership before touching state;
        // this also permits full requests for obsolete/interrupted indexes.
        Ok(Queue::new(&self.data_root))
    }

    pub fn schedule_enqueue(&self, full: bool) -> Result<ScheduledJob> {
        self.schedule_enqueue_selected(full, None, false)
    }

    pub fn schedule_enqueue_selected(
        &self,
        full: bool,
        paths: Option<&[String]>,
        coalesced: bool,
    ) -> Result<ScheduledJob> {
        self.queue()?.enqueue_selected(
            &self.collection,
            &self.root_id(),
            full,
            false,
            false,
            now()?,
            paths,
            coalesced,
        )
    }
    pub fn schedule_status(&self) -> Result<Option<ScheduledJob>> {
        self.queue()?.status(&self.collection, &self.root_id())
    }
    pub fn schedule_pause(&self, paused: bool) -> Result<Option<ScheduledJob>> {
        self.queue()?
            .pause(&self.collection, &self.root_id(), paused)
    }
    pub fn schedule_forget(&self) -> Result<()> {
        self.queue()?.forget(&self.collection, &self.root_id())
    }

    /// Foreground watch workers reconcile only already registered roots, at most
    /// one bounded inventory per call and once per five minutes for each root.
    pub fn reconcile_scheduled_once(&self) -> Result<ReconcileOutcome> {
        let queue = self.queue()?;
        let Some(worker) = queue.worker_lock()? else {
            return Ok(ReconcileOutcome {
                state: "busy",
                collection: None,
                inventory_revision: None,
                requested_revision: None,
                error: None,
            });
        };
        let Some(job) = queue.claim_scan(&worker, now()?, 300000)? else {
            return Ok(ReconcileOutcome {
                state: "idle",
                collection: None,
                inventory_revision: None,
                requested_revision: None,
                error: None,
            });
        };
        let measured = Kernel::open_collection_with_recovery(
            &job.root,
            Some(queue.data_dir().into()),
            &job.collection,
            !job.full,
        )
        .and_then(|kernel| {
            ensure!(
                kernel.root_id() == job.root,
                "registered workspace root identity changed"
            );
            kernel.coverage(true)
        });
        match measured {
            Ok(report) => {
                let changed = !report.indexed
                    || report.stale > 0
                    || report.missing > 0
                    || report.pending_removals > 0
                    || report.source_configuration_pending;
                let partial = (!report.scan_complete || report.extraction_failed > 0 || report.capacity_blocked > 0)
                    .then_some("coverage inventory incomplete or eligible files failed/blocked; inspect airk coverage");
                queue.finish_scan(&worker, &job, now()?, partial)?;
                // Pause may have arrived during the scan. Enqueue is harmless,
                // but leaving policy untouched ensures no automatic write follows.
                let requested = if changed {
                    Some(
                        queue
                            .enqueue(&job.collection, &job.root, false, false, true, now()?)?
                            .requested_revision,
                    )
                } else {
                    None
                };
                Ok(ReconcileOutcome {
                    state: if changed {
                        "queued"
                    } else if partial.is_some() {
                        "partial"
                    } else {
                        "current"
                    },
                    collection: Some(job.collection),
                    inventory_revision: Some(report.inventory_revision),
                    requested_revision: requested,
                    error: partial.map(str::to_owned),
                })
            }
            Err(error) => {
                let error = format!("{error:#}");
                queue.finish_scan(&worker, &job, now()?, Some(&error))?;
                Ok(ReconcileOutcome {
                    state: "failed",
                    collection: Some(job.collection),
                    inventory_revision: None,
                    requested_revision: None,
                    error: Some(error),
                })
            }
        }
    }

    /// Run at most one due collection under one machine/data-root worker lock.
    /// No thread, service, network or source traversal is started by a read call.
    pub fn run_scheduled_once(&self) -> Result<RunOutcome> {
        let queue = self.queue()?;
        let Some(worker) = queue.worker_lock()? else {
            return Ok(RunOutcome {
                state: "busy",
                collection: None,
                revision: None,
                report: None,
                error: None,
                maintenance: None,
            });
        };
        let Some(job) = queue.claim(&worker, now()?)? else {
            if let Some(outcome) =
                self.continue_semantic(&queue, &worker, &queue.list()?, 1, None)?
            {
                return Ok(outcome);
            }
            match self.continue_retention(&queue.list()?) {
                Ok(Some((collection, maintenance))) => {
                    return Ok(RunOutcome {
                        state: "maintained",
                        collection: Some(collection),
                        revision: None,
                        report: None,
                        error: maintenance.receipt.error.clone(),
                        maintenance: Some(maintenance),
                    });
                }
                Err(error) => {
                    return Ok(RunOutcome {
                        state: "maintenance_failed",
                        collection: None,
                        revision: None,
                        report: None,
                        error: Some(format!("{error:#}").chars().take(4096).collect()),
                        maintenance: None,
                    });
                }
                Ok(None) => {}
            }
            return Ok(RunOutcome {
                state: "idle",
                collection: None,
                revision: None,
                report: None,
                error: None,
                maintenance: None,
            });
        };
        let result = Kernel::open_collection_with_recovery(
            &job.root,
            Some(queue.data_dir().into()),
            &job.collection,
            !job.full,
        )
        .and_then(|mut kernel| {
            kernel.progress = self.progress.clone();
            ensure!(
                kernel.root_id() == job.root,
                "registered workspace root identity changed"
            );
            let span = kernel.usage_span(
                if job.full {
                    UsageOperation::Reindex
                } else {
                    UsageOperation::Index
                },
                ClientFamily::NativeScheduler,
            );
            let result = if let Some(paths) = &job.paths {
                kernel.index_current_paths(paths, true)
            } else {
                kernel.index_current(job.full, true)
            };
            if result.as_ref().is_err_and(|error| {
                matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                )
            }) {
                // A deferred claim did no indexing work. Do not count
                // each contention retry as a failed usage observation.
                return result;
            }
            // Background jobs have a logical index-report JSON representation;
            // queue/watch envelopes and waiting CLI callers are not extra index work.
            let logical = result
                .as_ref()
                .ok()
                .and_then(|report| serde_json::to_value(report).ok());
            let bytes = logical
                .as_ref()
                .and_then(|value| serde_json::to_vec(value).ok())
                .map_or(0, |json| json.len());
            kernel.finish_usage(
                span,
                logical.as_ref(),
                bytes,
                result.is_err(),
                ResponseRepresentation::LogicalResultJson,
            );
            result
        });
        let outcome = match result {
            Ok(report) => {
                queue.finish(&worker, &job, now()?, Ok(&serde_json::to_string(&report)?))?;
                RunOutcome {
                    state: "indexed",
                    collection: Some(job.collection),
                    revision: Some(job.requested_revision),
                    report: Some(report),
                    error: None,
                    maintenance: None,
                }
            }
            Err(error) if error.is::<ScanPaused>() => {
                queue.defer(&worker, &job)?;
                RunOutcome {
                    state: "paused",
                    collection: Some(job.collection),
                    revision: Some(job.requested_revision),
                    report: None,
                    error: None,
                    maintenance: None,
                }
            }
            Err(error)
                if matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                ) =>
            {
                queue.defer(&worker, &job)?;
                RunOutcome {
                    state: "busy",
                    collection: Some(job.collection),
                    revision: Some(job.requested_revision),
                    report: None,
                    error: None,
                    maintenance: None,
                }
            }
            Err(error) => {
                let error = format!("{error:#}");
                queue.finish(&worker, &job, now()?, Err(&error))?;
                RunOutcome {
                    state: "failed",
                    collection: Some(job.collection),
                    revision: Some(job.requested_revision),
                    report: None,
                    error: Some(error),
                    maintenance: None,
                }
            }
        };
        Ok(outcome)
    }

    /// Persist first, then join one serialized worker. A killed extension/helper
    /// leaves the unfinished revision available to another host or CLI worker.
    pub fn scheduled_index(&self, full: bool, automatic: bool) -> Result<IndexReport> {
        self.scheduled_index_selected(full, automatic, None, false)
    }

    pub fn scheduled_index_selected(
        &self,
        full: bool,
        automatic: bool,
        paths: Option<&[String]>,
        coalesced: bool,
    ) -> Result<IndexReport> {
        let queue = self.queue()?;
        let requested = queue.enqueue_selected(
            &self.collection,
            &self.root_id(),
            full,
            !automatic,
            true,
            now()?,
            paths,
            coalesced,
        )?;
        ensure!(
            !automatic || !requested.paused,
            "native automatic scheduling is paused; request remains queued"
        );
        self.wait_scheduled_index(requested.requested_revision)
    }

    /// Join an existing durable request without enqueueing or clearing failures.
    pub fn wait_scheduled_index(&self, revision: u64) -> Result<IndexReport> {
        self.wait_scheduled_index_with_budget(revision, Duration::from_secs(240))
    }

    fn wait_scheduled_index_with_budget(
        &self,
        revision: u64,
        budget: Duration,
    ) -> Result<IndexReport> {
        ensure!(revision > 0, "scheduled revision must be positive");
        let queue = self.queue()?;
        let start = Instant::now();
        loop {
            let job = queue
                .status(&self.collection, &self.root_id())?
                .context("scheduled collection was removed")?;
            ensure!(
                revision <= job.requested_revision,
                "scheduled revision was not requested"
            );
            if job.completed_revision >= revision {
                let report = queue.completed_report(&self.collection, &self.root_id(), revision)?
                    .or_else(|| (job.completed_revision == revision).then_some(job.last_report).flatten())
                    .with_context(|| format!("scheduled report for revision {revision} was superseded by a later pass and is no longer retained; inspect schedule status"))?;
                return serde_json::from_str(&report).context("invalid scheduled report");
            }
            if job.blocked {
                bail!(
                    "scheduled indexing failed: {}",
                    job.error.as_deref().unwrap_or("unknown failure")
                );
            }
            if job.paused && !job.manual {
                return Err(ScanPaused.into());
            }
            if start.elapsed() >= budget {
                return Err(ScheduledIndexPending { revision }.into());
            }
            let outcome = self.run_scheduled_once()?;
            if outcome.collection.as_deref() == Some(&self.collection) && outcome.state == "indexed"
            {
                // Re-read the revision-bound durable result. Another pass may
                // have completed while this waiter was joining the worker.
                continue;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod join_tests {
    use super::*;

    #[test]
    fn full_request_is_durable_while_the_collection_writer_is_locked() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let writer = kernel.store.lock()?;
        let quota = airk_store::quota::Quota::open(data.path())?;
        let scope = kernel.current_scope()?.unwrap();
        let capacity = kernel.store.current_database_capacity(&scope)?;
        let current = kernel
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        let request = kernel.schedule_enqueue(true)?;
        let reopened = Kernel::open_collection_for_diagnostics(
            root.path(),
            Some(data.path().into()),
            "default",
        )?;
        let pending = reopened.schedule_status()?.unwrap();
        assert!(pending.full);
        assert_eq!(pending.requested_revision, request.requested_revision);
        assert_eq!(pending.completed_revision, 0);
        drop(current);
        drop(writer);
        Ok(())
    }

    #[test]
    fn joining_an_earlier_full_pass_does_not_return_a_later_scoped_report() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("fixture.txt"), "original queued evidence")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        let full = kernel.scheduled_index(true, false)?;
        let revision = kernel.schedule_status()?.unwrap().completed_revision;
        std::fs::write(root.path().join("fixture.txt"), "changed queued evidence")?;
        let scoped =
            kernel.scheduled_index_selected(false, false, Some(&["fixture.txt".into()]), true)?;
        assert!(scoped.path_scoped);
        assert!(!full.path_scoped);
        assert!(scoped.generation > full.generation);
        let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
        let joined = reopened.wait_scheduled_index_with_budget(revision, Duration::ZERO)?;
        assert_eq!(serde_json::to_value(joined)?, serde_json::to_value(full)?);
        for _ in 0..3 {
            reopened.scheduled_index_selected(false, false, Some(&["fixture.txt".into()]), true)?;
        }
        let error = reopened
            .wait_scheduled_index_with_budget(revision, Duration::ZERO)
            .unwrap_err();
        assert!(error.to_string().contains("superseded by a later pass"));
        Ok(())
    }

    #[test]
    fn expired_join_can_resume_the_same_revision_without_new_work() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("fixture.txt"), "original queued evidence")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        let request = kernel.schedule_enqueue(false)?;
        let revision = request.requested_revision;
        let error = kernel
            .wait_scheduled_index_with_budget(revision, Duration::ZERO)
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<ScheduledIndexPending>()
                .unwrap()
                .revision,
            revision
        );
        let pending = kernel.schedule_status()?.unwrap();
        assert!(!pending.blocked);
        assert_eq!(pending.requested_revision, revision);
        let report = kernel.wait_scheduled_index(revision)?;
        let completed = kernel.schedule_status()?.unwrap();
        assert_eq!(completed.requested_revision, revision);
        assert_eq!(completed.completed_revision, revision);
        // Check completion before the wait budget, including late completions.
        assert_eq!(
            kernel
                .wait_scheduled_index_with_budget(revision, Duration::ZERO)?
                .generation,
            report.generation
        );
        assert!(kernel.wait_scheduled_index(0).is_err());
        assert!(kernel.wait_scheduled_index(revision + 1).is_err());
        let next = kernel.schedule_enqueue(false)?;
        kernel.schedule_pause(true)?;
        assert!(
            kernel
                .wait_scheduled_index(next.requested_revision)
                .unwrap_err()
                .is::<ScanPaused>()
        );
        assert_eq!(
            kernel.schedule_status()?.unwrap().requested_revision,
            next.requested_revision
        );
        Ok(())
    }
}
