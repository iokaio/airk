// SPDX-License-Identifier: Apache-2.0
//! A read's readiness annotation, observed after its handler (change-log
//! entry 54). Typed contention is repeated within a bound, as current-index
//! admission repeats a contended open; a read never silently carries a
//! shorter body because a concurrent observer held the publication.
use std::time::{Duration, Instant};

/// The bound on repeating a contended readiness observation, the same two
/// seconds current-index and quota admission allow by default.
pub(crate) const READINESS_RETRY_BUDGET: Duration = Duration::from_secs(2);

/// Run `observe`, repeating it while it fails with a typed "repeat the same
/// request" contention and the budget lasts. Any other outcome, and the last
/// contended error once the budget is spent, is returned unchanged, so the
/// caller can report that error with its typed retry hint.
pub(crate) fn observe_with_retry<T>(
    mut observe: impl FnMut() -> anyhow::Result<T>,
    budget: Duration,
    now: &dyn Fn() -> Instant,
    sleep: &dyn Fn(Duration),
) -> anyhow::Result<T> {
    let started = now();
    loop {
        match observe() {
            Err(error) => {
                let Some(hint) = airk_kernel::retrieval_retry::read_retry_hint(&error) else {
                    return Err(error);
                };
                let elapsed = now().saturating_duration_since(started);
                let wait = Duration::from_millis(u64::from(hint.retry_after_ms).clamp(1, 25));
                if elapsed + wait > budget {
                    return Err(error);
                }
                sleep(wait);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, sync::Mutex};

    /// A typed "repeat the same request" contention (the live failure was
    /// the current reader's own contention, which carries the same hint).
    fn contention() -> anyhow::Error {
        anyhow::Error::new(airk_kernel::retrieval_retry::SelectionReadAdmissionContention)
            .context("readiness observation")
    }

    #[test]
    fn a_contended_observation_is_repeated_until_it_succeeds() {
        let base = Instant::now();
        let offset = Mutex::new(Duration::ZERO);
        let calls = Cell::new(0);
        let result = observe_with_retry(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err(contention())
                } else {
                    Ok("ready")
                }
            },
            READINESS_RETRY_BUDGET,
            &|| base + *offset.lock().unwrap(),
            &|wait| *offset.lock().unwrap() += wait,
        );
        assert_eq!(result.unwrap(), "ready");
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn contention_past_the_budget_keeps_its_typed_retry_hint() {
        let base = Instant::now();
        let offset = Mutex::new(Duration::ZERO);
        let calls = Cell::new(0);
        let error = observe_with_retry(
            || -> anyhow::Result<()> {
                calls.set(calls.get() + 1);
                Err(contention())
            },
            READINESS_RETRY_BUDGET,
            &|| base + *offset.lock().unwrap(),
            &|wait| *offset.lock().unwrap() += wait,
        )
        .unwrap_err();
        let hint = airk_kernel::retrieval_retry::read_retry_hint(&error).unwrap();
        assert_eq!(hint.action, "repeat_same_request");
        assert!(*offset.lock().unwrap() <= READINESS_RETRY_BUDGET);
        assert!(calls.get() > 1);
    }

    #[test]
    fn other_errors_are_not_repeated() {
        let calls = Cell::new(0);
        let error = observe_with_retry(
            || -> anyhow::Result<()> {
                calls.set(calls.get() + 1);
                anyhow::bail!("corrupt index")
            },
            READINESS_RETRY_BUDGET,
            &Instant::now,
            &|_| panic!("no wait for a terminal error"),
        )
        .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert!(airk_kernel::retrieval_retry::read_retry_hint(&error).is_none());
    }
}
