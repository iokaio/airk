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
    index_catalog::{DenyId, IndexDefinition, MAX_SOURCE_ROOTS, ScopeAtom, SourceImportState},
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
    source_policy::{PolicyChange, SourcePolicy, SourceRoot},
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
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
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
            && !index.tombstone,
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
        ensure!(
            registry
                .catalog_active_split_for_binding(index.backing.binding)?
                .is_none(),
            "active split owns this participant; cancel or finish its exact recovery before retirement"
        );
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
        SourceMutationSelection::RetireIndex
        | SourceMutationSelection::TrimPath { .. }
        | SourceMutationSelection::UnblockPath { .. } => {
            anyhow::bail!("this mutation does not change RootConfig")
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

fn policy_rule_id(operation: Incarnation) -> Result<DenyId> {
    let digest =
        airk_core::identity::framed_digest(b"airk/native-trim-rule/1", &[operation.as_bytes()])
            .to_string();
    Ok(DenyId::from_identity(digest[..32].parse()?))
}

/// Resolve existing components read-only before review. Missing suffixes are
/// intentional future selectors, not permission to follow a redirected parent.
/// Native policy matching supplies the platform's case equivalence separately.
fn policy_selector(source: &CatalogSource, selector: &ScopeAtom) -> Result<ScopeAtom> {
    selector.validate()?;
    ensure!(
        selector.path.split('/').count() <= 256,
        "policy selector exceeds native component review budget"
    );
    ensure!(
        source.id == selector.source_root_id && source.enabled,
        "policy selector requires an enabled source identity"
    );
    let root = Path::new(&source.canonical_path);
    let canonical_root = root
        .canonicalize()
        .context("policy source root is unavailable")?;
    let same_root = canonical_root == root;
    #[cfg(windows)]
    let same_root = same_root
        || match (canonical_root.to_str(), root.to_str()) {
            (Some(canonical), Some(saved)) => {
                canonical.strip_prefix(r"\\?\").unwrap_or(canonical)
                    == saved.strip_prefix(r"\\?\").unwrap_or(saved)
            }
            _ => false,
        };
    ensure!(same_root, "policy source root binding changed");
    let mut path = canonical_root.clone();
    let mut missing = false;
    for component in selector
        .path
        .split('/')
        .filter(|component| !component.is_empty())
    {
        path.push(component);
        if !missing {
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    ensure!(
                        !metadata.file_type().is_symlink(),
                        "policy selector crosses a link"
                    );
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        ensure!(
                            metadata.file_attributes() & 0x400 == 0,
                            "policy selector crosses a reparse point"
                        );
                    }
                    path = path.canonicalize()?;
                    ensure!(
                        path.starts_with(&canonical_root),
                        "policy selector escapes its source root"
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing = true,
                Err(error) => return Err(error.into()),
            }
        }
    }
    if !missing {
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            match selector.kind {
                airk_core::index_catalog::PathScope::Exact => metadata.is_file(),
                airk_core::index_catalog::PathScope::Subtree => metadata.is_dir(),
            },
            "policy selector kind differs from its source object"
        );
    }
    let relative = path
        .strip_prefix(&canonical_root)
        .context("policy selector source binding changed")?;
    let path = relative
        .components()
        .map(|part| {
            part.as_os_str()
                .to_str()
                .context("non-UTF-8 policy selector is unsupported")
        })
        .collect::<Result<Vec<_>>>()?
        .join("/");
    let canonical = ScopeAtom {
        source_root_id: source.id,
        kind: selector.kind,
        path,
    };
    canonical.validate()?;
    Ok(canonical)
}

fn inspect_policy_configuration(
    registry: &ServiceRegistry,
    control: &Path,
    namespace: Incarnation,
    review: &IndexReview,
    change: &PolicyChange,
    operation_id: Incarnation,
) -> Result<ConfigurationRequest> {
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
        .context("selected policy collection has no incarnation")?;
    let configured = RootConfig::load(selected.store.directory(), &registration.root)?;
    ensure!(
        SourceConfigurationSnapshot {
            primary_root: registration.root.clone(),
            aliases: configured.roots
        } == catalog_snapshot(&review.index, &review.sources)?,
        "catalog differs from actual native RootConfig"
    );
    let after = if let Some(policy) = registry.catalog_source_policy(review.index.id)? {
        policy
            .preview_change(policy.identity()?, change.clone())?
            .after
    } else {
        let PolicyChange::Trim { rule_id, selector } = change else {
            anyhow::bail!("unblock requires an existing permanent rule");
        };
        let roots = review
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| SourceRoot {
                id: source.id,
                canonical_path: source.canonical_path.clone(),
                aliases: source.aliases.clone(),
                owner: source.owner,
            })
            .collect();
        SourcePolicy::initial_trim(&review.index, roots, *rule_id, selector.clone())?
    };
    let request = ConfigurationRequest {
        prepare: false,
        incarnation,
        settings,
        configuration: ProjectConfiguration::ApplySourcePolicy {
            policy: Box::new(after),
            operation_id,
        },
    };
    request.validate()?;
    ensure!(
        settings_digest(&selected.store, &registration.root)? == settings,
        "source settings changed during native policy review"
    );
    Ok(request)
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
    let review = read_index(registry, selection).context("native source draft index review")?;
    let operation = match &selection.change {
        SourceMutationSelection::TrimPath {
            source_root_id,
            kind,
            path,
        } => {
            let operation_id = catalog_operation_id(caller, &selection.key)?;
            let source = review
                .sources
                .iter()
                .find(|source| source.id == *source_root_id)
                .context("selected policy source is unavailable")?;
            let selector = policy_selector(
                source,
                &ScopeAtom {
                    source_root_id: *source_root_id,
                    kind: *kind,
                    path: path.clone(),
                },
            )
            .context("native trim selector review")?;
            let change = PolicyChange::Trim {
                rule_id: policy_rule_id(operation_id)?,
                selector,
            };
            let configuration = inspect_policy_configuration(
                registry,
                control,
                selection.key.namespace,
                &review,
                &change,
                operation_id,
            )
            .context("native trim configuration review")?;
            CatalogOperation::ChangeIndexPolicy {
                id: review.index.id,
                expected_source_revision: review.index.source_revision,
                expected_policy_epoch: review
                    .index
                    .policy
                    .as_ref()
                    .map_or(0, |policy| policy.epoch.get()),
                expected_policy_digest: review.index.policy.as_ref().map(|policy| policy.digest),
                change,
                configuration: Box::new(configuration),
                engine_identity: engine,
            }
        }
        SourceMutationSelection::UnblockPath { rule_id } => {
            let operation_id = catalog_operation_id(caller, &selection.key)?;
            let change = PolicyChange::Unblock { rule_id: *rule_id };
            let configuration = inspect_policy_configuration(
                registry,
                control,
                selection.key.namespace,
                &review,
                &change,
                operation_id,
            )
            .context("native unblock configuration review")?;
            CatalogOperation::ChangeIndexPolicy {
                id: review.index.id,
                expected_source_revision: review.index.source_revision,
                expected_policy_epoch: review
                    .index
                    .policy
                    .as_ref()
                    .map_or(0, |policy| policy.epoch.get()),
                expected_policy_digest: review.index.policy.as_ref().map(|policy| policy.digest),
                change,
                configuration: Box::new(configuration),
                engine_identity: engine,
            }
        }
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
    let preview = registry
        .catalog_preview(&request)
        .context("canonical native source draft preview")?;
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
        CatalogOperation::ChangeIndexPolicy {
            id,
            change,
            engine_identity,
            ..
        } => {
            ensure!(
                *engine_identity == engine,
                "policy draft runtime engine changed"
            );
            let selection = match change {
                PolicyChange::Trim { selector, .. } => SourceMutationSelection::TrimPath {
                    source_root_id: selector.source_root_id,
                    kind: selector.kind,
                    path: selector.path.clone(),
                },
                PolicyChange::Unblock { rule_id } => {
                    SourceMutationSelection::UnblockPath { rule_id: *rule_id }
                }
                PolicyChange::SplitRemainder { .. } => {
                    anyhow::bail!("split cutover requires the accepted native split owner")
                }
            };
            (*id, selection)
        }
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

/// Build only the cutover already authorized by a durable split review. The
/// original worker has performed the bounded live-source witness; this metadata
/// turn does not walk or hash source files.
pub(crate) fn split_cutover(
    registry: &mut ServiceRegistry,
    control: &Path,
    snapshot: &airk_core::catalog_split::SplitSnapshot,
) -> Result<(CatalogRequest, Sha256Digest)> {
    split_cutover_with_engine(
        registry,
        control,
        snapshot,
        crate::runtime_pair::engine_identity()?,
    )
}

// Private unit fixture executes the actual accepted work in this test process.
// It cannot attest installed image identity or live provider/process drains.
#[cfg(all(test, feature = "local-inference"))]
pub(crate) fn split_cutover_for_native_fixture(
    registry: &mut ServiceRegistry,
    control: &Path,
    snapshot: &airk_core::catalog_split::SplitSnapshot,
    engine: Sha256Digest,
) -> Result<(CatalogRequest, Sha256Digest)> {
    split_cutover_with_engine(registry, control, snapshot, engine)
}

fn split_cutover_with_engine(
    registry: &mut ServiceRegistry,
    control: &Path,
    snapshot: &airk_core::catalog_split::SplitSnapshot,
    engine: Sha256Digest,
) -> Result<(CatalogRequest, Sha256Digest)> {
    use airk_core::{catalog_split::SplitStage, identity::framed_digest};
    snapshot.validate()?;
    ensure!(
        snapshot.receipt.stage == SplitStage::DestinationValidated,
        "split destination has not validated"
    );
    let current = registry
        .catalog_split_status(snapshot.receipt.operation_id)?
        .context("accepted split disappeared")?;
    ensure!(current == *snapshot, "split cutover review changed");
    let registration = &snapshot.request.original_registration;
    data_scope::check(
        Path::new(&registration.data_root),
        control,
        snapshot.request.key.namespace,
    )?;
    let selected = Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?
    .select()?;
    ensure!(
        selected.store.service_incarnation()? == Some(snapshot.request.original_incarnation)
            && airk_store::service::collection::inspect_settings_digest(
                &selected.store,
                &registration.root
            )? == snapshot.request.original_settings_digest,
        "split original native settings changed"
    );
    ensure!(
        RootConfig::load(selected.store.directory(), &registration.root)?.roots
            == snapshot.request.source_configuration.aliases,
        "split original source configuration changed"
    );
    let (namespace, epoch) = registry.saved_request_epoch()?;
    ensure!(
        namespace == snapshot.request.key.namespace,
        "split namespace changed"
    );
    let nonce = framed_digest(
        b"airk/native-split-cutover-key/1",
        &[
            snapshot.receipt.operation_id.as_bytes(),
            snapshot.receipt_digest.as_bytes(),
        ],
    )
    .to_string()[..32]
        .parse()?;
    let key = airk_core::service::RequestKey {
        namespace,
        epoch,
        nonce,
    };
    let operation_id = catalog_operation_id(snapshot.request.caller, &key)?;
    let state = registry
        .catalog_state()?
        .context("split catalog unavailable")?;
    let persisted = match &snapshot.request.baseline {
        airk_core::catalog_split::SplitBaseline::Persisted { reference } => Some(reference),
        airk_core::catalog_split::SplitBaseline::VirtualUnrestricted => None,
    };
    let request = CatalogRequest {
        caller: snapshot.request.caller,
        key,
        expected_catalog_revision: state.revision,
        operation: CatalogOperation::ChangeIndexPolicy {
            id: snapshot.request.original_index,
            expected_source_revision: snapshot.request.expected_source_revision,
            expected_policy_epoch: persisted.map_or(0, |reference| reference.epoch.get()),
            expected_policy_digest: persisted.map(|reference| reference.digest),
            change: PolicyChange::SplitRemainder {
                split_operation: snapshot.receipt.operation_id,
                expected_split_receipt: snapshot.receipt_digest,
            },
            configuration: Box::new(ConfigurationRequest {
                prepare: false,
                incarnation: snapshot.request.original_incarnation,
                settings: snapshot.request.original_settings_digest,
                configuration: ProjectConfiguration::ApplySourcePolicy {
                    policy: Box::new(snapshot.plan.remainder.clone()),
                    operation_id,
                },
            }),
            engine_identity: engine,
        },
    };
    let preview = registry.catalog_preview(&request)?.identity()?;
    Ok((request, preview))
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
mod tests;
