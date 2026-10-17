// SPDX-License-Identifier: Apache-2.0
//! Native scheduling shared by extension hosts and explicit command-line workers.
use crate::Kernel;
use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
use airk_ingest::IndexReport;
use airk_store::scheduler::{Queue, ScheduledJob};
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

impl Kernel {
    fn queue(&self) -> Result<Queue> {
        std::fs::create_dir_all(&self.data_root)?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        if self.persistent_revision()?.is_none() {
            self.load_optional()?;
        }
        Ok(Queue::new(&self.data_root))
    }

    pub fn schedule_enqueue(&self, full: bool) -> Result<ScheduledJob> {
        self.queue()?.enqueue(
            &self.collection,
            &self.root_id(),
            full,
            false,
            false,
            now()?,
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
        let measured =
            Kernel::open_collection(&job.root, Some(queue.data_dir().into()), &job.collection)
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
        let result =
            Kernel::open_collection(&job.root, Some(queue.data_dir().into()), &job.collection)
                .and_then(|kernel| {
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
                    let result = if job.full {
                        kernel.reindex()
                    } else {
                        kernel.index()
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
        let queue = self.queue()?;
        let requested = queue.enqueue(
            &self.collection,
            &self.root_id(),
            full,
            !automatic,
            true,
            now()?,
        )?;
        ensure!(
            !automatic || !requested.paused,
            "native automatic scheduling is paused; request remains queued"
        );
        let start = Instant::now();
        loop {
            let job = queue
                .status(&self.collection, &self.root_id())?
                .context("scheduled collection was removed")?;
            if job.completed_revision >= requested.requested_revision {
                return serde_json::from_str(
                    job.last_report
                        .as_deref()
                        .context("scheduled report missing")?,
                )
                .context("invalid scheduled report");
            }
            if job.blocked {
                bail!(
                    "scheduled indexing failed: {}",
                    job.error.as_deref().unwrap_or("unknown failure")
                );
            }
            let outcome = self.run_scheduled_once()?;
            if outcome.collection.as_deref() == Some(&self.collection)
                && outcome.state == "indexed"
                && outcome
                    .revision
                    .is_some_and(|revision| revision >= requested.requested_revision)
            {
                return outcome
                    .report
                    .context("successful scheduled report missing");
            }
            ensure!(
                start.elapsed() < Duration::from_secs(240),
                "scheduled indexing wait exceeded 240 seconds; request remains queued"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
}
