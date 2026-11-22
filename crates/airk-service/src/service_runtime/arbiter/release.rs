// SPDX-License-Identifier: Apache-2.0
//! The one release order every switch and idle unload uses: release the
//! prior resident, observe its native release, observe its final charge
//! drain, and only then run what comes next (the new reservation). The steps
//! are a seam so the order, a drain timeout and cancellation are tested
//! without a native owner.
use super::*;
use airk_core::identity::Sha256Digest;
use airk_inference::inference_engine::pool::arbiter::Handle;
use tokio_util::sync::CancellationToken;

/// The native steps of one release.
pub(super) trait ReleaseSteps {
    /// Asks the owner to release the resident. Never blocks on its drain.
    fn release(&self, manifest: Sha256Digest) -> Result<()>;
    /// Waits for the owner to report the resident released.
    async fn wait_released(&self, manifest: Sha256Digest, deadline: Instant) -> Result<()>;
    /// Waits for the resident's final memory charge to drain.
    async fn wait_charge(&self, deadline: Instant) -> Result<()>;
}

/// The real native owner and the resident's charge observer.
pub(super) struct NativeRelease<'a> {
    pub(super) handle: &'a Handle,
    pub(super) charge: airk_kernel::service_resources::OptionalRelease,
}
impl ReleaseSteps for NativeRelease<'_> {
    fn release(&self, manifest: Sha256Digest) -> Result<()> {
        self.handle.release_profile(manifest)
    }
    async fn wait_released(&self, manifest: Sha256Digest, deadline: Instant) -> Result<()> {
        self.handle.wait_profile_released(manifest, deadline).await
    }
    async fn wait_charge(&self, deadline: Instant) -> Result<()> {
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), self.charge.wait())
            .await
            .context("Actual Arbiter memory charge did not finish draining")??;
        Ok(())
    }
}

/// Releases `manifest`, observes the release and the charge drain, then runs
/// `next`. Cancellation before the release keeps the resident untouched. Once
/// the release is requested it is always observed to its end; a cancellation
/// that arrives meanwhile stops only `next`. A failed or timed-out drain
/// never runs `next`.
pub(super) async fn release_then<S: ReleaseSteps, T>(
    steps: &S,
    manifest: Sha256Digest,
    deadline: Instant,
    cancel: &CancellationToken,
    next: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    ensure!(
        !cancel.is_cancelled(),
        "Canceled before the release; the prior resident was kept"
    );
    steps.release(manifest)?;
    steps
        .wait_released(manifest, deadline)
        .await
        .context("Prior native session release was not observed")?;
    steps
        .wait_charge(deadline)
        .await
        .context("Prior final memory charge did not drain")?;
    ensure!(
        !cancel.is_cancelled(),
        "Canceled after the prior resident drained; nothing new was reserved"
    );
    next.await
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Records every step in order; each wait may be set to fail.
    #[derive(Default)]
    pub(in crate::service_runtime) struct Recorder {
        pub(in crate::service_runtime) events: StdMutex<Vec<&'static str>>,
        pub(in crate::service_runtime) fail_release_wait: bool,
        pub(in crate::service_runtime) fail_charge_wait: bool,
        /// Cancel this token while the release is being observed.
        pub(in crate::service_runtime) cancel_during_wait: Option<CancellationToken>,
    }
    impl Recorder {
        pub(in crate::service_runtime) fn push(&self, event: &'static str) {
            self.events.lock().unwrap().push(event);
        }
        pub(in crate::service_runtime) fn seen(&self) -> Vec<&'static str> {
            self.events.lock().unwrap().clone()
        }
    }
    impl ReleaseSteps for Recorder {
        fn release(&self, _: Sha256Digest) -> Result<()> {
            self.push("release");
            Ok(())
        }
        async fn wait_released(&self, _: Sha256Digest, _: Instant) -> Result<()> {
            if let Some(cancel) = &self.cancel_during_wait {
                cancel.cancel();
            }
            tokio::task::yield_now().await;
            if self.fail_release_wait {
                self.push("release-timeout");
                anyhow::bail!("native release deadline");
            }
            self.push("released");
            Ok(())
        }
        async fn wait_charge(&self, _: Instant) -> Result<()> {
            if self.fail_charge_wait {
                self.push("charge-timeout");
                anyhow::bail!("charge drain deadline");
            }
            self.push("charge-drained");
            Ok(())
        }
    }

    fn manifest() -> Sha256Digest {
        Sha256Digest::from_bytes([3; 32])
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    #[tokio::test]
    async fn the_new_reservation_runs_only_after_release_and_drain() -> Result<()> {
        let steps = Recorder::default();
        let value = release_then(
            &steps,
            manifest(),
            deadline(),
            &CancellationToken::new(),
            async {
                steps.push("reserve");
                Ok(7)
            },
        )
        .await?;
        assert_eq!(value, 7);
        assert_eq!(
            steps.seen(),
            vec!["release", "released", "charge-drained", "reserve"]
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_drain_timeout_never_reserves() {
        for (release, charge, seen) in [
            (true, false, vec!["release", "release-timeout"]),
            (false, true, vec!["release", "released", "charge-timeout"]),
        ] {
            let steps = Recorder {
                fail_release_wait: release,
                fail_charge_wait: charge,
                ..Recorder::default()
            };
            let result = release_then(
                &steps,
                manifest(),
                deadline(),
                &CancellationToken::new(),
                async {
                    steps.push("reserve");
                    Ok(())
                },
            )
            .await;
            assert!(result.is_err());
            assert_eq!(steps.seen(), seen);
        }
    }

    #[tokio::test]
    async fn cancellation_before_release_keeps_the_resident() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let steps = Recorder::default();
        let result = release_then(&steps, manifest(), deadline(), &cancel, async {
            steps.push("reserve");
            Ok(())
        })
        .await;
        assert!(result.is_err());
        assert!(steps.seen().is_empty());
    }

    #[tokio::test]
    async fn cancellation_during_the_drain_finishes_the_drain_and_reserves_nothing() {
        let cancel = CancellationToken::new();
        let steps = Recorder {
            cancel_during_wait: Some(cancel.clone()),
            ..Recorder::default()
        };
        let result = release_then(&steps, manifest(), deadline(), &cancel, async {
            steps.push("reserve");
            Ok(())
        })
        .await;
        assert!(result.is_err());
        assert_eq!(steps.seen(), vec!["release", "released", "charge-drained"]);
    }
}
