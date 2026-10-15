// SPDX-License-Identifier: Apache-2.0
//! Bounded original VB.NET tokens and block context; deliberately no compiler model.
use super::*;
struct Token<'a> {
    value: &'a str,
    start: usize,
    end: usize,
    literal: bool,
}
pub(super) fn analyze(path: &str, source: &str, budget: &mut HostBudget) -> Result<Analysis> {
    let mut output = Facts::new(path, source, "vbnet")?;
    let mut scope: Vec<(String, String)> = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        budget.charge(1, 0, 0, 0)?;
        let tokens = tokens(line, offset, budget)?;
        offset += line.len();
        if tokens.is_empty() {
            continue;
        }
        if tokens
            .windows(2)
            .any(|pair| pair[0].value == "<" && identifier(pair[1].value))
        {
            break;
        }
        let lower = tokens
            .iter()
            .map(|t| {
                if t.literal {
                    String::new()
                } else {
                    t.value.to_ascii_lowercase()
                }
            })
            .collect::<Vec<_>>();
        if lower[0] == "end" {
            if let Some(kind) = lower.get(1)
                && scope.last().is_some_and(|(open, _)| open == kind)
            {
                scope.pop();
            }
            continue;
        }
        let mut begin = 0;
        while lower.get(begin).is_some_and(|s| {
            matches!(
                s.as_str(),
                "public"
                    | "private"
                    | "protected"
                    | "friend"
                    | "shared"
                    | "partial"
                    | "overloads"
                    | "overrides"
                    | "overridable"
                    | "mustoverride"
                    | "readonly"
                    | "writeonly"
                    | "async"
                    | "iterator"
                    | "default"
            )
        }) {
            begin += 1;
        }
        let kind = lower.get(begin).map(String::as_str).unwrap_or("");
        let mut declaration_name = None;
        if matches!(
            kind,
            "namespace"
                | "class"
                | "structure"
                | "interface"
                | "enum"
                | "module"
                | "sub"
                | "function"
                | "property"
                | "imports"
        ) && let Some(name) = tokens
            .get(begin + 1)
            .filter(|t| !t.literal && identifier(t.value))
        {
            declaration_name = Some(begin + 1);
            let mut last = begin + 1;
            if matches!(kind, "namespace" | "imports") {
                while tokens.get(last + 1).is_some_and(|t| t.value == ".")
                    && tokens
                        .get(last + 2)
                        .is_some_and(|t| !t.literal && identifier(t.value))
                {
                    last += 2;
                }
            }
            let written_name = &source[name.start..tokens[last].end];
            let fact_kind = match kind {
                "imports" => "import",
                "namespace" => "namespace",
                "property" => "property",
                _ => "written_definition",
            };
            push(
                &mut output,
                budget,
                tokens[begin].start,
                offset - line.len() + line.trim_end_matches(['\r', '\n']).len(),
                fact_kind,
                written_name,
                &scope,
            )?;
            if !matches!(kind, "property" | "imports") {
                ensure!(scope.len() < 128, "VB.NET block depth limit");
                scope.push((kind.into(), written_name.into()));
            }
        }
        for i in 0..tokens.len().saturating_sub(1) {
            if tokens[i + 1].value != "("
                || tokens[i].literal
                || !identifier(tokens[i].value)
                || declaration_name == Some(i)
                || matches!(
                    lower[i].as_str(),
                    "if" | "while"
                        | "for"
                        | "select"
                        | "ctype"
                        | "directcast"
                        | "gettype"
                        | "nameof"
                        | "function"
                        | "sub"
                )
            {
                continue;
            }
            let mut start = i;
            while start >= 2
                && tokens[start - 1].value == "."
                && identifier(tokens[start - 2].value)
            {
                start -= 2;
            }
            let target = &source[tokens[start].start..tokens[i].end];
            push(
                &mut output,
                budget,
                tokens[start].start,
                tokens[i + 1].end,
                "call",
                target,
                &scope,
            )?;
        }
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "VB.NET evidence byte limit"
    );
    Ok(output.finish(true))
}
fn push(
    output: &mut Facts<'_>,
    budget: &mut HostBudget,
    start: usize,
    end: usize,
    kind: &str,
    name: &str,
    scope: &[(String, String)],
) -> Result<()> {
    budget.charge(0, 1, 0, 0)?;
    let index = output.facts.len();
    output.push(start, end, kind, "vbnet_written_form", name)?;
    if let Some(fact) = output.facts.get_mut(index) {
        let code = fact.code.as_mut().unwrap();
        code.producer = "airk-business-structure/vbnet".into();
        code.producer_version = "specialist-facts/1".into();
        code.basis = EvidenceBasis::StructuralRule;
        code.scope = scope.iter().map(|(_, name)| name.clone()).collect();
        code.coverage="Selected written VB.NET line forms; continuation, attributes, XML literals and conditional compilation are partial; no binding or body continuity".into();
        if kind == "call" || kind == "import" {
            code.target = Some(name.chars().take(1024).collect());
        }
    }
    Ok(())
}
fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}
fn tokens<'a>(line: &'a str, offset: usize, budget: &mut HostBudget) -> Result<Vec<Token<'a>>> {
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < line.len() {
        let ch = line[pos..].chars().next().unwrap();
        if ch.is_whitespace() {
            pos += ch.len_utf8();
            continue;
        }
        if ch == '\'' {
            break;
        }
        budget.charge(1, 0, 0, 0)?;
        let start = pos;
        let literal = ch == '"';
        pos += ch.len_utf8();
        if literal {
            let mut closed = false;
            while pos < line.len() {
                let c = line[pos..].chars().next().unwrap();
                pos += c.len_utf8();
                if c == '"' {
                    if line[pos..].starts_with('"') {
                        pos += 1;
                    } else {
                        closed = true;
                        break;
                    }
                }
            }
            ensure!(closed, "unterminated VB.NET literal");
        } else if ch == '_' || ch.is_alphabetic() {
            while pos < line.len() {
                let c = line[pos..].chars().next().unwrap();
                if c == '_' || c.is_alphanumeric() {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
        }
        let value = &line[start..pos];
        if result.is_empty() && value.eq_ignore_ascii_case("rem") {
            break;
        }
        ensure!(result.len() < 100_000, "VB.NET line token limit");
        result.push(Token {
            value,
            start: offset + start,
            end: offset + pos,
            literal,
        });
    }
    Ok(result)
}
