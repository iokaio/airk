// SPDX-License-Identifier: Apache-2.0
//! The 1.4 CLI is a native service client. Source/store ownership stays in workers.
use airk_cli::cli_io::CliIo;

use airk_cli::standalone_setup::SetupPrompt;
#[cfg(feature = "parser-training")]
use anyhow::Context;
use anyhow::{Result, ensure};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;
#[cfg(feature = "parser-training")]
mod parser_training;
#[cfg(any(test, all(windows, feature = "gpu-cuda")))]
mod startup_math;

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
        if let Command::Setup { standalone } = &self.command {
            ensure!(
                standalone.runtime_source.is_none() || self.service_runtime_dir.is_none(),
                "payload installation cannot target an arbitrary control directory"
            );
        }
        Ok(())
    }
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
    /// Standalone installation of a reviewed native payload. Agents connect with
    /// `airkctl agents connect` (the endpoint URL only; no enrollment).
    Setup {
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
#[derive(clap::Args, Default)]
struct StandaloneSetupArgs {
    /// Confirm the explicitly supplied install choices without prompting.
    #[arg(long)]
    yes: bool,
    /// Native payload supplied by a trusted offline installer or explicit distributor policy.
    #[arg(long, requires = "approved_sha256")]
    runtime_source: Option<PathBuf>,
    /// Distributor-pinned install-manifest digest; never inferred from the same payload.
    #[arg(long, requires = "runtime_source")]
    approved_sha256: Option<airk_core::identity::Sha256Digest>,
    /// Choose the native early rebuild option when upgrading an installed predecessor.
    #[arg(long, requires = "runtime_source")]
    rebuild_indexes: bool,
    /// Install the reviewed native payload only (setup no longer enrolls a client; kept so
    /// earlier invocations still parse).
    #[arg(long, requires = "runtime_source")]
    install_only: bool,
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
    #[cfg(all(windows, feature = "gpu-cuda"))]
    // SAFETY: this is the first operation in synchronous main, before run_cli
    // can initialize ORT or create the CLI worker and Tokio runtime.
    unsafe {
        startup_math::initialize_before_workers();
    }
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
        let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
        return dispatch(cli, &CliIo::process()).await;
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
async fn dispatch(cli: Cli, io: &CliIo) -> Result<()> {
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
        Command::Setup { standalone } => run_setup(&cli, standalone, io).await,
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
/// `airk setup`: install the reviewed payload when one is supplied, then
/// name the next step. Setup no longer enrolls a client (local trust plan B).
async fn run_setup(cli: &Cli, options: &StandaloneSetupArgs, io: &CliIo) -> Result<()> {
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
    print_json(
        io,
        &json!({"native_installed":installed_cli.is_some(),"client_enrolled":false,"install_dir":install,
        "cli_executable":installed_cli,"commands":{"setup":["setup"],"uninstall":["uninstall"]},
        "next":"Connect a coding agent with airkctl agents connect --client claude (or codex or vscode): it writes the local endpoint URL, and agents name their scope per call or use a workspace default (airkctl scope default). The executable directory is not added to PATH automatically.",
        "replaced_installation":replaced_installation,"reconnect":reconnect}),
    )
}

#[cfg(test)]
mod standalone_setup_tests;
