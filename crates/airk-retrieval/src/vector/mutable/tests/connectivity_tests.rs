// SPDX-License-Identifier: Apache-2.0
//! Mount as mutable::tests::connectivity_tests; actual DiskANN, no model/data.
use super::*;
use std::collections::{BTreeSet, VecDeque};

fn reachable(store: &Fixture, dimensions: usize) -> Result<BTreeSet<u32>> {
    let counts = store.counts()?;
    let mut seen = BTreeSet::from([0]);
    let mut queue = VecDeque::from([0]);
    while let Some(id) = queue.pop_front() {
        let record = store.read(id)?;
        let decoded = codec::decode(dimensions, counts.allocated_count, id, &record.bytes)?;
        for next in decoded.neighbors {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen.remove(&0);
    Ok(seen)
}

#[test]
fn orthogonal_live_nodes_remain_reachable_after_reserved_seed_backedge_pruning() -> Result<()> {
    const N: usize = 64;
    let store = Fixture::new(N)?;
    let mut index = MutableDiskIndex::create(store.clone(), N, PIPELINE)?;
    let mut first_disconnected = None;
    for axis in 0..N {
        let mut vector = vec![0.0; N];
        vector[axis] = 1.0;
        let id = index.insert(key(u32::try_from(axis + 1)?), &vector)?;
        assert_eq!(id, u32::try_from(axis + 1)?);
        let found = reachable(&store, N)?;
        if found.len() != axis + 1 && first_disconnected.is_none() {
            first_disconnected = Some(id);
        }
    }
    let reached = reachable(&store, N)?;
    let mut missing_self = Vec::new();
    for axis in 0..N {
        let mut vector = vec![0.0; N];
        vector[axis] = 1.0;
        let id = u32::try_from(axis + 1)?;
        let exact = index.search_subset(&vector, &(1..=64).collect::<Vec<_>>(), 1)?;
        assert_eq!(
            exact.hits,
            vec![(id, 0.0)],
            "each orthogonal self has unique exact distance zero"
        );
        if !index
            .search(&vector, 10)?
            .hits
            .iter()
            .any(|(node, distance)| *node == id && *distance == 0.0)
        {
            missing_self.push(id);
        }
    }
    eprintln!(
        "{}",
        serde_json::json!({"fixture":"64 orthogonal unit vectors; all self-distances0, all distinct pair distances2, seed-to-live1","first_disconnected_insert":first_disconnected,"reachable":reached,"missing_unique_self":missing_self})
    );
    assert_eq!(
        reached,
        (1..=64u32).collect::<BTreeSet<_>>(),
        "reserved seed must not strand committed live vectors"
    );
    assert!(
        missing_self.is_empty(),
        "reachable unique self vectors must be found in this bounded tiny graph"
    );
    Ok(())
}
