// SPDX-License-Identifier: Apache-2.0
//! Original authenticated sidecar and collection-local embedding GC fixtures.
use airk_core::changes::{
    ChangeCoverage, CoverageFloor, CoverageReason, LineageEnvelope, PublicationObservation,
    PublicationReason,
};
use airk_core::embedding::CachedEmbedding;
use airk_core::identity::{
    Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
};
use airk_core::publication::InventoryV2;
use airk_store::{
    ExpectedHead, FileStore,
    quota::Quota,
    vector_cache::{VectorCacheManifest, VectorCacheReference},
};
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn object(store: &FileStore, name: &str) -> Result<VectorCacheReference> {
    let key = digest(name.as_bytes());
    let bytes = serde_json::to_vec(&CachedEmbedding::new(key, vec![0.25, 0.75], 2)?)?;
    let directory = store.directory().join("objects/embeddings");
    fs::create_dir_all(&directory)?;
    fs::write(directory.join(key.to_string()), &bytes)?;
    Ok(VectorCacheReference {
        key,
        sha256: digest(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn publish(
    store: &FileStore,
    parent: Option<RevisionToken>,
    references: Option<&[VectorCacheReference]>,
) -> Result<PublicationReceipt> {
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
    let inventory = InventoryV2 {
        embedding_pipeline_digest: None,
        extraction_policy_digest: None,
        version: 2,
        incarnation,
        generation,
        collection: collection.into(),
        root: "/original-embedding-fixture".into(),
        roots: BTreeMap::new(),
        lineage_digest: digest(&serde_json::to_vec(&lineage)?),
        profiles: BTreeMap::new(),
        files: BTreeMap::new(),
        retrieval_specification: "original-cache-fixture/1".into(),
        embedding_model: None,
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
    let mut names = vec!["inventory.json", "lineage.json", "catalog.sqlite"];
    if let Some(references) = references {
        let mut references = references.to_vec();
        references.sort_by_key(|reference| reference.key);
        let sidecar = VectorCacheManifest {
            version: 1,
            references,
        }
        .canonical_bytes()?;
        reservation.charge(sidecar.len() as u64)?;
        fs::write(staging.join("embedding-cache.json"), sidecar)?;
        names.push("embedding-cache.json");
    }
    let mut components = BTreeMap::new();
    for name in names {
        let bytes = fs::read(staging.join(name))?;
        components.insert(name, json!({"bytes":bytes.len(),"sha256":digest(&bytes)}));
    }
    let manifest = serde_json::to_vec(
        &json!({"schema":2,"engine":inventory.retrieval_specification,"token":token,
        "collection":collection,"chunks":0,"vectors":0,"model_sha256":null,"embedding":"none",
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
    let store = FileStore::new(root.path().join("collections/original"));
    let _writer = store.lock()?;
    let reference = object(&store, "original vector")?;
    let receipt = publish(&store, None, Some(std::slice::from_ref(&reference)))?;
    let _reader = store.catalog_at(&receipt.token, 16 * 1024 * 1024)?.unwrap();
    assert_eq!(store.embedding_cache_references(&receipt)?, vec![reference]);
    let directory = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let original = fs::read(directory.join("embedding-cache.json"))?;
    let mut forged = original.clone();
    forged[0] = b'[';
    fs::write(directory.join("embedding-cache.json"), forged)?;
    assert!(store.embedding_cache_references(&receipt).is_err());
    fs::write(directory.join("embedding-cache.json"), original)?;
    let original_manifest = fs::read(directory.join("manifest.json"))?;
    let mut forged_manifest = original_manifest.clone();
    forged_manifest.push(b' ');
    fs::write(directory.join("manifest.json"), &forged_manifest)?;
    assert!(store.embedding_cache_references(&receipt).is_err());
    let mut uncommitted_receipt = receipt.clone();
    uncommitted_receipt.manifest_digest = digest(&forged_manifest);
    assert!(
        store
            .embedding_cache_references(&uncommitted_receipt)
            .is_err()
    );
    fs::write(directory.join("manifest.json"), original_manifest)?;
    let foreign = FileStore::new(root.path().join("collections/foreign"));
    let _foreign_writer = foreign.lock()?;
    publish(&foreign, None, None)?;
    assert!(foreign.embedding_cache_references(&receipt).is_err());
    Ok(())
}

#[test]
fn leased_roots_share_vectors_orphans_reclaim_once_and_other_collections_are_untouched()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let shared = object(&store, "shared vector")?;
    let old_only = object(&store, "old-only vector")?;
    let orphan = object(&store, "unused vector")?;
    let first = publish(&store, None, Some(&[shared.clone(), old_only.clone()]))?;
    let reader = store.catalog_at(&first.token, 16 * 1024 * 1024)?.unwrap();
    let second = publish(
        &store,
        Some(first.token),
        Some(std::slice::from_ref(&shared)),
    )?;
    let foreign = FileStore::new(root.path().join("collections/foreign"));
    let _foreign_writer = foreign.lock()?;
    let foreign_object = object(&foreign, "unused vector")?;
    let path = |reference: &VectorCacheReference| {
        store
            .directory()
            .join("objects/embeddings")
            .join(reference.key.to_string())
    };
    let preview = store.prune_retention(&writer, &quota, true)?;
    assert_eq!(preview.unreferenced_objects, 1);
    assert_eq!(preview.reclaimable_bytes, orphan.bytes);
    let applied = store.prune_retention(&writer, &quota, false)?;
    assert_eq!(applied.unreferenced_objects, 1);
    assert_eq!(applied.removed_bytes, orphan.bytes);
    assert!(path(&shared).is_file() && path(&old_only).is_file());
    assert!(!path(&orphan).exists());
    assert!(
        foreign
            .directory()
            .join("objects/embeddings")
            .join(foreign_object.key.to_string())
            .is_file()
    );
    drop(reader);
    let preview = store.prune_retention(&writer, &quota, true)?;
    assert_eq!(
        preview.unreferenced_objects, 1,
        "shared key remains rooted by current generation"
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
fn legacy_sidecar_absence_is_empty_and_corrupt_roots_block_orphan_deletion() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/original"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let first = publish(&store, None, None)?;
    assert!(store.embedding_cache_references(&first)?.is_empty());
    let reference = object(&store, "rooted vector")?;
    let current = publish(&store, Some(first.token), Some(&[reference]))?;
    let orphan = object(&store, "unreferenced vector")?;
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
            .join("objects/embeddings")
            .join(orphan.key.to_string())
            .is_file()
    );
    Ok(())
}
