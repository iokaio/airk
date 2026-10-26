// SPDX-License-Identifier: Apache-2.0
//! Admitted redb storage primitives. Collection activation remains owned by current.
mod backend;
pub mod catalog;
mod db;
pub mod directory;
pub mod journal;
pub mod lexical;
pub mod lexical_plan;
pub(crate) mod owner;
pub mod posting;
pub mod publication;
pub mod tables;
pub mod writer;

pub(crate) use backend::ReservationGrowthDenied;
pub use backend::{AirkBackend, CapacityExceeded};
pub(crate) use db::CapacityAdmitter;
pub use db::{Database, Snapshot};
pub use owner::{Engine, OwnedSnapshot};

/// New engine marker; never interpret a SQLite image as this format.
pub const FORMAT: u32 = 150;
pub const CACHE_BYTES: usize = 64 << 20;

// Opt-in development timings contain stage names and durations only.
pub(crate) fn profile_start() -> Option<std::time::Instant> {
    (cfg!(debug_assertions) && std::env::var_os("AIRK_STORAGE_PROFILE").is_some())
        .then(std::time::Instant::now)
}
pub(crate) fn profile_tick(stage: &str, start: &mut Option<std::time::Instant>) {
    if let Some(start) = start {
        eprintln!(
            "airk-storage-profile: {stage} {:.3}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        *start = std::time::Instant::now();
    }
}
