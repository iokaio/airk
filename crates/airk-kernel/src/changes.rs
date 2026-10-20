// SPDX-License-Identifier: Apache-2.0
//! Scheduler freshness is independent of exact current evidence.
use crate::Kernel;
use airk_core::changes::*;
use std::time::{SystemTime, UNIX_EPOCH};
impl Kernel {
    pub(crate) fn changes_freshness(&self) -> FreshnessObservation {
        let mut observation = FreshnessObservation {
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
        };
        match airk_store::scheduler::Queue::new(&self.data_root)
            .status(&self.collection, &self.root_id())
        {
            Ok(Some(job)) => {
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
}
