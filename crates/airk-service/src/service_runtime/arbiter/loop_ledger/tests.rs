// SPDX-License-Identifier: Apache-2.0
use super::*;

fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: 600_000_000_100,
        remaining_ns: 600_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
fn charge(part: u8, round: u8, tokens: usize) -> TokenAttribution {
    TokenAttribution {
        part,
        round,
        tokens,
    }
}
fn budget() -> BudgetV3 {
    BudgetV3 {
        context_calls: 3,
        focus_calls: 2,
        input_tokens: 20_000,
        round_input_tokens: 10_000,
        ..BudgetV3::default()
    }
}
fn index(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}
fn partition(byte: u8, cards: Vec<u8>, focus: Vec<u8>, tokens: usize) -> PartitionAllowanceV3 {
    PartitionAllowanceV3 {
        schema: airk_core::native_loop::ALLOWANCE_SCHEMA_V3.into(),
        attempt: attempt().request_id,
        selection: Sha256Digest::from_bytes([2; 32]),
        index_id: index(byte),
        parent_budget: budget(),
        resources: airk_core::native_loop::LoopResources {
            calls: 4,
            input_tokens: tokens,
            candidates: 16,
            bytes: 131_072,
            focus_pairs: 16,
        },
        card_ordinals: cards,
        focus_ordinals: focus,
    }
}

#[test]
fn card_slots_run_up_to_context_calls_and_are_never_reused() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    for slot in 1..=3 {
        ledger.reserve_card(attempt(), &parent, slot, &[charge(1, 1, 100)], None, 10)?;
    }
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 4, &[charge(1, 1, 100)], None, 11)
            .is_err(),
        "slot 4 is above context_calls"
    );
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 2, &[charge(1, 1, 100)], None, 12)
            .is_err(),
        "a used slot is never reusable"
    );
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 0, &[charge(1, 1, 100)], None, 13)
            .is_err()
    );
    Ok(())
}

#[test]
fn one_card_call_is_at_most_8192_tokens_and_the_pool_holds_the_attempt() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = BudgetV3 {
        context_calls: 8,
        input_tokens: 12_000,
        round_input_tokens: 12_000,
        ..budget()
    };
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 1, &[charge(1, 1, 8_193)], None, 10)
            .is_err()
    );
    ledger.reserve_card(attempt(), &parent, 1, &[charge(1, 1, 8_192)], None, 10)?;
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 2, &[charge(2, 1, 3_809)], None, 11)
            .is_err(),
        "the attempt pool is input_tokens"
    );
    ledger.reserve_card(attempt(), &parent, 2, &[charge(2, 1, 3_808)], None, 11)?;
    Ok(())
}

#[test]
fn per_round_allowance_is_enforced_per_part() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    ledger.reserve_card(attempt(), &parent, 1, &[charge(1, 1, 8_000)], None, 10)?;
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 2, &[charge(1, 1, 2_001)], None, 11)
            .is_err(),
        "part 1 round 1 would exceed round_input_tokens"
    );
    // Another part, or the same part's next round, has its own allowance.
    ledger.reserve_card(
        attempt(),
        &parent,
        2,
        &[charge(1, 2, 2_001), charge(2, 1, 2_001)],
        None,
        12,
    )?;
    // Split and Plan (round 0) draw on the pool only.
    let mut ledger = Ledger::default();
    ledger.reserve_card(attempt(), &parent, 1, &[charge(0, 0, 8_000)], None, 10)?;
    ledger.reserve_card(attempt(), &parent, 2, &[charge(0, 0, 8_000)], None, 11)?;
    Ok(())
}

#[test]
fn batched_attribution_is_proportional_with_the_remainder_to_the_lowest() -> Result<()> {
    let observed = attribute(1_000, &[charge(2, 1, 1), charge(1, 1, 2)]);
    assert_eq!(observed, vec![charge(1, 1, 667), charge(2, 1, 333)]);
    assert_eq!(attribute(0, &[charge(1, 1, 5)]), vec![charge(1, 1, 0)]);
    let mut ledger = Ledger::default();
    let ticket = ledger.reserve_card(
        attempt(),
        &budget(),
        1,
        &[charge(1, 1, 4_000), charge(2, 1, 4_000)],
        None,
        10,
    )?;
    let settled = ledger.completed(ticket, 5_001)?;
    assert_eq!(settled, vec![charge(1, 1, 2_501), charge(2, 1, 2_500)]);
    assert_eq!(settled.iter().map(|c| c.tokens).sum::<usize>(), 5_001);
    // The refund is real: part 1 round 1 has 7,499 left of 10,000.
    ledger.reserve_card(attempt(), &budget(), 2, &[charge(1, 1, 7_499)], None, 11)?;
    Ok(())
}

#[test]
fn refusal_releases_tokens_and_keeps_the_slot() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    let ticket = ledger.reserve_card(attempt(), &parent, 1, &[charge(1, 1, 8_000)], None, 10)?;
    ledger.refused(ticket)?;
    ledger.reserve_card(attempt(), &parent, 2, &[charge(1, 1, 8_000)], None, 11)?;
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 1, &[charge(1, 1, 10)], None, 12)
            .is_err(),
        "the refused slot stays used"
    );
    let compiled = ledger.reserve_card(attempt(), &parent, 3, &[charge(2, 1, 100)], None, 13)?;
    ledger.compiled(compiled, 90)?;
    assert!(
        ledger.refused(compiled).is_err(),
        "a compile receipt contradicts a refusal"
    );
    Ok(())
}

#[test]
fn lost_outcome_keeps_the_whole_reservation() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    let _lost = ledger.reserve_card(attempt(), &parent, 1, &[charge(1, 1, 8_000)], None, 10)?;
    // No completion arrives: the 8,000 tokens stay charged to part 1 round 1.
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 2, &[charge(1, 1, 2_001)], None, 11)
            .is_err()
    );
    ledger.reserve_card(attempt(), &parent, 2, &[charge(1, 1, 2_000)], None, 12)?;
    Ok(())
}

#[test]
fn focus_calls_charge_each_part_round_for_its_own_pairs() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    let ticket = ledger.reserve_focus(
        attempt(),
        &parent,
        FocusCall {
            index_id: index(3),
            ordinal: 1,
            pairs: 2,
        },
        &[charge(1, 1, 1_024), charge(2, 1, 2_048)],
        None,
        10,
    )?;
    ledger.compiled_prefix(ticket, 500)?;
    assert!(
        ledger.compiled_prefix(ticket, 400).is_err(),
        "prefix receipts never regress"
    );
    let settled = ledger.completed_prefix(ticket, 900, &[charge(1, 1, 1), charge(2, 1, 2)])?;
    assert_eq!(settled, vec![charge(1, 1, 300), charge(2, 1, 600)]);
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusCall {
                    index_id: index(3),
                    ordinal: 3,
                    pairs: 2,
                },
                &[charge(1, 1, 1)],
                None,
                11
            )
            .is_err(),
        "Focus ordinals run up to focus_calls"
    );
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusCall {
                    index_id: index(3),
                    ordinal: 2,
                    pairs: 2,
                },
                &[charge(0, 0, 1)],
                None,
                11
            )
            .is_err(),
        "Focus is charged to a part and round"
    );
    let other = ledger.reserve_focus(
        attempt(),
        &parent,
        FocusCall {
            index_id: index(3),
            ordinal: 2,
            pairs: 2,
        },
        &[charge(1, 1, 100)],
        None,
        12,
    )?;
    assert!(
        ledger
            .completed_prefix(other, 50, &[charge(2, 1, 1)])
            .is_err(),
        "settlement may not name a part-round it did not reserve"
    );
    Ok(())
}

#[test]
fn partitions_limit_their_card_and_focus_ordinals_and_tokens() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    let part = partition(3, vec![1], vec![1], 5_000);
    ledger.reserve_card(
        attempt(),
        &parent,
        1,
        &[charge(1, 1, 4_000)],
        Some(&part),
        10,
    )?;
    assert!(
        ledger
            .reserve_card(attempt(), &parent, 2, &[charge(1, 1, 10)], Some(&part), 11)
            .is_err(),
        "slot 2 is not this partition's"
    );
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusCall {
                    index_id: index(3),
                    ordinal: 1,
                    pairs: 2,
                },
                &[charge(1, 1, 1_001)],
                Some(&part),
                12
            )
            .is_err(),
        "the partition's tokens are 5,000"
    );
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                FocusCall {
                    index_id: index(4),
                    ordinal: 1,
                    pairs: 2,
                },
                &[charge(1, 1, 10)],
                Some(&part),
                13
            )
            .is_err(),
        "a Focus call uses only its own index's partition"
    );
    Ok(())
}

#[test]
fn attempt_identity_and_parent_budget_are_fixed_once_charged() -> Result<()> {
    let mut ledger = Ledger::default();
    ledger.reserve_card(attempt(), &budget(), 1, &[charge(1, 1, 10)], None, 10)?;
    let changed = BudgetV3 {
        input_tokens: 19_999,
        ..budget()
    };
    assert!(
        ledger
            .reserve_card(attempt(), &changed, 2, &[charge(1, 1, 10)], None, 11)
            .is_err()
    );
    assert!(
        ledger
            .reserve_card(
                attempt(),
                &budget(),
                2,
                &[charge(2, 1, 10), charge(1, 1, 10)],
                None,
                12
            )
            .is_err(),
        "attribution is ordered by part and round"
    );
    let mut expired = attempt();
    expired.remaining_ns = 0;
    assert!(
        Ledger::default()
            .reserve_card(
                expired,
                &budget(),
                1,
                &[charge(1, 1, 10)],
                None,
                600_000_000_100
            )
            .is_err()
    );
    Ok(())
}

/// A ticket bounds its call's deadline. The window opens at the attempt's
/// first reservation, lasts the parent deadline (300 s by default) within
/// the attempt's own expiry, and a later call can narrow it but never renew
/// it.
#[test]
fn a_ticket_s_window_is_the_parent_deadline_and_never_renews() -> Result<()> {
    let mut ledger = Ledger::default();
    let parent = budget();
    assert_eq!(parent.deadline_ms, 300_000);
    // The attempt has 600 s left at 100 ns; the parent deadline is shorter.
    let first = ledger.reserve_card(attempt(), &parent, 1, &[charge(1, 1, 100)], None, 100)?;
    assert_eq!(first.remaining_ns(100), 300_000_000_000);
    // Thirty seconds later the same window has 270 s left.
    let second = ledger.reserve_card(
        attempt(),
        &parent,
        2,
        &[charge(1, 1, 100)],
        None,
        30_000_000_100,
    )?;
    assert_eq!(second.remaining_ns(30_000_000_100), 270_000_000_000);
    // A call that states 5 s left narrows the window to those 5 s.
    let mut hurried = attempt();
    hurried.remaining_ns = 5_000_000_000;
    let third = ledger.reserve_card(
        hurried,
        &parent,
        3,
        &[charge(1, 1, 100)],
        None,
        40_000_000_100,
    )?;
    assert_eq!(third.remaining_ns(40_000_000_100), 5_000_000_000);
    assert_eq!(third.remaining_ns(45_000_000_100), 0);
    assert_eq!(third.remaining_ns(900_000_000_000), 0, "never negative");
    Ok(())
}

#[test]
fn focus_pairs_are_bounded_by_the_partition_and_the_parent() -> Result<()> {
    // The partition allows 16 pairs and two Focus calls.
    let mut ledger = Ledger::default();
    let parent = budget();
    let part = partition(3, vec![], vec![1, 2], 5_000);
    let call = |ordinal, pairs| FocusCall {
        index_id: index(3),
        ordinal,
        pairs,
    };
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                call(1, 17),
                &[charge(1, 1, 10)],
                Some(&part),
                10
            )
            .is_err(),
        "17 pairs exceed the partition's 16"
    );
    ledger.reserve_focus(
        attempt(),
        &parent,
        call(1, 12),
        &[charge(1, 1, 10)],
        Some(&part),
        11,
    )?;
    assert!(
        ledger
            .reserve_focus(
                attempt(),
                &parent,
                call(2, 5),
                &[charge(1, 1, 10)],
                Some(&part),
                12
            )
            .is_err(),
        "12 + 5 pairs exceed the partition's 16"
    );
    ledger.reserve_focus(
        attempt(),
        &parent,
        call(2, 4),
        &[charge(1, 1, 10)],
        Some(&part),
        13,
    )?;
    // Without a partition the parent's focus_pairs bounds the attempt.
    let small = BudgetV3 {
        focus_pairs: 6,
        ..budget()
    };
    let mut ledger = Ledger::default();
    ledger.reserve_focus(attempt(), &small, call(1, 4), &[charge(1, 1, 10)], None, 10)?;
    assert!(
        ledger
            .reserve_focus(attempt(), &small, call(2, 3), &[charge(1, 1, 10)], None, 11)
            .is_err(),
        "4 + 3 pairs exceed the parent's 6"
    );
    assert!(
        Ledger::default()
            .reserve_focus(
                attempt(),
                &budget(),
                call(1, 0),
                &[charge(1, 1, 10)],
                None,
                10
            )
            .is_err(),
        "a Focus call scores at least one pair"
    );
    Ok(())
}
