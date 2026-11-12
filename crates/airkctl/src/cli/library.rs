// SPDX-License-Identifier: Apache-2.0
//! `airkctl library` (split completion M4; U6, U97, U98): the index library
//! through the same catalog `ControlMethod`s the admin pages call, each
//! answer printed complete as one compact JSON line. `airkctl index` keeps
//! its pre-split meaning (queue indexing for a project), so the management
//! group is named after the admin page.
//!
//! Index setup is the reviewed native flow: `setup` prints the service's
//! draft (`CatalogIndexSetupDraft`), and `setup-apply --draft-file` applies
//! exactly that draft; `setup --apply` does both in one run. The request
//! identity is generated here from `RequestEpoch`, as every keyed `airkctl`
//! request is, and stays in the printed draft so a later apply or receipt
//! read can name it.
use super::{Context, Selection, VERSION, fail, installed, print_control, running, usage};
use airk_core::{
    catalog_management::{CatalogEntity, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES},
    catalog_setup::{IndexSetupDraft, IndexSetupSelection, SetupAdditionalRoot},
    index_catalog::{CatalogRevision, MAX_SELECTION},
    service::{ControlMethod, ControlResponse, RequestKey},
};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub(super) enum LibraryCommand {
    /// One catalog page (or every page with --all): indexes, index sets
    /// (projects), memberships, sources, grants or contexts.
    List {
        #[arg(long, default_value = "indexes", value_parser = ["indexes", "projects", "memberships", "sources", "grants", "contexts"])]
        kind: String,
        /// Limit memberships or sources to one index (`idx_…`).
        #[arg(long)]
        index: Option<String>,
        /// Limit memberships to one index set (`prj_…`).
        #[arg(long)]
        set: Option<String>,
        /// A continuation cursor a previous page printed (JSON).
        #[arg(long)]
        after: Option<String>,
        /// Follow every continuation at the first page's revision.
        #[arg(long)]
        all: bool,
        /// The service's control directory (default: --service-runtime-dir, else the installation's).
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// One catalog entity: an index, an index set or a grant.
    Show {
        #[arg(long, conflicts_with_all = ["set", "grant"], required_unless_present_any = ["set", "grant"])]
        index: Option<String>,
        #[arg(long, conflicts_with = "grant")]
        set: Option<String>,
        #[arg(long)]
        grant: Option<String>,
        /// Refuse unless the catalog is at this revision.
        #[arg(long)]
        revision: Option<u64>,
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Draft a new index for --root (and --data-dir) and print the service's
    /// review; --apply also creates it.
    Setup {
        /// The index's display name.
        #[arg(long)]
        name: String,
        /// The index's collection (default: from the name, lowercase letters, digits and dashes).
        #[arg(long)]
        index_collection: Option<String>,
        /// An additional source root as ALIAS=PATH (repeatable).
        #[arg(long = "additional")]
        additional: Vec<String>,
        /// Keep the index current after edits (native source watching).
        #[arg(long, value_parser = ["on", "off"], default_value = "on")]
        watch: String,
        /// Draft against this catalog revision (default: the current one).
        #[arg(long)]
        revision: Option<u64>,
        /// Apply the draft right after printing it.
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Apply a draft `setup` printed (the whole `catalog_index_setup_draft` line or its `draft`).
    SetupApply {
        #[arg(long)]
        draft_file: PathBuf,
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
}

/// The service's control directory, as `projects` resolves it.
fn control(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
) -> Result<PathBuf, u8> {
    if let Some(control) = runtime_dir.or_else(|| selection.control.clone()) {
        return Ok(control);
    }
    installed(cx).map(|verified| PathBuf::from(&verified.record.locations.state))
}

fn request(cx: &mut Context, control: &Path, method: ControlMethod) -> Result<ControlResponse, u8> {
    crate::service::request(control, method, VERSION).map_err(|line| fail(cx, &line))
}

fn parse<T: std::str::FromStr>(
    cx: &mut Context,
    flag: &str,
    value: Option<String>,
) -> Result<Option<T>, u8> {
    value
        .map(|text| {
            text.parse::<T>()
                .map_err(|_| usage(cx, &format!("invalid {flag}: {text}")))
        })
        .transpose()
}

fn revision(cx: &mut Context, value: Option<u64>) -> Result<Option<CatalogRevision>, u8> {
    value
        .map(|value| CatalogRevision::try_from(value).map_err(|_| usage(cx, "invalid --revision")))
        .transpose()
}

pub(super) fn run(cx: &mut Context, selection: Selection, command: LibraryCommand) -> u8 {
    let result = match command {
        LibraryCommand::List {
            kind,
            index,
            set,
            after,
            all,
            runtime_dir,
        } => list(cx, &selection, &kind, index, set, after, all, runtime_dir),
        LibraryCommand::Show {
            index,
            set,
            grant,
            revision,
            runtime_dir,
        } => show(cx, &selection, index, set, grant, revision, runtime_dir),
        LibraryCommand::Setup {
            name,
            index_collection,
            additional,
            watch,
            revision,
            apply,
            runtime_dir,
        } => setup(
            cx,
            &selection,
            Setup {
                name,
                index_collection,
                additional,
                watch: watch == "on",
                revision,
                apply,
            },
            runtime_dir,
        ),
        LibraryCommand::SetupApply {
            draft_file,
            runtime_dir,
        } => setup_apply(cx, &selection, &draft_file, runtime_dir),
    };
    result.unwrap_or_else(|code| code)
}

#[allow(clippy::too_many_arguments)]
fn list(
    cx: &mut Context,
    selection: &Selection,
    kind: &str,
    index: Option<String>,
    set: Option<String>,
    after: Option<String>,
    all: bool,
    runtime_dir: Option<PathBuf>,
) -> Result<u8, u8> {
    let control = control(cx, selection, runtime_dir)?;
    let kind: CatalogKind = serde_json::from_value(serde_json::Value::String(kind.into()))
        .map_err(|_| usage(cx, "invalid --kind"))?;
    let index_id = parse(cx, "--index", index)?;
    let project_id = parse(cx, "--set", set)?;
    let mut after = match after {
        Some(text) => {
            Some(serde_json::from_str(&text).map_err(|_| usage(cx, "invalid --after cursor"))?)
        }
        None => None,
    };
    let mut expected = None;
    loop {
        let method = ControlMethod::CatalogPage {
            request: CatalogPageRequest {
                kind,
                index_id,
                project_id,
                after: after.take(),
                expected_catalog_revision: expected,
                limit: MAX_SELECTION,
                max_bytes: MAX_PAGE_BYTES,
            },
        };
        let response = request(cx, &control, method)?;
        let code = print_control(cx, &response);
        let ControlResponse::CatalogPage { page, .. } = &response else {
            return Ok(code);
        };
        if code != 0 || !all {
            return Ok(code);
        }
        match &page.continuation {
            Some(next) => {
                after = Some(next.clone());
                expected = Some(page.revision);
            }
            None => return Ok(code),
        }
    }
}

fn show(
    cx: &mut Context,
    selection: &Selection,
    index: Option<String>,
    set: Option<String>,
    grant: Option<String>,
    expected: Option<u64>,
    runtime_dir: Option<PathBuf>,
) -> Result<u8, u8> {
    let control = control(cx, selection, runtime_dir)?;
    let entity = match (
        parse(cx, "--index", index)?,
        parse(cx, "--set", set)?,
        parse(cx, "--grant", grant)?,
    ) {
        (Some(id), None, None) => CatalogEntity::Index { id },
        (None, Some(id), None) => CatalogEntity::Project { id },
        (None, None, Some(id)) => CatalogEntity::Grant { id },
        _ => return Err(usage(cx, "name exactly one of --index, --set or --grant")),
    };
    let expected_catalog_revision = revision(cx, expected)?;
    let response = request(
        cx,
        &control,
        ControlMethod::CatalogDetail {
            entity,
            expected_catalog_revision,
        },
    )?;
    Ok(print_control(cx, &response))
}

struct Setup {
    name: String,
    index_collection: Option<String>,
    additional: Vec<String>,
    watch: bool,
    revision: Option<u64>,
    apply: bool,
}

/// The collection a name suggests: lowercase ASCII letters and digits, other
/// runs as one dash, at most 64 characters; `index` when nothing is left.
fn collection_from(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed: String = out.trim_end_matches('-').chars().take(64).collect();
    if trimmed.is_empty() {
        "index".into()
    } else {
        trimmed
    }
}

fn plain(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn setup(
    cx: &mut Context,
    selection: &Selection,
    choice: Setup,
    runtime_dir: Option<PathBuf>,
) -> Result<u8, u8> {
    let explicit = runtime_dir.is_some() || selection.control.is_some();
    let control = control(cx, selection, runtime_dir)?;
    let data_root = match &selection.data_dir {
        Some(data) => data.clone(),
        None => installed(cx).map(|verified| PathBuf::from(&verified.record.locations.data))?,
    };
    if !explicit {
        running(cx, &control, &mut None)?;
    }
    let additional_roots = choice
        .additional
        .iter()
        .map(|entry| match entry.split_once('=') {
            Some((alias, path)) if !alias.is_empty() && !path.is_empty() => {
                Ok(SetupAdditionalRoot {
                    alias: alias.to_owned(),
                    path: path.to_owned(),
                })
            }
            _ => Err(entry.clone()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|entry| usage(cx, &format!("--additional takes ALIAS=PATH, not {entry}")))?;
    let expected_catalog_revision = match revision(cx, choice.revision)? {
        Some(revision) => revision,
        None => current_revision(cx, &control)?,
    };
    let (namespace, epoch) =
        crate::journal::epoch(&control, VERSION).map_err(|line| fail(cx, &line))?;
    let nonce = crate::journal::mint().map_err(|line| fail(cx, &line))?;
    let selection = IndexSetupSelection {
        key: RequestKey {
            namespace,
            epoch,
            nonce,
        },
        expected_catalog_revision,
        name: choice.name.clone(),
        primary_root: plain(&selection.root),
        data_root: plain(&data_root),
        collection: choice
            .index_collection
            .unwrap_or_else(|| collection_from(&choice.name)),
        additional_roots,
        setup_watch: choice.watch,
    };
    let response = request(
        cx,
        &control,
        ControlMethod::CatalogIndexSetupDraft { selection },
    )?;
    let code = print_control(cx, &response);
    if code != 0 || !choice.apply {
        return Ok(code);
    }
    let ControlResponse::CatalogIndexSetupDraft { draft, .. } = response else {
        return Ok(code);
    };
    let applied = request(
        cx,
        &control,
        ControlMethod::CatalogIndexSetupApply { draft },
    )?;
    Ok(print_control(cx, &applied))
}

/// The catalog revision a draft is reviewed against: the first index page's.
fn current_revision(cx: &mut Context, control: &Path) -> Result<CatalogRevision, u8> {
    let method = ControlMethod::CatalogPage {
        request: CatalogPageRequest {
            kind: CatalogKind::Indexes,
            index_id: None,
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 1,
            max_bytes: MAX_PAGE_BYTES,
        },
    };
    match request(cx, control, method)? {
        ControlResponse::CatalogPage { page, .. } => Ok(page.revision),
        other => Err(print_control(cx, &other).max(1)),
    }
}

fn setup_apply(
    cx: &mut Context,
    selection: &Selection,
    file: &Path,
    runtime_dir: Option<PathBuf>,
) -> Result<u8, u8> {
    let control = control(cx, selection, runtime_dir)?;
    let text = std::fs::read_to_string(file)
        .map_err(|error| usage(cx, &format!("--draft-file {}: {error}", file.display())))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| usage(cx, "--draft-file is not JSON"))?;
    // The whole printed line (`{"status":"catalog_index_setup_draft","draft":…}`)
    // or the draft object itself.
    let draft = value.get("draft").cloned().unwrap_or(value);
    let draft: IndexSetupDraft = serde_json::from_value(draft)
        .map_err(|_| usage(cx, "--draft-file holds no index setup draft"))?;
    let response = request(
        cx,
        &control,
        ControlMethod::CatalogIndexSetupApply { draft },
    )?;
    Ok(print_control(cx, &response))
}

#[cfg(test)]
mod tests {
    use super::collection_from;

    #[test]
    fn collections_follow_the_name() {
        assert_eq!(collection_from("My Docs (2026)"), "my-docs-2026");
        assert_eq!(collection_from("  AIRK  "), "airk");
        assert_eq!(collection_from("!!!"), "index");
    }
}
