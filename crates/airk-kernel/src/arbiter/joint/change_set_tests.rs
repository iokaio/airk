// SPDX-License-Identifier: Apache-2.0
//! Change set 1 (change 6): Context chooses among the change-set reads, and the
//! provisional option-A rule accepts exactly the Read cards the roster builds.
use super::*;
use airk_core::arbiter::context_action::calibration::JointCalibration;

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}

fn provisional() -> Result<JointCalibration> {
    let reason = "provisional evaluation: terminal not qualified";
    Ok(serde_json::from_value(json!({
        "schema":"airk.arbiter.context-action.calibration/2","model":"arbiter-context-qwen3-17b",
        "compiler":digest(11),"task_policy":digest(13),
        "score_abi":{"readout":"decision_symbol","inputs":[{"name":"input_ids","element":"int64","axes":["batch","tokens"]}],
            "output":{"name":"scores","element":"float32","axes":["batch","options"]}},
        "assets":{},
        "rules":[{"action":"read","allowed_gaps":["unsupported"],"allowed_reasons":["additional_evidence_may_help"],
            "allowed_roles":["unknown"],"allowed_relevance":["uncertain"],"minimum_raw_margin":0.0}],
        "class_support":[{"action":"read","support":{"status":"supported","qualification":null}},
            {"action":"sufficient","support":{"status":"unsupported","reason":reason}},
            {"action":"clarify","support":{"status":"unsupported","reason":reason}},
            {"action":"no_justified_read","support":{"status":"unsupported","reason":reason}}]
    }))?)
}

#[test]
fn an_accepted_read_of_a_change_set_choice_runs_next_and_terminals_stay_unaccepted() -> Result<()> {
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
        question: "Where is `Gathering` defined and which tests cover plan_followups?".into(),
        budget: Budget::default(),
    };
    let now = Instant::now();
    let mut gather = Gathering::with_layers(
        request,
        scope.clone(),
        application,
        now,
        software::Layers::all(),
    )?;
    gather.next(now)?;
    let mut body = native.records[0].canonical_result.clone();
    body["publication"] = serde_json::to_value(&scope.publication)?;
    gather.observe(Ok(body), now)?;
    let choices = gather.choices();
    assert!(choices.len() >= 3, "change-set reads queued");
    let roster = airk_retrieval::arbiter::context_action::build_roster(
        &gather.request.question,
        &scope,
        gather.records(),
        &choices,
    )?;
    let second = choices[1].id;
    let card = roster
        .cards
        .iter()
        .find(|card| card.action == ContextAction::Read { choice: second })
        .context("roster offers the second queued read")?
        .clone();
    let calibration = provisional()?;
    calibration.validate_class_support()?;
    assert!(calibration.accepts(&card, 0.0));
    for terminal in roster
        .cards
        .iter()
        .filter(|card| !matches!(card.action, ContextAction::Read { .. }))
    {
        assert!(!calibration.accepts(terminal, 10.0));
    }
    assert!(roster
        .cards
        .iter()
        .filter(|card| matches!(card.action, ContextAction::Read { .. }))
        .all(|card| calibration.accepts(card, 0.0)));
    let mut joint = Joint {
        history: vec![],
        coverage: PartitionCoverage::Unassessed {
            reason: AssessmentUnavailable::TaskUnsupported,
        },
        gaps: vec![Gap::Unsupported],
        terminal: None,
        revoked: false,
        blocked: false,
        ordinals: vec![1, 2, 3],
        token_limit: 8192,
    };
    let advice = ContextActionAdvice {
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
    };
    joint.apply(&mut gather, roster, 1, advice, Instant::now(), true)?;
    assert_eq!(joint.terminal, None);
    let next = gather.next(Instant::now())?.context("chosen read")?;
    assert_eq!(next.choice.id, second);
    // Read targets state the read in words and end with an exact range.
    assert!(choices.iter().all(|choice| choice.target.contains(" at ")
        && choice.target.rsplit_once(':').is_some_and(|(_, range)| range.contains('-'))));
    Ok(())
}
