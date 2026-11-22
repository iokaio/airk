// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::combined_context::{
    CombinedContextSummary, PartitionOutcome, PartitionReport, Resources, SUMMARY_SCHEMA,
    Termination, initial_allocations,
};

/// The Answer /2 summary of a composed result.
fn v1(result: &CombinedReadResult) -> &CombinedContextSummary {
    match result.context.as_ref() {
        Some(airk_core::combined_context::CombinedContext::Context(summary)) => summary,
        _ => panic!("an Answer /2 summary"),
    }
}
fn replace_scope(value: &mut Value, scope: &Value) {
    if value.get("authorization").is_some()
        && value.get("index_id").is_some()
        && value.get("publication").is_some()
    {
        *value = scope.clone();
        return;
    }
    match value {
        Value::Object(fields) => {
            for item in fields.values_mut() {
                replace_scope(item, scope);
            }
        }
        Value::Array(items) => {
            for item in items {
                replace_scope(item, scope);
            }
        }
        _ => {}
    }
}
fn packet(i: u8) -> Result<Value> {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    let mut value = cases["cases"][0]["input"].clone();
    let mut scope = value["assessment"]["scope"].clone();
    scope["index_id"] = json!(index(i));
    scope["publication"] = serde_json::to_value(revision(u64::from(i)))?;
    replace_scope(&mut value, &scope);
    value["records"][0]["canonical_result"]["hits"][0]["citation"]["path"] =
        json!(format!("api/member-{i}.rs"));
    if i == 2 {
        value["assessment_history"][0]["ordinal"] = json!(3);
        value["assessment"]["coverage"]["ordinal"] = json!(3);
    }
    value["future_public_data"] = json!({"kept":[1,"exact"],"nothing_dropped":true});
    Ok(value)
}
fn context_fixture() -> Result<(CompositionRequest, Vec<MemberInput>, CombinedContextSummary)> {
    // Retained semantic cases keep their original allocation and serialized bytes.
    context_fixture_with_budget(airk_core::arbiter::Budget {
        calls: 8,
        rounds: 3,
        fanout: 8,
        input_tokens: 2048,
        candidates: 32,
        max_bytes: 262144,
        deadline_ms: 30_000,
    })
}
fn context_fixture_with_budget(
    budget: airk_core::arbiter::Budget,
) -> Result<(CompositionRequest, Vec<MemberInput>, CombinedContextSummary)> {
    let mut composition = request()?;
    composition.budgets.limit = 100;
    composition.budgets.max_bytes = 65536;
    let ids = vec![index(1), index(2)];
    let allocations = initial_allocations(
        Incarnation::from_bytes([9; 16]),
        composition.authority,
        &ids,
        &budget,
    )?;
    let mut reports = Vec::new();
    let mut inputs = Vec::new();
    let mut total = Resources::default();
    let mut bindings = Vec::new();
    for (offset, allowance) in allocations.into_iter().enumerate() {
        let i = u8::try_from(offset + 1)?;
        let body = packet(i)?;
        let typed =
            airk_core::arbiter::context_action::ContextResponseV2::from_canonical(&body, &budget)?;
        let used = Resources::from_packet(&typed)?;
        if let Some(binding) =
            airk_core::combined_context::ContextDeliveryBinding::from_packet(&typed)?
        {
            bindings.push(binding);
        }
        total = total.checked_add(&used)?;
        reports.push(PartitionReport {
            index_id: index(i),
            allowance,
            outcome: PartitionOutcome::Known {
                used,
                assessment_ordinals: vec![if i == 1 { 1 } else { 3 }],
                read_depth: 0,
                peak_fanout: 0,
            },
        });
        inputs.push(MemberInput {
            index_id: index(i),
            is_error: false,
            exhausted: false,
            result: body,
            sources: sources()?,
            elapsed_ms: Some(1),
        });
    }
    Ok((
        composition,
        inputs,
        CombinedContextSummary {
            schema: SUMMARY_SCHEMA.into(),
            attempt: Incarnation::from_bytes([9; 16]),
            selection: Sha256Digest::from_bytes([7; 32]),
            budget,
            members: reports,
            known_used: total,
            unknown_held: Resources::default(),
            termination: Termination::Complete,
            delivery_bindings: bindings,
        },
    ))
}

#[test]
fn combined_answer_preserves_both_complete_packets_publications_and_nested_ranking() -> Result<()> {
    let (request, inputs, summary) = context_fixture()?;
    let originals: Vec<_> = inputs.iter().map(|input| input.result.clone()).collect();
    let result = compose_context(&request, inputs, summary)?;
    assert_eq!(result.members.len(), 2);
    assert_eq!(result.ranking.len(), 2);
    assert_eq!(result.publication_vector.members.len(), 2);
    for (offset, original) in originals.iter().enumerate() {
        assert_eq!(
            member_result(&result, index(u8::try_from(offset + 1)?))?,
            *original
        );
        assert_eq!(result.members[offset].outcome, MemberOutcome::Complete);
        assert!(result.ranking.iter().any(|rank| {
            rank.index_id == index(u8::try_from(offset + 1).unwrap())
                && rank
                    .result_pointer
                    .contains("/records/0/canonical_result/hits/0")
        }));
    }
    assert_eq!(v1(&result).known_used.calls, 2);
    assert_eq!(
        v1(&result).known_used.input_tokens,
        200
    );
    assert!(result.read_retry.is_none());
    assert!(!result.partial);
    Ok(())
}

#[test]
fn combined_answer_local_covered_or_clarify_never_removes_other_member() -> Result<()> {
    for local in ["covered", "clarification_required"] {
        let (request, mut inputs, mut summary) = context_fixture()?;
        inputs[0].result["stop"] = json!(local);
        inputs[0].result["assessment"]["termination"] = json!(local);
        if local == "clarification_required" {
            let card = &mut inputs[0].result["assessment_history"][0]["result"]["card"];
            card["action"] = json!({"kind":"clarify"});
            card["gap"] = json!("ambiguous");
            card["reason"] = json!("ambiguous_question");
            inputs[0].result["gaps"] = json!(["ambiguous"]);
            let typed = airk_core::arbiter::context_action::ContextResponseV2::from_canonical(
                &inputs[0].result,
                &summary.budget,
            )?;
            if let PartitionOutcome::Known { used, .. } = &mut summary.members[0].outcome {
                let next = Resources::from_packet(&typed)?;
                summary.known_used.bytes = summary.known_used.bytes - used.bytes + next.bytes;
                *used = next;
            }
        }
        let second = inputs[1].result.clone();
        let result = compose_context(&request, inputs, summary)?;
        assert_eq!(member_result(&result, index(2))?, second);
        assert_eq!(result.members.len(), 2);
        assert_eq!(result.ranking.len(), 2);
    }
    Ok(())
}

#[test]
fn combined_answer_foreign_record_publication_or_scope_refuses() -> Result<()> {
    let (request, mut inputs, summary) = context_fixture()?;
    inputs[1].result["records"][0]["scope"]["index_id"] = json!(index(1));
    assert!(compose_context(&request, inputs, summary).is_err());
    let (request, mut inputs, summary) = context_fixture()?;
    inputs[1].result["records"][0]["scope"]["publication"] = serde_json::to_value(revision(99))?;
    assert!(compose_context(&request, inputs, summary).is_err());
    Ok(())
}

#[test]
fn emit_combined_context_semantic_cases() -> Result<()> {
    use airk_core::combined_context::UnqueriedReason;
    let mut cases = Vec::new();
    let (request, inputs, summary) = context_fixture()?;
    let originals: Vec<_> = inputs.iter().map(|input| input.result.clone()).collect();
    let complete = compose_context(&request, inputs, summary)?;
    for (offset, original) in originals.iter().enumerate() {
        assert_eq!(
            member_result(&complete, index(u8::try_from(offset + 1)?))?,
            *original
        );
    }
    cases.push(json!({"name":"two_complete_members","request":request,"native_member_results":originals,"output":complete}));

    let (request, mut inputs, mut summary) = context_fixture()?;
    let failed = json!({"isError":true,"content":[{"type":"text","text":"native member unavailable"}],"structuredContent":{"error":{"code":"helper_unavailable","retry":{"fresh_brokerage_required":true,"query_replay_safe":false},"custom_nested_detail":[1,2,3]},"publication":revision(2)}});
    inputs[1].is_error = true;
    inputs[1].result = failed.clone();
    summary.known_used = match &summary.members[0].outcome {
        PartitionOutcome::Known { used, .. } => used.clone(),
        _ => anyhow::bail!("complete first fixture member required"),
    };
    summary.unknown_held = summary.members[1]
        .allowance
        .as_ref()
        .context("second allocation")?
        .resources
        .clone();
    summary.members[1].outcome = PartitionOutcome::Unknown;
    summary.termination = Termination::Partial;
    summary
        .delivery_bindings
        .retain(|binding| binding.scope.index_id == index(1));
    let failed_result = compose_context(&request, inputs, summary)?;
    assert_eq!(member_result(&failed_result, index(2))?, failed);
    assert_eq!(failed_result.members[1].outcome, MemberOutcome::Failed);
    assert!(failed_result.partial);
    cases.push(json!({"name":"complete_and_failed_member","request":request,"native_failure":failed,"output":failed_result}));

    let (request, mut inputs, mut summary) = context_fixture()?;
    inputs.truncate(1);
    summary.known_used = match &summary.members[0].outcome {
        PartitionOutcome::Known { used, .. } => used.clone(),
        _ => anyhow::bail!("complete first fixture member required"),
    };
    summary.members[1].allowance = None;
    summary.members[1].outcome = PartitionOutcome::Unqueried {
        reason: UnqueriedReason::Budget,
    };
    summary.termination = Termination::Budget;
    summary
        .delivery_bindings
        .retain(|binding| binding.scope.index_id == index(1));
    let unqueried = compose_context(&request, inputs, summary)?;
    assert_eq!(unqueried.members[1].outcome, MemberOutcome::Unqueried);
    assert!(unqueried.members[1].result.is_null());
    assert!(unqueried.partial);
    cases
        .push(json!({"name":"complete_and_unqueried_member","request":request,"output":unqueried}));

    let document = json!({"schema":"airk.arbiter.combined-context.cases/1","cases":cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/arbiter/combined_answer_cases.json");
    if std::env::var_os("AIRK_WRITE_COMBINED_CONTEXT_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&document)?),
        )?;
    }
    let retained: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    assert_eq!(retained, document, "combined native semantic fixture drift");
    Ok(())
}

#[test]
fn combined_answer_counts_materialized_multi_hit_envelopes_without_changing_native_limits()
-> Result<()> {
    for representation in ["native", "text", "structured", "both"] {
        let (request, mut inputs, mut summary) = context_fixture()?;
        let mut originals = Vec::new();
        for (input, report) in inputs.iter_mut().zip(&mut summary.members) {
            let extra = input.result["records"][0]["canonical_result"]["hits"][0].clone();
            input.result["records"][0]["canonical_result"]["hits"]
                .as_array_mut()
                .unwrap()
                .push(extra);
            let typed = airk_core::arbiter::context_action::ContextResponseV2::from_canonical(
                &input.result,
                &summary.budget,
            )?;
            if let PartitionOutcome::Known { used, .. } = &mut report.outcome {
                let next = Resources::from_packet(&typed)?;
                summary.known_used.bytes = summary.known_used.bytes - used.bytes + next.bytes;
                *used = next;
            }
            let canonical = input.result.clone();
            originals.push(canonical.clone());
            input.result = match representation {
                "text" => {
                    json!({"isError":false,"content":[{"type":"text","text":canonical.to_string()}]})
                }
                "structured" => json!({"isError":false,"structuredContent":canonical}),
                "both" => {
                    json!({"isError":false,"content":[{"type":"text","text":canonical.to_string()}],"structuredContent":canonical})
                }
                _ => canonical,
            };
        }
        let original_budget = request.budgets;
        let expected_summary = serde_json::to_value(&summary)?;
        let result = compose_context(&request, inputs, summary)?;
        assert_eq!(result.ranking.len(), 4, "{representation}");
        assert_eq!(
            serde_json::to_value(v1(&result))?,
            expected_summary
        );
        assert_eq!(request.budgets, original_budget);
        for (offset, original) in originals.iter().enumerate() {
            let retained = member_result(&result, index(u8::try_from(offset + 1)?))?;
            assert_eq!(body(&retained).0, original, "{representation}");
            assert_eq!(result.members[offset].allocation.limit, 2);
            assert_eq!(
                result.members[offset].allocation.max_bytes,
                summary_bytes(&expected_summary, offset)?
            );
        }
    }
    Ok(())
}
fn summary_bytes(summary: &Value, offset: usize) -> Result<usize> {
    Ok(usize::try_from(
        summary["members"][offset]["allowance"]["resources"]["bytes"]
            .as_u64()
            .context("fixture byte allowance")?,
    )?)
}
#[test]
fn ordinary_multi_hit_count_refusals_are_identical_for_text_and_structured_envelopes() -> Result<()>
{
    for representation in ["text", "structured"] {
        let request = request()?;
        let mut member = input(
            1,
            vec![
                hit("api/a.rs", "a"),
                hit("api/b.rs", "b"),
                hit("api/c.rs", "c"),
            ],
        )?;
        let canonical = member.result.clone();
        member.result = if representation == "text" {
            json!({"isError":false,"content":[{"type":"text","text":canonical.to_string()}]})
        } else {
            json!({"isError":false,"structuredContent":canonical})
        };
        let error = compose(&request, vec![member]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("member result exceeded its count allocation"),
            "{error:#}"
        );
    }
    Ok(())
}
/// Combined Answer /3 admission sizes the widest `airk.combined-context/2`
/// summary inside the 131,072-byte Answer frame, never the 65,536 bytes of
/// other tools.
#[test]
fn combined_answer_v3_admission_takes_the_answer_frame() -> Result<()> {
    use airk_core::arbiter::answer_loop::{BudgetV3, MAX_ANSWER_RESPONSE_BYTES};
    let (request, _, summary) = context_fixture()?;
    let v2: Vec<_> = summary
        .members
        .iter()
        .map(|m| m.allowance.clone())
        .collect();
    let ids: Vec<_> = summary.members.iter().map(|m| m.index_id).collect();
    let budget = BudgetV3::default();
    let allowances = airk_core::combined_context::initial_allocations_v3(
        summary.attempt,
        request.authority,
        &ids,
        &budget,
    )?;
    let answer = answer_payload_admission(&request, &budget, &allowances, 1024)?;
    let context = context_payload_admission(&request, &summary.budget, &v2, 1024)?;
    assert!(answer > context, "{answer} > {context}");
    assert!(answer < MAX_ANSWER_RESPONSE_BYTES / 3);
    assert!(answer_payload_admission(&request, &budget, &allowances, usize::MAX).is_err());
    let tiny = BudgetV3 {
        max_bytes: 4096,
        ..budget.clone()
    };
    assert_eq!(
        answer_payload_admission(&request, &tiny, &allowances, 1024)?,
        0
    );
    assert!(answer_payload_admission(&request, &budget, &allowances[..1], 1024).is_err());
    Ok(())
}

#[test]
fn context_byte_admission_charges_closed_envelope_escaping_and_original_metadata() -> Result<()> {
    let (request, inputs, summary) = context_fixture()?;
    let allowances: Vec<_> = summary
        .members
        .iter()
        .map(|m| m.allowance.clone())
        .collect();
    let payload = context_payload_admission(&request, &summary.budget, &allowances, 1024)?;
    assert!(payload >= 2048 && payload < request.budgets.max_bytes / 3);
    assert!(context_payload_admission(&request, &summary.budget, &allowances, usize::MAX).is_err());
    let mut tiny = summary.budget.clone();
    tiny.max_bytes = 1024;
    assert_eq!(
        context_payload_admission(&request, &tiny, &allowances, 1024)?,
        0
    );
    for text in [
        "plain".to_string(),
        "\"\\\n\t\u{0001}".repeat(300),
        "caf\u{e9}\u{6771}\u{4eac}".repeat(100),
    ] {
        let canonical = json!({"native_text":text});
        let envelope = json!({"content":[{"type":"text","text":canonical.to_string()}],"structuredContent":canonical,"isError":false});
        assert_eq!(
            context_member_payload_bytes(&envelope)?,
            serde_json::to_vec(&canonical)?.len()
        );
        let mut text_only = envelope.clone();
        text_only
            .as_object_mut()
            .unwrap()
            .remove("structuredContent");
        assert_eq!(
            context_member_payload_bytes(&text_only)?,
            serde_json::to_vec(&canonical)?.len()
        );
        let mut materialized = text_only.clone();
        canonicalize_envelope(&mut materialized)?;
        assert_eq!(
            context_member_payload_bytes(&text_only)?,
            context_member_payload_bytes(&materialized)?
        );
        let mut structured_only = envelope.clone();
        structured_only.as_object_mut().unwrap().remove("content");
        assert_eq!(
            context_member_payload_bytes(&structured_only)?,
            serde_json::to_vec(&canonical)?.len()
        );
        let mut extended = envelope.clone();
        extended["future_envelope"] = json!({"must_not_disappear":true});
        assert!(context_member_payload_bytes(&extended).is_err());
        let mut divergent = envelope.clone();
        divergent["content"][0]["text"] = json!("{}");
        assert!(context_member_payload_bytes(&divergent).is_err());
    }
    let opaque = json!({"isError":true,"content":[{"type":"text","text":"native failure is opaque; do not reinterpret"}]});
    assert_eq!(
        context_member_payload_bytes(&opaque)?,
        serde_json::to_vec(&opaque)?.len()
    );
    let mut structured_failure = opaque.clone();
    structured_failure["structuredContent"] =
        json!({"error":{"code":"helper_unavailable","full_native_detail":[1,2,3]}});
    assert_eq!(
        context_member_payload_bytes(&structured_failure)?,
        serde_json::to_vec(&structured_failure)?.len()
    );
    let mut contradictory = structured_failure.clone();
    contradictory["content"][0]["text"] = json!("{\"error\":{\"code\":\"different\"}}");
    assert!(context_member_payload_bytes(&contradictory).is_err());
    let mut nominal = opaque.clone();
    nominal["isError"] = json!(false);
    assert!(context_member_payload_bytes(&nominal).is_err());
    let complete = compose_context(&request, inputs.clone(), summary.clone())?;
    let mut without = complete.clone();
    without.ranking.clear();
    without.truncated = true;
    let lower = serde_json::to_vec(&without)?.len();
    let mut bounded = request.clone();
    bounded.budgets.max_bytes = lower + 1;
    let fitted = compose_context(&bounded, inputs.clone(), summary.clone())?;
    assert!(fitted.truncated && fitted.ranking.len() < complete.ranking.len());
    for offset in 0..2 {
        assert_eq!(
            member_result(&fitted, index(offset + 1))?,
            member_result(&complete, index(offset + 1))?
        );
    }
    bounded.budgets.max_bytes = lower - 1;
    assert!(
        compose_context(&bounded, inputs, summary)
            .unwrap_err()
            .is::<ContextReplyBudget>()
    );
    Ok(())
}

#[test]
fn context_stage_binding_sizing_preserves_baseline_and_reserves_both_stages_at_byte_boundary()
-> Result<()> {
    use airk_core::arbiter::Stage;
    let (request, _, summary) = context_fixture()?;
    let allowances = summary
        .members
        .iter()
        .map(|member| member.allowance.clone())
        .collect::<Vec<_>>();
    let mut budget = summary.budget;
    let attachment = 1024;
    let baseline = context_admission_skeleton(&request, &budget, &allowances, false)?;
    let dual = context_admission_skeleton(&request, &budget, &allowances, true)?;
    let baseline_bindings = &v1(&baseline).delivery_bindings;
    let dual_bindings = &v1(&dual).delivery_bindings;
    assert_eq!(
        baseline_bindings
            .iter()
            .map(|binding| (binding.scope.index_id, binding.stage))
            .collect::<Vec<_>>(),
        vec![(index(1), Stage::Context), (index(2), Stage::Context)]
    );
    assert_eq!(
        dual_bindings
            .iter()
            .map(|binding| (binding.scope.index_id, binding.stage))
            .collect::<Vec<_>>(),
        vec![
            (index(1), Stage::Context),
            (index(1), Stage::Focus),
            (index(2), Stage::Context),
            (index(2), Stage::Focus)
        ]
    );
    let mut expected_extra = 0;
    for binding in baseline_bindings {
        // Independent wire value: default Context omits stage; Focus must add it.
        let row = json!({"stage":"focus","scope":binding.scope,"profile":binding.profile});
        expected_extra += serde_json::to_vec(&row)?.len() + 1;
    }
    assert_eq!(
        serde_json::to_vec(&dual)?.len() - serde_json::to_vec(&baseline)?.len(),
        expected_extra
    );
    // Account for the budget field's own decimal width at the boundary.
    for _ in 0..4 {
        budget.max_bytes = serde_json::to_vec(&context_admission_skeleton(
            &request,
            &budget,
            &allowances,
            false,
        )?)?
        .len()
            + context_delivery_receipt_reserve()?
            + attachment
            + 3 * 1024;
    }
    assert_eq!(
        context_payload_admission(&request, &budget, &allowances, attachment)?,
        1024
    );
    assert!(
        context_payload_admission_with_focus(&request, &budget, &allowances, attachment)? < 1024
    );
    budget.max_bytes += expected_extra;
    assert_eq!(
        context_payload_admission_with_focus(&request, &budget, &allowances, attachment)?,
        1024
    );
    budget.max_bytes -= 1;
    assert_eq!(
        context_payload_admission_with_focus(&request, &budget, &allowances, attachment)?,
        1023
    );
    Ok(())
}

#[test]
fn context_worst_roster_sizes_sixty_four_inert_stage_bindings() -> Result<()> {
    use airk_core::arbiter::Stage;
    let (mut request, _, summary) = context_fixture()?;
    let template = request.selection.members[0].clone();
    request.selection.members = (1..=32)
        .map(|number| {
            let mut member = template.clone();
            member.index_id = index(number);
            for membership in &mut member.memberships {
                membership.membership.index_id = member.index_id;
            }
            member
        })
        .collect();
    let allowances = vec![None; 32];
    let sizing = context_admission_skeleton(&request, &summary.budget, &allowances, true)?;
    let bindings = &v1(&sizing).delivery_bindings;
    assert_eq!(bindings.len(), 64);
    for number in 1..=32 {
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.scope.index_id == index(number))
                .map(|binding| binding.stage)
                .collect::<Vec<_>>(),
            vec![Stage::Context, Stage::Focus]
        );
    }
    assert_eq!(
        context_payload_admission_with_focus(&request, &summary.budget, &allowances, 0)?,
        0
    );
    Ok(())
}

#[test]
fn actual_focus_outer_binding_is_not_reclaimed_at_exact_child_boundary() -> Result<()> {
    let (_, _, summary) = context_fixture()?;
    let scope = &summary.delivery_bindings[0].scope;
    let profile = summary.delivery_bindings[0].profile;
    let expected =
        serde_json::to_vec(&json!({"stage":"focus","scope":scope,"profile":profile}))?.len() + 1;
    assert_eq!(context_focus_binding_bytes(scope, profile)?, expected);
    let body = 1024;
    assert_eq!(
        context_reclaimable_body_bytes(body, 0, body, 0)?,
        0,
        "both-off keeps its fitting original body"
    );
    assert!(context_reclaimable_body_bytes(body + expected - 1, 0, body, expected).is_err());
    assert_eq!(
        context_reclaimable_body_bytes(body + expected, 0, body, expected)?,
        0
    );
    assert_eq!(
        context_reclaimable_body_bytes(body + expected + 1, 0, body, expected)?,
        1
    );
    Ok(())
}

#[test]
fn current_combined_context_retains_full_8192_token_120_second_parent() -> Result<()> {
    let (request, inputs, summary) =
        context_fixture_with_budget(airk_core::arbiter::Budget::default())?;
    assert_eq!(summary.budget.input_tokens, 8192);
    assert_eq!(summary.budget.deadline_ms, 120_000);
    assert_eq!(
        summary
            .members
            .iter()
            .map(|member| member.allowance.as_ref().unwrap().resources.input_tokens)
            .sum::<usize>(),
        8192
    );
    for member in &summary.members {
        let allowance = member.allowance.as_ref().unwrap();
        assert_eq!(allowance.resources.input_tokens, 4096);
        assert_eq!(allowance.parent_budget.deadline_ms, 120_000);
    }
    let result = compose_context(&request, inputs, summary)?;
    let context = v1(&result);
    assert_eq!(context.budget.input_tokens, 8192);
    assert_eq!(context.budget.deadline_ms, 120_000);
    assert_eq!(context.known_used.input_tokens, 200);
    Ok(())
}
