// SPDX-License-Identifier: Apache-2.0
//! The model path with a synthetic scorer: batched card calls, abstention,
//! lost outcomes, foreign advice and configuration change. No model quality
//! or service transport is claimed.
use super::super::tests::{
    Fixture, Owner, PAIR_TOKENS, canned, fixture_with, focus_ready, focus_scores, request,
};
use super::*;
use airk_core::read_attempt::ReadClock;
use airk_core::service::{ScopedReadRef, WorkerIdentity};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The last card of every roster scores highest and is accepted.
    Accept,
    Abstain,
    /// The reply is lost after dispatch.
    Lost,
    /// The advice names another task policy.
    Foreign,
    /// Decline every call (no admitted card task).
    Decline,
    /// Accept the lowest-scoring card while claiming top score.
    NotTop,
    /// Plan takes every card, whatever the request's sub-need limit.
    AllPlan,
}

struct Scorer {
    owner: Arc<Owner>,
    mode: Mode,
    requests: Mutex<Vec<NativeCardRequest>>,
    profile: Sha256Digest,
    calls: AtomicU64,
    /// Synthetic Focus is ready beside the card scorer.
    focus: bool,
    deadlines: Mutex<Vec<Instant>>,
}

impl NativeController for Scorer {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut application = self.owner.application(worker, authority, publication)?;
        application.availability = Availability::Ready;
        application.profile = Some(self.profile);
        application.reason = "Synthetic scorer, not a qualified model".into();
        Ok(application)
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        Ok(Some((ReadClock::UnixMonotonic, 1000)))
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_core::arbiter::FocusApplication>> {
        if !self.focus {
            return Ok(None);
        }
        Ok(Some(focus_ready(self.owner.application(
            worker,
            authority,
            publication,
        )?)))
    }
    fn focus_batch(
        &self,
        request: airk_core::native_loop::NativeFocusRequestV3,
        _: Instant,
    ) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
        let identity = self
            .focus_application(
                request.worker,
                request.authority,
                &request.scope.publication,
            )?
            .and_then(|ready| ready.identity)
            .context("Focus is not ready")?;
        focus_scores(request, identity)
    }
    fn card_model(
        &self,
        _: WorkerIdentity,
        _: ScopedReadRef,
        _: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<LoopModel>> {
        Ok(Some(LoopModel {
            model: device::SelectedModel::ContextCompact,
            device: v3::ExecutionDevice::Cuda,
            precision: Some(device::DevicePrecision::Float16),
        }))
    }
    fn score_cards(
        &self,
        request: NativeCardRequest,
        deadline: Instant,
    ) -> Result<Option<v3::CardAdvice>> {
        self.deadlines.lock().unwrap().push(deadline);
        request.validate(
            request.worker,
            request.authority,
            ReadClock::UnixMonotonic,
            1000,
        )?;
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.requests.lock().unwrap().push(request.clone());
        match self.mode {
            Mode::Decline => return Ok(None),
            Mode::Lost => anyhow::bail!("synthetic lost native outcome"),
            _ => {}
        }
        let tokens = 100 * request.attribution.len();
        let segments = request
            .segments
            .iter()
            .map(|segment| {
                let count = segment.cards.len();
                let scores: Vec<f32> = (0..count).map(|index| index as f32).collect();
                let top = count - 1;
                if self.mode == Mode::Abstain {
                    return v3::SegmentOutcome {
                        ordinal: segment.ordinal,
                        raw_scores: scores,
                        outcome: Outcome::Abstained {
                            reason: "zero-rule calibration".into(),
                        },
                        rule: None,
                        margin: None,
                    };
                }
                let chosen = if self.mode == Mode::NotTop { 0 } else { top };
                let digest = segment.cards[chosen].text_digest().unwrap();
                v3::SegmentOutcome {
                    ordinal: segment.ordinal,
                    raw_scores: scores,
                    outcome: Outcome::Accepted {
                        decision: if segment.multiple && self.mode == Mode::AllPlan {
                            // Every card, so the choice is still the top scores.
                            Decision::LoopCards(
                                segment
                                    .cards
                                    .iter()
                                    .map(|card| card.text_digest().unwrap())
                                    .collect(),
                            )
                        } else if segment.multiple {
                            Decision::LoopCards(vec![digest])
                        } else {
                            Decision::LoopCard(digest)
                        },
                    },
                    rule: None,
                    margin: Some(top as f32 - (top as f32 - 1.0)),
                }
            })
            .collect();
        Ok(Some(v3::CardAdvice {
            schema: v3::CARD_ADVICE_SCHEMA.into(),
            task: request.task,
            profile: self.profile,
            runtime: format!(
                "onnxruntime-cuda/1.30.0/device-execution/{}",
                "a".repeat(64)
            ),
            compiler: Sha256Digest::from_bytes([41; 32]),
            calibration: Sha256Digest::from_bytes([42; 32]),
            policy: Sha256Digest::from_bytes([43; 32]),
            task_policy: if self.mode == Mode::Foreign {
                Sha256Digest::from_bytes([44; 32])
            } else {
                request.task_policy
            },
            scope: request.scope.clone(),
            input_digest: request.input_digest()?,
            card_slot: request.segments[0].card_slot,
            reserved_input_tokens: request.remaining_input_tokens,
            observed_input_tokens: Some(tokens),
            attribution: request
                .attribution
                .iter()
                .map(|charge| v3::TokenAttribution {
                    tokens: 100,
                    ..*charge
                })
                .collect(),
            work: Work {
                input_tokens: tokens,
                compile_us: Some(5),
                inference_us: Some(7),
                ..Work::default()
            },
            segments,
        }))
    }
}

fn scored(mode: Mode) -> Result<(Fixture, Arc<Scorer>)> {
    scored_with(mode, false)
}

fn scored_with(mode: Mode, focus: bool) -> Result<(Fixture, Arc<Scorer>)> {
    let owner = Arc::new(Owner::new());
    let scorer = Arc::new(Scorer {
        owner: owner.clone(),
        mode,
        requests: Mutex::new(vec![]),
        profile: Sha256Digest::from_bytes([40; 32]),
        calls: AtomicU64::new(0),
        focus,
        deadlines: Mutex::new(vec![]),
    });
    Ok((fixture_with(scorer.clone(), owner)?, scorer))
}

const QUESTION: &str =
    "1. explain the restart guard replay policy\n2. which tests cover the retry budget refusal?";

fn answer(fixture: &Fixture) -> Result<AnswerOutcome> {
    answer_to(fixture, request(QUESTION))
}

fn answer_to(fixture: &Fixture, request: v3::ContextRequestV3) -> Result<AnswerOutcome> {
    let publication = fixture.publication.clone();
    fixture.kernel.answer(
        request,
        Instant::now() + Duration::from_secs(600),
        &|| Ok(()),
        &|tool, arguments, _| {
            let mut body = canned(&publication, tool, arguments)?;
            if tool == "search" {
                body["hits"] = json!([super::super::tests::hit(
                    "crates/k/src/other.rs",
                    1,
                    30,
                    "pub fn unrelated_helper() {}"
                )]);
            }
            Ok(body)
        },
    )
}

#[test]
fn batched_segments_argmax_per_segment() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer(&fixture)?;
    let log = &outcome.custody.log;
    let requests = scorer.requests.lock().unwrap();
    assert!(
        requests.iter().any(|request| request.segments.len() == 2),
        "two sub-needs share a call"
    );
    assert!(requests.iter().all(|request| request.segments.len() <= 2));
    let batched: Vec<_> = log
        .entries
        .iter()
        .filter(|entry| !entry.decision.batched_with.is_empty())
        .collect();
    assert!(!batched.is_empty());
    for entry in &batched {
        let peer = &log.entries[usize::from(entry.decision.batched_with[0]) - 1];
        assert_eq!(peer.decision.card_slot, entry.decision.card_slot);
        assert_eq!(peer.decision.receipt, entry.decision.receipt);
    }
    // Every model decision is its own segment's argmax, from the scores the
    // scorer returned for that segment.
    let mut model_decisions = 0;
    for entry in &log.entries {
        if entry.decision.source.kind() != v3::SourceKind::Model {
            continue;
        }
        model_decisions += 1;
        let argmax = (0..entry.scores.len())
            .max_by(|left, right| entry.scores[*left].total_cmp(&entry.scores[*right]))
            .unwrap();
        assert_eq!(entry.chosen, vec![argmax as u8], "{:?}", entry.decision);
    }
    assert!(model_decisions > 2);
    // The split roster's highest card is a two-part partition.
    let split = &log.entries[0];
    assert_eq!(split.decision.source.kind(), v3::SourceKind::Model);
    assert_eq!(
        outcome.response.parts.len(),
        split.cards[usize::from(split.chosen[0])].partition.len()
    );
    assert!(log.receipts.len() == outcome.response.decision_log.card_calls);
    assert!(
        !outcome
            .response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ContextAbsent))
    );
    assert!(outcome.response.work.input_tokens > 0);
    Ok(())
}

#[test]
fn abstained_or_none_falls_back_with_recorded_rule() -> Result<()> {
    let (fixture, _) = scored(Mode::Abstain)?;
    let outcome = answer(&fixture)?;
    let abstained = outcome
        .custody
        .log
        .entries
        .iter()
        .filter(|entry| matches!(entry.decision.source, v3::DecisionSource::Abstained { .. }))
        .count();
    assert!(abstained > 0);
    assert!(
        outcome
            .response
            .degradations
            .iter()
            .any(|item| matches!(item,
        v3::Degradation::ContextAbstained { count } if *count == abstained))
    );
    let (fixture, scorer) = scored(Mode::Decline)?;
    let outcome = answer(&fixture)?;
    assert_eq!(
        scorer.calls.load(Ordering::Acquire),
        1,
        "a declined call is not retried"
    );
    assert!(
        outcome
            .custody
            .log
            .entries
            .iter()
            .all(|entry| !entry.decision.scored_by_model())
    );
    assert_eq!(outcome.response.decision_log.card_calls, 0);
    // Every decision fell back, so Context is reported absent, and each
    // part's rosters the model would have scored say they lost their share.
    let degradations = &outcome.response.degradations;
    assert!(
        degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ContextAbsent))
    );
    assert!(
        degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ModelShare { .. }))
    );
    Ok(())
}

#[test]
fn unknown_model_outcome_never_replays() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Lost)?;
    let outcome = answer(&fixture)?;
    assert_eq!(scorer.calls.load(Ordering::Acquire), 1);
    let unknown: Vec<_> = outcome
        .custody
        .log
        .entries
        .iter()
        .filter(|entry| entry.decision.work_unknown)
        .collect();
    assert!(!unknown.is_empty());
    assert!(
        unknown
            .iter()
            .all(|entry| entry.decision.card_slot == Some(1) && entry.decision.receipt.is_none())
    );
    assert!(
        outcome
            .response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ModelOutcomeUnknown))
    );
    assert!(
        outcome.response.work.input_tokens >= v3::CONTEXT_CALL_TOKENS,
        "the reservation stays charged"
    );
    Ok(())
}

#[test]
fn foreign_policy_or_scope_advice_refused() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Foreign)?;
    let outcome = answer(&fixture)?;
    assert_eq!(scorer.calls.load(Ordering::Acquire), 1);
    assert!(
        outcome.custody.log.receipts.is_empty(),
        "foreign advice is never kept as a receipt"
    );
    let split = &outcome.custody.log.entries[0];
    assert!(matches!(
        split.decision.source,
        v3::DecisionSource::Unavailable { .. }
    ));
    assert!(split.decision.work_unknown);
    Ok(())
}

#[test]
fn configuration_change_after_native_work_is_refused_without_replay() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    fixture.owner.change_after.store(6, Ordering::Release);
    let error = answer(&fixture).unwrap_err();
    let stale = error
        .downcast_ref::<AnswerPostExecutionStale>()
        .context("a stale refusal")?;
    assert_eq!(stale.receipt()["error"]["retryable"], false);
    assert!(scorer.calls.load(Ordering::Acquire) >= 1);
    Ok(())
}

#[test]
fn focus_and_card_tokens_share_one_pool() -> Result<()> {
    let (fixture, _) = scored_with(Mode::Accept, true)?;
    let mut small = request(QUESTION);
    small.budget.input_tokens = 8 * PAIR_TOKENS;
    small.budget.round_input_tokens = 8 * PAIR_TOKENS;
    let outcome = answer_to(&fixture, small)?;
    let log = &outcome.custody.log;
    assert!(!log.focus_receipts.is_empty(), "Focus ran");
    assert!(!log.receipts.is_empty(), "cards ran");
    assert!(outcome.response.work.input_tokens <= 8 * PAIR_TOKENS);
    Ok(())
}

#[test]
fn top_score_advice_must_choose_the_highest_card() -> Result<()> {
    let (fixture, scorer) = scored(Mode::NotTop)?;
    let outcome = answer(&fixture)?;
    assert_eq!(scorer.calls.load(Ordering::Acquire), 1, "no further calls");
    let log = &outcome.custody.log;
    assert!(log.receipts.is_empty(), "the advice is not kept");
    assert!(matches!(
        log.entries[0].decision.source,
        v3::DecisionSource::Unavailable { .. }
    ));
    assert!(log.entries[0].decision.work_unknown);
    Ok(())
}

#[test]
fn plan_advice_beyond_the_sub_need_limit_is_refused() -> Result<()> {
    let (fixture, scorer) = scored(Mode::AllPlan)?;
    let mut limited = request(QUESTION);
    limited.budget.max_sub_needs = 1;
    let outcome = answer_to(&fixture, limited)?;
    let requests = scorer.requests.lock().unwrap();
    assert!(
        requests.iter().any(|request| request
            .segments
            .iter()
            .any(|segment| segment.multiple && segment.cards.len() >= 2)),
        "a Plan roster offered more than one card"
    );
    for entry in &outcome.custody.log.entries {
        if entry.decision.stage == v3::LoopStage::Plan {
            assert!(entry.chosen.len() <= 1, "{:?}", entry.decision);
        }
    }
    assert!(
        outcome
            .response
            .parts
            .iter()
            .all(|part| part.sub_needs.len() <= 1)
    );
    Ok(())
}

#[test]
fn model_calls_end_before_the_compose_reserve() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    let before = Instant::now();
    answer(&fixture)?;
    let deadlines = scorer.deadlines.lock().unwrap();
    assert!(!deadlines.is_empty());
    // The default 300 s deadline less the two-part Compose and Union
    // reserve (9 s + 1 s + 2 x 250 ms).
    for deadline in deadlines.iter() {
        assert!(*deadline < before + Duration::from_secs(295));
    }
    Ok(())
}

#[test]
fn card_slots_follow_each_parts_share() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    let mut tight = request(
        "1. where is `restart_guard` defined, who calls it, which tests cover it and where is it documented?\n2. explain the retry budget refusal",
    );
    tight.budget.context_calls = 5;
    tight.budget.rounds = 1;
    let outcome = answer_to(&fixture, tight)?;
    let log = &outcome.custody.log;
    assert_eq!(outcome.response.parts.len(), 2);
    // A slot is charged to the part of its first segment.
    let mut first: BTreeMap<u8, Option<u8>> = BTreeMap::new();
    for entry in &log.entries {
        if let Some(slot) = entry.decision.card_slot {
            first.entry(slot).or_insert(entry.decision.part);
        }
    }
    let charged = |part: u8| first.values().filter(|owner| **owner == Some(part)).count();
    // Split takes one slot; the other four are two a part.
    assert!(charged(1) <= 2 && charged(2) <= 2, "{first:?}");
    assert!(charged(2) >= 1, "part 2 is not starved");
    let coverage_one = log
        .entries
        .iter()
        .filter(|entry| {
            entry.decision.stage == v3::LoopStage::Coverage && entry.decision.part == Some(1)
        })
        .count();
    assert!(coverage_one >= 3, "part 1 had more rosters than its share");
    // A call never batches two parts' rosters.
    for request in scorer.requests.lock().unwrap().iter() {
        let parts: BTreeSet<u8> = request
            .segments
            .iter()
            .map(|segment| segment.part)
            .collect();
        assert_eq!(parts.len(), 1, "{parts:?}");
    }
    Ok(())
}
