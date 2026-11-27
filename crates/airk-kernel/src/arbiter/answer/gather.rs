// SPDX-License-Identifier: Apache-2.0
//! The shared read owner of one Answer /3. Every part's reads are tagged
//! dispatches in one queue; a read two parts or sub-needs want is dispatched
//! once and each keeps its provenance. Records are retained in dispatch
//! order (`seq`), never completion order, so concurrency cannot change them.
use super::*;
use std::collections::BTreeMap;

/// Identity of one canonical read: tool, exact arguments and revision.
pub(crate) type ReadKey = Sha256Digest;

pub(crate) fn read_key(
    tool: &str,
    arguments: &Map<String, Value>,
    revision: &str,
) -> Result<ReadKey> {
    let arguments = canonical(&Value::Object(arguments.clone()));
    Ok(framed_digest(
        b"airk.arbiter.read-key/1",
        &[
            tool.as_bytes(),
            &serde_json::to_vec(&arguments)?,
            revision.as_bytes(),
        ],
    ))
}

/// Object keys sorted at every depth: argument order never splits a read.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> = map
                .iter()
                .map(|(key, value)| (key, canonical(value)))
                .collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key.clone(), value))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// Why a read was dispatched; the reply's `UseCause` adds `SharedRead`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Cause {
    PartSearch,
    /// A caller anchor's exact lookup: a part-level search of round one
    /// that, unlike the part search, may fail without refusing the request.
    Anchor,
    NeedSearch,
    Structural,
    FollowUp,
}
impl Cause {
    fn wire(self) -> v3::UseCause {
        match self {
            Self::PartSearch | Self::Anchor => v3::UseCause::PartSearch,
            Self::NeedSearch => v3::UseCause::NeedSearch,
            Self::Structural => v3::UseCause::StructuralRead,
            Self::FollowUp => v3::UseCause::FollowUp,
        }
    }
}

/// One finding of a read by a part and, except the part search, a sub-need.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Use {
    pub(crate) part: u8,
    pub(crate) need: Option<u8>,
    pub(crate) round: u8,
    pub(crate) cause: v3::UseCause,
    /// The user's own part search: its failure refuses that part.
    pub(crate) mandatory: bool,
    /// The cause this use would have dispatched the read with.
    pub(crate) intended: Cause,
}

/// What a read is for inside its sub-need's recipe (`retrieve`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// A read whose result plans nothing further.
    Leaf,
    /// The part search or a sub-need search: its hits are spans.
    Search,
    /// `symbols`: its definition locations are excerpted, then outlined.
    Definitions,
    /// `references`: caller and test locations are excerpted.
    References,
    /// A definition excerpt: its file is outlined next.
    Excerpt,
    /// A whole-chunk search for configuration and implementation needs.
    Chunks,
    /// `imports` of the top file; `symbols` follows.
    Imports,
    /// An error-string search: the context of its first hit follows.
    Literal,
}

#[derive(Clone, Debug)]
pub(crate) struct Dispatch {
    pub(crate) tool: &'static str,
    pub(crate) operation: v3::LoopRead,
    pub(crate) arguments: Map<String, Value>,
    pub(crate) revision: String,
    pub(crate) key: ReadKey,
    pub(crate) part: u8,
    pub(crate) need: Option<u8>,
    pub(crate) round: u8,
    pub(crate) cause: Cause,
    pub(crate) kind: software::FollowUp,
    pub(crate) step: Step,
    /// The identifier, path or query the read is about.
    pub(crate) subject: String,
    /// Deterministic dispatch order over the whole Answer.
    pub(crate) seq: u32,
}
impl Dispatch {
    fn use_of(&self) -> Use {
        Use {
            part: self.part,
            need: self.need,
            round: self.round,
            cause: self.cause.wire(),
            mandatory: self.cause == Cause::PartSearch,
            intended: self.cause,
        }
    }
    pub(crate) fn target(&self) -> String {
        let text = ["query", "name", "path"]
            .iter()
            .find_map(|key| self.arguments.get(*key).and_then(Value::as_str))
            .unwrap_or(self.tool);
        let mut target = match (
            self.arguments.get("start_line").and_then(Value::as_u64),
            self.arguments.get("end_line").and_then(Value::as_u64),
            self.arguments.get("line").and_then(Value::as_u64),
        ) {
            (Some(start), Some(end), _) => format!("{text}:{start}-{end}"),
            (_, _, Some(line)) => format!("{text}:{line}"),
            _ => text.to_owned(),
        };
        if target.len() > v3::MAX_TEXT_BYTES {
            let mut end = v3::MAX_TEXT_BYTES;
            while !target.is_char_boundary(end) {
                end -= 1;
            }
            target.truncate(end);
        }
        target
    }
}

/// A retained canonical read: its body, its uses and its kind.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub(crate) id: Sha256Digest,
    pub(crate) tool: &'static str,
    pub(crate) operation: v3::LoopRead,
    pub(crate) arguments: Map<String, Value>,
    /// The result without its volatile per-read status
    /// (Core `VOLATILE_READ_FIELDS`): what the id, `bytes`, the reply's
    /// `full_digest` and retention accounting bind (change-log entry 54).
    pub(crate) body: Value,
    /// The complete result as received, status included, for custody.
    pub(crate) received: Value,
    /// Serialized bytes of `body`.
    pub(crate) bytes: usize,
    pub(crate) kind: software::FollowUp,
    pub(crate) subject: String,
    pub(crate) seq: u32,
    pub(crate) uses: Vec<Use>,
    /// A canonical failure receipt of an optional read.
    pub(crate) failure: bool,
}
impl Record {
    pub(crate) fn used_by(&self, part: u8, need: Option<u8>, round: u8) -> bool {
        self.uses
            .iter()
            .any(|item| item.part == part && item.need == need && item.round == round)
    }
    fn attach(&mut self, item: Use) -> bool {
        if self
            .uses
            .iter()
            .any(|old| (old.part, old.need, old.round) == (item.part, item.need, item.round))
        {
            return false;
        }
        self.uses.push(item);
        true
    }
}

enum Entry {
    Queued(Vec<Use>),
    Pending(Vec<Use>),
    Retained(usize),
    Failed(Option<usize>),
    Omitted,
}

/// What the whole Answer stops on, before or beside the parts' own stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlobalStop {
    /// A read returned another publication: every part stops.
    Stale,
    /// Calls, candidates or retention bytes are exhausted.
    Budget,
    /// The deadline reserve for Compose and Union is reached.
    Deadline,
}

/// The result of one observed read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Observed {
    /// Retained as `records[index]`.
    Retained(usize),
    /// The read failed; its receipt is kept when one was returned. Every
    /// part whose own search it was is refused.
    Failed {
        record: Option<usize>,
        refused: Vec<u8>,
    },
    Stale,
    /// The retention budget could not keep it.
    Omitted,
}

/// What `offer` did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Offer {
    Queued,
    /// Another dispatch of the same read is queued or pending.
    Waiting,
    /// Served from a retained record.
    Cached(usize),
    /// The read already failed; its receipt, if any, records this use.
    Failed(Option<usize>),
    /// The read was omitted or failed without a receipt; nothing to serve.
    Unavailable,
}

pub(crate) struct Gathering {
    pub(crate) budget: v3::BudgetV3,
    pub(crate) revision: String,
    queue: VecDeque<Dispatch>,
    pending: BTreeMap<u32, Dispatch>,
    reads: BTreeMap<ReadKey, Entry>,
    pub(crate) records: Vec<Record>,
    pub(crate) calls: usize,
    /// Distinct reads observed, retained or omitted.
    pub(crate) candidates: usize,
    pub(crate) omitted: usize,
    pub(crate) bytes: usize,
    nomination_bytes: usize,
    nomination_candidates: usize,
    pub(crate) failed: usize,
    next_seq: u32,
    pub(crate) stop: Option<GlobalStop>,
}

impl Gathering {
    pub(crate) fn new(budget: v3::BudgetV3, scope: ScopeBinding) -> Result<Self> {
        budget.validate()?;
        let revision = scope.publication.token.to_string();
        Ok(Self {
            budget,
            revision,
            queue: VecDeque::new(),
            pending: BTreeMap::new(),
            reads: BTreeMap::new(),
            records: Vec::new(),
            calls: 0,
            candidates: 0,
            omitted: 0,
            bytes: 0,
            nomination_bytes: 0,
            nomination_candidates: 0,
            failed: 0,
            next_seq: 0,
            stop: None,
        })
    }

    /// A dispatch at this revision, numbered in offer order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dispatch(
        &mut self,
        (tool, operation): (&'static str, v3::LoopRead),
        arguments: Value,
        (part, need, round): (u8, Option<u8>, u8),
        cause: Cause,
        kind: software::FollowUp,
        step: Step,
        subject: &str,
    ) -> Result<Dispatch> {
        let arguments = arguments
            .as_object()
            .cloned()
            .context("Answer read arguments are not an object")?;
        let key = read_key(tool, &arguments, &self.revision)?;
        self.next_seq += 1;
        Ok(Dispatch {
            tool,
            operation,
            arguments,
            revision: self.revision.clone(),
            key,
            part,
            need,
            round,
            cause,
            kind,
            step,
            subject: subject.to_owned(),
            seq: self.next_seq,
        })
    }

    /// The state of a read, for de-duplicating proposals.
    pub(crate) fn known(&self, key: &ReadKey) -> bool {
        self.reads.contains_key(key)
    }

    /// Queue a read, or attach this use to the one already queued, pending
    /// or retained. A retained or failed read is served from the cache.
    pub(crate) fn offer(&mut self, dispatch: Dispatch) -> Offer {
        let mut item = dispatch.use_of();
        match self.reads.get_mut(&dispatch.key) {
            None => {
                self.reads.insert(dispatch.key, Entry::Queued(vec![item]));
                self.queue.push_back(dispatch);
                Offer::Queued
            }
            Some(Entry::Queued(uses) | Entry::Pending(uses)) => {
                item.cause = v3::UseCause::SharedRead;
                if !uses.iter().any(|old| {
                    (old.part, old.need, old.round) == (item.part, item.need, item.round)
                }) {
                    uses.push(item);
                }
                Offer::Waiting
            }
            Some(Entry::Retained(index)) => {
                let index = *index;
                item.cause = v3::UseCause::SharedRead;
                self.records[index].attach(item);
                Offer::Cached(index)
            }
            Some(Entry::Failed(record)) => {
                let record = *record;
                if let Some(index) = record {
                    item.cause = v3::UseCause::SharedRead;
                    self.records[index].attach(item);
                }
                Offer::Failed(record)
            }
            Some(Entry::Omitted) => Offer::Unavailable,
        }
    }

    pub(crate) fn queued_for(&self, part: u8) -> usize {
        self.queue.iter().filter(|item| item.part == part).count()
    }
    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Remove every queued read of `part`, for its unrun list. A read
    /// another part also waits on stays queued, re-tagged to the first
    /// remaining waiter, which then owns its dispatch.
    pub(crate) fn drain_part(&mut self, part: u8) -> Vec<Dispatch> {
        let mut drained = Vec::new();
        // A stopped part no longer waits on reads other parts queued, so it
        // can never be handed one of them later.
        for entry in self.reads.values_mut() {
            if let Entry::Queued(uses) = entry
                && uses.first().is_some_and(|first| first.part != part)
            {
                uses.retain(|use_| use_.part != part);
            }
        }
        for mut item in std::mem::take(&mut self.queue) {
            if item.part != part {
                self.queue.push_back(item);
                continue;
            }
            let others: Vec<Use> = match self.reads.remove(&item.key) {
                Some(Entry::Queued(uses)) => {
                    uses.into_iter().filter(|use_| use_.part != part).collect()
                }
                _ => vec![],
            };
            drained.push(item.clone());
            if let Some(first) = others.first() {
                item.part = first.part;
                item.need = first.need;
                item.round = first.round;
                item.cause = first.intended;
                let mut owned = others;
                owned[0].cause = item.cause.wire();
                self.reads.insert(item.key, Entry::Queued(owned));
                self.queue.push_back(item);
            }
        }
        drained
    }

    /// Up to `limit` of `part`'s queued reads, admitted within the call
    /// budget. Exhausting it stops the whole Answer on Budget.
    pub(crate) fn take(&mut self, part: u8, limit: usize) -> Vec<Dispatch> {
        let mut taken = Vec::new();
        while taken.len() < limit && self.stop.is_none() {
            let Some(position) = self.queue.iter().position(|item| item.part == part) else {
                break;
            };
            if self.calls >= self.budget.calls
                || self.admitted_candidates() >= self.budget.candidates
            {
                self.stop = Some(GlobalStop::Budget);
                break;
            }
            let dispatch = self.queue.remove(position).expect("queued read");
            if let Some(Entry::Queued(uses)) = self.reads.remove(&dispatch.key) {
                self.reads.insert(dispatch.key, Entry::Pending(uses));
            }
            self.calls += 1;
            self.pending.insert(dispatch.seq, dispatch.clone());
            taken.push(dispatch);
        }
        taken
    }

    /// Return an admitted read that was never started to the front of its
    /// queue; it is no longer counted as a call.
    pub(crate) fn release(&mut self, seq: u32) {
        if let Some(dispatch) = self.pending.remove(&seq) {
            if let Some(Entry::Pending(uses)) = self.reads.remove(&dispatch.key) {
                self.reads.insert(dispatch.key, Entry::Queued(uses));
            }
            self.calls -= 1;
            self.queue.push_front(dispatch);
        }
    }

    /// Observe one admitted read. The driver calls this in `seq` order.
    pub(crate) fn observe(&mut self, seq: u32, result: Result<Value>) -> Result<Observed> {
        ensure!(
            self.pending.keys().next() == Some(&seq),
            "Answer reads are observed in dispatch order"
        );
        let dispatch = self
            .pending
            .remove(&seq)
            .context("Answer observation has no admitted read")?;
        let uses = match self.reads.remove(&dispatch.key) {
            Some(Entry::Pending(uses)) => uses,
            _ => vec![dispatch.use_of()],
        };
        let body = match result {
            Ok(body) => body,
            Err(error) => {
                if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
                    return Err(error);
                }
                self.failed += 1;
                let receipt = error
                    .downcast_ref::<crate::workbench::ReadFailure>()
                    .map(|failure| failure.0.clone());
                let mut refused: Vec<u8> = uses
                    .iter()
                    .filter(|use_| use_.mandatory)
                    .map(|use_| use_.part)
                    .collect();
                refused.dedup();
                // A receipt that does not fit the retention bytes is not an
                // omitted record: the read failed either way, so the
                // failure is kept without its receipt and nothing stops.
                let index = match receipt {
                    Some(receipt) if self.fits(&receipt)? => {
                        self.retain(&dispatch, receipt, uses, true)?
                    }
                    _ => None,
                };
                self.reads.insert(dispatch.key, Entry::Failed(index));
                return Ok(Observed::Failed {
                    record: index,
                    refused,
                });
            }
        };
        ensure!(
            token(&body).is_some(),
            "canonical Answer read omitted its publication"
        );
        if token(&body) != Some(dispatch.revision.as_str()) {
            self.stop = Some(GlobalStop::Stale);
            self.reads.insert(dispatch.key, Entry::Omitted);
            return Ok(Observed::Stale);
        }
        match self.retain(&dispatch, body, uses, false)? {
            Some(index) => {
                self.reads.insert(dispatch.key, Entry::Retained(index));
                Ok(Observed::Retained(index))
            }
            None => {
                self.reads.insert(dispatch.key, Entry::Omitted);
                Ok(Observed::Omitted)
            }
        }
    }

    fn admitted_candidates(&self) -> usize {
        // The wire candidate count describes retained and omitted evidence.
        // Private probes spend the same admissions without becoming evidence.
        self.candidates + self.nomination_candidates + self.pending.len()
    }
    pub(crate) fn reserve_nomination(&mut self) -> bool {
        if self.stop.is_some()
            || self.budget.calls.saturating_sub(self.calls) < 2
            || self
                .budget
                .candidates
                .saturating_sub(self.admitted_candidates())
                < 2
        {
            return false;
        }
        self.calls += 1;
        self.nomination_candidates += 1;
        true
    }

    pub(crate) fn remaining_bytes(&self) -> usize {
        self.budget
            .max_bytes
            .saturating_sub(self.bytes.saturating_add(self.nomination_bytes))
    }

    pub(crate) fn charge_nomination_bytes(&mut self, bytes: usize) {
        self.nomination_bytes += bytes;
    }

    fn fits(&self, body: &Value) -> Result<bool> {
        let bytes = serde_json::to_vec(&super::volatile::stable(body))?.len();
        Ok(bytes <= self.remaining_bytes() && self.records.len() < self.budget.candidates)
    }

    fn retain(
        &mut self,
        dispatch: &Dispatch,
        body: Value,
        uses: Vec<Use>,
        failure: bool,
    ) -> Result<Option<usize>> {
        // Identity and accounting bind the stable form, so two runs of one
        // read at one publication retain the same record (entry 54).
        let received = body;
        let body = super::volatile::stable(&received);
        let bytes = serde_json::to_vec(&body)?;
        self.candidates += 1;
        if bytes.len() > self.remaining_bytes() || self.records.len() >= self.budget.candidates {
            ensure!(
                dispatch.cause != Cause::PartSearch || !self.records.is_empty(),
                "Answer mandatory part search exceeds the original retention budget"
            );
            self.omitted += 1;
            self.stop = Some(GlobalStop::Budget);
            return Ok(None);
        }
        self.bytes += bytes.len();
        let id = framed_digest(
            b"airk.arbiter.loop-record/1",
            &[dispatch.key.as_bytes(), &bytes],
        );
        let index = self.records.len();
        self.records.push(Record {
            id,
            tool: dispatch.tool,
            operation: dispatch.operation,
            arguments: dispatch.arguments.clone(),
            body,
            received,
            bytes: bytes.len(),
            kind: dispatch.kind,
            subject: dispatch.subject.clone(),
            seq: dispatch.seq,
            uses: Vec::new(),
            failure,
        });
        for item in uses {
            self.records[index].attach(item);
        }
        Ok(Some(index))
    }

    /// Records this part first used in `round` that showed it at least one
    /// excerpt it had not seen in an earlier round: the round's progress.
    pub(crate) fn new_evidence(&self, part: u8, round: u8) -> usize {
        let earlier: BTreeSet<Sha256Digest> = self
            .records
            .iter()
            .filter(|record| {
                record
                    .uses
                    .iter()
                    .any(|item| item.part == part && item.round < round)
            })
            .flat_map(excerpts)
            .collect();
        self.records
            .iter()
            .filter(|record| {
                !record.failure
                    && record
                        .uses
                        .iter()
                        .any(|item| item.part == part && item.round == round)
                    && !record
                        .uses
                        .iter()
                        .any(|item| item.part == part && item.round < round)
                    && excerpts(record)
                        .iter()
                        .any(|digest| !earlier.contains(digest))
            })
            .count()
    }
}

/// Digests of a record's evidence-bearing values (hits, reference evidence
/// hits and facts), the same progress units as the /2 gathering.
fn excerpts(record: &Record) -> Vec<Sha256Digest> {
    software::plan::evidence_units(&record.body)
        .into_iter()
        .filter_map(|unit| serde_json::to_vec(unit).ok())
        .map(|bytes| framed_digest(b"airk.arbiter.source-excerpt/1", &[&bytes]))
        .collect()
}

#[cfg(test)]
mod tests;
