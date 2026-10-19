// SPDX-License-Identifier: Apache-2.0
//! Original fixtures for bounded occurrence search independent of display chunks.
mod support;
use airk_core::{Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot};
use airk_retrieval::{model, persistent::PersistentIndex};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
#[ignore = "explicit 4097-node scoped-bound qualification; about 90 seconds in debug"]
fn occurrence_scope_ceiling_reports_incomplete_without_loading_all_payloads() -> Result<()> {
    scope_fixture(true)
}

#[test]
fn unembedded_display_chunks_do_not_consume_vector_scope_budget() -> Result<()> {
    scope_fixture(false)
}

fn scope_fixture(large_graph: bool) -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().join("collection");
    let model_directory = temporary.path().join("model");
    fs::create_dir(&model_directory)?;
    let vocabulary = br#"{"needle":[1.0,0.0],"distant":[0.0,1.0]}"#;
    fs::write(model_directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model_directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-scope-fixture","license":"Apache-2.0",
            "dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))
        }))?,
    )?;
    let mut snapshot = Snapshot::empty("legacy-scope-fixture".into());
    snapshot.embedding_model = Some(model::inspect(&model_directory)?.identity);
    snapshot.persistent_indexes = true;
    let chunks = (0..4097)
        .map(|index| Chunk {
            text: if index == 4096 {
                "needle"
            } else if large_graph {
                "distant"
            } else {
                "unknown"
            }
            .into(),
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
        })
        .collect();
    snapshot.artifacts.insert(
        "wanted/file.txt".into(),
        Artifact {
            path: "wanted/file.txt".into(),
            content_hash: format!("{:x}", Sha256::digest(b"original")),
            bytes: 4097 * 7,
            language: "text".into(),
            extraction_method: "fixture".into(),
            parse_status: "complete".into(),
            chunks,
            facts: vec![],
            retained: None,
        },
    );
    let source = support::FixtureSource::new(&snapshot)?;
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model_directory))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    let mut query = SearchQuery {
        text: "needle".into(),
        path_prefix: Some("wanted/".into()),
        limit: 1,
        max_bytes: 100,
    };
    let exhausted = index.semantic_search(&query)?;
    if large_graph {
        assert_eq!(exhausted.hits[0].text, "distant");
        assert!(
            exhausted.truncated,
            "scope beyond 4096 graph IDs is not exhausted evidence"
        );
    } else {
        assert_eq!(index.vectors, 1);
        assert_eq!(exhausted.hits[0].text, "needle");
        assert!(
            !exhausted.truncated,
            "display chunks without vectors must not exhaust scope"
        );
    }
    query.path_prefix = Some("absent/".into());
    let empty = index.semantic_search(&query)?;
    assert!(empty.hits.is_empty());
    assert!(!empty.truncated);
    query.path_prefix = None;
    assert_eq!(index.semantic_search(&query)?.hits[0].text, "needle");
    query.path_prefix = Some(String::new());
    assert_eq!(index.semantic_search(&query)?.hits[0].text, "needle");
    Ok(())
}
