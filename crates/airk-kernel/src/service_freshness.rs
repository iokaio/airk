// SPDX-License-Identifier: Apache-2.0
//! Fenced cached source metadata; no registry I/O on the evidence path.
use crate::Kernel;
use airk_core::{
    changes::{FreshnessObservation, FreshnessState},
    service::{ProjectRegistration, WorkerIdentity},
};
use anyhow::{Result, ensure};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_AGE: Duration = Duration::from_secs(5);
pub(crate) struct Cache {
    identity: WorkerIdentity,
    sample: Mutex<Option<(Instant, FreshnessObservation)>>,
}
pub(crate) fn unavailable(code: &str) -> FreshnessObservation {
    FreshnessObservation {
        source_job: None,
        observed_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
        state: FreshnessState::Unavailable,
        requested_watermark: None,
        completed_watermark: None,
        running_watermark: None,
        last_success_unix_ms: None,
        error: Some(code.into()),
        live_source_verified: false,
        last_inventory_unix_ms: None,
        last_index: None,
    }
}
impl Cache {
    pub(crate) fn read(&self, now: Instant) -> FreshnessObservation {
        let Ok(sample) = self.sample.lock() else {
            return unavailable("service_freshness_cache_unavailable");
        };
        let Some((observed, value)) = sample.as_ref() else {
            return unavailable("service_freshness_observation_unavailable");
        };
        if now.saturating_duration_since(*observed) >= MAX_AGE {
            let mut stale = value.clone();
            stale.state = FreshnessState::Unavailable;
            let mut code = stale.error.take().unwrap_or_default();
            code.truncate(code.floor_char_boundary(384));
            if !code.is_empty() {
                code.push(';');
            }
            code.push_str("service_freshness_observation_expired");
            stale.error = Some(code);
            return stale;
        }
        value.clone()
    }
    fn update(
        &self,
        identity: WorkerIdentity,
        value: Option<FreshnessObservation>,
        now: Instant,
    ) -> Result<()> {
        ensure!(
            identity == self.identity,
            "foreign service freshness generation"
        );
        if let Some(value) = &value {
            ensure!(
                !value.live_source_verified
                    && value.error.as_ref().is_none_or(|code| code.len() <= 512)
                    && serde_json::to_vec(value)?.len() <= 4096,
                "invalid service freshness sample"
            );
            if let (Some(requested), Some(completed)) =
                (value.requested_watermark, value.completed_watermark)
            {
                ensure!(
                    completed <= requested
                        && value
                            .running_watermark
                            .is_none_or(|running| running <= requested),
                    "invalid service source watermarks"
                );
            }
        }
        *self
            .sample
            .lock()
            .map_err(|_| anyhow::anyhow!("service freshness cache unavailable"))? =
            value.map(|value| (now, value));
        Ok(())
    }
}
impl Kernel {
    pub fn with_service_freshness(mut self, identity: WorkerIdentity) -> Result<Self> {
        ensure!(
            self.service_collection.is_some()
                && self.service_freshness.is_none()
                && identity.fencing_generation > 0,
            "service freshness requires a fresh service-bound kernel"
        );
        let binding = ProjectRegistration::new(
            self.root
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("source root is not UTF-8"))?
                .into(),
            self.data_root
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("data root is not UTF-8"))?
                .into(),
            self.collection.clone(),
        )?
        .binding;
        ensure!(
            binding == identity.project,
            "foreign service freshness scope"
        );
        self.service_freshness = Some(Arc::new(Cache {
            identity,
            sample: Mutex::new(None),
        }));
        Ok(self)
    }
    pub fn observe_service_freshness(
        &self,
        identity: WorkerIdentity,
        sample: Option<FreshnessObservation>,
    ) -> Result<()> {
        self.service_freshness
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("service freshness not installed"))?
            .update(identity, sample, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_observation_is_fenced_expirable_and_never_live_source_proof() -> Result<()> {
        use airk_core::{
            changes::FreshnessState,
            identity::{Incarnation, Sha256Digest},
        };
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: Sha256Digest::from_bytes([3; 32]),
            fencing_generation: 4,
        };
        let cache = Cache {
            identity,
            sample: Mutex::new(None),
        };
        let now = Instant::now();
        let mut value = unavailable("fixture");
        value.state = FreshnessState::Idle;
        value.error = None;
        value.requested_watermark = Some(9);
        value.completed_watermark = Some(8);
        cache.update(identity, Some(value.clone()), now)?;
        assert_eq!(cache.read(now).state, FreshnessState::Idle);
        let stale = cache.read(now + MAX_AGE);
        assert_eq!(stale.state, FreshnessState::Unavailable);
        assert_eq!(stale.completed_watermark, Some(8));
        let foreign = WorkerIdentity {
            fencing_generation: 5,
            ..identity
        };
        assert!(cache.update(foreign, None, now).is_err());
        assert_eq!(cache.read(now).state, FreshnessState::Idle);
        value.live_source_verified = true;
        assert!(cache.update(identity, Some(value.clone()), now).is_err());
        value.live_source_verified = false;
        value.completed_watermark = Some(10);
        assert!(cache.update(identity, Some(value), now).is_err());
        cache.update(identity, None, now)?;
        assert_eq!(cache.read(now).state, FreshnessState::Unavailable);
        Ok(())
    }

    #[test]
    fn cache_binding_rejects_another_collection_and_is_shared_by_kernel_clones() -> Result<()> {
        use airk_core::{identity::Incarnation, service::ProjectRegistration};
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        let data = directory.path().join("data");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&data)?;
        let root = root.canonicalize()?;
        let data = data.canonicalize()?;
        let binding = ProjectRegistration::new(
            root.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "one".into(),
        )?
        .binding;
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: binding,
            fencing_generation: 1,
        };
        let kernel = Kernel::open_service_collection(&root, Some(data.clone()), "one")?;
        let other = ProjectRegistration::new(
            root.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "two".into(),
        )?
        .binding;
        assert!(
            kernel
                .clone()
                .with_service_freshness(WorkerIdentity {
                    project: other,
                    ..identity
                })
                .is_err()
        );
        let kernel = kernel.with_service_freshness(identity)?;
        let mut value = unavailable("fixture");
        value.state = FreshnessState::Failed;
        value.error = Some("source_unavailable".into());
        kernel.observe_service_freshness(identity, Some(value))?;
        assert_eq!(
            kernel.clone().changes_freshness().state,
            FreshnessState::Failed
        );
        assert!(
            kernel
                .observe_service_freshness(
                    WorkerIdentity {
                        fencing_generation: 2,
                        ..identity
                    },
                    None
                )
                .is_err()
        );
        assert_eq!(
            kernel.changes_freshness().error.as_deref(),
            Some("source_unavailable")
        );
        Ok(())
    }
}
