// SPDX-License-Identifier: Apache-2.0
//! Bounded, exact-publication reference continuation. Legacy references retain
//! their original response and ordering; this additive path is explicit.
use crate::{Kernel, ReferencesResponse, current};
use airk_core::{
    Artifact, Citation, DocumentLocation, Hit, SearchQuery, SearchResponse,
    discovery::MatchCount,
    identity::{Sha256Digest, framed_digest},
    references::{
        ReferenceCoverage, ReferenceFileSummary, ReferenceOptions, ReferenceOrder, ReferencePage,
    },
    search_filters::{EvidenceKind, evidence_kind},
};
use airk_store::current::lexical::LexicalReferencePosition;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

const CANDIDATES_PER_PAGE: usize = 100;
const MAX_RETAINED_BYTES: usize = 16_777_216;
const MAX_SUMMARY_RECORDS: usize = 16;
const MAX_SUMMARY_BYTES: usize = 4096;
const MAX_CANONICAL_LINES: usize = 65_536;
const MAX_CHUNKS: usize = 8192;
const MAX_SOURCE_LINE_OFFSETS: usize = 1_048_576;

/// Cooperative retained scanning starts after the first bounded candidate
/// preparation, not before FTS verification/selection. After slow retained
/// preparation, one bounded step must still advance the cursor before yielding.
struct ScanBudget {
    deadline: Instant,
    progressed: bool,
}
impl ScanBudget {
    fn prepared(now: Instant) -> Self {
        Self {
            deadline: now + Duration::from_millis(250),
            progressed: false,
        }
    }
    fn exhausted(&self, now: Instant) -> bool {
        self.progressed && now >= self.deadline
    }
    fn progress(&mut self) {
        self.progressed = true;
    }
}

fn prepare_page<T>(
    budget: &mut Option<ScanBudget>,
    prepare: impl FnOnce() -> Result<T>,
    clock: impl FnOnce() -> Instant,
) -> Result<T> {
    let page = prepare()?;
    if budget.is_none() {
        *budget = Some(ScanBudget::prepared(clock()));
    }
    Ok(page)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    phase: u8,
    candidate: Option<LexicalReferencePosition>,
    /// Some means this candidate is included again, beginning at this line.
    /// None means it was consumed and the next candidate is selected.
    next_line: Option<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    binding: String,
    position: Position,
    checksum: String,
}

fn checksum(binding: &str, position: &Position) -> Result<String> {
    Ok(framed_digest(
        b"reference-continuation/1",
        &[binding.as_bytes(), &serde_json::to_vec(position)?],
    )
    .to_string())
}

fn decode(encoded: Option<&str>, binding: &str, order: ReferenceOrder) -> Result<Position> {
    let Some(encoded) = encoded else {
        return Ok(Position::default());
    };
    ensure!(encoded.len() <= 32_768, "reference cursor exceeds 32 KiB");
    let cursor: Cursor = serde_json::from_str(
        encoded
            .strip_prefix("airk-rf1:")
            .context("invalid reference cursor version")?,
    )
    .context("malformed reference cursor")?;
    ensure!(
        cursor.binding == binding,
        "reference cursor expired or query/filter/order changed; restart references"
    );
    ensure!(
        cursor.checksum == checksum(binding, &cursor.position)?,
        "reference cursor checksum mismatch"
    );
    ensure!(
        cursor.position.phase <= u8::from(order == ReferenceOrder::CodeFirst)
            && (cursor.position.next_line.is_none() || cursor.position.candidate.is_some()),
        "invalid reference cursor position"
    );
    Ok(cursor.position)
}

fn encode(binding: &str, position: Position) -> Result<String> {
    let cursor = Cursor {
        binding: binding.into(),
        checksum: checksum(binding, &position)?,
        position,
    };
    let encoded = format!("airk-rf1:{}", serde_json::to_string(&cursor)?);
    ensure!(encoded.len() <= 32_768, "reference cursor exceeds 32 KiB");
    Ok(encoded)
}

fn line_location(artifact: &Artifact, chunk: &airk_core::Chunk, number: usize) -> DocumentLocation {
    if artifact.extraction_method == "utf8"
        && matches!(chunk.location, DocumentLocation::Lines { .. })
    {
        DocumentLocation::Lines {
            start: number,
            end: number,
        }
    } else {
        chunk.location.clone()
    }
}

/// Full physical source lines are independent of the byte-limited lexical
/// partitions. Ranges choose the first lexical-eligible source chunk touching a
/// line, so a match in its last fragment is returned once with the whole line.
struct SourceLines {
    text: String,
    offsets: Vec<usize>,
    owners: Vec<(usize, usize, usize)>,
}
impl SourceLines {
    fn build(bytes: Vec<u8>, artifact: &Artifact, name: &str) -> Result<Option<Self>> {
        let text = String::from_utf8(bytes).context("retained utf8 source is invalid")?;
        let mut offsets = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                if offsets.len() == MAX_SOURCE_LINE_OFFSETS {
                    return Ok(None);
                }
                offsets.push(offset + 1);
            }
        }
        if offsets.last().copied() != Some(text.len()) {
            if offsets.len() == MAX_SOURCE_LINE_OFFSETS {
                return Ok(None);
            }
            offsets.push(text.len());
        }
        let wanted = airk_core::lexical::terms(name);
        let path_terms = airk_core::lexical::terms(&artifact.path);
        let path_matches = wanted.is_subset(&path_terms);
        let mut events = BTreeMap::<usize, Vec<(usize, bool)>>::new();
        for (local, chunk) in artifact.chunks.iter().enumerate() {
            if !matches!(chunk.location, DocumentLocation::Lines { .. }) {
                continue;
            }
            ensure!(
                chunk.start_line > 0 && chunk.end_line < offsets.len(),
                "reference source line span is unavailable"
            );
            if !path_matches {
                let chunk_terms = airk_core::lexical::terms(&chunk.text);
                if !wanted
                    .iter()
                    .all(|term| path_terms.contains(term) || chunk_terms.contains(term))
                {
                    continue;
                }
            }
            events
                .entry(chunk.start_line)
                .or_default()
                .push((local, true));
            events
                .entry(chunk.end_line + 1)
                .or_default()
                .push((local, false));
        }
        let mut active = BTreeSet::new();
        let mut owners = Vec::new();
        let mut previous = None;
        for (line, changes) in events {
            if let Some(start) = previous.filter(|start| *start < line)
                && let Some(owner) = active.first().copied()
            {
                owners.push((start, line - 1, owner));
            }
            for (local, enter) in changes {
                if enter {
                    active.insert(local);
                } else {
                    active.remove(&local);
                }
            }
            previous = Some(line);
        }
        Ok(Some(Self {
            text,
            offsets,
            owners,
        }))
    }
    fn line(&self, number: usize) -> &str {
        &self.text[self.offsets[number - 1]..self.offsets[number]]
    }
    fn is_canonical(&self, local: usize, number: usize) -> bool {
        let at = self.owners.partition_point(|(_, end, _)| *end < number);
        self.owners
            .get(at)
            .is_some_and(|(start, _, owner)| *start <= number && *owner == local)
    }
}

fn line_matches(line: &str, name: &str) -> bool {
    line.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|word| word == name)
}

fn overlaps(ranges: &[(usize, usize)], number: usize) -> bool {
    let at = ranges.partition_point(|(_, end)| *end < number);
    ranges.get(at).is_some_and(|(start, _)| *start <= number)
}

/// Build overlap intervals once, then hash only matching lines in those
/// intervals. Nonoverlapping chunks need no line map. This avoids rescanning
/// every preceding chunk for each returned line and preserves distinct original
/// document locations. The map is explicitly bounded, independently of text.
struct CanonicalLines {
    groups: Vec<Sha256Digest>,
    overlap: BTreeMap<Sha256Digest, Vec<(usize, usize)>>,
    owners: HashMap<Sha256Digest, usize>,
}
impl CanonicalLines {
    fn key(group: Sha256Digest, number: usize, text: &str) -> Sha256Digest {
        framed_digest(
            b"reference-canonical-line/1",
            &[
                group.as_bytes(),
                &(number as u64).to_be_bytes(),
                text.as_bytes(),
            ],
        )
    }
    /// None is an explicit ownership-index capacity gap, never a duplicate-free
    /// or exhaustive result. No hidden unbounded map is allocated.
    fn build(artifact: &Artifact, name: &str) -> Result<Option<Self>> {
        Self::build_with_capacity(artifact, name, MAX_CANONICAL_LINES)
    }
    fn build_with_capacity(
        artifact: &Artifact,
        name: &str,
        capacity: usize,
    ) -> Result<Option<Self>> {
        ensure!(
            artifact.chunks.len() <= MAX_CHUNKS,
            "reference chunk count exceeds lexical bound"
        );
        let mut groups = Vec::with_capacity(artifact.chunks.len());
        let mut events = BTreeMap::<Sha256Digest, BTreeMap<usize, i32>>::new();
        for chunk in &artifact.chunks {
            ensure!(
                chunk.start_line <= chunk.end_line,
                "invalid retained reference line span"
            );
            let group = if artifact.extraction_method == "utf8"
                && matches!(chunk.location, DocumentLocation::Lines { .. })
            {
                Sha256Digest::from_bytes([0; 32])
            } else {
                framed_digest(
                    b"reference-original-location/1",
                    &[&serde_json::to_vec(&chunk.location)?],
                )
            };
            groups.push(group);
            let group_events = events.entry(group).or_default();
            *group_events.entry(chunk.start_line).or_default() += 1;
            *group_events
                .entry(
                    chunk
                        .end_line
                        .checked_add(1)
                        .context("reference line span overflow")?,
                )
                .or_default() -= 1;
        }
        let mut overlap = BTreeMap::<Sha256Digest, Vec<(usize, usize)>>::new();
        for (group, events) in events {
            let mut active = 0;
            let mut previous = None;
            let mut ranges = Vec::new();
            for (line, delta) in events {
                if let Some(start) = previous.filter(|start| active >= 2 && *start < line) {
                    ranges.push((start, line - 1));
                }
                active += delta;
                previous = Some(line);
            }
            if !ranges.is_empty() {
                overlap.insert(group, ranges);
            }
        }
        let mut index = Self {
            groups,
            overlap,
            owners: HashMap::new(),
        };
        if index.overlap.is_empty() {
            return Ok(Some(index));
        }
        for (local, chunk) in artifact.chunks.iter().enumerate() {
            let group = index.groups[local];
            let Some(ranges) = index.overlap.get(&group) else {
                continue;
            };
            for (offset, line) in chunk.text.split_inclusive('\n').enumerate() {
                let number = chunk
                    .start_line
                    .checked_add(offset)
                    .context("reference line number overflow")?;
                if !overlaps(ranges, number) || !line_matches(line, name) {
                    continue;
                }
                let key = Self::key(group, number, line);
                let full = index.owners.len() == capacity;
                if let std::collections::hash_map::Entry::Vacant(entry) = index.owners.entry(key) {
                    if full {
                        return Ok(None);
                    }
                    entry.insert(local);
                }
            }
        }
        Ok(Some(index))
    }
    fn is_canonical(&self, local: usize, number: usize, text: &str) -> bool {
        let group = self.groups[local];
        let Some(ranges) = self.overlap.get(&group) else {
            return true;
        };
        !overlaps(ranges, number)
            || self.owners.get(&Self::key(group, number, text)) == Some(&local)
    }
}

fn summaries(hits: &[Hit]) -> Result<(Vec<ReferenceFileSummary>, bool)> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for hit in hits {
        *counts.entry(&hit.citation.path).or_default() += 1;
    }
    let mut result = Vec::new();
    let mut bytes = 0;
    for (path, count) in counts {
        let row = ReferenceFileSummary {
            path: path.into(),
            count,
        };
        let size = serde_json::to_vec(&row)?.len();
        if result.len() == MAX_SUMMARY_RECORDS || bytes + size > MAX_SUMMARY_BYTES {
            return Ok((result, true));
        }
        bytes += size;
        result.push(row);
    }
    Ok((result, false))
}

/// Use complete bounded metadata selectors before FTS LIMIT when possible. A
/// capped roster must never become an allowlist: fall back to resumable
/// candidate filtering rather than silently dropping paths beyond that cap.
fn selected_paths(
    read: &airk_store::current::CurrentRead,
    prefix: &str,
    options: &ReferenceOptions,
) -> Result<Option<[BTreeSet<String>; 2]>> {
    if options.filters.is_empty() && options.order != ReferenceOrder::CodeFirst {
        return Ok(None);
    }
    let deadline = Instant::now() + Duration::from_millis(25);
    let lookup = options.filters.language.as_deref().map_or(
        airk_store::current::postings::Lookup::Files,
        airk_store::current::postings::Lookup::Language,
    );
    let mut after = None;
    let mut scanned = 0;
    let mut bytes = 2usize;
    let mut selected = [BTreeSet::new(), BTreeSet::new()];
    loop {
        let page = read.lookup_paths(lookup, prefix, after.as_deref(), 256)?;
        for path in page.paths {
            if scanned == 4096 || Instant::now() >= deadline {
                return Ok(None);
            }
            scanned += 1;
            bytes = bytes.saturating_add(serde_json::to_vec(&path)?.len() + 1);
            if bytes > 4 << 20 {
                return Ok(None);
            }
            let artifact = read
                .artifact(&path)?
                .context("reference filter path missing")?;
            if options.filters.matches(&artifact) {
                let phase = usize::from(
                    options.order == ReferenceOrder::CodeFirst
                        && evidence_kind(&artifact) != EvidenceKind::Code,
                );
                selected[phase].insert(path);
            }
        }
        after = page.next_after;
        if after.is_none() {
            return Ok(Some(selected));
        }
    }
}

impl Kernel {
    /// One bounded candidate scan with optional navigation metadata. `max_bytes`
    /// still counts text; this path returns complete matching lines and reports
    /// a too-large next line explicitly rather than losing its continuation.
    pub fn references_page(
        &self,
        name: &str,
        path_prefix: &str,
        limit: usize,
        max_bytes: usize,
        options: &ReferenceOptions,
    ) -> Result<ReferencesResponse> {
        airk_core::validate_budget(limit, max_bytes)?;
        ensure!(
            !name.is_empty()
                && name.len() <= 1024
                && path_prefix.len() <= 4096
                && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "reference name must be a single identifier"
        );
        options.filters.validate()?;
        let read = self.current_reader()?;
        let publication = current::runtime::envelope(&read)?;
        let binding = airk_core::discovery::binding(
            &self.scope_bound_cursor_identity(&serde_json::to_string(&read.token()?)?)?,
            "references/1",
            (
                name,
                path_prefix,
                &options.filters,
                options.order,
                options.include_files,
                options.include_count,
            ),
        )?;
        let mut position = decode(options.cursor.as_deref(), &binding, options.order)?;
        let first_page = options.cursor.is_none();
        let mut evidence = SearchResponse {
            execution: None,
            publication: Some(publication),
            semantic_readiness: if !read.selected_scope_restricts_paths() {
                Some(current::enrichment::readiness(&read, &self.store)?)
            } else {
                None
            },
            discovery_readiness: if !read.selected_scope_restricts_paths() {
                Some(current::progress::readiness(&read, &self.store)?)
            } else {
                None
            },
            generation: read.head().sequence,
            collection: self.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        // No full inventory projection is needed to page reference candidates.
        let paths = selected_paths(&read, path_prefix, options)?;
        let mut scan_budget = None;
        let mut candidate_budget = CANDIDATES_PER_PAGE;
        let mut retained_bytes = 0usize;
        let mut artifacts = BTreeMap::<String, Artifact>::new();
        let mut source_lines = BTreeMap::<String, SourceLines>::new();
        let mut source_line_offsets = 0usize;
        let mut canonical = BTreeMap::<String, CanonicalLines>::new();
        let mut canonical_lines = 0usize;
        let mut remaining = max_bytes;
        let mut matched = 0usize;
        let mut reason = None;
        let mut required = None;
        let mut more = false;
        let mut complete = false;
        let mut complete_lines_seen = false;
        let mut fragments_seen = false;
        let phases = if options.order == ReferenceOrder::CodeFirst {
            2
        } else {
            1
        };
        'scan: loop {
            let query = SearchQuery {
                text: name.into(),
                path_prefix: Some(path_prefix.into()),
                limit: candidate_budget,
                max_bytes: 65_536,
            };
            let page = prepare_page(
                &mut scan_budget,
                || {
                    read.lexical_reference_page(
                        &query,
                        position.candidate.as_ref(),
                        position.next_line.is_some(),
                        options.order != ReferenceOrder::Index,
                        paths.as_ref().map(|p| &p[usize::from(position.phase)]),
                    )
                },
                Instant::now,
            )?;
            ensure!(
                page.manifest_digest == read.head().lexical_commit,
                "reference candidate publication mismatch"
            );
            let scan_budget = scan_budget
                .as_mut()
                .context("reference scan budget missing after preparation")?;
            if position.next_line.is_some_and(|line| line > 0) {
                ensure!(
                    page.values.first().is_some_and(
                        |c| Some(LexicalReferencePosition::from(c)) == position.candidate
                    ),
                    "reference continuation candidate is unavailable"
                );
            }
            for candidate in &page.values {
                let key = LexicalReferencePosition::from(candidate);
                let next_line = if position.candidate.as_ref() == Some(&key) {
                    position.next_line.unwrap_or(0)
                } else {
                    0
                };
                position.candidate = Some(key);
                position.next_line = Some(next_line);
                if scan_budget.exhausted(Instant::now()) {
                    reason = Some("scan_deadline".into());
                    more = true;
                    break 'scan;
                }
                candidate_budget -= 1;
                let path = &candidate.citation.path;
                let entry = read
                    .artifact(path)?
                    .context("reference candidate missing from current index")?;
                let is_code = evidence_kind(&entry) == EvidenceKind::Code;
                if !options.filters.matches(&entry)
                    || (options.order == ReferenceOrder::CodeFirst
                        && (position.phase == 0) != is_code)
                {
                    position.next_line = None;
                    scan_budget.progress();
                    continue;
                }
                if !artifacts.contains_key(path) {
                    let bytes = entry
                        .extracted_bytes()
                        .saturating_add(entry.bytes)
                        .saturating_add(
                            entry
                                .retained
                                .as_ref()
                                .map_or(0, |retained| retained.object_bytes),
                        );
                    if bytes > MAX_RETAINED_BYTES {
                        reason = Some("artifact_exceeds_retained_scan_budget".into());
                        break 'scan;
                    }
                    if retained_bytes.saturating_add(bytes) > MAX_RETAINED_BYTES {
                        reason = Some("retained_scan_budget".into());
                        more = true;
                        break 'scan;
                    }
                    retained_bytes += bytes;
                    // Profile contract 1 is the normal native adapter; contract
                    // 2 denotes composite hosts, not a universal source-line
                    // capability. Prove direct text against retained bytes.
                    let (artifact, payload) = if entry
                        .retained
                        .as_ref()
                        .filter(|r| r.legacy_origin.is_none())
                        .and_then(|r| r.profile.as_ref())
                        .is_some_and(|p| p.chunk_version == 3)
                    {
                        let retained = self.store.load_retrieval_artifact(&entry)?;
                        (retained.artifact, Some(retained.payload))
                    } else {
                        (self.store.load_retained(&entry)?, None)
                    };
                    // Original source may be unavailable for saved historical
                    // evidence. Canonical saved chunks remain reference evidence;
                    // direct source-line proof requires a current payload.
                    let source = payload
                        .as_ref()
                        .map(|_| self.store.read_retained_source(&entry))
                        .transpose()?;
                    if artifact.extraction_method == "utf8"
                        && let (Some(payload), Some(source)) = (&payload, source)
                        && payload.text.as_bytes() == source.as_slice()
                    {
                        let Some(lines) = SourceLines::build(source, &artifact, name)? else {
                            reason = Some("artifact_exceeds_source_line_index_budget".into());
                            break 'scan;
                        };
                        if source_line_offsets.saturating_add(lines.offsets.len())
                            > MAX_SOURCE_LINE_OFFSETS
                        {
                            reason = Some("source_line_index_scan_budget".into());
                            more = true;
                            break 'scan;
                        }
                        source_line_offsets += lines.offsets.len();
                        source_lines.insert(path.clone(), lines);
                    }
                    let Some(index) = CanonicalLines::build(&artifact, name)? else {
                        reason = Some("artifact_exceeds_canonical_line_budget".into());
                        break 'scan;
                    };
                    if canonical_lines.saturating_add(index.owners.len()) > MAX_CANONICAL_LINES {
                        reason = Some("canonical_line_scan_budget".into());
                        more = true;
                        break 'scan;
                    }
                    canonical_lines += index.owners.len();
                    canonical.insert(path.clone(), index);
                    artifacts.insert(path.clone(), artifact);
                }
                let artifact = &artifacts[path];
                let local = usize::try_from(candidate.local)?;
                let chunk = artifact
                    .chunks
                    .get(local)
                    .context("reference candidate chunk missing")?;
                ensure!(
                    candidate.stable_id
                        == airk_store::current::lexical::stable_id(path, candidate.local)?
                        && candidate.citation.content_hash == artifact.content_hash
                        && candidate.citation.start_line == chunk.start_line
                        && candidate.citation.end_line == chunk.end_line
                        && candidate.citation.location == chunk.location,
                    "reference candidate differs from retained evidence"
                );
                let original_lines = source_lines
                    .get(path)
                    .filter(|_| matches!(chunk.location, DocumentLocation::Lines { .. }));
                let line_count = original_lines.map_or_else(
                    || chunk.text.split_inclusive('\n').count(),
                    |_| chunk.end_line - chunk.start_line + 1,
                );
                ensure!(
                    next_line <= line_count,
                    "reference line continuation is unavailable"
                );
                let lines: Box<dyn Iterator<Item = (usize, &str)> + '_> = if let Some(source) =
                    original_lines
                {
                    Box::new(
                        (chunk.start_line..=chunk.end_line)
                            .map(move |number| (number - chunk.start_line, source.line(number))),
                    )
                } else {
                    Box::new(chunk.text.split_inclusive('\n').enumerate())
                };
                for (offset, line) in lines {
                    if offset < next_line {
                        continue;
                    }
                    position.next_line = Some(offset);
                    if scan_budget.exhausted(Instant::now()) {
                        reason = Some("scan_deadline".into());
                        more = true;
                        break 'scan;
                    }
                    // A bounded line is examined before honoring an already
                    // expired cooperative deadline after slow preparation.
                    scan_budget.progress();
                    position.next_line = Some(offset + 1);
                    if !line_matches(line, name) {
                        continue;
                    }
                    let number = chunk.start_line + offset;
                    let location = line_location(artifact, chunk, number);
                    let is_canonical = original_lines.map_or_else(
                        || canonical[path].is_canonical(local, number, line),
                        |source| source.is_canonical(local, number),
                    );
                    if !is_canonical {
                        continue;
                    }
                    if original_lines.is_some() {
                        complete_lines_seen = true;
                    } else {
                        fragments_seen = true;
                    }
                    matched += 1;
                    if line.len() > airk_core::MAX_RESPONSE_BYTES {
                        reason = Some("matching_line_exceeds_maximum_text_budget".into());
                        required = Some(line.len());
                        // No legal max_bytes can fit this complete line. Expose
                        // a terminal coverage gap rather than an endless cursor.
                        break 'scan;
                    }
                    if evidence.hits.len() == limit || line.len() > remaining {
                        position.next_line = Some(offset);
                        reason = Some(
                            if evidence.hits.len() == limit {
                                "hit_limit"
                            } else {
                                "text_byte_budget"
                            }
                            .into(),
                        );
                        required = evidence.hits.is_empty().then_some(line.len());
                        more = true;
                        break 'scan;
                    }
                    remaining -= line.len();
                    evidence.hits.push(Hit {
                        text: line.into(),
                        truncated: original_lines.is_none(),
                        citation: Citation {
                            path: path.clone(),
                            content_hash: artifact.content_hash.clone(),
                            start_line: number,
                            end_line: number,
                            location,
                        },
                    });
                }
                position.next_line = None;
                scan_budget.progress();
            }
            if page.incomplete {
                reason = Some("candidate_scan_budget".into());
                more = true;
                break;
            }
            if position.phase + 1 < phases {
                position.phase += 1;
                position.candidate = None;
                position.next_line = None;
                if candidate_budget == 0 {
                    reason = Some("candidate_scan_budget".into());
                    more = true;
                    break;
                }
                continue;
            }
            complete = true;
            break;
        }
        evidence.truncated = !complete || fragments_seen;
        if complete && fragments_seen {
            reason = Some("original_line_completeness_unverified".into());
        }
        let cursor = more.then(|| encode(&binding, position)).transpose()?;
        let (files, files_truncated) = if options.include_files {
            let (files, truncated) = summaries(&evidence.hits)?;
            (Some(files), truncated)
        } else {
            (None, false)
        };
        let candidate_count_complete = first_page && complete && !fragments_seen;
        read.token()?;
        Ok(ReferencesResponse {
            resolution: "textual occurrences in verified lexical candidates; token boundaries may omit identifiers; includes comments, strings and definitions; no name resolution",
            evidence,
            page: Some(ReferencePage {
                cursor,
                more,
                incomplete_reason: reason,
                required_record_bytes: required,
                files,
                files_truncated,
                match_count: options.include_count.then(|| MatchCount {
                    total_matches: candidate_count_complete.then_some(matched),
                    lower_bound: matched,
                    complete: candidate_count_complete,
                    scope: if candidate_count_complete {
                        "verified_lexical_candidates"
                    } else {
                        "page_scan"
                    }
                    .into(),
                }),
                count_unit: if fragments_seen {
                    if complete_lines_seen {
                        "matching_lines_and_fragments"
                    } else {
                        "matching_line_fragments"
                    }
                } else {
                    "matching_lines"
                }
                .into(),
                coverage: ReferenceCoverage {
                    basis: "verified_lexical_candidates".into(),
                    candidate_scan_complete: complete,
                    query_complete: false,
                    query_total_matches: None,
                    token_boundary_complete: false,
                    line_completeness: if fragments_seen {
                        "includes_unverified_fragments"
                    } else if complete_lines_seen {
                        "complete_source_lines"
                    } else {
                        "no_returned_lines"
                    }
                    .into(),
                },
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(chunks: Vec<airk_core::Chunk>, method: &str) -> Artifact {
        Artifact {
            path: "fixture".into(),
            content_hash: "fixture".into(),
            bytes: 100,
            language: "rust".into(),
            extraction_method: method.into(),
            parse_status: "ok".into(),
            chunks,
            facts: Vec::new(),
            retained: None,
        }
    }

    #[test]
    fn overlapping_retained_lines_deduplicate_but_original_document_locations_stay_distinct()
    -> Result<()> {
        let first = airk_core::Chunk {
            start_line: 1,
            end_line: 1,
            text: "Session\n".into(),
            location: DocumentLocation::Lines { start: 1, end: 1 },
        };
        let source = artifact(vec![first.clone(), first.clone()], "utf8");
        let canonical = CanonicalLines::build(&source, "Session")?.context("canonical index")?;
        assert!(!canonical.is_canonical(1, 1, "Session\n"));
        assert!(canonical.is_canonical(0, 1, "Session\n"));
        assert_eq!(canonical.owners.len(), 1);
        let mut second = first.clone();
        second.location = DocumentLocation::Block { number: 2 };
        let mut first = first;
        first.location = DocumentLocation::Block { number: 1 };
        let document = artifact(vec![first, second], "docx");
        let canonical = CanonicalLines::build(&document, "Session")?.context("canonical index")?;
        assert!(canonical.is_canonical(1, 1, "Session\n"));
        assert!(
            canonical.owners.is_empty(),
            "nonoverlapping original locations need no line map"
        );
        Ok(())
    }

    #[test]
    fn canonical_overlap_index_caps_unique_owners_without_indexing_disjoint_text() -> Result<()> {
        let chunk = airk_core::Chunk {
            start_line: 1,
            end_line: 3,
            text: "Session one\nSession two\nSession three\n".into(),
            location: DocumentLocation::Lines { start: 1, end: 3 },
        };
        let source = artifact(vec![chunk.clone(), chunk.clone()], "utf8");
        assert!(CanonicalLines::build_with_capacity(&source, "Session", 2)?.is_none());
        let canonical =
            CanonicalLines::build_with_capacity(&source, "Session", 3)?.context("bounded index")?;
        assert_eq!(canonical.owners.len(), 3);
        let mut disjoint = chunk.clone();
        disjoint.start_line = 4;
        disjoint.end_line = 6;
        let source = artifact(vec![chunk, disjoint], "utf8");
        assert!(
            CanonicalLines::build_with_capacity(&source, "Session", 0)?
                .context("no overlap index needed")?
                .owners
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn slow_preparation_has_a_fresh_scan_budget_and_expired_work_still_advances_once() -> Result<()>
    {
        let queued = Instant::now();
        let prepared = queued + Duration::from_secs(5);
        // Inject a long preparation interval; no sleep or scheduler assumption.
        let clock = std::cell::Cell::new(queued);
        let mut budget = None;
        let value = prepare_page(
            &mut budget,
            || {
                clock.set(prepared);
                Ok(7)
            },
            || clock.get(),
        )?;
        assert_eq!(value, 7);
        let mut budget = budget.context("fresh scan budget")?;
        assert!(!budget.exhausted(prepared));
        budget.progress();
        assert!(!budget.exhausted(prepared + Duration::from_millis(249)));
        assert!(budget.exhausted(prepared + Duration::from_millis(250)));
        // Retained artifact/ownership preparation can consume the cooperative
        // interval too. Exactly one bounded step is allowed before yielding.
        let mut budget = ScanBudget::prepared(prepared);
        let after_retained_preparation = prepared + Duration::from_secs(2);
        assert!(!budget.exhausted(after_retained_preparation));
        budget.progress();
        assert!(budget.exhausted(after_retained_preparation));
        Ok(())
    }

    #[test]
    fn cursor_binds_query_identity_and_code_pass_and_rejects_tampering() -> Result<()> {
        let binding = "fictional immutable query";
        let position = Position {
            phase: 1,
            ..Default::default()
        };
        let cursor = encode(binding, position)?;
        assert_eq!(
            decode(Some(&cursor), binding, ReferenceOrder::CodeFirst)?.phase,
            1
        );
        assert!(
            decode(
                Some(&cursor),
                "other publication",
                ReferenceOrder::CodeFirst
            )
            .is_err()
        );
        assert!(decode(Some(&cursor), binding, ReferenceOrder::Path).is_err());
        assert!(
            decode(
                Some(&cursor.replace("\"phase\":1", "\"phase\":0")),
                binding,
                ReferenceOrder::CodeFirst
            )
            .is_err()
        );
        Ok(())
    }
}
