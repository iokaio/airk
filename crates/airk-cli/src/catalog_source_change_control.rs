// SPDX-License-Identifier: Apache-2.0
//! Native read-only source reviews. The metadata actor revalidates these before
//! accepting the existing catalog fence; a frontend digest is never native proof.
use airk_core::{
    catalog_management::{
        CatalogEntity, CatalogItem, CatalogKind, CatalogOperation, CatalogPageRequest,
        CatalogRequest, CatalogSource, MAX_PAGE_BYTES, catalog_operation_id,
    },
    catalog_source_change::{
        IndexRetirementReason, SourceChangePlan, SourceConfigurationSnapshot, new_source_id,
        source_job_id,
    },
    catalog_source_draft::{
        SourceMutationDraft, SourceMutationDraftSelection, SourceMutationSelection,
    },
    identity::{Incarnation, Sha256Digest},
    index_catalog::{IndexDefinition, MAX_SOURCE_ROOTS, SourceImportState},
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
    source_policy::SourceRoot,
};
use airk_store::{
    roots::RootConfig,
    service::{
        collection::{Collection, settings_digest},
        data_scope,
        registry::ServiceRegistry,
    },
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, path::Path};

struct IndexReview {
    index: IndexDefinition,
    sources: Vec<CatalogSource>,
}
/// A native event can be discarded only after these explicit no-effect
/// observations. Other metadata/filesystem errors retain the accepted event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMissingSourceObsolete {
    RegistrationRetired,
    EpochChanged,
    RootReturned,
}
impl std::fmt::Display for NativeMissingSourceObsolete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RegistrationRetired => "native missing-source registration is retired",
            Self::EpochChanged => "native missing-source observation epochs changed",
            Self::RootReturned => "native missing-source path has returned",
        })
    }
}
impl std::error::Error for NativeMissingSourceObsolete {}
fn read_index(
    registry: &mut ServiceRegistry,
    selection: &SourceMutationDraftSelection,
) -> Result<IndexReview> {
    selection.validate()?;
    let state = registry
        .catalog_state()?
        .context("Indexing catalog unavailable")?;
    ensure!(
        state.activated
            && state.authority_contract == 1
            && state.policy_contract == 0
            && state.namespace == selection.key.namespace
            && state.revision == selection.expected_catalog_revision,
        "source review catalog changed"
    );
    let (namespace, epoch) = registry.saved_request_epoch()?;
    ensure!(
        namespace == selection.key.namespace && epoch == selection.key.epoch,
        "source draft request key expired"
    );
    let detail = registry.catalog_detail(
        &CatalogEntity::Index {
            id: selection.index_id,
        },
        Some(selection.expected_catalog_revision),
    )?;
    let Some(CatalogItem::Index(index)) = detail.item else {
        anyhow::bail!("selected index unavailable")
    };
    ensure!(
        detail.namespace == selection.key.namespace
            && detail.revision == selection.expected_catalog_revision
            && !index.tombstone
            && index.policy.is_none(),
        "source index is not ready for reviewed mutation"
    );
    ensure!(
        registry.get(index.backing.binding)?.registration == index.backing,
        "source registration binding changed"
    );
    // Retirement is also the exit from a failed or unknown source mutation.
    // Its native worker census and fence bind the immutable index backing;
    // it does not need an enabled source roster or readable RootConfig.
    if selection.change == SourceMutationSelection::RetireIndex {
        return Ok(IndexReview {
            index,
            sources: Vec::new(),
        });
    }
    ensure!(
        index.source_import == SourceImportState::Complete,
        "source index is not ready for reviewed mutation"
    );
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Sources,
        index_id: Some(index.id),
        project_id: None,
        after: None,
        expected_catalog_revision: Some(selection.expected_catalog_revision),
        limit: 32,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    ensure!(
        page.namespace == selection.key.namespace
            && page.revision == selection.expected_catalog_revision
            && page.continuation.is_none()
            && page.items.len() <= MAX_SOURCE_ROOTS,
        "source metadata review incomplete"
    );
    let sources = page
        .items
        .into_iter()
        .map(|item| match item {
            CatalogItem::Source(source) => Ok(source),
            _ => anyhow::bail!("source metadata kind changed"),
        })
        .collect::<Result<Vec<_>>>()?;
    catalog_snapshot(&index, &sources)?;
    Ok(IndexReview { index, sources })
}
fn catalog_snapshot(
    index: &IndexDefinition,
    sources: &[CatalogSource],
) -> Result<SourceConfigurationSnapshot> {
    let mut paths = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut owners = 0;
    for source in sources {
        ensure!(
            source.index_id == index.id
                && paths.insert(&source.canonical_path, source.id).is_none(),
            "source identity or path duplicated"
        );
        if !source.enabled {
            continue;
        }
        if source.owner {
            ensure!(
                source.canonical_path == index.backing.root && source.aliases.is_empty(),
                "catalog primary source changed"
            );
            owners += 1;
        } else {
            ensure!(
                source.aliases.len() == 1,
                "catalog source requires exactly one alias"
            );
            ensure!(
                aliases
                    .insert(source.aliases[0].clone(), source.canonical_path.clone())
                    .is_none(),
                "catalog source alias duplicated"
            );
        }
    }
    ensure!(owners == 1, "catalog primary source unavailable");
    let snapshot = SourceConfigurationSnapshot {
        primary_root: index.backing.root.clone(),
        aliases,
    };
    snapshot.validate()?;
    Ok(snapshot)
}
/// Native selected-collection observation only: no Kernel status, source walk,
/// extraction, index creation, configuration write or model opening.
fn inspect_configuration(
    control: &Path,
    namespace: Incarnation,
    review: &IndexReview,
    change: &SourceMutationSelection,
) -> Result<(
    ConfigurationRequest,
    SourceConfigurationSnapshot,
    SourceConfigurationSnapshot,
)> {
    let registration = &review.index.backing;
    airk_kernel::service_projects::validate_retained(registration)?;
    data_scope::check(Path::new(&registration.data_root), control, namespace)?;
    let selected = Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?
    .select()?;
    let settings = settings_digest(&selected.store, &registration.root)?;
    let incarnation = selected
        .store
        .service_incarnation()?
        .context("selected source collection has no incarnation")?;
    let mut roots = RootConfig::load(selected.store.directory(), &registration.root)?;
    let before = SourceConfigurationSnapshot {
        primary_root: registration.root.clone(),
        aliases: roots.roots.clone(),
    };
    ensure!(
        before == catalog_snapshot(&review.index, &review.sources)?,
        "catalog differs from actual native RootConfig"
    );
    let configuration = match change {
        SourceMutationSelection::AddRoot { alias, path } => {
            roots.add(
                alias,
                Path::new(path),
                Path::new(&registration.root),
                Path::new(&registration.data_root),
            )?;
            ProjectConfiguration::AddRoot {
                alias: alias.clone(),
                path: roots
                    .roots
                    .get(alias)
                    .context("prepared alias missing")?
                    .clone(),
            }
        }
        SourceMutationSelection::RemoveRoot { alias } => {
            roots.remove(alias)?;
            ProjectConfiguration::RemoveRoot {
                alias: alias.clone(),
            }
        }
        SourceMutationSelection::RetireIndex => {
            anyhow::bail!("retirement does not require RootConfig inspection")
        }
    };
    airk_store::roots::validate_sources(
        Path::new(&registration.root),
        Path::new(&registration.data_root),
        &roots.roots,
    )?;
    // An additional root may neither live inside storage nor contain it. Keep
    // inherited primary-root storage rules unchanged for the immutable backing.
    let data = Path::new(&registration.data_root);
    ensure!(
        roots
            .roots
            .values()
            .all(|root| !data.starts_with(root) && !Path::new(root).starts_with(data)),
        "additional source overlaps index storage"
    );
    let after = SourceConfigurationSnapshot {
        primary_root: registration.root.clone(),
        aliases: roots.roots,
    };
    before.validate()?;
    after.validate()?;
    ensure!(
        settings_digest(&selected.store, &registration.root)? == settings,
        "source settings changed during native review"
    );
    let configuration = ConfigurationRequest {
        prepare: false,
        incarnation,
        settings,
        configuration,
    };
    configuration.validate()?;
    Ok((configuration, before, after))
}
fn desired_sources(
    review: &IndexReview,
    after: &SourceConfigurationSnapshot,
    operation: Incarnation,
) -> Result<Vec<SourceRoot>> {
    let mut desired = vec![(after.primary_root.clone(), true, Vec::new())];
    desired.extend(
        after
            .aliases
            .iter()
            .map(|(alias, path)| (path.clone(), false, vec![alias.clone()])),
    );
    let mut sources = Vec::with_capacity(desired.len());
    for (path, owner, aliases) in desired {
        let id = if let Some(existing) = review
            .sources
            .iter()
            .find(|source| source.canonical_path == path)
        {
            ensure!(
                existing.owner == owner,
                "existing source owner cannot be rebound"
            );
            existing.id
        } else {
            new_source_id(review.index.id, operation, &path)?
        };
        let source = SourceRoot {
            id,
            canonical_path: path,
            aliases,
            owner,
        };
        source.validate()?;
        sources.push(source);
    }
    sources.sort_by_key(|source| source.id);
    Ok(sources)
}

pub(crate) fn draft(
    registry: &mut ServiceRegistry,
    control: &Path,
    selection: &SourceMutationDraftSelection,
) -> Result<SourceMutationDraft> {
    crate::runtime_install::validate_control_root(control)?;
    draft_native(
        registry,
        control,
        selection,
        crate::local_ipc::caller_identity()?,
        crate::runtime_pair::engine_identity()?,
    )
}
fn draft_native(
    registry: &mut ServiceRegistry,
    control: &Path,
    selection: &SourceMutationDraftSelection,
    caller: Incarnation,
    engine: Sha256Digest,
) -> Result<SourceMutationDraft> {
    let review = read_index(registry, selection)?;
    let operation = match &selection.change {
        SourceMutationSelection::RetireIndex => CatalogOperation::RetireIndex {
            id: review.index.id,
            expected_definition_revision: review.index.definition_revision,
            expected_source_revision: review.index.source_revision,
            reason: IndexRetirementReason::Explicit,
        },
        change => {
            let (configuration, before, after) =
                inspect_configuration(control, selection.key.namespace, &review, change)?;
            let operation = catalog_operation_id(caller, &selection.key)?;
            let plan = SourceChangePlan {
                registration: review.index.backing.clone(),
                engine_identity: engine,
                job: source_job_id(review.index.backing.binding, caller, &selection.key)?,
                configuration,
                before,
                sources: desired_sources(&review, &after, operation)?,
                after,
            };
            plan.validate()?;
            CatalogOperation::BeginIndexSourceChange {
                id: review.index.id,
                expected_source_revision: review.index.source_revision,
                plan: Box::new(plan),
            }
        }
    };
    let request = CatalogRequest {
        caller,
        key: selection.key.clone(),
        expected_catalog_revision: selection.expected_catalog_revision,
        operation,
    };
    let preview = registry.catalog_preview(&request)?;
    let preview_digest = preview.identity()?;
    let draft = SourceMutationDraft {
        request,
        preview,
        preview_digest,
    };
    draft.validate()?;
    Ok(draft)
}
/// Invoke for FIRST admission of generic SourceBegin/ExplicitRetire preview/apply.
/// Exact saved pending/terminal replay must be resolved before this call, because
/// accepted effects legitimately change the native before-state. Missing-source
/// retirement observations cannot arrive through this public-request path.
pub(crate) fn review_request(
    registry: &mut ServiceRegistry,
    control: &Path,
    request: &CatalogRequest,
    expected_preview: Option<Sha256Digest>,
) -> Result<SourceMutationDraft> {
    crate::runtime_install::validate_control_root(control)?;
    review_request_native(
        registry,
        control,
        request,
        expected_preview,
        crate::local_ipc::caller_identity()?,
        crate::runtime_pair::engine_identity()?,
    )
}
fn review_request_native(
    registry: &mut ServiceRegistry,
    control: &Path,
    request: &CatalogRequest,
    expected_preview: Option<Sha256Digest>,
    caller: Incarnation,
    engine: Sha256Digest,
) -> Result<SourceMutationDraft> {
    request.validate()?;
    ensure!(request.caller == caller, "foreign source draft caller");
    let (index_id, change) = match &request.operation {
        CatalogOperation::BeginIndexSourceChange { id, plan, .. } => {
            ensure!(
                plan.engine_identity == engine,
                "source draft runtime engine changed"
            );
            let change = match &plan.configuration.configuration {
                ProjectConfiguration::AddRoot { alias, path } => SourceMutationSelection::AddRoot {
                    alias: alias.clone(),
                    path: path.clone(),
                },
                ProjectConfiguration::RemoveRoot { alias } => SourceMutationSelection::RemoveRoot {
                    alias: alias.clone(),
                },
                _ => anyhow::bail!("source begin requires a native root-change review"),
            };
            (*id, change)
        }
        CatalogOperation::RetireIndex {
            id,
            reason: IndexRetirementReason::Explicit,
            ..
        } => (*id, SourceMutationSelection::RetireIndex),
        _ => anyhow::bail!("source request requires the native event or completion owner"),
    };
    let draft = draft_native(
        registry,
        control,
        &SourceMutationDraftSelection {
            key: request.key.clone(),
            expected_catalog_revision: request.expected_catalog_revision,
            index_id,
            change,
        },
        caller,
        engine,
    )?;
    ensure!(
        draft.request == *request
            && expected_preview.is_none_or(|digest| digest == draft.preview_digest),
        "actual native source review changed"
    );
    Ok(draft)
}

fn require_missing_with_plain_parent(root: &Path) -> Result<()> {
    ensure!(
        root.is_absolute(),
        "native missing source requires an absolute saved path"
    );
    let missing = || -> Result<()> {
        match std::fs::symlink_metadata(root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(NativeMissingSourceObsolete::RootReturned.into()),
            Err(error) => Err(error.into()),
        }
    };
    missing()?;
    let mut parent = root.parent();
    for _ in 0..256 {
        let path = parent.context("missing source has no verified existing parent")?;
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir()
                        && !metadata.file_type().is_symlink()
                        && path.canonicalize()? == path,
                    "missing source parent is redirected or unavailable"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "missing source parent is redirected"
                    );
                }
                return missing();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => parent = path.parent(),
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("missing source parent observation exceeds bound")
}
/// Private metadata-actor entry only. The event has no wire constructor: only
/// SourceSupervisor's validated native observation can produce it. This fresh
/// check admits the original native retirement decision, not a promise that a
/// missing path cannot subsequently reappear. Accepted retries use saved intent.
pub(crate) fn draft_missing_from_event(
    registry: &mut ServiceRegistry,
    control: &Path,
    selection: &SourceMutationDraftSelection,
    event: &crate::source_supervisor::NativeMissingSourceEvent,
) -> Result<SourceMutationDraft> {
    crate::runtime_install::validate_control_root(control)?;
    ensure!(
        selection.change == SourceMutationSelection::RetireIndex,
        "native missing event requires index retirement"
    );
    ensure!(
        registry.is_registered(event.binding())?,
        NativeMissingSourceObsolete::RegistrationRetired
    );
    let review = read_index(registry, selection)?;
    ensure!(
        review.index.backing.binding == event.binding(),
        "native missing event belongs to another index backing"
    );
    let watch = registry.watch(event.binding())?;
    ensure!(
        watch.project.registration == review.index.backing
            && watch.project.fencing_generation == event.registration_epoch()
            && watch.configuration_epoch == event.configuration_epoch()
            && watch.observation.enabled,
        NativeMissingSourceObsolete::EpochChanged
    );
    require_missing_with_plain_parent(Path::new(&review.index.backing.root))?;
    let request = CatalogRequest {
        caller: crate::local_ipc::caller_identity()?,
        key: selection.key.clone(),
        expected_catalog_revision: selection.expected_catalog_revision,
        operation: CatalogOperation::RetireIndex {
            id: review.index.id,
            expected_definition_revision: review.index.definition_revision,
            expected_source_revision: review.index.source_revision,
            reason: IndexRetirementReason::ConfirmedMissingSource {
                registration_epoch: event.registration_epoch(),
                configuration_epoch: event.configuration_epoch(),
            },
        },
    };
    let preview = registry.catalog_preview(&request)?;
    let preview_digest = preview.identity()?;
    let draft = SourceMutationDraft {
        request,
        preview,
        preview_digest,
    };
    draft.validate()?;
    Ok(draft)
}

#[cfg(test)]
#[path = "catalog_source_change_control_tests.rs"]
mod tests;
