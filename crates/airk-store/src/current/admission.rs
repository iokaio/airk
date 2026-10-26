// SPDX-License-Identifier: Apache-2.0
//! Scoped native-work admission, limited to opening current-index connections.
use anyhow::{Result, ensure};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Policy {
    budget: Duration,
    checkpoint: fn() -> Result<()>,
    waiting: fn(bool),
}
thread_local! { static ADMISSION: Cell<Option<Policy>> = const { Cell::new(None) }; }

/// Query threads retain immediate typed rejection. Only connection admission is
/// repeated; caller work, captures, transactions and publications are never replayed.
pub fn with_work_admission<T>(
    budget: Duration,
    checkpoint: fn() -> Result<()>,
    waiting: fn(bool),
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ensure!(
        !budget.is_zero() && budget <= Duration::from_secs(30),
        "invalid current database admission budget"
    );
    struct Restore(Option<Policy>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ADMISSION.set(self.0);
        }
    }
    let _restore = Restore(ADMISSION.replace(Some(Policy {
        budget,
        checkpoint,
        waiting,
    })));
    work()
}

#[derive(Clone, Copy, Debug)]
pub struct CurrentOpenContention;
impl std::fmt::Display for CurrentOpenContention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("current writer connection admission is temporarily busy")
    }
}
impl std::error::Error for CurrentOpenContention {}

pub(super) fn writer_open_error(error: anyhow::Error) -> anyhow::Error {
    if matches!(error.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(code, _)) if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    {
        error.context(CurrentOpenContention)
    } else {
        error
    }
}

pub(super) fn admit_open<T>(mut open: impl FnMut() -> Result<T>) -> Result<T> {
    let Some(policy) = ADMISSION.get() else {
        return open();
    };
    struct Clear(fn(bool));
    impl Drop for Clear {
        fn drop(&mut self) {
            (self.0)(false);
        }
    }
    let _clear = Clear(policy.waiting);
    let started = Instant::now();
    loop {
        (policy.checkpoint)()?;
        match open() {
            Err(error)
                if (error.is::<super::CurrentReadContention>()
                    || error.is::<CurrentOpenContention>())
                    && started.elapsed() < policy.budget =>
            {
                (policy.waiting)(true);
                std::thread::sleep(
                    Duration::from_millis(10).min(policy.budget.saturating_sub(started.elapsed())),
                );
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sqlite_open_exclusion_is_retryable_and_terminal_errors_do_not_repeat_work() -> Result<()>
    {
        for (code, expected) in [
            (rusqlite::ffi::SQLITE_BUSY, true),
            (rusqlite::ffi::SQLITE_LOCKED, true),
            (rusqlite::ffi::SQLITE_CORRUPT, false),
            (rusqlite::ffi::SQLITE_IOERR, false),
        ] {
            let error = writer_open_error(
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into(),
            );
            assert_eq!(error.is::<CurrentOpenContention>(), expected);
        }
        let mut turns = 0;
        let mut opens = 0;
        let result: Result<()> = with_work_admission(
            Duration::from_secs(1),
            || Ok(()),
            |_| {},
            || {
                turns += 1;
                admit_open(|| {
                    opens += 1;
                    Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into())
                })
            },
        );
        assert_eq!((turns, opens), (1, 1));
        assert!(result.unwrap_err().is::<std::io::Error>());
        let result: Result<()> = with_work_admission(
            Duration::from_secs(1),
            || anyhow::bail!("canceled at admission"),
            |_| {},
            || {
                admit_open(|| {
                    opens += 1;
                    Ok(())
                })
            },
        );
        assert_eq!(opens, 1, "cancellation admitted a connection");
        assert!(result.is_err());
        Ok(())
    }
}
