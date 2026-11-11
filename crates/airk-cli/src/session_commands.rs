// SPDX-License-Identifier: Apache-2.0
//! The CLI side of `airk service session`: parsing with the complete command
//! tree, the exhaustive in-session allowlist and the one-shot failure rendering.
//!
//! The allowlist is exactly the one-shot commands the VS Code extension runs.
//! Long-lived roles (serve, managed/login serve, foreground, host, mcp, bridge,
//! workers), interactive commands (tools shell, setup, the console) and anything
//! else are refused with `session_command_refused`. A new command is refused
//! until it is reviewed here: every enum is matched exhaustively.
use super::*;
use airk_cli::{
    runtime_install::{LoginCommand, RuntimeCommand},
    session::{Admission, Invoker, Prepared},
};

pub(super) fn run(parent_watch: bool, executable: &Path) -> Result<()> {
    let invoker = CliInvoker {
        program: std::env::args_os()
            .next()
            .unwrap_or_else(|| executable.as_os_str().to_owned()),
        executable: executable.to_path_buf(),
    };
    airk_cli::session::serve(
        std::sync::Arc::new(invoker),
        airk_cli::session::Options {
            parent_watch,
            stack_bytes: CLI_STACK_BYTES,
        },
    )
}

struct CliInvoker {
    /// The session's own argv[0]: parse errors and help name the program as the
    /// one-shot process started from the same image would.
    program: std::ffi::OsString,
    executable: PathBuf,
}

impl Invoker for CliInvoker {
    fn prepare(&self, argv: Vec<String>) -> Option<Prepared> {
        let arguments =
            std::iter::once(self.program.clone()).chain(argv.into_iter().map(Into::into));
        let matches = match Cli::command().try_get_matches_from(arguments) {
            Ok(matches) => matches,
            Err(error) => return Some(complete(&error)),
        };
        let explicit_root =
            matches.value_source("root") == Some(clap::parser::ValueSource::CommandLine);
        let cli = match Cli::from_arg_matches(&matches) {
            Ok(cli) => cli,
            Err(error) => return Some(complete(&error)),
        };
        let admission = admission(&cli)?;
        let executable = self.executable.clone();
        Some(Prepared::Run {
            admission,
            job: Box::new(move |io, cancel| {
                // The same runtime shape as the one-shot process's `run`.
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()?;
                let result = runtime.block_on(io.scope_diagnostics(async {
                    tokio::select! {
                        result = dispatch(cli, explicit_root, &executable, io) => result,
                        () = cancel.cancelled() => Err(anyhow::anyhow!("session request cancelled")),
                    }
                }));
                // As at process exit, abandon detached background work rather
                // than holding this request's slot for it.
                runtime.shutdown_background();
                result
            }),
        })
    }

    fn report_failure(&self, error: &anyhow::Error, io: &CliIo) {
        report_failure(error, io);
    }
}

/// What Clap's `exit()` would have printed, on the stream it would have used.
fn complete(error: &clap::Error) -> Prepared {
    let text = error.render().to_string();
    let success = error.exit_code() == 0;
    if error.use_stderr() {
        Prepared::Complete {
            success,
            stdout: String::new(),
            stderr: text,
        }
    } else {
        Prepared::Complete {
            success,
            stdout: text,
            stderr: String::new(),
        }
    }
}

/// The reviewed in-session allowlist. `None` refuses the command.
pub(super) fn admission(cli: &Cli) -> Option<Admission> {
    use Admission::{Concurrent, Exclusive};
    Some(match &cli.command {
        Command::Runtime { action } => match action {
            // Discovery and read-only observations.
            RuntimeCommand::Location { .. }
            | RuntimeCommand::Inspect { .. }
            | RuntimeCommand::Snapshot { .. }
            | RuntimeCommand::Diagnose { .. }
            | RuntimeCommand::SelfTest
            | RuntimeCommand::InspectCandidate { .. }
            | RuntimeCommand::Inventory { .. }
            | RuntimeCommand::LookupHttpConnection { .. }
            | RuntimeCommand::HttpConnectionInfo { .. }
            | RuntimeCommand::HttpEndpoint { .. }
            | RuntimeCommand::SourceScope { .. }
            | RuntimeCommand::InspectLegacyLaunchers { .. }
            | RuntimeCommand::InspectLegacyCaches { .. } => Concurrent,
            RuntimeCommand::Login { action, .. } => match action {
                LoginCommand::Plan | LoginCommand::Status => Concurrent,
                LoginCommand::Install
                | LoginCommand::Start
                | LoginCommand::Stop
                | LoginCommand::Uninstall => Exclusive,
            },
            // Installer, lifecycle and HTTPS administration.
            RuntimeCommand::Stage { .. }
            | RuntimeCommand::Activate { .. }
            | RuntimeCommand::Managed { .. }
            | RuntimeCommand::Ensure { .. }
            | RuntimeCommand::Start { .. }
            | RuntimeCommand::PrepareClient { .. }
            | RuntimeCommand::ReplaceInstallation { .. }
            | RuntimeCommand::RepairPayload { .. }
            | RuntimeCommand::Lifecycle { .. }
            | RuntimeCommand::AbandonLifecycle { .. }
            | RuntimeCommand::AbortActivation { .. }
            | RuntimeCommand::Purge { .. }
            | RuntimeCommand::HttpConnection { .. }
            | RuntimeCommand::HttpLegacyConnection { .. }
            | RuntimeCommand::HttpLegacyUnionConnection { .. }
            | RuntimeCommand::HttpTrust { .. }
            | RuntimeCommand::HttpIssuerRotation { .. }
            | RuntimeCommand::RetirePrevious { .. }
            | RuntimeCommand::RetireLegacyLaunchers { .. }
            | RuntimeCommand::PrepareLegacyCaches { .. }
            | RuntimeCommand::ReprepareLegacyCaches { .. }
            | RuntimeCommand::RetireLegacyCaches { .. }
            | RuntimeCommand::Uninstall { .. } => Exclusive,
            // Long-lived roles, launchers and commands the extension does not run.
            RuntimeCommand::Foreground { .. }
            | RuntimeCommand::ManagedServe { .. }
            | RuntimeCommand::LoginForeground { .. }
            | RuntimeCommand::LoginServe { .. }
            | RuntimeCommand::ContainerHost { .. }
            | RuntimeCommand::Launch { .. }
            | RuntimeCommand::MacLogin { .. }
            // A bearer token never leaves through a session response; agents
            // started outside VS Code run this helper themselves.
            | RuntimeCommand::HttpCredential { .. }
            | RuntimeCommand::HttpRevoke { .. }
            | RuntimeCommand::AbortLayout { .. }
            | RuntimeCommand::Remove { .. }
            | RuntimeCommand::AbortStage { .. }
            | RuntimeCommand::RepairLauncher { .. }
            | RuntimeCommand::AbortLauncherRepair { .. }
            | RuntimeCommand::ResetStartup { .. }
            | RuntimeCommand::Rollback { .. }
            | RuntimeCommand::AdoptLegacyIndex { .. }
            | RuntimeCommand::AdoptLegacyIndexes { .. }
            | RuntimeCommand::LegacyRetirementCandidates { .. }
            | RuntimeCommand::LegacyRetirementReview { .. }
            | RuntimeCommand::LegacyRetirementAuthorize { .. }
            | RuntimeCommand::LegacyRetirementReceipt { .. }
            | RuntimeCommand::LegacyRetirementResume { .. }
            | RuntimeCommand::CatalogUpgradeProbe { .. }
            | RuntimeCommand::CatalogAuthorityProbe { .. }
            | RuntimeCommand::CatalogUpgradeMigrate { .. } => return None,
        },
        Command::Service { action } => match action {
            ServiceCommand::PrepareRegistration
            | ServiceCommand::Logs { .. }
            | ServiceCommand::Rpc { .. } => Concurrent,
            ServiceCommand::PrepareStorage { .. }
            | ServiceCommand::PrepareConfiguration { .. }
            | ServiceCommand::RecordError { .. } => Exclusive,
            ServiceCommand::Legacy { action } => {
                use airk_cli::legacy_startup::LegacyCommand;
                match action {
                    LegacyCommand::Review { .. }
                    | LegacyCommand::List { .. }
                    | LegacyCommand::Show { .. } => Concurrent,
                    LegacyCommand::RemoveStartup { .. } => Exclusive,
                    LegacyCommand::DerivedReceipts { .. }
                    | LegacyCommand::ForgetDerived { .. }
                    | LegacyCommand::PreviewDerived { .. }
                    | LegacyCommand::RemoveDerived { .. }
                    | LegacyCommand::Forget { .. } => return None,
                }
            }
            ServiceCommand::Console { .. }
            | ServiceCommand::Session { .. }
            | ServiceCommand::Requests { .. }
            | ServiceCommand::Register { .. }
            | ServiceCommand::Health { .. }
            | ServiceCommand::Shutdown { .. } => return None,
        },
        Command::Tools { action } => match action {
            tools::ToolsCommand::Call { .. } => Concurrent,
            tools::ToolsCommand::List { .. }
            | tools::ToolsCommand::Help { .. }
            | tools::ToolsCommand::Shell => return None,
        },
        Command::Usage { action } => match action {
            UsageCommand::Report { .. }
            | UsageCommand::Policy
            | UsageCommand::Import { preview: true, .. } => Concurrent,
            UsageCommand::Import { preview: false, .. }
            | UsageCommand::Clear
            | UsageCommand::Set { .. }
            | UsageCommand::Feedback { .. }
            | UsageCommand::Calibrate { .. } => Exclusive,
        },
        #[cfg(feature = "parser-training")]
        Command::DumpVocab { .. } => return None,
        Command::Parse { .. }
        | Command::Settings { .. }
        | Command::Configure { .. }
        | Command::ProjectWorker
        | Command::DerivedWorker
        | Command::SourceWorker
        | Command::ProcessGuardian
        | Command::Bridge { .. }
        | Command::Serve { .. }
        | Command::ModelCatalog
        | Command::ExplorerAssistance { .. }
        | Command::Changes { .. }
        | Command::CodeFacts { .. }
        | Command::Relationships { .. }
        | Command::Setup { .. }
        | Command::Uninstall { .. }
        | Command::Init
        | Command::Status { .. }
        | Command::Search { .. }
        | Command::Files { .. }
        | Command::Read { .. }
        | Command::Mcp { .. }
        | Command::Info { .. }
        | Command::Outline { .. }
        | Command::Symbol { .. }
        | Command::Imports { .. }
        | Command::Facts { .. }
        | Command::Refs { .. }
        | Command::Context { .. }
        | Command::Index { .. }
        | Command::SetupModel
        | Command::Inspect
        | Command::Pause
        | Command::Resume
        | Command::Watch { .. }
        | Command::Projects { .. }
        | Command::Explorer { .. }
        | Command::Host => return None,
    })
}

#[cfg(test)]
mod tests;
