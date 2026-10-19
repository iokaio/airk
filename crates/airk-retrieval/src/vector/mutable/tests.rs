// SPDX-License-Identifier: Apache-2.0
//! Small fault-injecting storage fixture; these tests still execute real DiskANN.
//! Durable authenticated storage/replay is qualified separately by kernel integration.
use super::*;
use airk_core::{graph_storage::GraphScope, identity::Incarnation};
use std::sync::Mutex;

const PIPELINE: Sha256Digest = Sha256Digest::from_bytes([3; 32]);

struct Fixture {
    scope: GraphScope,
    state: Mutex<State>,
    poisoned: AtomicBool,
}

#[derive(Default)]
struct State {
    records: Vec<GraphRecord>,
    writes: usize,
    fail_at: Option<usize>,
}

impl Fixture {
    fn new(dimensions: usize) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            scope: GraphScope {
                collection: "fixture".into(),
                root: "fixture-root".into(),
                incarnation: Incarnation::from_bytes([1; 16]),
                graph_instance: Incarnation::from_bytes([2; 16]),
                specification: specification(dimensions, PIPELINE)?,
                record_bytes: record_bytes(dimensions)?,
            },
            state: Mutex::new(State::default()),
            poisoned: AtomicBool::new(false),
        }))
    }

    fn mutate(&self, change: impl FnOnce(&mut State) -> Result<()>) -> Result<()> {
        let result = (|| {
            ensure!(!self.poisoned.load(Ordering::Relaxed), "fixture poisoned");
            let mut state = self.state.lock().unwrap();
            ensure!(
                state.fail_at != Some(state.writes),
                "injected after-image write failure"
            );
            change(&mut state)?;
            state.writes += 1;
            Ok(())
        })();
        if result.is_err() {
            self.poison();
        }
        result
    }
}

impl GraphRecordReader for Fixture {
    fn scope(&self) -> &GraphScope {
        &self.scope
    }
    fn counts(&self) -> Result<GraphCounts> {
        ensure!(!self.poisoned.load(Ordering::Relaxed), "fixture poisoned");
        let state = self.state.lock().unwrap();
        Ok(GraphCounts {
            allocated_count: state.records.len() as u64,
            live_nodes: state.records.iter().filter(|record| record.live).count() as u64,
        })
    }
    fn read(&self, id: u32) -> Result<GraphRecord> {
        ensure!(!self.poisoned.load(Ordering::Relaxed), "fixture poisoned");
        self.state
            .lock()
            .unwrap()
            .records
            .get(id as usize)
            .cloned()
            .context("unknown fixture ID")
    }
}

impl GraphRecordOverlay for Fixture {
    fn append(&self, id: u32, record: GraphRecord) -> Result<()> {
        self.mutate(|state| {
            ensure!(
                id as usize == state.records.len(),
                "nonsequential fixture ID"
            );
            record.validate(&self.scope, id)?;
            state.records.push(record);
            Ok(())
        })
    }
    fn put(&self, id: u32, record: GraphRecord) -> Result<()> {
        self.mutate(|state| {
            record.validate(&self.scope, id)?;
            let previous = state
                .records
                .get_mut(id as usize)
                .context("unknown fixture ID")?;
            ensure!(
                previous.embedding_key == record.embedding_key,
                "fixture key reassignment"
            );
            *previous = record;
            Ok(())
        })
    }
    fn set_live(&self, id: u32, live: bool) -> Result<()> {
        self.mutate(|state| {
            ensure!(id != 0, "reserved fixture ID");
            state
                .records
                .get_mut(id as usize)
                .context("unknown fixture ID")?
                .live = live;
            Ok(())
        })
    }
    fn poison(&self) {
        self.poisoned.store(true, Ordering::Relaxed);
    }
}

fn key(id: u32) -> Sha256Digest {
    let mut bytes = [0; 32];
    bytes[..4].copy_from_slice(&id.to_be_bytes());
    Sha256Digest::from_bytes(bytes)
}

#[test]
fn actual_diskann_incremental_ids_reopen_and_recall() -> Result<()> {
    let store = Fixture::new(8)?;
    let mut index = MutableDiskIndex::create(store.clone(), 8, PIPELINE)?;
    assert_eq!(
        index.counts()?,
        GraphCounts {
            allocated_count: 1,
            live_nodes: 0
        }
    );
    let mut vectors = Vec::new();
    let mut seed = 7u64;
    for id in 1..=200 {
        let vector: Vec<_> = (0..8)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((seed >> 40) as f32) / 16_777_216.0
            })
            .collect();
        assert_eq!(index.insert(key(id), &vector)?, id);
        assert_eq!(store.read(id)?.embedding_key, Some(key(id)));
        vectors.push(vector);
        if id == 180 {
            drop(index);
            index = MutableDiskIndex::open_overlay(store.clone(), 8, PIPELINE)?;
        }
    }
    drop(index);
    let mut reader = MutableDiskIndex::open(store.clone(), 8, PIPELINE)?;
    assert_eq!(
        reader.io_statistics().record_reads,
        1,
        "open only reads reserved seed"
    );
    let mut recalled = 0;
    for offset in 0..20 {
        let query: Vec<_> = vectors[offset * 7]
            .iter()
            .enumerate()
            .map(|(i, value)| value + (i as f32 - 3.0) * 0.011)
            .collect();
        let mut exact: Vec<_> = vectors
            .iter()
            .enumerate()
            .map(|(i, vector)| (i as u32 + 1, L2.evaluate_similarity(&query, vector)))
            .collect();
        exact.sort_by(|a, b| a.1.total_cmp(&b.1));
        let result = reader.search(&query, 10)?;
        assert!(!result.partial);
        assert_eq!(result.hits.len(), 10);
        recalled += result
            .hits
            .iter()
            .filter(|hit| exact[..10].iter().any(|e| e.0 == hit.0))
            .count();
    }
    assert!(
        recalled >= 180,
        "synthetic recall@10 fell below90%: {recalled}/200"
    );
    assert!(
        reader.insert(key(201), &[0.0; 8]).is_err(),
        "committed adapter must reject writes"
    );
    assert!(
        !store.poisoned.load(Ordering::Relaxed),
        "read-only adapter cannot poison storage"
    );
    Ok(())
}

#[test]
fn dead_nodes_remain_traversable_refill_and_same_key_resurrection() -> Result<()> {
    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    for id in 1..=240 {
        index.insert(key(id), &[id as f32 / 240.0, 0.1])?;
    }
    for id in 1..=180 {
        index.set_live(id, key(id), false)?;
    }
    index.set_live(1, key(1), false)?;
    assert_eq!(
        index.counts()?,
        GraphCounts {
            allocated_count: 241,
            live_nodes: 60
        }
    );
    drop(index);
    let reader = MutableDiskIndex::open(store.clone(), 2, PIPELINE)?;
    let result = reader.search(&[0.0, 0.1], 10)?;
    assert!(!result.partial);
    assert_eq!(result.hits.len(), 10);
    assert!(result.hits.iter().all(|hit| hit.0 > 180));
    assert!(result.passes > 1, "dead nearest nodes must require refill");
    let capped = reader.search_with_limits(
        &[0.0, 0.1],
        10,
        SearchLimits {
            candidates: 64,
            ..Default::default()
        },
    )?;
    assert!(capped.partial);
    assert!(capped.hits.len() < 10);
    let across_passes = reader.search_with_limits(
        &[0.0, 0.1],
        10,
        SearchLimits {
            record_reads: capped.record_reads as usize + 1,
            ..Default::default()
        },
    )?;
    assert!(across_passes.partial && across_passes.passes > 1);
    assert_eq!(
        across_passes.record_reads,
        capped.record_reads + 1,
        "refill must share the original request's read budget"
    );
    drop(reader);
    let mut index = MutableDiskIndex::open_overlay(store.clone(), 2, PIPELINE)?;
    index.set_live(1, key(1), true)?;
    assert_eq!(index.search(&[0.0, 0.1], 1)?.hits[0].0, 1);
    index.set_live(1, key(1), false)?;
    for id in 181..=240 {
        index.set_live(id, key(id), false)?;
    }
    let empty = index.search(&[0.0, 0.1], 10)?;
    assert!(empty.hits.is_empty() && !empty.partial);
    assert_eq!(empty.record_reads, 0);
    assert_eq!(
        index.insert(key(241), &[1.1, 0.1])?,
        241,
        "deleted IDs are never reused"
    );
    Ok(())
}

#[test]
fn shared_record_budget_includes_adjacency_and_filtering() -> Result<()> {
    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    for id in 1..=12 {
        index.insert(key(id), &[id as f32, 0.5])?;
    }
    drop(index);
    let reader = MutableDiskIndex::open(store, 2, PIPELINE)?;
    let before = reader.io_statistics().record_reads;
    let result = reader.search_with_limits(
        &[0.1, 0.5],
        5,
        SearchLimits {
            record_reads: 3,
            candidates: 64,
        },
    )?;
    assert!(result.partial);
    assert_eq!(result.record_reads, 3);
    assert_eq!(reader.io_statistics().record_reads - before, 3);
    let full = reader.search(&[0.1, 0.5], 5)?;
    assert!(
        !full.partial,
        "request budget does not leak into next query"
    );
    let filter_limited = reader.search_with_limits(
        &[0.1, 0.5],
        5,
        SearchLimits {
            record_reads: full.record_reads as usize - 1,
            candidates: 64,
        },
    )?;
    assert!(filter_limited.partial);
    assert_eq!(
        filter_limited.hits.len(),
        4,
        "live-filter reads consume the shared budget"
    );
    Ok(())
}

#[test]
fn insertion_and_validation_failures_poison_the_entire_candidate() -> Result<()> {
    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    index.insert(key(1), &[1.0, 0.0])?;
    {
        let mut state = store.state.lock().unwrap();
        state.fail_at = Some(state.writes + 1); // append succeeds; first adjacency write fails.
    }
    assert!(index.insert(key(2), &[0.0, 1.0]).is_err());
    assert!(store.poisoned.load(Ordering::Relaxed));
    assert!(index.counts().is_err());
    assert!(index.insert(key(3), &[0.5, 0.5]).is_err());

    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    index.insert(key(1), &[1.0, 0.0])?;
    assert!(index.set_live(1, key(2), false).is_err());
    assert!(store.poisoned.load(Ordering::Relaxed));

    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    assert!(index.insert(key(1), &[f32::NAN, 0.0]).is_err());
    assert!(store.poisoned.load(Ordering::Relaxed));
    Ok(())
}

#[test]
fn malformed_authenticated_payload_and_specification_fail_closed() -> Result<()> {
    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store.clone(), 2, PIPELINE)?;
    index.insert(key(1), &[1.0, 0.0])?;
    drop(index);
    assert!(MutableDiskIndex::open(store.clone(), 2, key(99)).is_err());
    let reader = MutableDiskIndex::open(store.clone(), 2, PIPELINE)?;
    // Storage authenticity alone does not validate provider-specific vector bits.
    store.state.lock().unwrap().records[1].bytes[..4]
        .copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    assert!(reader.search(&[1.0, 0.0], 1).is_err());
    Ok(())
}
