// SPDX-License-Identifier: Apache-2.0
//! I1 control transport: one framed request per connection with bounded
//! deadlines, the `registry_busy` retry window, and the Health identity check.
//! Moved verbatim from `service_runtime` (split plan G2); the baseline-gate
//! policy stays with the service wrappers that call these functions.
use super::endpoint as local_ipc;
use airk_core::service::{ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION};
use anyhow::{Result, ensure};
use std::{path::Path, time::Duration};

pub(crate) async fn registry_busy_retry<F, Fut>(mut request: F) -> Result<ControlResponse>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<ControlResponse>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut response = request().await?;
    for millis in [100, 200, 400, 600, 700, 800] {
        if !matches!(&response, ControlResponse::Failure { code } if code == "registry_busy") {
            break;
        }
        let next = tokio::time::Instant::now() + Duration::from_millis(millis);
        if next >= deadline {
            break;
        }
        tokio::time::sleep_until(next).await;
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        // Keep the existing timeout for an accepted in-flight request. A
        // transport timeout is uncertain and must never trigger another send.
        response = request().await?;
    }
    Ok(response)
}

pub(crate) async fn request_once(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected_instance: Option<airk_core::identity::Incarnation>,
) -> Result<ControlResponse> {
    let response_deadline = if matches!(method, ControlMethod::ObserveUpgradeCheckpoint { .. }) {
        Duration::from_secs(30)
    } else if matches!(method, ControlMethod::UpgradeRecoveryProbe { .. }) {
        Duration::from_secs(10)
    } else if matches!(method, ControlMethod::Connect { .. }) {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(5)
    };
    let address = local_ipc::endpoint(root, "control")?;
    let mut stream =
        tokio::time::timeout(Duration::from_secs(5), local_ipc::connect(&address)).await??;
    local_ipc::write_message(
        &mut stream,
        &ControlRequest {
            protocol: PROTOCOL_VERSION,
            expected_instance,
            method,
        },
        Duration::from_secs(5),
    )
    .await?;
    let response = local_ipc::read_message(&mut stream, response_deadline)
        .await?
        .ok_or_else(|| anyhow::anyhow!("service disconnected before control response"))?;
    if let ControlResponse::Health { observation } = &response {
        ensure!(
            observation.protocol == PROTOCOL_VERSION
                && observation.version == version
                && observation.user == local_ipc::current_user()?,
            "incompatible native service identity"
        );
    }
    Ok(response)
}
