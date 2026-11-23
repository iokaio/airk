// SPDX-License-Identifier: Apache-2.0
//! The concurrent reads of one wave (L3-9). A wave's reads run on scoped
//! threads, at most `READS_IN_FLIGHT` at a time, and come back in dispatch
//! order whatever order they completed in. Reads are claimed in dispatch
//! order, each claim only after a fresh check that the reserve is not
//! reached and no earlier read halted the wave; a worker that passed that
//! check may still claim one read after another worker saw the stop, and
//! that read runs. Every claimed read runs, so the reads that ran are always
//! a prefix of the wave: the unstarted rest is released exactly as in a
//! sequential driver. A read that panics halts the wave; the panic of the
//! lowest such read is raised again after every worker has stopped. Each
//! threaded read runs under the caller's inherited worker bindings
//! (`service_execution::Inherited`), as an inline read does on the caller's
//! own thread; an unbound caller binds nothing.
use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// Canonical reads one Answer keeps in flight at once.
pub(super) const READS_IN_FLIGHT: usize = 4;

/// A test observer of each threaded wave's size.
#[cfg(test)]
pub(super) type WaveHook = Option<Box<dyn Fn(usize)>>;

#[cfg(test)]
thread_local! {
    /// Called on the driver's thread with the size of each threaded wave.
    pub(super) static ON_WAVE: std::cell::RefCell<WaveHook> =
        const { std::cell::RefCell::new(None) };
}

/// Run reads `0..count`: `stop` is checked before each claim, `halt` after
/// each read. The result holds the reads that ran, in index order.
pub(super) fn run<T: Send>(
    count: usize,
    stop: &(dyn Fn() -> bool + Sync),
    read: &(dyn Fn(usize) -> T + Sync),
    halt: &(dyn Fn(&T) -> bool + Sync),
) -> Vec<T> {
    if count <= 1 {
        // One read needs no thread; the same claim rule applies.
        return if count == 0 || stop() {
            Vec::new()
        } else {
            vec![read(0)]
        };
    }
    #[cfg(test)]
    ON_WAVE.with(|hook| {
        if let Some(hook) = hook.borrow().as_ref() {
            hook(count);
        }
    });
    // A read is part of the caller's work: it runs under exactly the caller's
    // worker bindings (execution scope, embedding connector, accepted grant),
    // bound for that read alone and restored when it returns, so nothing one
    // read binds reaches the next read on the same worker.
    let inherited = crate::service_execution::Inherited::capture();
    let next = AtomicUsize::new(0);
    let halted = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<std::thread::Result<T>>>> =
        (0..count).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..READS_IN_FLIGHT.min(count) {
            scope.spawn(|| {
                loop {
                    if halted.load(Ordering::Acquire) || stop() {
                        halted.store(true, Ordering::Release);
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::AcqRel);
                    if index >= count {
                        break;
                    }
                    let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _bound = inherited.bind();
                        read(index)
                    }));
                    if value.as_ref().map_or(true, halt) {
                        halted.store(true, Ordering::Release);
                    }
                    *slots[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
                }
            });
        }
    });
    // Every claimed index ran, and claims are consecutive, so this is a prefix.
    let ran: Vec<std::thread::Result<T>> = slots
        .into_iter()
        .map_while(|slot| slot.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect();
    let mut out = Vec::with_capacity(ran.len());
    for value in ran {
        match value {
            Ok(value) => out.push(value),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
    out
}

#[cfg(test)]
mod tests;
