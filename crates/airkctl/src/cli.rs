// SPDX-License-Identifier: Apache-2.0
//! The `airkctl` command line (split plan §6): argument parsing, the I1 calls
//! and the I2 output and exit classes. `main` runs it over the process streams;
//! tests run the same code in process with captured streams.
//!
//! This build serves `--version`, `version`, `tools list|help|call` and the
//! fifteen evidence reads; every other command answers the typed
//! `not_yet_served` refusal (exit 5) until its handler lands. The service is
//! found through `--service-runtime-dir` until installation discovery (step 3b).
use crate::contract::{
    ErrorLine, ExitClass, I2_CONTRACT, I2_CONTRACTS_SUPPORTED, Installation, InstallationState,
    PRODUCT, VERSION_SCHEMA, VersionDocument,
};
use crate::evidence::EvidenceCommand;
use airk_core::combined_reads::MemberAvailability;
use airk_core::index_catalog::{IndexId, QueryProjectId};
use airk_core::service::{
    CombinedConnectRequest, ControlMethod, ControlResponse, ProjectBinding, ScopedConnectRequest,
    ToolResponseMode, ToolSelection, ToolsCatalogDetail,
};
use clap::{Parser, Subcommand};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Capabilities of the served operations of this build (contracts §3.5).
const CAPABILITIES: &[&str] = &["tools/1"];

/// The streams and environment one run uses.
pub struct Io<'a> {
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    /// `AIRK_MCP_RESPONSE_MODE=full`, as the pre-split CLI read it.
    pub full_responses: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Output {
    Text,
    Json,
}

/// One run's output mode and streams.
struct Context<'a, 'b> {
    output: Output,
    io: &'a mut Io<'b>,
}

#[derive(Parser)]
#[command(
    name = "airkctl",
    version,
    about = "AIRK command-line client: reaches the AIRK service through its local control endpoint",
    disable_help_subcommand = true
)]
struct Cli {
    /// Result and error format: json writes one JSON document or error line.
    #[arg(long, global = true, value_parser = ["json", "text"], default_value = "text")]
    output: String,
    /// The AIRK service's control directory.
    #[arg(long, global = true)]
    service_runtime_dir: Option<PathBuf>,
    /// A registered project binding.
    #[arg(long, global = true)]
    service_project: Option<ProjectBinding>,
    /// Explicit reviewed single-index session JSON; preferences alone grant no access.
    #[arg(long, global = true, value_parser = parse_catalog_session,
        conflicts_with = "service_project")]
    catalog_session: Option<ScopedConnectRequest>,
    /// Frozen reviewed multi-Project session JSON; indexes are executed once each.
    #[arg(long, global = true, value_parser = parse_combined_session,
        conflicts_with_all = ["catalog_session", "service_project"])]
    combined_session: Option<CombinedConnectRequest>,
    /// Narrow a combined read to explicit Projects from its reviewed session.
    #[arg(
        long,
        global = true,
        value_delimiter = ',',
        num_args = 1,
        requires = "combined_session"
    )]
    project_ids: Option<Vec<QueryProjectId>>,
    /// Narrow a combined read to member indexes; requires explicit Project scope.
    #[arg(long, global = true, value_delimiter = ',', num_args = 1, requires_all = ["combined_session", "project_ids"])]
    index_ids: Option<Vec<IndexId>>,
    /// Report member failures or require every selected member to answer.
    #[arg(long, global = true, value_parser = ["report", "require"], requires_all = ["combined_session", "project_ids"])]
    member_availability: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the version document: airkctl, its contracts and the installation.
    Version,
    /// The unified tools: list, help and call.
    Tools {
        #[command(subcommand)]
        action: ToolsCommand,
    },
    #[command(flatten)]
    Evidence(Box<EvidenceCommand>),
}

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
enum ToolsCommand {
    /// List the connection's unified tools; --json includes complete schemas and annotations.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a tool's description, input schema, defaults and argument template.
    Help {
        tool: String,
        #[arg(long)]
        json: bool,
    },
    /// Invoke an MCP tool and print its unchanged result envelope as JSON.
    Call {
        tool: String,
        /// JSON object; omit for {}. Maximum 64 KiB.
        #[arg(long, conflicts_with = "args_file")]
        args: Option<String>,
        /// Read a UTF-8 JSON object from a regular file (maximum 64 KiB).
        #[arg(long)]
        args_file: Option<PathBuf>,
    },
}

fn parse_catalog_session(value: &str) -> Result<ScopedConnectRequest, String> {
    if value.len() > 4096 {
        return Err("catalog session exceeds its byte allowance".into());
    }
    let selection: ScopedConnectRequest =
        serde_json::from_str(value).map_err(|_| "invalid catalog session JSON".to_owned())?;
    selection
        .validate()
        .map_err(|_| "invalid reviewed catalog session".to_owned())?;
    Ok(selection)
}

fn parse_combined_session(value: &str) -> Result<CombinedConnectRequest, String> {
    if value.len() > 8192 {
        return Err("combined session exceeds its byte allowance".into());
    }
    let selection: CombinedConnectRequest =
        serde_json::from_str(value).map_err(|_| "invalid combined session JSON".to_owned())?;
    selection
        .validate()
        .map_err(|_| "invalid reviewed combined session".to_owned())?;
    Ok(selection)
}

fn own_digest() -> Option<String> {
    let bytes = std::fs::read(std::env::current_exe().ok()?).ok()?;
    let digest = Sha256::digest(bytes);
    Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn version_document() -> VersionDocument {
    VersionDocument {
        schema: VERSION_SCHEMA.into(),
        product: PRODUCT.into(),
        version: VERSION.into(),
        // The install pointer and record (I4) arrive in step C6; until then no
        // installation can be verified, so none is reported.
        channel: None,
        i2_contract: I2_CONTRACT,
        i2_contracts_supported: I2_CONTRACTS_SUPPORTED.to_vec(),
        i1_protocol: airk_core::service::PROTOCOL_VERSION,
        capabilities: CAPABILITIES.iter().map(|&name| name.into()).collect(),
        executable_sha256: own_digest(),
        installation: Installation {
            state: InstallationState::NotFound,
            airk_version: None,
            pair: None,
            install_id: None,
        },
    }
}

fn fail(cx: &mut Context, line: &ErrorLine) -> u8 {
    let _ = match cx.output {
        Output::Json => writeln!(
            cx.io.stderr,
            "{}",
            serde_json::to_string(line).unwrap_or_else(|_| "{}".into())
        ),
        Output::Text => writeln!(cx.io.stderr, "airkctl: {}", line.message),
    };
    line.class.code()
}

fn usage(cx: &mut Context, message: &str) -> u8 {
    fail(
        cx,
        &ErrorLine::new(ExitClass::Usage, "usage", false, message),
    )
}

fn not_yet_served(cx: &mut Context) -> u8 {
    fail(
        cx,
        &ErrorLine::new(
            ExitClass::UnsupportedCapability,
            "not_yet_served",
            false,
            "This command is not served by this AIRK Service version.",
        ),
    )
}

/// A write to stdout that failed (a closed pipe) is an internal failure.
fn written(cx: &mut Context, result: std::io::Result<()>) -> u8 {
    match result.and_then(|()| cx.io.stdout.flush()) {
        Ok(()) => 0,
        Err(_) => ExitClass::Internal.code(),
    }
}

fn print_json(cx: &mut Context, value: &Value) -> u8 {
    match serde_json::to_string_pretty(value) {
        Ok(text) => {
            let result = writeln!(cx.io.stdout, "{text}");
            written(cx, result)
        }
        Err(_) => ExitClass::Internal.code(),
    }
}

/// The global options that select a service and a project.
struct Selection {
    control: Option<PathBuf>,
    project: Option<ProjectBinding>,
    catalog: Option<ScopedConnectRequest>,
    combined: Option<CombinedConnectRequest>,
    project_ids: Option<Vec<QueryProjectId>>,
    index_ids: Option<Vec<IndexId>>,
    member_availability: Option<MemberAvailability>,
}

impl Selection {
    /// The M3 selection, in the CLI's precedence: a combined or catalog
    /// session, then a project binding.
    fn tool_selection(&self) -> ToolSelection {
        if let Some(request) = &self.combined {
            return ToolSelection::Combined {
                request: request.clone(),
                project_ids: self.project_ids.clone(),
                index_ids: self.index_ids.clone(),
                member_availability: self.member_availability,
            };
        }
        match (&self.catalog, self.project) {
            (Some(request), _) => ToolSelection::Catalog {
                request: request.clone(),
            },
            (None, Some(binding)) => ToolSelection::Project { binding },
            (None, None) => ToolSelection::Unselected,
        }
    }
}

/// A call or catalog that failed before a result existed: the CLI's
/// `operation_failed` line with the complete native response.
fn operation_failed(cx: &mut Context, message: String) -> u8 {
    let mut line = ErrorLine::new(ExitClass::Refused, "operation_failed", false, &message);
    line.native = serde_json::to_value(ControlResponse::ToolCallFailed { message }).ok();
    fail(cx, &line)
}

/// The service's control directory: explicit until the install pointer (3b).
fn control(cx: &mut Context, selection: &Selection) -> Result<PathBuf, u8> {
    match &selection.control {
        Some(control) => Ok(control.clone()),
        None => Err(fail(
            cx,
            &ErrorLine::new(
                ExitClass::ServiceMissing,
                "installation_not_found",
                false,
                "No AIRK installation was found; pass --service-runtime-dir <service control directory>.",
            ),
        )),
    }
}

fn version(cx: &mut Context) -> u8 {
    let document = version_document();
    let result = match cx.output {
        Output::Json => match serde_json::to_string(&document) {
            Ok(text) => writeln!(cx.io.stdout, "{text}"),
            Err(_) => {
                return fail(
                    cx,
                    &ErrorLine::new(ExitClass::Internal, "internal", false, "version document"),
                );
            }
        },
        Output::Text => writeln!(
            cx.io.stdout,
            "airkctl {VERSION} ({PRODUCT})\nI2 contract {I2_CONTRACT}; I1 protocol {}\ninstallation: not found",
            document.i1_protocol
        ),
    };
    written(cx, result)
}

fn catalog(cx: &mut Context, selection: &Selection, action: ToolsCommand) -> u8 {
    let (detail, json) = match action {
        ToolsCommand::List { json } => (
            ToolsCatalogDetail::List {
                schemas: json || cx.output == Output::Json,
            },
            json,
        ),
        ToolsCommand::Help { tool, json } => (ToolsCatalogDetail::Help { tool }, json),
        ToolsCommand::Call { .. } => unreachable!("calls are not catalog reads"),
    };
    let control = match control(cx, selection) {
        Ok(control) => control,
        Err(code) => return code,
    };
    let help = match &detail {
        ToolsCatalogDetail::Help { tool } => Some(tool.clone()),
        ToolsCatalogDetail::List { .. } => None,
    };
    let response = match crate::service::request(
        &control,
        ControlMethod::ToolsCatalog {
            detail,
            combined: selection.combined.clone().map(Box::new),
        },
        VERSION,
    ) {
        Ok(response) => response,
        Err(line) => return fail(cx, &line),
    };
    let document = match response {
        ControlResponse::ToolsCatalog { document } => document,
        ControlResponse::Failure { code } if code == "unknown_tool" => {
            let name = help.unwrap_or_default();
            let mut line = crate::service::refusal(&ControlResponse::Failure { code });
            line.message = format!("Unknown MCP tool '{name}'. Use 'airkctl tools list'.");
            return fail(cx, &line);
        }
        ControlResponse::ToolCallFailed { message } => return operation_failed(cx, message),
        other => return fail(cx, &crate::service::refusal(&other)),
    };
    if json || cx.output == Output::Json {
        let result = writeln!(cx.io.stdout, "{document}");
        return written(cx, result);
    }
    let text = serde_json::from_str::<Value>(&document)
        .ok()
        .and_then(|value| match &help {
            Some(name) => crate::tools::help_text(&value, name),
            None => crate::tools::list_text(&value),
        });
    match text {
        Some(text) => {
            let result = write!(cx.io.stdout, "{text}");
            written(cx, result)
        }
        None => fail(
            cx,
            &ErrorLine::new(
                ExitClass::Internal,
                "malformed_response",
                false,
                "The AIRK service returned a malformed tool catalog.",
            ),
        ),
    }
}

/// One M3 call printed as the pre-split CLI printed it: the complete envelope
/// as pretty JSON, then a refusal when the envelope reports an error.
fn call(
    cx: &mut Context,
    selection: &Selection,
    tool: String,
    arguments: Map<String, Value>,
) -> u8 {
    let control = match control(cx, selection) {
        Ok(control) => control,
        Err(code) => return code,
    };
    let selection = selection.tool_selection();
    let local_help = matches!(
        selection,
        ToolSelection::Unselected | ToolSelection::Project { .. }
    ) && matches!(tool.as_str(), "airk_help" | "help");
    let response_mode = if cx.io.full_responses {
        ToolResponseMode::Full
    } else {
        ToolResponseMode::Terse
    };
    let response = match crate::service::request(
        &control,
        ControlMethod::ToolsCall {
            tool,
            arguments,
            selection,
            response_mode,
            usage_client: airk_core::usage::ClientFamily::Cli,
        },
        VERSION,
    ) {
        Ok(response) => response,
        Err(line) => return fail(cx, &line),
    };
    match response {
        ControlResponse::ToolResult { result } => {
            let printed = print_json(cx, &result);
            if result["isError"].as_bool().unwrap_or(false) {
                let message = if local_help {
                    "AIRK help request failed"
                } else {
                    "MCP tool failed; see the result above"
                };
                return fail(
                    cx,
                    &ErrorLine::new(ExitClass::Refused, "tool_failed", false, message),
                );
            }
            printed
        }
        ControlResponse::ToolCallFailed { message } => operation_failed(cx, message),
        other => fail(cx, &crate::service::refusal(&other)),
    }
}

fn evidence(cx: &mut Context, mut selection: Selection, command: EvidenceCommand) -> u8 {
    if let EvidenceCommand::Parse {
        project: Some(project),
        ..
    } = &command
    {
        if selection.catalog.is_some() {
            return usage(
                cx,
                "catalog sessions are read-only; administrative commands require an explicit administrative invocation",
            );
        }
        if selection.combined.is_some() {
            return usage(
                cx,
                "combined sessions are read-only; settings and administrative commands require an explicit index or administrative invocation",
            );
        }
        if selection
            .project
            .is_some_and(|selected| selected != *project)
        {
            return usage(cx, "conflicting project selections");
        }
        selection.project = Some(*project);
    }
    let (tool, arguments) = match crate::evidence::query(command) {
        Ok(query) => query,
        Err(error) => return usage(cx, &format!("{error:#}")),
    };
    // The pre-split CLI passed the mapped arguments through the same bounded
    // reader as `tools call --args`.
    let arguments = match serde_json::to_string(&arguments)
        .map_err(anyhow::Error::from)
        .and_then(|text| crate::tools::read_arguments(Some(&text), None))
    {
        Ok(arguments) => arguments,
        Err(error) => return usage(cx, &format!("{error:#}")),
    };
    call(cx, &selection, tool.into(), arguments)
}

fn dispatch(cx: &mut Context, command: Command, selection: Selection) -> u8 {
    match command {
        Command::Version => version(cx),
        Command::Tools {
            action:
                ToolsCommand::Call {
                    tool,
                    args,
                    args_file,
                },
        } => match crate::tools::read_arguments(args.as_deref(), args_file.as_deref()) {
            Ok(arguments) => call(cx, &selection, tool, arguments),
            Err(error) => usage(cx, &format!("{error:#}")),
        },
        Command::Tools { action } => catalog(cx, &selection, action),
        Command::Evidence(command) => evidence(cx, selection, *command),
    }
}

/// The output mode, read before parsing so an argument error can honor it.
fn requested_output(arguments: &[OsString]) -> Output {
    let mut iterator = arguments.iter().filter_map(|argument| argument.to_str());
    while let Some(argument) = iterator.next() {
        if argument == "--output" {
            if iterator.next() == Some("json") {
                return Output::Json;
            }
        } else if argument == "--output=json" {
            return Output::Json;
        }
    }
    Output::Text
}

/// Run one `airkctl` command line (the program name first) and return its
/// I2 exit code.
pub fn run(arguments: Vec<OsString>, io: &mut Io) -> u8 {
    let output = requested_output(arguments.get(1..).unwrap_or_default());
    let mut cx = Context { output, io };
    let cli = match Cli::try_parse_from(&arguments) {
        Ok(cli) => cli,
        Err(error) => {
            use clap::error::ErrorKind;
            return match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    let result = write!(cx.io.stdout, "{}", error.render());
                    written(&mut cx, result)
                }
                ErrorKind::InvalidSubcommand => not_yet_served(&mut cx),
                ErrorKind::MissingSubcommand
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    usage(&mut cx, "a command is required; run `airkctl version`")
                }
                _ => {
                    let rendered = error.render().to_string();
                    let message = rendered
                        .lines()
                        .next()
                        .unwrap_or("invalid arguments")
                        .trim_start_matches("error: ")
                        .to_owned();
                    usage(&mut cx, &message)
                }
            };
        }
    };
    cx.output = if cli.output == "json" {
        Output::Json
    } else {
        Output::Text
    };
    let selection = Selection {
        control: cli.service_runtime_dir,
        project: cli.service_project,
        catalog: cli.catalog_session,
        combined: cli.combined_session,
        project_ids: cli.project_ids,
        index_ids: cli.index_ids,
        member_availability: cli.member_availability.map(|policy| {
            if policy == "require" {
                MemberAvailability::Require
            } else {
                MemberAvailability::Report
            }
        }),
    };
    dispatch(&mut cx, cli.command, selection)
}

#[cfg(test)]
mod tests;
