// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::index_catalog::PathScope;
use airk_core::scoped_reads::{CompiledPathAtom, CompiledReadScope, ScopeReadRefused};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn publish(fault: &str) -> Result<Fixture> {
    let f = Fixture::new()?;
    let source = retained_input(&f, "Original occurrence source.")?;
    let (_reservation, graph) = f.sealed()?;
    let mut target = f.target(&graph);
    let key = Sha256Digest::from_bytes([6; 32]);
    let foreign = Sha256Digest::from_bytes([99; 32]);
    let mut ptr = prepared_occurrence(&f, &source, key, 1)?.pointer;
    let id = ptr.occurrence_id;
    let path = ptr.path.clone();
    let mut paths = vector_map::PathOccurrences {
        version: 1,
        path: path.clone(),
        source_hash: ptr.source_hash,
        count: 1,
    };
    let mut binding = vector_map::VectorBinding {
        version: 1,
        embedding_key: key,
        graph_scope: target.graph_head.scope.digest(),
        graph_id: 1,
        occurrence_ids: vec![id],
    };
    match fault {
        "foreign_key" => binding.embedding_key = foreign,
        "foreign_graph" => binding.graph_scope = foreign,
        "other_id" => binding.graph_id = 0,
        "absent_id" => binding.graph_id = 99,
        "empty_binding" => binding.occurrence_ids.clear(),
        "foreign_occurrence" => ptr.occurrence_id = foreign,
        "foreign_vector" => ptr.embedding_key = foreign,
        "wrong_ordinal" => ptr.path_ordinal = 1,
        "foreign_source" => paths.source_hash = foreign,
        "valid" | "missing_binding" | "missing_occurrence" | "missing_path" => {}
        _ => anyhow::bail!("unknown test fixture"),
    }
    let specs = [
        (
            map::MapDomain::Vector,
            key.as_bytes().as_slice(),
            serde_json::to_vec(&binding)?,
            "missing_binding",
        ),
        (
            map::MapDomain::Occurrence,
            id.as_bytes().as_slice(),
            serde_json::to_vec(&ptr)?,
            "missing_occurrence",
        ),
        (
            map::MapDomain::PathOccurrences,
            path.as_bytes(),
            serde_json::to_vec(&paths)?,
            "missing_path",
        ),
    ];
    let mut changes = vec![map::MapChange::new(
        &f.scope,
        map::MapDomain::Artifact,
        path.as_bytes(),
        Some(source.bytes()?),
    )?];
    for (domain, name, bytes, absent) in specs {
        if fault != absent {
            changes.push(map::MapChange::new(&f.scope, domain, name, Some(bytes))?);
        }
    }
    changes.sort_by_key(map::MapChange::key);
    let db = database::open_reader(&f.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&db, &f.scope, f.head.artifact_root)?;
    for change in &changes {
        plan.put(change.key(), change.value())?;
    }
    target.artifact_root = plan.root()?;
    drop(db);
    let lock = f.store.lock()?;
    let mut write = f.store.current_write(&lock, &f.quota, &f.scope, CAPACITY)?;
    let empty: [CapturedInput; 0] = [];
    let transaction =
        write.prepare_mapped(&f.head.token()?, target, &graph, &empty, None, &changes)?;
    write.mark_applying(&transaction)?;
    write.commit(&transaction, transaction.target.lexical_commit)?;
    drop(write);
    drop(lock);
    Ok(f)
}

// Original caller's graph-to-key lookup, graph recheck, then key-to-pointer lookup.
fn reference(read: &read::CurrentRead, id: u32) -> Result<Vec<vector_map::OccurrencePointer>> {
    let row = read.read(id)?;
    ensure!(row.live, "current graph candidate is not live");
    let key = row
        .embedding_key
        .context("current graph candidate has no key")?;
    let mapped = read
        .vector_binding(key)?
        .context("live graph candidate has no current vector binding")?;
    ensure!(mapped.graph_id == id, "current graph candidate ID mismatch");
    read.vector_occurrence_pointers(mapped.embedding_key)?
        .context("current candidate occurrences missing")
}

#[test]
fn authenticated_graph_candidate_refuses_every_broken_link_with_original_error() -> Result<()> {
    for fault in [
        "missing_binding",
        "foreign_key",
        "foreign_graph",
        "other_id",
        "absent_id",
        "empty_binding",
        "missing_occurrence",
        "foreign_occurrence",
        "foreign_vector",
        "wrong_ordinal",
        "missing_path",
        "foreign_source",
    ] {
        let fixture = publish(fault)?;
        let old = reference(fixture.store.current_read(&fixture.scope)?.as_ref(), 1).unwrap_err();
        let new = fixture
            .store
            .current_read(&fixture.scope)?
            .graph_candidate_pointers(1)
            .unwrap_err();
        assert_eq!(format!("{old:#}"), format!("{new:#}"), "{fault}");
    }
    Ok(())
}

#[test]
fn authenticated_graph_candidate_preserves_order_restriction_revocation_and_tombstone() -> Result<()>
{
    let fixture = publish("valid")?;
    let reader = fixture.store.current_read(&fixture.scope)?;
    let expected = reference(&reader, 1)?;
    assert_eq!(expected.len(), 1);
    assert_eq!(reader.graph_candidate_pointers(1)?, expected);
    assert_eq!(
        reader.graph_candidate_pointers(0).unwrap_err().to_string(),
        "current graph candidate is not live"
    );
    let revoked = Arc::new(AtomicBool::new(false));
    reader.attach_read_scope_fence(
        Arc::new(CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([49; 32]),
            atoms: vec![CompiledPathAtom {
                kind: PathScope::Exact,
                path: "different.txt".into(),
            }],
            source_atoms: vec![],
            external_aliases: vec![],
        }),
        revoked.clone(),
    )?;
    assert!(reference(&reader, 1)?.is_empty());
    assert!(reader.graph_candidate_pointers(1)?.is_empty());
    revoked.store(true, Ordering::Release);
    assert!(
        reader
            .graph_candidate_pointers(1)
            .unwrap_err()
            .downcast_ref::<ScopeReadRefused>()
            .is_some()
    );
    Ok(())
}
