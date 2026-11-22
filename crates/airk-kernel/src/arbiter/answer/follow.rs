// SPDX-License-Identifier: Apache-2.0
//! Follow up (step 4): each gap gets one targeted query or structural hop,
//! built only from terms found in what was just read. Candidates are drawn
//! in a fixed order and de-duplicated against the searches already run, the
//! sub-need's tried reads and the shared read cache; a model may choose
//! among them, and with no model the first candidate runs.
use super::coverage::Span;
use super::gather::{Gathering, ReadKey, Step, read_key};
use super::retrieve::Need;
use super::*;
use airk_core::native_loop::LoopCard;
use software::FollowUp;

/// One follow-up read a gap may take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) tool: &'static str,
    pub(crate) operation: v3::LoopRead,
    pub(crate) arguments: Map<String, Value>,
    pub(crate) key: ReadKey,
    pub(crate) kind: FollowUp,
    pub(crate) step: Step,
    /// The term, path or location the read is built from.
    pub(crate) term: String,
    pub(crate) class: v3::CardClass,
}
impl Candidate {
    pub(crate) fn unrun(&self, reason: v3::UnrunReason) -> v3::UnrunRead {
        v3::UnrunRead {
            operation: self.operation,
            target: self.term.clone(),
            reason,
        }
    }
    pub(crate) fn wire(&self) -> v3::FollowUp {
        v3::FollowUp {
            operation: self.operation,
            target: self.term.clone(),
        }
    }
    /// The card text a roster shows for this read of sub-need `need`.
    pub(crate) fn text(&self, need: u8) -> String {
        match self.operation {
            v3::LoopRead::Search => format!("s{need} search \"{}\"", self.term),
            v3::LoopRead::Symbols => format!("s{need} symbols {}", self.term),
            v3::LoopRead::References => format!("s{need} references {}", self.term),
            v3::LoopRead::Outline => format!("s{need} outline {}", self.term),
            _ => format!("s{need} read {}", self.term),
        }
    }
}

const ERROR_WORDS: [&str; 6] = [
    "error",
    "failed",
    "invalid",
    "cannot",
    "refused",
    "unavailable",
];
/// Candidates kept per gap.
pub(crate) const MAX_CANDIDATES: usize = 8;

fn bounded(term: &str) -> Option<String> {
    let term = term.trim();
    (3..=200).contains(&term.len()).then(|| term.to_owned())
}

/// String literals in `text` that name an error condition.
fn error_strings(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for mark in ['"', '\''] {
        let mut rest = text;
        while let Some(start) = rest.find(mark) {
            let after = &rest[start + 1..];
            let Some(end) = after.find(mark) else { break };
            let literal = &after[..end];
            let lower = literal.to_ascii_lowercase();
            if !literal.contains('\n')
                && ERROR_WORDS.iter().any(|word| lower.contains(word))
                && let Some(literal) = bounded(literal)
                && !found.contains(&literal)
            {
                found.push(literal);
                rest = &after[end + 1..];
            } else {
                // A rejected opening quote may still close nothing: move past it only.
                rest = &rest[start + 1..];
            }
        }
    }
    found
}

/// Path-like tokens of `text` that the index already returned in this
/// Answer, so a read of them is never a guess.
fn known_paths(gather: &Gathering, text: &str) -> Vec<String> {
    let indexed: BTreeSet<String> = gather
        .records
        .iter()
        .flat_map(|record| {
            super::coverage::record_hits(record)
                .into_iter()
                .map(|(_, hit)| hit.citation.path)
        })
        .collect();
    let mut found = Vec::new();
    for token in
        text.split(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | ')' | ','))
    {
        let token = token.trim_end_matches(['.', ':', ';']);
        if token.contains('/') && indexed.contains(token) && !found.iter().any(|seen| seen == token)
        {
            found.push(token.to_owned());
        }
    }
    found
}

/// Whether a definition of `name` was already read for this part.
fn defined(gather: &Gathering, part: u8, name: &str) -> bool {
    gather.records.iter().any(|record| {
        record.operation == v3::LoopRead::Symbols
            && record.subject == name
            && record.uses.iter().any(|item| item.part == part)
    })
}

/// The ordered, de-duplicated follow-up candidates of one gap.
pub(crate) fn candidates(
    gather: &Gathering,
    part: u8,
    need: &Need,
    spans: &[Span],
    searched: &BTreeSet<String>,
    tried: &BTreeSet<ReadKey>,
) -> Result<Vec<Candidate>> {
    let revision = gather.revision.clone();
    let mut out: Vec<Candidate> = Vec::new();
    let push = |out: &mut Vec<Candidate>,
                (tool, operation): (&'static str, v3::LoopRead),
                arguments: Value,
                (kind, step): (FollowUp, Step),
                term: &str,
                class: v3::CardClass|
     -> Result<()> {
        let arguments = arguments
            .as_object()
            .cloned()
            .context("follow-up arguments")?;
        let key = read_key(tool, &arguments, &revision)?;
        let query = (tool == "search")
            .then(|| {
                arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .map(str::to_lowercase)
            })
            .flatten();
        if out.len() < MAX_CANDIDATES
            && !gather.known(&key)
            && !tried.contains(&key)
            && query.is_none_or(|query| !searched.contains(&query))
            && !out.iter().any(|old| old.key == key)
        {
            out.push(Candidate {
                tool,
                operation,
                arguments,
                key,
                kind,
                step,
                term: term.to_owned(),
                class,
            });
        }
        Ok(())
    };
    let symbols = |name: &str| json!({"name":name,"limit":8,"max_bytes":4096});
    let references = |name: &str| json!({"name":name,"limit":20,"max_bytes":8192});
    let hop = v3::CardClass::FollowUpHop;
    // 1. Identifiers of the sub-need's query with no definition read yet.
    for name in software::identifiers(&need.query, 4) {
        if !defined(gather, part, &name) {
            push(
                &mut out,
                ("symbols", v3::LoopRead::Symbols),
                symbols(&name),
                (FollowUp::Definition, Step::Definitions),
                &name,
                hop,
            )?;
        }
    }
    let top = spans.first();
    // A truncated or snippet-only top span: read its whole chunk.
    if let Some(span) = top.filter(|span| {
        span.hit.truncated
            || span
                .hit
                .citation
                .end_line
                .saturating_sub(span.hit.citation.start_line)
                < 8
    }) {
        let citation = &span.hit.citation;
        push(
            &mut out,
            ("context", v3::LoopRead::Context),
            json!({"path":citation.path,"line":citation.start_line,"before":0,"after":software::Shape::PRODUCT.whole_chunk_lines,"max_bytes":software::Shape::PRODUCT.read_bytes}),
            (FollowUp::WholeChunk, Step::Leaf),
            &span.location(),
            hop,
        )?;
    }
    if let Some(span) = top {
        // 2. Names declared in the top span.
        for name in software::declared_names(&span.hit.text, 4) {
            push(
                &mut out,
                ("symbols", v3::LoopRead::Symbols),
                symbols(&name),
                (FollowUp::Definition, Step::Definitions),
                &name,
                hop,
            )?;
        }
        // 3. Paths the span names that the index already returned.
        for path in known_paths(gather, &span.hit.text) {
            push(
                &mut out,
                ("outline", v3::LoopRead::Outline),
                json!({"path":path,"limit":40,"max_bytes":4096}),
                (FollowUp::Outline, Step::Leaf),
                &path,
                hop,
            )?;
        }
        // 4. Error strings, as a literal search.
        for literal in error_strings(&span.hit.text) {
            push(
                &mut out,
                ("search", v3::LoopRead::Search),
                json!({"query":literal,"limit":4,"max_bytes":4096,"full_chunks":true}),
                (FollowUp::Literal, Step::Literal),
                &literal,
                v3::CardClass::FollowUpQuery,
            )?;
        }
        // 5. Other identifiers of the span, by first occurrence.
        for name in software::identifiers(&span.hit.text, 4) {
            push(
                &mut out,
                ("symbols", v3::LoopRead::Symbols),
                symbols(&name),
                (FollowUp::Definition, Step::Definitions),
                &name,
                hop,
            )?;
            push(
                &mut out,
                ("references", v3::LoopRead::References),
                references(&name),
                (FollowUp::References, Step::References),
                &name,
                hop,
            )?;
        }
    }
    Ok(out)
}

/// The follow-up roster of one gap: at most `limit` candidates.
pub(crate) fn cards(need: u8, candidates: &[Candidate], limit: usize) -> Vec<LoopCard> {
    let symbols = ["A", "B", "C", "D", "E", "F", "G", "H"];
    candidates
        .iter()
        .take(limit.min(symbols.len()))
        .enumerate()
        .map(|(index, candidate)| LoopCard {
            symbol: symbols[index].into(),
            class: candidate.class,
            text: candidate.text(need),
            partition: vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests;
