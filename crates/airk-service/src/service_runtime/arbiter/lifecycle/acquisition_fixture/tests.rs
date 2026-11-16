// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test]
async fn report_retains_io_phase_and_cancel_even_when_the_control_outcome_failed() -> Result<()> {
    let (sender, receiver) = oneshot::channel();
    let mut retained_receiver = Some(receiver);
    let control_outcome: Result<()> = async {
        // This is the actual reporter; the observer belongs to outer cleanup.
        let result = report_result(
            Err(anyhow::anyhow!("original Store I/O failed")),
            Err(anyhow::anyhow!("phase observation failed")),
            Duration::from_millis(1),
            true,
            sender,
        );
        assert_eq!(result.unwrap_err().to_string(), "original Store I/O failed");
        anyhow::bail!("actual native control failed")
    }
    .await;
    assert!(control_outcome.is_err());
    let observed = retained_receiver
        .take()
        .context("receiver lost during outcome")?
        .await?;
    assert!(!observed.io_succeeded);
    assert_eq!(
        observed.io_error.as_deref(),
        Some("original Store I/O failed")
    );
    assert_eq!(
        observed.phase_error.as_deref(),
        Some("phase observation failed")
    );
    assert!(observed.original_cancellation_observed);
    assert_eq!(observed.elapsed_us, Some(1000));
    Ok(())
}
#[test]
fn dropped_report_observer_never_replaces_actual_store_io_result() {
    for io_failed in [false, true] {
        let (sender, receiver) = oneshot::channel();
        drop(receiver);
        let io = if io_failed {
            Err(anyhow::anyhow!("original Store I/O failed"))
        } else {
            Ok(())
        };
        let result = report_result(io, Ok(Some(Phase::Ready)), Duration::ZERO, false, sender);
        if io_failed {
            assert_eq!(result.unwrap_err().to_string(), "original Store I/O failed");
        } else {
            assert!(result.is_ok());
        }
    }
}
#[tokio::test]
async fn successful_io_with_failed_phase_remains_distinct_from_original_io_failure() -> Result<()> {
    let (sender, receiver) = oneshot::channel();
    let result = report_result(
        Ok(()),
        Err(anyhow::anyhow!("phase unavailable")),
        Duration::ZERO,
        false,
        sender,
    );
    assert_eq!(result.unwrap_err().to_string(), "phase unavailable");
    let observed = receiver.await?;
    assert!(observed.io_succeeded);
    assert_eq!(observed.io_error, None);
    assert_eq!(observed.phase_error.as_deref(), Some("phase unavailable"));
    Ok(())
}
