// SPDX-License-Identifier: Apache-2.0
//! Native process lifetime primitives shared by supervision and extraction.
pub mod bounded_command;
pub mod cert_trust;
pub mod child_job;
pub mod client_owner;
pub mod cpu_fit;
pub mod detached;
pub mod directory;
pub mod monotonic;
#[cfg(windows)]
pub mod named_mutex;
pub mod private_stdio;
pub mod signature;
pub mod user_folders;
pub use child_job::ChildJob;
#[cfg(windows)]
pub use child_job::{AtomicJobChild, NamedJobOpen};
#[cfg(unix)]
pub mod child_group;
#[cfg(unix)]
pub use child_group::GroupChild;
