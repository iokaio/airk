// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::arbiter::stage_tests::{digest, scope};
use serde_json::json;

pub(crate) fn ranking() -> Result<FocusApplicationReceipt> {
    Ok(FocusApplicationReceipt {
        schema: RANKING_SCHEMA.into(),
        scope: scope(),
        identity: Some(FocusIdentity {
            profile: digest(7),
            runtime: RUNTIME.into(),
            compiler: digest(8),
            calibration: digest(9),
            task_policy: digest(10),
        }),
        availability: Availability::Ready,
        reason: "Synthetic complete prefix ranking; no model qualification".into(),
        applied: true,
        input_digest: digest(11),
        original_order: vec![digest(20), digest(21), digest(22), digest(23)],
        ranked_order: vec![digest(22), digest(20), digest(21), digest(23)],
        scores: vec![
            FocusScore::from_logits(digest(20), 0., 1.)?,
            FocusScore::from_logits(digest(21), 0., 0.)?,
            FocusScore::from_logits(digest(22), 0., 2.)?,
        ],
        presentations: (20..23)
            .map(|byte| FocusPresentation {
                candidate: digest(byte),
                utf8_start: 0,
                utf8_end: 5,
                source_bytes: 10,
            })
            .collect(),
        presented_count: 3,
        candidate_count: 4,
        scored_count: 3,
        omitted_count: 1,
        reserved_input_tokens: 100,
        observed_input_tokens: Some(90),
        work: Work {
            input_tokens: 90,
            candidates: 3,
            compile_us: Some(1),
            inference_us: Some(2),
            ..Work::default()
        },
    })
}
#[test]
fn relevance_ranking_preserves_complete_future_bodies_and_unscored_tail() -> Result<()> {
    let ranking = ranking()?;
    ranking.validate(&Budget::default())?;
    let original = vec![
        json!({"text":"first","future":{"secret_known_only_to_owner":1}}),
        json!({"text":"second"}),
        json!({"text":"third"}),
        json!({"text":"unscored","future_array":[1,2,3]}),
    ];
    assert_eq!(
        ranking.reorder(&original, &ranking.original_order, &Budget::default())?,
        vec![
            original[2].clone(),
            original[0].clone(),
            original[1].clone(),
            original[3].clone()
        ]
    );
    assert!(
        ranking
            .reorder(&original[..3], &ranking.original_order, &Budget::default())
            .is_err()
    );
    Ok(())
}
#[test]
fn exact_ties_remain_stable_and_never_borrow_sufficiency_authority() -> Result<()> {
    let mut value = ranking()?;
    value.scores[2] = FocusScore::from_logits(digest(22), 0., 1.)?;
    value.ranked_order = vec![digest(20), digest(22), digest(21), digest(23)];
    value.validate(&Budget::default())?;
    value.ranked_order.swap(0, 1);
    assert!(value.validate(&Budget::default()).is_err());
    assert!(
        serde_json::from_value::<context_action::ContextActionAdvice>(serde_json::to_value(
            &value
        )?)
        .is_err()
    );
    Ok(())
}
#[test]
fn malformed_duplicate_missing_and_nonprefix_scores_refuse() -> Result<()> {
    let original = ranking()?;
    let mut value = original.clone();
    value.ranked_order.pop();
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.ranked_order[0] = digest(99);
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.original_order[3] = digest(20);
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.scores.swap(0, 1);
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.scores[0].relevance = 0.9;
    assert!(value.validate(&Budget::default()).is_err());
    assert!(FocusScore::from_logits(digest(1), f32::NAN, 0.).is_err());
    assert!(FocusScore::from_logits(digest(1), 0., f32::INFINITY).is_err());
    Ok(())
}
#[test]
fn failed_partial_work_keeps_original_order_and_truthful_cost_without_replay() -> Result<()> {
    let mut value = ranking()?;
    value.applied = false;
    value.availability = Availability::Failed;
    assert!(value.validate(&Budget::default()).is_err());
    value.ranked_order = value.original_order.clone();
    value.validate(&Budget::default())?;
    assert_eq!(value.work.input_tokens, 90);
    assert_eq!(value.scored_count, 3);
    value.observed_input_tokens = Some(101);
    value.work.input_tokens = 101;
    value.validate(&Budget::default())?;
    value.work.calls = 1;
    assert!(value.validate(&Budget::default()).is_err());
    Ok(())
}
#[test]
fn observed_work_original_deadline_and_missing_completion_never_become_applied() -> Result<()> {
    let original = ranking()?;
    let mut value = original.clone();
    value.observed_input_tokens = None;
    value.work.input_tokens = 0;
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.work.inference_us = Some(30_000_001);
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.work.inference_us = Some(u64::MAX);
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original.clone();
    value.reserved_input_tokens = 89;
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = original;
    value.identity = None;
    assert!(value.validate(&Budget::default()).is_err());
    Ok(())
}
#[test]
fn native_focus_receipt_fixture_emitter() -> Result<()> {
    let applied = ranking()?;
    applied.validate(&Budget::default())?;
    let mut failed = applied.clone();
    failed.applied = false;
    failed.availability = Availability::Failed;
    failed.ranked_order = failed.original_order.clone();
    failed.validate(&Budget::default())?;
    let document = json!({"schema":"airk.arbiter.focus-semantic-cases/1","qualification":false,
        "budget":Budget::default(),"cases":[{"id":"complete_prefix","receipt":applied},{"id":"failed_atomic_fallback","receipt":failed}]});
    let mut document = document;
    document["compact_not_applied"] = serde_json::to_value(
        FocusObservation::not_applied_if_fits(
            Availability::Failed,
            "Original native scope or clock unavailable before dispatch".into(),
            4,
            None,
            512,
        )?
        .unwrap(),
    )?;
    document["observation_receipt"] = serde_json::to_value(FocusObservation::Receipt {
        receipt: Box::new(ranking()?),
    })?;
    document["focus_core_policy"] = serde_json::to_value(semantic_policy_digest()?)?;
    let attempt = crate::read_attempt::ReadAttemptBudget {
        schema: 1,
        request_id: crate::identity::Incarnation::from_bytes([9; 16]),
        clock: crate::read_attempt::ReadClock::UnixMonotonic,
        expires_monotonic_ns: 30_000_000_100,
        remaining_ns: 30_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    let limits = FocusProfileLimits {
        max_candidates: 10,
        pair_input_tokens: 128,
        max_input_tokens: 2048,
    };
    document["ordinary_allocation"] = json!({"input_attempt":attempt,"limits":limits,
        "returned_count":100,"requested_limit":100,"now_ns":100,"remaining_frame_bytes":100_000,
        "budget":ordinary_search_allocation(attempt,crate::read_attempt::ReadClock::UnixMonotonic,100,100,100,&limits,100_000)?});
    let pair_limits = FocusProfileLimits {
        max_candidates: 8,
        pair_input_tokens: 512,
        max_input_tokens: 2048,
    };
    let allocation_cases = [
        ("context_ready_initial", 2048, true, 0, 2048, 1024),
        ("context_ready_later_round", 2048, true, 512, 1536, 512),
        ("context_ready_exhausted", 2048, true, 1024, 1024, 0),
        ("context_not_ready", 2048, false, 0, 2048, 2048),
        ("actual_local_partition", 1024, true, 0, 2048, 512),
        ("shared_remainder", 2048, true, 0, 700, 512),
        ("no_whole_pair", 1023, true, 0, 1023, 0),
    ];
    let mut emitted = Vec::new();
    for (id, parent, ready, used, remaining, expected) in allocation_cases {
        let allowance = focus_token_allowance(parent, ready, used, remaining, &pair_limits)?;
        assert_eq!(allowance, expected);
        emitted.push(json!({"id":id,"local_parent_tokens":parent,
            "initial_context_ready":ready,"cumulative_focus_tokens":used,
            "shared_remaining_tokens":remaining,"allowance_tokens":allowance}));
    }
    document["nested_allocation"] = json!({"limits":pair_limits,"cases":emitted,
        "context_roster_fit_guaranteed":false});
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_FOCUS_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    assert_eq!(document["cases"].as_array().unwrap().len(), 2);
    Ok(())
}
#[test]
fn ordinary_search_allocation_uses_only_actual_original_time_and_profile_caps() -> Result<()> {
    use crate::identity::Incarnation;
    use crate::read_attempt::{ReadAttemptBudget, ReadClock};
    let attempt = ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([9; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 30_000_000_100,
        remaining_ns: 30_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    let limits = FocusProfileLimits {
        max_candidates: 10,
        pair_input_tokens: 128,
        max_input_tokens: 2048,
    };
    assert_eq!(
        ordinary_search_allocation(
            attempt,
            ReadClock::UnixMonotonic,
            100,
            100,
            100,
            &limits,
            100_000
        )?,
        Some(Budget {
            calls: 1,
            rounds: 1,
            fanout: 1,
            input_tokens: 2048,
            candidates: 10,
            max_bytes: 65_536,
            deadline_ms: 30_000
        })
    );
    let limits = FocusProfileLimits {
        max_candidates: 5,
        pair_input_tokens: 128,
        max_input_tokens: 640,
    };
    let narrow = ReadAttemptBudget {
        remaining_ns: 250_000_000,
        ..attempt
    };
    assert_eq!(
        ordinary_search_allocation(narrow, ReadClock::UnixMonotonic, 100, 8, 10, &limits, 2000)?,
        Some(Budget {
            calls: 1,
            rounds: 1,
            fanout: 1,
            input_tokens: 640,
            candidates: 5,
            max_bytes: 2000,
            deadline_ms: 250
        })
    );
    assert!(
        ordinary_search_allocation(
            attempt,
            ReadClock::UnixMonotonic,
            100,
            0,
            10,
            &limits,
            65536
        )?
        .is_none()
    );
    assert!(
        ordinary_search_allocation(attempt, ReadClock::UnixMonotonic, 100, 8, 10, &limits, 1023)?
            .is_none()
    );
    let submillisecond = ReadAttemptBudget {
        remaining_ns: 999_999,
        ..attempt
    };
    assert!(
        ordinary_search_allocation(
            submillisecond,
            ReadClock::UnixMonotonic,
            100,
            8,
            10,
            &limits,
            65536
        )?
        .is_none()
    );
    assert!(
        ordinary_search_allocation(
            attempt,
            ReadClock::UnixMonotonic,
            30_000_000_100,
            8,
            10,
            &limits,
            65536
        )?
        .is_none()
    );
    assert!(
        ordinary_search_allocation(
            attempt,
            ReadClock::WindowsPerformanceCounter,
            100,
            8,
            10,
            &limits,
            65536
        )
        .is_err()
    );
    assert!(
        ordinary_search_allocation(
            attempt,
            ReadClock::UnixMonotonic,
            100,
            11,
            10,
            &limits,
            65536
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn focus_application_ready_requires_native_profile_caps_and_other_states_grant_none() -> Result<()>
{
    let base = IndexApplication {
        scope: scope(),
        profile: Some(digest(7)),
        availability: Availability::Ready,
        reason: "Synthetic actual-native binding".into(),
    };
    let mut app = FocusApplication {
        application: base,
        identity: ranking()?.identity,
        limits: Some(FocusProfileLimits {
            max_candidates: 5,
            pair_input_tokens: 128,
            max_input_tokens: 640,
        }),
    };
    app.validate()?;
    let original_identity = app.identity.clone();
    app.identity.as_mut().unwrap().profile = digest(99);
    assert!(app.validate().is_err());
    app.identity = original_identity;
    app.limits = None;
    assert!(app.validate().is_err());
    app.application.availability = Availability::Constrained;
    app.identity = None;
    app.validate()?;
    app.limits = Some(FocusProfileLimits {
        max_candidates: 5,
        pair_input_tokens: 2048,
        max_input_tokens: 640,
    });
    assert!(app.validate().is_err());
    Ok(())
}
#[test]
fn absent_focus_preserves_existing_search_serialized_bytes() -> Result<()> {
    let response = crate::SearchResponse {
        focus: None,
        execution: None,
        publication: None,
        semantic_readiness: None,
        discovery_readiness: None,
        generation: 1,
        collection: "fixture".into(),
        hits: vec![],
        truncated: false,
    };
    assert_eq!(
        serde_json::to_string(&response)?,
        r#"{"generation":1,"collection":"fixture","hits":[],"truncated":false}"#
    );
    Ok(())
}
#[test]
fn compact_focus_notice_is_known_predispatch_and_never_consumes_native_custody() -> Result<()> {
    let observation = FocusObservation::not_applied_if_fits(
        Availability::Failed,
        "Original clock unavailable".into(),
        4,
        None,
        512,
    )?
    .unwrap();
    observation.validate(&Budget::default())?;
    assert!(observation.receipt().is_none());
    assert!(
        FocusObservation::not_applied_if_fits(
            Availability::Failed,
            "No metadata headroom".into(),
            4,
            None,
            0
        )?
        .is_none()
    );
    assert!(
        FocusObservation::not_applied_if_fits(Availability::Disabled, "off".into(), 4, None, 512)
            .is_err()
    );
    let mut raw = serde_json::to_value(&observation)?;
    raw["work_unknown"] = true.into();
    let forged: FocusObservation = serde_json::from_value(raw)?;
    assert!(forged.validate(&Budget::default()).is_err());
    Ok(())
}
pub(crate) fn search_record(record_scope: ScopeBinding) -> Result<EvidenceRecord> {
    let mut receipt = ranking()?;
    receipt.scope = record_scope.clone();
    let hit = crate::Hit {
        citation: crate::Citation {
            path: "policy.md".into(),
            content_hash: digest(12).to_string(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
        },
        text: "abcdefghij".into(),
        truncated: false,
    };
    let response = crate::SearchResponse {
        focus: Some(FocusObservation::Receipt {
            receipt: Box::new(receipt),
        }),
        execution: None,
        publication: Some(record_scope.publication.clone()),
        semantic_readiness: None,
        discovery_readiness: None,
        generation: record_scope.publication.generation,
        collection: record_scope.publication.collection.clone(),
        hits: vec![hit; 4],
        truncated: false,
    };
    Ok(EvidenceRecord {
        id: digest(55),
        scope: record_scope,
        operation: NativeRead::Search,
        arguments: BTreeMap::from([("query".into(), json!("rules"))]),
        role: EvidenceRole::Unknown,
        canonical_result: serde_json::to_value(response)?,
    })
}
#[test]
fn observed_focus_work_uses_only_typed_search_root_and_retains_unknown_actual_cost() -> Result<()> {
    let original = search_record(scope())?;
    let summary = observed_focus_work(std::slice::from_ref(&original), &Budget::default())?;
    assert_eq!(summary.work.input_tokens, 90);
    assert_eq!(summary.work.inference_us, Some(2));
    assert_eq!(summary.work.candidates, 3);
    assert!(!summary.work_unknown);
    let mut wrong = original.clone();
    wrong.operation = NativeRead::Context;
    assert_eq!(
        observed_focus_work(&[wrong], &Budget::default())?
            .work
            .input_tokens,
        0
    );
    let mut duplicate = original.clone();
    duplicate.id = digest(56);
    assert!(observed_focus_work(&[original.clone(), duplicate], &Budget::default()).is_err());
    let mut unknown = original;
    {
        let raw = &mut unknown.canonical_result["focus"]["receipt"];
        raw["applied"] = false.into();
        raw["availability"] = "failed".into();
        raw["ranked_order"] = raw["original_order"].clone();
        raw["observed_input_tokens"] = serde_json::Value::Null;
        raw["work"]["input_tokens"] = 0.into();
    }
    assert!(observed_focus_work(std::slice::from_ref(&unknown), &Budget::default())?.work_unknown);
    let raw = &mut unknown.canonical_result["focus"]["receipt"];
    raw["observed_input_tokens"] = 101.into();
    raw["work"]["input_tokens"] = 101.into();
    assert!(observed_focus_work(&[unknown], &Budget::default())?.failed_token_overrun);
    Ok(())
}

#[test]
fn nested_focus_allocation_is_cumulative_local_and_leaves_context_capacity() -> Result<()> {
    let mut limits = FocusProfileLimits {
        max_candidates: 8,
        pair_input_tokens: 512,
        max_input_tokens: 2048,
    };
    assert_eq!(focus_token_allowance(2048, true, 0, 2048, &limits)?, 1024);
    assert_eq!(focus_token_allowance(2048, true, 512, 1536, &limits)?, 512);
    assert_eq!(focus_token_allowance(2048, true, 1024, 1024, &limits)?, 0);
    assert_eq!(
        focus_token_allowance(2048, true, usize::MAX, 2048, &limits)?,
        0
    );
    assert_eq!(focus_token_allowance(2048, false, 0, 2048, &limits)?, 2048);
    assert_eq!(focus_token_allowance(1024, true, 0, 2048, &limits)?, 512);
    assert_eq!(focus_token_allowance(2048, true, 0, 700, &limits)?, 512);
    assert_eq!(focus_token_allowance(1023, true, 0, 1023, &limits)?, 0);
    assert_eq!(focus_token_allowance(2048, true, 0, 0, &limits)?, 0);
    limits.max_input_tokens = 512;
    assert_eq!(focus_token_allowance(2048, false, 0, 2048, &limits)?, 512);
    assert!(focus_token_allowance(0, true, 0, 2048, &limits).is_err());
    assert!(focus_token_allowance(2049, true, 0, 2048, &limits).is_err());
    limits.pair_input_tokens = 0;
    assert!(focus_token_allowance(2048, true, 0, 2048, &limits).is_err());
    Ok(())
}
