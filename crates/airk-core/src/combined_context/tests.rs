// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::read_attempt::ReadAttemptBudget;
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
        &historical_budget(),
    )
    .unwrap()
}
fn summary() -> CombinedContextSummary {
    let allowances = allocations(2);
    CombinedContextSummary {
        schema: SUMMARY_SCHEMA.into(),
        attempt: Incarnation::from_bytes([9; 16]),
        selection: Sha256Digest::from_bytes([8; 32]),
        budget: historical_budget(),
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
        unknown_held: Resources::budget(&historical_budget()),
        termination: Termination::Partial,
        delivery_bindings: Vec::new(),
    }
}

// Historical controls and their retained native fixtures keep their original
// 2048-token/30-second input identity when the shipping default changes.
fn historical_budget() -> crate::arbiter::Budget {
    crate::arbiter::Budget {
        calls: 8,
        rounds: 3,
        fanout: 8,
        input_tokens: 2048,
        candidates: 32,
        max_bytes: 262_144,
        deadline_ms: 30_000,
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
                &historical_budget()
            )
            .is_err()
        );
    }
    let mut malformed = serde_json::to_value(allocations(2).remove(0).unwrap()).unwrap();
    malformed["caller_supplied_authority"] = json!(true);
    assert!(serde_json::from_value::<PartitionAllowance>(malformed).is_err());
}

#[test]
fn combined_context_known_failed_overrun_retains_observation_without_enlarging_capacity() {
    let literal = fixture()["known_violation"].clone();
    let observed: Resources = serde_json::from_value(literal["observed"].clone()).unwrap();
    let mut value = summary();
    value.members[0].outcome = PartitionOutcome::KnownViolation {
        observed: observed.clone(),
        assessment_ordinals: serde_json::from_value(literal["ordinals"].clone()).unwrap(),
        read_depth: 1,
        peak_fanout: 1,
    };
    value.members[1].allowance = None;
    value.members[1].outcome = PartitionOutcome::Unqueried {
        reason: UnqueriedReason::AccountingViolation,
    };
    value.known_used = observed;
    value.unknown_held = Resources::default();
    value.termination = Termination::AccountingViolation;
    value.validate(&ids(2)).unwrap();
    assert_eq!(value.known_used.input_tokens, 2300);
    assert_eq!(value.budget.input_tokens, 2048);
    assert_eq!(
        value.members[0]
            .allowance
            .as_ref()
            .unwrap()
            .resources
            .input_tokens,
        1024
    );
    let mut bad = value.clone();
    bad.termination = Termination::Complete;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = value.clone();
    if let PartitionOutcome::KnownViolation { observed, .. } = &mut bad.members[0].outcome {
        observed.input_tokens = 100;
    }
    bad.known_used.input_tokens = 100;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = value.clone();
    bad.members[1].allowance = allocations(2).remove(1);
    bad.members[1].outcome = PartitionOutcome::Unknown;
    assert!(bad.validate(&ids(2)).is_err());
    let mut bad = value;
    let over = bad.known_used.clone();
    bad.members[0].outcome = PartitionOutcome::Known {
        used: over,
        assessment_ordinals: vec![1],
        read_depth: 1,
        peak_fanout: 1,
    };
    assert!(bad.validate(&ids(2)).is_err());
}

#[test]
fn context_delivery_binding_required_fields_cannot_disappear_or_wildcard_profile() {
    let value = serde_json::to_value(summary()).unwrap();
    assert!(serde_json::from_value::<CombinedContextSummary>(value.clone()).is_ok());
    assert_eq!(value["delivery_bindings"], json!([]));
    let mut absent = value;
    absent.as_object_mut().unwrap().remove("delivery_bindings");
    assert!(serde_json::from_value::<CombinedContextSummary>(absent).is_err());
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))
    .unwrap();
    let packet: crate::arbiter::context_action::ContextResponseV2 =
        serde_json::from_value(cases["cases"][0]["input"].clone()).unwrap();
    let binding = ContextDeliveryBinding::from_packet(&packet)
        .unwrap()
        .unwrap();
    let mut raw = serde_json::to_value(binding).unwrap();
    raw.as_object_mut().unwrap().remove("profile");
    assert!(serde_json::from_value::<ContextDeliveryBinding>(raw.clone()).is_err());
    raw["profile"] = Value::Null;
    assert!(serde_json::from_value::<ContextDeliveryBinding>(raw).is_ok());
}

#[test]
fn context_delivery_extractor_retains_full_packet_and_fences_actual_reply_roster() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))
    .unwrap();
    let mut raw = cases["cases"][0]["input"].clone();
    raw["future_public_field"] = json!({"preserved":true});
    let original = raw.clone();
    let bound = context_delivery_bindings(&raw).unwrap();
    assert_eq!(bound.len(), 1);
    assert!(bound[0].profile.is_some());
    assert_eq!(raw, original);
    let text = json!({"content":[{"type":"text","text":raw.to_string()}]});
    assert_eq!(context_delivery_bindings(&text).unwrap(), bound);
    let mut packet: crate::arbiter::context_action::ContextResponseV2 =
        serde_json::from_value(cases["cases"][0]["input"].clone()).unwrap();
    packet.assessment_history.clear();
    packet.work.input_tokens = 0;
    assert!(
        ContextDeliveryBinding::from_packet(&packet)
            .unwrap()
            .is_none()
    );
    let mut unknown = packet;
    let first: crate::arbiter::context_action::AssessmentEvent =
        serde_json::from_value(cases["cases"][0]["input"]["assessment_history"][0].clone())
            .unwrap();
    let mut event = first;
    event.result = crate::arbiter::context_action::AssessmentResult::Unavailable {
        reason: crate::arbiter::context_action::AssessmentUnavailable::Unavailable,
        work: crate::arbiter::Work::default(),
        work_unknown: true,
        advice: None,
    };
    unknown.assessment_history.push(event);
    assert!(
        ContextDeliveryBinding::from_packet(&unknown).is_err(),
        "unknown work without an observed profile cannot grant delivery"
    );
    unknown.applications.push(crate::arbiter::IndexApplication {
        scope: unknown.assessment.scope.clone(),
        profile: bound[0].profile,
        availability: crate::arbiter::Availability::Failed,
        reason: "native work outcome unknown".into(),
    });
    assert!(
        ContextDeliveryBinding::from_packet(&unknown)
            .unwrap()
            .is_some()
    );
}

#[test]
fn context_delivery_known_work_cannot_omit_its_actual_native_packet() {
    let mut summary = summary();
    let rows: Vec<_> = ids(2)
        .into_iter()
        .map(
            |index_id| json!({"index_id":index_id,"result":{"error":{"code":"transport_unknown"}}}),
        )
        .collect();
    let mut body = json!({"schema":crate::combined_reads::COMBINED_READ_SCHEMA,"members":rows,"context":summary});
    assert!(context_delivery_bindings(&body).unwrap().is_empty());
    summary.members[0].outcome = PartitionOutcome::Known {
        used: Resources::default(),
        assessment_ordinals: Vec::new(),
        read_depth: 0,
        peak_fanout: 0,
    };
    summary.unknown_held = summary.members[1]
        .allowance
        .as_ref()
        .unwrap()
        .resources
        .clone();
    summary.validate(&ids(2)).unwrap();
    body["context"] = serde_json::to_value(summary).unwrap();
    assert!(context_delivery_bindings(&body).is_err());
}

#[test]
fn context_delivery_known_cannot_hide_unknown_work_or_actual_charged_ordinal() {
    use crate::arbiter::context_action::ContextResponseV2;
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))
    .unwrap();
    let mut packet = cases["cases"][0]["input"].clone();
    let typed: ContextResponseV2 = serde_json::from_value(packet.clone()).unwrap();
    let index = typed.assessment.scope.index_id;
    let allowance = initial_allocations(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &[index],
        &historical_budget(),
    )
    .unwrap()
    .remove(0);
    let mut summary = CombinedContextSummary {
        schema: SUMMARY_SCHEMA.into(),
        attempt: Incarnation::from_bytes([9; 16]),
        selection: Sha256Digest::from_bytes([8; 32]),
        budget: historical_budget(),
        members: vec![PartitionReport {
            index_id: index,
            allowance,
            outcome: PartitionOutcome::Known {
                used: Resources::from_packet(&typed).unwrap(),
                assessment_ordinals: vec![1],
                read_depth: 0,
                peak_fanout: 0,
            },
        }],
        known_used: Resources::from_packet(&typed).unwrap(),
        unknown_held: Resources::default(),
        termination: Termination::Complete,
        delivery_bindings: vec![
            ContextDeliveryBinding::from_packet(&typed)
                .unwrap()
                .unwrap(),
        ],
    };
    let mut body = json!({"schema":crate::combined_reads::COMBINED_READ_SCHEMA,"members":[{"index_id":index,"result":packet}],"context":summary});
    assert_eq!(context_delivery_bindings(&body).unwrap().len(), 1);
    body["context"]["members"][0]["outcome"]["assessment_ordinals"] = json!([]);
    assert!(
        context_delivery_bindings(&body).is_err(),
        "actual charged ordinal cannot vanish from Known report"
    );
    let mut advice = packet["assessment_history"][0]["result"]["advice"].clone();
    advice["outcome"] = json!({"status":"unavailable","reason":"completion remains unknown"});
    packet["assessment_history"][0]["result"] = json!({"status":"unavailable","reason":"unavailable","work":advice["work"],"work_unknown":true,"advice":advice});
    packet["stop"] = json!("deadline");
    packet["assessment"]["termination"] = json!("deadline");
    packet["assessment"]["coverage"] = json!({"status":"unassessed","reason":"deadline"});
    packet["gaps"] = json!([]);
    let typed = ContextResponseV2::from_canonical(&packet, &historical_budget()).unwrap();
    let used = Resources::from_packet(&typed).unwrap();
    summary.known_used = used.clone();
    if let PartitionOutcome::Known { used: observed, .. } = &mut summary.members[0].outcome {
        *observed = used;
    }
    summary.validate(&[index]).unwrap();
    body["context"] = serde_json::to_value(summary).unwrap();
    body["members"][0]["result"] = packet;
    assert!(
        context_delivery_bindings(&body).is_err(),
        "Unknown native work cannot be relabeled ordinary Known"
    );
}
#[test]
fn focus_delivery_extraction_is_canonical_staged_and_ignores_arbitrary_metadata()
-> anyhow::Result<()> {
    use crate::arbiter::{
        Stage,
        stage_tests::{digest, scope},
    };
    let record = crate::arbiter::focus::tests::search_record(scope())?;
    let search = record.canonical_result;
    let bindings = context_delivery_bindings(&search)?;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].stage, Stage::Focus);
    assert_eq!(bindings[0].profile, Some(digest(7)));
    let text = json!({"content":[{"type":"text","text":search.to_string()}]});
    assert_eq!(context_delivery_bindings(&text)?, bindings);
    assert!(context_delivery_bindings(&json!({"metadata":search.clone()}))?.is_empty());
    let mut changed = search.clone();
    changed["generation"] = 2.into();
    assert!(context_delivery_bindings(&changed).is_err());
    let mut changed = search.clone();
    changed["focus"]["receipt"]["presentations"][0]["source_bytes"] = 11.into();
    assert!(context_delivery_bindings(&changed).is_err());
    let mut future = search;
    future.as_object_mut().unwrap().remove("focus");
    future["hits"][0]["text"] = serde_json::to_value(&text)?.to_string().into();
    assert!(context_delivery_bindings(&future)?.is_empty());
    Ok(())
}
#[test]
fn legacy_context_delivery_bytes_omit_default_stage_and_focus_is_explicit() -> anyhow::Result<()> {
    use crate::arbiter::{
        Stage,
        stage_tests::{digest, scope},
    };
    let context = ContextDeliveryBinding {
        stage: Stage::Context,
        scope: scope(),
        profile: Some(digest(7)),
    };
    let expected = format!(
        "{{\"scope\":{},\"profile\":\"{}\"}}",
        serde_json::to_string(&scope())?,
        digest(7)
    );
    assert_eq!(serde_json::to_string(&context)?, expected);
    let prior: ContextDeliveryBinding = serde_json::from_str(&expected)?;
    assert_eq!(prior, context);
    let focus = ContextDeliveryBinding {
        stage: Stage::Focus,
        ..context
    };
    assert_eq!(serde_json::to_value(focus)?["stage"], "focus");
    Ok(())
}
#[test]
fn same_index_may_bind_both_stages_but_duplicate_stage_is_refused() -> anyhow::Result<()> {
    use crate::arbiter::{
        Stage,
        stage_tests::{digest, scope},
    };
    let mut value = summary();
    let mut source = scope();
    source.index_id = ids(2)[0];
    let focus = ContextDeliveryBinding {
        stage: Stage::Focus,
        scope: source.clone(),
        profile: Some(digest(7)),
    };
    let context = ContextDeliveryBinding {
        stage: Stage::Context,
        scope: source,
        profile: Some(digest(8)),
    };
    value.delivery_bindings = vec![focus.clone(), context];
    value.validate(&ids(2))?;
    value.delivery_bindings.push(focus);
    assert!(value.validate(&ids(2)).is_err());
    Ok(())
}
#[test]
fn staged_delivery_native_fixture_emitter() -> anyhow::Result<()> {
    use crate::arbiter::{
        Stage,
        stage_tests::{digest, scope},
    };
    let bindings = vec![
        ContextDeliveryBinding {
            stage: Stage::Focus,
            scope: scope(),
            profile: Some(digest(7)),
        },
        ContextDeliveryBinding {
            stage: Stage::Context,
            scope: scope(),
            profile: Some(digest(8)),
        },
    ];
    validate_stage_roster(&bindings)?;
    let document = json!({"schema":"airk.stage-delivery-bindings/1","qualification":false,"bindings":bindings,
        "maximum_bindings":MAX_DELIVERY_BINDINGS,"legacy_context_bytes":serde_json::to_string(&bindings[1])?});
    if let Some(path) = std::env::var_os("AIRK_WRITE_STAGE_DELIVERY_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    Ok(())
}
fn ordinary_focus_allowance() -> PartitionAllowance {
    PartitionAllowance {
        schema: ALLOWANCE_SCHEMA.into(),
        attempt: Incarnation::from_bytes([9; 16]),
        selection: Sha256Digest::from_bytes([8; 32]),
        index_id: ids(1)[0],
        parent_budget: Budget {
            calls: 1,
            rounds: 1,
            fanout: 1,
            input_tokens: 512,
            candidates: 4,
            max_bytes: 65536,
            deadline_ms: 1000,
        },
        resources: Resources {
            calls: 1,
            input_tokens: 512,
            candidates: 4,
            bytes: 65536,
        },
        assessment_ordinals: vec![],
    }
}
#[test]
fn ordinary_focus_allowance_exact_purpose_preserves_existing_context_and_zero_work_rules()
-> anyhow::Result<()> {
    let focus = ordinary_focus_allowance();
    assert!(focus.is_ordinary_focus());
    focus.validate()?;
    let mut context = focus.clone();
    context.assessment_ordinals = vec![1];
    assert!(!context.is_ordinary_focus());
    context.validate()?;
    let mut known_zero = context.clone();
    known_zero.assessment_ordinals.clear();
    known_zero.resources.input_tokens = 0;
    assert!(!known_zero.is_ordinary_focus());
    known_zero.validate()?;
    assert_eq!(
        serde_json::from_slice::<PartitionAllowance>(&serde_json::to_vec(&context)?)?
            .assessment_ordinals,
        [1]
    );
    assert_eq!(
        serde_json::to_string(&context)?,
        r#"{"schema":"airk.context-partition-allowance/1","attempt":"09090909090909090909090909090909","selection":"0808080808080808080808080808080808080808080808080808080808080808","index_id":"idx_01010101010101010101010101010101","parent_budget":{"calls":1,"rounds":1,"fanout":1,"input_tokens":512,"candidates":4,"max_bytes":65536,"deadline_ms":1000},"resources":{"calls":1,"input_tokens":512,"candidates":4,"bytes":65536},"assessment_ordinals":[1]}"#
    );
    Ok(())
}
#[test]
fn ordinary_focus_allowance_does_not_infer_purpose_from_empty_ordinals_or_widen_bounds()
-> anyhow::Result<()> {
    let good = ordinary_focus_allowance();
    for name in ["calls", "rounds", "fanout"] {
        let mut raw = serde_json::to_value(&good)?;
        raw["parent_budget"][name] = 2.into();
        let bad: PartitionAllowance = serde_json::from_value(raw)?;
        assert!(!bad.is_ordinary_focus());
        assert!(bad.validate().is_err());
    }
    for (name, too_large) in [
        ("calls", 2),
        ("input_tokens", 513),
        ("candidates", 5),
        ("bytes", 65537),
    ] {
        let mut raw = serde_json::to_value(&good)?;
        raw["resources"][name] = too_large.into();
        let bad: PartitionAllowance = serde_json::from_value(raw)?;
        assert!(!bad.is_ordinary_focus());
        assert!(bad.validate().is_err());
    }
    let mut malformed = good.clone();
    malformed.schema = "foreign".into();
    assert!(!malformed.is_ordinary_focus());
    assert!(malformed.validate().is_err());
    let ordinary_context = allocations(1).remove(0).unwrap();
    assert!(!ordinary_context.is_ordinary_focus());
    ordinary_context.validate()?;
    Ok(())
}
#[test]
fn native_focus_only_allowance_fixture_emitter() -> anyhow::Result<()> {
    let focus = ordinary_focus_allowance();
    focus.validate()?;
    let mut context = focus.clone();
    context.assessment_ordinals = vec![1];
    context.validate()?;
    let mut zero = context.clone();
    zero.resources.input_tokens = 0;
    zero.assessment_ordinals.clear();
    zero.validate()?;
    let document = json!({"schema":"airk.focus-only-allowance-cases/1","qualification":false,
        "ordinary_focus":{"allowance":focus,"is_ordinary_focus":true,"canonical_tool":"search"},
        "context":{"allowance":context,"is_ordinary_focus":false,"canonical_tool":"answer"},
        "zero_optional_work":{"allowance":zero,"is_ordinary_focus":false,"canonical_tool":"answer"},
        "ordinary_focus_requires_own_outer_delivery_guard":true});
    if let Some(path) = std::env::var_os("AIRK_WRITE_FOCUS_ONLY_ALLOWANCE_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    Ok(())
}
