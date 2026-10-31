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
#[cfg(feature = "parser-training")]
mod parser_training;
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
    /// Developer-only native extraction for the offline intent corpus.
    #[cfg(feature = "parser-training")]
    #[command(hide = true)]
    DumpVocab {
        #[arg(long)]
        output: PathBuf,
    },
    /// Plan, and with --execute run, a multi-step read-only evidence request.
    ///
    /// Give a natural-language request as the TEXT argument, or a structured plan with --request FILE ('-' for stdin).
    /// Without --execute it only prints the plan; same contract as the MCP `parse` tool. Never writes sources.
    Parse {
        text: Option<String>,
        /// Configured native project binding (alias for global --service-project).
        #[arg(long)]
        project: Option<airk_core::service::ProjectBinding>,
        #[arg(long)]
        execute: bool,
        /// Structured request file; '-' reads bounded JSON from stdin.
        #[arg(long, conflicts_with = "text")]
        request: Option<PathBuf>,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Read one project setting view; read-only.
    ///
    /// Views: extraction_policy, vector_selection, vector_status, roots, scan_limits, quota_policy and
    /// quota_status. --validate-file checks a configuration draft without applying it.
    Settings {
        #[arg(value_parser = ["extraction_policy", "vector_selection", "vector_status", "roots", "scan_limits", "quota_policy", "quota_status"])]
        view: String,
        #[arg(long)]
        validate_file: Option<PathBuf>,
    },
    /// Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation").
    ///
    /// Examples: {"operation":"unregister"} deletes the project's index and saved settings (sources untouched);
    /// {"operation":"vectors","enabled":true} turns on semantic search. Prints the accepted job receipt.
    Configure {
        #[arg(long, required_unless_present = "stdin", conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
    },
    /// Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate.
    Usage {
        #[command(subcommand)]
        action: UsageCommand,
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
    #[command(hide = true)]
    ProcessGuardian,
    /// Internal read-only adapter used by trusted launchers.
    Bridge {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    /// Run the AIRK service in the foreground for a private control directory (created if missing).
    ///
    /// It owns the project registry, job queue and worker processes. Stop it with `airk service shutdown`.
    Serve {
        #[arg(long)]
        runtime_dir: PathBuf,
        /// On-demand grace after all clients/workers and runnable jobs are gone.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=3600))]
        idle_exit_seconds: Option<u64>,
    },
    /// Print the offline model catalog and the CPU-based model choice for this machine; opens no project state.
    ModelCatalog,
    /// Start the VS Code extension's Explorer Assistance MCP server (help and UI drafts only).
    #[command(hide = true)]
    ExplorerAssistance {
        /// The extension-written launch descriptor (…/explorer-assistance/launch-1.json).
        #[arg(long)]
        descriptor: PathBuf,
    },
    /// List, describe or call the read-only MCP tools from the terminal.
    ///
    /// `tools list --json`, `tools help <tool>` and `tools call <tool> --args '<json>'`.
    /// Start here to learn every query an agent can make.
    Tools {
        #[command(subcommand)]
        action: tools::ToolsCommand,
    },
    /// Report indexed changes since a revision token: added, modified and removed files, symbols and documents.
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
    /// List code facts (definitions, signatures, durable symbol keys) for a symbol or path, with continuation cursors.
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
    /// Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels.
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
    /// Print the MCP client configuration (command and arguments) for an already registered project.
    Setup {
        #[arg(long,value_enum,default_value_t=SetupClient::Claude)]
        client: SetupClient,
    },
    /// Register --root as a project of the installed runtime and prepare its index storage.
    ///
    /// For a service you started with `airk serve`, use `airk service register --runtime-dir <control>` instead.
    Init,
    /// Print the project's publication, freshness, coverage and job status.
    Status,
    /// Search indexed evidence; results carry file, line and revision citations.
    ///
    /// Without --mode the project's search policy applies, which uses vectors when they are enabled; that
    /// fails until the embedding model is set up (`setup-model`). --mode lexical (all words must appear)
    /// always works on a published index; vector and hybrid need ready vectors.
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
    /// List indexed files under --prefix, with continuation cursors.
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
    /// Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation.
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
    /// Serve the read-only MCP protocol on stdin/stdout for one agent connection (this is what agents launch).
    Mcp {
        /// Optional verified Explorer adapter; native retrieval works without it.
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
        /// Additional explicitly configured project bindings in this same service.
        #[arg(long)]
        additional_project: Vec<airk_core::service::ProjectBinding>,
        /// Additional source folders; registrations are resolved on each request.
        #[arg(long, conflicts_with = "additional_project")]
        additional_root: Vec<PathBuf>,
        /// Choose a collection only when this source has multiple active indexes.
        #[arg(long)]
        index_collection: Option<String>,
        /// Require a project argument for scoped tools in multi-root workspaces.
        #[arg(long)]
        no_default_project: bool,
    },
    /// Show what the index knows about one file: language, size, extraction method and parse status.
    Info {
        path: String,
        #[command(flatten)]
        evidence: EvidenceOptions,
    },
    /// Outline one file's structure: symbols, headings and sections with line ranges.
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
    /// Find symbol definitions by name, optionally filtered by language or path prefix.
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
    /// List import and dependency statements found under a path prefix.
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
    /// List document facts (requirements, dates, decisions, features) extracted from indexed documents.
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
    /// Find indexed references to a name under a path prefix.
    Refs {
        name: String,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Read lines around --line in a file (--before/--after), with a citation.
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
    /// Queue indexing of the project's sources and print the job receipt. Follow it with `tools call airk_status`.
    ///
    /// Full rebuilds (--full) need a guarded maintenance preview through the service instead.
    Index {
        #[arg(long)]
        full: bool,
    },
    /// Prepare the bundled offline embedding model for vector search; keeps an existing or custom selection.
    SetupModel,
    /// Queue a bounded inspection job that refreshes coverage and readiness without reindexing.
    Inspect,
    /// Pause automatic and queued work for the project; existing evidence stays queryable.
    Pause,
    /// Resume work paused with `pause`.
    Resume,
    /// Turn native source watching on or off for the project (on keeps the index current after edits).
    Watch {
        #[arg(value_parser=["on","off"])]
        mode: String,
    },
    /// List the projects registered with the service (same fields as the MCP `projects` tool).
    ///
    /// Uses the installed runtime, or the service you started when --runtime-dir names its control directory.
    Projects {
        /// Control directory of a service started with `airk serve`.
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Explorer Assistance tools without an editor; every result is written to --output.
    ///
    /// Drafts are checked against the same schemas as the MCP tools and written to a new file for
    /// review. Nothing is sent to an Explorer Inbox or executed. get-handoff alone needs a running editor.
    Explorer {
        #[command(subcommand)]
        action: ExplorerCommand,
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
    /// Prepare, list, retry or forget durable client requests (jobs and configuration) for a control directory.
    Requests {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[command(subcommand)]
        action: ClientRequestCommand,
    },
    /// Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only.
    PrepareRegistration,
    /// Create the reserved index storage for an already reserved project (owner-only on Unix).
    PrepareStorage {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    /// Prepare a guarded configuration request for a project; submit it with `requests retry`.
    PrepareConfiguration {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
    },
    /// Persistent JSON management transport on stdin/stdout (used by the VS Code extension).
    Console {
        #[arg(long)]
        runtime_dir: PathBuf,
        /// Pin the approved complete payload identity supplied by the trusted launcher.
        #[arg(long)]
        runtime_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    /// Print recent service log records (--limit), export all retained logs (--all) or the raw job-error ring (--raw).
    Logs {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        limit: usize,
        /// Export retained logs locally (200 MB high-water, FIFO purge to 100 MB).
        #[arg(long, conflicts_with = "limit")]
        all: bool,
        /// Open the separate local raw job-error ring (may contain paths/error values).
        #[arg(long, conflicts_with_all = ["all", "limit", "format"])]
        raw: bool,
        /// Capture raw retained diagnostics into a new local file without buffering stdout.
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long,default_value="json",value_parser=["json","text"])]
        format: String,
    },
    /// Record a bounded local extension/setup error without starting the service.
    RecordError {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        operation: String,
    },
    /// Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist.
    Register {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
    /// Send one raw management request (JSON from --request-file or --stdin) and print the response.
    Rpc {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long, required_unless_present = "stdin", conflicts_with = "stdin")]
        request_file: Option<PathBuf>,
        /// Read bounded management JSON from stdin instead of a request file.
        #[arg(long)]
        stdin: bool,
    },
    /// Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise.
    Health {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
    /// Ask the service for --runtime-dir to drain its workers and exit.
    Shutdown {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
}
#[derive(clap::Args)]
struct ExplorerIo {
    /// Tool arguments as one JSON object, exactly as for the MCP tool of the same name.
    #[arg(long, default_value = "{}")]
    args: String,
    /// New file for the result (must not exist). Results are never sent to an Explorer Inbox.
    #[arg(long)]
    output: PathBuf,
}
#[derive(Subcommand)]
enum ExplorerCommand {
    /// Write a proposed Explorer Search form (MCP draft_search) after schema validation.
    DraftSearch {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Write a proposed Studio strategy (MCP draft_strategy) after schema validation.
    DraftStrategy {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Write proposed Planner tasks (MCP draft_plan) after schema validation.
    DraftPlan {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Write a Navigation, Review or Management suggestion (MCP suggest_view; --args {"kind":…,"draft":…}).
    SuggestView {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Validate a draft without writing it as a proposal (MCP validate_draft; --args {"kind":…,"draft":…}).
    ValidateDraft {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// List the bundled strategy templates (MCP list_templates; optional offset, limit, max_bytes).
    ListTemplates {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Read one strategy template with its operators (MCP get_template; --args {"template_id":…}).
    GetTemplate {
        #[command(flatten)]
        io: ExplorerIo,
    },
    /// Report the status of a draft file written by this CLI (it is never admitted to an Inbox).
    ProposalStatus {
        /// Draft file written by draft-search, draft-strategy, draft-plan or suggest-view.
        #[arg(long)]
        draft: PathBuf,
        /// New file for the result (must not exist).
        #[arg(long)]
        output: PathBuf,
    },
    /// Read a handoff the user shared from a running Explorer window (MCP get_handoff).
    ///
    /// Handoff tokens exist only in that window, so this needs its launch descriptor
    /// (<VS Code global storage>/iokaio.airk/explorer-assistance/launch-1.json).
    GetHandoff {
        /// The ex-share-… token the user copied from Explorer.
        #[arg(long)]
        token: String,
        #[arg(long)]
        explorer_descriptor: PathBuf,
        /// New file for the result (must not exist).
        #[arg(long)]
        output: PathBuf,
    },
}
/// Create `path` exclusively (owner-only on Unix) and write `value` as JSON.
fn write_new(path: &std::path::Path, value: &Value) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).with_context(|| {
        format!(
            "create new output file {} (it must not exist)",
            path.display()
        )
    })?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.write_all(b"\n")?;
    Ok(())
}
async fn explorer_command(cli: &Cli, action: &ExplorerCommand) -> Result<()> {
    use airk_mcp::explorer_offline as offline;
    let arguments = |io: &ExplorerIo| -> Result<serde_json::Map<String, Value>> {
        match serde_json::from_str(&io.args).context("--args must be one JSON object")? {
            Value::Object(map) => Ok(map),
            _ => anyhow::bail!("--args must be one JSON object"),
        }
    };
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?;
    let project = cli.service_project.map(|binding| binding.to_string());
    let (output, value, ok) = match action {
        ExplorerCommand::DraftSearch { io }
        | ExplorerCommand::DraftStrategy { io }
        | ExplorerCommand::DraftPlan { io }
        | ExplorerCommand::SuggestView { io } => {
            let tool = match action {
                ExplorerCommand::DraftSearch { .. } => "draft_search",
                ExplorerCommand::DraftStrategy { .. } => "draft_strategy",
                ExplorerCommand::DraftPlan { .. } => "draft_plan",
                _ => "suggest_view",
            };
            let value = offline::draft(tool, arguments(io)?, project, now)?;
            let ok = value["valid"] == json!(true);
            (&io.output, value, ok)
        }
        ExplorerCommand::ValidateDraft { io } => {
            (&io.output, offline::validate_draft(&arguments(io)?)?, true)
        }
        ExplorerCommand::ListTemplates { io } => {
            (&io.output, offline::list_templates(&arguments(io)?)?, true)
        }
        ExplorerCommand::GetTemplate { io } => {
            (&io.output, offline::get_template(&arguments(io)?)?, true)
        }
        ExplorerCommand::ProposalStatus { draft, output } => {
            let file: Value =
                serde_json::from_slice(&bounded(std::fs::File::open(draft)?, 1 << 20)?)?;
            (output, offline::proposal_status(&file)?, true)
        }
        ExplorerCommand::GetHandoff {
            token,
            explorer_descriptor,
            output,
        } => {
            let descriptor = explorer_descriptor.clone();
            let arguments = json!({"token":token})
                .as_object()
                .cloned()
                .unwrap_or_default();
            let value = tokio::task::spawn_blocking(move || {
                airk_cli::unified_mcp::call_explorer(
                    descriptor,
                    None,
                    None,
                    "airk_explorer_handoff_get".into(),
                    arguments,
                    airk_core::usage::ClientFamily::Cli,
                )
            })
            .await??;
            let ok = value["ok"] != json!(false);
            (output, value, ok)
        }
    };
    write_new(output, &value)?;
    print_json(&json!({"written":output,"ok":ok}))?;
    ensure!(
        ok,
        "result written to {} reports a problem",
        output.display()
    );
    Ok(())
}
async fn list_projects(cli: &Cli, runtime_dir: Option<&std::path::Path>) -> Result<()> {
    let control = match runtime_dir {
        Some(control) => control.to_path_buf(),
        None => airk_cli::runtime_install::installed_control(&install_root(cli)?).await?,
    };
    let (mut after, mut revision, mut projects) = (None, None, Vec::new());
    loop {
        let response = airk_cli::console_client::request(
            &control,
            ControlMethod::Projects {
                after,
                revision,
                limit: 32,
            },
        )
        .await?;
        let airk_core::service::ControlResponse::Projects { page } = response else {
            anyhow::bail!("unexpected service response to a projects page");
        };
        for project in &page.projects {
            let registration = &project.registration;
            projects.push(json!({"project":registration.binding,
                "name":std::path::Path::new(&registration.root).file_name().map(|name| name.to_string_lossy().into_owned()),
                "collection":registration.collection,"root":registration.root,"data_root":registration.data_root}));
        }
        match page.after {
            Some(next) => {
                after = Some(next);
                revision = Some(page.revision);
            }
            None => break,
        }
    }
    print_json(&json!({"default_project":cli.service_project,"projects":projects}))
}
#[derive(Subcommand)]
enum ClientRequestCommand {
    /// Save a configuration request for a project from a draft --file, without submitting it.
    PrepareConfiguration {
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
        #[arg(long)]
        file: PathBuf,
    },
    /// Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain).
    Prepare {
        #[arg(long)]
        project: airk_core::service::ProjectBinding,
        #[arg(long,default_value="index",value_parser=["index","inspect","reconcile","vectors","model_setup","verify","recover","maintain"])]
        kind: String,
    },
    /// List saved client requests and their outcomes.
    List,
    /// Submit, or resubmit after an unknown outcome, a saved request by its nonce.
    Retry {
        nonce: airk_core::identity::Incarnation,
    },
    /// Discard a saved request record by its nonce.
    Forget {
        nonce: airk_core::identity::Incarnation,
    },
}
#[derive(Subcommand)]
enum UsageCommand {
    /// Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file).
    Report {
        #[arg(long, default_value_t = 7)]
        days: u16,
        #[arg(long, default_value = "json", value_parser = ["json", "csv", "markdown"])]
        format: String,
        #[arg(long)]
        price_scenario: Option<PathBuf>,
    },
    /// Show the project's usage recording mode and retention.
    Policy,
    /// Delete recorded usage for the project.
    Clear,
    /// Set the recording mode: disabled, aggregates_only or diagnostic_events.
    #[command(name = "configure", alias = "set")]
    Set {
        #[arg(long, value_parser = ["disabled", "aggregates_only", "diagnostic_events"])]
        mode: String,
    },
    /// Record whether one returned request was useful, partly useful or not useful.
    Feedback {
        request_id: String,
        #[arg(long, value_parser = ["useful", "partly_useful", "not_useful", "partly-useful", "not-useful"])]
        usefulness: String,
    },
    /// Import a usage export; --preview shows what would change and --if-sha256 pins the file.
    Import {
        file: PathBuf,
        #[arg(long)]
        preview: bool,
        #[arg(long)]
        if_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    /// Calibrate token estimates against a tokenizer and baseline/returned samples (advanced).
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
    let mut cli = Cli::parse();
    if let Command::Parse {
        project: Some(project),
        ..
    } = &cli.command
    {
        ensure!(
            cli.service_project
                .as_ref()
                .is_none_or(|selected| selected == project),
            "conflicting project selections"
        );
        cli.service_project = Some(*project);
    }
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
        #[cfg(feature = "parser-training")]
        Command::DumpVocab { output } => {
            return parser_training::dump(
                &cli.root,
                cli.data_dir
                    .as_deref()
                    .context("dump-vocab requires explicit --data-dir outside the source")?,
                output,
            );
        }
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
            additional_root,
            index_collection,
            no_default_project,
        } => {
            if cli.service_project.is_none()
                && cli.data_dir.is_none()
                && cli.service_runtime_dir.is_none()
            {
                anyhow::ensure!(
                    additional_project.is_empty(),
                    "root-based MCP cannot include internal project IDs"
                );
                let (install, control) = airk_cli::service_frontend::root_mcp_control(
                    cli.service_install_dir.as_deref(),
                )?;
                let mut scopes = vec![airk_cli::service_frontend::RootScope::new(
                    &cli.root,
                    index_collection.clone(),
                )?];
                for root in additional_root {
                    scopes.push(airk_cli::service_frontend::RootScope::new(root, None)?);
                }
                return airk_cli::unified_mcp::serve_roots(
                    control,
                    install,
                    scopes,
                    *no_default_project,
                    explorer_descriptor.clone(),
                )
                .await;
            }
            anyhow::ensure!(
                additional_root.is_empty() && index_collection.is_none(),
                "root selectors cannot be combined with a pinned project binding"
            );
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
        Command::Projects { runtime_dir } => {
            return list_projects(&cli, runtime_dir.as_deref()).await;
        }
        Command::Explorer { action } => return explorer_command(&cli, action).await,
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
            only_if_missing: false,
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
        ServiceCommand::RecordError {
            runtime_dir,
            operation,
        } => {
            let input = bounded(std::io::stdin(), 16384)?;
            airk_cli::service_logs::record_client_error(runtime_dir, operation, &input)?;
            print_json(&serde_json::json!({"recorded":true}))
        }
        ServiceCommand::Logs {
            runtime_dir,
            limit,
            all,
            raw,
            output,
            format,
        } => {
            if *raw {
                if let Some(output) = output {
                    let parent = output
                        .parent()
                        .context("diagnostic output parent missing")?;
                    let mut file = tempfile::NamedTempFile::new_in(parent)?;
                    let written =
                        airk_cli::service_logs::export_raw_to(runtime_dir, file.as_file_mut())?;
                    file.as_file().sync_all()?;
                    file.persist_noclobber(output)?;
                    return print_json(&serde_json::json!({"written_bytes":written}));
                }
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
            anyhow::ensure!(
                output.is_none() || *all && format == "json",
                "file capture requires --raw or --all --format json"
            );
            if let Some(output) = output {
                let mut file = tempfile::NamedTempFile::new_in(
                    output
                        .parent()
                        .context("diagnostic output parent missing")?,
                )?;
                serde_json::to_writer(file.as_file_mut(), &page)?;
                file.as_file().sync_all()?;
                let written = file.as_file().metadata()?.len();
                file.persist_noclobber(output)?;
                return print_json(&serde_json::json!({"written_bytes":written}));
            }
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
        Command::Parse {
            text,
            execute,
            request,
            path_prefix,
            ..
        } => {
            let mut value: Value = if let Some(path) = request {
                let mut bytes = Vec::new();
                if path == Path::new("-") {
                    std::io::stdin().take(32769).read_to_end(&mut bytes)?;
                } else {
                    std::fs::File::open(path)?
                        .take(32769)
                        .read_to_end(&mut bytes)?;
                }
                ensure!(bytes.len() <= 32768, "parse request exceeds 32 KiB");
                serde_json::from_slice(&bytes)?
            } else {
                json!({"schema":"airk.parse.request/1","text":text.context("provide text or --request")?,"mode":if execute {"execute"} else {"prepare"},"path_prefix":path_prefix})
            };
            if execute {
                value
                    .as_object_mut()
                    .context("parse request must be an object")?
                    .insert("mode".into(), json!("execute"));
            }
            if !path_prefix.is_empty() {
                value
                    .as_object_mut()
                    .context("parse request must be an object")?
                    .insert("path_prefix".into(), json!(path_prefix));
            }
            ("airk_parse", value)
        }
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
                    && text.contains("--root")
                    && !text.contains("--service-project")
                    && !text.contains("--data-dir")
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
                assert!(args.contains(&json!(project.root)));
                assert!(!args.contains(&json!("--data-dir")));
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

/// docs/cli-reference.md is generated from the parser, so every public command is
/// documented exactly as `--help` prints it. Regenerate with AIRK_WRITE_CLI_REFERENCE=1.
#[cfg(test)]
mod cli_reference_tests {
    use super::*;

    fn reference() -> String {
        // Clap's generated `help` subcommands repeat other entries.
        let catalog: std::collections::BTreeMap<String, String> = cli_help_catalog()
            .into_iter()
            .filter(|(name, _)| !name.split(' ').any(|part| part == "help"))
            .collect();
        let mut text = String::from(
            "# AIRK CLI reference\n\n<!-- Generated from the native parser by `cargo test -p airk-cli --bin airk cli_reference`; do not edit. Set AIRK_WRITE_CLI_REFERENCE=1 to regenerate. -->\n\nEvery public command and subcommand, exactly as `airk <command> --help` prints it. For concepts, workflows and the feature map, see [Running AIRK headless](headless-cli.md) and [the CLI guide](cli.md).\n\n## Contents\n\n",
        );
        for name in catalog.keys() {
            let title = if name.is_empty() {
                "airk".to_owned()
            } else {
                format!("airk {name}")
            };
            let anchor = title.replace(' ', "-");
            text.push_str(&format!("- [`{title}`](#{anchor})\n"));
        }
        for (name, help) in &catalog {
            let title = if name.is_empty() {
                "airk".to_owned()
            } else {
                format!("airk {name}")
            };
            text.push_str(&format!(
                "\n## {title}\n\n```text\n{}\n```\n",
                help.trim_end()
            ));
        }
        text
    }

    #[test]
    fn cli_reference_documents_every_public_command() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-reference.md");
        let expected = reference();
        if std::env::var_os("AIRK_WRITE_CLI_REFERENCE").is_some() {
            std::fs::write(&path, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            actual == expected,
            "docs/cli-reference.md is stale: run AIRK_WRITE_CLI_REFERENCE=1 cargo test -p airk-cli --bin airk cli_reference"
        );
    }

    #[test]
    fn every_public_command_has_help_text() {
        use clap::CommandFactory;
        let mut root = Cli::command();
        root.build();
        fn visit(command: &clap::Command, path: &str, missing: &mut Vec<String>) {
            for child in command
                .get_subcommands()
                .filter(|child| !child.is_hide_set() && child.get_name() != "help")
            {
                let name = format!("{path} {}", child.get_name()).trim().to_owned();
                if child.get_about().is_none()
                    && child.get_long_about().is_none()
                    && child.get_name() != "help"
                {
                    missing.push(name.clone());
                }
                visit(child, &name, missing);
            }
        }
        let mut missing = Vec::new();
        visit(&root, "", &mut missing);
        assert!(
            missing.is_empty(),
            "commands without help text: {missing:?}"
        );
    }
}
