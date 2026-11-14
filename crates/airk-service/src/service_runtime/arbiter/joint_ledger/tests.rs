// SPDX-License-Identifier: Apache-2.0
use super::*;
impl Ledger {
    fn reserve_default(
        &mut self,
        attempt: ReadAttemptBudget,
        ordinal: u8,
        requested: usize,
        partition: Option<&PartitionAllowance>,
        now_ns: u64,
    ) -> Result<Ticket> {
        self.reserve(
            attempt,
            &Budget::default(),
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
        parent_budget: airk_core::arbiter::Budget::default(),
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
        .reserve_default(attempt(), 1, 1024, Some(&one), 100)
        .unwrap();
    let other = partition(4, 1, 1024);
    assert!(
        ledger
            .reserve_default(attempt(), 1, 1024, Some(&other), 101)
            .is_err()
    );
    let two = partition(4, 2, 1024);
    ledger
        .reserve_default(attempt(), 2, 1024, Some(&two), 102)
        .unwrap();
    let three = partition(5, 3, 1);
    assert!(
        ledger
            .reserve_default(attempt(), 3, 1, Some(&three), 103)
            .is_err()
    );
}
#[test]
fn only_actual_compiled_tokens_release_unused_reservation_and_never_replay() {
    let mut ledger = Ledger::default();
    let ticket = ledger
        .reserve_default(attempt(), 1, 2048, None, 100)
        .unwrap();
    assert!(ledger.reserve_default(attempt(), 2, 1, None, 101).is_err());
    ledger.compiled(ticket, 512).unwrap();
    assert!(ledger.reserve_default(attempt(), 2, 1, None, 102).is_err());
    ledger.completed(ticket, 512).unwrap();
    ledger
        .reserve_default(attempt(), 2, 1536, None, 102)
        .unwrap();
    assert!(ledger.reserve_default(attempt(), 1, 1, None, 103).is_err());
    assert!(ledger.reserve_default(attempt(), 3, 1, None, 104).is_err());
    assert!(ledger.compiled(ticket, 0).is_err());
    assert!(ledger.compiled(ticket, 256).is_err());
    assert!(ledger.compiled(ticket, 513).is_err());
}
#[test]
fn original_parent_and_partition_identity_cannot_change() {
    let mut ledger = Ledger::default();
    let ticket = ledger.reserve_default(attempt(), 1, 1, None, 100).unwrap();
    assert_eq!(ticket.remaining_ns(100), 30_000_000_000);
    let mut changed = attempt();
    changed.expires_monotonic_ns += 1;
    assert!(ledger.reserve_default(changed, 2, 1, None, 101).is_err());
    assert!(
        ledger
            .reserve_default(attempt(), 2, 1, Some(&partition(3, 2, 1)), 101)
            .is_err()
    );
    let mut ledger = Ledger::default();
    let mut one = partition(3, 1, 1024);
    one.assessment_ordinals = vec![1, 2];
    ledger
        .reserve_default(attempt(), 1, 100, Some(&one), 100)
        .unwrap();
    one.resources.input_tokens = 1023;
    assert!(
        ledger
            .reserve_default(attempt(), 2, 100, Some(&one), 101)
            .is_err()
    );
}
#[test]
fn thirty_second_window_cannot_be_renewed_under_longer_original_parent() {
    let mut ledger = Ledger::default();
    ledger.reserve_default(attempt(), 1, 1, None, 100).unwrap();
    assert!(
        ledger
            .reserve_default(attempt(), 2, 1, None, 30_000_000_100)
            .is_err()
    );
    assert!(
        ledger
            .reserve_default(attempt(), 3, 1, None, 60_000_000_100)
            .is_err()
    );
    assert!(
        ledger
            .reserve_default(attempt(), 3, 1, None, 120_000_000_100)
            .is_err()
    );
}
#[test]
fn partition_token_ceiling_and_ordinal_are_both_enforced() {
    let mut ledger = Ledger::default();
    let mut one = partition(3, 1, 1000);
    one.assessment_ordinals = vec![1, 2];
    ledger
        .reserve_default(attempt(), 1, 600, Some(&one), 100)
        .unwrap();
    assert!(
        ledger
            .reserve_default(attempt(), 2, 401, Some(&one), 101)
            .is_err()
    );
    ledger
        .reserve_default(attempt(), 2, 400, Some(&one), 102)
        .unwrap();
    assert!(
        ledger
            .reserve_default(attempt(), 3, 1, Some(&one), 103)
            .is_err()
    );
}
#[test]
fn captured_small_parent_budget_cannot_be_enlarged_by_later_callback() {
    let parent = Budget {
        input_tokens: 512,
        rounds: 1,
        ..Default::default()
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
            .reserve(attempt(), &Budget::default(), 2, 1, None, 101)
            .is_err()
    );
    let another = Budget {
        input_tokens: 512,
        rounds: 3,
        ..Default::default()
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
