// SPDX-License-Identifier: Apache-2.0
//! The evidence reads of `airkctl` (split step 3a.2c): Clap arguments and the
//! argument-to-tool mapping, ported verbatim from the pre-split `airk` by
//! `scripts/dev/split_3a2c_port_evidence.py` (unformatted: `#[rustfmt::skip]`).
//! Do not edit by hand; edit the script's inputs or its reviewed EDITS and
//! re-run it. Each command becomes one M3 `ToolsCall` with the returned tool
//! name and arguments.
use anyhow::{Context, Result, ensure};
use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(clap::Args, Default, serde::Serialize)]
pub struct EvidenceOptions {
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

#[derive(clap::Args, Default)]
pub struct ReplyOptions {
    /// Lean/Rich reply projection; independent of retained indexing detail.
    #[arg(long,value_parser=airk_core::reply::ReplyDetail::ALL.map(|value|value.as_str()))]
    reply_detail: Option<String>,
}

#[derive(clap::Args, Default, serde::Serialize)]
pub struct VerificationOptions {
    /// Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking.
    #[arg(long, value_parser = ["off", "report", "require"])]
    #[serde(skip_serializing_if = "Option::is_none")]
    verification: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=256))]
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_max_sources: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=1073741824))]
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_max_bytes: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=30000))]
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_deadline_ms: Option<u64>,
}

#[derive(clap::Args, Default, serde::Serialize)]
pub struct SearchFilters {
    /// Exact indexed extraction-language label; not a filename-extension filter.
    #[arg(long)]
    language: Option<String>,
    /// Indexed evidence classification; unknown includes unclassified extraction.
    #[arg(long, value_parser = ["code", "document", "configuration", "unknown"])]
    evidence_kind: Option<String>,
    /// Exclude one literal collection-relative prefix from this query only.
    #[arg(long)]
    exclude_path_prefix: Option<String>,
}

#[derive(clap::Args, Default, serde::Serialize)]
pub struct NameMatchOptions {
    /// Default exact lookup; suffix/contains match simple or qualified names case-sensitively.
    #[arg(long, value_parser = ["exact", "suffix", "contains"], requires = "name")]
    name_match: Option<String>,
    /// Request qualified match counts; a bounded partial scan can report an unknown total.
    #[arg(long)]
    include_count: bool,
}

#[derive(clap::Args, Default, serde::Serialize)]
pub struct ReferenceOptions {
    #[command(flatten)]
    #[serde(flatten)]
    filters: SearchFilters,
    /// Continue one exact publication with identical identifier, filters and ordering.
    #[arg(long)]
    cursor: Option<String>,
    /// Omit for the legacy response; paged index order uses stable IDs, with path/code_first alternatives.
    #[arg(long, value_parser = ["index", "path", "code_first"])]
    order: Option<String>,
    /// Summarize returned page records by file; bounded metadata is additional to the text-byte budget.
    #[arg(long)]
    include_files: bool,
    /// Request candidate-scoped counts; the whole-query total remains unknown.
    #[arg(long)]
    include_count: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SearchMode {
    Lexical,
    Vector,
    Hybrid,
}

/// The fifteen evidence reads, as `airk` declared them.
#[derive(Subcommand)]
pub enum EvidenceCommand {
    /// Plan, and with --execute run, a multi-step read-only evidence request.
    ///
    /// Give a natural-language request as the TEXT argument, or a structured plan with --request FILE ('-' for stdin).
    /// Without --execute it only prints the plan; same contract as the MCP `parse` tool. Never writes sources.
    Parse {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Report indexed changes since a revision token: added, modified and removed files, symbols and documents.
    Changes {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Inspect code facts and signatures with exact default name matching and bounded continuation.
    CodeFacts {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        #[command(flatten)]
        matching: NameMatchOptions,
        /// False includes rich facts and durable IDs; independent of response mode.
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        compact: Option<bool>,
        /// Continue the same revision-bound discovery with unchanged filters.
        #[arg(long)]
        cursor: Option<String>,
        /// Durable symbol key from indexed continuity evidence; excludes textual filters.
        #[arg(long, conflicts_with_all = ["name", "kind", "language", "path_prefix", "cursor", "name_match", "include_count"])]
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
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Print the project's publication, freshness, coverage and job status.
    Status {
        #[command(flatten)]
        reply: ReplyOptions,
    },
    /// Search indexed evidence; results carry file, line and revision citations.
    ///
    /// Without --mode the saved search policy applies. Eligible policies can return
    /// lexical matches during helper outages; execution reports the served mode and
    /// semantic incompleteness. Explicit vector/hybrid and saved RequireComplete refuse
    /// helper outages. Hybrid can still serve lexical evidence during a pending index
    /// upgrade, with that separate limitation disclosed. --mode lexical uses normalized
    /// AND terms: punctuation and quotation marks separate terms; quotes do not enable
    /// phrase matching. Lexical order is stable index identity, not relevance.
    Search {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        #[command(flatten)]
        filters: SearchFilters,
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
    /// List indexed files under an optional path prefix, with continuation cursors.
    Files {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Show what the index knows about one file: language, size, extraction method and parse status.
    Info {
        #[arg(long,help=airk_core::native_reads::FILE_INFO_PROFILE_FAMILIES_HELP)]
        include_profile_families: bool,
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        path: String,
        #[command(flatten)]
        evidence: EvidenceOptions,
    },
    /// Read one indexed file's structure in file order; this returns evidence, not an Explorer page proposal.
    Outline {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Find definitions by exact name by default; use --name-match for suffix/contains variants.
    Symbol {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        #[command(flatten)]
        matching: NameMatchOptions,
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
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// List import and dependency statements found under a path prefix.
    Imports {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
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
    /// Find exact-case textual identifier uses; these are candidates, not resolved language references.
    Refs {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        #[command(flatten)]
        options: ReferenceOptions,
        name: String,
        #[arg(long, default_value = "")]
        path_prefix: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    /// Read lines around --line in a file (--before/--after), with a citation.
    Context {
        #[command(flatten)]
        verification: VerificationOptions,
        #[command(flatten)]
        reply: ReplyOptions,
        #[command(flatten)]
        evidence: EvidenceOptions,
        path: String,
        line: usize,
        #[arg(long, default_value_t = airk_core::native_reads::CONTEXT_RADIUS_DEFAULT)]
        before: usize,
        #[arg(long, default_value_t = airk_core::native_reads::CONTEXT_RADIUS_DEFAULT)]
        after: usize,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
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
fn query_options(mut value: Value, options: impl serde::Serialize) -> Result<Value> {
    let options = serde_json::to_value(options)?;
    value
        .as_object_mut()
        .context("query arguments must be an object")?
        .extend(
            options
                .as_object()
                .context("query options must be an object")?
                .iter()
                .filter(|(_, value)| !value.is_null() && **value != Value::Bool(false))
                .map(|(name, value)| (name.clone(), value.clone())),
        );
    Ok(value)
}
/// The tool and arguments one evidence read sends.
pub fn query(command: EvidenceCommand) -> Result<(&'static str, Value)> {
    let reply_detail = match &command {
        EvidenceCommand::Status { reply, .. }
        | EvidenceCommand::Parse { reply, .. }
        | EvidenceCommand::CodeFacts { reply, .. }
        | EvidenceCommand::Changes { reply, .. }
        | EvidenceCommand::Search { reply, .. }
        | EvidenceCommand::Files { reply, .. }
        | EvidenceCommand::Read { reply, .. }
        | EvidenceCommand::Outline { reply, .. }
        | EvidenceCommand::Symbol { reply, .. }
        | EvidenceCommand::Imports { reply, .. }
        | EvidenceCommand::Facts { reply, .. }
        | EvidenceCommand::Info { reply, .. }
        | EvidenceCommand::Refs { reply, .. }
        | EvidenceCommand::Context { reply, .. }
        | EvidenceCommand::Relationships { reply, .. } => reply.reply_detail.clone(),
    };
    let verification = match &command {
        EvidenceCommand::Parse { verification, .. }
        | EvidenceCommand::CodeFacts { verification, .. }
        | EvidenceCommand::Changes { verification, .. }
        | EvidenceCommand::Search { verification, .. }
        | EvidenceCommand::Files { verification, .. }
        | EvidenceCommand::Read { verification, .. }
        | EvidenceCommand::Outline { verification, .. }
        | EvidenceCommand::Symbol { verification, .. }
        | EvidenceCommand::Imports { verification, .. }
        | EvidenceCommand::Facts { verification, .. }
        | EvidenceCommand::Info { verification, .. }
        | EvidenceCommand::Refs { verification, .. }
        | EvidenceCommand::Context { verification, .. }
        | EvidenceCommand::Relationships { verification, .. } => serde_json::to_value(verification)?,
        _ => json!({}),
    };
    let (tool, mut value) = match command {
        EvidenceCommand::Status { .. } => ("airk_status", json!({})),
        EvidenceCommand::Parse {
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
        EvidenceCommand::Search {
            filters,
            query,
            mode,
            semantic,
            path_prefix,
            limit,
            max_bytes,
            ..
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
                query_options(
                    json!({"query":query,"mode":mode,"path_prefix":path_prefix,"limit":limit,"max_bytes":max_bytes}),
                    filters,
                )?,
            )
        }
        EvidenceCommand::Files {
            cursor,
            prefix,
            limit,
            max_bytes,
            ..
        } => (
            "airk_list_files",
            json!({"cursor":cursor,"prefix":prefix,"limit":limit,"max_bytes":max_bytes}),
        ),
        EvidenceCommand::Read {
            evidence: options,
            path,
            start_line,
            end_line,
            max_bytes,
            ..
        } => (
            "airk_read_file",
            evidence(
                json!({"path":path,"start_line":start_line,"end_line":end_line,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        EvidenceCommand::Info {
            path,
            include_profile_families,
            evidence: options,
            ..
        } => {
            let mut arguments = json!({"path":path});
            if include_profile_families {
                arguments["include_profile_families"] = json!(true);
            }
            ("airk_file_info", evidence(arguments, options)?)
        }
        EvidenceCommand::Outline {
            cursor,
            compact,
            evidence: options,
            path,
            limit,
            max_bytes,
            ..
        } => (
            "airk_outline",
            evidence(
                json!({"path":path,"cursor":cursor,"compact":compact,"limit":limit,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        EvidenceCommand::Symbol {
            matching,
            compact,
            cursor,
            name,
            language,
            path_prefix,
            limit,
            max_bytes,
            ..
        } => (
            "airk_symbols",
            query_options(
                json!({"compact":compact,"cursor":cursor,"name":name,"language":language,"path_prefix":path_prefix,"limit":limit,"max_bytes":max_bytes}),
                matching,
            )?,
        ),
        EvidenceCommand::Imports {
            compact,
            cursor,
            path_prefix,
            ..
        } => (
            "airk_imports",
            json!({"compact":compact,"cursor":cursor,"path_prefix":path_prefix}),
        ),
        EvidenceCommand::Facts {
            compact,
            cursor,
            document_key,
            path_prefix,
            name,
            kind,
            limit,
            max_bytes,
            ..
        } => (
            "airk_document_facts",
            json!({"compact":compact,"cursor":cursor,"document_key":document_key,"path_prefix":path_prefix,"name":name,"kind":kind,"limit":limit,"max_bytes":max_bytes}),
        ),
        EvidenceCommand::CodeFacts {
            matching,
            compact,
            cursor,
            symbol_key,
            path_prefix,
            name,
            kind,
            language,
            limit,
            max_bytes,
            ..
        } => (
            "airk_code_facts",
            query_options(
                json!({"compact":compact,"cursor":cursor,"symbol_key":symbol_key,"path_prefix":path_prefix,"name":name,"kind":kind,"language":language,"limit":limit,"max_bytes":max_bytes}),
                matching,
            )?,
        ),
        EvidenceCommand::Relationships {
            symbol,
            direction,
            kind,
            path_prefix,
            hops,
            limit,
            max_bytes,
            ..
        } => (
            "airk_relationships",
            json!({"symbol":symbol,"direction":direction,"kind":kind,"path_prefix":path_prefix,"hops":hops,"limit":limit,"max_bytes":max_bytes}),
        ),
        EvidenceCommand::Refs {
            options,
            name,
            path_prefix,
            limit,
            max_bytes,
            ..
        } => (
            "airk_references",
            query_options(
                json!({"name":name,"path_prefix":path_prefix,"limit":limit,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        EvidenceCommand::Context {
            evidence: options,
            path,
            line,
            before,
            after,
            max_bytes,
            ..
        } => (
            "airk_context",
            evidence(
                json!({"path":path,"line":line,"before":before,"after":after,"max_bytes":max_bytes}),
                options,
            )?,
        ),
        EvidenceCommand::Changes {
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
            ..
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
    };
    if let Some(detail) = reply_detail {
        if let Some(existing) = value.get("reply_detail").filter(|value| !value.is_null()) {
            ensure!(
                existing == &json!(detail),
                "--reply-detail conflicts with the request's reply_detail"
            );
        }
        value
            .as_object_mut()
            .context("read arguments must be an object")?
            .insert("reply_detail".into(), json!(detail));
    }
    for (key, selected) in verification
        .as_object()
        .context("verification flags must be an object")?
    {
        if let Some(existing) = value.get(key) {
            ensure!(
                existing == selected,
                "verification flag conflicts with request field {key}"
            );
        }
        value
            .as_object_mut()
            .context("read arguments must be an object")?
            .insert(key.clone(), selected.clone());
    }
    let controls: airk_core::verification::VerificationControls =
        serde_json::from_value(Value::Object(
            value
                .as_object()
                .context("read arguments must be an object")?
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "verification"
                            | "verification_max_sources"
                            | "verification_max_bytes"
                            | "verification_deadline_ms"
                    )
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ))?;
    controls.resolve()?;
    Ok((tool, value))
}
