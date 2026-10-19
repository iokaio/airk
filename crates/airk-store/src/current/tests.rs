// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    FileStore,
    quota::{Quota, Reservation},
};
use airk_core::{
    graph_storage::{
        GraphHead, GraphRecord, GraphRecordOverlay, GraphRecordReader, GraphScope,
        PreparedGraphDelta,
    },
    identity::{Incarnation, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[path = "postings_tests.rs"]
mod postings_tests;

const CAPACITY: u64 = 8 * 1024 * 1024;

struct Fixture {
    directory: tempfile::TempDir,
    store: FileStore,
    quota: Quota,
    scope: CurrentScope,
    head: CurrentHead,
}
impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self::empty()?;
        let lock = fixture.store.lock()?;
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?
            .initialize(fixture.head.clone())?;
        drop(lock);
        Ok(fixture)
    }
    fn empty() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("source"))?;
        let quota = Quota::open(directory.path().join("data"))?;
        let store = FileStore::new(quota.root().join("collections/project-current"));
        let scope = CurrentScope {
            root: directory
                .path()
                .join("source")
                .to_string_lossy()
                .into_owned(),
            collection: "project-current".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let graph_scope = GraphScope {
            root: scope.root.clone(),
            collection: scope.collection.clone(),
            incarnation: scope.incarnation,
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 16,
        };
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: Sha256Digest::from_bytes([5; 32]),
            graph_head: GraphHead::empty(graph_scope)?,
        };
        let lock = store.lock()?;
        drop(lock);
        Ok(Self {
            directory,
            store,
            quota,
            scope,
            head,
        })
    }
    fn sealed(&self) -> Result<(Reservation, graph::SealedGraph)> {
        let reservation = self
            .quota
            .reserve(&self.scope.collection, 4 * 1024 * 1024)?;
        let read = self.store.current_read(&self.scope)?;
        let overlay = graph::GraphOverlay::new(
            &reservation,
            read,
            graph::GraphOverlayLimits {
                maximum_bytes: 1024 * 1024,
                ..Default::default()
            },
        )?;
        overlay.append(
            0,
            GraphRecord {
                embedding_key: None,
                live: false,
                bytes: vec![0; 16],
            },
        )?;
        overlay.append(
            1,
            GraphRecord {
                embedding_key: Some(Sha256Digest::from_bytes([6; 32])),
                live: true,
                bytes: vec![7; 16],
            },
        )?;
        Ok((reservation, overlay.seal()?))
    }
    fn target(&self, sealed: &graph::SealedGraph) -> CurrentHead {
        let mut head = self.head.clone();
        head.sequence = 1;
        head.graph_head = sealed.descriptor().target.clone();
        head
    }
    fn prepare(&self, inputs: &dyn CapturedInputs) -> Result<PendingTransaction> {
        let (_reservation, sealed) = self.sealed()?;
        let lock = self.store.lock()?;
        self.store
            .current_write(&lock, &self.quota, &self.scope, CAPACITY)?
            .prepare(
                &self.head.token()?,
                self.target(&sealed),
                &sealed,
                inputs,
                None,
            )
    }
}

#[test]
fn current_collection_rejects_legacy_sqlite_coexistence() -> Result<()> {
    let fixture = Fixture::new()?;
    let old = fixture.store.directory().join("index.sqlite3");
    fs::write(&old, b"legacy")?;
    let error = fixture
        .store
        .current_read(&fixture.scope)
        .err()
        .context("old database accepted")?;
    assert!(error.to_string().contains("incompatible with 1.2.0"));
    let writer = fixture.store.lock()?;
    let error = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
        .err()
        .context("old database accepted by writer")?;
    assert!(error.to_string().contains("incompatible with 1.2.0"));
    drop(writer);
    fs::remove_file(old)?;
    fixture.store.current_read(&fixture.scope)?;
    Ok(())
}

#[test]
fn gates_pin_one_request_prepared_is_readable_and_old_tokens_expire() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let clone = read.clone();
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(read);
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(clone);
    drop(writer);
    let pending = fixture.prepare(&[])?;
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        fixture.head.token()?
    );
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.pending()?, Some(pending.clone()));
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    let token = committed.token;
    assert!(current.pending()?.is_none());
    assert!(
        current
            .commit(&pending, pending.target.lexical_commit)
            .is_err()
    );
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, token);
    assert!(read.require_token(&fixture.head.token()?).is_err());
    assert_eq!(read.read(1)?.bytes, vec![7; 16]);
    assert!(read.vector_for_graph_id(1).is_err());
    assert_eq!(read.counts()?.allocated_count, 2);
    Ok(())
}

#[test]
fn failed_capture_rolls_back_graph_rows_and_preserves_orphan_capacity() -> Result<()> {
    struct Failure;
    impl CapturedInputs for Failure {
        fn visit(&self, _: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
            anyhow::bail!("injected capture failure")
        }
    }
    let fixture = Fixture::new()?;
    let error = fixture.prepare(&Failure).unwrap_err();
    assert!(error.to_string().contains("injected"));
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.head(),
        &fixture.head
    );
    fixture.quota.status()?;
    let writer = fixture.store.lock()?;
    assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
    let current = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    assert!(current.pending()?.is_none());
    assert_eq!(current.release_orphan_grants()?, 1);
    assert!(fixture.quota.current_grants(&writer)?.is_empty());
    Ok(())
}

fn retained_input(fixture: &Fixture, text: &str) -> Result<CapturedInput> {
    retained_input_at(fixture, "evidence.txt", text)
}

fn retained_input_at(fixture: &Fixture, path: &str, text: &str) -> Result<CapturedInput> {
    retained_input_with_facts(fixture, path, text, Vec::new())
}

fn retained_input_with_facts(
    fixture: &Fixture,
    path: &str,
    text: &str,
    facts: Vec<airk_core::extraction::LocalFact>,
) -> Result<CapturedInput> {
    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use std::collections::BTreeMap;
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "current-test/1".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    };
    let payload = ExtractionPayload {
        semantic_projection: Default::default(),
        continuity_coverage: Default::default(),
        contract_version: 1,
        extraction_method: "utf8".into(),
        parse_status: "plain".into(),
        provenance: vec![],
        text: text.into(),
        syntax_identities: facts
            .iter()
            .filter_map(|fact| {
                fact.code
                    .as_ref()
                    .map(|code| airk_core::extraction::LocalSyntaxIdentity {
                        id: code.syntax_id,
                        span: fact.span.clone(),
                        syntax_kind: fact.syntax_kind.clone(),
                        region: None,
                    })
            })
            .collect(),
        facts,
        chunks: vec![LocalChunk {
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            region: None,
        }],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    };
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path,
            source_hash: Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into()),
            source_bytes: text.len(),
        },
    )?;
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 1024 * 1024)?;
    let artifact = fixture.store.retain_payload_with_budget(
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(&reservation),
    )?;
    Ok(CapturedInput { artifact })
}

fn prepared_occurrence(
    fixture: &Fixture,
    input: &CapturedInput,
    embedding_key: Sha256Digest,
    graph_id: u32,
) -> Result<vector_map::PreparedOccurrence> {
    let index = vector_map::PathOccurrences {
        version: 1,
        path: input.artifact.path.clone(),
        source_hash: input.artifact.content_hash.parse()?,
        count: 1,
    };
    Ok(vector_map::PreparedOccurrence {
        pointer: vector_map::OccurrencePointer {
            members: vec![airk_core::vector_evidence::VectorMemberSpan {
                chunk_index: 0,
                span: airk_core::extraction::LocalSpan {
                    start: 0,
                    end: input.artifact.extracted_bytes(),
                },
            }],
            version: 1,
            embedding_key,
            occurrence_id: index.occurrence_id(&fixture.scope, 0)?,
            path: index.path,
            path_ordinal: 0,
            source_hash: index.source_hash,
            payload_digest: fixture
                .store
                .load_retrieval_artifact(&input.artifact)?
                .payload_digest,
            object: input
                .artifact
                .retained
                .as_ref()
                .context("test extraction missing")?
                .object
                .parse()?,
        },
        graph_id,
    })
}

#[test]
fn semantic_body_preserves_unicode_and_rejects_invalid_or_cross_location_spans() -> Result<()> {
    use airk_core::{
        extraction::LocalSpan,
        vector_evidence::{VectorMemberSpan, materialize},
    };
    let fixture = Fixture::new()?;
    let input = retained_input(&fixture, "alpha\nβeta\nlast")?;
    let mut retained = fixture.store.load_retrieval_artifact(&input.artifact)?;
    let member = VectorMemberSpan {
        chunk_index: 0,
        span: LocalSpan { start: 6, end: 11 },
    };
    let hit = materialize(&retained, std::slice::from_ref(&member))?;
    assert_eq!(hit.text, "βeta");
    assert_eq!((hit.citation.start_line, hit.citation.end_line), (2, 2));
    let mut invalid = member.clone();
    invalid.span.start = 7;
    assert!(materialize(&retained, &[invalid]).is_err());
    assert!(materialize(&retained, &[]).is_err());
    assert!(materialize(&retained, &[member.clone(), member]).is_err());
    let original = retained.artifact.chunks[0].clone();
    let mut first = original.clone();
    first.text = "alpha\n".into();
    let mut second = original;
    second.text = "βeta\nlast".into();
    second.start_line = 2;
    retained.artifact.chunks = vec![first, second];
    let mut local = retained.payload.chunks[0].clone();
    local.span.end = 6;
    let mut next = local.clone();
    next.span = LocalSpan { start: 6, end: 16 };
    retained.payload.chunks = vec![local, next];
    let members = vec![
        VectorMemberSpan {
            chunk_index: 0,
            span: LocalSpan { start: 0, end: 6 },
        },
        VectorMemberSpan {
            chunk_index: 1,
            span: LocalSpan { start: 6, end: 16 },
        },
    ];
    let hit = materialize(&retained, &members)?;
    assert_eq!(hit.text, "alpha\nβeta\nlast");
    assert_eq!((hit.citation.start_line, hit.citation.end_line), (1, 3));
    retained.artifact.chunks[0].location = airk_core::DocumentLocation::Page { number: 1 };
    retained.artifact.chunks[1].location = airk_core::DocumentLocation::Page { number: 2 };
    assert!(materialize(&retained, &members).is_err());
    retained.artifact.chunks[0].location = airk_core::DocumentLocation::Lines { start: 1, end: 1 };
    retained.artifact.chunks[1].location = airk_core::DocumentLocation::Lines { start: 2, end: 3 };
    retained.payload.chunks[1].region = Some(1);
    assert!(materialize(&retained, &members).is_err());
    Ok(())
}

#[test]
fn changed_artifact_attaches_and_replaces_exact_vector_occurrence() -> Result<()> {
    let fixture = Fixture::new()?;
    let key = Sha256Digest::from_bytes([6; 32]);
    let first = retained_input(&fixture, "First source")?;
    let occurrence = prepared_occurrence(&fixture, &first, key, 1)?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let mut foreign = occurrence.clone();
    foreign.pointer.object = Sha256Digest::from_bytes([99; 32]);
    assert!(
        current
            .prepare_artifacts_with_occurrences(
                &fixture.head.token()?,
                fixture.target(&sealed),
                &sealed,
                ArtifactVectorChanges {
                    artifacts: &[ArtifactChange::Upsert(Box::new(first.clone()))],
                    occurrences: &[foreign],
                },
                None,
                br#"{"engine":"test","sequence":1}"#,
            )
            .is_err()
    );
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    let mut invalid_span = occurrence.clone();
    invalid_span.pointer.members[0].span.end += 1;
    assert!(
        current
            .prepare_artifacts_with_occurrences(
                &fixture.head.token()?,
                fixture.target(&sealed),
                &sealed,
                ArtifactVectorChanges {
                    artifacts: &[ArtifactChange::Upsert(Box::new(first.clone()))],
                    occurrences: &[invalid_span],
                },
                None,
                br#"{"engine":"test","sequence":1}"#,
            )
            .is_err()
    );
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    let pending = current.prepare_artifacts_with_occurrences(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        ArtifactVectorChanges {
            artifacts: &[ArtifactChange::Upsert(Box::new(first.clone()))],
            occurrences: std::slice::from_ref(&occurrence),
        },
        None,
        br#"{"engine":"test","sequence":1}"#,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(
        read.vector_for_graph_id(1)?.occurrence_ids,
        vec![occurrence.pointer.occurrence_id]
    );
    assert_eq!(
        read.vector_occurrence_pointers(key)?,
        Some(vec![occurrence.pointer.clone()])
    );
    let hit = read.occurrence_hit(&fixture.store, &occurrence.pointer)?;
    assert_eq!(hit.text, "First source");
    assert_eq!(hit.citation.content_hash, first.artifact.content_hash);
    let mut forged = occurrence.pointer.clone();
    forged.members[0].span.end -= 1;
    assert!(read.occurrence_hit(&fixture.store, &forged).is_err());
    drop(read);

    let second = retained_input(&fixture, "Second source")?;
    let replacement = prepared_occurrence(&fixture, &second, key, 1)?;
    let base = fixture.store.current_read(&fixture.scope)?.head().clone();
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let sealed = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?
    .seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_artifacts_with_occurrences(
        &base.token()?,
        target,
        &sealed,
        ArtifactVectorChanges {
            artifacts: &[ArtifactChange::Upsert(Box::new(second.clone()))],
            occurrences: std::slice::from_ref(&replacement),
        },
        None,
        br#"{"engine":"test","sequence":2}"#,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    assert!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .occurrence_hit(&fixture.store, &occurrence.pointer)
            .is_err()
    );
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(
        read.vector_occurrence_pointers(key)?,
        Some(vec![replacement.pointer.clone()])
    );
    assert!(
        read.map_get(
            map::MapDomain::Occurrence,
            occurrence.pointer.occurrence_id.as_bytes()
        )?
        .is_none()
    );
    assert_eq!(read.vector_for_graph_id(1)?.graph_id, 1);
    drop(read);

    let alias = retained_input_at(&fixture, "alias.txt", "Same embedding input")?;
    let additional = prepared_occurrence(&fixture, &alias, key, 1)?;
    let base = fixture.store.current_read(&fixture.scope)?.head().clone();
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let sealed = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?
    .seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_artifacts_with_occurrences(
        &base.token()?,
        target,
        &sealed,
        ArtifactVectorChanges {
            artifacts: &[ArtifactChange::Upsert(Box::new(alias))],
            occurrences: std::slice::from_ref(&additional),
        },
        None,
        br#"{"engine":"test","sequence":3}"#,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    let mut expected = vec![replacement.pointer, additional.pointer];
    expected.sort_by_key(|pointer| pointer.occurrence_id);
    assert_eq!(read.vector_occurrence_pointers(key)?, Some(expected));
    assert_eq!(read.counts()?.live_nodes, 1);
    Ok(())
}

#[test]
fn new_occurrence_cannot_adopt_unmapped_committed_graph_node() -> Result<()> {
    let fixture = Fixture::new()?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[],
        None,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);

    let input = retained_input(&fixture, "Captured source")?;
    let occurrence = prepared_occurrence(&fixture, &input, Sha256Digest::from_bytes([6; 32]), 1)?;
    let base = fixture.store.current_read(&fixture.scope)?.head().clone();
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let sealed = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?
    .seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .prepare_artifacts_with_occurrences(
            &base.token()?,
            target,
            &sealed,
            ArtifactVectorChanges {
                artifacts: &[ArtifactChange::Upsert(Box::new(input))],
                occurrences: &[occurrence],
            },
            None,
            br#"{"engine":"test","sequence":2}"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("unmapped base graph node"));
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    Ok(())
}

#[test]
fn replay_uses_captured_bytes_and_detects_corruption_without_live_fallback() -> Result<()> {
    let fixture = Fixture::new()?;
    let original = "Captured source α.\n";
    fs::write(
        std::path::Path::new(&fixture.scope.root).join("evidence.txt"),
        original,
    )?;
    let inputs = [retained_input(&fixture, original)?];
    let pending = fixture.prepare(&inputs)?;
    fs::write(
        std::path::Path::new(&fixture.scope.root).join("evidence.txt"),
        "Later live edit",
    )?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let mut observed = 0;
    current.visit_captured(&pending, &mut |captured| {
        assert_eq!(
            fixture.store.read_retained_source(&captured.artifact)?,
            original.as_bytes()
        );
        observed += 1;
        Ok(())
    })?;
    assert_eq!(observed, 1);
    let path = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&inputs[0].artifact.content_hash);
    fs::write(path, b"corrupt")?;
    assert!(current.mark_applying(&pending).is_err());
    assert_eq!(current.phase()?, TransactionPhase::Prepared);
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&fixture.scope.root).join("evidence.txt"))?,
        "Later live edit"
    );
    Ok(())
}

#[test]
fn prepared_map_and_graph_replay_commit_one_authenticated_head() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input(&fixture, "Captured citation body.")?;
    let (_reservation, sealed) = fixture.sealed()?;
    let mut target = fixture.target(&sealed);
    let embedding_key = Sha256Digest::from_bytes([6; 32]);
    let path_occurrences = vector_map::PathOccurrences {
        version: 1,
        path: input.artifact.path.clone(),
        source_hash: input.artifact.content_hash.parse()?,
        count: 1,
    };
    let occurrence_id = path_occurrences.occurrence_id(&fixture.scope, 0)?;
    let binding = vector_map::VectorBinding {
        version: 1,
        embedding_key,
        graph_scope: target.graph_head.scope.digest(),
        graph_id: 1,
        occurrence_ids: vec![occurrence_id],
    };
    let pointer = vector_map::OccurrencePointer {
        members: vec![airk_core::vector_evidence::VectorMemberSpan {
            chunk_index: 0,
            span: airk_core::extraction::LocalSpan {
                start: 0,
                end: input.artifact.extracted_bytes(),
            },
        }],
        version: 1,
        embedding_key,
        occurrence_id,
        path: input.artifact.path.clone(),
        path_ordinal: 0,
        source_hash: input.artifact.content_hash.parse()?,
        payload_digest: Sha256Digest::from_bytes([9; 32]),
        object: Sha256Digest::from_bytes([10; 32]),
    };
    let mut changes = vec![
        map::MapChange::new(
            &fixture.scope,
            map::MapDomain::Artifact,
            input.artifact.path.as_bytes(),
            Some(input.bytes()?),
        )?,
        map::MapChange::new(
            &fixture.scope,
            map::MapDomain::Vector,
            embedding_key.as_bytes(),
            Some(binding.canonical_bytes(&target)?),
        )?,
        map::MapChange::new(
            &fixture.scope,
            map::MapDomain::Occurrence,
            occurrence_id.as_bytes(),
            Some(pointer.canonical_bytes()?),
        )?,
        map::MapChange::new(
            &fixture.scope,
            map::MapDomain::PathOccurrences,
            input.artifact.path.as_bytes(),
            Some(path_occurrences.canonical_bytes()?),
        )?,
    ];
    changes.sort_by_key(map::MapChange::key);
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, fixture.head.artifact_root)?;
    for change in &changes {
        plan.put(change.key(), change.value())?;
    }
    let mut postings = postings::PostingChanges::default();
    postings.update(
        &mut plan,
        &fixture.scope,
        &fixture.store,
        None,
        Some(&input.artifact),
    )?;
    changes.extend(postings.into_changes());
    changes.sort_by_key(map::MapChange::key);
    target.artifact_root = plan.root()?;
    drop(connection);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let captured = [input.clone()];
    let pending = current.prepare_mapped(
        &fixture.head.token()?,
        target,
        &sealed,
        &captured,
        None,
        &changes,
    )?;
    assert_eq!(pending.map_count, 6);
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, committed.token);
    assert_eq!(
        read.artifact(&input.artifact.path)?.unwrap().content_hash,
        input.artifact.content_hash
    );
    assert_eq!(read.vector_for_graph_id(1)?, binding);
    assert_eq!(
        read.path_occurrences(&input.artifact.path)?,
        Some(path_occurrences)
    );
    assert_eq!(
        read.vector_occurrence_pointers(embedding_key)?,
        Some(vec![pointer])
    );
    drop(read);

    let base = fixture.store.current_read(&fixture.scope)?.head().clone();
    let no_op_reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let no_op = graph::GraphOverlay::new(
        &no_op_reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?
    .seal()?;
    let mut invalid_target = base.clone();
    invalid_target.sequence += 1;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .prepare_artifacts(
            &base.token()?,
            invalid_target,
            &no_op,
            &[ArtifactChange::Delete(input.artifact.path.clone())],
            None,
            br#"{"engine":"test","version":2}"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("live mapping"));
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    drop(current);
    drop(writer);
    drop(no_op);
    drop(no_op_reservation);

    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let overlay = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?;
    overlay.set_live(1, false)?;
    let sealed = overlay.seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let deletion = current.prepare_artifacts(
        &base.token()?,
        target,
        &sealed,
        &[ArtifactChange::Delete(input.artifact.path.clone())],
        None,
        br#"{"engine":"test","version":2}"#,
    )?;
    assert_eq!(deletion.map_count, 6);
    current.mark_applying(&deletion)?;
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&deletion)?;
    current.commit(&deletion, deletion.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.artifact(&input.artifact.path)?.is_none());
    assert!(read.path_occurrences(&input.artifact.path)?.is_none());
    assert_eq!(
        read.vector_binding(embedding_key)?.unwrap().occurrence_ids,
        Vec::<Sha256Digest>::new()
    );
    assert!(read.vector_for_graph_id(1).is_err());
    assert_eq!(
        read.map_get(map::MapDomain::Occurrence, occurrence_id.as_bytes())?,
        None
    );
    Ok(())
}

#[test]
fn changed_path_retires_only_its_share_of_a_live_vector() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = retained_input_at(&fixture, "evidence.txt", "First source")?;
    let second = retained_input_at(&fixture, "second.txt", "Second source")?;
    let (_reservation, sealed) = fixture.sealed()?;
    let mut target = fixture.target(&sealed);
    let embedding_key = Sha256Digest::from_bytes([6; 32]);
    let mut changes = Vec::new();
    let mut occurrence_ids = Vec::new();
    for input in [&first, &second] {
        let path_index = vector_map::PathOccurrences {
            version: 1,
            path: input.artifact.path.clone(),
            source_hash: input.artifact.content_hash.parse()?,
            count: 1,
        };
        let occurrence_id = path_index.occurrence_id(&fixture.scope, 0)?;
        occurrence_ids.push(occurrence_id);
        let pointer = vector_map::OccurrencePointer {
            members: vec![airk_core::vector_evidence::VectorMemberSpan {
                chunk_index: 0,
                span: airk_core::extraction::LocalSpan {
                    start: 0,
                    end: input.artifact.extracted_bytes(),
                },
            }],
            version: 1,
            embedding_key,
            occurrence_id,
            path: input.artifact.path.clone(),
            path_ordinal: 0,
            source_hash: path_index.source_hash,
            payload_digest: Sha256Digest::from_bytes([9; 32]),
            object: Sha256Digest::from_bytes([10; 32]),
        };
        for (domain, identity, value) in [
            (
                map::MapDomain::Artifact,
                input.artifact.path.as_bytes(),
                input.bytes()?,
            ),
            (
                map::MapDomain::PathOccurrences,
                input.artifact.path.as_bytes(),
                path_index.canonical_bytes()?,
            ),
            (
                map::MapDomain::Occurrence,
                occurrence_id.as_bytes().as_slice(),
                pointer.canonical_bytes()?,
            ),
        ] {
            changes.push(map::MapChange::new(
                &fixture.scope,
                domain,
                identity,
                Some(value),
            )?);
        }
    }
    occurrence_ids.sort();
    let binding = vector_map::VectorBinding {
        version: 1,
        embedding_key,
        graph_scope: target.graph_head.scope.digest(),
        graph_id: 1,
        occurrence_ids,
    };
    changes.push(map::MapChange::new(
        &fixture.scope,
        map::MapDomain::Vector,
        embedding_key.as_bytes(),
        Some(binding.canonical_bytes(&target)?),
    )?);
    changes.sort_by_key(map::MapChange::key);
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, fixture.head.artifact_root)?;
    for change in &changes {
        plan.put(change.key(), change.value())?;
    }
    let mut postings = postings::PostingChanges::default();
    for input in [&first, &second] {
        postings.update(
            &mut plan,
            &fixture.scope,
            &fixture.store,
            None,
            Some(&input.artifact),
        )?;
    }
    changes.extend(postings.into_changes());
    changes.sort_by_key(map::MapChange::key);
    target.artifact_root = plan.root()?;
    drop(connection);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_mapped(
        &fixture.head.token()?,
        target,
        &sealed,
        &[first.clone(), second.clone()],
        None,
        &changes,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);

    let base = fixture.store.current_read(&fixture.scope)?.head().clone();
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 4 * 1024 * 1024)?;
    let no_op = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?
    .seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_artifacts(
        &base.token()?,
        target,
        &no_op,
        &[ArtifactChange::Delete(first.artifact.path.clone())],
        None,
        br#"{"engine":"test","version":2}"#,
    )?;
    assert_eq!(pending.map_count, 6);
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.artifact(&first.artifact.path)?.is_none());
    assert_eq!(
        read.vector_for_graph_id(1)?.occurrence_ids,
        vec![
            vector_map::PathOccurrences {
                version: 1,
                path: second.artifact.path.clone(),
                source_hash: second.artifact.content_hash.parse()?,
                count: 1,
            }
            .occurrence_id(&fixture.scope, 0)?
        ]
    );
    assert!(read.path_occurrences(&second.artifact.path)?.is_some());
    Ok(())
}

#[test]
fn lexical_manifest_is_bound_at_bootstrap_and_replayed_with_the_new_head() -> Result<()> {
    let mut fixture = Fixture::empty()?;
    let initial_bytes = serde_json::to_vec(&serde_json::json!({"engine":"test","version":1}))?;
    let initial_digest =
        airk_core::identity::framed_digest(b"current-lexical-manifest/1", &[&initial_bytes]);
    fixture.head.lexical_commit = initial_digest;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.initialize(fixture.head.clone())?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.lexical_manifest().is_err());
    drop(read);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(
        current
            .install_initial_lexical_manifest(&fixture.head.token()?, b"wrong")
            .is_err()
    );
    assert_eq!(
        current
            .install_initial_lexical_manifest(&fixture.head.token()?, &initial_bytes)?
            .token,
        fixture.head.token()?
    );
    assert_eq!(
        current
            .install_initial_lexical_manifest(&fixture.head.token()?, &initial_bytes)?
            .token,
        fixture.head.token()?
    );
    drop(current);
    drop(writer);
    assert_eq!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .lexical_manifest()?,
        initial_bytes
    );
    let (_reservation, sealed) = fixture.sealed()?;
    let next_bytes = serde_json::to_vec(&serde_json::json!({"engine":"test","version":2}))?;
    let mut target = fixture.target(&sealed);
    target.lexical_commit =
        airk_core::identity::framed_digest(b"current-lexical-manifest/1", &[&next_bytes]);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let mut pointer_only = fixture.target(&sealed);
    pointer_only.lexical_sequence = pointer_only.sequence;
    assert!(
        current
            .prepare(&fixture.head.token()?, pointer_only, &sealed, &[], None)
            .is_err()
    );
    assert!(
        current
            .prepare(&fixture.head.token()?, target.clone(), &sealed, &[], None)
            .is_err()
    );
    let pending = current.prepare_lexical(
        &fixture.head.token()?,
        target,
        &sealed,
        &[],
        None,
        &next_bytes,
    )?;
    assert_eq!(pending.lexical_manifest_bytes, next_bytes.len() as u64);
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, committed.token);
    assert_eq!(read.lexical_manifest()?, next_bytes);
    assert!(read.require_token(&fixture.head.token()?).is_err());
    Ok(())
}

#[test]
fn corrupt_staged_lexical_manifest_never_advances_the_current_head() -> Result<()> {
    let mut fixture = Fixture::empty()?;
    let initial_bytes = serde_json::to_vec(&serde_json::json!({"engine":"test","version":1}))?;
    fixture.head.lexical_commit =
        airk_core::identity::framed_digest(b"current-lexical-manifest/1", &[&initial_bytes]);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.initialize(fixture.head.clone())?;
    current.install_initial_lexical_manifest(&fixture.head.token()?, &initial_bytes)?;
    drop(current);
    drop(writer);
    let (_reservation, sealed) = fixture.sealed()?;
    let next_bytes = serde_json::to_vec(&serde_json::json!({"engine":"test","version":2}))?;
    let mut target = fixture.target(&sealed);
    target.lexical_commit =
        airk_core::identity::framed_digest(b"current-lexical-manifest/1", &[&next_bytes]);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_lexical(
        &fixture.head.token()?,
        target,
        &sealed,
        &[],
        None,
        &next_bytes,
    )?;
    let tamper = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    tamper.execute(
        "UPDATE current_pending_lexical SET content=zeroblob(length(content))",
        [],
    )?;
    drop(tamper);
    assert!(current.mark_applying(&pending).is_err());
    assert_eq!(current.phase()?, TransactionPhase::Prepared);
    drop(current);
    drop(writer);
    assert_eq!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .lexical_manifest()?,
        initial_bytes
    );
    let repair = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    repair.execute(
        "UPDATE current_pending_lexical SET content=?1",
        [&next_bytes],
    )?;
    drop(repair);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    let tamper = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    tamper.execute(
        "UPDATE current_pending_lexical SET content=zeroblob(length(content))",
        [],
    )?;
    drop(tamper);
    assert!(
        current
            .commit(&pending, pending.target.lexical_commit)
            .is_err()
    );
    assert_eq!(current.phase()?, TransactionPhase::Applying);
    assert_eq!(current.head()?, fixture.head);
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    Ok(())
}

#[test]
fn changed_artifact_and_lexical_manifest_share_one_replayed_head() -> Result<()> {
    let mut fixture = Fixture::empty()?;
    let initial = serde_json::to_vec(&serde_json::json!({"engine":"test","sequence":0}))?;
    fixture.head.lexical_commit =
        airk_core::identity::framed_digest(b"current-lexical-manifest/1", &[&initial]);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.initialize(fixture.head.clone())?;
    current.install_initial_lexical_manifest(&fixture.head.token()?, &initial)?;
    drop(current);
    drop(writer);
    let input = retained_input(&fixture, "Changed artifact source.")?;
    let (_reservation, sealed) = fixture.sealed()?;
    let next = serde_json::to_vec(&serde_json::json!({"engine":"test","sequence":1}))?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(
        current
            .prepare_artifacts(
                &fixture.head.token()?,
                fixture.target(&sealed),
                &sealed,
                &[
                    ArtifactChange::Upsert(Box::new(input.clone())),
                    ArtifactChange::Upsert(Box::new(input.clone())),
                ],
                None,
                &next,
            )
            .is_err()
    );
    assert!(
        current
            .prepare_artifacts(
                &fixture.head.token()?,
                fixture.target(&sealed),
                &sealed,
                &[ArtifactChange::Delete("../outside".into())],
                None,
                &next,
            )
            .is_err()
    );
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    let pending = current.prepare_artifacts(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[ArtifactChange::Upsert(Box::new(input.clone()))],
        None,
        &next,
    )?;
    assert_eq!(pending.map_count, 3);
    assert_eq!(pending.inputs_count, 1);
    assert_eq!(pending.lexical_manifest_bytes, next.len() as u64);
    drop(current);
    drop(writer);
    let before = fixture.store.current_read(&fixture.scope)?;
    assert!(before.artifact(&input.artifact.path)?.is_none());
    assert_eq!(before.lexical_manifest()?, initial);
    drop(before);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let after = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(after.head(), &pending.target);
    assert_eq!(after.lexical_manifest()?, next);
    assert_eq!(
        after.artifact(&input.artifact.path)?.unwrap().content_hash,
        input.artifact.content_hash
    );
    drop(after);

    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 1024 * 1024)?;
    let overlay = graph::GraphOverlay::new(
        &reservation,
        fixture.store.current_read(&fixture.scope)?,
        graph::GraphOverlayLimits {
            maximum_bytes: 1024 * 1024,
            ..Default::default()
        },
    )?;
    let sealed = overlay.seal()?;
    let mut target = pending.target.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let deleted = serde_json::to_vec(&serde_json::json!({"engine":"test","sequence":2}))?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let deletion = current.prepare_artifacts(
        &pending.target.token()?,
        target,
        &sealed,
        &[ArtifactChange::Delete(input.artifact.path.clone())],
        None,
        &deleted,
    )?;
    current.mark_applying(&deletion)?;
    current.commit(&deletion, deletion.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let after_delete = fixture.store.current_read(&fixture.scope)?;
    assert!(after_delete.artifact(&input.artifact.path)?.is_none());
    assert_eq!(after_delete.lexical_manifest()?, deleted);
    assert!(
        after_delete
            .require_token(&pending.target.token()?)
            .is_err()
    );
    Ok(())
}

#[test]
fn changed_pending_map_bytes_block_applying_and_commit_without_advancing_head() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input(&fixture, "Captured citation body.")?;
    let (_reservation, sealed) = fixture.sealed()?;
    let mut target = fixture.target(&sealed);
    let valid_bytes = input.bytes()?;
    let change = map::MapChange::new(
        &fixture.scope,
        map::MapDomain::Artifact,
        input.artifact.path.as_bytes(),
        Some(valid_bytes.clone()),
    )?;
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, fixture.head.artifact_root)?;
    plan.put(change.key(), change.value())?;
    target.artifact_root = plan.root()?;
    drop(connection);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare_mapped(
        &fixture.head.token()?,
        target,
        &sealed,
        &[input],
        None,
        &[change],
    )?;
    let tamper = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    tamper.execute(
        "UPDATE current_pending_map SET value=zeroblob(length(value))",
        [],
    )?;
    drop(tamper);
    assert!(current.mark_applying(&pending).is_err());
    assert_eq!(current.phase()?, TransactionPhase::Prepared);
    assert_eq!(current.head()?, fixture.head);
    drop(current);
    drop(writer);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.head(),
        &fixture.head
    );
    let repair = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    repair.execute("UPDATE current_pending_map SET value=?1", [&valid_bytes])?;
    drop(repair);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    let tamper = rusqlite::Connection::open(fixture.store.directory().join("current.sqlite3"))?;
    tamper.execute(
        "UPDATE current_pending_map SET value=zeroblob(length(value))",
        [],
    )?;
    drop(tamper);
    assert!(
        current
            .commit(&pending, pending.target.lexical_commit)
            .is_err()
    );
    assert_eq!(current.phase()?, TransactionPhase::Applying);
    assert_eq!(current.head()?, fixture.head);
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    Ok(())
}

#[test]
fn catalog_only_artifact_updates_are_atomic_authenticated_and_revision_bound() -> Result<()> {
    let fixture = Fixture::new()?;
    let source = std::path::Path::new(&fixture.scope.root).join("evidence.txt");
    fs::write(&source, "Original evidence.")?;
    let original = retained_input(&fixture, "Original evidence.")?;
    let initial = fixture.store.current_read(&fixture.scope)?.token()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(
        current
            .commit_artifact(&initial, "other.txt", Some(&original))
            .is_err()
    );
    assert_eq!(current.head()?, fixture.head);
    let first = current.commit_artifact(&initial, "evidence.txt", Some(&original))?;
    assert!(first.maintenance_error.is_none());
    assert_ne!(first.token, initial);
    assert_eq!(
        current
            .commit_artifact(&first.token, "evidence.txt", Some(&original))?
            .token,
        first.token
    );
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.require_token(&initial).is_err());
    assert_eq!(
        serde_json::to_value(read.artifact("evidence.txt")?)?,
        serde_json::to_value(Some(&original.artifact))?
    );
    assert!(read.artifact("missing.txt")?.is_none());
    assert_eq!(read.map_entries(map::MapVisitLimits::default())?.len(), 3);
    drop(read);
    fs::write(&source, "Later live edit")?;
    assert_eq!(
        fixture.store.read_retained_source(&original.artifact)?,
        b"Original evidence."
    );
    let changed = retained_input(&fixture, "Later live edit")?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(
        current
            .commit_artifact(&initial, "evidence.txt", Some(&changed))
            .is_err()
    );
    let second = current.commit_artifact(&first.token, "evidence.txt", Some(&changed))?;
    assert!(second.maintenance_error.is_none());
    drop(current);
    drop(writer);
    assert_eq!(
        serde_json::to_value(
            fixture
                .store
                .current_read(&fixture.scope)?
                .artifact("evidence.txt")?
        )?,
        serde_json::to_value(Some(&changed.artifact))?
    );
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let third = current.commit_artifact(&second.token, "evidence.txt", None)?;
    assert!(third.maintenance_error.is_none());
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.artifact("evidence.txt")?.is_none());
    assert!(read.map_entries(map::MapVisitLimits::default())?.is_empty());
    Ok(())
}

#[test]
fn catalog_only_corruption_cannot_forge_a_hit_or_an_absence() -> Result<()> {
    for mutation in [
        "UPDATE current_map_buckets SET content=zeroblob(length(content)) WHERE prefix=?1",
        "DELETE FROM current_map_buckets WHERE prefix=?1",
    ] {
        let fixture = Fixture::new()?;
        let input = retained_input(&fixture, "Authenticated body.")?;
        let initial = fixture.store.current_read(&fixture.scope)?.token()?;
        let writer = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        current.commit_artifact(&initial, "evidence.txt", Some(&input))?;
        drop(current);
        drop(writer);
        let key = map::key(&fixture.scope, map::MapDomain::Artifact, b"evidence.txt")?;
        {
            let connection = database::open_writer(
                &fixture.store.directory().join("current.sqlite3"),
                CAPACITY,
                false,
            )?;
            connection.execute(
                mutation,
                [i64::from(u16::from_be_bytes(
                    key.as_bytes()[..2].try_into()?,
                ))],
            )?;
        }
        assert!(
            fixture
                .store
                .current_read(&fixture.scope)?
                .artifact("evidence.txt")
                .is_err(),
            "{mutation}"
        );
    }
    Ok(())
}

#[test]
fn current_scope_and_owned_header_reject_foreign_state_before_recovery() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut foreign = fixture.scope.clone();
    foreign.root.push_str("/other");
    assert!(fixture.store.current_read(&foreign).is_err());
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &foreign, CAPACITY)
            .is_err()
    );
    let before = fs::read(fixture.store.directory().join("current.sqlite3"))?;
    fs::write(
        fixture.store.directory().join("current.sqlite3-journal"),
        b"not a journal",
    )?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &foreign, CAPACITY)
            .is_err()
    );
    assert_eq!(
        before,
        fs::read(fixture.store.directory().join("current.sqlite3"))?
    );
    Ok(())
}

#[test]
fn journal_bound_sector_adapter_and_statement_contract_are_enforced() -> Result<()> {
    assert_eq!(journal_bound(0, 512)?, 2048);
    assert_eq!(journal_bound(10, 4096)?, 10 * (4096 + 8 + 8192) + 16384);
    assert!(journal_bound(u64::MAX, 65536).is_err());
    assert!(journal_bound(2, 513).is_err());
    let fixture = Fixture::new()?;
    let mut connection = database::open_writer(
        &fixture.store.directory().join("current.sqlite3"),
        CAPACITY,
        false,
    )?;
    let sector = database::effective_sector(&mut connection)?;
    assert!(sector.is_power_of_two() && (32..=65536).contains(&sector));
    let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    let sync: u32 = connection.pragma_query_value(None, "synchronous", |r| r.get(0))?;
    assert_eq!(mode, "delete");
    assert_eq!(sync, 3);
    database::restrict(&connection)?;
    for sql in [
        "SAVEPOINT forbidden",
        "ATTACH ':memory:' AS other",
        "PRAGMA journal_mode=OFF",
        "CREATE TEMP TABLE forbidden(a)",
        "CREATE TRIGGER forbidden AFTER UPDATE ON current_state BEGIN SELECT 1; END",
    ] {
        assert!(connection.execute_batch(sql).is_err(), "{sql}");
    }
    for sql in [
        "EXPLAIN UPDATE OR ROLLBACK current_state SET phase='applying' WHERE id=1",
        "EXPLAIN INSERT OR ROLLBACK INTO current_pending_inputs(path,metadata) VALUES('test',x'00')",
    ] {
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let opcode: String = row.get(1)?;
            let p1: i64 = row.get(2)?;
            let p2: i64 = row.get(3)?;
            assert!(
                !((opcode == "Halt" || opcode == "HaltIfNull") && p1 != 0 && p2 == 2),
                "statement contains ABORT conflict path"
            );
        }
    }
    connection.execute_batch(
        "BEGIN; UPDATE OR ROLLBACK current_state SET phase='prepared' WHERE id=1;",
    )?;
    assert!(
        connection
            .execute(
                "INSERT OR ROLLBACK INTO current_state(id,phase,head) VALUES(1,'ready',x'00')",
                []
            )
            .is_err()
    );
    assert!(connection.is_autocommit());
    assert_eq!(
        connection.query_row("SELECT phase FROM current_state", [], |r| r
            .get::<_, String>(0))?,
        "ready"
    );
    Ok(())
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn child(fixture: &Fixture, mode: &str) -> Result<ChildGuard> {
    let ready = fixture.directory.path().join("child-ready");
    if ready.exists() {
        fs::remove_file(&ready)?;
    }
    let mut child = ChildGuard(
        Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("current::tests::current_child_process")
            .arg("--nocapture")
            .env("AIRK_CURRENT_CHILD_DATA", fixture.quota.root())
            .env(
                "AIRK_CURRENT_CHILD_SCOPE",
                serde_json::to_string(&fixture.scope)?,
            )
            .env(
                "AIRK_CURRENT_CHILD_HEAD",
                serde_json::to_string(&fixture.head)?,
            )
            .env("AIRK_CURRENT_CHILD_READY", &ready)
            .env("AIRK_CURRENT_CHILD_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let started = Instant::now();
    while !ready.exists() {
        ensure!(
            child.0.try_wait()?.is_none(),
            "current test child exited before readiness"
        );
        ensure!(
            started.elapsed() < Duration::from_secs(20),
            "current test child readiness timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(child)
}

#[test]
fn current_child_process() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_CURRENT_CHILD_DATA") else {
        return Ok(());
    };
    let quota = Quota::open(root)?;
    let scope: CurrentScope = serde_json::from_str(&std::env::var("AIRK_CURRENT_CHILD_SCOPE")?)?;
    let store = FileStore::new(quota.root().join("collections").join(&scope.collection));
    let ready = std::env::var_os("AIRK_CURRENT_CHILD_READY").context("ready path missing")?;
    let mode = std::env::var("AIRK_CURRENT_CHILD_MODE")?;
    if mode == "read" {
        let _read = store.current_read(&scope)?;
        fs::write(ready, b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let writer = store.lock()?;
    let mut current = store.current_write(&writer, &quota, &scope, CAPACITY)?;
    if mode.starts_with("bootstrap-") {
        let head: CurrentHead = serde_json::from_str(&std::env::var("AIRK_CURRENT_CHILD_HEAD")?)?;
        return current
            .initialize_observed(head, &mut |stage| {
                let selected = matches!(
                    (mode.as_str(), stage),
                    (
                        "bootstrap-before-open",
                        journal::InitializationStage::BeforeDatabaseOpen
                    ) | (
                        "bootstrap-before-commit",
                        journal::InitializationStage::BeforeSchemaCommit
                    ) | (
                        "bootstrap-after-commit",
                        journal::InitializationStage::ReadyCommitted
                    )
                );
                if selected {
                    fs::write(&ready, b"ready")?;
                    loop {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(())
            })
            .map(|_| ());
    }
    if mode.starts_with("grow-") {
        return current
            .grow_capacity_observed(CAPACITY * 2, &mut |stage| {
                let selected = matches!(
                    (mode.as_str(), stage),
                    ("grow-before-owner", journal::CapacityStage::BeforeOwner)
                        | ("grow-after-owner", journal::CapacityStage::AfterOwner)
                );
                if selected {
                    fs::write(&ready, b"ready")?;
                    loop {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(())
            })
            .map(|_| ());
    }
    let pending = current.pending()?.context("child pending missing")?;
    current.mark_applying(&pending)?;
    if mode == "hot-apply" {
        let mut connection =
            database::open_writer(&store.directory().join("current.sqlite3"), CAPACITY, false)?;
        let transaction = connection.transaction()?;
        graph::apply(&transaction, pending.id, &pending.graph)?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='ready',head=?1 WHERE id=1",
            [pending.target.canonical_bytes()?],
        )?;
        // Force journal synchronization and dirty database pages to disk while
        // the transaction remains uncommitted, then let the parent kill us.
        transaction.cache_flush()?;
        fs::write(&ready, b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fs::write(ready, b"ready")?;
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn process_death_releases_gate_but_never_bypasses_applying_marker() -> Result<()> {
    let fixture = Fixture::new()?;
    let reader_child = child(&fixture, "read")?;
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(writer);
    drop(reader_child);
    let pending = fixture.prepare(&[])?;
    let child = child(&fixture, "applying")?;
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    drop(child);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    fixture.quota.status()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.pending()?, Some(pending.clone()));
    current.commit(&pending, pending.target.lexical_commit)?;
    assert_eq!(current.release_orphan_grants()?, 0);
    drop(current);
    drop(writer);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.read(1)?.bytes,
        vec![7; 16]
    );
    Ok(())
}

#[test]
fn initialization_replays_exact_captured_bootstrap_after_process_death() -> Result<()> {
    for mode in [
        "bootstrap-before-open",
        "bootstrap-before-commit",
        "bootstrap-after-commit",
    ] {
        let fixture = Fixture::empty()?;
        let child = child(&fixture, mode)?;
        drop(child);
        fixture.quota.status()?;
        let writer = fixture.store.lock()?;
        assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        assert_eq!(current.head()?, fixture.head);
        assert_eq!(current.initialize(fixture.head.clone())?, fixture.head);
        assert!(fixture.quota.current_grants(&writer)?.is_empty());
        drop(current);
        drop(writer);
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.head(),
            &fixture.head
        );
    }
    Ok(())
}

#[test]
fn oversized_recovery_journal_is_rejected_before_database_open() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let _grant = fixture.quota.reserve_current(
        &writer,
        crate::quota::CurrentGrantScope {
            collection: fixture.scope.collection.clone(),
            incarnation: fixture.scope.incarnation,
            transaction: Sha256Digest::from_bytes([99; 32]),
        },
        1024,
    )?;
    let before = fs::read(fixture.store.directory().join("current.sqlite3"))?;
    fs::write(
        fixture.store.directory().join("current.sqlite3-journal"),
        vec![0u8; 1025],
    )?;
    let error = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
        .err()
        .context("expected recovery capacity failure")?;
    assert!(
        error
            .to_string()
            .contains("journal exceeds durable allowance")
    );
    assert_eq!(
        before,
        fs::read(fixture.store.directory().join("current.sqlite3"))?
    );
    Ok(())
}

#[test]
fn hot_final_apply_rolls_back_then_replays_exact_afterimages() -> Result<()> {
    let fixture = Fixture::new()?;
    let pending = fixture.prepare(&[])?;
    let child = child(&fixture, "hot-apply")?;
    drop(child);
    let journal = fs::read(fixture.store.directory().join("current.sqlite3-journal"))?;
    assert!(journal.len() > 28);
    assert_eq!(
        &journal[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)
            .is_err()
    );
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.head()?, fixture.head);
    assert_eq!(current.phase()?, TransactionPhase::Applying);
    assert_eq!(current.pending()?, Some(pending.clone()));
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.counts()?.allocated_count, 2);
    assert_eq!(read.read(1)?.bytes, vec![7; 16]);
    Ok(())
}

#[test]
fn admitted_capacity_growth_preserves_head_and_reopens_with_captured_limit() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = fixture.store.current_read(&fixture.scope)?.token()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let changed = current.grow_capacity(CAPACITY * 2)?;
    assert_eq!(changed.capacity_bytes, CAPACITY * 2);
    assert!(changed.maintenance_error.is_none());
    assert_eq!(current.database_capacity_bytes(), CAPACITY * 2);
    assert_eq!(current.head()?, fixture.head);
    drop(current);
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    let current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)?;
    assert_eq!(current.release_orphan_grants()?, 0);
    drop(current);
    drop(writer);
    assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)?;
    let pending = current.prepare(&before, fixture.target(&sealed), &sealed, &[], None)?;
    assert_eq!(pending.database_capacity_bytes, CAPACITY * 2);
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    drop(current);
    drop(writer);
    assert!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .require_token(&before)
            .is_err()
    );
    Ok(())
}

#[test]
fn capacity_growth_rejects_quota_pressure_and_prepared_transactions() -> Result<()> {
    let fixture = Fixture::new()?;
    let status = fixture.quota.status()?;
    fixture.quota.configure(crate::quota::QuotaPolicy {
        root_bytes: status.used_bytes + CAPACITY + 128 * 1024,
        default_collection_bytes: status.used_bytes + CAPACITY + 128 * 1024,
        collections: Default::default(),
    })?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .grow_capacity(CAPACITY * 2)
        .err()
        .context("growth should fail under quota")?;
    assert!(error.is::<crate::quota::QuotaPressure>());
    assert_eq!(current.database_capacity_bytes(), CAPACITY);
    assert_eq!(
        database::check_owner(fixture.store.directory(), &fixture.scope)?,
        CAPACITY
    );
    drop(current);
    drop(writer);
    assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.head(),
        &fixture.head
    );

    let fixture = Fixture::new()?;
    let pending = fixture.prepare(&[])?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(current.grow_capacity(CAPACITY * 2).is_err());
    current.abort_prepared(&pending)?;
    Ok(())
}

#[test]
fn killed_capacity_growth_keeps_old_or_new_bound_and_releases_only_orphans() -> Result<()> {
    for (mode, expected) in [
        ("grow-before-owner", CAPACITY),
        ("grow-after-owner", CAPACITY * 2),
    ] {
        let fixture = Fixture::new()?;
        let before = fixture.store.current_read(&fixture.scope)?.token()?;
        let child = child(&fixture, mode)?;
        assert!(fixture.quota.status()?.reserved_bytes > 0);
        drop(child);
        assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
        assert_eq!(
            database::check_owner(fixture.store.directory(), &fixture.scope)?,
            expected
        );
        let writer = fixture.store.lock()?;
        let current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, expected)?;
        assert_eq!(current.release_orphan_grants()?, 1);
        drop(current);
        drop(writer);
        assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    }
    Ok(())
}
