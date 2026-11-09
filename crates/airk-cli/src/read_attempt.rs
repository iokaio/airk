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

pub(crate) fn encode(
    context: &ReadAttemptContext,
    request_id: Incarnation,
) -> Result<ReadAttemptBudget> {
    // Tick first, Instant second: sampling overhead consumes rather than adds time.
    let ticks = airk_process::monotonic::now_ns()?;
    let now = Instant::now();
    let remaining = context
        .deadline()
        .saturating_duration_since(now)
        .min(MAX_READ_BUDGET);
    let remaining_ns = u64::try_from(remaining.as_nanos())?;
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
        expires_monotonic_ns: ticks
            .checked_add(remaining_ns)
            .ok_or_else(|| anyhow::anyhow!("native read deadline overflow"))?,
        remaining_ns,
        native_retry_expires_monotonic_ns,
    })
}

pub(crate) fn decode(budget: ReadAttemptBudget) -> Result<ReadAttemptContext> {
    // Instant first, tick second: IPC/decoding and sampling overhead are charged.
    let now = Instant::now();
    let ticks = airk_process::monotonic::now_ns()?;
    let remaining = budget.remaining_at(clock(), ticks)?;
    let mut context =
        ReadAttemptContext::new(now + remaining, Default::default()).remote_admission();
    if let Some(deadline) = budget.native_retry_expires_monotonic_ns {
        let allowance = std::time::Duration::from_nanos(deadline.saturating_sub(ticks))
            .min(remaining)
            .min(std::time::Duration::from_secs(5));
        context = context.with_native_retry_deadline(now + allowance);
    }
    Ok(context)
}

pub(crate) fn supported(version: &str) -> bool {
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    parts.is_ok_and(|parts| parts.len() == 3 && parts.as_slice() >= [1, 8, 14].as_slice())
}

#[cfg(test)]
mod tests;
