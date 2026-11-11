// SPDX-License-Identifier: Apache-2.0
//! Imported references cannot prove authority over their own symbol origin.
use super::{
    CurrentRead, CurrentScope,
    analysis::AnalysisEvidence,
    map::{self, MapDomain},
    postings::Lookup,
    query_projection::ProjectionRow,
    read::scoped_import_test_reader,
};
use crate::{
    FileStore,
    quota::{Quota, Reservation},
};
use airk_core::{
    AnalysisManifest, Citation, CodeEvidence, DocumentLocation, EvidenceBasis, FactHit,
    RelationshipHit,
    compression::CompressionMode,
    identity::{Incarnation, Sha256Digest},
    index_catalog::PathScope,
    scoped_reads::{CompiledPathAtom, CompiledReadScope, CompiledSourceAtom, ScopeReadRefused},
};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::{collections::BTreeMap, sync::Arc};

struct Fixture {
    read: CurrentRead,
    _analysis_reservation: Reservation,
    _projection_reservation: Option<Reservation>,
    store: FileStore,
    evidence: AnalysisEvidence,
    symbol: String,
    _directory: tempfile::TempDir,
}

fn fact(path: &str, kind: &str, symbol: &str, basis: EvidenceBasis) -> FactHit {
    FactHit {
        candidate: false,
        continuity_key: None,
        name: if kind == "definition" {
            "secret"
        } else {
            "secret-use"
        }
        .into(),
        kind: kind.into(),
        syntax_kind: kind.into(),
        language: "rust".into(),
        citation: Citation {
            path: path.into(),
            content_hash: Sha256Digest::from_bytes([4; 32]).to_string(),
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
        },
        document: None,
        code: Some(CodeEvidence {
            fingerprints: None,
            symbol_id: symbol.into(),
            qualified_name: "fixture::secret".into(),
            scope: vec![],
            owner_id: None,
            signature: None,
            span: None,
            signature_span: None,
            parameters: None,
            return_type: None,
            modifiers: vec![],
            documentation: None,
            target: None,
            basis,
            producer: "scoped-origin-fixture".into(),
            producer_version: "1".into(),
            coverage: "Original bounded fixture".into(),
            details_truncated: false,
        }),
    }
}

impl Fixture {
    fn new(private_definition: bool) -> Result<Self> {
        Self::with_basis(private_definition, EvidenceBasis::Imported)
    }
    fn with_basis(private_definition: bool, basis: EvidenceBasis) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        std::fs::create_dir(&source)?;
        let quota = Quota::open(directory.path().join("data"))?;
        let store = FileStore::new(quota.root().join("collections-1.2/import-origin"));
        let writer = store.lock()?;
        let scope = CurrentScope {
            root: source.to_string_lossy().into_owned(),
            collection: "import-origin".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let definition_path = if private_definition {
            "private/secret.rs"
        } else {
            "public/secret.rs"
        };
        let symbol = format!("lsp:fixture:{definition_path}:10:20");
        let owner = "lsp:fixture:public/use.rs:1:2";
        let definition = fact(
            definition_path,
            "definition",
            &symbol,
            EvidenceBasis::Imported,
        );
        let reference = fact("public/use.rs", "reference", &symbol, basis);
        let owner_definition = fact(
            "public/use.rs",
            "definition",
            owner,
            EvidenceBasis::Imported,
        );
        let edge = RelationshipHit {
            source: owner.into(),
            target: symbol.clone(),
            kind: "reference".into(),
            resolution: "provider_asserted".into(),
            basis: EvidenceBasis::Imported,
            producer: "scoped-origin-fixture".into(),
            producer_version: "1".into(),
            citation: reference.citation.clone(),
        };
        let evidence = AnalysisEvidence {
            manifest: AnalysisManifest {
                version: 1,
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                extractor_version: "fixture".into(),
                source_hashes: BTreeMap::from([
                    (
                        definition_path.into(),
                        definition.citation.content_hash.clone(),
                    ),
                    (
                        "public/use.rs".into(),
                        reference.citation.content_hash.clone(),
                    ),
                ]),
                build_inputs: BTreeMap::new(),
                producer: "scoped-origin-fixture".into(),
                producer_version: "1".into(),
                index_sha256: None,
            },
            facts: vec![definition, reference, owner_definition],
            relationships: vec![edge],
        };
        let analysis_reservation = quota.reserve(&scope.collection, 4 << 20)?;
        let analysis = store.retain_current_resource_owned(
            &writer,
            &evidence.canonical_bytes()?,
            &analysis_reservation,
        )?;
        let languages = evidence
            .manifest
            .source_hashes
            .keys()
            .map(|path| (path.clone(), "rust".into()))
            .collect();
        let prepared = store.prepare_import_projection(
            &evidence,
            &analysis,
            &languages,
            CompressionMode::Off,
        )?;
        let (projection, projection_reservation) = store
            .stage_query_projection(&writer, &quota, &prepared)?
            .context("fixture projection declined")?;
        let connection = Connection::open_in_memory()?;
        map::create(&connection)?;
        let mut root = map::empty_root(&scope)?;
        for (key, reference) in [("analysis", analysis), ("analysis-projection", projection)] {
            root = map::put(
                &connection,
                &scope,
                root,
                map::key(&scope, MapDomain::Metadata, key.as_bytes())?,
                Some(&serde_json::to_vec(&reference)?),
            )?;
        }
        drop(writer);
        let read = scoped_import_test_reader(&store, scope.clone(), root, connection)?;
        read.attach_read_scope(Arc::new(CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([7; 32]),
            atoms: vec![CompiledPathAtom {
                kind: PathScope::Subtree,
                path: "public".into(),
            }],
            source_atoms: vec![CompiledSourceAtom {
                original_root: scope.root,
                kind: PathScope::Subtree,
                path: "public".into(),
            }],
            external_aliases: vec![],
        }))?;
        Ok(Self {
            _directory: directory,
            _analysis_reservation: analysis_reservation,
            _projection_reservation: projection_reservation,
            store,
            read,
            evidence,
            symbol,
        })
    }
}

#[test]
fn private_imported_definition_cannot_be_authorized_by_its_public_reference() -> Result<()> {
    let fixture = Fixture::new(true)?;
    assert!(fixture.read.restricts_paths());
    assert!(
        !fixture
            .read
            .accepts_endpoint(&fixture.store, &fixture.symbol)?
    );
    let projection = fixture
        .read
        .imported_projection(&fixture.store)?
        .context("fixture projection missing")?;
    assert!(!projection.definition_origin(&fixture.symbol)?);
    Ok(())
}

#[test]
fn imported_resource_cannot_claim_native_basis_to_disclose_private_origin() -> Result<()> {
    for basis in [EvidenceBasis::Syntax, EvidenceBasis::StructuralRule] {
        let fixture = Fixture::with_basis(true, basis)?;
        assert!(
            !fixture
                .read
                .accepts_endpoint(&fixture.store, &fixture.symbol)?
        );
        let error = fixture.read.analysis(&fixture.store).unwrap_err();
        assert!(error.is::<ScopeReadRefused>());
        assert!(!error.to_string().contains("private/secret.rs"));
        let projection = fixture
            .read
            .imported_projection(&fixture.store)?
            .context("fixture projection missing")?;
        let selected =
            projection.lookup_paths(Lookup::FactName(&fixture.symbol), "public", None, 10)?;
        for page in &selected[0].pages {
            let error = projection.page(*page).unwrap_err();
            assert!(error.is::<ScopeReadRefused>());
            assert!(!error.to_string().contains("private/secret.rs"));
        }
    }
    Ok(())
}

#[test]
fn imported_owner_and_target_require_allowed_definition_origins() -> Result<()> {
    let fixture = Fixture::new(true)?;
    let original = fixture
        .evidence
        .facts
        .iter()
        .find(|hit| hit.kind == "definition" && hit.citation.path == "public/use.rs")
        .context("public owner definition missing")?;
    for owner in [true, false] {
        let mut hit = original.clone();
        hit.kind = "reference".into();
        let code = hit.code.as_mut().context("fixture code missing")?;
        if owner {
            code.owner_id = Some(fixture.symbol.clone());
        } else {
            code.target = Some(fixture.symbol.clone());
        }
        let error = fixture
            .read
            .require_imported_fact(&fixture.store, &hit)
            .unwrap_err();
        assert!(error.is::<ScopeReadRefused>());
        assert!(!error.to_string().contains("private/secret.rs"));
    }
    Ok(())
}

#[test]
fn scoped_imported_analysis_and_fact_projection_refuse_private_symbol_origins() -> Result<()> {
    let fixture = Fixture::new(true)?;
    let error = fixture.read.analysis(&fixture.store).unwrap_err();
    assert!(error.downcast_ref::<ScopeReadRefused>().is_some());
    assert!(!error.to_string().contains("private/secret.rs"));
    let projection = fixture
        .read
        .imported_projection(&fixture.store)?
        .context("fixture imported projection missing")?;
    assert!(!projection.definition_origin(&fixture.symbol)?);
    let selected =
        projection.lookup_paths(Lookup::FactName(&fixture.symbol), "public", None, 10)?;
    assert_eq!(
        selected.len(),
        1,
        "the public reference remains a selector candidate, not an origin proof"
    );
    for page in &selected[0].pages {
        let error = projection.page(*page).unwrap_err();
        assert!(error.downcast_ref::<ScopeReadRefused>().is_some());
        assert!(!error.to_string().contains("private/secret.rs"));
    }
    let selected =
        projection.lookup_paths(Lookup::EdgeTarget(&fixture.symbol), "public", None, 10)?;
    assert_eq!(selected.len(), 1);
    for page in &selected[0].pages {
        assert!(
            projection.page(*page)?.is_empty(),
            "a reference must not self-authorize its graph target"
        );
    }
    Ok(())
}

#[test]
fn allowed_imported_definition_preserves_exact_fact_and_edge_payloads() -> Result<()> {
    let fixture = Fixture::new(false)?;
    assert!(
        fixture
            .read
            .accepts_endpoint(&fixture.store, &fixture.symbol)?
    );
    let analysis = fixture
        .read
        .analysis(&fixture.store)?
        .context("fixture analysis missing")?;
    assert_eq!(
        serde_json::to_value(&analysis.facts)?,
        serde_json::to_value(&fixture.evidence.facts)?
    );
    assert_eq!(
        serde_json::to_value(&analysis.relationships)?,
        serde_json::to_value(&fixture.evidence.relationships)?
    );
    let projection = fixture
        .read
        .imported_projection(&fixture.store)?
        .context("fixture projection missing")?;
    assert!(projection.definition_origin(&fixture.symbol)?);
    let mut facts = Vec::new();
    let mut edges = Vec::new();
    for page in 0..projection.header().pages {
        for row in projection.page(page)? {
            match row {
                ProjectionRow::Fact { ordinal, hit, .. } => facts.push((ordinal, hit)),
                ProjectionRow::Edge { ordinal, hit } => edges.push((ordinal, hit)),
            }
        }
    }
    facts.sort_by_key(|(ordinal, _)| *ordinal);
    edges.sort_by_key(|(ordinal, _)| *ordinal);
    assert_eq!(
        serde_json::to_value(facts.into_iter().map(|(_, hit)| hit).collect::<Vec<_>>())?,
        serde_json::to_value(&fixture.evidence.facts)?
    );
    assert_eq!(
        serde_json::to_value(edges.into_iter().map(|(_, hit)| hit).collect::<Vec<_>>())?,
        serde_json::to_value(&fixture.evidence.relationships)?
    );
    Ok(())
}
