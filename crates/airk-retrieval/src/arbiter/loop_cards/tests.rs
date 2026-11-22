// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    arbiter::{
        ModelChoice,
        answer_loop::{LOOP_CALIBRATION_SCHEMA, LoopRule, QuestionSpan},
    },
    native_loop::LoopCard,
};

fn card(symbol: &str, class: CardClass, text: &str) -> LoopCard {
    LoopCard {
        symbol: symbol.into(),
        class,
        text: text.into(),
        partition: vec![],
    }
}

fn segment(stage: LoopStage, part: u8, need: u8, cards: Vec<LoopCard>) -> CardSegment {
    let (need, round) = match stage {
        LoopStage::Split | LoopStage::Plan => (0, 0),
        _ => (need, 1),
    };
    CardSegment {
        ordinal: 1,
        card_slot: 1,
        stage,
        part,
        need,
        round,
        subject: "where is the restart guard defined".into(),
        cards,
        multiple: stage == LoopStage::Plan,
    }
}

fn coverage(need: u8, alternatives: usize) -> CardSegment {
    let mut cards = vec![card(
        "A",
        CardClass::Covered,
        &format!("s{need} covered: e1"),
    )];
    for index in 0..alternatives {
        cards.push(card(
            SYMBOLS[index + 1],
            CardClass::SearchForTerm,
            &format!("s{need} search: term{index}"),
        ));
    }
    segment(LoopStage::Coverage, 1, need, cards)
}

fn plan(proposals: usize) -> CardSegment {
    let mut cards: Vec<LoopCard> = (0..proposals)
        .map(|index| {
            card(
                SYMBOLS[index],
                CardClass::PlanNeed,
                &format!("need definition: Name{index}"),
            )
        })
        .collect();
    cards.push(card(
        SYMBOLS[proposals],
        CardClass::PlanNeed,
        super::super::loop_plan::REFERENCE,
    ));
    segment(LoopStage::Plan, 1, 0, cards)
}

fn split() -> CardSegment {
    let whole = |symbol: &str, class, text: &str, partition: Vec<(usize, usize)>| LoopCard {
        symbol: symbol.into(),
        class,
        text: text.into(),
        partition: partition
            .into_iter()
            .map(|(utf8_start, utf8_end)| QuestionSpan {
                utf8_start,
                utf8_end,
            })
            .collect(),
    };
    let mut segment = segment(
        LoopStage::Split,
        0,
        0,
        vec![
            whole("A", CardClass::KeepWhole, "keep whole", vec![(0, 36)]),
            whole(
                "B",
                CardClass::SplitAt,
                "parts: [1] 0-12 [2] 12-36",
                vec![(0, 12), (12, 36)],
            ),
        ],
    );
    segment.subject = "where is it, and who calls the guard".into();
    segment
}

fn calibration(task: Task, rules: Vec<LoopRule>) -> LoopCalibration {
    let profile = crate::arbiter::runtime::catalog_profiles()
        .expect("catalog")
        .into_iter()
        .find(|profile| profile.model == ModelChoice::ContextQwenCompact)
        .expect("a Context profile");
    LoopCalibration {
        schema: LOOP_CALIBRATION_SCHEMA.into(),
        model: profile.model,
        task,
        compiler: profile.compiler,
        task_policy: task_policy(task).expect("policy"),
        score_abi: profile.abi,
        assets: Default::default(),
        rules,
    }
}

fn unruled(stage: LoopStage) -> LoopCalibration {
    calibration(card_task(stage).expect("card task"), vec![])
}

#[test]
fn policy_digests_are_stable_distinct_and_bound_to_their_tasks() -> Result<()> {
    let policies = task_policies()?;
    assert_eq!(policies.len(), 4);
    let distinct: std::collections::BTreeSet<_> =
        policies.iter().map(|(_, policy)| *policy).collect();
    assert_eq!(distinct.len(), 4, "each stage has its own identity");
    for (task, policy) in &policies {
        assert_eq!(task_policy(*task)?, *policy);
        assert_eq!(task_policy(*task)?, *policy, "stable across calls");
        assert_ne!(
            *policy,
            crate::arbiter::context_action::policy_digest()?,
            "never the one-turn Context policy"
        );
    }
    assert!(task_policy(Task::ContextAction).is_err());
    assert!(task_policy(Task::FocusRank).is_err());
    assert!(policy_digest(LoopStage::Read).is_err());
    Ok(())
}

#[test]
fn tampered_rosters_are_refused_before_compile() {
    let mut reordered = coverage(1, 2);
    reordered.cards.swap(0, 1);
    reordered.cards[0].symbol = "A".into();
    reordered.cards[1].symbol = "B".into();
    assert!(validate_segments(LoopStage::Coverage, &[reordered]).is_err());
    let mut skipped = coverage(1, 2);
    skipped.cards[1].symbol = "C".into();
    assert!(validate_segments(LoopStage::Coverage, &[skipped]).is_err());
    let mut no_reference = plan(2);
    no_reference.cards.last_mut().expect("card").text = "need tests: guard test".into();
    assert!(validate_segments(LoopStage::Plan, &[no_reference]).is_err());
    let mut split_first = split();
    split_first.cards.swap(0, 1);
    assert!(validate_segments(LoopStage::Split, &[split_first]).is_err());
    assert!(validate_segments(LoopStage::Split, &[split(), split()]).is_err());
    let single_follow = segment(
        LoopStage::FollowUp,
        1,
        1,
        vec![card("A", CardClass::FollowUpHop, "s1 symbols Guard")],
    );
    assert!(validate_segments(LoopStage::FollowUp, &[single_follow]).is_err());
    assert!(validate_segments(LoopStage::Plan, &[coverage(1, 1)]).is_err());
    assert!(validate_segments(LoopStage::Coverage, &[coverage(1, 1)]).is_ok());
    assert!(validate_segments(LoopStage::Plan, &[plan(3)]).is_ok());
    assert!(validate_segments(LoopStage::Split, &[split()]).is_ok());
}

#[test]
fn eight_card_bound_holds_across_a_batched_call() {
    assert!(validate_segments(LoopStage::Coverage, &[coverage(1, 3), coverage(2, 3)]).is_ok());
    assert!(
        validate_segments(
            LoopStage::Coverage,
            &[coverage(1, 3), coverage(2, 3), coverage(3, 1)]
        )
        .is_err()
    );
}

#[test]
fn prompt_shows_every_card_labelled_by_segment_within_lane_bounds() -> Result<()> {
    let segments = [coverage(1, 2), coverage(2, 1)];
    let policy = policy_digest(LoopStage::Coverage)?;
    let prompt = render(Task::ContextCoverage, policy, &segments)?;
    assert_eq!(prompt.candidates.len(), 5);
    assert_eq!(prompt.candidates[0].id, "c1");
    assert!(prompt.candidates[0].text.starts_with("[s1] "));
    assert!(prompt.candidates[3].text.starts_with("[s2] "));
    assert_eq!(prompt.question, super::super::loop_coverage::QUESTION);
    assert_eq!(prompt.policy, policy);
    assert!(prompt.state.len() <= 65_536);
    let state: serde_json::Value = serde_json::from_str(&prompt.state)?;
    assert_eq!(state["task"], "coverage");
    assert_eq!(state["segments"][1]["cards"][1], "c5");
    assert!(
        render(
            Task::ContextCoverage,
            policy_digest(LoopStage::Plan)?,
            &segments
        )
        .is_err(),
        "a foreign policy is refused"
    );
    assert!(render(Task::ContextAction, policy, &segments).is_err());
    let mut huge = split();
    huge.subject = "x".repeat(70_000);
    assert!(
        render(
            Task::ContextSplit,
            policy_digest(LoopStage::Split)?,
            &[huge]
        )
        .is_err(),
        "the state stays within the lane's 65,536 bytes"
    );
    Ok(())
}

#[test]
fn zero_rule_calibration_abstains_every_segment() -> Result<()> {
    let segments = [coverage(1, 1), coverage(2, 1)];
    let choices = select(
        LoopStage::Coverage,
        &segments,
        &[3.0, 1.0, 0.5, 2.5],
        &unruled(LoopStage::Coverage),
        DecisionMode::Calibrated,
    )?;
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().all(|choice| choice.chosen.is_empty()
        && choice.abstained.is_some()
        && choice.margin.is_none()
        && choice.rule.is_none()));
    assert_eq!(choices[0].raw_scores, vec![3.0, 1.0]);
    assert_eq!(choices[1].raw_scores, vec![0.5, 2.5]);
    Ok(())
}

#[test]
fn calibrated_rule_admits_per_segment_with_exact_top_minus_runner_up() -> Result<()> {
    let rules = vec![
        LoopRule {
            class: CardClass::Covered,
            minimum_raw_margin: 1.0,
        },
        LoopRule {
            class: CardClass::SearchForTerm,
            minimum_raw_margin: 0.25,
        },
    ];
    let calibration = calibration(Task::ContextCoverage, rules);
    let segments = [coverage(1, 2), coverage(2, 1)];
    let scores = [2.1f32, 1.3, 0.7, 0.2, 0.6];
    let choices = select(
        LoopStage::Coverage,
        &segments,
        &scores,
        &calibration,
        DecisionMode::Calibrated,
    )?;
    // Segment 1: Covered wins at 0.8, below its 1.0 rule: abstains.
    assert!(choices[0].chosen.is_empty());
    // Segment 2: SearchForTerm wins at 0.4 over 0.25: rule 1 admits.
    assert_eq!(choices[1].chosen, vec![1]);
    assert_eq!(choices[1].rule, Some(1));
    assert_eq!(
        choices[1].margin.map(f32::to_bits),
        Some((0.6f32 - 0.2f32).to_bits())
    );
    let tied = select(
        LoopStage::Coverage,
        &[coverage(1, 1)],
        &[1.0, 1.0],
        &calibration,
        DecisionMode::Calibrated,
    )?;
    assert_eq!(tied[0].abstained, Some("The top card was tied."));
    assert!(
        select(
            LoopStage::Coverage,
            &[coverage(1, 1)],
            &[1.0, f32::NAN],
            &calibration,
            DecisionMode::Calibrated,
        )
        .is_err()
    );
    assert!(
        select(
            LoopStage::Coverage,
            &[coverage(1, 1)],
            &[1.0],
            &calibration,
            DecisionMode::Calibrated,
        )
        .is_err(),
        "score shape must match the presented cards"
    );
    assert!(
        select(
            LoopStage::Coverage,
            &[coverage(1, 1)],
            &[1.0, 0.0],
            &unruled(LoopStage::FollowUp),
            DecisionMode::Calibrated,
        )
        .is_err(),
        "another task's calibration is refused"
    );
    Ok(())
}

#[test]
fn top_score_never_abstains_and_ties_go_to_the_lowest_symbol() -> Result<()> {
    let choices = select(
        LoopStage::Coverage,
        &[coverage(1, 2)],
        &[0.5, 2.0, 2.0],
        &unruled(LoopStage::Coverage),
        DecisionMode::TopScore,
    )?;
    assert_eq!(choices[0].chosen, vec![1]);
    assert_eq!(choices[0].rule, None);
    assert_eq!(choices[0].margin, Some(0.0));
    assert_eq!(choices[0].abstained, None);
    Ok(())
}

#[test]
fn plan_takes_the_proposals_above_the_reference_at_least_one_at_most_four() -> Result<()> {
    let calibration = calibration(
        Task::ContextPlan,
        vec![LoopRule {
            class: CardClass::PlanNeed,
            minimum_raw_margin: 0.0,
        }],
    );
    let roster = plan(6);
    // Five proposals score above the reference (last, 1.0): the four best.
    let scores = [3.0f32, 1.5, 2.0, 0.5, 2.5, 1.2, 1.0];
    let choices = select(
        LoopStage::Plan,
        std::slice::from_ref(&roster),
        &scores,
        &calibration,
        DecisionMode::Calibrated,
    )?;
    assert_eq!(choices[0].chosen, vec![0, 4, 2, 1]);
    assert_eq!(
        choices[0].margin.map(f32::to_bits),
        Some((3.0f32 - 2.5).to_bits())
    );
    // The reference wins outright: still the best proposal, never the reference.
    let low = [0.1f32, 0.3, 0.2, 0.0, 0.0, 0.0, 5.0];
    let choices = select(
        LoopStage::Plan,
        std::slice::from_ref(&roster),
        &low,
        &calibration,
        DecisionMode::Calibrated,
    )?;
    assert_eq!(choices[0].chosen, vec![1]);
    // Unruled Plan abstains, so the kernel takes every proposed need.
    let choices = select(
        LoopStage::Plan,
        std::slice::from_ref(&roster),
        &scores,
        &unruled(LoopStage::Plan),
        DecisionMode::Calibrated,
    )?;
    assert!(choices[0].chosen.is_empty());
    Ok(())
}

/// The recorded identities (compiler/CONTRACT.md, "Answer loop card task
/// policies"). A change to any policy source, the shared owner, a question
/// or the Core loop semantics moves them and needs a recorded decision.
#[test]
fn recorded_loop_policy_identities() -> Result<()> {
    let recorded = [
        (
            Task::ContextSplit,
            "42669ee164c7a620926274a1efe6f0597ca971c7c52fecf20e8d7f0b0c873ccc",
        ),
        (
            Task::ContextPlan,
            "28c43b34d4859710908dde7fe5e2c3fd390027d01af82e6f67021626b286a0d6",
        ),
        (
            Task::ContextCoverage,
            "5eb717cf1d4c6b3484e77c1c275bf93e4e85ebbda0ea34c1a50357e9354fe1a7",
        ),
        (
            Task::ContextFollowUp,
            "a3bbaf848d2c2e983f24e8af0a4627dc7e8f1e53730dc2bbdec81c7d51d86781",
        ),
    ];
    let core = airk_core::arbiter::answer_loop::loop_semantic_policy_digest()?.to_string();
    let live: Vec<String> = recorded
        .iter()
        .map(|(task, _)| task_policy(*task).map(|policy| policy.to_string()))
        .collect::<Result<_>>()?;
    assert_eq!(
        core, "3b919cb0193f7bcdea4f58a23d7543ff6e980d7da38aec474035e5798b031357",
        "live {live:?}"
    );
    for ((task, expected), live) in recorded.iter().zip(&live) {
        assert_eq!(live, expected, "{task:?}");
    }
    Ok(())
}
