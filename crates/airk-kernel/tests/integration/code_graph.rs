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
fn rust_type_navigation_uses_same_file_scope_and_declared_methods() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("sessions.rs"),
        r#"
mod left {
    pub struct Session<T>(T);
    trait Lifecycle { fn ready(); }
    impl<T> Session<T> {
        pub fn spawn(value: T) -> Self { Self(value) }
        fn request(&self) {}
    }
    impl<T> Lifecycle for Session<T> { fn ready() {} }
    fn build() { Session::spawn(1); }
}
mod right {
    pub struct Session;
    impl Session { fn unrelated() -> Self { Self } }
}
type Alias = left::Session<u8>;
impl Alias { fn alias_only() {} }
struct Shadow;
trait Marker { fn generic_only(); }
impl<Shadow: Marker> Marker for Shadow { fn generic_only() {} }
struct Attributed;
impl<#[allow(unused)] Attributed: Marker> Marker for Attributed { fn generic_only() {} }
"#,
    )?;
    fs::write(
        root.path().join("other.rs"),
        "struct Session; impl Session { fn foreign_only() {} }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::AgentLean)?;
    kernel.index()?;
    let types = kernel.code_facts(&fact_query(Some("Session"), Some("definition")))?;
    let declaration = types
        .facts
        .iter()
        .find(|fact| {
            fact.syntax_kind == "struct_item"
                && fact
                    .code
                    .as_ref()
                    .is_some_and(|code| code.scope == ["left"])
        })
        .expect("left Session declaration");
    let symbol = &declaration.code.as_ref().unwrap().symbol_id;
    let incoming = kernel.relationships(&edges(symbol, None))?;
    let mut targets: Vec<_> = incoming
        .suggested_targets
        .iter()
        .map(|s| s.target.as_str())
        .collect();
    targets.sort();
    assert_eq!(
        targets,
        ["Session::ready", "Session::request", "Session::spawn"]
    );
    assert!(incoming.suggested_targets.iter().all(|suggestion| {
        suggestion.resolution == "syntax_candidate"
            && suggestion.basis == EvidenceBasis::Syntax
            && suggestion.citation.path == "sessions.rs"
            && suggestion.citation.content_hash == declaration.citation.content_hash
    }));
    // Type association never turns written-name callers into resolved incoming
    // edges of that type. The user may follow a declared spelling explicitly.
    assert!(
        incoming
            .relationships
            .iter()
            .all(|edge| edge.kind != "call")
    );
    let callers = kernel.relationships(&edges("Session::spawn", Some("call")))?;
    assert_eq!(callers.relationships.len(), 1);
    assert_eq!(callers.relationships[0].resolution, "syntax_candidate");
    let mut outgoing = edges(symbol, Some("contains"));
    outgoing.direction = "outgoing".into();
    outgoing.hops = 2;
    let outgoing = kernel.relationships(&outgoing)?;
    assert!(outgoing.relationships.iter().any(|edge| {
        edge.source == *symbol && edge.producer == "airk-retained-rust-type-navigation"
    }));
    let spawn = incoming
        .suggested_targets
        .iter()
        .find(|s| s.target == "Session::spawn")
        .unwrap();
    assert!(
        outgoing
            .relationships
            .iter()
            .any(|edge| edge.target == spawn.symbol_id)
    );
    // A written name still has its old semantics; only exact returned type IDs
    // opt into declaration association. Another project's/path's facts cannot join.
    assert!(
        kernel
            .relationships(&edges("Session", None))?
            .suggested_targets
            .is_empty()
    );
    let mut excluded = edges(symbol, None);
    excluded.path_prefix = "other.rs".into();
    assert!(
        kernel
            .relationships(&excluded)?
            .suggested_targets
            .is_empty()
    );
    let shadow = kernel.code_facts(&fact_query(Some("Shadow"), Some("definition")))?;
    let shadow = shadow
        .facts
        .iter()
        .find(|fact| fact.syntax_kind == "struct_item")
        .unwrap();
    let shadow = &shadow.code.as_ref().unwrap().symbol_id;
    assert!(
        kernel
            .relationships(&edges(shadow, None))?
            .suggested_targets
            .is_empty()
    );
    let attributed = kernel.code_facts(&fact_query(Some("Attributed"), Some("definition")))?;
    let attributed = attributed
        .facts
        .iter()
        .find(|fact| fact.syntax_kind == "struct_item")
        .unwrap();
    let attributed = &attributed.code.as_ref().unwrap().symbol_id;
    assert!(
        kernel
            .relationships(&edges(attributed, None))?
            .suggested_targets
            .is_empty()
    );
    Ok(())
}

#[test]
fn rust_type_navigation_rejects_ambiguous_declarations_and_respects_budget() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("ambiguous.rs"),
        "#[cfg(first)] struct Session;\n#[cfg(second)] struct Session;\nimpl Session { fn spawn() {} }\n",
    )?;
    fs::write(
        root.path().join("unique.rs"),
        "struct Unique; impl Unique { fn open() {} fn close() {} }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let declarations = kernel.code_facts(&fact_query(Some("Session"), Some("definition")))?;
    for declaration in declarations
        .facts
        .iter()
        .filter(|fact| fact.syntax_kind == "struct_item")
    {
        let symbol = &declaration.code.as_ref().unwrap().symbol_id;
        let response = kernel.relationships(&edges(symbol, None))?;
        assert!(response.suggested_targets.is_empty());
        assert!(
            !response
                .relationships
                .iter()
                .any(|edge| edge.producer == "airk-retained-rust-type-navigation")
        );
    }
    let unique = kernel.code_facts(&fact_query(Some("Unique"), Some("definition")))?;
    let declaration = unique
        .facts
        .iter()
        .find(|fact| fact.syntax_kind == "struct_item")
        .unwrap();
    let symbol = &declaration.code.as_ref().unwrap().symbol_id;
    let normal = kernel.relationships(&edges(symbol, None))?;
    assert_eq!(normal.suggested_targets.len(), 2);
    let mut tiny = edges(symbol, None);
    tiny.max_bytes = 1;
    let tiny = kernel.relationships(&tiny)?;
    assert!(tiny.truncated && tiny.relationships.is_empty() && tiny.suggested_targets.is_empty());
    let mut one = edges(symbol, None);
    one.limit = 1;
    let one = kernel.relationships(&one)?;
    assert_eq!(one.suggested_targets.len(), 1);
    assert!(one.truncated);
    // Live bytes cannot replace the retained declaration or its original hash.
    fs::write(root.path().join("unique.rs"), "struct Replacement;\n")?;
    let retained = kernel.relationships(&edges(symbol, None))?;
    assert_eq!(retained.suggested_targets.len(), 2);
    assert_eq!(
        retained.suggested_targets[0].citation.content_hash,
        declaration.citation.content_hash
    );
    kernel.index()?;
    assert!(
        kernel
            .relationships(&edges(symbol, None))?
            .suggested_targets
            .is_empty()
    );
    Ok(())
}

#[test]
fn terminal_method_names_find_receiver_and_chained_calls_without_resolution() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    for (path, source) in [
        (
            "one.py",
            "def run(x):\n    x.reclaim_objects()\n    x.next().reclaim_objects()\n    x.Reclaim_objects()\n    # x.reclaim_objects()\n    label = 'reclaim_objects'\n",
        ),
        (
            "one.rs",
            "fn run(x: &Thing) { x.reclaim_objects(); x.next().reclaim_objects(); x.Reclaim_objects(); }\n",
        ),
        (
            "one.ts",
            "function run(x) { x.reclaim_objects(); x.next().reclaim_objects(); x.Reclaim_objects(); }\n",
        ),
    ] {
        fs::write(root.path().join(path), source)?;
    }
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let query = fact_query(Some("reclaim_objects"), Some("call"));
    let found = kernel.code_facts(&query)?;
    assert_eq!(found.facts.len(), 6);
    assert!(
        found
            .facts
            .iter()
            .all(|hit| hit.candidate && hit.code.as_ref().unwrap().basis == EvidenceBasis::Syntax)
    );
    assert!(found.facts.iter().any(|hit| hit.name.contains("next()")));
    let mut scoped = query.clone();
    scoped.path_prefix = "one.py".into();
    assert_eq!(kernel.code_facts(&scoped)?.facts.len(), 2);
    let mut small = query.clone();
    small.max_bytes = 1;
    let small = kernel.code_facts(&small)?;
    assert!(small.facts.is_empty() && small.truncated);
    fs::remove_file(root.path().join("one.py"))?;
    fs::write(
        root.path().join("one.rs"),
        "fn run(x: &Thing) { x.release_objects(); }\n",
    )?;
    kernel.index()?;
    assert_eq!(kernel.code_facts(&query)?.facts.len(), 2);
    let reopened = Kernel::open(root.path(), Some(state.path().into()))?;
    assert_eq!(reopened.code_facts(&query)?.facts.len(), 2);
    Ok(())
}

#[test]
fn call_candidates_are_available_at_each_structured_detail() -> anyhow::Result<()> {
    for detail in [
        airk_core::extraction::IndexDetail::AgentLean,
        airk_core::extraction::IndexDetail::Balanced,
        airk_core::extraction::IndexDetail::Detailed,
        airk_core::extraction::IndexDetail::Essential,
    ] {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        fs::write(
            root.path().join("a.ts"),
            "class Auth { run() { db.query('x'); } }\n",
        )?;
        let kernel = Kernel::open_collection(root.path(), Some(state.path().into()), "fixture")?;
        kernel.configure_index_detail(detail)?;
        kernel.index()?;
        let facts = kernel.code_facts(&fact_query(Some("db.query"), Some("call")))?;
        let relationships = kernel.relationships(&edges("db.query", Some("call")))?;
        let expected = usize::from(detail != airk_core::extraction::IndexDetail::Essential);
        assert_eq!(facts.facts.len(), expected, "{detail:?}");
        assert_eq!(relationships.relationships.len(), expected, "{detail:?}");
        if let Some(edge) = relationships.relationships.first() {
            assert_eq!(edge.resolution, "syntax_candidate");
        }
    }
    Ok(())
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
    let projection_store = FileStore::new(state.path().join("collections-1.2/default"));
    super::current_inspection::assert_import_projection_parity(&projection_store)?;
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
    super::current_inspection::assert_import_projection_parity(&projection_store)?;
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
