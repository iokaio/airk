// SPDX-License-Identifier: Apache-2.0
//! Declarations and AIRK ownership recognition: ports of
//! `globalConfigDeclarations` (unifiedAgentConfig.ts), `stdioCommand`
//! (agentConfigHealth.ts), `recordedRoot`/`recordedCollection`/`samePath`
//! (agentReconnect.ts) and `httpUrl` (serviceHttp.ts). Local trust plan C:
//! the endpoint is `http://127.0.0.1:<port>/mcp`; an `https://` entry an
//! HTTPS build wrote is still AIRK's (and always an endpoint mismatch). U110: `/mcp` needs no
//! credential, so an HTTPS entry is AIRK's by its loopback AIRK URL plus an
//! AIRK server key (`airk`, `airk-…`, `airk_…`), or plus a legacy AIRK
//! credential marker (the U92 `<absolute airkctl> agents credential
//! --connection-id <id> --format header-json` helper, an `AIRK_MCP_TOKEN_<ID>`
//! reference or a `${input:airkToken_<id>}` reference). Entries that still
//! carry any credential field (`Authorization`, `headersHelper`,
//! `http_headers_helper`, `bearer_token_env_var`) are recognized as stale so
//! `agents repair` can strip them; a `<connection-id>` placeholder no longer
//! hides an entry.
use super::{Agent, Scope, jsonc, toml};
use airk_core::identity::Incarnation;
use serde_json::{Map, Value};

/// Configuration files above this are refused (the TS editing limit).
pub(super) const MAX_BYTES: usize = 1_048_576;
const MAX_DECLARATIONS: usize = 512;
const HELPER_MIDDLE: &str = " agents credential --connection-id ";
const HELPER_END: &str = " --format header-json";

/// The declaration grammar of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Parser {
    Codex,
    Copilot,
    Claude,
}

impl Agent {
    pub(super) fn parser(self) -> Parser {
        match self {
            Agent::Codex => Parser::Codex,
            Agent::Copilot => Parser::Copilot,
            _ => Parser::Claude,
        }
    }
}

/// One MCP server declaration.
#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub(super) server_key: String,
    pub(super) container: Vec<String>,
    pub(super) fields: Map<String, Value>,
}

/// Every server declaration, sorted by container and key. JSON files
/// declare `servers` (VS Code) or `mcpServers` plus Claude's
/// `projects.<path>.mcpServers`; TOML files every `mcp_servers` table.
pub(super) fn declarations(parser: Parser, source: &str) -> Result<Vec<Declaration>, String> {
    if source.len() > MAX_BYTES {
        return Err("configuration_size_bound".into());
    }
    let mut rows = if parser == Parser::Codex {
        toml::declarations(source)?
    } else {
        let root = jsonc::parse(source)?.value();
        let mut rows = Vec::new();
        if parser == Parser::Copilot {
            group(&mut rows, &["servers"], root.get("servers"))?;
        } else {
            group(&mut rows, &["mcpServers"], root.get("mcpServers"))?;
            if let Some(projects) = root.get("projects") {
                let projects = projects.as_object().ok_or("invalid_claude_projects")?;
                for (project, settings) in projects {
                    let settings = settings.as_object().ok_or("invalid_claude_project")?;
                    group(
                        &mut rows,
                        &["projects", project, "mcpServers"],
                        settings.get("mcpServers"),
                    )?;
                }
            }
        }
        rows
    };
    rows.sort_by(|a, b| (&a.container, &a.server_key).cmp(&(&b.container, &b.server_key)));
    Ok(rows)
}

fn group(
    rows: &mut Vec<Declaration>,
    container: &[&str],
    raw: Option<&Value>,
) -> Result<(), String> {
    let Some(raw) = raw else { return Ok(()) };
    let servers = raw.as_object().ok_or("invalid_mcp_server_group")?;
    for (key, fields) in servers {
        let fields = fields.as_object().ok_or("invalid_mcp_server_declaration")?;
        rows.push(Declaration {
            server_key: key.clone(),
            container: container.iter().map(|part| part.to_string()).collect(),
            fields: fields.clone(),
        });
        if rows.len() > MAX_DECLARATIONS {
            return Err("configuration_declaration_bound".into());
        }
    }
    Ok(())
}

/// Keys AIRK writes or recognizes in project files: `airk`, `airk-<…>`, `airk_<…>`.
pub(super) fn airk_key(key: &str) -> bool {
    key == "airk" || key.starts_with("airk-") || key.starts_with("airk_")
}

/// `httpUrl`: exactly `http://127.0.0.1:<port>/mcp`, or the
/// `https://127.0.0.1:<port>/mcp` an HTTPS build wrote (`legacy_url`).
pub(super) fn http_url(value: &str) -> bool {
    current_url(value) || legacy_url(value)
}

/// The current endpoint form, `http://127.0.0.1:<port>/mcp`.
pub(super) fn current_url(value: &str) -> bool {
    loopback_url(value, "http://127.0.0.1:")
}

/// Local trust plan C: the `https://127.0.0.1:<port>/mcp` form an HTTPS build
/// wrote. Still AIRK's, never current.
pub(super) fn legacy_url(value: &str) -> bool {
    loopback_url(value, "https://127.0.0.1:")
}

fn loopback_url(value: &str, prefix: &str) -> bool {
    let Some(port) = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix("/mcp"))
    else {
        return false;
    };
    (1..=5).contains(&port.len())
        && !port.starts_with('0')
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port
            .parse::<u32>()
            .is_ok_and(|port| (1..=65535).contains(&port))
}

/// A Windows (`C:\`, `C:/`, `\\`) or POSIX absolute path, on any host.
pub(super) fn is_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Remove the Win32 extended-length prefix (`\\?\C:\…`, `\\?\UNC\…`).
pub(super) fn plain_path(value: &str) -> String {
    if value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("\\\\?\\UNC\\"))
    {
        return format!("\\\\{}", &value[8..]);
    }
    let bytes = value.as_bytes();
    if value.starts_with("\\\\?\\")
        && bytes.len() >= 6
        && bytes[4].is_ascii_alphabetic()
        && bytes[5] == b':'
    {
        return value[4..].to_string();
    }
    value.to_string()
}

pub(super) fn same_path(left: &str, right: &str, windows: bool) -> bool {
    let normal = |value: &str| {
        let plain = plain_path(value);
        if windows {
            plain.replace('/', "\\").to_lowercase()
        } else {
            plain
        }
    };
    normal(left) == normal(right)
}

fn is_airkctl(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name == "airkctl" || name.eq_ignore_ascii_case("airkctl.exe")
}

/// The connection a legacy U92 helper command names, or `None` for any other
/// command (including the pre-split `airk runtime http-credential` and a
/// `<connection-id>` placeholder).
pub(super) fn helper_connection(command: &str) -> Option<Incarnation> {
    if command.len() > 4096 {
        return None;
    }
    let (executable, rest) = if let Some(quoted) = command.strip_prefix('"') {
        let end = quoted.find('"')?;
        (&quoted[..end], &quoted[end + 1..])
    } else {
        let end = command.find(char::is_whitespace).unwrap_or(command.len());
        (&command[..end], &command[end..])
    };
    if executable.is_empty() || executable.contains('"') {
        return None;
    }
    let id = rest.strip_prefix(HELPER_MIDDLE)?.strip_suffix(HELPER_END)?;
    let id: Incarnation = id.parse().ok()?;
    (is_absolute(executable) && is_airkctl(executable)).then_some(id)
}

/// A legacy AIRK header helper: the U92 `airkctl agents credential` form
/// (any connection id, placeholders included) or the pre-split
/// `airk runtime http-credential` form.
fn legacy_helper(command: &str) -> bool {
    command.len() <= 4096
        && ((command.contains("airkctl") && command.contains(HELPER_MIDDLE))
            || (command.contains("airk") && command.contains(" runtime http-credential ")))
}

/// `Bearer ${AIRK_MCP_TOKEN_<ID>}` or `Bearer ${env:AIRK_MCP_TOKEN_<ID>}`.
fn env_reference(value: &str) -> Option<Incarnation> {
    let inner = value.strip_prefix("Bearer ${")?.strip_suffix('}')?;
    let inner = inner.strip_prefix("env:").unwrap_or(inner);
    let id = inner.strip_prefix("AIRK_MCP_TOKEN_")?;
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
    {
        return None;
    }
    id.to_ascii_lowercase().parse().ok()
}

/// `Bearer ${input:airkToken_<id>}`.
pub(super) fn input_reference(value: &str) -> Option<Incarnation> {
    value
        .strip_prefix("Bearer ${input:airkToken_")?
        .strip_suffix('}')?
        .parse()
        .ok()
}

fn stdio_command(fields: &Map<String, Value>) -> Option<&str> {
    let command = fields.get("command")?.as_str()?;
    if fields.contains_key("url") || fields.contains_key("serverUrl") {
        return None;
    }
    match fields.get("type") {
        None => {}
        Some(Value::String(kind)) if kind == "stdio" || kind == "local" => {}
        Some(_) => return None,
    }
    Some(command)
}

/// The value following `flag` in a stdio argument list, when it appears once.
fn argument<'a>(fields: &'a Map<String, Value>, flag: &str) -> Option<&'a str> {
    let args = fields.get("args")?.as_array()?;
    let first = args.iter().position(|value| value.as_str() == Some(flag))?;
    let last = args
        .iter()
        .rposition(|value| value.as_str() == Some(flag))?;
    if first != last {
        return None;
    }
    args.get(first + 1)?.as_str()
}

fn recorded_root(fields: &Map<String, Value>) -> Option<String> {
    let root = plain_path(argument(fields, "--root")?);
    (root.len() <= 4096 && is_absolute(&root) && !root.chars().any(|c| c.is_control()))
        .then_some(root)
}

fn recorded_collection(fields: &Map<String, Value>) -> Option<String> {
    let collection = argument(fields, "--collection")?;
    (!collection.is_empty()
        && collection.len() <= 128
        && collection
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && collection != "."
        && collection != "..")
        .then(|| collection.to_string())
}

/// The members of the object `fields[table]` named `name`, compared ASCII
/// case-insensitively as HTTP header names are; `Err` when `table` is present
/// but not an object (manual review).
fn header_keys(fields: &Map<String, Value>, table: &str, name: &str) -> Result<Vec<String>, ()> {
    match fields.get(table) {
        None => Ok(Vec::new()),
        Some(Value::Object(headers)) => Ok(headers
            .keys()
            .filter(|key| key.eq_ignore_ascii_case(name))
            .cloned()
            .collect()),
        Some(_) => Err(()),
    }
}

/// A recognized AIRK entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Recognized {
    /// Every stdio AIRK entry is retired; the service refuses stdio MCP.
    Stdio {
        command: String,
        root: Option<String>,
        collection: Option<String>,
    },
    Https {
        url: String,
        /// The connection a legacy credential field names, when it names one.
        id: Option<Incarnation>,
        /// U110: the legacy credential fields the entry still carries, as
        /// paths below the server declaration (`["headersHelper"]`,
        /// `["headers", "Authorization"]`, …). Empty for the URL-only form.
        stale: Vec<Vec<String>>,
    },
}

/// Whether `row` is an AIRK entry. A stdio entry needs a verified launcher
/// or, in a project file, an AIRK key. An HTTPS entry (U110) needs the
/// loopback AIRK URL plus an AIRK key or a legacy AIRK credential marker,
/// in either scope.
pub(super) fn recognize(
    agent: Agent,
    scope: Scope,
    row: &Declaration,
    verified: &[String],
    windows: bool,
) -> Option<Recognized> {
    let fields = &row.fields;
    let keyed = airk_key(&row.server_key);
    if let Some(command) = stdio_command(fields) {
        let owned = is_absolute(&plain_path(command))
            && verified
                .iter()
                .any(|launcher| same_path(launcher, command, windows));
        if !owned && !(scope == Scope::Project && keyed) {
            return None;
        }
        return Some(Recognized::Stdio {
            command: command.to_string(),
            root: recorded_root(fields),
            collection: recorded_collection(fields),
        });
    }
    let url = fields.get("url")?.as_str()?;
    if !http_url(url) {
        return None;
    }
    let parser = agent.parser();
    let (helper_field, tables): (&str, &[&str]) = match parser {
        Parser::Codex => (
            "http_headers_helper",
            &["http_headers", "env_http_headers"][..],
        ),
        Parser::Claude | Parser::Copilot => {
            if fields.contains_key("command")
                || (parser == Parser::Copilot && fields.contains_key("args"))
            {
                return None;
            }
            ("headersHelper", &["headers"][..])
        }
    };
    let mut stale = Vec::new();
    let mut id = None;
    // An AIRK credential marker, even with a `<connection-id>` placeholder.
    let mut marked = false;
    if let Some(helper) = fields.get(helper_field) {
        if let Some(command) = helper.as_str() {
            id = helper_connection(command);
            marked = legacy_helper(command);
        }
        stale.push(vec![helper_field.to_string()]);
    }
    if parser == Parser::Codex && fields.contains_key("bearer_token_env_var") {
        stale.push(vec!["bearer_token_env_var".to_string()]);
    }
    for table in tables {
        for key in header_keys(fields, table, "Authorization").ok()? {
            if let Some(value) = fields
                .get(*table)
                .and_then(|headers| headers.get(key.as_str()))
                .and_then(Value::as_str)
            {
                id = id.or_else(|| env_reference(value).or_else(|| input_reference(value)));
                marked |= value.contains("AIRK_MCP_TOKEN_") || value.contains("airkToken_");
            }
            stale.push(vec![table.to_string(), key]);
        }
    }
    if !keyed && !marked && id.is_none() {
        return None;
    }
    Some(Recognized::Https {
        url: url.to_string(),
        id,
        stale,
    })
}
