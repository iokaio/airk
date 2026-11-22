// SPDX-License-Identifier: Apache-2.0
//! The settlement of one card call, driven by a fake lane: the ledger, the
//! lane admission result, the compile receipt and the completion are all
//! injected, and the outcome, ledger state and advice are checked together.
use super::*;
use airk_core::{
    arbiter::{
        ModelChoice,
        answer_loop::{
            BudgetV3, CardClass, LOOP_CALIBRATION_SCHEMA, LoopCalibration, LoopRule, LoopStage,
            SegmentOutcome, TokenAttribution,
        },
    },
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::IndexId,
    native_loop::{CardSegment, LoopCard},
    read_attempt::{ReadAttemptBudget, ReadClock},
};
use airk_inference::inference_engine::pool::arbiter::{
    AdmissionRefused, CompiledReceipt, PendingScore, Scored,
};
use airk_retrieval::arbiter::loop_cards::DecisionMode;

fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: 600_000_000_100,
        remaining_ns: 600_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
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
            card("A", CardClass::PlanNeed, "need definition: A"),
            card("B", CardClass::PlanNeed, "need definition: B"),
            card("C", CardClass::PlanNeed, "need definition: C"),
            card("D", CardClass::PlanNeed, "no further sub-need"),
        ],
        multiple: true,
        evidence: vec![],
    }
}
fn calibration(task: Task, rules: Vec<LoopRule>) -> Result<LoopCalibration> {
    let profile = airk_retrieval::arbiter::runtime::catalog_profiles()?
        .into_iter()
        .find(|profile| profile.model == ModelChoice::ContextQwenCompact)
        .context("a Context profile")?;
    Ok(LoopCalibration {
        schema: LOOP_CALIBRATION_SCHEMA.into(),
        model: profile.model,
        task,
        compiler: profile.compiler,
        task_policy: airk_retrieval::arbiter::loop_cards::task_policy(task)?,
        score_abi: profile.abi,
        assets: Default::default(),
        rules,
    })
}
fn rule(class: CardClass, minimum_raw_margin: f32) -> Vec<LoopRule> {
    vec![LoopRule {
        class,
        minimum_raw_margin,
    }]
}
fn budget() -> BudgetV3 {
    BudgetV3 {
        round_input_tokens: 8_192,
        ..BudgetV3::default()
    }
}
fn charge(round: u8, tokens: usize) -> TokenAttribution {
    TokenAttribution {
        part: 1,
        round,
        tokens,
    }
}
/// A ledger holding one 8,000-token reservation on part 1 and `round`.
fn reserved(round: u8) -> Result<(Mutex<loop_ledger::Ledger>, loop_ledger::Ticket)> {
    let ledger = Mutex::new(loop_ledger::Ledger::default());
    let ticket = ledger.lock().unwrap().reserve_card(
        attempt(),
        &budget(),
        1,
        &[charge(round, 8_000)],
        None,
        10,
    )?;
    Ok((ledger, ticket))
}
/// Whether part 1, round 1 can still take `tokens` in card slot `slot`.
fn room(ledger: &Mutex<loop_ledger::Ledger>, slot: u8, tokens: usize) -> bool {
    ledger
        .lock()
        .unwrap()
        .reserve_card(attempt(), &budget(), slot, &[charge(1, tokens)], None, 11)
        .is_ok()
}
fn advice(task: Task, round: u8, segments: &[CardSegment]) -> CardAdvice {
    CardAdvice {
        schema: CARD_ADVICE_SCHEMA.into(),
        task,
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
        reserved_input_tokens: 8_000,
        observed_input_tokens: None,
        attribution: vec![charge(round, 8_000)],
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
fn settle(
    ledger: &Mutex<loop_ledger::Ledger>,
    ticket: loop_ledger::Ticket,
    segments: &[CardSegment],
    calibration: &LoopCalibration,
    max_sub_needs: usize,
    advice: &mut CardAdvice,
    pending: Option<Result<PendingScore>>,
) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(settle_card_call(
            CardCall {
                ledger,
                ticket,
                stage: segments[0].stage,
                segments,
                calibration,
                mode: DecisionMode::Calibrated,
                max_sub_needs,
                reserved: 8_000,
                deadline: Instant::now() + Duration::from_secs(30),
            },
            advice,
            pending,
        ))
}
/// A lane job that compiled `tokens` and completed with `scores`.
fn completed(tokens: usize, scores: Vec<f32>) -> PendingScore {
    let (compiled_tx, compiled) = tokio::sync::oneshot::channel();
    let (completed_tx, completed) = tokio::sync::oneshot::channel();
    let _ = compiled_tx.send(CompiledReceipt {
        input_digest: Sha256Digest::from_bytes([20; 32]),
        input_tokens: tokens,
        queue_us: 1,
        compile_us: 2,
    });
    let _ = completed_tx.send(Ok(Scored {
        scores,
        input_digest: Sha256Digest::from_bytes([20; 32]),
        input_tokens: tokens,
        queue_us: 1,
        compile_us: 2,
        inference_us: 3,
    }));
    PendingScore {
        compiled,
        completed,
    }
}

#[test]
fn a_completed_call_settles_observed_tokens_and_selects_per_segment() -> Result<()> {
    let (ledger, ticket) = reserved(1)?;
    let segments = [coverage(1, 1), coverage(2, 2)];
    let calibration = calibration(Task::ContextCoverage, rule(CardClass::Covered, 0.5))?;
    let mut advice = advice(Task::ContextCoverage, 1, &segments);
    settle(
        &ledger,
        ticket,
        &segments,
        &calibration,
        4,
        &mut advice,
        Some(Ok(completed(3_000, vec![3.0, 1.0, 1.2, 1.0]))),
    )?;
    advice.validate()?;
    assert_eq!(advice.observed_input_tokens, Some(3_000));
    assert_eq!(advice.attribution, vec![charge(1, 3_000)]);
    assert!(matches!(
        advice.segments[0].outcome,
        Outcome::Accepted { .. }
    ));
    assert_eq!(advice.segments[0].margin, Some(2.0));
    assert!(
        matches!(advice.segments[1].outcome, Outcome::Abstained { .. }),
        "a 0.2 margin is below the 0.5 rule"
    );
    // The unused 5,000 tokens were refunded to part 1, round 1.
    assert!(room(&ledger, 2, 5_192));
    Ok(())
}

#[test]
fn a_refusal_before_queueing_releases_the_reservation() -> Result<()> {
    let segments = [coverage(1, 1)];
    let calibration = calibration(Task::ContextCoverage, rule(CardClass::Covered, 0.5))?;
    let refused: anyhow::Error =
        AdmissionRefused::synthetic(anyhow::anyhow!("Arbiter queue constrained")).into();
    for (pending, reason) in [
        (Some(Err(refused)), QUEUE_REFUSED),
        (None, RESIDENT_UNAVAILABLE),
    ] {
        let (ledger, ticket) = reserved(1)?;
        let mut advice = advice(Task::ContextCoverage, 1, &segments);
        settle(
            &ledger,
            ticket,
            &segments,
            &calibration,
            4,
            &mut advice,
            pending,
        )?;
        advice.validate()?;
        assert_eq!(advice.observed_input_tokens, Some(0));
        assert_eq!(advice.attribution, vec![charge(1, 0)]);
        assert!(matches!(
            &advice.segments[0].outcome,
            Outcome::Unavailable { reason: actual } if actual == reason
        ));
        assert!(room(&ledger, 2, 8_192), "the whole allowance is free again");
        assert!(!room(&ledger, 1, 1), "the refused slot stays used");
    }
    Ok(())
}

#[test]
fn a_lost_outcome_keeps_the_reservation_and_decides_nothing() -> Result<()> {
    let (ledger, ticket) = reserved(1)?;
    let segments = [coverage(1, 1)];
    let calibration = calibration(Task::ContextCoverage, rule(CardClass::Covered, 0.5))?;
    let mut advice = advice(Task::ContextCoverage, 1, &segments);
    let (compiled_tx, compiled) = tokio::sync::oneshot::channel();
    let (completed_tx, completed) = tokio::sync::oneshot::channel::<Result<Scored>>();
    drop((compiled_tx, completed_tx));
    settle(
        &ledger,
        ticket,
        &segments,
        &calibration,
        4,
        &mut advice,
        Some(Ok(PendingScore {
            compiled,
            completed,
        })),
    )?;
    advice.validate()?;
    assert_eq!(advice.observed_input_tokens, None);
    assert!(matches!(
        &advice.segments[0].outcome,
        Outcome::Unavailable { reason } if reason == UNKNOWN
    ));
    assert!(!room(&ledger, 2, 193), "the 8,000 tokens stay held");
    Ok(())
}

#[test]
fn plan_settlement_never_chooses_more_than_max_sub_needs() -> Result<()> {
    let segments = [plan()];
    let calibration = calibration(Task::ContextPlan, rule(CardClass::PlanNeed, 0.0))?;
    let (ledger, ticket) = reserved(0)?;
    let mut advice = advice(Task::ContextPlan, 0, &segments);
    settle(
        &ledger,
        ticket,
        &segments,
        &calibration,
        2,
        &mut advice,
        Some(Ok(completed(2_000, vec![3.0, 2.0, 2.5, 1.0]))),
    )?;
    advice.validate()?;
    let Outcome::Accepted {
        decision: Decision::LoopCards(cards),
    } = &advice.segments[0].outcome
    else {
        anyhow::bail!("the Plan decision was accepted");
    };
    assert_eq!(
        *cards,
        vec![
            segments[0].cards[0].text_digest()?,
            segments[0].cards[2].text_digest()?
        ]
    );
    Ok(())
}
