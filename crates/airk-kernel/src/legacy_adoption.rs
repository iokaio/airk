// SPDX-License-Identifier: Apache-2.0
//! Observation of saved legacy evidence that an earlier release adopted into a
//! Current publication. Adoption itself is no longer supported; the canonical
//! `legacy-adoption` resource stays visible in status, diagnostics and search.
use airk_store::legacy_adoption::AdoptionResource;
use anyhow::{Context, Result, ensure};

#[derive(serde::Serialize)]
pub struct LegacySavedFileCompatibility {
    pub origin: airk_core::legacy_adoption::LegacySavedOrigin,
    pub vector_selection_available: bool,
    pub profile_families_available: bool,
}

pub(crate) struct CurrentLegacyAdoption {
    pub observation: serde_json::Value,
    semantic_unavailable: bool,
}

impl CurrentLegacyAdoption {
    pub fn qualify(&self, readiness: &mut airk_core::readiness::SemanticReadiness) {
        if self.semantic_unavailable {
            readiness.state = airk_core::readiness::SemanticState::Unavailable;
            readiness.completed_artifacts = 0;
            readiness.completed_units = 0;
            readiness.first_ready_unix_ms = None;
            readiness.fully_ready_unix_ms = None;
            readiness.selection_digest = None;
            readiness.pipeline_digest = None;
            readiness.failure = Some("legacy_saved_semantic_conversion_unavailable".into());
        }
    }
}

/// One canonical observation for status, diagnostics and search readiness. The
/// complete original resource stays visible, including explicit old/new mapping.
pub(crate) fn current_observation(
    store: &airk_store::FileStore,
    read: &airk_store::current::CurrentRead,
) -> Result<Option<CurrentLegacyAdoption>> {
    let Some(reference) = read.resource_reference("legacy-adoption")? else {
        return Ok(None);
    };
    ensure!(
        reference.bytes <= 2 << 20,
        "legacy adoption observation exceeds bound"
    );
    let bytes = store.read_current_resource(&reference)?;
    let resource: AdoptionResource = serde_json::from_slice(&bytes)?;
    let state = read
        .collection_state()?
        .context("legacy adoption configuration missing")?;
    ensure!(
        serde_json::to_vec(&resource)? == bytes
            && resource.schema == 1
            && resource.root == read.head().scope.root
            && resource.collection == read.head().scope.collection
            && resource.roots == state.configuration.roots,
        "legacy adoption observation scope differs"
    );
    let semantic_unavailable = resource.compatibility.original_embedding_model.is_some()
        && !resource.compatibility.retrieval_converted
        && state.configuration.model.is_none();
    let matches_current = resource.target_incarnation == read.head().scope.incarnation
        && resource.target_sequence == read.head().sequence;
    let mut observation = serde_json::to_value(&resource)?;
    observation["matches_current_publication"] = matches_current.into();
    Ok(Some(CurrentLegacyAdoption {
        observation,
        semantic_unavailable,
    }))
}
