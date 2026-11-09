// SPDX-License-Identifier: Apache-2.0
//! Native configuration observations and source-ID invariants. These component
//! tests do not qualify actor/worker transport, hierarchy activation or indexing.
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, IndexId, SourceRootId},
    service::ProjectRegistration,
};
use std::path::PathBuf;

fn assert_wrong_file_change_flag_refused(draft: &SourceMutationDraft) -> Result<()> {
    let mut changed = draft.clone();
    changed.preview.file_data_changes = !changed.preview.file_data_changes;
    // Rebind the representation digest so this exercises the operation's exact
    // effect contract, rather than merely detecting altered serialized bytes.
    changed.preview_digest = changed.preview.identity()?;
    assert!(
        changed.validate().is_err(),
        "source draft accepted a misrepresented derived-file effect"
    );
    Ok(())
}

fn policy_registry_fixture() -> Result<(
    tempfile::TempDir,
    PathBuf,
    ServiceRegistry,
    IndexDefinition,
    CatalogSource,
)> {
    use airk_store::service::registry::catalog::upgrade::CatalogUpgrade;
    let (owner, control, review) = fixture()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(&control, instance)?;
    registry.register(&review.index.backing)?;
    drop(registry);
    let upgrade = CatalogUpgrade::acquire(&control)?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, Sha256Digest::from_bytes([2; 32]))?;
    upgrade.migrate()?;
    drop(upgrade);
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute_batch(include_str!(
        "../../../airk-store/src/service_catalog_authority_v1.sql"
    ))?;
    tx.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    tx.commit()?;
    drop(sql);
    let owner_path = control.join("service-1.4.owner.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner_path)?)?;
    record.as_object_mut().unwrap().remove("catalog_upgrade");
    std::fs::write(owner_path, serde_json::to_vec(&record)?)?;
    let mut registry = ServiceRegistry::open(&control, instance)?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 2,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let [CatalogItem::Index(index)] = page.items.as_slice() else {
        anyhow::bail!("expected one imported native index");
    };
    let index = index.clone();
    let sources = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Sources,
        index_id: Some(index.id),
        project_id: None,
        after: None,
        expected_catalog_revision: Some(page.revision),
        limit: 32,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let source = sources
        .items
        .into_iter()
        .find_map(|item| match item {
            CatalogItem::Source(source) if source.owner => Some(source),
            _ => None,
        })
        .context("native primary source missing")?;
    Ok((owner, control, registry, index, source))
}

fn apply_policy_draft_fixture(
    registry: &mut ServiceRegistry,
    draft: &SourceMutationDraft,
    index: &IndexDefinition,
) -> Result<()> {
    use airk_core::catalog_fences::{FencePlan, FencePreparation, WorkerDrainObligation};
    let plan = FencePlan::new(Incarnation::from_bytes([87; 16]), 1, vec![])?
        .with_worker_obligation(WorkerDrainObligation {
            index_id: index.id,
            backing: index.backing.binding,
            workers: vec![],
            providers: vec![],
        })?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&draft.request, draft.preview_digest, &plan)?
    else {
        anyhow::bail!("fresh native policy fixture fence expected");
    };
    registry.catalog_record_fence_confirmations(
        draft.request.caller,
        &draft.request.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&draft.request, draft.preview_digest, intent.prepared_digest)?;
    Ok(())
}

#[test]
fn native_unblock_review_recovers_terminal_pending_trim_without_requalifying_removed_selector()
-> Result<()> {
    use airk_core::{
        index_catalog::PathScope,
        service::{JobState, RequestKey},
    };
    let (_owner, control, mut registry, index, source) = policy_registry_fixture()?;
    let caller = Incarnation::from_bytes([1; 16]);
    let engine = Sha256Digest::from_bytes([2; 32]);
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let mut selection = SourceMutationDraftSelection {
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([84; 16]),
        },
        expected_catalog_revision: registry.catalog_state()?.unwrap().revision,
        index_id: index.id,
        change: SourceMutationSelection::TrimPath {
            source_root_id: source.id,
            kind: PathScope::Exact,
            path: "original.txt".into(),
        },
    };
    let initial = draft_native(&mut registry, &control, &selection, caller, engine)?;
    apply_policy_draft_fixture(&mut registry, &initial, &index)?;
    let pending = registry
        .catalog_policy_status(index.id)?
        .context("admitted native policy missing")?;
    assert!(pending.application.is_none());
    let rule_id = pending.policy.denies[0].rule_id;
    selection.key.nonce = Incarnation::from_bytes([85; 16]);
    selection.expected_catalog_revision = registry.catalog_state()?.unwrap().revision;
    selection.change = SourceMutationSelection::UnblockPath { rule_id };
    assert!(
        draft_native(&mut registry, &control, &selection, caller, engine).is_err(),
        "unblock cannot supersede an accepted pending/running cleanup job"
    );
    assert_eq!(
        registry.cancel_pending(pending.job)?.state,
        JobState::Canceled
    );

    // The original selector can no longer be qualified as an exact file. A
    // separately reviewed unblock validates the remaining policy, not the
    // selector being removed, and never opens the old indexed payload.
    let original_path = Path::new(&source.canonical_path).join("original.txt");
    let preserved_path = Path::new(&source.canonical_path).join("preserved.txt");
    std::fs::rename(&original_path, &preserved_path)?;
    std::fs::create_dir(&original_path)?;
    let old_selector = pending.policy.denies[0].atom();
    assert!(policy_selector(&source, &old_selector).is_err());
    let unblock = draft_native(&mut registry, &control, &selection, caller, engine)
        .context("reviewed unblock escape after terminal native trim")?;
    unblock.validate()?;
    let CatalogOperation::ChangeIndexPolicy {
        change,
        configuration,
        expected_policy_epoch,
        ..
    } = &unblock.request.operation
    else {
        anyhow::bail!("unblock omitted canonical policy operation");
    };
    assert_eq!(*change, PolicyChange::Unblock { rule_id });
    assert_eq!(*expected_policy_epoch, pending.policy.epoch.get());
    let ProjectConfiguration::ApplySourcePolicy { policy, .. } = &configuration.configuration
    else {
        anyhow::bail!("unblock omitted exact after-policy configuration");
    };
    assert!(policy.denies.is_empty());
    assert_eq!(policy.epoch, pending.policy.epoch.next()?);
    assert_eq!(policy.roots, pending.policy.roots);
    assert_eq!(
        registry.catalog_source_policy(index.id)?.unwrap(),
        pending.policy,
        "review itself must retain the active deny"
    );
    apply_policy_draft_fixture(&mut registry, &unblock, &index)?;
    let after = registry.catalog_policy_status(index.id)?.unwrap();
    assert!(after.policy.denies.is_empty());
    assert!(after.application.is_none());
    assert_ne!(after.job, pending.job);
    let retry_key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([86; 16]),
    };
    assert!(
        registry
            .catalog_resume_policy_job(index.id, pending.operation_id, caller, &retry_key)
            .is_err(),
        "an explicit unblock must not revive the old failed trim lineage"
    );
    assert_eq!(std::fs::read(preserved_path)?, b"original source bytes");
    assert!(original_path.is_dir());
    Ok(())
}

#[test]
fn policy_selectors_preserve_sources_and_reject_wrong_roots_kinds_and_traversal() -> Result<()> {
    use airk_core::index_catalog::PathScope;
    let (_owner, _control, review) = fixture()?;
    let source = &review.sources[0];
    let original = std::fs::read(Path::new(&source.canonical_path).join("original.txt"))?;
    let exact = ScopeAtom {
        source_root_id: source.id,
        kind: PathScope::Exact,
        path: "original.txt".into(),
    };
    assert_eq!(policy_selector(source, &exact)?, exact);
    let mut subtree = exact.clone();
    subtree.kind = PathScope::Subtree;
    assert!(policy_selector(source, &subtree).is_err());
    subtree.path.clear();
    assert_eq!(policy_selector(source, &subtree)?, subtree);
    let mut wrong = exact.clone();
    wrong.source_root_id = review.sources[1].id;
    assert!(policy_selector(source, &wrong).is_err());
    for path in [
        "../secret",
        "a/../secret",
        "/secret",
        "a\\secret",
        "a//secret",
        "C:secret",
    ] {
        wrong = exact.clone();
        wrong.path = path.into();
        assert!(policy_selector(source, &wrong).is_err(), "{path}");
    }
    let future = ScopeAtom {
        source_root_id: source.id,
        kind: PathScope::Subtree,
        path: "future/private".into(),
    };
    assert_eq!(policy_selector(source, &future)?, future);
    assert!(!Path::new(&source.canonical_path).join("future").exists());
    assert_eq!(
        std::fs::read(Path::new(&source.canonical_path).join("original.txt"))?,
        original
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn policy_selectors_refuse_redirected_existing_parents() -> Result<()> {
    let (owner, _control, review) = fixture()?;
    let source = &review.sources[0];
    std::os::unix::fs::symlink(
        owner.path().join("support"),
        Path::new(&source.canonical_path).join("redirect"),
    )?;
    let selector = ScopeAtom {
        source_root_id: source.id,
        kind: airk_core::index_catalog::PathScope::Subtree,
        path: "redirect/new-child".into(),
    };
    assert!(policy_selector(source, &selector).is_err());
    assert!(!owner.path().join("support/new-child").exists());
    Ok(())
}

#[test]
fn native_trim_rule_identity_is_stable_and_does_not_accept_frontend_rule_ids() -> Result<()> {
    let operation = Incarnation::from_bytes([17; 16]);
    assert_eq!(policy_rule_id(operation)?, policy_rule_id(operation)?);
    assert_ne!(
        policy_rule_id(operation)?,
        policy_rule_id(Incarnation::from_bytes([18; 16]))?
    );
    let value = serde_json::json!({"operation":"trim_path","source_root_id":SourceRootId::from_identity(operation),
        "kind":"subtree","path":"private","rule_id":policy_rule_id(operation)?});
    assert!(serde_json::from_value::<SourceMutationSelection>(value).is_err());
    Ok(())
}

#[test]
fn complete_native_trim_draft_binds_canonical_policy_configuration_and_review_without_writes()
-> Result<()> {
    use airk_core::{index_catalog::PathScope, service::RequestKey};
    use airk_store::service::registry::catalog::upgrade::CatalogUpgrade;
    let (_owner, control, review) = fixture()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let engine = Sha256Digest::from_bytes([2; 32]);
    let mut registry = ServiceRegistry::open(&control, instance)?;
    registry.register(&review.index.backing)?;
    drop(registry);
    let upgrade = CatalogUpgrade::acquire(&control)?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, engine)?;
    upgrade.migrate()?;
    drop(upgrade);
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute_batch(include_str!(
        "../../../airk-store/src/service_catalog_authority_v1.sql"
    ))?;
    tx.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    tx.commit()?;
    drop(sql);
    let owner_path = control.join("service-1.4.owner.json");
    let mut owner: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner_path)?)?;
    owner.as_object_mut().unwrap().remove("catalog_upgrade");
    std::fs::write(owner_path, serde_json::to_vec(&owner)?)?;
    let mut registry = ServiceRegistry::open(&control, instance)?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 2,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let [CatalogItem::Index(index)] = page.items.as_slice() else {
        anyhow::bail!("expected one imported native index");
    };
    let index = index.clone();
    let sources = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Sources,
        index_id: Some(index.id),
        project_id: None,
        after: None,
        expected_catalog_revision: Some(page.revision),
        limit: 32,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let source = sources
        .items
        .iter()
        .find_map(|item| match item {
            CatalogItem::Source(source) if source.owner => Some(source.clone()),
            _ => None,
        })
        .context("native primary source missing")?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let selection = SourceMutationDraftSelection {
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([83; 16]),
        },
        expected_catalog_revision: page.revision,
        index_id: index.id,
        change: SourceMutationSelection::TrimPath {
            source_root_id: source.id,
            kind: PathScope::Exact,
            path: "original.txt".into(),
        },
    };
    let settings = Collection::existing(
        Path::new(&index.backing.data_root),
        &index.backing.root,
        &index.backing.collection,
    )?
    .select()?;
    let original_settings = settings_digest(&settings.store, &index.backing.root)?;
    let original_source = std::fs::read(Path::new(&source.canonical_path).join("original.txt"))?;
    let draft = draft_native(&mut registry, &control, &selection, instance, engine)
        .context("complete positive native trim draft")?;
    draft.validate()?;
    let CatalogOperation::ChangeIndexPolicy {
        id,
        expected_source_revision,
        expected_policy_epoch,
        expected_policy_digest,
        change,
        configuration,
        engine_identity,
    } = &draft.request.operation
    else {
        anyhow::bail!("native trim omitted canonical policy operation");
    };
    assert_eq!(*id, index.id);
    assert_eq!(*expected_source_revision, index.source_revision);
    assert_eq!(*expected_policy_epoch, 0);
    assert!(expected_policy_digest.is_none());
    assert_eq!(*engine_identity, engine);
    let operation_id = catalog_operation_id(instance, &selection.key)?;
    let PolicyChange::Trim { rule_id, selector } = change else {
        anyhow::bail!("native trim omitted deny selector");
    };
    assert_eq!(*rule_id, policy_rule_id(operation_id)?);
    assert_eq!(selector.source_root_id, source.id);
    assert_eq!(selector.path, "original.txt");
    let ProjectConfiguration::ApplySourcePolicy {
        policy,
        operation_id: queued_operation,
    } = &configuration.configuration
    else {
        anyhow::bail!("native trim omitted exact policy configuration");
    };
    assert_eq!(*queued_operation, operation_id);
    assert_eq!(policy.epoch, CatalogRevision::INITIAL);
    assert!(!policy.allows_canonical(source.id, "original.txt"));
    assert_eq!(configuration.settings, original_settings);
    assert!(draft.preview.authority_fence_required && draft.preview.file_data_changes);
    assert_wrong_file_change_flag_refused(&draft)?;
    let impact = draft
        .preview
        .source_policy_impact
        .as_ref()
        .context("native trim omitted reviewed impact")?;
    assert_eq!(&impact.target, selector);
    assert_eq!(impact.canonical_root, source.canonical_path);
    assert_eq!(impact.affected_source_root_ids, vec![source.id]);
    assert!(!impact.affected_project_ids.is_empty());
    assert!(impact.other_covering_index_ids.is_empty());
    assert!(
        impact.affected_artifacts.is_none()
            && impact.affected_vectors.is_none()
            && impact.affected_facts.is_none()
            && impact.affected_relationships.is_none(),
        "metadata-only review cannot manufacture evidence counts"
    );
    let rebuilt = review_request_native(
        &mut registry,
        &control,
        &draft.request,
        Some(draft.preview_digest),
        instance,
        engine,
    )?;
    assert_eq!(rebuilt, draft);
    assert_eq!(registry.catalog_state()?.unwrap().revision, page.revision);
    assert!(registry.catalog_source_policy(index.id)?.is_none());
    assert!(
        registry
            .request_receipt(instance, &selection.key)?
            .0
            .is_none()
    );
    assert_eq!(
        settings_digest(&settings.store, &index.backing.root)?,
        original_settings
    );
    assert_eq!(
        std::fs::read(Path::new(&source.canonical_path).join("original.txt"))?,
        original_source
    );
    Ok(())
}

#[test]
fn native_retirement_draft_remains_available_after_accepted_source_change() -> Result<()> {
    use airk_core::{
        catalog_fences::{FencePlan, FencePreparation},
        service::RequestKey,
    };
    use airk_store::service::registry::catalog::upgrade::CatalogUpgrade;
    let (_owner, control, review) = fixture()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(&control, instance)?;
    registry.register(&review.index.backing)?;
    drop(registry);
    let upgrade = CatalogUpgrade::acquire(&control)?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, Sha256Digest::from_bytes([2; 32]))?;
    upgrade.migrate()?;
    drop(upgrade);
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute_batch(include_str!(
        "../../../airk-store/src/service_catalog_authority_v1.sql"
    ))?;
    tx.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    tx.commit()?;
    drop(sql);
    let owner_path = control.join("service-1.4.owner.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner_path)?)?;
    record.as_object_mut().unwrap().remove("catalog_upgrade");
    std::fs::write(owner_path, serde_json::to_vec(&record)?)?;
    let mut registry = ServiceRegistry::open(&control, instance)?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 2,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let [CatalogItem::Index(index)] = page.items.as_slice() else {
        anyhow::bail!("expected imported fixture index")
    };
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let mut selection = SourceMutationDraftSelection {
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([80; 16]),
        },
        expected_catalog_revision: page.revision,
        index_id: index.id,
        change: SourceMutationSelection::RemoveRoot {
            alias: "support".into(),
        },
    };
    let begin = draft_native(
        &mut registry,
        &control,
        &selection,
        instance,
        Sha256Digest::from_bytes([2; 32]),
    )?;
    assert!(!begin.preview.file_data_changes);
    assert_wrong_file_change_flag_refused(&begin)?;
    let plan = FencePlan::new(Incarnation::from_bytes([81; 16]), 1, vec![])?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&begin.request, begin.preview_digest, &plan)?
    else {
        anyhow::bail!("expected fresh source begin")
    };
    registry.catalog_record_fence_confirmations(
        instance,
        &begin.request.key,
        intent.prepared_digest,
        &[],
    )?;
    let applied = registry.catalog_apply_fenced(
        &begin.request,
        begin.preview_digest,
        intent.prepared_digest,
    )?;
    selection.expected_catalog_revision = applied.metadata.catalog_revision;
    selection.key.nonce = Incarnation::from_bytes([82; 16]);
    assert!(
        read_index(&mut registry, &selection).is_err(),
        "further source mutation must wait for actual completion"
    );
    selection.change = SourceMutationSelection::RetireIndex;
    let retirement = draft_native(
        &mut registry,
        &control,
        &selection,
        instance,
        Sha256Digest::from_bytes([2; 32]),
    )?;
    assert!(!retirement.preview.file_data_changes);
    assert_wrong_file_change_flag_refused(&retirement)?;
    assert!(
        matches!(retirement.request.operation,CatalogOperation::RetireIndex {expected_source_revision,..} if expected_source_revision==CatalogRevision::INITIAL.next()?)
    );
    assert!(
        read_index(&mut registry, &selection)?.sources.is_empty(),
        "retirement must not enumerate disabled source paths"
    );
    Ok(())
}

fn fixture() -> Result<(tempfile::TempDir, PathBuf, IndexReview)> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    for name in ["source", "support", "extra", "data"] {
        std::fs::create_dir(base.join(name))?;
    }
    let control = base.join("control");
    crate::local_ipc::prepare_root(&control)?;
    let namespace = Incarnation::from_bytes([1; 16]);
    let registration = ProjectRegistration::new(
        base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "source-draft".into(),
    )?;
    let _bound = data_scope::bind(Path::new(&registration.data_root), &control, namespace)?;
    let collection = Collection::initialize(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([10; 16]),
    )?;
    let selected = collection.select()?;
    let writer = selected.store.lock()?;
    let mut roots = RootConfig::load(selected.store.directory(), &registration.root)?;
    roots.add(
        "support",
        &base.join("support"),
        Path::new(&registration.root),
        Path::new(&registration.data_root),
    )?;
    roots.save(selected.store.directory())?;
    drop(writer);
    drop(selected);
    let index = IndexDefinition {
        id: IndexId::from_identity(Incarnation::from_bytes([11; 16])),
        name: "Source draft fixture".into(),
        backing: registration.clone(),
        definition_revision: CatalogRevision::INITIAL,
        source_revision: CatalogRevision::INITIAL,
        policy: None,
        tombstone: false,
        source_import: SourceImportState::Complete,
    };
    let sources = vec![
        CatalogSource {
            id: SourceRootId::from_identity(Incarnation::from_bytes([12; 16])),
            index_id: index.id,
            canonical_path: registration.root.clone(),
            owner: true,
            enabled: true,
            aliases: vec![],
        },
        CatalogSource {
            id: SourceRootId::from_identity(Incarnation::from_bytes([13; 16])),
            index_id: index.id,
            canonical_path: base.join("support").to_str().unwrap().into(),
            owner: false,
            enabled: true,
            aliases: vec!["support".into()],
        },
    ];
    std::fs::write(base.join("source/original.txt"), b"original source bytes")?;
    Ok((owner, control, IndexReview { index, sources }))
}

#[test]
fn actual_owned_inspection_preserves_collection_identity_and_changes_no_configuration() -> Result<()>
{
    let (owner, control, review) = fixture()?;
    let collection = Collection::existing(
        Path::new(&review.index.backing.data_root),
        &review.index.backing.root,
        &review.index.backing.collection,
    )?;
    let selected = collection.select()?;
    let original = std::fs::read(selected.store.directory().join("roots.json"))?;
    let settings = settings_digest(&selected.store, &review.index.backing.root)?;
    let (configuration, before, after) = inspect_configuration(
        &control,
        Incarnation::from_bytes([1; 16]),
        &review,
        &SourceMutationSelection::AddRoot {
            alias: "extra".into(),
            path: owner
                .path()
                .canonicalize()?
                .join("extra")
                .to_str()
                .unwrap()
                .into(),
        },
    )?;
    assert_eq!(configuration.incarnation, Incarnation::from_bytes([10; 16]));
    assert_ne!(
        configuration.incarnation,
        Incarnation::from_bytes([1; 16]),
        "collection incarnation is not the registry namespace"
    );
    assert_eq!(configuration.settings, settings);
    assert_eq!(before.aliases.len(), 1);
    assert_eq!(after.aliases.len(), 2);
    assert!(!before.aliases.contains_key("extra"));
    assert_eq!(
        std::fs::read(selected.store.directory().join("roots.json"))?,
        original
    );
    assert_eq!(
        std::fs::read(Path::new(&review.index.backing.root).join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}

#[test]
fn native_inspection_refuses_incoherent_catalog_and_storage_overlap() -> Result<()> {
    let (owner, control, mut review) = fixture()?;
    review.sources[1].aliases = vec!["changed".into()];
    assert!(
        inspect_configuration(
            &control,
            Incarnation::from_bytes([1; 16]),
            &review,
            &SourceMutationSelection::RemoveRoot {
                alias: "support".into()
            }
        )
        .is_err()
    );
    review.sources[1].aliases = vec!["support".into()];
    assert!(
        inspect_configuration(
            &control,
            Incarnation::from_bytes([1; 16]),
            &review,
            &SourceMutationSelection::AddRoot {
                alias: "private".into(),
                path: review.index.backing.data_root.clone()
            }
        )
        .is_err()
    );
    assert!(
        inspect_configuration(
            &control,
            Incarnation::from_bytes([1; 16]),
            &review,
            &SourceMutationSelection::AddRoot {
                alias: "parent".into(),
                path: owner.path().canonicalize()?.to_str().unwrap().into()
            }
        )
        .is_err()
    );
    assert!(
        inspect_configuration(
            &control,
            Incarnation::from_bytes([1; 16]),
            &review,
            &SourceMutationSelection::RemoveRoot {
                alias: "absent".into()
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn desired_sources_preserve_existing_and_disabled_physical_identities() -> Result<()> {
    let (owner, _control, mut review) = fixture()?;
    let base = owner.path().canonicalize()?;
    let extra = base.join("extra").to_str().unwrap().to_owned();
    let historical = SourceRootId::from_identity(Incarnation::from_bytes([20; 16]));
    review.sources.push(CatalogSource {
        id: historical,
        index_id: review.index.id,
        canonical_path: extra.clone(),
        owner: false,
        enabled: false,
        aliases: vec![],
    });
    let mut after = catalog_snapshot(&review.index, &review.sources)?;
    after.aliases.insert("extra".into(), extra.clone());
    let operation = Incarnation::from_bytes([21; 16]);
    let desired = desired_sources(&review, &after, operation)?;
    assert_eq!(
        desired
            .iter()
            .find(|source| source.canonical_path == review.index.backing.root)
            .unwrap()
            .id,
        review.sources[0].id
    );
    assert_eq!(
        desired
            .iter()
            .find(|source| source.canonical_path == extra)
            .unwrap()
            .id,
        historical
    );
    review.sources.pop();
    let new = desired_sources(&review, &after, operation)?;
    assert_eq!(
        new.iter()
            .find(|source| source.canonical_path == extra)
            .unwrap()
            .id,
        new_source_id(review.index.id, operation, &extra)?
    );
    assert_ne!(
        new.iter()
            .find(|source| source.canonical_path == extra)
            .unwrap()
            .id,
        new_source_id(review.index.id, Incarnation::from_bytes([22; 16]), &extra)?
    );
    Ok(())
}

#[test]
fn missing_event_review_requires_not_found_and_a_real_canonical_parent() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let missing = base.join("missing");
    require_missing_with_plain_parent(&missing)?;
    require_missing_with_plain_parent(&base.join("gone/child"))?;
    // Explicit fixture changes model a path returning before native admission.
    std::fs::write(&missing, b"returned source")?;
    let returned = require_missing_with_plain_parent(&missing).unwrap_err();
    assert_eq!(
        returned.downcast_ref::<NativeMissingSourceObsolete>(),
        Some(&NativeMissingSourceObsolete::RootReturned)
    );
    assert_eq!(std::fs::read(&missing)?, b"returned source");
    assert!(require_missing_with_plain_parent(&missing.join("child")).is_err());
    let relative = Path::new("missing-relative-source");
    assert!(require_missing_with_plain_parent(relative).is_err());
    #[cfg(unix)]
    {
        let parent = base.join("redirected-parent");
        std::os::unix::fs::symlink(&base, &parent)?;
        assert!(require_missing_with_plain_parent(&parent.join("missing-child")).is_err());
    }
    Ok(())
}
