// SPDX-License-Identifier: Apache-2.0
//! Free-source written COBOL forms; the admitted native scanner is fixed-column only.
use super::*;
struct Token<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    literal: bool,
    line_first: bool,
}
pub(in crate::specialist_business) fn analyze(
    path: &str,
    source: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    let tokens = tokenize(source, budget)?;
    let mut facts = Facts::new(path, source, "cobol-free")?;
    let mut data = tokens.first().is_some_and(|token| {
        token.text.len() == 2 && token.text.bytes().all(|b| b.is_ascii_digit())
    });
    let mut procedure = false;
    for (i, token) in tokens.iter().enumerate() {
        budget.charge(1, 0, 0, 0)?;
        if token.literal {
            continue;
        }
        let next = tokens.get(i + 1);
        if token.text.eq_ignore_ascii_case("PROGRAM-ID") && next.is_some_and(|n| n.text == ".") {
            if let Some(name) = tokens.get(i + 2).filter(|n| word(n.text) && !n.literal) {
                emit(
                    &mut facts,
                    budget,
                    token.start,
                    name.end,
                    "program",
                    name.text,
                )?;
            }
        } else if next.is_some_and(|n| n.text.eq_ignore_ascii_case("DIVISION")) && word(token.text)
        {
            data = token.text.eq_ignore_ascii_case("DATA");
            procedure = token.text.eq_ignore_ascii_case("PROCEDURE");
            emit(
                &mut facts,
                budget,
                token.start,
                next.unwrap().end,
                "division",
                token.text,
            )?;
        } else if next.is_some_and(|n| n.text.eq_ignore_ascii_case("SECTION")) && word(token.text) {
            emit(
                &mut facts,
                budget,
                token.start,
                next.unwrap().end,
                "section",
                token.text,
            )?;
        } else if token.text.eq_ignore_ascii_case("COPY") || token.text.eq_ignore_ascii_case("CALL")
        {
            if let Some(target) = next.filter(|n| n.literal || word(n.text)) {
                emit(
                    &mut facts,
                    budget,
                    token.start,
                    target.end,
                    if token.text.eq_ignore_ascii_case("COPY") {
                        "import"
                    } else {
                        "call"
                    },
                    target.text,
                )?;
            }
        } else if data
            && token.line_first
            && token.text.len() == 2
            && token.text.bytes().all(|b| b.is_ascii_digit())
        {
            if let Some(name) = next.filter(|n| word(n.text) && !n.literal) {
                emit(
                    &mut facts,
                    budget,
                    token.start,
                    name.end,
                    "declaration",
                    name.text,
                )?;
            }
        } else if procedure
            && token.line_first
            && word(token.text)
            && !matches!(
                token.text.to_ascii_uppercase().as_str(),
                "CONTINUE" | "GOBACK" | "EXIT" | "STOP"
            )
            && next.is_some_and(|n| n.text == ".")
        {
            emit(
                &mut facts,
                budget,
                token.start,
                next.unwrap().end,
                "paragraph",
                token.text,
            )?;
        }
    }
    ensure!(
        serde_json::to_vec(&facts.facts)?.len() <= 16_777_216,
        "free COBOL evidence byte limit"
    );
    Ok(facts.finish(true))
}
fn word(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_alphabetic())
        && chars.all(|c| c.is_alphanumeric() || c == '-' || c == '_')
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
    facts.push(start, end, kind, "cobol_free_written_form", name)?;
    if let Some(f) = facts.facts.get_mut(index) {
        let c = f.code.as_mut().unwrap();
        c.basis = EvidenceBasis::StructuralRule;
        c.producer = "airk-business-structure/cobol-free".into();
        c.producer_version = "specialist-facts/1".into();
        c.coverage="Selected free-source written forms; dialect clauses, continuations, COPY expansion and compiler directives are not evaluated".into();
        if kind == "call" || kind == "import" {
            c.target = Some(name.chars().take(1024).collect());
        }
    }
    Ok(())
}
fn tokenize<'a>(source: &'a str, budget: &mut HostBudget) -> Result<Vec<Token<'a>>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut line_first = true;
    while pos < source.len() {
        let ch = source[pos..].chars().next().unwrap();
        if ch.is_whitespace() {
            if ch == '\n' {
                line_first = true;
            }
            pos += ch.len_utf8();
            continue;
        }
        budget.charge(1, 0, 0, 0)?;
        if source[pos..].starts_with("*>") {
            pos = source[pos..].find('\n').map_or(source.len(), |n| pos + n);
            continue;
        }
        if source[pos..].starts_with(">>") {
            pos = source[pos..].find('\n').map_or(source.len(), |n| pos + n);
            continue;
        }
        let start = pos;
        let literal = matches!(ch, '\'' | '"');
        pos += ch.len_utf8();
        if literal {
            let mut closed = false;
            while pos < source.len() {
                let c = source[pos..].chars().next().unwrap();
                ensure!(
                    c != '\n' && c != '\r',
                    "unsupported free COBOL literal continuation"
                );
                pos += c.len_utf8();
                if c == ch {
                    if source[pos..].starts_with(ch) {
                        pos += ch.len_utf8();
                    } else {
                        closed = true;
                        break;
                    }
                }
            }
            ensure!(closed, "unterminated free COBOL literal");
        } else if ch.is_alphanumeric() || ch == '_' {
            while pos < source.len() {
                let c = source[pos..].chars().next().unwrap();
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
        }
        ensure!(tokens.len() < 100_000, "free COBOL token limit");
        tokens.push(Token {
            text: &source[start..pos],
            start,
            end: pos,
            literal,
            line_first,
        });
        line_first = false;
    }
    Ok(tokens)
}
