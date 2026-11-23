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
    /// The last card of every roster scores highest and is accepted; a Plan
    /// roster's last card is the reference, so Plan accepts the best
    /// proposals below it, up to the sub-need limit.
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
    /// Plan takes every proposal, whatever the request's sub-need limit.
    AllPlan,
    /// The native lane refuses the input before any model work, as it does
    /// when the compiled input passes its token reservation: advice with no
    /// observed tokens and every segment unavailable.
    Refused,
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
    /// The mode applies to calls of this task only; others are accepted.
    only: Option<airk_core::arbiter::Task>,
    /// Synthetic Focus scores spans at this path above every other span.
    prefer: Option<&'static str>,
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
        let paths: Vec<String> = request
            .candidates
            .iter()
            .map(|candidate| candidate.hit.citation.path.clone())
            .collect();
        let mut response = focus_scores(request, identity)?;
        if let (Some(prefer), Some(response)) = (self.prefer, response.as_mut()) {
            for pair in &mut response.ranking.pairs {
                if paths[usize::from(pair.candidate)] != prefer {
                    pair.yes_logit -= 3.0;
                }
            }
        }
        Ok(response)
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
        let mode = if self.only.is_none_or(|task| task == request.task) {
            self.mode
        } else {
            Mode::Accept
        };
        match mode {
            Mode::Decline => return Ok(None),
            Mode::Lost => anyhow::bail!("synthetic lost native outcome"),
            _ => {}
        }
        let refused = mode == Mode::Refused;
        let tokens = if refused {
            0
        } else {
            100 * request.attribution.len()
        };
        let segments = request
            .segments
            .iter()
            .map(|segment| {
                let count = segment.cards.len();
                if refused {
                    return v3::SegmentOutcome {
                        ordinal: segment.ordinal,
                        raw_scores: vec![],
                        outcome: Outcome::Unavailable {
                            reason: "refused before any model work".into(),
                        },
                        rule: None,
                        margin: None,
                    };
                }
                let scores: Vec<f32> = (0..count).map(|index| index as f32).collect();
                let top = count - 1;
                if mode == Mode::Abstain {
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
                // Plan's reference card (the last) is never choosable.
                let best = if segment.multiple { top - 1 } else { top };
                let chosen = if mode == Mode::NotTop { 0 } else { best };
                let digest = segment.cards[chosen].text_digest().unwrap();
                v3::SegmentOutcome {
                    ordinal: segment.ordinal,
                    raw_scores: scores,
                    outcome: Outcome::Accepted {
                        decision: if segment.multiple && mode == Mode::AllPlan {
                            // Every card, so the choice is still the top scores.
                            Decision::LoopCards(
                                segment.cards[..top]
                                    .iter()
                                    .map(|card| card.text_digest().unwrap())
                                    .collect(),
                            )
                        } else if segment.multiple && mode != Mode::NotTop {
                            // The highest-scoring proposals below the
                            // reference, up to the request's sub-need limit.
                            let take = request.parent_budget.max_sub_needs.min(top);
                            Decision::LoopCards(
                                (top - take..top)
                                    .rev()
                                    .map(|index| segment.cards[index].text_digest().unwrap())
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
            task_policy: if mode == Mode::Foreign {
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
                    tokens: if refused { 0 } else { 100 },
                    ..*charge
                })
                .collect(),
            work: if refused {
                Work::default()
            } else {
                Work {
                    input_tokens: tokens,
                    compile_us: Some(5),
                    inference_us: Some(7),
                    ..Work::default()
                }
            },
            segments,
        }))
    }
}

fn scored(mode: Mode) -> Result<(Fixture, Arc<Scorer>)> {
    scored_with(mode, false)
}

fn scored_with(mode: Mode, focus: bool) -> Result<(Fixture, Arc<Scorer>)> {
    scorer(mode, focus, None, None)
}

/// A scorer whose `mode` applies to calls of `task` only.
fn scored_on(mode: Mode, task: airk_core::arbiter::Task) -> Result<(Fixture, Arc<Scorer>)> {
    scorer(mode, false, Some(task), None)
}

fn scorer(
    mode: Mode,
    focus: bool,
    only: Option<airk_core::arbiter::Task>,
    prefer: Option<&'static str>,
) -> Result<(Fixture, Arc<Scorer>)> {
    let owner = Arc::new(Owner::new());
    let scorer = Arc::new(Scorer {
        owner: owner.clone(),
        mode,
        requests: Mutex::new(vec![]),
        profile: Sha256Digest::from_bytes([40; 32]),
        calls: AtomicU64::new(0),
        focus,
        deadlines: Mutex::new(vec![]),
        only,
        prefer,
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
        // Plan's reference card (the last) is a threshold, not a choice.
        let choosable = if entry.decision.stage == v3::LoopStage::Plan {
            entry.scores.len() - 1
        } else {
            entry.scores.len()
        };
        let argmax = (0..choosable)
            .max_by(|left, right| entry.scores[*left].total_cmp(&entry.scores[*right]))
            .unwrap();
        assert_eq!(entry.chosen[0], argmax as u8, "{:?}", entry.decision);
        if entry.decision.stage != v3::LoopStage::Plan {
            assert_eq!(entry.chosen.len(), 1);
        }
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
    // The model was present, so each roster it would have scored is logged
    // as a call not dispatched, with the model and the reason; a one-card
    // roster stays the no-model path's.
    let summary = &outcome.response.decision_log;
    let shed = summary.count(None, v3::SourceKind::NotDispatched);
    assert!(shed > 0);
    assert_eq!(
        shed + summary.count(None, v3::SourceKind::Deterministic),
        summary.total
    );
    for entry in &outcome.custody.log.entries {
        let decision = &entry.decision;
        if entry.cards.len() >= 2 {
            assert!(
                matches!(
                    decision.source,
                    v3::DecisionSource::NotDispatched {
                        reason: AssessmentUnavailable::Unavailable,
                        ..
                    }
                ),
                "{decision:?}"
            );
            assert_eq!(decision.model, Some(device::SelectedModel::ContextCompact));
        } else {
            assert_eq!(decision.source.kind(), v3::SourceKind::Deterministic);
            assert_eq!(decision.model, None);
        }
    }
    // No card call ran, so Context is reported absent, and each part's
    // rosters the model would have scored say they lost their share.
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

/// Review finding 4 of L3-10 (a): a delivered reply with card calls always
/// has its application on the reply's own scope, so its Context delivery
/// binding is built from the reply alone. A configuration change after a
/// card call never delivers (the refusal above), and a reply that claims
/// one with card work is refused by the binding owner.
#[test]
fn delivered_card_work_binds_context_to_its_own_application() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    let reply = answer(&fixture)?.response;
    assert!(scorer.calls.load(Ordering::Acquire) >= 1);
    assert!(reply.decision_log.card_calls > 0, "the reply made card calls");
    let bindings = airk_core::combined_context::reply_delivery_bindings(&reply)?;
    let context = bindings
        .iter()
        .find(|binding| binding.stage == airk_core::arbiter::Stage::Context)
        .context("a Context delivery binding")?;
    assert_eq!(context.scope, reply.scope);
    assert_eq!(context.profile, reply.applications[0].profile);
    assert!(context.profile.is_some());
    let mut changed = reply;
    changed.applications[0].scope.configuration_revision += 1;
    changed
        .degradations
        .push(v3::Degradation::ConfigurationChanged);
    assert!(
        airk_core::combined_context::reply_delivery_bindings(&changed).is_err(),
        "card work under another configuration has no delivery binding"
    );
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

fn answer_on(
    fixture: &Fixture,
    kernel: &crate::Kernel,
    request: v3::ContextRequestV3,
) -> Result<AnswerOutcome> {
    let publication = fixture.publication.clone();
    kernel.answer(
        request,
        Instant::now() + Duration::from_secs(600),
        &|| Ok(()),
        &|tool, arguments, _| canned(&publication, tool, arguments),
    )
}

/// A combined Answer member runs under its `PartitionAllowanceV3`: the
/// request budget is clamped to it, every card and Focus request carries it,
/// and the reply's work stays within it.
#[test]
fn a_combined_member_answer_stays_within_its_partition() -> Result<()> {
    use airk_core::native_loop::{ALLOWANCE_SCHEMA_V3, LoopResources, PartitionAllowanceV3};
    let (fixture, scorer) = scored_with(Mode::Accept, true)?;
    let baseline = answer_to(&fixture, request(QUESTION))?.response;
    assert!(
        baseline.decision_log.card_calls > 2,
        "unbounded uses more slots"
    );
    assert!(
        baseline.decision_log.focus_calls > 0,
        "unbounded runs Focus"
    );
    scorer.requests.lock().unwrap().clear();
    let attempt = airk_core::read_attempt::ReadAttemptBudget {
        schema: 1,
        request_id: airk_core::identity::Incarnation::from_bytes([10; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 100_000_001_000,
        remaining_ns: 100_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    let allowance = PartitionAllowanceV3 {
        schema: ALLOWANCE_SCHEMA_V3.into(),
        attempt: attempt.request_id,
        selection: Sha256Digest::from_bytes([41; 32]),
        index_id: baseline.scope.index_id,
        parent_budget: v3::BudgetV3::default(),
        resources: LoopResources {
            calls: 20,
            input_tokens: 16_384,
            candidates: 20,
            bytes: 65_536,
            focus_pairs: 0,
        },
        card_ordinals: vec![1, 2],
        focus_ordinals: vec![],
    };
    let mut foreign = allowance.clone();
    foreign.index_id = airk_core::index_catalog::IndexId::from_identity(
        airk_core::identity::Incarnation::from_bytes([99; 16]),
    );
    assert!(
        fixture
            .unattempted
            .with_answer_partition_for("airk_answer", foreign, attempt)
            .is_err(),
        "an allowance for another index is refused"
    );
    let member =
        fixture
            .unattempted
            .with_answer_partition_for("airk_answer", allowance.clone(), attempt)?;
    let request = request(QUESTION);
    let clamped = allowance.clamp(&request.budget)?;
    let outcome = answer_on(&fixture, &member, request)?;
    let reply = &outcome.response;
    reply.validate_with_log(&outcome.custody.log, &clamped)?;
    assert!(reply.work.calls <= 20 && reply.candidate_count <= 20);
    assert!(reply.work.input_tokens <= 16_384);
    assert!(reply.decision_log.card_calls <= 2 && reply.decision_log.card_calls > 0);
    assert!(reply.decision_log.focus_calls == 0 && reply.decision_log.focus_pairs == 0);
    let requests = scorer.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.partition.as_ref() == Some(&allowance)
                && request.parent_budget == clamped)
    );
    Ok(())
}

const EVIDENCE_QUESTION: &str = "where is `restart_guard` defined?";
const GUARD: &str = "pub fn restart_guard() {\n    budget_refusal();\n}\n";
const GUIDE: &str = "# Restart guard\nThe guard replays nothing.";
const GUARD_PATH: &str = "crates/k/src/guard.rs";
const GUIDE_PATH: &str = "docs/guard.md";

/// 9,000 bytes of ASCII source: the guard, then filler lines.
fn long_guard() -> String {
    let mut text = String::from(GUARD);
    while text.len() < 9000 {
        text.push_str("// filler line\n");
    }
    text.truncate(9000);
    text
}

/// 9,000 bytes of ASCII source in which the guard starts at byte 1,500,
/// after 100 filler lines.
fn late_guard() -> String {
    let mut text = "// filler line\n".repeat(100);
    text.push_str(GUARD);
    while text.len() < 9000 {
        text.push_str("// filler line\n");
    }
    text.truncate(9000);
    text
}

/// One Answer to `EVIDENCE_QUESTION` over reads in which every search finds
/// the same two hits (`guard` at the guard path, then the guide) and every
/// other read finds nothing. Its part plans a definition and a callers
/// sub-need of `restart_guard`, and both gather those two spans.
fn answer_over_two_hits(
    kernel: &crate::Kernel,
    publication: &airk_core::evidence::RevisionEnvelope,
    guard: &str,
) -> Result<AnswerOutcome> {
    answer_two_hits_to(kernel, publication, request(EVIDENCE_QUESTION), guard)
}

fn answer_two_hits_to(
    kernel: &crate::Kernel,
    publication: &airk_core::evidence::RevisionEnvelope,
    request: v3::ContextRequestV3,
    guard: &str,
) -> Result<AnswerOutcome> {
    let hits = json!([
        super::super::tests::hit(GUARD_PATH, 10, 40, guard),
        super::super::tests::hit(GUIDE_PATH, 1, 12, GUIDE)
    ]);
    answer_over_hits(kernel, publication, request, &hits)
}

/// One Answer over reads in which every search finds `hits`, in that order,
/// and every other read finds nothing.
fn answer_over_hits(
    kernel: &crate::Kernel,
    publication: &airk_core::evidence::RevisionEnvelope,
    request: v3::ContextRequestV3,
    hits: &Value,
) -> Result<AnswerOutcome> {
    kernel.answer(
        request,
        Instant::now() + Duration::from_secs(600),
        &|| Ok(()),
        &|tool, _, _| {
            let mut body = json!({"publication":publication,"generation":1,
                "collection":"fixture","truncated":false});
            match tool {
                "search" => body["hits"] = hits.clone(),
                "symbols" | "references" | "outline" | "imports" => body["facts"] = json!([]),
                _ => body["hits"] = json!([]),
            }
            Ok(body)
        },
    )
}

const GUARD_PLACE: (&str, usize, usize) = (GUARD_PATH, 10, 40);
const GUIDE_PLACE: (&str, usize, usize) = (GUIDE_PATH, 1, 12);

/// A row at `place` that shows `excerpt`, with `hidden` bytes of its span
/// before and after it.
fn shown(
    place: (&str, usize, usize),
    excerpt: &str,
    hidden: (usize, usize),
) -> airk_core::native_loop::CardEvidence {
    airk_core::native_loop::CardEvidence {
        path: place.0.into(),
        start_line: place.1,
        end_line: place.2,
        focus: None,
        excerpt: excerpt.into(),
        skipped_bytes: hidden.0,
        cut_bytes: hidden.1,
        truncated: false,
        same_as: None,
    }
}

/// A row at `place` whose text an earlier row of the call shows.
fn repeat(place: (&str, usize, usize)) -> airk_core::native_loop::CardEvidence {
    airk_core::native_loop::CardEvidence {
        same_as: Some(airk_core::native_loop::EvidencePlace {
            path: place.0.into(),
            start_line: place.1,
            end_line: place.2,
        }),
        ..shown(place, "", (0, 0))
    }
}

/// The first card call of `task`, which holds the round-one rosters of
/// `needs` of part 1.
fn first_call<'a>(
    requests: &'a [NativeCardRequest],
    task: airk_core::arbiter::Task,
    needs: &[u8],
) -> Result<&'a NativeCardRequest> {
    let call = requests
        .iter()
        .find(|request| request.task == task)
        .with_context(|| {
            format!(
                "a {task:?} card call among {:?}",
                requests
                    .iter()
                    .map(|request| request.task)
                    .collect::<Vec<_>>()
            )
        })?;
    assert_eq!(
        call.segments
            .iter()
            .map(|segment| (segment.part, segment.need, segment.round))
            .collect::<Vec<_>>(),
        needs.iter().map(|need| (1, *need, 1)).collect::<Vec<_>>()
    );
    Ok(call)
}

/// The first Coverage call: the part's two sub-needs in one call.
fn coverage_call(requests: &[NativeCardRequest]) -> Result<&NativeCardRequest> {
    first_call(requests, airk_core::arbiter::Task::ContextCoverage, &[1, 2])
}

/// The first Follow-up call. The two sub-needs name the same identifier and
/// read the same spans, so they propose the same reads; a read is offered
/// to one gap only, and the second sub-need has none left to choose from.
fn follow_up_call(requests: &[NativeCardRequest]) -> Result<&NativeCardRequest> {
    first_call(requests, airk_core::arbiter::Task::ContextFollowUp, &[1])
}

/// A byte range of one hit of the sub-needs' own search, as the log names it.
fn search_unit(
    outcome: &AnswerOutcome,
    hit: usize,
    range: (usize, usize),
) -> Result<airk_core::arbiter::context_action::EvidenceRef> {
    let record = outcome
        .custody
        .records
        .iter()
        .find(|record| {
            record.tool == "search"
                && record.arguments["query"] == "restart_guard"
                && record.arguments["full_chunks"] == false
        })
        .context("the sub-needs' own search")?;
    Ok(airk_core::arbiter::context_action::EvidenceRef {
        record: record.id,
        kind: airk_core::arbiter::context_action::SourceUnitKind::Hit,
        result_pointer: format!("/hits/{hit}"),
        utf8_start: range.0,
        utf8_end: range.1,
    })
}

/// The round-one decisions of `stage`, one a sub-need.
fn round_one(outcome: &AnswerOutcome, stage: v3::LoopStage) -> Vec<&v3::DecisionLogEntry> {
    outcome
        .custody
        .log
        .entries
        .iter()
        .filter(|entry| entry.decision.stage == stage && entry.decision.round == Some(1))
        .collect()
}

/// What a decision rests on: the presented and the omitted ranges, with the
/// counts the reply carries.
fn rests_on(
    entry: &v3::DecisionLogEntry,
    presented: &[airk_core::arbiter::context_action::EvidenceRef],
    omitted: &[airk_core::arbiter::context_action::EvidenceRef],
) {
    assert_eq!(entry.basis, presented, "{:?}", entry.decision);
    assert_eq!(entry.basis_omitted, omitted, "{:?}", entry.decision);
    assert_eq!(
        (
            usize::from(entry.decision.basis.presented),
            usize::from(entry.decision.basis.omitted)
        ),
        (presented.len(), omitted.len())
    );
}

#[test]
fn coverage_and_follow_up_calls_show_each_sub_need_what_was_read() -> Result<()> {
    use airk_core::arbiter::Task;
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, GUARD)?;
    let requests = scorer.requests.lock().unwrap();
    let call = coverage_call(&requests)?;
    assert_eq!(call.remaining_input_tokens, 8192);
    // The model is told what kind of sub-need each roster judges.
    let subjects = [
        "need definition: restart_guard",
        "need callers: restart_guard",
    ];
    assert_eq!(
        call.segments
            .iter()
            .map(|segment| segment.subject.as_str())
            .collect::<Vec<_>>(),
        subjects
    );
    // Both spans fit the allowance whole, in gathered order (no Focus ran).
    // The second sub-need gathered the same two spans: the call shows each
    // text once, and its rows name the rows that show them.
    let whole = [
        shown(GUARD_PLACE, GUARD, (0, 0)),
        shown(GUIDE_PLACE, GUIDE, (0, 0)),
    ];
    let repeats = [repeat(GUARD_PLACE), repeat(GUIDE_PLACE)];
    let read = [
        search_unit(&outcome, 0, (0, 49))?,
        search_unit(&outcome, 1, (0, 42))?,
    ];
    assert_eq!(call.segments[0].evidence, whole);
    assert_eq!(call.segments[1].evidence, repeats);
    for segment in &call.segments {
        assert_eq!((segment.unshown, segment.verdict), (0, None));
        assert_eq!(
            segment.cards[0].text,
            format!(
                "s{} covered: crates/k/src/guard.rs:10-40, docs/guard.md:1-12",
                segment.need
            ),
            "the Covered card cites the same locations"
        );
        let entry = &outcome.custody.log.entries[usize::from(segment.ordinal) - 1];
        assert_eq!(entry.decision.stage, v3::LoopStage::Coverage);
        assert_eq!(entry.decision.need, Some(segment.need));
        assert_eq!(entry.decision.source.kind(), v3::SourceKind::Model);
        rests_on(entry, &read, &[]);
    }
    // Follow-up: the scorer chose a read-more card for the sub-need, so it
    // is partly covered by the model's judgement. The roster says so, names
    // the sub-need with its kind and shows the same spans.
    let follow = follow_up_call(&requests)?;
    assert_eq!(follow.segments[0].subject, subjects[0]);
    assert_eq!(follow.segments[0].evidence, whole);
    for segment in &follow.segments {
        assert_eq!(
            (segment.unshown, segment.verdict),
            (
                0,
                Some(airk_core::native_loop::CardVerdict {
                    coverage: v3::Coverage::Partial,
                    by_model: true,
                })
            )
        );
        let entry = &outcome.custody.log.entries[usize::from(segment.ordinal) - 1];
        assert_eq!(entry.decision.stage, v3::LoopStage::FollowUp);
        assert_eq!(entry.decision.source.kind(), v3::SourceKind::Model);
        rests_on(entry, &read, &[]);
    }
    // Split and Plan rosters carry neither evidence nor a verdict.
    assert!(
        requests
            .iter()
            .filter(|request| matches!(request.task, Task::ContextSplit | Task::ContextPlan))
            .flat_map(|request| &request.segments)
            .all(|segment| segment.evidence.is_empty()
                && segment.unshown == 0
                && segment.verdict.is_none())
    );
    assert!(
        requests
            .iter()
            .any(|request| request.task == Task::ContextPlan)
    );
    Ok(())
}

#[test]
fn a_model_s_choice_rests_on_the_excerpts_it_was_shown() -> Result<()> {
    let long = long_guard();
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
    let requests = scorer.requests.lock().unwrap();
    // 8,192 reserved tokens leave 7,680 excerpt bytes. The call shows each
    // text once: the 42-byte guide whole, and 7,638 of the guard's 9,000
    // bytes. The second sub-need's rows repeat the first's.
    for call in [coverage_call(&requests)?, follow_up_call(&requests)?] {
        assert_eq!(call.remaining_input_tokens, 8192);
        assert_eq!(
            call.segments[0].evidence,
            [
                shown(GUARD_PLACE, &long[..7638], (0, 1362)),
                shown(GUIDE_PLACE, GUIDE, (0, 0)),
            ]
        );
        if let Some(second) = call.segments.get(1) {
            assert_eq!(second.evidence, [repeat(GUARD_PLACE), repeat(GUIDE_PLACE)]);
        }
        let total: usize = call
            .segments
            .iter()
            .flat_map(|segment| &segment.evidence)
            .map(|evidence| evidence.excerpt.len())
            .sum();
        assert_eq!(total, 7680, "exactly the allowance");
        // The log names the ranges shown, and the cut tail as omitted.
        for segment in &call.segments {
            let entry = &outcome.custody.log.entries[usize::from(segment.ordinal) - 1];
            assert_eq!(entry.decision.source.kind(), v3::SourceKind::Model);
            rests_on(
                entry,
                &[
                    search_unit(&outcome, 0, (0, 7638))?,
                    search_unit(&outcome, 1, (0, 42))?,
                ],
                &[search_unit(&outcome, 0, (7638, 9000))?],
            );
        }
    }
    Ok(())
}

/// The basis of a decision is what the decision that stands rests on. When
/// the model abstains, the lane refuses its input or its outcome is lost,
/// the deterministic order decides, and that order read the whole spans:
/// the excerpts the call was built with are not what the decision saw.
#[test]
fn a_decision_the_model_does_not_take_rests_on_the_whole_spans() -> Result<()> {
    use airk_core::arbiter::Task;
    let long = long_guard();
    for (mode, kind) in [
        (Mode::Abstain, v3::SourceKind::Abstained),
        (Mode::Refused, v3::SourceKind::Unavailable),
        (Mode::Lost, v3::SourceKind::Unavailable),
    ] {
        let (fixture, scorer) = scored_on(mode, Task::ContextCoverage)?;
        let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
        let requests = scorer.requests.lock().unwrap();
        // The call was built and sent with a cut excerpt.
        let call = coverage_call(&requests)?;
        assert_eq!(call.segments[0].evidence[0].cut_bytes, 1362);
        let whole = [
            search_unit(&outcome, 0, (0, 9000))?,
            search_unit(&outcome, 1, (0, 42))?,
        ];
        let coverage = round_one(&outcome, v3::LoopStage::Coverage);
        assert_eq!(coverage.len(), 2, "one decision a sub-need");
        for entry in &coverage {
            let decision = &entry.decision;
            assert_eq!(decision.source.kind(), kind, "{decision:?}");
            assert_eq!(decision.card_slot, Some(call.segments[0].card_slot));
            rests_on(entry, &whole, &[]);
        }
        let log = &outcome.custody.log;
        let follow = round_one(&outcome, v3::LoopStage::FollowUp);
        assert_eq!(follow.len(), 1, "one sub-need has reads left to choose");
        match mode {
            // A refusal before model work is a known outcome: its receipt
            // observed no tokens, nothing is charged for it, and the model
            // goes on to score the Follow-up rosters from their excerpts.
            Mode::Refused => {
                let receipt = log
                    .receipts
                    .iter()
                    .find(|receipt| receipt.task == Task::ContextCoverage)
                    .context("the refused call's receipt")?;
                assert_eq!(receipt.observed_input_tokens, Some(0));
                assert!(coverage.iter().all(|entry| {
                    entry.decision.receipt == Some(receipt.input_digest)
                        && !entry.decision.work_unknown
                }));
                for entry in &follow {
                    assert_eq!(entry.decision.source.kind(), v3::SourceKind::Model);
                    rests_on(
                        entry,
                        &[
                            search_unit(&outcome, 0, (0, 7638))?,
                            search_unit(&outcome, 1, (0, 42))?,
                        ],
                        &[search_unit(&outcome, 0, (7638, 9000))?],
                    );
                }
            }
            // A lost outcome blocks the model: the Follow-up calls are not
            // dispatched, and the log names the model and says why.
            Mode::Lost => {
                assert!(coverage.iter().all(|entry| entry.decision.work_unknown));
                for entry in &follow {
                    assert!(
                        matches!(
                            entry.decision.source,
                            v3::DecisionSource::NotDispatched {
                                fallback: v3::DeterministicRule::FirstExtractedTerm,
                                reason: AssessmentUnavailable::Unavailable,
                            }
                        ),
                        "{:?}",
                        entry.decision
                    );
                    assert_eq!(
                        entry.decision.model,
                        Some(device::SelectedModel::ContextCompact)
                    );
                    rests_on(entry, &whole, &[]);
                }
                assert!(
                    !requests
                        .iter()
                        .any(|request| request.task == Task::ContextFollowUp),
                    "no Follow-up call was sent"
                );
            }
            // The Follow-up roster of an abstained Coverage decision states
            // the deterministic verdict.
            _ => {
                let call = follow_up_call(&requests)?;
                assert!(call.segments.iter().all(|segment| {
                    segment.verdict
                        == Some(airk_core::native_loop::CardVerdict {
                            coverage: v3::Coverage::Partial,
                            by_model: false,
                        })
                }));
            }
        }
    }
    Ok(())
}

/// The tokens the calls before the first Coverage call settle: 100 for each
/// part and round a call is charged to (the synthetic scorer's count).
fn tokens_before_coverage(requests: &[NativeCardRequest]) -> usize {
    requests
        .iter()
        .take_while(|request| request.task != airk_core::arbiter::Task::ContextCoverage)
        .map(|request| 100 * request.attribution.len())
        .sum()
}

/// A call whose reservation leaves its spans less than the least excerpt is
/// not made: the deterministic order decides and the log says the model's
/// share ran out. One token more of reservation and the call is made.
#[test]
fn a_reservation_too_small_to_show_the_spans_makes_no_call() -> Result<()> {
    use airk_core::arbiter::Task;
    let long = long_guard();
    let (fixture, scorer) = scored(Mode::Accept)?;
    answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
    let before = tokens_before_coverage(&scorer.requests.lock().unwrap());
    assert!(before > 0, "Plan ran before Coverage");
    // The reservation left for the Coverage call, and the bytes of the long
    // span it then shows. The guide's 42 bytes are always shown whole.
    // - 2,048 tokens are the frame: no excerpt bytes at all.
    // - 2,286 tokens allow 297 bytes, 255 for the long span: below the least.
    // - 2,287 tokens allow 298 bytes, 256 for the long span: the least.
    for (reserve, long_bytes) in [
        (2048, None),
        (2286, None),
        (2287, Some(256)),
        (2300, Some(273)),
    ] {
        let (fixture, scorer) = scored(Mode::Accept)?;
        let mut small = request(EVIDENCE_QUESTION);
        small.budget.input_tokens = before + reserve;
        small.budget.round_input_tokens = before + reserve;
        let outcome = answer_two_hits_to(&fixture.kernel, &fixture.publication, small, &long)?;
        let requests = scorer.requests.lock().unwrap();
        let coverage = round_one(&outcome, v3::LoopStage::Coverage);
        assert_eq!(coverage.len(), 2, "{reserve} tokens");
        let sent = requests
            .iter()
            .find(|request| request.task == Task::ContextCoverage);
        let Some(bytes) = long_bytes else {
            assert!(sent.is_none(), "{reserve} tokens: no Coverage call");
            for entry in coverage {
                let decision = &entry.decision;
                assert!(
                    matches!(
                        decision.source,
                        v3::DecisionSource::NotDispatched {
                            fallback: v3::DeterministicRule::TopFocusSpan,
                            reason: AssessmentUnavailable::TokenBudget,
                        }
                    ),
                    "{reserve} tokens: {decision:?}"
                );
                assert_eq!(decision.model, Some(device::SelectedModel::ContextCompact));
                assert_eq!(decision.card_slot, None, "no slot is used");
                rests_on(
                    entry,
                    &[
                        search_unit(&outcome, 0, (0, 9000))?,
                        search_unit(&outcome, 1, (0, 42))?,
                    ],
                    &[],
                );
            }
            // The reply counts them apart from the no-model path and says
            // the part's Coverage lost its model share.
            let summary = &outcome.response.decision_log;
            assert_eq!(
                summary.count(Some(v3::LoopStage::Coverage), v3::SourceKind::NotDispatched),
                2
            );
            assert!(outcome.response.degradations.iter().any(|item| matches!(
                item,
                v3::Degradation::ModelShare {
                    part: 1,
                    stage: v3::LoopStage::Coverage
                }
            )));
            continue;
        };
        let call = sent.with_context(|| format!("{reserve} tokens: a Coverage call"))?;
        assert_eq!(call.remaining_input_tokens, reserve);
        assert_eq!(
            call.segments[0].evidence,
            [
                shown(GUARD_PLACE, &long[..bytes], (0, 9000 - bytes)),
                shown(GUIDE_PLACE, GUIDE, (0, 0)),
            ],
            "{reserve} tokens"
        );
        assert!(
            coverage
                .iter()
                .all(|entry| entry.decision.source.kind() == v3::SourceKind::Model)
        );
    }
    Ok(())
}

/// The search returned the guide before the guard. Focus scored the guard
/// above the guide, and scored the guard from the line that names it, at
/// byte 1,500 of 9,000. The evidence follows the Focus rank, and the guard's
/// excerpt holds the scored text and uses its whole share: the 7,638 bytes
/// that end with the span. The test fails if ranking does nothing.
#[test]
fn evidence_follows_the_focus_rank_and_holds_the_scored_text() -> Result<()> {
    let late = late_guard();
    assert_eq!(late.find(GUARD), Some(1500));
    let (fixture, scorer) = scorer(Mode::Accept, true, None, Some(GUARD_PATH))?;
    let hits = json!([
        super::super::tests::hit(GUIDE_PATH, 1, 12, GUIDE),
        super::super::tests::hit(GUARD_PATH, 10, 40, &late)
    ]);
    let outcome = answer_over_hits(
        &fixture.kernel,
        &fixture.publication,
        request(EVIDENCE_QUESTION),
        &hits,
    )?;
    let requests = scorer.requests.lock().unwrap();
    let call = coverage_call(&requests)?;
    let rows = &call.segments[0].evidence;
    assert_eq!(
        rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
        [GUARD_PATH, GUIDE_PATH],
        "the higher-scored span first, not the gathered order"
    );
    let focus: Vec<f32> = rows
        .iter()
        .map(|row| row.focus.context("Focus scored every span"))
        .collect::<Result<_>>()?;
    assert!(focus[0] > focus[1], "{focus:?}");
    assert!(focus.iter().all(|score| (0.0..=1.0).contains(score)));
    // The Covered card cites the spans in the same order.
    assert_eq!(
        call.segments[0].cards[0].text,
        "s1 covered: crates/k/src/guard.rs:10-40, docs/guard.md:1-12"
    );
    // The guide is short and shown whole, which leaves the guard 7,638 of
    // the 7,680 bytes. Its scored text starts at byte 1,500, and 7,500
    // bytes follow it. The window therefore starts 138 bytes earlier, at
    // byte 1,362, and no byte of the share is unused.
    assert_eq!(
        (
            rows[0].excerpt.as_str(),
            rows[0].skipped_bytes,
            rows[0].cut_bytes
        ),
        (&late[1362..], 1362, 0)
    );
    assert_eq!(
        (rows[1].excerpt.as_str(), rows[1].skipped_bytes),
        (GUIDE, 0)
    );
    // The log names the shown ranges in the same order, and the skipped
    // head as omitted. The guard is the search's second hit.
    let entry = &outcome.custody.log.entries[usize::from(call.segments[0].ordinal) - 1];
    rests_on(
        entry,
        &[
            search_unit(&outcome, 1, (1362, 9000))?,
            search_unit(&outcome, 0, (0, 42))?,
        ],
        &[search_unit(&outcome, 1, (0, 1362))?],
    );
    Ok(())
}

/// The no-model path builds no excerpt and makes no card call: its
/// decisions rest on the whole spans, as before, however long they are.
#[test]
fn without_a_model_the_log_names_whole_spans_and_nothing_is_cut() -> Result<()> {
    let long = long_guard();
    let fixture = super::super::tests::fixture()?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
    let log = &outcome.custody.log;
    assert!(log.receipts.is_empty(), "no card call was made");
    let whole = [
        search_unit(&outcome, 0, (0, 9000))?,
        search_unit(&outcome, 1, (0, 42))?,
    ];
    // One Coverage decision a sub-need; one Follow-up decision, for the one
    // sub-need with reads left to choose from.
    for (stage, rule, count) in [
        (
            v3::LoopStage::Coverage,
            v3::DeterministicRule::TopFocusSpan,
            2,
        ),
        (
            v3::LoopStage::FollowUp,
            v3::DeterministicRule::FirstExtractedTerm,
            1,
        ),
    ] {
        let decisions = round_one(&outcome, stage);
        assert_eq!(decisions.len(), count, "{stage:?}");
        for entry in decisions {
            assert_eq!(
                entry.decision.source,
                v3::DecisionSource::Deterministic { rule },
                "the no-model path, not a call that was not dispatched"
            );
            assert_eq!(entry.decision.model, None);
            rests_on(entry, &whole, &[]);
        }
    }
    let summary = &outcome.response.decision_log;
    assert_eq!(
        summary.count(None, v3::SourceKind::Deterministic),
        summary.total
    );
    Ok(())
}

#[test]
fn top_score_plan_is_accepted_when_the_reference_card_scores_highest() -> Result<()> {
    // In Accept mode every roster's last card scores highest; for Plan that
    // is the reference "no further sub-need", which no decision may choose.
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer(&fixture)?;
    let requests = scorer.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .any(|request| request.segments.iter().any(|segment| segment.multiple))
    );
    let plans: Vec<_> = outcome
        .custody
        .log
        .entries
        .iter()
        .filter(|entry| entry.decision.stage == v3::LoopStage::Plan)
        .collect();
    assert!(!plans.is_empty());
    for entry in plans {
        let reference = entry.scores.len() - 1;
        assert!(
            entry.scores[reference] > entry.scores[reference - 1],
            "the reference scores highest"
        );
        assert!(
            matches!(
                entry.decision.source,
                v3::DecisionSource::Model {
                    mode: v3::ModelMode::TopScore,
                    ..
                }
            ),
            "the top-score Plan advice is accepted: {:?}",
            entry.decision
        );
        assert_eq!(
            entry.chosen[0],
            (reference - 1) as u8,
            "the best proposal first"
        );
        assert!(
            !entry.chosen.contains(&(reference as u8)),
            "never the reference"
        );
        assert!(!entry.decision.work_unknown);
    }
    Ok(())
}

/// A hit of `path` at `lines` in the file whose content hash is `file`.
fn file_hit(path: &str, file: u8, lines: (usize, usize), text: &str) -> Value {
    json!({"citation":{"path":path,
            "content_hash":Sha256Digest::from_bytes([file; 32]).to_string(),
            "start_line":lines.0,"end_line":lines.1,
            "location":airk_core::DocumentLocation::Lines {start:lines.0,end:lines.1}},
        "text":text,"truncated":false})
}

/// The 29-byte lines `first..=last` of one file.
fn module_lines(first: usize, last: usize) -> String {
    (first..=last)
        .map(|number| format!("line {number:03} of the guard module\n"))
        .collect()
}

const VENDOR_PATH: &str = "vendor/guard.md";

/// Every search finds the same four hits: lines 10 to 40 of the guard, the
/// overlapping lines 30 to 60 of the same file, the guide, and a copy of
/// the guide at another place.
fn repeated_hits() -> Value {
    json!([
        file_hit(GUARD_PATH, 1, (10, 40), &module_lines(10, 40)),
        file_hit(GUARD_PATH, 1, (30, 60), &module_lines(30, 60)),
        file_hit(GUIDE_PATH, 2, (1, 12), GUIDE),
        file_hit(VENDOR_PATH, 3, (1, 12), GUIDE),
    ])
}

/// The rule "Answer does not repeat facts", on the reply. Two parts and
/// their sub-needs each run searches that find the same four hits. The
/// reply carries each text once, names that copy from every repeat, trims
/// the overlap of the two guard spans, lists the guide's other place, and
/// keeps every location. The no-model path and the model path do the same.
#[test]
fn the_reply_carries_each_text_once_and_keeps_every_location() -> Result<()> {
    let hits = repeated_hits();
    let plain = super::super::tests::fixture()?;
    let (scored, scorer) = scored(Mode::Accept)?;
    for (name, fixture) in [("no model", &plain), ("model", &scored)] {
        let outcome = answer_over_hits(
            &fixture.kernel,
            &fixture.publication,
            request(QUESTION),
            &hits,
        )?;
        let reply = &outcome.response;
        assert_eq!(reply.parts.len(), 2, "{name}");
        // Every span row of the reply, with its record.
        let rows: Vec<(Sha256Digest, &Value)> = reply
            .records
            .iter()
            .flat_map(|record| {
                record.canonical_result["spans"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|row| (record.id, row))
            })
            .collect();
        // No location is dropped: every search record still lists its four
        // hits, each with its place.
        let searches: Vec<_> = reply
            .records
            .iter()
            .filter(|record| record.operation == v3::LoopRead::Search)
            .collect();
        assert!(
            searches.len() >= 3,
            "{name}: the parts and sub-needs searched"
        );
        for record in &searches {
            let places: Vec<(&str, u64)> = record.canonical_result["spans"]
                .as_array()
                .context("spans")?
                .iter()
                .map(|row| {
                    (
                        row["path"].as_str().unwrap_or(""),
                        row["start_line"].as_u64().unwrap_or(0),
                    )
                })
                .collect();
            assert_eq!(
                places,
                [
                    (GUARD_PATH, 10),
                    (GUARD_PATH, 30),
                    (GUIDE_PATH, 1),
                    (VENDOR_PATH, 1)
                ],
                "{name}"
            );
            assert_eq!(record.canonical_result["hits_total"], 4);
        }
        // Each text is carried exactly once in the whole reply: the first
        // guard span whole, the second without the eleven lines (319 bytes)
        // they share, and the guide.
        let mut texts: Vec<&str> = rows
            .iter()
            .filter_map(|(_, row)| row["text"].as_str())
            .collect();
        texts.sort_unstable();
        let (first, rest) = (module_lines(10, 40), module_lines(41, 60));
        let mut expected = [first.as_str(), rest.as_str(), GUIDE];
        expected.sort_unstable();
        assert_eq!(texts, expected, "{name}");
        // The single copies are in the earliest search: no Focus ran, so
        // the earliest read and hit ranks highest.
        let carrier = searches[0].id;
        let at = |pointer: &str| json!({"record":carrier,"pointer":pointer});
        let mut repeats = 0;
        for (record, row) in &rows {
            let pointer = row["pointer"].as_str().context("pointer")?;
            let carries = *record == carrier;
            match (row["path"].as_str(), row["start_line"].as_u64()) {
                (Some(GUARD_PATH), Some(10)) if carries => {
                    assert_eq!(row["text"], first.as_str());
                    assert!(row.get("overlaps").is_none() && row.get("same_as").is_none());
                }
                (Some(GUARD_PATH), Some(30)) if carries => {
                    assert_eq!(row["text"], rest.as_str());
                    let mut shared = at("/hits/0");
                    shared["bytes"] = json!(319);
                    shared["side"] = json!("head");
                    assert_eq!(row["overlaps"], json!([shared]), "{name}");
                }
                (Some(GUIDE_PATH), _) if carries => {
                    assert_eq!(row["text"], GUIDE);
                    assert_eq!(row["also_at"], json!(["vendor/guard.md:1-12"]), "{name}");
                }
                // Every other row is a repeat: it has no text and names the
                // row that carries its text.
                (path, _) => {
                    repeats += 1;
                    assert!(row.get("text").is_none(), "{name}: {row}");
                    assert!(row.get("within").is_none(), "{name}: {row}");
                    let of = match (path, pointer) {
                        (Some(GUIDE_PATH | VENDOR_PATH), _) => "/hits/2",
                        (_, "/hits/0") => "/hits/0",
                        _ => "/hits/1",
                    };
                    assert_eq!(row["same_as"], at(of), "{name}: {row}");
                }
            }
        }
        // The reply says how many spans were merged and trimmed.
        assert!(repeats >= 9, "{name}: three searches or more, {repeats}");
        assert_eq!(reply.union.repeats_merged, repeats, "{name}");
        assert_eq!(reply.union.overlaps_trimmed, 1, "{name}");
        // The repeats came from both parts and from more than one sub-need.
        let uses: Vec<_> = reply
            .record_uses
            .iter()
            .filter(|item| searches.iter().any(|record| record.id == item.record))
            .collect();
        let parts: BTreeSet<u8> = uses.iter().map(|item| item.part).collect();
        let needs: BTreeSet<(u8, Option<u8>)> =
            uses.iter().map(|item| (item.part, item.need)).collect();
        assert_eq!(parts, BTreeSet::from([1, 2]), "{name}");
        assert!(needs.len() >= 3, "{name}: {needs:?}");
        // A search of a later round, where one ran, repeats nothing either:
        // the single copies above already account for every text.
        for item in uses.iter().filter(|item| item.round >= 2) {
            assert_ne!(
                item.record, carrier,
                "{name}: the carrier is a round-one read"
            );
        }
    }
    // The model saw each text once in a call as well: no call shows a text
    // in two rows.
    for request in scorer.requests.lock().unwrap().iter() {
        let shown: Vec<&str> = request
            .segments
            .iter()
            .flat_map(|segment| &segment.evidence)
            .filter(|row| row.same_as.is_none())
            .map(|row| row.excerpt.as_str())
            .collect();
        let distinct: BTreeSet<&str> = shown.iter().copied().collect();
        assert_eq!(shown.len(), distinct.len(), "{:?}", request.task);
    }
    Ok(())
}
