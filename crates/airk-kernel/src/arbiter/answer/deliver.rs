// SPDX-License-Identifier: Apache-2.0
//! Rules 5 and 6 of the gathering rules (kernel CONTRACT "Gathering rules"):
//! a passage fetched for one sub-need is a candidate for every sub-need of
//! the request, and the reply carries each sub-need's best passages inside
//! one byte budget. Without a model the score is deterministic: the share of
//! the sub-need's rarer words a passage holds, and the search's own order.
use super::coverage::{Span, record_hits};
use super::gather::Gathering;
use super::*;
use std::collections::BTreeMap;

/// Passages of other sub-needs a sub-need's ranking sees at a barrier, after
/// the cheap first cut (rule 5). Focus, when present, scores these with the
/// sub-need's own spans.
pub(crate) const CROSS_PER_NEED: usize = 3;
/// Passages of other sub-needs a sub-need's delivery ranking sees (rule 5).
pub(crate) const CROSS_DELIVERY: usize = 6;
/// A delivered passage scores at least this share of its sub-need's best.
pub(crate) const KEEP_SHARE: f32 = 0.5;
/// Passages one sub-need keeps for delivery.
pub(crate) const PER_NEED: usize = 8;
/// A passage four fifths of whose lines another delivered passage of the
/// same file already holds is not delivered again: the sub-need cites that
/// one instead.
const HELD_NUMERATOR: usize = 4;
const HELD_DENOMINATOR: usize = 5;

/// The lower-case words of a text: letters split at case changes and
/// non-letters, digits on their own, three characters or more.
pub(crate) fn words(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut BTreeSet<String>| {
        if word.chars().count() >= 3 {
            out.insert(word.to_lowercase());
        }
        word.clear();
    };
    for (index, ch) in chars.iter().copied().enumerate() {
        if !ch.is_alphanumeric() {
            flush(&mut word, &mut out);
            continue;
        }
        if let Some(previous) = word.chars().last() {
            let next = chars.get(index + 1).copied();
            let boundary = (previous.is_numeric() != ch.is_numeric())
                || ((previous.is_lowercase() || previous.is_numeric()) && ch.is_uppercase())
                || (previous.is_uppercase()
                    && ch.is_uppercase()
                    && next.is_some_and(char::is_lowercase));
            if boundary {
                flush(&mut word, &mut out);
            }
        }
        word.push(ch);
    }
    flush(&mut word, &mut out);
    out
}

/// How rare each word is among a pool of passages.
pub(crate) struct Rarity {
    seen: BTreeMap<String, usize>,
}
impl Rarity {
    pub(crate) fn new<'a>(bags: impl IntoIterator<Item = &'a BTreeSet<String>>) -> Self {
        let mut seen = BTreeMap::new();
        for bag in bags {
            for word in bag {
                *seen.entry(word.clone()).or_insert(0usize) += 1;
            }
        }
        Self { seen }
    }
    fn weight(&self, word: &str) -> f32 {
        1.0 / ((1 + self.seen.get(word).copied().unwrap_or(0)) as f32).sqrt()
    }
    /// The share, 0 to 1, of the wanted words' weight a bag holds.
    pub(crate) fn share(&self, wanted: &BTreeSet<String>, bag: &BTreeSet<String>) -> f32 {
        let total: f32 = wanted.iter().map(|word| self.weight(word)).sum();
        if total <= 0.0 {
            return 0.0;
        }
        let held: f32 = wanted.intersection(bag).map(|word| self.weight(word)).sum();
        held / total
    }
}

fn span_key(span: &Span) -> (String, usize, usize, String) {
    let citation = &span.hit.citation;
    (
        citation.path.clone(),
        citation.start_line,
        citation.end_line,
        citation.content_hash.clone(),
    )
}

fn spans_of(gather: &Gathering, record: usize, round: u8) -> Vec<Span> {
    record_hits(&gather.records[record])
        .into_iter()
        .map(|(pointer, hit)| Span {
            record,
            pointer,
            hit,
            round,
            focus: None,
            from: 0,
        })
        .collect()
}

fn usable(gather: &Gathering, record: usize) -> bool {
    let record = &gather.records[record];
    !record.failure && !super::follow::is_join_search(record)
}

/// The cheap first cut of rule 5: hits of this round's reads for the part's
/// other sub-needs that `own` does not hold, most of the sub-need's rarer
/// words first, at most `limit`, each sharing at least one such word.
pub(crate) fn cross_spans(
    gather: &Gathering,
    part: u8,
    need: u8,
    text: &str,
    round: u8,
    own: &[Span],
    limit: usize,
) -> Vec<Span> {
    let held: BTreeSet<_> = own.iter().map(span_key).collect();
    let texts: BTreeSet<&str> = own.iter().map(|span| span.hit.text.as_str()).collect();
    let mut pool: Vec<Span> = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, record) in gather.records.iter().enumerate() {
        if !usable(gather, index)
            || !record.uses.iter().any(|item| {
                item.part == part && item.round == round && item.need.is_some_and(|id| id != need)
            })
        {
            continue;
        }
        for span in spans_of(gather, index, round) {
            let key = span_key(&span);
            if !held.contains(&key) && !texts.contains(span.hit.text.as_str()) && seen.insert(key) {
                pool.push(span);
            }
        }
    }
    let bags: Vec<BTreeSet<String>> = pool.iter().map(|span| words(&span.hit.text)).collect();
    let rarity = Rarity::new(&bags);
    let wanted = words(text);
    let mut scored: Vec<(f32, usize)> = bags
        .iter()
        .enumerate()
        .map(|(index, bag)| (rarity.share(&wanted, bag), index))
        .filter(|(score, _)| *score > 0.0)
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0).then(left.1.cmp(&right.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, index)| pool[index].clone())
        .collect()
}

/// One passage a sub-need may be delivered, with its score.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) span: Span,
    pub(crate) score: f32,
}

/// A sub-need's delivery candidates, best first. Its own reads' hits come in
/// dispatch order, then the part-level reads' (the part search and anchor
/// lookups), then, with `cross`, the cheap first cut of the other sub-needs'.
/// A passage Focus scored keeps its best Focus relevance; every other scores
/// the share of the sub-need's rarer words it holds, plus a bonus for its
/// place in its own sub-need's reads (the first place's for a passage the
/// guidance, cited-file or structure rules fetched for it).
pub(crate) fn candidates(
    gather: &Gathering,
    part: u8,
    need: u8,
    text: &str,
    scored: &[Span],
    cross: bool,
) -> Vec<Candidate> {
    let mut own = Vec::new();
    let mut shared = Vec::new();
    let mut others = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, record) in gather.records.iter().enumerate() {
        if !usable(gather, index) {
            continue;
        }
        let lane = if record
            .uses
            .iter()
            .any(|item| item.part == part && item.need == Some(need))
        {
            &mut own
        } else if record
            .uses
            .iter()
            .any(|item| item.part == part && item.need.is_none())
        {
            &mut shared
        } else if cross && record.uses.iter().any(|item| item.part == part) {
            &mut others
        } else {
            continue;
        };
        let round = record
            .uses
            .iter()
            .filter(|item| item.part == part)
            .map(|item| item.round)
            .min()
            .unwrap_or(1);
        lane.extend(spans_of(gather, index, round));
    }
    let every: Vec<&Span> = own.iter().chain(&shared).chain(&others).collect();
    let bags: Vec<BTreeSet<String>> = every.iter().map(|span| words(&span.hit.text)).collect();
    let rarity = Rarity::new(&bags);
    let wanted = words(text);
    let focus = |span: &Span| {
        scored
            .iter()
            .filter(|other| other.record == span.record && other.pointer == span.pointer)
            .filter_map(|other| other.focus)
            .reduce(f32::max)
    };
    let mut out: Vec<Candidate> = Vec::new();
    let (own_len, shared_len) = (own.len(), shared.len());
    let mut cut: Vec<Candidate> = Vec::new();
    for (index, span) in every.into_iter().enumerate() {
        if !seen.insert(span_key(span)) {
            continue;
        }
        let share = rarity.share(&wanted, &bags[index]);
        // A passage a gathering rule fetched for this sub-need (its guidance,
        // a file the evidence named, its code structure) ranks as its first
        // own hit does: the rule chose it for the sub-need.
        let gathered = matches!(
            gather.records[span.record].stage,
            v3::GatheringStageKind::Guidance
                | v3::GatheringStageKind::Cited
                | v3::GatheringStageKind::Structure
        );
        let score = focus(span).unwrap_or(if index < own_len {
            share
                + if gathered {
                    0.25
                } else {
                    0.25 / (1.0 + index as f32)
                }
        } else {
            share
        });
        let candidate = Candidate {
            span: span.clone(),
            score,
        };
        if index < own_len + shared_len {
            out.push(candidate);
        } else if share > 0.0 {
            cut.push(candidate);
        }
    }
    cut.sort_by(|left, right| right.score.total_cmp(&left.score));
    out.extend(cut.into_iter().take(CROSS_DELIVERY));
    // Stable: equal scores keep dispatch order.
    out.sort_by(|left, right| right.score.total_cmp(&left.score));
    out
}

fn lines(span: &Span) -> std::ops::RangeInclusive<usize> {
    span.hit.citation.start_line..=span.hit.citation.end_line
}

/// Rule 6: each sub-need's kept passages inside one byte budget of passage
/// text. A passage is kept when it scores at least `KEEP_SHARE` of its
/// sub-need's best (at most `PER_NEED`, and at least the best). A quarter of
/// the budget is split evenly across sub-needs and the rest by how many
/// passages each kept; the passages go out in rounds across sub-needs, each
/// within its own share first, then within the whole budget. A passage
/// already delivered, or four fifths held by delivered passages of its file,
/// is cited, not sent again. A sub-need that kept any passage never comes
/// back empty: its best is delivered even past the budget, and Union then
/// fits the reply to its frame.
pub(crate) fn deliver(per_need: &[Vec<Candidate>], budget: usize) -> Vec<Vec<Span>> {
    let needs = per_need.len();
    let kept: Vec<Vec<&Candidate>> = per_need
        .iter()
        .map(|ranked| {
            let best = ranked.first().map_or(0.0, |item| item.score);
            let kept: Vec<&Candidate> = ranked
                .iter()
                .filter(|item| item.score > 0.0 && item.score >= KEEP_SHARE * best)
                .take(PER_NEED)
                .collect();
            if kept.is_empty() {
                ranked.iter().take(1).collect()
            } else {
                kept
            }
        })
        .collect();
    let total = kept.iter().map(Vec::len).sum::<usize>().max(1);
    let allowance: Vec<usize> = kept
        .iter()
        .map(|kept| budget / (4 * needs.max(1)) + (budget * 3 / 4) * kept.len() / total)
        .collect();
    let mut spent = vec![0usize; needs];
    let mut used = 0usize;
    let mut delivered: Vec<Span> = Vec::new();
    let mut out: Vec<Vec<Span>> = vec![Vec::new(); needs];
    for limited in [true, false] {
        for rank in 0..PER_NEED {
            for need in 0..needs {
                let Some(item) = kept[need].get(rank) else {
                    continue;
                };
                let key = span_key(&item.span);
                if out[need].iter().any(|span| span_key(span) == key) {
                    continue;
                }
                if let Some(row) = delivered.iter().find(|row| span_key(row) == key) {
                    out[need].push(row.clone());
                    continue;
                }
                let size = item.span.hit.text.len();
                if limited && spent[need] > 0 && spent[need] + size > allowance[need] {
                    continue;
                }
                let wanted = lines(&item.span);
                let covering: Vec<&Span> = delivered
                    .iter()
                    .filter(|row| {
                        row.hit.citation.path == item.span.hit.citation.path
                            && lines(row).start() <= wanted.end()
                            && wanted.start() <= lines(row).end()
                    })
                    .collect();
                let held = wanted
                    .clone()
                    .filter(|line| covering.iter().any(|row| lines(row).contains(line)))
                    .count();
                let length = wanted.end() - wanted.start() + 1;
                if held * HELD_DENOMINATOR >= length * HELD_NUMERATOR {
                    for row in covering {
                        if !out[need].iter().any(|span| span_key(span) == span_key(row)) {
                            out[need].push(row.clone());
                        }
                    }
                    continue;
                }
                if used + size > budget {
                    continue;
                }
                used += size;
                spent[need] += size;
                delivered.push(item.span.clone());
                out[need].push(item.span.clone());
            }
        }
    }
    for need in 0..needs {
        if out[need].is_empty()
            && let Some(best) = kept[need].first()
        {
            out[need].push(best.span.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests;
