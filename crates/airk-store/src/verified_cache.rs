// SPDX-License-Identifier: Apache-2.0
//! Memoize semantic validation, never the authenticity of current disk bytes.
use airk_core::{extraction::IndexDetail, identity::PublicationReceipt};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 16;
const MAX_KEY_BYTES: usize = 512 * 1024;
struct Entry {
    directory: PathBuf,
    receipt: PublicationReceipt,
    detail: IndexDetail,
    bytes: usize,
}
static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();

pub(crate) fn get(directory: &Path, receipt: &PublicationReceipt) -> Option<IndexDetail> {
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
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
    let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() else {
        return;
    };
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
