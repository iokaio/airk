// SPDX-License-Identifier: Apache-2.0
//! A strict JSONC reader that keeps byte spans, and the few span edits AIRK
//! needs. It replaces the `jsonc-parser` `parseTree`/`modify`/`applyEdits`
//! use in agentConfig.ts: `//` and `/* */` comments and trailing commas are
//! accepted; syntax errors, duplicate keys and a non-object root are refused.
//! Every edit replaces one span and leaves all other bytes unchanged. New
//! members follow the file's detected indentation and line ending.
use serde_json::{Map, Value};

const MAX_DEPTH: usize = 128;

#[derive(Debug)]
pub(super) struct Node {
    pub(super) start: usize,
    /// Exclusive.
    pub(super) end: usize,
    pub(super) kind: Kind,
}

#[derive(Debug)]
pub(super) enum Kind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    String(String),
    Scalar(Value),
}

#[derive(Debug)]
pub(super) struct Member {
    pub(super) key: String,
    /// Offset of the key's opening quote.
    pub(super) start: usize,
    pub(super) value: Node,
}

impl Node {
    pub(super) fn member(&self, key: &str) -> Option<&Member> {
        match &self.kind {
            Kind::Object(members) => members.iter().find(|member| member.key == key),
            _ => None,
        }
    }

    pub(super) fn at(&self, path: &[String]) -> Option<&Node> {
        path.iter().try_fold(self, |node, key| {
            node.member(key).map(|member| &member.value)
        })
    }

    pub(super) fn value(&self) -> Value {
        match &self.kind {
            Kind::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|member| (member.key.clone(), member.value.value()))
                    .collect::<Map<_, _>>(),
            ),
            Kind::Array(items) => Value::Array(items.iter().map(Node::value).collect()),
            Kind::String(text) => Value::String(text.clone()),
            Kind::Scalar(value) => value.clone(),
        }
    }

    fn child_spans(&self) -> Vec<(usize, usize)> {
        match &self.kind {
            Kind::Object(members) => members.iter().map(|m| (m.start, m.value.end)).collect(),
            Kind::Array(items) => items.iter().map(|n| (n.start, n.end)).collect(),
            _ => Vec::new(),
        }
    }
}

/// Parse a configuration file whose root must be an object. A leading BOM is skipped.
pub(super) fn parse(source: &str) -> Result<Node, String> {
    let mut reader = Reader {
        text: source,
        bytes: source.as_bytes(),
        at: if source.starts_with('\u{feff}') { 3 } else { 0 },
    };
    reader.trivia()?;
    let root = reader.value(0)?;
    reader.trivia()?;
    if reader.at != reader.bytes.len() {
        return Err(reader.error("unexpected content after the configuration object"));
    }
    if !matches!(root.kind, Kind::Object(_)) {
        return Err("invalid JSON configuration: the root must be an object".into());
    }
    Ok(root)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn error(&self, what: &str) -> String {
        format!("invalid JSON configuration at byte {}: {what}", self.at)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn trivia(&mut self) -> Result<(), String> {
        loop {
            match (self.peek(), self.bytes.get(self.at + 1)) {
                (Some(b' ' | b'\t' | b'\r' | b'\n'), _) => self.at += 1,
                (Some(b'/'), Some(b'/')) => {
                    while !matches!(self.peek(), None | Some(b'\n')) {
                        self.at += 1;
                    }
                }
                (Some(b'/'), Some(b'*')) => {
                    let end = self.text[self.at + 2..]
                        .find("*/")
                        .ok_or_else(|| self.error("unterminated comment"))?;
                    self.at += end + 4;
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting exceeds the bound"));
        }
        let start = self.at;
        let kind = match self.peek() {
            Some(b'{') => {
                self.at += 1;
                let mut members: Vec<Member> = Vec::new();
                loop {
                    self.trivia()?;
                    match self.peek() {
                        Some(b'}') => break,
                        Some(b'"') => {}
                        _ => return Err(self.error("expected a property name")),
                    }
                    let key_start = self.at;
                    let key = self.string()?;
                    if members.iter().any(|member| member.key == key) {
                        return Err(format!("invalid JSON configuration: duplicate key {key:?}"));
                    }
                    self.trivia()?;
                    if self.peek() != Some(b':') {
                        return Err(self.error("expected ':'"));
                    }
                    self.at += 1;
                    self.trivia()?;
                    let value = self.value(depth + 1)?;
                    members.push(Member {
                        key,
                        start: key_start,
                        value,
                    });
                    self.trivia()?;
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => break,
                        _ => return Err(self.error("expected ',' or '}'")),
                    }
                }
                self.at += 1;
                Kind::Object(members)
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.trivia()?;
                    if self.peek() == Some(b']') {
                        break;
                    }
                    items.push(self.value(depth + 1)?);
                    self.trivia()?;
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => break,
                        _ => return Err(self.error("expected ',' or ']'")),
                    }
                }
                self.at += 1;
                Kind::Array(items)
            }
            Some(b'"') => Kind::String(self.string()?),
            Some(b'-' | b'0'..=b'9') => {
                while matches!(
                    self.peek(),
                    Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                ) {
                    self.at += 1;
                }
                let number: Value = serde_json::from_str(&self.text[start..self.at])
                    .map_err(|_| self.error("invalid number"))?;
                if !number.is_number() {
                    return Err(self.error("invalid number"));
                }
                Kind::Scalar(number)
            }
            _ => {
                let text = self.text;
                let rest = &text[self.at..];
                let (word, value) = [
                    ("true", Value::Bool(true)),
                    ("false", Value::Bool(false)),
                    ("null", Value::Null),
                ]
                .into_iter()
                .find(|(word, _)| rest.starts_with(*word))
                .ok_or_else(|| self.error("unexpected character"))?;
                self.at += word.len();
                if self
                    .peek()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(self.error("unexpected character"));
                }
                Kind::Scalar(value)
            }
        };
        Ok(Node {
            start,
            end: self.at,
            kind,
        })
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .text
            .get(self.at..self.at + 4)
            .filter(|digits| digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| self.error("invalid unicode escape"))?;
        self.at += 4;
        u32::from_str_radix(digits, 16).map_err(|_| self.error("invalid unicode escape"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        let mut run = self.at;
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| self.error("unterminated string"))?;
            match byte {
                b'"' => {
                    out.push_str(&self.text[run..self.at]);
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    out.push_str(&self.text[run..self.at]);
                    let escape = self
                        .bytes
                        .get(self.at + 1)
                        .copied()
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.at += 2;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            let mut decoded = char::from_u32(unit);
                            if (0xD800..0xDC00).contains(&unit)
                                && self.text[self.at..].starts_with("\\u")
                            {
                                let save = self.at;
                                self.at += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    decoded = char::from_u32(
                                        0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00),
                                    );
                                } else {
                                    self.at = save;
                                }
                            }
                            out.push(decoded.unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                    run = self.at;
                }
                0..=0x1f => return Err(self.error("control character in string")),
                _ => self.at += 1,
            }
        }
    }
}

/// A value AIRK writes, in the order it renders (`serde_json::Map` sorts).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Out {
    Str(String),
    Obj(Vec<(String, Out)>),
    Arr(Vec<Out>),
}

impl Out {
    pub(super) fn to_value(&self) -> Value {
        match self {
            Out::Str(text) => Value::String(text.clone()),
            Out::Obj(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect(),
            ),
            Out::Arr(items) => Value::Array(items.iter().map(Out::to_value).collect()),
        }
    }

    fn pretty(&self, indent: &str, format: &Format) -> String {
        let eol = format.eol;
        let inner = format!("{indent}{}", format.unit);
        let (open, close, body): (&str, &str, Vec<String>) = match self {
            Out::Str(text) => return quoted(text),
            Out::Obj(fields) => (
                "{",
                "}",
                fields
                    .iter()
                    .map(|(key, value)| {
                        format!("{inner}{}: {}", quoted(key), value.pretty(&inner, format))
                    })
                    .collect(),
            ),
            Out::Arr(items) => (
                "[",
                "]",
                items
                    .iter()
                    .map(|value| format!("{inner}{}", value.pretty(&inner, format)))
                    .collect(),
            ),
        };
        if body.is_empty() {
            return format!("{open}{close}");
        }
        format!(
            "{open}{eol}{}{eol}{indent}{close}",
            body.join(&format!(",{eol}"))
        )
    }
}

pub(super) fn quoted(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

struct Format {
    unit: String,
    eol: &'static str,
}

impl Format {
    /// The first indented line that starts a key sets the unit (as the TS
    /// writers detect it); two spaces otherwise.
    fn of(source: &str) -> Self {
        let unit = source
            .split('\n')
            .find_map(|line| {
                let width = line.len() - line.trim_start_matches([' ', '\t']).len();
                (width > 0 && line[width..].starts_with('"')).then(|| &line[..width])
            })
            .map_or_else(
                || "  ".to_string(),
                |indent| {
                    if indent.contains('\t') {
                        "\t".to_string()
                    } else {
                        " ".repeat(indent.len())
                    }
                },
            );
        Format {
            unit,
            eol: if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            },
        }
    }
}

fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_indent(source: &str, at: usize) -> &str {
    let start = line_start(source, at);
    let rest = &source[start..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

fn blank(text: &str) -> bool {
    text.bytes().all(|byte| byte == b' ' || byte == b'\t')
}

/// What follows a value on its line: an optional comma, then only spaces and
/// comments up to `end` (the line break) when `clean`.
struct After {
    comma: Option<usize>,
    end: usize,
    clean: bool,
}

fn after_line(source: &str, from: usize) -> After {
    let bytes = source.as_bytes();
    let skip = |at: &mut usize| {
        while matches!(bytes.get(*at), Some(b' ' | b'\t')) {
            *at += 1;
        }
    };
    let mut at = from;
    let mut comma = None;
    skip(&mut at);
    if bytes.get(at) == Some(&b',') {
        comma = Some(at);
        at += 1;
        skip(&mut at);
    }
    loop {
        if source[at..].starts_with("//") {
            while !matches!(bytes.get(at), None | Some(b'\n' | b'\r')) {
                at += 1;
            }
        } else if source[at..].starts_with("/*")
            && let Some(end) = source[at + 2..].find("*/")
            && !source[at..at + 2 + end].contains('\n')
        {
            at += end + 4;
            skip(&mut at);
            continue;
        }
        break;
    }
    After {
        comma,
        end: at,
        clean: matches!(bytes.get(at), None | Some(b'\n' | b'\r')),
    }
}

/// The first comma after `from`, before `limit`, skipping spaces and comments.
fn comma_after(source: &str, from: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut at = from;
    while at < limit {
        match bytes[at] {
            b',' => return Some(at),
            b' ' | b'\t' | b'\r' | b'\n' => at += 1,
            b'/' if source[at..].starts_with("//") => {
                at += source[at..].find('\n').unwrap_or(limit - at)
            }
            b'/' if source[at..].starts_with("/*") => {
                at += source[at..].find("*/").map_or(limit - at, |end| end + 2)
            }
            _ => return None,
        }
    }
    None
}

/// Insert one child after the last one, on its own line when the existing
/// children are one per line; `render` receives the child's indentation.
fn insert_child(
    source: &str,
    container: &Node,
    render: &dyn Fn(&str, &Format) -> String,
) -> String {
    let format = Format::of(source);
    let (open, close) = (container.start, container.end - 1);
    match container.child_spans().last() {
        Some(&(last_start, last_end)) => {
            let start = line_start(source, last_start);
            let after = after_line(source, last_end);
            if blank(&source[start..last_start]) && after.clean {
                let indent = &source[start..last_start];
                let comma = if after.comma.is_none() { "," } else { "" };
                return format!(
                    "{}{comma}{}{}{indent}{}{}",
                    &source[..last_end],
                    &source[last_end..after.end],
                    format.eol,
                    render(indent, &format),
                    &source[after.end..]
                );
            }
            let indent = format!("{}{}", line_indent(source, open), format.unit);
            format!(
                "{}, {}{}",
                &source[..last_end],
                render(&indent, &format),
                &source[last_end..]
            )
        }
        None => {
            let outer = line_indent(source, open);
            let indent = format!("{outer}{}", format.unit);
            let child = render(&indent, &format);
            if source[open + 1..close].trim().is_empty() {
                format!(
                    "{}{}{indent}{child}{}{outer}{}",
                    &source[..=open],
                    format.eol,
                    format.eol,
                    &source[close..]
                )
            } else {
                format!(
                    "{}{}{indent}{child}{}",
                    &source[..=open],
                    format.eol,
                    &source[open + 1..]
                )
            }
        }
    }
}

/// Remove one child with its line when it has a line of its own, keeping
/// the remaining children's commas valid; an emptied container becomes `{}`/`[]`.
fn remove_child(source: &str, container: &Node, index: usize) -> String {
    let spans = container.child_spans();
    let (start, end) = spans[index];
    let (open, close) = (container.start, container.end - 1);
    let line = line_start(source, start);
    let after = after_line(source, end);
    if blank(&source[line..start]) && after.clean {
        let mut cut = after.end;
        if source[cut..].starts_with("\r\n") {
            cut += 2;
        } else if source[cut..].starts_with('\n') {
            cut += 1;
        }
        if spans.len() == 1
            && source[open + 1..line].trim().is_empty()
            && source[cut..close].trim().is_empty()
        {
            return format!("{}{}", &source[..=open], &source[close..]);
        }
        let mut out = format!("{}{}", &source[..line], &source[cut..]);
        if after.comma.is_none()
            && index > 0
            && let Some(comma) = comma_after(source, spans[index - 1].1, line)
        {
            out.remove(comma);
        }
        return out;
    }
    if index > 0 {
        return format!("{}{}", &source[..spans[index - 1].1], &source[end..]);
    }
    if let Some(&(next, _)) = spans.get(1) {
        return format!("{}{}", &source[..start], &source[next..]);
    }
    format!("{}{}", &source[..=open], &source[close..])
}

fn object_at<'a>(tree: &'a Node, path: &[String]) -> Result<Option<&'a Node>, String> {
    match tree.at(path) {
        None => Ok(None),
        Some(node) if matches!(node.kind, Kind::Object(_)) => Ok(Some(node)),
        Some(_) => Err(format!(
            "{} must be an object; existing configuration preserved.",
            path.join(".")
        )),
    }
}

/// Add `key` to the object at `path`, creating that object when only its
/// last segment is missing. The key must not exist.
pub(super) fn insert_member(
    source: &str,
    path: &[String],
    key: &str,
    value: &Out,
) -> Result<String, String> {
    let tree = parse(source)?;
    if let Some(node) = object_at(&tree, path)? {
        if node.member(key).is_some() {
            return Err(format!(
                "{key} already exists; existing configuration preserved."
            ));
        }
        let render = |indent: &str, format: &Format| {
            format!("{}: {}", quoted(key), value.pretty(indent, format))
        };
        return Ok(insert_child(source, node, &render));
    }
    let (last, parent) = path.split_last().ok_or("configuration root missing")?;
    let parent = object_at(&tree, parent)?.ok_or_else(|| {
        format!(
            "{} is missing; existing configuration preserved.",
            parent.join(".")
        )
    })?;
    let wrapped = Out::Obj(vec![(key.to_string(), value.clone())]);
    let render = |indent: &str, format: &Format| {
        format!("{}: {}", quoted(last), wrapped.pretty(indent, format))
    };
    Ok(insert_child(source, parent, &render))
}

/// Set the string at `path`, inserting it when absent; unchanged when equal.
pub(super) fn set_string(source: &str, path: &[String], value: &str) -> Result<String, String> {
    let tree = parse(source)?;
    match tree.at(path) {
        Some(node) if matches!(&node.kind, Kind::String(current) if current == value) => {
            Ok(source.to_string())
        }
        Some(node) => Ok(format!(
            "{}{}{}",
            &source[..node.start],
            quoted(value),
            &source[node.end..]
        )),
        None => {
            let (key, parent) = path.split_last().ok_or("configuration root missing")?;
            insert_member(source, parent, key, &Out::Str(value.to_string()))
        }
    }
}

pub(super) fn remove_member(source: &str, path: &[String], key: &str) -> Result<String, String> {
    let tree = parse(source)?;
    let node = object_at(&tree, path)?.ok_or_else(|| format!("{} is missing", path.join(".")))?;
    let index = match &node.kind {
        Kind::Object(members) => members.iter().position(|member| member.key == key),
        _ => None,
    }
    .ok_or_else(|| format!("{key} is missing"))?;
    Ok(remove_child(source, node, index))
}

pub(super) fn remove_element(
    source: &str,
    path: &[String],
    index: usize,
) -> Result<String, String> {
    let tree = parse(source)?;
    match tree.at(path) {
        Some(node) if matches!(&node.kind, Kind::Array(items) if index < items.len()) => {
            Ok(remove_child(source, node, index))
        }
        _ => Err(format!(
            "{} changed; existing configuration preserved.",
            path.join(".")
        )),
    }
}

pub(super) fn append_element(source: &str, path: &[String], value: &Out) -> Result<String, String> {
    let tree = parse(source)?;
    match tree.at(path) {
        Some(node) if matches!(node.kind, Kind::Array(_)) => Ok(insert_child(
            source,
            node,
            &|indent: &str, format: &Format| value.pretty(indent, format),
        )),
        Some(_) => Err(format!(
            "{} must be an array; existing configuration preserved.",
            path.join(".")
        )),
        None => {
            let (key, parent) = path.split_last().ok_or("configuration root missing")?;
            insert_member(source, parent, key, &Out::Arr(vec![value.clone()]))
        }
    }
}
