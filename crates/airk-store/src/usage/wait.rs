// SPDX-License-Identifier: Apache-2.0
//! Bounded FIFO admission for usage callers within one process. The file lock
//! remains authoritative across processes and alternative spellings of a path.
use anyhow::{Result, ensure};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

const MAX_QUEUES: usize = 4096;
const MAX_WAITERS: usize = 4096;
static QUEUES: OnceLock<Mutex<HashMap<PathBuf, Weak<Queue>>>> = OnceLock::new();

#[derive(Default)]
struct Queue {
    order: Mutex<VecDeque<Arc<()>>>,
    changed: Condvar,
}

pub(super) struct Turn {
    queue: Arc<Queue>,
    token: Arc<()>,
}

pub(super) fn acquire(path: &Path, started: Instant, budget: Duration) -> Result<Turn> {
    let queue = {
        let mut queues = QUEUES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        queues.retain(|_, queue| queue.strong_count() != 0);
        if let Some(queue) = queues.get(path).and_then(Weak::upgrade) {
            queue
        } else {
            ensure!(
                queues.len() < MAX_QUEUES,
                "usage lock queue capacity reached"
            );
            let queue = Arc::new(Queue::default());
            queues.insert(path.to_path_buf(), Arc::downgrade(&queue));
            queue
        }
    };
    Turn::enqueue(queue)?.wait(started, budget)
}

impl Turn {
    fn enqueue(queue: Arc<Queue>) -> Result<Self> {
        let token = Arc::new(());
        {
            let mut order = queue
                .order
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            ensure!(
                order.len() < MAX_WAITERS,
                "usage lock waiter capacity reached"
            );
            order.push_back(token.clone());
        }
        Ok(Self { queue, token })
    }

    fn wait(self, started: Instant, budget: Duration) -> Result<Self> {
        {
            let mut order = self
                .queue
                .order
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while !order
                .front()
                .is_some_and(|token| Arc::ptr_eq(token, &self.token))
            {
                let remaining = budget.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    // Unlock before dropping the ticket; its removal wakes successors.
                    drop(order);
                    return Err(anyhow::Error::from(std::fs::TryLockError::WouldBlock)
                        .context("usage state is busy; observation may be dropped"));
                }
                (order, _) = self
                    .queue
                    .changed
                    .wait_timeout(order, remaining)
                    .unwrap_or_else(|error| error.into_inner());
            }
        }
        Ok(self)
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        let mut order = self
            .queue
            .order
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        order.retain(|token| !Arc::ptr_eq(token, &self.token));
        self.queue.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_worker_cannot_overtake_a_queued_worker() -> Result<()> {
        let queue = Arc::new(Queue::default());
        let first = Turn::enqueue(queue.clone())?.wait(Instant::now(), Duration::ZERO)?;
        let waiting = Turn::enqueue(queue.clone())?;
        drop(first);
        let returning = Turn::enqueue(queue.clone())?;
        // This worker would acquire a free OS lock, but must yield to the waiter.
        assert!(returning.wait(Instant::now(), Duration::ZERO).is_err());
        let waiting = waiting.wait(Instant::now(), Duration::ZERO)?;
        drop(waiting);
        assert!(queue.order.lock().unwrap().is_empty());
        Ok(())
    }

    #[test]
    fn timed_out_waiter_is_removed_without_blocking_successors() -> Result<()> {
        let queue = Arc::new(Queue::default());
        let first = Turn::enqueue(queue.clone())?;
        let expired = Turn::enqueue(queue.clone())?;
        let successor = Turn::enqueue(queue.clone())?;
        assert!(expired.wait(Instant::now(), Duration::ZERO).is_err());
        assert_eq!(queue.order.lock().unwrap().len(), 2);
        drop(first);
        drop(successor.wait(Instant::now(), Duration::ZERO)?);
        assert!(queue.order.lock().unwrap().is_empty());
        Ok(())
    }

    #[test]
    fn release_wakes_waiter_and_other_paths_remain_independent() -> Result<()> {
        let root = tempfile::tempdir()?;
        let first_path = root.path().join("first");
        let first = acquire(&first_path, Instant::now(), Duration::from_secs(2))?;
        let waiting = Turn::enqueue(first.queue.clone())?;
        let other = acquire(&root.path().join("other"), Instant::now(), Duration::ZERO)?;
        std::thread::scope(|threads| -> Result<()> {
            let worker =
                threads.spawn(move || waiting.wait(Instant::now(), Duration::from_secs(2)));
            drop(first);
            drop(worker.join().unwrap()?);
            Ok(())
        })?;
        drop(other);
        Ok(())
    }
}
