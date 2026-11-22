// SPDX-License-Identifier: Apache-2.0
//! The concurrent reads of one wave (L3-9). A wave's reads run on scoped
//! threads, at most `READS_IN_FLIGHT` at a time, and come back in dispatch
//! order whatever order they completed in. Reads are claimed in dispatch
//! order, and none is claimed once the reserve is reached or a read must
//! stop the Answer, so the reads that ran are always a prefix of the wave:
//! the unstarted rest is released exactly as in a sequential driver.
use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// Canonical reads one Answer keeps in flight at once.
pub(super) const READS_IN_FLIGHT: usize = 4;

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
    let next = AtomicUsize::new(0);
    let halted = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<T>>> = (0..count).map(|_| Mutex::new(None)).collect();
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
                    let value = read(index);
                    if halt(&value) {
                        halted.store(true, Ordering::Release);
                    }
                    *slots[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
                }
            });
        }
    });
    // Every claimed index ran, and claims are consecutive, so this is a prefix.
    slots
        .into_iter()
        .map_while(|slot| slot.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect()
}

#[cfg(test)]
mod tests;
