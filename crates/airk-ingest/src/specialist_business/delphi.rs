// SPDX-License-Identifier: Apache-2.0
use super::*;
struct Token<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    string: bool,
}
/// The pinned Pascal grammar has no package form. Interpret this finite original
/// token sequence as structural evidence, never as a grammar declaration.
pub(super) fn package(
    path: &str,
    source: &str,
    budget: &mut HostBudget,
) -> Result<Option<Analysis>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < source.len() {
        let c = source[pos..].chars().next().unwrap();
        if c.is_whitespace() {
            pos += c.len_utf8();
            continue;
        }
        budget.charge(1, 0, 0, 0)?;
        if source[pos..].starts_with("//") {
            pos = source[pos..].find('\n').map_or(source.len(), |n| pos + n);
            continue;
        }
        if c == '{' {
            let Some(end) = source[pos + 1..].find('}') else {
                return Ok(None);
            };
            pos += end + 2;
            continue;
        }
        if source[pos..].starts_with("(*") {
            let Some(end) = source[pos + 2..].find("*)") else {
                return Ok(None);
            };
            pos += end + 4;
            continue;
        }
        let start = pos;
        let string = c == '\'';
        pos += c.len_utf8();
        if string {
            let mut closed = false;
            while pos < source.len() {
                let c = source[pos..].chars().next().unwrap();
                pos += c.len_utf8();
                if c == '\'' {
                    if source[pos..].starts_with('\'') {
                        pos += 1;
                    } else {
                        closed = true;
                        break;
                    }
                }
            }
            ensure!(closed, "unterminated Delphi package string");
        } else if c == '_' || c.is_alphabetic() {
            while pos < source.len() {
                let c = source[pos..].chars().next().unwrap();
                if c == '_' || c.is_alphanumeric() {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
        }
        let token = Token {
            text: &source[start..pos],
            start,
            end: pos,
            string,
        };
        if tokens.is_empty() && !token.text.eq_ignore_ascii_case("package") {
            return Ok(None);
        }
        ensure!(tokens.len() < 100_000, "Delphi package token limit");
        tokens.push(token);
    }
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut facts = Facts::new(path, source, "delphi")?;
    let Some(name) = tokens.get(1).filter(|t| !t.string && identifier(t.text)) else {
        return Ok(Some(facts.finish(true)));
    };
    if tokens.get(2).is_none_or(|t| t.text != ";") {
        return Ok(Some(facts.finish(true)));
    }
    emit(
        &mut facts,
        budget,
        tokens[0].start,
        tokens[2].end,
        "package",
        name.text,
    )?;
    let mut index = 3;
    while index < tokens.len() {
        let clause = tokens[index].text.to_ascii_lowercase();
        if clause == "end" {
            break;
        }
        if !matches!(clause.as_str(), "requires" | "contains") {
            break;
        }
        index += 1;
        loop {
            let Some(name) = tokens
                .get(index)
                .filter(|t| !t.string && identifier(t.text))
            else {
                index = tokens.len();
                break;
            };
            emit(
                &mut facts,
                budget,
                name.start,
                name.end,
                if clause == "requires" {
                    "dependency"
                } else {
                    "module"
                },
                name.text,
            )?;
            index += 1;
            if clause == "contains"
                && tokens
                    .get(index)
                    .is_some_and(|t| t.text.eq_ignore_ascii_case("in"))
            {
                index += 1;
                let Some(path) = tokens.get(index).filter(|t| t.string) else {
                    index = tokens.len();
                    break;
                };
                emit(
                    &mut facts, budget, path.start, path.end, "import", path.text,
                )?;
                index += 1;
            }
            match tokens.get(index).map(|t| t.text) {
                Some(",") => index += 1,
                Some(";") => {
                    index += 1;
                    break;
                }
                _ => {
                    index = tokens.len();
                    break;
                }
            }
        }
    }
    Ok(Some(facts.finish(true)))
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}
fn emit(
    facts: &mut Facts<'_>,
    budget: &mut HostBudget,
    start: usize,
    end: usize,
    kind: &str,
    name: &str,
) -> Result<()> {
    budget.charge(0, 1, 0, 0)?;
    let index = facts.facts.len();
    facts.push(start, end, kind, "delphi_package_form", name)?;
    if let Some(f) = facts.facts.get_mut(index) {
        let code = f.code.as_mut().unwrap();
        code.basis = EvidenceBasis::StructuralRule;
        code.producer = "airk-business-structure/delphi".into();
        code.producer_version = "specialist-facts/1".into();
        code.coverage="Written package/contains/requires only; conditional source and package loading are not evaluated".into();
    }
    Ok(())
}
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "program" | "unit" => named(node, "moduleName")
            .map(|n| cue(node, "definition", text(n, source), true))
            .into_iter()
            .collect(),
        "defProc" => node
            .child_by_field_name("header")
            .and_then(|n| field(n, source, "name"))
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "declProc" if !node.parent().is_some_and(|p| p.kind() == "defProc") => {
            field(node, source, "name")
                .map(|n| {
                    let mut c = cue(node, "signature", n, true);
                    c.signature = Some(ByteSpan {
                        start: node.start_byte(),
                        end: node.end_byte(),
                    });
                    c
                })
                .into_iter()
                .collect()
        }
        "declType" => field(node, source, "name")
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "declProp" | "declField" | "declVar" | "declConst" => field(node, source, "name")
            .map(|n| cue(node, "member", n, false))
            .into_iter()
            .collect(),
        "declUses" => vec![cue(node, "import", text(node, source), false)],
        "exprCall" => field(node, source, "entity")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
