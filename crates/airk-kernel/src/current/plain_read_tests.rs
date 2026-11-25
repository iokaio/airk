// SPDX-License-Identifier: Apache-2.0
//! Kernel reads over a plain index built from real ingest extraction, laid out
//! as the mapping contract says (airk-store index CONTRACT.md, "Mapping of the
//! current index"). Expectations come from the extractor's own output.
use airk_core::{
    Artifact, ArtifactRef, Citation, FactQuery, SearchQuery,
    changes::{FileStamps, FileState},
    discovery::{DiscoveryOptions, FactDiscoveryOptions},
    extraction::{ExtractionPayload, ExtractionProfile},
    identity::{ArtifactId, Incarnation, Sha256Digest},
    publication::InventoryFile,
};
use airk_store::{
    FileStore,
    current::{
        CapturedInput, CurrentScope,
        collection::{CollectionConfiguration, CollectionState},
        lexical,
    },
    index::{
        Batch, ChunkEntry, EncodedPayload, FileChange, FileEntry, Handle, HandleKind, IndexWriter,
        Selector, SelectorKind,
    },
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

struct Extracted {
    profile: ExtractionProfile,
    full: Artifact,
    source: Vec<u8>,
    payload: ExtractionPayload,
}

fn extract(root: &std::path::Path) -> Result<BTreeMap<String, Extracted>> {
    let data = tempfile::tempdir()?;
    let policy = airk_ingest::adapters::profile::ExtractionPolicy {
        retained_profile: None,
        legacy_fallback_detail: None,
        detail: None,
        version: 1,
        rules: vec![],
    };
    let mut extracted = BTreeMap::new();
    airk_ingest::scan_payloads_with_budgets(
        root,
        data.path(),
        &airk_core::Snapshot::empty(root.canonicalize()?.to_string_lossy().into_owned()),
        &BTreeMap::new(),
        airk_ingest::ScanLimits::default(),
        &policy,
        true,
        |_| Ok(None),
        |profile, artifact, bytes, payload| {
            let payload = payload.context("extraction payload missing")?.clone();
            let full = airk_core::binding::bind_payload(
                &payload,
                profile,
                airk_core::binding::ArtifactBinding {
                    path: &artifact.path,
                    source_hash: artifact.content_hash.parse()?,
                    source_bytes: artifact.bytes,
                },
            )?;
            extracted.insert(
                artifact.path.clone(),
                Extracted {
                    profile: profile.clone(),
                    full,
                    source: bytes.to_vec(),
                    payload,
                },
            );
            Ok(artifact.clone())
        },
    )?;
    Ok(extracted)
}

/// The contract's selectors: fact name and kind, edge source and target, with
/// the terminal-call rule of postings version 1.
fn selectors(artifact: &Artifact) -> Vec<Selector> {
    let mut set = BTreeSet::new();
    let mut add = |kind, value: &str| {
        set.insert(Selector {
            kind,
            value: value.into(),
        });
    };
    for fact in &artifact.facts {
        add(SelectorKind::FactName, &fact.name);
        add(SelectorKind::FactKind, &fact.kind);
        if let Some(code) = &fact.code {
            if let Some(name) = airk_core::code::terminal_call_name(code, &fact.kind) {
                add(SelectorKind::FactName, name);
            }
            add(SelectorKind::FactName, &code.qualified_name);
            add(SelectorKind::FactName, &code.symbol_id);
            if let Some(owner) = &code.owner_id {
                add(SelectorKind::EdgeSource, owner);
                add(SelectorKind::EdgeTarget, &code.symbol_id);
            }
            if let Some(target) = &code.target {
                let source = code
                    .owner_id
                    .clone()
                    .unwrap_or_else(|| format!("file:{}", artifact.path));
                add(SelectorKind::EdgeSource, &source);
                add(SelectorKind::EdgeTarget, target);
            }
        }
    }
    set.into_iter().collect()
}

fn entry(root: &str, extracted: &Extracted) -> Result<FileEntry> {
    let full = &extracted.full;
    let payload_bytes = serde_json::to_vec(&extracted.payload)?;
    let encoded = EncodedPayload::encode(&payload_bytes)?;
    let mut compact = full.clone();
    compact.retained = Some(ArtifactRef {
        legacy_origin: None,
        source_encoding: None,
        payload_encoding: None,
        object: encoded.digest.to_string(),
        object_bytes: payload_bytes.len(),
        raw_source: true,
        chunks: full.chunks.len(),
        extracted_bytes: full.extracted_bytes(),
        facts: full.facts.len(),
        profile: Some(extracted.profile.clone()),
    });
    compact.chunks.clear();
    compact.facts.clear();
    let mut id = [0; 16];
    id.copy_from_slice(&Sha256::digest(full.path.as_bytes())[..16]);
    let file = InventoryFile {
        continuity: Vec::new(),
        continuity_coverage: Default::default(),
        state: FileState {
            artifact_id: ArtifactId::from_bytes(id),
            stamps: FileStamps {
                first_generation: 1,
                path_generation: 1,
                content_generation: 1,
                evidence_generation: 1,
            },
            source_hash: full.content_hash.parse()?,
            profile_id: extracted.profile.identity()?,
            payload_digest: encoded.digest,
            evidence_digest: Sha256Digest::from_bytes([7; 32]),
            root: root.into(),
            path: full.path.clone(),
        },
        source_bytes: full.bytes,
        payload_bytes: payload_bytes.len(),
        extracted_bytes: full.extracted_bytes(),
        chunks: full.chunks.len(),
        indexed_lines: full.chunks.iter().map(|c| c.end_line).max().unwrap_or(0),
        facts: full.facts.len(),
        raw_source: true,
        language: full.language.clone(),
        extraction_method: full.extraction_method.clone(),
        parse_status: full.parse_status.clone(),
    };
    let chunks = full
        .chunks
        .iter()
        .enumerate()
        .map(|(ordinal, chunk)| -> Result<ChunkEntry> {
            Ok(ChunkEntry {
                stable_id: lexical::stable_id(&full.path, ordinal as u64)?,
                citation: serde_json::to_vec(&Citation {
                    path: full.path.clone(),
                    content_hash: full.content_hash.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                })?,
                terms: lexical::chunk_terms(&full.path, &chunk.text),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(FileEntry {
        path: full.path.clone(),
        source_hash: full.content_hash.parse()?,
        language: full.language.clone(),
        record: serde_json::to_vec(&CapturedInput::new(compact))?,
        metadata: serde_json::to_vec(&file)?,
        stat: None,
        payload: encoded,
        chunks,
        selectors: selectors(full),
        handles: vec![Handle {
            kind: HandleKind::Artifact,
            value: id.to_vec(),
        }],
        vector_pending: false,
    })
}

#[test]
fn kernel_reads_a_plain_index_of_real_extractions() -> Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("src"))?;
    let code = "pub struct Widget;\n\npub fn build_widget() -> Widget {\n    Widget\n}\n";
    std::fs::write(source.path().join("src").join("lib.rs"), code)?;
    let notes = "# Widget notes\n\nThe widget is assembled by hand.\n";
    std::fs::write(source.path().join("notes.md"), notes)?;
    let extracted = extract(source.path())?;
    assert_eq!(
        extracted.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["notes.md", "src/lib.rs"]
    );

    let directory = tempfile::tempdir()?;
    let store = FileStore::new(directory.path().join("collections-1.2").join("plain"));
    drop(store.lock()?);
    let scope = CurrentScope {
        root: "fixture-root".into(),
        collection: "plain".into(),
        incarnation: Incarnation::from_bytes([3; 16]),
    };
    let mut writer = IndexWriter::create(store.directory())?;
    let mut batch = Batch::default();
    for value in extracted.values() {
        batch
            .files
            .push(FileChange::Upsert(Box::new(entry(&scope.root, value)?)));
    }
    writer.apply(batch)?;
    let state = CollectionState {
        version: 1,
        configuration: CollectionConfiguration {
            retained_profile: None,
            source_policy_digest: None,
            roots: BTreeMap::new(),
            extractor_version: "plain-kernel-test/1".into(),
            extraction_policy_digest: Sha256Digest::from_bytes([4; 32]),
            index_detail: airk_core::extraction::IndexDetail::Balanced,
            vector_selection: Default::default(),
            model: None,
        },
        counts: Default::default(),
    };
    let connection = rusqlite::Connection::open(store.directory().join("index.sqlite3"))?;
    for (key, value) in [
        ("scope", serde_json::to_vec(&scope)?),
        ("collection_state", serde_json::to_vec(&state)?),
        (
            "lexical_commit",
            airk_store::current::plain::lexical_commit(&scope, 1)
                .as_bytes()
                .to_vec(),
        ),
    ] {
        connection.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            rusqlite::params![key, value],
        )?;
    }
    drop(connection);

    assert_eq!(
        store.current_scope(&scope.root, &scope.collection)?,
        Some(scope.clone())
    );
    let read = store.current_read(&scope)?;
    assert_eq!(read.head().sequence, 1);

    // Lexical candidates materialize against the stored extraction.
    let query = SearchQuery {
        text: "build_widget".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 65_536,
    };
    let selected = super::open_selected_lexical(&store, &read)?;
    let candidates = selected.search(&query)?;
    assert!(!candidates.values.is_empty());
    let evidence = super::materialize_lexical(&store, &read, &query, candidates)?;
    assert!(
        evidence
            .hits
            .iter()
            .all(|hit| hit.citation.path == "src/lib.rs" && hit.text.contains("build_widget"))
    );

    // Symbols come from the payload the extractor produced.
    let library = &extracted["src/lib.rs"];
    let expected = library
        .full
        .facts
        .iter()
        .filter(|fact| fact.kind == "definition" && fact.name == "Widget")
        .count();
    assert!(
        expected > 0,
        "the Rust extractor reports the Widget definition"
    );
    let symbols = read.native_facts_page_with_options(
        &store,
        &FactQuery {
            path_prefix: String::new(),
            name: Some("Widget".into()),
            kind: Some("definition".into()),
            language: None,
            exact_name: true,
            limit: 50,
            max_bytes: 65_536,
            path_exact: false,
            document_only: false,
        },
        true,
        &DiscoveryOptions::default(),
        &FactDiscoveryOptions::default(),
    )?;
    assert_eq!(
        symbols
            .facts
            .iter()
            .filter(|hit| hit.citation.path == "src/lib.rs" && hit.name == "Widget")
            .count(),
        expected
    );

    // Source bytes exist exactly for utf8 extractions (Q5).
    for (path, value) in &extracted {
        let compact = read.artifact(path)?.context("record missing")?;
        assert_eq!(
            serde_json::to_vec(&read.load_retained(&compact)?)?,
            serde_json::to_vec(&value.full)?
        );
        read.verify_retained_source(&compact)?;
        let stored = read.read_retained_source(&compact);
        if value.full.extraction_method == "utf8" {
            assert_eq!(stored?, value.source);
        } else {
            assert!(stored.is_err(), "{path} ({})", value.full.extraction_method);
        }
        let artifact = super::load_lexical_artifact(&read, &compact)?;
        assert_eq!(artifact.chunks.len(), value.full.chunks.len());
    }
    Ok(())
}
