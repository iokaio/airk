// SPDX-License-Identifier: Apache-2.0
//! The model path with a synthetic scorer: batched card calls, abstention,
//! lost outcomes, foreign advice and configuration change. No model quality
//! or service transport is claimed.
use super::super::tests::{Fixture, Owner, canned, fixture_with, request};
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
}

struct Scorer {
    owner: Arc<Owner>,
    mode: Mode,
    requests: Mutex<Vec<NativeCardRequest>>,
    profile: Sha256Digest,
    calls: AtomicU64,
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
        _: Instant,
    ) -> Result<Option<v3::CardAdvice>> {
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
                let digest = segment.cards[top].text_digest().unwrap();
                v3::SegmentOutcome {
                    ordinal: segment.ordinal,
                    raw_scores: scores,
                    outcome: Outcome::Accepted {
                        decision: if segment.multiple {
                            Decision::LoopCards(vec![segment.cards[0].text_digest().unwrap()])
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
    let owner = Arc::new(Owner::new());
    let scorer = Arc::new(Scorer {
        owner: owner.clone(),
        mode,
        requests: Mutex::new(vec![]),
        profile: Sha256Digest::from_bytes([40; 32]),
        calls: AtomicU64::new(0),
    });
    Ok((fixture_with(scorer.clone(), owner)?, scorer))
}

fn answer(fixture: &Fixture) -> Result<AnswerOutcome> {
    let publication = fixture.publication.clone();
    fixture.kernel.answer(
        request("1. explain the restart guard replay policy\n2. which tests cover the retry budget refusal?"),
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
