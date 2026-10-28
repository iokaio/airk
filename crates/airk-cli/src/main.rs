// SPDX-License-Identifier: Apache-2.0
//! The 1.4 CLI is a native service client. Source/store ownership stays in workers.
use airk_core::service::{ControlMethod, ControlResponse, JobKind};
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
mod tools;
#[derive(clap::Args, Default, serde::Serialize)]
struct EvidenceOptions {
    #[arg(long)]
    revision: Option<airk_core::identity::RevisionToken>,
    #[arg(long)]
    if_evidence: Option<airk_core::identity::Sha256Digest>,
    #[arg(long)]
    follow_moves: bool,
    #[arg(long)]
    known_hash: Option<airk_core::identity::Sha256Digest>,
    #[arg(long)]
    artifact_id: Option<airk_core::identity::ArtifactId>,
}

#[derive(Parser)]
#[command(
    name = "airk",
    version,
    about = "Artifact Indexing and Retrieval Kernel — native service client"
)]
struct Cli {
    #[arg(long, default_value = ".", global = true)]
    root: PathBuf,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, default_value = "default", global = true)]
    collection: String,
    #[arg(long, global = true)]
    service_install_dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        hide = true,
        requires = "service_project",
        conflicts_with = "service_install_dir"
    )]
    service_runtime_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    service_project: Option<airk_core::service::ProjectBinding>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Read registered project settings through exact bounded direct pages.
    Settings {
        #[arg(value_parser = ["extraction_policy", "vector_selection", "vector_status", "roots", "scan_limits", "quota_policy", "quota_status"])]
        view: String,
        #[arg(long)]
        validate_file: Option<PathBuf>,
    },
    /// Submit a guarded project configuration using a bounded native draft.
    Configure {
        #[arg(long, required_unless_present = "stdin", conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
    },
    /// Collection-scoped reports over the direct native worker connection.
    Usage {
        #[command(subcommand)]
        action: UsageCommand,
    },
    /// Manage an approved offline service runtime.
    Runtime {
        #[command(subcommand)]
        action: airk_cli::runtime_install::RuntimeCommand,
    },
    #[command(hide = true)]
    ProjectWorker,
    #[command(hide = true)]
    DerivedWorker,
    #[command(hide = true)]
    SourceWorker,
    #[command(hide = true)]
    ProcessGuardian,
    /// Read-only service adapter for Bridge.
    Bridge {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    /// Private native control loop for an explicit service runtime.
    Serve {
        #[arg(long)]
        runtime_dir: PathBuf,
        /// On-demand grace after all clients/workers and runnable jobs are gone.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=3600))]
        idle_exit_seconds: Option<u64>,
    },
    /// Read-only service adapter for ModelCatalog.
    ModelCatalog,
    /// Start the VS Code extension's Explorer Assistance MCP server (help and UI drafts only).
    #[command(hide = true)]
    ExplorerAssistance {
        /// The extension-written launch descriptor (…/explorer-assistance/launch-1.json).
        #[arg(long)]
        descriptor: PathBuf,
    },
    /// List, inspect or call the same read-only MCP tools.
    Tools {
        #[command(subcommand)]
        action: tools::ToolsCommand,
    },
    /// Read-only service adapter for Changes.
    Changes {
        #[arg(long)]
        since: Option<airk_core::identity::RevisionToken>,
        /// Repeatable tagged JSON selector, e.g. {"kind":"path","path":"one.rs"}.
        #[arg(long = "selector")]
        selectors: Vec<String>,
        /// Remember an exact collection-relative indexed path.
        #[arg(long = "path")]
        paths: Vec<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
        #[arg(long)]
        include_symbols: bool,
        #[arg(long)]
        include_documents: bool,
        /// Bounded similarity/directory hints; never follow or promote identity.
        #[arg(long)]
        include_suggestions: bool,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Read-only service adapter for CodeFacts.
    CodeFacts {
        /// False includes rich facts and durable IDs; independent of response mode.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        /// Durable symbol key from indexed continuity evidence; excludes textual filters.
        #[arg(long, conflicts_with_all = ["name", "kind", "language", "path_prefix", "cursor"])]
        symbol_key: Option<airk_core::identity::SymbolKey>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read-only service adapter for Relationships.
    Relationships {
        symbol: String,
        #[arg(long, default_value = "incoming")]
        direction: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
        #[arg(long, default_value_t = 1)]
        hops: usize,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Print a stable-launcher MCP configuration for an existing registration.
    Setup {
        #[arg(long,value_enum,default_value_t=SetupClient::Claude)]
        client: SetupClient,
    },
    /// Register canonical roots and prepare reserved service storage.
    Init,
    /// Read the registered project status through the direct client.
    Status,
    /// Read-only service adapter for Search.
    Search {
        query: String,
        #[arg(long, value_enum, conflicts_with = "semantic")]
        mode: Option<SearchMode>,
        /// Use the configured local embedding model and disk-backed vector graph.
        #[arg(long)]
        semantic: bool,
        #[arg(long)]
        path_prefix: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read-only service adapter for Files.
    Files {
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        #[arg(default_value = "")]
        prefix: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read-only service adapter for Read.
    Read {
        #[command(flatten)]
        evidence: EvidenceOptions,
        path: String,
        #[arg(long)]
        start_line: usize,
        #[arg(long)]
        end_line: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Serve read-only MCP through the installed broker and direct project worker.
    Mcp {
        /// Optional verified Explorer adapter; native retrieval works without it.
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
        /// Additional explicitly configured project bindings in this same service.
        #[arg(long)]
        additional_project: Vec<airk_core::service::ProjectBinding>,
        /// Require a project argument for scoped tools in multi-root workspaces.
        #[arg(long)]
        no_default_project: bool,
    },
    /// Read-only service adapter for Info.
    Info {
        path: String,
        #[command(flatten)]
        evidence: EvidenceOptions,
    },
    /// Read-only service adapter for Outline.
    Outline {
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        /// Return minimal names/kinds/lines/signatures with shared original file/provenance bindings.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        #[command(flatten)]
        evidence: EvidenceOptions,
        path: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read-only service adapter for Symbol.
    Symbol {
        /// False includes rich facts and durable IDs; independent of response mode.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        name: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Read-only service adapter for Imports.
    Imports {
        /// False includes rich facts and durable IDs; independent of response mode.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        #[arg(default_value = "")]
        path_prefix: String,
    },
    /// Read-only service adapter for Facts.
    Facts {
        /// False includes rich facts and durable IDs; independent of response mode.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        /// Durable document key; excludes textual filters.
        #[arg(long, conflicts_with_all = ["name", "kind", "path_prefix", "cursor"])]
        document_key: Option<airk_core::identity::DocumentKey>,
        #[arg(default_value = "")]
        path_prefix: String,
        #[arg(long)]
        name: Option<String>,
        /// Exact document fact category, such as requirement, date, architecture or feature.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read-only service adapter for Refs.
    Refs {
        name: String,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Read-only service adapter for Context.
    Context {
        #[command(flatten)]
        evidence: EvidenceOptions,
        path: String,
        line: usize,
        #[arg(long, default_value_t = 5)]
        before: usize,
        #[arg(long, default_value_t = 5)]
        after: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Accept durable indexing work for the registered project.
    Index {
        #[arg(long)]
        full: bool,
    },
    /// Explicit shared bundled-model setup; preserve an existing/custom selection.
    SetupModel,
    /// Accept a bounded native inspection job.
    Inspect,
    Pause,
    Resume,
    Watch {
        #[arg(value_parser=["on","off"])]
        mode: String,
    },
    /// Run the installed service under an explicit container/user supervisor.
    Host,
    /// Bounded native metadata and request recovery; never a project writer.
    Service {
        #[command(subcommand)]
        action: ServiceCommand,
    },
}
#[derive(Subcommand)]
enum ServiceCommand {
    Legacy {
        #[command(subcommand)]
        action: airk_cli::legacy_startup::LegacyCommand,
    },
    Requests {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[command(subcommand)]
        action: ClientRequestCommand,
    },
    PrepareRegistration,
    PrepareStorage {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    PrepareConfiguration {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    Console {
        #[arg(long)]
        runtime_dir: PathBuf,
        /// Pin the approved complete payload identity supplied by the trusted launcher.
        #[arg(long)]
        runtime_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    Logs {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        limit: usize,
        /// Export all retained rotated logs locally (at most four MiB of input).
        #[arg(long, conflicts_with = "limit")]
        all: bool,
        /// Open the separate local raw job-error ring (may contain paths/error values).
        #[arg(long, conflicts_with_all = ["all", "limit", "format"])]
        raw: bool,
        #[arg(long,default_value="json",value_parser=["json","text"])]
        format: String,
    },
    Register {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
    Rpc {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long, required_unless_present = "stdin", conflicts_with = "stdin")]
        request_file: Option<PathBuf>,
        /// Read bounded management JSON from stdin instead of a request file.
        #[arg(long)]
        stdin: bool,
    },
    Health {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
    Shutdown {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
}
#[derive(Subcommand)]
enum ClientRequestCommand {
    PrepareConfiguration {
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
        #[arg(long)]
        file: PathBuf,
    },
    Prepare {
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
        #[arg(long,default_value="index",value_parser=["index","inspect","reconcile","vectors","model_setup","verify","recover","maintain"])]
        kind: String,
    },
    List,
    Retry {
        nonce: airk_core::identity::Incarnation,
    },
    Forget {
        nonce: airk_core::identity::Incarnation,
    },
}
#[derive(Subcommand)]
enum UsageCommand {
    Report {
        #[arg(long, default_value_t = 7)]
        days: u16,
        #[arg(long, default_value = "json", value_parser = ["json", "csv", "markdown"])]
        format: String,
        #[arg(long)]
        price_scenario: Option<PathBuf>,
    },
    Policy,
    Clear,
    #[command(name = "configure", alias = "set")]
    Set {
        #[arg(long, value_parser = ["disabled", "aggregates_only", "diagnostic_events"])]
        mode: String,
    },
    Feedback {
        request_id: String,
        #[arg(long, value_parser = ["useful", "partly_useful", "not_useful", "partly-useful", "not-useful"])]
        usefulness: String,
    },
    Import {
        file: PathBuf,
        #[arg(long)]
        preview: bool,
        #[arg(long)]
        if_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    Calibrate {
        #[arg(long)]
        tokenizer: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        returned: PathBuf,
        #[arg(long)]
        generation: u64,
        #[arg(long)]
        content_kind: String,
        #[arg(long)]
        record: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum SetupClient {
    Claude,
    Codex,
    Vscode,
    Cursor,
    Gemini,
    Cline,
    Roo,
    Junie,
    CopilotCli,
    Opencode,
    OpencodeV2,
    Kilo,
}
#[derive(Clone, Copy, ValueEnum)]
enum SearchMode {
    Lexical,
    Vector,
    Hybrid,
}

fn cli_help_catalog() -> std::collections::BTreeMap<String, String> {
    use clap::CommandFactory;
    fn visit(
        mut command: clap::Command,
        name: String,
        catalog: &mut std::collections::BTreeMap<String, String>,
    ) {
        command = command
            .color(clap::ColorChoice::Never)
            .term_width(100)
            .bin_name(if name.is_empty() {
                "airk".into()
            } else {
                format!("airk {name}")
            });
        catalog.insert(name.clone(), command.render_long_help().to_string());
        for child in command
            .get_subcommands()
            .filter(|child| !child.is_hide_set())
        {
            let path = if name.is_empty() {
                child.get_name().into()
            } else {
                format!("{name} {}", child.get_name())
            };
            visit(child.clone(), path, catalog);
        }
    }
    let mut root = Cli::command();
    root.build();
    let mut catalog = std::collections::BTreeMap::new();
    visit(root, String::new(), &mut catalog);
    catalog
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
fn bounded(input: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "native input exceeds byte budget"
    );
    Ok(bytes)
}
fn main() -> std::process::ExitCode {
    match run_cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if std::env::var("AIRK_ERROR_FORMAT").as_deref() == Ok("json") {
                eprintln!(
                    "{}",
                    json!({"airk_error":1,"code":"operation_failed","retryable":false,"message":format!("{error:#}").chars().take(4096).collect::<String>()})
                );
            } else {
                eprintln!("Error: {error:#}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
fn run_cli() -> Result<()> {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "process-guardian")
    {
        ensure!(
            std::env::args_os().count() == 2,
            "invalid guardian invocation"
        );
        #[cfg(unix)]
        return airk_cli::unix_guardian::serve();
        #[cfg(not(unix))]
        anyhow::bail!("provider guardians require Unix");
    }
    if airk_kernel::dispatch_pdf_worker()? {
        return Ok(());
    }
    std::thread::Builder::new()
        .name("airk-cli".into())
        .stack_size(8 << 20)
        .spawn(run)?
        .join()
        .map_err(|_| anyhow::anyhow!("AIRK CLI thread panicked"))?
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn run() -> Result<()> {
    airk_mcp::help::install_cli_help(cli_help_catalog);
    let executable = std::env::current_exe()?;
    // The universal launcher forwards before parsing, so arguments added by later
    // versions reach the installation that understands them.
    if airk_cli::universal_launcher::is_universal(&executable) {
        return airk_cli::universal_launcher::run(
            &executable.canonicalize()?,
            std::env::args_os().skip(1).collect(),
        );
    }
    let cli = Cli::parse();
    if !matches!(cli.command, Command::Runtime { .. })
        && executable.file_name()
            == Some(std::ffi::OsStr::new(if cfg!(windows) {
                "airk-launcher.exe"
            } else {
                "airk-launcher"
            }))
    {
        return airk_cli::runtime_install::run(
            &airk_cli::runtime_install::RuntimeCommand::Launch {
                install_dir: airk_cli::runtime_install::caller_install_root()?,
                args: std::env::args_os().skip(1).collect(),
            },
        )
        .await;
    }
    match &cli.command {
        Command::Runtime { action } => return airk_cli::runtime_install::run(action).await,
        // Hands stdio to the extension's server; prints nothing and never contacts the service.
        Command::ExplorerAssistance { descriptor } => {
            return airk_cli::explorer_assistance::run(descriptor);
        }
        Command::ProjectWorker => return airk_cli::project_worker::run().await,
        Command::DerivedWorker => return airk_cli::derived_worker::run(),
        Command::SourceWorker => return airk_cli::source_worker::run(),
        Command::ProcessGuardian => {
            anyhow::bail!("guardian requires its standalone private invocation")
        }
        Command::Bridge {
            runtime_dir,
            project,
        } => return airk_cli::project_client::bridge(runtime_dir.clone(), *project).await,
        Command::Serve {
            runtime_dir,
            idle_exit_seconds,
        } => {
            return airk_cli::service_runtime::serve_with_idle(
                runtime_dir,
                idle_exit_seconds.map(std::time::Duration::from_secs),
            )
            .await;
        }
        Command::Service { action } => return service(&cli, action).await,
        Command::ModelCatalog => {
            let mut report = serde_json::to_value(airk_kernel::inspect_model_catalog()?)?;
            report["bundled_model"] = json!({"manifest":airk_cli::bundled_assets::manifest()?});
            return print_json(&report);
        }
        Command::Tools { action } if tools::inspect(action)? => return Ok(()),
        Command::Mcp {
            explorer_descriptor,
            additional_project,
            no_default_project,
        } => {
            let project = airk_cli::service_frontend::resolve_mcp(
                cli.service_install_dir.as_deref(),
                cli.service_runtime_dir.as_deref(),
                cli.service_project,
                &cli.root,
                cli.data_dir.as_deref(),
                &cli.collection,
            )?;
            return airk_cli::unified_mcp::serve(
                project.control,
                project.install,
                project.registration,
                additional_project.clone(),
                *no_default_project,
                explorer_descriptor.clone(),
            )
            .await;
        }
        Command::Host => {
            return airk_cli::runtime_install::run(
                &airk_cli::runtime_install::RuntimeCommand::Foreground {
                    install_dir: install_root(&cli)?,
                },
            )
            .await;
        }
        Command::Init => return initialize(&cli).await,
        Command::Index { full: true } => anyhow::bail!(
            "full rebuilds require a guarded maintenance preview and separate approval; use Service Console or typed service RPC"
        ),
        _ => (),
    }
    let project = if let Some(control) = &cli.service_runtime_dir {
        airk_cli::service_frontend::resolve_control(
            control,
            cli.service_project.context("project binding required")?,
        )
        .await?
    } else {
        airk_cli::service_frontend::resolve(
            cli.service_install_dir.as_deref(),
            cli.service_project,
            &cli.root,
            cli.data_dir.as_deref(),
            &cli.collection,
        )
        .await?
    };
    let registration = &project.registration;
    match cli.command {
        Command::Settings {
            view,
            validate_file,
        } => {
            let view = serde_json::from_value(json!(view))?;
            let value = match validate_file {
                Some(file) => {
                    airk_cli::usage_client::validate_settings(
                        &project.control,
                        registration.binding,
                        view,
                        airk_cli::service_configuration::input(&file, 131_072)?,
                    )
                    .await?
                }
                None => {
                    airk_cli::usage_client::settings(&project.control, registration.binding, view)
                        .await?
                }
            };
            print_json(&value)
        }
        Command::Configure { file, stdin: _ } => {
            let bytes = match file {
                Some(file) => bounded(
                    std::fs::File::open(file)?,
                    airk_core::service_configuration::MAX_CONFIGURATION_BYTES as u64,
                )?,
                None => bounded(
                    std::io::stdin(),
                    airk_core::service_configuration::MAX_CONFIGURATION_BYTES as u64,
                )?,
            };
            configure(
                &project.control,
                registration.binding,
                serde_json::from_slice(&bytes)?,
            )
            .await
        }
        Command::Usage { action } => match action {
            UsageCommand::Report {
                days,
                format,
                price_scenario,
            } => {
                let price = price_scenario
                    .map(|file| -> Result<airk_core::usage::PriceScenario> {
                        let price: airk_core::usage::PriceScenario =
                            serde_json::from_slice(&bounded(std::fs::File::open(file)?, 16_384)?)?;
                        price.validate()?;
                        Ok(price)
                    })
                    .transpose()?;
                let value = airk_cli::usage_client::render(
                    &project.control,
                    registration.binding,
                    days,
                    serde_json::from_value(json!(format))?,
                    price,
                )
                .await?;
                if let Some(text) = value.as_str() {
                    print!("{text}");
                    Ok(())
                } else {
                    print_json(&value)
                }
            }
            UsageCommand::Policy => {
                let mut connection = airk_cli::project_client::Connection::connect(
                    &project.control,
                    registration.binding,
                )
                .await?;
                print_json(
                    &connection
                        .read_native(airk_mcp::local::NativeRead::UsagePolicy)
                        .await?,
                )
            }
            UsageCommand::Import {
                file,
                preview: true,
                ..
            } => {
                use sha2::Digest;
                let bytes = bounded(std::fs::File::open(file)?, 65_536)?;
                let input_sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
                let measurement: airk_core::usage::SupplementalMeasurement =
                    serde_json::from_slice(&bytes)?;
                measurement.validate()?;
                ensure!(
                    measurement.provenance
                        != airk_core::usage::MeasurementProvenance::LocalTokenizer,
                    "local-tokenizer provenance requires usage calibrate"
                );
                print_json(
                    &json!({"preview":measurement,"input_sha256":input_sha256,"collection":registration.collection,"recorded":false,"link_validation":"Native execution verifies retained request links and collection scope."}),
                )
            }
            action => {
                use airk_cli::service_configuration::Draft;
                let draft = match action {
                    UsageCommand::Clear => Draft::ClearUsage,
                    UsageCommand::Set { mode } => Draft::Usage {
                        mode: serde_json::from_value(json!(mode))?,
                    },
                    UsageCommand::Feedback {
                        request_id,
                        usefulness,
                    } => Draft::UsageFeedback {
                        request_id,
                        usefulness: serde_json::from_value(json!(usefulness.replace('-', "_")))?,
                    },
                    UsageCommand::Import {
                        file, if_sha256, ..
                    } => Draft::ImportUsage {
                        path: file,
                        expected_sha256: if_sha256,
                    },
                    UsageCommand::Calibrate {
                        tokenizer,
                        baseline,
                        returned,
                        generation,
                        content_kind,
                        record,
                    } => Draft::Calibrate {
                        tokenizer,
                        baseline,
                        returned,
                        generation,
                        content_kind: serde_json::from_value(json!(content_kind))?,
                        record,
                    },
                    _ => unreachable!(),
                };
                configure(&project.control, registration.binding, draft).await
            }
        },
        Command::Mcp { .. } => unreachable!("MCP resolves saved bindings before service election"),
        Command::Tools { action } => {
            tools::run_remote(
                action,
                project.control,
                registration.binding,
                Path::new(&registration.root),
                Path::new(&registration.data_root),
                &registration.collection,
            )
            .await
        }
        Command::Setup { client } => {
            let install = project
                .install
                .as_deref()
                .context("agent setup requires an installed stable launcher")?;
            let command = airk_cli::runtime_install::verified_launcher(install)?;
            println!(
                "{}",
                configuration(&command, install, registration, client)?
            );
            Ok(())
        }
        Command::Index { .. } => job(&project.control, registration.binding, JobKind::Index).await,
        Command::SetupModel => {
            job(&project.control, registration.binding, JobKind::ModelSetup).await
        }
        Command::Inspect => job(&project.control, registration.binding, JobKind::Inspect).await,
        Command::Pause | Command::Resume => print_control(
            airk_cli::console_client::request(
                &project.control,
                ControlMethod::Pause {
                    project: registration.binding,
                    paused: matches!(cli.command, Command::Pause),
                },
            )
            .await?,
        ),
        Command::Watch { mode } => print_control(
            airk_cli::console_client::request(
                &project.control,
                ControlMethod::ConfigureWatch {
                    project: registration.binding,
                    enabled: mode == "on",
                },
            )
            .await?,
        ),
        command => {
            let (name, arguments) = query(command)?;
            tools::run_remote(
                tools::ToolsCommand::Call {
                    tool: name.into(),
                    args: Some(serde_json::to_string(&arguments)?),
                    args_file: None,
                },
                project.control,
                registration.binding,
                Path::new(&registration.root),
                Path::new(&registration.data_root),
                &registration.collection,
            )
            .await
        }
    }
}
fn install_root(cli: &Cli) -> Result<PathBuf> {
    match &cli.service_install_dir {
        Some(root) => Ok(root.clone()),
        None => airk_cli::runtime_install::caller_install_root(),
    }
}
async fn initialize(cli: &Cli) -> Result<()> {
    ensure!(
        cli.service_runtime_dir.is_none(),
        "raw service registration requires explicit service metadata commands"
    );
    let install = install_root(cli)?;
    let control = airk_cli::runtime_install::installed_control(&install).await?;
    let data = match &cli.data_dir {
        Some(data) => data.clone(),
        None => airk_cli::runtime_install::default_project_data(&install)?,
    };
    let registration = airk_cli::service_registration::prepare(&cli.root, &data, &cli.collection)?;
    ensure!(
        cli.service_project
            .is_none_or(|binding| binding == registration.binding),
        "requested project binding differs from canonical registration"
    );
    let response = airk_cli::console_client::request(
        &control,
        ControlMethod::ReserveProject {
            project: registration.clone(),
            setup_watch: None,
        },
    )
    .await?;
    let ControlResponse::Registered { project } = response else {
        print_control(response)?;
        anyhow::bail!("registration was not acknowledged")
    };
    ensure!(
        project.registration == registration,
        "crossed native registration"
    );
    ensure!(
        airk_cli::service_registration::prepare_storage(&control, registration.binding)?
            == registration,
        "changed prepared storage identity"
    );
    print_json(&project)
}
async fn job(
    control: &Path,
    project: airk_core::service::ProjectBinding,
    kind: JobKind,
) -> Result<()> {
    let nonce = airk_cli::client_requests::prepare(control, project, kind).await?;
    eprintln!(
        "Saved CLI request {nonce}; use 'airk service requests --runtime-dir <control> list/retry' for uncertain outcomes."
    );
    print_json(&airk_cli::client_requests::submit(control, nonce).await?)?;
    airk_cli::client_requests::forget(control, nonce)
}
async fn configure(
    control: &Path,
    project: airk_core::service::ProjectBinding,
    draft: airk_cli::service_configuration::Draft,
) -> Result<()> {
    use airk_core::service::JobOutcome;
    let nonce = airk_cli::client_requests::prepare(control, project, JobKind::Inspect).await?;
    let ControlResponse::Job { job } = airk_cli::client_requests::submit(control, nonce).await?
    else {
        anyhow::bail!("configuration inspection not acknowledged")
    };
    airk_cli::client_requests::forget(control, nonce)?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let ControlResponse::Job { job } =
            airk_cli::console_client::request(control, ControlMethod::Job { id: job.id }).await?
        else {
            anyhow::bail!("configuration inspection receipt unavailable")
        };
        if let Some(outcome) = job.outcome {
            ensure!(
                matches!(outcome, JobOutcome::Succeeded { .. }),
                "configuration inspection failed: {outcome:?}"
            );
            break;
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "configuration inspection is still pending: {}",
            job.id
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let nonce = airk_cli::client_requests::prepare_configuration(control, project, draft).await?;
    eprintln!(
        "Saved CLI configuration request {nonce}; retry only with its saved key after uncertain outcomes."
    );
    print_json(&airk_cli::client_requests::submit(control, nonce).await?)?;
    airk_cli::client_requests::forget(control, nonce)
}
fn print_control(response: ControlResponse) -> Result<()> {
    print_json(&response)?;
    if let ControlResponse::Failure { code } = response {
        anyhow::bail!("native request refused: {code}");
    }
    Ok(())
}
async fn service(cli: &Cli, action: &ServiceCommand) -> Result<()> {
    match action {
        ServiceCommand::Legacy { action } => airk_cli::legacy_startup::run(action),
        ServiceCommand::Requests {
            runtime_dir,
            action,
        } => match action {
            ClientRequestCommand::PrepareConfiguration { project, file } => {
                let draft = serde_json::from_slice(&bounded(
                    std::fs::File::open(file)?,
                    airk_core::service_configuration::MAX_CONFIGURATION_BYTES as u64,
                )?)?;
                let nonce =
                    airk_cli::client_requests::prepare_configuration(runtime_dir, *project, draft)
                        .await?;
                print_json(&json!({"prepared":nonce,"submitted":false}))
            }
            ClientRequestCommand::List => {
                print_json(&airk_cli::client_requests::list(runtime_dir)?)
            }
            ClientRequestCommand::Prepare { project, kind } => {
                let kind = serde_json::from_value(json!(kind))?;
                let nonce = airk_cli::client_requests::prepare(runtime_dir, *project, kind).await?;
                print_json(&json!({"prepared":nonce,"submitted":false}))
            }
            ClientRequestCommand::Retry { nonce } => {
                print_json(&airk_cli::client_requests::submit(runtime_dir, *nonce).await?)?;
                airk_cli::client_requests::forget(runtime_dir, *nonce)
            }
            ClientRequestCommand::Forget { nonce } => {
                airk_cli::client_requests::forget(runtime_dir, *nonce)?;
                print_json(&json!({"forgotten":nonce,"native_work_canceled":false}))
            }
        },
        ServiceCommand::PrepareRegistration => {
            print_json(&airk_cli::service_registration::prepare(
                &cli.root,
                cli.data_dir
                    .as_deref()
                    .context("registration preparation requires explicit --data-dir")?,
                &cli.collection,
            )?)
        }
        ServiceCommand::PrepareStorage {
            runtime_dir,
            project,
        } => print_json(&airk_cli::service_registration::prepare_storage(
            runtime_dir,
            *project,
        )?),
        ServiceCommand::PrepareConfiguration {
            runtime_dir,
            project,
        } => {
            let draft = serde_json::from_slice(&bounded(
                std::io::stdin(),
                airk_core::service_configuration::MAX_CONFIGURATION_BYTES as u64,
            )?)?;
            print_json(
                &airk_cli::service_configuration::prepare(runtime_dir, *project, draft).await?,
            )
        }
        ServiceCommand::Console {
            runtime_dir,
            runtime_sha256,
        } => airk_cli::console_client::run(runtime_dir, *runtime_sha256).await,
        ServiceCommand::Logs {
            runtime_dir,
            limit,
            all,
            raw,
            format,
        } => {
            if *raw {
                std::io::Write::write_all(
                    &mut std::io::stdout().lock(),
                    &airk_cli::service_logs::export_raw(runtime_dir)?,
                )?;
                return Ok(());
            }
            anyhow::ensure!(
                !all || format == "json",
                "full log export requires JSON format"
            );
            let page = if *all {
                airk_cli::service_logs::export(runtime_dir)?
            } else {
                airk_cli::service_logs::recent(runtime_dir, *limit)?
            };
            if format == "text" {
                println!("{}", airk_cli::service_logs::render(&page)?);
                Ok(())
            } else {
                print_json(&page)
            }
        }
        ServiceCommand::Register { runtime_dir } => {
            let project = airk_kernel::service_projects::registration(
                &cli.root,
                cli.data_dir
                    .as_deref()
                    .context("service registration requires explicit --data-dir")?,
                &cli.collection,
            )?;
            print_control(
                airk_cli::console_client::request(runtime_dir, ControlMethod::Register { project })
                    .await?,
            )
        }
        ServiceCommand::Rpc {
            runtime_dir,
            request_file,
            stdin,
        } => {
            let bytes = if *stdin {
                bounded(std::io::stdin(), airk_core::service::MAX_FRAME_BYTES as u64)?
            } else {
                bounded(
                    std::fs::File::open(request_file.as_ref().context("request file required")?)?,
                    airk_core::service::MAX_FRAME_BYTES as u64,
                )?
            };
            let method: ControlMethod = serde_json::from_slice(&bytes)?;
            ensure!(
                !matches!(method, ControlMethod::Connect { .. }),
                "metadata RPC cannot expose evidence credentials; use mcp or tools"
            );
            // Transport success and native application status are distinct in this raw metadata API.
            print_json(&airk_cli::console_client::request(runtime_dir, method).await?)
        }
        ServiceCommand::Health { runtime_dir } => {
            let response =
                airk_cli::console_client::request(runtime_dir, ControlMethod::Health).await?;
            ensure!(
                matches!(response, ControlResponse::Health { .. }),
                "service health unavailable or incompatible"
            );
            print_json(&response)
        }
        ServiceCommand::Shutdown { runtime_dir } => print_control(
            airk_cli::console_client::request(runtime_dir, ControlMethod::Shutdown).await?,
        ),
    }
}
fn evidence(mut value: Value, evidence: EvidenceOptions) -> Result<Value> {
    value
        .as_object_mut()
        .context("tool arguments missing")?
        .extend(
            serde_json::to_value(evidence)?
                .as_object()
                .context("evidence options missing")?
                .clone(),
        );
    Ok(value)
}
fn query(command: Command) -> Result<(&'static str, Value)> {
    Ok(match command {
        Command::Status => ("airk_status", json!({})),
        Command::Search {
            query,
            mode,
            semantic,
            path_prefix,
            limit,
            max_bytes,
        } => {
            let mode = if semantic {
                Some("vector")
            } else {
                mode.map(|mode| match mode {
                    SearchMode::Lexical => "lexical",
                    SearchMode::Vector => "vector",
                    SearchMode::Hybrid => "hybrid",
                })
            };
            (
                "airk_search",
                json!({"query":query,"mode":mode,"path_prefix":path_prefix,"limit":limit,"max_bytes":max_bytes}),
            )
        }
        Command::Files {
            cursor,
            prefix,
            limit,
            max_bytes,
        } => (
            "airk_list_files",
            json!({"cursor":cursor,"prefix":prefix,"limit":limit,"max_bytes":max_bytes}),
        ),
        Command::Read {
            evidence: options,
            path,
            start_line,
            end_line,
            max_bytes,
        } => (
            "airk_read_file",
            evidence(
                json!({"path":path,"start_line":start_line,"end_line":end_line,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        Command::Info {
            path,
            evidence: options,
        } => ("airk_file_info", evidence(json!({"path":path}), options)?),
        Command::Outline {
            cursor,
            compact,
            evidence: options,
            path,
            limit,
            max_bytes,
        } => (
            "airk_outline",
            evidence(
                json!({"path":path,"cursor":cursor,"compact":compact,"limit":limit,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        Command::Symbol {
            compact,
            cursor,
            name,
            language,
            path_prefix,
        } => (
            "airk_symbols",
            json!({"compact":compact,"cursor":cursor,"name":name,"language":language,"path_prefix":path_prefix}),
        ),
        Command::Imports {
            compact,
            cursor,
            path_prefix,
        } => (
            "airk_imports",
            json!({"compact":compact,"cursor":cursor,"path_prefix":path_prefix}),
        ),
        Command::Facts {
            compact,
            cursor,
            document_key,
            path_prefix,
            name,
            kind,
            limit,
            max_bytes,
        } => (
            "airk_document_facts",
            json!({"compact":compact,"cursor":cursor,"document_key":document_key,"path_prefix":path_prefix,"name":name,"kind":kind,"limit":limit,"max_bytes":max_bytes}),
        ),
        Command::CodeFacts {
            compact,
            cursor,
            symbol_key,
            path_prefix,
            name,
            kind,
            language,
            limit,
            max_bytes,
        } => (
            "airk_code_facts",
            json!({"compact":compact,"cursor":cursor,"symbol_key":symbol_key,"path_prefix":path_prefix,"name":name,"kind":kind,"language":language,"limit":limit,"max_bytes":max_bytes}),
        ),
        Command::Relationships {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
        } => (
            "airk_relationships",
            json!({"symbol":symbol,"direction":direction,"kind":kind,"path_prefix":path_prefix,"hops":hops,"limit":limit,"max_bytes":max_bytes}),
        ),
        Command::Refs { name, path_prefix } => (
            "airk_references",
            json!({"name":name,"path_prefix":path_prefix}),
        ),
        Command::Context {
            evidence: options,
            path,
            line,
            before,
            after,
            max_bytes,
        } => (
            "airk_context",
            evidence(
                json!({"path":path,"line":line,"before":before,"after":after,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        Command::Changes {
            since,
            selectors,
            paths,
            path_prefix,
            include_symbols,
            include_documents,
            include_suggestions,
            limit,
            max_bytes,
            cursor,
        } => {
            let mut selectors = selectors
                .into_iter()
                .map(|value| serde_json::from_str::<Value>(&value))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            selectors.extend(
                paths
                    .into_iter()
                    .map(|path| json!({"kind":"path","path":path})),
            );
            (
                "airk_changes",
                json!({"since":since,"selectors":selectors,"path_prefix":path_prefix,"include_symbols":include_symbols,"include_documents":include_documents,"include_suggestions":include_suggestions,"limit":limit,"max_bytes":max_bytes,"cursor":cursor}),
            )
        }
        _ => anyhow::bail!("not a read-only tool alias"),
    })
}
fn configuration(
    command: &Path,
    install: &Path,
    project: &airk_core::service::ProjectRegistration,
    client: SetupClient,
) -> Result<String> {
    let command = command.to_str().context("launcher path is not UTF-8")?;
    let args = vec![
        "runtime",
        "launch",
        "--install-dir",
        install.to_str().context("installation path is not UTF-8")?,
        "--",
        "--root",
        &project.root,
        "--data-dir",
        &project.data_root,
        "--collection",
        &project.collection,
        "--service-project",
        &project.binding.to_string(),
        "mcp",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let name = "airk";
    let mut server = json!({"command":command,"args":args});
    let result = match client {
        SetupClient::Codex => {
            return Ok(format!(
                "[mcp_servers.{name}]\ncommand = {}\nargs = {}",
                serde_json::to_string(command)?,
                serde_json::to_string(&args)?
            ));
        }
        SetupClient::Vscode => {
            server["type"] = json!("stdio");
            json!({"servers":{name:server}})
        }
        SetupClient::Cursor => {
            server["type"] = json!("stdio");
            json!({"mcpServers":{name:server}})
        }
        SetupClient::CopilotCli => {
            server["type"] = json!("local");
            server["tools"] = json!(["*"]);
            json!({"mcpServers":{name:server}})
        }
        SetupClient::Opencode | SetupClient::OpencodeV2 | SetupClient::Kilo => {
            let mut full = vec![command.to_owned()];
            full.extend(args);
            let entry = json!({"type":"local","command":full});
            if matches!(client, SetupClient::OpencodeV2) {
                json!({"mcp":{"servers":{name:entry}}})
            } else {
                json!({"mcp":{name:entry}})
            }
        }
        _ => json!({"mcpServers":{name:server}}),
    };
    Ok(serde_json::to_string_pretty(&result)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_formats_preserve_stable_launcher_and_opaque_scope() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let project =
            airk_kernel::service_projects::registration(root.path(), data.path(), "setup-fixture")?;
        let launcher = Path::new("/approved install/airk-launcher");
        let install = Path::new("/approved install");
        for client in [
            SetupClient::Claude,
            SetupClient::Codex,
            SetupClient::Vscode,
            SetupClient::Cursor,
            SetupClient::Opencode,
            SetupClient::OpencodeV2,
            SetupClient::CopilotCli,
            SetupClient::Kilo,
        ] {
            let text = configuration(launcher, install, &project, client)?;
            assert!(
                text.contains("airk")
                    && text.contains("--service-project")
                    && text.contains(&project.binding.to_string())
            );
            if matches!(client, SetupClient::Codex) {
                assert!(text.starts_with("[mcp_servers.airk]"));
            } else {
                let value: Value = serde_json::from_str(&text)?;
                let entry = match client {
                    SetupClient::Vscode => &value["servers"]["airk"],
                    SetupClient::Opencode | SetupClient::Kilo => &value["mcp"]["airk"],
                    SetupClient::OpencodeV2 => &value["mcp"]["servers"]["airk"],
                    _ => &value["mcpServers"]["airk"],
                };
                let args = entry
                    .get("args")
                    .unwrap_or(&entry["command"])
                    .as_array()
                    .context("separate launcher arguments")?;
                assert_eq!(args.last(), Some(&json!("mcp")));
                for path in [&project.root, &project.data_root] {
                    assert!(args.contains(&json!(path)));
                }
            }
        }
        Ok(())
    }
    #[test]
    fn read_aliases_use_the_actual_mcp_argument_contracts() -> Result<()> {
        macro_rules! check {
            ($kind:ty,$args:expr) => {{
                let cli = Cli::try_parse_from($args)?;
                let (_, arguments) = query(cli.command)?;
                serde_json::from_value::<$kind>(arguments)?;
            }};
        }
        check!(airk_mcp::SearchArgs, ["airk", "search", "fixture"]);
        check!(
            airk_mcp::ReadArgs,
            [
                "airk",
                "read",
                "memo.txt",
                "--start-line",
                "1",
                "--end-line",
                "2"
            ]
        );
        check!(airk_mcp::PathArgs, ["airk", "info", "memo.txt"]);
        check!(airk_mcp::OutlineArgs, ["airk", "outline", "memo.txt"]);
        check!(airk_mcp::FactArgs, ["airk", "symbol", "fixture"]);
        check!(airk_mcp::FactArgs, ["airk", "imports"]);
        check!(airk_mcp::DocumentFactArgs, ["airk", "facts"]);
        check!(airk_mcp::CodeFactArgs, ["airk", "code-facts"]);
        check!(
            airk_mcp::RelationshipArgs,
            ["airk", "relationships", "fixture"]
        );
        check!(airk_mcp::ReferenceArgs, ["airk", "refs", "fixture"]);
        check!(airk_mcp::ContextArgs, ["airk", "context", "memo.txt", "1"]);
        check!(airk_mcp::ListArgs, ["airk", "files"]);
        check!(
            airk_mcp::ChangesArgs,
            ["airk", "changes", "--path", "memo.txt"]
        );
        Ok(())
    }
    #[test]
    fn continuity_keys_keep_exact_identity_and_exclude_text_filters() {
        let key = "01".repeat(16);
        for (command, selector) in [("code-facts", "--symbol-key"), ("facts", "--document-key")] {
            assert!(Cli::try_parse_from(["airk", command, selector, &key]).is_ok());
            assert!(Cli::try_parse_from(["airk", command, selector, "short"]).is_err());
            assert!(
                Cli::try_parse_from(["airk", command, selector, &key, "--name", "original"])
                    .is_err()
            );
        }
    }
    #[test]
    fn retired_direct_writers_are_not_public_entry_points() {
        for args in [
            vec!["airk", "schedule", "run"],
            vec![
                "airk",
                "collections",
                "vectors",
                "set",
                "--enabled",
                "false",
            ],
            vec!["airk", "storage", "relocate", "elsewhere"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
    #[test]
    fn opaque_service_bindings_and_native_metadata_are_independent_of_live_paths() {
        let key = "ab".repeat(32);
        assert!(Cli::try_parse_from(["airk", "--service-project", &key, "mcp"]).is_ok());
        assert!(
            Cli::try_parse_from(["airk", "--root", "/missing/source", "tools", "list"]).is_ok()
        );
        assert!(Cli::try_parse_from(["airk", "--service-project", "bad", "mcp"]).is_err());
    }
}
