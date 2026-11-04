// SPDX-License-Identifier: Apache-2.0
//! Native configuration observations and source-ID invariants. These component
//! tests do not qualify actor/worker transport, hierarchy activation or indexing.
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, IndexId, SourceRootId},
    service::ProjectRegistration,
};
use std::path::PathBuf;

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
        "../../airk-store/src/service_catalog_authority_v1.sql"
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
