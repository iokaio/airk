// SPDX-License-Identifier: Apache-2.0
//! Private, bounded observations of actual development-fixture card calls.
use airk_core::{
    arbiter::answer_loop::CardAdvice,
    identity::{Incarnation, Sha256Digest},
    native_loop::{CardSegment, NativeCardRequest},
};
use airk_retrieval::arbiter::loop_cards::DecisionMode;
use anyhow::{Result, ensure};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::Relaxed},
};
use tokio::sync::mpsc;

pub(in crate::service_runtime) const MAX_RECORDS: usize = 512;
const QUEUE_CAPACITY: usize = 16;

#[derive(Default, Serialize)]
pub(super) struct ScoreEvidence {
    pub(super) compiled_input_digest: Option<Sha256Digest>,
    pub(super) scored_input_digest: Option<Sha256Digest>,
}

#[derive(Serialize)]
pub(in crate::service_runtime) struct Record {
    schema: &'static str,
    request_id: Incarnation,
    max_sub_needs: usize,
    decision_mode: Mode,
    segments: Vec<CardSegment>,
    roster_digests: Vec<Sha256Digest>,
    raw_score_bits: Vec<Vec<u32>>,
    advice: CardAdvice,
    score_evidence: ScoreEvidence,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Calibrated,
    TopScore,
}

impl Record {
    pub(super) fn settled(
        request_id: Incarnation,
        max_sub_needs: usize,
        segments: &[CardSegment],
        advice: &CardAdvice,
        mode: DecisionMode,
        score_evidence: ScoreEvidence,
    ) -> Result<Self> {
        ensure!(
            segments.len() == advice.segments.len()
                && segments
                    .iter()
                    .zip(&advice.segments)
                    .all(|(segment, outcome)| {
                        segment.ordinal == outcome.ordinal
                            && segment.card_slot == advice.card_slot
                            && (outcome.raw_scores.is_empty()
                                || outcome.raw_scores.len() == segment.cards.len())
                    }),
            "private card capture roster and advice differ"
        );
        ensure!(
            !score_evidence
                .compiled_input_digest
                .zip(score_evidence.scored_input_digest)
                .is_some_and(|(compiled, scored)| compiled != scored),
            "private card capture native compiled and scored digests differ"
        );
        Ok(Self {
            schema: "airk.private.development-card-call/1",
            request_id,
            max_sub_needs,
            decision_mode: match mode {
                DecisionMode::Calibrated => Mode::Calibrated,
                DecisionMode::TopScore => Mode::TopScore,
            },
            segments: segments.to_vec(),
            roster_digests: segments
                .iter()
                .map(CardSegment::roster_digest)
                .collect::<Result<_>>()?,
            raw_score_bits: advice
                .segments
                .iter()
                .map(|outcome| {
                    outcome
                        .raw_scores
                        .iter()
                        .map(|score| score.to_bits())
                        .collect()
                })
                .collect(),
            advice: advice.clone(),
            score_evidence,
        })
    }
}

#[derive(Default)]
struct Counts {
    offered: AtomicUsize,
    dropped: AtomicUsize,
}

#[derive(Clone)]
pub(in crate::service_runtime) struct Observer {
    sender: mpsc::Sender<Record>,
    counts: Arc<Counts>,
}

impl Observer {
    pub(in crate::service_runtime) fn channel() -> (Self, mpsc::Receiver<Record>) {
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        (
            Self {
                sender,
                counts: Arc::new(Counts::default()),
            },
            receiver,
        )
    }

    pub(in crate::service_runtime) fn dropped(&self) -> usize {
        self.counts.dropped.load(Relaxed)
    }

    pub(super) fn offer(
        &self,
        request: &NativeCardRequest,
        advice: &CardAdvice,
        mode: DecisionMode,
        evidence: ScoreEvidence,
    ) {
        self.offer_with(|| {
            Record::settled(
                request.read_attempt.request_id,
                request.parent_budget.max_sub_needs,
                &request.segments,
                advice,
                mode,
                evidence,
            )
        });
    }

    pub(super) fn offer_with(&self, build: impl FnOnce() -> Result<Record>) {
        if self.counts.offered.fetch_add(1, Relaxed) < MAX_RECORDS
            && let Ok(permit) = self.sender.try_reserve()
            && let Ok(record) = build()
        {
            permit.send(record);
        } else {
            self.counts.dropped.fetch_add(1, Relaxed);
        }
    }
}

#[test]
fn private_capture_reports_loss_without_constructing_payload() -> Result<()> {
    let (observer, receiver) = Observer::channel();
    let permits = (0..QUEUE_CAPACITY)
        .map(|_| observer.sender.try_reserve())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    observer.offer_with(|| panic!("a full channel must never build an evaluation record"));
    assert_eq!(observer.dropped(), 1);
    drop(permits);

    observer.counts.offered.store(MAX_RECORDS, Relaxed);
    observer.offer_with(|| panic!("the lifetime limit must never build an evaluation record"));
    assert_eq!(observer.dropped(), 2);

    observer.counts.offered.store(0, Relaxed);
    drop(receiver);
    observer.offer_with(|| panic!("a closed channel must never build an evaluation record"));
    assert_eq!(observer.dropped(), 3);
    Ok(())
}
