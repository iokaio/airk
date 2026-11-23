// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    arbiter::answer_loop::{CardClass, LoopStage, TokenAttribution},
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::IndexId,
    native_loop::{CardSegment, LoopCard},
};
use airk_retrieval::arbiter::loop_cards::SegmentChoice;

fn card(symbol: &str, class: CardClass, text: &str) -> LoopCard {
    LoopCard {
        symbol: symbol.into(),
        class,
        text: text.into(),
        partition: vec![],
    }
}
fn coverage(ordinal: u8, need: u8) -> CardSegment {
    CardSegment {
        ordinal,
        card_slot: 1,
        stage: LoopStage::Coverage,
        part: 1,
        need,
        round: 1,
        subject: "guard definition".into(),
        cards: vec![
            card("A", CardClass::Covered, &format!("s{need} covered: e1")),
            card(
                "B",
                CardClass::SearchForTerm,
                &format!("s{need} search: guard"),
            ),
        ],
        multiple: false,
        evidence: vec![],
        unshown: 0,
        verdict: None,
    }
}
fn plan() -> CardSegment {
    CardSegment {
        ordinal: 1,
        card_slot: 1,
        stage: LoopStage::Plan,
        part: 1,
        need: 0,
        round: 0,
        subject: "explain the restart guard".into(),
        cards: vec![
            card("A", CardClass::PlanNeed, "need definition: RestartGuard"),
            card("B", CardClass::PlanNeed, "need tests: RestartGuard test"),
            card("C", CardClass::PlanNeed, "no further sub-need"),
        ],
        multiple: true,
        evidence: vec![],
        unshown: 0,
        verdict: None,
    }
}
fn advice(segments: &[CardSegment]) -> CardAdvice {
    CardAdvice {
        schema: CARD_ADVICE_SCHEMA.into(),
        task: Task::ContextCoverage,
        profile: Sha256Digest::from_bytes([1; 32]),
        runtime: RUNTIME.into(),
        compiler: Sha256Digest::from_bytes([2; 32]),
        calibration: Sha256Digest::from_bytes([3; 32]),
        policy: Sha256Digest::from_bytes([4; 32]),
        task_policy: Sha256Digest::from_bytes([5; 32]),
        scope: ScopeBinding {
            installation: Sha256Digest::from_bytes([6; 32]),
            catalog: Incarnation::from_bytes([7; 16]),
            index_id: IndexId::from_identity(Incarnation::from_bytes([8; 16])),
            authorization: Sha256Digest::from_bytes([9; 32]),
            configuration_revision: 2,
            publication: airk_core::evidence::RevisionEnvelope::new(
                "fixture".into(),
                RevisionToken {
                    incarnation: Incarnation::from_bytes([10; 16]),
                    generation: 1,
                    revision: Sha256Digest::from_bytes([11; 32]),
                },
            ),
        },
        input_digest: Sha256Digest::from_bytes([12; 32]),
        card_slot: 1,
        reserved_input_tokens: 4_000,
        observed_input_tokens: None,
        attribution: vec![TokenAttribution {
            part: 1,
            round: 1,
            tokens: 4_000,
        }],
        work: Work::default(),
        segments: segments
            .iter()
            .map(|segment| SegmentOutcome {
                ordinal: segment.ordinal,
                raw_scores: vec![],
                outcome: Outcome::Unavailable {
                    reason: UNKNOWN.into(),
                },
                rule: None,
                margin: None,
            })
            .collect(),
    }
}
fn observed(advice: &mut CardAdvice, tokens: usize) {
    advice.observed_input_tokens = Some(tokens);
    advice.attribution[0].tokens = tokens;
    advice.work.input_tokens = tokens;
    advice.work.compile_us = Some(1);
    advice.work.inference_us = Some(1);
}

#[test]
fn unknown_outcome_advice_keeps_the_whole_reservation_and_validates() -> Result<()> {
    let segments = [coverage(1, 1), coverage(2, 2)];
    let advice = advice(&segments);
    advice.validate()?;
    assert_eq!(advice.observed_input_tokens, None);
    assert_eq!(advice.attribution[0].tokens, 4_000);
    Ok(())
}

#[test]
fn a_known_non_dispatch_charges_nothing_and_names_its_reason() -> Result<()> {
    let segments = [coverage(1, 1)];
    let mut refused = advice(&segments);
    not_dispatched(&mut refused, NOT_ADMITTED, true);
    refused.validate()?;
    assert_eq!(refused.observed_input_tokens, Some(0));
    assert_eq!(
        refused.attribution[0].tokens, 0,
        "the part-round charge is kept at zero"
    );
    assert!(matches!(
        &refused.segments[0].outcome,
        Outcome::Unsupported { reason } if reason == NOT_ADMITTED
    ));
    Ok(())
}

#[test]
fn per_segment_choices_become_their_own_outcomes() -> Result<()> {
    let segments = [coverage(1, 1), coverage(2, 2)];
    let mut advice = advice(&segments);
    observed(&mut advice, 3_000);
    apply_choices(
        &mut advice,
        &segments,
        vec![
            SegmentChoice {
                raw_scores: vec![2.0, 1.0],
                chosen: vec![0],
                rule: Some(0),
                margin: Some(1.0),
                abstained: None,
            },
            SegmentChoice {
                raw_scores: vec![0.5, 0.5],
                chosen: vec![],
                rule: None,
                margin: None,
                abstained: Some("The top card was tied."),
            },
        ],
    )?;
    advice.validate()?;
    assert!(matches!(
        &advice.segments[0].outcome,
        Outcome::Accepted { decision: Decision::LoopCard(card) }
            if *card == segments[0].cards[0].text_digest()?
    ));
    assert_eq!(advice.segments[0].margin, Some(1.0));
    assert_eq!(advice.segments[0].rule, Some(0));
    assert!(matches!(
        advice.segments[1].outcome,
        Outcome::Abstained { .. }
    ));
    assert_eq!(advice.segments[1].raw_scores, vec![0.5, 0.5]);
    assert!(
        apply_choices(&mut advice, &segments, vec![]).is_err(),
        "a lost segment refuses"
    );
    Ok(())
}

#[test]
fn plan_choices_are_several_cards_in_chosen_order() -> Result<()> {
    let segments = [plan()];
    let mut advice = CardAdvice {
        task: Task::ContextPlan,
        attribution: vec![TokenAttribution {
            part: 1,
            round: 0,
            tokens: 4_000,
        }],
        ..advice(&segments)
    };
    observed(&mut advice, 2_000);
    apply_choices(
        &mut advice,
        &segments,
        vec![SegmentChoice {
            raw_scores: vec![1.0, 3.0, 0.5],
            chosen: vec![1, 0],
            rule: None,
            margin: Some(2.0),
            abstained: None,
        }],
    )?;
    advice.validate()?;
    let expected = vec![
        segments[0].cards[1].text_digest()?,
        segments[0].cards[0].text_digest()?,
    ];
    assert!(matches!(
        &advice.segments[0].outcome,
        Outcome::Accepted { decision: Decision::LoopCards(cards) } if *cards == expected
    ));
    assert!(
        apply_choices(
            &mut advice,
            &segments,
            vec![SegmentChoice {
                raw_scores: vec![1.0, 3.0, 0.5],
                chosen: vec![7],
                rule: None,
                margin: Some(2.0),
                abstained: None,
            }],
        )
        .is_err(),
        "a choice outside the roster refuses"
    );
    Ok(())
}

#[test]
fn a_failed_score_keeps_no_decision() -> Result<()> {
    let segments = [coverage(1, 1)];
    let mut advice = advice(&segments);
    observed(&mut advice, 1_000);
    unavailable(&mut advice, SCORE_FAILED);
    advice.validate()?;
    assert!(advice.segments[0].raw_scores.is_empty());
    assert!(matches!(
        advice.segments[0].outcome,
        Outcome::Unavailable { .. }
    ));
    Ok(())
}

/// The private Answer route's rule for one operation (retrieval
/// `bounded_hybrid::require_answer_deadline`), restated by hand: a job's
/// deadline is ahead of now and at most one operation (120 s) away.
fn route_admits(now: Instant, deadline: Instant) -> bool {
    now < deadline && deadline.duration_since(now) <= Duration::from_secs(120)
}

/// Change-log entry 53: a card call made 250 s before a 300 s Answer's end
/// carried the Answer's deadline and the route refused it before dispatch.
/// Its lane deadline is now one operation away; the operation's own limit is
/// unchanged, and a nearer Answer deadline is kept.
#[test]
fn a_card_call_far_from_the_answer_deadline_gets_one_operation() {
    let now = Instant::now();
    for seconds in [250, 600] {
        let answer = now + Duration::from_secs(seconds);
        assert!(!route_admits(now, answer), "the old lane deadline");
        let lane = operation_deadline(answer, now);
        assert_eq!(lane, now + Duration::from_secs(120));
        assert!(route_admits(now, lane) && lane < answer, "{seconds} s");
        // Later in the same operation it still fits; it never exceeds one.
        assert!(route_admits(now + Duration::from_secs(119), lane));
    }
    let near = now + Duration::from_secs(60);
    assert_eq!(operation_deadline(near, now), near);
}