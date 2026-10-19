// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::current::postings::Lookup;

fn fact(name: &str, kind: &str) -> airk_core::extraction::LocalFact {
    airk_core::extraction::LocalFact {
        id: 0,
        owner: None,
        kind: kind.into(),
        name: name.into(),
        span: airk_core::extraction::LocalSpan { start: 0, end: 4 },
        syntax_kind: "heading".into(),
        start_line: 1,
        end_line: 1,
        location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
        document: None,
        code: None,
        region: None,
        attributes: Default::default(),
    }
}

#[test]
fn catalog_postings_follow_replacement_deletion_and_exact_case() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = retained_input_with_facts(
        &fixture,
        "src/a.txt",
        "body",
        vec![fact("Alpha", "heading")],
    )?;
    let second = retained_input_with_facts(
        &fixture,
        "src/b.txt",
        "body",
        vec![fact("Alpha", "heading")],
    )?;
    let changed =
        retained_input_with_facts(&fixture, "src/a.txt", "body", vec![fact("Beta", "label")])?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let first_commit =
        current.commit_artifact(&fixture.head.token()?, &first.artifact.path, Some(&first))?;
    let both =
        current.commit_artifact(&first_commit.token, &second.artifact.path, Some(&second))?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(
        read.lookup_paths(Lookup::FactName("Alpha"), "", None, 10)?
            .paths,
        ["src/a.txt", "src/b.txt"]
    );
    assert!(
        read.lookup_paths(Lookup::FactName("alpha"), "", None, 10)?
            .paths
            .is_empty()
    );
    assert_eq!(
        read.lookup_paths(Lookup::Language("text"), "src/b", None, 1)?
            .paths,
        ["src/b.txt"]
    );
    let page = read.lookup_paths(Lookup::Files, "", None, 1)?;
    assert_eq!(page.paths, ["src/a.txt"]);
    assert_eq!(page.next_after.as_deref(), Some("src/a.txt"));
    let last = read.lookup_paths(Lookup::Files, "", page.next_after.as_deref(), 1)?;
    assert_eq!(last.paths, ["src/b.txt"]);
    assert!(last.next_after.is_none());
    assert!(read.lookup_paths(Lookup::Files, "", None, 0).is_err());
    assert!(
        read.lookup_paths(Lookup::Files, "", Some("../x"), 1)
            .is_err()
    );
    drop(read);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let replacement =
        current.commit_artifact(&both.token, &changed.artifact.path, Some(&changed))?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.require_token(&both.token).is_err());
    for lookup in [Lookup::FactName("Alpha"), Lookup::FactKind("heading")] {
        assert_eq!(
            read.lookup_paths(lookup, "", None, 10)?.paths,
            ["src/b.txt"]
        );
    }
    for lookup in [Lookup::FactName("Beta"), Lookup::FactKind("label")] {
        assert_eq!(
            read.lookup_paths(lookup, "", None, 10)?.paths,
            ["src/a.txt"]
        );
    }
    drop(read);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let deleted = current.commit_artifact(&replacement.token, &second.artifact.path, None)?;
    current.commit_artifact(&deleted.token, &changed.artifact.path, None)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.lookup_paths(Lookup::Files, "", None, 10)?
            .paths
            .is_empty()
    );
    assert!(
        read.lookup_paths(Lookup::FactName("Alpha"), "", None, 10)?
            .paths
            .is_empty()
    );
    assert!(
        read.lookup_paths(Lookup::FactName("Beta"), "", None, 10)?
            .paths
            .is_empty()
    );
    assert!(read.map_entries(map::MapVisitLimits::default())?.is_empty());
    Ok(())
}

#[test]
fn postings_replay_with_artifacts_and_remain_unchanged_on_failed_prepare() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input_with_facts(
        &fixture,
        "evidence.txt",
        "body",
        vec![fact("Alpha", "heading")],
    )?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let changes = [ArtifactChange::Upsert(Box::new(input.clone()))];
    assert!(
        current
            .prepare_artifacts(
                &fixture.head.token()?,
                fixture.target(&sealed),
                &sealed,
                &changes,
                None,
                b"invalid json"
            )
            .is_err()
    );
    assert!(current.pending()?.is_none());
    let pending = current.prepare_artifacts(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &changes,
        None,
        br#"{"version":1}"#,
    )?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.lookup_paths(Lookup::FactName("Alpha"), "", None, 10)?
            .paths
            .is_empty()
    );
    drop(read);
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
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(
        read.lookup_paths(Lookup::FactName("Alpha"), "", None, 10)?
            .paths,
        ["evidence.txt"]
    );
    assert_eq!(read.token()?, pending.target.token()?);
    Ok(())
}

fn query() -> airk_core::FactQuery {
    airk_core::FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 1,
        max_bytes: 16_384,
        path_exact: false,
        document_only: false,
    }
}

#[test]
fn fact_pages_bind_filters_revision_and_retained_evidence() -> Result<()> {
    use airk_core::discovery::DiscoveryOptions;
    let fixture = Fixture::new()?;
    let mut facts = vec![
        fact("Zulu", "heading"),
        fact("Alpha", "heading"),
        fact("Beta", "label"),
    ];
    for (id, fact) in facts.iter_mut().enumerate() {
        fact.id = id as u32;
    }
    let input = retained_input_with_facts(&fixture, "evidence.txt", "body", facts)?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let committed =
        current.commit_artifact(&fixture.head.token()?, &input.artifact.path, Some(&input))?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    let mut q = query();
    let first = read.native_facts_page(&fixture.store, &q, false, &Default::default())?;
    assert_eq!(first.revision, committed.token);
    assert_eq!(first.facts[0].name, "Alpha");
    assert!(first.truncated);
    let options = DiscoveryOptions {
        cursor: first.page.cursor,
        compact: false,
    };
    let second = read.native_facts_page(&fixture.store, &q, false, &options)?;
    assert_eq!(second.facts[0].name, "Zulu");
    let third = read.native_facts_page(
        &fixture.store,
        &q,
        false,
        &DiscoveryOptions {
            cursor: second.page.cursor,
            compact: false,
        },
    )?;
    assert_eq!(third.facts[0].name, "Beta");
    assert!(!third.truncated);
    q.name = Some("lP".into());
    assert!(
        read.native_facts_page(&fixture.store, &q, false, &options)
            .is_err()
    );
    assert_eq!(
        read.native_facts_page(&fixture.store, &q, false, &Default::default())?
            .facts[0]
            .name,
        "Alpha"
    );
    q.exact_name = true;
    assert!(
        read.native_facts_page(&fixture.store, &q, false, &Default::default())?
            .facts
            .is_empty()
    );
    q.name = Some("Alpha".into());
    assert_eq!(
        read.native_facts_page(&fixture.store, &q, false, &Default::default())?
            .facts
            .len(),
        1
    );
    q = query();
    q.max_bytes = 1;
    let small = read.native_facts_page(&fixture.store, &q, false, &Default::default())?;
    assert!(small.truncated && small.facts.is_empty());
    assert!(small.page.required_record_bytes.unwrap() > 1);
    q = query();
    q.path_exact = true;
    q.path_prefix = "evidence.txt".into();
    let compact = read.native_facts_page(
        &fixture.store,
        &q,
        false,
        &DiscoveryOptions {
            cursor: None,
            compact: true,
        },
    )?;
    assert_eq!(compact.outline[0].name, "Alpha");
    assert!(compact.facts.is_empty());
    assert!(
        read.native_facts_page(&fixture.store, &q, true, &Default::default())?
            .facts
            .is_empty()
    );
    drop(read);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.commit_artifact(&committed.token, &input.artifact.path, None)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.native_facts_page(&fixture.store, &query(), false, &options)
            .is_err()
    );
    Ok(())
}

#[test]
fn native_fact_page_rejects_retained_source_corruption_without_live_fallback() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input_with_facts(
        &fixture,
        "evidence.txt",
        "body",
        vec![fact("Alpha", "heading")],
    )?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.commit_artifact(&fixture.head.token()?, &input.artifact.path, Some(&input))?;
    drop(current);
    drop(writer);
    fs::write(
        std::path::Path::new(&fixture.scope.root).join("evidence.txt"),
        b"body",
    )?;
    fs::write(
        fixture
            .store
            .directory()
            .join("objects/sources")
            .join(&input.artifact.content_hash),
        b"evil",
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.native_facts_page(&fixture.store, &query(), false, &Default::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn qualified_names_ids_and_relationships_share_current_native_bindings() -> Result<()> {
    use airk_core::extraction::{LocalCodeEvidence, LocalSyntaxId};
    let fixture = Fixture::new()?;
    let mut caller = fact("caller", "definition");
    caller.code = Some(LocalCodeEvidence {
        fingerprints: None,
        syntax_id: LocalSyntaxId(0),
        owner_id: None,
        qualified_name: "module.caller".into(),
        scope: vec!["module".into()],
        signature: None,
        span: None,
        signature_span: None,
        parameters: None,
        return_type: None,
        modifiers: Vec::new(),
        documentation: None,
        target: None,
        basis: airk_core::EvidenceBasis::Syntax,
        producer: "test-native".into(),
        producer_version: "1".into(),
        coverage: "written syntax".into(),
        details_truncated: false,
    });
    let mut call = caller.clone();
    call.id = 1;
    call.name = "callee".into();
    call.kind = "call".into();
    call.syntax_kind = "call".into();
    let code = call.code.as_mut().unwrap();
    code.syntax_id = LocalSyntaxId(1);
    code.owner_id = Some(LocalSyntaxId(0));
    code.target = Some("callee".into());
    code.qualified_name = "module.caller.callee".into();
    let input = retained_input_with_facts(&fixture, "src/a.txt", "body", vec![caller, call])?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.commit_artifact(&fixture.head.token()?, &input.artifact.path, Some(&input))?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    let mut q = query();
    q.name = Some("module.caller".into());
    q.exact_name = true;
    let found = read.native_facts_page(&fixture.store, &q, true, &Default::default())?;
    let symbol = found.facts[0].code.as_ref().unwrap().symbol_id.clone();
    q.name = Some(symbol.clone());
    assert_eq!(
        read.native_facts_page(&fixture.store, &q, true, &Default::default())?
            .facts[0]
            .name,
        "caller"
    );
    let mut q = airk_core::RelationshipQuery {
        symbol,
        direction: "outgoing".into(),
        kind: None,
        path_prefix: "src/".into(),
        hops: 1,
        limit: 10,
        max_bytes: 16_384,
    };
    let outgoing = read.native_relationships(&fixture.store, &q)?;
    assert_eq!(outgoing.relationships.len(), 2);
    assert!(!outgoing.truncated);
    assert!(
        outgoing
            .relationships
            .iter()
            .any(|edge| edge.kind == "contains")
    );
    assert!(
        outgoing
            .relationships
            .iter()
            .any(|edge| edge.kind == "call" && edge.target == "callee")
    );
    q.kind = Some("call".into());
    q.symbol = "callee".into();
    q.direction = "incoming".into();
    let incoming = read.native_relationships(&fixture.store, &q)?;
    assert_eq!(incoming.relationships.len(), 1);
    assert_eq!(incoming.relationships[0].citation.path, "src/a.txt");
    q.max_bytes = 1;
    let limited = read.native_relationships(&fixture.store, &q)?;
    assert!(limited.relationships.is_empty() && limited.truncated);
    q.direction = "invalid".into();
    assert!(read.native_relationships(&fixture.store, &q).is_err());
    Ok(())
}

#[test]
fn empty_fact_pages_make_bounded_progress_without_skipping_later_matches() -> Result<()> {
    let fixture = Fixture::new()?;
    let inputs = (0..258)
        .map(|index| {
            retained_input_with_facts(
                &fixture,
                &format!("src/{index:03}.txt"),
                "body",
                if index == 257 {
                    vec![fact("last", "heading")]
                } else {
                    Vec::new()
                },
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let changes = inputs
        .into_iter()
        .map(|input| ArtifactChange::Upsert(Box::new(input)))
        .collect::<Vec<_>>();
    let pending = current.prepare_artifacts(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &changes,
        None,
        br#"{"version":1}"#,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    let first = read.native_facts_page(&fixture.store, &query(), false, &Default::default())?;
    assert!(first.facts.is_empty() && first.truncated);
    assert!(first.page.cursor.is_some());
    let last = read.native_facts_page(
        &fixture.store,
        &query(),
        false,
        &airk_core::discovery::DiscoveryOptions {
            cursor: first.page.cursor,
            compact: false,
        },
    )?;
    assert_eq!(last.facts[0].name, "last");
    assert_eq!(last.facts[0].citation.path, "src/257.txt");
    assert!(!last.truncated && last.page.cursor.is_none());
    Ok(())
}
