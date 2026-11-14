// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation,
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecyclePhase},
};
use airk_store::service::registry::ServiceRegistry;

async fn accepted_lifecycle_survives_failed_delivery(action: LifecycleAction) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let instance = Incarnation::from_bytes([0x21; 16]);
    let caller = Incarnation::from_bytes([0x22; 16]);
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([0x23; 16]))?;
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([0x24; 16]),
    };
    drop(registry);
    let runtime = "25".repeat(32);
    let accepted = lifecycle::accept(&root, instance, &runtime, action, caller, key.clone())?;
    assert_eq!(accepted.phase, LifecyclePhase::Draining);
    let shutdown = Notify::new();
    let error = deliver_control_response(&shutdown, true, async {
        anyhow::bail!("injected response writer failure")
    })
    .await
    .expect_err("response failure stays observable");
    assert_eq!(error.to_string(), "injected response writer failure");
    // No timing or sleep: an accepted shutdown must already have a pending
    // notification even though the response writer failed immediately.
    tokio::time::timeout(Duration::ZERO, shutdown.notified())
        .await
        .expect("accepted lifecycle must notify drain despite failed delivery");
    let replay = lifecycle::accept(&root, instance, &runtime, action, caller, key)?;
    assert_eq!(
        serde_json::to_value(replay)?,
        serde_json::to_value(&accepted)?,
        "the original receipt survives delivery loss"
    );
    lifecycle::finish_drain(&root, &accepted)?;
    let settled = ServiceRegistry::open_saved(&root)?
        .lifecycle_by_id(accepted.id)?
        .expect("original durable record");
    assert_eq!(settled.id, accepted.id);
    assert_eq!(settled.request, accepted.request);
    assert_eq!(
        settled.phase,
        match action {
            LifecycleAction::Stop => LifecyclePhase::Complete,
            LifecycleAction::Restart => LifecyclePhase::Starting,
        }
    );
    Ok(())
}

#[tokio::test]
async fn accepted_stop_notifies_drain_after_response_writer_failure() -> Result<()> {
    accepted_lifecycle_survives_failed_delivery(LifecycleAction::Stop).await
}

#[tokio::test]
async fn accepted_restart_notifies_drain_after_response_writer_failure() -> Result<()> {
    accepted_lifecycle_survives_failed_delivery(LifecycleAction::Restart).await
}

#[tokio::test]
async fn a_non_shutdown_response_failure_does_not_request_shutdown() -> Result<()> {
    let shutdown = Notify::new();
    assert!(
        deliver_control_response(&shutdown, false, async {
            anyhow::bail!("injected response writer failure")
        })
        .await
        .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::ZERO, shutdown.notified())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn a_successfully_delivered_shutdown_still_notifies() -> Result<()> {
    let shutdown = Notify::new();
    deliver_control_response(&shutdown, true, async { Ok(()) }).await?;
    tokio::time::timeout(Duration::ZERO, shutdown.notified()).await?;
    Ok(())
}

#[tokio::test]
async fn an_accepted_shutdown_is_notified_before_the_writer_is_polled() -> Result<()> {
    let shutdown = Notify::new();
    deliver_control_response(&shutdown, true, async {
        tokio::time::timeout(Duration::ZERO, shutdown.notified())
            .await
            .expect("drain notification must not wait for any response progress");
        Ok(())
    })
    .await
}
