// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::current::{
    analysis::AnalysisEvidence,
    postings::Lookup,
    query_projection::{ProjectionRow, select_paths},
};
use airk_core::{
    AnalysisManifest, Citation, EvidenceBasis, FactHit, RelationshipHit,
    compression::CompressionMode,
};
use std::collections::BTreeMap;

fn evidence(
    fixture: &Fixture,
    facts: usize,
) -> Result<(AnalysisEvidence, BTreeMap<String, String>)> {
    let input = retained_input_at(fixture, "one.txt", "body")?;
    let citation = Citation {
        path: "one.txt".into(),
        content_hash: input.artifact.content_hash,
        start_line: 1,
        end_line: 1,
        location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
    };
    let fact = FactHit {
        candidate: false,
        continuity_key: None,
        name: "same".into(),
        kind: "heading".into(),
        syntax_kind: "heading".into(),
        language: "producer-language".into(),
        citation: citation.clone(),
        document: None,
        code: None,
    };
    let facts = (0..facts)
        .map(|i| FactHit {
            name: format!("Name {i:04} Ω"),
            ..fact.clone()
        })
        .collect();
    let relationships = (0..300)
        .map(|i| RelationshipHit {
            source: format!("local:{}", i % 7),
            target: format!("foreign:{}", i % 11),
            kind: "reference".into(),
            resolution: "provider_asserted".into(),
            basis: EvidenceBasis::Imported,
            producer: "original-fixture".into(),
            producer_version: "1".into(),
            citation: citation.clone(),
        })
        .collect();
    Ok((
        AnalysisEvidence {
            manifest: AnalysisManifest {
                version: 1,
                root: fixture.scope.root.clone(),
                collection: fixture.scope.collection.clone(),
                extractor_version: "fixture".into(),
                source_hashes: BTreeMap::from([("one.txt".into(), citation.content_hash)]),
                build_inputs: BTreeMap::new(),
                producer: "original-fixture".into(),
                producer_version: "1".into(),
                index_sha256: None,
            },
            facts,
            relationships,
        },
        BTreeMap::from([("one.txt".into(), "text".into())]),
    ))
}

#[test]
fn imported_pages_preserve_ordinals_languages_foreign_targets_and_selected_io() -> Result<()> {
    let fixture = Fixture::new()?;
    let (evidence, languages) = evidence(&fixture, 600)?;
    let writer = fixture.store.lock()?;
    let mandatory = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
    let bytes = evidence.canonical_bytes()?;
    let analysis = fixture
        .store
        .retain_current_resource_owned(&writer, &bytes, &mandatory)?;
    for mode in [
        CompressionMode::Off,
        CompressionMode::Automatic,
        CompressionMode::Space,
    ] {
        let prepared = fixture
            .store
            .prepare_import_projection(&evidence, &analysis, &languages, mode)?;
        let (root, _guard) = fixture
            .store
            .stage_query_projection(&writer, &fixture.quota, &prepared)?
            .context("projection declined")?;
        fixture
            .store
            .verify_import_projection(&root, &evidence, &analysis, &languages)?;
        let header = fixture.store.projection_header(&root)?;
        let selected = select_paths(
            &fixture.store,
            &header,
            Lookup::FactName("Name 0000 Ω"),
            "",
            None,
            256,
        )?;
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].pages, [0]);
        assert_eq!(
            select_paths(
                &fixture.store,
                &header,
                Lookup::Language("text"),
                "",
                None,
                256
            )?
            .len(),
            1
        );
        assert!(
            select_paths(
                &fixture.store,
                &header,
                Lookup::Language("producer-language"),
                "",
                None,
                256
            )?
            .is_empty()
        );
        let incoming = select_paths(
            &fixture.store,
            &header,
            Lookup::EdgeTarget("foreign:1"),
            "",
            None,
            256,
        )?;
        assert_eq!(incoming.len(), 1);
        assert!(incoming[0].pages.iter().all(|page| *page >= 3));
        let mut paths = std::collections::BTreeSet::new();
        fixture.store.retain_projection_paths(&root, &mut paths)?;
        assert!(paths.len() > header.pages);
        let unrelated = fixture.store.projection_leaf(&header, 2)?;
        fs::write(
            fixture
                .store
                .directory()
                .join("current-resources")
                .join(format!("{}.json", unrelated.object_digest()?)),
            b"damaged",
        )?;
        // Logical source/membership certification does not consume data pages.
        // It cannot substitute for the resumed physical check of those pages.
        fixture
            .store
            .certify_import_projection(&root, &evidence, &analysis, &languages)?;
        // The selected lookup and first fact page still work without decoding
        // the damaged unrelated data page. Complete certification must refuse.
        assert_eq!(
            select_paths(
                &fixture.store,
                &header,
                Lookup::FactName("Name 0000 Ω"),
                "",
                None,
                256
            )?
            .len(),
            1
        );
        assert!(matches!(
            fixture.store.projection_page(&header, 0)?[0],
            ProjectionRow::Fact { ordinal: 0, .. }
        ));
        assert!(
            fixture
                .store
                .verify_import_projection(&root, &evidence, &analysis, &languages)
                .is_err()
        );
        // Restore through the same admitted immutable producer representation.
        fs::remove_file(
            fixture
                .store
                .directory()
                .join("current-resources")
                .join(format!("{}.json", unrelated.object_digest()?)),
        )?;
        for resource in &prepared.resources {
            fixture
                .store
                .retain_prepared_resource(resource, Some(&mandatory), None)?;
        }
    }
    assert_eq!(fixture.store.read_current_resource(&analysis)?, bytes);
    Ok(())
}

#[test]
fn omitted_selector_is_not_certified_by_intact_fact_and_edge_pages() -> Result<()> {
    let fixture = Fixture::new()?;
    let (evidence, languages) = evidence(&fixture, 8)?;
    let writer = fixture.store.lock()?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
    let analysis = fixture.store.retain_current_resource_owned(
        &writer,
        &evidence.canonical_bytes()?,
        &reservation,
    )?;
    let prepared = fixture.store.prepare_import_projection(
        &evidence,
        &analysis,
        &languages,
        CompressionMode::Off,
    )?;
    let (root, _guard) = fixture
        .store
        .stage_query_projection(&writer, &fixture.quota, &prepared)?
        .unwrap();
    let mut header = fixture.store.projection_header(&root)?;
    header.selectors = None;
    header.selector_keys = 0;
    let mut omitted = fixture.store.retain_current_resource_owned(
        &writer,
        &serde_json::to_vec(&header)?,
        &reservation,
    )?;
    omitted.projection = root.projection.clone();
    assert!(
        fixture
            .store
            .certify_import_projection(&omitted, &evidence, &analysis, &languages)
            .is_err()
    );
    assert!(
        fixture
            .store
            .verify_import_projection(&omitted, &evidence, &analysis, &languages)
            .is_err()
    );
    fixture
        .store
        .verify_import_projection(&root, &evidence, &analysis, &languages)?;
    Ok(())
}

#[test]
fn imported_interleaved_paths_keep_original_per_path_cursor_ordinals() -> Result<()> {
    let fixture = Fixture::new()?;
    let (mut evidence, mut languages) = evidence(&fixture, 600)?;
    let hash = evidence.facts[0].citation.content_hash.clone();
    evidence
        .manifest
        .source_hashes
        .insert("two.txt".into(), hash);
    languages.insert("two.txt".into(), "text".into());
    for (ordinal, hit) in evidence.facts.iter_mut().enumerate() {
        if ordinal % 2 == 1 {
            hit.citation.path = "two.txt".into();
        }
    }
    let writer = fixture.store.lock()?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let analysis = fixture.store.retain_current_resource_owned(
        &writer,
        &evidence.canonical_bytes()?,
        &reservation,
    )?;
    let prepared = fixture.store.prepare_import_projection(
        &evidence,
        &analysis,
        &languages,
        CompressionMode::Off,
    )?;
    let (root, _guard) = fixture
        .store
        .stage_query_projection(&writer, &fixture.quota, &prepared)?
        .unwrap();
    fixture
        .store
        .verify_import_projection(&root, &evidence, &analysis, &languages)?;
    let header = fixture.store.projection_header(&root)?;
    for page in 0..header.pages {
        for row in fixture.store.projection_page(&header, page)? {
            if let ProjectionRow::Fact {
                ordinal,
                path_ordinal,
                ..
            } = row
            {
                assert_eq!(path_ordinal, ordinal / 2);
            }
        }
    }
    Ok(())
}

#[test]
fn valid_large_import_row_keeps_authoritative_resource_when_optional_preparation_declines()
-> Result<()> {
    let fixture = Fixture::new()?;
    let (mut evidence, languages) = evidence(&fixture, 1)?;
    evidence.facts[0].name = "large".repeat((17 << 20) / 5);
    let writer = fixture.store.lock()?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
    let bytes = evidence.canonical_bytes()?;
    let analysis = fixture
        .store
        .retain_current_resource_owned(&writer, &bytes, &reservation)?;
    let error = fixture
        .store
        .prepare_import_projection(&evidence, &analysis, &languages, CompressionMode::Automatic)
        .err()
        .context("large optional row was not declined")?;
    assert!(error.is::<crate::current::native_projection::ProjectionPreparationLimit>());
    assert_eq!(fixture.store.read_current_resource(&analysis)?, bytes);
    Ok(())
}

#[test]
fn import_format_ceiling_supports_100000_facts_edges_and_source_entries() -> Result<()> {
    let fixture = Fixture::new()?;
    let (mut evidence, languages) = evidence(&fixture, 1)?;
    let mut fact = evidence.facts.pop().unwrap();
    fact.name = "a".into();
    fact.kind = "k".into();
    fact.syntax_kind = "k".into();
    fact.language = "x".into();
    let mut edge = evidence.relationships.pop().unwrap();
    edge.source = "s".into();
    edge.target = "t".into();
    edge.kind = "k".into();
    edge.resolution = "x".into();
    edge.producer = "p".into();
    edge.producer_version = "1".into();
    let hash = fact.citation.content_hash.clone();
    evidence.facts = vec![fact; 100_000];
    evidence.relationships = vec![edge; 100_000];
    evidence.manifest.source_hashes = (0..99_999)
        .map(|i| (format!("p{i:05}"), hash.clone()))
        .collect();
    evidence
        .manifest
        .source_hashes
        .insert("one.txt".into(), hash);
    let bytes = evidence.canonical_bytes()?;
    assert!(bytes.len() <= 64 << 20);
    // Pure format preparation does not assert that the other source paths have
    // been published. The accepting journal separately requires that full set.
    let analysis = crate::current::resources::ResourceReference {
        digest: airk_core::identity::framed_digest(b"current-resource/1", &[&bytes]),
        bytes: bytes.len(),
        encoding: None,
        projection: None,
    };
    drop(bytes);
    let prepared = fixture.store.prepare_import_projection(
        &evidence,
        &analysis,
        &languages,
        CompressionMode::Off,
    )?;
    let summary = prepared
        .root
        .projection
        .as_ref()
        .context("projection summary absent")?;
    assert_eq!((summary.facts, summary.edges), (100_000, 100_000));
    assert!(prepared.memory_bytes() <= 64 << 20);
    Ok(())
}
