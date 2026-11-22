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

#[test]
fn answer_attempts_run_to_their_own_ceiling_and_other_tools_keep_125_seconds() -> Result<()> {
    assert_eq!(MAX_READ_BUDGET, Duration::from_millis(125_000));
    assert_eq!(MAX_ANSWER_DEADLINE, Duration::from_millis(600_000));
    assert_eq!(MAX_ANSWER_READ_BUDGET, Duration::from_millis(605_000));
    assert_eq!(MAX_ANSWER_ARBITER_REMAINING_MS, 600_000);
    for tool in ["airk_answer", "answer"] {
        assert!(is_answer_tool(tool));
        assert_eq!(read_budget_for_tool(tool), Duration::from_millis(605_000));
    }
    for tool in [
        "airk_search",
        "search",
        "airk_read_file",
        "airk_answers",
        "",
    ] {
        assert!(!is_answer_tool(tool));
        assert_eq!(read_budget_for_tool(tool), Duration::from_millis(125_000));
    }
    let at = |remaining_ns: u64| ReadAttemptBudget {
        schema: 1,
        request_id: crate::identity::Incarnation::from_bytes([2; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 1_000 + remaining_ns,
        remaining_ns,
        native_retry_expires_monotonic_ns: None,
    };
    let clock = ReadClock::UnixMonotonic;
    let search = 125_000_000_000;
    let answer = 605_000_000_000;
    assert_eq!(
        at(search).remaining_for_tool("airk_search", clock, 1_000)?,
        Duration::from_nanos(search)
    );
    assert!(
        at(search + 1)
            .remaining_for_tool("airk_search", clock, 1_000)
            .is_err()
    );
    assert_eq!(
        at(answer).remaining_for_tool("airk_answer", clock, 1_000)?,
        Duration::from_nanos(answer)
    );
    assert!(
        at(answer + 1)
            .remaining_for_tool("airk_answer", clock, 1_000)
            .is_err()
    );
    // A read nested in an Answer carries the Answer's attempt; its own
    // operation limit applies after this.
    assert_eq!(
        at(answer).remaining_at(clock, 1_000)?,
        Duration::from_nanos(answer)
    );
    assert!(at(answer + 1).remaining_at(clock, 1_000).is_err());
    Ok(())
}
