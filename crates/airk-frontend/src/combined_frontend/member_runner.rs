// SPDX-License-Identifier: Apache-2.0
//! U107: the member reads of one combined read, run concurrently. At most
//! `capacity` reads are in flight (the read's query concurrency; 1 is the
//! former serial loop), later members start only as earlier ones finish,
//! and outputs are yielded in canonical member order. When an output in
//! that order asks to stop, the outputs up to it are returned, members not
//! yet started never start, and members still in flight are dropped.
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// One member read.
pub(super) type MemberFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Run `reads` in order with at most `capacity` in flight; see the module
/// documentation. `stop` sees each output in canonical order.
pub(super) async fn run_in_order<'a, T>(
    capacity: usize,
    reads: impl IntoIterator<Item = MemberFuture<'a, T>>,
    stop: impl Fn(&T) -> bool,
) -> Vec<T> {
    let capacity = capacity.max(1);
    let mut pending = reads.into_iter();
    let mut running: Vec<(usize, MemberFuture<'a, T>)> = Vec::new();
    let mut finished: Vec<Option<T>> = Vec::new();
    let mut started = 0usize;
    let mut yielded = 0usize;
    let mut outputs = Vec::new();
    loop {
        while running.len() < capacity {
            match pending.next() {
                Some(read) => {
                    running.push((started, read));
                    finished.push(None);
                    started += 1;
                }
                None => break,
            }
        }
        // Yield the completed prefix, in canonical order.
        while let Some(output) = finished.get_mut(yielded).and_then(Option::take) {
            yielded += 1;
            let halt = stop(&output);
            outputs.push(output);
            if halt {
                return outputs;
            }
        }
        if running.is_empty() {
            return outputs;
        }
        std::future::poll_fn(|cx: &mut Context<'_>| {
            let mut progressed = false;
            running.retain_mut(|(offset, read)| match read.as_mut().poll(cx) {
                Poll::Ready(output) => {
                    finished[*offset] = Some(output);
                    progressed = true;
                    false
                }
                Poll::Pending => true,
            });
            if progressed {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    /// Fake member reads: each records its start and end in `log`, tracks
    /// the in-flight count and answers its own offset after `delay_ms`.
    struct Fakes {
        log: Arc<Mutex<Vec<String>>>,
        running: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }
    impl Fakes {
        fn new() -> Self {
            Self {
                log: Arc::default(),
                running: Arc::default(),
                peak: Arc::default(),
            }
        }
        fn read(&self, offset: usize, delay_ms: u64) -> MemberFuture<'static, usize> {
            let (log, running, peak) = (
                Arc::clone(&self.log),
                Arc::clone(&self.running),
                Arc::clone(&self.peak),
            );
            Box::pin(async move {
                log.lock().unwrap().push(format!("start {offset}"));
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                running.fetch_sub(1, Ordering::SeqCst);
                log.lock().unwrap().push(format!("end {offset}"));
                offset
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn capacity_one_is_the_serial_loop_and_capacity_n_keeps_the_order() {
        let delays = [30u64, 5, 20, 1, 10];
        let serial = Fakes::new();
        let out = run_in_order(
            1,
            delays.iter().enumerate().map(|(n, d)| serial.read(n, *d)),
            |_| false,
        )
        .await;
        assert_eq!(out, vec![0, 1, 2, 3, 4]);
        assert_eq!(serial.peak.load(Ordering::SeqCst), 1);
        let expected: Vec<String> = (0..5)
            .flat_map(|n| [format!("start {n}"), format!("end {n}")])
            .collect();
        assert_eq!(*serial.log.lock().unwrap(), expected);
        for capacity in [2, 3, 8] {
            let fakes = Fakes::new();
            let out = run_in_order(
                capacity,
                delays.iter().enumerate().map(|(n, d)| fakes.read(n, *d)),
                |_| false,
            )
            .await;
            assert_eq!(out, vec![0, 1, 2, 3, 4], "capacity {capacity}");
            assert!(fakes.peak.load(Ordering::SeqCst) <= capacity);
            assert!(fakes.peak.load(Ordering::SeqCst) >= 2);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_member_does_not_delay_a_fast_one() {
        let fakes = Fakes::new();
        let started = tokio::time::Instant::now();
        let finished = Arc::new(Mutex::new(Vec::new()));
        let reads = [1000u64, 10].into_iter().enumerate().map(|(n, delay)| {
            let read = fakes.read(n, delay);
            let finished = Arc::clone(&finished);
            Box::pin(async move {
                let offset = read.await;
                finished.lock().unwrap().push((offset, started.elapsed()));
                offset
            }) as MemberFuture<'static, usize>
        });
        assert_eq!(run_in_order(2, reads, |_| false).await, vec![0, 1]);
        let finished = finished.lock().unwrap().clone();
        // The fast member finished at its own 10 ms, not after the slow one.
        assert_eq!(finished[0], (1, Duration::from_millis(10)));
        assert_eq!(finished[1], (0, Duration::from_millis(1000)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_in_order_ends_the_run_before_later_members_start() {
        let fakes = Fakes::new();
        let out = run_in_order(1, (0..4).map(|n| fakes.read(n, 5)), |offset| *offset == 1).await;
        assert_eq!(out, vec![0, 1]);
        let log = fakes.log.lock().unwrap().clone();
        assert!(
            !log.iter()
                .any(|entry| entry == "start 2" || entry == "start 3")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn members_starting_after_the_deadline_are_cut_off_in_order() {
        // The combined owner checks its overall deadline when a member
        // starts; members starting after it answer "skipped" at once.
        let started = tokio::time::Instant::now();
        let deadline = Duration::from_millis(25);
        let reads = [20u64, 20, 20].into_iter().enumerate().map(|(n, delay)| {
            Box::pin(async move {
                if started.elapsed() >= deadline {
                    return (n, false);
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                (n, true)
            }) as MemberFuture<'static, (usize, bool)>
        });
        // Serial: the third member starts at 40 ms, after the deadline.
        assert_eq!(
            run_in_order(1, reads, |_| false).await,
            vec![(0, true), (1, true), (2, false)]
        );
    }
}
