// SPDX-License-Identifier: Apache-2.0
//! Pure actuation/packing controls; no model or transport qualification claim.
use super::*;
fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
fn fixture() -> Result<(Gathering, Joint, ContextActionRoster)> {
    let raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    let native: ContextResponseV2 = serde_json::from_value(raw["cases"][0]["input"].clone())?;
    let scope = native.assessment.scope;
    let application = IndexApplication {
        scope: scope.clone(),
        profile: Some(digest(10)),
        availability: Availability::Ready,
        reason: "Synthetic actuation fixture, not a qualified model.".into(),
    };
    let request = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: native.question,
        budget: Budget::default(),
    };
    let mut gather = Gathering::new(request, scope.clone(), application, Instant::now())?;
    gather.next(Instant::now())?;
    let mut body = native.records[0].canonical_result.clone();
    body["publication"] = serde_json::to_value(&scope.publication)?;
    gather.observe(Ok(body), Instant::now())?;
    let roster = airk_retrieval::arbiter::context_action::build_roster(
        &gather.request.question,
        &scope,
        gather.records(),
        &gather.choices(),
    )?;
    let joint = Joint {
        history: vec![],
        coverage: PartitionCoverage::Unassessed {
            reason: AssessmentUnavailable::TaskUnsupported,
        },
        gaps: vec![Gap::Unsupported],
        terminal: None,
        revoked: false,
        blocked: false,
        ordinals: vec![1, 3],
        token_limit: 2048,
    };
    Ok((gather, joint, roster))
}
fn advice(
    gather: &Gathering,
    roster: &ContextActionRoster,
    card: &ContextActionCard,
) -> ContextActionAdvice {
    ContextActionAdvice {
        schema: ADVICE_SCHEMA_V2.into(),
        task: Task::ContextAction,
        profile: digest(10),
        runtime: RUNTIME.into(),
        compiler: digest(11),
        calibration: digest(12),
        policy: digest(13),
        task_policy: roster.policy,
        scope: gather.scope.clone(),
        input_digest: digest(14),
        evidence: roster
            .basis
            .source_units_presented
            .iter()
            .map(|unit| unit.record)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        reserved_input_tokens: 100,
        observed_input_tokens: Some(100),
        work: Work {
            input_tokens: 100,
            compile_us: Some(0),
            inference_us: Some(0),
            ..Work::default()
        },
        outcome: Outcome::Accepted {
            decision: Decision::ContextAction(card.id),
        },
    }
}
#[test]
fn typed_terminal_cards_stop_only_local_gathering_with_full_native_records() -> Result<()> {
    for (action, expected) in [
        (ContextAction::Sufficient, PartitionTermination::Covered),
        (
            ContextAction::Clarify,
            PartitionTermination::ClarificationRequired,
        ),
        (
            ContextAction::NoJustifiedRead,
            PartitionTermination::NoJustifiedRead,
        ),
    ] {
        let (mut gather, mut joint, roster) = fixture()?;
        let card = roster
            .cards
            .iter()
            .find(|card| card.action == action)
            .unwrap();
        let before = gather.records()[0].canonical_result.clone();
        let advice = advice(&gather, &roster, card);
        joint.apply(&mut gather, roster, 1, advice, Instant::now(), true)?;
        assert_eq!(joint.terminal, Some(expected));
        assert_eq!(gather.records()[0].canonical_result, before);
        assert_eq!(gather.output.work.input_tokens, 100);
    }
    Ok(())
}
#[test]
fn read_action_uses_exact_verified_choice_then_invalidates_pre_read_sufficiency() -> Result<()> {
    let (mut gather, mut joint, roster) = fixture()?;
    let card = roster
        .cards
        .iter()
        .find(|card| matches!(card.action, ContextAction::Read { .. }))
        .unwrap();
    let ContextAction::Read { choice } = card.action else {
        unreachable!()
    };
    let advice = advice(&gather, &roster, card);
    joint.apply(
        &mut gather,
        roster.clone(),
        1,
        advice.clone(),
        Instant::now(),
        true,
    )?;
    assert!(
        joint
            .apply(&mut gather, roster, 1, advice, Instant::now(), true)
            .is_err()
    );
    let dispatch = gather.next(Instant::now())?.unwrap();
    assert_eq!(dispatch.choice.id, choice);
    let body = gather.records()[0].canonical_result.clone();
    gather.observe(Ok(body), Instant::now())?;
    joint.invalidate(AssessmentUnavailable::NewEvidence);
    assert_eq!(
        joint.history[0].applicability,
        AssessmentApplicability::HistoricalAfterRead
    );
    assert!(matches!(
        joint.coverage,
        PartitionCoverage::Unassessed { .. }
    ));
    assert_eq!(gather.output.work.calls, 2);
    Ok(())
}
#[test]
fn lost_or_foreign_native_advice_never_replays_or_redirects() -> Result<()> {
    let (mut gather, mut joint, roster) = fixture()?;
    let card = roster
        .cards
        .iter()
        .find(|card| card.action == ContextAction::Sufficient)
        .unwrap();
    let mut wrong = advice(&gather, &roster, card);
    wrong.task_policy = digest(99);
    assert!(
        joint
            .apply(&mut gather, roster.clone(), 1, wrong, Instant::now(), true)
            .is_err()
    );
    joint.lost(1, &gather.scope, &roster);
    let actual = advice(&gather, &roster, card);
    assert!(
        joint
            .apply(&mut gather, roster, 3, actual, Instant::now(), true)
            .is_err()
    );
    assert_eq!(gather.output.work.calls, 1);
    assert!(matches!(
        joint.history[0].result,
        AssessmentResult::Unavailable {
            work_unknown: true,
            advice: None,
            ..
        }
    ));
    Ok(())
}
#[test]
fn packing_support_omission_invalidates_current_coverage_and_dependent_paragraphs() -> Result<()> {
    let (mut gather, mut joint, _) = fixture()?;
    let dispatch = gather.next(Instant::now())?.unwrap();
    let mut body = gather.records()[0].canonical_result.clone();
    body["hits"][0]["text"] = json!("A distinct retained constraint, with full native metadata.");
    gather.observe(Ok(body), Instant::now())?;
    assert_eq!(dispatch.choice.operation, NativeRead::Context);
    let mut roster = airk_retrieval::arbiter::context_action::build_roster(
        &gather.request.question,
        &gather.scope,
        gather.records(),
        &gather.choices(),
    )?;
    let card = roster
        .cards
        .iter_mut()
        .find(|card| card.action == ContextAction::Sufficient)
        .unwrap();
    assert_eq!(card.assessments.len(), 2);
    card.assessments[0].role = EvidenceRole::Definition;
    card.assessments[1].role = EvidenceRole::Constraint;
    let card = card.clone();
    let advice = advice(&gather, &roster, &card);
    joint.apply(&mut gather, roster, 1, advice, Instant::now(), true)?;
    let native = gather.finish_native(Instant::now(), false)?;
    let mut response = ContextResponseV2 {
        schema: CONTEXT_SCHEMA_V2.into(),
        packet_schema: PACKET_SCHEMA.into(),
        question: native.question,
        records: native.records,
        paragraphs: vec![],
        gaps: joint.gaps,
        stop: PartitionTermination::Covered,
        work: native.work,
        candidate_count: native.candidate_count,
        omitted_count: 0,
        assessment_history: joint.history,
        assessment: PartitionAssessment {
            scope: fixture()?.0.scope,
            coverage: joint.coverage,
            termination: PartitionTermination::Covered,
        },
        applications: native.applications,
    };
    response.paragraphs = organize(&response)?;
    assert_eq!(response.paragraphs.len(), 2);
    let fact = GroundedParagraph {
        text: "fn café() {}".into(),
        evidence: vec![response.records[0].id],
        interpretation: false,
    };
    response.paragraphs.push(fact.clone());
    response.validate(&Budget::default())?;
    let size = serde_json::to_vec(&response)?.len();
    pack(&mut response, None, size - 1)?;
    assert_eq!(response.records.len(), 1);
    assert_eq!(response.omitted_count, 1);
    assert_eq!(response.stop, PartitionTermination::Budget);
    assert_eq!(
        response.assessment_history[0].applicability,
        AssessmentApplicability::BasisOmitted
    );
    assert_eq!(response.paragraphs.len(), 1);
    assert_eq!(response.paragraphs[0].text, fact.text);
    response.validate(&Budget::default())?;
    Ok(())
}

#[test]
fn expired_v2_deadline_precedes_native_context_lookup_and_zero_allowance_is_budget() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let kernel = crate::Kernel::open(temp.path(), Some(temp.path().join("data")))?;
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = kernel.answer_with_reader_v2(
        ContextRequestV2 {
            schema: REQUEST_SCHEMA_V2.into(),
            question: "original deadline".into(),
            budget: Budget::default(),
        },
        Instant::now(),
        &|| Ok(()),
        &|_, _, _| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(json!({}))
        },
    );
    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("original deadline exhausted before native admission")
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let (gather, _, _) = fixture()?;
    assert_eq!(
        initial_reason(&gather.output.applications[0], 0, true),
        AssessmentUnavailable::TokenBudget
    );
    assert_eq!(
        initial_reason(&gather.output.applications[0], 2048, false),
        AssessmentUnavailable::TaskUnsupported
    );
    let mut unsupported = gather.output.applications[0].clone();
    unsupported.availability = Availability::UnsupportedTask;
    assert_eq!(
        initial_reason(&unsupported, 2048, false),
        AssessmentUnavailable::TaskUnsupported
    );
    Ok(())
}
