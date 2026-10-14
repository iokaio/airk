// SPDX-License-Identifier: Apache-2.0
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_kernel::{FactQuery, Kernel};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "airk",
    version,
    about = "Artifact Indexing and Retrieval Kernel"
)]
struct Cli {
    #[arg(long, default_value = ".", global = true)]
    root: PathBuf,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, default_value = "default", global = true)]
    collection: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Optional producer-side semantic binding and revision-checked import.
    Analysis {
        #[command(subcommand)]
        action: AnalysisCommand,
    },
    /// Query enriched code syntax and imported facts without broad source text.
    CodeFacts {
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
    /// Bounded incoming/outgoing relationships; syntax edges remain candidates.
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
    /// Manage per-user disk usage; never deletes source workspace files.
    Collections {
        #[command(subcommand)]
        action: CollectionCommand,
    },
    /// Keep one container host running for collection-scoped docker-exec workers.
    Host,
    /// Index the workspace and print a ready-to-merge native MCP configuration.
    Setup {
        #[arg(long,value_enum,default_value_t=SetupClient::Claude)]
        client: SetupClient,
    },
    /// Initialize local derived state without indexing sources.
    Init,
    /// Incrementally index eligible UTF-8 files; report exclusions.
    Index {
        path: Option<PathBuf>,
        /// Re-extract every eligible file while preserving the prior index on failure.
        #[arg(long)]
        full: bool,
    },
    /// Show the active snapshot and counts.
    Status,
    /// Retrieve bounded lexical evidence as JSON.
    Search {
        query: String,
        #[arg(long)]
        path_prefix: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// List indexed paths by prefix.
    Files {
        #[arg(default_value = "")]
        prefix: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read indexed source lines, with inclusive 1-based bounds.
    Read {
        path: String,
        #[arg(long)]
        start_line: usize,
        #[arg(long)]
        end_line: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Serve read-only MCP tools over stdio.
    Mcp,
    /// Inspect format and extraction quality for an indexed file.
    Info { path: String },
    /// Retrieve a file's native syntax/document outline.
    Outline {
        path: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Find exact native syntax definitions.
    Symbol {
        name: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Retrieve native import declarations.
    Imports {
        #[arg(default_value = "")]
        path_prefix: String,
    },
    /// Retrieve structured document/configuration facts.
    Facts {
        #[arg(default_value = "")]
        path_prefix: String,
        #[arg(long)]
        name: Option<String>,
        /// Exact document fact category, such as requirement, date, architecture or feature.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Find textual identifier occurrences; no semantic name resolution.
    Refs {
        name: String,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Retrieve neighboring indexed lines.
    Context {
        path: String,
        line: usize,
        #[arg(long, default_value_t = 5)]
        before: usize,
        #[arg(long, default_value_t = 5)]
        after: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
}

#[derive(Subcommand)]
enum AnalysisCommand {
    /// Print manifest BEFORE running producer; keep it outside indexed sources.
    Prepare {
        #[arg(long)]
        producer: String,
        #[arg(long)]
        producer_version: String,
        #[arg(long, required = true)]
        build_input: Vec<String>,
    },
    /// Validate and transactionally import SCIP plus producer-completed manifest.
    Import {
        index: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[derive(Subcommand)]
enum CollectionCommand {
    /// List collections and actual derived-state disk bytes.
    List,
    /// Remove an explicitly named managed collection; retains source files.
    Remove { id: String },
}

fn print_json(value: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
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

fn print_client_config(kernel: &Kernel, client: SetupClient) -> anyhow::Result<()> {
    let command = std::env::current_exe()?
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("executable is not UTF-8"))?
        .to_owned();
    let root = kernel
        .root()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("root is not UTF-8"))?
        .to_owned();
    let data = kernel
        .data_dir()
        .canonicalize()?
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("data directory is not UTF-8"))?
        .to_owned();
    let args = vec![
        "--root".to_owned(),
        root,
        "--data-dir".to_owned(),
        data,
        "--collection".to_owned(),
        kernel.collection().into(),
        "mcp".into(),
    ];
    let mut server = serde_json::json!({"command":command,"args":args});
    let config = match client {
        SetupClient::Codex => {
            println!(
                "[mcp_servers.airk]\ncommand = {}\nargs = {}",
                serde_json::to_string(&command)?,
                serde_json::to_string(&args)?
            );
            return Ok(());
        }
        SetupClient::Vscode => {
            server["type"] = serde_json::json!("stdio");
            serde_json::json!({"servers":{"airk":server}})
        }
        SetupClient::Cursor => {
            server["type"] = serde_json::json!("stdio");
            serde_json::json!({"mcpServers":{"airk":server}})
        }
        SetupClient::CopilotCli => {
            server["type"] = serde_json::json!("local");
            server["tools"] = serde_json::json!(["*"]);
            serde_json::json!({"mcpServers":{"airk":server}})
        }
        SetupClient::Opencode | SetupClient::OpencodeV2 | SetupClient::Kilo => {
            let mut full_command = vec![command];
            full_command.extend(args);
            let server = serde_json::json!({"type":"local","command":full_command});
            if matches!(client, SetupClient::OpencodeV2) {
                serde_json::json!({"mcp":{"servers":{"airk":server}}})
            } else {
                serde_json::json!({"mcp":{"airk":server}})
            }
        }
        _ => serde_json::json!({"mcpServers":{"airk":server}}),
    };
    print_json(&config)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let root = match &cli.command {
        Command::Index {
            path: Some(path), ..
        } => path.clone(),
        _ => cli.root,
    };
    let kernel = Kernel::open_collection(root, cli.data_dir, &cli.collection)?;
    match cli.command {
        Command::Analysis { action } => match action {
            AnalysisCommand::Prepare {
                producer,
                producer_version,
                build_input,
            } => {
                print_json(&kernel.prepare_analysis(&producer, &producer_version, &build_input)?)
            }
            AnalysisCommand::Import { index, manifest } => {
                print_json(&kernel.import_scip(&index, &manifest)?)
            }
        },
        Command::CodeFacts {
            name,
            kind,
            language,
            path_prefix,
            limit,
            max_bytes,
        } => print_json(&kernel.code_facts(&FactQuery {
            path_prefix,
            name,
            kind,
            language,
            exact_name: true,
            limit,
            max_bytes,
            path_exact: false,
            document_only: false,
        })?),
        Command::Relationships {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
        } => print_json(&kernel.relationships(&RelationshipQuery {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
        })?),
        Command::Collections { action } => match action {
            CollectionCommand::List => print_json(&kernel.collections()?),
            CollectionCommand::Remove { id } => print_json(&kernel.remove_collection(&id)?),
        },
        Command::Host => {
            eprintln!(
                "AIRK container host ready; launch collection-scoped workers with docker exec"
            );
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                tokio::select! { result=tokio::signal::ctrl_c() => result?, _=terminate.recv() => {} }
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await?;
            Ok(())
        }
        Command::Setup { client } => {
            let report = kernel.index()?;
            eprintln!(
                "Indexed {} changed files, reused {}, removed {}; {} skipped",
                report.indexed,
                report.reused,
                report.removed,
                report.skipped.len()
            );
            print_client_config(&kernel, client)
        }
        Command::Init => print_json(&kernel.initialize()?),
        Command::Index { full, .. } => print_json(&if full {
            kernel.reindex()?
        } else {
            kernel.index()?
        }),
        Command::Status => print_json(&kernel.status()?),
        Command::Search {
            query,
            path_prefix,
            limit,
            max_bytes,
        } => print_json(&kernel.search(&SearchQuery {
            text: query,
            path_prefix,
            limit,
            max_bytes,
        })?),
        Command::Files {
            prefix,
            limit,
            max_bytes,
        } => print_json(&kernel.list_files(&prefix, limit, max_bytes)?),
        Command::Read {
            path,
            start_line,
            end_line,
            max_bytes,
        } => print_json(&kernel.read_file(&path, start_line, end_line, max_bytes)?),
        Command::Mcp => airk_mcp::serve(kernel).await,
        Command::Info { path } => print_json(&kernel.file_info(&path)?),
        Command::Outline {
            path,
            limit,
            max_bytes,
        } => print_json(&kernel.outline(&path, limit, max_bytes)?),
        Command::Symbol {
            name,
            language,
            path_prefix,
        } => print_json(&kernel.facts(&FactQuery {
            path_prefix,
            name: Some(name),
            kind: Some("definition".into()),
            language,
            exact_name: true,
            limit: 50,
            max_bytes: 8192,
            path_exact: false,
            document_only: false,
        })?),
        Command::Imports { path_prefix } => print_json(&kernel.facts(&FactQuery {
            path_prefix,
            name: None,
            kind: Some("import".into()),
            language: None,
            exact_name: false,
            limit: 50,
            max_bytes: 8192,
            path_exact: false,
            document_only: false,
        })?),
        Command::Facts {
            path_prefix,
            name,
            kind,
        } => print_json(&kernel.facts(&FactQuery {
            path_prefix,
            name,
            kind,
            language: None,
            exact_name: false,
            limit: 50,
            max_bytes: 8192,
            path_exact: false,
            document_only: true,
        })?),
        Command::Refs { name, path_prefix } => {
            print_json(&kernel.references(&name, &path_prefix, 50, 8192)?)
        }
        Command::Context {
            path,
            line,
            before,
            after,
            max_bytes,
        } => print_json(&kernel.context(&path, line, before, after, max_bytes)?),
    }
}
