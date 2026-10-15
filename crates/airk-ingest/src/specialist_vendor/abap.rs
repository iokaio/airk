// SPDX-License-Identifier: Apache-2.0
use super::*;

/// Period-delimited original statements; templates and chains stay unsupported.
pub(super) fn analyze(path: &str, source: &str, budget: &mut HostBudget) -> Result<Analysis> {
    let mut output = Facts::new(path, source, "abap")?;
    let mut lines = vec![0];
    lines.extend(source.match_indices('\n').map(|(i, _)| i + 1));
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut words: Vec<(usize, usize)> = vec![];
    let mut start = None;
    let mut partial = false;
    let mut unsupported = false;
    let mut line_start = true;
    let mut owners: Vec<(String, usize)> = vec![];
    while at < bytes.len() {
        budget.charge(1, 0, 0, 0)?;
        if bytes[at] == b'\n' {
            line_start = true;
            at += 1;
            continue;
        }
        if bytes[at] == b'"' || (line_start && bytes[at] == b'*') {
            at = source[at..].find('\n').map_or(bytes.len(), |n| at + n);
            continue;
        }
        line_start = false;
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        start.get_or_insert(at);
        if matches!(bytes[at], b'\'' | b'`' | b'|') {
            let quote = bytes[at];
            unsupported |= quote == b'|';
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                budget.charge(1, 0, 0, 0)?;
                if bytes[at] == b'\\' && quote == b'|' {
                    at = (at + 2).min(bytes.len());
                    continue;
                }
                if bytes[at] == quote {
                    at += 1;
                    if at < bytes.len() && bytes[at] == quote {
                        at += 1;
                        continue;
                    }
                    closed = true;
                    break;
                }
                at += 1;
            }
            if !closed {
                partial = true;
                break;
            }
            continue;
        }
        if bytes[at] == b'.' {
            if !unsupported {
                statement(
                    source,
                    start.unwrap_or(at)..at + 1,
                    &words,
                    &mut owners,
                    &mut output,
                    budget,
                    &lines,
                )?;
            } else {
                partial = true;
            }
            words.clear();
            start = None;
            unsupported = false;
            at += 1;
            continue;
        }
        if bytes[at] == b':' || bytes[at] == b',' {
            unsupported = true;
        }
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' || bytes[at] >= 128 {
            let begin = at;
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric()
                    || matches!(bytes[at], b'_' | b'~' | b'-' | b'/')
                    || bytes[at] >= 128)
            {
                at += 1;
            }
            ensure!(words.len() < 4096, "ABAP statement token limit");
            words.push((begin, at));
        } else {
            at += 1;
        }
    }
    partial |= start.is_some() || !owners.is_empty();
    // Closing statements establish exact containing spans. Rebind all written
    // identities only after those spans are final, never to header-only owners.
    let identities: std::collections::BTreeMap<_, _> = output
        .facts
        .iter()
        .map(|fact| {
            let code = fact.code.as_ref().unwrap();
            let span = code.span.as_ref().unwrap();
            (
                code.symbol_id.clone(),
                (
                    format!(
                        "syntax:{path}:{}:{}:{}",
                        span.start_byte, span.end_byte, fact.syntax_kind
                    ),
                    span.start_byte,
                    span.end_byte,
                ),
            )
        })
        .collect();
    for fact in &mut output.facts {
        let code = fact.code.as_mut().unwrap();
        code.symbol_id = identities[&code.symbol_id].0.clone();
        if let Some(owner) = code.owner_id.take() {
            let (id, start, end) = &identities[&owner];
            let span = code.span.as_ref().unwrap();
            if *start <= span.start_byte && *end >= span.end_byte {
                code.owner_id = Some(id.clone());
            } else {
                partial = true;
            }
        }
    }
    finish(output, partial, "abap")
}

fn statement(
    source: &str,
    span: std::ops::Range<usize>,
    words: &[(usize, usize)],
    owners: &mut Vec<(String, usize)>,
    output: &mut Facts<'_>,
    budget: &mut HostBudget,
    lines: &[usize],
) -> Result<()> {
    let Some(&(first, last)) = words.first() else {
        return Ok(());
    };
    let keyword = source[first..last].to_ascii_lowercase();
    if let Some(closing) = keyword.strip_prefix("end") {
        if owners.last().is_some_and(|(kind, _)| kind == closing) {
            let (_, index) = owners.pop().unwrap();
            let fact = &mut output.facts[index];
            let code = fact.code.as_mut().unwrap();
            let original = code.span.as_ref().unwrap();
            code.signature_span = Some(airk_core::CodeSpan {
                start_byte: original.start_byte,
                end_byte: original.end_byte,
            });
            code.span.as_mut().unwrap().end_byte = span.end;
            fact.end_line = lines
                .partition_point(|offset| *offset < span.end)
                .max(fact.start_line);
            fact.location = airk_core::DocumentLocation::Lines {
                start: fact.start_line,
                end: fact.end_line,
            };
        }
        return Ok(());
    }
    let kind = match keyword.as_str() {
        "report" | "program" => "report",
        "class" => "class",
        "interface" => "interface",
        "method" | "methods" | "class-methods" => "method",
        "form" => "form",
        "module" => "module",
        "include" => "include",
        "interfaces" | "perform" => "reference",
        _ => return Ok(()),
    };
    let Some(&(a, b)) = words.get(1) else {
        return Ok(());
    };
    let name = &source[a..b];
    budget.charge(0, 1, 0, 0)?;
    let index = output.facts.len();
    output.push(
        span.start,
        span.end,
        kind,
        &format!("abap_{keyword}_statement"),
        name,
    )?;
    let scope: Vec<_> = owners
        .iter()
        .map(|(_, i)| output.facts[*i].name.clone())
        .collect();
    let owner_id = owners
        .last()
        .map(|(_, i)| output.facts[*i].code.as_ref().unwrap().symbol_id.clone());
    let code = output.facts[index].code.as_mut().unwrap();
    code.scope = scope;
    code.owner_id = owner_id;
    if matches!(kind, "include" | "reference") {
        code.target = Some(name.chars().take(1024).collect());
    }
    if matches!(
        keyword.as_str(),
        "class" | "interface" | "method" | "form" | "module"
    ) {
        let deferred = words
            .iter()
            .any(|&(a, b)| source[a..b].eq_ignore_ascii_case("deferred"));
        if !deferred {
            ensure!(owners.len() < 128, "ABAP owner depth limit");
            owners.push((keyword, index));
        }
    }
    Ok(())
}
