// SPDX-License-Identifier: Apache-2.0
use super::*;
// Original ledger controls retain the explicit original parent budget.
fn historical_budget() -> Budget {
    Budget {
        calls: 8,
        rounds: 3,
        fanout: 8,
        input_tokens: 2048,
        candidates: 32,
        max_bytes: 262144,
        deadline_ms: 30_000,
    }
}

impl Ledger {
    fn reserve_historical(
        &mut self,
        attempt: ReadAttemptBudget,
        ordinal: u8,
        requested: usize,
        partition: Option<&PartitionAllowance>,
        now_ns: u64,
    ) -> Result<Ticket> {
        self.reserve(
            attempt,
            &historical_budget(),
            ordinal,
            requested,
            partition,
            now_ns,
        )
    }
}

fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: 120_000_000_100,
        remaining_ns: 120_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
fn partition(index: u8, ordinal: u8, tokens: usize) -> PartitionAllowance {
    PartitionAllowance {
        schema: airk_core::combined_context::ALLOWANCE_SCHEMA.into(),
        attempt: attempt().request_id,
        selection: Sha256Digest::from_bytes([2; 32]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([index; 16])),
        parent_budget: historical_budget(),
        resources: airk_core::combined_context::Resources {
            calls: 4,
            input_tokens: tokens,
            candidates: 16,
            bytes: 131072,
        },
        assessment_ordinals: vec![ordinal],
    }
}
#[test]
fn unknown_work_keeps_tokens_and_global_ordinal_across_partitions() {
    let mut ledger = Ledger::default();
    let one = partition(3, 1, 1024);
    ledger
        .reserve_historical(attempt(), 1, 1024, Some(&one), 100)
        .unwrap();
    let other = partition(4, 1, 1024);
    assert!(
        ledger
            .reserve_historical(attempt(), 1, 1024, Some(&other), 101)
            .is_err()
    );
    let two = partition(4, 2, 1024);
    ledger
        .reserve_historical(attempt(), 2, 1024, Some(&two), 102)
        .unwrap();
    let three = partition(5, 3, 1);
    assert!(
        ledger
            .reserve_historical(attempt(), 3, 1, Some(&three), 103)
            .is_err()
    );
}
#[test]
fn only_actual_compiled_tokens_release_unused_reservation_and_never_replay() {
    let mut ledger = Ledger::default();
    let ticket = ledger
        .reserve_historical(attempt(), 1, 2048, None, 100)
        .unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 1, None, 101)
            .is_err()
    );
    ledger.compiled(ticket, 512).unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 1, None, 102)
            .is_err()
    );
    ledger.completed(ticket, 512).unwrap();
    ledger
        .reserve_historical(attempt(), 2, 1536, None, 102)
        .unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 1, 1, None, 103)
            .is_err()
    );
    assert!(
        ledger
            .reserve_historical(attempt(), 3, 1, None, 104)
            .is_err()
    );
    assert!(ledger.compiled(ticket, 0).is_err());
    assert!(ledger.compiled(ticket, 256).is_err());
    assert!(ledger.compiled(ticket, 513).is_err());
}
#[test]
fn original_parent_and_partition_identity_cannot_change() {
    let mut ledger = Ledger::default();
    let ticket = ledger
        .reserve_historical(attempt(), 1, 1, None, 100)
        .unwrap();
    assert_eq!(ticket.remaining_ns(100), 30_000_000_000);
    let mut changed = attempt();
    changed.expires_monotonic_ns += 1;
    assert!(ledger.reserve_historical(changed, 2, 1, None, 101).is_err());
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 1, Some(&partition(3, 2, 1)), 101)
            .is_err()
    );
    let mut ledger = Ledger::default();
    let mut one = partition(3, 1, 1024);
    one.assessment_ordinals = vec![1, 2];
    ledger
        .reserve_historical(attempt(), 1, 100, Some(&one), 100)
        .unwrap();
    one.resources.input_tokens = 1023;
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 100, Some(&one), 101)
            .is_err()
    );
}
#[test]
fn thirty_second_window_cannot_be_renewed_under_longer_original_parent() {
    let mut ledger = Ledger::default();
    ledger
        .reserve_historical(attempt(), 1, 1, None, 100)
        .unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 1, None, 30_000_000_100)
            .is_err()
    );
    assert!(
        ledger
            .reserve_historical(attempt(), 3, 1, None, 60_000_000_100)
            .is_err()
    );
    assert!(
        ledger
            .reserve_historical(attempt(), 3, 1, None, 120_000_000_100)
            .is_err()
    );
}
#[test]
fn partition_token_ceiling_and_ordinal_are_both_enforced() {
    let mut ledger = Ledger::default();
    let mut one = partition(3, 1, 1000);
    one.assessment_ordinals = vec![1, 2];
    ledger
        .reserve_historical(attempt(), 1, 600, Some(&one), 100)
        .unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 2, 401, Some(&one), 101)
            .is_err()
    );
    ledger
        .reserve_historical(attempt(), 2, 400, Some(&one), 102)
        .unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 3, 1, Some(&one), 103)
            .is_err()
    );
}
#[test]
fn captured_small_parent_budget_cannot_be_enlarged_by_later_callback() {
    let parent = Budget {
        input_tokens: 512,
        rounds: 1,
        ..historical_budget()
    };
    let mut ledger = Ledger::default();
    assert!(
        ledger
            .reserve(attempt(), &parent, 1, 513, None, 100)
            .is_err()
    );
    let ticket = ledger
        .reserve(attempt(), &parent, 1, 512, None, 100)
        .unwrap();
    ledger.completed(ticket, 128).unwrap();
    assert!(ledger.reserve(attempt(), &parent, 2, 1, None, 101).is_err());
    assert!(
        ledger
            .reserve(attempt(), &historical_budget(), 2, 1, None, 101)
            .is_err()
    );
    let another = Budget {
        input_tokens: 512,
        rounds: 3,
        ..historical_budget()
    };
    assert!(
        ledger
            .reserve(attempt(), &another, 2, 1, None, 101)
            .is_err()
    );
    let mut ledger = Ledger::default();
    let mut part = partition(3, 1, 512);
    part.parent_budget = parent.clone();
    ledger
        .reserve(attempt(), &parent, 1, 512, Some(&part), 100)
        .unwrap();
    let other = partition(4, 2, 512);
    assert!(
        ledger
            .reserve(attempt(), &parent, 2, 1, Some(&other), 101)
            .is_err()
    );
}

#[test]
fn focus_and_context_share_tokens_but_not_ordinal_slots() {
    let mut ledger = Ledger::default();
    let index_id = partition(3, 1, 2048).index_id;
    let focus = ledger
        .reserve_focus(
            attempt(),
            &historical_budget(),
            FocusInvocation {
                index_id,
                ordinal: 1,
            },
            1536,
            None,
            100,
        )
        .unwrap();
    // Unknown pair completion holds the entire prefix maximum.
    ledger.compiled_prefix(focus, 512).unwrap();
    assert!(
        ledger
            .reserve_historical(attempt(), 1, 513, None, 101)
            .is_err()
    );
    // Actual aggregate settlement can release unused tokens. The distinct
    // Context assessment slot 1 remains available, but Focus slot 1 cannot replay.
    ledger.completed_prefix(focus, 512).unwrap();
    ledger
        .reserve_historical(attempt(), 1, 1536, None, 102)
        .unwrap();
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &historical_budget(),
                FocusInvocation {
                    index_id,
                    ordinal: 1
                },
                1,
                None,
                103
            )
            .is_err()
    );
    assert!(ledger.compiled_prefix(focus, 511).is_err());
}

#[test]
fn focus_uses_captured_call_caps_and_never_context_assessment_ordinals() {
    let parent = Budget {
        calls: 2,
        ..historical_budget()
    };
    let mut part = partition(3, 3, 1024);
    part.parent_budget = parent.clone();
    part.resources.calls = 1;
    let mut ledger = Ledger::default();
    ledger
        .reserve_focus(
            attempt(),
            &parent,
            FocusInvocation {
                index_id: part.index_id,
                ordinal: 1,
            },
            512,
            Some(&part),
            100,
        )
        .unwrap();
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusInvocation {
                    index_id: part.index_id,
                    ordinal: 2
                },
                1,
                Some(&part),
                101
            )
            .is_err()
    );
    let mut other = partition(4, 2, 1024);
    other.parent_budget = parent.clone();
    other.resources.calls = 1;
    ledger
        .reserve_focus(
            attempt(),
            &parent,
            FocusInvocation {
                index_id: other.index_id,
                ordinal: 1,
            },
            512,
            Some(&other),
            102,
        )
        .unwrap();
    let mut third = partition(5, 1, 1024);
    third.parent_budget = parent.clone();
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusInvocation {
                    index_id: third.index_id,
                    ordinal: 1
                },
                1,
                Some(&third),
                103
            )
            .is_err()
    );
}

#[test]
fn current_default_reserves_8192_and_releases_only_known_completed_work() {
    let parent = Budget::default();
    assert_eq!(parent.input_tokens, 8192);
    assert_eq!(parent.deadline_ms, 120_000);
    let mut ledger = Ledger::default();
    assert!(
        ledger
            .reserve(attempt(), &parent, 1, 8193, None, 100)
            .is_err()
    );
    let ticket = ledger
        .reserve(attempt(), &parent, 1, 8192, None, 100)
        .unwrap();
    assert_eq!(ticket.remaining_ns(100), 120_000_000_000);
    assert!(ledger.reserve(attempt(), &parent, 2, 1, None, 101).is_err());
    ledger.compiled(ticket, 512).unwrap();
    assert!(ledger.reserve(attempt(), &parent, 2, 1, None, 102).is_err());
    ledger.completed(ticket, 512).unwrap();
    ledger
        .reserve(attempt(), &parent, 2, 7680, None, 103)
        .unwrap();
    assert!(ledger.reserve(attempt(), &parent, 3, 1, None, 104).is_err());
    assert!(
        ledger
            .reserve(attempt(), &historical_budget(), 3, 1, None, 104)
            .is_err()
    );
}
#[test]
fn current_120_second_window_never_renews_inside_125_second_transport_parent() {
    let parent = Budget::default();
    let mut original = attempt();
    original.expires_monotonic_ns = 125_000_000_100;
    original.remaining_ns = 125_000_000_000;
    let mut ledger = Ledger::default();
    let ticket = ledger.reserve(original, &parent, 1, 1, None, 100).unwrap();
    assert_eq!(ticket.remaining_ns(100), 120_000_000_000);
    let second = ledger
        .reserve(original, &parent, 2, 1, None, 30_000_000_100)
        .unwrap();
    assert_eq!(second.remaining_ns(30_000_000_100), 90_000_000_000);
    for now in [120_000_000_100, 121_000_000_100, 125_000_000_100] {
        assert!(ledger.reserve(original, &parent, 3, 1, None, now).is_err());
    }
}
