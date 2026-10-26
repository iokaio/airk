// SPDX-License-Identifier: Apache-2.0
//! Admitted redb storage primitives. Collection activation remains owned by current.
mod backend;
pub mod catalog;
mod db;
pub mod journal;
pub mod tables;

pub use backend::{AirkBackend, CapacityExceeded};
pub use db::{Database, Snapshot};

/// New engine marker; never interpret a SQLite image as this format.
pub const FORMAT: u32 = 150;
pub const CACHE_BYTES: usize = 64 << 20;
