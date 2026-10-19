// SPDX-License-Identifier: Apache-2.0
//! Fixed, lazy workers with one bounded batch and source-order result delivery.
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtractionExecution {
    pub workers: usize,
    pub max_in_flight_source_bytes: usize,
}

impl Default for ExtractionExecution {
    fn default() -> Self {
        Self {
            workers: 1,
            max_in_flight_source_bytes: 16 * 1024 * 1024,
        }
    }
}

impl ExtractionExecution {
    pub const MAX_PARALLEL_SOURCE_BYTES: usize = crate::MAX_STRUCTURED_BYTES;
    pub const MAX_RESULT_BYTES: usize = airk_core::extraction::MAX_PAYLOAD_BYTES;

    pub fn validate(self) -> Result<()> {
        ensure!(
            (1..=4).contains(&self.workers),
            "extraction workers must be 1..4"
        );
        ensure!(
            (1..=4 * crate::MAX_STRUCTURED_BYTES).contains(&self.max_in_flight_source_bytes),
            "extraction in-flight source allowance must be 1 byte..16 MiB"
        );
        Ok(())
    }
}

pub(crate) struct Completed<R> {
    pub result: Result<R>,
    pub work_us: u64,
}

pub(crate) fn execute<J, R>(job: J, work: fn(J) -> R) -> Completed<R> {
    let started = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| work(job)))
        .map_err(|_| anyhow::anyhow!("extraction worker panicked; previous generation retained"));
    Completed {
        result,
        work_us: micros(started),
    }
}

fn micros(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
}

pub(crate) struct OrderedPool<J, R> {
    execution: ExtractionExecution,
    work: fn(J) -> R,
    input: Option<mpsc::SyncSender<(u64, J)>>,
    output: Option<mpsc::Receiver<(u64, Completed<R>)>>,
    workers: Vec<JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    pending: VecDeque<(u64, usize)>,
    ready: BTreeMap<u64, Completed<R>>,
    next_id: u64,
    source_bytes: usize,
    pub source_peak_bytes: usize,
    pub wait_us: u64,
}

impl<J: Send + 'static, R: Send + 'static> OrderedPool<J, R> {
    pub fn new(execution: ExtractionExecution, work: fn(J) -> R) -> Result<Self> {
        execution.validate()?;
        Ok(Self {
            execution,
            work,
            input: None,
            output: None,
            workers: vec![],
            cancelled: Arc::new(AtomicBool::new(false)),
            pending: VecDeque::new(),
            ready: BTreeMap::new(),
            next_id: 0,
            source_bytes: 0,
            source_peak_bytes: 0,
            wait_us: 0,
        })
    }

    pub fn workers_spawned(&self) -> usize {
        self.workers.len()
    }

    pub fn can_admit(&self, bytes: usize) -> bool {
        self.pending.len() < self.execution.workers
            && bytes
                <= self
                    .execution
                    .max_in_flight_source_bytes
                    .saturating_sub(self.source_bytes)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn start(&mut self) -> Result<()> {
        if self.input.is_some() {
            return Ok(());
        }
        let (input, incoming) = mpsc::sync_channel::<(u64, J)>(self.execution.workers);
        let (outgoing, output) = mpsc::sync_channel(self.execution.workers);
        let incoming = Arc::new(Mutex::new(incoming));
        self.input = Some(input);
        self.output = Some(output);
        for ordinal in 0..self.execution.workers {
            let incoming = Arc::clone(&incoming);
            let outgoing = outgoing.clone();
            let cancelled = Arc::clone(&self.cancelled);
            let work = self.work;
            self.workers.push(
                std::thread::Builder::new()
                    .name(format!("airk-extract-{ordinal}"))
                    .spawn(move || {
                        loop {
                            let next = match incoming.lock() {
                                Ok(receiver) => receiver.recv(),
                                Err(_) => break,
                            };
                            let Ok((id, job)) = next else {
                                break;
                            };
                            if cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            let result = execute(job, work);
                            if outgoing.send((id, result)).is_err() {
                                break;
                            }
                        }
                    })
                    .context("start extraction worker")?,
            );
        }
        Ok(())
    }

    pub fn push(&mut self, job: J, bytes: usize) -> Result<()> {
        ensure!(
            self.can_admit(bytes),
            "extraction batch admission exceeded its bound"
        );
        self.start()?;
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .context("extraction job ordinal overflow")?;
        self.input
            .as_ref()
            .unwrap()
            .send((id, job))
            .map_err(|_| anyhow::anyhow!("extraction workers stopped"))?;
        self.pending.push_back((id, bytes));
        self.source_bytes += bytes;
        self.source_peak_bytes = self.source_peak_bytes.max(self.source_bytes);
        Ok(())
    }

    pub fn next(&mut self) -> Result<Option<Completed<R>>> {
        let Some(&(wanted, bytes)) = self.pending.front() else {
            return Ok(None);
        };
        while !self.ready.contains_key(&wanted) {
            let started = Instant::now();
            let result = self
                .output
                .as_ref()
                .context("extraction result channel missing")?
                .recv();
            self.wait_us = self.wait_us.saturating_add(micros(started));
            let (id, completed) = result.context("extraction worker exited without a result")?;
            ensure!(
                self.pending.iter().any(|&(pending, _)| pending == id)
                    && !self.ready.contains_key(&id),
                "invalid extraction result ordinal"
            );
            self.ready.insert(id, completed);
        }
        self.pending.pop_front();
        self.source_bytes -= bytes;
        Ok(self.ready.remove(&wanted))
    }
}

impl<J, R> Drop for OrderedPool<J, R> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        // Closing both ends releases blocked receivers and senders before join.
        self.input.take();
        self.output.take();
        self.ready.clear();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn reordered(value: usize) -> usize {
        if value == 0 {
            std::thread::sleep(Duration::from_millis(30));
        }
        value
    }

    #[test]
    fn bounded_batch_delivers_source_order_and_starts_lazily() -> Result<()> {
        let mut pool = OrderedPool::new(
            ExtractionExecution {
                workers: 4,
                max_in_flight_source_bytes: 7,
            },
            reordered,
        )?;
        assert_eq!(pool.workers_spawned(), 0);
        pool.push(0, 3)?;
        pool.push(1, 4)?;
        assert!(!pool.can_admit(1));
        assert_eq!(pool.source_peak_bytes, 7);
        assert_eq!(pool.next()?.unwrap().result?, 0);
        assert_eq!(pool.next()?.unwrap().result?, 1);
        assert!(pool.is_empty());
        assert!(pool.can_admit(7));
        assert!(!pool.can_admit(8));
        for value in 0..4 {
            pool.push(value, 0)?;
        }
        assert!(!pool.can_admit(0));
        for value in 0..4 {
            assert_eq!(pool.next()?.unwrap().result?, value);
        }
        Ok(())
    }

    #[test]
    fn invalid_execution_limits_are_rejected_before_workers_start() {
        for execution in [
            ExtractionExecution {
                workers: 0,
                ..Default::default()
            },
            ExtractionExecution {
                workers: 5,
                ..Default::default()
            },
            ExtractionExecution {
                max_in_flight_source_bytes: 0,
                ..Default::default()
            },
            ExtractionExecution {
                max_in_flight_source_bytes: 16 * 1024 * 1024 + 1,
                ..Default::default()
            },
        ] {
            assert!(OrderedPool::new(execution, reordered).is_err());
        }
    }

    #[test]
    fn panic_is_ordered_and_dropping_full_results_joins_workers() -> Result<()> {
        fn work(value: usize) -> usize {
            if value == 0 {
                panic!("fixture worker failure");
            }
            value
        }
        let mut pool = OrderedPool::new(
            ExtractionExecution {
                workers: 4,
                ..Default::default()
            },
            work,
        )?;
        for value in 0..4 {
            pool.push(value, 1)?;
        }
        assert!(
            pool.next()?
                .unwrap()
                .result
                .unwrap_err()
                .to_string()
                .contains("panicked")
        );
        drop(pool);
        Ok(())
    }
}
