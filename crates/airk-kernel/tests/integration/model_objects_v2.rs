// SPDX-License-Identifier: Apache-2.0
//! Original tiny word vectors qualify ownership, not pretrained/ONNX quality.
use super::current_inspection::read_current;
use airk_core::{
    SearchQuery,
    identity::RevisionToken,
    model_objects::{ModelAssetName, ModelObjectSet},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
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
fn active(store: &FileStore) -> Result<(RevisionToken, ModelObjectSet)> {
    let read = read_current(store)?;
    let token = read.token()?;
    let revision = RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    };
    Ok((
        revision,
        read.collection_state()?
            .context("current state")?
            .configuration
            .model
            .context("model")?,
    ))
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
    kernel.index()?;
    let store = FileStore::new(data.join("collections-1.2/default"));
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
        initial_count
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
fn committed_model_survives_provisioning_changes_and_idle_readers_do_not_keep_old_assets()
-> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "first model", false)?;
    fs::write(source.path().join("original.txt"), "login original\n")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let (token, first) = active(&store)?;
    let before = serde_json::to_value(kernel.semantic_search(&query())?.hits)?;
    let idle = Kernel::open(source.path(), Some(data.path().into()))?;
    idle.semantic_search(&query())?;
    for name in ["model.json", "vocabulary.json"] {
        fs::remove_file(data.path().join("models/local").join(name))?;
    }
    kernel.maintain_current()?;
    assert_eq!(active(&store)?.0, token);
    assert_eq!(
        serde_json::to_value(kernel.semantic_search(&query())?.hits)?,
        before
    );
    model(data.path(), "second model", true)?;
    let request = read_current(&store)?;
    kernel.index()?;
    assert!(object(&store, first.assets[&ModelAssetName::VocabularyJson].sha256).exists());
    drop(request);
    kernel.index()?;
    assert!(!object(&store, first.assets[&ModelAssetName::VocabularyJson].sha256).exists());
    assert!(!idle.semantic_search(&query())?.hits.is_empty());
    Ok(())
}

#[test]
fn corrupt_object_and_quota_failure_do_not_advance_head() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original model", false)?;
    fs::write(source.path().join("original.txt"), "login original\n")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let (receipt, set) = active(&store)?;
    let weights = &set.assets[&ModelAssetName::VocabularyJson];
    let path = object(&store, weights.sha256);
    let original = fs::read(&path)?;
    fs::write(&path, vec![b'x'; weights.bytes as usize])?;
    assert!(kernel.semantic_search(&query()).is_err());
    fs::write(source.path().join("new.txt"), "invoice new evidence\n")?;
    assert!(kernel.index().is_err());
    fs::write(&path, original)?;
    assert_eq!(active(&store)?.0, receipt);
    let quota = airk_store::quota::Quota::open(data.path())?;
    let status = quota.status()?;
    let used =
        status.collections["default"].used_bytes + status.collections["default"].reserved_bytes;
    let mut policy = quota.policy()?;
    policy.collections.insert("default".into(), used + 1);
    quota.configure(policy)?;
    model(
        data.path(),
        "new model metadata exceeding one-byte allowance",
        false,
    )?;
    assert!(kernel.index().is_err());
    assert_eq!(active(&store)?.0, receipt);
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
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    assert!(
        read_current(&store)?
            .collection_state()?
            .unwrap()
            .configuration
            .model
            .is_none()
    );
    assert!(!store.directory().join("generations").exists());
    assert!(!store.directory().join("objects/models").exists());
    assert!(!kernel.search(&query())?.hits.is_empty());
    assert!(kernel.semantic_search(&query()).is_err());
    Ok(())
}

#[test]
fn obsolete_database_version_is_rejected_before_reads_or_updates() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(source.path().join("original.txt"), "original evidence")?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let path = data
        .path()
        .join("collections-1.2/default/current-owner.json");
    let mut owner: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    owner["version"] = serde_json::json!(119);
    fs::write(&path, serde_json::to_vec(&owner)?)?;
    assert!(kernel.search(&query()).is_err());
    assert!(kernel.index().is_err());
    assert!(Kernel::open(source.path(), Some(data.path().into())).is_err());
    assert_eq!(
        fs::read_to_string(source.path().join("original.txt"))?,
        "original evidence"
    );
    Ok(())
}
