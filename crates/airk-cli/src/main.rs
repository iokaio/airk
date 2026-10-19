// SPDX-License-Identifier: Apache-2.0
use airk_core::RelationshipQuery;
use airk_core::SearchQuery;
use airk_core::usage::{ClientFamily, RecordingMode, ResponseRepresentation, UsageOperation};
use airk_kernel::{FactQuery, Kernel};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

mod tools;

#[derive(clap::Args, Default)]
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

impl From<EvidenceOptions> for airk_core::evidence::EvidenceReadOptions {
    fn from(value: EvidenceOptions) -> Self {
        Self {
            revision: value.revision,
            if_evidence: value.if_evidence,
            follow_moves: value.follow_moves,
            known_hash: value.known_hash,
            artifact_id: value.artifact_id,
        }
    }
}

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

#[cfg(test)]
mod continuity_argument_tests {
    use super::*;

    #[test]
    fn keys_exclude_text_filters_and_validate_exact_identity() {
        let key = "01".repeat(16);
        for (command, selector) in [("code-facts", "--symbol-key"), ("facts", "--document-key")] {
            assert!(Cli::try_parse_from(["airk", command, selector, &key]).is_ok());
            assert!(Cli::try_parse_from(["airk", command, selector, "short"]).is_err());
            assert!(
                Cli::try_parse_from(["airk", command, selector, &key, "--name", "original"])
                    .is_err()
            );
            assert!(
                Cli::try_parse_from(["airk", command, selector, &key, "--kind", "definition"])
                    .is_err()
            );
        }
        assert!(Cli::try_parse_from(["airk", "facts", "src", "--document-key", &key]).is_err());
        assert!(
            Cli::try_parse_from([
                "airk",
                "code-facts",
                "--symbol-key",
                &key,
                "--path-prefix",
                "src"
            ])
            .is_err()
        );
    }
}

#[derive(Subcommand)]
enum Command {
    /// Inspect and call the read-only MCP tools, or open an interactive tools console.
    Tools {
        #[command(subcommand)]
        action: tools::ToolsCommand,
    },
    /// Inspect observed indexed changes and validate remembered evidence.
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
    /// Extract explicit local OCR pages using a bounded provider request JSON.
    Ocr {
        path: String,
        #[arg(long)]
        request: PathBuf,
    },
    /// Manage an explicit per-user native indexing service for this data root.
    Service {
        #[command(subcommand)]
        action: ServiceCommand,
    },
    /// Relocate an explicitly selected data root while keeping old managed state.
    Storage {
        #[command(subcommand)]
        action: StorageCommand,
    },
    /// Persistent local indexing queue shared by CLI and editor workers.
    Schedule {
        #[command(subcommand)]
        action: ScheduleCommand,
    },
    /// Local content-free activity, performance and counterfactual context estimates.
    Usage {
        #[command(subcommand)]
        action: UsageCommand,
    },
    /// Inspect eligible workspace indexing coverage; optionally refresh discovery.
    Coverage {
        #[arg(long)]
        refresh: bool,
    },
    /// Optional producer-side semantic binding and revision-checked import.
    Analysis {
        #[command(subcommand)]
        action: AnalysisCommand,
    },
    /// Query enriched code syntax and imported facts without broad source text.
    CodeFacts {
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
        /// Persist and join the shared local indexing queue.
        #[arg(long)]
        scheduled: bool,
        /// Respect native pause state when scheduled by an editor.
        #[arg(long, requires = "scheduled")]
        automatic: bool,
        /// Re-extract every eligible file while preserving the prior index on failure.
        #[arg(long)]
        full: bool,
    },
    /// Show the active snapshot and counts.
    Status,
    /// Retrieve bounded evidence using the collection default (hybrid unless opted out).
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
    /// List indexed paths by prefix.
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
    /// Read indexed source lines, with inclusive 1-based bounds.
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
    /// Serve read-only MCP tools over stdio.
    Mcp,
    /// Inspect format and extraction quality for an indexed file.
    Info {
        path: String,
        #[command(flatten)]
        evidence: EvidenceOptions,
    },
    /// Retrieve a file's native and imported code/document outline.
    Outline {
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        /// Return names, kinds, original locations, spans and durable keys.
        #[arg(long)]
        compact: bool,
        #[command(flatten)]
        evidence: EvidenceOptions,
        path: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Find exact native syntax and imported provider definitions.
    Symbol {
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        name: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Retrieve native import declarations.
    Imports {
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        #[arg(default_value = "")]
        path_prefix: String,
    },
    /// Retrieve structured document/configuration facts.
    Facts {
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
    /// Find textual identifier occurrences; no semantic name resolution.
    Refs {
        name: String,
        #[arg(long, default_value = "")]
        path_prefix: String,
    },
    /// Retrieve neighboring indexed lines.
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
}

#[derive(Subcommand)]
enum ScheduleCommand {
    Enqueue {
        #[arg(long)]
        full: bool,
    },
    Status,
    Pause,
    Resume,
    Forget,
    Run {
        #[arg(long)]
        watch: bool,
        #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(250..=60000))]
        poll_ms: u64,
    },
}

#[derive(Subcommand)]
enum ServiceCommand {
    Plan,
    Install,
    Status,
    Start,
    Stop,
    Uninstall,
}

#[derive(Subcommand)]
enum StorageCommand {
    /// Inspect the copy and required quota headroom without moving data.
    Plan { destination: PathBuf },
    /// Copy, verify and activate a new data root; all clients must be stopped.
    Relocate { destination: PathBuf },
    /// Resume a recorded interrupted relocation from the original data root.
    Recover,
}

#[derive(Subcommand)]
enum UsageCommand {
    /// Import one bounded content-free client/session measurement JSON (at most 16 KiB).
    Import {
        file: PathBuf,
        /// Validate and display the content-free measurement without recording it.
        #[arg(long)]
        preview: bool,
    },
    /// Count comparable logical JSON samples with a digest-bound offline tokenizer.
    Calibrate {
        #[arg(long)]
        tokenizer: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        returned: PathBuf,
        #[arg(long)]
        generation: u64,
        #[arg(long, default_value="unknown", value_parser=["code","structured_text","markdown","pdf","docx","xlsx","pptx","svg","plain_text","mixed","unknown"])]
        content_kind: String,
        /// Persist only aggregate counts and tokenizer identity; preview is the default.
        #[arg(long)]
        record: bool,
    },
    /// Explicit usefulness feedback for a retained request_id; missing feedback remains unknown.
    Feedback {
        request_id: String,
        #[arg(long, value_parser = ["useful", "partly-useful", "not-useful"])]
        usefulness: String,
    },
    /// Export bounded selected-collection aggregates; does not record its own output.
    Report {
        #[arg(long, default_value_t = 30)]
        days: u16,
        #[arg(long, default_value = "json", value_parser = ["json", "csv", "markdown"])]
        format: String,
        /// Explicit dated input-price scenario JSON; never fetched or saved implicitly.
        #[arg(long)]
        price_scenario: Option<PathBuf>,
    },
    /// Disable recording, enable aggregates, or opt into short-lived content-free diagnostics.
    Configure {
        #[arg(long, value_enum)]
        mode: UsageMode,
    },
    /// Clear this collection's local usage history while preserving indexed sources.
    Clear,
}

#[derive(Clone, Copy, ValueEnum)]
enum UsageMode {
    Disabled,
    AggregatesOnly,
    DiagnosticEvents,
}

#[derive(Clone, Copy, ValueEnum)]
enum SearchMode {
    Lexical,
    Vector,
    Hybrid,
}

impl Command {
    fn usage_operation(&self) -> Option<UsageOperation> {
        Some(match self {
            Self::Usage { .. }
            | Self::Tools { .. }
            | Self::Mcp
            | Self::Host
            | Self::Schedule { .. }
            | Self::Service { .. }
            | Self::Storage { .. } => return None,
            Self::Collections {
                action: CollectionCommand::Quota { .. },
            } => return None,
            // The native worker records the actual job once, including joined callers.
            Self::Index {
                scheduled: true, ..
            } => return None,
            Self::Coverage { .. } => UsageOperation::Coverage,
            Self::Analysis { .. } | Self::Ocr { .. } | Self::Collections { .. } | Self::Init => {
                UsageOperation::CollectionMaintenance
            }
            Self::Setup { .. } | Self::Index { full: false, .. } => UsageOperation::Index,
            Self::Index { full: true, .. } => UsageOperation::Reindex,
            Self::Status => UsageOperation::Status,
            Self::Search { .. } => UsageOperation::Search,
            Self::Files { .. } => UsageOperation::ListFiles,
            Self::Read { .. } => UsageOperation::ReadFile,
            Self::Changes { .. } => UsageOperation::Changes,
            Self::Info { .. } => UsageOperation::FileInfo,
            Self::Outline { .. } => UsageOperation::Outline,
            Self::Symbol { .. } => UsageOperation::Symbols,
            Self::Imports { .. } => UsageOperation::Imports,
            Self::Facts { .. } => UsageOperation::DocumentFacts,
            Self::Refs { .. } => UsageOperation::References,
            Self::Context { .. } => UsageOperation::Context,
            Self::CodeFacts { .. } => UsageOperation::CodeFacts,
            Self::Relationships { .. } => UsageOperation::Relationships,
        })
    }
}

struct OutputRecorder<'a> {
    kernel: &'a Kernel,
    span: std::cell::RefCell<Option<airk_kernel::usage::RequestSpan>>,
}

impl OutputRecorder<'_> {
    fn print(&self, value: &impl serde::Serialize) -> anyhow::Result<()> {
        let mut logical = serde_json::to_value(value)?;
        if let Some(object) = logical.as_object_mut()
            && let Some(id) = self
                .span
                .borrow()
                .as_ref()
                .and_then(|span| span.request_id())
        {
            object.insert("request_id".into(), id.into());
        }
        let text = serde_json::to_string(&logical)?;
        self.print_text(&text, Some(&logical));
        Ok(())
    }
    fn print_text(&self, text: &str, logical: Option<&serde_json::Value>) {
        println!("{text}");
        if let Some(span) = self.span.borrow_mut().take() {
            self.kernel.finish_usage(
                span,
                logical,
                text.len(),
                false,
                ResponseRepresentation::LogicalResultJson,
            );
        }
    }
    fn failure(&self) {
        if let Some(span) = self.span.borrow_mut().take() {
            self.kernel.finish_usage(
                span,
                None,
                0,
                true,
                ResponseRepresentation::LogicalResultJson,
            );
        }
    }
}

#[derive(Subcommand)]
enum AnalysisCommand {
    /// Run an explicitly configured local LSP against an isolated indexed source copy.
    Lsp {
        #[arg(long)]
        request: PathBuf,
    },
    /// Run Universal Ctags against explicit indexed paths in an isolated copy.
    Ctags {
        #[arg(long)]
        request: PathBuf,
    },
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
enum VectorCommand {
    Show,
    /// Inspect or save explicit vector-only exclusions; lexical evidence is retained.
    Selection {
        #[command(subcommand)]
        action: VectorSelectionCommand,
    },
    /// Verify and install local model inputs under native storage quotas.
    Install {
        source: PathBuf,
    },
    Set {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
}

#[derive(Subcommand)]
enum VectorSelectionCommand {
    Show,
    /// Validate a proposed policy without saving or indexing it.
    Validate {
        #[arg(long, required_unless_present = "json", conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
    /// Save a policy for the next index; preserves the vector enabled setting.
    Set {
        #[arg(long, required_unless_present = "json", conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
}

fn vector_selection_input(
    file: Option<PathBuf>,
    json: Option<String>,
) -> anyhow::Result<airk_core::vector_selection::VectorSelectionPolicy> {
    use std::io::Read;
    const MAXIMUM: u64 = 131_072;
    let bytes = match (file, json) {
        (Some(file), None) => {
            let input = std::fs::File::open(file)?;
            anyhow::ensure!(
                input.metadata()?.len() <= MAXIMUM,
                "vector selection exceeds 128 KiB"
            );
            let mut bytes = Vec::new();
            input.take(MAXIMUM + 1).read_to_end(&mut bytes)?;
            bytes
        }
        (None, Some(json)) => json.into_bytes(),
        _ => anyhow::bail!("provide exactly one vector selection --file or --json"),
    };
    anyhow::ensure!(
        bytes.len() <= MAXIMUM as usize,
        "vector selection exceeds 128 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

#[derive(Subcommand)]
enum CollectionCommand {
    /// Inspect or save the collection vector setting used by the panel.
    Vectors {
        #[command(subcommand)]
        action: VectorCommand,
    },
    /// Inspect or explicitly administer optional retained history.
    History {
        #[command(subcommand)]
        action: HistoryCommand,
    },
    /// Pin already committed evidence; never captures unsaved source files.
    Checkpoint {
        #[command(subcommand)]
        action: CheckpointCommand,
    },
    /// Inspect or persist extraction selection; changes apply on successful indexing.
    Extraction {
        #[command(subcommand)]
        action: ExtractionCommand,
    },
    /// Inspect or explicitly configure searchable revision retention.
    Retention {
        #[command(subcommand)]
        action: RetentionCommand,
    },
    /// Inspect or explicitly replace collection ingestion bounds.
    Limits {
        #[command(subcommand)]
        action: LimitsCommand,
    },
    /// Inspect or explicitly replace the quota policy shared by this data root.
    Quota {
        #[command(subcommand)]
        action: QuotaCommand,
    },
    /// Register source aliases; changes take effect on the next successful index.
    Roots {
        #[command(subcommand)]
        action: RootsCommand,
    },
    /// List collections and actual derived-state disk bytes.
    List,
    /// Remove an explicitly named managed collection; retains source files.
    Remove { id: String },
    /// Inspect reclaimable retained vector generations; --apply removes unpinned candidates.
    Prune {
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum CheckpointCommand {
    List,
    Create {
        label: String,
        #[arg(long)]
        revision: airk_core::identity::RevisionToken,
    },
    Remove {
        label: String,
        #[arg(long)]
        revision: airk_core::identity::RevisionToken,
    },
}
#[derive(Subcommand)]
enum HistoryCommand {
    Show,
    Set {
        #[arg(long)]
        json: String,
    },
    Preview,
    Run {
        #[arg(long)]
        apply: bool,
    },
    Available {
        #[arg(long)]
        revision: Option<airk_core::identity::RevisionToken>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = 64)]
        limit: usize,
        #[arg(long, default_value_t = 65536)]
        max_bytes: usize,
    },
    /// Preview logical purge; apply requires the exact preview digest.
    Purge {
        #[arg(long)]
        json: String,
        #[arg(long)]
        apply: bool,
        #[arg(long, requires = "apply")]
        expected_plan: Option<airk_core::identity::Sha256Digest>,
    },
}

#[derive(Subcommand)]
enum RetentionCommand {
    /// Continue pending automatic cleanup once; skip manual policy or a busy writer.
    Continue,
    /// Read effective policy, protected storage and pending maintenance; never clean up.
    Show,
    /// Preview the current or proposed policy without changing managed state.
    Preview {
        #[arg(long, conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
    /// Persist an explicit policy; does not immediately remove generations.
    Set {
        #[arg(required_unless_present = "json", conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
    /// Preview policy maintenance; --apply performs one bounded pass.
    Run {
        #[arg(long)]
        apply: bool,
        /// Maximum removal bytes for this pass (1..64 GiB); does not change policy.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=68_719_476_736))]
        max_removal_bytes: Option<u64>,
        /// Cooperative elapsed-time budget for this pass (1..60000 ms).
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=60_000))]
        max_duration_ms: Option<u64>,
        /// Maximum surviving inventory metadata examined (16 KiB..8 GiB).
        #[arg(long, value_parser = clap::value_parser!(u64).range(16_384..=8_589_934_592))]
        max_metadata_bytes: Option<u64>,
    },
}

#[derive(Subcommand)]
enum ExtractionCommand {
    Show,
    /// Select optional evidence for the next successful index; preserves adapter rules.
    Detail {
        #[arg(value_parser = ["essential", "balanced", "detailed"])]
        level: String,
    },
    Validate {
        #[arg(long, required_unless_present = "json", conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
    Set {
        #[arg(long, required_unless_present = "json", conflicts_with = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
    },
}

fn extraction_policy_input(
    file: Option<PathBuf>,
    json: Option<String>,
) -> anyhow::Result<airk_kernel::extraction::ExtractionPolicy> {
    use std::io::Read;
    const MAXIMUM: u64 = 131_072;
    let bytes = if let Some(file) = file {
        let input = std::fs::File::open(file)?;
        anyhow::ensure!(
            input.metadata()?.len() <= MAXIMUM,
            "extraction policy exceeds 128 KiB"
        );
        let mut bytes = Vec::new();
        input.take(MAXIMUM + 1).read_to_end(&mut bytes)?;
        bytes
    } else {
        json.expect("clap requires policy input").into_bytes()
    };
    anyhow::ensure!(
        bytes.len() <= MAXIMUM as usize,
        "extraction policy exceeds 128 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn retention_policy_bytes(
    file: Option<PathBuf>,
    json: Option<String>,
) -> anyhow::Result<Option<Vec<u8>>> {
    const MAXIMUM: u64 = 65_536;
    let bytes = if let Some(file) = file {
        use std::io::Read;
        let input = std::fs::File::open(file)?;
        anyhow::ensure!(
            input.metadata()?.len() <= MAXIMUM,
            "retention policy exceeds 64 KiB"
        );
        let mut bytes = Vec::new();
        input.take(MAXIMUM + 1).read_to_end(&mut bytes)?;
        Some(bytes)
    } else {
        json.map(String::into_bytes)
    };
    anyhow::ensure!(
        bytes
            .as_ref()
            .is_none_or(|bytes| bytes.len() <= MAXIMUM as usize),
        "retention policy exceeds 64 KiB"
    );
    Ok(bytes)
}

#[derive(Subcommand)]
enum LimitsCommand {
    Show,
    /// Show effective policy, automatic recommendation and detected memory.
    Describe,
    /// Remove the explicit override and restore machine-aware defaults.
    Reset,
    /// Validate a ScanLimits JSON object without saving it.
    Validate {
        file: PathBuf,
    },
    /// Read a ScanLimits JSON object; quotas still constrain publication.
    Set {
        #[arg(required_unless_present = "json")]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
        #[arg(long)]
        expected_json: Option<String>,
    },
}

#[derive(Subcommand)]
enum RootsCommand {
    List,
    Add {
        alias: String,
        path: PathBuf,
    },
    Remove {
        alias: String,
    },
    /// Resolve an indexed path through its published revision's source aliases.
    Locate {
        path: String,
    },
}

#[derive(Subcommand)]
enum QuotaCommand {
    Status,
    Policy,
    /// Set the entire shared policy from a bounded JSON file, including collection overrides.
    Set {
        #[arg(required_unless_present = "json", conflicts_with = "json")]
        policy: Option<PathBuf>,
        #[arg(long)]
        json: Option<String>,
        #[arg(long)]
        expected_json: Option<String>,
    },
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

fn client_config(kernel: &Kernel, client: SetupClient) -> anyhow::Result<String> {
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
            return Ok(format!(
                "[mcp_servers.airk]\ncommand = {}\nargs = {}",
                serde_json::to_string(&command)?,
                serde_json::to_string(&args)?
            ));
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
    Ok(serde_json::to_string(&config)?)
}

fn main() -> anyhow::Result<()> {
    if airk_kernel::dispatch_pdf_worker()? {
        return Ok(());
    }
    main_async()
}

#[tokio::main]
async fn main_async() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Command::Tools { action } = &cli.command
        && tools::inspect(action)?
    {
        return Ok(());
    }
    if matches!(cli.command, Command::Service { .. }) && cli.data_dir.is_none() {
        anyhow::bail!("service lifecycle requires explicit --data-dir");
    }
    // Recovery must run before Kernel opens the intentionally frozen old root.
    if let Command::Storage { action } = &cli.command {
        let source = cli
            .data_dir
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("storage relocation requires explicit --data-dir"))?;
        return match action {
            StorageCommand::Plan { destination } => {
                print_json(&airk_kernel::storage::relocation_plan(source, destination)?)
            }
            StorageCommand::Relocate { destination } => {
                print_json(&airk_kernel::storage::relocate(source, destination)?)
            }
            StorageCommand::Recover => {
                print_json(&airk_kernel::storage::recover_relocation(source)?)
            }
        };
    }
    let root = match &cli.command {
        Command::Index {
            path: Some(path), ..
        } => path.clone(),
        _ => cli.root,
    };
    let kernel = Kernel::open_collection(root, cli.data_dir, &cli.collection)?;
    let recorder = OutputRecorder {
        kernel: &kernel,
        span: std::cell::RefCell::new(
            cli.command
                .usage_operation()
                .map(|operation| kernel.usage_span(operation, ClientFamily::Cli)),
        ),
    };
    let result = async { match cli.command {
        Command::Tools { action } => tools::run(action, kernel.clone()).await,
        Command::Storage { .. } => unreachable!("handled before opening kernel"),
        Command::Service { action } => {
            let manager = airk_kernel::service::ServiceManager::for_kernel(&kernel)?;
            match action {
                ServiceCommand::Plan => print_json(&manager.plan()?),
                ServiceCommand::Status => print_json(&manager.status()?),
                ServiceCommand::Install => {
                    kernel.schedule_enqueue(false)?;
                    print_json(&manager.install()?)
                }
                ServiceCommand::Start => print_json(&manager.start()?),
                ServiceCommand::Stop => print_json(&manager.stop()?),
                ServiceCommand::Uninstall => print_json(&manager.uninstall()?),
            }
        },
        Command::Schedule { action } => match action {
            ScheduleCommand::Enqueue { full } => print_json(&kernel.schedule_enqueue(full)?),
            ScheduleCommand::Status => print_json(&kernel.schedule_status()?),
            ScheduleCommand::Pause => print_json(&kernel.schedule_pause(true)?),
            ScheduleCommand::Resume => print_json(&kernel.schedule_pause(false)?),
            ScheduleCommand::Forget => {
                kernel.schedule_forget()?;
                print_json(&serde_json::json!({"forgotten":true,"collection":kernel.collection()}))
            }
            ScheduleCommand::Run { watch, poll_ms } => {
                let mut watcher = if watch { Some(airk_kernel::watcher::NativeWatcher::new(&kernel)?) } else { None };
                loop {
                    if let Some(watcher) = &mut watcher {
                        let events = watcher.poll()?;
                        if !events.queued_collections.is_empty() || !events.errors.is_empty() || events.recovering_dropped_events {
                            print_json(&events)?;
                        }
                    }
                    let worker = kernel.clone();
                    let outcome = tokio::task::spawn_blocking(move || worker.run_scheduled_once()).await??;
                    if !watch || matches!(outcome.state, "indexed" | "failed" | "maintained" | "maintenance_failed") { print_json(&outcome)?; }
                    if !watch { break; }
                    if matches!(outcome.state, "idle" | "indexed" | "failed" | "maintained" | "maintenance_failed") {
                        let worker = kernel.clone();
                        let reconciliation = tokio::task::spawn_blocking(move || worker.reconcile_scheduled_once()).await??;
                        if !matches!(reconciliation.state, "idle" | "busy") { print_json(&reconciliation)?; }
                    }
                    tokio::select! {
                        result = tokio::signal::ctrl_c() => { result?; break; },
                        _ = tokio::time::sleep(std::time::Duration::from_millis(poll_ms)) => {}
                    }
                }
                Ok(())
            }
        },
        Command::Usage { action } => match action {
            UsageCommand::Import { file, preview } => {
                let event: airk_core::usage::SupplementalMeasurement = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&file,16_384)?)?;
                if preview {
                    event.validate()?;
                    print_json(&serde_json::json!({"measurement":event,"recorded":false,"notice":"Validated content-free input. Request links and collection admission are checked only when importing."}))
                } else { print_json(&kernel.usage_import(&event)?) }
            },
            UsageCommand::Calibrate {tokenizer,baseline,returned,generation,content_kind,record} => {
                let content_kind = serde_json::from_value(serde_json::Value::String(content_kind))?;
                print_json(&kernel.usage_calibrate(airk_kernel::usage::CalibrationRequest {
                    tokenizer:&tokenizer,baseline:&baseline,returned:&returned,generation,content_kind,record,
                })?)
            },
            UsageCommand::Feedback { request_id, usefulness } => print_json(&kernel.usage_feedback(&request_id, match usefulness.as_str() {
                "useful" => airk_core::usage::Usefulness::Useful,
                "partly-useful" => airk_core::usage::Usefulness::PartlyUseful,
                _ => airk_core::usage::Usefulness::NotUseful,
            })?),
            UsageCommand::Report { days, format, price_scenario } => {
                let view = if let Some(path) = price_scenario {
                    let scenario = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&path,4096)?)?;
                    kernel.usage_report_with_price(days,&scenario)?
                } else { kernel.usage_report(days)? };
                if format == "json" {
                    let stdout = std::io::stdout();
                    let mut output = stdout.lock();
                    serde_json::to_writer(&mut output, &view)?;
                    std::io::Write::write_all(&mut output, b"\n")?;
                } else {
                    println!("{}", airk_kernel::usage::render_report(&view, &format)?);
                }
                Ok(())
            }
            UsageCommand::Configure { mode } => print_json(&kernel.configure_usage(match mode {
                UsageMode::Disabled => RecordingMode::Disabled,
                UsageMode::AggregatesOnly => RecordingMode::AggregatesOnly,
                UsageMode::DiagnosticEvents => RecordingMode::DiagnosticEvents,
            })?),
            UsageCommand::Clear => print_json(&kernel.clear_usage()?),
        },
        Command::Coverage { refresh } => recorder.print(&kernel.coverage(refresh)?),
        Command::Ocr { path, request } => {
            let request = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&request,65_536)?)?;
            recorder.print(&kernel.extract_ocr(&path,&request)?)
        },
        Command::Analysis { action } => match action {
            AnalysisCommand::Lsp { request } => {
                let request = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&request,65_536)?)?;
                recorder.print(&kernel.run_provider("lsp",&request)?)
            }
            AnalysisCommand::Ctags { request } => {
                let request = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&request,65_536)?)?;
                recorder.print(&kernel.run_provider("ctags",&request)?)
            }
            AnalysisCommand::Prepare {
                producer,
                producer_version,
                build_input,
            } => {
                recorder.print(&kernel.prepare_analysis(&producer, &producer_version, &build_input)?)
            }
            AnalysisCommand::Import { index, manifest } => {
                recorder.print(&kernel.import_scip(&index, &manifest)?)
            }
        },
        Command::CodeFacts {
            cursor,
            symbol_key,
            name,
            kind,
            language,
            path_prefix,
            limit,
            max_bytes,
        } => if let Some(key) = symbol_key {
            recorder.print(&kernel.continuity_facts(airk_core::continuity::ContinuityKey::Symbol(key), limit, max_bytes)?)
        } else { recorder.print(&kernel.code_facts_page(&FactQuery {
            path_prefix,
            name,
            kind,
            language,
            exact_name: true,
            limit,
            max_bytes,
            path_exact: false,
            document_only: false,
        }, &airk_core::discovery::DiscoveryOptions { cursor, compact: false })?) },
        Command::Relationships {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
        } => recorder.print(&kernel.relationships(&RelationshipQuery {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
        })?),
        Command::Collections { action } => match action {
            CollectionCommand::Vectors { action } => match action {
                VectorCommand::Show => recorder.print(&kernel.vector_status()?),
                VectorCommand::Install { source } => recorder.print(&kernel.install_vector_model(&source)?),
                VectorCommand::Set { enabled } => recorder.print(&kernel.configure_vectors(enabled)?),
                VectorCommand::Selection { action } => match action {
                    VectorSelectionCommand::Show => recorder.print(&kernel.vector_selection_status()?),
                    VectorSelectionCommand::Validate { file, json } => recorder.print(&kernel.validate_vector_selection(&vector_selection_input(file, json)?)?),
                    VectorSelectionCommand::Set { file, json } => recorder.print(&kernel.configure_vector_selection(vector_selection_input(file, json)?)?),
                },
            },
            CollectionCommand::Extraction { action } => match action {
                ExtractionCommand::Show => recorder.print(&kernel.extraction_policy()?),
                ExtractionCommand::Detail { level } => recorder.print(&kernel.configure_index_detail(serde_json::from_value(serde_json::Value::String(level))?)?),
                ExtractionCommand::Validate { file, json } => recorder.print(&kernel.validate_extraction_policy(&extraction_policy_input(file, json)?)?),
                ExtractionCommand::Set { file, json } => recorder.print(&kernel.configure_extraction_policy(extraction_policy_input(file, json)?)?),
            },
            CollectionCommand::Limits { action } => match action {
                LimitsCommand::Show => recorder.print(&kernel.scan_limits()?),
                LimitsCommand::Describe => recorder.print(&kernel.describe_scan_limits()?),
                LimitsCommand::Reset => recorder.print(&kernel.reset_scan_limits()?),
                LimitsCommand::Validate { file } => {
                    let limits: airk_kernel::limits::ScanLimits = serde_json::from_slice(&airk_kernel::usage::read_usage_input(&file,16_384)?)?;
                    limits.validate()?;
                    recorder.print(&limits)
                }
                LimitsCommand::Set { file, json, expected_json } => {
                    let bytes = match (file, json) {
                        (Some(file), None) => airk_kernel::usage::read_usage_input(&file,16_384)?,
                        (None, Some(json)) => json.into_bytes(),
                        _ => anyhow::bail!("provide exactly one limits file or --json"),
                    };
                    anyhow::ensure!(bytes.len() <= 16_384 && expected_json.as_ref().is_none_or(|json| json.len() <= 16_384), "scan configuration exceeds 16 KiB");
                    let limits = serde_json::from_slice(&bytes)?;
                    let expected = expected_json.map(|json| serde_json::from_str(&json)).transpose()?;
                    recorder.print(&kernel.configure_scan_limits_if_current(limits, expected)?)
                }
            },
            CollectionCommand::Quota { action } => match action {
                QuotaCommand::Status => print_json(&kernel.quota_status()?),
                QuotaCommand::Policy => print_json(&kernel.quota_policy()?),
                QuotaCommand::Set { policy, json, expected_json } => {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    if let Some(policy) = policy { std::fs::File::open(policy)?.take(65_537).read_to_end(&mut bytes)?; }
                    else { bytes = json.unwrap_or_default().into_bytes(); }
                    anyhow::ensure!(bytes.len() <= 65_536 && expected_json.as_ref().is_none_or(|value| value.len() <= 65_536), "quota policy exceeds 64 KiB");
                    let expected = expected_json.map(|value| serde_json::from_str(&value)).transpose()?;
                    print_json(&kernel.configure_quota_if_current(serde_json::from_slice(&bytes)?, expected.as_ref())?)
                },
            },
            CollectionCommand::Roots { action } => match action {
                RootsCommand::List => recorder.print(&kernel.roots()?),
                RootsCommand::Add { alias, path } => recorder.print(&kernel.add_root(&alias, &path)?),
                RootsCommand::Remove { alias } => recorder.print(&kernel.remove_root(&alias)?),
                RootsCommand::Locate { path } => recorder.print(&kernel.source_location(&path)?),
            },
            CollectionCommand::List => recorder.print(&kernel.collections()?),
            CollectionCommand::Remove { id } => recorder.print(&kernel.remove_collection(&id)?),
            CollectionCommand::Retention { action } => match action {
                RetentionCommand::Continue => recorder.print(&kernel.continue_automatic_retention()?),
                RetentionCommand::Show => recorder.print(&kernel.retention_status()?),
                RetentionCommand::Preview { file, json } => {
                    let bytes = retention_policy_bytes(file, json)?;
                    let policy = bytes.as_ref().map(|bytes| serde_json::from_slice(bytes)).transpose()?;
                    recorder.print(&kernel.retention_preview(policy.as_ref())?)
                },
                RetentionCommand::Set { file, json } => {
                    let bytes = retention_policy_bytes(file, json)?.expect("clap requires policy input");
                    recorder.print(&kernel.configure_retention(serde_json::from_slice(&bytes)?)?)
                },
                RetentionCommand::Run { apply, max_removal_bytes, max_duration_ms, max_metadata_bytes } => {
                    let mut budget = airk_kernel::maintenance::MaintenanceBudget::default();
                    if let Some(bytes) = max_removal_bytes { budget.max_removal_bytes = bytes; }
                    if let Some(milliseconds) = max_duration_ms { budget.max_duration_ms = milliseconds; }
                    if let Some(bytes) = max_metadata_bytes { budget.max_metadata_bytes = bytes; }
                    if apply { recorder.print(&kernel.maintain_retention_with_budget(budget)?) }
                    else { recorder.print(&kernel.retention_preview_with_budget(None, budget)?) }
                },
            },
            CollectionCommand::History { action } => {
                use airk_kernel::history_controls::*;
                let budget = HistoryWorkBudget::default();
                match action {
                    HistoryCommand::Show => recorder.print(&kernel.history_policy()?),
                    HistoryCommand::Set { json } => { let bytes = retention_policy_bytes(None, Some(json))?.unwrap(); recorder.print(&kernel.configure_history(serde_json::from_slice(&bytes)?)?) },
                    HistoryCommand::Preview => recorder.print(&kernel.history_preview(&HistorySelectionRequest::default(), budget)?),
                    HistoryCommand::Run { apply } => if apply { recorder.print(&kernel.maintain_history(budget)?) } else { recorder.print(&kernel.history_preview(&HistorySelectionRequest::default(), budget)?) },
                    HistoryCommand::Available { revision, cursor, limit, max_bytes } => recorder.print(&kernel.history_availability(&airk_kernel::HistoryAvailabilityRequest {revision,cursor,limit,max_bytes})?),
                    HistoryCommand::Purge { json, apply, expected_plan } => {
                        let bytes = retention_policy_bytes(None, Some(json))?.unwrap();
                        let request: PurgeRequest = serde_json::from_slice(&bytes)?;
                        if apply {
                            let expected_plan_digest = expected_plan.ok_or_else(|| anyhow::anyhow!("--apply requires --expected-plan from the preview"))?;
                            recorder.print(&kernel.purge_history(&PurgeApplyRequest {request,expected_plan_digest}, budget)?)
                        } else { recorder.print(&kernel.purge_history_preview(&request, budget)?) }
                    },
                }
            },
            CollectionCommand::Checkpoint { action } => {
                use airk_kernel::history_controls::*;
                match action {
                    CheckpointCommand::List => recorder.print(&kernel.checkpoints()?),
                    CheckpointCommand::Create { label, revision } => recorder.print(&kernel.create_checkpoint(&CreateCheckpoint {label,revision}, HistoryWorkBudget::default())?),
                    CheckpointCommand::Remove { label, revision } => recorder.print(&kernel.remove_checkpoint(&RemoveCheckpoint {label,expected_revision: revision}, HistoryWorkBudget::default())?),
                }
            },
            CollectionCommand::Prune { apply } => recorder.print(&kernel.prune_indexes(!apply)?),
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
            let config = client_config(&kernel, client)?;
            recorder.print_text(&config, Some(&serde_json::to_value(&report)?));
            Ok(())
        }
        Command::Init => recorder.print(&kernel.initialize()?),
        Command::Index { full, scheduled, automatic, .. } => recorder.print(&if scheduled {
            kernel.scheduled_index(full, automatic)?
        } else if full {
            kernel.reindex()?
        } else {
            kernel.index()?
        }),
        Command::Status => recorder.print(&kernel.status()?),
        Command::Search {
            query,
            mode,
            semantic,
            path_prefix,
            limit,
            max_bytes,
        } => {
            let query = SearchQuery { text: query, path_prefix, limit, max_bytes };
            recorder.print(&match mode {
                Some(SearchMode::Lexical) => kernel.search(&query)?,
                Some(SearchMode::Vector) => kernel.semantic_search(&query)?,
                Some(SearchMode::Hybrid) => kernel.hybrid_search(&query)?,
                None if semantic => kernel.semantic_search(&query)?,
                None => kernel.default_search(&query)?,
            })
        },
        Command::Files {
            cursor,
            prefix,
            limit,
            max_bytes,
        } => recorder.print(&kernel.list_files_page(&prefix, limit, max_bytes, &airk_core::discovery::DiscoveryOptions { cursor, compact: false })?),
        Command::Read {
            evidence,
            path,
            start_line,
            end_line,
            max_bytes,
        } => recorder.print(&kernel.read_file_conditional(&path, start_line, end_line, max_bytes, &evidence.into())?),
        Command::Changes { since, selectors, paths, path_prefix, include_symbols, include_documents, include_suggestions, limit, max_bytes, cursor } => {
            let mut selectors = selectors.iter().map(|value| serde_json::from_str(value)).collect::<Result<Vec<airk_core::changes::RememberedSelector>, _>>()?;
            selectors.extend(paths.into_iter().map(|path| airk_core::changes::RememberedSelector::Path { path }));
            recorder.print(&kernel.changes(&airk_core::changes::ChangesRequest { since, selectors, path_prefix, include_symbols, include_documents, include_suggestions, limit, max_bytes, cursor })?)
        },
        Command::Mcp => airk_mcp::serve(kernel.clone()).await,
        Command::Info { path, evidence } => recorder.print(&kernel.file_info_conditional(&path, &evidence.into())?),
        Command::Outline {
            cursor,
            compact,
            evidence,
            path,
            limit,
            max_bytes,
        } => recorder.print(&kernel.outline_conditional_page(&path, limit, max_bytes, &evidence.into(), &airk_core::discovery::DiscoveryOptions { cursor, compact })?),
        Command::Symbol {
            cursor,
            name,
            language,
            path_prefix,
        } => recorder.print(&kernel.facts_page(&FactQuery {
            path_prefix,
            name: Some(name),
            kind: Some("definition".into()),
            language,
            exact_name: true,
            limit: 50,
            max_bytes: 8192,
            path_exact: false,
            document_only: false,
        }, &airk_core::discovery::DiscoveryOptions { cursor, compact: false })?),
        Command::Imports { path_prefix, cursor } => recorder.print(&kernel.facts_page(&FactQuery {
            path_prefix,
            name: None,
            kind: Some("import".into()),
            language: None,
            exact_name: false,
            limit: 50,
            max_bytes: 8192,
            path_exact: false,
            document_only: false,
        }, &airk_core::discovery::DiscoveryOptions { cursor, compact: false })?),
        Command::Facts {
            cursor,
            document_key,
            path_prefix,
            name,
            kind,
            limit,
            max_bytes,
        } => if let Some(key) = document_key {
            recorder.print(&kernel.continuity_facts(airk_core::continuity::ContinuityKey::Document(key), limit, max_bytes)?)
        } else { recorder.print(&kernel.facts_page(&FactQuery {
            path_prefix,
            name,
            kind,
            language: None,
            exact_name: false,
            limit,
            max_bytes,
            path_exact: false,
            document_only: true,
        }, &airk_core::discovery::DiscoveryOptions { cursor, compact: false })?) },
        Command::Refs { name, path_prefix } => {
            recorder.print(&kernel.references(&name, &path_prefix, 50, 8192)?)
        }
        Command::Context {
            evidence,
            path,
            line,
            before,
            after,
            max_bytes,
        } => recorder.print(&kernel.context_conditional(&path, line, before, after, max_bytes, &evidence.into())?),
    }}.await;
    if result.is_err() {
        recorder.failure();
    }
    result
}
