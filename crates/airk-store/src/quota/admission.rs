// SPDX-License-Identifier: Apache-2.0
//! Bounded admission to the shared quota ledger; never replays ledger operations.
use anyhow::{Result, ensure};
use std::{
    cell::Cell,
    fs::File,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Policy {
    budget: Duration,
    checkpoint: fn() -> Result<()>,
    waiting: fn(bool),
}
thread_local! { static POLICY: Cell<Option<Policy>> = const { Cell::new(None) }; }

#[derive(Clone, Copy, Debug)]
pub struct QuotaContention;
impl std::fmt::Display for QuotaContention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("shared storage accounting ledger is temporarily busy")
    }
}
impl std::error::Error for QuotaContention {}

pub fn with_lock_admission<T>(
    budget: Duration,
    checkpoint: fn() -> Result<()>,
    waiting: fn(bool),
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ensure!(
        !budget.is_zero() && budget <= Duration::from_secs(30),
        "invalid quota admission budget"
    );
    struct Restore(Option<Policy>);
    impl Drop for Restore {
        fn drop(&mut self) {
            POLICY.set(self.0);
        }
    }
    let _restore = Restore(POLICY.replace(Some(Policy {
        budget,
        checkpoint,
        waiting,
    })));
    work()
}

pub(super) fn acquire(file: &File, deadline: Option<Instant>) -> Result<()> {
    // An explicit caller deadline (optional accounting) remains authoritative.
    let policy = deadline.is_none().then(|| POLICY.get()).flatten();
    let budget = policy.map_or(Duration::from_secs(2), |p| p.budget);
    let started = Instant::now();
    struct Clear(Option<Policy>);
    impl Drop for Clear {
        fn drop(&mut self) {
            if let Some(p) = self.0 {
                (p.waiting)(false);
            }
        }
    }
    let _clear = Clear(policy);
    loop {
        if let Some(p) = policy {
            (p.checkpoint)()?;
        }
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock)
                if deadline.is_none() && started.elapsed() < budget =>
            {
                if let Some(p) = policy {
                    (p.waiting)(true);
                }
                std::thread::sleep(
                    Duration::from_millis(10).min(budget.saturating_sub(started.elapsed())),
                );
            }
            Err(error @ std::fs::TryLockError::WouldBlock) => {
                // Keep the native cause for existing bounded admission callers,
                // while durable diagnostics can identify quota-ledger contention.
                return Err(anyhow::Error::new(error).context(QuotaContention));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::OpenOptions,
        sync::atomic::{AtomicBool, Ordering},
    };
    static WAITING: AtomicBool = AtomicBool::new(false);
    fn waiting(value: bool) {
        WAITING.store(value, Ordering::SeqCst);
    }
    #[test]
    fn contended_ledger_waits_without_replaying_work_and_cancellation_preserves_lock() -> Result<()>
    {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("ledger.lock");
        let first = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        first.lock()?;
        let second = OpenOptions::new().read(true).write(true).open(&path)?;
        let busy = acquire(&second, Some(Instant::now())).unwrap_err();
        assert!(busy.is::<QuotaContention>());
        assert!(matches!(
            busy.downcast_ref::<std::fs::TryLockError>(),
            Some(std::fs::TryLockError::WouldBlock)
        ));
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2200));
            drop(first);
        });
        let mut turns = 0;
        with_lock_admission(
            Duration::from_secs(5),
            || Ok(()),
            waiting,
            || {
                turns += 1;
                acquire(&second, None)
            },
        )?;
        release.join().unwrap();
        assert_eq!(turns, 1);
        assert!(!WAITING.load(Ordering::SeqCst));
        let third = OpenOptions::new().read(true).write(true).open(path)?;
        let result = with_lock_admission(
            Duration::from_secs(1),
            || anyhow::bail!("canceled"),
            waiting,
            || acquire(&third, None),
        );
        assert!(result.is_err());
        assert!(matches!(
            third.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(!WAITING.load(Ordering::SeqCst));
        Ok(())
    }
}
