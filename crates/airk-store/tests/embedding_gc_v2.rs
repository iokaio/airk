// SPDX-License-Identifier: Apache-2.0
//! Original authenticated sidecar and collection-local embedding GC fixtures.
use airk_core::changes::{
    ChangeCoverage, CoverageFloor, CoverageReason, LineageEnvelope, PublicationObservation,
    PublicationReason,
};
use airk_core::embedding::{CachedEmbedding, EmbeddingCache, EmbeddingDescriptor};
use airk_core::identity::{
    Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
};
use airk_core::model_objects::{ModelAssetName, ModelObjectRef, ModelObjectSet};
use airk_core::publication::InventoryV2;
use airk_store::{
    ExpectedHead, FileStore,
    quota::{Quota, Reservation},
    vector_cache::{VectorCache, VectorCacheManifest, pack_path},
};
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn descriptor() -> EmbeddingDescriptor {
    let value = digest(b"original fixture pipeline");
    EmbeddingDescriptor {
        version: 1,
        model: value,
        tokenizer: value,
        preprocessing: value,
        pooling: value,
        normalization: value,
        inference: value,
        dimensions: 2,
    }
}

fn object(store: &FileStore, names: &[&str]) -> Result<VectorCacheManifest> {
    let quota = Quota::open(store.directory().parent().unwrap().parent().unwrap())?;
    let collection = store.directory().file_name().unwrap().to_str().unwrap();
    let reservation = quota.reserve(collection, 64 * 1024 * 1024)?;
    let cache = VectorCache::new(store.directory(), &reservation, &descriptor(), None)?;
    for name in names {
        cache.store(&CachedEmbedding::new(
            digest(name.as_bytes()),
            vec![0.25, 0.75],
            2,
        )?)?;
    }
    cache.finalize()
}

fn merge(caches: &[&VectorCacheManifest]) -> Result<VectorCacheManifest> {
    let mut merged = VectorCacheManifest {
        version: 2,
        pipeline_digest: descriptor().identity()?,
        dimensions: 2,
        packs: vec![],
        references: vec![],
    };
    for cache in caches {
        merged.packs.extend(cache.packs.iter().cloned());
        merged.references.extend(cache.references.iter().cloned());
    }
    merged.packs.sort_by_key(|pack| pack.sha256);
    merged.references.sort_by_key(|reference| reference.key);
    merged.validate()?;
    Ok(merged)
}

fn model_objects(store: &FileStore) -> Result<ModelObjectSet> {
    let mut assets = BTreeMap::new();
    let directory = store.directory().join("objects/models");
    fs::create_dir_all(&directory)?;
    for (name, bytes) in [
        (
            ModelAssetName::ModelJson,
            br#"{"schema":1,"dimensions":2}"#.as_slice(),
        ),
        (
            ModelAssetName::VocabularyJson,
            br#"{"original":[0.25,0.75]}"#.as_slice(),
        ),
    ] {
        let sha256 = digest(bytes);
        fs::write(directory.join(sha256.to_string()), bytes)?;
        assets.insert(
            name,
            ModelObjectRef {
                sha256,
                bytes: bytes.len() as u64,
            },
        );
    }
    Ok(ModelObjectSet {
        version: 1,
        pipeline_digest: descriptor().identity()?,
        legacy_identity: digest(b"original model").to_string(),
        assets,
    })
}

fn prepare(
    store: &FileStore,
    parent: Option<RevisionToken>,
    cache: Option<&VectorCacheManifest>,
) -> Result<(InventoryV2, PublicationReceipt, Reservation)> {
    let collection = store.directory().file_name().unwrap().to_str().unwrap();
    let quota = Quota::open(store.directory().parent().unwrap().parent().unwrap())?;
    let reservation = quota.reserve(collection, 16 * 1024 * 1024)?;
    let generation = parent.map_or(1, |token| token.generation + 1);
    let incarnation = parent.map_or(Incarnation::from_bytes([17; 16]), |token| token.incarnation);
    let floor = CoverageFloor {
        complete_after: generation,
        reason: Some(CoverageReason::BaselineUnavailable),
    };
    let lineage = LineageEnvelope {
        version: 2,
        incarnation,
        observation: PublicationObservation {
            generation,
            parent,
            observed_unix_seconds: generation,
            reason: PublicationReason::Index,
        },
        coverage: ChangeCoverage {
            files: floor.clone(),
            code: floor.clone(),
            documents: floor.clone(),
            ancestry: floor.clone(),
            resolution: floor,
        },
        ancestry: vec![],
        events: vec![],
        resolvers: vec![],
    };
    let models = model_objects(store)?;
    let inventory = InventoryV2 {
        vector_selection: Default::default(),
        embedding_pipeline_digest: Some(models.pipeline_digest),
        index_detail: None,
        extraction_policy_digest: None,
        version: airk_core::publication::INVENTORY_VERSION,
        incarnation,
        generation,
        collection: collection.into(),
        root: "/original-embedding-fixture".into(),
        roots: BTreeMap::new(),
        lineage_digest: digest(&serde_json::to_vec(&lineage)?),
        profiles: BTreeMap::new(),
        files: BTreeMap::new(),
        retrieval_specification: airk_store::reset::CURRENT_RETRIEVAL_ENGINE.into(),
        embedding_model: Some(models.legacy_identity.clone()),
        extractor_version: "original-fixture/1".into(),
        analysis: None,
        imported_facts: vec![],
        imported_relationships: vec![],
    };
    let bytes = inventory.canonical_bytes()?;
    let token = RevisionToken {
        incarnation,
        generation,
        revision: revision_digest(inventory.retrieval_specification.as_bytes(), &bytes),
    };
    let staging = reservation
        .staging_directory()?
        .join(format!("cache-{generation}"));
    fs::create_dir_all(&staging)?;
    let snapshot = inventory.projection()?;
    let source = airk_store::blobs::RetainedSource {
        store,
        snapshot: &snapshot,
    };
    store.prepare_catalog_v2(
        &staging,
        &inventory,
        &lineage,
        &token,
        &source,
        Some(&reservation),
        2 * 1024 * 1024,
    )?;
    reservation.charge(bytes.len() as u64)?;
    fs::write(staging.join("inventory.json"), &bytes)?;
    let mut names = vec![
        "inventory.json",
        "lineage.json",
        "catalog.sqlite",
        "vector-map/meta.json",
        "model-objects.json",
    ];
    fs::write(
        staging.join("model-objects.json"),
        models.canonical_bytes()?,
    )?;
    if let Some(cache) = cache {
        let sidecar = cache.canonical_bytes()?;
        reservation.charge(sidecar.len() as u64)?;
        fs::write(staging.join("embedding-cache.json"), sidecar)?;
        names.push("embedding-cache.json");
    }
    fs::create_dir_all(staging.join("vector-map"))?;
    fs::write(staging.join("vector-map/meta.json"), b"{}")?;
    let mut components = BTreeMap::new();
    for name in names {
        let bytes = fs::read(staging.join(name))?;
        components.insert(name, json!({"bytes":bytes.len(),"sha256":digest(&bytes)}));
    }
    let manifest = serde_json::to_vec(
        &json!({"schema":4,"engine":inventory.retrieval_specification,"token":token,
        "collection":collection,"chunks":0,"vectors":0,"vector_occurrences":0,"vector_selection_digest":inventory.vector_selection.digest()?,"vector_eligible_artifacts":inventory.files.len(),"vector_excluded_artifacts":0,"model_sha256":inventory.embedding_model,"embedding":"original fixture",
        "inventory_digest":digest(&bytes),"components":components}),
    )?;
    reservation.charge(manifest.len() as u64)?;
    fs::write(staging.join("manifest.json"), &manifest)?;
    let lease =
        airk_store::blobs::reader_lease_path(store.directory(), &token.revision.to_string())?;
    fs::create_dir_all(lease.parent().unwrap())?;
    fs::write(lease, [])?;
    fs::create_dir_all(store.directory().join("generations"))?;
    fs::rename(
        staging,
        store
            .directory()
            .join("generations")
            .join(token.revision.to_string()),
    )?;
    let receipt = PublicationReceipt {
        token,
        inventory_digest: digest(&bytes),
        manifest_digest: digest(&manifest),
    };
    Ok((inventory, receipt, reservation))
}

fn publish(
    store: &FileStore,
    parent: Option<RevisionToken>,
    cache: Option<&VectorCacheManifest>,
) -> Result<PublicationReceipt> {
    let (inventory, receipt, reservation) = prepare(store, parent, cache)?;
    store.publish_catalog_v2(
        &inventory,
        &receipt,
        &parent.map_or(ExpectedHead::Empty, ExpectedHead::V2),
        &reservation,
    )?;
    Ok(receipt)
}

#[test]
fn reference_lookup_authenticates_receipt_sidecar_and_collection() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let _writer = store.lock()?;
    let cache = object(&store, &["original vector"])?;
    let receipt = publish(&store, None, Some(&cache))?;
    let _reader = store.catalog_at(&receipt.token, 16 * 1024 * 1024)?.unwrap();
    assert_eq!(
        store
            .embedding_cache_manifest(&receipt)?
            .unwrap()
            .canonical_bytes()?,
        cache.canonical_bytes()?
    );
    let directory = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let original = fs::read(directory.join("embedding-cache.json"))?;
    let mut forged = original.clone();
    forged[0] = b'[';
    fs::write(directory.join("embedding-cache.json"), forged)?;
    assert!(store.embedding_cache_manifest(&receipt).is_err());
    fs::write(directory.join("embedding-cache.json"), original)?;
    let original_manifest = fs::read(directory.join("manifest.json"))?;
    let mut forged_manifest = original_manifest.clone();
    forged_manifest.push(b' ');
    fs::write(directory.join("manifest.json"), &forged_manifest)?;
    assert!(store.embedding_cache_manifest(&receipt).is_err());
    let mut uncommitted_receipt = receipt.clone();
    uncommitted_receipt.manifest_digest = digest(&forged_manifest);
    assert!(
        store
            .embedding_cache_manifest(&uncommitted_receipt)
            .is_err()
    );
    fs::write(directory.join("manifest.json"), original_manifest)?;
    let foreign = FileStore::new(root.path().join("collections-1.2/foreign"));
    let _foreign_writer = foreign.lock()?;
    publish(&foreign, None, None)?;
    assert!(foreign.embedding_cache_manifest(&receipt).is_err());
    Ok(())
}

#[test]
fn leased_roots_share_vectors_orphans_reclaim_once_and_other_collections_are_untouched()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let shared = object(&store, &["shared vector", "formerly live record"])?;
    let old_only = object(&store, &["old-only vector"])?;
    let orphan = object(&store, &["unused vector"])?;
    let first = publish(&store, None, Some(&merge(&[&shared, &old_only])?))?;
    let reader = store.catalog_at(&first.token, 16 * 1024 * 1024)?.unwrap();
    let mut partial = shared.clone();
    partial.references.truncate(1);
    let second = publish(&store, Some(first.token), Some(&partial))?;
    let foreign = FileStore::new(root.path().join("collections-1.2/foreign"));
    let _foreign_writer = foreign.lock()?;
    let foreign_object = object(&foreign, &["unused vector"])?;
    let path = |cache: &VectorCacheManifest| {
        store
            .directory()
            .join("objects/embedding-packs")
            .join(cache.packs[0].sha256.to_string())
    };
    let preview = store.prune_retention(&writer, &quota, true)?;
    assert_eq!(preview.unreferenced_objects, 1);
    assert_eq!(preview.reclaimable_bytes, orphan.packs[0].bytes);
    let applied = store.prune_retention(&writer, &quota, false)?;
    assert_eq!(applied.unreferenced_objects, 1);
    assert_eq!(applied.removed_bytes, orphan.packs[0].bytes);
    assert!(path(&shared).is_file() && path(&old_only).is_file());
    assert!(!path(&orphan).exists());
    assert!(
        foreign
            .directory()
            .join("objects/embedding-packs")
            .join(foreign_object.packs[0].sha256.to_string())
            .is_file()
    );
    drop(reader);
    let preview = store.prune_retention(&writer, &quota, true)?;
    assert_eq!(
        preview.unreferenced_objects, 1,
        "one live record retains its entire shared pack"
    );
    let applied = store.prune_retention(&writer, &quota, false)?;
    assert_eq!(applied.unreferenced_objects, 1);
    assert_eq!(applied.removed_bytes, preview.reclaimable_bytes);
    assert!(!path(&old_only).exists() && path(&shared).is_file());
    publish(&store, Some(second.token), None)?;
    let applied = store.prune_retention(&writer, &quota, false)?;
    assert_eq!(applied.unreferenced_objects, 1);
    assert!(!path(&shared).exists());
    Ok(())
}

#[test]
fn cacheless_publication_is_none_and_corrupt_roots_block_orphan_deletion() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let first = publish(&store, None, None)?;
    assert!(store.embedding_cache_manifest(&first)?.is_none());
    let cache = object(&store, &["rooted vector"])?;
    let current = publish(&store, Some(first.token), Some(&cache))?;
    let orphan = object(&store, &["unreferenced vector"])?;
    let sidecar = store
        .directory()
        .join("generations")
        .join(current.token.revision.to_string())
        .join("embedding-cache.json");
    fs::write(sidecar, b"{}")?;
    assert!(store.prune_retention(&writer, &quota, true).is_err());
    assert!(store.prune_retention(&writer, &quota, false).is_err());
    assert!(
        store
            .directory()
            .join("objects/embedding-packs")
            .join(orphan.packs[0].sha256.to_string())
            .is_file()
    );
    Ok(())
}

#[test]
fn publication_rejects_cache_pipeline_and_model_dimension_mismatches() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let _writer = store.lock()?;
    let first = publish(&store, None, None)?;
    let mut cache = object(&store, &["original vector"])?;
    cache.pipeline_digest = digest(b"different pipeline");
    for pack in &mut cache.packs {
        pack.pipeline_digest = cache.pipeline_digest;
    }
    let (inventory, receipt, reservation) = prepare(&store, Some(first.token), Some(&cache))?;
    assert!(
        store
            .publish_catalog_v2(
                &inventory,
                &receipt,
                &ExpectedHead::V2(first.token),
                &reservation
            )
            .is_err()
    );
    assert_eq!(store.expected_head()?, ExpectedHead::V2(first.token));

    // A structurally valid three-dimensional pack cannot be attached to a
    // publication whose digest-authenticated model metadata declares two.
    let other = FileStore::new(root.path().join("collections-1.2/dimensions"));
    let _other_writer = other.lock()?;
    let quota = Quota::open(root.path())?;
    let reservation = quota.reserve("dimensions", 64 * 1024 * 1024)?;
    let mut three = descriptor();
    three.dimensions = 3;
    let writer = VectorCache::new(other.directory(), &reservation, &three, None)?;
    writer.store(&CachedEmbedding::new(
        digest(b"three values"),
        vec![0.25, 0.5, 0.75],
        3,
    )?)?;
    let mut cache = writer.finalize()?;
    cache.pipeline_digest = descriptor().identity()?;
    for pack in &mut cache.packs {
        pack.pipeline_digest = cache.pipeline_digest;
    }
    let (inventory, receipt, reservation) = prepare(&other, None, Some(&cache))?;
    let error = other
        .publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)
        .unwrap_err();
    assert!(format!("{error:#}").contains("dimensions"), "{error:#}");
    assert_eq!(other.expected_head()?, ExpectedHead::Empty);
    Ok(())
}

#[test]
fn maintenance_authenticates_unused_records_before_reclaiming_orphans() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let mut cache = object(&store, &["live record", "retired record"])?;
    cache.references.retain(|reference| reference.record == 0);
    assert_eq!(cache.references.len(), 1);
    publish(&store, None, Some(&cache))?;
    let orphan = object(&store, &["orphan"])?;
    let path = pack_path(store.directory(), &cache.packs[0].sha256)?;
    let original = fs::read(&path)?;
    let mut corrupt = original.clone();
    let offset = airk_store::vector_cache::PACK_HEADER_BYTES
        + airk_store::vector_cache::record_bytes(2)?
        + 68;
    corrupt[offset as usize] ^= 1;
    fs::write(&path, corrupt)?;
    assert!(store.prune_retention(&writer, &quota, true).is_err());
    assert!(store.prune_retention(&writer, &quota, false).is_err());
    assert!(pack_path(store.directory(), &orphan.packs[0].sha256)?.is_file());
    fs::write(path, original)?;
    let removed = store.prune_retention(&writer, &quota, false)?;
    assert_eq!(removed.unreferenced_objects, 1);
    assert!(pack_path(store.directory(), &cache.packs[0].sha256)?.is_file());
    Ok(())
}

#[test]
fn prepared_publication_blocks_pack_collection_until_its_commit() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections-1.2/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let first_cache = object(&store, &["first"])?;
    let first = publish(&store, None, Some(&first_cache))?;
    let pending_cache = object(&store, &["pending"])?;
    let orphan = object(&store, &["orphan"])?;
    let (inventory, receipt, reservation) =
        prepare(&store, Some(first.token), Some(&pending_cache))?;
    let target = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let staged = reservation.staging_directory()?.join("before-rename");
    fs::rename(&target, &staged)?;
    reservation.track_prepared_publication(&ExpectedHead::V2(first.token), &receipt)?;
    fs::rename(staged, &target)?;
    assert!(store.prune_retention(&writer, &quota, true).is_err());
    assert!(store.prune_retention(&writer, &quota, false).is_err());
    for cache in [&first_cache, &pending_cache, &orphan] {
        assert!(pack_path(store.directory(), &cache.packs[0].sha256)?.is_file());
    }
    assert_eq!(store.expected_head()?, ExpectedHead::V2(first.token));
    store.publish_catalog_v2(
        &inventory,
        &receipt,
        &ExpectedHead::V2(first.token),
        &reservation,
    )?;
    drop(reservation);
    store.prune_retention(&writer, &quota, false)?;
    assert!(pack_path(store.directory(), &pending_cache.packs[0].sha256)?.is_file());
    assert!(!pack_path(store.directory(), &orphan.packs[0].sha256)?.exists());
    Ok(())
}

#[test]
fn pack_publication_crash_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_PACK_CRASH_ROOT") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    let store = FileStore::new(root.join("collections-1.2/original"));
    let _writer = store.lock()?;
    let ExpectedHead::V2(parent) = store.expected_head()? else {
        anyhow::bail!("fixture head missing");
    };
    let cache = object(&store, &["crashed publication record"])?;
    let (inventory, receipt, reservation) = prepare(&store, Some(parent), Some(&cache))?;
    let target = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let staged = reservation.staging_directory()?.join("before-rename");
    fs::rename(&target, &staged)?;
    reservation.track_prepared_publication(&ExpectedHead::V2(parent), &receipt)?;
    fs::rename(staged, &target)?;
    if std::env::var("AIRK_PACK_CRASH_MODE")? == "committed" {
        store.publish_catalog_v2(
            &inventory,
            &receipt,
            &ExpectedHead::V2(parent),
            &reservation,
        )?;
    }
    fs::write(root.join("crashed-cache.json"), cache.canonical_bytes()?)?;
    fs::write(
        root.join("crashed-receipt.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    // Leave the durable publication journal and reservation without destructors.
    std::process::exit(0);
}

#[test]
fn process_exit_around_publication_commit_preserves_exact_pack_ownership() -> Result<()> {
    for committed in [false, true] {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections-1.2/original"));
        let writer = store.lock()?;
        let first_cache = object(&store, &["previous publication record"])?;
        let first = publish(&store, None, Some(&first_cache))?;
        let reader = store.catalog_at(&first.token, 16 * 1024 * 1024)?.unwrap();
        drop(writer);
        let child = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "pack_publication_crash_child", "--nocapture"])
            .env("AIRK_PACK_CRASH_ROOT", root.path())
            .env(
                "AIRK_PACK_CRASH_MODE",
                if committed { "committed" } else { "prepared" },
            )
            .output()?;
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let pending: PublicationReceipt =
            serde_json::from_slice(&fs::read(root.path().join("crashed-receipt.json"))?)?;
        let cache =
            VectorCacheManifest::from_bytes(&fs::read(root.path().join("crashed-cache.json"))?)?;
        let quota = Quota::open(root.path())?;
        assert_eq!(quota.status()?.active_reservations, 0);
        assert_eq!(
            store.expected_head()?,
            ExpectedHead::V2(if committed {
                pending.token
            } else {
                first.token
            })
        );
        let writer = store.lock()?;
        store.prune_retention(&writer, &quota, false)?;
        assert!(
            pack_path(store.directory(), &first_cache.packs[0].sha256)?.is_file(),
            "old reader retains its pack"
        );
        assert_eq!(
            pack_path(store.directory(), &cache.packs[0].sha256)?.exists(),
            committed
        );
        drop(reader);
        store.prune_retention(&writer, &quota, false)?;
        assert_eq!(
            pack_path(store.directory(), &first_cache.packs[0].sha256)?.exists(),
            !committed
        );
        if committed {
            assert_eq!(store.embedding_cache_manifest(&pending)?, Some(cache));
        }
    }
    Ok(())
}
