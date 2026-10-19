// SPDX-License-Identifier: Apache-2.0
//! Exact input interning for one build, with bounded in-memory batches.
use super::*;
use airk_core::{
    embedding::{EmbeddingDescriptor, embedding_key},
    vector_selection::{PreparedVectorNode, VectorNodeRegistry},
};

/// The collection-scoped registry is private to this build. It stores successful
/// outcomes only; a failed inference or graph insertion never becomes a miss.
#[allow(clippy::too_many_arguments)]
pub(super) fn intern(
    model: &EmbeddingModel,
    texts: &[&str],
    descriptor: &EmbeddingDescriptor,
    registry: &dyn VectorNodeRegistry,
    graph: &mut DiskIndex,
    mut cache: Option<&mut EmbeddingCacheContext<'_>>,
    local_stats: &mut crate::model::EmbeddingCacheStats,
) -> Result<Vec<Option<u32>>> {
    ensure!(
        texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS,
        "vector input batch exceeds bound"
    );
    let mut groups = BTreeMap::new();
    let mut keys = Vec::new();
    let mut representatives = Vec::new();
    let mut assignments = Vec::with_capacity(texts.len());
    for (index, text) in texts.iter().enumerate() {
        let key = embedding_key(descriptor, text.as_bytes())?;
        let group = if let Some(group) = groups.get(&key).copied() {
            ensure!(
                texts[representatives[group]] == *text,
                "exact vector input identity collision"
            );
            group
        } else {
            let group = keys.len();
            groups.insert(key, group);
            keys.push(key);
            representatives.push(index);
            group
        };
        assignments.push(group);
    }
    let mut outcomes = vec![None; keys.len()];
    let mut missing = Vec::new();
    for (group, key) in keys.iter().copied().enumerate() {
        match registry.lookup(key)? {
            Some(node) => {
                ensure!(
                    node.graph_id
                        .is_none_or(|id| id > 0 && id as usize <= graph.len()),
                    "vector registry refers outside this build's graph"
                );
                outcomes[group] = Some(node);
            }
            None => missing.push(group),
        }
    }
    let pending: Vec<_> = missing
        .iter()
        .map(|group| texts[representatives[*group]])
        .collect();
    let vectors = if pending.is_empty() {
        Vec::new()
    } else if let Some(context) = cache.as_mut() {
        crate::embedding_batch::embed(
            model,
            &pending,
            descriptor,
            context.cache,
            context.bypass,
            context.stats,
            context.max_workers,
            context.working_memory_bytes,
        )?
    } else {
        crate::embedding_batch::embed(
            model,
            &pending,
            descriptor,
            None,
            false,
            local_stats,
            1,
            128 << 20,
        )?
    };
    let stats = match cache.as_mut() {
        Some(context) => &mut *context.stats,
        None => local_stats,
    };
    stats.vector_reused_inputs = stats
        .vector_reused_inputs
        .saturating_add((texts.len() - missing.len()) as u64);
    ensure!(
        vectors.len() == missing.len(),
        "embedding result count differs from exact input count"
    );
    for (group, vector) in missing.into_iter().zip(vectors) {
        let graph_id = if let Some(vector) = vector {
            let started = std::time::Instant::now();
            let id = graph.insert(&vector)?;
            stats.graph_insert_us = stats
                .graph_insert_us
                .saturating_add(crate::embedding_batch::elapsed_us(started));
            stats.vector_graph_nodes = stats.vector_graph_nodes.saturating_add(1);
            Some(id)
        } else {
            None
        };
        let node = PreparedVectorNode { graph_id };
        registry.remember(keys[group], node)?;
        outcomes[group] = Some(node);
    }
    assignments
        .into_iter()
        .map(|group| {
            Ok(outcomes[group]
                .context("vector input outcome missing")?
                .graph_id)
        })
        .collect()
}
