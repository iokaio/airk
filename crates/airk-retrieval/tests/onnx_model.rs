// SPDX-License-Identifier: Apache-2.0
use airk_core::{Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot};
use airk_retrieval::{
    model,
    persistent::{self, PersistentIndex},
};
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
    persistent::prepare(&base, &snapshot, Some(&directory))?;
    let index = PersistentIndex::open(&base, &snapshot)?;
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
    // A fresh read reopens copied assets, independent of the provisioned source directory.
    let copied = persistent::component_directory(&base, &snapshot)?.join("model");
    assert_eq!(
        model::inspect(&copied)?.identity,
        snapshot.embedding_model.as_ref().unwrap().as_str()
    );
    drop(index);
    let reopened = PersistentIndex::open(&base, &snapshot)?;
    assert_eq!(
        reopened.hybrid_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(copied.join("model.json"))?)?;
    manifest["max_tokens"] = serde_json::json!(128);
    fs::write(copied.join("model.json"), serde_json::to_vec(&manifest)?)?;
    let altered = PersistentIndex::open(&base, &snapshot)?;
    assert!(
        altered.semantic_search(&query).is_err(),
        "changed tokenization cannot pair with old vectors"
    );
    Ok(())
}
