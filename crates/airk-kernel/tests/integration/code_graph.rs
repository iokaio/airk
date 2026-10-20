// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::{EvidenceBasis, FactQuery, RelationshipQuery};
use airk_kernel::Kernel;
use airk_store::FileStore;
use protobuf::{Message, MessageField};
use scip::types::*;
use std::{fs, path::Path};

fn fact_query(name: Option<&str>, kind: Option<&str>) -> FactQuery {
    FactQuery {
        path_prefix: String::new(),
        name: name.map(str::to_owned),
        kind: kind.map(str::to_owned),
        language: None,
        exact_name: true,
        limit: 100,
        max_bytes: 65536,
        path_exact: false,
        document_only: false,
    }
}
fn edges(symbol: &str, kind: Option<&str>) -> RelationshipQuery {
    RelationshipQuery {
        symbol: symbol.into(),
        direction: "incoming".into(),
        kind: kind.map(str::to_owned),
        path_prefix: String::new(),
        hops: 1,
        limit: 100,
        max_bytes: 65536,
    }
}

#[test]
fn sqlite_facts_graph_budgets_scope_refresh_and_candidates() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("a.ts"),
        "class Auth { run() { db.query('x'); } }\nrouter.get('/x', handler);\n",
    )?;
    fs::write(
        root.path().join("a.md"),
        "# Requirements\nThe client shall respond within 100 ms.\n",
    )?;
    let kernel = Kernel::open_collection(root.path(), Some(state.path().into()), "fixture")?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    assert_eq!(kernel.status()?.exact_fact_backend, "sqlite");
    let facts = kernel.code_facts(&fact_query(Some("Auth::run"), Some("definition")))?;
    assert_eq!(facts.facts.len(), 1);
    let method = facts.facts[0].code.as_ref().expect("method");
    let calls = kernel.relationships(&edges("db.query", Some("call")))?;
    assert_eq!(calls.relationships.len(), 1);
    assert_eq!(calls.relationships[0].source, method.symbol_id);
    assert_eq!(calls.relationships[0].resolution, "syntax_candidate");
    assert!(
        kernel
            .code_facts(&fact_query(None, None))?
            .facts
            .iter()
            .all(|f| f.code.is_some())
    );
    let mut documents = fact_query(None, None);
    documents.document_only = true;
    assert!(
        kernel
            .facts(&documents)?
            .facts
            .iter()
            .all(|f| f.code.is_none())
    );
    let mut small = fact_query(None, None);
    small.max_bytes = 1;
    assert!(kernel.code_facts(&small)?.truncated);
    let mut small_edge = edges("db.query", None);
    small_edge.max_bytes = 1;
    assert!(kernel.relationships(&small_edge)?.truncated);
    small_edge.hops = 4;
    assert!(kernel.relationships(&small_edge).is_err());
    assert!(
        Kernel::open(root.path(), Some(state.path().into()))?
            .code_facts(&fact_query(None, None))
            .is_err()
    );
    let foreign = tempfile::tempdir()?;
    assert!(
        Kernel::open_collection(foreign.path(), Some(state.path().into()), "fixture")
            .and_then(|k| k.relationships(&edges("db.query", None)))
            .is_err()
    );
    fs::write(
        root.path().join("a.ts"),
        "class Auth { run() { logger.info('x'); } }\n",
    )?;
    kernel.index()?;
    assert!(
        kernel
            .relationships(&edges("db.query", Some("call")))?
            .relationships
            .is_empty()
    );
    assert_eq!(
        kernel
            .relationships(&edges("logger.info", Some("call")))?
            .relationships
            .len(),
        1
    );
    Ok(())
}

fn scip_fixture(root: &Path) -> anyhow::Result<Vec<u8>> {
    let mut index = Index::new();
    index.metadata = MessageField::some(Metadata {
        project_root: url::Url::from_directory_path(root)
            .map_err(|_| anyhow::anyhow!("root URI"))?
            .into(),
        text_document_encoding: TextEncoding::UTF8.into(),
        tool_info: MessageField::some(ToolInfo {
            name: "fixture-indexer".into(),
            version: "1".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut symbol = SymbolInformation {
        symbol: "scip npm fixture 1 run().".into(),
        display_name: "run".into(),
        ..Default::default()
    };
    symbol.relationships.push(Relationship {
        symbol: "scip npm fixture 1 Runnable#".into(),
        is_implementation: true,
        ..Default::default()
    });
    index.documents.push(Document {
        relative_path: "a.ts".into(),
        language: "TypeScript".into(),
        position_encoding: PositionEncoding::UTF16CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![
            Occurrence {
                symbol: "scip npm fixture 1 run().".into(),
                symbol_roles: 1,
                range: vec![0, 9, 12],
                ..Default::default()
            },
            Occurrence {
                symbol: "scip npm fixture 1 run().".into(),
                range: vec![1, 0, 3],
                ..Default::default()
            },
        ],
        symbols: vec![symbol],
        ..Default::default()
    });
    Ok(index.write_to_bytes()?)
}

#[test]
fn scip_requires_binding_and_invalidates_on_source_and_unindexed_build_changes()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let import_dir = tempfile::tempdir()?;
    fs::write(root.path().join("a.ts"), "function run() {}\nrun();\n")?;
    fs::write(root.path().join("flags.bin"), [0, 255])?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let mut binding = kernel.prepare_analysis("fixture-indexer", "1", &["flags.bin".into()])?;
    let bytes = scip_fixture(root.path())?;
    let index_path = import_dir.path().join("index.scip");
    let manifest_path = import_dir.path().join("binding.json");
    fs::write(&index_path, &bytes)?;
    fs::write(&manifest_path, serde_json::to_vec(&binding)?)?;
    let prior = kernel.status()?.generation;
    assert!(
        kernel.import_scip(&index_path, &manifest_path).is_err(),
        "unbound output rejected"
    );
    assert_eq!(kernel.status()?.generation, prior);
    binding.index_sha256 = Some(airk_ingest::semantic::digest(&bytes));
    fs::write(&manifest_path, serde_json::to_vec(&binding)?)?;
    assert_eq!(
        kernel
            .import_scip(&index_path, &manifest_path)?
            .imported_facts,
        2
    );
    let imported = kernel.code_facts(&fact_query(Some("run"), Some("definition")))?;
    assert!(imported.facts.iter().any(|f| {
        f.code
            .as_ref()
            .is_some_and(|c| c.basis == EvidenceBasis::Imported)
    }));
    let semantic_definition = imported
        .facts
        .iter()
        .find(|f| {
            f.code
                .as_ref()
                .is_some_and(|c| c.basis == EvidenceBasis::Imported)
        })
        .expect("semantic definition");
    let span = semantic_definition
        .code
        .as_ref()
        .expect("code")
        .span
        .as_ref()
        .expect("normalized source span");
    assert_eq!((span.start_byte, span.end_byte), (9, 12));
    let implementation = kernel.relationships(&edges(
        "scip:fixture-indexer:scip npm fixture 1 Runnable#",
        Some("implements"),
    ))?;
    assert_eq!(implementation.relationships.len(), 1);
    assert_eq!(
        implementation.relationships[0].resolution,
        "provider_asserted"
    );
    assert!(
        kernel
            .relationships(&edges(
                "scip:fixture-indexer:scip npm fixture 1 run().",
                Some("call")
            ))?
            .relationships
            .is_empty(),
        "SCIP reference does not become a call"
    );
    let generation = kernel.status()?.generation;
    kernel.index()?;
    assert_eq!(
        kernel.status()?.imported_facts,
        2,
        "unchanged bindings retained"
    );
    fs::write(root.path().join("flags.bin"), [0, 254])?;
    kernel.index()?;
    assert_eq!(
        kernel.status()?.imported_facts,
        0,
        "unindexed build change invalidates whole provider domain"
    );
    assert!(kernel.status()?.generation > generation);
    assert!(
        kernel.import_scip(&index_path, &manifest_path).is_err(),
        "changed build rejected"
    );
    fs::write(root.path().join("flags.bin"), [0, 255])?;
    kernel.import_scip(&index_path, &manifest_path)?;
    fs::write(
        root.path().join("a.ts"),
        "function run() { return 1; }\nrun();\n",
    )?;
    assert!(
        kernel.import_scip(&index_path, &manifest_path).is_err(),
        "live source mismatch rejected before publication"
    );
    kernel.index()?;
    assert_eq!(kernel.status()?.imported_relationships, 0);
    fs::remove_file(root.path().join("a.ts"))?;
    kernel.index()?;
    assert!(
        kernel
            .code_facts(&fact_query(Some("run"), None))?
            .facts
            .is_empty()
    );
    assert!(
        FileStore::new(state.path().join("collections-1.2/default"))
            .current_test_catalog()?
            .expect("current observation")
            .snapshot
            .analysis
            .is_none()
    );
    Ok(())
}

#[test]
fn invalid_scip_documents_producer_digest_and_ranges_leave_revision_intact() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let files = tempfile::tempdir()?;
    fs::write(root.path().join("a.ts"), "function run() {}\nrun();\n")?;
    fs::write(root.path().join("tsconfig.json"), "{}")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let binding = kernel.prepare_analysis("fixture-indexer", "1", &["tsconfig.json".into()])?;
    let original = Index::parse_from_bytes(&scip_fixture(root.path())?)?;
    let generation = kernel.status()?.generation;
    for failure in [
        "path", "range", "encoding", "language", "producer", "root", "digest",
    ] {
        let mut index = original.clone();
        match failure {
            "path" => index.documents[0].relative_path = "../a.ts".into(),
            "range" => index.documents[0].occurrences[0].range = vec![0, 9, 900],
            "encoding" => {
                index.documents[0].position_encoding =
                    PositionEncoding::UnspecifiedPositionEncoding.into()
            }
            "language" => index.documents[0].language = "Python".into(),
            "producer" => {
                index
                    .metadata
                    .as_mut()
                    .expect("metadata")
                    .tool_info
                    .as_mut()
                    .expect("tool")
                    .version = "2".into()
            }
            "root" => {
                index.metadata.as_mut().expect("metadata").project_root =
                    "file:///foreign-root/".into()
            }
            _ => {}
        }
        let bytes = index.write_to_bytes()?;
        let mut m = binding.clone();
        m.index_sha256 = Some(if failure == "digest" {
            "incorrect".into()
        } else {
            airk_ingest::semantic::digest(&bytes)
        });
        fs::write(files.path().join("index.scip"), bytes)?;
        fs::write(files.path().join("binding.json"), serde_json::to_vec(&m)?)?;
        assert!(
            kernel
                .import_scip(
                    &files.path().join("index.scip"),
                    &files.path().join("binding.json")
                )
                .is_err(),
            "{failure}"
        );
        assert_eq!(kernel.status()?.generation, generation);
        assert_eq!(kernel.status()?.imported_facts, 0);
    }
    Ok(())
}
