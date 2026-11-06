// SPDX-License-Identifier: Apache-2.0
//! Small applied collection configuration and counters, authenticated with artifacts.
use super::{CurrentRead, CurrentScope, map};
use airk_core::{
    Artifact, extraction::IndexDetail, identity::Sha256Digest, model_objects::ModelObjectSet,
    vector_selection::VectorSelectionPolicy,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &[u8] = b"collection";

#[cfg(test)]
mod policy_configuration_tests {
    use super::*;
    #[test]
    fn only_policy_attestation_is_exempt_from_complete_retained_replacement() -> Result<()> {
        let before = CollectionConfiguration {
            retained_profile: None,
            source_policy_digest: None,
            roots: Default::default(),
            extractor_version: "fixture/1".into(),
            extraction_policy_digest: Sha256Digest::from_bytes([1; 32]),
            index_detail: IndexDetail::Balanced,
            vector_selection: Default::default(),
            model: None,
        };
        let mut marker = before.clone();
        marker.source_policy_digest = Some(Sha256Digest::from_bytes([2; 32]));
        assert!(before.retains_existing_artifacts(&marker)?);
        let mut changed = marker.clone();
        changed.extractor_version = "fixture/2".into();
        assert!(!before.retains_existing_artifacts(&changed)?);
        changed = marker.clone();
        changed.extraction_policy_digest = Sha256Digest::from_bytes([3; 32]);
        assert!(!before.retains_existing_artifacts(&changed)?);
        changed = marker.clone();
        changed.index_detail = IndexDetail::Detailed;
        assert!(!before.retains_existing_artifacts(&changed)?);
        changed = marker;
        changed
            .roots
            .insert("other".into(), "/unrelated-root".into());
        assert!(!before.retains_existing_artifacts(&changed)?);
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_profile: Option<airk_core::retention::RetainedProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_policy_digest: Option<Sha256Digest>,
    pub roots: BTreeMap<String, String>,
    pub extractor_version: String,
    pub extraction_policy_digest: Sha256Digest,
    pub index_detail: IndexDetail,
    pub vector_selection: VectorSelectionPolicy,
    pub model: Option<ModelObjectSet>,
}

impl CollectionConfiguration {
    pub fn index_coverage(&self) -> airk_core::extraction::IndexCoverage {
        airk_core::extraction::IndexCoverage::with_retained_profile(
            self.index_detail,
            self.retained_profile,
        )
    }
    /// A policy attestation fences native admission and changes no extractor,
    /// source binding, model or retained representation. Every other serialized
    /// field (including future additions) still requires complete replacement.
    fn retains_existing_artifacts(&self, next: &Self) -> Result<bool> {
        let mut before = self.clone();
        before.source_policy_digest = next.source_policy_digest;
        Ok(serde_json::to_vec(&before)? == serde_json::to_vec(next)?)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.roots.len() <= 16
                && !self.extractor_version.is_empty()
                && self.extractor_version.len() <= 1024,
            "invalid current collection configuration"
        );
        for (alias, root) in &self.roots {
            crate::roots::validate_alias(alias)?;
            ensure!(
                !root.is_empty() && root.len() <= 32_768 && !root.contains('\0'),
                "invalid current source root binding"
            );
        }
        self.vector_selection.validate()?;
        if let Some(model) = &self.model {
            model.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionCounts {
    pub files: u64,
    pub chunks: u64,
    pub facts: u64,
    pub source_bytes: u64,
    pub extracted_bytes: u64,
    pub vector_eligible: u64,
    pub vector_excluded: u64,
}

impl CollectionCounts {
    fn update(
        &mut self,
        artifact: &Artifact,
        selection: &VectorSelectionPolicy,
        insert: bool,
    ) -> Result<()> {
        let retained = artifact
            .retained
            .as_ref()
            .context("current collection requires retained metadata")?;
        let profile = retained
            .profile
            .as_ref()
            .context("current collection profile missing")?;
        let eligible = selection
            .decision(&artifact.path, profile, &artifact.language)?
            .eligible;
        for (count, amount) in [
            (&mut self.files, 1),
            (&mut self.chunks, retained.chunks as u64),
            (&mut self.facts, retained.facts as u64),
            (&mut self.source_bytes, artifact.bytes as u64),
            (&mut self.extracted_bytes, retained.extracted_bytes as u64),
            (&mut self.vector_eligible, u64::from(eligible)),
            (&mut self.vector_excluded, u64::from(!eligible)),
        ] {
            *count = if insert {
                count.checked_add(amount)
            } else {
                count.checked_sub(amount)
            }
            .context("current collection counter overflow or underflow")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionState {
    pub version: u32,
    pub configuration: CollectionConfiguration,
    pub counts: CollectionCounts,
}
impl CollectionState {
    fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(self.version == 1, "unsupported current collection state");
        self.configuration.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "current collection metadata limit exceeded"
        );
        Ok(bytes)
    }
    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "oversized current collection state"
        );
        let state: Self = serde_json::from_slice(bytes)?;
        ensure!(
            state.bytes()? == bytes,
            "noncanonical current collection state"
        );
        Ok(state)
    }
}

impl CurrentRead {
    pub fn collection_state(&self) -> Result<Option<CollectionState>> {
        self.map_get(map::MapDomain::Metadata, KEY)?
            .as_deref()
            .map(CollectionState::decode)
            .transpose()
    }
}

pub(crate) struct CollectionPlan {
    prior_selection: VectorSelectionPolicy,
    state: Option<CollectionState>,
    required_previous: Option<u64>,
    visited_previous: u64,
}
impl CollectionPlan {
    pub(crate) fn require_supported_retention(
        &self,
        store: &crate::FileStore,
        scope: &CurrentScope,
    ) -> Result<()> {
        if self
            .state
            .as_ref()
            .is_some_and(|state| state.configuration.retained_profile.is_some())
        {
            super::database::require_retained_contract(store, scope)?;
        }
        Ok(())
    }
    pub(crate) fn require_all_previous(&mut self) -> Result<()> {
        self.required_previous = Some(
            self.state
                .as_ref()
                .context("graph replacement requires collection metadata")?
                .counts
                .files,
        );
        Ok(())
    }
    pub(crate) fn verify_model(&self, captured: &Option<ModelObjectSet>) -> Result<()> {
        ensure!(
            self.state
                .as_ref()
                .is_none_or(|state| state.configuration.model == *captured),
            "current configuration model differs from captured transaction assets"
        );
        Ok(())
    }
    pub(crate) fn load(
        plan: &map::MapPlan<'_>,
        scope: &CurrentScope,
        configuration: Option<&CollectionConfiguration>,
    ) -> Result<Self> {
        let old = plan.get(map::key(scope, map::MapDomain::Metadata, KEY)?)?;
        let mut state = old.as_deref().map(CollectionState::decode).transpose()?;
        let prior_selection = state
            .as_ref()
            .map(|s| s.configuration.vector_selection.clone())
            .unwrap_or_default();
        let mut required_previous = None;
        if let Some(configuration) = configuration {
            configuration.validate()?;
            if let Some(state) = &mut state {
                if !state
                    .configuration
                    .retains_existing_artifacts(configuration)?
                {
                    required_previous = Some(state.counts.files);
                }
                state.configuration = configuration.clone();
            } else {
                ensure!(
                    plan.root()? == map::empty_root(scope)?,
                    "current collection metadata missing from nonempty index; rebuild"
                );
                state = Some(CollectionState {
                    version: 1,
                    configuration: configuration.clone(),
                    counts: Default::default(),
                });
            }
        }
        Ok(Self {
            prior_selection,
            state,
            required_previous,
            visited_previous: 0,
        })
    }
    pub(crate) fn update(
        &mut self,
        previous: Option<&Artifact>,
        next: Option<&Artifact>,
    ) -> Result<()> {
        if self.state.is_none()
            && let Some(profile) = next
                .and_then(|next| next.retained.as_ref())
                .and_then(|retained| retained.profile.as_ref())
        {
            ensure!(
                !matches!(
                    airk_core::retention::AppliedRetention::from_profile(profile)?,
                    airk_core::retention::AppliedRetention::Selected { .. }
                ),
                "selected artifact requires authenticated collection configuration"
            );
        }
        if let Some(state) = &mut self.state {
            if let Some(previous) = previous {
                self.visited_previous += 1;
                state
                    .counts
                    .update(previous, &self.prior_selection, false)?;
            }
            if let Some(next) = next {
                let profile = next
                    .retained
                    .as_ref()
                    .and_then(|retained| retained.profile.as_ref())
                    .context("current collection extraction profile missing")?;
                match airk_core::retention::AppliedRetention::from_profile(profile)? {
                    airk_core::retention::AppliedRetention::Legacy { detail, .. } => ensure!(
                        detail == state.configuration.index_detail,
                        "current artifact legacy detail differs from applied fallback"
                    ),
                    airk_core::retention::AppliedRetention::Selected {
                        profile: target, ..
                    } => ensure!(
                        state.configuration.retained_profile == Some(target),
                        "current artifact retained profile differs from applied configuration"
                    ),
                }
                state
                    .counts
                    .update(next, &state.configuration.vector_selection, true)?;
            }
        }
        Ok(())
    }
    pub(crate) fn finish(
        self,
        plan: &mut map::MapPlan<'_>,
        scope: &CurrentScope,
    ) -> Result<Option<map::MapChange>> {
        let Some(state) = self.state else {
            return Ok(None);
        };
        ensure!(
            self.required_previous
                .is_none_or(|required| required == self.visited_previous),
            "changed collection configuration requires replacing every current artifact"
        );
        let change =
            map::MapChange::new(scope, map::MapDomain::Metadata, KEY, Some(state.bytes()?))?;
        plan.put(change.key(), change.value())?;
        Ok(Some(change))
    }
}

#[cfg(test)]
mod selected_configuration_guard_tests {
    use super::*;
    #[test]
    fn selected_artifact_cannot_bypass_collection_configuration() -> Result<()> {
        use airk_core::{
            ArtifactRef,
            extraction::ExtractionProfile,
            retention::{AppliedRetention, RetainedFamily, RetainedProfile},
        };
        let legacy = ExtractionProfile {
            contract_version: 2,
            adapter_id: "rust".into(),
            adapter_version: "fixture/1".into(),
            language: "rust".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: Default::default(),
            configuration_inputs: Default::default(),
        };
        let selected =
            AppliedRetention::bind(&legacy, RetainedProfile::Lean, RetainedFamily::Code)?;
        let mut artifact = Artifact {
            path: "kept.rs".into(),
            content_hash: "00".repeat(32),
            bytes: 1,
            chunks: vec![],
            language: "rust".into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: Some(ArtifactRef {
                source_encoding: None,
                payload_encoding: None,
                object: "01".repeat(32),
                object_bytes: 1,
                raw_source: false,
                chunks: 0,
                extracted_bytes: 0,
                facts: 0,
                profile: Some(selected),
            }),
        };
        let mut plan = CollectionPlan {
            prior_selection: Default::default(),
            state: None,
            required_previous: None,
            visited_previous: 0,
        };
        assert!(
            plan.update(None, Some(&artifact)).is_err(),
            "standalone selected artifact bypassed configured target/owner guard"
        );
        assert!(plan.state.is_none());
        artifact.retained.as_mut().unwrap().profile = Some(legacy);
        plan.update(None, Some(&artifact))?;
        assert!(
            plan.state.is_none(),
            "legacy low-level adapter unexpectedly created metadata"
        );
        Ok(())
    }
}
