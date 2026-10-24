// SPDX-License-Identifier: Apache-2.0
//! Scheduler freshness is independent of exact current evidence.
use crate::Kernel;
use airk_core::changes::*;
use std::time::{SystemTime, UNIX_EPOCH};
impl Kernel {
    pub fn changes_freshness(&self) -> FreshnessObservation {
        let mut observation = FreshnessObservation {
            source_job: None,
            observed_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |value| {
                    value.as_millis().min(u128::from(u64::MAX)) as u64
                }),
            state: FreshnessState::Unregistered,
            requested_watermark: None,
            completed_watermark: None,
            running_watermark: None,
            last_success_unix_ms: None,
            error: None,
            live_source_verified: false,
            last_inventory_unix_ms: None,
            last_index: None,
        };
        if self.service_collection.is_some() {
            // Service source intents live in the user's service registry, not
            // the legacy per-root scheduler namespace. Never substitute that
            // unrelated queue for an unavailable service observation.
            return self.service_freshness.as_ref().map_or_else(
                || {
                    crate::service_freshness::unavailable(
                        "service_freshness_observation_unavailable",
                    )
                },
                |cache| cache.read(std::time::Instant::now()),
            );
        }
        match airk_store::scheduler::Queue::new(&self.data_root)
            .status(&self.collection, &self.root_id())
        {
            Ok(Some(job)) => {
                let inventory = job
                    .scan_completed_unix_ms
                    .max(job.inventory_completed_unix_ms);
                observation.last_inventory_unix_ms = (inventory != 0).then_some(inventory);
                observation.last_index = job
                    .last_report
                    .as_deref()
                    .and_then(|bytes| serde_json::from_str::<IndexTimingObservation>(bytes).ok())
                    .map(|mut timing| {
                        timing.completed_unix_ms = job.last_completed_unix_ms;
                        timing
                    });
                observation.requested_watermark = Some(job.requested_revision);
                observation.completed_watermark = Some(job.completed_revision);
                observation.running_watermark = Some(job.running_revision);
                observation.last_success_unix_ms =
                    (job.last_completed_unix_ms != 0).then_some(job.last_completed_unix_ms);
                observation.state = if job.paused {
                    FreshnessState::Paused
                } else if job.blocked {
                    FreshnessState::Blocked
                } else if job.running_revision != 0 {
                    FreshnessState::Running
                } else if job.error.is_some() || job.scan_error.is_some() {
                    FreshnessState::Failed
                } else if job.requested_revision > job.completed_revision {
                    FreshnessState::Pending
                } else {
                    FreshnessState::Idle
                };
                observation.error = job
                    .error
                    .or(job.scan_error)
                    .map(|error| error.chars().take(512).collect());
            }
            Ok(None) => {}
            Err(error) => {
                observation.state = FreshnessState::Unavailable;
                observation.error = Some(error.to_string().chars().take(512).collect());
            }
        }
        observation
    }

    pub fn discovery_observation(
        &self,
        expected: airk_core::identity::RevisionToken,
    ) -> anyhow::Result<Option<airk_core::readiness::DiscoveryReadiness>> {
        let read = self.current_reader()?;
        if crate::current::runtime::revision(read.token()?) != expected {
            return Ok(None);
        }
        Ok(Some(crate::current::progress::readiness(
            &read,
            &self.store,
        )?))
    }

    pub fn readiness_observation(
        &self,
        expected: airk_core::identity::RevisionToken,
    ) -> anyhow::Result<
        Option<(
            airk_core::readiness::DiscoveryReadiness,
            airk_core::readiness::SemanticReadiness,
        )>,
    > {
        let read = self.current_reader()?;
        if crate::current::runtime::revision(read.token()?) != expected {
            return Ok(None);
        }
        Ok(Some((
            crate::current::progress::readiness(&read, &self.store)?,
            crate::current::enrichment::readiness(&read, &self.store)?,
        )))
    }
}
