// SPDX-License-Identifier: Apache-2.0
//! Native read budget conversion samples monotonic clocks conservatively.
use airk_core::{
    identity::Incarnation,
    read_attempt::{MAX_READ_BUDGET, ReadAttemptBudget, ReadClock},
};
use airk_mcp::read_attempt::ReadAttemptContext;
use anyhow::Result;
use std::time::Instant;

fn clock() -> ReadClock {
    #[cfg(unix)]
    {
        ReadClock::UnixMonotonic
    }
    #[cfg(windows)]
    {
        ReadClock::WindowsPerformanceCounter
    }
}

pub fn encode(context: &ReadAttemptContext, request_id: Incarnation) -> Result<ReadAttemptBudget> {
    // Tick first, Instant second: sampling overhead consumes rather than adds time.
    let ticks = airk_process::monotonic::now_ns()?;
    let now = Instant::now();
    encode_sampled(context, request_id, ticks, now)
}

fn encode_sampled(
    context: &ReadAttemptContext,
    request_id: Incarnation,
    ticks: u64,
    now: Instant,
) -> Result<ReadAttemptBudget> {
    anyhow::ensure!(
        !context.cancellation().is_cancelled(),
        "native read canceled"
    );
    let remaining = context
        .deadline()
        .saturating_duration_since(now)
        .min(MAX_READ_BUDGET);
    let remaining_ns = u64::try_from(remaining.as_nanos())?;
    anyhow::ensure!(remaining_ns > 0, "native read deadline expired");
    if let Some(captured) = context.captured_transport_attempt() {
        anyhow::ensure!(
            captured.request_id == request_id,
            "captured native read identity mismatch"
        );
        let actual = captured.remaining_at(clock(), ticks)?;
        anyhow::ensure!(!actual.is_zero(), "native read deadline expired");
        return Ok(ReadAttemptBudget {
            remaining_ns: remaining_ns.min(u64::try_from(actual.as_nanos())?),
            ..captured
        });
    }
    let original = context.wire_attempt();
    let expires_monotonic_ns = ticks
        .checked_add(remaining_ns)
        .ok_or_else(|| anyhow::anyhow!("native read deadline overflow"))?;
    let expires_monotonic_ns = if let Some(prior) = original {
        let actual = prior.remaining_at(clock(), ticks)?;
        anyhow::ensure!(
            !actual.is_zero() && prior.request_id == request_id,
            "original native read expired or identity changed"
        );
        expires_monotonic_ns.min(prior.expires_monotonic_ns)
    } else {
        expires_monotonic_ns
    };
    let native_retry_expires_monotonic_ns = context
        .native_retry_deadline()
        .map(|deadline| -> Result<_> {
            let allowance = u64::try_from(
                deadline
                    .saturating_duration_since(now)
                    .min(remaining)
                    .as_nanos(),
            )?;
            ticks
                .checked_add(allowance)
                .ok_or_else(|| anyhow::anyhow!("native settings deadline overflow"))
        })
        .transpose()?;
    Ok(ReadAttemptBudget {
        schema: 1,
        request_id,
        clock: clock(),
        expires_monotonic_ns,
        remaining_ns: remaining_ns.min(expires_monotonic_ns.saturating_sub(ticks)),
        native_retry_expires_monotonic_ns: native_retry_expires_monotonic_ns
            .map(|expiry| expiry.min(expires_monotonic_ns)),
    })
}

pub fn decode(budget: ReadAttemptBudget) -> Result<ReadAttemptContext> {
    // Instant first, tick second: IPC/decoding and sampling overhead are charged.
    let now = Instant::now();
    let ticks = airk_process::monotonic::now_ns()?;
    decode_sampled(budget, now, ticks)
}

fn decode_sampled(
    budget: ReadAttemptBudget,
    now: Instant,
    ticks: u64,
) -> Result<ReadAttemptContext> {
    let remaining = budget.remaining_at(clock(), ticks)?;
    let mut context = ReadAttemptContext::new(now + remaining, Default::default())
        .with_wire_attempt(budget)?
        .remote_admission();
    if let Some(deadline) = budget.native_retry_expires_monotonic_ns {
        let allowance = std::time::Duration::from_nanos(deadline.saturating_sub(ticks))
            .min(remaining)
            .min(std::time::Duration::from_secs(5));
        context = context.with_native_retry_deadline(now + allowance);
    }
    Ok(context)
}

pub fn supported(version: &str) -> bool {
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    parts.is_ok_and(|parts| parts.len() == 3 && parts.as_slice() >= [1, 8, 14].as_slice())
}

#[cfg(test)]
mod tests;
