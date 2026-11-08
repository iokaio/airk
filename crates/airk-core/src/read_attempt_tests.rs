// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn delivered_budget_charges_elapsed_time_and_never_exceeds_sender_cap() -> Result<()> {
    let budget = ReadAttemptBudget {
        schema: 1,
        request_id: crate::identity::Incarnation::from_bytes([1; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 1_000,
        remaining_ns: 500,
        native_retry_expires_monotonic_ns: None,
    };
    assert_eq!(
        budget.remaining_at(ReadClock::UnixMonotonic, 700)?,
        Duration::from_nanos(300)
    );
    assert_eq!(
        budget.remaining_at(ReadClock::UnixMonotonic, 100)?,
        Duration::from_nanos(500)
    );
    assert!(
        budget
            .remaining_at(ReadClock::UnixMonotonic, 1_001)?
            .is_zero()
    );
    assert!(
        budget
            .remaining_at(ReadClock::WindowsPerformanceCounter, 700)
            .is_err()
    );
    assert!(
        ReadAttemptBudget {
            schema: 2,
            ..budget
        }
        .remaining_at(budget.clock, 700)
        .is_err()
    );
    assert!(
        ReadAttemptBudget {
            remaining_ns: u64::MAX,
            ..budget
        }
        .remaining_at(budget.clock, 700)
        .is_err()
    );
    Ok(())
}
