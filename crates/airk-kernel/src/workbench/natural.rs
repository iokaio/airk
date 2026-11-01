// SPDX-License-Identifier: Apache-2.0
//! Anchored, finite evidence requests shared by prose-facing adapters.
//!
//! These templates consume a complete request and preserve its literal target.
//! They select existing read operations; they neither answer questions nor infer
//! runtime behavior, test coverage, dependencies or source freshness.
use super::{safe_path, step, tokens};
use airk_core::workbench::{Recipe, Step};
use anyhow::Result;
use serde_json::{Map, Value, json};

fn atom(word: &str, alternatives: &str) -> bool {
    alternatives
        .split('|')
        .any(|candidate| word.eq_ignore_ascii_case(candidate))
}

fn fixed(words: &[String], pattern: &[&str]) -> Option<usize> {
    let mut consumed = 0;
    for part in pattern {
        let optional = part.starts_with('[') && part.ends_with(']');
        let part = part.trim_start_matches('[').trim_end_matches(']');
        if words.get(consumed).is_some_and(|word| atom(word, part)) {
            consumed += 1;
        } else if !optional {
            return None;
        }
    }
    Some(consumed)
}

/// One variable slot, with anchored literal/alternative/optional words on both
/// sides. The suffix is tested at every boundary, so no tail is silently lost.
fn capture<'a>(words: &'a [String], pattern: &str) -> Option<&'a [String]> {
    let pattern: Vec<_> = pattern.split_whitespace().collect();
    let slot = pattern.iter().position(|word| *word == "{}")?;
    let start = fixed(words, &pattern[..slot])?;
    for end in start + 1..=words.len() {
        if fixed(&words[end..], &pattern[slot + 1..]) == Some(words.len() - end) {
            return Some(&words[start..end]);
        }
    }
    None
}

fn captured<'a>(words: &'a [String], patterns: &[&str]) -> Option<&'a [String]> {
    patterns.iter().find_map(|pattern| capture(words, pattern))
}

fn complete(words: &[String], patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| {
        fixed(words, &pattern.split_whitespace().collect::<Vec<_>>()) == Some(words.len())
    })
}

fn remove_prefix(words: &mut Vec<String>, patterns: &[&str]) {
    if let Some(amount) = patterns
        .iter()
        .find_map(|pattern| fixed(words, &pattern.split_whitespace().collect::<Vec<_>>()))
        && amount < words.len()
    {
        words.drain(..amount);
    }
}

fn clean_target(words: &[String]) -> Vec<String> {
    let mut words = words.to_vec();
    remove_prefix(&mut words, &["the", "a", "an"]);
    for suffix in [
        "for me",
        "in this codebase",
        "in this project",
        "in the indexed project",
        "in the tests",
        "in the test suite",
        "for an API reference",
        "so I can check terminology",
        "before touching the implementation",
    ] {
        let suffix: Vec<_> = suffix.split_whitespace().collect();
        if words.len() > suffix.len()
            && fixed(&words[words.len() - suffix.len()..], &suffix) == Some(suffix.len())
        {
            words.truncate(words.len() - suffix.len());
            break;
        }
    }
    words
}

fn simple_target(words: &[String]) -> bool {
    !words.is_empty()
        && words.iter().all(|word| {
            !word.contains([';', '\n', '\r', '{', '}', '&', '|', '?', '!'])
                && ![
                    "it",
                    "this",
                    "that",
                    "them",
                    "something",
                    "anything",
                    "and",
                    "or",
                    "then",
                    "also",
                    "why",
                    "whether",
                    "prove",
                    "evaluate",
                    "best",
                    "better",
                ]
                .contains(&word.to_lowercase().as_str())
                && (words.len() == 1
                    || ![
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
                        "publish",
                        "deploy",
                        "summarize",
                        "implement",
                        "fix",
                        "repair",
                        "unregister",
                        "reindex",
                    ]
                    .contains(&word.to_lowercase().as_str()))
        })
}

fn symbol(words: &[String]) -> Option<String> {
    let mut words = clean_target(words);
    // These are syntactic role/type modifiers, never an inferred symbol kind.
    remove_prefix(
        &mut words,
        &[
            "qualified name",
            "exact name",
            "exact symbol",
            "type alias",
            "fixture builder",
        ],
    );
    remove_prefix(&mut words, &["test", "mock"]);
    remove_prefix(
        &mut words,
        &[
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
            "identifier",
            "fixture",
        ],
    );
    remove_prefix(&mut words, &["named", "called"]);
    if words.len() > 1
        && words.last().is_some_and(|word| {
            atom(
                word,
                "struct|class|enum|trait|interface|type|function|method|module|symbol",
            )
        })
    {
        words.pop();
    }
    if !simple_target(&words) || words.len() > 4 {
        return None;
    }
    let name = words.join(" ");
    let name = name.trim_matches(['`', '"', '\'']);
    let name = name
        .strip_suffix("'s")
        .or_else(|| name.strip_suffix("’s"))
        .unwrap_or(name);
    let valid = name.split_whitespace().all(|word| {
        word.split("::")
            .flat_map(|part| part.split('.'))
            .all(|part| {
                let mut chars = part.chars();
                chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                    && chars.all(|c| c.is_alphanumeric() || c == '_')
            })
    });
    valid.then(|| name.to_string())
}

fn query(words: &[String]) -> Option<String> {
    let words = clean_target(words);
    simple_target(&words).then(|| {
        words
            .join(" ")
            .trim_matches(['`', '"', '\'', ','])
            .to_string()
    })
}

fn one(tool: &str, args: Value) -> Vec<Step> {
    vec![step("evidence", tool, args)]
}

fn definition(words: &[String]) -> Result<Option<Vec<Step>>> {
    let Some((target, scope)) = scoped_subject(words) else {
        return Ok(None);
    };
    let Some(name) = symbol(&target) else {
        return Ok(None);
    };
    let mut steps = super::recipe(&Recipe {
        id: "definition".into(),
        parameters: Map::from_iter([("name".into(), json!(name))]),
    })?;
    if let Some(scope) = scope {
        steps[0].args.insert("path_prefix".into(), json!(scope));
    }
    Ok(Some(steps))
}

/// Reusable verb + qualified object + relation + target grammar. Noun phrases
/// and relation phrases stay exact, rather than searching for a word anywhere.
fn object_slot<'a>(words: &'a [String], nouns: &[&str]) -> Option<&'a [String]> {
    let mut start = 0;
    for prefix in [
        "tell me about",
        "show me",
        "give me",
        "i need",
        "i want",
        "bring me",
        "find",
        "locate",
        "retrieve",
        "show",
        "get",
        "list",
        "which",
        "what",
        "look up",
        "look for",
        "bring up",
        "can I see",
    ] {
        if let Some(amount) = fixed(words, &prefix.split_whitespace().collect::<Vec<_>>()) {
            start = amount;
            break;
        }
    }
    while words.get(start).is_some_and(|word| atom(word,
        "the|a|an|written|documented|recorded|indexed|stated|existing|source-bound|published|exact"))
    {
        start += 1;
    }
    for noun in nouns {
        let Some(amount) = fixed(
            &words[start..],
            &noun.split_whitespace().collect::<Vec<_>>(),
        ) else {
            continue;
        };
        let end = start + amount;
        for relation in [
            "recording the motivation for",
            "that discusses how",
            "that describe",
            "that discusses",
            "that demonstrate",
            "that mentions",
            "that mention",
            "written for",
            "refer to",
            "about",
            "for",
            "of",
            "to",
            "on",
            "mentioning",
            "concerning",
            "regarding",
            "matching",
            "mention",
            "concern",
            "describe",
            "describing",
            "explains",
            "explain",
            "discuss",
            "discussing",
            "demonstrate",
            "recording",
            "handling",
            "referencing",
            "covering",
            "around",
            "named",
            "called",
            "from",
            "in",
            "explaining",
        ] {
            if let Some(amount) = fixed(
                &words[end..],
                &relation.split_whitespace().collect::<Vec<_>>(),
            ) {
                let target = &words[end + amount..];
                if !target.is_empty() {
                    return Some(target);
                }
            }
        }
    }
    None
}

fn literal_path(value: &str) -> bool {
    safe_path(value)
        && !value.is_empty()
        && (value.contains(['/', '\\'])
            || value.contains('.')
            || matches!(value, "README" | "LICENSE"))
}

fn known_file_path(value: &str) -> bool {
    value.contains(['/', '\\'])
        || (value.starts_with('.') && value.len() > 1)
        || matches!(value, "README" | "LICENSE")
        || value.rsplit_once('.').is_some_and(|(_, extension)| {
            [
                "rs", "py", "ts", "tsx", "js", "jsx", "md", "mdx", "json", "toml", "yaml", "yml",
                "go", "cs", "java", "c", "h", "cpp", "hpp", "txt", "pdf", "docx", "pptx", "xlsx",
                "html", "css", "xml", "sql", "sh", "ps1", "rb", "php", "swift", "kt",
            ]
            .contains(&extension.to_lowercase().as_str())
        })
}

fn number(value: &str) -> Option<u32> {
    match value {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        _ => value.parse().ok().filter(|number| *number > 0),
    }
}

fn range(words: &[String]) -> Option<(u32, u32)> {
    let values: Vec<_> = words
        .iter()
        .filter(|word| !atom(word, "the|indexed|just|from|of|in|at"))
        .map(|word| word.to_lowercase())
        .collect();
    let words: Vec<_> = values.iter().map(String::as_str).collect();
    let (start, end) = match words.as_slice() {
        [] => (1, 200),
        ["first", amount, "lines"] => (1, number(amount)?),
        ["line" | "lines", first, "to" | "through" | "-", last] => (number(first)?, number(last)?),
        [
            "line" | "lines",
            first,
            "to" | "through",
            "line" | "lines",
            last,
        ] => (number(first)?, number(last)?),
        ["line" | "lines", value] | [value] => {
            let (first, last) = value.split_once(['-', '–', '—']).unwrap_or((value, value));
            (number(first)?, number(last)?)
        }
        _ => return None,
    };
    (start <= end).then_some((start, end))
}

fn file_subject(words: &[String], allow_range: bool) -> Option<(String, u32, u32)> {
    let mut path = None;
    let mut rest = vec![];
    for word in words {
        let word = word.trim_matches(['`', '"', '\'', ',']);
        let (candidate, inline) = word
            .rsplit_once(':')
            .filter(|(_, tail)| {
                !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-')
            })
            .map_or((word, None), |(path, range)| (path, Some(range)));
        if literal_path(candidate) {
            if path.replace(candidate.to_string()).is_some() {
                return None;
            }
            if let Some(inline) = inline {
                rest.push(inline.to_string());
            }
        } else {
            rest.push(word.to_string());
        }
    }
    let path = path?;
    if !allow_range && !rest.is_empty() {
        return None;
    }
    let (start, end) = range(&rest)?;
    Some((path, start, end))
}

fn scoped_subject(words: &[String]) -> Option<(Vec<String>, Option<String>)> {
    if words.len() == 1 {
        return Some((words.to_vec(), None));
    }
    for index in 0..words.len() {
        if atom(&words[index], "under|beneath|within") {
            if index == 0 || index + 2 != words.len() {
                return None;
            }
            let prefix = words[index + 1].trim_matches(['`', '"', '\'']);
            if prefix.is_empty() || !safe_path(prefix) {
                return None;
            }
            return Some((words[..index].to_vec(), Some(prefix.into())));
        }
    }
    Some((words.to_vec(), None))
}

fn with_scope(mut args: Value, scope: Option<String>) -> Value {
    if let Some(scope) = scope {
        args["path_prefix"] = json!(scope);
    }
    args
}

fn revision_steps(words: &[String]) -> Option<Vec<Step>> {
    let baseline: Vec<_> = words
        .iter()
        .filter(|word| word.starts_with("airk-r1:"))
        .collect();
    if baseline.len() != 1
        || baseline[0]
            .parse::<airk_core::identity::RevisionToken>()
            .is_err()
    {
        return None;
    }
    let mut remaining = words.to_vec();
    let mut scope = None;
    if let Some(at) = remaining
        .iter()
        .position(|word| atom(word, "under|beneath|within"))
    {
        let path = remaining.get(at + 1)?;
        if path.is_empty() || !safe_path(path) {
            return None;
        }
        scope = Some(path.clone());
        remaining.drain(at..at + 2);
    }
    if remaining.iter().all(|word| word == baseline[0] || atom(word,
        "what|which|show|find|check|validate|compare|the|my|exact|saved|remembered|retained|observed|indexed|source-bound|evidence|documents|facts|files|changes|changed|since|after|against|with|relative|to|from|baseline|publication|token|in|index"))
        && remaining.iter().any(|word| atom(word, "changes|changed|validate|compare"))
    {
        return Some(one("changes", with_scope(json!({"since":baseline[0]}), scope)));
    }
    None
}

pub(super) fn explicit_steps(text: &str) -> Result<Option<Vec<Step>>> {
    if text.trim_start().starts_with('/') || text.len() > 4096 {
        return Ok(None);
    }
    let mut words = tokens(text.trim().trim_end_matches(['?', '.', '!']))?;
    remove_prefix(&mut words, &["please"]);
    remove_prefix(&mut words, &["can you", "could you", "would you"]);
    remove_prefix(&mut words, &["please"]);
    if words.is_empty() {
        return Ok(None);
    }
    if let Some(steps) = revision_steps(&words) {
        return Ok(Some(steps));
    }

    if complete(
        &words,
        &[
            "is [the|this] [local] index ready [to] [search]",
            "is semantic indexing ready",
            "show [current] semantic readiness",
            "show [the] current index publication",
            "what is [the] current index publication",
            "how many [source] files are [currently] indexed [right] [now]",
            "tell me [the] current publication and semantic readiness",
            "how many [source] files are indexed in this collection",
            "tell me [the] current indexing status",
            "has [the] project finished becoming searchable",
            "before I investigate, is this collection ready for retrieval",
            "is there a published index available for looking up documentation",
            "what is [the] current published index generation",
            "can this collection answer retrieval requests",
        ],
    ) || captured(&words, &["is [the|this] [local] index ready for {} "])
        .is_some_and(simple_target)
    {
        return Ok(Some(one("status", json!({}))));
    }
    if complete(
        &words,
        &[
            "show|inspect|get [me] [the] [detailed|current|native] [service|worker|runtime] diagnostics [for] [this] [project]",
            "inspect|get [the] [runtime|native] troubleshooting details [for] [this] [project]",
            "show runtime versions and process observations",
            "what watcher activity is being reported",
            "inspect [the] current indexing queue and worker progress",
            "inspect [the] current indexing worker progress",
            "what runtime jobs and watcher observations are available",
            "what jobs and watcher activity does AIRK report",
            "what are [the] current source watcher and job observations",
            "get [the] troubleshooting details for [the] native service",
            "I need [the] native service troubleshooting details",
            "I need detailed observations of native workers, queued jobs and the source watcher",
            "inspect [the] runtime observations for this project",
        ],
    ) {
        return Ok(Some(one("diagnostics", json!({}))));
    }
    if complete(
        &words,
        &[
            "show [me] [the] available read commands",
            "what read-only commands are available [here]",
            "show [me] AIRK command guidance",
            "show instructions for using evidence recipes",
            "where is [the] help for scoping a Workbench request",
            "how do I prepare a read-only parse plan",
            "how do I use [the] evidence commands",
            "which retrieval recipes can I use",
            "where can I find [AIRK] query syntax help",
            "where can I find help with Workbench",
            "show me how definition recipes work",
            "where is [the] guide to AIRK evidence recipes",
            "tell me how to use [the] index search commands",
        ],
    ) || captured(
        &words,
        &[
            "how do I retrieve {} with AIRK",
            "how do I search for {} with AIRK",
            "how do I ask AIRK for {} ",
        ],
    )
    .is_some_and(simple_target)
    {
        return Ok(Some(one("help", json!({}))));
    }

    // File operation templates are checked before ordinary object/topic requests.
    for (tool, patterns) in [
        (
            "outline",
            &[
                "outline {}",
                "show|list|get|give [me] [the] [indexed|structural|heading|test] outline|headings|declarations|sections|structure of|in|for|from {}",
                "what is [the] outline of {}",
                "what headings|declarations|sections are [captured|indexed] in {}",
                "what functions and types are declared in {}",
                "how is {} structured according to [the] index",
                "list [the] navigation entries for|in {}",
                "what headings and declarations are indexed in {}",
                "which sections are captured in {}",
                "what headings are in {}",
                "show [the] declaration list in {}",
                "help me inspect [the] structural outline for {}",
            ][..],
        ),
        (
            "file_info",
            &[
                "what language was detected for {}",
                "what language does [the] index report for {}",
                "what [indexed] language or profile does {} have",
                "how was {} parsed",
                "how did [the] parser classify {}",
                "which|what extraction|parser [method|profile] was used for {}",
                "tell me which extraction profile was used for {}",
                "which extraction profile produced [the] indexed representation of {}",
                "I need [the] extraction metadata associated with {} rather than its text",
                "show|get|find [the] [indexed] [file|extraction|content] metadata|hash|information for|of|about {}",
                "get metadata about {}",
                "show parsing quality information for {}",
                "get [the] indexed hash and parser information for {}",
                "what is [the] indexed content hash of {}",
            ][..],
        ),
        (
            "read_file",
            &[
                "read {}",
                "read [the] contents of {}",
                "read [the] source contents of {}",
                "show|retrieve|get|open|display [me] [the] [indexed|saved|source] contents|text|excerpt|window of|from|at {}",
                "retrieve [the] indexed source window from {}",
                "show [me] [the] text from {}",
                "retrieve [the] source contents of {}",
                "retrieve [the] source excerpt from {}",
                "pull up {}",
                "get {}",
                "open {}",
                "display {}",
                "show {}",
                "bring up {}",
                "give me indexed {}",
                "I would like to read {}",
                "I need to read {}",
                "I would like [the] indexed source text for {}",
                "could I see [the] test source in {}",
            ][..],
        ),
    ] {
        for pattern in patterns {
            if let Some(target) = capture(&words, pattern)
                && let Some((path, start, end)) = file_subject(target, tool == "read_file")
            {
                if [
                    "show {}",
                    "get {}",
                    "open {}",
                    "display {}",
                    "pull up {}",
                    "bring up {}",
                ]
                .contains(pattern)
                    && !known_file_path(&path)
                {
                    continue;
                }
                return Ok(Some(one(
                    tool,
                    if tool == "read_file" {
                        json!({"path":path,"start_line":start,"end_line":end})
                    } else {
                        json!({"path":path})
                    },
                )));
            }
        }
    }

    if let Some(target) = captured(
        &words,
        &[
            "list|find|show|browse|enumerate [me] [the] [indexed|captured|saved] [source|documentation|fixture|test|snapshot] files|paths|filenames under|beneath|in {}",
            "which [indexed|source] files|paths are indexed under|beneath {}",
            "which indexed files are beneath {}",
            "what [source] files are available under {}",
            "what filenames were captured under {}",
            "find [the] indexed test paths beneath {}",
            "show|give|find [me] [the] [file] inventory for|starting|at {}",
            "browse [the] indexed directory {}",
            "find [the] file inventory for {}",
        ],
    ) {
        let target = clean_target(target);
        if target.len() == 1 && safe_path(&target[0]) && simple_target(&target) {
            return Ok(Some(one("list_files", json!({"prefix":target[0]}))));
        }
    }
    if complete(
        &words,
        &["what files does [the] index retain for this project"],
    ) {
        return Ok(Some(one("list_files", json!({"prefix":""}))));
    }

    if let Some(target) = captured(
        &words,
        &[
            "list [the] written dependencies under|beneath {}",
            "list|show [the] imports under|beneath {}",
        ],
    ) {
        let target = clean_target(target);
        if target.len() == 1 && !target[0].is_empty() && safe_path(&target[0]) {
            return Ok(Some(one("imports", json!({"path_prefix":target[0]}))));
        }
    }
    if let Some(target) = captured(
        &words,
        &[
            "where is {} imported [in] [the] [indexed] [source]",
            "tell me where {} is imported",
            "what|which imports|modules are written|imported in|by {}",
        ],
    )
    .or_else(|| {
        object_slot(
            &words,
            &[
                "import statements",
                "import declarations",
                "include declarations",
                "include statements",
                "imported dependencies",
                "written dependency imports",
                "written imports",
                "imports",
                "imported module names",
                "source import statements",
            ],
        )
    }) {
        let filename = target.len() == 1
            && target[0].rsplit_once('.').is_some_and(|(_, extension)| {
                [
                    "rs", "py", "ts", "tsx", "js", "jsx", "go", "cs", "java", "c", "h", "cpp",
                    "hpp", "rb", "php", "swift", "kt",
                ]
                .contains(&extension.to_lowercase().as_str())
            });
        if (filename || target.iter().any(|word| word.contains(['/', '\\'])))
            && let Some((path, _, _)) = file_subject(target, false)
        {
            return Ok(Some(one("imports", json!({"path":path}))));
        }
        if let Some(name) = symbol(target) {
            return Ok(Some(one("imports", json!({"name":name}))));
        }
    }

    if let Some(target) = captured(&words, &["read [the] [indexed] document facts in {}"])
        && let Some((path, _, _)) = file_subject(target, false)
    {
        return Ok(Some(one("document_facts", json!({"path":path}))));
    }
    if let Some(target) = captured(&words, &["list [the] documented {} limits"])
        && let Some(target) = query(target)
    {
        return Ok(Some(one(
            "document_facts",
            json!({"name":target,"kind":"requirement"}),
        )));
    }
    for (kind, nouns) in [
        ("decision", &["document facts for the decision"][..]),
        (
            "requirement",
            &[
                "requirements",
                "requirement statements",
                "stated requirements",
                "constraints",
                "numeric limits",
                "retry budget limits",
                "bounds",
            ][..],
        ),
        (
            "acceptance",
            &["acceptance criteria", "acceptance statements"][..],
        ),
        (
            "decision",
            &[
                "design decisions",
                "decision statements",
                "decisions",
                "architecture decision statements",
            ][..],
        ),
        ("architecture", &["architecture statements"][..]),
        ("design", &["design facts", "design statements"][..]),
        ("feature", &["feature statements"][..]),
        ("date", &["dated statements"][..]),
        (
            "",
            &["document facts", "indexed document facts", "facts"][..],
        ),
    ] {
        if let Some(target) = object_slot(&words, nouns) {
            let target = if kind.is_empty() {
                capture(target, "[the] design of {}").unwrap_or(target)
            } else {
                target
            };
            let mut args = if let Some((path, _, _)) = file_subject(target, false) {
                json!({"path":path})
            } else if let Some(target) = query(target) {
                json!({"name":target})
            } else {
                continue;
            };
            if !kind.is_empty() {
                args["kind"] = json!(kind);
            }
            return Ok(Some(one("document_facts", args)));
        }
    }

    if let Some(target) = captured(
        &words,
        &[
            "where is [the] {} referenced in [the] code",
            "where is [the] {} used",
            "where does [the] indexed code refer to {}",
            "tell me where {} is used",
            "where do regression tests reference {}",
            "who calls {}",
            "where does [the] exact identifier {} occur",
            "where is {} named in source comments",
            "how often does [the] source mention {}",
            "could I see every indexed place where [the] identifier {} is written",
        ],
    )
    .or_else(|| {
        object_slot(
            &words,
            &[
                "textual references",
                "references",
                "usages",
                "uses",
                "textual occurrences",
                "occurrences",
                "call-site candidates",
                "call-site mentions",
                "candidate usages",
                "test cases",
                "tests",
                "test files",
                "test setup routines",
                "test cleanup code",
                "parameterized tests",
                "regression tests",
                "places",
                "locations",
                "indexed locations",
            ],
        )
    }) && let Some((target, scope)) = scoped_subject(target)
        && let Some(name) = symbol(&target)
    {
        return Ok(Some(one(
            "references",
            with_scope(json!({"name":name}), scope),
        )));
    }

    if let Some(target) = captured(
        &words,
        &[
            "where does [the] {} get declared",
            "the type {} is unfamiliar to me; could I see its declaration site",
            "where is [the] test named {} defined",
            "which [indexed] source location declares {}",
            "which source file declares {}",
            "where is [the] declaration of {}",
            "which declaration defines [the] {}",
            "take me to [the] definition|declaration of {}",
            "take me to [the] function named {}",
            "point me at [the] source that defines {}",
            "show me where [the] {} is written",
            "point me to {} declaration",
            "I am looking for [the] definition of {}",
            "pull up [the] indexed definition of {}",
            "find [the] test|mock function|class {}",
            "I want to inspect [the] function called {}",
            "where can I find [the] function {}",
            "show [the] defining source for {}",
        ],
    )
    .or_else(|| {
        object_slot(
            &words,
            &[
                "definition",
                "definition location",
                "declaration",
                "declaration site",
                "declaration location",
                "source declaration",
                "declaring source",
                "defining source",
                "source location",
                "type alias",
                "symbol",
            ],
        )
    }) {
        return definition(target);
    }

    // Existing explanations, requirements and comments are retrievable. Requests
    // to generate an explanation or judge a design are deliberately not templates.
    if let Some(target) = captured(
        &words,
        &[
            "where do [the] docs explain [the] rationale for {}",
            "where do [the] docs|ADRs explain|discuss {}",
            "search for material discussing {}",
            "search for tests discussing {}",
            "look for {} evidence",
            "search [the] repository for {}",
            "where are [the] {} documented",
            "where does [the] repository discuss {}",
            "what indexed text explains {}",
            "what do [the] indexed docs say about {}",
            "where is [the] user guide for {}",
            "where is [the] term {} described in [the] docs",
            "where is {} explained in [the] repository",
            "where is [the] contract for {} written down",
            "is there [any] documentation about {}",
            "is there [a] guide that discusses {}",
            "which changelog passages discuss {}",
            "where are [the] documented examples of {}",
            "point me toward [the] code handling {}",
            "point me to [existing] snippets about {}",
            "help me locate [the] existing walkthrough for {}",
            "look for reader-facing examples of {}",
            "I need [existing] project passages about {}",
            "I need background material on {}",
            "I am looking for existing repository wording on {} with source locations",
            "search for [the] exact phrase {}",
            "find [the] string|error-message {}",
            "where is [the] configuration key {} mentioned",
            "what is written about {}",
            "is there project text describing how {} works",
            "find [the] error message {}",
            "find text that discusses [the] tradeoffs for {}",
            "show [me] passages recording [the] motivation behind {}",
            "show [me] source comments about [the] lifecycle of {}",
            "where does [the] manual mention limitations of {}",
        ],
    )
    .or_else(|| {
        object_slot(
            &words,
            &[
                "documentation",
                "user guide",
                "guide",
                "manual",
                "introduction",
                "implementation notes",
                "code snippets",
                "snippets",
                "comments",
                "examples",
                "evidence",
                "written explanation",
                "explanation",
                "overview",
                "release notes",
                "changelog passages",
                "wording",
                "warnings",
                "caveats",
                "how-to instructions",
                "troubleshooting advice",
                "glossary passages",
                "architecture discussion",
                "documented boundaries",
                "boundaries",
                "code and document passages",
                "rationale",
                "text",
                "passages",
                "ownership boundary",
                "source comments",
                "indexed examples",
                "ADR",
                "design notes",
                "notes",
                "architecture record",
                "migration notes",
                "regression notes",
                "recorded failures",
                "failures",
                "sample payloads",
                "negative tests",
                "reproduction steps",
                "assertion text",
                "test evidence",
            ],
        )
    }) && let Some((target, scope)) = scoped_subject(target)
        && let Some(target) = query(&target)
    {
        return Ok(Some(one(
            "search",
            with_scope(json!({"query":target}), scope),
        )));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_role_requests_keep_operation_and_target() -> Result<()> {
        for (text, tool, key, target) in [
            (
                "Find the written requirements about worker isolation.",
                "document_facts",
                "name",
                "worker isolation",
            ),
            (
                "Which recorded design decisions mention shutdown?",
                "document_facts",
                "name",
                "shutdown",
            ),
            (
                "Where is the user guide for cancellation?",
                "search",
                "query",
                "cancellation",
            ),
            (
                "Where do the docs explain retry budgets?",
                "search",
                "query",
                "retry budgets",
            ),
            (
                "Find the examples that demonstrate durable receipts.",
                "search",
                "query",
                "durable receipts",
            ),
            ("Which tests mention Owner?", "references", "name", "Owner"),
            (
                "Find uses of RequestOwner under tests/unit/.",
                "references",
                "name",
                "RequestOwner",
            ),
            (
                "Find the mock class PacketClient.",
                "symbols",
                "name",
                "PacketClient",
            ),
            (
                "Where does the enum ResultCode get declared?",
                "symbols",
                "name",
                "ResultCode",
            ),
            (
                "Show the extraction metadata for src/client.rs.",
                "file_info",
                "path",
                "src/client.rs",
            ),
            (
                "What imports are written in src/client.rs?",
                "imports",
                "path",
                "src/client.rs",
            ),
            (
                "Where is package.client imported?",
                "imports",
                "name",
                "package.client",
            ),
            (
                "List the documentation files under docs/.",
                "list_files",
                "prefix",
                "docs/",
            ),
            (
                "Show the test declarations in tests/worker.rs.",
                "outline",
                "path",
                "tests/worker.rs",
            ),
        ] {
            let result = explicit_steps(text)?.unwrap_or_else(|| panic!("{text}"));
            assert_eq!(result[0].tool, tool, "{text}");
            assert_eq!(result[0].args[key], target, "{text}");
        }
        Ok(())
    }

    #[test]
    fn source_ranges_and_scopes_are_literal_and_complete() -> Result<()> {
        for text in [
            "Read lines 4 through 18 of src/owner.rs.",
            "Display src/owner.rs:4-18.",
            "Read src/owner.rs lines 4 through 18.",
        ] {
            let result = explicit_steps(text)?.unwrap_or_else(|| panic!("{text}"));
            assert_eq!(result[0].args["start_line"], 4, "{text}");
            assert_eq!(result[0].args["end_line"], 18, "{text}");
        }
        let result =
            explicit_steps("Show the first ten lines of \"docs/developer guide.md\".")?.unwrap();
        assert_eq!(result[0].args["path"], "docs/developer guide.md");
        assert_eq!(result[0].args["end_line"], 10);
        let result = explicit_steps("Find uses of Owner under tests/unit/.")?.unwrap();
        assert_eq!(result[0].args["path_prefix"], "tests/unit/");
        Ok(())
    }

    #[test]
    fn unsupported_tails_scope_escapes_and_missing_slots_are_not_consumed() -> Result<()> {
        for text in [
            "Read src/a.rs and delete it",
            "Find the requirements about retry and change them",
            "Which tests mention Owner then run them",
            "Where is the user guide for it",
            "Read ../../private.rs",
            "Show the outline of C:\\private.rs",
            "Read /etc/passwd",
            "Find the design decisions about whether Owner is correct",
            "Explain why Owner fails",
            "Find changes since last time",
            "Find uses of Owner under ../other/",
            "Show file metadata for",
            "Read src/a.rs lines 9 through 2",
            "Find the mock class Owner then rewrite it",
            "Find uses of Owner under",
            "Get Owner.capture",
            "Show package.client",
        ] {
            assert!(explicit_steps(text)?.is_none(), "{text}");
        }
        Ok(())
    }

    #[test]
    fn literal_names_that_are_also_modifiers_remain_targets() -> Result<()> {
        let result = explicit_steps("Which tests mention test?")?.unwrap();
        assert_eq!(result[0].args["name"], "test");
        let result = explicit_steps("Which tests mention under?")?.unwrap();
        assert_eq!(result[0].args["name"], "under");
        let result = explicit_steps("Search for the exact phrase \"the\".")?.unwrap();
        assert_eq!(result[0].args["query"], "the");
        Ok(())
    }
}
