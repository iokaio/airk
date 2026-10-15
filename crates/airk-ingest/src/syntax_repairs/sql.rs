// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit(node: Node<'_>, source: &str, output: &mut Facts<'_>) -> Result<()> {
    match node.kind() {
        "statement" | "subquery" if child(node, "select").is_some() => {
            emit(output, node, "query_scope", text(node, source))?
        }
        "select" => emit(output, node, "query", text(node, source))?,
        "cte" => {
            if let Some(name) = node
                .child_by_field_name("name")
                .or_else(|| child(node, "identifier"))
            {
                emit(output, node, "cte", text(name, source))?;
            }
        }
        "join" | "lateral_join" | "lateral_cross_join" => {
            emit(output, node, "join", text(node, source))?
        }
        "relation" => {
            if let Some(reference) = child(node, "object_reference") {
                emit(
                    output,
                    reference,
                    "table_reference",
                    text(reference, source),
                )?;
            }
        }
        "create_function"
        | "create_trigger"
        | "create_index"
        | "create_table"
        | "create_view"
        | "create_materialized_view" => {
            if let Some(name) =
                child(node, "object_reference").or_else(|| child(node, "identifier"))
            {
                emit(output, node, "definition", text(name, source))?;
            }
        }
        _ => {}
    }
    if matches!(
        node.kind(),
        "relation" | "term" | "statement" | "lateral_join" | "lateral_cross_join"
    ) && let Some(alias) = node.child_by_field_name("alias")
    {
        emit(output, alias, "alias", text(alias, source))?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum TokenKind {
    Word,
    Quoted,
    Literal,
    Symbol,
}
#[derive(Clone, Copy)]
struct Token<'a> {
    raw: &'a str,
    start: usize,
    end: usize,
    kind: TokenKind,
}
impl Token<'_> {
    fn is(self, value: &str) -> bool {
        self.kind == TokenKind::Word && self.raw.eq_ignore_ascii_case(value)
    }
}

/// A finite header/body recognizer, independent of recovered/error grammar nodes.
/// Only explicit SQL profiles admit it. Unsupported syntax yields no procedure fact.
pub(super) fn procedures(path: &str, source: &str, dialect: &str) -> Result<Vec<Fact>> {
    let Some(tokens) = tokenize(source, dialect == "sql-postgresql")? else {
        return Ok(vec![]);
    };
    // Client-side delimiter switching is deliberately not interpreted as server SQL.
    if tokens.iter().any(|token| token.is("delimiter")) {
        return Ok(vec![]);
    }
    let mut facts = Facts::new(path, source, dialect)?;
    let mut start = 0;
    let mut block_depth = 0usize;
    let mut paren_depth = 0usize;
    while start < tokens.len() {
        if block_depth == 0
            && paren_depth == 0
            && tokens[start].is("create")
            && (start == 0 || tokens[start - 1].raw == ";")
            && let Some((end, name_start, name_end, header_end)) =
                procedure(&tokens, start, dialect)
        {
            facts.push(
                tokens[start].start,
                tokens[end].end,
                "procedure",
                "procedure_statement",
                &source[name_start..name_end],
            )?;
            let fact = facts.facts.last_mut().expect("procedure fact");
            let code = fact.code.as_mut().expect("procedure evidence");
            code.producer = format!("airk-sql-structure/{dialect}");
            code.producer_version = "repair-facts/1".into();
            code.signature_span = Some(airk_core::CodeSpan {
                start_byte: tokens[start].start,
                end_byte: header_end,
            });
            code.coverage = "Bounded written CREATE PROCEDURE header and selected literal body framing; procedural SQL semantics, dynamic SQL and references are not evaluated".into();
            start = end + 1;
            continue;
        }
        if tokens[start].is("begin") {
            block_depth = block_depth.saturating_add(1);
        }
        if tokens[start].is("end") {
            block_depth = block_depth.saturating_sub(1);
        }
        if tokens[start].raw == "(" {
            paren_depth = paren_depth.saturating_add(1);
        }
        if tokens[start].raw == ")" {
            paren_depth = paren_depth.saturating_sub(1);
        }
        start += 1;
    }
    Ok(facts.facts)
}

fn procedure(
    tokens: &[Token<'_>],
    start: usize,
    dialect: &str,
) -> Option<(usize, usize, usize, usize)> {
    let mut at = start + 1;
    if tokens.get(at)?.is("or") {
        at += 1;
        if !tokens.get(at)?.is("replace") {
            return None;
        }
        at += 1;
    }
    if !tokens.get(at)?.is("procedure") {
        return None;
    }
    at += 1;
    let name_start = tokens.get(at)?.start;
    let mut parts = 0;
    loop {
        let token = tokens.get(at)?;
        if !matches!(token.kind, TokenKind::Word | TokenKind::Quoted) {
            return None;
        }
        if token.kind == TokenKind::Quoted
            && !token.raw.starts_with(if dialect == "sql-postgresql" {
                '"'
            } else {
                '`'
            })
        {
            return None;
        }
        parts += 1;
        if parts > 3 {
            return None;
        }
        at += 1;
        if tokens.get(at)?.raw != "." {
            break;
        }
        at += 1;
    }
    let name_end = tokens[at - 1].end;
    if tokens.get(at)?.raw != "(" {
        return None;
    }
    let mut depth = 1usize;
    at += 1;
    let parameter_start = at;
    while depth > 0 {
        let token = tokens.get(at)?;
        if at - parameter_start > 4096 || token.raw == ";" {
            return None;
        }
        if token.kind == TokenKind::Symbol
            && !matches!(
                token.raw,
                "(" | ")" | "," | "." | "[" | "]" | "=" | ":" | "-" | "+"
            )
            && !token.raw.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        match token.raw {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        if depth > 64 {
            return None;
        }
        at += 1;
    }
    let header_end = tokens[at - 1].end;
    if dialect == "sql-postgresql" {
        let mut language = false;
        let mut body = false;
        for _ in 0..2 {
            if tokens.get(at)?.is("language") && !language {
                at += 1;
                let token = tokens.get(at)?;
                if !(token.is("sql") || token.is("plpgsql")) {
                    return None;
                }
                language = true;
                at += 1;
            } else if tokens.get(at)?.is("as") && !body {
                at += 1;
                if tokens.get(at)?.kind != TokenKind::Literal {
                    return None;
                }
                body = true;
                at += 1;
            } else {
                return None;
            }
        }
        if language && body && tokens.get(at)?.raw == ";" {
            return Some((at, name_start, name_end, header_end));
        }
    } else {
        if !tokens.get(at)?.is("begin") {
            return None;
        }
        let mut depth = 1usize;
        at += 1;
        let body_start = at;
        while depth > 0 {
            let token = tokens.get(at)?;
            if at - body_start > 100_000
                || ["if", "loop", "while", "repeat", "case", "delimiter"]
                    .iter()
                    .any(|word| token.is(word))
            {
                return None;
            }
            if token.is("begin") {
                depth += 1;
            }
            if token.is("end") {
                depth -= 1;
            }
            if depth > 64 {
                return None;
            }
            at += 1;
        }
        if tokens.get(at)?.raw == ";" {
            return Some((at, name_start, name_end, header_end));
        }
    }
    None
}

fn tokenize(source: &str, postgres: bool) -> Result<Option<Vec<Token<'_>>>> {
    let bytes = source.as_bytes();
    let mut at = 0usize;
    let mut tokens = Vec::new();
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"--") || (!postgres && bytes[at] == b'#') {
            at = source[at..]
                .find('\n')
                .map_or(bytes.len(), |end| at + end + 1);
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1usize;
            while at < bytes.len() && depth > 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
                if depth > 32 {
                    return Ok(None);
                }
            }
            if depth != 0 {
                return Ok(None);
            }
            continue;
        }
        let start = at;
        let kind;
        if matches!(bytes[at], b'\'' | b'"' | b'`') {
            let quote = bytes[at];
            kind = if quote == b'\'' {
                TokenKind::Literal
            } else {
                TokenKind::Quoted
            };
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    // SQL modes change backslash meaning: decline the whole source.
                    return Ok(None);
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
                return Ok(None);
            }
        } else if postgres && bytes[at] == b'$' {
            let Some(length) = source[at + 1..].find('$').filter(|length| *length <= 128) else {
                return Ok(None);
            };
            let tag = &source[at + 1..at + 1 + length];
            if (!tag.is_empty()
                && !tag.as_bytes()[0].is_ascii_alphabetic()
                && tag.as_bytes()[0] != b'_')
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Ok(None);
            }
            let delimiter = &source[at..at + length + 2];
            at += delimiter.len();
            let Some(end) = source[at..].find(delimiter) else {
                return Ok(None);
            };
            at += end + delimiter.len();
            kind = TokenKind::Literal;
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' || bytes[at] >= 128 {
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric()
                    || matches!(bytes[at], b'_' | b'$')
                    || bytes[at] >= 128)
            {
                at += 1;
            }
            kind = TokenKind::Word;
        } else {
            at += 1;
            kind = TokenKind::Symbol;
        }
        ensure!(tokens.len() < 500_000, "SQL procedure token limit exceeded");
        tokens.push(Token {
            raw: &source[start..at],
            start,
            end: at,
            kind,
        });
    }
    Ok(Some(tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_dollar_body_and_quoted_name_keep_original_bytes() -> Result<()> {
        let source = "/* outer /* nested */ note */\r\nCREATE OR REPLACE PROCEDURE archive.\"café\"(IN count integer)\r\nLANGUAGE plpgsql AS $body$ BEGIN RAISE NOTICE 'CREATE PROCEDURE fake()'; END $body$;\r\n";
        let facts = procedures("p.sql", source, "sql-postgresql")?;
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, "archive.\"café\"");
        let code = facts[0].code.as_ref().unwrap();
        let span = code.span.as_ref().unwrap();
        assert_eq!(
            &source[span.start_byte..span.end_byte],
            source
                .trim_start_matches("/* outer /* nested */ note */\r\n")
                .trim_end()
        );
        assert_eq!(facts[0].start_line, 2);
        assert!(code.signature_span.is_some());
        Ok(())
    }

    #[test]
    fn mysql_nested_begin_end_ignores_keywords_in_strings_and_comments() -> Result<()> {
        let source = "CREATE PROCEDURE `archive_count`(IN n INT) BEGIN SELECT 'END; CREATE PROCEDURE fake()'; /* END */ BEGIN SELECT n; END; END;";
        let facts = procedures("p.sql", source, "sql-mysql")?;
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, "`archive_count`");
        assert_eq!(
            facts[0]
                .code
                .as_ref()
                .unwrap()
                .span
                .as_ref()
                .unwrap()
                .end_byte,
            source.len()
        );
        Ok(())
    }

    #[test]
    fn procedures_are_not_inferred_from_literals_comments_calls_or_incomplete_headers() -> Result<()>
    {
        for (dialect, source) in [
            (
                "sql-postgresql",
                "-- CREATE PROCEDURE fake() LANGUAGE SQL AS $$ SELECT 1 $$;\nSELECT 'CREATE PROCEDURE fake()';",
            ),
            ("sql-postgresql", "CALL archive.rotate();"),
            (
                "sql-postgresql",
                "CREATE PROCEDURE broken(@@@) LANGUAGE SQL AS $$ SELECT 1 $$;",
            ),
            (
                "sql-postgresql",
                "CREATE PROCEDURE broken() LANGUAGE SQL AS $body$ unterminated;",
            ),
            (
                "sql-postgresql",
                "CREATE PROCEDURE broken() LANGUAGE SQL AS $1$ SELECT 1 $1$;",
            ),
            ("sql-mysql", "CREATE PROCEDURE broken() BEGIN SELECT 1;"),
            (
                "sql-mysql",
                "DELIMITER $$\nCREATE PROCEDURE unsupported() BEGIN SELECT 1; END$$",
            ),
            (
                "sql-mysql",
                "CREATE PROCEDURE unsupported() BEGIN IF n THEN SELECT 1; END IF; END;",
            ),
            (
                "sql-mysql",
                "CREATE PROCEDURE broken() BEGIN ; CREATE PROCEDURE nested() BEGIN SELECT 1; END;",
            ),
        ] {
            assert!(
                procedures("p.sql", source, dialect)?.is_empty(),
                "unexpected declaration: {source}"
            );
        }
        Ok(())
    }
}
