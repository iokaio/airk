// SPDX-License-Identifier: Apache-2.0
//! Declarations and AIRK ownership recognition: ports of
//! `globalConfigDeclarations` (unifiedAgentConfig.ts), `stdioCommand`
//! (agentConfigHealth.ts), `recordedRoot`/`recordedCollection`/`samePath`
//! (agentReconnect.ts) and `httpUrl` (serviceHttp.ts), with HTTPS ownership
//! narrowed by U92: Claude Code and Codex entries are AIRK's only when their
//! header helper is `<absolute airkctl> agents credential --connection-id <id>
//! --format header-json`. The pre-split `airk runtime http-credential` helper
//! and bare `AIRK_MCP_TOKEN_*` forms are not recognized, so nothing here ever
//! edits them. VS Code has no helper; its entries are recognized by the
//! `${input:airkToken_<id>}` Authorization reference the extension writes.
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

/// `httpUrl`: exactly `https://127.0.0.1:<port>/mcp`.
pub(super) fn http_url(value: &str) -> bool {
    let Some(port) = value
        .strip_prefix("https://127.0.0.1:")
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

/// The U92 helper command for `airkctl`, quoted as `credentialHelperCommand`
/// quotes paths: plain when a client shell passes it through unchanged,
/// double-quoted otherwise; refused when a shell could reinterpret it.
pub(super) fn helper_command(airkctl: &str, id: Incarnation) -> Result<String, String> {
    let plain = airkctl
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_.:\\/+-".contains(&byte));
    if !is_absolute(airkctl)
        || airkctl.len() > 1024
        || airkctl
            .chars()
            .any(|c| "\"'`$%!".contains(c) || c.is_control())
        || airkctl.starts_with("\\\\")
        || !is_airkctl(airkctl)
    {
        return Err(format!(
            "agents credential helper path unsupported: {airkctl:?} must be an absolute airkctl path without shell metacharacters"
        ));
    }
    let executable = if plain {
        airkctl.to_string()
    } else {
        format!("\"{airkctl}\"")
    };
    Ok(format!("{executable}{HELPER_MIDDLE}{id}{HELPER_END}"))
}

/// The executable and connection of a U92 helper command, or `None` for any
/// other command (including the pre-split `airk runtime http-credential`).
pub(super) fn helper_connection(command: &str) -> Option<(String, Incarnation)> {
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
    (is_absolute(executable) && is_airkctl(executable)).then(|| (executable.to_string(), id))
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

fn header<'a>(fields: &'a Map<String, Value>, name: &str) -> Result<Option<&'a str>, ()> {
    match fields.get("headers") {
        None => Ok(None),
        Some(Value::Object(headers)) => match headers.get(name) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value)),
            Some(_) => Err(()),
        },
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
        id: Incarnation,
        url: String,
        /// The helper executable (Claude-style and Codex entries).
        helper: Option<String>,
        /// The U92 helper proves AIRK wrote the entry; VS Code entries lack one.
        proven: bool,
    },
}

/// Whether `row` is an AIRK entry. A project entry needs an AIRK key (or the
/// U92 helper); a user-level entry needs a verified launcher (stdio) or the
/// U92 helper (HTTPS). VS Code user entries are recognized here but count
/// only once the service confirms them (see `classify`).
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
    let (id, helper) = match agent.parser() {
        Parser::Codex => {
            if fields.contains_key("bearer_token_env_var") {
                return None;
            }
            let (executable, id) = helper_connection(fields.get("http_headers_helper")?.as_str()?)?;
            (id, Some(executable))
        }
        Parser::Claude => {
            if fields.contains_key("command") {
                return None;
            }
            let (executable, id) = helper_connection(fields.get("headersHelper")?.as_str()?)?;
            match header(fields, "Authorization").ok()? {
                None => {}
                Some(value) if env_reference(value) == Some(id) => {}
                Some(_) => return None,
            }
            (id, Some(executable))
        }
        Parser::Copilot => {
            if fields.contains_key("command") || fields.contains_key("args") {
                return None;
            }
            (
                input_reference(header(fields, "Authorization").ok()??)?,
                None,
            )
        }
    };
    let proven = helper.is_some();
    if scope == Scope::Project && !keyed && !proven {
        return None;
    }
    Some(Recognized::Https {
        id,
        url: url.to_string(),
        helper,
        proven,
    })
}
