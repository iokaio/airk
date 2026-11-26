// SPDX-License-Identifier: Apache-2.0
//! Plan (step 1): a part becomes one to four sub-needs, each with its own
//! short query wording. Proposals come only from the part's text, in a fixed
//! priority order; a model may choose several, and with no model the first
//! ones in priority order are taken.
use super::retrieve::{Need, NeedKind};
use super::*;
use airk_core::native_loop::LoopCard;

/// At most this many proposals, beside the reference card.
pub(crate) const MAX_PROPOSALS: usize = 7;
/// The reference card's text; a model chooses the cards scored above it.
pub(crate) const REFERENCE: &str = airk_retrieval::arbiter::loop_plan::REFERENCE;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Proposal {
    pub(crate) kind: NeedKind,
    pub(crate) subject: Option<String>,
    pub(crate) query: String,
    /// Named in the whole request, not in this part: lowest priority.
    pub(crate) inherited: bool,
}

const EXTENSIONS: [&str; 16] = [
    ".rs", ".md", ".ts", ".tsx", ".js", ".py", ".toml", ".json", ".cs", ".go", ".java", ".yml",
    ".yaml", ".sh", ".ps1", ".txt",
];

/// Closed cue lists, never drawn from the evaluation scenarios.
const CUES: [(NeedKind, &[&str]); 7] = [
    (
        NeedKind::Tests,
        &["test", "tests", "qa", "coverage", "assert"],
    ),
    (
        NeedKind::Documentation,
        &["readme", "doc", "docs", "document", "write"],
    ),
    (
        NeedKind::Contract,
        &[
            "contract",
            "contracts",
            "rule",
            "rules",
            "invariant",
            "must",
            "govern",
            "governs",
        ],
    ),
    (
        NeedKind::Configuration,
        &[
            "config", "setting", "settings", "flag", "flags", "env", "option", "options",
        ],
    ),
    (
        NeedKind::Errors,
        &[
            "error",
            "errors",
            "fail",
            "fails",
            "panic",
            "exception",
            "message",
        ],
    ),
    (
        NeedKind::Implementation,
        &["change", "add", "implement", "fix", "update"],
    ),
    (
        NeedKind::Interfaces,
        &["api", "interface", "import", "export"],
    ),
];

fn bounded(text: &str) -> String {
    let text = text.trim();
    if text.len() <= 200 {
        return text.to_owned();
    }
    let mut end = 200;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim_end().to_owned()
}

fn indexed_path_hint(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    (path.contains('/')
        || EXTENSIONS
            .iter()
            .any(|extension| lower.ends_with(extension)))
        && path.len() > 3
        && crate::workbench::safe_path(path)
        && path.chars().any(char::is_alphanumeric)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KnownAddress<'a> {
    pub(super) path: &'a str,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// A task-supplied relative indexed address, with inclusive one-based lines.
pub(super) fn known_address(token: &str) -> Option<KnownAddress<'_>> {
    let (path, lines) = token.rsplit_once(':')?;
    if !indexed_path_hint(path)
        || path.contains(['\\', '?', '#'])
        || path.split('/').any(str::is_empty)
    {
        return None;
    }
    let number = |value: &str| {
        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            value.parse::<u32>().ok().map(|line| line as usize)
        } else {
            None
        }
    };
    let (first, last) = lines.split_once('-').unwrap_or((lines, lines));
    let (start, end) = (number(first)?, number(last)?);
    (start > 0 && start <= end).then_some(KnownAddress { path, start, end })
}

fn path_hints(text: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    for raw in
        text.split(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | ',' | '(' | ')'))
    {
        let token = raw.trim_end_matches(['.', ';', '?', '!']);
        let (hint, path) = match known_address(token) {
            Some(address) => (token, address.path),
            None => {
                let path = raw.trim_end_matches(['.', ':', ';', '?', '!']);
                if !indexed_path_hint(path) {
                    continue;
                }
                (path, path)
            }
        };
        if !found.contains(&(hint, path)) {
            found.push((hint, path));
        }
    }
    found
}

/// `"..."` or `'...'` terms that are not backticked identifiers.
fn quoted(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for mark in ['"', '\''] {
        let mut rest = text;
        while let Some(start) = rest.find(mark) {
            let after = &rest[start + 1..];
            let Some(end) = after.find(mark) else { break };
            let term = after[..end].trim();
            // An apostrophe inside a word is not a quotation.
            let opening = start == 0 || !rest[..start].ends_with(|c: char| c.is_alphanumeric());
            if !opening {
                // An apostrophe is not an opening quote: move past it only.
                rest = &rest[start + 1..];
                continue;
            }
            if (3..=200).contains(&term.len()) && !found.iter().any(|seen| seen == term) {
                found.push(term.to_owned());
            }
            rest = &after[end + 1..];
        }
    }
    found
}

fn cues(text: &str) -> Vec<NeedKind> {
    let mut found = Vec::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let lower = word.to_ascii_lowercase();
        for (kind, list) in CUES {
            if list.contains(&lower.as_str()) && !found.contains(&kind) {
                found.push(kind);
            }
        }
    }
    found
}

/// The part's sub-need proposals in priority order, at most seven.
pub(crate) fn propose(part: &str, request: &str) -> Vec<Proposal> {
    let mut out: Vec<Proposal> = Vec::new();
    let push =
        |out: &mut Vec<Proposal>, kind, subject: Option<String>, query: String, inherited| {
            let query = bounded(&query);
            let explicit = kind == NeedKind::Implementation
                && subject.as_deref().and_then(known_address).is_some();
            if !query.is_empty()
                && out.len() < MAX_PROPOSALS
                && !out.iter().any(|old| {
                    old.kind == kind && old.query == query && (!explicit || old.subject == subject)
                })
            {
                out.push(Proposal {
                    kind,
                    subject,
                    query,
                    inherited,
                });
            }
        };
    let identifiers = software::identifiers(part, 3);
    let nouns = software::key_nouns(part, 4);
    let noun_phrase = |count: usize| {
        nouns
            .iter()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let subject = identifiers.first().cloned();
    // 1. Path hints.
    for (hint, path) in path_hints(part) {
        push(
            &mut out,
            NeedKind::Implementation,
            Some(hint.to_owned()),
            hint.to_owned(),
            false,
        );
        if let Some((folder, _)) = path.rsplit_once('/') {
            push(
                &mut out,
                NeedKind::Contract,
                Some(folder.to_owned()),
                format!("{folder} contract rules"),
                false,
            );
        }
    }
    // 2. Definitions of named identifiers.
    for name in &identifiers {
        push(
            &mut out,
            NeedKind::Definition,
            Some(name.clone()),
            name.clone(),
            false,
        );
    }
    // 3. Cue-word templates, in the order the cues appear.
    let topic = subject.clone().unwrap_or_else(|| noun_phrase(2));
    let literals = quoted(part);
    for kind in cues(part) {
        let query = match kind {
            NeedKind::Tests => format!("{topic} test"),
            NeedKind::Documentation => format!("{} documentation", noun_phrase(2)),
            NeedKind::Contract => format!("{topic} contract rules"),
            NeedKind::Configuration => format!("{} configuration", noun_phrase(2)),
            NeedKind::Errors => literals
                .first()
                .cloned()
                .unwrap_or_else(|| format!("{topic} error")),
            NeedKind::Interfaces => format!("{topic} interface"),
            _ => software::keyword_queries(part, 2)
                .first()
                .cloned()
                .unwrap_or_else(|| noun_phrase(2)),
        };
        let subject = match kind {
            NeedKind::Errors => literals.first().cloned(),
            _ => subject.clone(),
        };
        push(&mut out, kind, subject, query, false);
    }
    // 4. Quoted terms.
    for term in &literals {
        push(
            &mut out,
            NeedKind::Errors,
            Some(term.clone()),
            term.clone(),
            false,
        );
    }
    // 5. Callers of each identifier.
    for name in &identifiers {
        push(
            &mut out,
            NeedKind::Callers,
            Some(name.clone()),
            name.clone(),
            false,
        );
    }
    // 6. Identifiers the whole request names outside this part.
    if request != part {
        for name in software::identifiers(request, 3) {
            if !identifiers.contains(&name) {
                push(
                    &mut out,
                    NeedKind::Definition,
                    Some(name.clone()),
                    name,
                    true,
                );
            }
        }
    }
    // 7. An overview when little else was proposed.
    if out.len() < 2 {
        let query = software::keyword_queries(part, 2)
            .first()
            .cloned()
            .filter(|query| !query.is_empty())
            .unwrap_or_else(|| bounded(part));
        push(&mut out, NeedKind::Overview, None, query, false);
    }
    out
}

/// The deterministic plan: the first proposals in priority order.
pub(crate) fn deterministic(proposals: &[Proposal], max_sub_needs: usize) -> Vec<usize> {
    (0..proposals
        .len()
        .min(max_sub_needs.clamp(1, v3::MAX_SUB_NEEDS)))
        .collect()
}

/// A sub-need in words: its kind, its query and, when it differs, its
/// subject. A Plan card proposes a sub-need in this wording, and a Coverage
/// roster names the sub-need it judges the same way, so the model is told
/// what kind of evidence the sub-need asks for.
pub(crate) fn wording(kind: NeedKind, subject: Option<&str>, query: &str) -> String {
    match subject {
        Some(subject)
            if subject != query
                && !(kind == NeedKind::Implementation && known_address(subject).is_some()) =>
        {
            format!("need {}: {query} (subject {subject})", kind.name())
        }
        _ => format!("need {}: {query}", kind.name()),
    }
}

/// The roster: one card per proposal, then the reference card.
pub(crate) fn cards(proposals: &[Proposal]) -> Vec<LoopCard> {
    let symbols = ["A", "B", "C", "D", "E", "F", "G", "H"];
    proposals
        .iter()
        .enumerate()
        .map(|(index, proposal)| LoopCard {
            symbol: symbols[index].into(),
            class: v3::CardClass::PlanNeed,
            text: wording(proposal.kind, proposal.subject.as_deref(), &proposal.query),
            partition: vec![],
        })
        .chain(std::iter::once(LoopCard {
            symbol: symbols[proposals.len()].into(),
            class: v3::CardClass::PlanNeed,
            text: REFERENCE.into(),
            partition: vec![],
        }))
        .collect()
}

/// The chosen proposals as numbered sub-needs, in priority order.
pub(crate) fn needs(proposals: &[Proposal], chosen: &[usize]) -> Vec<Need> {
    let mut chosen: Vec<usize> = chosen
        .iter()
        .copied()
        .filter(|index| *index < proposals.len())
        .collect();
    chosen.sort_unstable();
    chosen.dedup();
    chosen
        .into_iter()
        .take(v3::MAX_SUB_NEEDS)
        .enumerate()
        .map(|(position, index)| Need {
            id: position as u8 + 1,
            kind: proposals[index].kind,
            subject: proposals[index].subject.clone(),
            query: proposals[index].query.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
