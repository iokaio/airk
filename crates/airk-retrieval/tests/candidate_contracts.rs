// SPDX-License-Identifier: Apache-2.0
//! Original fixtures for bounded scoped search and legacy chunk identities.
use airk_core::{Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot};
use airk_retrieval::{
    model,
    persistent::{self, PersistentIndex},
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn legacy_scope_ceiling_reports_incomplete_without_loading_all_payloads() -> Result<()> {
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
            text: if index == 4096 { "distant" } else { "unknown" }.into(),
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
    persistent::prepare(&base, &snapshot, Some(&model_directory))?;
    let index = PersistentIndex::open(&base, &snapshot)?;
    let mut query = SearchQuery {
        text: "needle".into(),
        path_prefix: Some("wanted/".into()),
        limit: 1,
        max_bytes: 100,
    };
    let exhausted = index.semantic_search(&query)?;
    assert!(exhausted.hits.is_empty());
    assert!(
        exhausted.truncated,
        "scope beyond 4096 chunks is not exhausted evidence"
    );
    query.path_prefix = Some("absent/".into());
    let empty = index.semantic_search(&query)?;
    assert!(empty.hits.is_empty());
    assert!(!empty.truncated);
    query.path_prefix = None;
    assert_eq!(index.semantic_search(&query)?.hits[0].text, "distant");
    query.path_prefix = Some(String::new());
    assert_eq!(index.semantic_search(&query)?.hits[0].text, "distant");
    Ok(())
}

#[test]
fn old_generation_ordinals_keep_mapped_and_document_chunks_distinct() -> Result<()> {
    use airk_core::location::{ByteSpan, RegionLocation};
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().join("collection");
    let model_directory = temporary.path().join("model");
    fs::create_dir(&model_directory)?;
    let vocabulary = br#"{"needle":[1.0,0.0]}"#;
    fs::write(model_directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model_directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-locator-fixture","license":"Apache-2.0",
            "dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))
        }))?,
    )?;
    let mut snapshot = Snapshot::empty("legacy-locator-fixture".into());
    snapshot.embedding_model = Some(model::inspect(&model_directory)?.identity);
    snapshot.persistent_indexes = true;
    let locations = vec![
        DocumentLocation::Page { number: 1 },
        DocumentLocation::Page { number: 2 },
        DocumentLocation::Mapped {
            region: RegionLocation::Embedded {
                region_id: "first".into(),
                local_span: ByteSpan { start: 0, end: 6 },
                original_spans: vec![ByteSpan { start: 20, end: 26 }],
            },
        },
        DocumentLocation::Mapped {
            region: RegionLocation::Embedded {
                region_id: "second".into(),
                local_span: ByteSpan { start: 0, end: 6 },
                original_spans: vec![ByteSpan { start: 30, end: 36 }],
            },
        },
    ];
    snapshot.artifacts.insert(
        "evidence.fixture".into(),
        Artifact {
            path: "evidence.fixture".into(),
            content_hash: format!("{:x}", Sha256::digest(b"original")),
            bytes: 36,
            language: "text".into(),
            extraction_method: "fixture".into(),
            parse_status: "complete".into(),
            chunks: locations
                .iter()
                .map(|location| Chunk {
                    text: "needle".into(),
                    start_line: 1,
                    end_line: 1,
                    location: location.clone(),
                })
                .collect(),
            facts: vec![],
            retained: None,
        },
    );
    persistent::prepare(&base, &snapshot, Some(&model_directory))?;
    let index = PersistentIndex::open(&base, &snapshot)?;
    let query = SearchQuery {
        text: "needle".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 100,
    };
    let response = index.hybrid_search(&query)?;
    assert_eq!(response.hits.len(), locations.len());
    assert!(!response.truncated);
    assert_eq!(
        response
            .hits
            .iter()
            .map(|hit| hit.citation.location.clone())
            .collect::<Vec<_>>(),
        locations
    );
    Ok(())
}
