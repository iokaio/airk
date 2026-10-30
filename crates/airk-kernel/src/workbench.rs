// SPDX-License-Identifier: Apache-2.0
//! Finite read composition. No transport recursion, source writes or background jobs.
use crate::{FactQuery, Kernel};
use airk_core::{
    SearchQuery, discovery::DiscoveryOptions, evidence::EvidenceReadOptions, workbench::*,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

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
            let path = word.trim_matches(['`', '"', '\'', '?', ',', ';']);
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
        .filter(|name| !name.is_empty());
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
            .find(|word| word.starts_with("airk-r1:"))
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
        "facts" => "document_facts",
        "changes" => "changes",
        "context" => "context",
        "relationships" => "relationships",
        "status" => "status",
        "diagnostics" => "diagnostics",
        "capabilities" => "capabilities",
        "help" => "help",
        _ if !explicit => {
            let prefixes = [
                ("who calls ", "references"),
                ("references to ", "references"),
                ("usages of ", "references"),
                ("where is ", "symbols"),
                ("find definition of ", "symbols"),
                ("find ", "search"),
                ("search for ", "search"),
            ];
            if let Some((prefix, tool)) = prefixes
                .iter()
                .find(|(prefix, _)| lower.starts_with(prefix))
            {
                let tail = text[prefix.len()..]
                    .trim()
                    .trim_end_matches('?')
                    .trim_end_matches(" defined");
                return Ok((
                    vec![step(
                        "query",
                        tool,
                        if *tool == "search" {
                            json!({"query":tail})
                        } else {
                            json!({"name":tail})
                        },
                    )],
                    vec![],
                ));
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
                    " Candidate intent: {} (logit {}, margin {}); the model is not qualified for automatic execution. Choose an explicit command or recipe.",
                    prediction.intent, prediction.logit, prediction.margin
                )
            } else {
                String::new()
            };
            return Ok((
                vec![],
                vec![format!(
                    "Choose a slash command or recipe, or provide explicit steps. This request has no unambiguous supported intent.{hint}"
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
                _ => bail!("unknown command option"),
            };
            i += 1;
            let value = words.get(i).context("option needs a value")?;
            args.insert(
                key.into(),
                if matches!(key, "limit" | "max_bytes") {
                    json!(value.parse::<usize>()?)
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
        "symbols" | "references" | "imports" => {
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
        "search" => &["query", "mode", "path_prefix", "limit", "max_bytes"],
        "symbols" | "imports" => &[
            "path",
            "name",
            "language",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
        ],
        "code_facts" | "document_facts" => &[
            "path",
            "name",
            "kind",
            "language",
            "path_prefix",
            "limit",
            "max_bytes",
            "cursor",
        ],
        "references" => &["name", "path_prefix", "limit", "max_bytes"],
        "read_file" => &[
            "path",
            "start_line",
            "end_line",
            "known_hash",
            "revision",
            "max_bytes",
        ],
        "file_info" => &["path", "known_hash", "revision"],
        "outline" => &[
            "path",
            "known_hash",
            "revision",
            "limit",
            "max_bytes",
            "cursor",
        ],
        "context" => &[
            "path",
            "line",
            "before",
            "after",
            "known_hash",
            "revision",
            "max_bytes",
        ],
        "list_files" => &["prefix", "limit", "max_bytes", "cursor"],
        "changes" => &["since", "path_prefix", "limit", "max_bytes", "cursor"],
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
        let properties:Map<String,Value>=fields(tool).iter().map(|key|{
            let schema=if matches!(*key,"limit"|"max_bytes"|"start_line"|"end_line"|"line"|"before"|"after"|"hops") {json!({"type":"integer","minimum":if matches!(*key,"before"|"after"){0}else{1},"maximum":numeric_max(key)})}else if *key=="mode" {json!({"type":"string","enum":["lexical","vector","hybrid"]})}else if *key=="direction" {json!({"type":"string","enum":["incoming","outgoing"]})}else{json!({"type":"string","maxLength":4096})};
            (key.to_string(),schema)
        }).collect();
        let required:Vec<&str>=match *tool {"search"=>vec!["query"],"references"=>vec!["name"],"read_file"=>vec!["path","start_line","end_line"],"file_info"|"outline"|"context"=>vec!["path"],"changes"=>vec!["since"],"relationships"=>vec!["symbol"],_=>vec![]};
        (tool.to_string(),json!({"type":"object","additionalProperties":false,"properties":properties,"required":required}))
    }).collect();
    json!({"catalog":CATALOG,"operations":operations,"selectors":["unique_definition_path","hit_locations","selected_symbol_ids"],"recipes":["definition","documents","impact","tests","component","readiness"],"classifier":"candidate suggestions only"})
}
fn validate_step(step: &Step, prefix: &str, bound: bool) -> Result<()> {
    ensure!(
        TOOLS.contains(&step.tool.as_str()),
        "operation is not read-only evidence"
    );
    for (key, value) in &step.args {
        ensure!(
            fields(&step.tool).contains(&key.as_str()),
            "unknown {} argument {key}",
            step.tool
        );
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
        } else {
            ensure!(
                value
                    .as_str()
                    .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
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
        "changes" => &["since"],
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

/// Extract rich kernel citations without flattening original document locations.
pub fn locations(value: &Value) -> Vec<Value> {
    fn visit(value: &Value, out: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if let Some(citation) = map
                    .get("citation")
                    .filter(|c| c.get("path").and_then(Value::as_str).is_some())
                {
                    out.push(json!({"citation":citation,"text":map.get("text").or_else(|| map.get("name")).unwrap_or(&Value::Null),"fact":value}));
                } else {
                    for child in map.values() {
                        visit(child, out);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, out);
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    visit(value, &mut out);
    let mut seen = BTreeSet::new();
    out.retain(|v| seen.insert(v["citation"].to_string()));
    out
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

fn bounded_result(mut result: ResultEnvelope) -> Result<ResultEnvelope> {
    // Clip whole records/payloads, never serialized JSON or a citation tuple.
    while serde_json::to_vec(&result)?.len() > result.plan.output.max_bytes {
        result.truncated = true;
        if !result.completeness.iter().any(|r| r == "output clipped") {
            result.completeness.push("output clipped".into());
        }
        if result.evidence.pop().is_none() {
            if let Some(report) = result.steps.iter_mut().rev().find(|r| r.result.is_some()) {
                report.result = None;
            } else {
                bail!("output budget cannot fit plan and reports");
            }
        }
        if result.status == "complete" {
            result.status = "partial".into();
        }
    }
    Ok(result)
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
        ensure!(plan.steps.len() <= 6, "plan exceeds six steps");
        ensure!(
            !plan.steps.is_empty() || !needs.is_empty(),
            "explicit plan has no steps"
        );
        let mut ids: BTreeMap<String, String> = BTreeMap::new();
        for step in &mut plan.steps {
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
                        && matches!(step.tool.as_str(), "outline" | "file_info"))
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
        if plan.steps.iter().any(|s| is_evidence(&s.tool)) {
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
        if text_request && !stale {
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
                    let value = serde_json::to_value(self.facts_page(
                        &FactQuery {
                            path_prefix: plan.path_prefix.clone(),
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
                    )?)?;
                    ensure!(
                        publication(&value) == plan.publication,
                        "publication changed during name resolution"
                    );
                    resolution_bytes += serde_json::to_vec(&value)?.len();
                    if !locations(&value).is_empty() {
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
        let mut result = ResultEnvelope {schema: RESULT_SCHEMA.into(), collection:self.collection().into(),publication:plan.publication.clone(), status:if needs.is_empty(){"prepared"}else{"needs_input"}.into(),executed:false, plan:plan.clone(),requires_input:needs.into_iter().map(|reason|RequiredInput{field:"text".into(),step:None,reason,choices:vec![]}).collect(),steps:vec![],evidence:vec![],warnings:vec!["Untrusted indexed evidence; verify live files before edits. Syntax relationships are candidates, not resolved calls.".into()],truncated:false,completeness:vec![],calls,payload_bytes:resolution_bytes};
        result.requires_input.extend(choices);
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
                let answer =
                    self.plan_read(&step.tool, &args, plan.publication.as_deref(), observations);
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
                        result
                            .completeness
                            .push(format!("{}: read failed", step.id));
                    }
                    Ok(value) => {
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
                            for mut record in locations(&value) {
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
        bounded_result(result)
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
        let options = DiscoveryOptions {
            cursor: args
                .get("cursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
            compact: false,
            minimal: false,
        };
        let evidence: EvidenceReadOptions = serde_json::from_value(
            json!({"revision":revision,"known_hash":args.get("known_hash")}),
        )?;
        Ok(match tool {
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
                json!({"commands":"/search /symbols /refs /read /outline /files /info /imports /facts /changes --since /status /diagnostics /capabilities /context /relationships /help","recipes":["definition","documents","impact","tests","component","readiness"],"parser":"deterministic rules; unsupported phrasing returns needs_input"})
            }
            "capabilities" => {
                json!({"adapters":Kernel::extraction_adapters().iter().map(|a|json!({"id":a.profile.adapter_id})).collect::<Vec<_>>(),"workbench":operation_catalog()})
            }
            "search" => serde_json::to_value(self.search_response(
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
                Some(768),
            )?)?,
            "list_files" => serde_json::to_value(self.list_files_page(
                args.get("prefix").and_then(Value::as_str).unwrap_or(""),
                limit,
                max,
                &options,
            )?)?,
            "references" => {
                serde_json::to_value(self.references(string(args, "name")?, prefix, limit, max)?)?
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
                serde_json::to_value(if tool == "code_facts" {
                    self.code_facts_page(&query, &options)?
                } else {
                    self.facts_page(&query, &options)?
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
        })
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
    );
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
        ensure!(!symbols.is_empty(), "dependency has no native symbol IDs");
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
        let (proposal, needs) = parse_text("Please retrieve the source contents of fixture_0.py")?;
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
        let short = kernel.evidence_plan(request("/read one.md"))?;
        assert_eq!(short.status, "complete", "{:?}", short.steps);
        assert_eq!(short.plan.steps[0].args["end_line"], 2);
        assert!(short.plan.steps[0].args.contains_key("known_hash"));
        let proposal =
            kernel.evidence_plan(request("Please retrieve the source contents of one.md"))?;
        assert!(!proposal.executed);
        assert_eq!(proposal.plan.steps[0].args["end_line"], 2);
        let confirmed = kernel.evidence_plan(serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":proposal.plan}),
        )?)?;
        assert_eq!(confirmed.status, "complete", "{:?}", confirmed.steps);
        assert!(!confirmed.evidence.is_empty());
        let review =
            kernel.evidence_plan(request("Could you retrieve the definition of QueueClient?"))?;
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
