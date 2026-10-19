// SPDX-License-Identifier: Apache-2.0
//! Qualifies real DiskANN algorithms across the authenticated store/journal boundary.
use airk_core::{
    graph_storage::{GraphHead, GraphRecordReader, GraphScope, PreparedGraphDelta},
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use airk_retrieval::vector::mutable::{self, MutableDiskIndex, SearchLimits};
use airk_store::{
    FileStore,
    current::{
        CurrentHead, CurrentScope, PendingTransaction,
        graph::{GraphOverlay, GraphOverlayLimits, SealedGraph},
    },
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, sync::Arc, time::Instant};

const DIMENSIONS: usize = 8;
const CAPACITY: u64 = 16 * 1024 * 1024;

fn pipeline() -> Sha256Digest {
    framed_digest(b"current-foundation-original-fixture/1", &[])
}
fn key(id: u32) -> Sha256Digest {
    framed_digest(b"current-foundation-input/1", &[&id.to_be_bytes()])
}
fn vector(id: u32) -> Vec<f32> {
    let mut state = u64::from(id) + 0x1234_5678;
    let mut vector: Vec<f32> = (0..DIMENSIONS)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state % 20_001) as f32 - 10_000.0) / 10_000.0
        })
        .collect();
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    for value in &mut vector {
        *value /= norm;
    }
    vector
}
fn neighbors(query: &[f32], ids: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut distances: Vec<_> = ids
        .map(|id| {
            let distance = query
                .iter()
                .zip(vector(id))
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>();
            (id, distance)
        })
        .collect();
    distances.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    distances.into_iter().map(|(id, _)| id).collect()
}

struct Fixture {
    _directory: tempfile::TempDir,
    quota: Quota,
    store: FileStore,
    scope: CurrentScope,
}
impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let quota = Quota::open(directory.path().join("state"))?;
        let store = FileStore::new(quota.root().join("collections/foundation"));
        let scope = CurrentScope {
            collection: "foundation".into(),
            root: directory
                .path()
                .join("original-source")
                .to_string_lossy()
                .into_owned(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let graph_scope = GraphScope {
            collection: scope.collection.clone(),
            root: scope.root.clone(),
            incarnation: scope.incarnation,
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: mutable::specification(DIMENSIONS, pipeline())?,
            record_bytes: mutable::record_bytes(DIMENSIONS)?,
        };
        let head = CurrentHead {
            version: 1,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: airk_store::current::map::empty_root(&scope)?,
            lexical_commit: framed_digest(b"foundation-no-lexical/1", &[]),
            graph_head: GraphHead::empty(graph_scope)?,
        };
        let lock = store.lock()?;
        store
            .current_write(&lock, &quota, &scope, CAPACITY)?
            .initialize(head)?;
        drop(lock);
        Ok(Self {
            _directory: directory,
            quota,
            store,
            scope,
        })
    }
    fn candidate(&self) -> Result<(Reservation, CurrentHead, Arc<GraphOverlay>)> {
        let reservation = self
            .quota
            .reserve(&self.scope.collection, 8 * 1024 * 1024)?;
        let read = self.store.current_read(&self.scope)?;
        let head = read.head().clone();
        let overlay = Arc::new(GraphOverlay::new(
            &reservation,
            read,
            GraphOverlayLimits {
                maximum_bytes: 4 * 1024 * 1024,
                ..Default::default()
            },
        )?);
        Ok((reservation, head, overlay))
    }
    fn seal(overlay: Arc<GraphOverlay>) -> Result<SealedGraph> {
        Arc::try_unwrap(overlay)
            .map_err(|_| anyhow::anyhow!("DiskANN retained its overlay"))?
            .seal()
    }
    fn prepare(&self, head: &CurrentHead, graph: &SealedGraph) -> Result<PendingTransaction> {
        let mut target = head.clone();
        target.sequence += 1;
        target.graph_head = graph.descriptor().target.clone();
        let lock = self.store.lock()?;
        self.store
            .current_write(&lock, &self.quota, &self.scope, CAPACITY)?
            .prepare(&head.token()?, target, graph, &[], None)
    }
    fn publish(&self, head: &CurrentHead, graph: &SealedGraph) -> Result<CurrentHead> {
        let pending = self.prepare(head, graph)?;
        let lock = self.store.lock()?;
        let mut current = self
            .store
            .current_write(&lock, &self.quota, &self.scope, CAPACITY)?;
        current.mark_applying(&pending)?;
        current.commit(&pending, pending.target.lexical_commit)?;
        current.head()
    }
    fn populate(&self, count: u32) -> Result<CurrentHead> {
        let (_reservation, head, overlay) = self.candidate()?;
        let mut graph = MutableDiskIndex::create(overlay.clone(), DIMENSIONS, pipeline())?;
        for id in 1..=count {
            assert_eq!(graph.insert(key(id), &vector(id))?, id);
        }
        drop(graph);
        let sealed = Self::seal(overlay)?;
        self.publish(&head, &sealed)
    }
}

#[test]
fn real_diskann_reopens_appends_and_changes_one_record_without_copying_the_graph() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = fixture.populate(96)?;
    let (_reservation, head, overlay) = fixture.candidate()?;
    assert_eq!(head, first);
    let mut graph = MutableDiskIndex::open_overlay(overlay.clone(), DIMENSIONS, pipeline())?;
    for id in 97..=106 {
        assert_eq!(graph.insert(key(id), &vector(id))?, id);
    }
    drop(graph);
    let sealed = Fixture::seal(overlay)?;
    let second = fixture.publish(&head, &sealed)?;
    drop(sealed);
    drop(_reservation);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.require_token(&first.token()?).is_err());
    for id in 1..=106 {
        assert_eq!(read.read(id)?.embedding_key, Some(key(id)));
    }
    let graph = MutableDiskIndex::open(read.clone(), DIMENSIONS, pipeline())?;
    for id in [3, 27, 74] {
        let query = vector(id);
        let wanted: BTreeSet<_> = neighbors(&query, 1..=106).into_iter().take(10).collect();
        let found = graph.search(&query, 10)?;
        assert!(!found.partial);
        assert!(
            found
                .hits
                .iter()
                .filter(|(id, _)| wanted.contains(id))
                .count()
                >= 9
        );
    }
    let partial = graph.search_with_limits(
        &vector(3),
        10,
        SearchLimits {
            record_reads: 3,
            candidates: 64,
        },
    )?;
    assert!(partial.partial && partial.record_reads <= 3);
    drop(graph);
    drop(read);

    let (_reservation, head, overlay) = fixture.candidate()?;
    assert_eq!(head, second);
    let mut graph = MutableDiskIndex::open_overlay(overlay.clone(), DIMENSIONS, pipeline())?;
    graph.set_live(3, key(3), false)?;
    drop(graph);
    let sealed = Fixture::seal(overlay)?;
    assert_eq!(sealed.descriptor().record_count, 1);
    assert_eq!(sealed.descriptor().node_count, 33);
    assert_eq!(sealed.descriptor().target.allocated_count, 107);
    assert_eq!(sealed.descriptor().target.live_nodes, 105);
    fixture.publish(&head, &sealed)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let graph = MutableDiskIndex::open(read, DIMENSIONS, pipeline())?;
    assert!(
        graph
            .search(&vector(3), 10)?
            .hits
            .iter()
            .all(|(id, _)| *id != 3)
    );
    Ok(())
}

#[test]
fn captured_diskann_afterimages_recover_after_private_state_is_gone() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.populate(96)?;
    let (reservation, head, overlay) = fixture.candidate()?;
    let mut graph = MutableDiskIndex::open_overlay(overlay.clone(), DIMENSIONS, pipeline())?;
    for id in 97..=106 {
        assert_eq!(graph.insert(key(id), &vector(id))?, id);
    }
    let query = vector(3);
    let dead: BTreeSet<_> = neighbors(&query, 1..=106).into_iter().take(80).collect();
    for &id in &dead {
        graph.set_live(id, key(id), false)?;
    }
    drop(graph);
    let sealed = Fixture::seal(overlay)?;
    let expected_graph = sealed.descriptor().target.clone();
    let pending = fixture.prepare(&head, &sealed)?;
    drop(sealed);
    drop(reservation);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.head(), &head);
    {
        let lock = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        current.mark_applying(&pending)?;
    }
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    // Reopen with no private candidate or DiskANN writer. Recovery copies exactly
    // the captured afterimages; a second attempt cannot allocate duplicate IDs.
    {
        let lock = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        let recovered = current.pending()?.context("lost prepared graph")?;
        assert_eq!(recovered, pending);
        current.mark_applying(&recovered)?;
        current.commit(&recovered, recovered.target.lexical_commit)?;
        assert!(current.pending()?.is_none());
        assert!(
            current
                .commit(&recovered, recovered.target.lexical_commit)
                .is_err()
        );
    }
    assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.head().graph_head, expected_graph);
    let graph = MutableDiskIndex::open(read.clone(), DIMENSIONS, pipeline())?;
    let found = graph.search(&query, 10)?;
    ensure!(
        !found.partial && found.hits.len() == 10 && found.passes > 1,
        "live refill failed: {found:?}"
    );
    assert!(found.hits.iter().all(|(id, _)| !dead.contains(id)));
    drop(graph);
    drop(read);

    let (_reservation, head, overlay) = fixture.candidate()?;
    let mut graph = MutableDiskIndex::open_overlay(overlay.clone(), DIMENSIONS, pipeline())?;
    graph.set_live(3, key(3), true)?;
    drop(graph);
    fixture.publish(&head, &Fixture::seal(overlay)?)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.counts()?.allocated_count, 107);
    let graph = MutableDiskIndex::open(read, DIMENSIONS, pipeline())?;
    assert_eq!(graph.search(&query, 1)?.hits[0].0, 3);
    Ok(())
}

#[test]
#[ignore = "release timing qualification; run without competing builds or tests"]
fn authenticated_mutable_diskann_workload_measurement() -> Result<()> {
    for sample in 0..5 {
        let fixture = Fixture::new()?;
        let started = Instant::now();
        fixture.populate(512)?;
        let initial_us = started.elapsed().as_micros();
        for operation in ["append-10-percent", "delete-one"] {
            let started = Instant::now();
            let (reservation, head, overlay) = fixture.candidate()?;
            let mut graph =
                MutableDiskIndex::open_overlay(overlay.clone(), DIMENSIONS, pipeline())?;
            if operation == "append-10-percent" {
                for id in 513..=563 {
                    assert_eq!(graph.insert(key(id), &vector(id))?, id);
                }
            } else {
                graph.set_live(17, key(17), false)?;
            }
            let io = graph.io_statistics();
            drop(graph);
            let sealed = Fixture::seal(overlay)?;
            let preparation_us = started.elapsed().as_micros();
            let publish_started = Instant::now();
            fixture.publish(&head, &sealed)?;
            let publication_us = publish_started.elapsed().as_micros();
            println!(
                "{}",
                serde_json::json!({"sample":sample,"operation":operation,
                "initial_us":initial_us,"preparation_us":preparation_us,"publication_us":publication_us,
                "record_reads":io.record_reads,"record_writes":io.record_writes,
                "store":sealed.statistics()})
            );
            drop(sealed);
            drop(reservation);
        }
    }
    Ok(())
}
