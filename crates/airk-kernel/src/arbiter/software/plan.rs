// SPDX-License-Identifier: Apache-2.0
//! Change-set follow-up planning, governing pre-checks and presentation.
//! One Gathering state machine; every read uses the same canonical reader.
use super::*;
use crate::arbiter::Gathering;
use std::collections::BTreeMap;

struct Probe {
    path: String,
    folder: String,
    round: usize,
    revision: String,
}

pub(crate) struct State {
    pub(crate) layers: Layers,
    kinds: BTreeMap<Sha256Digest, FollowUp>,
    subjects: BTreeSet<String>,
    outlined: BTreeSet<String>,
    excerpted: BTreeSet<(String, usize, usize)>,
    probes: VecDeque<Probe>,
    probed: BTreeSet<String>,
    governed: BTreeSet<String>,
    probes_run: usize,
    governing: usize,
    literals: usize,
    per_read: usize,
    kept: Vec<usize>,
    pub(crate) failed: usize,
}

impl State {
    pub(crate) fn new(layers: Layers) -> Self {
        Self {
            layers,
            kinds: BTreeMap::new(),
            subjects: BTreeSet::new(),
            outlined: BTreeSet::new(),
            excerpted: BTreeSet::new(),
            probes: VecDeque::new(),
            probed: BTreeSet::new(),
            governed: BTreeSet::new(),
            probes_run: 0,
            governing: 0,
            literals: 0,
            per_read: 4096,
            kept: vec![],
            failed: 0,
        }
    }
    /// The kind of the record just retained.
    pub(crate) fn record(&mut self, records: &[EvidenceRecord], kind: FollowUp) {
        if let Some(record) = records.last() {
            self.kinds.insert(record.id, kind);
        }
    }
    /// Kinds for the packing omission rank; `None` keeps product packing.
    pub(crate) fn omission_kinds(&self) -> Option<BTreeMap<Sha256Digest, FollowUp>> {
        self.layers.present.then(|| self.kinds.clone())
    }
}

/// Evidence-bearing values of a canonical reply: hits, reference evidence
/// hits and facts. Used for progress, never to rewrite the retained result.
pub(crate) fn evidence_units(body: &Value) -> Vec<&Value> {
    [
        body.get("hits"),
        body.get("evidence").and_then(|evidence| evidence.get("hits")),
        body.get("facts"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_array)
    .flatten()
    .collect()
}

/// Change 8: a Context call is admitted only when its measured (or, before the
/// first call, estimated) cost plus the read reserve fits the deadline.
#[cfg_attr(not(feature = "local-inference"), allow(dead_code))]
pub(crate) struct ContextGate {
    on: bool,
    strict: bool,
    last_ms: Option<u64>,
    stopped: bool,
}

#[cfg_attr(not(feature = "local-inference"), allow(dead_code))]
impl ContextGate {
    #[cfg(test)]
    pub(crate) fn new(on: bool) -> Self {
        Self::with_deadline(on, false)
    }
    /// Change set 3 (`deadline=strict`): the first call is assumed to take its
    /// worst measured case, and the gate is on even without `device`.
    pub(crate) fn with_deadline(on: bool, strict: bool) -> Self {
        Self {
            on: on || strict,
            strict,
            last_ms: None,
            stopped: false,
        }
    }
    pub(crate) fn admits(&self, deadline: Instant, now: Instant) -> bool {
        if !self.on {
            return true;
        }
        let first = if self.strict {
            STRICT_FIRST_CONTEXT_MS
        } else {
            FIRST_CONTEXT_ESTIMATE_MS
        };
        let expected = self
            .last_ms
            .map_or(first, |ms| ms.saturating_mul(5) / 4);
        !self.stopped
            && deadline.saturating_duration_since(now).as_millis()
                >= u128::from(expected.saturating_add(READ_RESERVE_MS))
    }
    pub(crate) fn observe(&mut self, elapsed: Duration) {
        let ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        self.stopped |= ms > SINGLE_CONTEXT_CALL_MS;
        self.last_ms = Some(ms);
    }
}

/// Hits (in returned, Focus-ranked order) kept for whole chunks and follow-ups:
/// scored hits at or above `FOCUS_KEEP`, then the unscored tail in order, then
/// the remaining scored hits. Focus never removes a hit from its record.
#[cfg(test)]
pub(crate) fn select_kept(order: &[Sha256Digest], scores: &BTreeMap<Sha256Digest, f32>) -> Vec<usize> {
    select_kept_with(order, scores, KEPT, FOCUS_KEEP)
}

/// `select_kept` with change set 2's kept count and relevance threshold.
pub(crate) fn select_kept_with(
    order: &[Sha256Digest],
    scores: &BTreeMap<Sha256Digest, f32>,
    count: usize,
    threshold: f32,
) -> Vec<usize> {
    let mut kept: Vec<usize> = (0..order.len())
        .filter(|index| scores.get(&order[*index]).is_some_and(|r| *r >= threshold))
        .take(count)
        .collect();
    for unscored in [true, false] {
        for (index, candidate) in order.iter().enumerate() {
            if kept.len() < count
                && !kept.contains(&index)
                && scores.contains_key(candidate) != unscored
            {
                kept.push(index);
            }
        }
    }
    kept
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn safe_range(path: &str, start: usize, end: usize) -> bool {
    crate::workbench::safe_path(path) && start > 0 && end >= start && end <= u32::MAX as usize
}

const GROUPS: [&str; 7] = [
    "Where to change",
    "Rules to respect",
    "Callers and impact",
    "Tests",
    "Documentation",
    "Declarations",
    "Other evidence",
];

fn group(kind: FollowUp, path: &str) -> usize {
    match kind {
        FollowUp::Governing => 1,
        _ if is_governing_path(path) => 1,
        FollowUp::Tests => 3,
        _ if is_test_path(path) => 3,
        FollowUp::Callers | FollowUp::References => 2,
        FollowUp::Outline | FollowUp::Definition => 5,
        _ if is_document_path(path) => 4,
        FollowUp::Literal => 6,
        _ => 0,
    }
}

struct Unit {
    group: usize,
    path: String,
    start: usize,
    end: usize,
    text: String,
    record: Sha256Digest,
    exempt: bool,
}

impl Gathering {
    fn followup(
        &self,
        kind: FollowUp,
        (tool, operation): (&'static str, NativeRead),
        arguments: Value,
        subject: &str,
        anchor: (&str, usize, usize),
        round: usize,
    ) -> Result<Dispatch> {
        let arguments = object(arguments);
        let revision = self.scope.publication.token.to_string();
        Ok(Dispatch {
            choice: ReadChoice {
                id: choice_id(kind, &revision, tool, &arguments)?,
                operation,
                target: target(kind, subject, anchor.0, anchor.1, anchor.2),
            },
            tool,
            arguments,
            revision,
            round,
            kind,
            subject: subject.to_owned(),
        })
    }

    fn capacity(&self) -> usize {
        let budget = &self.request.budget;
        if self.plan.layers.spread {
            budget.fanout.min(budget.candidates)
        } else {
            budget.fanout.min(8).min(budget.candidates)
        }
    }

    fn admissible(&self, dispatch: &Dispatch) -> bool {
        !self.seen.contains(&dispatch.choice.id)
            && !self
                .queue
                .iter()
                .any(|queued| queued.choice.id == dispatch.choice.id)
    }

    /// `front` reads run next (depth on one subject); the rest join the back.
    fn merge(&mut self, front: Vec<Dispatch>, back: Vec<Dispatch>) {
        let capacity = self.capacity();
        let mut added = 0usize;
        for dispatch in front.into_iter().rev() {
            if self.admissible(&dispatch) {
                self.queue.push_front(dispatch);
                added += 1;
            }
        }
        for dispatch in back {
            if self.queue.len() >= capacity {
                break;
            }
            if self.admissible(&dispatch) {
                self.queue.push_back(dispatch);
                added += 1;
            }
        }
        while self.queue.len() > capacity {
            self.queue.pop_back();
            added = added.saturating_sub(1);
        }
        self.output.work.fanout = self.output.work.fanout.max(added.min(capacity));
        self.output.work.candidates = self.output.work.candidates.max(self.queue.len());
    }

    fn kept(&self, body: &Value, count: usize) -> Vec<usize> {
        if self.plan.layers.focus
            && let Some(observation) = body.get("focus").and_then(|focus| {
                serde_json::from_value::<airk_core::arbiter::FocusObservation>(focus.clone()).ok()
            })
            && let Some(receipt) = observation.receipt()
            && receipt.applied
            && receipt.ranked_order.len() == count
        {
            let scores: BTreeMap<_, _> = receipt
                .scores
                .iter()
                .map(|score| (score.candidate, score.relevance))
                .collect();
            return select_kept_with(
                &receipt.ranked_order,
                &scores,
                self.plan.layers.kept(),
                self.plan.layers.keep_threshold(),
            );
        }
        (0..count.min(self.plan.layers.kept())).collect()
    }

    pub(in crate::arbiter) fn plan_followups(&mut self, dispatch: &Dispatch, body: &Value) -> Result<()> {
        let round = dispatch.round.saturating_add(1);
        if round > self.request.budget.rounds || self.output.work.bytes * 5 / 4 > FRAME_PLAN {
            return Ok(());
        }
        let (front, back) = match dispatch.kind {
            FollowUp::Initial => self.plan_initial(dispatch, body, round)?,
            FollowUp::Definition => self.plan_locations(body, &dispatch.subject, round, false)?,
            FollowUp::References => self.plan_locations(body, &dispatch.subject, round, true)?,
            _ => (vec![], vec![]),
        };
        self.merge(front, back);
        if self.queue.is_empty() && self.plan.probes.is_empty() {
            self.output.stop = StopReason::NoValidRead;
        }
        Ok(())
    }

    /// `pool_full=<n>` below an initial snippet pool: one search of the same
    /// question for the top `n` hits with full chunks, run next.
    fn full_chunk_search(&self, initial: &Dispatch, hits: &[Hit], round: usize) -> Result<Option<Dispatch>> {
        let limit = initial.arguments.get("limit").and_then(Value::as_u64).unwrap_or(0) as usize;
        let snippets = initial.arguments.get("full_chunks") == Some(&Value::Bool(false));
        let Some(full) = self.plan.layers.pool_full.map(usize::from) else {
            return Ok(None);
        };
        let Some(top) = hits.first().map(|hit| &hit.citation) else {
            return Ok(None);
        };
        if !snippets || full == 0 || full >= limit {
            return Ok(None);
        }
        let arguments = object(json!({"query":self.request.question,"limit":full,
            "max_bytes":(full * 1536).clamp(4096, 16_384),"full_chunks":true}));
        let revision = self.scope.publication.token.to_string();
        Ok(Some(Dispatch {
            choice: ReadChoice {
                id: choice_id(FollowUp::WholeChunk, &revision, "search", &arguments)?,
                operation: NativeRead::Search,
                target: format!(
                    "Search the index for whole chunks of the top {full} hits for: {} at {}:{}-{}",
                    self.request.question, top.path, top.start_line, top.end_line
                ),
            },
            tool: "search",
            arguments,
            revision,
            round,
            kind: FollowUp::WholeChunk,
            subject: String::new(),
        }))
    }

    fn plan_initial(
        &mut self,
        initial: &Dispatch,
        body: &Value,
        round: usize,
    ) -> Result<(Vec<Dispatch>, Vec<Dispatch>)> {
        let values = body
            .get("hits")
            .and_then(Value::as_array)
            .context("canonical search hits missing")?;
        let hits = values
            .iter()
            .map(|value| serde_json::from_value::<Hit>(value.clone()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for hit in &hits {
            ensure!(
                safe_range(&hit.citation.path, hit.citation.start_line, hit.citation.end_line),
                "unsafe indexed Arbiter target"
            );
        }
        let full_search = self.full_chunk_search(initial, &hits, round)?;
        let reserved = full_search.as_ref().map_or(0, |search| {
            search.arguments.get("max_bytes").and_then(Value::as_u64).unwrap_or(0) as usize
        });
        let layers = self.plan.layers;
        self.plan.per_read = per_read_bytes_with(
            serde_json::to_vec(body)?.len() + reserved,
            layers.kept(),
            layers.read_floor(),
        );
        let per_read = self.plan.per_read;
        let kept = self.kept(body, hits.len());
        self.plan.kept.clone_from(&kept);
        let record = self.output.records.last().context("missing initial record")?.id;
        let mut lanes: BTreeMap<FollowUp, Vec<Dispatch>> = BTreeMap::new();
        let mut in_order = Vec::new();
        for &index in &kept {
            let hit = &hits[index];
            let (path, start, end) = (
                hit.citation.path.as_str(),
                hit.citation.start_line,
                hit.citation.end_line,
            );
            let chunk = self.followup(
                FollowUp::WholeChunk,
                ("context", NativeRead::Context),
                // A pool hit cites only its focused snippet (often 1-12 lines);
                // the whole-chunk read covers at least WHOLE_CHUNK_LINES from it.
                json!({"path":path,"line":start,"before":0,"after":(end - start).clamp(WHOLE_CHUNK_LINES, 100),"max_bytes":per_read}),
                "",
                (path, start, end),
                round,
            )?;
            lanes.entry(FollowUp::WholeChunk).or_default().push(chunk.clone());
            in_order.push(chunk);
            if self.plan.outlined.len() < OUTLINED_FILES && self.plan.outlined.insert(path.to_owned()) {
                let outline = self.followup(
                    FollowUp::Outline,
                    ("outline", NativeRead::DocumentSections),
                    json!({"path":path,"limit":40,"max_bytes":4096}),
                    path,
                    (path, start, end),
                    round,
                )?;
                lanes.entry(FollowUp::Outline).or_default().push(outline.clone());
                in_order.push(outline);
            }
            for candidate in governing_candidates(path)
                .into_iter()
                .filter(|candidate| !layers.governing_project || in_project(path, candidate))
                .take(4)
            {
                if self.plan.probed.len() < GOVERNING_PROBES && self.plan.probed.insert(candidate.clone())
                {
                    let folder = candidate.rsplit_once('/').map_or("", |(folder, _)| folder).to_owned();
                    self.plan.probes.push_back(Probe {
                        path: candidate,
                        folder,
                        round,
                        revision: self.scope.publication.token.to_string(),
                    });
                }
            }
            if is_document_path(path) {
                for (from, to) in literal::source_spans(&hit.text) {
                    let query = &hit.text[from..to];
                    if self.plan.literals >= 2 || !self.literal_queries.insert(query.to_lowercase()) {
                        continue;
                    }
                    let source = literal::SourceLiteral {
                        record,
                        path: path.to_owned(),
                        line: start,
                        returned_utf8_start: from,
                        returned_utf8_end: to,
                        query: query.into(),
                    };
                    let arguments = object(json!({"query":query,"limit":4,"max_bytes":4096,"full_chunks":true}));
                    let revision = self.scope.publication.token.to_string();
                    let literal = Dispatch {
                        choice: ReadChoice {
                            id: choice_id(FollowUp::Literal, &revision, "search", &arguments)?,
                            operation: NativeRead::Search,
                            target: source.target()?,
                        },
                        tool: "search",
                        arguments,
                        revision,
                        round,
                        kind: FollowUp::Literal,
                        subject: query.into(),
                    };
                    self.plan.literals += 1;
                    lanes.entry(FollowUp::Literal).or_default().push(literal.clone());
                    in_order.push(literal);
                }
            }
        }
        // `subjects=strict` scans further but admits only strict names that
        // kept hit text names; loose is change set 1's first-three rule.
        let strict = layers.subjects_strict;
        let scan = if strict { 4 * SUBJECTS } else { SUBJECTS };
        let admitted = |name: &String| {
            !strict
                || strict_subject(name)
                    && kept.iter().any(|index| names_identifier(&hits[*index].text, name))
        };
        let mut subjects: Vec<String> = identifiers(&self.request.question, scan)
            .into_iter()
            .filter(admitted)
            .take(SUBJECTS)
            .collect();
        for &index in &kept {
            for name in declared_names(&hits[index].text, scan) {
                if subjects.len() < SUBJECTS && !subjects.contains(&name) && admitted(&name) {
                    subjects.push(name);
                }
            }
        }
        let top = kept.first().map(|index| &hits[*index].citation);
        for subject in subjects {
            if !self.plan.subjects.insert(subject.clone()) {
                continue;
            }
            // With no hit, the subject itself is the stated anchor.
            let anchor = top.map_or((subject.as_str(), 1, 1), |citation| {
                (citation.path.as_str(), citation.start_line, citation.end_line)
            });
            let definition = self.followup(
                FollowUp::Definition,
                ("symbols", NativeRead::Definitions),
                json!({"name":subject,"limit":8,"max_bytes":4096}),
                &subject,
                anchor,
                round,
            )?;
            let references = self.followup(
                FollowUp::References,
                ("references", NativeRead::References),
                json!({"name":subject,"limit":20,"max_bytes":8192}),
                &subject,
                anchor,
                round,
            )?;
            lanes.entry(FollowUp::Definition).or_default().push(definition.clone());
            lanes.entry(FollowUp::References).or_default().push(references.clone());
            in_order.extend([definition, references]);
        }
        // Change set 3: identifier and key-noun searches, one lane like the others.
        let anchor = top.map_or(("index", 1, 1), |citation| {
            (citation.path.as_str(), citation.start_line, citation.end_line)
        });
        for query in keyword_queries(
            &self.request.question,
            usize::from(layers.keyword_searches.unwrap_or(0)),
        ) {
            if !self.literal_queries.insert(query.to_lowercase()) {
                continue;
            }
            let search = self.followup(
                FollowUp::Keyword,
                ("search", NativeRead::Search),
                json!({"query":query,"limit":6,"max_bytes":4096,"full_chunks":false}),
                &query,
                anchor,
                round,
            )?;
            lanes.entry(FollowUp::Keyword).or_default().push(search.clone());
            in_order.push(search);
        }
        let front: Vec<Dispatch> = full_search.into_iter().collect();
        if !self.plan.layers.spread {
            return Ok((front, in_order));
        }
        // Change 2: one read of each kind per cycle, hits in rank order within a kind.
        let mut spread = Vec::new();
        let mut lanes: Vec<VecDeque<Dispatch>> = lanes.into_values().map(VecDeque::from).collect();
        while lanes.iter().any(|lane| !lane.is_empty()) {
            for lane in &mut lanes {
                if let Some(dispatch) = lane.pop_front() {
                    spread.push(dispatch);
                }
            }
        }
        Ok((front, spread))
    }

    fn plan_locations(
        &mut self,
        body: &Value,
        subject: &str,
        round: usize,
        references: bool,
    ) -> Result<(Vec<Dispatch>, Vec<Dispatch>)> {
        let per_read = self.plan.per_read;
        let mut code = Vec::new();
        let mut tests = Vec::new();
        // A reply shape without cited rows simply plans nothing.
        for row in crate::workbench::locations(body).unwrap_or_default() {
            let citation = &row["citation"];
            let (Some(path), Some(start), Some(end)) = (
                citation["path"].as_str(),
                citation["start_line"].as_u64(),
                citation["end_line"].as_u64(),
            ) else {
                continue;
            };
            let (start, end) = (start as usize, end as usize);
            if !safe_range(path, start, end)
                || self.plan.excerpted.contains(&(path.to_owned(), start, end))
            {
                continue;
            }
            let test = references && is_test_path(path);
            let span = if references { 60 } else { 150 };
            let kind = match (references, test) {
                (false, _) => FollowUp::Excerpt,
                (true, false) => FollowUp::Callers,
                (true, true) => FollowUp::Tests,
            };
            let mut arguments = json!({"path":path,"start_line":start,"end_line":end.min(start + span),"max_bytes":per_read});
            if let Some(hash) = citation["content_hash"].as_str() {
                arguments["known_hash"] = json!(hash);
            }
            let read = self.followup(
                kind,
                ("read_file", NativeRead::ReadEvidence),
                arguments,
                subject,
                (path, start, end),
                round,
            )?;
            let lane = if test { &mut tests } else { &mut code };
            if lane.len() < 2 {
                self.plan.excerpted.insert((path.to_owned(), start, end));
                lane.push(read);
            }
        }
        if references && tests.is_empty() && !code.is_empty() {
            let (path, start, end) = code
                .first()
                .and_then(|read: &Dispatch| {
                    Some((
                        read.arguments.get("path")?.as_str()?.to_owned(),
                        read.arguments.get("start_line")?.as_u64()? as usize,
                        read.arguments.get("end_line")?.as_u64()? as usize,
                    ))
                })
                .context("reference excerpt anchor missing")?;
            tests.push(self.followup(
                FollowUp::Tests,
                ("search", NativeRead::Search),
                json!({"query":format!("{subject} test"),"limit":4,"max_bytes":4096,"full_chunks":true}),
                subject,
                (&path, start, end),
                round,
            )?);
        }
        // The best excerpt of each lane runs next; the second joins the back.
        let mut front = Vec::new();
        let mut back = Vec::new();
        for mut lane in [code, tests] {
            if !lane.is_empty() {
                front.push(lane.remove(0));
            }
            back.extend(lane);
        }
        if !self.plan.layers.spread {
            back.splice(0..0, front.drain(..));
        }
        Ok((front, back))
    }

    /// Decision 6: a governing document is queued only after the canonical
    /// reader shows it is indexed at this revision; no failing guessed reads.
    pub(in crate::arbiter) fn resolve_governing(&mut self, reader: &CanonicalReader<'_>) -> Result<()> {
        if !self.plan.layers.followups {
            return Ok(());
        }
        while let Some(probe) = self.plan.probes.pop_front() {
            if self.plan.governing >= GOVERNING_DOCUMENTS || Instant::now() >= self.deadline {
                self.plan.probes.clear();
                break;
            }
            if self.plan.governed.contains(&probe.folder) {
                continue;
            }
            self.plan.probes_run += 1;
            let arguments = object(json!({"path":probe.path}));
            let Ok(info) = reader("file_info", &arguments, Some(&probe.revision)) else {
                continue;
            };
            let lines = info
                .get("indexed_lines")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if token(&info) != Some(probe.revision.as_str()) || lines == 0 {
                continue;
            }
            let end = lines.min(120);
            let read = self.followup(
                FollowUp::Governing,
                ("read_file", NativeRead::ReadEvidence),
                json!({"path":probe.path,"start_line":1,"end_line":end,"max_bytes":self.plan.per_read}),
                &probe.path,
                (&probe.path, 1, end),
                probe.round,
            )?;
            if !self.admissible(&read) {
                continue;
            }
            self.plan.governed.insert(probe.folder);
            self.plan.governing += 1;
            let at = self.queue.len().min(1);
            self.queue.insert(at, read);
            while self.queue.len() > self.capacity() {
                self.queue.pop_back();
            }
            self.output.work.candidates = self.output.work.candidates.max(self.queue.len());
            self.output.work.fanout = self.output.work.fanout.max(1);
        }
        Ok(())
    }

    /// Change set 3 (`deadline=strict`): with less than the dispatch reserve
    /// left no read starts; with less than the search reserve left, queued
    /// follow-up searches (which may run Focus) are dropped. Answer then
    /// returns inside its deadline with what it has. Off: never stops.
    pub(in crate::arbiter) fn strict_deadline_stops(&mut self, now: Instant) -> bool {
        if !self.plan.layers.deadline_strict {
            return false;
        }
        let left = self.deadline.saturating_duration_since(now).as_millis();
        if left < u128::from(STRICT_DISPATCH_RESERVE_MS) {
            return true;
        }
        if left < u128::from(STRICT_SEARCH_RESERVE_MS) {
            self.queue
                .retain(|dispatch| dispatch.tool != "search" || dispatch.round <= 1);
        }
        false
    }

    /// Change 7: every hit, overlap dropped, grouped by purpose, with exact
    /// locations and a closing disclosure. Records are never altered.
    pub(in crate::arbiter) fn present(&mut self) -> Result<()> {
        let mut units = Vec::new();
        // Change set 3: a pool wider than POOL shows more distinct files first,
        // within a byte budget; the remaining pool hits become a location menu.
        let wide = self.plan.layers.pool && self.plan.layers.pool_size() > POOL;
        let mut menu: Vec<String> = Vec::new();
        let mut wide_bytes = 0usize;
        for (index, record) in self.output.records.iter().enumerate() {
            let kind = self.plan.kinds.get(&record.id).copied().unwrap_or(if index == 0 {
                FollowUp::Initial
            } else {
                FollowUp::Literal
            });
            let body = &record.canonical_result;
            if matches!(kind, FollowUp::Outline | FollowUp::Definition) {
                let mut lines: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for row in crate::workbench::locations(body).unwrap_or_default() {
                    let citation = &row["citation"];
                    let Some(path) = citation["path"].as_str() else { continue };
                    let name = row["text"].as_str().unwrap_or("");
                    let detail = row["fact"]["kind"].as_str().unwrap_or("");
                    lines.entry(path.to_owned()).or_default().push(format!(
                        "{name} {detail} {}-{}",
                        citation["start_line"], citation["end_line"]
                    ));
                }
                for (path, list) in lines {
                    units.push(Unit {
                        group: group(kind, &path),
                        text: format!("{path}: {}", list.join("; ")),
                        path,
                        start: 0,
                        end: 0,
                        record: record.id,
                        exempt: true,
                    });
                }
                continue;
            }
            let Some(values) = body
                .get("hits")
                .or_else(|| body.get("evidence").and_then(|evidence| evidence.get("hits")))
                .and_then(Value::as_array)
            else {
                continue;
            };
            let mut order: Vec<usize> = if index == 0 {
                self.plan.kept.clone()
            } else {
                vec![]
            };
            let rest: Vec<usize> = (0..values.len())
                .filter(|position| !order.contains(position))
                .collect();
            let kept_count = order.len();
            if wide && index == 0 {
                let paths: Vec<&str> = rest
                    .iter()
                    .map(|position| values[*position]["citation"]["path"].as_str().unwrap_or(""))
                    .collect();
                order.extend(distinct_files_first(&paths).into_iter().map(|at| rest[at]));
            } else {
                order.extend(rest);
            }
            let limit = if kind == FollowUp::References { 6 } else { usize::MAX };
            for (rank, position) in order.into_iter().take(limit).enumerate() {
                let Ok(mut hit) = serde_json::from_value::<Hit>(values[position].clone()) else {
                    continue;
                };
                if hit.text.trim().is_empty() {
                    continue;
                }
                if wide && index == 0 && rank >= kept_count {
                    let mut end = hit.text.len().min(WIDE_POOL_SNIPPET_BYTES);
                    while !hit.text.is_char_boundary(end) {
                        end -= 1;
                    }
                    if wide_bytes + end > WIDE_POOL_PRESENT_BYTES {
                        menu.push(format!(
                            "{}:{}-{}",
                            hit.citation.path, hit.citation.start_line, hit.citation.end_line
                        ));
                        continue;
                    }
                    wide_bytes += end;
                    hit.text.truncate(end);
                }
                units.push(Unit {
                    group: group(kind, &hit.citation.path),
                    path: hit.citation.path,
                    start: hit.citation.start_line,
                    end: hit.citation.end_line,
                    text: hit.text,
                    record: record.id,
                    exempt: false,
                });
            }
        }
        // Longer ranges first: a contained unit, or one overlapping more than
        // half of the shorter range, is dropped; different files never are.
        let mut by_length: Vec<usize> = (0..units.len()).collect();
        by_length.sort_by_key(|index| {
            std::cmp::Reverse(units[*index].end.saturating_sub(units[*index].start))
        });
        let mut accepted = vec![false; units.len()];
        let mut texts = BTreeSet::new();
        let mut dropped = 0usize;
        for index in by_length {
            let unit = &units[index];
            let overlaps = !unit.exempt
                && units.iter().zip(&accepted).any(|(other, kept)| {
                    *kept && !other.exempt && other.path == unit.path && {
                        let shared = unit.end.min(other.end) + 1;
                        let shared = shared.saturating_sub(unit.start.max(other.start));
                        let shorter = (unit.end.saturating_sub(unit.start) + 1)
                            .min(other.end.saturating_sub(other.start) + 1);
                        shared * 2 > shorter
                    }
                });
            if overlaps || !texts.insert((unit.path.clone(), unit.start, unit.end, unit.text.clone())) {
                dropped += 1;
            } else {
                accepted[index] = true;
            }
        }
        let mut presented = 0usize;
        for (number, name) in GROUPS.iter().enumerate() {
            let members: Vec<&Unit> = units
                .iter()
                .zip(&accepted)
                .filter(|(unit, kept)| **kept && unit.group == number)
                .map(|(unit, _)| unit)
                .collect();
            if members.is_empty() {
                continue;
            }
            let mut evidence = Vec::new();
            for unit in &members {
                if !evidence.contains(&unit.record) {
                    evidence.push(unit.record);
                }
            }
            self.output.paragraphs.push(GroundedParagraph {
                text: format!("{name}:"),
                evidence,
                interpretation: true,
            });
            for unit in members {
                presented += 1;
                let text = if unit.exempt {
                    unit.text.clone()
                } else {
                    format!("({}:{}–{}):\n\n{}", unit.path, unit.start, unit.end, unit.text)
                };
                self.output.paragraphs.push(GroundedParagraph {
                    text,
                    evidence: vec![unit.record],
                    interpretation: false,
                });
            }
        }
        if let Some(first) = self.output.records.first()
            && !menu.is_empty()
        {
            self.output.paragraphs.push(GroundedParagraph {
                text: format!(
                    "Further pool locations not shown above (read any with read_file or context): {}",
                    menu.join("; ")
                ),
                evidence: vec![first.id],
                interpretation: true,
            });
        }
        if let Some(first) = self.output.records.first() {
            self.output.paragraphs.push(GroundedParagraph {
                text: format!(
                    "Answer presented {presented} evidence units from {} canonical reads; {dropped} overlapping or repeated units were dropped; {} queued reads were not run; {} optional follow-up reads failed and keep their receipts; {} governing documents were checked and {} queued. Records omitted later for the reply frame are counted in omitted_count.",
                    self.output.records.len(),
                    self.queue.len(),
                    self.plan.failed,
                    self.plan.probes_run,
                    self.plan.governing,
                ),
                evidence: vec![first.id],
                interpretation: true,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
impl Gathering {
    pub(crate) fn plan_probe_paths(&self) -> Vec<String> {
        self.plan.probes.iter().map(|probe| probe.path.clone()).collect()
    }
}
