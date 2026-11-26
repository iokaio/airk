// SPDX-License-Identifier: Apache-2.0
//! Follow up (step 4): each gap gets one targeted query or structural hop,
//! built only from terms found in what was just read. Candidates are drawn
//! in a fixed order and de-duplicated against the searches already run, the
//! sub-need's tried reads and the shared read cache; a model may choose
//! among them, and with no model the first candidate runs.
use super::coverage::Span;
use super::gather::{Gathering, ReadKey, Record, Step, read_key};
use super::retrieve::{Need, NeedKind};
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
            // A quote right after a letter or digit (an apostrophe, as in
            // "don't") opens nothing, and neither does one whose closing
            // quote is on another line: move past it only.
            let opens = rest[..start]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_alphanumeric());
            if !opens || literal.contains('\n') {
                rest = &rest[start + 1..];
                continue;
            }
            // Any other literal, kept or not, is skipped whole, so the text
            // between two literals is never read as one.
            let lower = literal.to_ascii_lowercase();
            if ERROR_WORDS.iter().any(|word| lower.contains(word))
                && let Some(literal) = bounded(literal)
                && !found.contains(&literal)
            {
                found.push(literal);
            }
            rest = &after[end + 1..];
        }
    }
    found
}

const MAX_JOIN_LITERALS: usize = 2;
const MAX_JOIN_ORIGINS: usize = 4;

struct CallLiteral<'a> {
    text: &'a str,
    site: &'a str,
}

fn punctuated_literal(text: &str) -> bool {
    if !(3..=128).contains(&text.len())
        || !text.contains(['.', '-', '/', ':'])
        || !text.bytes().any(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    text.strip_prefix('/')
        .unwrap_or(text)
        .split(['.', '-', '/', ':'])
        .all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

/// Complete, unescaped quoted first arguments with a named call site.
/// Quoted data and comments cannot supply a nested apparent call.
fn call_literals(text: &str) -> Vec<CallLiteral<'_>> {
    let mut chars = text.char_indices().peekable();
    let mut calls = Vec::new();
    while let Some((start, mark)) = chars.next() {
        if mark == '#' || (mark == '/' && chars.peek().is_some_and(|(_, ch)| *ch == '/')) {
            for (_, ch) in chars.by_ref() {
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }
        if mark == '/' && chars.peek().is_some_and(|(_, ch)| *ch == '*') {
            chars.next();
            while let Some((_, ch)) = chars.next() {
                if ch == '*' && chars.peek().is_some_and(|(_, next)| *next == '/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        if !matches!(mark, '"' | '\'' | '`') {
            continue;
        }
        let mut escaped = false;
        let end = loop {
            match chars.next() {
                Some((_, '\\')) => {
                    escaped = true;
                    chars.next();
                }
                Some((end, ch)) if ch == mark => break Some(end),
                Some(_) => {}
                None => break None,
            }
        };
        let Some(end) = end else { break };
        let literal = &text[start + 1..end];
        if escaped
            || !punctuated_literal(literal)
            || !text[end + 1..].trim_start().starts_with([',', ')'])
        {
            continue;
        }
        let prefix = text[..start].rsplit('\n').next().unwrap_or("").trim_end();
        if let Some(callee) = prefix.strip_suffix('(')
            && let Some(site) = callee
                .trim_end()
                .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '.' | ':'))
                .next()
            && site.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
        {
            calls.push(CallLiteral {
                text: literal,
                site,
            });
        }
    }
    calls
}

fn source_read(record: &Record) -> bool {
    !record.failure && matches!(record.operation, v3::LoopRead::ReadFile | v3::LoopRead::Context)
}

/// This exact call shape is used exclusively for join discovery. Keep its
/// canonical result intact, including candidates that miss the full literal.
pub(super) fn is_join_search(record: &Record) -> bool {
    record.operation == v3::LoopRead::Search
        && record.kind == FollowUp::Literal
        && record.arguments.get("mode").and_then(Value::as_str) == Some("lexical")
}

pub(super) fn only_join_discovery(gather: &Gathering, part: u8, need: u8, round: u8) -> bool {
    let mut records = gather
        .records
        .iter()
        .filter(|record| !record.failure && record.used_by(part, Some(need), round));
    records.next().is_some_and(is_join_search) && records.all(is_join_search)
}

fn usable_join_span(hit: &Hit) -> bool {
    !hit.truncated
        && matches!(hit.citation.location, airk_core::DocumentLocation::Lines { .. })
        && !software::is_document_path(&hit.citation.path)
        && crate::workbench::safe_path(&hit.citation.path)
        && airk_core::index_catalog::relative_path(&hit.citation.path, false).is_ok()
        && hit.citation.start_line > 0
        && hit.citation.end_line >= hit.citation.start_line
        && hit.citation.end_line <= u32::MAX as usize
        && hit.citation.content_hash.parse::<Sha256Digest>().is_ok()
}

fn contrasting_call(origin: &Hit, literal: &CallLiteral<'_>, target: &Hit) -> bool {
    let (left, right) = (&origin.citation, &target.citation);
    usable_join_span(target)
        && (left.path != right.path
            || left.end_line < right.start_line
            || right.end_line < left.start_line)
        && call_literals(&target.text)
            .iter()
            .any(|other| other.text == literal.text && other.site != literal.site)
}

enum JoinAction {
    Read(airk_core::Citation),
    Search(String),
}

fn confirm_discovery(
    gather: &Gathering,
    part: u8,
    need: u8,
    discovery: &Record,
) -> Option<JoinAction> {
    let term = discovery.arguments.get("query")?.as_str()?;
    let targets = super::coverage::record_hits(discovery);
    for (_, origin) in gather
        .records
        .iter()
        .rev()
        .filter(|record| {
            source_read(record)
                && record
                    .uses
                    .iter()
                    .any(|use_| use_.part == part && use_.need == Some(need))
        })
        .take(MAX_JOIN_ORIGINS)
        .flat_map(super::coverage::record_hits)
        .filter(|(_, hit)| usable_join_span(hit))
    {
        for literal in call_literals(&origin.text).into_iter().filter(|literal| literal.text == term) {
            if let Some((_, target)) = targets
                .iter()
                .find(|(_, target)| contrasting_call(&origin, &literal, target))
            {
                return Some(JoinAction::Read(target.citation.clone()));
            }
        }
    }
    None
}

fn join_actions(gather: &Gathering, part: u8, need: &Need, spans: &[Span]) -> Vec<JoinAction> {
    if !matches!(
        need.kind,
        NeedKind::Implementation
            | NeedKind::Interfaces
            | NeedKind::Callers
            | NeedKind::Errors
    ) {
        return Vec::new();
    }
    if let Some(discovery) = gather
        .records
        .iter()
        .rev()
        .find(|record| {
            record
                .uses
                .iter()
                .any(|use_| use_.part == part && use_.need == Some(need.id))
        })
        .filter(|record| !record.failure && is_join_search(record))
    {
        return confirm_discovery(gather, part, need.id, discovery)
            .into_iter()
            .collect();
    }
    let mut seen = BTreeSet::new();
    let mut actions = Vec::new();
    for source in spans.iter().filter(|span| {
        usable_join_span(&span.hit)
            && gather.records.get(span.record).is_some_and(|record| {
                source_read(record) && record.used_by(part, Some(need.id), span.round)
            })
    }) {
        for literal in call_literals(&source.hit.text) {
            if !seen.insert(literal.text) {
                continue;
            }
            let target = spans
                .iter()
                .find(|target| contrasting_call(&source.hit, &literal, &target.hit));
            actions.push(match target {
                Some(target) => JoinAction::Read(target.hit.citation.clone()),
                None => JoinAction::Search(literal.text.to_owned()),
            });
            if actions.len() == MAX_JOIN_LITERALS {
                return actions;
            }
        }
    }
    actions
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
    for action in join_actions(gather, part, need, spans) {
        match action {
            JoinAction::Read(citation) => {
                push(
                    &mut out,
                    ("read_file", v3::LoopRead::ReadFile),
                    json!({"path":citation.path,"start_line":citation.start_line,
                           "end_line":citation.end_line,"known_hash":citation.content_hash,
                           "max_bytes":software::Shape::PRODUCT.read_bytes}),
                    (FollowUp::Excerpt, Step::Leaf),
                    &format!("{}:{}-{}", citation.path, citation.start_line, citation.end_line),
                    hop,
                )?;
            }
            JoinAction::Search(literal) => {
                push(
                    &mut out,
                    ("search", v3::LoopRead::Search),
                    json!({"query":literal,"mode":"lexical","limit":4,
                           "max_bytes":4096,"full_chunks":true}),
                    (FollowUp::Literal, Step::Leaf),
                    &literal,
                    v3::CardClass::FollowUpQuery,
                )?;
            }
        }
    }
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
    // A truncated or snippet-only top span: read its whole chunk, within
    // what its location admits (`software::whole_chunk_after`).
    if let Some((span, after)) = top
        .filter(|span| {
            span.hit.truncated
                || span
                    .hit
                    .citation
                    .end_line
                    .saturating_sub(span.hit.citation.start_line)
                    < 8
        })
        .and_then(|span| {
            software::whole_chunk_after(&span.hit, software::Shape::PRODUCT.whole_chunk_lines)
                .map(|after| (span, after))
        })
    {
        let citation = &span.hit.citation;
        push(
            &mut out,
            ("context", v3::LoopRead::Context),
            json!({"path":citation.path,"line":citation.start_line,"before":0,"after":after,"max_bytes":software::Shape::PRODUCT.read_bytes}),
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
