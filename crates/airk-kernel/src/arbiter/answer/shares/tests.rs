// SPDX-License-Identifier: Apache-2.0
//! Part shares, hand-back and the reserve.
use super::*;

#[test]
fn equal_shares_with_remainder_to_lowest_part() {
    let budget = v3::BudgetV3::default();
    let shares = Shares::allocate(&budget, 4, 47, 131_000);
    let calls: Vec<_> = shares.parts.iter().map(|share| share.calls).collect();
    assert_eq!(calls, [33, 33, 33, 33]);
    let slots: Vec<_> = shares.parts.iter().map(|share| share.card_slots).collect();
    assert_eq!(slots, [14, 11, 11, 11]);
    let time: u64 = shares.parts.iter().map(|share| share.deadline_ms).sum();
    assert_eq!(time + shares.reserve.total_ms(), budget.deadline_ms);
}

#[test]
fn finished_part_hands_back_calls_tokens_and_model_time() {
    let budget = v3::BudgetV3::default();
    let mut shares = Shares::allocate(&budget, 3, 30, 30_000);
    let before = shares.parts[1];
    shares.finish(
        0,
        Share {
            calls: 4,
            card_slots: 2,
            focus_pairs: 10,
            tokens: 1_000,
            deadline_ms: 0,
            done: false,
        },
        10_000,
    );
    let first = shares.parts[0];
    assert!(first.done);
    assert_eq!(
        (
            first.calls,
            first.card_slots,
            first.tokens,
            first.deadline_ms
        ),
        (4, 2, 1_000, 10_000)
    );
    let second = shares.parts[1];
    assert_eq!(second.calls, before.calls + (44 - 4) / 2);
    assert_eq!(second.card_slots, before.card_slots + 4);
    assert_eq!(second.tokens, before.tokens + 4_500);
    assert!(second.deadline_ms > before.deadline_ms);
    let total: u64 = shares.parts.iter().map(|share| share.deadline_ms).sum();
    assert!(total + shares.reserve.total_ms() <= budget.deadline_ms);
    assert_eq!(shares.active(), 2);
}

#[test]
fn model_share_divides_time_left_by_parts_and_rounds() {
    let budget = v3::BudgetV3::default();
    let shares = Shares::allocate(&budget, 2, 10, 10_000);
    let left = 100_000;
    let each = shares.round_model_ms(left, 2);
    assert_eq!(each, (left - shares.reserve.compose_union_ms) / 2 / 2);
    let mut cost = CostModel::default();
    assert_eq!(
        cost.estimate(v3::LoopStage::Coverage),
        software::FIRST_CONTEXT_ESTIMATE_MS
    );
    cost.observe(v3::LoopStage::Coverage, 2_000);
    cost.observe(v3::LoopStage::Coverage, 6_000);
    assert_eq!(cost.estimate(v3::LoopStage::Coverage), 3_000);
}

#[test]
fn reserve_keeps_compose_and_union_time() {
    let reserve = Reserve::new(300_000, 4);
    assert_eq!(reserve.compose_union_ms, 9_000 + 1_000 + 1_000);
    assert_eq!(reserve.dispatch_ms, 5_000);
    let short = Reserve::new(8_000, 1);
    assert!(
        short.total_ms() <= 2_000,
        "a short deadline keeps most of its time for reads"
    );
    assert!(short.compose_union_ms > 0 && short.dispatch_ms > 0);
}
