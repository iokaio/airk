// SPDX-License-Identifier: Apache-2.0
//! Original tiny word vectors qualify ownership, not pretrained/ONNX quality.
use airk_core::{
    SearchQuery,
    identity::PublicationReceipt,
    model_objects::{ModelAssetName, ModelObjectSet},
};
use airk_kernel::Kernel;
use airk_retrieval::persistent::PersistentIndex;
use airk_store::{
    FileStore,
    retention::{RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // Keep model fixtures below a physical temporary parent on macOS.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn model(data: &Path, name: &str, alternate: bool) -> Result<()> {
    let directory = data.join("models/local");
    fs::create_dir_all(&directory)?;
    let vocabulary = if alternate {
        br#"{"login":[0.9,0.1],"invoice":[0.1,0.9]}"#.as_slice()
    } else {
        br#"{"login":[1.0,0.0],"invoice":[0.0,1.0]}"#.as_slice()
    };
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":name,"license":"Apache-2.0","dimensions":2,
            "sha256":format!("{:x}",Sha256::digest(vocabulary))
        }))?,
    )?;
    Ok(())
}
fn query() -> SearchQuery {
    SearchQuery {
        text: "login".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 8192,
    }
}
fn manual(kernel: &Kernel) -> Result<()> {
    kernel.configure_retention(RetentionPolicy {
        mode: RetentionMode::Manual,
        ..Default::default()
    })?;
    Ok(())
}
fn active(store: &FileStore) -> Result<(PublicationReceipt, ModelObjectSet)> {
    let catalog = store.catalog()?.context("catalog")?;
    let receipt = catalog.receipt.clone().context("receipt")?;
    let directory = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    assert!(!directory.join("model").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    assert_eq!(manifest["schema"], 4);
    let set = ModelObjectSet::from_bytes(&fs::read(directory.join("model-objects.json"))?)?;
    Ok((receipt, set))
}
fn object(store: &FileStore, digest: impl std::fmt::Display) -> std::path::PathBuf {
    store
        .directory()
        .join("objects/models")
        .join(digest.to_string())
}

#[test]
fn equal_assets_and_metadata_only_edits_share_bytes_across_relocation() -> Result<()> {
    let source = tempfile::tempdir()?;
    let parent = unlinked_tempdir()?;
    let data = parent.path().join("data");
    model(&data, "original metadata", false)?;
    fs::write(
        source.path().join("original.txt"),
        "login original evidence\n",
    )?;
    let kernel = Kernel::open(source.path(), Some(data.clone()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.join("collections/default"));
    let (_, first) = active(&store)?;
    let initial_count = fs::read_dir(store.directory().join("objects/models"))?.count();
    fs::write(
        source.path().join("second.txt"),
        "invoice unrelated evidence\n",
    )?;
    kernel.index()?;
    assert_eq!(active(&store)?.1, first);
    assert_eq!(
        fs::read_dir(store.directory().join("objects/models"))?.count(),
        initial_count
    );
    model(&data, "changed metadata only", false)?;
    kernel.index()?;
    let (_, changed) = active(&store)?;
    assert_eq!(
        first.assets[&ModelAssetName::VocabularyJson],
        changed.assets[&ModelAssetName::VocabularyJson]
    );
    assert_ne!(first.pipeline_digest, changed.pipeline_digest);
    assert_eq!(
        fs::read_dir(store.directory().join("objects/models"))?.count(),
        initial_count + 1
    );
    let before = kernel.semantic_search(&query())?.hits;
    drop(kernel);
    let destination = parent.path().join("relocated");
    airk_kernel::storage::relocate(&data, &destination)?;
    let reopened = Kernel::open(source.path(), Some(destination))?;
    assert_eq!(
        serde_json::to_value(reopened.semantic_search(&query())?.hits)?,
        serde_json::to_value(before)?
    );
    Ok(())
}

#[test]
fn lineage_publications_keep_authenticated_assets_when_provisioned_model_changes_or_disappears()
-> Result<()> {
    use airk_kernel::history_controls::{HistoryWorkBudget, PurgeApplyRequest, PurgeRequest};
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "published original model", false)?;
    fs::write(
        source.path().join("original.txt"),
        "login original evidence\n",
    )?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let (mut previous, original) = active(&store)?;
    let original_hits = serde_json::to_value(kernel.semantic_search(&query())?.hits)?;
    // Same legacy identity but different complete pipeline must not silently
    // activate outside indexing. The next pass also proves no provisioning read.
    model(data.path(), "unpublished metadata change", false)?;
    for remove_provisioned in [false, true] {
        if remove_provisioned {
            fs::remove_file(data.path().join("models/local/model.json"))?;
            fs::remove_file(data.path().join("models/local/vocabulary.json"))?;
        }
        let request = PurgeRequest {
            revisions: vec![],
            checkpoints: vec![],
            clear_lineage: true,
        };
        let preview = kernel.purge_history_preview(&request, HistoryWorkBudget::default())?;
        let result = kernel.purge_history(
            &PurgeApplyRequest {
                request,
                expected_plan_digest: preview.digest,
            },
            HistoryWorkBudget::default(),
        )?;
        assert!(result.storage.error.is_none(), "{:?}", result.storage.error);
        assert!(result.lineage_publication.is_some(), "{result:?}");
        let (receipt, set) = active(&store)?;
        assert!(receipt.token.generation > previous.token.generation);
        assert_eq!(set, original);
        assert_eq!(
            store
                .catalog()?
                .unwrap()
                .inventory
                .as_ref()
                .unwrap()
                .embedding_pipeline_digest,
            Some(original.pipeline_digest)
        );
        assert_eq!(
            serde_json::to_value(kernel.semantic_search(&query())?.hits)?,
            original_hits
        );
        previous = receipt;
    }
    assert_eq!(
        fs::read_to_string(source.path().join("original.txt"))?,
        "login original evidence\n"
    );
    Ok(())
}

#[test]
fn pinned_old_generation_keeps_its_model_until_reader_release() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "first model", false)?;
    fs::write(source.path().join("original.txt"), "login original\n")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let (receipt, first) = active(&store)?;
    let pinned = PersistentIndex::open_v2(store.directory(), "default", &receipt)?;
    model(data.path(), "second model", true)?;
    kernel.index()?;
    fs::write(source.path().join("another.txt"), "invoice original\n")?;
    kernel.index()?;
    kernel.configure_retention(RetentionPolicy {
        mode: RetentionMode::Automatic,
        ..Default::default()
    })?;
    for _ in 0..8 {
        kernel.maintain_retention()?;
    }
    let old_weights = first.assets[&ModelAssetName::VocabularyJson].sha256;
    assert!(object(&store, old_weights).exists());
    assert!(!pinned.semantic_search(&query())?.hits.is_empty());
    drop(pinned);
    for _ in 0..16 {
        kernel.maintain_retention()?;
    }
    assert!(
        !object(&store, old_weights).exists(),
        "unreferenced old model should be collectible after reader release"
    );
    Ok(())
}

#[test]
fn corrupt_object_and_quota_failure_do_not_advance_head() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original model", false)?;
    fs::write(source.path().join("original.txt"), "login original\n")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let (receipt, set) = active(&store)?;
    let weights = &set.assets[&ModelAssetName::VocabularyJson];
    let path = object(&store, weights.sha256);
    let original = fs::read(&path)?;
    fs::write(&path, vec![b'x'; weights.bytes as usize])?;
    assert!(PersistentIndex::open_v2(store.directory(), "default", &receipt).is_err());
    fs::write(source.path().join("new.txt"), "invoice new evidence\n")?;
    assert!(kernel.index().is_err());
    fs::write(&path, original)?;
    assert_eq!(
        store.persistent_revision()?.unwrap().token,
        Some(receipt.token)
    );
    let quota = airk_store::quota::Quota::open(data.path())?;
    let used = quota.status()?.collections["default"].used_bytes;
    let mut policy = quota.policy()?;
    policy.collections.insert("default".into(), used + 1);
    quota.configure(policy)?;
    model(
        data.path(),
        "new model metadata exceeding one-byte allowance",
        false,
    )?;
    assert!(kernel.index().is_err());
    assert_eq!(
        store.persistent_revision()?.unwrap().token,
        Some(receipt.token)
    );
    Ok(())
}

#[test]
fn lexical_only_generation_has_no_model_objects_or_model_requirement() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(
        source.path().join("original.txt"),
        "login original lexical evidence\n",
    )?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.context("catalog")?;
    let receipt = catalog.receipt.as_ref().context("receipt")?;
    let generation = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    assert!(!generation.join("model-objects.json").exists());
    assert!(!generation.join("model").exists());
    assert!(!store.directory().join("objects/models").exists());
    assert!(!kernel.search(&query())?.hits.is_empty());
    assert!(kernel.semantic_search(&query()).is_err());
    Ok(())
}

#[test]
fn obsolete_retrieval_format_requires_rebuild() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original model", false)?;
    fs::write(source.path().join("original.txt"), "login original\n")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.context("catalog")?;
    let mut receipt = catalog.receipt.as_ref().context("receipt")?.clone();
    let path = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    manifest["schema"] = 3.into();
    manifest["engine"] = "obsolete-original-fixture".into();
    let bytes = serde_json::to_vec(&manifest)?;
    fs::write(path, &bytes)?;
    receipt.manifest_digest =
        airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
    let error = PersistentIndex::open_v2(store.directory(), "default", &receipt)
        .err()
        .context("obsolete format was accepted")?;
    assert!(format!("{error:#}").contains("rebuild"), "{error:#}");
    Ok(())
}

#[test]
fn full_rebuild_resets_obsolete_state_and_preserves_source_and_saved_policy() -> Result<()> {
    use airk_core::{Snapshot, SnapshotStore};
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original saved model", false)?;
    let text = b"login original evidence remains byte exact\n";
    fs::write(source.path().join("original.txt"), text)?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    let store = FileStore::new(data.path().join("collections/default"));
    let root = source.path().canonicalize()?.to_string_lossy().into_owned();
    let writer = store.lock()?;
    let mut old = Snapshot::empty(root.clone());
    old.generation = 99;
    store.save(&old)?;
    let quota = airk_store::quota::Quota::open(data.path())?;
    store.configure_extraction_policy_bytes(
        &writer,
        &quota,
        &root,
        br#"{"version":3,"detail":"essential","rules":[]}"#,
    )?;
    airk_store::roots::RootConfig::load(store.directory(), &root)?.save(store.directory())?;
    // A malformed obsolete extraction must never be decoded during reset.
    fs::create_dir_all(store.directory().join("objects/artifacts"))?;
    fs::write(
        store.directory().join("objects/artifacts/obsolete.json"),
        b"not an extraction payload",
    )?;
    let policy_path = store.directory().join("extraction-policy.json");
    let roots_path = store.directory().join("roots.json");
    let model_path = data.path().join("models/local/model.json");
    let preserved = [
        (&policy_path, fs::read(&policy_path)?),
        (&roots_path, fs::read(&roots_path)?),
        (&model_path, fs::read(&model_path)?),
    ];
    drop(writer);
    assert!(format!("{:#}", kernel.index().unwrap_err()).contains("airk index --full"));
    let report = kernel.reindex()?;
    assert_eq!(report.generation, 1);
    assert_eq!(fs::read(source.path().join("original.txt"))?, text);
    for (path, bytes) in preserved {
        assert_eq!(fs::read(path)?, bytes);
    }
    assert!(!store.reset_required()?);
    assert_eq!(
        kernel.extraction_policy()?.configured_detail,
        airk_core::extraction::IndexDetail::Essential
    );
    assert!(!kernel.search(&query())?.hits.is_empty());
    assert!(!kernel.semantic_search(&query())?.hits.is_empty());
    let current = kernel.status()?.publication.context("current revision")?;
    assert_eq!(current.generation, 1);
    Ok(())
}
