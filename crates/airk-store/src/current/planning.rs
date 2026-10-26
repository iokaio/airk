// SPDX-License-Identifier: Apache-2.0
//! Storage-independent catalog planning for captured artifact/vector updates.
use super::journal::valid_current_path;
use super::{
    ArtifactChange, ArtifactVectorChanges, CapturedInput, CurrentHead, CurrentScope,
    MAX_PENDING_INPUTS, graph, map, vector_map,
};
use crate::FileStore;
use airk_core::{identity::Sha256Digest, model_objects::ModelObjectSet};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) trait Storage: map::MapStorage + graph::GraphStorage {}
impl<T: map::MapStorage + graph::GraphStorage> Storage for T {}
pub(crate) struct Context<'a> {
    pub storage: &'a dyn Storage,
    pub store: &'a FileStore,
    pub scope: &'a CurrentScope,
    pub base: &'a CurrentHead,
    pub target: &'a CurrentHead,
    pub graph_delta: &'a dyn airk_core::graph_storage::PreparedGraphDelta,
    pub update: ArtifactVectorChanges<'a>,
    pub model: &'a Option<ModelObjectSet>,
    pub metadata: &'a [super::metadata::FileMetadataRef],
    pub resources: &'a [super::resources::ResourceChange],
}
pub(crate) struct Artifacts<'a> {
    pub plan: map::MapPlan<'a>,
    pub inputs: Vec<CapturedInput>,
    pub changes: Vec<map::MapChange>,
}
pub(crate) fn artifacts(context: Context<'_>) -> Result<Artifacts<'_>> {
    let Context {
        storage,
        store,
        scope,
        base,
        target,
        graph_delta,
        update,
        model,
        metadata,
        resources,
    } = context;
    ensure!(
        base.scope == *scope
            && target.scope == *scope
            && base.sequence.checked_add(1) == Some(target.sequence)
            && graph_delta.descriptor().base == base.graph_head
            && graph_delta.descriptor().target == target.graph_head,
        "artifact planning component base/target mismatch"
    );
    let changes = update.artifacts;
    let vector_inputs = update.vector_inputs;
    let occurrences = update.occurrences;
    ensure!(
        changes.len() <= MAX_PENDING_INPUTS
            && (!changes.is_empty() || update.configuration.is_some() || !resources.is_empty()),
        "invalid changed-artifact count"
    );
    let mut previous = None;
    let mut inputs = Vec::new();
    let mut prior_sources = BTreeMap::new();
    let mut map_changes = Vec::with_capacity(changes.len());
    for change in changes {
        let path = change.path();
        ensure!(
            valid_current_path(path) && previous.is_none_or(|prior| prior < path),
            "changed artifact paths are invalid, unsorted or repeated"
        );
        previous = Some(path);
        let value = match change {
            ArtifactChange::Upsert(input) => {
                let bytes = input.bytes()?;
                inputs.push(input.as_ref().clone());
                Some(bytes)
            }
            ArtifactChange::Delete(_) => None,
        };
        map_changes.push(map::MapChange::new(
            scope,
            map::MapDomain::Artifact,
            path.as_bytes(),
            value,
        )?);
    }
    if let Some(vector_inputs) = vector_inputs {
        ensure!(
            changes.is_empty()
                && vector_inputs.len() <= MAX_PENDING_INPUTS
                && vector_inputs
                    .windows(2)
                    .all(|pair| pair[0].artifact.path < pair[1].artifact.path),
            "invalid vector-only input batch"
        );
        for input in vector_inputs {
            inputs.push(input.clone());
        }
    }
    map_changes.sort_by_key(map::MapChange::key);
    ensure!(
        map_changes
            .windows(2)
            .all(|pair| pair[0].key() < pair[1].key()),
        "duplicate current artifact map identity"
    );
    let mut plan = map::MapPlan::new(storage, scope, base.artifact_root)?;
    if let Some(inputs) = vector_inputs {
        for input in inputs {
            let key = map::key(
                scope,
                map::MapDomain::Artifact,
                input.artifact.path.as_bytes(),
            )?;
            ensure!(
                plan.get(key)?.as_deref() == Some(input.bytes()?.as_slice()),
                "vector-only input differs from current artifact"
            );
            prior_sources.insert(
                input.artifact.path.clone(),
                Some(input.artifact.content_hash.parse()?),
            );
        }
    }
    // Validate bootstrap configuration against the committed base. Retained
    // change metadata below mutates the plan even when no artifacts exist.
    let mut collection =
        super::collection::CollectionPlan::load(&plan, scope, update.configuration)?;
    let recent_observation =
        super::recent_events::observe(store, scope, &plan, base, changes, metadata)?;
    map_changes.extend(super::recent::prepare(
        scope,
        &mut plan,
        base,
        recent_observation,
        super::recent::unix_seconds(),
    )?);
    if base.graph_head.scope != target.graph_head.scope {
        collection.require_all_previous()?;
    }
    collection.verify_model(model)?;
    let mut postings = super::postings::PostingChanges::new(
        base.sequence == 0
            && changes.iter().all(|change| match change {
                ArtifactChange::Upsert(input) => input.postings_version == 1,
                ArtifactChange::Delete(_) => true,
            }),
    );
    for change in changes {
        let key = map::key(scope, map::MapDomain::Artifact, change.path().as_bytes())?;
        let previous = if let Some(bytes) = plan.get(key)? {
            let previous: CapturedInput = serde_json::from_slice(&bytes)?;
            ensure!(
                previous.artifact.path == change.path() && previous.bytes()? == bytes,
                "changed artifact has invalid prior catalog binding"
            );
            Some(previous)
        } else {
            None
        };
        prior_sources.insert(
            change.path().to_owned(),
            previous
                .as_ref()
                .map(|a| a.artifact.content_hash.parse::<Sha256Digest>())
                .transpose()?,
        );
        collection.update(
            previous.as_ref().map(|input| &input.artifact),
            match change {
                ArtifactChange::Upsert(input) => Some(&input.artifact),
                ArtifactChange::Delete(_) => None,
            },
        )?;
        postings.update(
            &mut plan,
            scope,
            store,
            previous.as_ref(),
            match change {
                ArtifactChange::Upsert(input) => Some(input.as_ref()),
                ArtifactChange::Delete(_) => None,
            },
        )?;
        let value = match change {
            ArtifactChange::Upsert(input) => Some(input.bytes()?),
            ArtifactChange::Delete(_) => None,
        };
        plan.put(key, value.as_deref())?;
    }
    map_changes.extend(postings.finish(&mut plan, scope)?);
    map_changes.extend(collection.finish(&mut plan, scope)?);
    map_changes.extend(super::metadata::prepare(
        store,
        scope,
        &mut plan,
        changes,
        metadata,
        target.sequence,
    )?);
    map_changes.extend(super::resources::prepare(
        store, scope, &mut plan, resources,
    )?);
    let reattached = occurrences
        .iter()
        .map(|item| item.pointer.embedding_key)
        .collect::<BTreeSet<_>>();
    {
        let mut context = vector_map::VectorPlanContext {
            plan: &mut plan,
            connection: storage,
            store,
            scope,
            base,
            target,
            delta: graph_delta,
        };
        map_changes.extend(vector_map::retire_changed_paths(
            &mut context,
            &prior_sources,
            &reattached,
        )?);
        map_changes.extend(vector_map::attach_new_occurrences(
            &mut context,
            changes,
            vector_inputs,
            occurrences,
        )?);
    }
    // A replaced path may retire and reattach one exact key. Replay only
    // its final afterimage, not both intermediate map changes.
    let mut final_changes = BTreeMap::new();
    for change in map_changes {
        final_changes.insert(change.key(), change);
    }
    let map_changes = final_changes.into_values().collect::<Vec<_>>();
    ensure!(
        map_changes
            .windows(2)
            .all(|pair| pair[0].key() < pair[1].key()),
        "duplicate current component map identity"
    );
    Ok(Artifacts {
        plan,
        inputs,
        changes: map_changes,
    })
}
