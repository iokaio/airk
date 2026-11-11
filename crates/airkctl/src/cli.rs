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
use std::path::{Path, PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Capabilities of the served operations of this build (contracts §3.5).
const CAPABILITIES: &[&str] = &["tools/1"];

/// The streams and environment one run uses.
pub struct Io<'a> {
    pub stdin: &'a mut dyn std::io::Read,
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
        conflicts_with_all = ["service_project", "root", "data_dir", "collection"])]
    catalog_session: Option<ScopedConnectRequest>,
    /// Frozen reviewed multi-Project session JSON; indexes are executed once each.
    #[arg(long, global = true, value_parser = parse_combined_session,
        conflicts_with_all = ["catalog_session", "service_project", "root", "data_dir", "collection"])]
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
    /// The project folder (default: the current folder).
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    /// The project's data folder (default: the installation's data location).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true, default_value = "default")]
    collection: String,
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
    /// The AIRK service: health and shutdown.
    Service {
        #[command(subcommand)]
        action: ServiceCommand,
    },
    #[command(flatten)]
    Evidence(Box<EvidenceCommand>),
}

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
enum ServiceCommand {
    /// Start the installed service (the install record's verified airk) and wait until it is healthy.
    Start {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Stop the service and wait until its process exits; a saved Stop keeps automatic starts off.
    Stop {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Stop the service, wait until its process exits, then start the installed service again.
    Restart {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Read the service's diagnostic logs (recent from the running service; all, raw or
    /// stopped-service reads from the local files).
    Logs {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 32)]
        limit: usize,
        /// Export retained logs locally (200 MB high-water, FIFO purge to 100 MB).
        #[arg(long, conflicts_with = "limit")]
        all: bool,
        /// Open the separate local raw job-error ring (may contain paths/error values).
        #[arg(long, conflicts_with_all = ["all", "limit", "format"])]
        raw: bool,
        /// Capture raw retained diagnostics into a new local file without buffering stdout (U79).
        #[arg(long)]
        output_file: Option<PathBuf>,
        #[arg(long, default_value = "json", value_parser = ["json", "text"])]
        format: String,
    },
    /// Report the installation and whether the service is running, with its saved Stop and
    /// any unfinished stop or restart.
    Status {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Record a bounded client error (JSON on stdin) without needing the service to run.
    RecordError {
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
        #[arg(long)]
        operation: String,
    },
    /// Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise.
    Health {
        /// The service's control directory (default: --service-runtime-dir).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Ask the service for --runtime-dir to drain its workers and exit.
    Shutdown {
        /// The service's control directory (default: --service-runtime-dir).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
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

/// This user's installation as the I4 reader verified it (step 3b.1c), with
/// its channel. The pair is exact when the record names this version and this
/// executable's digest.
fn installation() -> (Installation, Option<String>) {
    installation_of(&crate::install::discover())
}

fn installation_of(discovery: &crate::install::Discovery) -> (Installation, Option<String>) {
    let absent = |state| Installation {
        state,
        airk_version: None,
        pair: None,
        install_id: None,
    };
    match discovery {
        crate::install::Discovery::NotFound => (absent(InstallationState::NotFound), None),
        crate::install::Discovery::Unverified { .. } => {
            (absent(InstallationState::Unverified), None)
        }
        crate::install::Discovery::Verified(verified) => {
            let record = &verified.record;
            let pair = if pair_exact(record) {
                crate::contract::PairState::Exact
            } else {
                crate::contract::PairState::Mismatch
            };
            (
                Installation {
                    state: InstallationState::Complete,
                    airk_version: Some(record.version.clone()),
                    pair: Some(pair),
                    install_id: Some(record.install_id.to_string()),
                },
                Some(record.channel.clone()),
            )
        }
    }
}

/// The pair is exact when the record names this version and this executable.
fn pair_exact(record: &airk_core::install_record::InstallRecord) -> bool {
    record.version == VERSION
        && own_digest().as_deref() == Some(record.executables.airkctl.sha256.to_string().as_str())
}

fn version_document() -> VersionDocument {
    let (installation, channel) = installation();
    VersionDocument {
        schema: VERSION_SCHEMA.into(),
        product: PRODUCT.into(),
        version: VERSION.into(),
        channel,
        i2_contract: I2_CONTRACT,
        i2_contracts_supported: I2_CONTRACTS_SUPPORTED.to_vec(),
        i1_protocol: airk_core::service::PROTOCOL_VERSION,
        capabilities: CAPABILITIES.iter().map(|&name| name.into()).collect(),
        executable_sha256: own_digest(),
        installation,
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
    /// `--root`, `--data-dir` and `--collection` for a project named by its folder.
    root: PathBuf,
    data_dir: Option<PathBuf>,
    collection: String,
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
            "airkctl {VERSION} ({PRODUCT})\nI2 contract {I2_CONTRACT}; I1 protocol {}\ninstallation: {}",
            document.i1_protocol,
            match document.installation.state {
                InstallationState::Complete => "complete",
                InstallationState::Incomplete => "incomplete",
                InstallationState::NotFound => "not found",
                InstallationState::Unverified => "unverified",
            }
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

/// The control root and M3 selection of one call. Without a binding or a
/// session (and for any tool but the in-process help), the project the
/// `--root`/`--data-dir`/`--collection` folder names is resolved through M1
/// (step 3c.1), starting the installed service automatically when it is down
/// (contracts §3.2; a saved explicit Stop is honored).
fn resolve_call(
    cx: &mut Context,
    selection: &Selection,
    tool: &str,
) -> Result<(PathBuf, ToolSelection), u8> {
    let explicit = selection.tool_selection();
    if !matches!(explicit, ToolSelection::Unselected) || matches!(tool, "airk_help" | "help") {
        return Ok((control(cx, selection)?, explicit));
    }
    let not_found = |cx: &mut Context, message: &str| {
        fail(
            cx,
            &ErrorLine::new(
                ExitClass::ServiceMissing,
                "installation_not_found",
                false,
                message,
            ),
        )
    };
    let mut verified = None;
    let discover = |verified: &mut Option<Box<crate::install::Verified>>| {
        if verified.is_none()
            && let crate::install::Discovery::Verified(found) = crate::install::discover()
        {
            *verified = Some(found);
        }
    };
    if selection.control.is_none() || selection.data_dir.is_none() {
        discover(&mut verified);
    }
    let control = match selection.control.clone().or_else(|| {
        verified
            .as_ref()
            .map(|found| PathBuf::from(&found.record.locations.state))
    }) {
        Some(control) => control,
        None => {
            return Err(not_found(
                cx,
                "No AIRK installation was found; pass --service-runtime-dir <service control directory>.",
            ));
        }
    };
    let data = match selection.data_dir.clone().or_else(|| {
        verified
            .as_ref()
            .map(|found| PathBuf::from(&found.record.locations.data))
    }) {
        Some(data) => data,
        None => {
            return Err(not_found(
                cx,
                "No AIRK installation was found; pass --data-dir <project data folder>.",
            ));
        }
    };
    match crate::service::request(&control, ControlMethod::Health, VERSION) {
        Ok(ControlResponse::Health { .. }) => {}
        Ok(other) => return Err(fail(cx, &crate::service::refusal(&other))),
        Err(line) if line.class == ExitClass::ServiceMissing => {
            discover(&mut verified);
            let Some(found) = verified.as_ref() else {
                return Err(fail(cx, &line));
            };
            if !pair_exact(&found.record) {
                return Err(fail(
                    cx,
                    &ErrorLine::new(
                        ExitClass::IncompatiblePair,
                        "pair_mismatch",
                        false,
                        "This airkctl is not the installation's airkctl; use the installed one.",
                    ),
                ));
            }
            if let Err(line) = crate::start::start_with(
                &found.airk,
                &control,
                VERSION,
                airk_core::install_record::StartMode::Automatic,
            ) {
                return Err(fail(cx, &line));
            }
        }
        Err(line) => return Err(fail(cx, &line)),
    }
    let absolute = |path: &Path| {
        std::path::absolute(path)
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned))
    };
    let (Some(root), Some(data_root)) = (absolute(&selection.root), absolute(&data)) else {
        return Err(usage(cx, "--root and --data-dir must be UTF-8 paths"));
    };
    let target = airk_core::service::ResolveTarget::Registration {
        root,
        data_root,
        collection: selection.collection.clone(),
    };
    match crate::service::request(&control, ControlMethod::ResolveProject { target }, VERSION) {
        Ok(ControlResponse::Resolved { registration }) => Ok((
            control,
            ToolSelection::Project {
                binding: registration.binding,
            },
        )),
        Ok(ControlResponse::Failure { code }) if code == "project_not_registered" => Err(fail(
            cx,
            &ErrorLine::new(
                ExitClass::Refused,
                &code,
                false,
                "project is not registered with this service; review registration before indexing or connecting",
            ),
        )),
        Ok(other) => Err(fail(cx, &crate::service::refusal(&other))),
        Err(line) => Err(fail(cx, &line)),
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
    let (control, selection) = match resolve_call(cx, selection, &tool) {
        Ok(resolved) => resolved,
        Err(code) => return code,
    };
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

/// The compact JSON line `airk service health|shutdown` printed.
fn print_compact(cx: &mut Context, response: &ControlResponse) -> u8 {
    match serde_json::to_string(response) {
        Ok(text) => {
            let result = writeln!(cx.io.stdout, "{text}");
            written(cx, result)
        }
        Err(_) => ExitClass::Internal.code(),
    }
}

/// `service health` and `service shutdown`, as `airk service health|shutdown
/// --runtime-dir` ran them: `Health` with the pair check (version and
/// protocol; the executable digest joins with the install record, step
/// 3b.1c), then for shutdown a `Shutdown` fenced to the observed instance.
/// `service start`: a verified installation with an exact pair, then the
/// record's `airk` started detached (step 3b.1d), printing the `Health` line.
/// The verified installation with an exact pair, or the exit code that refuses it.
fn installed(cx: &mut Context) -> Result<Box<crate::install::Verified>, u8> {
    let verified = match crate::install::discover() {
        crate::install::Discovery::Verified(verified) => verified,
        crate::install::Discovery::NotFound => {
            return Err(fail(
                cx,
                &ErrorLine::new(
                    ExitClass::ServiceMissing,
                    "installation_not_found",
                    false,
                    "No AIRK installation was found for this user.",
                ),
            ));
        }
        crate::install::Discovery::Unverified { code, message } => {
            return Err(fail(
                cx,
                &ErrorLine::new(ExitClass::InstallationUnverified, &code, false, &message),
            ));
        }
    };
    if !pair_exact(&verified.record) {
        return Err(fail(
            cx,
            &ErrorLine::new(
                ExitClass::IncompatiblePair,
                "pair_mismatch",
                false,
                "This airkctl is not the installation's airkctl; use the installed one.",
            ),
        ));
    }
    Ok(verified)
}

fn start_service(cx: &mut Context, selection: Selection, runtime_dir: Option<PathBuf>) -> u8 {
    let verified = match installed(cx) {
        Ok(verified) => verified,
        Err(code) => return code,
    };
    let control = runtime_dir
        .or(selection.control)
        .unwrap_or_else(|| PathBuf::from(&verified.record.locations.state));
    match crate::start::start(&verified.airk, &control, VERSION) {
        Ok(started) => print_compact(cx, &started.health),
        Err(line) => fail(cx, &line),
    }
}

/// `service stop` and `service restart` (U85; step 3b.1e-3). Stop prints the
/// accepted lifecycle record once the process exited; restart then starts the
/// installed service and prints its new `Health`.
fn lifecycle_service(
    cx: &mut Context,
    selection: Selection,
    runtime_dir: Option<PathBuf>,
    action: airk_core::service_lifecycle::LifecycleAction,
) -> u8 {
    use airk_core::service_lifecycle::LifecycleAction;
    let explicit = runtime_dir.or(selection.control);
    let verified = if action == LifecycleAction::Restart || explicit.is_none() {
        match installed(cx) {
            Ok(verified) => Some(verified),
            Err(code) => return code,
        }
    } else {
        None
    };
    let control = match (explicit, &verified) {
        (Some(control), _) => control,
        (None, Some(verified)) => PathBuf::from(&verified.record.locations.state),
        (None, None) => unreachable!("an installation was required above"),
    };
    let stopped = match crate::lifecycle::stop(&control, VERSION, action) {
        Ok(stopped) => stopped,
        Err(line) => return fail(cx, &line),
    };
    let Some(verified) = verified.filter(|_| action == LifecycleAction::Restart) else {
        return print_compact(cx, &stopped.accepted);
    };
    match crate::start::start(&verified.airk, &control, VERSION) {
        Ok(started) => match &started.health {
            ControlResponse::Health { observation } if observation.instance != stopped.instance => {
                print_compact(cx, &started.health)
            }
            _ => fail(
                cx,
                &ErrorLine::new(
                    ExitClass::OutcomeUnknown,
                    "lifecycle_predecessor_is_still_active",
                    false,
                    "The restarted service reports the stopped instance.",
                ),
            ),
        },
        Err(line) => fail(cx, &line),
    }
}

/// `service logs` (U86/U88/U89; step 3b.1f-2), printing what `airk service
/// logs` printed: recent pages from the running service (`Logs`), otherwise and
/// for `--all`/`--raw` the local files through the shared parser.
fn logs_service(cx: &mut Context, selection: Selection, options: LogOptions) -> u8 {
    let control = match options.runtime_dir.clone().or(selection.control) {
        Some(control) => control,
        None => match installed(cx) {
            Ok(verified) => PathBuf::from(&verified.record.locations.state),
            Err(code) => return code,
        },
    };
    match read_logs(cx, &control, options) {
        Ok(code) => code,
        Err(error) => fail(
            cx,
            &ErrorLine::new(
                ExitClass::Refused,
                "diagnostic_logs_unavailable",
                false,
                &format!("{error:#}"),
            ),
        ),
    }
}

/// `service logs` arguments.
struct LogOptions {
    runtime_dir: Option<PathBuf>,
    limit: usize,
    all: bool,
    raw: bool,
    output_file: Option<PathBuf>,
    format: String,
}

/// A new file written through a temporary sibling and never replacing one.
fn capture(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> anyhow::Result<()>,
) -> anyhow::Result<u64> {
    use anyhow::Context as _;
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent().context("diagnostic output parent missing")?,
    )?;
    write(file.as_file_mut())?;
    file.as_file().sync_all()?;
    let written = file.as_file().metadata()?.len();
    file.persist_noclobber(path)?;
    Ok(written)
}

fn read_logs(cx: &mut Context, control: &Path, options: LogOptions) -> anyhow::Result<u8> {
    if options.raw {
        if let Some(path) = &options.output_file {
            let mut captured = 0;
            capture(path, |file| {
                captured = crate::logs::export_raw_to(control, file)?;
                Ok(())
            })?;
            let line = serde_json::json!({"written_bytes": captured}).to_string();
            let result = writeln!(cx.io.stdout, "{line}");
            return Ok(written(cx, result));
        }
        let mut bytes = Vec::new();
        crate::logs::export_raw_to(control, &mut bytes)?;
        let result = cx.io.stdout.write_all(&bytes);
        return Ok(written(cx, result));
    }
    if options.all && options.format != "json" {
        return Ok(usage(cx, "full log export requires JSON format"));
    }
    if options.output_file.is_some() && !(options.all && options.format == "json") {
        return Ok(usage(
            cx,
            "file capture requires --raw or --all --format json",
        ));
    }
    let page = if options.all {
        crate::logs::export(control)?
    } else {
        match crate::service::request(
            control,
            ControlMethod::Logs {
                limit: options.limit,
            },
            VERSION,
        ) {
            Ok(ControlResponse::Logs { page }) => page,
            Err(line) if line.class == ExitClass::ServiceMissing => {
                crate::logs::recent(control, options.limit)?
            }
            Ok(other) => return Ok(fail(cx, &crate::service::refusal(&other))),
            Err(line) => return Ok(fail(cx, &line)),
        }
    };
    if let Some(path) = &options.output_file {
        let captured = capture(path, |file| Ok(serde_json::to_writer(file, &page)?))?;
        let line = serde_json::json!({"written_bytes": captured}).to_string();
        let result = writeln!(cx.io.stdout, "{line}");
        return Ok(written(cx, result));
    }
    let text = if options.format == "text" {
        airk_core::service_logs::render(&page)?
    } else {
        serde_json::to_string(&page)?
    };
    let result = writeln!(cx.io.stdout, "{text}");
    Ok(written(cx, result))
}

/// The service part of the status document for `control`.
fn service_state(control: Option<&Path>) -> Result<crate::contract::ServiceState, ErrorLine> {
    use crate::contract::ServiceState;
    let Some(control) = control else {
        return Ok(ServiceState::Unknown);
    };
    let instance = match crate::service::request(control, ControlMethod::Health, VERSION) {
        Ok(ControlResponse::Health { observation }) => {
            let instance = observation.instance;
            (
                instance,
                serde_json::to_value(&observation).unwrap_or_default(),
            )
        }
        Ok(other) => {
            return Ok(ServiceState::Incompatible {
                code: crate::service::refusal(&other).code,
            });
        }
        Err(line) if line.class == ExitClass::ServiceMissing => {
            return Ok(ServiceState::NotRunning);
        }
        Err(line) if line.class == ExitClass::IncompatiblePair => {
            return Ok(ServiceState::Incompatible { code: line.code });
        }
        Err(line) => return Err(line),
    };
    let (instance, health) = instance;
    match crate::service::request_fenced(
        control,
        ControlMethod::LifecycleStatus,
        VERSION,
        Some(instance),
    )? {
        ControlResponse::LifecycleStatus {
            explicitly_stopped,
            pending,
        } => Ok(ServiceState::Running {
            health,
            explicitly_stopped,
            pending_lifecycle: pending.and_then(|record| serde_json::to_value(record).ok()),
        }),
        other => Err(crate::service::refusal(&other)),
    }
}

/// `service status` (U87; step 3b.1f-3): one report, exit 0 unless the
/// service could not be asked.
fn status_service(cx: &mut Context, selection: Selection, runtime_dir: Option<PathBuf>) -> u8 {
    use crate::contract::{STATUS_SCHEMA, ServiceState, ServiceStatus};
    let discovery = crate::install::discover();
    let (installation, _) = installation_of(&discovery);
    let control = runtime_dir.or(selection.control).or(match &discovery {
        crate::install::Discovery::Verified(verified) => {
            Some(PathBuf::from(&verified.record.locations.state))
        }
        _ => None,
    });
    let service = match service_state(control.as_deref()) {
        Ok(service) => service,
        Err(line) => return fail(cx, &line),
    };
    let status = ServiceStatus {
        schema: STATUS_SCHEMA.into(),
        installation,
        service,
    };
    let result = match cx.output {
        Output::Json => match serde_json::to_string(&status) {
            Ok(text) => writeln!(cx.io.stdout, "{text}"),
            Err(_) => return ExitClass::Internal.code(),
        },
        Output::Text => {
            let installation = match status.installation.state {
                InstallationState::Complete => "complete",
                InstallationState::Incomplete => "incomplete",
                InstallationState::NotFound => "not found",
                InstallationState::Unverified => "unverified",
            };
            let service = match &status.service {
                ServiceState::Unknown => {
                    "unknown (no control directory; pass --runtime-dir)".to_owned()
                }
                ServiceState::NotRunning => "not running".to_owned(),
                ServiceState::Incompatible { code } => format!("incompatible ({code})"),
                ServiceState::Running {
                    health,
                    explicitly_stopped,
                    pending_lifecycle,
                } => format!(
                    "running (instance {}, version {})\nsaved stop: {}\npending lifecycle: {}",
                    health["instance"].as_str().unwrap_or_default(),
                    health["version"].as_str().unwrap_or_default(),
                    if *explicitly_stopped { "yes" } else { "no" },
                    match pending_lifecycle {
                        Some(record) => format!(
                            "{} {}",
                            record["request"]["action"].as_str().unwrap_or_default(),
                            record["phase"].as_str().unwrap_or_default()
                        ),
                        None => "none".to_owned(),
                    }
                ),
            };
            writeln!(
                cx.io.stdout,
                "installation: {installation}\nservice: {service}"
            )
        }
    };
    written(cx, result)
}

/// `service record-error` (M14; U75; step 3b.2), printing `{"recorded":true}`
/// as `airk service record-error` did.
fn record_error_service(
    cx: &mut Context,
    selection: Selection,
    runtime_dir: Option<PathBuf>,
    operation: String,
) -> u8 {
    let input = match crate::record::bounded(&mut cx.io.stdin) {
        Ok(input) => input,
        Err(line) => return fail(cx, &line),
    };
    let explicit = runtime_dir.or(selection.control);
    let mut verified = None;
    let control = match explicit {
        Some(control) => control,
        None => match installed(cx) {
            Ok(installation) => {
                let state = PathBuf::from(&installation.record.locations.state);
                verified = Some(installation);
                state
            }
            Err(code) => return code,
        },
    };
    match crate::record::by_service(&control, &operation, &input, VERSION) {
        Ok(crate::record::Recorded::ByService) => {}
        Ok(crate::record::Recorded::ServiceDown) => {
            let verified = match verified {
                Some(verified) => verified,
                None => match installed(cx) {
                    Ok(verified) => verified,
                    Err(code) => return code,
                },
            };
            if let Err(line) = crate::record::by_role(&verified.airk, &control, &operation, &input)
            {
                return fail(cx, &line);
            }
        }
        Err(line) => return fail(cx, &line),
    }
    let result = writeln!(cx.io.stdout, "{}", serde_json::json!({"recorded": true}));
    written(cx, result)
}

fn service(cx: &mut Context, mut selection: Selection, action: ServiceCommand) -> u8 {
    let (runtime_dir, shutdown) = match action {
        ServiceCommand::RecordError {
            runtime_dir,
            operation,
        } => {
            return record_error_service(cx, selection, runtime_dir, operation);
        }
        ServiceCommand::Status { runtime_dir } => {
            return status_service(cx, selection, runtime_dir);
        }
        ServiceCommand::Logs {
            runtime_dir,
            limit,
            all,
            raw,
            output_file,
            format,
        } => {
            return logs_service(
                cx,
                selection,
                LogOptions {
                    runtime_dir,
                    limit,
                    all,
                    raw,
                    output_file,
                    format,
                },
            );
        }
        ServiceCommand::Start { runtime_dir } => {
            return start_service(cx, selection, runtime_dir);
        }
        ServiceCommand::Stop { runtime_dir } => {
            return lifecycle_service(
                cx,
                selection,
                runtime_dir,
                airk_core::service_lifecycle::LifecycleAction::Stop,
            );
        }
        ServiceCommand::Restart { runtime_dir } => {
            return lifecycle_service(
                cx,
                selection,
                runtime_dir,
                airk_core::service_lifecycle::LifecycleAction::Restart,
            );
        }
        ServiceCommand::Health { runtime_dir } => (runtime_dir, false),
        ServiceCommand::Shutdown { runtime_dir } => (runtime_dir, true),
    };
    if runtime_dir.is_some() {
        selection.control = runtime_dir;
    }
    let control = match control(cx, &selection) {
        Ok(control) => control,
        Err(code) => return code,
    };
    let health = match crate::service::request(&control, ControlMethod::Health, VERSION) {
        Ok(response @ ControlResponse::Health { .. }) => response,
        Ok(other) => return fail(cx, &crate::service::refusal(&other)),
        Err(line) => return fail(cx, &line),
    };
    let ControlResponse::Health { observation } = &health else {
        unreachable!("matched above")
    };
    if !shutdown {
        return print_compact(cx, &health);
    }
    let response = match crate::service::request_fenced(
        &control,
        ControlMethod::Shutdown,
        VERSION,
        Some(observation.instance),
    ) {
        Ok(response) => response,
        Err(mut line) => {
            // The request may have reached the service before the transport
            // failed: its outcome is unknown, never retried.
            if line.class == ExitClass::Internal {
                line.class = ExitClass::OutcomeUnknown;
                line.code = "service_request_outcome_unknown".into();
            }
            return fail(cx, &line);
        }
    };
    let printed = print_compact(cx, &response);
    match &response {
        ControlResponse::Failure { code } => {
            let mut line = crate::service::refusal(&response);
            line.message = format!("native request refused: {code}");
            fail(cx, &line)
        }
        ControlResponse::Incompatible { .. } => {
            let mut line = crate::service::refusal(&response);
            line.message = "native request protocol is incompatible".into();
            fail(cx, &line)
        }
        _ => printed,
    }
}

fn dispatch(cx: &mut Context, command: Command, selection: Selection) -> u8 {
    match command {
        Command::Version => version(cx),
        Command::Service { action } => service(cx, selection, action),
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
        root: cli.root,
        data_dir: cli.data_dir,
        collection: cli.collection,
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
