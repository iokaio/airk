// SPDX-License-Identifier: Apache-2.0
//! Memoize semantic validation, never the authenticity of current disk bytes.
use airk_core::{extraction::IndexDetail, identity::PublicationReceipt};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

const MAX_ENTRIES: usize = 16;
const MAX_KEY_BYTES: usize = 512 * 1024;
struct Entry {
    directory: PathBuf,
    receipt: PublicationReceipt,
    detail: IndexDetail,
    bytes: usize,
}
type Cache = VecDeque<Entry>;
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

/// Every hosted owner shares this process-wide cache. A panic in one owner
/// while it held the lock leaves entry integrity unproven, so the whole cache
/// is discarded and the lock is made usable again; callers then re-verify.
fn lock(cache: &Mutex<Cache>) -> MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(|poisoned| {
        let mut entries = poisoned.into_inner();
        entries.clear();
        cache.clear_poison();
        entries
    })
}

fn shared() -> MutexGuard<'static, Cache> {
    lock(CACHE.get_or_init(Default::default))
}

/// Forget every memoized validation for generations under `prefixes`.
pub(crate) fn release_under(prefixes: &[PathBuf]) {
    if let Some(cache) = CACHE.get() {
        retain_outside(&mut lock(cache), prefixes);
    }
}

fn retain_outside(cache: &mut Cache, prefixes: &[PathBuf]) {
    cache.retain(|entry| {
        !prefixes
            .iter()
            .any(|prefix| entry.directory.starts_with(prefix))
    });
}

pub(crate) fn get(directory: &Path, receipt: &PublicationReceipt) -> Option<IndexDetail> {
    let mut cache = shared();
    let index = cache
        .iter()
        .position(|entry| entry.directory == directory && &entry.receipt == receipt)?;
    let entry = cache.remove(index)?;
    let detail = entry.detail;
    cache.push_back(entry);
    Some(detail)
}

/// Only called after the complete manifest, inventory and SQLite quick_check
/// have passed. Entries hold no reader leases or connections. Model-bearing
/// generations use the full verifier instead, including shared model objects.
pub(crate) fn remember(directory: PathBuf, receipt: PublicationReceipt, detail: IndexDetail) {
    let bytes = directory
        .as_os_str()
        .len()
        .saturating_mul(2)
        .saturating_add(512);
    if bytes > MAX_KEY_BYTES {
        return;
    }
    let mut cache = shared();
    cache.retain(|entry| entry.directory != directory || entry.receipt != receipt);
    while cache.len() >= MAX_ENTRIES
        || cache.iter().map(|entry| entry.bytes).sum::<usize>() + bytes > MAX_KEY_BYTES
    {
        cache.pop_front();
    }
    cache.push_back(Entry {
        directory,
        receipt,
        detail,
        bytes,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::{Incarnation, RevisionToken, Sha256Digest};

    fn receipt(byte: u8) -> PublicationReceipt {
        let digest = Sha256Digest::from_bytes([byte; 32]);
        PublicationReceipt {
            token: RevisionToken {
                incarnation: Incarnation::from_bytes([byte; 16]),
                generation: u64::from(byte),
                revision: digest,
            },
            inventory_digest: digest,
            manifest_digest: digest,
        }
    }

    fn entry(directory: &str, byte: u8) -> Entry {
        Entry {
            directory: PathBuf::from(directory),
            receipt: receipt(byte),
            detail: IndexDetail::default(),
            bytes: 1,
        }
    }

    #[test]
    fn poisoned_cache_is_discarded_and_usable_again() {
        let cache = Mutex::new(Cache::from([entry("a/generations/1", 1)]));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.lock().unwrap();
            panic!("owner panicked while holding the shared verification cache");
        }));
        assert!(panicked.is_err() && cache.is_poisoned());
        assert!(
            lock(&cache).is_empty(),
            "no possibly half-updated entry survives"
        );
        assert!(!cache.is_poisoned());
        lock(&cache).push_back(entry("a/generations/2", 2));
        assert_eq!(lock(&cache).len(), 1);
    }

    #[test]
    fn release_drops_only_entries_under_the_released_collection() {
        let mut cache = Cache::from([
            entry("data/collections-1.4/one/incarnations/1/generations/1", 1),
            entry(
                "data/collections-1.4/one-two/incarnations/1/generations/1",
                2,
            ),
            entry("data/collections-1.4/two/incarnations/1/generations/1", 3),
        ]);
        retain_outside(&mut cache, &[PathBuf::from("data/collections-1.4/one")]);
        let kept: Vec<_> = cache.iter().map(|entry| entry.receipt.clone()).collect();
        assert_eq!(kept, [receipt(2), receipt(3)]);
    }
}
