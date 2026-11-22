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
                // Plan's reference card (the last) is never choosable.
                let best = if segment.multiple { top - 1 } else { top };
                let chosen = if self.mode == Mode::NotTop { 0 } else { best };
                let digest = segment.cards[chosen].text_digest().unwrap();
                v3::SegmentOutcome {
                    ordinal: segment.ordinal,
                    raw_scores: scores,
                    outcome: Outcome::Accepted {
                        decision: if segment.multiple && self.mode == Mode::AllPlan {
                            // Every card, so the choice is still the top scores.
                            Decision::LoopCards(
                                segment.cards[..top]
                                    .iter()
                                    .map(|card| card.text_digest().unwrap())
                                    .collect(),
                            )
                        } else if segment.multiple && self.mode != Mode::NotTop {
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

/// One Answer to `EVIDENCE_QUESTION` over reads in which every search finds
/// the same two hits (`guard` at the guard path, then the guide) and every
/// other read finds nothing. Its part plans a definition and a callers
/// sub-need of `restart_guard`, and both gather those two spans.
fn answer_over_two_hits(
    kernel: &crate::Kernel,
    publication: &airk_core::evidence::RevisionEnvelope,
    guard: &str,
) -> Result<AnswerOutcome> {
    kernel.answer(
        request(EVIDENCE_QUESTION),
        Instant::now() + Duration::from_secs(600),
        &|| Ok(()),
        &|tool, _, _| {
            let mut body = json!({"publication":publication,"generation":1,
                "collection":"fixture","truncated":false});
            match tool {
                "search" => {
                    body["hits"] = json!([
                        super::super::tests::hit(GUARD_PATH, 10, 40, guard),
                        super::super::tests::hit(GUIDE_PATH, 1, 12, GUIDE)
                    ]);
                }
                "symbols" | "references" | "outline" | "imports" => body["facts"] = json!([]),
                _ => body["hits"] = json!([]),
            }
            Ok(body)
        },
    )
}

fn shown(
    path: &str,
    lines: (usize, usize),
    excerpt: &str,
    cut_bytes: usize,
) -> airk_core::native_loop::CardEvidence {
    airk_core::native_loop::CardEvidence {
        path: path.into(),
        start_line: lines.0,
        end_line: lines.1,
        focus: None,
        excerpt: excerpt.into(),
        cut_bytes,
    }
}

/// The first Coverage card call: the part's two sub-needs in one call.
fn coverage_call(requests: &[NativeCardRequest]) -> Result<&NativeCardRequest> {
    let call = requests
        .iter()
        .find(|request| request.task == airk_core::arbiter::Task::ContextCoverage)
        .context("a Coverage card call")?;
    assert_eq!(
        call.segments
            .iter()
            .map(|segment| (segment.part, segment.need, segment.round))
            .collect::<Vec<_>>(),
        [(1, 1, 1), (1, 2, 1)]
    );
    Ok(call)
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

#[test]
fn coverage_calls_show_each_sub_need_its_spans_and_log_what_was_shown() -> Result<()> {
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, GUARD)?;
    let requests = scorer.requests.lock().unwrap();
    let call = coverage_call(&requests)?;
    assert_eq!(call.remaining_input_tokens, 8192);
    // The model is told what kind of sub-need each roster judges.
    assert_eq!(
        call.segments
            .iter()
            .map(|segment| segment.subject.as_str())
            .collect::<Vec<_>>(),
        [
            "need definition: restart_guard",
            "need callers: restart_guard"
        ]
    );
    // Both spans fit the allowance whole, in gathered order (no Focus ran).
    let whole = [
        shown(GUARD_PATH, (10, 40), GUARD, 0),
        shown(GUIDE_PATH, (1, 12), GUIDE, 0),
    ];
    for segment in &call.segments {
        assert_eq!(segment.evidence, whole, "sub-need {}", segment.need);
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
        assert_eq!(
            entry.basis,
            [
                search_unit(&outcome, 0, (0, 49))?,
                search_unit(&outcome, 1, (0, 42))?
            ]
        );
        assert!(entry.basis_omitted.is_empty());
        assert_eq!(
            (entry.decision.basis.presented, entry.decision.basis.omitted),
            (2, 0)
        );
    }
    // No other stage's roster carries evidence.
    assert!(
        requests
            .iter()
            .filter(|request| request.task != airk_core::arbiter::Task::ContextCoverage)
            .all(|request| request
                .segments
                .iter()
                .all(|segment| segment.evidence.is_empty()))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.task == airk_core::arbiter::Task::ContextFollowUp),
        "a Follow-up call ran"
    );
    Ok(())
}

#[test]
fn a_long_span_is_cut_to_the_excerpt_allowance_and_the_log_names_the_cut() -> Result<()> {
    let long = long_guard();
    let (fixture, scorer) = scored(Mode::Accept)?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
    let requests = scorer.requests.lock().unwrap();
    let call = coverage_call(&requests)?;
    // 8,192 reserved tokens leave 6,144 excerpt bytes for the call's four
    // spans, 1,536 each. The two 42-byte guide spans leave 2,988 over, which
    // go to the first cut span: 1,536 + 2,988 = 4,524 of its 9,000 bytes.
    assert_eq!(call.remaining_input_tokens, 8192);
    assert_eq!(
        call.segments[0].evidence,
        [
            shown(GUARD_PATH, (10, 40), &long[..4524], 4476),
            shown(GUIDE_PATH, (1, 12), GUIDE, 0),
        ]
    );
    assert_eq!(
        call.segments[1].evidence,
        [
            shown(GUARD_PATH, (10, 40), &long[..1536], 7464),
            shown(GUIDE_PATH, (1, 12), GUIDE, 0),
        ]
    );
    let total: usize = call
        .segments
        .iter()
        .flat_map(|segment| &segment.evidence)
        .map(|evidence| evidence.excerpt.len())
        .sum();
    assert_eq!(total, 6144, "exactly the allowance");
    // The log names the ranges shown, and the cut tails as omitted.
    for (segment, cut) in call.segments.iter().zip([4524, 1536]) {
        let entry = &outcome.custody.log.entries[usize::from(segment.ordinal) - 1];
        assert_eq!(
            entry.basis,
            [
                search_unit(&outcome, 0, (0, cut))?,
                search_unit(&outcome, 1, (0, 42))?
            ]
        );
        assert_eq!(
            entry.basis_omitted,
            [search_unit(&outcome, 0, (cut, 9000))?]
        );
        assert_eq!(
            (entry.decision.basis.presented, entry.decision.basis.omitted),
            (2, 1)
        );
    }
    Ok(())
}

#[test]
fn evidence_carries_each_span_s_focus_score_in_rank_order() -> Result<()> {
    let (fixture, scorer) = scored_with(Mode::Accept, true)?;
    answer_over_two_hits(&fixture.kernel, &fixture.publication, GUARD)?;
    let requests = scorer.requests.lock().unwrap();
    let call = coverage_call(&requests)?;
    for segment in &call.segments {
        let focus: Vec<f32> = segment
            .evidence
            .iter()
            .map(|evidence| evidence.focus.context("Focus scored every span"))
            .collect::<Result<_>>()?;
        assert_eq!(focus.len(), 2);
        assert!(focus[0] >= focus[1], "{focus:?}");
        assert!(focus.iter().all(|score| (0.0..=1.0).contains(score)));
    }
    Ok(())
}

/// The deterministic path builds no excerpt: its decisions rest on the whole
/// spans, as before, however long they are.
#[test]
fn without_a_model_the_log_names_whole_spans_and_nothing_is_cut() -> Result<()> {
    let long = long_guard();
    let fixture = super::super::tests::fixture()?;
    let outcome = answer_over_two_hits(&fixture.kernel, &fixture.publication, &long)?;
    let log = &outcome.custody.log;
    assert!(log.receipts.is_empty(), "no card call was made");
    let coverage: Vec<_> = log
        .entries
        .iter()
        .filter(|entry| {
            entry.decision.stage == v3::LoopStage::Coverage && entry.decision.round == Some(1)
        })
        .collect();
    assert_eq!(coverage.len(), 2, "one decision a sub-need");
    for entry in coverage {
        assert!(matches!(
            entry.decision.source,
            v3::DecisionSource::Deterministic {
                rule: v3::DeterministicRule::TopFocusSpan
            }
        ));
        assert_eq!(
            entry.basis,
            [
                search_unit(&outcome, 0, (0, 9000))?,
                search_unit(&outcome, 1, (0, 42))?
            ]
        );
        assert!(entry.basis_omitted.is_empty());
        assert_eq!(
            (entry.decision.basis.presented, entry.decision.basis.omitted),
            (2, 0)
        );
    }
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
