// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../fixtures/split/i1/combined_context_allowance.json"
    ))
    .unwrap()
}
fn ids(count: usize) -> Vec<IndexId> {
    (1..=count)
        .map(|value| {
            IndexId::from_identity(Incarnation::from_bytes([u8::try_from(value).unwrap(); 16]))
        })
        .collect()
}
fn allocations(count: usize) -> Vec<Option<PartitionAllowance>> {
    initial_allocations(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &ids(count),
        &Budget::default(),
    )
    .unwrap()
}
fn summary() -> CombinedContextSummary {
    let allowances = allocations(2);
    CombinedContextSummary {
        schema: SUMMARY_SCHEMA.into(),
        attempt: Incarnation::from_bytes([9; 16]),
        selection: Sha256Digest::from_bytes([8; 32]),
        budget: Budget::default(),
        members: allowances
            .into_iter()
            .enumerate()
            .map(|(offset, allowance)| PartitionReport {
                index_id: ids(2)[offset],
                allowance,
                outcome: PartitionOutcome::Unknown,
            })
            .collect(),
        known_used: Resources::default(),
        unknown_held: Resources::budget(&Budget::default()),
        termination: Termination::Partial,
    }
}

#[test]
fn combined_context_literal_allocations_preserve_one_global_budget() {
    for case in fixture()["allocations"].as_array().unwrap() {
        let budget: Budget = serde_json::from_value(case["budget"].clone()).unwrap();
        let count = usize::try_from(case["members"].as_u64().unwrap()).unwrap();
        let actual = initial_allocations(
            Incarnation::from_bytes([9; 16]),
            Sha256Digest::from_bytes([8; 32]),
            &ids(count),
            &budget,
        )
        .unwrap();
        let values: Vec<_> = actual.into_iter().map(|allowance| match allowance {
            None => Value::Null,
            Some(a) => json!({"calls":a.resources.calls,"input_tokens":a.resources.input_tokens,
                "candidates":a.resources.candidates,"bytes":a.resources.bytes,"ordinals":a.assessment_ordinals}),
        }).collect();
        assert_eq!(
            values,
            *case["expected"].as_array().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn combined_context_allowance_rejects_foreign_bindings_and_bad_ordinals() {
    let a = allocations(2).remove(0).unwrap();
    let attempt = ReadAttemptBudget {
        schema: 1,
        request_id: a.attempt,
        clock: crate::read_attempt::ReadClock::UnixMonotonic,
        expires_monotonic_ns: 2_000_000_000,
        remaining_ns: 1_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    a.validate_binding(&attempt, a.selection, a.index_id)
        .unwrap();
    assert!(
        a.validate_binding(&attempt, a.selection, ids(2)[1])
            .is_err()
    );
    assert!(
        a.validate_binding(&attempt, Sha256Digest::from_bytes([7; 32]), a.index_id)
            .is_err()
    );
    let mut foreign = attempt;
    foreign.request_id = Incarnation::from_bytes([1; 16]);
    assert!(
        a.validate_binding(&foreign, a.selection, a.index_id)
            .is_err()
    );
    for ordinals in [vec![0], vec![4], vec![1, 1], vec![2, 1], vec![]] {
        let mut bad = a.clone();
        bad.assessment_ordinals = ordinals;
        assert!(bad.validate().is_err());
    }
}

#[test]
fn combined_context_unknown_work_holds_full_original_reservations() {
    let good = summary();
    good.validate(&ids(2)).unwrap();
    let mut bad = good.clone();
    bad.unknown_held.input_tokens = 0;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = good.clone();
    bad.termination = Termination::Complete;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = good.clone();
    bad.members.remove(1);
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = good;
    bad.members[1].allowance = None;
    assert!(bad.validate(&ids(2)).is_err());
}

#[test]
fn combined_context_known_work_cannot_spend_unknown_assessment_or_token_credit() {
    let mut value = summary();
    let allowance = value.members[0].allowance.as_ref().unwrap().clone();
    value.members[0].outcome = PartitionOutcome::Known {
        used: Resources::default(),
        assessment_ordinals: vec![],
        read_depth: 1,
        peak_fanout: 1,
    };
    value.unknown_held = value.members[1]
        .allowance
        .as_ref()
        .unwrap()
        .resources
        .clone();
    value.validate(&ids(2)).unwrap();
    let mut bad = value.clone();
    bad.members[1]
        .allowance
        .as_mut()
        .unwrap()
        .assessment_ordinals = vec![1];
    bad.members[0].outcome = PartitionOutcome::Known {
        used: Resources {
            input_tokens: 1,
            ..Resources::default()
        },
        assessment_ordinals: vec![1],
        read_depth: 1,
        peak_fanout: 1,
    };
    bad.known_used.input_tokens = 1;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = value;
    bad.members[0].outcome = PartitionOutcome::Known {
        used: Resources {
            calls: allowance.resources.calls + 1,
            ..Resources::default()
        },
        assessment_ordinals: vec![],
        read_depth: 1,
        peak_fanout: 1,
    };
    assert!(bad.validate(&ids(2)).is_err());
}

#[test]
fn combined_context_completed_assessment_requires_positive_actual_token_charge() {
    let mut value = summary();
    value.members[0].outcome = PartitionOutcome::Known {
        used: Resources {
            input_tokens: 7,
            ..Resources::default()
        },
        assessment_ordinals: vec![1],
        read_depth: 1,
        peak_fanout: 1,
    };
    value.known_used.input_tokens = 7;
    value.unknown_held = value.members[1]
        .allowance
        .as_ref()
        .unwrap()
        .resources
        .clone();
    value.validate(&ids(2)).unwrap();
    if let PartitionOutcome::Known { used, .. } = &mut value.members[0].outcome {
        used.input_tokens = 0;
    }
    value.known_used.input_tokens = 0;
    assert!(value.validate(&ids(2)).is_err());
}

#[test]
fn combined_context_explicit_unqueried_members_cannot_claim_complete() {
    let mut value = summary();
    value.members[1].allowance = None;
    value.members[1].outcome = PartitionOutcome::Unqueried {
        reason: UnqueriedReason::Budget,
    };
    value.unknown_held = value.members[0]
        .allowance
        .as_ref()
        .unwrap()
        .resources
        .clone();
    value.validate(&ids(2)).unwrap();
    value.termination = Termination::Complete;
    assert!(value.validate(&ids(2)).is_err());
}

#[test]
fn combined_context_accounting_overflow_and_noncanonical_rosters_refuse() {
    let huge = Resources {
        calls: usize::MAX,
        ..Resources::default()
    };
    assert!(
        huge.checked_add(&Resources {
            calls: 1,
            ..Resources::default()
        })
        .is_err()
    );
    for members in [vec![], vec![ids(1)[0]; 2], vec![ids(2)[1], ids(2)[0]]] {
        assert!(
            initial_allocations(
                Incarnation::from_bytes([9; 16]),
                Sha256Digest::from_bytes([8; 32]),
                &members,
                &Budget::default()
            )
            .is_err()
        );
    }
    let mut malformed = serde_json::to_value(allocations(2).remove(0).unwrap()).unwrap();
    malformed["caller_supplied_authority"] = json!(true);
    assert!(serde_json::from_value::<PartitionAllowance>(malformed).is_err());
}
