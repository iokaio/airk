// SPDX-License-Identifier: Apache-2.0
//! Retrieve per sub-need: an explicit address or a search, followed by the
//! structural reads software work needs (symbols, references, imports,
//! outline, the governing CONTRACT.md or README, tests). These are change
//! set 1's follow-ups, promoted from evaluation into fixed product recipes.
//! Every read is offered to the shared read owner, so a shared read runs once.
use super::gather::{Cause, Dispatch, Gathering, Record, Step};
use super::*;
use software::{FollowUp, Shape};

/// The ten sub-need templates (plan section 12.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NeedKind {
    Definition,
    Callers,
    Tests,
    Contract,
    Documentation,
    Configuration,
    Implementation,
    /// Imports and public types.
    Interfaces,
    Errors,
    Overview,
}
impl NeedKind {
    pub(crate) fn wire(self) -> v3::SubNeedKind {
        match self {
            Self::Definition => v3::SubNeedKind::Definition,
            Self::Callers => v3::SubNeedKind::Callers,
            Self::Tests => v3::SubNeedKind::Tests,
            Self::Contract => v3::SubNeedKind::Contract,
            Self::Documentation => v3::SubNeedKind::Documentation,
            Self::Configuration => v3::SubNeedKind::Configuration,
            Self::Implementation | Self::Interfaces | Self::Errors | Self::Overview => {
                v3::SubNeedKind::Other
            }
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Callers => "callers",
            Self::Tests => "tests",
            Self::Contract => "contract",
            Self::Documentation => "documentation",
            Self::Configuration => "configuration",
            Self::Implementation => "implementation",
            Self::Interfaces => "interfaces",
            Self::Errors => "errors",
            Self::Overview => "overview",
        }
    }
    /// The /2 gap kind an uncovered sub-need of this kind leaves.
    pub(crate) fn gap(self) -> Gap {
        match self {
            Self::Definition | Self::Callers | Self::Interfaces => Gap::Definition,
            Self::Contract => Gap::Constraint,
            Self::Tests => Gap::Test,
            Self::Documentation
            | Self::Configuration
            | Self::Implementation
            | Self::Errors
            | Self::Overview => Gap::Example,
        }
    }
}

/// One planned sub-need of a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Need {
    /// Numbered from 1 within its part.
    pub(crate) id: u8,
    pub(crate) kind: NeedKind,
    /// The identifier, path or literal the sub-need is about.
    pub(crate) subject: Option<String>,
    /// The sub-need's own short query wording.
    pub(crate) query: String,
}

/// A governing-document candidate, queued only after a canonical
/// `file_info` read at the same revision shows it indexed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Probe {
    pub(crate) path: String,
    pub(crate) part: u8,
    pub(crate) need: u8,
    pub(crate) round: u8,
}

/// Reads a recipe step plans next, and governing documents to probe.
#[derive(Debug, Default)]
pub(crate) struct Reaction {
    pub(crate) reads: Vec<Dispatch>,
    pub(crate) probes: Vec<Probe>,
}

const SHAPE: Shape = Shape::PRODUCT;
/// The broad first-round read for each question part.
const PART_SEARCH_LIMIT: usize = 32;
const SEARCH_HIT_BYTES: usize = 768;
/// An inclusive excerpt ends at most 150 lines after its start.
const EXCERPT_LINE_OFFSET: usize = 150;

fn search_args(query: &str, limit: usize, full: bool) -> Value {
    let max_bytes = (limit * SEARCH_HIT_BYTES).clamp(4096, PART_SEARCH_LIMIT * SEARCH_HIT_BYTES);
    json!({"query":query,"limit":limit,"max_bytes":max_bytes,"full_chunks":full})
}

/// A part's own search over its text, the first read of round one.
pub(crate) fn part_search(gather: &mut Gathering, part: u8, text: &str) -> Result<Dispatch> {
    gather.dispatch(
        ("search", v3::LoopRead::Search),
        search_args(text, PART_SEARCH_LIMIT, false),
        (part, None, 1),
        Cause::PartSearch,
        FollowUp::Initial,
        Step::Search,
        text,
    )
}

/// Hits an exact anchor lookup may return.
const ANCHOR_HITS: usize = 8;

/// A caller anchor's exact lookup, a part-level read of round one. The
/// anchor is the whole lexical query and is never read as a pattern. A
/// lookup that fails or finds nothing does not refuse the request.
pub(crate) fn anchor_search(gather: &mut Gathering, part: u8, anchor: &str) -> Result<Dispatch> {
    let mut arguments = search_args(anchor, ANCHOR_HITS, true);
    arguments["mode"] = json!("lexical");
    gather.dispatch(
        ("search", v3::LoopRead::Search),
        arguments,
        (part, None, 1),
        Cause::Anchor,
        FollowUp::Keyword,
        Step::Leaf,
        anchor,
    )
}

/// Hits of a sub-need's configuration lane (rule 1).
const CONFIGURATION_LANE_HITS: usize = 4;

/// Bytes one lane search may return: bounded snippets, never whole chunks.
const LANE_BYTES: usize = 4096;

/// One more round-one search of a sub-need in its own words, restricted to
/// one evidence kind (rule 1, search lanes): source code, so documents
/// cannot crowd code out of its candidates, or configuration for a standing
/// sub-need about tests and builds. Its hits are bounded snippets and are
/// spans; whole text comes from the sub-need's recipe and structure reads.
pub(crate) fn lane_search(
    gather: &mut Gathering,
    part: u8,
    need: &Need,
    kind: airk_core::search_filters::EvidenceKind,
) -> Result<Dispatch> {
    let limit = match kind {
        airk_core::search_filters::EvidenceKind::Configuration => CONFIGURATION_LANE_HITS,
        _ => SHAPE.need_hits,
    };
    let mut arguments = search_args(&need.query, limit, false);
    arguments["max_bytes"] = json!(LANE_BYTES);
    arguments["evidence_kind"] = serde_json::to_value(kind)?;
    gather.dispatch(
        ("search", v3::LoopRead::Search),
        arguments,
        (part, Some(need.id), 1),
        Cause::NeedSearch,
        FollowUp::Keyword,
        Step::Leaf,
        &need.query,
    )
}

/// The first reads of a sub-need in round one.
pub(crate) fn initial(gather: &mut Gathering, part: u8, need: &Need) -> Result<Vec<Dispatch>> {
    let at = (part, Some(need.id), 1);
    if need.kind == NeedKind::Implementation
        && let Some(address) = need.subject.as_deref().and_then(super::plan::known_address)
    {
        return Ok(vec![excerpt(
            gather,
            at,
            (address.path, address.start, address.end, None),
            EXCERPT_LINE_OFFSET,
            (FollowUp::Excerpt, Step::Leaf, Cause::Structural),
            address.path,
        )?]);
    }
    let (limit, full, step) = match need.kind {
        NeedKind::Errors => (SHAPE.need_hits, true, Step::Literal),
        NeedKind::Implementation | NeedKind::Configuration | NeedKind::Overview => {
            (SHAPE.need_hits + 2, false, Step::Chunks)
        }
        NeedKind::Interfaces => (SHAPE.need_hits, false, Step::Imports),
        _ => (SHAPE.need_hits, false, Step::Search),
    };
    let mut reads = vec![gather.dispatch(
        ("search", v3::LoopRead::Search),
        search_args(&need.query, limit, full),
        at,
        Cause::NeedSearch,
        FollowUp::Keyword,
        step,
        &need.query,
    )?];
    if let Some(subject) = &need.subject {
        match need.kind {
            NeedKind::Definition => reads.push(gather.dispatch(
                ("symbols", v3::LoopRead::Symbols),
                json!({"name":subject,"limit":8,"max_bytes":4096}),
                at,
                Cause::Structural,
                FollowUp::Definition,
                Step::Definitions,
                subject,
            )?),
            NeedKind::Callers | NeedKind::Tests => reads.push(gather.dispatch(
                ("references", v3::LoopRead::References),
                json!({"name":subject,"limit":20,"max_bytes":8192}),
                at,
                Cause::Structural,
                FollowUp::References,
                Step::References,
                subject,
            )?),
            _ => {}
        }
    }
    Ok(reads)
}

fn hits(body: &Value) -> Vec<Hit> {
    body.get("hits")
        .or_else(|| {
            body.get("evidence")
                .and_then(|evidence| evidence.get("hits"))
        })
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| serde_json::from_value::<Hit>(value.clone()).ok())
                .filter(|hit| {
                    safe_range(
                        &hit.citation.path,
                        hit.citation.start_line,
                        hit.citation.end_line,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn safe_range(path: &str, start: usize, end: usize) -> bool {
    crate::workbench::safe_path(path) && start > 0 && end >= start && end <= u32::MAX as usize
}

/// Cited `(path, start, end, content hash)` rows of a structural reply.
fn locations(body: &Value) -> Vec<(String, usize, usize, Option<String>)> {
    crate::workbench::locations(body)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let citation = &row["citation"];
            let path = citation["path"].as_str()?.to_owned();
            let start = citation["start_line"].as_u64()? as usize;
            let end = citation["end_line"].as_u64()? as usize;
            safe_range(&path, start, end).then(|| {
                (
                    path,
                    start,
                    end,
                    citation["content_hash"].as_str().map(str::to_owned),
                )
            })
        })
        .collect()
}

/// A hit's whole-chunk read, or `None` when its location admits no useful
/// one (`software::whole_chunk_after`): a document hit is never given a
/// line window past its own location.
fn whole_chunk(
    gather: &mut Gathering,
    at: (u8, Option<u8>, u8),
    hit: &Hit,
    cause: Cause,
) -> Result<Option<Dispatch>> {
    let citation = &hit.citation;
    let lines = (citation.end_line - citation.start_line).clamp(SHAPE.whole_chunk_lines, 100);
    let Some(after) = software::whole_chunk_after(hit, lines) else {
        return Ok(None);
    };
    gather
        .dispatch(
            ("context", v3::LoopRead::Context),
            json!({"path":citation.path,"line":citation.start_line,"before":0,"after":after,"max_bytes":SHAPE.read_bytes}),
            at,
            cause,
            FollowUp::WholeChunk,
            Step::Leaf,
            &citation.path,
        )
        .map(Some)
}

fn outline(
    gather: &mut Gathering,
    at: (u8, Option<u8>, u8),
    path: &str,
    cause: Cause,
) -> Result<Dispatch> {
    gather.dispatch(
        ("outline", v3::LoopRead::Outline),
        json!({"path":path,"limit":40,"max_bytes":4096}),
        at,
        cause,
        FollowUp::Outline,
        Step::Leaf,
        path,
    )
}

/// A bounded excerpt of a cited location, with its known content hash.
pub(crate) fn excerpt(
    gather: &mut Gathering,
    at: (u8, Option<u8>, u8),
    (path, start, end, hash): (&str, usize, usize, Option<&str>),
    span: usize,
    (kind, step, cause): (FollowUp, Step, Cause),
    subject: &str,
) -> Result<Dispatch> {
    let mut arguments = json!({"path":path,"start_line":start,"end_line":end.min(start.saturating_add(span)),"max_bytes":SHAPE.read_bytes});
    if let Some(hash) = hash {
        arguments["known_hash"] = json!(hash);
    }
    gather.dispatch(
        ("read_file", v3::LoopRead::ReadFile),
        arguments,
        at,
        cause,
        kind,
        step,
        subject,
    )
}

/// The governing documents a hit's folder may have, inside its project.
pub(crate) fn governing(path: &str) -> Vec<String> {
    software::governing_candidates(path)
        .into_iter()
        .filter(|candidate| software::in_project(path, candidate))
        .take(4)
        .collect()
}

/// The reads a round-one recipe step plans from its retained record, for
/// the sub-need that dispatched it. Later rounds plan nothing here: their
/// one read per gap comes from `follow`.
pub(crate) fn react(
    gather: &mut Gathering,
    part: u8,
    need: &Need,
    step: Step,
    record: &Record,
) -> Result<Reaction> {
    let at = (part, Some(need.id), 1);
    let mut out = Reaction::default();
    let subject = need.subject.clone().unwrap_or_else(|| need.query.clone());
    match (step, need.kind) {
        (Step::Definitions, _) => {
            let rows = locations(&record.body);
            match rows.first() {
                Some((path, start, end, hash)) => out.reads.push(excerpt(
                    gather,
                    at,
                    (path, *start, *end, hash.as_deref()),
                    EXCERPT_LINE_OFFSET,
                    (FollowUp::Excerpt, Step::Excerpt, Cause::Structural),
                    &subject,
                )?),
                None => out.reads.push(fallback(gather, at, &subject)?),
            }
        }
        (Step::Excerpt, _) => {
            if let Some(path) = record.arguments.get("path").and_then(Value::as_str) {
                out.reads
                    .push(outline(gather, at, path, Cause::Structural)?);
            }
        }
        (Step::References, kind) => {
            let rows = locations(&record.body);
            if rows.is_empty() {
                out.reads.push(fallback(gather, at, &subject)?);
            }
            let want_tests = kind == NeedKind::Tests;
            let (lane_kind, followup) = if want_tests {
                (FollowUp::Tests, true)
            } else {
                (FollowUp::Callers, false)
            };
            for (path, start, end, hash) in rows
                .iter()
                .filter(|row| software::is_test_path(&row.0) == followup)
                .take(2)
            {
                out.reads.push(excerpt(
                    gather,
                    at,
                    (path, *start, *end, hash.as_deref()),
                    60,
                    (lane_kind, Step::Leaf, Cause::Structural),
                    &subject,
                )?);
            }
            if want_tests && out.reads.is_empty() {
                out.reads.push(gather.dispatch(
                    ("search", v3::LoopRead::Search),
                    search_args(&format!("{subject} test"), 4, true),
                    at,
                    Cause::Structural,
                    FollowUp::Tests,
                    Step::Leaf,
                    &subject,
                )?);
            }
        }
        (Step::Search, NeedKind::Contract | NeedKind::Documentation) => {
            let found = hits(&record.body);
            if let Some(top) = found.first() {
                out.probes
                    .extend(governing(&top.citation.path).into_iter().map(|path| Probe {
                        path,
                        part,
                        need: need.id,
                        round: 1,
                    }));
            }
            // The first documentation hit a whole-chunk read can add to.
            if need.kind == NeedKind::Documentation
                && let Some(document) = found.iter().find(|hit| {
                    software::is_document_path(&hit.citation.path)
                        && software::whole_chunk_after(hit, SHAPE.whole_chunk_lines).is_some()
                })
            {
                out.reads
                    .extend(whole_chunk(gather, at, document, Cause::Structural)?);
            }
        }
        (Step::Chunks, _) => {
            let found = hits(&record.body);
            let mut outlined = BTreeSet::new();
            for hit in found.iter().take(SHAPE.kept) {
                out.reads
                    .extend(whole_chunk(gather, at, hit, Cause::Structural)?);
                if outlined.len() < SHAPE.outlined_files
                    && outlined.insert(hit.citation.path.clone())
                {
                    out.reads
                        .push(outline(gather, at, &hit.citation.path, Cause::Structural)?);
                }
            }
        }
        (Step::Imports, _) => {
            let found = hits(&record.body);
            if let Some(top) = found.first() {
                out.reads.push(gather.dispatch(
                    ("imports", v3::LoopRead::Imports),
                    json!({"path":top.citation.path,"limit":20,"max_bytes":4096}),
                    at,
                    Cause::Structural,
                    FollowUp::Outline,
                    Step::Leaf,
                    &top.citation.path,
                )?);
                let name = need
                    .subject
                    .clone()
                    .or_else(|| software::declared_names(&top.text, 1).into_iter().next());
                if let Some(name) = name {
                    out.reads.push(gather.dispatch(
                        ("symbols", v3::LoopRead::Symbols),
                        json!({"name":name,"limit":8,"max_bytes":4096}),
                        at,
                        Cause::Structural,
                        FollowUp::Definition,
                        Step::Leaf,
                        &name,
                    )?);
                }
            }
        }
        (Step::Literal, _) => {
            if let Some(top) = hits(&record.body).first() {
                out.reads
                    .extend(whole_chunk(gather, at, top, Cause::Structural)?);
            }
        }
        _ => {}
    }
    Ok(out)
}

/// A structural read that found nothing falls back to a whole-chunk search.
fn fallback(gather: &mut Gathering, at: (u8, Option<u8>, u8), subject: &str) -> Result<Dispatch> {
    gather.dispatch(
        ("search", v3::LoopRead::Search),
        search_args(subject, 4, true),
        at,
        Cause::Structural,
        FollowUp::Keyword,
        Step::Leaf,
        subject,
    )
}

/// The governing read for a probe the canonical `file_info` showed indexed.
pub(crate) fn governing_read(
    gather: &mut Gathering,
    probe: &Probe,
    lines: usize,
) -> Result<Dispatch> {
    ensure!(
        lines > 0,
        "a governing document with no indexed lines is not read"
    );
    let end = lines.min(120);
    excerpt(
        gather,
        (probe.part, Some(probe.need), probe.round),
        (&probe.path, 1, end, None),
        120,
        (FollowUp::Governing, Step::Leaf, Cause::Structural),
        &probe.path,
    )
}

#[cfg(test)]
mod tests;
