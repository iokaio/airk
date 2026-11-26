// SPDX-License-Identifier: Apache-2.0
//! Read and check coverage (step 3): each sub-need's spans from this round's
//! reads, one batched Focus call over the spans of every ready part, the
//! coverage roster per sub-need, and the deterministic verdict used when no
//! model chooses. Focus without a model never claims coverage lexically
//! beyond the structural rules below.
use super::gather::{Gathering, Record};
use super::retrieve::{Need, NeedKind};
use super::*;
use airk_core::arbiter::context_action::{EvidenceRef, SourceUnitKind};
use airk_core::native_loop::{CardEvidence, LoopCard, MIN_EXCERPT_BYTES};
use software::{FollowUp, Shape};

/// Spans per sub-need and round scored by Focus.
pub(crate) const FOCUS_SPANS_PER_NEED: usize = 4;

/// One candidate span: a hit of a retained record.
#[derive(Clone, Debug)]
pub(crate) struct Span {
    pub(crate) record: usize,
    pub(crate) pointer: String,
    pub(crate) hit: Hit,
    pub(crate) round: u8,
    pub(crate) focus: Option<f32>,
    /// Where the text Focus scored starts in the hit's text (its Focus
    /// window); 0 when Focus did not score the span. An excerpt always
    /// holds this place, so the relevance beside it describes text that is
    /// shown.
    pub(crate) from: usize,
}
impl Span {
    pub(crate) fn location(&self) -> String {
        format!(
            "{}:{}-{}",
            self.hit.citation.path, self.hit.citation.start_line, self.hit.citation.end_line
        )
    }
}

/// The hits of a record with their JSON pointers.
pub(crate) fn record_hits(record: &Record) -> Vec<(String, Hit)> {
    let mut out = Vec::new();
    for (base, values) in [
        ("/hits", record.body.get("hits")),
        (
            "/evidence/hits",
            record
                .body
                .get("evidence")
                .and_then(|evidence| evidence.get("hits")),
        ),
    ] {
        if let Some(values) = values.and_then(Value::as_array) {
            for (index, value) in values.iter().enumerate() {
                if let Ok(hit) = serde_json::from_value::<Hit>(value.clone())
                    && !hit.text.trim().is_empty()
                    && crate::workbench::safe_path(&hit.citation.path)
                    && hit.citation.start_line > 0
                    && hit.citation.end_line >= hit.citation.start_line
                {
                    out.push((format!("{base}/{index}"), hit));
                }
            }
        }
    }
    out
}

/// This round's spans of a sub-need, distinct files first, at most four;
/// round one adds the part search's hits after the sub-need's own.
pub(crate) fn spans(gather: &Gathering, part: u8, need: u8, round: u8) -> Vec<Span> {
    let mut own = Vec::new();
    let mut shared = Vec::new();
    for (index, record) in gather.records.iter().enumerate() {
        if record.failure || super::follow::is_join_search(record) {
            continue;
        }
        let lane = if record.used_by(part, Some(need), round) {
            &mut own
        } else if round == 1 && record.used_by(part, None, 1) {
            &mut shared
        } else {
            continue;
        };
        for (pointer, hit) in record_hits(record) {
            lane.push(Span {
                record: index,
                pointer,
                hit,
                round,
                focus: None,
                from: 0,
            });
        }
    }
    let mut ordered = Vec::new();
    let mut seen = BTreeSet::new();
    for lane in [own, shared] {
        let paths: Vec<&str> = lane
            .iter()
            .map(|span| span.hit.citation.path.as_str())
            .collect();
        for index in software::distinct_files_first(&paths) {
            let span = &lane[index];
            let key = (
                span.hit.citation.path.clone(),
                span.hit.citation.start_line,
                span.hit.citation.end_line,
                span.hit.citation.content_hash.clone(),
            );
            if ordered.len() < FOCUS_SPANS_PER_NEED && seen.insert(key) {
                ordered.push(span.clone());
            }
        }
    }
    ordered
}

/// Order spans by Focus relevance, unscored ones after in canonical order.
pub(crate) fn rank(spans: &mut [Span]) {
    spans.sort_by(|left, right| match (left.focus, right.focus) {
        (Some(left), Some(right)) => right.total_cmp(&left),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
}

/// What a sub-need's structural reads in this Answer established.
fn structural(gather: &Gathering, part: u8, need: &Need) -> (bool, bool, bool, bool, bool) {
    let mine: Vec<&Record> = gather
        .records
        .iter()
        .filter(|record| {
            !record.failure
                && record
                    .uses
                    .iter()
                    .any(|item| item.part == part && item.need == Some(need.id))
        })
        .collect();
    let located = |operation: v3::LoopRead| {
        mine.iter().any(|record| {
            record.operation == operation
                && !crate::workbench::locations(&record.body)
                    .unwrap_or_default()
                    .is_empty()
        })
    };
    let definition = located(v3::LoopRead::Symbols);
    let excerpt = mine.iter().any(|record| record.kind == FollowUp::Excerpt);
    let caller_reference = mine.iter().any(|record| {
        record.operation == v3::LoopRead::References
            && crate::workbench::locations(&record.body)
                .unwrap_or_default()
                .iter()
                .any(|row| {
                    row["citation"]["path"]
                        .as_str()
                        .is_some_and(|path| !software::is_test_path(path))
                })
    });
    let caller_excerpt = mine.iter().any(|record| record.kind == FollowUp::Callers);
    // A governing document counts only for the folder of the sub-need's
    // own evidence (its anchor), as its probe was planned.
    let anchors: Vec<String> = mine
        .iter()
        .filter(|record| record.kind != FollowUp::Governing)
        .flat_map(|record| {
            record_hits(record)
                .into_iter()
                .map(|(_, hit)| hit.citation.path)
        })
        .collect();
    let governing = mine.iter().any(|record| {
        record.kind == FollowUp::Governing
            && record
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| {
                    software::is_governing_path(path)
                        && anchors.iter().any(|anchor| {
                            software::governing_candidates(anchor)
                                .iter()
                                .any(|candidate| candidate == path)
                        })
                })
    });
    (
        definition,
        excerpt,
        caller_reference && caller_excerpt,
        governing,
        located(v3::LoopRead::References),
    )
}

fn whole_chunk_read(gather: &Gathering, part: u8, need: u8, path: &str) -> bool {
    gather.records.iter().any(|record| {
        !record.failure
            && record.kind == FollowUp::WholeChunk
            && record.arguments.get("path").and_then(Value::as_str) == Some(path)
            && record
                .uses
                .iter()
                .any(|item| item.part == part && item.need == Some(need))
    })
}

/// The identifiers and key nouns a sub-need's query names.
fn terms(need: &Need) -> Vec<String> {
    let mut terms = software::identifiers(&need.query, 4);
    for noun in software::key_nouns(&need.query, 4) {
        if ![
            "test",
            "tests",
            "documentation",
            "configuration",
            "contract",
            "rules",
            "error",
            "interface",
        ]
        .contains(&noun.as_str())
        {
            terms.push(noun);
        }
    }
    terms
}

fn names(text: &str, term: &str) -> bool {
    if term
        .bytes()
        .any(|byte| byte.is_ascii_uppercase() || byte == b'_')
    {
        software::names_identifier(text, term)
    } else {
        text.to_ascii_lowercase().contains(term)
    }
}

/// The deterministic verdict of section 3.4: structural rules, then Focus.
pub(crate) fn verdict(
    gather: &Gathering,
    part: u8,
    need: &Need,
    spans: &[Span],
    structural_records: usize,
) -> v3::Coverage {
    let shape = Shape::PRODUCT;
    let (definition, excerpt, callers, governing, _) = structural(gather, part, need);
    let top = spans.first();
    let focus = top.and_then(|span| span.focus);
    let covered = match need.kind {
        NeedKind::Definition => definition && excerpt,
        NeedKind::Callers => callers,
        NeedKind::Tests => spans.iter().any(|span| {
            software::is_test_path(&span.hit.citation.path)
                && (span.focus.is_some_and(|score| score >= shape.covered)
                    || need
                        .subject
                        .as_deref()
                        .is_some_and(|subject| software::names_identifier(&span.hit.text, subject)))
        }),
        NeedKind::Contract => governing,
        NeedKind::Documentation => spans.iter().any(|span| {
            software::is_document_path(&span.hit.citation.path)
                && match span.focus {
                    Some(score) => score >= shape.covered,
                    None => {
                        let terms = terms(need);
                        !terms.is_empty()
                            && terms
                                .iter()
                                .filter(|term| names(&span.hit.text, term))
                                .count()
                                * 2
                                >= terms.len()
                    }
                }
        }),
        _ => top.is_some_and(|span| match focus {
            Some(score) => {
                score >= shape.covered
                    && whole_chunk_read(gather, part, need.id, &span.hit.citation.path)
            }
            None => {
                let terms = terms(need);
                !terms.is_empty() && terms.iter().all(|term| names(&span.hit.text, term))
            }
        }),
    };
    if covered && (!spans.is_empty() || structural_records > 0) {
        v3::Coverage::Covered
    } else if !spans.is_empty()
        || structural_records > 0
        || (need.kind == NeedKind::Definition && definition)
    {
        v3::Coverage::Partial
    } else {
        v3::Coverage::Missing
    }
}

/// Structural needs can close only after the corresponding native reads
/// establish the definition, caller, or governing contract.
pub(crate) fn may_offer_covered(kind: NeedKind, verdict: v3::Coverage) -> bool {
    verdict == v3::Coverage::Covered
        || !matches!(
            kind,
            NeedKind::Definition | NeedKind::Callers | NeedKind::Contract
        )
}

/// A model's coverage choice: Covered only with evidence; otherwise partial
/// when the top Focus score reaches the keep threshold (or, without Focus,
/// when any evidence exists), else missing.
pub(crate) fn from_card(covered: bool, spans: &[Span], evidence: usize) -> v3::Coverage {
    if evidence == 0 {
        return v3::Coverage::Missing;
    }
    if covered {
        return v3::Coverage::Covered;
    }
    match spans.first().and_then(|span| span.focus) {
        Some(score) if score < Shape::PRODUCT.keep_threshold => v3::Coverage::Missing,
        _ => v3::Coverage::Partial,
    }
}

/// An alternative coverage card: read more at a location or search a term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Alternative {
    pub(crate) class: v3::CardClass,
    pub(crate) text: String,
}

/// The coverage roster of one sub-need: covered (citing its top two
/// spans), then at most three alternatives.
pub(crate) fn cards(need: u8, spans: &[Span], alternatives: &[Alternative]) -> Vec<LoopCard> {
    let symbols = ["A", "B", "C", "D"];
    let cited: Vec<String> = spans.iter().take(2).map(Span::location).collect();
    let mut out = vec![LoopCard {
        symbol: symbols[0].into(),
        class: v3::CardClass::Covered,
        text: if cited.is_empty() {
            format!("s{need} covered")
        } else {
            format!("s{need} covered: {}", cited.join(", "))
        },
        partition: vec![],
    }];
    for alternative in alternatives {
        if out.len() >= symbols.len() || out.iter().any(|card| card.text == alternative.text) {
            continue;
        }
        out.push(LoopCard {
            symbol: symbols[out.len()].into(),
            class: alternative.class,
            text: alternative.text.clone(),
            partition: vec![],
        });
    }
    out
}

/// How many bytes of each text one card call shows. `texts` are what the
/// call's spans have to show, and `allowance` is the call's excerpt bytes
/// (`native_loop::evidence_allowance` of its token reservation). Every text
/// gets the same number of bytes, the level, or all of itself where it is
/// shorter; the level is the largest that keeps the sum within the
/// allowance. Each length then moves back to a character boundary. Equal
/// texts get equal cuts, and the result depends only on the texts and the
/// allowance, not on their order.
pub(crate) fn excerpt_cuts(texts: &[&str], allowance: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..texts.len()).collect();
    order.sort_by_key(|index| texts[*index].len());
    let mut left = allowance;
    let mut level = usize::MAX;
    for (position, index) in order.iter().enumerate() {
        let rest = texts.len() - position;
        let length = texts[*index].len();
        if length.saturating_mul(rest) <= left {
            left -= length;
        } else {
            level = left / rest;
            break;
        }
    }
    texts
        .iter()
        .map(|text| {
            let mut cut = text.len().min(level);
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            cut
        })
        .collect()
}

/// The byte range of `text` that an excerpt of at most `size` bytes shows.
/// - All of the text when it fits.
/// - Otherwise a window of `size` bytes that holds `from`, the start of the
///   text Focus scored: it starts at `from`, or earlier where the text ends
///   before `size` bytes from there, so the whole size is used.
///
/// Both ends fall on character boundaries, which can make the window up to
/// three bytes shorter at each end.
pub(crate) fn window(text: &str, from: usize, size: usize) -> (usize, usize) {
    if size >= text.len() {
        return (0, text.len());
    }
    let mut start = from.min(text.len() - size);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (start + size).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (start, end)
}

/// What one card call shows of one sub-need's spans: the rows its roster
/// carries, and the log's account of them for a decision the model takes.
/// A shown span is logged as the byte range of its excerpt; the text before
/// and after it, and a span not shown at all, are logged as omitted.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Shown {
    pub(crate) evidence: Vec<CardEvidence>,
    /// Gathered spans the call shows nothing of.
    pub(crate) unshown: usize,
    pub(crate) presented: Vec<EvidenceRef>,
    pub(crate) omitted: Vec<EvidenceRef>,
}

/// The evidence rows of one card call: each sub-need's ranked spans, cut to
/// the call's excerpt allowance.
/// - A text that fits its share is shown whole. A longer one is shown as a
///   window that holds the start of the text Focus scored (`window`), so
///   the relevance beside it describes text that is shown and no byte of
///   the share is left unused.
/// - A text is shown once in a call. A later span with the same text (the
///   same span gathered for a second sub-need, or a copy at another place)
///   is a row that names the row showing it, and takes no allowance.
/// - `None`: the allowance cannot give every shown span
///   `MIN_EXCERPT_BYTES`, or all it has to show. The call is then not made.
///
/// The texts are those of records already read and retained under the
/// Answer's revision fence; nothing is read here.
pub(crate) fn present(
    gather: &Gathering,
    needs: &[&[Span]],
    allowance: usize,
) -> Option<Vec<Shown>> {
    let spans: Vec<&Span> = needs.iter().flat_map(|spans| spans.iter()).collect();
    // The first span of the call with each text shows it.
    let first: Vec<usize> = spans
        .iter()
        .enumerate()
        .map(|(index, span)| {
            spans[..index]
                .iter()
                .position(|earlier| earlier.hit.text == span.hit.text)
                .unwrap_or(index)
        })
        .collect();
    let shows: Vec<usize> = (0..spans.len())
        .filter(|index| first[*index] == *index)
        .collect();
    let texts: Vec<&str> = shows
        .iter()
        .map(|index| spans[*index].hit.text.as_str())
        .collect();
    let cuts = excerpt_cuts(&texts, allowance);
    let mut windows: Vec<(usize, usize)> = shows
        .iter()
        .zip(&cuts)
        .map(|(index, cut)| window(&spans[*index].hit.text, spans[*index].from, *cut))
        .collect();
    // Character boundaries can leave a window a few bytes short of the
    // least excerpt although its share was enough. Such a window grows by
    // whole characters while the call's allowance still has the bytes.
    let used: usize = windows.iter().map(|(start, end)| end - start).sum();
    let mut spare = allowance.saturating_sub(used);
    for (text, (start, end)) in texts.iter().zip(&mut windows) {
        while *end - *start < MIN_EXCERPT_BYTES && *end - *start < text.len() {
            let (mut lower, mut upper) = (*start, *end);
            if upper < text.len() {
                upper += 1;
                while !text.is_char_boundary(upper) {
                    upper += 1;
                }
            } else {
                lower -= 1;
                while !text.is_char_boundary(lower) {
                    lower -= 1;
                }
            }
            let cost = (upper - lower) - (*end - *start);
            if cost > spare {
                break;
            }
            spare -= cost;
            (*start, *end) = (lower, upper);
        }
    }
    if windows
        .iter()
        .zip(&texts)
        .any(|((start, end), text)| end - start < MIN_EXCERPT_BYTES && end - start < text.len())
    {
        return None;
    }
    // The shown byte range of every span, a repeat taking its first's.
    let range = |index: usize| {
        let position = shows.iter().position(|item| *item == first[index])?;
        Some(windows[position])
    };
    let mut index = 0;
    // Each shown span's position among all the call's rows, so a repeat
    // names the one row that shows its text (Core `CardEvidence.same_as`).
    let mut row_of: Vec<Option<usize>> = vec![None; spans.len()];
    let mut rows = 0;
    let mut out = Vec::with_capacity(needs.len());
    for need in needs {
        let mut shown = Shown::default();
        for span in *need {
            let (from, to) = range(index).unwrap_or((0, 0));
            let text = &span.hit.text;
            let unit = |utf8_start, utf8_end| EvidenceRef {
                record: gather.records[span.record].id,
                kind: SourceUnitKind::Hit,
                result_pointer: span.pointer.clone(),
                utf8_start,
                utf8_end,
            };
            let citation = &span.hit.citation;
            if to > from {
                let repeat = first[index] != index;
                row_of[index] = Some(rows);
                rows += 1;
                shown.evidence.push(CardEvidence {
                    path: citation.path.clone(),
                    start_line: citation.start_line,
                    end_line: citation.end_line,
                    focus: span.focus,
                    excerpt: if repeat {
                        String::new()
                    } else {
                        text[from..to].to_owned()
                    },
                    skipped_bytes: if repeat { 0 } else { from },
                    cut_bytes: if repeat { 0 } else { text.len() - to },
                    truncated: span.hit.truncated,
                    same_as: if repeat { row_of[first[index]] } else { None },
                });
                shown.presented.push(unit(from, to));
                if from > 0 {
                    shown.omitted.push(unit(0, from));
                }
                if to < text.len() {
                    shown.omitted.push(unit(to, text.len()));
                }
            } else {
                shown.unshown += 1;
                shown.omitted.push(unit(0, text.len()));
            }
            index += 1;
        }
        out.push(shown);
    }
    Some(out)
}

/// The questions and spans of one batched Focus call. Each sub-need's query
/// is its spans' question; equal queries share one question.
pub(crate) fn batch(
    ready: &[(u8, &Need, u8, &[Span])],
    limit: usize,
) -> (Vec<String>, Vec<(usize, usize)>, Vec<focus::BatchSpan>) {
    let mut questions: Vec<String> = Vec::new();
    let mut owners = Vec::new();
    let mut spans = Vec::new();
    for (slot, (part, need, round, candidates)) in ready.iter().enumerate() {
        for (index, span) in candidates.iter().enumerate() {
            if spans.len() >= limit
                || span
                    .hit
                    .citation
                    .content_hash
                    .parse::<Sha256Digest>()
                    .is_err()
            {
                continue;
            }
            let question = match questions
                .iter()
                .position(|question| *question == need.query)
            {
                Some(position) => position,
                None if questions.len() < v3::MAX_FOCUS_QUESTIONS => {
                    questions.push(need.query.clone());
                    questions.len() - 1
                }
                None => continue,
            };
            owners.push((slot, index));
            spans.push(focus::BatchSpan {
                question: question as u8,
                hit: span.hit.clone(),
                part: *part,
                round: *round,
            });
        }
    }
    (questions, owners, spans)
}

#[cfg(test)]
mod tests;
