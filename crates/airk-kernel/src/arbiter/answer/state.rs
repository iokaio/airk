// SPDX-License-Identifier: Apache-2.0
//! Loop state: parts, their sub-needs and per-round provenance.
use super::coverage::Span;
use super::follow::Candidate;
use super::gather::ReadKey;
use super::retrieve::Need;
use super::*;

/// One round of one part, as the reply's provenance reports it.
#[derive(Clone, Debug, Default)]
pub(crate) struct RoundState {
    pub(crate) calls: usize,
    pub(crate) fanout: usize,
    pub(crate) new_evidence: usize,
    pub(crate) follow_ups_run: Vec<v3::FollowUp>,
    pub(crate) covered: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct NeedState {
    pub(crate) need: Need,
    pub(crate) coverage: v3::Coverage,
    /// A model's Coverage choice set the verdict at the latest barrier;
    /// otherwise the deterministic order did.
    pub(crate) coverage_by_model: bool,
    /// Record indexes this sub-need cites.
    pub(crate) evidence: Vec<usize>,
    /// The latest round's ranked spans.
    pub(crate) spans: Vec<Span>,
    /// Spans of every round, for Compose.
    pub(crate) history: Vec<Span>,
    pub(crate) tried: BTreeSet<ReadKey>,
    /// Round-one reads offered for this sub-need.
    pub(crate) reads: usize,
    /// The latest follow-up candidates, for the unrun list.
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) unrun: Vec<v3::UnrunRead>,
}
impl NeedState {
    pub(crate) fn new(need: Need) -> Self {
        Self {
            need,
            coverage: v3::Coverage::Missing,
            coverage_by_model: false,
            evidence: vec![],
            spans: vec![],
            history: vec![],
            tried: BTreeSet::new(),
            reads: 0,
            candidates: vec![],
            unrun: vec![],
        }
    }
    pub(crate) fn covered(&self) -> bool {
        self.coverage == v3::Coverage::Covered
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PartState {
    pub(crate) id: u8,
    /// The part's trimmed span inside its chosen Split range.
    pub(crate) span: (usize, usize),
    pub(crate) text: String,
    pub(crate) needs: Vec<NeedState>,
    pub(crate) rounds: Vec<RoundState>,
    pub(crate) stop: Option<v3::PartStop>,
    /// The stage a part stopped in early (deadline, budget, stale, refused).
    pub(crate) stopped_in: Option<v3::LoopStage>,
    pub(crate) shortened: bool,
    /// Follow-ups chosen at the last barrier, run in the next round.
    pub(crate) chosen: Vec<(u8, Candidate)>,
    /// The part's deadline share after hand-back.
    pub(crate) share_ms: u64,
}
impl PartState {
    pub(crate) fn new(id: u8, span: (usize, usize), text: &str) -> Self {
        Self {
            id,
            span,
            text: text.to_owned(),
            needs: vec![],
            rounds: vec![],
            stop: None,
            stopped_in: None,
            shortened: false,
            chosen: vec![],
            share_ms: 0,
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.stop.is_none()
    }
    pub(crate) fn round(&self) -> u8 {
        self.rounds.len() as u8
    }
    pub(crate) fn current(&mut self) -> &mut RoundState {
        if self.rounds.is_empty() {
            self.rounds.push(RoundState::default());
        }
        self.rounds.last_mut().expect("a round")
    }
    pub(crate) fn need(&mut self, id: u8) -> Option<&mut NeedState> {
        self.needs.iter_mut().find(|need| need.need.id == id)
    }

    /// The /2 gap kinds that remain: sufficient only when every sub-need is
    /// covered and the part stopped covered.
    pub(crate) fn gaps(&self) -> Vec<Gap> {
        if self.stop == Some(v3::PartStop::Covered) {
            return vec![Gap::Sufficient];
        }
        let mut gaps: Vec<Gap> = self
            .needs
            .iter()
            .filter(|need| !need.covered())
            .map(|need| need.need.kind.gap())
            .collect();
        gaps.sort_by_key(|gap| *gap as u8);
        gaps.dedup();
        if gaps.is_empty() {
            gaps.push(Gap::Unsupported);
        }
        gaps
    }
}
