// SPDX-License-Identifier: Apache-2.0
//! One isolated PDF parser per process: the admission slot and its waiting rule.
//!
//! The service hosts every project owner, so a process-wide slot means one live
//! `__airk_pdf_worker` child for the whole service. A parser is created and torn
//! down for each document. The slot is released only after the parse returned,
//! which happens after its Windows job or Unix process group was closed and the
//! child reaped. Waiters are admitted in arrival order. A waiter gives up when
//! its work is interrupted (a stopped owner draining, a cancellation or a stalled
//! job) or when its bounded wait expires; neither starts a child, and neither
//! delays an owner's stop deadline by more than one poll interval.
//!
//! Neither refusal is a document outcome. [`slot_outcome`] classifies them for
//! the caller: an expired wait ([`PdfSlotBusy`]) defers the document, which is
//! retried later and keeps its previously indexed generation meanwhile; an
//! interrupted wait ([`PdfSlotInterrupted`]) stops the job at this safe point
//! with the document unsettled, and the host reports its own stop reason.
use anyhow::Result;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// Longest wait for the slot: four complete 15-second parses, the most a
/// single work grant's extraction threads can queue ahead of one document.
/// The parser's own 15-second budget starts only after admission.
pub(crate) const SLOT_WAIT: Duration = Duration::from_secs(60);
/// How often a waiter re-checks interruption while the slot is busy.
const POLL: Duration = Duration::from_millis(50);

/// Injected time source for the admission decision.
pub(crate) trait Clock {
    fn now(&self) -> Instant;
}
pub(crate) struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The waiter's work was interrupted before a parser started.
#[derive(Debug)]
pub struct PdfSlotInterrupted;
impl std::fmt::Display for PdfSlotInterrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PDF extraction stopped while waiting for the parser slot")
    }
}
impl std::error::Error for PdfSlotInterrupted {}

/// The bounded wait for the parser slot expired; no parser started.
#[derive(Debug)]
pub struct PdfSlotBusy;
impl std::fmt::Display for PdfSlotBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PDF parser slot stayed busy past its wait budget")
    }
}
impl std::error::Error for PdfSlotBusy {}

/// How the caller of a refused PDF extraction must treat its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotOutcome {
    /// The bounded wait expired: defer the document. It is neither extracted,
    /// failed nor missing, and it keeps its previously indexed generation.
    Busy,
    /// The owner is stopping: stop the job here, leaving the document unsettled.
    Interrupted,
}

/// The slot refusal behind `error`, if any. Interruption wins, so stopped work
/// is never deferred for a retry. Both the anyhow context stack and standard
/// error sources are searched.
pub(crate) fn slot_outcome(error: &anyhow::Error) -> Option<SlotOutcome> {
    if caused_by::<PdfSlotInterrupted>(error) {
        Some(SlotOutcome::Interrupted)
    } else if caused_by::<PdfSlotBusy>(error) {
        Some(SlotOutcome::Busy)
    } else {
        None
    }
}

fn caused_by<T: std::error::Error + Send + Sync + 'static>(error: &anyhow::Error) -> bool {
    error.is::<T>() || error.chain().any(|cause| cause.is::<T>())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Acquire,
    Wait(Duration),
    Interrupted,
    Expired,
}

/// Pure admission rule. Interruption wins over everything, so stopped work
/// never starts a parser; otherwise the earliest waiter takes a free slot.
pub(crate) fn admission_step(
    held: bool,
    first_in_line: bool,
    interrupted: bool,
    now: Instant,
    deadline: Instant,
) -> Step {
    if interrupted {
        return Step::Interrupted;
    }
    if !held && first_in_line {
        return Step::Acquire;
    }
    match deadline.checked_duration_since(now) {
        Some(remaining) if !remaining.is_zero() => Step::Wait(remaining.min(POLL)),
        _ => Step::Expired,
    }
}

#[derive(Default)]
struct State {
    held: bool,
    waiting: BTreeSet<u64>,
    next: u64,
}

#[derive(Default)]
pub(crate) struct PdfSlot {
    state: Mutex<State>,
    freed: Condvar,
}

/// Holding this is holding the one parser slot; dropping it releases it.
pub(crate) struct SlotPermit<'a>(&'a PdfSlot);
impl Drop for SlotPermit<'_> {
    fn drop(&mut self) {
        self.0.lock().held = false;
        self.0.freed.notify_all();
    }
}

impl PdfSlot {
    fn lock(&self) -> MutexGuard<'_, State> {
        // The state is two counters and a set; it stays consistent if a holder panics.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn acquire(
        &self,
        clock: &dyn Clock,
        budget: Duration,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<SlotPermit<'_>> {
        let deadline = clock.now() + budget;
        let mut state = self.lock();
        let ticket = state.next;
        state.next = state.next.wrapping_add(1);
        state.waiting.insert(ticket);
        loop {
            let first = state.waiting.first() == Some(&ticket);
            match admission_step(state.held, first, interrupted(), clock.now(), deadline) {
                Step::Acquire => {
                    state.waiting.remove(&ticket);
                    state.held = true;
                    return Ok(SlotPermit(self));
                }
                Step::Wait(slice) => {
                    state = self
                        .freed
                        .wait_timeout(state, slice)
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .0;
                }
                outcome @ (Step::Interrupted | Step::Expired) => {
                    state.waiting.remove(&ticket);
                    drop(state);
                    // The next waiter may now be first in line.
                    self.freed.notify_all();
                    return Err(if outcome == Step::Interrupted {
                        PdfSlotInterrupted.into()
                    } else {
                        PdfSlotBusy.into()
                    });
                }
            }
        }
    }
}

/// The one slot of this process.
pub(crate) fn process_slot() -> &'static PdfSlot {
    static SLOT: std::sync::OnceLock<PdfSlot> = std::sync::OnceLock::new();
    SLOT.get_or_init(PdfSlot::default)
}

/// Host-provided probe: true when the calling work must stop at once.
pub type Interruption = Arc<dyn Fn() -> bool + Send + Sync>;
thread_local! { static INTERRUPTION: RefCell<Option<Interruption>> = const { RefCell::new(None) }; }

/// Run `work` with an interruption probe visible to PDF admission on this
/// thread and on the extraction threads it starts. Restores the previous probe.
pub fn with_interruption<T>(probe: Interruption, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Interruption>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            let _ = INTERRUPTION.try_with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let _restore = Restore(INTERRUPTION.with(|slot| slot.borrow_mut().replace(probe)));
    work()
}

/// The calling thread's probe, for carrying into its extraction threads.
pub(crate) fn current_interruption() -> Option<Interruption> {
    INTERRUPTION.with(|slot| slot.borrow().clone())
}

/// True when the calling thread's work must stop before starting a parser.
pub fn interrupted() -> bool {
    current_interruption().is_some_and(|probe| probe())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Manual(Mutex<Instant>);
    impl Clock for Manual {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    #[test]
    fn admission_rule_is_ordered_bounded_and_interruptible() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(60);
        assert_eq!(
            admission_step(false, true, false, start, deadline),
            Step::Acquire
        );
        assert_eq!(
            admission_step(false, false, false, start, deadline),
            Step::Wait(POLL)
        );
        assert_eq!(
            admission_step(true, true, false, start, deadline),
            Step::Wait(POLL)
        );
        assert_eq!(
            admission_step(
                true,
                true,
                false,
                deadline - Duration::from_millis(10),
                deadline
            ),
            Step::Wait(Duration::from_millis(10))
        );
        assert_eq!(
            admission_step(true, true, false, deadline, deadline),
            Step::Expired
        );
        // Interrupted work never starts a parser, even when the slot is free.
        assert_eq!(
            admission_step(false, true, true, start, deadline),
            Step::Interrupted
        );
    }

    #[test]
    fn expiry_follows_the_injected_clock_and_frees_the_queue() -> Result<()> {
        let slot = PdfSlot::default();
        let clock = Manual(Mutex::new(Instant::now()));
        let held = slot.acquire(&clock, SLOT_WAIT, &|| false)?;
        // A zero budget on the injected clock expires without waiting for time.
        let error = match slot.acquire(&clock, Duration::ZERO, &|| false) {
            Ok(_) => panic!("a busy slot admitted a second parser"),
            Err(error) => error,
        };
        assert!(error.is::<PdfSlotBusy>(), "{error:#}");
        assert!(slot.lock().waiting.is_empty());
        drop(held);
        drop(slot.acquire(&clock, Duration::ZERO, &|| false)?);
        Ok(())
    }

    #[test]
    fn four_concurrent_extractions_never_have_more_than_one_live_parser() -> Result<()> {
        let slot = PdfSlot::default();
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let admitted = AtomicUsize::new(0);
        std::thread::scope(|threads| {
            for _ in 0..4 {
                threads.spawn(|| -> Result<()> {
                    let _permit = slot.acquire(&SystemClock, SLOT_WAIT, &|| false)?;
                    let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    // Let other threads queue behind the holder.
                    for _ in 0..1000 {
                        std::thread::yield_now();
                    }
                    live.fetch_sub(1, Ordering::SeqCst);
                    admitted.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                });
            }
        });
        assert_eq!(admitted.load(Ordering::SeqCst), 4);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[test]
    fn a_waiter_observes_interruption_and_a_failed_holder_still_releases() -> Result<()> {
        let slot = PdfSlot::default();
        let stop = AtomicBool::new(false);
        let held = slot.acquire(&SystemClock, SLOT_WAIT, &|| false)?;
        std::thread::scope(|threads| -> Result<()> {
            let waiter = threads.spawn(|| {
                slot.acquire(&SystemClock, SLOT_WAIT, &|| stop.load(Ordering::SeqCst))
                    .map(drop)
            });
            // Wait for the observed event: the waiter is queued.
            while slot.lock().waiting.is_empty() {
                std::thread::yield_now();
            }
            stop.store(true, Ordering::SeqCst);
            let error = waiter.join().expect("waiter panicked").unwrap_err();
            assert!(error.is::<PdfSlotInterrupted>(), "{error:#}");
            Ok(())
        })?;
        assert!(
            slot.lock().held,
            "an interrupted waiter released another's slot"
        );
        drop(held);
        // A holder whose parse failed releases the slot on its error path.
        let failed: Result<()> = (|| {
            let _permit = slot.acquire(&SystemClock, SLOT_WAIT, &|| false)?;
            anyhow::bail!("parser exited unsuccessfully")
        })();
        assert!(failed.is_err());
        assert!(!slot.lock().held);
        drop(slot.acquire(&SystemClock, Duration::ZERO, &|| false)?);
        Ok(())
    }

    #[test]
    fn an_interruption_probe_reaches_the_calling_thread_only_while_in_scope() {
        assert!(!interrupted());
        let flag = Arc::new(AtomicBool::new(true));
        let probe: Interruption = {
            let flag = flag.clone();
            Arc::new(move || flag.load(Ordering::SeqCst))
        };
        with_interruption(probe, || {
            assert!(interrupted());
            flag.store(false, Ordering::SeqCst);
            assert!(!interrupted());
            assert!(current_interruption().is_some());
        });
        assert!(current_interruption().is_none());
    }
}
