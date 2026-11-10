// SPDX-License-Identifier: Apache-2.0
//! Read limits shared by combined reads, saved-byte verification and workbench
//! plans. Leaf with no dependencies. `verification` and `workbench` re-export
//! these from their original paths. See `CONTRACT.md` beside this file.

pub const MAX_SOURCES: usize = 256;
pub const MAX_BYTES: u64 = 1_073_741_824;
pub const MAX_DEADLINE_MS: u64 = 30_000;

/// Reports are expanded by typed bindings; they are not limited to the six
/// authored plan steps. Native composition retains every bounded fanout report.
pub const MAX_STEP_REPORTS: usize = 6 * 12;
