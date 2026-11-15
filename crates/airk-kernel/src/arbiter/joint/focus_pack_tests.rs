// SPDX-License-Identifier: Apache-2.0
//! Actual model receipts remain mandatory even when presentation is omitted.
use super::*;
use airk_core::arbiter::{
    FocusApplicationReceipt, FocusIdentity, FocusObservation, FocusPresentation, FocusScore,
};
use airk_core::native_focus::{FocusSourceWindow, NativeFocusCandidate};

fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}
fn packet(unknown: bool) -> Result<ContextResponseV2> {
    let raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    let sample: ContextResponseV2 = serde_json::from_value(raw["cases"][0]["input"].clone())?;
    let scope = sample.assessment.scope;
    let hit = airk_core::Hit {
        citation: airk_core::Citation {
            path: "public/rule.md".into(),
            content_hash: digest(90).to_string(),
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
        },
        text: "The original request retains its deadline.".into(),
        truncated: false,
    };
    let candidate = NativeFocusCandidate::new(
        &scope,
        "deadline",
        0,
        hit.clone(),
        FocusSourceWindow {
            utf8_start: 0,
            utf8_end: hit.text.len(),
        },
    )?;
    let identity = FocusIdentity {
        profile: digest(91),
        runtime: RUNTIME.into(),
        compiler: digest(92),
        calibration: digest(93),
        task_policy: digest(94),
    };
    let receipt = FocusApplicationReceipt {
        schema: airk_core::arbiter::focus::RANKING_SCHEMA.into(),
        scope: scope.clone(),
        identity: Some(identity),
        availability: if unknown {
            Availability::Failed
        } else {
            Availability::Ready
        },
        reason: "Synthetic actual receipt for custody preservation".into(),
        applied: !unknown,
        input_digest: digest(95),
        original_order: vec![candidate.id],
        ranked_order: vec![candidate.id],
        scores: if unknown {
            vec![]
        } else {
            vec![FocusScore::from_logits(candidate.id, 0., 1.)?]
        },
        presentations: if unknown {
            vec![]
        } else {
            vec![FocusPresentation {
                candidate: candidate.id,
                utf8_start: 0,
                utf8_end: hit.text.len(),
                source_bytes: hit.text.len(),
            }]
        },
        presented_count: usize::from(!unknown),
        candidate_count: 1,
        scored_count: usize::from(!unknown),
        omitted_count: usize::from(unknown),
        reserved_input_tokens: 32,
        observed_input_tokens: (!unknown).then_some(16),
        work: if unknown {
            Work::default()
        } else {
            Work {
                input_tokens: 16,
                candidates: 1,
                compile_us: Some(1),
                inference_us: Some(2),
                ..Work::default()
            }
        },
    };
    receipt.validate(&Budget::default())?;
    let body = json!({"publication":scope.publication,"collection":scope.publication.collection,"generation":scope.publication.generation,
        "hits":[hit],"truncated":false,"focus":FocusObservation::Receipt{receipt:Box::new(receipt)}});
    let mut first = sample.records[0].clone();
    first.id = digest(80);
    first.scope = scope.clone();
    first.operation = NativeRead::Search;
    first.canonical_result = json!({"publication":scope.publication,"collection":scope.publication.collection,"generation":scope.publication.generation,
        "hits":[],"truncated":false});
    let mut large = first.clone();
    large.id = digest(81);
    large.operation = NativeRead::Context;
    large.canonical_result["large_future_metadata"] = json!("x".repeat(40_000));
    let focused = EvidenceRecord {
        id: digest(82),
        scope: scope.clone(),
        operation: NativeRead::Search,
        arguments: std::collections::BTreeMap::from([("query".into(), json!("deadline"))]),
        role: EvidenceRole::Unknown,
        canonical_result: body,
    };
    let records = vec![first, large, focused];
    let work = airk_core::arbiter::focus::observed_focus_work(&records, &Budget::default())?.work;
    Ok(ContextResponseV2 {
        schema: CONTEXT_SCHEMA_V2.into(),
        packet_schema: PACKET_SCHEMA.into(),
        question: "deadline".into(),
        records,
        paragraphs: vec![],
        gaps: vec![Gap::Unsupported],
        stop: PartitionTermination::NoProgress,
        work: Work {
            calls: 3,
            rounds: 1,
            candidates: 3,
            bytes: 45_000,
            ..work
        },
        candidate_count: 3,
        omitted_count: 0,
        assessment_history: vec![],
        assessment: PartitionAssessment {
            scope: scope.clone(),
            coverage: PartitionCoverage::Unassessed {
                reason: AssessmentUnavailable::TaskUnsupported,
            },
            termination: PartitionTermination::NoProgress,
        },
        applications: vec![IndexApplication {
            scope,
            profile: None,
            availability: Availability::Disabled,
            reason: "Context independently disabled".into(),
        }],
    })
}

#[test]
fn packet_budget_omits_uncharged_record_before_later_known_or_unknown_focus_custody() -> Result<()>
{
    for unknown in [false, true] {
        let mut response = packet(unknown)?;
        let retained = response.records[2].canonical_result.clone();
        pack(&mut response, None, 32_768, false, 4096)?;
        assert_eq!(
            response
                .records
                .iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            vec![digest(80), digest(82)]
        );
        assert_eq!(response.records[1].canonical_result, retained);
        assert_eq!(response.omitted_count, 1);
        let bindings = airk_core::combined_context::packet_delivery_bindings(&response)?;
        assert!(bindings.iter().any(|binding|binding.stage==Stage::Focus&&binding.profile==Some(digest(91))));
        response.validate(&Budget::default())?;
    }
    Ok(())
}
#[test]
fn packet_budget_trims_presentation_before_removing_any_charged_focus_record() -> Result<()> {
    let mut response = packet(false)?;
    response.records.remove(1);
    response.candidate_count = 2;
    response.paragraphs.push(GroundedParagraph {
        text: "explanation ".repeat(4000),
        evidence: vec![digest(82)],
        interpretation: true,
    });
    let records = serde_json::to_vec(&response.records)?;
    pack(&mut response, None, 32_768, false, 4096)?;
    assert_eq!(serde_json::to_vec(&response.records)?, records);
    assert!(response.paragraphs.is_empty());
    assert_eq!(response.stop, PartitionTermination::Budget);
    assert_eq!(response.omitted_count, 0);
    Ok(())
}
#[test]
fn insufficient_mandatory_receipt_budget_refuses_without_erasing_actual_focus_work() -> Result<()> {
    let mut response = packet(true)?;
    response.records.remove(1);
    response.candidate_count = 2;
    let records = serde_json::to_vec(&response.records)?;
    assert!(pack(&mut response, None, 1024, false, 4096).is_err());
    assert_eq!(serde_json::to_vec(&response.records)?, records);
    assert!(
        airk_core::arbiter::focus::observed_focus_work(&response.records, &Budget::default())?
            .work_unknown
    );
    Ok(())
}

#[test]
fn combined_packet_keeps_fitting_off_bytes_and_requires_actual_focus_outer_headroom() -> Result<()>
{
    fn rendered_bytes(packet: &ContextResponseV2) -> Result<usize> {
        let mut value = serde_json::to_value(packet)?;
        value["request_id"] = json!("f".repeat(32));
        airk_core::reply::ReplyDetail::annotate(
            &mut value,
            Some(airk_core::reply::ReplyDetail::Rich),
        );
        airk_core::guidance::annotate(&mut value);
        Ok(serde_json::to_vec(&value)?.len())
    }
    let mut active = packet(false)?;
    active.records.remove(1);
    active.candidate_count = 2;
    let active_bytes = rendered_bytes(&active)?;
    let charged = serde_json::to_vec(&active.records)?;
    let outer = crate::combined_reads::context_packet_focus_binding_bytes(&active)?;
    assert!(outer > 0);
    let mut fitting = active.clone();
    pack(&mut fitting, None, active_bytes + outer, true, 4096)?;
    assert_eq!(serde_json::to_vec(&fitting.records)?, charged);
    assert!(pack(&mut active, None, active_bytes + outer - 1, true, 4096).is_err());
    assert_eq!(
        serde_json::to_vec(&active.records)?,
        charged,
        "charged receipt never drops to fit"
    );
    let mut off = fitting;
    off.records[1]
        .canonical_result
        .as_object_mut()
        .unwrap()
        .remove("focus");
    off.work.input_tokens = 0;
    off.work.compile_us = None;
    off.work.inference_us = None;
    let off_bytes = rendered_bytes(&off)?;
    let original = serde_json::to_vec(&off)?;
    assert_eq!(
        crate::combined_reads::context_packet_focus_binding_bytes(&off)?,
        0
    );
    pack(&mut off, None, off_bytes, true, 4096)?;
    assert_eq!(
        serde_json::to_vec(&off)?,
        original,
        "off retains the exact fitting packet and budget"
    );
    Ok(())
}

#[test]
fn derived_verification_reserve_preserves_off_packet_and_trims_only_presentation() -> Result<()> {
    use airk_core::verification::{VerificationBudget, VerificationMode, receipt_budget_for_reply};
    fn projection(packet: &ContextResponseV2) -> Result<Value> {
        let mut value = serde_json::to_value(packet)?;
        value["request_id"] = json!("f".repeat(32));
        airk_core::reply::ReplyDetail::annotate(
            &mut value,
            Some(airk_core::reply::ReplyDetail::Rich),
        );
        airk_core::guidance::annotate(&mut value);
        Ok(value)
    }
    for unknown in [false, true] {
        let mut packet = packet(unknown)?;
        packet.records.remove(1);
        packet.candidate_count = 2;
        packet.paragraphs.push(GroundedParagraph {
            text: "x".into(),
            evidence: vec![digest(82)],
            interpretation: true,
        });
        let current = serde_json::to_vec(&serde_json::to_string(&projection(&packet)?)?)?.len();
        let target = airk_core::MAX_RESPONSE_BYTES - 4096;
        packet.paragraphs[0].text = "x".repeat(1 + target - current);
        assert_eq!(receipt_budget_for_reply(&projection(&packet)?)?, 1024);
        let original = serde_json::to_vec(&packet)?;
        let mandatory = serde_json::to_vec(&packet.records)?;
        let mut off = packet.clone();
        pack(&mut off, None, 262_144, false, 4096)?;
        assert_eq!(
            serde_json::to_vec(&off)?,
            original,
            "Off keeps the exact fitting packet"
        );
        for mode in [VerificationMode::Report, VerificationMode::Require] {
            let budget = VerificationBudget {
                mode,
                max_sources: 25,
                max_bytes: 16 * 1024 * 1024,
                deadline_ms: 2000,
            };
            let floor = crate::verification::verification_reply_metadata_floor(budget)?;
            assert!(floor > 1024);
            let mut verified = packet.clone();
            pack(&mut verified, None, 262_144, false, 2048 + 2 * floor)?;
            assert_eq!(serde_json::to_vec(&verified.records)?, mandatory);
            assert!(verified.paragraphs.is_empty());
            assert_eq!(verified.omitted_count, 0);
            assert!(receipt_budget_for_reply(&projection(&verified)?)? >= floor);
        }
    }
    Ok(())
}
