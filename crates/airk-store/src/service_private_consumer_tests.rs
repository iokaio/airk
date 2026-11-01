// SPDX-License-Identifier: Apache-2.0
//! Actual sealed service reads, distinct from public strict adapter fixtures.
use super::*;
use airk_core::{FactQuery, discovery::DiscoveryOptions};
use std::sync::Arc;

struct ConsumerFixture {
    collection: Collection,
    registry: crate::service::registry::ServiceRegistry,
    bound: crate::service::data_scope::BoundDataRoot,
    store: FileStore,
    scope: CurrentScope,
    owner: integrity::native_owner::NativeOwnerSession,
    lease: Arc<ProjectLease>,
    _temporary: tempfile::TempDir,
}
fn fact_query(limit: usize, max_bytes: usize) -> FactQuery {
    FactQuery {
        path_prefix: "one.txt".into(),
        path_exact: true,
        limit,
        max_bytes,
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        document_only: false,
    }
}
fn fixture(imported: bool) -> Result<ConsumerFixture> {
    fixture_mode(imported, airk_core::compression::CompressionMode::Off)
}
fn fixture_mode(
    imported: bool,
    mode: airk_core::compression::CompressionMode,
) -> Result<ConsumerFixture> {
    let (temporary, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let quota = Quota::open_service(store.managed_path()?.root)?;
    let facts = (0..600)
        .map(|id| airk_core::extraction::LocalFact {
            id,
            owner: None,
            kind: "call".into(),
            name: format!("native_{:04}", 599 - id),
            span: airk_core::extraction::LocalSpan {
                start: id as usize * 4,
                end: if id == 599 {
                    200_000
                } else {
                    id as usize * 4 + 4
                },
            },
            syntax_kind: "heading".into(),
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            document: None,
            code: Some(airk_core::extraction::LocalCodeEvidence {
                fingerprints: None,
                syntax_id: airk_core::extraction::LocalSyntaxId(id),
                owner_id: None,
                qualified_name: format!("native_{:04}", 599 - id),
                scope: vec![],
                signature: None,
                span: None,
                signature_span: None,
                parameters: None,
                return_type: None,
                modifiers: vec![],
                documentation: None,
                target: Some(format!("target_{}", id % 5)),
                basis: airk_core::EvidenceBasis::Syntax,
                producer: "private-fixture".into(),
                producer_version: "1".into(),
                coverage: "written syntax".into(),
                details_truncated: false,
            }),
            region: None,
            attributes: Default::default(),
        })
        .collect();
    let compact = projection_fixture_artifact(&store, &scope, facts)?;
    publish_projection_fixture_mode(&store, &quota, &scope, compact, mode)?;
    if imported {
        let source = store
            .current_read(&scope)?
            .artifact("one.txt")?
            .context("fixture source missing")?;
        let citation = airk_core::Citation {
            path: "one.txt".into(),
            content_hash: source.content_hash,
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
        };
        let relationships = [
            ("target_0", "imported_next"),
            ("imported_next", "file:one.txt"),
        ]
        .into_iter()
        .map(|(source, target)| airk_core::RelationshipHit {
            source: source.into(),
            target: target.into(),
            kind: "reference".into(),
            resolution: "provider_asserted".into(),
            basis: airk_core::EvidenceBasis::Imported,
            producer: "private-fixture".into(),
            producer_version: "1".into(),
            citation: citation.clone(),
        })
        .collect();
        publish_import_projection_fixture_edges(&store, &quota, &scope, relationships)?;
    }
    let lease = Arc::new(lease);
    let identity = airk_core::service::WorkerIdentity {
        service: registry.incarnation()?,
        worker: Incarnation::from_bytes([121; 16]),
        project: lease.identity().0,
        fencing_generation: lease.identity().1.context("fixture epoch missing")?,
    };
    let observed = store.current_read(&scope)?;
    registry.observe_retained_audit_index(&lease, &observed, 0, 512 << 20)?;
    drop(observed);
    let owner = collection.arm_native_owner(&mut registry, lease.clone(), &bound, identity)?;
    Ok(ConsumerFixture {
        _temporary: temporary,
        collection,
        registry,
        bound,
        store,
        scope,
        owner,
        lease,
    })
}

#[test]
fn private_native_imported_fact_pages_preserve_complete_cursor_and_representation_parity()
-> Result<()> {
    let mut fixture = fixture(true)?;
    let strict = fixture.store.current_read(&fixture.scope)?;
    let private = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    assert!(private.private_query_eligible()?);
    for (compact, minimal, max_bytes) in [
        (false, false, 65536),
        (true, false, 1700),
        (false, true, 3500),
    ] {
        let query = fact_query(97, max_bytes);
        let mut options = DiscoveryOptions {
            compact,
            minimal,
            ..Default::default()
        };
        let mut total = 0;
        for page in 0..500 {
            let expected = strict.native_facts_page(&fixture.store, &query, false, &options)?;
            let actual = private.native_facts_page(&fixture.store, &query, false, &options)?;
            assert_eq!(
                serde_json::to_value(&actual)?,
                serde_json::to_value(&expected)?,
                "page {page}, compact={compact}, minimal={minimal}"
            );
            total += actual.facts.len()
                + actual.outline.len()
                + actual.minimal.as_ref().map_or(0, |value| value.items.len());
            if !actual.truncated {
                break;
            }
            options.cursor = actual.page.cursor;
            assert!(
                options.cursor.is_some(),
                "positive response budget cannot stall"
            );
            assert!(page < 499, "cursor did not exhaust");
        }
        assert_eq!(total, 1200);
    }
    drop(private);
    fixture
        .owner
        .settle_after_owned_drain(&mut fixture.registry)?;
    Ok(())
}

#[test]
fn private_native_fact_budget_skips_later_leaf_and_retained_source() -> Result<()> {
    let fixture = fixture(false)?;
    let read = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    let projection = read
        .native_fact_projection(&fixture.store, "one.txt")?
        .context("native facts missing")?;
    assert!(projection.header().pages.len() > 1);
    let reference = &projection.header().pages.last().unwrap().resource;
    let leaf = fixture
        .store
        .directory()
        .join("current-resources")
        .join(format!("{}.json", reference.object_digest()?));
    let held = leaf.with_extension("held");
    fs::rename(&leaf, &held)?;
    let query = fact_query(1, 65536);
    let first = read.native_facts_page(&fixture.store, &query, false, &Default::default());
    fs::rename(held, leaf)?;
    assert_eq!(first?.facts[0].name, "native_0000");
    let compact = read.artifact("one.txt")?.context("artifact missing")?;
    let source = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&compact.content_hash);
    let held = source.with_extension("held");
    fs::rename(&source, &held)?;
    let selected = read.native_facts_page(&fixture.store, &query, false, &Default::default());
    let strict = fixture
        .store
        .current_read(&fixture.scope)?
        .native_facts_page(&fixture.store, &query, false, &Default::default());
    fs::rename(held, source)?;
    assert_eq!(selected?.facts[0].name, "native_0000");
    assert!(
        strict.is_err(),
        "strict reader unexpectedly skipped full source verification"
    );
    Ok(())
}

#[test]
fn private_selected_text_and_fact_corruption_revoke_authority_and_persist_damage() -> Result<()> {
    let fixture = fixture(false)?;
    let read = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    let text = read
        .text_range_projection(&fixture.store, "one.txt")?
        .context("text pages missing")?;
    let expected = text.read_strict(1, 1, 7)?;
    let actual = text.read_private(1, 1, 7)?;
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.location, expected.location);
    assert_eq!(actual.truncated, expected.truncated);
    let strict_chunk = fixture
        .store
        .load_retained(&read.artifact("one.txt")?.unwrap())?
        .chunks[0]
        .clone();
    let actual_chunk = text.chunk_private(0)?;
    assert_eq!(
        serde_json::to_value(actual_chunk)?,
        serde_json::to_value(strict_chunk)?
    );
    let projection = read
        .native_fact_projection(&fixture.store, "one.txt")?
        .context("native facts missing")?;
    let reference = &projection.header().pages[0].resource;
    let leaf = fixture
        .store
        .directory()
        .join("current-resources")
        .join(format!("{}.json", reference.object_digest()?));
    let mut bytes = fs::read(&leaf)?;
    bytes[0] ^= 1;
    fs::write(leaf, bytes)?;
    let query = fact_query(1, 65536);
    assert!(
        read.native_facts_page(&fixture.store, &query, false, &Default::default())
            .is_err()
    );
    assert!(
        read.token().is_err(),
        "damaged query authority remained live"
    );
    assert!(fixture.bound.live_integrity().revoked());
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture
            .collection
            .directory
            .join("integrity-observation.json"),
    )?)?;
    assert!(
        !receipt["known_damage"]
            .as_array()
            .context("damage array missing")?
            .is_empty()
    );
    Ok(())
}

#[test]
fn private_native_imported_relationships_preserve_traversal_and_budget_parity() -> Result<()> {
    let fixture = fixture(true)?;
    let strict = fixture.store.current_read(&fixture.scope)?;
    let private = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    let mut nonempty = 0;
    for symbol in ["file:one.txt", "target_0", "imported_next"] {
        for direction in ["incoming", "outgoing"] {
            for hops in 1..=3 {
                for prefix in ["", "one.txt", "other/"] {
                    let query = airk_core::RelationshipQuery {
                        symbol: symbol.into(),
                        direction: direction.into(),
                        kind: None,
                        path_prefix: prefix.into(),
                        hops,
                        limit: 17,
                        max_bytes: 65536,
                    };
                    let expected = strict.native_relationships(&fixture.store, &query)?;
                    let actual = private.native_relationships(&fixture.store, &query)?;
                    nonempty += usize::from(!actual.relationships.is_empty());
                    assert_eq!(
                        serde_json::to_value(actual)?,
                        serde_json::to_value(expected)?,
                        "{symbol}:{direction}:{hops}:{prefix}"
                    );
                }
            }
        }
    }
    assert!(
        nonempty > 12,
        "fixture failed to exercise both native/imported traversal"
    );
    Ok(())
}

#[test]
fn private_compressed_selected_fact_corruption_persists_known_damage() -> Result<()> {
    for mode in [
        airk_core::compression::CompressionMode::Automatic,
        airk_core::compression::CompressionMode::Space,
    ] {
        let fixture = fixture_mode(false, mode)?;
        let read = fixture
            .owner
            .attach_read(fixture.store.current_read(&fixture.scope)?)?;
        let projection = read
            .native_fact_projection(&fixture.store, "one.txt")?
            .context("native pages missing")?;
        let reference = &projection.header().pages[0].resource;
        assert!(
            reference.encoding.is_some(),
            "fixture failed to produce compressed page"
        );
        let leaf = fixture
            .store
            .directory()
            .join("current-resources")
            .join(format!("{}.json", reference.object_digest()?));
        let mut bytes = fs::read(&leaf)?;
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(leaf, bytes)?;
        assert!(
            read.native_facts_page(
                &fixture.store,
                &fact_query(1, 65536),
                false,
                &Default::default()
            )
            .is_err()
        );
        assert!(read.token().is_err());
        let receipt: serde_json::Value = serde_json::from_slice(&fs::read(
            fixture
                .collection
                .directory
                .join("integrity-observation.json"),
        )?)?;
        assert!(
            !receipt["known_damage"]
                .as_array()
                .context("damage array absent")?
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn private_native_source_ranges_preserve_utf8_and_reject_selected_raw_and_compressed_damage()
-> Result<()> {
    use airk_core::compression::CompressionMode;
    use sha2::{Digest, Sha256};
    for mode in [
        CompressionMode::Off,
        CompressionMode::Automatic,
        CompressionMode::Space,
    ] {
        let (_temporary, collection, mut registry, bound, lease, store, scope) =
            native_state_fixture()?;
        let quota = Quota::open_service(store.managed_path()?.root)?;
        let seed = projection_fixture_artifact(&store, &scope, Vec::new())?;
        let mut retrieval = store.load_retrieval_artifact(&seed)?;
        let text = format!("{}\u{1f980}{}tail", "x".repeat(65535), "y".repeat(150_000));
        retrieval.profile.contract_version = 2;
        retrieval.payload.contract_version = 2;
        retrieval.payload.text = text.clone();
        retrieval.payload.chunks[0].span.end = text.len();
        let artifact = airk_core::binding::bind_payload(
            &retrieval.payload,
            &retrieval.profile,
            airk_core::binding::ArtifactBinding {
                path: "one.txt",
                source_hash: Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into()),
                source_bytes: text.len(),
            },
        )?;
        let reservation = quota.reserve(&scope.collection, 8 << 20)?;
        let compact = store.retain_payload_encoded(
            &artifact,
            &retrieval.payload,
            &retrieval.profile,
            Some(text.as_bytes()),
            Some(&reservation),
            mode,
        )?;
        drop(reservation);
        publish_projection_fixture_mode(&store, &quota, &scope, compact.clone(), mode)?;
        let lease = Arc::new(lease);
        let identity = airk_core::service::WorkerIdentity {
            service: registry.incarnation()?,
            worker: Incarnation::from_bytes([122; 16]),
            project: lease.identity().0,
            fencing_generation: lease.identity().1.context("epoch missing")?,
        };
        let observed = store.current_read(&scope)?;
        registry.observe_retained_audit_index(&lease, &observed, 0, 512 << 20)?;
        drop(observed);
        let owner = collection.arm_native_owner(&mut registry, lease, &bound, identity)?;
        let private = owner.attach_read(store.current_read(&scope)?)?;
        let range = private
            .native_line_projection(&store, "one.txt")?
            .context("native line index absent")?;
        for budget in [1, 7, 65535, 65536] {
            let expected = range.read_strict(1, 1, budget)?;
            let actual = range.read_private(1, 1, budget)?;
            assert_eq!(actual.text, expected.text);
            assert_eq!(actual.truncated, expected.truncated);
        }
        let source_digest = compact
            .retained
            .as_ref()
            .unwrap()
            .source_encoding
            .as_ref()
            .map_or(compact.content_hash.as_str(), |encoding| {
                encoding.stored_digest.as_str()
            });
        let source = store
            .directory()
            .join("objects/sources")
            .join(source_digest);
        let original = fs::read(&source)?;
        if mode == CompressionMode::Off {
            let mut damaged = original.clone();
            damaged[2 * 65536] ^= 1;
            fs::write(&source, damaged)?;
            assert_eq!(range.read_private(1, 1, 7)?.text, "xxxxxxx");
            let strict = store.current_read(&scope)?;
            assert!(
                strict
                    .native_line_projection(&store, "one.txt")?
                    .unwrap()
                    .read_strict(1, 1, 7)
                    .is_err()
            );
            fs::write(&source, &original)?;
        }
        let mut damaged = original;
        damaged[0] ^= 1;
        fs::write(source, damaged)?;
        assert!(range.read_private(1, 1, 7).is_err());
        assert!(private.token().is_err());
        let receipt: serde_json::Value = serde_json::from_slice(&fs::read(
            collection.directory.join("integrity-observation.json"),
        )?)?;
        assert!(
            !receipt["known_damage"]
                .as_array()
                .context("damage missing")?
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn private_lexical_omitted_match_is_detected_by_actual_physical_maintenance() -> Result<()> {
    use airk_core::{SearchQuery, service::JobKind};
    let mut fixture = fixture(false)?;
    let query = SearchQuery {
        text: "one".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 65536,
    };
    let before = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    assert_eq!(before.lexical_search(&query)?.values.len(), 1);
    fixture
        .collection
        .integrity_owner(&fixture.registry, &fixture.lease, &fixture.bound)?
        .anchor(&before)?;
    drop(before);
    let path = fixture.store.directory().join("current.sqlite3");
    // Deliberate physical tamper outside the publication writer: preserve the
    // authenticated head while removing its promised FTS match.
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute("INSERT INTO current_fts(current_fts,rowid,terms) SELECT 'delete',id,terms FROM current_lexical_docs WHERE path='one.txt'",[])?;
    connection.execute("INSERT INTO current_fts(current_fts) VALUES('flush')", [])?;
    drop(connection);
    let read = fixture
        .owner
        .attach_read(fixture.store.current_read(&fixture.scope)?)?;
    assert!(
        read.lexical_search(&query)?.values.is_empty(),
        "fixture did not omit a physical FTS match"
    );
    drop(read);
    let namespace = fixture.registry.incarnation()?;
    let job = fixture.registry.enqueue_internal(
        fixture.lease.identity().0,
        namespace,
        JobKind::Verify,
        Incarnation::from_bytes([123; 16]),
    )?;
    fixture
        .registry
        .claim_job_id(&fixture.lease, job.id)?
        .context("audit claim missing")?;
    let audit =
        fixture
            .collection
            .integrity_owner(&fixture.registry, &fixture.lease, &fixture.bound)?;
    let mut writer = fixture.store.lock()?;
    let error = audit
        .verify_turn(&mut writer, job.id, 1, 512 << 20, 100, || Ok(()))
        .err()
        .context("physical maintenance accepted omitted lexical match")?;
    assert!(
        format!("{error:#}").contains("current FTS5 physical index digest mismatch"),
        "physical maintenance failed before checking the omitted match: {error:#}"
    );
    drop(writer);
    assert!(
        !fixture
            .registry
            .recorded_integrity_damage(&fixture.lease)?
            .is_empty()
    );
    assert!(fixture.bound.live_integrity().revoked());
    Ok(())
}

#[test]
fn interleaved_imported_paths_authenticate_each_shared_page_once_per_request() -> Result<()> {
    use crate::current::{
        ArtifactVectorChanges, analysis::AnalysisEvidence, graph, resources::ResourceChange,
    };
    let (_temporary, collection, mut registry, bound, lease, store, scope) =
        native_state_fixture()?;
    let quota = Quota::open_service(store.managed_path()?.root)?;
    let seed = projection_fixture_artifact(&store, &scope, Vec::new())?;
    let mut sources = std::collections::BTreeMap::new();
    for index in 0..8 {
        let mut compact = seed.clone();
        compact.path = format!("path{index}.txt");
        sources.insert(compact.path.clone(), compact.content_hash.clone());
        publish_projection_fixture(&store, &quota, &scope, compact)?;
    }
    let read = store.current_read(&scope)?;
    let base = read.head().clone();
    let configuration = read.collection_state()?.context("configuration missing")?;
    let evidence = AnalysisEvidence {
        manifest: airk_core::AnalysisManifest {
            version: 1,
            root: scope.root.clone(),
            collection: scope.collection.clone(),
            extractor_version: configuration.configuration.extractor_version,
            source_hashes: sources.clone(),
            build_inputs: Default::default(),
            producer: "interleaved-fixture".into(),
            producer_version: "1".into(),
            index_sha256: None,
        },
        facts: (0..600)
            .map(|ordinal| {
                let path = format!("path{}.txt", ordinal % 8);
                airk_core::FactHit {
                    candidate: false,
                    continuity_key: None,
                    name: format!("imported_{ordinal:04}"),
                    kind: "heading".into(),
                    syntax_kind: "heading".into(),
                    language: "text".into(),
                    citation: airk_core::Citation {
                        content_hash: sources[&path].clone(),
                        path,
                        start_line: 1,
                        end_line: 1,
                        location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
                    },
                    document: None,
                    code: None,
                }
            })
            .collect(),
        relationships: Vec::new(),
    };
    let reservation = quota.reserve(&scope.collection, 16 << 20)?;
    let sealed = graph::GraphOverlay::new(
        &reservation,
        read,
        graph::GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?
    .seal()?;
    let lock = store.lock()?;
    let reference =
        store.retain_current_resource_owned(&lock, &evidence.canonical_bytes()?, &reservation)?;
    let mut target = base.clone();
    target.sequence += 1;
    target.lexical_sequence = target.sequence;
    let capacity = store.current_database_capacity(&scope)?;
    let mut writer = store.current_write(&lock, &quota, &scope, capacity)?;
    let (pending, _) = writer.prepare_artifacts_with_fts(
        &base.token()?,
        target,
        &sealed,
        ArtifactVectorChanges {
            artifacts: &[],
            vector_inputs: None,
            occurrences: &[],
            configuration: None,
        },
        None,
        &[],
        &[ResourceChange {
            key: "analysis".into(),
            reference: Some(reference),
        }],
    )?;
    writer.mark_applying(&pending)?;
    writer.commit(&pending, pending.target.lexical_commit)?;
    drop(writer);
    drop(lock);
    drop(reservation);
    let strict = store.current_read(&scope)?;
    let lease = Arc::new(lease);
    registry.observe_retained_audit_index(&lease, &strict, 0, 512 << 20)?;
    let identity = airk_core::service::WorkerIdentity {
        service: registry.incarnation()?,
        worker: Incarnation::from_bytes([124; 16]),
        project: lease.identity().0,
        fencing_generation: lease.identity().1.context("epoch missing")?,
    };
    let owner = collection.arm_native_owner(&mut registry, lease, &bound, identity)?;
    let private = owner.attach_read(store.current_read(&scope)?)?;
    let projection = private
        .imported_projection(&store)?
        .context("imported projection missing")?;
    assert!(projection.header().pages > 1);
    let unique_pages = projection.header().pages;
    let query = FactQuery {
        path_prefix: "path".into(),
        path_exact: false,
        limit: 100,
        max_bytes: 65536,
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        document_only: false,
    };
    let options = DiscoveryOptions::default();
    let expected = strict.native_facts_page(&store, &query, false, &options)?;
    let before = crate::current::query_projection::query_page_read_count();
    let actual = private.native_facts_page(&store, &query, false, &options)?;
    let reads = crate::current::query_projection::query_page_read_count() - before;
    assert_eq!(
        serde_json::to_value(&actual)?,
        serde_json::to_value(expected)?
    );
    assert_eq!(actual.facts.len(), 100);
    assert!(
        actual
            .facts
            .iter()
            .any(|fact| fact.citation.path == "path1.txt"),
        "fixture did not cross a shared-page path boundary"
    );
    assert_eq!(
        reads, unique_pages,
        "interleaved paths reread a shared imported page"
    );
    // A new request starts its own bounded owner; no cross-query result reuse.
    let before = crate::current::query_projection::query_page_read_count();
    private.native_facts_page(&store, &query, false, &options)?;
    assert_eq!(
        crate::current::query_projection::query_page_read_count() - before,
        unique_pages
    );
    Ok(())
}
