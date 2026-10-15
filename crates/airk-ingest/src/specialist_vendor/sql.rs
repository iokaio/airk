// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::syntax_repairs::sql::{Token, TokenKind, tokenize_vendor};
use tree_sitter::Parser;

fn name(tokens: &[Token<'_>], at: usize) -> Option<(usize, usize)> {
    let mut end = at;
    for part in 0..4 {
        let token = tokens.get(end)?;
        if !matches!(token.kind, TokenKind::Word | TokenKind::Quoted) {
            return None;
        }
        end += 1;
        if tokens.get(end).is_none_or(|t| t.raw != ".") {
            return Some((at, end));
        }
        if part == 3 {
            return None;
        }
        end += 1;
    }
    None
}

pub(super) fn analyze(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    let mut output = Facts::new(path, source, adapter)?;
    let Some(tokens) = tokenize_vendor(source, adapter)? else {
        return finish(output, true, adapter);
    };
    budget.charge(tokens.len(), 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_sequel::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("SQL parser returned no tree"))?;
    let literals: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Literal)
        .map(|t| t.start..t.end)
        .collect();
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let i = literals.partition_point(|r| r.end <= node.start_byte());
        if literals
            .get(i)
            .is_some_and(|r| r.start <= node.start_byte() && node.start_byte() < r.end)
        {
            continue;
        }
        if !node.has_error() && !node.is_missing() {
            let before = output.facts.len();
            crate::syntax_repairs::sql::visit(node, source, &mut output)?;
            budget.charge(0, output.facts.len() - before, 0, 0)?;
            for fact in &mut output.facts[before..] {
                let code = fact.code.as_mut().unwrap();
                code.basis = EvidenceBasis::Syntax;
                code.producer = format!("airk-tree-sitter/{adapter}");
                code.producer_version = "vendor-facts/1".into();
                code.coverage = "Selected generic SQL grammar nodes within explicitly selected vendor dialect; procedural semantics and database references unresolved; native continuity unavailable".into();
            }
        }
        if matches!(node.kind(), "comment" | "string" | "string_literal") {
            continue;
        }
        for i in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "vendor SQL pending limit");
            if let Some(child) = node.named_child(i) {
                pending.push(child);
            }
        }
    }
    let mut partial = tree.root_node().has_error();
    for (at, token) in tokens.iter().enumerate() {
        budget.charge(1, 0, 0, 0)?;
        if adapter == "tsql" && token.is("go") {
            let line_start = source[..token.start].rfind('\n').map_or(0, |p| p + 1);
            let line_end = source[token.end..]
                .find('\n')
                .map_or(source.len(), |p| token.end + p);
            if source[line_start..token.start].trim().is_empty()
                && source[token.end..line_end].trim().is_empty()
            {
                budget.charge(0, 1, 0, 0)?;
                output.push(
                    token.start,
                    token.end,
                    "batch",
                    "tsql_batch_separator",
                    token.raw,
                )?;
            }
        }
        if (token.is("from") || token.is("join") || token.is("into") || token.is("update"))
            && let Some((a, b)) = name(&tokens, at + 1)
        {
            budget.charge(0, 1, 0, 0)?;
            output.push(
                tokens[a].start,
                tokens[b - 1].end,
                "table_reference",
                "vendor_written_sql_reference",
                &source[tokens[a].start..tokens[b - 1].end],
            )?;
        }
        let mut head = at;
        let create = token.is("create");
        if create {
            head += 1;
            if tokens.get(head).is_some_and(|t| t.is("or")) {
                head += 1;
                if !tokens
                    .get(head)
                    .is_some_and(|t| t.is("replace") || (adapter == "tsql" && t.is("alter")))
                {
                    continue;
                }
                head += 1;
            }
        } else if adapter != "plsql"
            || !(token.is("procedure") || token.is("function") || token.is("cursor"))
            || at
                .checked_sub(1)
                .and_then(|i| tokens.get(i))
                .is_some_and(|t| t.is("create") || t.is("replace"))
        {
            continue;
        }
        let Some(keyword) = tokens.get(head) else {
            partial = true;
            continue;
        };
        let kind = if keyword.is("package") {
            "package"
        } else if keyword.is("procedure") || keyword.is("proc") {
            "procedure"
        } else if keyword.is("function") {
            "function"
        } else if keyword.is("trigger") {
            "trigger"
        } else if keyword.is("cursor") {
            "cursor"
        } else if keyword.is("table") {
            "table"
        } else if keyword.is("view") {
            "view"
        } else {
            continue;
        };
        head += 1;
        let body = kind == "package" && tokens.get(head).is_some_and(|t| t.is("body"));
        if body {
            head += 1;
        }
        let Some((a, b)) = name(&tokens, head) else {
            partial = true;
            continue;
        };
        // Emit only a complete bounded header, never pretend a body is parsed.
        let mut end = b;
        let mut depth = 0usize;
        let mut terminated = false;
        while let Some(next) = tokens.get(end) {
            budget.charge(1, 0, 0, 0)?;
            if end - b >= 4096 {
                break;
            }
            if next.raw == "(" {
                depth += 1;
            }
            if next.raw == ")" {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            if depth > 64 {
                break;
            }
            if depth == 0 && (next.raw == ";" || next.is("as") || next.is("is") || next.is("begin"))
            {
                end += 1;
                terminated = true;
                break;
            }
            if depth == 0 && (next.is("create") || next.is("go")) {
                break;
            }
            end += 1;
        }
        if !terminated {
            partial = true;
            continue;
        }
        budget.charge(0, 1, 0, 0)?;
        output.push(
            token.start,
            tokens[end - 1].end,
            kind,
            if body {
                "vendor_package_body_header"
            } else {
                "vendor_declaration_header"
            },
            &source[tokens[a].start..tokens[b - 1].end],
        )?;
    }
    // A generic grammar may also emit an identical reference; preserve provenance
    // distinction but collapse literal duplicates from traversal/overlay.
    output.facts.sort_by_key(|f| {
        (
            f.start_line,
            f.code.as_ref().unwrap().span.as_ref().unwrap().start_byte,
            f.kind.clone(),
            f.name.clone(),
        )
    });
    output.facts.dedup_by(|a, b| {
        a.kind == b.kind
            && a.name == b.name
            && a.code
                .as_ref()
                .unwrap()
                .span
                .as_ref()
                .map(|s| (s.start_byte, s.end_byte))
                == b.code
                    .as_ref()
                    .unwrap()
                    .span
                    .as_ref()
                    .map(|s| (s.start_byte, s.end_byte))
    });
    finish(output, partial, adapter)
}
