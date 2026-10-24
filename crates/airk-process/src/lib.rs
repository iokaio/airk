// SPDX-License-Identifier: Apache-2.0
//! Native process lifetime primitives shared by supervision and extraction.
pub mod child_job;
pub use child_job::ChildJob;
#[cfg(unix)]
pub mod child_group;
#[cfg(unix)]
pub use child_group::GroupChild;
