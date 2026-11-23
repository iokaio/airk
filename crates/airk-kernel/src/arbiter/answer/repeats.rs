// SPDX-License-Identifier: Apache-2.0
//! Repeated and overlapping text among the spans an Answer gathered. The
//! same text is delivered once: a repeat names the span that carries it,
//! and an overlap of two spans of one file is kept in one of them. No
//! location is ever dropped, and two different texts are never merged:
//! every rule here compares bytes exactly.
use super::*;

/// The least overlap of two spans that is trimmed. A shorter match (a
/// closing brace, a blank line) says nothing about position.
pub(crate) const MIN_OVERLAP_BYTES: usize = 32;

/// How a text `b` relates to a text `a` of the same file region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overlap {
    None,
    /// `b` is found whole in `a` (the two may be equal).
    Within,
    /// `b` holds `a` whole, at this byte offset of `b`.
    Holds(usize),
    /// The first bytes of `b` are the last bytes of `a`.
    Head(usize),
    /// The last bytes of `b` are the first bytes of `a`.
    Tail(usize),
}

/// The exact overlap of `b` with `a`. Head and tail overlaps count from
/// `MIN_OVERLAP_BYTES` and fall on character boundaries of both texts.
pub(crate) fn overlap(a: &str, b: &str) -> Overlap {
    if a.is_empty() || b.is_empty() {
        return Overlap::None;
    }
    if a.contains(b) {
        return Overlap::Within;
    }
    if let Some(at) = b.find(a) {
        return Overlap::Holds(at);
    }
    let most = a.len().min(b.len());
    let (left, right) = (a.as_bytes(), b.as_bytes());
    let shared = |first: &str, first_bytes: &[u8], second: &str, second_bytes: &[u8]| {
        (MIN_OVERLAP_BYTES..=most).rev().find(|size| {
            first.is_char_boundary(first.len() - size)
                && second.is_char_boundary(*size)
                && first_bytes[first.len() - size..] == second_bytes[..*size]
        })
    };
    if let Some(size) = shared(a, left, b, right) {
        return Overlap::Head(size);
    }
    match shared(b, right, a, left) {
        Some(size) => Overlap::Tail(size),
        None => Overlap::None,
    }
}

/// Two spans lie in one region of one file at one revision: the same path
/// and content hash, and line ranges that meet.
pub(crate) fn same_region(a: &Hit, b: &Hit) -> bool {
    let (a, b) = (&a.citation, &b.citation);
    a.path == b.path
        && a.content_hash == b.content_hash
        && a.start_line <= b.end_line
        && b.start_line <= a.end_line
}

fn place(hit: &Hit) -> String {
    format!(
        "{}:{}-{}",
        hit.citation.path, hit.citation.start_line, hit.citation.end_line
    )
}

/// One span the reply is about to carry.
pub(crate) struct Candidate<'a> {
    pub(crate) hit: &'a Hit,
    pub(crate) focus: Option<f32>,
}

/// What the reply carries of one span.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Carried {
    /// Bytes `start..end` of its text. `overlaps` are the parts trimmed
    /// from it because another span carries them: that span's index, the
    /// bytes, and whether they were at this text's head. `also_at` are the
    /// other places where the very same text was found.
    Text {
        start: usize,
        end: usize,
        overlaps: Vec<(usize, usize, bool)>,
        also_at: Vec<String>,
    },
    /// No text: span `of` carries it. `within`: this span's text is a part
    /// of that span's text, not all of it.
    Same { of: usize, within: bool },
}

/// Decide what each span carries, so every text is delivered once.
///
/// - **Exact repeats.** Of spans with the same text, the highest-ranked one
///   carries it: the highest Focus relevance (a scored span before an
///   unscored one), then the earliest in `spans` (read order, then hit
///   order). The others name it, and where they are at another place, the
///   carrier lists that place in `also_at`.
/// - **Overlaps.** Among the carriers of one file region, the longer text
///   keeps what two of them share (ties as above). A text found whole in a
///   longer one names it (`within`); a text that shares its head or tail
///   with one is trimmed by those bytes.
pub(crate) fn plan(spans: &[Candidate<'_>]) -> Vec<Carried> {
    let mut rank: Vec<usize> = (0..spans.len()).collect();
    rank.sort_by(
        |left, right| match (spans[*left].focus, spans[*right].focus) {
            (Some(left), Some(right)) => right.total_cmp(&left),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
    let whole = |index: usize| Carried::Text {
        start: 0,
        end: spans[index].hit.text.len(),
        overlaps: vec![],
        also_at: vec![],
    };
    let mut out: Vec<Carried> = (0..spans.len()).map(whole).collect();
    // Exact repeats, by rank.
    let mut carriers: Vec<usize> = Vec::new();
    for index in rank {
        let text = &spans[index].hit.text;
        match carriers
            .iter()
            .copied()
            .find(|carrier| spans[*carrier].hit.text == *text)
        {
            Some(carrier) => {
                let here = place(spans[index].hit);
                if let Carried::Text { also_at, .. } = &mut out[carrier]
                    && here != place(spans[carrier].hit)
                    && !also_at.contains(&here)
                {
                    also_at.push(here);
                }
                out[index] = Carried::Same {
                    of: carrier,
                    within: false,
                };
            }
            None => carriers.push(index),
        }
    }
    // Overlaps among the carriers: longer texts first, so a later text
    // never holds an earlier one. The sort is stable, so ties keep rank.
    carriers.sort_by_key(|index| std::cmp::Reverse(spans[*index].hit.text.len()));
    for position in 1..carriers.len() {
        let index = carriers[position];
        let text = spans[index].hit.text.as_str();
        let (mut start, mut end) = (0, text.len());
        let mut trimmed = Vec::new();
        let mut holder = None;
        for earlier in &carriers[..position] {
            let Carried::Text {
                start: from,
                end: to,
                ..
            } = &out[*earlier]
            else {
                continue;
            };
            if !same_region(spans[*earlier].hit, spans[index].hit) {
                continue;
            }
            match overlap(&spans[*earlier].hit.text[*from..*to], &text[start..end]) {
                Overlap::Within => {
                    holder = Some(*earlier);
                    break;
                }
                Overlap::Head(size) => {
                    start += size;
                    trimmed.push((*earlier, size, true));
                }
                Overlap::Tail(size) => {
                    end -= size;
                    trimmed.push((*earlier, size, false));
                }
                Overlap::Holds(_) | Overlap::None => {}
            }
        }
        match (holder, &mut out[index]) {
            (Some(of), slot) => *slot = Carried::Same { of, within: true },
            (
                None,
                Carried::Text {
                    start: from,
                    end: to,
                    overlaps,
                    ..
                },
            ) => {
                *from = start;
                *to = end;
                *overlaps = trimmed;
            }
            (None, Carried::Same { .. }) => {}
        }
    }
    // A repeat of a span that turned out to lie within another names the
    // span that carries the text.
    for index in 0..out.len() {
        let Carried::Same { mut of, mut within } = out[index] else {
            continue;
        };
        while let Carried::Same {
            of: next,
            within: part,
        } = out[of]
        {
            of = next;
            within |= part;
        }
        out[index] = Carried::Same { of, within };
    }
    out
}

#[cfg(test)]
mod tests;
