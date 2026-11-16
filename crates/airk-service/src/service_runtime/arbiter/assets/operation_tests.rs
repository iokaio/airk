// SPDX-License-Identifier: Apache-2.0
//! Observed blocking boundaries, not model/download qualification.
use super::*;
use airk_store::service::ServiceLease;

#[tokio::test(flavor = "current_thread")]
async fn blocked_asset_work_keeps_control_responsive_and_retains_lease_until_real_settlement()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(ServiceLease::acquire(root.path())?);
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let (entered, observing) = tokio::sync::oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let work = tokio::spawn(owned_asset_operation(async move {
        let _actual_lease = lease;
        entered
            .send(())
            .map_err(|_| anyhow::anyhow!("asset observer closed"))?;
        // This observed release simulates the actual synchronous Store hash/I/O.
        // The timeout is failure diagnosis, never a delay or retry allowance.
        waiting.recv_timeout(Duration::from_secs(5))?;
        ensure!(
            !worker_cancel.is_cancelled(),
            "original asset operation canceled"
        );
        Ok(())
    }));
    tokio::time::timeout(Duration::from_secs(5), observing).await??;
    assert!(ServiceLease::acquire(root.path()).is_err());
    cancel.cancel();
    let control = tokio::spawn(async { "observed native control lane progress" });
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), control).await??,
        "observed native control lane progress"
    );
    assert!(
        !work.is_finished(),
        "cancellation cannot manufacture asset completion"
    );
    assert!(ServiceLease::acquire(root.path()).is_err());
    release.send(())?;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), work)
            .await??
            .is_err()
    );
    // Only the real joined worker return releases its actual lease.
    let _reacquired = ServiceLease::acquire(root.path())?;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn blocking_asset_owner_keeps_the_original_runtime_clock_and_deadline() -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let (entered, observing) = tokio::sync::oneshot::channel();
    let work = tokio::spawn(owned_asset_operation(async move {
        entered
            .send(())
            .map_err(|_| anyhow::anyhow!("clock observer closed"))?;
        let expired = tokio::time::timeout_at(deadline, std::future::pending::<()>()).await;
        ensure!(expired.is_err(), "original deadline did not expire");
        Ok(tokio::time::Instant::now())
    }));
    observing.await?;
    tokio::time::advance(Duration::from_secs(120)).await;
    assert_eq!(work.await??, deadline);
    Ok(())
}
