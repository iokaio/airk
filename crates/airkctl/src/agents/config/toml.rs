// SPDX-License-Identifier: Apache-2.0
//! Port of agentToml.ts: a non-executing, bounded scan of logical TOML
//! statements (so a multiline string cannot manufacture a table header), the
//! value decoder for MCP declarations, and the span edits AIRK makes to
//! `[mcp_servers.<key>]` tables. Edits replace whole tables or single-line
//! values; every other byte is kept.
use serde_json::{Map, Number, Value};

const MAX_DECLARATIONS: usize = 512;
/// Subtables AIRK accepts under its own server table (unifiedAgentConfig.ts).
const NESTED: [&str; 3] = ["env", "http_headers", "env_http_headers"];

#[derive(Debug)]
pub(super) struct Statement {
    /// Statement text without comments, trimmed.
    pub(super) text: String,
    /// The statement's region: after the previous statement up to and
    /// including its terminating line break.
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) comments: Vec<String>,
    /// First and one-past-last non-blank source byte of the text, when any.
    raw: Option<(usize, usize)>,
}

#[derive(Default)]
struct Scan {
    out: Vec<Statement>,
    text: String,
    comments: Vec<String>,
    start: usize,
    raw: Option<(usize, usize)>,
}

impl Scan {
    fn push(&mut self, text: &str, at: usize) {
        self.text.push_str(text);
        for (offset, c) in text.char_indices() {
            if !c.is_whitespace() {
                let begin = self.raw.map_or(at + offset, |raw| raw.0);
                self.raw = Some((begin, at + offset + c.len_utf8()));
            }
        }
    }

    fn finish(&mut self, end: usize) {
        if !self.text.trim().is_empty() || !self.comments.is_empty() {
            self.out.push(Statement {
                text: self.text.trim().to_string(),
                start: self.start,
                end,
                comments: std::mem::take(&mut self.comments),
                raw: self.raw,
            });
        }
        self.text.clear();
        self.comments.clear();
        self.raw = None;
        self.start = end;
    }
}

fn fail<T>(code: &str) -> Result<T, String> {
    Err(format!("unsupported TOML configuration: {code}"))
}

pub(super) fn scan(source: &str) -> Result<Vec<Statement>, String> {
    let mut scan = Scan::default();
    let (mut quote, mut triple, mut escaped, mut comment) = (None::<char>, false, false, false);
    let mut comment_text = String::new();
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(c) = source[at..].chars().next() {
        let next = at + c.len_utf8();
        if comment {
            if c == '\n' {
                comment = false;
                scan.comments.push(comment_text.trim_end().to_string());
                comment_text.clear();
                if depth == 0 {
                    scan.finish(next);
                } else {
                    scan.push("\n", at);
                }
            } else {
                comment_text.push(c);
            }
            at = next;
            continue;
        }
        if let Some(q) = quote {
            scan.push(&source[at..next], at);
            if escaped {
                escaped = false;
            } else if q == '"' && c == '\\' {
                escaped = true;
            } else if c == q {
                if !triple {
                    quote = None;
                } else if source[at..].starts_with(&q.to_string().repeat(3)) {
                    let mut end = at + 3;
                    while source.as_bytes().get(end) == Some(&(q as u8)) {
                        end += 1;
                    }
                    if end - at > 5 {
                        return fail("invalid_toml_string");
                    }
                    scan.push(&source[at + 1..end], at + 1);
                    quote = None;
                    triple = false;
                    at = end;
                    continue;
                }
            }
            at = next;
            continue;
        }
        match c {
            '#' => {
                comment = true;
                comment_text = "#".into();
            }
            '"' | '\'' => {
                quote = Some(c);
                triple = source[at..].starts_with(&c.to_string().repeat(3));
                let width = if triple { 3 } else { 1 };
                scan.push(&source[at..at + width], at);
                at += width;
                continue;
            }
            '\n' if depth == 0 => scan.finish(next),
            _ => {
                if c == '[' || c == '{' {
                    depth += 1;
                } else if c == ']' || c == '}' {
                    depth = match depth.checked_sub(1) {
                        Some(depth) => depth,
                        None => return fail("invalid_toml_structure"),
                    };
                }
                scan.push(&source[at..next], at);
            }
        }
        at = next;
    }
    if quote.is_some() || depth != 0 {
        return fail("unterminated_toml_structure");
    }
    if comment {
        scan.comments.push(comment_text.trim_end().to_string());
    }
    scan.finish(source.len());
    Ok(scan.out)
}

/// `TomlValue`: a bounded decoder over one statement's text.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Cursor { text, at: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, expected: char) -> Result<(), String> {
        if self.peek() != Some(expected) {
            return fail("invalid_toml_assignment");
        }
        self.at += expected.len_utf8();
        Ok(())
    }

    fn string(&mut self) -> Result<String, String> {
        let Some(quote) = self.peek() else {
            return fail("invalid_toml_string");
        };
        let closing = quote.to_string().repeat(3);
        let triple = self.rest().starts_with(&closing);
        let closing = if triple { closing } else { quote.to_string() };
        self.at += closing.len();
        if triple {
            if self.rest().starts_with("\r\n") {
                self.at += 2;
            } else if self.rest().starts_with('\n') {
                self.at += 1;
            }
        }
        let mut value = String::new();
        loop {
            if self.rest().starts_with(&closing) {
                if triple {
                    let bytes = self.text.as_bytes();
                    let mut end = self.at + 3;
                    while bytes.get(end) == Some(&(quote as u8)) {
                        end += 1;
                    }
                    if end - self.at > 5 {
                        return fail("invalid_toml_string");
                    }
                    value.push_str(&quote.to_string().repeat(end - self.at - 3));
                    self.at = end;
                } else {
                    self.at += 1;
                }
                return Ok(value);
            }
            let Some(c) = self.peek() else {
                return fail("unterminated_toml_string");
            };
            self.at += c.len_utf8();
            if c != '\\' || quote == '\'' {
                if !triple && (c == '\n' || c == '\r') {
                    return fail("invalid_toml_string");
                }
                value.push(c);
                continue;
            }
            let Some(escape) = self.peek() else {
                return fail("unterminated_toml_string");
            };
            self.at += escape.len_utf8();
            if triple && escape.is_whitespace() {
                self.space();
                continue;
            }
            let simple = match escape {
                'b' => Some('\u{8}'),
                't' => Some('\t'),
                'n' => Some('\n'),
                'f' => Some('\u{c}'),
                'r' => Some('\r'),
                '"' => Some('"'),
                '\\' => Some('\\'),
                _ => None,
            };
            if let Some(simple) = simple {
                value.push(simple);
                continue;
            }
            if escape == 'u' || escape == 'U' {
                let width = if escape == 'u' { 4 } else { 8 };
                let decoded = self
                    .rest()
                    .get(..width)
                    .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .and_then(char::from_u32);
                let Some(decoded) = decoded else {
                    return fail("invalid_toml_escape");
                };
                value.push(decoded);
                self.at += width;
                continue;
            }
            return fail("unsupported_toml_escape");
        }
    }

    fn keys(&mut self) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        loop {
            self.space();
            let key = if matches!(self.peek(), Some('"' | '\'')) {
                self.string()?
            } else {
                let rest = self.rest();
                let width = rest.len()
                    - rest
                        .trim_start_matches(|c: char| {
                            c.is_ascii_alphanumeric() || c == '_' || c == '-'
                        })
                        .len();
                if width == 0 {
                    return fail("invalid_toml_key");
                }
                self.at += width;
                rest[..width].to_string()
            };
            keys.push(key);
            if keys.len() > 32 {
                return fail("toml_key_depth_bound");
            }
            self.space();
            if self.peek() != Some('.') {
                return Ok(keys);
            }
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > 32 {
            return fail("toml_value_depth_bound");
        }
        self.space();
        match self.peek() {
            Some('"' | '\'') => Ok(Value::String(self.string()?)),
            Some('[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                while self.peek() != Some(']') {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    if self.peek() == Some(',') {
                        self.at += 1;
                        self.space();
                    } else if self.peek() != Some(']') {
                        return fail("invalid_toml_array");
                    }
                    if items.len() > 65536 {
                        return fail("toml_array_bound");
                    }
                }
                self.at += 1;
                Ok(Value::Array(items))
            }
            Some('{') => {
                self.at += 1;
                let mut table = Map::new();
                self.space();
                while self.peek() != Some('}') {
                    let keys = self.keys()?;
                    if self.peek() != Some('=') {
                        return fail("invalid_inline_toml");
                    }
                    self.at += 1;
                    let value = self.value(depth + 1)?;
                    set(&mut table, &keys, value)?;
                    self.space();
                    if self.peek() == Some(',') {
                        self.at += 1;
                        self.space();
                    } else if self.peek() != Some('}') {
                        return fail("invalid_inline_toml");
                    }
                }
                self.at += 1;
                Ok(Value::Object(table))
            }
            _ => {
                let rest = self.rest();
                let width = rest
                    .find(|c: char| c.is_whitespace() || c == ',' || c == ']' || c == '}')
                    .unwrap_or(rest.len());
                if width == 0 {
                    return fail("invalid_toml_value");
                }
                self.at += width;
                scalar(&rest[..width])
            }
        }
    }

    fn complete(&mut self) -> Result<(), String> {
        self.space();
        if self.at != self.text.len() {
            return fail("unsupported_mcp_toml_value");
        }
        Ok(())
    }
}

/// Booleans and numbers decode; other TOML scalars (dates, `inf`, numbers
/// outside the safe range) are kept opaque as `{"tomlScalar": raw}`.
fn scalar(raw: &str) -> Result<Value, String> {
    if raw == "true" || raw == "false" {
        return Ok(Value::Bool(raw == "true"));
    }
    let opaque = || {
        let mut map = Map::new();
        map.insert("tomlScalar".into(), Value::String(raw.into()));
        Ok(Value::Object(map))
    };
    let digits = raw.replace('_', "");
    let unsigned = digits.trim_start_matches(['+', '-']);
    let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| raw.strip_prefix(prefix).map(|_| (&digits[2..], radix)));
    if let Some((body, radix)) = radix {
        if body.is_empty() || !body.chars().all(|c| c.is_digit(radix)) {
            return fail("unsupported_mcp_toml_value");
        }
        return match i64::from_str_radix(body, radix) {
            Ok(value) if value.unsigned_abs() < (1 << 53) => Ok(Value::Number(value.into())),
            _ => opaque(),
        };
    }
    let decimal = !unsigned.is_empty()
        && unsigned.starts_with(|c: char| c.is_ascii_digit())
        && unsigned
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
    if decimal {
        if let Ok(value) = digits.parse::<i64>() {
            return if value.unsigned_abs() < (1 << 53) {
                Ok(Value::Number(value.into()))
            } else {
                opaque()
            };
        }
        return match digits.parse::<f64>().ok().and_then(Number::from_f64) {
            Some(number) => Ok(Value::Number(number)),
            None => opaque(),
        };
    }
    let bytes = raw.as_bytes();
    let date = bytes.len() >= 10 && bytes[..4].iter().all(u8::is_ascii_digit) && bytes[4] == b'-';
    let time = bytes.len() >= 8 && bytes[..2].iter().all(u8::is_ascii_digit) && bytes[2] == b':';
    if date || time || matches!(unsigned, "inf" | "nan") {
        return opaque();
    }
    fail("unsupported_mcp_toml_value")
}

fn set(target: &mut Map<String, Value>, keys: &[String], value: Value) -> Result<(), String> {
    let Some((last, parents)) = keys.split_last() else {
        return fail("invalid_toml_key");
    };
    let mut target = target;
    for key in parents {
        let entry = target
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        target = match entry {
            Value::Object(map) => map,
            _ => return fail("conflicting_mcp_toml_field"),
        };
    }
    if target.contains_key(last) {
        return fail("duplicate_mcp_toml_field");
    }
    target.insert(last.clone(), value);
    Ok(())
}

fn header_keys(text: &str) -> Result<Option<(bool, Vec<String>)>, String> {
    if !text.starts_with('[') {
        return Ok(None);
    }
    let array = text.starts_with("[[");
    let close = if array { "]]" } else { "]" };
    if !text.ends_with(close) || text.len() < 2 * close.len() {
        return fail("invalid_toml_header");
    }
    let mut cursor = Cursor::new(&text[close.len()..text.len() - close.len()]);
    let keys = cursor.keys()?;
    cursor.complete()?;
    Ok(Some((array, keys)))
}

/// `parseTomlDeclarations`: every server under any `mcp_servers` table.
pub(super) fn declarations(source: &str) -> Result<Vec<super::recognize::Declaration>, String> {
    let mut root = Map::new();
    let mut seen: Vec<Vec<String>> = Vec::new();
    let mut sealed: Vec<Vec<String>> = Vec::new();
    let mut table: Vec<String> = Vec::new();
    let servers = "mcp_servers".to_string();
    for statement in scan(source)? {
        let text = statement.text.as_str();
        if text.is_empty() {
            continue;
        }
        if let Some((array, keys)) = header_keys(text)? {
            table = keys;
            if table.contains(&servers) {
                if array {
                    return fail("unsupported_mcp_array_table");
                }
                if seen.contains(&table) {
                    return fail("duplicate_mcp_toml_table");
                }
                seen.push(table.clone());
            }
            continue;
        }
        let mut cursor = Cursor::new(text);
        let keys = cursor.keys()?;
        cursor.eat('=')?;
        let full: Vec<String> = table.iter().chain(&keys).cloned().collect();
        if !full.contains(&servers) {
            continue;
        }
        if sealed
            .iter()
            .any(|keys| keys.len() <= full.len() && keys[..] == full[..keys.len()])
        {
            return fail("conflicting_inline_mcp_toml");
        }
        cursor.space();
        let inline = cursor.peek() == Some('{');
        let value = cursor.value(0)?;
        cursor.complete()?;
        set(&mut root, &full, value)?;
        if inline {
            sealed.push(full);
        }
    }
    let mut rows = Vec::new();
    visit(&root, &mut Vec::new(), &mut rows)?;
    Ok(rows)
}

fn visit(
    value: &Map<String, Value>,
    prefix: &mut Vec<String>,
    rows: &mut Vec<super::recognize::Declaration>,
) -> Result<(), String> {
    for (key, item) in value {
        if key == "mcp_servers" {
            let Value::Object(group) = item else {
                return fail("invalid_mcp_server_group");
            };
            for (server_key, fields) in group {
                let Value::Object(fields) = fields else {
                    return fail("invalid_mcp_server_declaration");
                };
                let mut container = prefix.clone();
                container.push(key.clone());
                rows.push(super::recognize::Declaration {
                    server_key: server_key.clone(),
                    container,
                    fields: fields.clone(),
                });
                if rows.len() > MAX_DECLARATIONS {
                    return fail("configuration_declaration_bound");
                }
            }
        } else if let Value::Object(item) = item {
            prefix.push(key.clone());
            visit(item, prefix, rows)?;
            prefix.pop();
        }
    }
    Ok(())
}

fn eol(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn bare(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn quoted(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

/// One `[mcp_servers.<key>]` table or AIRK subtable: header start to next header.
struct Table {
    header: usize,
    start: usize,
    end: usize,
    nested: bool,
}

/// The tables of top-level server `key`. Inline, dotted or custom-subtable
/// spellings need manual review, as in `consolidateToml`.
fn server_tables(source: &str, key: &str) -> Result<(Vec<Statement>, Vec<Table>), String> {
    declarations(source)?;
    let statements = scan(source)?;
    let mut headers = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        if let Some((_, keys)) = header_keys(&statement.text)? {
            headers.push((index, keys));
        }
    }
    let mut tables = Vec::new();
    for (position, (index, keys)) in headers.iter().enumerate() {
        if keys.first().map(String::as_str) != Some("mcp_servers")
            || keys.get(1).map(String::as_str) != Some(key)
        {
            continue;
        }
        let nested = match keys.len() {
            2 => false,
            3 if NESTED.contains(&keys[2].as_str()) => true,
            _ => {
                return Err(
                    "Custom AIRK TOML subtables require review; existing file preserved.".into(),
                );
            }
        };
        let end = headers
            .get(position + 1)
            .map_or(source.len(), |(next, _)| statements[*next].start);
        tables.push(Table {
            header: *index,
            start: statements[*index].start,
            end,
            nested,
        });
    }
    if !tables.iter().any(|table| !table.nested) {
        return Err(
            "Inline or dotted AIRK TOML requires manual review; existing file preserved.".into(),
        );
    }
    Ok((statements, tables))
}

/// Set string `field` of `[mcp_servers.<key>]`: replace a single-line value,
/// or add `field = "…"` after the table's last assignment.
pub(super) fn set_string(
    source: &str,
    key: &str,
    field: &str,
    value: &str,
) -> Result<String, String> {
    let (statements, tables) = server_tables(source, key)?;
    let Some(main) = tables.iter().find(|table| !table.nested) else {
        return Err("AIRK TOML table missing".into());
    };
    let mut last = &statements[main.header];
    for statement in &statements[main.header + 1..] {
        if statement.start >= main.end {
            break;
        }
        if statement.text.is_empty() {
            continue;
        }
        last = statement;
        let mut cursor = Cursor::new(&statement.text);
        if cursor.keys()? != [field] {
            continue;
        }
        cursor.space();
        cursor.eat('=')?;
        cursor.space();
        let offset = cursor.at;
        let current = cursor.value(0)?;
        cursor.complete()?;
        if current.as_str() == Some(value) {
            return Ok(source.to_string());
        }
        let Some((begin, end)) = statement
            .raw
            .filter(|(begin, end)| source[*begin..*end] == statement.text)
        else {
            return Err(format!(
                "The AIRK TOML field {field} spans several lines or comments; review it manually."
            ));
        };
        return Ok(format!(
            "{}{}{}",
            &source[..begin + offset],
            quoted(value),
            &source[end..]
        ));
    }
    let at = last.end;
    let eol = eol(source);
    let gap = if source[..at].ends_with('\n') {
        ""
    } else {
        eol
    };
    Ok(format!(
        "{}{gap}{field} = {}{eol}{}",
        &source[..at],
        quoted(value),
        &source[at..]
    ))
}

/// Remove `[mcp_servers.<key>]` and its AIRK subtables. Comments inside them
/// are kept in place, as `consolidateToml` keeps them.
pub(super) fn remove_server(source: &str, key: &str) -> Result<String, String> {
    let (statements, tables) = server_tables(source, key)?;
    let eol = eol(source);
    let mut out = source.to_string();
    for table in tables.iter().rev() {
        let comments: Vec<&str> = statements
            .iter()
            .filter(|statement| statement.start >= table.start && statement.start < table.end)
            .flat_map(|statement| statement.comments.iter().map(String::as_str))
            .collect();
        let replacement = if comments.is_empty() {
            String::new()
        } else {
            format!("{}{eol}", comments.join(eol))
        };
        out.replace_range(table.start..table.end, &replacement);
    }
    Ok(out)
}

/// Append the HTTPS server table `serializeServers` renders for Codex, after
/// one blank line.
pub(super) fn append_server(source: &str, key: &str, url: &str, helper: &str) -> String {
    let eol = eol(source);
    let header = if bare(key) {
        key.to_string()
    } else {
        quoted(key)
    };
    let mut out = source.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(eol);
        }
        if !(out.ends_with("\n\n") || out.ends_with("\n\r\n")) {
            out.push_str(eol);
        }
    }
    out.push_str(&format!(
        "[mcp_servers.{header}]{eol}url = {}{eol}http_headers_helper = {}{eol}{eol}[mcp_servers.{header}.http_headers]{eol}\"X-AIRK-Response-Mode\" = \"terse\"{eol}",
        quoted(url),
        quoted(helper)
    ));
    out
}
