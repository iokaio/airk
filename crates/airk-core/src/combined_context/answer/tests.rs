// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::arbiter::answer_loop::ContextResponseV3;
use serde_json::Value;

fn ids(count: u8) -> Vec<IndexId> {
    (1..=count)
        .map(|value| IndexId::from_identity(Incarnation::from_bytes([value; 16])))
        .collect()
}
fn attempt() -> Incarnation {
    Incarnation::from_bytes([40; 16])
}
fn selection() -> Sha256Digest {
    Sha256Digest::from_bytes([41; 32])
}
/// The valid two-part reply of the Rust-emitted Answer /3 semantic cases.
fn reply() -> ContextResponseV3 {
    let cases: Value = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/split/arbiter/answer_loop_cases.json"
    ))
    .unwrap();
    serde_json::from_value(cases["response_cases"][0]["response"].clone()).unwrap()
}

#[test]
fn initial_v3_allocations_share_every_counter_with_ordinal_prefixes() -> Result<()> {
    let budget = BudgetV3::default();
    let allowances = initial_allocations_v3(attempt(), selection(), &ids(3), &budget)?;
    let rows: Vec<_> = allowances
        .iter()
        .map(|allowance| {
            let allowance = allowance.as_ref().unwrap();
            (
                allowance.resources.calls,
                allowance.resources.candidates,
                allowance.resources.bytes,
                allowance.resources.input_tokens,
                allowance.resources.focus_pairs,
                allowance.card_ordinals.clone(),
                allowance.focus_ordinals.clone(),
            )
        })
        .collect();
    let slots: Vec<u8> = (1..=16).collect();
    let focus: Vec<u8> = (1..=8).collect();
    assert_eq!(
        rows,
        vec![
            (44, 44, 87_382, 65_536, 64, slots.clone(), focus.clone()),
            (44, 44, 87_381, 65_536, 64, slots.clone(), focus.clone()),
            (44, 44, 87_381, 65_536, 64, slots, focus),
        ]
    );
    // One slot and one Focus call: only the first member carries model work,
    // and only it is given tokens.
    let narrow = BudgetV3 {
        context_calls: 1,
        focus_calls: 1,
        focus_pairs: 1,
        ..BudgetV3::default()
    };
    let allowances = initial_allocations_v3(attempt(), selection(), &ids(2), &narrow)?;
    let second = allowances[1].as_ref().unwrap();
    assert!(second.card_ordinals.is_empty() && second.focus_ordinals.is_empty());
    assert_eq!(
        (second.resources.input_tokens, second.resources.focus_pairs),
        (0, 0)
    );
    let first = allowances[0].as_ref().unwrap();
    assert_eq!(
        (
            first.card_ordinals.clone(),
            first.focus_ordinals.clone(),
            first.resources.input_tokens
        ),
        (vec![1], vec![1], 196_608)
    );
    // Too little room for every member: the rest are not admitted.
    let tight = BudgetV3 {
        calls: 1,
        candidates: 1,
        ..BudgetV3::default()
    };
    let allowances = initial_allocations_v3(attempt(), selection(), &ids(2), &tight)?;
    assert!(allowances[0].is_some() && allowances[1].is_none());
    // Review finding 6: model work is shared over the members that can carry
    // it. Two tokens carry two of four members: each gets half of the 48
    // slots and 24 Focus calls, and the Focus pairs go to those two only.
    let scarce = BudgetV3 {
        input_tokens: 2,
        round_input_tokens: 2,
        ..BudgetV3::default()
    };
    let allowances = initial_allocations_v3(attempt(), selection(), &ids(4), &scarce)?;
    let model: Vec<_> = allowances
        .iter()
        .map(|allowance| {
            let allowance = allowance.as_ref().unwrap();
            (
                allowance.card_ordinals.len(),
                allowance.focus_ordinals.len(),
                allowance.resources.focus_pairs,
                allowance.resources.input_tokens,
            )
        })
        .collect();
    assert_eq!(
        model,
        vec![(24, 12, 96, 1), (24, 12, 96, 1), (0, 0, 0, 0), (0, 0, 0, 0)]
    );
    // One Focus call over three model members: its member takes every pair.
    let one_focus = BudgetV3 {
        focus_calls: 1,
        ..BudgetV3::default()
    };
    let allowances = initial_allocations_v3(attempt(), selection(), &ids(3), &one_focus)?;
    let pairs: Vec<_> = allowances
        .iter()
        .map(|allowance| allowance.as_ref().unwrap().resources.focus_pairs)
        .collect();
    assert_eq!(pairs, vec![192, 0, 0]);
    Ok(())
}

#[test]
fn unused_v3_work_is_handed_back_by_count() -> Result<()> {
    let budget = BudgetV3::default();
    let mut allowances = initial_allocations_v3(attempt(), selection(), &ids(2), &budget)?;
    let first = allowances[0].clone().unwrap();
    let used = LoopUse {
        calls: 10,
        input_tokens: 9_000,
        candidates: 10,
        bytes: 20_000,
        card_slots: 5,
        focus_calls: 2,
        focus_pairs: 30,
    };
    let unused = LoopUse::allowance(&first).checked_sub(&used)?;
    assert_eq!(
        unused,
        LoopUse {
            calls: 56,
            input_tokens: 89_304,
            candidates: 56,
            bytes: 111_072,
            card_slots: 19,
            focus_calls: 10,
            focus_pairs: 66,
        }
    );
    let second = allowances[1].as_mut().unwrap();
    grow_allowance(second, &unused)?;
    second.validate()?;
    assert_eq!(second.card_ordinals, (1..=43).collect::<Vec<u8>>());
    assert_eq!(second.focus_ordinals, (1..=22).collect::<Vec<u8>>());
    assert_eq!(second.resources.calls, 122);
    let over = LoopUse {
        card_slots: 25,
        ..used.clone()
    };
    assert!(LoopUse::allowance(&first).checked_sub(&over).is_err());
    // A member the initial allocation did not admit runs on returned work.
    let returned = returned_allowance(attempt(), selection(), ids(1)[0], &budget, &unused, 0)?
        .expect("returned work carries a read");
    returned.validate()?;
    // Returned bytes below the question plus a minimal reply admit no member.
    assert!(
        returned_allowance(attempt(), selection(), ids(1)[0], &budget, &unused, 111_073)?
            .is_none()
    );
    assert!(
        returned_allowance(attempt(), selection(), ids(1)[0], &budget, &unused, 111_072)?
            .is_some()
    );
    let reads_only = LoopUse {
        input_tokens: 0,
        ..unused.clone()
    };
    let returned =
        returned_allowance(attempt(), selection(), ids(1)[0], &budget, &reads_only, 0)?
            .expect("reads alone");
    assert!(returned.card_ordinals.is_empty() && returned.focus_ordinals.is_empty());
    assert_eq!(returned.resources.focus_pairs, 0);
    returned.validate()?;
    let tiny = LoopUse {
        bytes: 1023,
        ..unused
    };
    assert!(returned_allowance(attempt(), selection(), ids(1)[0], &budget, &tiny, 0)?.is_none());
    let mut withdrawn = first;
    without_models(&mut withdrawn);
    withdrawn.validate()?;
    Ok(())
}

fn summary() -> Result<CombinedContextSummaryV2> {
    let budget = BudgetV3::default();
    let members = ids(2);
    let allowances = initial_allocations_v3(attempt(), selection(), &members, &budget)?;
    let used = LoopUse {
        calls: 10,
        input_tokens: 9_000,
        candidates: 10,
        bytes: 20_000,
        card_slots: 5,
        focus_calls: 2,
        focus_pairs: 30,
    };
    let held = LoopUse::allowance(allowances[1].as_ref().unwrap());
    Ok(CombinedContextSummaryV2 {
        schema: SUMMARY_SCHEMA_V2.into(),
        attempt: attempt(),
        selection: selection(),
        budget,
        members: vec![
            PartitionReportV3 {
                index_id: members[0],
                allowance: allowances[0].clone(),
                outcome: PartitionOutcomeV3::Known {
                    used: used.clone(),
                    read_depth: 3,
                    peak_fanout: 8,
                },
            },
            PartitionReportV3 {
                index_id: members[1],
                allowance: allowances[1].clone(),
                outcome: PartitionOutcomeV3::Unknown,
            },
        ],
        known_used: used,
        unknown_held: held,
        termination: Termination::Partial,
        delivery_bindings: vec![],
    })
}

#[test]
fn combined_context_v2_summary_reconciles_known_and_held_counts() -> Result<()> {
    let members = ids(2);
    summary()?.validate(&members)?;
    type Mutation = fn(&mut CombinedContextSummaryV2);
    let refusals: [(&str, Mutation); 7] = [
        ("v1 schema", |s| s.schema = "airk.combined-context/1".into()),
        ("known total", |s| s.known_used.card_slots += 1),
        ("held total", |s| s.unknown_held.focus_pairs -= 1),
        ("violation terminal", |s| {
            s.termination = Termination::AccountingViolation
        }),
        ("complete with unknown", |s| {
            s.termination = Termination::Complete
        }),
        ("work over allowance", |s| {
            if let PartitionOutcomeV3::Known { used, .. } = &mut s.members[0].outcome {
                used.card_slots = 25;
            }
            s.known_used.card_slots = 25;
        }),
        ("foreign parent budget", |s| {
            s.members[0]
                .allowance
                .as_mut()
                .unwrap()
                .parent_budget
                .rounds = 2
        }),
    ];
    for (id, mutate) in refusals {
        let mut wrong = summary()?;
        mutate(&mut wrong);
        assert!(wrong.validate(&members).is_err(), "{id}");
    }
    // Review finding 7: after unknown model work, a later member holding
    // model work is refused; the same member without models is accepted.
    let three = ids(3);
    let budget = BudgetV3::default();
    let allowances = initial_allocations_v3(attempt(), selection(), &three, &budget)?;
    let held = LoopUse::allowance(allowances[0].as_ref().unwrap());
    let later = |allowance: PartitionAllowanceV3| -> Result<CombinedContextSummaryV2> {
        let second = LoopUse::allowance(&allowance);
        Ok(CombinedContextSummaryV2 {
            schema: SUMMARY_SCHEMA_V2.into(),
            attempt: attempt(),
            selection: selection(),
            budget: budget.clone(),
            members: vec![
                PartitionReportV3 {
                    index_id: three[0],
                    allowance: allowances[0].clone(),
                    outcome: PartitionOutcomeV3::Unknown,
                },
                PartitionReportV3 {
                    index_id: three[1],
                    allowance: Some(allowance),
                    outcome: PartitionOutcomeV3::Unknown,
                },
                PartitionReportV3 {
                    index_id: three[2],
                    allowance: None,
                    outcome: PartitionOutcomeV3::Unqueried {
                        reason: UnqueriedReason::Budget,
                    },
                },
            ],
            known_used: LoopUse::default(),
            unknown_held: held.checked_add(&second)?,
            termination: Termination::Partial,
            delivery_bindings: vec![],
        })
    };
    let with_models = allowances[1].clone().unwrap();
    assert!(later(with_models.clone())?.validate(&three).is_err());
    let mut withdrawn = with_models;
    without_models(&mut withdrawn);
    later(withdrawn)?.validate(&three)?;
    Ok(())
}

#[test]
fn an_answer_v3_reply_binds_focus_and_context_from_its_own_facts() -> Result<()> {
    let reply = reply();
    let focus = reply
        .model_work
        .focus_profile
        .expect("fixture Focus profile");
    let context = reply.applications[0].profile;
    let bindings = reply_delivery_bindings(&reply)?;
    assert_eq!(
        bindings
            .iter()
            .map(|binding| (binding.stage, binding.profile))
            .collect::<Vec<_>>(),
        vec![(Stage::Focus, Some(focus)), (Stage::Context, context)]
    );
    assert!(bindings.iter().all(|binding| binding.scope == reply.scope));
    // The same roster from the delivered value.
    assert_eq!(
        crate::combined_context::context_delivery_bindings(&serde_json::to_value(&reply)?)?,
        bindings
    );
    let mut deterministic = reply.clone();
    deterministic.decision_log.card_calls = 0;
    deterministic.model_work.focus_profile = None;
    assert!(reply_delivery_bindings(&deterministic)?.is_empty());
    let mut unprofiled = reply;
    unprofiled.applications[0].profile = None;
    assert!(reply_delivery_bindings(&unprofiled).is_err());
    Ok(())
}
