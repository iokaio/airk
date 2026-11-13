// SPDX-License-Identifier: Apache-2.0
//! The 1.4 CLI is a native service client. Source/store ownership stays in workers.
use airk_cli::cli_io::CliIo;

use anyhow::{Context, Result, ensure};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
#[cfg(feature = "parser-training")]
mod parser_training;

#[derive(Parser)]
#[command(
    name = "airk",
    version,
    about = "Artifact Indexing and Retrieval Kernel — native service client",
    long_about = "Artifact Indexing and Retrieval Kernel: local, read-only code and document retrieval through a native service. Run `airk --help` (not -h) for the full guide.",
    after_long_help = include_str!("help_guide.txt")
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
    /// Explicit reviewed single-index session JSON; preferences alone grant no access.
    #[arg(long, global = true, value_parser = parse_catalog_session,
        conflicts_with_all = ["service_project", "root", "data_dir", "collection"])]
    catalog_session: Option<airk_core::service::ScopedConnectRequest>,
    /// Frozen reviewed multi-Project session JSON; indexes are executed once each.
    #[arg(long, global = true, value_parser = parse_combined_session,
        conflicts_with_all = ["catalog_session", "service_project", "root", "data_dir", "collection"])]
    combined_session: Option<airk_core::service::CombinedConnectRequest>,
    /// Narrow a combined read to explicit Projects from its reviewed session.
    #[arg(
        long,
        global = true,
        value_delimiter = ',',
        num_args = 1,
        requires = "combined_session"
    )]
    project_ids: Option<Vec<airk_core::index_catalog::QueryProjectId>>,
    /// Narrow a combined read to member indexes; requires explicit Project scope.
    #[arg(long, global = true, value_delimiter = ',', num_args = 1, requires_all = ["combined_session", "project_ids"])]
    index_ids: Option<Vec<airk_core::index_catalog::IndexId>>,
    /// Report member failures or require every selected member to answer.
    #[arg(long, global = true, value_parser = ["report", "require"], requires_all = ["combined_session", "project_ids"])]
    member_availability: Option<String>,
    #[arg(
        long,
        global = true,
        hide = true,
        conflicts_with = "service_install_dir"
    )]
    service_runtime_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    service_project: Option<airk_core::service::ProjectBinding>,
    #[command(subcommand)]
    command: Command,
}
impl Cli {
    /// Clap validates subcommands before propagating parent-level global values.
    /// Check this cross-level relationship only after the complete CLI is merged.
    fn validate_setup(&self) -> Result<()> {
        if let Command::Setup {
            standalone,
            existing_connection_id,
            explorer_descriptor,
            ..
        } = &self.command
        {
            let guided_scope = !standalone.projects.is_empty()
                || !standalone.indexes.is_empty()
                || standalone.create_root.is_some();
            ensure!(
                existing_connection_id.is_none()
                    || !guided_scope && standalone.runtime_source.is_none(),
                "existing connection cannot be combined with guided enrollment inputs"
            );
            ensure!(
                self.catalog_session.is_none() && self.combined_session.is_none()
                    || !guided_scope && standalone.runtime_source.is_none(),
                "choose a frozen reviewed session or guided setup inputs, not both"
            );
            ensure!(
                standalone.runtime_source.is_none() || self.service_runtime_dir.is_none(),
                "payload installation cannot target an arbitrary control directory"
            );
            ensure!(
                !standalone.launch || self.service_runtime_dir.is_none(),
                "client launch requires the verified installation, not a nominated control directory"
            );
            ensure!(
                !guided_scope || explorer_descriptor.is_none(),
                "standalone setup does not attach an editor Assistance descriptor"
            );
        }
        if let Command::Setup {
            existing_connection_id: Some(_),
            connection_id,
            explorer_descriptor,
            ..
        } = &self.command
        {
            ensure!(
                self.service_install_dir.is_some(),
                "existing connection requires --service-install-dir"
            );
            ensure!(
                connection_id.is_none()
                    && explorer_descriptor.is_none()
                    && self.catalog_session.is_none()
                    && self.combined_session.is_none()
                    && self.service_runtime_dir.is_none(),
                "existing connection cannot be combined with enrollment inputs"
            );
        }
        Ok(())
    }
}
fn parse_catalog_session(
    value: &str,
) -> std::result::Result<airk_core::service::ScopedConnectRequest, String> {
    if value.len() > 4096 {
        return Err("catalog session exceeds its byte allowance".into());
    }
    let selection: airk_core::service::ScopedConnectRequest =
        serde_json::from_str(value).map_err(|_| "invalid catalog session JSON".to_owned())?;
    selection
        .validate()
        .map_err(|_| "invalid reviewed catalog session".to_owned())?;
    Ok(selection)
}
fn parse_combined_session(
    value: &str,
) -> std::result::Result<airk_core::service::CombinedConnectRequest, String> {
    if value.len() > 8192 {
        return Err("combined session exceeds its byte allowance".into());
    }
    let selection: airk_core::service::CombinedConnectRequest =
        serde_json::from_str(value).map_err(|_| "invalid combined session JSON".to_owned())?;
    selection
        .validate()
        .map_err(|_| "invalid reviewed combined session".to_owned())?;
    Ok(selection)
}
#[derive(Subcommand)]
enum Command {
    /// Developer-only native extraction for the offline intent corpus.
    #[cfg(feature = "parser-training")]
    #[command(hide = true)]
    DumpVocab {
        #[arg(long)]
        output: PathBuf,
    },
    /// Installed-runtime lifecycle: location, inspect, ensure, start, login, uninstall, inventory, purge and repair.
    ///
    /// The VS Code extension uses these; headless experiments can instead run `airk serve` from a local build.
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
    /// Guided standalone installation, index selection and coding-agent MCP setup.
    Setup {
        #[arg(long, value_enum)]
        client: Option<SetupClient>,
        /// Reuse the exact connection ID when retrying enrollment.
        #[arg(long)]
        connection_id: Option<airk_core::identity::Incarnation>,
        /// Render an existing credential without enrollment or changing its authority.
        #[arg(long, conflicts_with_all = ["connection_id", "explorer_descriptor", "catalog_session", "combined_session", "service_runtime_dir"])]
        existing_connection_id: Option<airk_core::identity::Incarnation>,
        /// Exact private explorer-assistance/windows/<session>.json attachment.
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
        #[command(flatten)]
        standalone: StandaloneSetupArgs,
    },
    /// Uninstall the native per-user service and owned runtime files; preserve source files and indexes.
    Uninstall {
        /// Explicitly confirm uninstall without an interactive prompt.
        #[arg(long)]
        yes: bool,
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
    /// Review and retire pre-1.4 startup entries and derived state (explicit, previewed).
    Legacy {
        #[command(subcommand)]
        action: airk_cli::legacy_startup::LegacyCommand,
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
#[derive(clap::Args, Default)]
struct StandaloneSetupArgs {
    /// Select existing index sets by native ID; repeat or separate IDs with commas.
    #[arg(
        long = "project",
        value_delimiter = ',',
        conflicts_with = "create_root"
    )]
    projects: Vec<airk_core::index_catalog::QueryProjectId>,
    /// Select indexes, or narrow explicitly selected index sets to these indexes.
    #[arg(long = "index", value_delimiter = ',', conflicts_with = "create_root")]
    indexes: Vec<airk_core::index_catalog::IndexId>,
    /// Review and create an index from this source folder through the native setup owner.
    #[arg(long)]
    create_root: Option<PathBuf>,
    /// Storage for the explicitly created index; otherwise use the native installation's default.
    #[arg(long, requires = "create_root")]
    storage: Option<PathBuf>,
    #[arg(long, requires = "create_root")]
    name: Option<String>,
    /// Watch the explicitly created source folder; defaults to off.
    #[arg(long, requires = "create_root")]
    watch: bool,
    /// Confirm the explicitly supplied install and scope choices without prompting.
    #[arg(long)]
    yes: bool,
    /// Native payload supplied by a trusted offline installer or explicit distributor policy.
    #[arg(
        long,
        requires = "approved_sha256",
        conflicts_with = "existing_connection_id"
    )]
    runtime_source: Option<PathBuf>,
    /// Distributor-pinned install-manifest digest; never inferred from the same payload.
    #[arg(long, requires = "runtime_source")]
    approved_sha256: Option<airk_core::identity::Sha256Digest>,
    /// Choose the native early rebuild option when upgrading an installed predecessor.
    #[arg(long, requires = "runtime_source")]
    rebuild_indexes: bool,
    /// Install the reviewed native payload and leave client enrollment for a later setup invocation.
    #[arg(long,requires="runtime_source",conflicts_with_all=["projects","indexes","create_root","launch"])]
    install_only: bool,
    /// Launch the selected client with protected credentials and additive trust in its process environment.
    #[arg(long)]
    launch: bool,
    /// Additional foreground client arguments, passed directly without a shell string.
    #[arg(last = true, requires = "launch")]
    client_args: Vec<std::ffi::OsString>,
}

fn print_json(io: &CliIo, value: &impl serde::Serialize) -> Result<()> {
    io.line(serde_json::to_string(value)?);
    Ok(())
}
const CLI_STACK_BYTES: usize = 8 << 20;

/// The setup tests exercise the parser and help under the CLI's stack size;
/// libtest's default thread stack is smaller than the complete debug derive.
#[cfg(test)]
fn on_cli_stack<T: Send + 'static>(test: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name("airk-cli-test".into())
        .stack_size(CLI_STACK_BYTES)
        .spawn(test)
        .expect("start CLI test thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn main() -> std::process::ExitCode {
    match run_cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error_line(&error));
            std::process::ExitCode::FAILURE
        }
    }
}
/// The final diagnostic line of a failed command (`AIRK_ERROR_FORMAT=json`
/// selects the typed body). Shared by the process and `service session`.
fn error_line(error: &anyhow::Error) -> String {
    if std::env::var("AIRK_ERROR_FORMAT").as_deref() == Ok("json") {
        json!({"airk_error":1,"code":"operation_failed","retryable":false,"message":format!("{error:#}").chars().take(4096).collect::<String>()}).to_string()
    } else {
        format!("Error: {error:#}")
    }
}
fn run_cli() -> Result<()> {
    if std::env::args_os()
        .skip(1)
        .eq([std::ffi::OsStr::new("--verify-model-smoke")])
    {
        #[cfg(all(feature = "local-inference", feature = "bundled-model"))]
        {
            println!("{}", airk_cli::bundled_assets::verify_model_smoke()?);
            return Ok(());
        }
        #[cfg(not(all(feature = "local-inference", feature = "bundled-model")))]
        anyhow::bail!("this build does not include bundled inference model verification");
    }
    if std::env::args_os()
        .skip(1)
        .eq([std::ffi::OsStr::new("--verify-bundled-models")])
    {
        #[cfg(feature = "local-inference")]
        {
            println!("{}", airk_cli::bundled_assets::verify_inventory()?);
            return Ok(());
        }
        #[cfg(not(feature = "local-inference"))]
        anyhow::bail!("this build does not include bundled inference models");
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "shared-process-guardian")
    {
        ensure!(
            std::env::args_os().count() == 2,
            "invalid shared guardian invocation"
        );
        airk_cli::runtime_pair::require_legacy_inference_process(&std::env::current_exe()?)?;
        #[cfg(unix)]
        return airk_cli::unix_guardian::serve_shared();
        #[cfg(not(unix))]
        anyhow::bail!("shared provider guardians require Unix");
    }
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
    let result = std::thread::Builder::new()
        .name("airk-cli".into())
        .stack_size(CLI_STACK_BYTES)
        .spawn(run)?
        .join()
        .map_err(|_| anyhow::anyhow!("AIRK CLI thread panicked"))?;
    if let Err(error) = &result
        && let Some(native) = error.downcast_ref::<airk_cli::usage_client::NativeReadFailure>()
    {
        print_json(&CliIo::process(), native.body())?;
    }
    result
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn run() -> Result<()> {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // U93: the pre-split operations with Phase 7 or developer owners, reached
    // only behind the private-role selector until those owners land.
    if arguments.get(1).map(std::ffi::OsString::as_os_str)
        == Some(airk_cli::role_admission::SELECTOR.as_ref())
        && arguments.get(2).map(std::ffi::OsString::as_os_str)
            == Some(airk_cli::role_admission::INTERIM.as_ref())
    {
        let line =
            std::iter::once(std::ffi::OsString::from("airk")).chain(arguments[3..].iter().cloned());
        let matches = Cli::command().get_matches_from(line);
        let explicit_root =
            matches.value_source("root") == Some(clap::parser::ValueSource::CommandLine);
        let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
        return dispatch(cli, explicit_root, &CliIo::process()).await;
    }
    // Private roles are admitted before anything else (split plan §5.1).
    if let Some(role) = airk_cli::role_admission::parse(&arguments) {
        return airk_cli::role_admission::run(role?).await;
    }
    // §5.1 (U94): no arguments starts the installed service; any other
    // arguments explain that airkctl is the CLI. Nothing else is parsed.
    let outcome = airk_cli::manual_start::run(&std::env::current_exe()?, arguments.len() > 1);
    if outcome.code == 0 {
        println!("{}", outcome.message);
        return Ok(());
    }
    eprintln!("{}", outcome.message);
    std::process::exit(i32::from(outcome.code));
}
/// The pre-split operations U93 keeps reachable behind the interim selector
/// until their Phase 7 setup-role or developer owners land.
async fn dispatch(cli: Cli, explicit_root: bool, io: &CliIo) -> Result<()> {
    cli.validate_setup()?;
    match &cli.command {
        #[cfg(feature = "parser-training")]
        Command::DumpVocab { output } => parser_training::dump(
            &cli.root,
            cli.data_dir
                .as_deref()
                .context("dump-vocab requires explicit --data-dir outside the source")?,
            output,
        ),
        Command::Runtime { action } => airk_cli::runtime_commands::run_with(action, io).await,
        Command::Setup {
            client,
            connection_id,
            existing_connection_id,
            explorer_descriptor,
            standalone,
        } => {
            run_setup(
                &cli,
                *client,
                *connection_id,
                *existing_connection_id,
                explorer_descriptor.as_deref(),
                standalone,
                explicit_root,
                io,
            )
            .await
        }
        Command::Uninstall { yes } => {
            airk_cli::standalone_setup::uninstall(
                cli.service_install_dir.as_deref(),
                *yes,
                &mut airk_cli::standalone_setup::Terminal,
            )
            .await
        }
        Command::ProjectWorker => airk_cli::run_project_worker().await,
        Command::DerivedWorker => airk_cli::derived_worker::run(),
        Command::SourceWorker => airk_cli::source_worker::run(),
        Command::Host => {
            airk_cli::runtime_commands::run(
                &airk_cli::runtime_install::RuntimeCommand::Foreground {
                    install_dir: install_root(&cli)?,
                },
            )
            .await
        }
        Command::Service {
            action: ServiceCommand::Legacy { action },
        } => airk_cli::legacy_startup::run_with(action, io),
    }
}
fn install_root(cli: &Cli) -> Result<PathBuf> {
    match &cli.service_install_dir {
        Some(root) => Ok(root.clone()),
        None => airk_cli::runtime_install::caller_install_root(),
    }
}
#[allow(clippy::too_many_arguments)] // The parsed setup inputs stay explicit.
async fn run_setup(
    cli: &Cli,
    selected_client: Option<SetupClient>,
    connection_id: Option<airk_core::identity::Incarnation>,
    existing_connection_id: Option<airk_core::identity::Incarnation>,
    explorer_descriptor: Option<&Path>,
    options: &StandaloneSetupArgs,
    explicit_root: bool,
    io: &CliIo,
) -> Result<()> {
    use airk_cli::standalone_setup::{Client as StandaloneClient, SetupPrompt};
    let mut terminal = airk_cli::standalone_setup::Terminal;
    let install = airk_cli::standalone_setup::installation(cli.service_install_dir.as_deref())?;
    let replaced_installation = if let (Some(source), Some(approved)) =
        (&options.runtime_source, options.approved_sha256)
    {
        airk_cli::standalone_setup::install_payload(
            &install,
            source,
            &approved.to_string(),
            options.yes,
            options.rebuild_indexes,
            &mut terminal,
        )
        .await?
    } else {
        None
    };
    let installed_cli = if install.join("selected.json").try_exists()? {
        Some(airk_cli::runtime_install::selected(&install)?.1)
    } else {
        None
    };
    if let Some(executable) = &installed_cli {
        terminal.message(&format!("Installed AIRK executable: {executable:?}"))?;
        terminal.message("Run that executable with setup or uninstall, or add its directory to PATH. Setup does not change shell startup files.")?;
    }
    let reconnect = replaced_installation.as_ref().map(|receipt| {
        airk_cli::standalone_setup::reconnect_instruction(
            &receipt["client_references"],
            installed_cli.as_deref(),
        )
    });
    if let Some(reconnect) = &reconnect {
        terminal.message(reconnect)?;
    }
    if options.install_only {
        return print_json(
            io,
            &json!({"native_installed":true,"client_enrolled":false,"install_dir":install,
            "cli_executable":installed_cli,"commands":{"setup":["setup"],"uninstall":["uninstall"]},"next":"Run the installed executable with setup to review an index scope and connect a coding agent. The executable directory is not added to PATH automatically.",
            "replaced_installation":replaced_installation,"reconnect":reconnect}),
        );
    }
    let guided = existing_connection_id.is_none()
        && cli.catalog_session.is_none()
        && cli.combined_session.is_none();
    let client = match selected_client {
        Some(client) => client,
        None if guided && terminal.interactive() && !options.yes => match terminal
            .answer("Connect [1] Claude Code or [2] Codex? Choice [1]:")?
            .as_str()
        {
            "" | "1" => SetupClient::Claude,
            "2" => SetupClient::Codex,
            _ => anyhow::bail!("select Claude Code or Codex"),
        },
        None => SetupClient::Claude,
    };
    ensure!(
        matches!(
            client,
            SetupClient::Claude | SetupClient::Codex | SetupClient::Vscode
        ),
        "http_client_compatibility_unverified: this client has no qualified HTTPS trust/credential adapter"
    );
    let standalone_client = match client {
        SetupClient::Claude => Some(StandaloneClient::Claude),
        SetupClient::Codex => Some(StandaloneClient::Codex),
        _ => None,
    };
    let mut native_receipts = Vec::new();
    let mut journal_files = Vec::new();
    let mut suggested_directory = explicit_root.then(|| cli.root.clone());
    let connection = if let Some(id) = existing_connection_id {
        serde_json::from_value(
            airk_cli::runtime_commands::existing_http_connection(&install, id).await?,
        )?
    } else if guided {
        let outcome=airk_cli::standalone_setup::enroll(airk_cli::standalone_setup::Options{
            client:standalone_client.context("guided standalone setup supports Claude Code and Codex; the VS Code adapter requires its extension")?,
            install_dir:Some(install.clone()),control_dir:cli.service_runtime_dir.clone(),context_directory:cli.root.clone(),
            project_ids:options.projects.clone(),index_ids:options.indexes.clone(),create_root:options.create_root.clone(),
            data_root:options.storage.clone(),name:options.name.clone(),watch:options.watch,yes:options.yes,connection_id,
        },&mut terminal).await?;
        native_receipts = outcome.native_receipts;
        journal_files = outcome.journal_files;
        if !explicit_root {
            suggested_directory = outcome.suggested_directory;
        }
        outcome.connection
    } else {
        let selection = if let Some(selection) = &cli.catalog_session {
            airk_core::service_http::HttpSelection::Scoped {
                selection: selection.clone(),
            }
        } else {
            airk_core::service_http::HttpSelection::Combined {
                selection: cli
                    .combined_session
                    .clone()
                    .context("reviewed setup selection missing")?,
            }
        };
        let id = connection_id
            .map(Ok)
            .unwrap_or_else(airk_kernel::service_broker::new_instance_id)?;
        let (value, journal) = airk_cli::standalone_setup::enroll_selection(
            cli.service_install_dir.as_deref(),
            cli.service_runtime_dir.as_deref(),
            selection,
            id,
            match client {
                SetupClient::Claude => "claude",
                SetupClient::Codex => "codex",
                _ => "vscode",
            },
            explorer_descriptor,
        )
        .await?;
        journal_files.push(journal);
        native_receipts.push(value.clone());
        serde_json::from_value(value)?
    };
    let (configuration, environment) = http_configuration(&connection, client)?;
    let launch_instructions = standalone_client.map(|client| {
        airk_cli::standalone_setup::launch_instructions(
            client,
            &connection,
            &install,
            suggested_directory.as_deref(),
        )
    });
    let output = json!({"connection":&connection,"configuration":configuration,"environment":environment,
        "native_receipts":native_receipts,"setup_journal_files":journal_files,"cli_executable":installed_cli,
        "launch":launch_instructions,
        "trust":"Use normal CA validation. Native launch adds this installation CA without replacing existing public or enterprise roots; no OS trust change is performed.",
        "credential":"The native launcher reads the protected credential into the child process environment. Token bytes are never printed or placed in project configuration.",
        "command_path":"Use cli_executable as the full command path, or add its directory to PATH yourself; setup does not modify shell startup files.",
        "replaced_installation":replaced_installation,"reconnect":reconnect});
    let launch = options.launch || guided && !options.yes && terminal.interactive();
    if launch {
        let recovery = format!(
            "Connection {} is retained; reuse --existing-connection-id {} with --root <working-folder> --launch",
            connection.connection_id, connection.connection_id
        );
        let directory = match suggested_directory {
            Some(directory) => directory,
            None if terminal.interactive() => PathBuf::from(terminal.answer("Choose the coding agent working folder (required for multiple scopes or an existing connection):")?),
            None => anyhow::bail!("--launch requires explicit --root when no single authorized subtree was selected. {recovery}"),
        };
        let directory = directory
            .canonicalize()
            .with_context(|| format!("working folder is unavailable. {recovery}"))?;
        ensure!(
            directory.is_dir(),
            "working folder is not a directory. {recovery}"
        );
        if !options.launch
            && !terminal
                .answer(&format!(
                    "Launch {} in {} with this scoped AIRK connection? [y/N]",
                    standalone_client
                        .context("native launch client missing")?
                        .label(),
                    directory.display()
                ))?
                .eq_ignore_ascii_case("y")
        {
            return print_json(io, &output);
        }
        terminal.message(&format!(
            "Launching {} in {}",
            standalone_client
                .context("native launch client missing")?
                .label(),
            directory.display()
        ))?;
        terminal.message(&serde_json::to_string_pretty(&output)?)?;
        let status = airk_cli::standalone_setup::launch_client(
            standalone_client.context("native standalone launch supports Claude Code and Codex")?,
            &connection,
            Some(&install),
            &options.client_args,
            &directory,
        )
        .await?;
        if status != 0 {
            std::process::exit(status);
        }
    } else {
        print_json(io, &output)?;
    }
    Ok(())
}

fn http_configuration(
    connection: &airk_core::service_http::HttpConnectionInfo,
    client: SetupClient,
) -> Result<(String, Value)> {
    connection.validate()?;
    let name = format!(
        "AIRK_MCP_TOKEN_{}",
        connection.connection_id.to_string().to_uppercase()
    );
    let url = connection.endpoint.url.to_string();
    let configuration = match client {
        SetupClient::Codex => format!(
            "[mcp_servers.airk]\nurl = {}\nbearer_token_env_var = {}\n[mcp_servers.airk.http_headers]\nX-AIRK-Response-Mode = \"terse\"\n",
            serde_json::to_string(&url)?,
            serde_json::to_string(&name)?,
        ),
        SetupClient::Claude | SetupClient::Vscode => {
            let input_id = format!("airkToken_{}", connection.connection_id);
            let token = if matches!(client, SetupClient::Vscode) {
                format!("Bearer ${{input:{input_id}}}")
            } else {
                format!("Bearer ${{{name}}}")
            };
            let server = json!({"type":"http","url":url,"headers":{"Authorization":token,"X-AIRK-Response-Mode":"terse"}});
            serde_json::to_string_pretty(&if matches!(client, SetupClient::Vscode) {
                json!({"servers":{"airk":server},"inputs":[{
                    "id":input_id,"type":"command","command":"airk.httpCredential",
                    "args":{"connection_id":connection.connection_id}
                }]})
            } else {
                json!({"mcpServers":{"airk":server}})
            })?
        }
        _ => anyhow::bail!("http_client_compatibility_unverified"),
    };
    let trust_variable = match client {
        SetupClient::Codex => Some("CODEX_CA_CERTIFICATE"),
        SetupClient::Claude => Some("NODE_EXTRA_CA_CERTS"),
        _ => None,
    };
    Ok((
        configuration,
        json!({"token_variable":if matches!(client,SetupClient::Vscode){None}else{Some(name)},"token_source_file":connection.credential_file,
        "credential_input_command":if matches!(client,SetupClient::Vscode){Some("airk.httpCredential")}else{None},
        "requires_airk_extension":matches!(client,SetupClient::Vscode),
        "additional_ca_file":connection.ca_certificate_file,"ca_variable":trust_variable,
        "preserve_existing_trust":true,"vscode_uses_os_user_trust":matches!(client,SetupClient::Vscode)}),
    ))
}
#[cfg(test)]
mod http_setup_tests;

#[cfg(test)]
mod standalone_setup_tests;
