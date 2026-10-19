// SPDX-License-Identifier: Apache-2.0
mod support;
use airk_core::{Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot};
use airk_retrieval::{model, persistent::PersistentIndex};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

/// Deliberate qualification only: ordinary CI never downloads weights.
#[test]
#[ignore = "requires explicitly provisioned AIRK_TEST_ONNX assets"]
fn real_transformer_publishes_reopens_and_keeps_generation_assets_isolated() -> Result<()> {
    let directory = PathBuf::from(
        std::env::var_os("AIRK_TEST_ONNX")
            .context("AIRK_TEST_ONNX must name provisioned local assets")?,
    );
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().join("collection");
    fs::create_dir_all(&base)?;
    let mut snapshot = Snapshot::empty("original-transformer-fixture".into());
    snapshot.embedding_model = Some(model::inspect(&directory)?.identity);
    snapshot.persistent_indexes = true;
    for (path, text) in [
        (
            "auth.txt",
            "Reset a forgotten account password with a time-limited recovery link sent to the verified email address.\n",
        ),
        (
            "food.txt",
            "The recipe combines flour and yeast, then bakes bread in a hot oven.\n",
        ),
    ] {
        snapshot.artifacts.insert(
            path.into(),
            Artifact {
                path: path.into(),
                content_hash: format!("{:x}", Sha256::digest(text)),
                bytes: text.len(),
                language: "text".into(),
                extraction_method: "utf8".into(),
                parse_status: "complete".into(),
                chunks: vec![Chunk {
                    text: text.into(),
                    start_line: 1,
                    end_line: 1,
                    location: DocumentLocation::Lines { start: 1, end: 1 },
                }],
                facts: Vec::new(),
                retained: None,
            },
        );
    }
    let source = support::FixtureSource::new(&snapshot)?;
    let receipt = support::prepare(&base, &snapshot, &source, Some(&directory))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    let query = SearchQuery {
        text: "Recover access after forgetting a login secret".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 1024,
    };
    let answer = index.semantic_search(&query)?;
    assert_eq!(answer.hits[0].citation.path, "auth.txt");
    assert_eq!(
        answer.hits[0].citation.content_hash,
        snapshot.artifacts["auth.txt"].content_hash
    );
    // Receipt-bound shared objects remain independent of provisioned source assets.
    let set = model::inspect_object_plan(&directory)?;
    model::inspect_objects_cache_descriptor(&base, &set)?;
    drop(index);
    let reopened = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(
        reopened.hybrid_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    drop(reopened);
    let manifest_object = &set.assets[&airk_core::model_objects::ModelAssetName::ModelJson];
    let copied = base
        .join("objects/models")
        .join(manifest_object.sha256.to_string());
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&copied)?)?;
    manifest["max_tokens"] = serde_json::json!(128);
    fs::write(copied, serde_json::to_vec(&manifest)?)?;
    assert!(
        PersistentIndex::open_v2(&base, &snapshot.collection, &receipt).is_err(),
        "changed tokenization cannot pair with old vectors"
    );
    Ok(())
}
