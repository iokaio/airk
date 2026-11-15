// SPDX-License-Identifier: Apache-2.0
//! Companion regression for explicit prune-center custody, not candidate order.
use super::*;
use diskann::{
    graph::glue::{PruneAccessor, PruneStrategy},
    provider::NeighborAccessor,
};

#[test]
fn reserved_seed_view_tracks_actual_center_across_cached_source_zero_and_ordinary_pools()
-> Result<()> {
    let store = Fixture::new(2)?;
    let mut index = MutableDiskIndex::create(store, 2, PIPELINE)?;
    index.insert(key(1), &[1.0, 0.0])?;
    let bootstrap = OperationContext {
        insertion_id: Some(1),
        ..OperationContext::new(MAX_VECTOR_READS)
    };
    let mut pruner = Strategy.prune_accessor(index.inner.provider(), &bootstrap, 64)?;
    {
        let (view, _) = futures::executor::block_on(pruner.fill([0, 1].into_iter()))?;
        assert!(
            workingset::View::get(&view, 0).is_some(),
            "one-real-node bootstrap retains its only seed"
        );
    }
    drop(pruner);
    index.insert(key(2), &[0.0, 1.0])?;
    let context = OperationContext {
        insertion_id: Some(2),
        ..OperationContext::new(MAX_VECTOR_READS)
    };
    let mut pruner = Strategy.prune_accessor(index.inner.provider(), &context, 64)?;
    // Actual initial robust_prune supplies a distance-sorted candidate pool:
    // its first ID0 is NOT the inserted center2.
    {
        let (view, _) = futures::executor::block_on(pruner.fill([0, 1].into_iter()))?;
        assert!(workingset::View::get(&view, 0).is_none());
        assert!(workingset::View::get(&view, 1).is_some());
    }
    assert!(
        pruner.vectors.get(&0).is_some(),
        "inner map really cached0; only the view excludes it"
    );
    let mut neighbors = AdjacencyList::new();
    futures::executor::block_on(pruner.neighbors().get_neighbors(0, &mut neighbors))?;
    {
        let (view, _) = futures::executor::block_on(pruner.fill([0, 1, 2].into_iter()))?;
        assert!(
            workingset::View::get(&view, 0).is_some(),
            "actual source0 remains available for its own prune"
        );
    }
    futures::executor::block_on(pruner.neighbors().get_neighbors(1, &mut neighbors))?;
    {
        let (view, _) = futures::executor::block_on(pruner.fill([0, 1, 2].into_iter()))?;
        assert!(
            workingset::View::get(&view, 0).is_none(),
            "cached0 cannot reappear for ordinary source1"
        );
        assert!(workingset::View::get(&view, 1).is_some());
    }
    assert!(pruner.vectors.get(&0).is_some());
    assert!(
        Strategy
            .prune_accessor(
                index.inner.provider(),
                &OperationContext::new(MAX_VECTOR_READS),
                64
            )
            .is_err(),
        "missing actual insertion identity cannot be guessed"
    );
    Ok(())
}
