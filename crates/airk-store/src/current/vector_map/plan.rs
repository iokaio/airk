// SPDX-License-Identifier: Apache-2.0
//! Remove only changed paths' old vector citations from the planned current
//! map. A later enrichment transaction may attach newly computed vectors.
use super::{OccurrencePointer, PathOccurrences, VectorBinding};
use crate::current::{CurrentHead, CurrentScope, graph, map};
use airk_core::{
    graph_storage::{GraphRecord, PreparedGraphDelta},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn change(
    plan: &mut map::MapPlan<'_>,
    scope: &CurrentScope,
    domain: map::MapDomain,
    identity: &[u8],
    value: Option<Vec<u8>>,
    changes: &mut Vec<map::MapChange>,
) -> Result<()> {
    let change = map::MapChange::new(scope, domain, identity, value)?;
    plan.put(change.key(), change.value())?;
    changes.push(change);
    Ok(())
}

/// Called after the artifact afterimages have been planned. `prior_sources`
/// comes from proof-checked old catalog records, before those replacements.
/// Every old path-local ordinal is checked against its pointer and vector
/// binding. Graph records must agree with the resulting live memberships.
pub(crate) fn retire_changed_paths(
    plan: &mut map::MapPlan<'_>,
    connection: &Connection,
    scope: &CurrentScope,
    base: &CurrentHead,
    target: &CurrentHead,
    delta: &dyn PreparedGraphDelta,
    prior_sources: &BTreeMap<String, Option<Sha256Digest>>,
) -> Result<Vec<map::MapChange>> {
    ensure!(
        delta.descriptor().base == base.graph_head
            && delta.descriptor().target == target.graph_head,
        "changed-path vector retirement graph target mismatch"
    );
    let mut changes = Vec::new();
    let mut bindings = BTreeMap::<Sha256Digest, VectorBinding>::new();
    for (path, prior_hash) in prior_sources {
        let path_key = map::key(scope, map::MapDomain::PathOccurrences, path.as_bytes())?;
        let Some(bytes) = plan.get(path_key)? else {
            continue;
        };
        let index = PathOccurrences::decode(&bytes, path)?;
        ensure!(
            *prior_hash == Some(index.source_hash),
            "current path occurrence index differs from prior artifact"
        );
        for ordinal in 0..index.count {
            let occurrence_id = index.occurrence_id(scope, ordinal)?;
            let key = map::key(scope, map::MapDomain::Occurrence, occurrence_id.as_bytes())?;
            let bytes = plan
                .get(key)?
                .context("current path occurrence pointer missing")?;
            let pointer = OccurrencePointer::decode(&bytes, occurrence_id)?;
            ensure!(
                pointer.path == *path
                    && pointer.path_ordinal == ordinal
                    && pointer.source_hash == index.source_hash,
                "current path occurrence pointer differs from path index"
            );
            if let std::collections::btree_map::Entry::Vacant(entry) =
                bindings.entry(pointer.embedding_key)
            {
                let binding_key = map::key(
                    scope,
                    map::MapDomain::Vector,
                    pointer.embedding_key.as_bytes(),
                )?;
                let binding_bytes = plan
                    .get(binding_key)?
                    .context("current occurrence vector binding missing")?;
                let binding = VectorBinding::decode(&binding_bytes, base, pointer.embedding_key)?;
                let record = graph::read_record(connection, &base.graph_head, binding.graph_id)?;
                ensure!(
                    record.embedding_key == Some(pointer.embedding_key)
                        && record.live == !binding.occurrence_ids.is_empty(),
                    "current occurrence binding differs from base graph"
                );
                entry.insert(binding);
            }
            let binding = bindings
                .get_mut(&pointer.embedding_key)
                .context("current vector binding disappeared")?;
            let position = binding
                .occurrence_ids
                .binary_search(&occurrence_id)
                .map_err(|_| anyhow::anyhow!("current path occurrence missing from vector"))?;
            binding.occurrence_ids.remove(position);
            change(
                plan,
                scope,
                map::MapDomain::Occurrence,
                occurrence_id.as_bytes(),
                None,
                &mut changes,
            )?;
        }
        change(
            plan,
            scope,
            map::MapDomain::PathOccurrences,
            path.as_bytes(),
            None,
            &mut changes,
        )?;
    }
    if bindings.is_empty() {
        return Ok(changes);
    }
    let affected = bindings
        .values()
        .map(|binding| (binding.graph_id, binding.embedding_key))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        affected.len() == bindings.len(),
        "changed current vectors share a graph ID"
    );
    let mut afterimages = BTreeMap::<u32, GraphRecord>::new();
    delta.visit_records(&mut |id, record| {
        if affected.contains_key(&id) {
            ensure!(
                afterimages.insert(id, record.clone()).is_none(),
                "duplicate changed graph afterimage"
            );
        }
        Ok(())
    })?;
    for (embedding_key, binding) in bindings {
        binding.validate(target)?;
        let record = match afterimages.remove(&binding.graph_id) {
            Some(record) => record,
            None => graph::read_record(connection, &base.graph_head, binding.graph_id)?,
        };
        ensure!(
            record.embedding_key == Some(embedding_key)
                && record.live == !binding.occurrence_ids.is_empty(),
            "changed current vector live mapping differs from graph target"
        );
        change(
            plan,
            scope,
            map::MapDomain::Vector,
            embedding_key.as_bytes(),
            Some(binding.canonical_bytes(target)?),
            &mut changes,
        )?;
    }
    Ok(changes)
}
