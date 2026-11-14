// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::combined_context::{
    CombinedContextSummary, PartitionOutcome, PartitionReport, Resources, SUMMARY_SCHEMA,
    Termination, initial_allocations,
};

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
    let mut composition = request()?;
    composition.budgets.limit = 100;
    composition.budgets.max_bytes = 65536;
    let budget = airk_core::arbiter::Budget::default();
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
    assert_eq!(result.context.as_ref().unwrap().known_used.calls, 2);
    assert_eq!(
        result.context.as_ref().unwrap().known_used.input_tokens,
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
