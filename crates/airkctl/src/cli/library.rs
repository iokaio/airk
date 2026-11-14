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
//!
//! Catalog changes (`change` and the set/rename/retire conveniences) print
//! the service's preview; `--yes` applies it under the same request identity.
//! A change that removes or retires something needs `--confirm <name>`
//! (U98), checked against the target's current name with the core rule the
//! admin pages use (`CatalogOperation::confirmation_target`).
use super::{Context, Selection, VERSION, fail, installed, print_control, running, usage};
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    catalog_management::{
        CatalogCursor, CatalogEntity, CatalogItem, CatalogKind, CatalogOperation,
        CatalogPageRequest, CatalogRequest, CatalogSource, ConfirmationTarget, MAX_PAGE_BYTES,
    },
    catalog_setup::{IndexSetupDraft, IndexSetupSelection, SetupAdditionalRoot},
    catalog_source_change::IndexRetirementReason,
    index_catalog::{
        CatalogRevision, IndexDefinition, IndexId, MAX_SELECTION, Membership, PathScope,
        QueryProject, QueryProjectId, ScopeAtom,
    },
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
    /// Preview any catalog operation (JSON, as the admin pages send it);
    /// --yes applies the preview.
    Change {
        /// The `CatalogOperation` JSON, for example {"operation":"rename_index",…}.
        #[arg(long)]
        operation: String,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Create an index set; with --index, add each index with every enabled source root, whole
    /// (an empty set with the same name is reused instead of creating another).
    SetCreate {
        #[arg(long)]
        name: String,
        /// An index (`idx_…`) to add to the set (repeatable); the memberships apply with --yes.
        #[arg(long = "index")]
        indexes: Vec<String>,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Rename an index set.
    SetRename {
        #[arg(long)]
        set: String,
        #[arg(long)]
        name: String,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Delete an index set (its indexes stay); needs --confirm <set name>.
    SetDelete {
        #[arg(long)]
        set: String,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Rename an index.
    Rename {
        #[arg(long)]
        index: String,
        #[arg(long)]
        name: String,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Retire an index (stop tracking it); needs --confirm <index name>.
    Retire {
        #[arg(long)]
        index: String,
        #[command(flatten)]
        apply: ApplyArgs,
    },
    /// Set or clear the description of an index or an index set; agents read it in
    /// list_indexes (without one, a description is generated from the name and paths).
    Describe {
        /// The index (`idx_…`).
        #[arg(long, conflicts_with = "set", required_unless_present = "set")]
        index: Option<String>,
        /// The index set (`prj_…`).
        #[arg(long)]
        set: Option<String>,
        /// The description: at most 1 KiB, newlines allowed, no other control characters.
        #[arg(long, conflicts_with = "clear", required_unless_present = "clear")]
        text: Option<String>,
        /// Remove the stored description.
        #[arg(long)]
        clear: bool,
        #[command(flatten)]
        apply: ApplyArgs,
    },
}

/// Preview, then apply only with --yes; destructive changes need --confirm.
#[derive(clap::Args)]
pub(super) struct ApplyArgs {
    /// Apply the previewed change.
    #[arg(long)]
    yes: bool,
    /// The target's current name (or a grant's or context's id), typed to
    /// confirm a delete, removal or retirement (U98).
    #[arg(long)]
    confirm: Option<String>,
    #[arg(long)]
    runtime_dir: Option<PathBuf>,
}

/// The service's control directory, as `projects` resolves it.
fn control(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
) -> Result<PathBuf, u8> {
    let control = resolve_control(cx, selection, runtime_dir)?;
    super::online_control(cx, control)
}

fn resolve_control(
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
        LibraryCommand::Change { operation, apply } => {
            match serde_json::from_str::<CatalogOperation>(&operation) {
                Ok(operation) => change(cx, &selection, apply, |_, _| Ok(operation)),
                Err(error) => Err(usage(cx, &format!("invalid --operation: {error}"))),
            }
        }
        LibraryCommand::SetCreate {
            name,
            indexes,
            apply,
        } if !indexes.is_empty() => set_create_with(cx, &selection, name, indexes, apply),
        LibraryCommand::SetCreate { name, apply, .. } => change(cx, &selection, apply, |cx, _| {
            let id = new_set_id(cx)?;
            Ok(CatalogOperation::CreateProject { id, name })
        }),
        LibraryCommand::SetRename { set, name, apply } => {
            change(cx, &selection, apply, |cx, control| {
                let project = project(cx, control, &set)?;
                Ok(CatalogOperation::RenameProject {
                    id: project.id,
                    expected_definition_revision: project.definition_revision,
                    name,
                })
            })
        }
        LibraryCommand::SetDelete { set, apply } => change(cx, &selection, apply, |cx, control| {
            let project = project(cx, control, &set)?;
            Ok(CatalogOperation::DeleteProject {
                id: project.id,
                expected_definition_revision: project.definition_revision,
                expected_scope_revision: project.scope_revision,
                expected_defaults_revision: project.defaults_revision,
            })
        }),
        LibraryCommand::Rename { index, name, apply } => {
            change(cx, &selection, apply, |cx, control| {
                let index = index_definition(cx, control, &index)?;
                Ok(CatalogOperation::RenameIndex {
                    id: index.id,
                    expected_definition_revision: index.definition_revision,
                    name,
                })
            })
        }
        LibraryCommand::Describe {
            index,
            set,
            text,
            apply,
            ..
        } => change(cx, &selection, apply, |cx, control| {
            // `--clear` (required when `--text` is absent) stores `None`.
            match (index, set) {
                (Some(index), _) => {
                    let index = index_definition(cx, control, &index)?;
                    Ok(CatalogOperation::SetIndexDescription {
                        id: index.id,
                        expected_definition_revision: index.definition_revision,
                        description: text,
                    })
                }
                (None, Some(set)) => {
                    let project = project(cx, control, &set)?;
                    Ok(CatalogOperation::SetProjectDescription {
                        id: project.id,
                        expected_definition_revision: project.definition_revision,
                        description: text,
                    })
                }
                (None, None) => Err(usage(cx, "name --index or --set")),
            }
        }),
        LibraryCommand::Retire { index, apply } => change(cx, &selection, apply, |cx, control| {
            let index = index_definition(cx, control, &index)?;
            Ok(CatalogOperation::RetireIndex {
                id: index.id,
                expected_definition_revision: index.definition_revision,
                expected_source_revision: index.source_revision,
                reason: IndexRetirementReason::Explicit,
            })
        }),
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
    let control = resolve_control(cx, selection, runtime_dir)?;
    let data_root = match &selection.data_dir {
        Some(data) => data.clone(),
        None => installed(cx).map(|verified| PathBuf::from(&verified.record.locations.data))?,
    };
    running(cx, &control, &mut None)?;
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

/// The current detail of `entity` (at any revision).
fn detail(
    cx: &mut Context,
    control: &Path,
    entity: CatalogEntity,
    revision: Option<CatalogRevision>,
) -> Result<Option<CatalogItem>, u8> {
    match request(
        cx,
        control,
        ControlMethod::CatalogDetail {
            entity,
            expected_catalog_revision: revision,
        },
    )? {
        ControlResponse::CatalogDetail { detail, .. } => Ok(detail.item),
        other => Err(print_control(cx, &other).max(1)),
    }
}

fn project(cx: &mut Context, control: &Path, set: &str) -> Result<QueryProject, u8> {
    let id = parse(cx, "--set", Some(set.to_owned()))?.ok_or(2u8)?;
    match detail(cx, control, CatalogEntity::Project { id }, None)? {
        Some(CatalogItem::Project(project)) if project.id == id => Ok(project),
        _ => Err(fail(
            cx,
            &refused("catalog_entity_unknown", "no such index set"),
        )),
    }
}

fn index_definition(cx: &mut Context, control: &Path, index: &str) -> Result<IndexDefinition, u8> {
    let id = parse(cx, "--index", Some(index.to_owned()))?.ok_or(2u8)?;
    match detail(cx, control, CatalogEntity::Index { id }, None)? {
        Some(CatalogItem::Index(index)) if index.id == id && !index.tombstone => Ok(index),
        _ => Err(fail(
            cx,
            &refused("catalog_entity_unknown", "no such live index"),
        )),
    }
}

fn refused(code: &str, message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, code, false, message)
}

/// The text `--confirm` must equal for `operation` at `revision`, if any.
fn confirmation(
    cx: &mut Context,
    control: &Path,
    operation: &CatalogOperation,
    revision: CatalogRevision,
) -> Result<Option<String>, u8> {
    let missing = |cx: &mut Context| {
        fail(
            cx,
            &refused(
                "confirmation_mismatch",
                "the target is no longer in the catalog",
            ),
        )
    };
    match operation.confirmation_target() {
        None => Ok(None),
        Some(ConfirmationTarget::Id(id)) => Ok(Some(id)),
        Some(ConfirmationTarget::ProjectName(id)) => {
            match detail(cx, control, CatalogEntity::Project { id }, Some(revision))? {
                Some(CatalogItem::Project(project)) if project.id == id => Ok(Some(project.name)),
                _ => Err(missing(cx)),
            }
        }
        Some(ConfirmationTarget::IndexName(id)) => {
            match detail(cx, control, CatalogEntity::Index { id }, Some(revision))? {
                Some(CatalogItem::Index(index)) if index.id == id => Ok(Some(index.name)),
                _ => Err(missing(cx)),
            }
        }
    }
}

/// Preview `build`'s operation at the current revision; with --yes, check
/// the typed confirmation and apply under the same request identity.
fn change(
    cx: &mut Context,
    selection: &Selection,
    apply: ApplyArgs,
    build: impl FnOnce(&mut Context, &Path) -> Result<CatalogOperation, u8>,
) -> Result<u8, u8> {
    let control = control(cx, selection, apply.runtime_dir)?;
    let operation = build(cx, &control)?;
    reviewed(cx, &control, operation, apply.yes, apply.confirm.as_deref()).map(|(code, _)| code)
}

/// Preview `operation` at the current revision and print the preview; when
/// `yes`, check the typed confirmation and apply under the same request
/// identity, printing the answer. Returns the exit code and whether the
/// service answered with a receipt.
fn reviewed(
    cx: &mut Context,
    control: &Path,
    operation: CatalogOperation,
    yes: bool,
    confirm: Option<&str>,
) -> Result<(u8, bool), u8> {
    let expected_catalog_revision = current_revision(cx, control)?;
    let (namespace, epoch) =
        crate::journal::epoch(control, VERSION).map_err(|line| fail(cx, &line))?;
    let caller = crate::journal::mint().map_err(|line| fail(cx, &line))?;
    let nonce = crate::journal::mint().map_err(|line| fail(cx, &line))?;
    let request = CatalogRequest {
        caller,
        key: RequestKey {
            namespace,
            epoch,
            nonce,
        },
        expected_catalog_revision,
        operation,
    };
    let response = self::request(
        cx,
        control,
        ControlMethod::CatalogPreview {
            request: request.clone(),
        },
    )?;
    let code = print_control(cx, &response);
    let ControlResponse::CatalogPreview { preview_digest, .. } = response else {
        return Ok((code, false));
    };
    if code != 0 || !yes {
        return Ok((code, false));
    }
    // U98: the same rule and the same current name the admin pages check.
    if let Some(name) = confirmation(cx, control, &request.operation, expected_catalog_revision)?
        && confirm != Some(name.as_str())
    {
        return Err(fail(
            cx,
            &refused(
                "confirmation_mismatch",
                &format!("type --confirm \"{name}\" to apply this change"),
            ),
        ));
    }
    let applied = self::request(
        cx,
        control,
        ControlMethod::CatalogApply {
            request,
            preview_digest,
        },
    )?;
    let code = print_control(cx, &applied);
    Ok((
        code,
        code == 0 && matches!(applied, ControlResponse::CatalogApplied { .. }),
    ))
}

fn new_set_id(cx: &mut Context) -> Result<QueryProjectId, u8> {
    let id = crate::journal::mint().map_err(|line| fail(cx, &line))?;
    format!("prj_{id}")
        .parse()
        .map_err(|_| usage(cx, "index set id"))
}

/// Every page of `kind` (for one index or set) at one catalog revision.
fn pages(
    cx: &mut Context,
    control: &Path,
    kind: CatalogKind,
    index_id: Option<IndexId>,
    project_id: Option<QueryProjectId>,
    revision: CatalogRevision,
) -> Result<Vec<CatalogItem>, u8> {
    let mut after: Option<CatalogCursor> = None;
    let mut items = Vec::new();
    loop {
        let method = ControlMethod::CatalogPage {
            request: CatalogPageRequest {
                kind,
                index_id,
                project_id,
                after: after.clone(),
                expected_catalog_revision: Some(revision),
                limit: MAX_SELECTION,
                max_bytes: MAX_PAGE_BYTES,
            },
        };
        let page = match request(cx, control, method)? {
            ControlResponse::CatalogPage { page, .. } => page,
            other => return Err(print_control(cx, &other).max(1)),
        };
        if page.revision != revision {
            return Err(fail(
                cx,
                &refused(
                    "catalog_changed",
                    "the catalog changed while it was read; run the command again",
                ),
            ));
        }
        items.extend(page.items);
        match page.continuation {
            Some(next)
                if after
                    .as_ref()
                    .is_none_or(|previous| previous.after < next.after) =>
            {
                after = Some(next);
            }
            Some(_) => {
                return Err(fail(
                    cx,
                    &refused("catalog_changed", "catalog continuation did not advance"),
                ));
            }
            None => return Ok(items),
        }
    }
}

/// The membership scope that reads all of `index`: one whole-subtree atom per
/// enabled source root of that index, as index setup and the admin's Make
/// readable build it.
fn whole_scope(index: IndexId, sources: &[CatalogSource]) -> Vec<ScopeAtom> {
    sources
        .iter()
        .filter(|source| source.index_id == index && source.enabled)
        .map(|source| ScopeAtom {
            source_root_id: source.id,
            kind: PathScope::Subtree,
            path: String::new(),
        })
        .collect()
}

/// `set-create --index` (U104): the admin's Make readable for several
/// indexes. Each index's whole scope is read first; an empty set named
/// `name` is reused, otherwise one is created; then each index is added by a
/// reviewed `SetMembership`. Every step's answer is printed and the first
/// refusal stops. Without --yes only the creation of the set is previewed.
fn set_create_with(
    cx: &mut Context,
    selection: &Selection,
    name: String,
    indexes: Vec<String>,
    apply: ApplyArgs,
) -> Result<u8, u8> {
    let control = control(cx, selection, apply.runtime_dir)?;
    let mut ids: Vec<IndexId> = Vec::new();
    for text in indexes {
        let id = parse(cx, "--index", Some(text))?.ok_or(2u8)?;
        if ids.contains(&id) {
            return Err(usage(cx, &format!("--index {id} is named twice")));
        }
        ids.push(id);
    }
    let revision = current_revision(cx, &control)?;
    let mut scopes = Vec::new();
    for index in &ids {
        let sources = pages(
            cx,
            &control,
            CatalogKind::Sources,
            Some(*index),
            None,
            revision,
        )?
        .into_iter()
        .filter_map(|item| match item {
            CatalogItem::Source(source) => Some(source),
            _ => None,
        })
        .collect::<Vec<_>>();
        let scope = whole_scope(*index, &sources);
        if scope.is_empty() {
            return Err(fail(
                cx,
                &refused(
                    "index_set_scope_empty",
                    &format!(
                        "index {index} has no enabled source root, so a membership would grant nothing"
                    ),
                ),
            ));
        }
        scopes.push((*index, scope));
    }
    // Only an EMPTY set with this name is reused (one an interrupted run
    // left); a set that already holds indexes is never merged into.
    let mut reused = None;
    for item in pages(cx, &control, CatalogKind::Projects, None, None, revision)? {
        let CatalogItem::Project(project) = item else {
            continue;
        };
        if project.name != name {
            continue;
        }
        let members = pages(
            cx,
            &control,
            CatalogKind::Memberships,
            None,
            Some(project.id),
            revision,
        )?;
        if members.is_empty() {
            reused = Some(project.id);
            break;
        }
    }
    let id = match reused {
        Some(id) => {
            let code = super::json_line(
                cx,
                &serde_json::json!({"status": "index_set_reused", "id": id, "name": name}),
            );
            if code != 0 {
                return Ok(code);
            }
            id
        }
        None => {
            let id = new_set_id(cx)?;
            let operation = CatalogOperation::CreateProject {
                id,
                name: name.clone(),
            };
            let (code, applied) = reviewed(cx, &control, operation, apply.yes, None)?;
            if code != 0 || !applied {
                return Ok(code);
            }
            id
        }
    };
    if !apply.yes {
        return Ok(0);
    }
    let set = id.to_string();
    for (index, scope) in scopes {
        let project = project(cx, &control, &set)?;
        let operation = CatalogOperation::SetMembership {
            membership: Membership {
                project_id: id,
                index_id: index,
                revision: CatalogRevision::INITIAL,
                scope,
            },
            expected_scope_revision: project.scope_revision,
            expected_membership_revision: None,
        };
        let (code, applied) = reviewed(cx, &control, operation, true, None)?;
        if code != 0 || !applied {
            return Ok(code.max(1));
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::{collection_from, whole_scope};
    use airk_core::{
        catalog_management::CatalogSource,
        identity::Incarnation,
        index_catalog::{IndexId, PathScope, ScopeAtom, SourceRootId},
    };

    #[test]
    fn a_new_membership_reads_every_enabled_root_of_its_index_whole() {
        let index: IndexId = "idx_01010101010101010101010101010101".parse().unwrap();
        let other: IndexId = "idx_02020202020202020202020202020202".parse().unwrap();
        let root = |byte: u8| SourceRootId::from_identity(Incarnation::from_bytes([byte; 16]));
        let source = |id: u8, of: IndexId, enabled: bool| CatalogSource {
            id: root(id),
            index_id: of,
            canonical_path: format!("/src/{id}"),
            owner: id == 3,
            enabled,
            aliases: Vec::new(),
        };
        let sources = [
            source(3, index, true),
            source(4, index, false),
            source(5, other, true),
            source(6, index, true),
        ];
        let whole = |id: u8| ScopeAtom {
            source_root_id: root(id),
            kind: PathScope::Subtree,
            path: String::new(),
        };
        assert_eq!(whole_scope(index, &sources), vec![whole(3), whole(6)]);
        assert_eq!(whole_scope(index, &sources[1..3]), Vec::<ScopeAtom>::new());
    }

    #[test]
    fn collections_follow_the_name() {
        assert_eq!(collection_from("My Docs (2026)"), "my-docs-2026");
        assert_eq!(collection_from("  AIRK  "), "airk");
        assert_eq!(collection_from("!!!"), "index");
    }
}
