// SPDX-License-Identifier: Apache-2.0
//! Test-support observation only; no default production hash behavior changes.
use airk_core::arbiter::Asset;
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashPhase {
    /// The first positive file read has already been applied to the digest.
    Entered,
    /// The actual hash scope returned or unwound, including verification failure.
    Returned,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct HashEvent {
    pub asset: Asset,
    pub phase: HashPhase,
    pub bytes_hashed: u64,
}
/// Callbacks must be synchronous, nonblocking observations: no waits, I/O or
/// changes to asset/configuration state. They cannot supply a hash result.
pub type Observer = Arc<dyn Fn(HashEvent) + Send + Sync>;
thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}
struct Restore(Option<Observer>);
impl Drop for Restore {
    fn drop(&mut self) {
        OBSERVER.with(|slot| {
            let _ = slot.replace(self.0.take());
        });
    }
}
/// Install an observer only for this thread and lexical operation. Nested scopes
/// restore the original observer, including when the supplied operation unwinds.
/// This feature-gated public seam has a genuine external Rust consumer in the
/// Service's private same-owner acquisition regression; it is absent by default.
pub fn with_observer<T>(observer: Observer, operation: impl FnOnce() -> T) -> T {
    let prior = OBSERVER.with(|slot| slot.replace(Some(observer)));
    let _restore = Restore(prior);
    operation()
}
pub(super) struct HashScope {
    observer: Option<Observer>,
    asset: Asset,
    bytes_hashed: u64,
}
impl HashScope {
    pub(super) fn new(asset: &Asset) -> Self {
        Self {
            observer: OBSERVER.with(|slot| slot.borrow().clone()),
            asset: asset.clone(),
            bytes_hashed: 0,
        }
    }
    pub(super) fn after_digest(&mut self, bytes_hashed: u64) {
        let first = self.bytes_hashed == 0;
        self.bytes_hashed = bytes_hashed;
        if first && bytes_hashed > 0 {
            self.emit(HashPhase::Entered);
        }
    }
    fn emit(&self, phase: HashPhase) {
        if let Some(observer) = &self.observer {
            let event = HashEvent {
                asset: self.asset.clone(),
                phase,
                bytes_hashed: self.bytes_hashed,
            };
            // An observer's failure must never replace actual Store I/O failure
            // or cause a second panic while the actual hash is unwinding.
            let _ = catch_unwind(AssertUnwindSafe(|| observer(event)));
        }
    }
}
impl Drop for HashScope {
    fn drop(&mut self) {
        if self.bytes_hashed > 0 {
            self.emit(HashPhase::Returned);
        }
    }
}
#[cfg(test)]
mod tests;
