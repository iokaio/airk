// SPDX-License-Identifier: Apache-2.0
//! `tools shell` (split step 3c.4c-2): `airk`'s connection-bound tools
//! console. The project or session is resolved once, before the banner, as
//! `airk` connected once (M1 for a binding, a folder or a catalog session);
//! `list` and `help` read M2 and `call` runs M3 with that one selection. The
//! banner, help, prompt, result layout and `Error:` lines are `airk`'s.
use super::{Context, ErrorLine, ExitClass, Selection, VERSION, fail};
use airk_core::service::{
    CombinedConnectRequest, ControlMethod, ControlResponse, MAX_TOOL_ARGUMENT_BYTES,
    ProjectRegistration, ResolveTarget, ToolResponseMode, ToolSelection, ToolsCatalogDetail,
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const HELP: &str = "Commands:\n  list                         List MCP tools\n  help <tool>                  Arguments, defaults and examples\n  call <tool> <JSON object>     Call a tool; omit JSON for {}\n  call <tool> --args-file <path> Read JSON from a file\n  help                         Show this help\n  exit                         Close this console\n\nTry:\n  call airk_status\n  help airk_search\n  call airk_search {\"query\":\"authentication\",\"mode\":\"vector\",\"limit\":5}\n\nThis is an AIRK tools console, not an operating-system shell.\nCalls read the existing index; they do not index or download models.\nJSON is entered directly without shell quoting. File paths may contain spaces.\n";

/// The one connection the console reads through.
struct Console {
    control: PathBuf,
    selection: ToolSelection,
    combined: Option<Box<CombinedConnectRequest>>,
    /// `--args-file` paths are relative to the workspace (`.` for a
    /// combined session), as they were.
    root: PathBuf,
}

/// Why a console line could not be read.
enum Input {
    TooLong,
    Failed,
}

pub(super) fn run(cx: &mut Context, selection: &Selection) -> u8 {
    let (console, registration) = match open(cx, selection) {
        Ok(opened) => opened,
        Err(code) => return code,
    };
    // Debug formatting escapes control characters in user-selected paths and names.
    let banner = match &registration {
        Some(registration) => format!(
            "AIRK Tools\nCollection: {:?}\nWorkspace: {:?}\nStorage: {:?}\n",
            registration.collection.as_str(),
            Path::new(&registration.root),
            Path::new(&registration.data_root)
        ),
        None => "AIRK Tools\nExplicit reviewed combined session. Use query_scope to inspect permitted members.\n".into(),
    };
    let _ = writeln!(cx.io.stdout, "{banner}");
    let _ = writeln!(cx.io.stdout, "{HELP}");
    let mut pending = Vec::new();
    loop {
        let _ = write!(cx.io.stdout, "airk> ");
        let _ = cx.io.stdout.flush();
        let line = match next_line(cx.io.stdin, &mut pending) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(Input::TooLong) => {
                return fail(
                    cx,
                    &ErrorLine::new(
                        ExitClass::Refused,
                        "operation_failed",
                        false,
                        "Console input exceeds 64 KiB; closing console",
                    ),
                );
            }
            Err(Input::Failed) => {
                return fail(
                    cx,
                    &ErrorLine::new(
                        ExitClass::Internal,
                        "internal",
                        false,
                        "console input failed",
                    ),
                );
            }
        };
        let Ok(line) = std::str::from_utf8(&line) else {
            let _ = writeln!(cx.io.stderr, "Error: input must be UTF-8");
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }
        if matches!(line, "help" | "--help" | "?") {
            let _ = writeln!(cx.io.stdout, "{HELP}");
            continue;
        }
        if let Err(message) = command(cx, &console, line) {
            let _ = writeln!(cx.io.stderr, "Error: {message}");
        }
    }
    0
}

/// Resolve the console's one selection and the project its banner names.
fn open(
    cx: &mut Context,
    selection: &Selection,
) -> Result<(Console, Option<ProjectRegistration>), u8> {
    if let Some(combined) = &selection.combined {
        return Ok((
            Console {
                control: super::control(cx, selection)?,
                selection: selection.tool_selection(),
                combined: Some(Box::new(combined.clone())),
                root: PathBuf::from("."),
            },
            None,
        ));
    }
    let (control, registration, tool_selection) = match &selection.catalog {
        Some(request) => {
            let control = super::control(cx, selection)?;
            let target = ResolveTarget::Catalog {
                request: request.clone(),
            };
            let registration = match crate::service::request(
                &control,
                ControlMethod::ResolveProject { target },
                VERSION,
            ) {
                Ok(ControlResponse::Resolved { registration }) => registration,
                Ok(ControlResponse::RegistrationRefused { message }) => {
                    return Err(fail(cx, &crate::registration::failed(&message)));
                }
                Ok(other) => return Err(fail(cx, &crate::service::refusal(&other))),
                Err(line) => return Err(fail(cx, &line)),
            };
            let tool_selection = ToolSelection::Catalog {
                request: request.clone(),
            };
            (control, registration, tool_selection)
        }
        None => {
            let (control, registration) = super::project_for(cx, selection)?;
            let tool_selection = ToolSelection::Project {
                binding: registration.binding,
            };
            (control, registration, tool_selection)
        }
    };
    Ok((
        Console {
            control,
            selection: tool_selection,
            combined: None,
            root: PathBuf::from(&registration.root),
        },
        Some(registration),
    ))
}

/// The next console line, newline included, within the 64 KiB bound; the
/// last unterminated line at end of input; `None` at end of input.
fn next_line(
    stdin: &mut dyn std::io::Read,
    pending: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>, Input> {
    loop {
        if let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            if end + 1 > MAX_TOOL_ARGUMENT_BYTES {
                return Err(Input::TooLong);
            }
            let rest = pending.split_off(end + 1);
            return Ok(Some(std::mem::replace(pending, rest)));
        }
        if pending.len() > MAX_TOOL_ARGUMENT_BYTES {
            return Err(Input::TooLong);
        }
        let mut chunk = [0u8; 4096];
        match stdin.read(&mut chunk) {
            Ok(0) if pending.is_empty() => return Ok(None),
            Ok(0) => return Ok(Some(std::mem::take(pending))),
            Ok(read) => pending.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(Input::Failed),
        }
    }
}

fn command(cx: &mut Context, console: &Console, line: &str) -> Result<(), String> {
    let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    match command {
        "list" if rest.is_empty() => catalog(
            cx,
            console,
            ToolsCatalogDetail::List { schemas: false },
            None,
        ),
        "help" if !rest.is_empty() => catalog(
            cx,
            console,
            ToolsCatalogDetail::Help { tool: rest.into() },
            Some(rest),
        ),
        "call" if !rest.is_empty() => {
            let (tool, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let args = args.trim();
            let arguments = if let Some(path) = args.strip_prefix("--args-file ") {
                let path = path.trim();
                let path = path
                    .strip_prefix('"')
                    .and_then(|path| path.strip_suffix('"'))
                    .unwrap_or(path);
                crate::tools::read_arguments(None, Some(&console.root.join(path)))
            } else {
                crate::tools::read_arguments((!args.is_empty()).then_some(args), None)
            }
            .map_err(|error| format!("{error:#}"))?;
            call(cx, console, tool, arguments)
        }
        _ => Err("Unknown command. Type help for instructions.".into()),
    }
}

/// `list` and `help <tool>`: the M2 document as `tools list|help` printed it.
fn catalog(
    cx: &mut Context,
    console: &Console,
    detail: ToolsCatalogDetail,
    help: Option<&str>,
) -> Result<(), String> {
    let method = ControlMethod::ToolsCatalog {
        detail,
        combined: console.combined.clone(),
    };
    let document = match crate::service::request(&console.control, method, VERSION) {
        Ok(ControlResponse::ToolsCatalog { document }) => document,
        Ok(ControlResponse::Failure { code }) if code == "unknown_tool" => {
            return Err(format!(
                "Unknown MCP tool '{}'. Use 'airkctl tools list'.",
                help.unwrap_or_default()
            ));
        }
        Ok(ControlResponse::ToolCallFailed { message }) => return Err(message),
        Ok(other) => return Err(crate::service::refusal(&other).message),
        Err(line) => return Err(line.message),
    };
    let text = serde_json::from_str::<Value>(&document)
        .ok()
        .and_then(|value| match help {
            Some(name) => crate::tools::help_text(&value, name),
            None => crate::tools::list_text(&value),
        })
        .ok_or_else(|| "The AIRK service returned a malformed tool catalog.".to_owned())?;
    let _ = write!(cx.io.stdout, "{text}");
    Ok(())
}

/// `call`: M3 with the console's selection, its JSON text content made
/// readable as `airk`'s console made it.
fn call(
    cx: &mut Context,
    console: &Console,
    tool: &str,
    arguments: Map<String, Value>,
) -> Result<(), String> {
    let method = ControlMethod::ToolsCall {
        tool: tool.into(),
        arguments,
        selection: console.selection.clone(),
        response_mode: if cx.io.full_responses {
            ToolResponseMode::Full
        } else {
            ToolResponseMode::Terse
        },
        usage_client: airk_core::usage::ClientFamily::Cli,
    };
    let result = match crate::service::request(&console.control, method, VERSION) {
        Ok(ControlResponse::ToolResult { result }) => result,
        Ok(ControlResponse::ToolCallFailed { message }) => return Err(message),
        Ok(other) => return Err(crate::service::refusal(&other).message),
        Err(line) => return Err(line.message),
    };
    let failed = result["isError"].as_bool().unwrap_or(false);
    let _ = writeln!(
        cx.io.stdout,
        "{}",
        if failed { "MCP error:" } else { "Result:" }
    );
    let contents = result["content"]
        .as_array()
        .ok_or_else(|| "Missing MCP content".to_owned())?;
    for content in contents {
        let shown = if content["type"] == "text" {
            let text = content["text"]
                .as_str()
                .ok_or_else(|| "Missing MCP text".to_owned())?;
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::from(text))
        } else {
            content.clone()
        };
        let text = serde_json::to_string_pretty(&shown).map_err(|error| error.to_string())?;
        let _ = writeln!(cx.io.stdout, "{text}");
    }
    if failed {
        return Err("MCP tool failed; see the result above".into());
    }
    Ok(())
}
