// SPDX-License-Identifier: Apache-2.0
//! End-to-end tests of `CurrentRead` over a plain index built by the mapping
//! contract (index CONTRACT.md, "Mapping of the current index"). The fixture
//! writes rows with `IndexWriter` the way the contract says the writer will;
//! expectations come from the fixture's own inputs, not from the reader.
use super::{
    CurrentRead, CurrentScope, journal::CapturedInput, lexical, map::MapDomain, postings::Lookup,
};
use crate::FileStore;
use crate::index::{
    Batch, ChunkEntry, EncodedPayload, FileChange, FileEntry, Handle, HandleKind,
    IncompatibleIndex, IndexWriter, ResourceChange, Selector, SelectorKind,
};
use airk_core::{
    Artifact, ArtifactRef, Citation, DocumentLocation, FactQuery, RelationshipQuery, SearchQuery,
    changes::{FileStamps, FileState},
    discovery::{DiscoveryOptions, FactDiscoveryOptions},
    extraction::{
        ExtractionPayload, ExtractionProfile, LocalChunk, LocalCodeEvidence, LocalFact, LocalSpan,
        LocalSyntaxId, LocalSyntaxIdentity,
    },
    identity::{ArtifactId, Incarnation, Sha256Digest, framed_digest},
    index_catalog::PathScope,
    publication::InventoryFile,
    scoped_reads::{CompiledPathAtom, CompiledReadScope},
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const COLLECTION: &str = "plain";
const ROOT: &str = "fixture-root";

/// One fixture source file and how it was extracted.
struct Source {
    path: &'static str,
    /// The original bytes (equal to `text` for `utf8`).
    source: Vec<u8>,
    /// The extracted text; one chunk per line.
    text: &'static str,
    language: &'static str,
    method: &'static str,
    facts: Vec<LocalFact>,
}

fn utf8(path: &'static str, text: &'static str, facts: Vec<LocalFact>) -> Source {
    Source {
        path,
        source: text.as_bytes().to_vec(),
        text,
        language: "rust",
        method: "utf8",
        facts,
    }
}

fn document() -> Source {
    Source {
        path: "docs/guide.pdf",
        source: b"%PDF-1.7 binary original bytes".to_vec(),
        text: "Guide heading\nalpha notes here",
        language: "pdf",
        method: "pdf-text",
        facts: vec![fact(10, "Guide", "heading", 0..5, 1, None)],
    }
}

fn fact(
    id: u32,
    name: &str,
    kind: &str,
    span: std::ops::Range<usize>,
    line: usize,
    code: Option<LocalCodeEvidence>,
) -> LocalFact {
    LocalFact {
        id,
        owner: None,
        kind: kind.into(),
        name: name.into(),
        span: LocalSpan {
            start: span.start,
            end: span.end,
        },
        syntax_kind: kind.into(),
        start_line: line,
        end_line: line,
        location: DocumentLocation::Lines {
            start: line,
            end: line,
        },
        document: None,
        code,
        region: None,
        attributes: Default::default(),
    }
}

fn code(syntax: u32, qualified: &str, target: Option<&str>) -> LocalCodeEvidence {
    LocalCodeEvidence {
        fingerprints: None,
        syntax_id: LocalSyntaxId(syntax),
        owner_id: None,
        qualified_name: qualified.into(),
        scope: vec![],
        signature: None,
        span: None,
        signature_span: None,
        parameters: None,
        return_type: None,
        modifiers: vec![],
        documentation: None,
        target: target.map(Into::into),
        basis: airk_core::EvidenceBasis::Syntax,
        producer: "test-native".into(),
        producer_version: "1".into(),
        coverage: "written syntax".into(),
        details_truncated: false,
    }
}

/// The default fixture: a definition, a caller, a long and a short text file
/// and one document whose source is not kept.
fn sources() -> Vec<Source> {
    vec![
        utf8(
            "a/long.rs",
            "alpha one two three four five six seven eight nine ten eleven twelve\nbeta only\n",
            vec![],
        ),
        document(),
        utf8(
            "src/def.rs",
            "struct Widget;\nalpha gamma\n",
            vec![fact(
                1,
                "Widget",
                "definition",
                7..13,
                1,
                Some(code(1, "crate::Widget", None)),
            )],
        ),
        utf8(
            "src/use.rs",
            "fn main() { Widget }\nalpha delta\n",
            vec![fact(
                2,
                "Widget",
                "call",
                12..18,
                1,
                Some(code(2, "Widget", Some("Widget"))),
            )],
        ),
        utf8("z/short.rs", "alpha\n", vec![]),
    ]
}

fn profile(language: &str) -> ExtractionProfile {
    ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "plain-test/1".into(),
        language: language.into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    }
}

fn payload(source: &Source) -> ExtractionPayload {
    let mut chunks = Vec::new();
    let mut start = 0;
    for (index, line) in source.text.split_inclusive('\n').enumerate() {
        chunks.push(LocalChunk {
            span: LocalSpan {
                start,
                end: start + line.len(),
            },
            start_line: index + 1,
            end_line: index + 1,
            location: DocumentLocation::Lines {
                start: index + 1,
                end: index + 1,
            },
            region: None,
        });
        start += line.len();
    }
    ExtractionPayload {
        semantic_projection: Default::default(),
        continuity_coverage: Default::default(),
        contract_version: 1,
        extraction_method: source.method.into(),
        parse_status: "plain".into(),
        provenance: vec![],
        text: source.text.into(),
        syntax_identities: source
            .facts
            .iter()
            .filter_map(|fact| {
                fact.code.as_ref().map(|code| LocalSyntaxIdentity {
                    id: code.syntax_id,
                    span: fact.span.clone(),
                    syntax_kind: fact.syntax_kind.clone(),
                    region: None,
                })
            })
            .collect(),
        facts: source.facts.clone(),
        chunks,
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    }
}

fn artifact_id(path: &str) -> ArtifactId {
    let mut id = [0; 16];
    id.copy_from_slice(&Sha256::digest(path.as_bytes())[..16]);
    ArtifactId::from_bytes(id)
}

/// Everything the fixture wrote for one file, for expectations.
struct Written {
    full: Artifact,
    compact: Artifact,
    file: InventoryFile,
}

/// The selectors the contract names: fact name, fact kind, edge source and
/// edge target, with the terminal-call rule (postings version 1).
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

fn entry(source: &Source, sequence: u64) -> Result<(FileEntry, Written)> {
    let payload = payload(source);
    let profile = profile(source.language);
    let content_hash = Sha256Digest::from_bytes(Sha256::digest(&source.source).into());
    let full = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: source.path,
            source_hash: content_hash,
            source_bytes: source.source.len(),
        },
    )?;
    let payload_bytes = serde_json::to_vec(&payload)?;
    let encoded = EncodedPayload::encode(&payload_bytes)?;
    let mut compact = full.clone();
    compact.retained = Some(ArtifactRef {
        legacy_origin: None,
        source_encoding: None,
        payload_encoding: None,
        object: encoded.digest.to_string(),
        object_bytes: payload_bytes.len(),
        // Today's `CapturedInput::bytes` requires it; the plain reader ignores it.
        raw_source: true,
        chunks: full.chunks.len(),
        extracted_bytes: full.extracted_bytes(),
        facts: full.facts.len(),
        profile: Some(profile.clone()),
    });
    compact.chunks.clear();
    compact.facts.clear();
    let file = InventoryFile {
        continuity: Vec::new(),
        continuity_coverage: Default::default(),
        state: FileState {
            artifact_id: artifact_id(source.path),
            stamps: FileStamps {
                first_generation: 1,
                path_generation: sequence,
                content_generation: sequence,
                evidence_generation: sequence,
            },
            source_hash: content_hash,
            profile_id: profile.identity()?,
            payload_digest: encoded.digest,
            evidence_digest: Sha256Digest::from_bytes([7; 32]),
            root: ROOT.into(),
            path: source.path.into(),
        },
        source_bytes: source.source.len(),
        payload_bytes: payload_bytes.len(),
        extracted_bytes: full.extracted_bytes(),
        chunks: full.chunks.len(),
        indexed_lines: full.chunks.len(),
        facts: full.facts.len(),
        raw_source: source.method == "utf8",
        language: source.language.into(),
        extraction_method: source.method.into(),
        parse_status: "plain".into(),
    };
    let chunks = full
        .chunks
        .iter()
        .enumerate()
        .map(|(ordinal, chunk)| -> Result<ChunkEntry> {
            Ok(ChunkEntry {
                stable_id: lexical::stable_id(source.path, ordinal as u64)?,
                citation: serde_json::to_vec(&Citation {
                    path: source.path.into(),
                    content_hash: full.content_hash.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                })?,
                terms: lexical::chunk_terms(source.path, &chunk.text),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let entry = FileEntry {
        path: source.path.into(),
        source_hash: content_hash,
        language: full.language.clone(),
        record: CapturedInput::new(compact.clone()).bytes()?,
        metadata: serde_json::to_vec(&file)?,
        stat: None,
        payload: encoded,
        chunks,
        selectors: selectors(&full),
        handles: vec![Handle {
            kind: HandleKind::Artifact,
            value: file.state.artifact_id.as_bytes().to_vec(),
        }],
        vector_pending: false,
    };
    Ok((
        entry,
        Written {
            full,
            compact,
            file,
        },
    ))
}

fn state(roots: BTreeMap<String, String>) -> super::collection::CollectionState {
    super::collection::CollectionState {
        version: 1,
        configuration: super::collection::CollectionConfiguration {
            retained_profile: None,
            source_policy_digest: None,
            roots,
            extractor_version: "plain-test/1".into(),
            extraction_policy_digest: Sha256Digest::from_bytes([4; 32]),
            index_detail: airk_core::extraction::IndexDetail::Balanced,
            vector_selection: Default::default(),
            model: None,
        },
        counts: Default::default(),
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: FileStore,
    scope: CurrentScope,
    writer: IndexWriter,
    written: BTreeMap<String, Written>,
}

impl Fixture {
    fn new() -> Result<Self> {
        Self::with(sources(), BTreeMap::new())
    }
    fn with(sources: Vec<Source>, roots: BTreeMap<String, String>) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let store = FileStore::new(directory.path().join("collections-1.2").join(COLLECTION));
        drop(store.lock()?);
        let scope = CurrentScope {
            root: ROOT.into(),
            collection: COLLECTION.into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let mut writer = IndexWriter::create(store.directory())?;
        let mut sources = sources;
        sources.sort_by_key(|source| source.path);
        let mut batch = Batch::default();
        let mut written = BTreeMap::new();
        for source in &sources {
            let (entry, files) = entry(source, 1)?;
            batch.files.push(FileChange::Upsert(Box::new(entry)));
            written.insert(source.path.to_owned(), files);
        }
        batch.resources.push(ResourceChange {
            key: "semantic-policy".into(),
            body: Some(br#"{"policy":"fixture"}"#.to_vec()),
        });
        batch.resources.push(ResourceChange {
            key: "source-measurement/src/def.rs".into(),
            body: Some(b"{\"measured\":1}".to_vec()),
        });
        let commit = writer.apply(batch)?;
        assert_eq!(commit.sequence, 1);
        // The engine leaves caller meta keys to the caller (contract "meta keys").
        let connection = rusqlite::Connection::open(store.directory().join("index.sqlite3"))?;
        for (key, value) in [
            ("scope", serde_json::to_vec(&scope)?),
            ("collection_state", state(roots).bytes()?),
            (
                "lexical_commit",
                super::plain::lexical_commit(&scope, 1).as_bytes().to_vec(),
            ),
        ] {
            connection.execute(
                "INSERT INTO meta(key,value) VALUES(?1,?2)",
                rusqlite::params![key, value],
            )?;
        }
        Ok(Self {
            _directory: directory,
            store,
            scope,
            writer,
            written,
        })
    }
    fn read(&self) -> Result<Arc<CurrentRead>> {
        self.store.current_read(&self.scope)
    }
    fn compact(&self, path: &str) -> &Artifact {
        &self.written[path].compact
    }
}

fn query(text: &str, limit: usize) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit,
        max_bytes: 4096,
    }
}

#[test]
fn current_missing_fts_restore_publishes_once_and_preserves_pinned_readers() -> Result<()> {
    use super::plain_writer::{CapturedPayload, PlainChange, PlainPublication, PlainUpsert};
    use crate::index::{FileVectors, FtsRestore, FtsWitness, UnitEntry, VectorBatch, VectorSetup};
    use std::time::{Duration, Instant};

    const TARGET: &str = "src/a.rs";
    const CONTROL: &str = "src/b.rs";
    let directory = tempfile::tempdir()?;
    let store = FileStore::new(directory.path().join("collections-1.2").join(COLLECTION));
    let scope = CurrentScope {
        root: ROOT.into(),
        collection: COLLECTION.into(),
        incarnation: Incarnation::from_bytes([3; 16]),
    };
    let lock = store.lock()?;
    let mut configuration = state(BTreeMap::new()).configuration;
    // This fixture's retained profile has no legacy detail option.
    configuration.index_detail = airk_core::extraction::IndexDetail::Detailed;
    store.plain_create(&lock, &scope, &configuration)?;
    let mut writer = store.plain_writer(&lock)?;
    let sources = [
        utf8(TARGET, "firstline\nrepairneedle\n", vec![]),
        utf8(CONTROL, "controlneedle\n", vec![]),
    ];
    let mut changes = Vec::new();
    let mut expected = Vec::new();
    for source in &sources {
        let (_, written) = entry(source, 2)?;
        let (compact, captured) =
            CapturedPayload::capture(&written.full, &payload(source), &profile(source.language))?;
        let file = written.file.clone();
        expected.push(written);
        changes.push(PlainChange::Upsert(Box::new(PlainUpsert {
            input: CapturedInput::new(compact),
            file,
            captured,
            vector_pending: source.path == TARGET,
        })));
    }
    let normal = writer.publish(PlainPublication {
        changes,
        resources: vec![],
        configuration: None,
        recount: false,
    })?;
    assert_eq!((normal.files_changed, normal.chunks_added), (2, 3));
    let stale = store.capture_current_fts_permit(&lock)?;
    let vector = VectorSetup {
        dimensions: 2,
        pipeline: Sha256Digest::from_bytes([11; 32]),
    };
    let key = Sha256Digest::from_bytes([12; 32]);
    writer.configure_vectors(Some(&vector))?;
    writer.commit_vectors(
        VectorBatch {
            setup: vector,
            files: vec![FileVectors {
                path: TARGET.into(),
                source_hash: expected[0].file.state.source_hash,
                units: vec![UnitEntry {
                    ordinal: 0,
                    embedding_key: key,
                    members: b"fixture members".to_vec(),
                }],
                failed: false,
            }],
            vectors: vec![(key, vec![1.0, 0.0])],
        },
        vec![],
    )?;
    let healthy = store.current_read(&scope)?;
    let previous = healthy.token()?;
    let target_query = query("repairneedle", 5);
    let positive_query = query("controlneedle", 5);
    let no_match_query = query("unmatchedneedle", 5);
    let target = healthy.lexical_search(&target_query)?;
    assert_eq!(hit_paths(&target), [(TARGET.to_owned(), 1)]);
    let positive = healthy.lexical_search(&positive_query)?;
    assert_eq!(hit_paths(&positive), [(CONTROL.to_owned(), 0)]);
    assert!(healthy.lexical_search(&no_match_query)?.values.is_empty());
    let expected_citation = Citation {
        path: TARGET.into(),
        content_hash: expected[0].file.state.source_hash.to_string(),
        start_line: 2,
        end_line: 2,
        location: DocumentLocation::Lines { start: 2, end: 2 },
    };
    let target_citation = serde_json::to_vec(&expected_citation)?;
    let positive_citation = serde_json::to_vec(&positive.values[0].citation)?;
    assert_eq!(
        serde_json::to_vec(&target.values[0].citation)?,
        target_citation
    );
    assert_ne!(
        expected[0].file.state.source_hash,
        expected[0].file.state.payload_digest
    );

    let injection =
        rusqlite::Connection::open(store.directory().join(crate::index::DATABASE_FILE))?;
    assert_eq!(
        injection.execute(
            "DELETE FROM fts WHERE rowid=(SELECT id FROM chunks WHERE stable_id=?1)",
            [target.values[0].stable_id.as_bytes().as_slice()],
        )?,
        1
    );
    drop(injection);
    let damaged = store.current_read(&scope)?;
    let pinned = store.plain_read()?;
    assert_eq!(damaged.token()?, previous);
    assert!(damaged.lexical_search(&target_query)?.values.is_empty());
    let permit = store.capture_current_fts_permit(&lock)?;
    let proposal = FtsRestore {
        sequence: previous.sequence,
        source_hash: expected[0].file.state.source_hash,
        payload_digest: expected[0].file.state.payload_digest,
        canonical_terms: "xa xrepairneedle xrs xsrc",
        missing: FtsWitness {
            path: TARGET,
            terms: &["xrepairneedle"],
            expected: Some((1, target.values[0].stable_id, &target_citation)),
        },
        positive: FtsWitness {
            path: CONTROL,
            terms: &["xcontrolneedle"],
            expected: Some((0, positive.values[0].stable_id, &positive_citation)),
        },
        no_match: FtsWitness {
            path: TARGET,
            terms: &["xunmatchedneedle"],
            expected: None,
        },
    };
    let deadline = || Instant::now() + Duration::from_secs(10);
    let stale_proposal = FtsRestore {
        sequence: normal.token.sequence,
        ..proposal.clone()
    };
    let wrong_source = FtsRestore {
        source_hash: Sha256Digest::from_bytes([13; 32]),
        ..proposal.clone()
    };
    let wrong_payload = FtsRestore {
        payload_digest: Sha256Digest::from_bytes([14; 32]),
        ..proposal.clone()
    };
    let wrong_positive = FtsRestore {
        positive: FtsWitness {
            expected: Some((0, positive.values[0].stable_id, b"wrong citation")),
            ..proposal.positive
        },
        ..proposal.clone()
    };
    let wrong_negative = FtsRestore {
        no_match: FtsWitness {
            terms: &["xrepairneedle"],
            ..proposal.no_match
        },
        ..proposal.clone()
    };
    for (permit, proposal, message) in [
        (&stale, &stale_proposal, "publication is stale"),
        (&permit, &wrong_source, "binding differs"),
        (&permit, &wrong_payload, "binding differs"),
        (&permit, &wrong_positive, "witness evidence differs"),
        (&permit, &wrong_negative, "witness match differs"),
    ] {
        let error = writer
            .restore_missing_fts_row(&store, &lock, permit, proposal, deadline(), || false)
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error:#}");
    }
    // Saving a native roots setting is independently visible to the writer
    // permit even when the applied publication and its token stay unchanged.
    let settings = crate::service::collection::settings_digest_with_writer(&store, ROOT, &lock)?;
    let settings_path = store.directory().join("roots.json");
    assert!(!settings_path.try_exists()?);
    crate::roots::RootConfig::load(store.directory(), ROOT)?.save(store.directory())?;
    let error = writer
        .restore_missing_fts_row(&store, &lock, &permit, &proposal, deadline(), || false)
        .unwrap_err();
    assert!(
        error.to_string().contains("saved settings changed"),
        "{error:#}"
    );
    let refused = store.current_read(&scope)?;
    assert_eq!(refused.token()?, previous);
    assert_eq!(refused.head().lexical_commit, damaged.head().lexical_commit);
    assert!(refused.lexical_search(&target_query)?.values.is_empty());

    std::fs::remove_file(settings_path)?;
    assert_eq!(
        crate::service::collection::settings_digest_with_writer(&store, ROOT, &lock)?,
        settings
    );
    let mut changed_configuration = configuration;
    changed_configuration.extractor_version.push_str("/next");
    let drift = writer.publish(PlainPublication {
        changes: vec![],
        resources: vec![],
        configuration: Some(&changed_configuration),
        recount: false,
    })?;
    let drifted = store.current_read(&scope)?;
    assert_eq!(drifted.token()?, drift.token);
    assert_eq!(drifted.head().lexical_commit, target.manifest_digest);
    assert_eq!(
        drifted
            .collection_state()?
            .expect("applied configuration")
            .configuration
            .extractor_version,
        changed_configuration.extractor_version
    );
    assert!(drifted.lexical_search(&target_query)?.values.is_empty());
    let error = writer
        .restore_missing_fts_row(&store, &lock, &permit, &proposal, deadline(), || false)
        .unwrap_err();
    assert!(
        error.to_string().contains("publication is stale"),
        "{error:#}"
    );

    let proposal = FtsRestore {
        sequence: drift.token.sequence,
        ..proposal
    };
    let permit = store.capture_current_fts_permit(&lock)?;
    let committed =
        writer.restore_missing_fts_row(&store, &lock, &permit, &proposal, deadline(), || false)?;
    let reused = writer
        .restore_missing_fts_row(&store, &lock, &permit, &proposal, deadline(), || false)
        .unwrap_err();
    assert!(
        reused.to_string().contains("publication is stale"),
        "{reused:#}"
    );
    let repaired = store.current_read(&scope)?;
    assert_eq!(committed, repaired.token()?);
    assert_eq!(committed.sequence, drift.token.sequence + 1);
    let replay = repaired.lexical_search(&target_query)?;
    assert_eq!(hit_paths(&replay), [(TARGET.to_owned(), 1)]);
    assert!(!replay.incomplete);
    assert_eq!(replay.values[0].stable_id, target.values[0].stable_id);
    assert_eq!(
        serde_json::to_vec(&replay.values[0].citation)?,
        target_citation
    );
    assert_eq!(
        replay.manifest_digest,
        framed_digest(
            b"airk-index/lexical/1",
            &[
                scope.incarnation.as_bytes(),
                &committed.sequence.to_be_bytes()
            ]
        )
    );
    assert_eq!(replay.manifest_digest, repaired.head().lexical_commit);
    let compact = repaired.artifact(TARGET)?.expect("compact record");
    let retained =
        repaired.load_retained_bounded(&compact, crate::index::FTS_RESTORE_MAX_BYTES as u64)?;
    assert_eq!(retained.chunks[1].text, "repairneedle\n");
    assert_eq!(damaged.token()?, previous);
    assert_eq!(damaged.head().lexical_commit, target.manifest_digest);
    assert!(damaged.lexical_search(&target_query)?.values.is_empty());
    assert_eq!(
        hit_paths(&healthy.lexical_search(&target_query)?),
        [(TARGET.to_owned(), 1)]
    );
    assert_eq!(
        hit_paths(&repaired.lexical_search(&positive_query)?),
        [(CONTROL.to_owned(), 0)]
    );
    assert!(repaired.lexical_search(&no_match_query)?.values.is_empty());
    let after = store.plain_read()?;
    assert_eq!(after.sequence(), committed.sequence);
    assert_eq!(after.reader().file(TARGET)?, pinned.reader().file(TARGET)?);
    assert_eq!(
        after.reader().file(CONTROL)?,
        pinned.reader().file(CONTROL)?
    );
    assert_eq!(
        after.reader().vector_setup()?,
        pinned.reader().vector_setup()?
    );
    assert_eq!(
        after.reader().units(TARGET)?,
        pinned.reader().units(TARGET)?
    );
    let matrix = after.reader().load_vectors()?.expect("vector matrix");
    assert_eq!(matrix.rows(), 1);
    assert_eq!(matrix.search(&[1.0, 0.0], 1, None)?[0].score, 1.0);
    Ok(())
}

fn hit_paths(candidates: &lexical::CurrentLexicalCandidates) -> Vec<(String, u64)> {
    candidates
        .values
        .iter()
        .map(|value| (value.citation.path.clone(), value.local))
        .collect()
}

fn exact_scope(path: &str) -> Arc<CompiledReadScope> {
    Arc::new(CompiledReadScope {
        fingerprint: Sha256Digest::from_bytes([8; 32]),
        atoms: vec![CompiledPathAtom {
            kind: PathScope::Exact,
            path: path.into(),
        }],
        source_atoms: vec![],
        external_aliases: vec![],
    })
}

#[test]
fn head_token_counts_and_collection_state_come_from_the_index() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let head = read.head();
    assert_eq!(head.scope, fixture.scope);
    assert_eq!(head.sequence, 1);
    assert_eq!(head.lexical_sequence, 1);
    assert_eq!(
        head.lexical_commit,
        framed_digest(b"airk-index/lexical/1", &[&[1; 16], &1u64.to_be_bytes()])
    );
    assert_eq!(
        head.artifact_root,
        framed_digest(b"airk-index/root/1", &[&[1; 16], &1u64.to_be_bytes()])
    );
    assert_eq!(head.graph_head.allocated_count, 0);
    assert_eq!(head.graph_head.live_nodes, 0);
    assert_eq!(
        head.graph_head.scope.specification,
        framed_digest(b"current-lexical-only/1", &[])
    );
    let token = read.token()?;
    assert_eq!(token.sequence, 1);
    assert_eq!(
        token.digest,
        framed_digest(
            b"current-index-head/3",
            &[&[1; 16], &1u64.to_be_bytes(), b"airk-index/1"]
        )
    );
    read.require_token(&token)?;
    assert_eq!(read.indexed_file_count()?, 5);
    assert_eq!(
        read.collection_state()?.unwrap().bytes()?,
        state(BTreeMap::new()).bytes()?
    );
    assert!(!read.private_query_eligible()?);
    assert!(read.query_projection_version()?.is_none());
    assert!(!read.has_prepared_update()?);
    assert!(read.terminal_call_postings_complete()?);
    Ok(())
}

#[test]
fn ranked_lexical_search_applies_prefix_paths_and_scope_before_the_limit() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let all = read.lexical_search(&query("alpha", 10))?;
    assert!(!all.incomplete);
    assert_eq!(all.manifest_digest, read.head().lexical_commit);
    let mut found = hit_paths(&all);
    found.sort();
    assert_eq!(
        found,
        vec![
            ("a/long.rs".into(), 0),
            ("docs/guide.pdf".into(), 1),
            ("src/def.rs".into(), 1),
            ("src/use.rs".into(), 1),
            ("z/short.rs".into(), 0),
        ]
    );
    // BM25: the one-word chunk ranks above the long one; path order is reverse.
    let position = |path: &str| {
        all.values
            .iter()
            .position(|value| value.citation.path == path)
            .unwrap()
    };
    assert!(position("z/short.rs") < position("a/long.rs"));
    // Every term is required.
    assert_eq!(
        hit_paths(&read.lexical_search(&query("alpha gamma", 10))?),
        vec![("src/def.rs".into(), 1)]
    );
    let mut prefixed = query("alpha", 10);
    prefixed.path_prefix = Some("src/".into());
    assert_eq!(read.lexical_search(&prefixed)?.values.len(), 2);
    let selected = BTreeSet::from(["src/use.rs".to_owned(), "z/short.rs".to_owned()]);
    let mut paths = hit_paths(&read.lexical_search_paths(&query("alpha", 10), Some(&selected))?);
    paths.sort();
    assert_eq!(
        paths,
        vec![("src/use.rs".into(), 1), ("z/short.rs".into(), 0)]
    );
    let limited = read.lexical_search(&query("alpha", 2))?;
    assert_eq!(limited.values.len(), 2);
    assert!(limited.incomplete);
    // The path term is indexed with the chunk.
    assert_eq!(read.lexical_search(&query("short", 10))?.values.len(), 1);
    // A read scope filters before the limit: the only allowed path is found
    // even though other chunks rank above it.
    let scoped = fixture.read()?;
    scoped.attach_read_scope(exact_scope("a/long.rs"))?;
    let result = scoped.lexical_search(&query("alpha", 1))?;
    assert_eq!(hit_paths(&result), vec![("a/long.rs".into(), 0)]);
    assert!(!result.incomplete);
    assert!(scoped.artifact("src/def.rs").is_err());
    Ok(())
}

#[test]
fn reference_pages_continue_in_path_and_stable_order() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let mut expected = vec![
        ("a/long.rs".to_owned(), 0u64),
        ("docs/guide.pdf".into(), 1),
        ("src/def.rs".into(), 1),
        ("src/use.rs".into(), 1),
        ("z/short.rs".into(), 0),
    ];
    for path_order in [true, false] {
        if !path_order {
            expected.sort_by_key(|(path, local)| lexical::stable_id(path, *local).unwrap());
        }
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = read.lexical_reference_page(
                &query("alpha", 2),
                after.as_ref(),
                false,
                path_order,
                None,
            )?;
            seen.extend(hit_paths(&page));
            if !page.incomplete {
                break;
            }
            after = page
                .values
                .last()
                .map(lexical::LexicalReferencePosition::from);
        }
        assert_eq!(seen, expected, "path order {path_order}");
        // An inclusive continuation repeats its own position.
        let first =
            read.lexical_reference_page(&query("alpha", 1), None, false, path_order, None)?;
        let again = read.lexical_reference_page(
            &query("alpha", 1),
            Some(&lexical::LexicalReferencePosition::from(&first.values[0])),
            true,
            path_order,
            None,
        )?;
        assert_eq!(hit_paths(&first), hit_paths(&again));
    }
    Ok(())
}

#[test]
fn path_lookups_page_by_files_language_and_selectors() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let files = read.lookup_paths(Lookup::Files, "", None, 100)?;
    assert_eq!(
        files.paths,
        vec![
            "a/long.rs",
            "docs/guide.pdf",
            "src/def.rs",
            "src/use.rs",
            "z/short.rs"
        ]
    );
    assert!(files.next_after.is_none());
    let first = read.lookup_paths(Lookup::Files, "src/", None, 1)?;
    assert_eq!(first.paths, vec!["src/def.rs"]);
    assert_eq!(first.next_after.as_deref(), Some("src/def.rs"));
    let second = read.lookup_paths(Lookup::Files, "src/", first.next_after.as_deref(), 1)?;
    assert_eq!(second.paths, vec!["src/use.rs"]);
    assert!(second.next_after.is_none());
    assert_eq!(
        read.lookup_paths(Lookup::Language("pdf"), "", None, 10)?
            .paths,
        vec!["docs/guide.pdf"]
    );
    assert_eq!(
        read.lookup_paths(Lookup::FactName("Widget"), "", None, 10)?
            .paths,
        vec!["src/def.rs", "src/use.rs"]
    );
    assert_eq!(
        read.lookup_paths(Lookup::FactKind("definition"), "", None, 10)?
            .paths,
        vec!["src/def.rs"]
    );
    assert_eq!(
        read.lookup_paths(Lookup::EdgeTarget("Widget"), "", None, 10)?
            .paths,
        vec!["src/use.rs"]
    );
    let filtered =
        read.lookup_paths_filtered(
            Lookup::Files,
            "",
            None,
            10,
            |path| Ok(path.ends_with(".rs")),
        )?;
    assert_eq!(filtered.paths.len(), 4);
    Ok(())
}

#[test]
fn symbols_and_outline_come_from_the_stored_payload() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let symbols = read.native_facts_page_with_options(
        &fixture.store,
        &FactQuery {
            path_prefix: String::new(),
            name: Some("Widget".into()),
            kind: Some("definition".into()),
            language: None,
            exact_name: true,
            limit: 10,
            max_bytes: 65_536,
            path_exact: false,
            document_only: false,
        },
        true,
        &DiscoveryOptions::default(),
        &FactDiscoveryOptions::default(),
    )?;
    assert_eq!(symbols.facts.len(), 1);
    let hit = &symbols.facts[0];
    let expected = &fixture.written["src/def.rs"].full.facts[0];
    assert_eq!(hit.citation.path, "src/def.rs");
    assert_eq!(hit.name, expected.name);
    assert_eq!(
        hit.code.as_ref().unwrap().symbol_id,
        expected.code.as_ref().unwrap().symbol_id
    );
    assert!(!symbols.truncated);
    let outline = read.native_facts_page(
        &fixture.store,
        &FactQuery {
            path_prefix: "docs/guide.pdf".into(),
            name: None,
            kind: None,
            language: None,
            exact_name: false,
            limit: 10,
            max_bytes: 65_536,
            path_exact: true,
            document_only: false,
        },
        false,
        &DiscoveryOptions {
            compact: true,
            ..Default::default()
        },
    )?;
    assert_eq!(outline.outline.len(), 1);
    assert_eq!(outline.outline[0].name, "Guide");
    Ok(())
}

#[test]
fn utf8_source_is_the_payload_text_and_documents_have_none() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let compact = read.artifact("src/def.rs")?.unwrap();
    assert_eq!(
        serde_json::to_vec(&compact)?,
        serde_json::to_vec(fixture.compact("src/def.rs"))?
    );
    let full = read.load_retained(&compact)?;
    assert_eq!(
        serde_json::to_vec(&full)?,
        serde_json::to_vec(&fixture.written["src/def.rs"].full)?
    );
    assert_eq!(
        read.read_retained_source(&compact)?,
        b"struct Widget;\nalpha gamma\n"
    );
    read.verify_retained_source(&compact)?;
    let retrieval = read.load_retrieval_artifact(&compact)?;
    assert_eq!(retrieval.payload.text, "struct Widget;\nalpha gamma\n");
    let document = read.artifact("docs/guide.pdf")?.unwrap();
    assert!(!read.retained_source_available(&document));
    assert!(read.read_retained_source(&document).is_err());
    read.verify_retained_source(&document)?;
    assert_eq!(read.load_retained(&document)?.chunks.len(), 2);
    let mut changed = document.clone();
    changed.content_hash = Sha256Digest::from_bytes([3; 32]).to_string();
    assert!(read.verify_retained_source(&changed).is_err());
    assert!(read.load_retained(&changed).is_err());
    assert!(read.artifact("missing.rs")?.is_none());
    Ok(())
}

#[test]
fn file_metadata_handles_resources_and_measurements() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let file = read.file_metadata(&fixture.store, "src/use.rs")?.unwrap();
    assert_eq!(
        serde_json::to_vec(&file)?,
        serde_json::to_vec(&fixture.written["src/use.rs"].file)?
    );
    assert_eq!(
        read.file_metadata_bytes("src/use.rs")?,
        serde_json::to_vec(&fixture.written["src/use.rs"].file)?.len()
    );
    assert_eq!(
        read.artifact_path(&fixture.store, artifact_id("src/use.rs"))?
            .as_deref(),
        Some("src/use.rs")
    );
    assert!(
        read.artifact_path(&fixture.store, artifact_id("absent.rs"))?
            .is_none()
    );
    assert_eq!(
        read.resource(&fixture.store, "semantic-policy")?.as_deref(),
        Some(br#"{"policy":"fixture"}"#.as_slice())
    );
    let reference = read.resource_reference("semantic-policy")?.unwrap();
    assert_eq!(reference.bytes, br#"{"policy":"fixture"}"#.len());
    assert!(read.resource(&fixture.store, "analysis")?.is_none());
    assert!(read.analysis(&fixture.store)?.is_none());
    assert!(read.resource_reference("analysis-projection")?.is_none());
    assert!(read.resource_reference("not-a-key").is_err());
    assert_eq!(
        read.map_get(MapDomain::SourceMeasurement, b"src/def.rs")?
            .as_deref(),
        Some(b"{\"measured\":1}".as_slice())
    );
    assert!(
        read.map_get(MapDomain::SourceMeasurement, b"src/use.rs")?
            .is_none()
    );
    assert!(
        read.map_get(MapDomain::FileMetadata, b"src/use.rs")
            .is_err()
    );
    let recent = read.recent_changes(0)?;
    assert!(recent.events.is_empty());
    assert_eq!(
        recent.view_digest,
        framed_digest(b"recent-change-view/1", &[b"absent"])
    );
    Ok(())
}

#[test]
fn relationships_follow_stored_edge_selectors() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.read()?;
    let incoming = read.native_relationships(
        &fixture.store,
        &RelationshipQuery {
            symbol: "Widget".into(),
            direction: "incoming".into(),
            kind: None,
            path_prefix: String::new(),
            hops: 1,
            limit: 10,
            max_bytes: 65_536,
        },
    )?;
    assert_eq!(incoming.relationships.len(), 1);
    let edge = &incoming.relationships[0];
    assert_eq!(edge.source, "file:src/use.rs");
    assert_eq!(edge.target, "Widget");
    assert_eq!(edge.citation.path, "src/use.rs");
    Ok(())
}

#[test]
fn a_reader_keeps_its_snapshot_across_a_commit() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let before = fixture.read()?;
    fixture.writer.apply(Batch {
        files: vec![FileChange::Delete("z/short.rs".into())],
        ..Default::default()
    })?;
    assert!(before.artifact("z/short.rs")?.is_some());
    assert_eq!(before.head().sequence, 1);
    let after = fixture.read()?;
    assert_eq!(after.head().sequence, 2);
    assert!(after.artifact("z/short.rs")?.is_none());
    assert_ne!(before.token()?, after.token()?);
    Ok(())
}

#[test]
fn old_and_foreign_directories_are_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = FileStore::new(directory.path().join("collections-1.2").join(COLLECTION));
    drop(store.lock()?);
    let scope = CurrentScope {
        root: ROOT.into(),
        collection: COLLECTION.into(),
        incarnation: Incarnation::from_bytes([1; 16]),
    };
    std::fs::write(store.directory().join("current.sqlite3"), b"old")?;
    assert!(store.current_scope(ROOT, COLLECTION)?.is_none());
    let error = store.current_read(&scope).err().expect("old index refused");
    assert!(
        error.downcast_ref::<IncompatibleIndex>().is_some(),
        "{error:#}"
    );
    std::fs::remove_file(store.directory().join("current.sqlite3"))?;
    // An SQLite file without the plain-index format marks.
    rusqlite::Connection::open(store.directory().join("index.sqlite3"))?
        .execute_batch("CREATE TABLE old(x);")?;
    let error = store
        .current_read(&scope)
        .err()
        .expect("foreign format refused");
    assert!(
        error.downcast_ref::<IncompatibleIndex>().is_some(),
        "{error:#}"
    );
    assert!(store.current_scope(ROOT, COLLECTION).is_err());
    // A plain index of another root is not this collection's.
    let fixture = Fixture::new()?;
    assert!(
        fixture
            .store
            .current_scope("another-root", COLLECTION)
            .is_err()
    );
    assert_eq!(
        fixture.store.current_scope(ROOT, COLLECTION)?,
        Some(fixture.scope.clone())
    );
    Ok(())
}

#[test]
fn published_aliases_freeze_into_the_reader_sources() -> Result<()> {
    let fixture = Fixture::with(
        vec![
            utf8("@docs/a.md", "alpha\n", vec![]),
            utf8("b.rs", "alpha\n", vec![]),
        ],
        BTreeMap::from([("docs".to_owned(), "/other/documents".to_owned())]),
    )?;
    let read = fixture.read()?;
    assert!(read.accepts_path("@docs/a.md"));
    assert_eq!(read.lexical_search(&query("alpha", 10))?.values.len(), 2);
    let reserved = Fixture::with(
        vec![utf8("@docs/a.md", "alpha\n", vec![])],
        BTreeMap::from([(
            "docs".to_owned(),
            "/other/.airk/agent-edits/v1/old".to_owned(),
        )]),
    )?;
    assert!(
        reserved.read().is_err()
            || reserved
                .read()?
                .lexical_search(&query("alpha", 10))
                .is_err()
    );
    Ok(())
}
