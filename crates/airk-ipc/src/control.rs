// SPDX-License-Identifier: Apache-2.0
//! I1 control transport: one framed request per connection with bounded
//! deadlines, the `registry_busy` retry window, and the Health identity check.
//! Moved verbatim from `service_runtime` (split plan G2); the baseline-gate
//! policy stays with the service wrappers that call these functions.
use super::endpoint as local_ipc;
use airk_core::service::{ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION};
use anyhow::{Context, Result, ensure};
use std::{path::Path, time::Duration};

pub async fn registry_busy_retry<F, Fut>(mut request: F) -> Result<ControlResponse>
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

pub async fn request_once(
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
    } else if matches!(method, ControlMethod::ToolsCall { .. }) {
        // The service runs the call under the CLI's 125 s tool context.
        Duration::from_secs(135)
    } else {
        Duration::from_secs(5)
    };
    let original_deadline = match &method {
        ControlMethod::ArbiterContextAction { request } => {
            let sampled_at = tokio::time::Instant::now();
            let remaining = arbiter_remaining(
                Some(request.read_attempt),
                request.remaining_ms,
                host_read_clock(),
                airk_process::monotonic::now_ns()?,
            )?;
            Some(
                sampled_at
                    .checked_add(remaining)
                    .context("Joint Arbiter native deadline overflow")?,
            )
        }
        ControlMethod::ArbiterNextRead { request } => {
            // Sample Tokio first so clock conversion can only shorten the
            // original same-host absolute expiry, never add sampling time.
            let sampled_at = tokio::time::Instant::now();
            let remaining = arbiter_remaining(
                request.read_attempt,
                request.remaining_ms,
                host_read_clock(),
                airk_process::monotonic::now_ns()?,
            )?;
            Some(
                sampled_at
                    .checked_add(remaining)
                    .context("Arbiter native deadline overflow")?,
            )
        }
        ControlMethod::ToolsCall {
            read_attempt: Some(attempt),
            ..
        } => {
            let sampled_at = tokio::time::Instant::now();
            let remaining = read_remaining(
                *attempt,
                host_read_clock(),
                airk_process::monotonic::now_ns()?,
            )?;
            Some(
                sampled_at
                    .checked_add(remaining)
                    .context("Original native read deadline overflow")?,
            )
        }
        _ => None,
    };
    let response_budget = method.response_frame_budget();
    let request_budget = method.request_frame_budget();
    let address = local_ipc::endpoint(root, "control")?;
    let connect = local_ipc::connect(&address);
    let mut stream = if let Some(deadline) = original_deadline {
        tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(5)),
            connect,
        )
        .await
        .context("Original native read connect deadline exceeded")??
    } else {
        tokio::time::timeout(Duration::from_secs(5), connect).await??
    };
    let request = ControlRequest {
        protocol: PROTOCOL_VERSION,
        expected_instance,
        method,
    };
    let response = exchange_once(
        &mut stream,
        &request,
        request_budget,
        response_budget,
        response_deadline,
        original_deadline,
    )
    .await?;
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

fn host_read_clock() -> airk_core::read_attempt::ReadClock {
    if cfg!(windows) {
        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
    } else {
        airk_core::read_attempt::ReadClock::UnixMonotonic
    }
}

fn read_remaining(
    attempt: airk_core::read_attempt::ReadAttemptBudget,
    clock: airk_core::read_attempt::ReadClock,
    now_ns: u64,
) -> Result<Duration> {
    let remaining = attempt.remaining_at(clock, now_ns)?;
    ensure!(
        !remaining.is_zero(),
        "Original native read deadline exhausted before send"
    );
    Ok(remaining)
}

fn arbiter_remaining(
    attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
    remaining_ms: u64,
    clock: airk_core::read_attempt::ReadClock,
    now_ns: u64,
) -> Result<Duration> {
    ensure!(
        (1..=30_000).contains(&remaining_ms),
        "invalid Arbiter remaining budget"
    );
    let remaining = attempt
        .context("native Arbiter attempt budget required")?
        .remaining_at(clock, now_ns)?
        .min(Duration::from_millis(remaining_ms))
        .min(Duration::from_secs(30));
    ensure!(
        !remaining.is_zero(),
        "Original native read deadline exhausted before send"
    );
    Ok(remaining)
}

async fn exchange_once<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    request: &ControlRequest,
    request_budget: usize,
    response_budget: usize,
    response_deadline: Duration,
    original_deadline: Option<tokio::time::Instant>,
) -> Result<ControlResponse> {
    let exchange = async {
        if let Some(deadline) = original_deadline {
            // Encoding is synchronous. Recheck after it and before the first
            // write so crossing expiry while encoding cannot send a request.
            let mut frame = Vec::new();
            crate::frame::write_frame_limited(&mut frame, request, request_budget)?;
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Original native read deadline exhausted before send"
            );
            use tokio::io::AsyncWriteExt;
            tokio::time::timeout_at(
                deadline.min(tokio::time::Instant::now() + Duration::from_secs(5)),
                async {
                    stream.write_all(&frame).await?;
                    stream.flush().await
                },
            )
            .await
            .context("Original native read write deadline exceeded")??;
        } else {
            local_ipc::write_message_limited(
                stream,
                request,
                Duration::from_secs(5),
                request_budget,
            )
            .await?;
        }
        let read_budget = if let Some(deadline) = original_deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            ensure!(
                !remaining.is_zero(),
                "Original native read deadline exhausted before response"
            );
            remaining
        } else {
            response_deadline
        };
        local_ipc::read_message_limited(stream, read_budget, response_budget)
            .await?
            .ok_or_else(|| anyhow::anyhow!("service disconnected before control response"))
    };
    if let Some(deadline) = original_deadline {
        tokio::time::timeout_at(deadline, exchange)
            .await
            .context("Original native read deadline exhausted in exchange")?
    } else {
        exchange.await
    }
}

#[cfg(test)]
mod tests;
