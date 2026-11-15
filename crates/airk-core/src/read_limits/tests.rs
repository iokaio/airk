// SPDX-License-Identifier: Apache-2.0
//! Independent literal expectations for the shared read limits.
use super::*;

#[test]
fn deadline_ceiling_is_a_literal_120_seconds() {
    assert_eq!(MAX_DEADLINE_MS, 120_000);
    assert_eq!(MAX_SOURCES, 256);
    assert_eq!(MAX_BYTES, 1_073_741_824);
    assert_eq!(MAX_STEP_REPORTS, 72);
}
