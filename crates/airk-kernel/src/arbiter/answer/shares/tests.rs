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

/// Change log entry 50 (user decision, 23 November 2026): the default pool of
/// 380,000 tokens funds a four-part Answer through its last round when card
/// calls carry evidence. Costs measured at the gate (entry 44): a full
/// evidence call observes about 3,800 tokens; a part's round about 30,200
/// (16 Focus pairs at 1,024 plus its card calls); the last round, which has
/// no Follow-up, about 24,000. The Split call is charged before the shares.
#[test]
fn default_pool_funds_four_parts_through_three_evidence_rounds() {
    const SPLIT: usize = 3_800;
    const PLAN: usize = 3_800;
    const ROUNDS: [usize; 3] = [30_200, 30_200, 24_000];
    let budget = v3::BudgetV3::default();
    assert_eq!(budget.input_tokens, 380_000);
    // Each measured round fits the round allowance, so only the part's
    // share can stop it.
    assert!(ROUNDS.iter().all(|round| *round <= budget.round_input_tokens));
    let shares = Shares::allocate(&budget, 4, budget.context_calls - 1, 380_000 - SPLIT);
    let tokens: Vec<_> = shares.parts.iter().map(|share| share.tokens).collect();
    assert_eq!(tokens, [94_050, 94_050, 94_050, 94_050]);
    // Plan, then three rounds: 88,200 of the 94,050, so the last round's
    // calls are funded with 5,850 to spare.
    let mut left = 94_050 - PLAN;
    for round in ROUNDS {
        assert!(round <= left, "a round lost its model for tokens");
        left -= round;
    }
    assert_eq!(left, 5_850);
    // The old default of 196,608 gives 48,202 a part: Plan and round one
    // (34,000), then round two (30,200) no longer fits.
    let old = v3::BudgetV3 {
        input_tokens: 196_608,
        ..v3::BudgetV3::default()
    };
    let shares = Shares::allocate(&old, 4, old.context_calls - 1, 196_608 - SPLIT);
    assert_eq!(shares.parts[0].tokens, 48_202);
    assert!(PLAN + ROUNDS[0] + ROUNDS[1] > shares.parts[0].tokens);
    // A pool above the maximum is refused.
    let over = v3::BudgetV3 {
        input_tokens: 380_001,
        ..v3::BudgetV3::default()
    };
    assert!(over.validate().is_err());
}