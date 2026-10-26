// SPDX-License-Identifier: Apache-2.0
//! Remove only changed paths' old vector citations from the planned current
//! map. A later enrichment transaction may attach newly computed vectors.
use super::{OccurrencePointer, PathOccurrences, PreparedOccurrence, VectorBinding};
use crate::{
    FileStore,
    current::{
        CurrentHead, CurrentScope, MAX_PENDING_VECTOR_INPUTS, graph,
        journal::{ArtifactChange, CapturedInput},
        map,
    },
};
use airk_core::{
    graph_storage::{GraphRecord, PreparedGraphDelta},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct VectorPlanContext<'plan, 'state> {
    pub plan: &'plan mut map::MapPlan<'state>,
    pub connection: &'state Connection,
    pub store: &'state FileStore,
    pub scope: &'state CurrentScope,
    pub base: &'state CurrentHead,
    pub target: &'state CurrentHead,
    pub delta: &'state dyn PreparedGraphDelta,
}

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
    context: &mut VectorPlanContext<'_, '_>,
    prior_sources: &BTreeMap<String, Option<Sha256Digest>>,
    reattached: &BTreeSet<Sha256Digest>,
) -> Result<Vec<map::MapChange>> {
    let VectorPlanContext {
        plan,
        connection,
        scope,
        base,
        target,
        delta,
        ..
    } = context;
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
    if base.graph_head.scope != target.graph_head.scope {
        ensure!(
            bindings
                .values()
                .all(|binding| binding.occurrence_ids.is_empty()),
            "graph replacement leaves old vector occurrences"
        );
        ensure!(
            base.graph_head.allocated_count <= 1_000_000,
            "graph replacement retirement bound exceeded"
        );
        for id in 0..base.graph_head.allocated_count {
            let record = graph::read_record(connection, &base.graph_head, u32::try_from(id)?)?;
            if let Some(key) = record.embedding_key {
                change(
                    plan,
                    scope,
                    map::MapDomain::Vector,
                    key.as_bytes(),
                    None,
                    &mut changes,
                )?;
            }
        }
        return Ok(changes);
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
                && (reattached.contains(&embedding_key)
                    || record.live == !binding.occurrence_ids.is_empty()),
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

/// Attach only the changed paths' new citations after old memberships have
/// been retired. The extraction object and payload digest are verified once
/// per path, and every binding must agree with the sealed graph target.
pub(crate) fn attach_new_occurrences(
    context: &mut VectorPlanContext<'_, '_>,
    changes: &[ArtifactChange],
    vector_inputs: Option<&[CapturedInput]>,
    occurrences: &[PreparedOccurrence],
) -> Result<Vec<map::MapChange>> {
    let VectorPlanContext {
        plan,
        connection,
        store,
        scope,
        base,
        target,
        delta,
    } = context;
    ensure!(
        occurrences.len() <= MAX_PENDING_VECTOR_INPUTS,
        "too many changed current vector occurrences"
    );
    ensure!(
        delta.descriptor().base == base.graph_head
            && delta.descriptor().target == target.graph_head,
        "new current vector graph target mismatch"
    );
    if occurrences.is_empty() {
        return Ok(Vec::new());
    }
    let mut inputs = BTreeMap::new();
    for change in changes {
        if let ArtifactChange::Upsert(input) = change {
            inputs.insert(input.artifact.path.as_str(), input.as_ref());
        }
    }
    if let Some(vector_inputs) = vector_inputs {
        ensure!(
            changes.is_empty(),
            "vector-only inputs cannot rewrite artifacts"
        );
        for input in vector_inputs {
            let key = map::key(
                scope,
                map::MapDomain::Artifact,
                input.artifact.path.as_bytes(),
            )?;
            ensure!(
                plan.get(key)?.as_deref() == Some(input.bytes()?.as_slice())
                    && inputs.insert(input.artifact.path.as_str(), input).is_none(),
                "vector-only input differs from current artifact"
            );
        }
    }
    let mut by_path = BTreeMap::<&str, Vec<&PreparedOccurrence>>::new();
    let mut graph_keys = BTreeMap::<u32, Sha256Digest>::new();
    for occurrence in occurrences {
        occurrence.pointer.validate()?;
        let pointer = &occurrence.pointer;
        ensure!(
            inputs.contains_key(pointer.path.as_str())
                && occurrence.graph_id != 0
                && u64::from(occurrence.graph_id) < target.graph_head.allocated_count,
            "new current occurrence has no changed artifact or graph node"
        );
        if let Some(previous) = graph_keys.insert(occurrence.graph_id, pointer.embedding_key) {
            ensure!(
                previous == pointer.embedding_key,
                "new current graph ID has conflicting embedding keys"
            );
        }
        by_path.entry(&pointer.path).or_default().push(occurrence);
    }
    let mut affected_ids = graph_keys.keys().copied().collect::<BTreeSet<_>>();
    for key in graph_keys.values() {
        let binding_key = map::key(scope, map::MapDomain::Vector, key.as_bytes())?;
        if let Some(bytes) = plan.get(binding_key)? {
            affected_ids.insert(VectorBinding::decode(&bytes, target, *key)?.graph_id);
        }
    }
    let mut afterimages = BTreeMap::<u32, GraphRecord>::new();
    delta.visit_records(&mut |id, record| {
        if affected_ids.contains(&id) {
            ensure!(
                afterimages.insert(id, record.clone()).is_none(),
                "duplicate new current graph afterimage"
            );
        }
        Ok(())
    })?;
    let mut map_changes = Vec::new();
    let mut bindings = BTreeMap::<Sha256Digest, VectorBinding>::new();
    for (path, mut items) in by_path {
        let input = inputs[path];
        let source_hash = input.artifact.content_hash.parse::<Sha256Digest>()?;
        let object = input
            .artifact
            .retained
            .as_ref()
            .context("new occurrence extraction object missing")?
            .object
            .parse::<Sha256Digest>()?;
        let retained = store.load_retrieval_artifact(&input.artifact)?;
        let payload_digest = retained.payload_digest;
        let mut evidence = airk_core::vector_evidence::VectorEvidenceReader::new(&retained)?;
        items.sort_by_key(|item| item.pointer.path_ordinal);
        let index = PathOccurrences {
            version: 1,
            path: path.to_owned(),
            source_hash,
            count: u32::try_from(items.len())?,
        };
        for (ordinal, item) in items.into_iter().enumerate() {
            let pointer = &item.pointer;
            let occurrence_id = index.occurrence_id(scope, u32::try_from(ordinal)?)?;
            ensure!(
                pointer.path_ordinal == u32::try_from(ordinal)?
                    && pointer.occurrence_id == occurrence_id
                    && pointer.source_hash == source_hash
                    && pointer.payload_digest == payload_digest
                    && pointer.object == object,
                "new current occurrence differs from captured extraction"
            );
            evidence.materialize(&pointer.members)?;
            change(
                plan,
                scope,
                map::MapDomain::Occurrence,
                occurrence_id.as_bytes(),
                Some(pointer.canonical_bytes()?),
                &mut map_changes,
            )?;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                bindings.entry(pointer.embedding_key)
            {
                let binding_key = map::key(
                    scope,
                    map::MapDomain::Vector,
                    pointer.embedding_key.as_bytes(),
                )?;
                let binding = match plan.get(binding_key)? {
                    Some(bytes) => VectorBinding::decode(&bytes, target, pointer.embedding_key)?,
                    None => {
                        ensure!(
                            base.graph_head.scope != target.graph_head.scope
                                || u64::from(item.graph_id) >= base.graph_head.allocated_count,
                            "new current vector key reuses an unmapped base graph node"
                        );
                        VectorBinding {
                            version: 1,
                            embedding_key: pointer.embedding_key,
                            graph_scope: target.graph_head.scope.digest(),
                            graph_id: item.graph_id,
                            occurrence_ids: Vec::new(),
                        }
                    }
                };
                entry.insert(binding);
            }
            let binding = bindings
                .get_mut(&pointer.embedding_key)
                .context("new current vector binding disappeared")?;
            if binding.graph_id != item.graph_id {
                let old = match afterimages.get(&binding.graph_id) {
                    Some(record) => record.clone(),
                    None => {
                        let record =
                            graph::read_record(connection, &base.graph_head, binding.graph_id)?;
                        ensure!(
                            !record.live && binding.occurrence_ids.is_empty(),
                            "replaced graph node has no sealed afterimage"
                        );
                        record
                    }
                };
                ensure!(
                    base.graph_head.scope == target.graph_head.scope
                        && u64::from(binding.graph_id) < base.graph_head.allocated_count
                        && u64::from(item.graph_id) >= base.graph_head.allocated_count
                        && old.embedding_key == Some(pointer.embedding_key)
                        && !old.live,
                    "new current occurrence graph ID differs from binding"
                );
                binding.graph_id = item.graph_id;
            }
            let position = binding
                .occurrence_ids
                .binary_search(&occurrence_id)
                .unwrap_or_else(|position| position);
            ensure!(
                binding.occurrence_ids.get(position) != Some(&occurrence_id),
                "duplicate new current occurrence ID"
            );
            binding.occurrence_ids.insert(position, occurrence_id);
        }
        change(
            plan,
            scope,
            map::MapDomain::PathOccurrences,
            path.as_bytes(),
            Some(index.canonical_bytes()?),
            &mut map_changes,
        )?;
    }
    for (embedding_key, binding) in bindings {
        binding.validate(target)?;
        let record = match afterimages.remove(&binding.graph_id) {
            Some(record) => record,
            None if base.graph_head.scope == target.graph_head.scope
                && u64::from(binding.graph_id) < base.graph_head.allocated_count =>
            {
                graph::read_record(connection, &base.graph_head, binding.graph_id)?
            }
            None => anyhow::bail!("new current vector graph record missing"),
        };
        ensure!(
            record.embedding_key == Some(embedding_key) && record.live,
            "new current vector binding differs from graph target"
        );
        change(
            plan,
            scope,
            map::MapDomain::Vector,
            embedding_key.as_bytes(),
            Some(binding.canonical_bytes(target)?),
            &mut map_changes,
        )?;
    }
    Ok(map_changes)
}
