// SPDX-License-Identifier: Apache-2.0
//! Per-stage counts and timings of one Answer's gathering (rule 7, Core
//! `GatheringReport`): reads by kind, private lookups, passages fetched,
//! offered and delivered, model judgments, and wall-clock time per stage.
use super::coverage::{Span, record_hits};
use super::gather::Gathering;
use super::state::PartState;
use super::*;
use std::collections::BTreeMap;
use v3::GatheringStageKind as Kind;

#[derive(Clone, Debug, Default)]
struct Stage {
    ran: bool,
    lookups: usize,
    offered: usize,
    judgments: usize,
    elapsed: Duration,
    skipped: Option<v3::GatheringSkip>,
}

/// The accumulator the loop updates as it moves between stages.
#[derive(Clone, Debug)]
pub(crate) struct Report {
    rules: Vec<v3::GatheringRule>,
    stages: BTreeMap<Kind, Stage>,
    /// The open stage, when it started and the judgments taken before it.
    open: Option<(Kind, Instant, usize)>,
}

impl Report {
    pub(crate) fn new(rules: Vec<v3::GatheringRule>) -> Self {
        Self {
            rules,
            stages: BTreeMap::new(),
            open: None,
        }
    }

    pub(crate) fn set_rules(&mut self, rules: Vec<v3::GatheringRule>) {
        self.rules = rules;
    }

    /// Close the open stage, if any, and open `kind`.
    pub(crate) fn begin(&mut self, kind: Kind, now: Instant, judgments: usize) {
        self.end(now, judgments);
        self.stages.entry(kind).or_default().ran = true;
        self.open = Some((kind, now, judgments));
    }

    /// Close the open stage, charging it the time and judgments since it
    /// opened.
    pub(crate) fn end(&mut self, now: Instant, judgments: usize) {
        if let Some((kind, started, before)) = self.open.take() {
            let stage = self.stages.entry(kind).or_default();
            stage.elapsed += now.saturating_duration_since(started);
            stage.judgments += judgments.saturating_sub(before);
        }
    }

    /// Why `kind` admits no read; the stage is reported even if it never
    /// began. A stage that did read reports no reason.
    pub(crate) fn skip(&mut self, kind: Kind, reason: v3::GatheringSkip) {
        let stage = self.stages.entry(kind).or_default();
        stage.ran = true;
        stage.skipped = Some(reason);
    }

    /// Whether `kind` began or was given a reason.
    pub(crate) fn has(&self, kind: Kind) -> bool {
        self.stages.get(&kind).is_some_and(|stage| stage.ran)
    }

    /// One private lookup charged as a call in `kind`.
    pub(crate) fn lookup(&mut self, kind: Kind) {
        self.stages.entry(kind).or_default().lookups += 1;
    }

    /// One sub-need's ranking was shown `spans`: each counts as offered in
    /// the stage whose read fetched it.
    pub(crate) fn offered(&mut self, spans: &[Span], gather: &Gathering) {
        for span in spans {
            let stage = gather.records[span.record].stage;
            self.stages.entry(stage).or_default().offered += 1;
        }
    }

    /// The wire report. `cited` gives each sub-need's delivered spans.
    pub(crate) fn build(
        &self,
        gather: &Gathering,
        parts: &[PartState],
        cited: impl Fn(&super::state::NeedState) -> Vec<Span>,
    ) -> v3::GatheringReport {
        let mut fetched: BTreeMap<Kind, usize> = BTreeMap::new();
        for record in &gather.records {
            if !record.failure {
                *fetched.entry(record.stage).or_default() += record_hits(record).len();
            }
        }
        let mut delivered: BTreeMap<Kind, usize> = BTreeMap::new();
        for part in parts {
            for need in &part.needs {
                let mut seen = BTreeSet::new();
                for span in cited(need) {
                    if seen.insert((span.record, span.pointer.clone())) {
                        *delivered
                            .entry(gather.records[span.record].stage)
                            .or_default() += 1;
                    }
                }
            }
        }
        let mut kinds: BTreeSet<Kind> = self
            .stages
            .iter()
            .filter(|(_, stage)| stage.ran || stage.lookups > 0)
            .map(|(kind, _)| *kind)
            .collect();
        kinds.extend(gather.taken.keys().map(|(kind, _)| *kind));
        let stages = kinds
            .into_iter()
            .map(|kind| {
                let stage = self.stages.get(&kind).cloned().unwrap_or_default();
                let reads: Vec<v3::GatheringReads> = gather
                    .taken
                    .iter()
                    .filter(|((stage, _), count)| *stage == kind && **count > 0)
                    .map(|((_, operation), count)| v3::GatheringReads {
                        operation: *operation,
                        count: *count,
                    })
                    .collect();
                v3::GatheringStage {
                    stage: kind,
                    skipped: stage.skipped.filter(|_| reads.is_empty()),
                    reads,
                    lookups: stage.lookups,
                    passages_fetched: fetched.get(&kind).copied().unwrap_or(0),
                    passages_offered: stage.offered,
                    passages_delivered: delivered.get(&kind).copied().unwrap_or(0),
                    model_judgments: stage.judgments,
                    elapsed_ms: stage.elapsed.as_millis() as u64,
                }
            })
            .collect();
        v3::GatheringReport {
            rules: self.rules.clone(),
            stages,
        }
    }
}
