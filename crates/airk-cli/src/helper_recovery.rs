// SPDX-License-Identifier: Apache-2.0
//! Service-owned helper recovery. Saved state is translated to monotonic time once.
use crate::embedding_pool_process::PoolHealth;
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_store::service::registry::{HelperRecoveryClear, HelperRecoverySnapshot, ServiceRegistry};
use anyhow::Result;
use std::time::{Duration, Instant};

const HEALTHY_WINDOW: Duration = Duration::from_secs(60);

pub(super) struct PoolRecovery {
    pub attempts: u32,
    pub suppressed: bool,
    pub requested: bool,
    retry_at: Instant,
    observed: Option<HelperRecoverySnapshot>,
    initialized: bool,
    healthy_basis: Option<(Incarnation, Option<HelperRecoverySnapshot>)>,
}

impl PoolRecovery {
    pub fn new(now: Instant) -> Self {
        Self {
            attempts: 0,
            suppressed: false,
            requested: false,
            retry_at: now,
            observed: None,
            initialized: false,
            healthy_basis: None,
        }
    }

    pub fn due(&self, now: Instant) -> bool {
        self.admission_pending() && now >= self.retry_at
    }

    pub fn admission_pending(&self) -> bool {
        !self.suppressed && self.attempts < 3
    }

    pub fn attempted(&mut self) {
        self.attempts += 1;
    }

    pub fn failed(&mut self, now: Instant) {
        self.healthy_basis = None;
        self.retry_at = now + Duration::from_secs(1 << self.attempts.min(3));
    }

    pub fn started(&mut self, instance: Incarnation) {
        self.healthy_basis = Some((instance, self.observed.clone()));
    }

    pub fn refresh(
        &mut self,
        registry: &mut ServiceRegistry,
        engine: Sha256Digest,
        now: Instant,
        wall_ms: u64,
    ) -> Result<Option<&'static str>> {
        if self.suppressed {
            return Ok(None); // Only explicit Start may reopen local terminal admission.
        }
        let mut observed = registry.helper_recovery_snapshot()?;
        let mut transition = None;
        if let Some(saved) = &observed
            && saved.engine != engine
            && !saved.suppressed
        {
            if !registry.clear_helper_recovery(engine, saved, HelperRecoveryClear::EngineChanged)? {
                self.initialized = false;
                self.attempts = 3; // Reload before another admission, never use stale proof.
                return Ok(None);
            }
            observed = None;
            transition = Some("helper_engine_changed");
        }
        if !self.initialized || self.observed != observed {
            self.initialized = true;
            self.observed = observed.clone();
            self.attempts = observed.as_ref().map_or(0, |saved| saved.failures);
            self.suppressed = observed.as_ref().is_some_and(|saved| saved.suppressed);
            self.retry_at = now
                + Duration::from_millis(
                    observed
                        .as_ref()
                        .map_or(0, |saved| saved.next_ms.saturating_sub(wall_ms).min(60_000)),
                );
            transition = transition.or(if self.suppressed {
                Some("helper_recovery_suppressed")
            } else if self.attempts == 3 {
                Some("helper_cooldown")
            } else if self.attempts > 0 {
                Some("helper_backoff")
            } else {
                None
            });
        }
        if !self.suppressed && self.attempts == 3 && now >= self.retry_at {
            let saved = self
                .observed
                .as_ref()
                .expect("three failures have a saved basis");
            if !registry.resume_helper_after_cooldown(engine, saved)? {
                self.initialized = false;
                return Ok(None);
            }
            let mut resumed = saved.clone();
            resumed.failures = 0;
            resumed.next_ms = 0;
            self.observed = Some(resumed);
            self.attempts = 0;
            self.retry_at = now;
            transition = Some("helper_cooldown_elapsed");
        }
        Ok(transition)
    }

    pub fn healthy(
        &mut self,
        registry: &mut ServiceRegistry,
        engine: Sha256Digest,
        health: PoolHealth,
        now: Instant,
    ) -> Result<bool> {
        let Some((instance, basis)) = &self.healthy_basis else {
            return Ok(false);
        };
        if self.suppressed
            || *instance != health.instance
            || now.saturating_duration_since(health.ready_at) < HEALTHY_WINDOW
            || health.last_ack_at < health.ready_at + HEALTHY_WINDOW
            || health.last_ack_at > now
        {
            return Ok(false);
        }
        let cleared = match basis {
            Some(saved) => {
                registry.clear_helper_recovery(engine, saved, HelperRecoveryClear::Healthy)?
            }
            None => registry.helper_recovery_snapshot()?.is_none(),
        };
        self.healthy_basis = None; // This pool's old proof cannot clear a later outage.
        if cleared {
            self.attempts = 0;
            self.observed = None;
            self.initialized = true;
        }
        Ok(cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> Sha256Digest {
        Sha256Digest::from_bytes([7; 32])
    }
    fn instance() -> Incarnation {
        Incarnation::from_bytes([8; 16])
    }

    #[test]
    fn cooldown_uses_one_monotonic_translation_and_does_not_reset_outage_anchor() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), instance())?;
        for wall in [100, 200, 300] {
            registry.recovery_failed("helper", engine(), wall)?;
        }
        let now = Instant::now();
        let mut recovery = PoolRecovery::new(now);
        assert_eq!(
            recovery.refresh(&mut registry, engine(), now, 300)?,
            Some("helper_cooldown")
        );
        assert!(!recovery.due(now + Duration::from_secs(59)));
        // Wall-clock jumps do not shorten or extend the already observed deadline.
        assert_eq!(
            recovery.refresh(
                &mut registry,
                engine(),
                now + Duration::from_secs(59),
                u64::MAX
            )?,
            None
        );
        assert!(!recovery.admission_pending());
        assert_eq!(
            recovery.refresh(&mut registry, engine(), now + Duration::from_secs(60), 0)?,
            Some("helper_cooldown_elapsed")
        );
        assert!(recovery.due(now + Duration::from_secs(60)));
        let saved = registry.helper_recovery_snapshot()?.unwrap();
        assert_eq!((saved.failures, saved.started_ms), (0, 100));
        registry.suppress_helper_recovery(engine())?;
        recovery.refresh(
            &mut registry,
            engine(),
            now + Duration::from_secs(61),
            61_300,
        )?;
        assert!(!recovery.due(now + Duration::from_secs(3600)));
        Ok(())
    }

    #[test]
    fn healthy_reset_requires_same_instance_later_ack_and_exact_captured_outage() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), instance())?;
        registry.recovery_failed("helper", engine(), 100)?;
        let now = Instant::now();
        let mut recovery = PoolRecovery::new(now);
        recovery.refresh(&mut registry, engine(), now, 100)?;
        recovery.started(instance());
        let health = PoolHealth {
            instance: instance(),
            ready_at: now,
            last_ack_at: now,
        };
        assert!(!recovery.healthy(
            &mut registry,
            engine(),
            health,
            now + Duration::from_secs(61)
        )?);
        let acknowledged = PoolHealth {
            last_ack_at: now + Duration::from_secs(60),
            ..health
        };
        assert!(!recovery.healthy(
            &mut registry,
            engine(),
            PoolHealth {
                instance: Incarnation::from_bytes([9; 16]),
                ..acknowledged
            },
            now + Duration::from_secs(61)
        )?);
        registry.recovery_failed("helper", engine(), 200)?;
        assert!(!recovery.healthy(
            &mut registry,
            engine(),
            acknowledged,
            now + Duration::from_secs(61)
        )?);
        assert_eq!(registry.helper_recovery_snapshot()?.unwrap().failures, 2);
        recovery.refresh(
            &mut registry,
            engine(),
            now + Duration::from_secs(62),
            62_000,
        )?;
        recovery.started(instance());
        assert!(recovery.healthy(
            &mut registry,
            engine(),
            acknowledged,
            now + Duration::from_secs(63)
        )?);
        assert!(registry.helper_recovery_snapshot()?.is_none());
        assert_eq!(recovery.attempts, 0);
        Ok(())
    }
}
