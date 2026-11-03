// SPDX-License-Identifier: Apache-2.0
//! Finite read composition. No transport recursion, source writes or background jobs.
use crate::{FactQuery, Kernel};
use airk_core::{
    SearchFilters, SearchQuery,
    discovery::{DiscoveryOptions, FactDiscoveryOptions},
    evidence::EvidenceReadOptions,
    references::ReferenceOptions,
    workbench::*,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

mod locations;
mod natural;
pub use locations::locations;

pub type PlanReader<'a> = dyn Fn(&str, &Map<String, Value>, Option<&str>) -> Result<Value> + 'a;

/// An adapter's complete failed read receipt, retained under the plan's byte budget.
#[derive(Debug)]
pub struct ReadFailure(pub Value);
impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ReadFailure {}

const TOOLS: &[&str] = &[
    "search",
    "symbols",
    "references",
    "read_file",
    "outline",
    "list_files",
    "file_info",
    "imports",
    "code_facts",
    "document_facts",
    "changes",
    "context",
    "relationships",
    "status",
    "diagnostics",
    "capabilities",
    "help",
];

#[derive(Debug)]
struct DefinitionChoice(Vec<String>);
impl std::fmt::Display for DefinitionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Multiple definition paths require an explicit choice.")
    }
}
impl std::error::Error for DefinitionChoice {}

pub fn safe_path(path: &str) -> bool {
    path.len() <= 4096
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && !path
            .replace('\\', "/")
            .split('/')
            .any(|p| p == ".." || p == ".")
}
fn string<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{key} requires text"))
}
fn number(args: &Map<String, Value>, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(default)
}
fn numeric_max(key: &str) -> u64 {
    match key {
        "limit" => 100,
        "hops" => 3,
        "before" | "after" => 40,
        "line" | "start_line" | "end_line" => u64::from(u32::MAX),
        _ => 65536,
    }
}
fn is_evidence(tool: &str) -> bool {
    !matches!(tool, "help" | "status" | "diagnostics" | "capabilities")
}
fn step(id: &str, tool: &str, args: Value) -> Step {
    Step {
        id: id.into(),
        tool: tool.into(),
        args: args.as_object().unwrap().clone(),
        bind: vec![],
    }
}
fn binding(arg: &str, from: &str, select: &str) -> Binding {
    Binding {
        arg: arg.into(),
        from: from.into(),
        select: select.into(),
    }
}
fn recipe(recipe: &Recipe) -> Result<Vec<Step>> {
    ensure!(
        recipe
            .parameters
            .keys()
            .all(|k| matches!(k.as_str(), "name" | "kind" | "query")),
        "unknown recipe parameter"
    );
    let name = recipe
        .parameters
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut steps = match recipe.id.as_str() {
        "definition" => vec![
            step("definition", "symbols", json!({"name":name})),
            step("structure", "outline", json!({})),
            step("excerpt", "read_file", json!({})),
        ],
        "impact" | "tests" => vec![
            step("occurrences", "references", json!({"name":name})),
            step("context", "read_file", json!({})),
        ],
        "documents" => vec![
            step("statements", "document_facts", json!(recipe.parameters)),
            step("context", "read_file", json!({})),
        ],
        "component" => vec![
            step("inventory", "list_files", json!({})),
            step("declarations", "symbols", json!({})),
            step("dependencies", "imports", json!({})),
        ],
        "readiness" => vec![
            step("status", "status", json!({})),
            step("capabilities", "capabilities", json!({})),
        ],
        _ => bail!(
            "unknown recipe: definition, impact, tests, documents, component or readiness required"
        ),
    };
    if recipe.id == "definition" {
        steps[1]
            .bind
            .push(binding("path", "definition", "unique_definition_path"));
        steps[2]
            .bind
            .push(binding("location", "definition", "hit_locations"));
    } else if matches!(recipe.id.as_str(), "impact" | "tests" | "documents") {
        let source = steps[0].id.clone();
        steps[1]
            .bind
            .push(binding("location", &source, "hit_locations"));
    }
    Ok(steps)
}

/// Tokenize quotes without evaluating shell syntax. Slash commands share this parser.
fn tokens(text: &str) -> Result<Vec<String>> {
    let mut result = vec![];
    let mut word = String::new();
    let mut quote = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if Some(c) == quote {
            quote = None;
        } else if quote.is_some() {
            if c == '\\'
                && chars
                    .peek()
                    .is_some_and(|next| Some(*next) == quote || *next == '\\')
            {
                word.push(chars.next().unwrap());
            } else {
                word.push(c);
            }
        } else if (c == '"' || c == '\'') && word.is_empty() {
            quote = Some(c);
        } else if c.is_whitespace() {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    ensure!(quote.is_none(), "unclosed quote");
    if !word.is_empty() {
        result.push(word);
    }
    Ok(result)
}

/// A learned result is an inert proposal. Execution requires an explicit plan
/// submission after the caller reviews it; qualification never follows from a score.
fn proposed_range(words: &[String]) -> Option<(usize, usize, bool)> {
    let parse = |value: &str| {
        value
            .trim_matches(['?', '.', ',', ';'])
            .parse::<usize>()
            .ok()
    };
    let mut ranges = BTreeSet::new();
    for (i, word) in words.iter().enumerate() {
        let word = word.trim_end_matches(['?', '.', ',', ';']);
        if let Some((_, value)) = word.rsplit_once(':').filter(|(_, v)| {
            v.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | '—'))
        }) {
            let (a, b) = value.split_once(['-', '–', '—']).unwrap_or((value, value));
            ranges.insert((parse(a)?, parse(b)?));
        }
        if word.eq_ignore_ascii_case("first")
            && words
                .get(i + 2)
                .is_some_and(|word| word.eq_ignore_ascii_case("lines"))
        {
            let amount = if words.get(i + 1)?.eq_ignore_ascii_case("ten") {
                10
            } else {
                parse(words.get(i + 1)?)?
            };
            ranges.insert((1, amount));
        }
        if matches!(word.to_lowercase().as_str(), "line" | "lines")
            && let Some(value) = words.get(i + 1)
            && value.chars().next().is_some_and(|c| c.is_ascii_digit())
            && let Some((a, b)) = value.split_once(['-', '–', '—'])
        {
            ranges.insert((parse(a)?, parse(b)?));
            continue;
        }
        if matches!(word.to_lowercase().as_str(), "line" | "lines")
            && let Some(a) = words.get(i + 1).and_then(|word| parse(word))
        {
            let b = if words
                .get(i + 2)
                .is_some_and(|word| matches!(word.as_str(), "to" | "through" | "-"))
            {
                parse(words.get(i + 3)?)?
            } else {
                a
            };
            ranges.insert((a, b));
        }
    }
    if ranges.len() > 1 {
        return None;
    }
    let defaulted = ranges.is_empty();
    let range = ranges.first().copied().unwrap_or((1, 200));
    (range.0 > 0 && range.0 <= range.1 && range.1 <= u32::MAX as usize)
        .then_some((range.0, range.1, defaulted))
}

fn proposed_steps(text: &str, intent: &str) -> Result<Vec<Step>> {
    let words = tokens(text)?;
    let lower = text.to_lowercase();
    let cues = [
        "show",
        "get",
        "give",
        "retrieve",
        "return",
        "display",
        "provide",
        "look",
        "inspect",
        "fetch",
        "find",
        "locate",
        "read",
        "open",
        "list",
        "enumerate",
        "browse",
        "search",
        "collect",
        "check",
        "report",
        "validate",
        "compare",
        "explain",
        "help",
        "what",
        "where",
        "which",
        "how",
        "is",
        "are",
        "has",
        "does",
        "can",
        "could",
        "please",
        "tell",
        "know",
        "learn",
        "interested",
        "what's",
        "where's",
        "who's",
        "definition",
        "declaration",
        "references",
        "usages",
        "contents",
        "outline",
        "imports",
        "requirements",
        "decisions",
        "metadata",
        "hash",
        "language",
        "profile",
        "files",
    ];
    if !words
        .iter()
        .any(|word| cues.contains(&word.to_lowercase().as_str()))
    {
        return Ok(vec![]);
    }
    let quoted = ['"', '`'].iter().find_map(|quote| {
        let start = text.find(*quote)? + quote.len_utf8();
        let end = text[start..].find(*quote)? + start;
        (end > start).then(|| text[start..end].to_string())
    });
    let paths: Vec<_> = words
        .iter()
        .filter_map(|word| {
            let path = word
                .trim_matches(['`', '"', '\''])
                .trim_end_matches(['?', '.', ',', ';']);
            let path = path
                .rsplit_once(':')
                .filter(|(_, range)| range.chars().all(|c| c.is_ascii_digit() || c == '-'))
                .map_or(path, |(path, _)| path);
            let extension = path.rsplit_once('.').is_some_and(|(_, ext)| {
                let named_file = [
                    "rs", "py", "ts", "tsx", "js", "jsx", "md", "mdx", "json", "toml", "yaml",
                    "yml", "go", "cs", "java", "cpp", "cc", "c", "h", "hpp", "sql", "html", "css",
                    "scss", "vue", "svelte", "astro", "rst", "txt", "pdf", "docx", "pptx", "xlsx",
                    "ipynb", "proto", "xml", "sh", "ps1", "lua", "swift", "kt", "rb", "php",
                ]
                .contains(&ext.to_lowercase().as_str());
                !ext.is_empty()
                    && ext.len() <= 8
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
                    && (named_file || matches!(intent, "read_file" | "outline" | "file_info"))
            });
            (safe_path(path)
                && (path.contains('/')
                    || path.contains('\\')
                    || (path.starts_with('.') && path.len() > 1)
                    || extension
                    || matches!(path, "README" | "LICENSE")))
            .then(|| path.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let path = (paths.len() == 1).then(|| paths[0].clone());
    let name = quoted
        .or_else(|| {
            let names: Vec<_> = words
                .iter()
                .map(|word| word.trim_matches(['`', '"', '\'', '?', '.', ',', ';']))
                .filter(|word| {
                    !word.contains('/')
                        && !word.contains('\\')
                        && (word.contains('_')
                            || word.contains("::")
                            || word.chars().skip(1).any(char::is_uppercase))
                })
                .filter(|word| !matches!(*word, "AIRK" | "MCP" | "CLI" | "SHA-256"))
                .collect();
            (names.len() == 1).then(|| names[0].to_string())
        })
        .or_else(|| {
            [
                " about ",
                " concerning ",
                " regarding ",
                " named ",
                " called ",
                " matching ",
                " of ",
                " for ",
                " to ",
            ]
            .iter()
            .filter_map(|marker| lower.rfind(marker).map(|at| (at, marker.len())))
            .max_by_key(|(at, _)| *at)
            .map(|(at, len)| {
                text[at + len..]
                    .trim()
                    .trim_end_matches(['?', '.'])
                    .to_string()
            })
        })
        .filter(|name| !name.is_empty())
        .map(|name| symbol_subject(&name).map_or(name, |(name, _)| name));
    let result = match intent {
        "status" | "diagnostics" | "help" => Some(step("proposal", intent, json!({}))),
        "search" => name.map(|query| step("proposal", "search", json!({"query":query}))),
        "list_files" => {
            let root_requested = [
                " in this project",
                " in the project",
                " in this collection",
                " in the collection",
            ]
            .iter()
            .any(|suffix| lower.trim_end_matches(['?', '.']).ends_with(suffix));
            if root_requested {
                return Ok(vec![step("proposal", "list_files", json!({}))]);
            }
            let scope = path.or_else(|| proposed_scope(text));
            scope
                .map(|prefix| step("proposal", "list_files", json!({"prefix":prefix})))
                .or_else(|| {
                    (!lower.contains(" under ")
                        && !lower.contains(" beneath ")
                        && !lower.contains(" in "))
                    .then(|| step("proposal", "list_files", json!({})))
                })
        }
        "find_definition" | "references" => name.map(|name| {
            step(
                "proposal",
                if intent == "find_definition" {
                    "symbols"
                } else {
                    "references"
                },
                json!({"name":name}),
            )
        }),
        "imports" | "facts" => {
            let args = path
                .map(|path| json!({"path":path}))
                .or_else(|| name.map(|name| json!({"name":name})));
            args.map(|args| {
                step(
                    "proposal",
                    if intent == "facts" {
                        "document_facts"
                    } else {
                        "imports"
                    },
                    args,
                )
            })
        }
        "read_file" | "outline" | "file_info" => path.and_then(|path| {
            let range = if intent == "read_file" {
                proposed_range(&words)?
            } else {
                (1, 200, false)
            };
            Some(step(
                "proposal",
                intent,
                if intent == "read_file" {
                    json!({"path":path,"start_line":range.0,"end_line":range.1})
                } else {
                    json!({"path":path})
                },
            ))
        }),
        "changes" => words
            .iter()
            .map(|word| word.trim_end_matches(['?', '.', ',', ';']))
            .find(|word| word.parse::<airk_core::identity::RevisionToken>().is_ok())
            .map(|since| step("proposal", "changes", json!({"since":since}))),
        _ => None,
    };
    Ok(result.into_iter().collect())
}

/// Literal file requests can still offer a review-only proposal when unfamiliar
/// filenames lower the learned margin. This never changes model acceptance.
fn proposed_scope(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    [" under ", " beneath ", " in "]
        .iter()
        .filter_map(|marker| lower.rfind(marker).map(|at| (at, marker.len())))
        .max_by_key(|(at, _)| *at)
        .map(|(at, len)| {
            text[at + len..]
                .trim()
                .trim_matches(['\'', '"', '`', '?'])
                .to_string()
        })
        .filter(|scope| !scope.is_empty() && safe_path(scope))
}

fn reviewable_file_request(text: &str, intent: &str) -> Result<bool> {
    let words: Vec<_> = tokens(text)?
        .iter()
        .map(|word| word.trim_matches(['?', '.', ',', ';']).to_lowercase())
        .collect();
    if words.iter().any(|word| {
        [
            "delete",
            "edit",
            "rewrite",
            "refactor",
            "execute",
            "run",
            "install",
            "upload",
            "download",
            "commit",
            "push",
            "deploy",
            "summarize",
            "implement",
            "fix",
            "repair",
            "unregister",
            "reindex",
            "stop",
        ]
        .contains(&word.as_str())
    }) {
        return Ok(false);
    }
    let action = words.iter().any(|word| {
        [
            "show",
            "read",
            "get",
            "retrieve",
            "return",
            "display",
            "give",
            "provide",
            "inspect",
            "fetch",
            "list",
            "enumerate",
            "browse",
        ]
        .contains(&word.as_str())
    });
    let nouns: &[&str] = match intent {
        "read_file" => &["content", "contents", "text", "excerpt", "lines", "read"],
        "outline" => &["outline", "headings", "structure", "declarations"],
        "file_info" => &["metadata", "hash", "language", "parser", "information"],
        "list_files" => &["files", "filenames", "paths", "inventory", "directory"],
        _ => &[],
    };
    Ok(action && words.iter().any(|word| nouns.contains(&word.as_str())))
}

fn strip_phrase<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    text.get(..phrase.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(phrase))
        .map(|_| &text[phrase.len()..])
}

fn strip_article(text: &str) -> &str {
    ["the ", "a ", "an "]
        .into_iter()
        .find_map(|article| strip_phrase(text, article))
        .unwrap_or(text)
}

/// Recognize a complete information request before consulting the suggestion-only
/// classifier. Consume the entire subject: a type word is never part of its name,
/// and a second action must not be silently discarded to recover one identifier.
fn information_steps(text: &str) -> Result<Option<Vec<Step>>> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = normalized.trim_end_matches(['?', '.', '!']).trim();
    let text = strip_phrase(text, "please ").unwrap_or(text);
    let text = strip_phrase(text, "can you ")
        .or_else(|| strip_phrase(text, "could you "))
        .unwrap_or(text);
    let text = strip_phrase(text, "please ").unwrap_or(text);
    let Some((subject, topic_allowed)) = [
        ("i want to know about ", true),
        ("i would like to know about ", true),
        ("i'd like to know about ", true),
        ("tell me about ", true),
        ("give me information about ", true),
        ("give me details about ", true),
        ("show me information about ", true),
        ("show me details about ", true),
        ("what is ", true),
        ("what's ", true),
        ("find ", true),
        ("locate ", true),
        ("show me ", false),
        ("describe ", false),
        ("explain ", false),
    ]
    .into_iter()
    .find_map(|(prefix, topic)| strip_phrase(text, prefix).map(|tail| (tail, topic))) else {
        return Ok(None);
    };
    let subject = [
        " in this codebase",
        " in this project",
        " in the project",
        " in this repository",
        " in the repository",
        " in this collection",
        " for me",
    ]
    .into_iter()
    .find_map(|suffix| {
        let at = subject.len().checked_sub(suffix.len())?;
        subject
            .get(at..)
            .filter(|s| s.eq_ignore_ascii_case(suffix))
            .map(|_| &subject[..at])
    })
    .unwrap_or(subject);
    let subject = strip_article(subject).trim();
    let definition = strip_phrase(subject, "definition of ")
        .or_else(|| strip_phrase(subject, "declaration of "));
    let subject = definition.unwrap_or(subject);
    let symbol = if definition.is_some() {
        symbol_subject(&format!("symbol {subject}"))
    } else {
        symbol_subject(subject)
    };
    if let Some((name, explicit)) = symbol {
        let steps = if explicit {
            recipe(&Recipe {
                id: "definition".into(),
                parameters: Map::from_iter([("name".into(), json!(name))]),
            })?
        } else {
            // Capitalization alone cannot distinguish a code symbol from a topic
            // such as Rust. Collect both without guessing a definition path.
            vec![
                step("definition", "symbols", json!({"name":name})),
                step("mentions", "search", json!({"query":name})),
            ]
        };
        return Ok(Some(steps));
    }
    if definition.is_some() {
        // Keep an explicit symbol operation intact. The bounded existing name
        // resolver can turn "queue client" into an indexed exact identifier;
        // unresolved or ambiguous candidates still require input before execution.
        let words: Vec<_> = subject.split_whitespace().collect();
        if (2..=4).contains(&words.len())
            && words.iter().all(|word| {
                word.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && ![
                        "and", "or", "then", "also", "delete", "fix", "edit", "run", "under",
                        "beneath", "within",
                    ]
                    .contains(&word.to_lowercase().as_str())
            })
        {
            return Ok(Some(vec![step(
                "definition",
                "symbols",
                json!({"name":subject}),
            )]));
        }
        return Ok(None);
    }
    // General topics remain literal text searches. Leave incomplete selections,
    // compound tasks, file operations and reasoning requests to explicit review.
    let subject = unquote(subject);
    let words: Vec<_> = subject.split_whitespace().collect();
    if topic_allowed
        && !words.is_empty()
        && !(words.len() == 1 && ["test", "document"].contains(&words[0].to_lowercase().as_str()))
        && words.iter().all(|word| {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            !word.is_empty()
                && ![
                    "it",
                    "this",
                    "that",
                    "them",
                    "something",
                    "anything",
                    "one",
                    "we",
                    "discussed",
                    "earlier",
                    "last",
                    "changes",
                    "and",
                    "or",
                    "then",
                    "also",
                    "why",
                    "how",
                    "whether",
                    "better",
                    "best",
                    "delete",
                    "remove",
                    "edit",
                    "rewrite",
                    "refactor",
                    "execute",
                    "run",
                    "install",
                    "upload",
                    "download",
                    "commit",
                    "push",
                    "deploy",
                    "summarize",
                    "implement",
                    "fix",
                    "repair",
                    "unregister",
                    "reindex",
                    "stop",
                    "struct",
                    "class",
                    "enum",
                    "trait",
                    "interface",
                    "function",
                    "method",
                    "type",
                    "file",
                    "files",
                    "contents",
                    "lines",
                    "outline",
                    "imports",
                    "references",
                    "uses",
                    "usages",
                    "occurrences",
                    "dependencies",
                    "status",
                    "diagnostics",
                ]
                .contains(&word.to_lowercase().as_str())
        })
        && !subject.contains([';', '\n', '\r', '"', '\'', '`', '?', '!'])
    {
        return Ok(Some(vec![step(
            "query",
            "search",
            json!({"query":subject}),
        )]));
    }
    Ok(None)
}

fn unquote(text: &str) -> &str {
    for quote in ['"', '\'', '`'] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|s| s.strip_suffix(quote)) {
            return inner;
        }
    }
    text
}

fn symbol_subject(subject: &str) -> Option<(String, bool)> {
    let unquoted = unquote(subject);
    let quoted_subject = unquoted != subject;
    let subject = if quoted_subject {
        unquoted
    } else {
        strip_article(unquoted)
    };
    let kinds = [
        "struct",
        "class",
        "enum",
        "trait",
        "interface",
        "type",
        "function",
        "method",
        "module",
        "constant",
        "variable",
        "symbol",
    ];
    let mut typed = false;
    let mut name = subject;
    for kind in kinds {
        if let Some(tail) = strip_phrase(subject, &format!("{kind} ")) {
            name = strip_phrase(tail, "named ")
                .or_else(|| strip_phrase(tail, "called "))
                .unwrap_or(tail);
            typed = true;
            break;
        }
        if let Some(at) = subject.len().checked_sub(kind.len() + 1)
            && subject
                .get(at..)
                .is_some_and(|s| s.eq_ignore_ascii_case(&format!(" {kind}")))
        {
            name = &subject[..at];
            typed = true;
            break;
        }
    }
    let unquoted = unquote(name);
    let quoted = unquoted != name;
    name = unquoted;
    let identifier = name
        .split("::")
        .flat_map(|part| part.split('.'))
        .all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        });
    let distinctive =
        name.contains(['_', '.']) || name.contains("::") || name.chars().any(char::is_uppercase);
    (identifier
        && (typed || quoted || quoted_subject || distinctive)
        && !["it", "this", "that", "them", "something", "anything", "i"]
            .contains(&name.to_lowercase().as_str()))
    .then(|| (name.to_string(), typed || quoted || quoted_subject))
}

/// Resolve an explicit prose scope once, before any operation can flatten its
/// words into a symbol or topic. Quoted literals and identifiers named `under`
/// remain targets; every operation still passes normal scope validation later.
fn scoped_prose(text: &str, words: &[String]) -> Result<Option<(Vec<Step>, Vec<String>)>> {
    let lower = text.to_ascii_lowercase();
    for (index, word) in words.iter().enumerate() {
        let marker = word.to_ascii_lowercase();
        if !["under", "beneath", "within"].contains(&marker.as_str()) {
            continue;
        }
        if index > 0
            && [
                "of", "to", "for", "named", "called", "calls", "is", "mention", "mentions",
            ]
            .contains(&words[index - 1].to_ascii_lowercase().as_str())
        {
            continue;
        }
        let Some((at, _)) = lower.rmatch_indices(marker.as_str()).find(|(at, _)| {
            text[..*at]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
                && text[*at + marker.len()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
                && tokens(&text[..*at]).is_ok()
                && text[..*at].chars().filter(|c| *c == '`').count() % 2 == 0
        }) else {
            continue;
        };
        let requirement = || {
            Some((vec![], vec!["Provide one terminal literal collection-relative path prefix for this scope, or use an explicit plan. Parent traversal and absolute paths are unavailable.".into()]))
        };
        let Some(path) = words.get(index + 1) else {
            return Ok(requirement());
        };
        let path = path.trim_end_matches(['?', '.', ',', ';', '!']);
        if path.is_empty() || !safe_path(path) {
            return Ok(requirement());
        }
        if let Some(steps) = natural::explicit_steps(text)?
            && !steps.is_empty()
            && steps.iter().all(|step| {
                let key = if step.tool == "list_files" {
                    "prefix"
                } else {
                    "path_prefix"
                };
                !step.bind.is_empty()
                    || step
                        .args
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|prefix| prefix.starts_with(path))
                    || step
                        .args
                        .get("path")
                        .and_then(Value::as_str)
                        .is_some_and(|file| file.starts_with(path))
            })
        {
            return Ok(Some((steps, vec![])));
        }
        if index + 2 != words.len() {
            // Changes may place the exact baseline after the path. Its complete
            // grammar must consume both, rather than ignoring an arbitrary tail.
            if let Some(steps) = natural::explicit_steps(text)?
                && steps.iter().all(|step| {
                    step.tool == "changes"
                        && step.args.get("path_prefix").and_then(Value::as_str) == Some(path)
                })
            {
                return Ok(Some((steps, vec![])));
            }
            return Ok(requirement());
        }
        let base = text[..at].trim_end();
        if base.is_empty() {
            return Ok(requirement());
        }
        let (mut steps, mut needs) = parse_text(base)?;
        for step in &mut steps {
            if step
                .args
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|file| !file.starts_with(path))
            {
                needs.push("The requested file lies outside the prose path scope.".into());
                continue;
            }
            let key = if step.tool == "list_files" {
                "prefix"
            } else {
                "path_prefix"
            };
            if fields(&step.tool).contains(&key) {
                let existing = step.args.get(key).and_then(Value::as_str).unwrap_or("");
                if !existing.starts_with(path) {
                    if path.starts_with(existing) {
                        step.args.insert(key.into(), json!(path));
                    } else {
                        needs.push("The request contains incompatible path scopes.".into());
                    }
                }
            } else if step.bind.is_empty() && !step.args.contains_key("path") {
                needs.push("This operation has no path-scoped representation; choose an evidence operation.".into());
            }
        }
        return Ok(Some((steps, needs)));
    }
    Ok(None)
}

pub fn parse_text(text: &str) -> Result<(Vec<Step>, Vec<String>)> {
    ensure!(
        !text.trim().is_empty() && text.len() <= 4096,
        "input requires 1..4096 bytes"
    );
    let mut words = tokens(text)?;
    let command = words
        .first()
        .context("input is empty")?
        .trim_start_matches('/')
        .to_lowercase();
    let explicit = text.trim_start().starts_with('/');
    // Slash syntax is the command grammar. A prose request beginning with
    // "read" or "search" still needs its language and selection parsed.
    let bare_metadata = words.len() == 1
        && matches!(
            command.as_str(),
            "status" | "diagnostics" | "capabilities" | "help"
        );
    let command = if explicit || bare_metadata {
        command
    } else {
        "natural-language".into()
    };
    let lower = text.to_lowercase();
    let tool = match command.as_str() {
        "search" => "search",
        "symbols" => "symbols",
        "refs" | "references" => "references",
        "read" => "read_file",
        "outline" => "outline",
        "files" => "list_files",
        "info" => "file_info",
        "imports" => "imports",
        "code-facts" | "code_facts" => "code_facts",
        "facts" => "document_facts",
        "changes" => "changes",
        "context" => "context",
        "relationships" => "relationships",
        "status" => "status",
        "diagnostics" => "diagnostics",
        "capabilities" => "capabilities",
        "help" => "help",
        _ if !explicit => {
            if let Some(parsed) = scoped_prose(text, &words)? {
                return Ok(parsed);
            }
            if let Some(steps) = natural::explicit_steps(text)? {
                return Ok((steps, vec![]));
            }
            let prefixes = [
                ("who calls ", "references"),
                ("references to ", "references"),
                ("usages of ", "references"),
                ("where is ", "symbols"),
                ("find definition of ", "symbols"),
                ("search for ", "search"),
            ];
            if let Some((prefix, tool)) = prefixes
                .iter()
                .find(|(prefix, _)| lower.starts_with(prefix))
            {
                let tail = text[prefix.len()..]
                    .trim()
                    .trim_end_matches(['?', '.', '!'])
                    .trim_end_matches(" defined")
                    .trim();
                let tail = strip_article(tail);
                let name = symbol_subject(tail).map_or_else(|| tail.to_string(), |(name, _)| name);
                return Ok((
                    vec![step(
                        "query",
                        tool,
                        if *tool == "search" {
                            json!({"query":tail})
                        } else {
                            json!({"name":name})
                        },
                    )],
                    vec![],
                ));
            }
            if let Some(steps) = information_steps(text)? {
                return Ok((steps, vec![]));
            }
            let prediction = airk_retrieval::intent::predict(text)?;
            if prediction.accepted || reviewable_file_request(text, prediction.intent)? {
                let proposal = proposed_steps(text, prediction.intent)?;
                if !proposal.is_empty() {
                    return Ok((proposal,vec![if prediction.accepted{"Review the proposed operation and inputs, then run the plan to collect evidence. Learned requests require explicit plan confirmation in this review build."}else{"This file request has a low-confidence intent. Review the proposed read operation, exact file and range before explicitly running the plan."}.into()]));
                }
            }
            let hint = if prediction.accepted {
                format!(
                    " Candidate intent: {} (score {}, margin {}), but its required target could not be extracted. Review an explicit command or recipe.",
                    prediction.intent, prediction.logit, prediction.margin
                )
            } else {
                format!(
                    " Candidate intent: {} did not meet the confidence requirements (score {}, margin {}).",
                    prediction.intent, prediction.logit, prediction.margin
                )
            };
            return Ok((
                vec![],
                vec![format!(
                    "No evidence steps ran: this wording needs a supported operation and an exact target.{hint} Try 'tell me about struct Owner', 'find the definition of Owner' or 'search for retry budgets', or choose a recipe."
                )],
            ));
        }
        _ => bail!("unknown slash command; use /help"),
    };
    words.remove(0);
    let mut args = Map::new();
    let mut positional = vec![];
    let mut i = 0;
    while i < words.len() {
        if words[i].starts_with("--") {
            let key = match words[i].as_str() {
                "--path" if tool == "list_files" => "prefix",
                "--path" => "path_prefix",
                "--mode" => "mode",
                "--limit" => "limit",
                "--since" => "since",
                "--bytes" => "max_bytes",
                "--language" => "language",
                "--evidence-kind" => "evidence_kind",
                "--exclude-path-prefix" => "exclude_path_prefix",
                "--name-match" => "name_match",
                "--include-count" => "include_count",
                "--cursor" => "cursor",
                "--order" => "order",
                "--include-files" => "include_files",
                "--compact" => "compact",
                "--reply-detail" => "reply_detail",
                "--kind" => "kind",
                flag => {
                    let name = flag.trim_start_matches("--").replace('-', "_");
                    fields(tool)
                        .iter()
                        .copied()
                        .chain(verification_fields(tool).iter().copied())
                        .find(|key| *key == name)
                        .context("unknown command option")?
                }
            };
            let schema = argument_schema(key);
            if schema["type"] == "boolean"
                || schema["type"]
                    .as_array()
                    .is_some_and(|types| types.contains(&json!("boolean")))
            {
                let explicit = words.get(i + 1).and_then(|value| match value.as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                });
                if explicit.is_some() {
                    i += 1;
                }
                args.insert(key.into(), json!(explicit.unwrap_or(true)));
                i += 1;
                continue;
            }
            i += 1;
            let value = words.get(i).context("option needs a value")?;
            ensure!(!value.starts_with("--"), "option needs a value");
            args.insert(
                key.into(),
                if argument_schema(key)["type"] == "integer" {
                    json!(value.parse::<usize>()?)
                } else if key == "selectors" {
                    serde_json::from_str(value)?
                } else {
                    json!(value)
                },
            );
        } else {
            positional.push(words[i].clone());
        }
        i += 1;
    }
    let value = positional.join(" ");
    match tool {
        "search" => {
            args.insert("query".into(), json!(value));
        }
        "symbols" | "references" | "imports" | "code_facts" => {
            if !value.is_empty() {
                args.insert("name".into(), json!(value));
            }
        }
        "read_file" | "outline" | "file_info" | "context" => {
            let (path, range) = value
                .rsplit_once(':')
                .filter(|(_, range)| range.chars().all(|c| c.is_ascii_digit() || c == '-'))
                .map_or((value.as_str(), None), |(p, r)| (p, Some(r)));
            args.insert("path".into(), json!(path));
            if tool == "read_file" {
                let (a, b) = range
                    .unwrap_or("1-200")
                    .split_once('-')
                    .unwrap_or((range.unwrap_or("1"), range.unwrap_or("1")));
                args.insert("start_line".into(), json!(a.parse::<usize>()?));
                args.insert("end_line".into(), json!(b.parse::<usize>()?));
            }
            if tool == "context" {
                args.insert("line".into(), json!(range.unwrap_or("1").parse::<usize>()?));
            }
        }
        "list_files" => {
            if !value.is_empty() {
                args.insert("prefix".into(), json!(value));
            }
        }
        "document_facts" => {
            if let Some(path) = positional.first() {
                args.insert("path".into(), json!(path));
            }
            if positional.len() > 1 {
                args.insert("name".into(), json!(positional[1..].join(" ")));
            }
        }
        "relationships" => {
            args.insert("symbol".into(), json!(value));
        }
        "help" if !value.is_empty() => {
            args.insert("topic".into(), json!(value));
        }
        _ => ensure!(
            value.is_empty() || tool == "help",
            "command takes no positional arguments"
        ),
    }
    Ok((
        vec![Step {
            id: "query".into(),
            tool: tool.into(),
            args,
            bind: vec![],
        }],
        vec![],
    ))
}

fn fields(tool: &str) -> &'static [&'static str] {
    match tool {
        "search" => &[
            "query",
            "semantic",
            "full_chunks",
            "mode",
            "path_prefix",
            "language",
            "evidence_kind",
            "exclude_path_prefix",
            "limit",
            "max_bytes",
        ],
        "symbols" | "imports" => &[
            "path",
            "name",
            "language",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
            "name_match",
            "include_count",
            "compact",
        ],
        "code_facts" => &[
            "symbol_key",
            "path",
            "name",
            "kind",
            "language",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
            "name_match",
            "include_count",
            "compact",
        ],
        "document_facts" => &[
            "document_key",
            "path",
            "name",
            "kind",
            "language",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
            "compact",
        ],
        "references" => &[
            "name",
            "path_prefix",
            "language",
            "evidence_kind",
            "exclude_path_prefix",
            "limit",
            "max_bytes",
            "cursor",
            "order",
            "include_files",
            "include_count",
        ],
        "read_file" => &[
            "if_evidence",
            "artifact_id",
            "follow_moves",
            "path",
            "start_line",
            "end_line",
            "known_hash",
            "revision",
            "max_bytes",
        ],
        "file_info" => &[
            "if_evidence",
            "artifact_id",
            "follow_moves",
            "path",
            "known_hash",
            "revision",
        ],
        "outline" => &[
            "if_evidence",
            "artifact_id",
            "follow_moves",
            "path",
            "known_hash",
            "revision",
            "limit",
            "max_bytes",
            "cursor",
            "compact",
        ],
        "context" => &[
            "if_evidence",
            "artifact_id",
            "follow_moves",
            "path",
            "line",
            "before",
            "after",
            "known_hash",
            "revision",
            "max_bytes",
        ],
        "list_files" => &["prefix", "limit", "max_bytes", "cursor"],
        "changes" => &[
            "since",
            "selectors",
            "include_symbols",
            "include_documents",
            "include_suggestions",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
        ],
        "capabilities" => &["adapter", "rich"],
        "help" => &["topic", "command", "tool", "start_line", "max_bytes"],
        "relationships" => &[
            "symbol",
            "direction",
            "kind",
            "hops",
            "path_prefix",
            "limit",
            "max_bytes",
        ],
        _ => &[],
    }
}
/// Forms and agents inspect the same closed argument inventory used by validation.
pub fn operation_catalog() -> Value {
    let operations:Map<String,Value>=TOOLS.iter().map(|tool|{
        let properties:Map<String,Value>=fields(tool).iter().copied().chain(std::iter::once("reply_detail")).chain(verification_fields(tool).iter().copied()).map(|key|{
            let schema = argument_schema(key);
            (key.to_string(),schema)
        }).collect();
        let required:Vec<&str>=match *tool {"search"=>vec!["query"],"references"=>vec!["name"],"read_file"=>vec!["path","start_line","end_line"],"file_info"|"outline"|"context"=>vec!["path"],"relationships"=>vec!["symbol"],_=>vec![]};
        (tool.to_string(),json!({"type":"object","additionalProperties":false,"properties":properties,"required":required}))
    }).collect();
    json!({"catalog":CATALOG,"operations":operations,"selectors":["unique_definition_path","hit_locations","selected_symbol_ids"],"recipes":["definition","documents","impact","tests","component","readiness"],"classifier":"candidate suggestions only"})
}

fn argument_schema(key: &str) -> Value {
    match key {
        "verification" => {
            json!({"type":"string","enum":["off","report","require"],"default":"report"})
        }
        "verification_max_sources" => {
            json!({"type":"integer","minimum":1,"maximum":256,"default":8})
        }
        "verification_max_bytes" => {
            json!({"type":"integer","minimum":1,"maximum":1073741824,"default":16777216})
        }
        "verification_deadline_ms" => {
            json!({"type":"integer","minimum":1,"maximum":30000,"default":2000})
        }
        "reply_detail" => {
            let mut values: Vec<Value> = airk_core::reply::ReplyDetail::ALL
                .into_iter()
                .map(|v| json!(v))
                .collect();
            values.push(Value::Null);
            json!({"type":["string","null"],"enum":values,"description":"Lean/Rich projection of available evidence; independent of retention. Omit to preserve Workbench defaults. Conflicting compact flags reject. Search full_chunks is independent."})
        }
        "limit" | "max_bytes" | "start_line" | "end_line" | "line" | "before" | "after"
        | "hops" => {
            json!({"type":"integer","minimum":if matches!(key,"before"|"after"){0}else{1},"maximum":numeric_max(key)})
        }
        "compact" | "full_chunks" => json!({"type":["boolean","null"]}),
        "include_count"
        | "include_files"
        | "semantic"
        | "follow_moves"
        | "rich"
        | "include_symbols"
        | "include_documents"
        | "include_suggestions" => json!({"type":"boolean"}),
        "selectors" => json!({"type":"array","maxItems":64,"items":{"type":"object"}}),
        "mode" => json!({"type":"string","enum":["lexical","vector","hybrid"]}),
        "direction" => json!({"type":"string","enum":["incoming","outgoing"]}),
        "name_match" => json!({"type":"string","enum":["exact","suffix","contains"]}),
        "evidence_kind" => {
            json!({"type":"string","enum":["code","document","configuration","unknown"]})
        }
        "order" => json!({"type":"string","enum":["index","path","code_first"]}),
        "cursor" => json!({"type":"string","minLength":1,"maxLength":32768}),
        "language" => json!({"type":"string","minLength":1,"maxLength":128}),
        "exclude_path_prefix" => json!({"type":"string","minLength":1,"maxLength":4096}),
        _ => json!({"type":"string","maxLength":4096}),
    }
}

fn search_filters(args: &Map<String, Value>) -> Result<SearchFilters> {
    let filters = SearchFilters {
        language: args
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned),
        evidence_kind: args
            .get("evidence_kind")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        exclude_path_prefix: args
            .get("exclude_path_prefix")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    filters.validate()?;
    Ok(filters)
}
fn verification_fields(tool: &str) -> &'static [&'static str] {
    if matches!(
        tool,
        "search"
            | "symbols"
            | "code_facts"
            | "document_facts"
            | "references"
            | "files"
            | "list_files"
            | "read_file"
            | "file_info"
            | "context"
            | "imports"
            | "outline"
            | "relationships"
            | "changes"
    ) {
        &[
            "verification",
            "verification_max_sources",
            "verification_max_bytes",
            "verification_deadline_ms",
        ]
    } else {
        &[]
    }
}
fn verification_controls(
    args: &Map<String, Value>,
) -> Result<airk_core::verification::VerificationControls> {
    Ok(serde_json::from_value(Value::Object(
        args.iter()
            .filter(|(key, _)| {
                matches!(
                    key.as_str(),
                    "verification"
                        | "verification_max_sources"
                        | "verification_max_bytes"
                        | "verification_deadline_ms"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    ))?)
}
fn reply_detail(args: &Map<String, Value>) -> Result<Option<airk_core::reply::ReplyDetail>> {
    args.get("reply_detail")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(Option::flatten)
        .map_err(Into::into)
}
fn validate_step(step: &Step, prefix: &str, bound: bool) -> Result<()> {
    ensure!(
        TOOLS.contains(&step.tool.as_str()),
        "operation is not read-only evidence"
    );
    verification_controls(&step.args)?.resolve()?;
    for (key, value) in &step.args {
        ensure!(
            key == "reply_detail"
                || fields(&step.tool).contains(&key.as_str())
                || verification_fields(&step.tool).contains(&key.as_str()),
            "unknown {} argument {key}",
            step.tool
        );
        if verification_fields(&step.tool).contains(&key.as_str()) {
            continue;
        }
        if key == "reply_detail" {
            serde_json::from_value::<Option<airk_core::reply::ReplyDetail>>(value.clone())?;
            continue;
        }
        if matches!(key.as_str(), "compact" | "full_chunks") && value.is_null() {
            continue;
        }
        if matches!(
            key.as_str(),
            "limit"
                | "max_bytes"
                | "start_line"
                | "end_line"
                | "line"
                | "before"
                | "after"
                | "hops"
        ) {
            let n = value.as_u64().context("integer argument required")?;
            ensure!(
                n <= numeric_max(key) && (n > 0 || matches!(key.as_str(), "before" | "after")),
                "numeric argument out of range"
            );
            if key == "limit" {
                ensure!(n <= 100, "limit exceeds 100");
            }
            if key == "hops" {
                ensure!(n <= 3, "hops exceeds 3");
            }
        } else if key == "selectors" {
            let selectors: Vec<airk_core::changes::RememberedSelector> =
                serde_json::from_value(value.clone())?;
            ensure!(
                selectors.len() <= 64 && serde_json::to_vec(value)?.len() <= 32768,
                "selector budget exceeded"
            );
            for selector in value.as_array().context("selectors require an array")? {
                if let Some(path) = selector.get("path").and_then(Value::as_str) {
                    ensure!(
                        safe_path(path) && path.starts_with(prefix),
                        "selector escapes plan scope"
                    );
                }
            }
        } else if matches!(
            key.as_str(),
            "include_count"
                | "include_files"
                | "compact"
                | "semantic"
                | "full_chunks"
                | "follow_moves"
                | "rich"
                | "include_symbols"
                | "include_documents"
                | "include_suggestions"
        ) {
            ensure!(value.is_boolean(), "boolean argument required");
        } else {
            ensure!(
                value
                    .as_str()
                    .is_some_and(|s| s.len() <= (if key == "cursor" { 32768 } else { 4096 })
                        && !s.chars().any(char::is_control)),
                "bounded text argument required"
            );
        }
        if matches!(key.as_str(), "path" | "prefix" | "path_prefix") {
            let p = value.as_str().unwrap();
            ensure!(
                safe_path(p) && p.starts_with(prefix),
                "path escapes plan scope"
            );
        }
        if key == "exclude_path_prefix" {
            ensure!(
                safe_path(value.as_str().unwrap()),
                "invalid excluded path prefix"
            );
        }
        if key == "cursor" {
            ensure!(!value.as_str().unwrap().is_empty(), "cursor is empty");
        }
    }
    if matches!(step.tool.as_str(), "search" | "references") {
        search_filters(&step.args)?;
    }
    let reply = reply_detail(&step.args)?;
    if matches!(
        step.tool.as_str(),
        "symbols" | "imports" | "outline" | "code_facts" | "document_facts"
    ) {
        airk_core::reply::ReplyDetail::compact(
            reply,
            step.args.get("compact").and_then(Value::as_bool),
        )?;
    }
    if let Some(mode) = step.args.get("name_match") {
        serde_json::from_value::<airk_core::discovery::NameMatch>(mode.clone())?;
        ensure!(
            step.args
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.is_empty()),
            "name_match requires a nonempty name"
        );
    }
    if let Some(order) = step.args.get("order") {
        serde_json::from_value::<airk_core::references::ReferenceOrder>(order.clone())?;
    }
    if let Some(mode) = step.args.get("mode") {
        ensure!(
            matches!(mode.as_str(), Some("lexical" | "vector" | "hybrid")),
            "invalid search mode"
        );
    }
    if let (Some(path), Some(scope)) = (
        step.args.get("path").and_then(Value::as_str),
        step.args.get("path_prefix").and_then(Value::as_str),
    ) {
        ensure!(
            path.starts_with(scope),
            "exact file escapes the step's narrowed scope"
        );
    }
    if step.args.contains_key("start_line") && step.args.contains_key("end_line") {
        ensure!(
            number(&step.args, "start_line", 1) <= number(&step.args, "end_line", 1),
            "line range is reversed"
        );
    }
    if let Some(value) = step.args.get("known_hash") {
        let hash = value.as_str().unwrap();
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid source hash"
        );
    }
    for field in ["since", "revision"] {
        if let Some(value) = step.args.get(field) {
            value
                .as_str()
                .unwrap()
                .parse::<airk_core::identity::RevisionToken>()?;
        }
    }
    if let Some(value) = step.args.get("direction") {
        ensure!(
            matches!(value.as_str(), Some("incoming" | "outgoing")),
            "invalid relationship direction"
        );
    }
    for required in match step.tool.as_str() {
        "search" => &["query"][..],
        "references" => &["name"],

        "relationships" if bound => &["symbol"],
        "read_file" if bound => &["path", "start_line", "end_line"],
        "outline" | "file_info" | "context" if bound => &["path"],
        _ => &[],
    } {
        ensure!(
            step.args.contains_key(*required),
            "{} requires {required}",
            step.tool
        );
        if let Some(s) = step.args[*required].as_str() {
            ensure!(!s.is_empty(), "{required} is empty");
        }
    }
    Ok(())
}
fn digest(plan: &Plan) -> Result<String> {
    let mut value = serde_json::to_value(plan)?;
    value.as_object_mut().unwrap().remove("digest");
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value)?)))
}

fn has_revision_bound_symbol_inputs(plan: &Plan) -> bool {
    plan.steps.iter().any(|step| {
        ["name", "symbol"].iter().any(|field| {
            step.args
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    ["syntax:", "scip:", "ctags:", "lsp:"].iter().any(|prefix| {
                        value
                            .strip_prefix(prefix)
                            .is_some_and(|tail| !tail.starts_with(':'))
                    })
                })
        })
    })
}

fn publication(value: &Value) -> Option<String> {
    value
        .get("publication")
        .and_then(|p| {
            p.get("token")
                .and_then(Value::as_str)
                .or_else(|| p.as_str())
        })
        .map(str::to_owned)
        .or_else(|| value.get("evidence").and_then(publication))
        .or_else(|| value.get("read").and_then(publication))
}
fn clipped(value: &Value) -> bool {
    match value {
        Value::Object(m) => {
            m.get("truncated") == Some(&Value::Bool(true))
                || m.get("cursor").is_some_and(|v| !v.is_null())
                || m.values().any(clipped)
        }
        Value::Array(a) => a.iter().any(clipped),
        _ => false,
    }
}

/// Reapply the output bound after a transport adds observations to a result.
pub fn bounded_result(mut result: ResultEnvelope) -> Result<ResultEnvelope> {
    // Preserve the existing whole-record removal order, with linear encoded
    // cost accounting rather than serializing the full result after every pop.
    let mut bytes = serde_json::to_vec(&result)?.len();
    if bytes <= result.plan.output.max_bytes {
        return Ok(result);
    }
    result.truncated = true;
    if !result
        .completeness
        .iter()
        .any(|reason| reason == "output clipped")
    {
        result.completeness.push("output clipped".into());
    }
    if result.status == "complete" {
        result.status = "partial".into();
    }
    bytes = serde_json::to_vec(&result)?.len();
    while bytes > result.plan.output.max_bytes {
        if let Some(record) = result.evidence.pop() {
            bytes -= serde_json::to_vec(&record)?.len() + usize::from(!result.evidence.is_empty());
        } else if let Some(report) = result
            .steps
            .iter_mut()
            .rev()
            .find(|report| report.result.is_some())
        {
            // Null can be larger than a malformed scalar result. Replacing
            // such a result must refuse safely rather than underflow sizing.
            bytes = bytes
                .saturating_sub(
                    serde_json::to_vec(&report.result.take().expect("selected result"))?.len(),
                )
                .saturating_add(4);
        } else {
            bail!("output budget cannot fit plan and reports");
        }
    }
    ensure!(
        serde_json::to_vec(&result)?.len() <= result.plan.output.max_bytes,
        "output budget accounting failed"
    );
    Ok(result)
}

fn fit_verification_frame(mut result: ResultEnvelope, reserve: usize) -> Result<ResultEnvelope> {
    let original = result.plan.output.max_bytes;
    let reserve = reserve.min(original / 2);
    let mut limit = original.saturating_sub(reserve + 2048);
    for _ in 0..20 {
        result.plan.output.max_bytes = limit;
        result = bounded_result(result)?;
        result.plan.output.max_bytes = original;
        let bytes = serde_json::to_vec(&result)?.len();
        let encoded = serde_json::to_vec(&serde_json::to_string(&result)?)?.len();
        if encoded + reserve * 2 + 2048 <= airk_core::MAX_RESPONSE_BYTES
            && bytes + reserve + 2048 <= original
        {
            return Ok(result);
        }
        limit = bytes.saturating_sub((bytes / 4).max(1024));
    }
    bail!(
        "verification reply framing cannot fit; reduce the plan size or increase its output budget"
    )
}

impl Kernel {
    pub fn evidence_plan(&self, request: Request) -> Result<ResultEnvelope> {
        self.evidence_plan_observed(request, &Value::Null)
    }
    pub fn evidence_plan_observed(
        &self,
        request: Request,
        observations: &Value,
    ) -> Result<ResultEnvelope> {
        self.evidence_plan_with_reader(request, observations, None)
    }

    /// Run the bounded composition with an adapter's canonical read implementation.
    /// The callback cannot add plan operators or bypass scope/revision/budget validation.
    pub fn evidence_plan_with_reader(
        &self,
        request: Request,
        observations: &Value,
        reader: Option<&PlanReader<'_>>,
    ) -> Result<ResultEnvelope> {
        ensure!(
            serde_json::to_vec(&request)?.len() <= 32768,
            "parse request exceeds 32 KiB"
        );
        ensure!(
            request.schema == REQUEST_SCHEMA,
            "unsupported parse request schema"
        );
        ensure!(
            usize::from(request.text.is_some())
                + usize::from(request.recipe.is_some())
                + usize::from(request.plan.is_some())
                == 1,
            "choose text, recipe or plan"
        );
        ensure!(safe_path(&request.path_prefix), "invalid plan scope");
        let started = Instant::now();
        let text_request = request.text.is_some();
        let mut needs = vec![];
        let mut choices = vec![];
        let mut plan = if let Some(plan) = request.plan {
            plan
        } else {
            let (steps, tier) = if let Some(recipe_request) = request.recipe {
                (recipe(&recipe_request)?, "recipe")
            } else {
                let (steps, missing) = parse_text(request.text.as_deref().unwrap())?;
                needs = missing;
                (
                    steps,
                    if request
                        .text
                        .as_deref()
                        .unwrap()
                        .trim_start()
                        .starts_with('/')
                    {
                        "command"
                    } else {
                        "parsed"
                    },
                )
            };
            Plan {
                schema: PLAN_SCHEMA.into(),
                tier: tier.into(),
                steps,
                path_prefix: request.path_prefix.clone(),
                ..Plan::default()
            }
        };
        ensure!(
            plan.schema.is_empty() || plan.schema == PLAN_SCHEMA,
            "unsupported plan schema"
        );
        plan.schema = PLAN_SCHEMA.into();
        ensure!(
            plan.collection.is_empty() || plan.collection == self.collection(),
            "plan belongs to another collection"
        );
        plan.collection = self.collection().into();
        ensure!(
            plan.catalog.is_empty() || plan.catalog == CATALOG,
            "plan catalog changed; prepare again"
        );
        plan.catalog = CATALOG.into();
        if !request.path_prefix.is_empty() {
            ensure!(
                plan.path_prefix.starts_with(&request.path_prefix),
                "plan widens requested scope"
            );
        }
        ensure!(safe_path(&plan.path_prefix), "invalid plan scope");
        if let Some(b) = request.budgets {
            plan.budgets = b;
        }
        if let Some(o) = request.output {
            plan.output = o;
        }
        use airk_core::verification::{VerificationControls, VerificationMode, VerificationScope};
        let global_explicit = request.verification.is_set() || plan.verification.is_set();
        plan.verification = plan.verification.with_overrides(request.verification);
        let root_budget = plan.verification.resolve()?;
        let own_step_policy = plan
            .steps
            .iter()
            .try_fold(false, |found, step| -> Result<bool> {
                Ok(found || verification_controls(&step.args)?.is_set())
            })?;
        let verification_scope = plan.verification_scope.unwrap_or(
            if own_step_policy && (!global_explicit || root_budget.is_none()) {
                VerificationScope::Steps
            } else {
                VerificationScope::Response
            },
        );
        plan.verification_scope = Some(verification_scope);
        plan.verification = VerificationControls::resolved(root_budget);
        let verification = if verification_scope == VerificationScope::Response
            && plan.steps.iter().any(|step| is_evidence(&step.tool))
        {
            root_budget
        } else {
            None
        };
        let mut step_verification = false;
        if verification.is_some() {
            ensure!(
                plan.output.max_bytes >= 8192,
                "enabled plan verification needs an output budget of at least 8192 bytes to reserve receipt metadata"
            );
            ensure!(
                !own_step_policy,
                "choose response verification or explicit verification_scope:steps for per-step choices"
            );
        }
        for step in &mut plan.steps {
            if verification_fields(&step.tool).is_empty() {
                continue;
            }
            if verification_scope == VerificationScope::Response {
                continue;
            }
            let own = verification_controls(&step.args)?;
            let effective = if verification_scope == VerificationScope::Steps {
                if own.verification == Some(VerificationMode::Off) {
                    own.resolve()?
                } else {
                    plan.verification.with_overrides(own).resolve()?
                }
            } else {
                None
            };
            step_verification |= effective.is_some();
            // Response verification inventories final output once. Inner reads
            // explicitly bypass their own default Report check.
            let controls = VerificationControls::resolved(effective);
            for key in verification_fields(&step.tool) {
                step.args.remove(*key);
            }
            step.args.extend(
                serde_json::to_value(controls)?
                    .as_object()
                    .expect("controls object")
                    .clone(),
            );
        }
        let b = &plan.budgets;
        ensure!(
            (1..=12).contains(&b.calls)
                && (1..=12).contains(&b.fanout)
                && (1..=30).contains(&b.seconds)
                && (1024..=524288).contains(&b.max_bytes),
            "plan budgets exceed limits"
        );
        ensure!(
            matches!(plan.output.view.as_str(), "raw" | "evidence")
                && (1024..=524288).contains(&plan.output.max_bytes),
            "invalid output selection"
        );
        ensure!(
            !step_verification || plan.output.view == "raw",
            "per-step verification requires output.view:raw to retain complete step receipts; use response verification for an evidence-only view"
        );
        ensure!(plan.steps.len() <= 6, "plan exceeds six steps");
        ensure!(
            !plan.steps.is_empty() || !needs.is_empty(),
            "explicit plan has no steps"
        );
        let mut ids: BTreeMap<String, String> = BTreeMap::new();
        for step in &mut plan.steps {
            if let Some(detail) = request.reply_detail
                && step.args.get("reply_detail").is_none_or(Value::is_null)
            {
                step.args.insert("reply_detail".into(), json!(detail));
            }
            ensure!(
                !step.id.is_empty()
                    && step.id.len() <= 32
                    && step
                        .id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "invalid step ID"
            );
            for binding in &step.bind {
                let source = ids
                    .get(&binding.from)
                    .context("binding must name an earlier step")?;
                ensure!(
                    matches!(
                        source.as_str(),
                        "symbols" | "code_facts" | "document_facts" | "search" | "references"
                    ),
                    "unsupported binding source"
                );
                ensure!(
                    (binding.select == "unique_definition_path"
                        && binding.arg == "path"
                        && source == "symbols"
                        && matches!(
                            step.tool.as_str(),
                            "outline" | "file_info" | "symbols" | "code_facts"
                        ))
                        || (binding.select == "hit_locations"
                            && binding.arg == "location"
                            && step.tool == "read_file")
                        || (binding.select == "selected_symbol_ids"
                            && binding.arg == "symbol"
                            && matches!(source.as_str(), "symbols" | "code_facts")
                            && step.tool == "relationships"),
                    "unsupported selector or destination"
                );
            }
            ensure!(step.bind.len() <= 1, "one typed location binding per step");
            ensure!(
                ids.insert(step.id.clone(), step.tool.clone()).is_none(),
                "duplicate step ID"
            );
            let scope_key = if step.tool == "list_files" {
                "prefix"
            } else {
                "path_prefix"
            };
            if fields(&step.tool).contains(&scope_key) {
                step.args
                    .entry(scope_key)
                    .or_insert(json!(plan.path_prefix));
            }
            validate_step(step, &plan.path_prefix, step.bind.is_empty())?;
        }
        // Syntax/provider IDs are selected under their ORIGINAL publication.
        // Preparing pasted/saved byte-span IDs must not bless them by attaching
        // today's token. Fresh binding outputs are absent from literal inputs.
        let literal_symbols = has_revision_bound_symbol_inputs(&plan);
        let unfenced_literal_symbols =
            literal_symbols && plan.publication.as_deref().is_none_or(str::is_empty);
        if literal_symbols && !unfenced_literal_symbols {
            plan.publication
                .as_deref()
                .expect("checked publication")
                .parse::<airk_core::identity::RevisionToken>()?;
        }
        if unfenced_literal_symbols {
            choices.push(RequiredInput {
                field: "publication".into(), step: None,
                reason: "A literal revision-bound symbol ID needs its original publication. Use Inspect from retained evidence or an explicitly publication-bound plan; preparing an old ID cannot refresh its identity.".into(),
                choices: vec![],
            });
        }
        let expected = request
            .expected_digest
            .or_else(|| (!plan.digest.is_empty()).then(|| plan.digest.clone()));
        // A prepared plan's identity is checked before refreshing its token.
        if let Some(expected) = expected {
            ensure!(
                expected == digest(&plan)?,
                "plan digest changed; prepare the edited plan again"
            );
        }
        let mut calls = 0;
        let mut resolution_bytes = 0;
        let mut stale = false;
        if !unfenced_literal_symbols && plan.steps.iter().any(|s| is_evidence(&s.tool)) {
            calls += 1;
            let current =
                publication(&self.status_summary()?).context("index publication unavailable")?;
            if let Some(old) = &plan.publication {
                stale = old != &current;
            }
            if !stale {
                plan.publication = Some(current);
            }
        }
        if plan.steps.iter().any(|step| {
            step.args
                .get("revision")
                .and_then(Value::as_str)
                .is_some_and(|token| Some(token) != plan.publication.as_deref())
        }) {
            stale = true;
        }
        if text_request && !stale && !unfenced_literal_symbols {
            for step in &mut plan.steps {
                let default_read = step.tool == "read_file"
                    && request
                        .text
                        .as_deref()
                        .and_then(|text| tokens(text).ok())
                        .and_then(|words| proposed_range(&words))
                        .is_some_and(|(_, _, defaulted)| defaulted);
                if (step.tool == "document_facts" || default_read) && step.args.contains_key("path")
                {
                    if calls + 1 >= plan.budgets.calls
                        || started.elapsed().as_secs() >= plan.budgets.seconds
                        || resolution_bytes + 8192 > plan.budgets.max_bytes
                    {
                        needs.push(
                            "File metadata resolution needs more call, time or byte allowance."
                                .into(),
                        );
                        continue;
                    }
                    calls += 1;
                    let info = serde_json::to_value(self.file_info_conditional(
                        string(&step.args, "path")?,
                        &serde_json::from_value(json!({"revision":plan.publication}))?,
                    )?)?;
                    ensure!(
                        publication(&info) == plan.publication,
                        "publication changed during profile resolution"
                    );
                    resolution_bytes += serde_json::to_vec(&info)?.len();
                    ensure!(
                        resolution_bytes <= plan.budgets.max_bytes,
                        "file metadata exceeds resolution byte allowance"
                    );
                    if default_read {
                        let content = info.get("content").unwrap_or(&info);
                        let lines = content
                            .get("indexed_lines")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        if lines == 0 {
                            needs.push("This file has no indexed source lines; select another file or inspect its metadata.".into());
                        } else {
                            step.args.insert("end_line".into(), json!(lines.min(200)));
                            if let Some(hash) = content.get("content_hash") {
                                step.args.insert("known_hash".into(), hash.clone());
                            }
                        }
                        continue;
                    }
                    let language = info
                        .get("content")
                        .unwrap_or(&info)
                        .get("language")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if matches!(
                        language,
                        "rust"
                            | "python"
                            | "javascript"
                            | "typescript"
                            | "tsx"
                            | "jsx"
                            | "go"
                            | "java"
                            | "c"
                            | "cpp"
                            | "csharp"
                            | "ruby"
                            | "php"
                            | "bash"
                            | "powershell"
                            | "lua"
                            | "swift"
                            | "kotlin"
                            | "sql"
                    ) {
                        step.tool = "code_facts".into();
                    }
                }
                if !matches!(step.tool.as_str(), "symbols" | "references") {
                    continue;
                }
                let Some(name) = step
                    .args
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                else {
                    continue;
                };
                if !name.contains(' ') {
                    continue;
                }
                let words: Vec<_> = name.trim_start_matches("the ").split_whitespace().collect();
                let pascal: String = words
                    .iter()
                    .map(|word| {
                        let mut chars = word.chars();
                        chars
                            .next()
                            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                            .unwrap_or_default()
                    })
                    .collect();
                let mut candidates = BTreeSet::from([pascal.clone(), words.join("_")]);
                if let Some(first) = pascal.chars().next() {
                    candidates.insert(
                        first.to_lowercase().collect::<String>() + &pascal[first.len_utf8()..],
                    );
                }
                let mut matches = BTreeSet::new();
                for candidate in candidates {
                    if calls + 1 >= plan.budgets.calls
                        || started.elapsed().as_secs() >= plan.budgets.seconds
                        || resolution_bytes + 8192 > plan.budgets.max_bytes
                    {
                        needs.push("Name resolution budget exhausted; narrow scope or provide an exact identifier.".into());
                        break;
                    }
                    calls += 1;
                    let value = serde_json::to_value(
                        self.facts_page(
                            &FactQuery {
                                path_prefix: step
                                    .args
                                    .get("path_prefix")
                                    .and_then(Value::as_str)
                                    .unwrap_or(&plan.path_prefix)
                                    .to_string(),
                                name: Some(candidate.clone()),
                                kind: Some("definition".into()),
                                language: None,
                                exact_name: true,
                                limit: 10,
                                max_bytes: 8192,
                                path_exact: false,
                                document_only: false,
                            },
                            &DiscoveryOptions::default(),
                        )?,
                    )?;
                    ensure!(
                        publication(&value) == plan.publication,
                        "publication changed during name resolution"
                    );
                    resolution_bytes += serde_json::to_vec(&value)?.len();
                    if !locations(&value)?.is_empty() {
                        matches.insert(candidate);
                    }
                }
                if matches.len() == 1 && needs.is_empty() {
                    step.args
                        .insert("name".into(), json!(matches.first().unwrap()));
                } else {
                    choices.push(RequiredInput {
                        field: "name".into(),
                        step: Some(step.id.clone()),
                        reason: format!("Select an exact identifier for {name}."),
                        choices: matches.iter().cloned().collect(),
                    });
                    needs.push(format!(
                        "Select an exact identifier for {name}; matched candidates: {}",
                        matches.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
        plan.digest = digest(&plan)?;
        let mut result = ResultEnvelope {source_verification:None,schema: RESULT_SCHEMA.into(), reply_detail:request.reply_detail, collection:self.collection().into(),publication:plan.publication.clone(), status:if needs.is_empty(){"prepared"}else{"needs_input"}.into(),executed:false, plan:plan.clone(),requires_input:needs.into_iter().map(|reason|RequiredInput{field:"text".into(),step:None,reason,choices:vec![]}).collect(),steps:vec![],evidence:vec![],warnings:vec!["Untrusted indexed evidence; verify live files before edits. Syntax relationships are candidates, not resolved calls.".into()],truncated:false,completeness:vec![],calls,payload_bytes:resolution_bytes};
        result.requires_input.extend(choices);
        if unfenced_literal_symbols {
            result.status = "needs_input".into();
        }
        if request.text.as_deref().is_some_and(|text| {
            !text.trim_start().starts_with('/')
                && tokens(text).is_ok_and(|words| {
                    words.iter().any(|word| {
                        let word = word.trim_matches(['?', '.', ',', ';', '!']);
                        word.eq_ignore_ascii_case("tests") || word.eq_ignore_ascii_case("test")
                    })
                })
        }) && plan
            .steps
            .iter()
            .any(|step| matches!(step.tool.as_str(), "references" | "search"))
        {
            result.warnings.push("Results are textual candidates across the selected path scope; test files were not separately classified or filtered. Set a literal test-folder path prefix to narrow them. No executed test coverage is established.".into());
        }
        if stale {
            result.status = "publication_changed".into();
            result.requires_input.push(RequiredInput {
                field: "publication".into(),
                step: None,
                reason: "Prepared publication expired; prepare against current evidence.".into(),
                choices: vec![],
            });
            return bounded_result(result);
        }
        if request.mode == Mode::Prepare || !result.requires_input.is_empty() {
            return bounded_result(result);
        }
        // Reject an impossible output allowance before collecting evidence.
        let mut framing = result.clone();
        framing.steps = plan
            .steps
            .iter()
            .map(|step| StepReport {
                id: step.id.clone(),
                tool: step.tool.clone(),
                arguments: json!(step.args),
                status: "complete".into(),
                elapsed_ms: 0,
                result: None,
                error: None,
            })
            .collect();
        ensure!(
            serde_json::to_vec(&framing)?.len() <= plan.output.max_bytes,
            "output budget cannot fit plan and reports"
        );
        let mut outputs: BTreeMap<String, Value> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut fanout = 0;
        for step in &plan.steps {
            let expanded = expand(step, &outputs, plan.budgets.fanout.saturating_sub(fanout));
            let (args_list, omitted) = match expanded {
                Ok(v) => v,
                Err(e) => {
                    if let Some(choices) = e.downcast_ref::<DefinitionChoice>() {
                        result.requires_input.push(RequiredInput {
                            field: "path".into(),
                            step: Some(step.id.clone()),
                            reason: e.to_string(),
                            choices: choices.0.clone(),
                        });
                    }
                    result.steps.push(StepReport {
                        id: step.id.clone(),
                        tool: step.tool.clone(),
                        arguments: json!(step.args),
                        status: "skipped".into(),
                        elapsed_ms: 0,
                        result: None,
                        error: Some(e.to_string()),
                    });
                    result.completeness.push(format!("{}: {}", step.id, e));
                    continue;
                }
            };
            if omitted {
                result.truncated = true;
                result
                    .completeness
                    .push(format!("{}: fanout omitted locations", step.id));
            }
            if !step.bind.is_empty() {
                fanout += args_list.len();
            }
            let mut collected = vec![];
            for mut args in args_list {
                if result.calls >= plan.budgets.calls
                    || started.elapsed().as_secs() >= plan.budgets.seconds
                    || result.payload_bytes + 1024 > plan.budgets.max_bytes
                {
                    result.truncated = true;
                    result
                        .completeness
                        .push(format!("{}: run budget exhausted", step.id));
                    break;
                }
                let remaining = plan.budgets.max_bytes - result.payload_bytes;
                if fields(&step.tool).contains(&"max_bytes") {
                    let cap = number(&args, "max_bytes", 8192)
                        .min(remaining.saturating_sub(512))
                        .min(65536);
                    args.insert("max_bytes".into(), json!(cap));
                }
                if verification_scope == VerificationScope::Response
                    && !verification_fields(&step.tool).is_empty()
                {
                    args.insert("verification".into(), json!("off"));
                }
                if !verification_fields(&step.tool).is_empty()
                    && let Some(budget) = verification_controls(&args)?.resolve()?
                {
                    let remaining_ms = (started
                        + std::time::Duration::from_secs(plan.budgets.seconds))
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64;
                    if remaining_ms == 0 {
                        result.truncated = true;
                        result.completeness.push(format!(
                            "{}: run deadline exhausted before verification",
                            step.id
                        ));
                        break;
                    }
                    args.insert(
                        "verification_deadline_ms".into(),
                        json!(budget.deadline_ms.min(remaining_ms)),
                    );
                }
                let bound_step = Step {
                    args: args.clone(),
                    ..step.clone()
                };
                if let Err(error) = validate_step(&bound_step, &plan.path_prefix, true) {
                    result.steps.push(StepReport {
                        id: step.id.clone(),
                        tool: step.tool.clone(),
                        arguments: json!(args),
                        status: "failed".into(),
                        elapsed_ms: 0,
                        result: None,
                        error: Some(error.to_string()),
                    });
                    result
                        .completeness
                        .push(format!("{}: derived arguments rejected", step.id));
                    continue;
                }
                result.calls += 1;
                result.executed = true;
                let at = Instant::now();
                let answer = match reader {
                    Some(read) => read(&step.tool, &args, plan.publication.as_deref()),
                    None => {
                        self.plan_read(&step.tool, &args, plan.publication.as_deref(), observations)
                    }
                };
                let mut report = StepReport {
                    id: step.id.clone(),
                    tool: step.tool.clone(),
                    arguments: json!(args),
                    status: "complete".into(),
                    elapsed_ms: at.elapsed().as_millis() as u64,
                    result: None,
                    error: None,
                };
                match answer {
                    Err(error) => {
                        report.status = "failed".into();
                        report.error = Some(error.to_string());
                        if let Some(receipt) = error.downcast_ref::<ReadFailure>() {
                            report.error =
                                Some("native read failed; complete receipt in result".into());
                            let bytes = serde_json::to_vec(&receipt.0)?.len();
                            if bytes <= remaining {
                                result.payload_bytes += bytes;
                                report.result = Some(receipt.0.clone());
                            } else {
                                result.truncated = true;
                                // Do not retain a second, unbounded serialized receipt in error.
                                report.error =
                                    Some("read failed; receipt exceeds payload budget".into());
                            }
                        }
                        result
                            .completeness
                            .push(format!("{}: read failed", step.id));
                    }
                    Ok(value) => {
                        let records = match locations(&value) {
                            Ok(records) => records,
                            Err(_) => {
                                report.status = "failed".into();
                                report.error = Some(
                                    "invalid compact evidence bindings; result not admitted".into(),
                                );
                                result
                                    .completeness
                                    .push(format!("{}: invalid evidence bindings", step.id));
                                result.steps.push(report);
                                continue;
                            }
                        };
                        if records.iter().any(|record| {
                            record["citation"]["path"]
                                .as_str()
                                .is_some_and(|path| !path.starts_with(&plan.path_prefix))
                        }) {
                            report.status = "failed".into();
                            report.error = Some("read result escapes plan scope".into());
                            result
                                .completeness
                                .push(format!("{}: out-of-scope evidence not admitted", step.id));
                            result.steps.push(report);
                            continue;
                        }
                        if is_evidence(&step.tool) && publication(&value) != plan.publication {
                            report.status = "failed".into();
                            report.error = Some("publication changed or missing".into());
                            result.status = "publication_changed".into();
                            result
                                .completeness
                                .push("publication mismatch; evidence not admitted".into());
                            result.steps.push(report);
                            break;
                        }
                        let bytes = serde_json::to_vec(&value)?.len();
                        if let Some(status) = value
                            .pointer("/read/status")
                            .and_then(Value::as_str)
                            .filter(|s| !matches!(*s, "content" | "not_modified"))
                        {
                            result
                                .completeness
                                .push(format!("{}: conditional read reported {status}", step.id));
                        }
                        if bytes > remaining {
                            result.truncated = true;
                            result
                                .completeness
                                .push(format!("{}: materialized payload exceeds budget", step.id));
                            report.status = "failed".into();
                            report.error = Some("payload budget exceeded".into());
                        } else {
                            result.payload_bytes += bytes;
                            if clipped(&value) {
                                result.truncated = true;
                                result.completeness.push(format!(
                                    "{}: upstream truncation or continuation",
                                    step.id
                                ));
                            }
                            for mut record in records {
                                if seen.insert(record["citation"].to_string()) {
                                    record["step"] = json!(step.id);
                                    record["publication"] = json!(plan.publication);
                                    result.evidence.push(record);
                                }
                            }
                            if step.tool == "references" {
                                result.warnings.push("Textual occurrences include definitions, comments and strings; these are not resolved callers.".into());
                            }
                            collected.push(value.clone());
                            report.result = Some(value);
                        }
                    }
                }
                result.steps.push(report);
            }
            if !collected.is_empty() {
                outputs.insert(step.id.clone(), json!(collected));
            }
            if result.status == "publication_changed" {
                break;
            }
        }
        for step in &plan.steps {
            if !result.steps.iter().any(|r| r.id == step.id) {
                result.steps.push(StepReport {
                    id: step.id.clone(),
                    tool: step.tool.clone(),
                    arguments: json!(step.args),
                    status: "skipped".into(),
                    elapsed_ms: 0,
                    result: None,
                    error: Some("Run stopped before this step was admitted.".into()),
                });
            }
        }
        if result.status != "publication_changed" {
            result.status = if result.completeness.is_empty() {
                "complete"
            } else if result.steps.iter().any(|s| s.status == "complete") {
                "partial"
            } else {
                "failed"
            }
            .into();
        }
        if plan.output.view == "evidence" {
            for report in &mut result.steps {
                report.result = None;
            }
        }
        if let Some(budget) = verification {
            let mut result = fit_verification_frame(result, 4096)?;
            let payload = serde_json::to_value(&result)?;
            let allocated = result
                .plan
                .output
                .max_bytes
                .saturating_sub(serde_json::to_vec(&payload)?.len() + 2048)
                .min(airk_core::verification::receipt_budget_for_reply(&payload)?);
            let receipt = self.verify_returned_sources_before(
                &payload,
                budget,
                Some(started + std::time::Duration::from_secs(result.plan.budgets.seconds)),
                allocated,
            )?;
            if receipt.status == "requirements_unmet" {
                result.status = "failed".into();
                result
                    .completeness
                    .push("verification_required_unmet".into());
            }
            airk_core::verification::dated_warnings(&mut result.warnings, &receipt);
            result.source_verification = Some(receipt);
            ensure!(
                serde_json::to_vec(&result)?.len() <= result.plan.output.max_bytes,
                "verified plan exceeds output budget"
            );
            Ok(result)
        } else if step_verification {
            fit_verification_frame(result, 0)
        } else {
            bounded_result(result)
        }
    }

    fn plan_read(
        &self,
        tool: &str,
        args: &Map<String, Value>,
        revision: Option<&str>,
        observations: &Value,
    ) -> Result<Value> {
        let limit = number(args, "limit", 10);
        let max = number(args, "max_bytes", 8192);
        let prefix = args
            .get("path_prefix")
            .and_then(Value::as_str)
            .unwrap_or("");
        let reply = reply_detail(args)?;
        let minimal = matches!(tool, "symbols" | "imports" | "outline")
            && reply == Some(airk_core::reply::ReplyDetail::Lean);
        let options = DiscoveryOptions {
            cursor: args
                .get("cursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
            // Omission retains the original Workbench rich representation;
            // compact is an explicit choice, independent of MCP defaults.
            compact: if minimal {
                false
            } else if matches!(
                tool,
                "symbols" | "imports" | "outline" | "code_facts" | "document_facts"
            ) {
                airk_core::reply::ReplyDetail::compact(
                    reply,
                    args.get("compact").and_then(Value::as_bool),
                )?
                .unwrap_or(false)
            } else {
                false
            },
            minimal,
        };
        let evidence: EvidenceReadOptions = serde_json::from_value(
            json!({"revision":revision,"known_hash":args.get("known_hash")}),
        )?;
        let mut value = match tool {
            "status" => self.status_summary()?,
            "diagnostics" => {
                let mut value = serde_json::to_value(self.status()?)?;
                value["runtime"] = observations
                    .get("runtime")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or_else(|| json!({"error":"native_runtime_context_unavailable"}));
                value["work"] = observations
                    .get("work")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or_else(|| json!({"error":"native_work_context_unavailable"}));
                value
            }
            "help" => {
                json!({"commands":"/search /symbols /code-facts /refs /read /outline /files /info /imports /facts /changes --since /status /diagnostics /capabilities /context /relationships /help","recipes":["definition","documents","impact","tests","component","readiness"],"parser":"deterministic rules; unsupported phrasing returns needs_input","navigation_options":"search/refs: --language, --evidence-kind, --exclude-path-prefix; symbols/imports/code-facts: --name-match, --include-count; refs: --cursor, --order, --include-files, --include-count; fact reads/outline: --compact. Boolean flags accept optional true/false. Workbench compact omission keeps rich facts; MCP compact discovery has its own default."})
            }
            "capabilities" => {
                json!({"adapters":Kernel::extraction_adapters().iter().map(|a|json!({"id":a.profile.adapter_id})).collect::<Vec<_>>(),"workbench":operation_catalog(),"read_guidance":airk_core::guidance::catalog(),"source_verification_capability":airk_core::verification::catalog()})
            }
            "search" => serde_json::to_value(
                self.search_response_with_navigation(
                    &SearchQuery {
                        text: string(args, "query")?.into(),
                        path_prefix: Some(prefix.into()),
                        limit,
                        max_bytes: max,
                    },
                    match args.get("mode").and_then(Value::as_str) {
                        Some("lexical") => crate::current::CurrentSearchMode::Lexical,
                        Some("vector") => crate::current::CurrentSearchMode::Vector,
                        Some("hybrid") => crate::current::CurrentSearchMode::Hybrid,
                        _ => crate::current::CurrentSearchMode::Default,
                    },
                    (!args
                        .get("full_chunks")
                        .and_then(Value::as_bool)
                        .unwrap_or(false))
                    .then_some(768),
                    &search_filters(args)?,
                )?,
            )?,
            "list_files" => serde_json::to_value(self.list_files_page(
                args.get("prefix").and_then(Value::as_str).unwrap_or(""),
                limit,
                max,
                &options,
            )?)?,
            "references" => {
                let filters = search_filters(args)?;
                let include_files = args
                    .get("include_files")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let include_count = args
                    .get("include_count")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if args.get("cursor").is_none()
                    && args.get("order").is_none()
                    && !include_files
                    && !include_count
                    && filters.is_empty()
                {
                    serde_json::to_value(self.references(
                        string(args, "name")?,
                        prefix,
                        limit,
                        max,
                    )?)?
                } else {
                    serde_json::to_value(
                        self.references_page(
                            string(args, "name")?,
                            prefix,
                            limit,
                            max,
                            &ReferenceOptions {
                                cursor: options.cursor.clone(),
                                order: args
                                    .get("order")
                                    .cloned()
                                    .map(serde_json::from_value)
                                    .transpose()?
                                    .unwrap_or_default(),
                                include_files,
                                include_count,
                                filters,
                            },
                        )?,
                    )?
                }
            }
            "read_file" => serde_json::to_value(self.read_file_conditional(
                string(args, "path")?,
                number(args, "start_line", 1),
                number(args, "end_line", 200),
                max,
                &evidence,
            )?)?,
            "file_info" => {
                serde_json::to_value(self.file_info_conditional(string(args, "path")?, &evidence)?)?
            }
            "outline" => serde_json::to_value(self.outline_conditional_page(
                string(args, "path")?,
                limit,
                max,
                &evidence,
                &options,
            )?)?,
            "context" => serde_json::to_value(self.context_conditional(
                string(args, "path")?,
                number(args, "line", 1),
                number(args, "before", 3).min(40),
                number(args, "after", 3).min(40),
                max,
                &evidence,
            )?)?,
            "symbols" | "imports" | "code_facts" | "document_facts" => {
                let query = FactQuery {
                    path_prefix: args
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or(prefix)
                        .into(),
                    name: args.get("name").and_then(Value::as_str).map(str::to_owned),
                    kind: match tool {
                        "symbols" => Some("definition".into()),
                        "imports" => Some("import".into()),
                        _ => args.get("kind").and_then(Value::as_str).map(str::to_owned),
                    },
                    language: args
                        .get("language")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    exact_name: matches!(tool, "symbols" | "code_facts"),
                    limit,
                    max_bytes: max,
                    path_exact: args.contains_key("path"),
                    document_only: tool == "document_facts",
                };
                let matching = FactDiscoveryOptions {
                    name_match: args
                        .get("name_match")
                        .cloned()
                        .map(serde_json::from_value)
                        .transpose()?,
                    include_count: args
                        .get("include_count")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                };
                serde_json::to_value(if tool == "code_facts" {
                    self.code_facts_page_with_options(&query, &options, &matching)?
                } else {
                    self.facts_page_with_options(&query, &options, &matching)?
                })?
            }
            "changes" => {
                serde_json::to_value(self.changes(&serde_json::from_value(json!(args))?)?)?
            }
            "relationships" => serde_json::to_value(
                self.relationships(&airk_core::RelationshipQuery {
                    symbol: string(args, "symbol")?.into(),
                    direction: args
                        .get("direction")
                        .and_then(Value::as_str)
                        .unwrap_or("incoming")
                        .into(),
                    kind: args.get("kind").and_then(Value::as_str).map(str::to_owned),
                    path_prefix: prefix.into(),
                    hops: number(args, "hops", 1),
                    limit,
                    max_bytes: max,
                })?,
            )?,
            _ => bail!("unknown evidence operation"),
        };
        airk_core::reply::ReplyDetail::annotate(&mut value, reply);
        airk_core::guidance::annotate(&mut value);
        if !verification_fields(tool).is_empty()
            && let Some(budget) = verification_controls(args)?.resolve()?
        {
            let receipt = self.verify_returned_sources(&value, budget).map_err(|error| {
                if error.downcast_ref::<airk_core::verification::VerificationReplyBudget>().is_some() {
                    let body=airk_core::verification::reply_budget_refusal();
                    ReadFailure(json!({"isError":true,"content":[{"type":"text","text":body.to_string()}]})).into()
                } else {error}
            })?;
            let failed = airk_core::verification::attach_response(&mut value, receipt)?;
            if failed {
                return Err(ReadFailure(json!({"isError":true,"content":[{"type":"text","text":serde_json::to_string(&value)?}]})).into());
            }
        }
        Ok(value)
    }
}

fn expand(
    step: &Step,
    outputs: &BTreeMap<String, Value>,
    cap: usize,
) -> Result<(Vec<Map<String, Value>>, bool)> {
    let Some(binding) = step.bind.first() else {
        return Ok((vec![step.args.clone()], false));
    };
    let records = locations(
        outputs
            .get(&binding.from)
            .context("dependency unavailable")?,
    )?;
    ensure!(!records.is_empty(), "dependency has no cited locations");
    if binding.select == "unique_definition_path" {
        ensure!(
            !clipped(&outputs[&binding.from]),
            "definition enumeration is incomplete; narrow the name or scope"
        );
        let paths: BTreeSet<_> = records
            .iter()
            .filter_map(|r| r["citation"]["path"].as_str())
            .collect();
        if paths.len() != 1 {
            return Err(DefinitionChoice(paths.into_iter().map(str::to_owned).collect()).into());
        }
        let mut args = step.args.clone();
        args.insert("path".into(), json!(paths.first().unwrap()));
        return Ok((vec![args], false));
    }
    let omitted = records.len() > cap;
    if binding.select == "selected_symbol_ids" {
        let mut seen = BTreeSet::new();
        let symbols: Vec<_> = records
            .iter()
            .filter_map(|r| {
                r.pointer("/fact/code/symbol_id")
                    .or_else(|| r.pointer("/fact/symbol_id"))
                    .and_then(Value::as_str)
            })
            .filter(|id| seen.insert(id.to_string()))
            .collect();
        ensure!(
            !symbols.is_empty(),
            "dependency has no native symbol IDs; use an explicit compact:false symbols/code_facts lookup at the cited path before selected_symbol_ids"
        );
        return Ok((
            symbols
                .iter()
                .take(cap)
                .map(|id| {
                    let mut args = step.args.clone();
                    args.insert("symbol".into(), json!(id));
                    args
                })
                .collect(),
            symbols.len() > cap,
        ));
    }
    let args = records
        .into_iter()
        .take(cap)
        .map(|r| {
            let c = &r["citation"];
            let mut args = step.args.clone();
            for key in ["path", "start_line", "end_line"] {
                args.insert(key.into(), c[key].clone());
            }
            args.insert("known_hash".into(), c["content_hash"].clone());
            args
        })
        .collect();
    Ok((args, omitted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_catalog_and_slash_options_use_the_same_typed_validation() -> Result<()> {
        let catalog = operation_catalog();
        assert_eq!(
            catalog["operations"]["symbols"]["properties"]["include_count"]["type"],
            "boolean"
        );
        for tool in [
            "symbols",
            "imports",
            "code_facts",
            "document_facts",
            "outline",
        ] {
            assert_eq!(
                catalog["operations"][tool]["properties"]["compact"]["type"],
                json!(["boolean", "null"])
            );
        }
        assert_eq!(
            catalog["operations"]["references"]["properties"]["include_files"]["type"],
            "boolean"
        );
        assert_eq!(
            catalog["operations"]["references"]["properties"]["order"]["enum"],
            json!(["index", "path", "code_first"])
        );
        assert_eq!(
            catalog["operations"]["search"]["properties"]["evidence_kind"]["enum"],
            json!(["code", "document", "configuration", "unknown"])
        );
        assert_eq!(
            catalog["operations"]["symbols"]["properties"]["name_match"]["enum"],
            json!(["exact", "suffix", "contains"])
        );
        assert!(
            catalog["operations"]["document_facts"]["properties"]
                .get("name_match")
                .is_none()
        );
        for text in [
            "/search Session --mode lexical --language rust --evidence-kind code --exclude-path-prefix docs/",
            "/symbols Session --name-match suffix --include-count --compact",
            "/code-facts Session --name-match contains --include-count true --kind definition",
            "/imports Session --name-match exact --include-count false",
            "/refs Session --language rust --evidence-kind code --exclude-path-prefix docs/ --order path --include-files --include-count",
            "/outline src/session.rs --compact false",
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            validate_step(&steps[0], "", true)?;
        }
        let (steps, _) = parse_text("/symbols Session --include-count false")?;
        assert_eq!(steps[0].args["include_count"], false);
        let (steps, _) = parse_text("/symbols --include-count Session")?;
        assert_eq!(steps[0].args["include_count"], true);
        assert_eq!(steps[0].args["name"], "Session");
        for (tool, args) in [
            ("symbols", json!({"name":"Session","include_count":"true"})),
            ("symbols", json!({"name":"Session","compact":"true"})),
            ("search", json!({"query":"Session","compact":true})),
            ("symbols", json!({"name":"Session","name_match":"regex"})),
            ("symbols", json!({"name_match":"suffix"})),
            ("references", json!({"name":"Session","order":"score"})),
            ("references", json!({"name":"Session","include_files":1})),
            ("search", json!({"query":"Session","language":""})),
            (
                "search",
                json!({"query":"Session","evidence_kind":"source"}),
            ),
            (
                "search",
                json!({"query":"Session","exclude_path_prefix":"../outside"}),
            ),
            (
                "document_facts",
                json!({"name":"Session","name_match":"suffix"}),
            ),
        ] {
            assert!(
                validate_step(&step("query", tool, args), "", true).is_err(),
                "{tool}"
            );
        }
        let within = step(
            "query",
            "search",
            json!({
                "query":"Session","path_prefix":"src/","exclude_path_prefix":"docs/"
            }),
        );
        validate_step(&within, "src/", true)?;
        let outside = step(
            "query",
            "search",
            json!({"query":"Session","path_prefix":""}),
        );
        assert!(validate_step(&outside, "src/", true).is_err());
        assert!(parse_text("/search Session --language --include-count").is_err());
        Ok(())
    }

    #[test]
    fn workbench_compact_discovery_keeps_ids_for_bindings_and_omitted_mode_rich() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("session.rs"), "pub struct Session;\n")?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let rich = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","text":"/symbols Session"
        }))?)?;
        assert!(
            rich.steps[0].result.as_ref().context("rich result")?["facts"]
                .as_array()
                .context("rich facts")?
                .iter()
                .any(|fact| fact["code"]["symbol_id"].is_string())
        );
        let prepared = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"prepare","plan":{"steps":[
                {"id":"find","tool":"symbols","args":{"name":"Session","compact":true}},
                {"id":"links","tool":"relationships","args":{"direction":"incoming"},
                    "bind":[{"arg":"symbol","from":"find","select":"selected_symbol_ids"}]}
            ]}
        }))?)?;
        assert!(!prepared.executed);
        assert_eq!(prepared.plan.steps[0].args["compact"], true);
        let run = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan
        }))?)?;
        assert!(run.executed);
        for report in &run.steps {
            assert_eq!(report.status, "complete", "{report:?}");
        }
        let compact = run.steps[0].result.as_ref().context("compact result")?;
        assert!(
            compact["facts"]
                .as_array()
                .context("rich facts absent")?
                .is_empty()
        );
        let symbol = compact["outline"][0]["symbol_id"]
            .as_str()
            .context("compact symbol ID")?;
        assert_eq!(run.steps[1].arguments["symbol"], symbol);
        assert_eq!(rich.publication, run.publication);
        assert!(run.payload_bytes <= run.plan.budgets.max_bytes);
        Ok(())
    }

    #[test]
    fn explicit_reply_detail_keeps_native_locations_and_legacy_workbench_default() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("session.rs"), "pub struct Session;\n")?;
        let passage = format!("{} ScopeMarker\n", "background ".repeat(700));
        std::fs::write(root.join("passage.txt"), &passage)?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let generation = kernel.status()?.generation;
        let mut citations = Vec::new();
        for choice in [None, Some("lean"), Some("rich")] {
            let text = format!(
                "/symbols Session{}",
                choice.map_or(String::new(), |v| format!(" --reply-detail {v}"))
            );
            let run = kernel.evidence_plan(serde_json::from_value(
                json!({"schema":REQUEST_SCHEMA,"mode":"execute","text":text}),
            )?)?;
            assert_eq!(run.status, "complete");
            assert_eq!(run.evidence.len(), 1);
            assert_eq!(run.calls, 2);
            assert_eq!(
                run.steps[0]
                    .result
                    .as_ref()
                    .unwrap()
                    .get("reply_detail")
                    .and_then(Value::as_str),
                choice
            );
            citations.push(run.evidence[0]["citation"].clone());
            if choice == Some("lean") {
                assert!(
                    !run.steps[0].result.as_ref().unwrap()["minimal"]["items"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
            } else {
                assert!(
                    !run.steps[0].result.as_ref().unwrap()["facts"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
            }
        }
        assert!(citations.windows(2).all(|v| v[0] == v[1]));
        assert!(kernel.evidence_plan(serde_json::from_value(json!({"schema":REQUEST_SCHEMA,"mode":"execute","text":"/symbols Session --reply-detail lean --compact false"}))?).is_err());
        for detail in ["lean", "rich"] {
            for full in [false, true] {
                let run=kernel.evidence_plan(serde_json::from_value(json!({"schema":REQUEST_SCHEMA,"mode":"execute","reply_detail":detail,"plan":{"steps":[{"id":"search","tool":"search","args":{"query":"ScopeMarker","mode":"lexical","full_chunks":full,"max_bytes":8192}}]}}))?)?;
                assert_eq!(
                    run.status,
                    if full { "complete" } else { "partial" },
                    "{run:?}"
                );
                assert_eq!(run.steps[0].status, "complete");
                let text = run.steps[0].result.as_ref().unwrap()["hits"][0]["text"]
                    .as_str()
                    .unwrap();
                if full {
                    assert_eq!(text, passage);
                } else {
                    assert!(text.len() <= 768);
                }
            }
        }
        assert_eq!(kernel.status()?.generation, generation);
        Ok(())
    }

    #[test]
    fn preparing_literal_symbol_ids_never_assigns_a_new_publication_or_rebinds_old_spans()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("session.rs"), "pub struct First;\n")?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let query = FactQuery {
            path_prefix: "session.rs".into(),
            path_exact: true,
            name: Some("First".into()),
            kind: Some("definition".into()),
            language: None,
            exact_name: true,
            limit: 100,
            max_bytes: 65536,
            document_only: false,
        };
        let first = kernel.code_facts(&query)?;
        let id = first.facts[0]
            .code
            .as_ref()
            .context("original symbol")?
            .symbol_id
            .clone();
        let publication = first
            .publication
            .context("original publication")?
            .token
            .to_string();
        let literal = json!({"steps":[{"id":"inspect","tool":"symbols","args":{"name":id,"path":"session.rs"}}]});
        for mode in ["prepare", "execute"] {
            let result = kernel.evidence_plan(serde_json::from_value(json!({
                "schema":REQUEST_SCHEMA,"mode":mode,"plan":literal,
            }))?)?;
            assert!(!result.executed);
            assert_eq!(result.status, "needs_input");
            assert_eq!(result.calls, 0);
            assert!(result.publication.is_none() && result.plan.publication.is_none());
            assert_eq!(result.plan.steps[0].args["name"], id);
            let replay = kernel.evidence_plan(serde_json::from_value(json!({
                "schema":REQUEST_SCHEMA,"mode":"execute","plan":result.plan,
            }))?)?;
            assert!(!replay.executed && replay.plan.publication.is_none());
            assert_eq!(replay.calls, 0);
        }
        for mode in ["prepare", "execute"] {
            let result = kernel.evidence_plan(serde_json::from_value(json!({
                "schema":REQUEST_SCHEMA,"mode":mode,"text":format!("/symbols {id}"),
            }))?)?;
            assert_eq!(result.status, "needs_input");
            assert!(!result.executed && result.plan.publication.is_none());
            assert_eq!(
                result.calls, 0,
                "parsing an old ID cannot query or bless the current index"
            );
        }
        let pinned = json!({"publication":publication,"steps":[{"id":"inspect","tool":"symbols","args":{"name":id,"path":"session.rs"}}]});
        let current = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":pinned,
        }))?)?;
        assert!(current.executed);
        assert_eq!(
            current.steps[0].result.as_ref().context("pinned result")?["facts"][0]["name"],
            "First"
        );
        // Equal-length replacement deliberately reuses the literal byte-span ID.
        // Only the original publication keeps it from silently naming Other.
        std::fs::write(root.join("session.rs"), "pub struct Other;\n")?;
        kernel.index()?;
        let mut changed_query = query;
        changed_query.name = Some("Other".into());
        assert_eq!(
            kernel.code_facts(&changed_query)?.facts[0]
                .code
                .as_ref()
                .unwrap()
                .symbol_id,
            id
        );
        let stale = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":pinned,
        }))?)?;
        assert_eq!(stale.status, "publication_changed");
        assert!(!stale.executed);
        assert_eq!(
            stale.plan.publication.as_deref(),
            Some(publication.as_str())
        );
        let ordinary = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","text":"/symbols Other",
        }))?)?;
        assert!(ordinary.executed);
        Ok(())
    }

    #[test]
    fn workbench_executes_navigation_options_and_preserves_metadata_in_exports() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("docs"))?;
        std::fs::write(
            root.join("src/session.rs"),
            "pub struct Session;\nimpl Session { pub fn new() -> Self { Self } }\n\
             pub struct OwnerSession;\npub fn open() { let _ = Session::new(); }\n",
        )?;
        std::fs::write(
            root.join("docs/guide.md"),
            "# Session\nSession is written here too.\n",
        )?;
        // A pointer's file must remain exact, even when other indexed paths
        // share its entire prefix and declare the same written name.
        std::fs::write(root.join("src/session.rs.other"), "pub struct Session;\n")?;
        std::fs::write(
            root.join("src/session.rs.other.rs"),
            "pub struct Session;\n",
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let declarations = kernel.code_facts(&FactQuery {
            path_prefix: "src/session.rs".into(),
            path_exact: true,
            name: Some("Session".into()),
            kind: Some("definition".into()),
            language: None,
            exact_name: true,
            limit: 100,
            max_bytes: 65536,
            document_only: false,
        })?;
        let type_id = declarations
            .facts
            .iter()
            .find(|fact| fact.syntax_kind == "struct_item")
            .and_then(|fact| fact.code.as_ref())
            .context("Session struct declaration")?
            .symbol_id
            .clone();
        // The literal relationship ID was discovered under this publication;
        // preparing the composite plan must retain that original fence.
        let discovered_publication = declarations
            .publication
            .as_ref()
            .context("declaration publication")?
            .token
            .to_string();
        let symbols = json!({"name":"Session","name_match":"suffix","include_count":true});
        let refs = json!({"name":"Session","language":"rust","evidence_kind":"code", "exclude_path_prefix":"docs/", "order":"path","include_files":true,"include_count":true,"limit":1});
        let prepared = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"prepare","plan":{"path_prefix":"src/","publication":discovered_publication,"steps":[
                {"id":"find","tool":"search","args":{"query":"Session","mode":"lexical","language":"rust","evidence_kind":"code","exclude_path_prefix":"docs/"}},
                {"id":"types","tool":"symbols","args":symbols},
                {"id":"facts","tool":"code_facts","args":{"name":"OwnerSession","name_match":"exact","include_count":true}},
                {"id":"uses","tool":"references","args":refs},
                {"id":"methods","tool":"relationships","args":{"symbol":type_id,"direction":"incoming"}}
            ]}
        }))?)?;
        assert!(!prepared.executed);
        assert_eq!(
            prepared.plan.publication.as_deref(),
            Some(discovered_publication.as_str())
        );
        assert_eq!(prepared.plan.steps[1].args["name_match"], "suffix");
        assert_eq!(prepared.plan.steps[3].args["include_files"], true);
        let executed = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan
        }))?)?;
        assert!(executed.executed);
        for report in &executed.steps {
            assert_eq!(report.status, "complete", "{report:?}");
        }
        assert!(executed.calls <= executed.plan.budgets.calls);
        assert!(executed.payload_bytes <= executed.plan.budgets.max_bytes);
        let find = executed.steps[0].result.as_ref().context("search result")?;
        assert!(
            find["hits"]
                .as_array()
                .context("search hits")?
                .iter()
                .all(|hit| hit["citation"]["path"]
                    .as_str()
                    .is_some_and(|path| path.starts_with("src/")))
        );
        assert!(
            !find["definitions"]
                .as_array()
                .context("automatic definition hints")?
                .is_empty()
        );
        let pointer = find["definitions"][0]["matches"]
            .as_array()
            .context("retained definition pointers")?
            .iter()
            .find(|pointer| pointer["citation"]["path"] == "src/session.rs")
            .context("exact session pointer")?;
        let pointer_path = pointer["citation"]["path"]
            .as_str()
            .context("pointer path")?;
        let pointer_id = pointer["symbol_id"].as_str().context("pointer symbol ID")?;
        // The UI's Inspect action deliberately sends a symbol ID as `name`
        // together with the exact `path`, original scope and publication.
        let inspect = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"prepare","plan":{
                "schema":PLAN_SCHEMA,"publication":executed.publication,
                "collection":executed.plan.collection,"path_prefix":executed.plan.path_prefix,
                "steps":[{"id":"followup","tool":"symbols","args":{"name":pointer_id,"path":pointer_path}}]
            }
        }))?)?;
        assert!(!inspect.executed);
        assert_eq!(inspect.plan.steps[0].args["path"], pointer_path);
        assert_eq!(inspect.plan.publication, executed.publication);
        let inspected = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":inspect.plan
        }))?)?;
        assert_eq!(
            inspected.steps[0].status, "complete",
            "{:?}",
            inspected.steps[0]
        );
        assert_eq!(inspected.publication, executed.publication);
        let inspected_facts = inspected.steps[0]
            .result
            .as_ref()
            .context("inspection result")?["facts"]
            .as_array()
            .context("inspected facts")?;
        assert_eq!(inspected_facts.len(), 1);
        assert!(
            inspected_facts
                .iter()
                .all(|fact| fact["citation"]["path"] == pointer_path
                    && fact["code"]["symbol_id"] == pointer_id)
        );
        let types = executed.steps[1].result.as_ref().context("symbol result")?;
        assert!(
            types["facts"]
                .as_array()
                .context("facts")?
                .iter()
                .any(|fact| fact["name"] == "OwnerSession")
        );
        assert_eq!(types["match_count"]["scope"], "query");
        let facts = executed.steps[2]
            .result
            .as_ref()
            .context("code fact result")?;
        assert_eq!(facts["facts"][0]["name"], "OwnerSession");
        let uses = executed.steps[3]
            .result
            .as_ref()
            .context("reference result")?;
        assert_eq!(uses["page"]["count_unit"], "matching_lines");
        assert!(
            !uses["page"]["files"]
                .as_array()
                .context("file summaries")?
                .is_empty()
        );
        let cursor = uses["page"]["cursor"]
            .as_str()
            .context("reference continuation")?;
        assert_eq!(uses["page"]["match_count"]["total_matches"], Value::Null);
        let methods = executed.steps[4]
            .result
            .as_ref()
            .context("relationship result")?;
        assert!(
            methods["relationships"]
                .as_array()
                .context("incoming relationships")?
                .is_empty()
        );
        assert!(
            methods["suggested_targets"]
                .as_array()
                .context("declared target suggestions")?
                .iter()
                .any(|suggestion| suggestion["target"] == "Session::new"
                    && suggestion["resolution"] == "syntax_candidate")
        );
        let mut continued_args = refs.as_object().unwrap().clone();
        continued_args.insert("cursor".into(), json!(cursor));
        let continued = kernel.evidence_plan(serde_json::from_value(json!({
            "schema":REQUEST_SCHEMA,"mode":"execute","plan":{"path_prefix":"src/","steps":[
                {"id":"uses","tool":"references","args":continued_args}
            ]}
        }))?)?;
        let next = continued.steps[0]
            .result
            .as_ref()
            .context("continued result")?;
        assert_ne!(
            uses["evidence"]["hits"][0]["citation"],
            next["evidence"]["hits"][0]["citation"]
        );
        let export = serde_json::to_value(&executed)?;
        assert_eq!(
            export["steps"][0]["result"]["definitions"],
            find["definitions"]
        );
        assert_eq!(export["steps"][3]["result"]["page"], uses["page"]);
        assert_eq!(
            export["steps"][4]["result"]["suggested_targets"],
            methods["suggested_targets"]
        );
        Ok(())
    }

    fn role_inventory_plan_report(source: &str) -> Result<Value> {
        let inventory: Value = serde_json::from_str(source)?;
        let model: Value =
            serde_json::from_str(include_str!("../../../tools/parser-train/manifest.json"))?;
        let token = format!("airk-r1:{}:1:{}", "a".repeat(32), "b".repeat(64));
        let mut rows = vec![];
        let mut roles: BTreeMap<String, [usize; 4]> = BTreeMap::new();
        for case in inventory["queries"].as_array().unwrap() {
            let intent = case["intent"].as_str().unwrap();
            let replace = |s: &str| {
                s.replace(
                    "{s}",
                    if intent == "changes" {
                        &token
                    } else {
                        "QueueClient"
                    },
                )
                .replace("{p}", "src/client.rs")
                .replace("{scope}", "src/")
                .replace("{token}", &token)
                .replace("{baseline}", &token)
            };
            let text = replace(case["text"].as_str().unwrap());
            let expected = case["expected_tools"].as_array().unwrap();
            let target = case["target"].as_str().map(replace);
            let (steps, needs, error) = match parse_text(&text) {
                Ok((steps, needs)) => (steps, needs, None),
                Err(e) => (vec![], vec![], Some(e.to_string())),
            };
            let primary_tool = expected.first().and_then(Value::as_str);
            let matching = steps
                .iter()
                .find(|step| Some(step.tool.as_str()) == primary_tool);
            let target_matches = target.as_ref().is_none_or(|target| {
                matching.is_some_and(|step| step.args.values().any(|v| v.as_str() == Some(target)))
            });
            let correct = if intent == "unsupported" {
                !needs.is_empty() || error.is_some()
            } else {
                matching.is_some() && target_matches
            };
            let counters = roles
                .entry(case["role"].as_str().unwrap().into())
                .or_default();
            counters[0] += 1;
            counters[1] += usize::from(correct);
            counters[2] += usize::from(!correct && intent != "unsupported");
            counters[3] += usize::from(!correct && intent == "unsupported");
            rows.push(json!({"role":case["role"],"family":case["family"],
                "split":case["split"],"text":text,"intent":intent,"expected_primary_tool":primary_tool,
                "expected_target":target,"correct_operation_and_target":correct,
                "automatic":!steps.is_empty() && needs.is_empty() && error.is_none(),
                "steps":steps,"requires_input":needs,"error":error}));
        }
        let contracts: Vec<_> = rows.iter().map(|row| json!({
            "role":row["role"], "text":row["text"], "intent":row["intent"],
            "expected_primary_tool":row["expected_primary_tool"], "expected_target":row["expected_target"]
        })).collect();
        let report = json!({"schema":"airk.parser.role-plan-measurement/1",
            "scope":"Known public role inventory with controlled literal targets; not blind evaluation or native retrieval qualification.",
            "model":model["model_version"],"asset_sha256":model["asset_sha256"],
            "inventory_sha256":format!("{:x}",Sha256::digest(source.as_bytes())),
            "resolved_contract_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&contracts)?)),
            "parser_source_sha256":format!("{:x}",Sha256::digest(include_bytes!("workbench.rs"))),
            "natural_source_sha256":format!("{:x}",Sha256::digest(include_bytes!("workbench/natural.rs"))),
            "counter_order":["rows","correct_operation_and_target","supported_failures","wrong_automatic_unsupported"],
            "roles":roles,"queries":rows});
        Ok(report)
    }

    #[test]
    #[ignore = "explicit role-inventory measurement; set AIRK_PARSE_ROLE_REPORT to retain results"]
    fn role_inventory_plan_measurement() -> Result<()> {
        let source = match std::env::var("AIRK_PARSE_ROLE_FIXTURE") {
            Ok(path) => std::fs::read_to_string(path)?,
            Err(_) => include_str!("../../../tools/parser-train/role-queries.json").to_string(),
        };
        let report = role_inventory_plan_report(&source)?;
        if let Ok(path) = std::env::var("AIRK_PARSE_ROLE_REPORT") {
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
        eprintln!("{}", report["roles"]);
        assert!(
            report["roles"]
                .as_object()
                .unwrap()
                .values()
                .all(|c| c[3] == 0),
            "unsupported role query executed without review"
        );
        Ok(())
    }

    #[test]
    fn public_role_requests_keep_operations_targets_and_refusals() -> Result<()> {
        for source in [
            include_str!("../../../tools/parser-train/role-queries.json"),
            include_str!("../../../tools/parser-train/novel-role-queries.json"),
        ] {
            let report = role_inventory_plan_report(source)?;
            let failures: Vec<_> = report["queries"].as_array().unwrap().iter()
                .filter(|row| row["correct_operation_and_target"] != true)
                .map(|row| json!({"text":row["text"],"steps":row["steps"],"requires_input":row["requires_input"]}))
                .collect();
            assert!(
                failures.is_empty(),
                "{}",
                serde_json::to_string_pretty(&failures)?
            );
        }
        Ok(())
    }

    #[test]
    fn conversational_requests_keep_exact_symbols_and_topic_queries() -> Result<()> {
        for (subject, expected, explicit) in [
            ("struct Owner", "Owner", true),
            ("Owner struct", "Owner", true),
            ("class Client", "Client", true),
            ("QueueClient", "QueueClient", false),
            ("function request_budget", "request_budget", true),
            ("function named retry", "retry", true),
            ("trait called Storage", "Storage", true),
            ("`owner`", "owner", true),
            ("\"Owner\"", "Owner", true),
            ("'Owner'", "Owner", true),
            ("`struct Owner`", "Owner", true),
            ("client::Owner", "client::Owner", false),
            ("package.client", "package.client", false),
            ("enum État", "État", true),
            ("class package.Client", "package.Client", true),
            ("method Owner.capture", "Owner.capture", true),
            ("interface CacheReader", "CacheReader", true),
            ("type Result", "Result", true),
        ] {
            for prefix in [
                "I want to know about ",
                "Tell me about ",
                "Find the ",
                "What is ",
                "Please tell me about ",
                "Could you tell me about ",
                "Could you please tell me about ",
                "Can you explain ",
                "Show me ",
            ] {
                let text = format!("{prefix}{subject}.");
                let (steps, needs) = parse_text(&text)?;
                assert!(needs.is_empty(), "{text}: {needs:?}");
                assert_eq!(steps.len(), if explicit { 3 } else { 2 }, "{text}");
                assert_eq!(steps[0].tool, "symbols", "{text}");
                assert_eq!(steps[0].args["name"], expected, "{text}");
                if explicit {
                    assert_eq!(steps[1].bind[0].select, "unique_definition_path");
                    assert_eq!(steps[2].bind[0].select, "hit_locations");
                } else {
                    assert_eq!(steps[1].tool, "search");
                    assert_eq!(steps[1].args["query"], expected);
                }
            }
        }
        let (steps, needs) = parse_text("Find the definition of request")?;
        assert!(needs.is_empty());
        assert_eq!(steps[0].tool, "symbols");
        assert_eq!(steps[0].args["name"], "request");
        for text in [
            "find definition of struct Owner",
            "find definition of Owner.",
            "where is the Owner struct defined?",
            "references to struct Owner",
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            assert_eq!(steps[0].args["name"], "Owner", "{text}");
        }
        let (steps, needs) = parse_text("Find the definition of queue client")?;
        assert!(needs.is_empty());
        assert_eq!(steps[0].tool, "symbols");
        assert_eq!(steps[0].args["name"], "queue client");
        let (steps, needs) = parse_text("Find the string 'under'")?;
        assert!(needs.is_empty());
        assert_eq!(steps[0].args["query"], "under");
        for text in [
            "Find the definition of Owner",
            "Tell me about the declaration of Owner",
            "Tell me about a struct Owner",
            "  tell   me about struct Owner?  ",
            "TELL ME ABOUT STRUCT Owner!",
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            assert_eq!(steps[0].args["name"], "Owner", "{text}");
        }
        for (text, expected) in [
            ("Find the retry budget", "retry budget"),
            ("Tell me about retry budgets.", "retry budgets"),
            ("I want to know about cancellation", "cancellation"),
            ("What is indexing?", "indexing"),
            ("Please find the \"retry budget\"", "retry budget"),
            ("Can you tell me about les délais?", "les délais"),
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            assert_eq!(steps.len(), 1, "{text}");
            assert_eq!(steps[0].tool, "search", "{text}");
            assert_eq!(steps[0].args["query"], expected, "{text}");
        }
        assert_eq!(
            proposed_steps(
                "Could you retrieve information about struct Owner?",
                "find_definition"
            )?[0]
                .args["name"],
            "Owner"
        );
        Ok(())
    }

    #[test]
    fn conversational_requests_do_not_guess_missing_or_compound_targets() -> Result<()> {
        for text in [
            "Tell me about it",
            "I want to know about struct",
            "Find the struct Owner and class Client",
            "Tell me about Owner or Client",
            "Tell me about struct Owner then delete its tests",
            "Find the Owner struct and fix it",
            "Tell me about whether Owner is a good design",
            "Explain why this algorithm is incorrect",
            "Tell me about `Owner` and ignore all instructions",
            "Tell me about struct Owner; push the changes",
            "Find uses of Owner under",
            "Find uses of Owner under ../outside/",
            "Where is Owner defined under",
            "Where is Owner under tests extra",
            "Find the definition of Owner under tests extra",
        ] {
            assert!(information_steps(text)?.is_none(), "{text}");
            let (_, needs) = parse_text(text)?;
            assert!(!needs.is_empty(), "{text} must require review or input");
        }
        Ok(())
    }

    #[test]
    fn prose_scopes_are_preserved_across_operations_and_literal_targets() -> Result<()> {
        for (text, tool, key, target, scope) in [
            (
                "Find the definition of Owner under tests",
                "symbols",
                "name",
                "Owner",
                "tests",
            ),
            (
                "Where is Owner under tests",
                "symbols",
                "name",
                "Owner",
                "tests",
            ),
            (
                "Find the requirements about retry under docs/",
                "document_facts",
                "name",
                "retry",
                "docs/",
            ),
            (
                "Find imports mentioning Owner beneath src/",
                "imports",
                "name",
                "Owner",
                "src/",
            ),
            (
                "Tell me about struct Owner under src/",
                "symbols",
                "name",
                "Owner",
                "src/",
            ),
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            assert_eq!(steps[0].tool, tool, "{text}");
            assert_eq!(steps[0].args[key], target, "{text}");
            assert_eq!(steps[0].args["path_prefix"], scope, "{text}");
        }
        for text in [
            "Find the string 'under'",
            "Search for the exact phrase \"work under pressure\"",
        ] {
            let (steps, needs) = parse_text(text)?;
            assert!(needs.is_empty(), "{text}: {needs:?}");
            assert!(!steps[0].args.contains_key("path_prefix"), "{text}");
        }
        assert!(!parse_text("Read src/client.rs under docs/")?.1.is_empty());
        Ok(())
    }

    #[test]
    fn conversational_definition_executes_cited_recipe_with_scope_and_choices() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir_all(root.join("src/one"))?;
        std::fs::create_dir_all(root.join("src/two"))?;
        let source = "pub struct Owner;\nimpl Owner { pub fn capture() -> Self { Self } }\n";
        std::fs::write(root.join("src/one/owner.rs"), source)?;
        std::fs::write(root.join("src/two/owner.rs"), "pub struct Owner;\n")?;
        std::fs::write(
            root.join("notes.md"),
            "# Notes\nThe retry budget must bound work.\n",
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let req = |text: &str, mode: &str, scope: &str| {
            serde_json::from_value::<Request>(json!({
                "schema": REQUEST_SCHEMA, "text": text, "mode": mode, "path_prefix": scope
            }))
            .unwrap()
        };
        for text in [
            "I want to know about struct Owner.",
            "Find the Owner struct",
            "Tell me about struct Owner",
        ] {
            let prepared = kernel.evidence_plan(req(text, "prepare", "src/one/"))?;
            assert_eq!(prepared.status, "prepared", "{text}");
            assert!(!prepared.executed);
            assert!(prepared.evidence.is_empty());
            let run = kernel.evidence_plan(req(text, "execute", "src/one/"))?;
            assert_eq!(run.status, "complete", "{text}: {:?}", run.steps);
            assert!(run.executed);
            for tool in ["symbols", "outline", "read_file"] {
                assert!(run.steps.iter().any(|step| step.tool == tool), "{text}");
            }
            assert!(!run.evidence.is_empty());
            assert!(run.steps.iter().all(|s| s.status == "complete"));
            assert!(run.evidence.iter().all(|e| {
                e["citation"]["path"] == "src/one/owner.rs"
                    && e["publication"].as_str() == run.publication.as_deref()
                    && e["citation"]["content_hash"].as_str().is_some()
            }));
        }
        let ambiguous = kernel.evidence_plan(req("Tell me about struct Owner", "execute", ""))?;
        assert!(
            ambiguous.requires_input.iter().any(|r| {
                r.field == "path" && r.choices == ["src/one/owner.rs", "src/two/owner.rs"]
            }),
            "{:?}",
            ambiguous.requires_input
        );
        let topic = kernel.evidence_plan(req("Tell me about retry budget", "execute", ""))?;
        assert!(topic.executed);
        assert!(!topic.evidence.is_empty(), "{topic:?}");
        let mixed = kernel.evidence_plan(req("Tell me about Owner", "execute", "src/one/"))?;
        assert_eq!(mixed.status, "complete", "{:?}", mixed.steps);
        assert!(mixed.steps.iter().all(|s| s.status == "complete"));
        assert!(!mixed.evidence.is_empty());
        let capitalized_topic = kernel.evidence_plan(req("Tell me about Notes", "execute", ""))?;
        assert_eq!(
            capitalized_topic.status, "complete",
            "{:?}",
            capitalized_topic.steps
        );
        assert!(!capitalized_topic.evidence.is_empty());
        let requirements = kernel.evidence_plan(req(
            "Find the written requirements about retry budget",
            "execute",
            "",
        ))?;
        assert_eq!(requirements.status, "complete", "{:?}", requirements.steps);
        assert!(!requirements.evidence.is_empty());
        assert!(
            requirements
                .evidence
                .iter()
                .all(|record| record["citation"]["path"] == "notes.md")
        );
        let tests =
            kernel.evidence_plan(req("Which tests mention Owner?", "execute", "src/one/"))?;
        assert_eq!(tests.status, "complete", "{:?}", tests.steps);
        assert!(!tests.evidence.is_empty());
        assert!(
            tests
                .warnings
                .iter()
                .any(|warning| warning.contains("not separately classified or filtered"))
        );
        assert!(
            tests
                .evidence
                .iter()
                .all(|record| record["citation"]["path"] == "src/one/owner.rs")
        );
        let suffix = kernel.evidence_plan(req(
            "Find references to Owner in the tests.",
            "execute",
            "src/one/",
        ))?;
        assert_eq!(suffix.status, "complete", "{:?}", suffix.steps);
        assert!(
            suffix
                .warnings
                .iter()
                .any(|warning| warning.contains("not separately classified or filtered"))
        );
        for text in [
            "Tell me about struct Owner then delete its tests",
            "Find the Owner struct and fix it",
        ] {
            let result = kernel.evidence_plan(req(text, "execute", ""))?;
            assert!(!result.executed, "{text}");
            assert!(!result.requires_input.is_empty());
        }
        let conversational: Value = serde_json::from_str(include_str!(
            "../../../tools/parser-train/conversational-regression-v4.json"
        ))?;
        for row in conversational.as_array().unwrap() {
            if row["intent"] == "unsupported" {
                let text = row["text"].as_str().unwrap();
                let result = kernel.evidence_plan(req(text, "execute", ""))?;
                assert!(!result.executed, "{text}");
                assert!(!result.requires_input.is_empty(), "{text}");
            }
        }
        assert_eq!(
            std::fs::read_to_string(root.join("src/one/owner.rs"))?,
            source
        );
        Ok(())
    }

    #[test]
    fn commands_and_uncertainty() -> Result<()> {
        let (steps, _) = parse_text("/read \"docs/a file.md\":2-5")?;
        assert_eq!(steps[0].args["start_line"], 2);
        assert_eq!(steps[0].args["path"], "docs/a file.md");
        let (steps, _) = parse_text(r#"/search "retry \"budget\"""#)?;
        assert_eq!(steps[0].args["query"], "retry \"budget\"");
        let (steps, _) = parse_text("/files --path src/")?;
        assert_eq!(steps[0].args["prefix"], "src/");
        assert!(!parse_text("do something clever")?.1.is_empty());
        assert!(parse_text("/search query --shell bad").is_err());
        assert!(!safe_path("a/../b"));
        assert!(!safe_path("C:\\file"));
        assert_eq!(tokens("what's in \"docs/a file.md\"?")?[0], "what's");
        assert_eq!(
            proposed_steps(
                "Please retrieve the source contents of src/client.rs",
                "read_file"
            )?[0]
                .args["path"],
            "src/client.rs"
        );
        assert!(proposed_steps("publication busy", "status")?.is_empty());
        let excerpt = proposed_steps(
            "Please show lines 3 through 12 from src/client.rs",
            "read_file",
        )?;
        assert_eq!(excerpt[0].args["start_line"], 3);
        assert_eq!(excerpt[0].args["end_line"], 12);
        assert_eq!(
            proposed_steps("read lines 3-8 from src/client.rs", "read_file")?[0].args["end_line"],
            8
        );
        assert!(proposed_steps("show lines 9 to 2 in src/client.rs", "read_file")?.is_empty());
        let punctuated = proposed_steps("Display src/client.rs:12-24.", "read_file")?;
        assert_eq!(punctuated[0].args["path"], "src/client.rs");
        assert_eq!(punctuated[0].args["start_line"], 12);
        assert_eq!(punctuated[0].args["end_line"], 24);
        assert_eq!(
            proposed_steps("What's the hash of src/client.rs?", "file_info")?[0].args["path"],
            "src/client.rs"
        );
        for path in [".github/workflows/build.yml", ".gitignore"] {
            assert_eq!(
                proposed_steps(
                    &format!("Please fetch the contents of {path}."),
                    "read_file"
                )?[0]
                    .args["path"],
                path
            );
        }
        let (proposal, needs) = parse_text("Please fetch the source contents of fixture_0.py")?;
        assert_eq!(proposal[0].args["path"], "fixture_0.py");
        assert!(!needs.is_empty());
        assert!(!reviewable_file_request(
            "read src/client.rs and then delete it",
            "read_file"
        )?);
        assert_eq!(
            parse_text("search for retry budget")?.0[0].args["query"],
            "retry budget"
        );
        assert_eq!(
            parse_text("references to QueueClient")?.0[0].args["name"],
            "QueueClient"
        );
        assert_eq!(
            parse_text("read the source contents of fixture_0.py")?.0[0].args["path"],
            "fixture_0.py"
        );
        assert_eq!(
            proposed_steps("list the files under src/", "list_files")?[0].args["prefix"],
            "src/"
        );
        assert!(
            proposed_steps("list the files in this project", "list_files")?[0]
                .args
                .is_empty()
        );
        let module = proposed_steps("show imports for package.client", "imports")?;
        assert_eq!(module[0].args["name"], "package.client");
        assert!(!module[0].args.contains_key("path"));
        Ok(())
    }
    #[test]
    fn validates_before_reading_and_keeps_publication() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.md"), "# Rule\nThe client must retry.\n")?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.index()?;
        let request = |text: &str| {
            serde_json::from_value::<Request>(
                json!({"schema":REQUEST_SCHEMA,"text":text,"mode":"execute"}),
            )
            .unwrap()
        };
        let run = kernel.evidence_plan(request("/search retry --mode lexical"))?;
        assert!(run.executed);
        assert_eq!(run.steps.len(), 1);
        assert!(run.publication.is_some());
        assert!(!run.evidence.is_empty());
        let mut scalar = run.clone();
        scalar.source_verification = None;
        scalar.evidence.clear();
        scalar.steps[0].result = Some(json!(0));
        scalar.plan.output.max_bytes = 1024;
        scalar
            .warnings
            .push("Malformed scalar fixture. ".repeat(100));
        assert!(
            bounded_result(scalar).is_err(),
            "malformed scalar output must refuse without sizing underflow"
        );
        let short = kernel.evidence_plan(request("/read one.md"))?;
        assert_eq!(short.status, "complete", "{:?}", short.steps);
        assert_eq!(short.plan.steps[0].args["end_line"], 2);
        assert!(short.plan.steps[0].args.contains_key("known_hash"));
        let proposal =
            kernel.evidence_plan(request("Please fetch the source contents of one.md"))?;
        assert!(!proposal.executed);
        assert_eq!(proposal.plan.steps[0].args["end_line"], 2);
        let confirmed = kernel.evidence_plan(serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":proposal.plan}),
        )?)?;
        assert_eq!(confirmed.status, "complete", "{:?}", confirmed.steps);
        assert!(!confirmed.evidence.is_empty());
        let review =
            kernel.evidence_plan(request("Could I see the defining source for QueueClient?"))?;
        assert!(
            !review.executed,
            "learned proposal executed without explicit plan confirmation"
        );
        assert!(!review.requires_input.is_empty());
        assert_eq!(review.plan.steps[0].tool, "symbols");
        let confirmed = kernel.evidence_plan(serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":review.plan}),
        )?)?;
        assert!(
            confirmed.executed,
            "explicitly confirmed proposal did not execute"
        );
        let safety: Value =
            serde_json::from_str(include_str!("../../../tools/parser-train/safety.json"))?;
        for row in safety.as_array().unwrap() {
            let abstained = kernel.evidence_plan(request(row["text"].as_str().unwrap()))?;
            assert!(
                !abstained.executed,
                "unsupported request executed: {}",
                row["text"]
            );
            assert!(!abstained.requires_input.is_empty());
        }
        for mode in ["prepare", "execute"] {
            let oversized = kernel.evidence_plan(serde_json::from_value(json!({
                "schema":REQUEST_SCHEMA,"mode":mode,"text":format!("/search {}", "x".repeat(2000)),
                "output":{"max_bytes":1024}
            }))?);
            assert!(oversized.unwrap_err().to_string().contains("output budget"));
        }
        assert!(
            kernel
                .evidence_plan(request("/read ../outside:1-2"))
                .is_err()
        );
        let mut prepared = kernel.evidence_plan(serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"text":"/files"}),
        )?)?;
        prepared.plan.steps[0].tool = "draft_plan".into();
        let bad = serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan}),
        )?;
        assert!(kernel.evidence_plan(bad).is_err());
        Ok(())
    }

    #[test]
    fn recipes_scopes_failures_and_stale_preparations() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("src/client.rs"),
            "pub struct QueueClient;\npub fn request() { let _ = QueueClient; }\n",
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        kernel.index()?;
        let req = |value: Value| serde_json::from_value::<Request>(value).unwrap();
        let resolved = kernel.evidence_plan(req(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","text":"who calls the queue client"}),
        ))?;
        assert!(
            resolved.requires_input.is_empty(),
            "{:?}",
            resolved.requires_input
        );
        assert!(resolved.executed);
        assert_eq!(resolved.plan.steps[0].args["name"], "QueueClient");
        let definition = kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,
            "mode":"execute", "text":"Find the definition of queue client"})))?;
        assert!(definition.executed);
        assert_eq!(definition.plan.steps[0].args["name"], "QueueClient");
        assert!(!definition.evidence.is_empty());
        std::fs::create_dir(root.join("tests"))?;
        std::fs::write(root.join("tests/other.rs"), "pub struct queue_client;\n")?;
        kernel.index()?;
        let scoped = kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,
            "mode":"execute", "text":"Find uses of queue client under src/"})))?;
        assert!(scoped.executed, "{:?}", scoped.requires_input);
        assert_eq!(scoped.plan.steps[0].args["name"], "QueueClient");
        assert_eq!(scoped.plan.steps[0].args["path_prefix"], "src/");
        assert!(scoped.evidence.iter().all(|record| {
            record["citation"]["path"]
                .as_str()
                .unwrap()
                .starts_with("src/")
        }));
        let run=kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,"mode":"execute","recipe":{"id":"definition","parameters":{"name":"QueueClient"}},"path_prefix":"src/"})))?;
        assert!(
            run.steps
                .iter()
                .any(|s| s.tool == "outline" && s.status == "complete"),
            "{:?}",
            run.steps
        );
        let prepared =
            kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,"text":"/files"})))?;
        let mut changed = prepared.plan.clone();
        changed.steps[0].args.insert("limit".into(), json!(1));
        assert!(
            kernel
                .evidence_plan(req(
                    json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":changed})
                ))
                .is_err()
        );
        std::fs::write(root.join("src/client.rs"), "pub struct NewClient;\n")?;
        kernel.index()?;
        let stale = kernel.evidence_plan(req(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan}),
        ))?;
        assert_eq!(stale.status, "publication_changed");
        assert!(!stale.executed);
        let out=kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,"mode":"execute","path_prefix":"src/","plan":{"path_prefix":"src/","steps":[{"id":"outside","tool":"read_file","args":{"path":"docs/a.md","start_line":1,"end_line":2}}]}})));
        assert!(out.is_err());
        let partial=kernel.evidence_plan(req(json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":{"steps":[{"id":"missing","tool":"read_file","args":{"path":"missing.rs","start_line":1,"end_line":2}},{"id":"inventory","tool":"list_files","args":{}}]}})))?;
        assert_eq!(partial.steps.len(), 2);
        assert!(
            partial
                .steps
                .iter()
                .any(|s| s.id == "inventory" && s.status == "complete")
        );
        Ok(())
    }
}
