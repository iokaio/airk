// SPDX-License-Identifier: Apache-2.0
//! Change set 2, Context option B (layer `context_b`): a wider roster under its
//! own task policy, the product's prompt rendering, and the best Read chosen
//! even when a terminal card scores highest. The product roster is unchanged.
use super::*;
use airk_core::arbiter::context_action::calibration::JointCalibration;
use airk_retrieval::arbiter::{context_action, context_wide};
use std::collections::BTreeMap;

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}

fn provisional(task_policy: Sha256Digest) -> Result<JointCalibration> {
    let reason = "provisional evaluation: terminal not qualified";
    Ok(serde_json::from_value(json!({
        "schema":"airk.arbiter.context-action.calibration/2","model":"arbiter-context-qwen3-17b",
        "compiler":digest(11),"task_policy":task_policy,
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

/// The change-set-1 gathering of `change_set_tests`: one search, its reads queued.
fn gathered() -> Result<Gathering> {
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
        budget: Budget {
            fanout: 12,
            calls: 12,
            ..Budget::default()
        },
    };
    let now = Instant::now();
    let mut gather = Gathering::with_layers(request, scope.clone(), application, now, software::Layers::all())?;
    gather.next(now)?;
    let mut body = native.records[0].canonical_result.clone();
    body["publication"] = serde_json::to_value(&scope.publication)?;
    gather.observe(Ok(body), now)?;
    Ok(gather)
}

fn rosters(gather: &Gathering) -> Result<(ContextActionRoster, ContextActionRoster)> {
    let choices = gather.choices();
    let eligible = choices.iter().map(|choice| choice.id).collect();
    Ok((
        context_action::build_roster_with_eligible(
            &gather.request.question,
            &gather.scope,
            gather.records(),
            &choices,
            &eligible,
        )?,
        context_wide::build_roster_with_eligible(
            &gather.request.question,
            &gather.scope,
            gather.records(),
            &choices,
            &eligible,
        )?,
    ))
}

#[test]
fn option_b_widens_the_roster_under_its_own_policy_and_leaves_the_product_roster_alone() -> Result<()> {
    let gather = gathered()?;
    let choices = gather.choices();
    assert!(choices.len() > 5, "more change-set reads queued than the product offers");
    let (product, wide) = rosters(&gather)?;
    assert_eq!(product.policy, context_action::policy_digest()?);
    assert_eq!(wide.policy, context_wide::policy_digest()?);
    assert_ne!(wide.policy, product.policy);
    assert_eq!(product.basis.read_choices_scored.len(), choices.len().min(5));
    assert_eq!(
        wide.basis.read_choices_scored.len(),
        choices.len().min(context_wide::READS)
    );
    // The product's reads come first, in the same order.
    assert_eq!(
        wide.basis.read_choices_scored[..product.basis.read_choices_scored.len()],
        product.basis.read_choices_scored[..]
    );
    let units = wide.basis.source_units_presented.len();
    assert!(units >= product.basis.source_units_presented.len());
    assert_eq!(
        units,
        product.basis.source_units_available.min(context_wide::UNITS)
    );
    assert!(wide.cards.len() <= airk_core::arbiter::context_action::MAX_ACTION_CARDS);
    wide.validate(gather.records())?;
    // No product identity is moved by option B: the product roster rebuilds
    // and its prompt still passes the product owner's checks.
    context_action::prompt_from_parts(&gather.request.question, gather.records(), &choices, &product)?;
    assert!(
        context_action::prompt_from_parts(&gather.request.question, gather.records(), &choices, &wide)
            .is_err()
    );
    Ok(())
}

#[test]
fn option_b_rendering_equals_the_product_prompt_on_a_product_roster() -> Result<()> {
    let gather = gathered()?;
    let choices = gather.choices();
    let (product, wide) = rosters(&gather)?;
    let official =
        context_action::prompt_from_parts(&gather.request.question, gather.records(), &choices, &product)?;
    let rendered = context_wide::render(&gather.request.question, gather.records(), &choices, &product)?;
    assert_eq!(serde_json::to_value(&rendered)?, serde_json::to_value(&official)?);
    let wide_prompt = context_wide::render(&gather.request.question, gather.records(), &choices, &wide)?;
    assert_eq!(wide_prompt.candidates.len(), wide.cards.len());
    assert_eq!(wide_prompt.policy, wide.policy);
    Ok(())
}

#[test]
fn option_b_takes_the_best_read_over_a_winning_terminal_and_the_product_rule_does_not() -> Result<()> {
    let gather = gathered()?;
    let (product, wide) = rosters(&gather)?;
    let scores_for = |roster: &ContextActionRoster| {
        let reads: Vec<usize> = (0..roster.cards.len())
            .filter(|index| matches!(roster.cards[*index].action, ContextAction::Read { .. }))
            .collect();
        let mut scores = vec![0.0_f32; roster.cards.len()];
        for (rank, index) in reads.iter().enumerate() {
            scores[*index] = rank as f32 * 0.1;
        }
        let sufficient = roster
            .cards
            .iter()
            .position(|card| card.action == ContextAction::Sufficient)
            .unwrap();
        scores[sufficient] = 9.0;
        (scores, reads)
    };
    let (scores, reads) = scores_for(&wide);
    let best = *reads.last().unwrap();
    let calibration = provisional(wide.policy)?;
    assert_eq!(
        context_wide::selected_action(&scores, &wide, &calibration)?,
        Some(wide.cards[best].id)
    );
    assert_eq!(
        context_wide::selected_action_for(&scores, &wide, &calibration)?,
        Some(wide.cards[best].id)
    );
    // Tied best Reads abstain.
    let mut tied = scores.clone();
    tied[reads[0]] = tied[best];
    assert_eq!(context_wide::selected_action(&tied, &wide, &calibration)?, None);
    // A calibration bound to another policy is refused.
    assert!(context_wide::selected_action(&scores, &wide, &provisional(product.policy)?).is_err());
    // The product owner on the same shape abstains: its top card is a terminal.
    let (product_scores, _) = scores_for(&product);
    let product_calibration = provisional(product.policy)?;
    assert_eq!(
        context_action::selected_action(&product_scores, &product, &product_calibration)?,
        None
    );
    assert_eq!(
        context_wide::selected_action_for(&product_scores, &product, &product_calibration)?,
        None
    );
    Ok(())
}

#[test]
fn only_a_profile_declaring_option_b_is_scored_under_it() -> Result<()> {
    use airk_core::arbiter::{Profile, Task};
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.task_policies = BTreeMap::from([(Task::ContextAction, context_action::policy_digest()?)]);
    assert_eq!(context_wide::policy_for(&profile)?, context_action::policy_digest()?);
    profile.task_policies = BTreeMap::from([(Task::ContextAction, context_wide::policy_digest()?)]);
    assert_eq!(context_wide::policy_for(&profile)?, context_wide::policy_digest()?);
    profile.task_policies = BTreeMap::from([(Task::ContextAction, digest(1))]);
    assert_eq!(context_wide::policy_for(&profile)?, context_action::policy_digest()?);
    Ok(())
}
