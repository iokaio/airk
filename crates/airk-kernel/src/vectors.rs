// SPDX-License-Identifier: Apache-2.0
//! Explicit collection vector configuration, applied only by successful indexing.
use crate::Kernel;
use airk_core::{
    SearchQuery, SearchResponse,
    identity::{RevisionToken, Sha256Digest},
    vector_selection::VectorSelectionPolicy,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorPolicy {
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<airk_core::model_objects::ModelObjectSet>,
    // Saved user configuration survives derived-index rebuilds. Absence means
    // no explicit exclusions, preserving an existing collection's enable switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selection: Option<VectorSelectionPolicy>,
}

#[derive(Serialize)]
pub struct VectorStatus {
    pub enabled: bool,
    pub published: bool,
    pub indexed_vectors: usize,
    pub semantic_readiness: Option<airk_core::readiness::SemanticReadiness>,
    pub discovery_readiness: Option<airk_core::readiness::DiscoveryReadiness>,
    pub selection_digest: Sha256Digest,
    pub applied_selection_digest: Option<Sha256Digest>,
    pub selection_pending_reindex: bool,
    pub pending_reindex: bool,
    pub model_pending_reindex: bool,
    pub default_search_mode: &'static str,
}

#[derive(Serialize)]
pub struct VectorSelectionStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<VectorSelectionPreview>,
    pub policy: VectorSelectionPolicy,
    pub digest: Sha256Digest,
    pub applied_digest: Option<Sha256Digest>,
    pub pending_reindex: bool,
    pub origin: &'static str,
    pub publication: Option<RevisionToken>,
}

#[derive(Serialize)]
pub struct VectorSelectionPreview {
    pub examined_files: usize,
    pub eligible_files: usize,
    pub excluded_files: usize,
    pub changed_files: usize,
    pub complete: bool,
    pub affected_paths: Vec<(String, airk_core::vector_selection::VectorSelectionDecision)>,
    pub paths_truncated: bool,
    pub selected_units: Option<u64>,
}

impl Kernel {
    /// Explicit trusted setup; source scans never provision or load model weights.
    pub fn install_service_default_vector_model(&self) -> Result<VectorStatus> {
        anyhow::ensure!(
            self.store.shared_model_runtime().is_some(),
            "service model binding required"
        );
        let _writer = self.store.lock()?;
        anyhow::ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        self.configured_model_objects(true)?;
        self.vector_status()
    }

    /// Install an offline default once; never overwrite an existing/custom model.
    pub fn install_default_vector_model(&self, source: &std::path::Path) -> Result<VectorStatus> {
        let _writer = self.store.lock()?;
        anyhow::ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        let quota = self.collection_quota()?;
        anyhow::ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain its source root"
        );
        if self.store.shared_model_runtime().is_some() {
            if self.selected_shared_model()?.is_some() {
                return self.vector_status();
            }
            self.configured_model_objects(true)?;
            return self.vector_status();
        }
        let set = airk_retrieval::model::inspect_object_plan(source)?;
        let installed = airk_store::model_objects::install_input_model_if_missing(
            source,
            &set,
            &quota,
            &self.collection,
        )?;
        if !installed {
            // Preserved assets must be complete and valid, not merely present.
            airk_retrieval::model::inspect_object_plan(&self.data_root.join("models/local"))?;
        }
        self.vector_status()
    }

    pub fn install_vector_model(&self, source: &std::path::Path) -> Result<VectorStatus> {
        self.install_vector_model_checked(source, None)
    }

    pub fn install_service_vector_model(
        &self,
        source: &std::path::Path,
        manifest: Sha256Digest,
    ) -> Result<VectorStatus> {
        anyhow::ensure!(
            self.store.shared_model_runtime().is_some(),
            "service model binding required"
        );
        self.install_vector_model_checked(source, Some(manifest))
    }

    fn install_vector_model_checked(
        &self,
        source: &std::path::Path,
        expected: Option<Sha256Digest>,
    ) -> Result<VectorStatus> {
        let _writer = self.store.lock()?;
        anyhow::ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        let quota = self.collection_quota()?;
        anyhow::ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain its source root"
        );
        if self.store.shared_model_runtime().is_some() {
            let objects = airk_retrieval::embedding_worker::connector()?
                .provision_model(&source.canonicalize()?)?;
            anyhow::ensure!(
                expected.is_none_or(|expected| objects
                    .assets
                    .get(&airk_core::model_objects::ModelAssetName::ModelJson)
                    .is_some_and(|manifest| manifest.sha256 == expected)),
                "selected model definition changed during setup"
            );
            self.store.installed_model(&objects)?;
            let mut policy = self.vector_policy()?;
            policy.model = Some(objects);
            self.store.configure_vector_policy_bytes(
                &_writer,
                &quota,
                &self.root_id(),
                &serde_json::to_vec(&policy)?,
            )?;
            return self.vector_status();
        }
        let set = airk_retrieval::model::inspect_object_plan(source)?;
        airk_store::model_objects::install_input_model(source, &set, &quota, &self.collection)?;
        self.vector_status()
    }

    /// Explicitly replace a custom selection with the paired bundled default.
    pub fn select_service_bundled_model(&self) -> Result<VectorStatus> {
        anyhow::ensure!(
            self.store.shared_model_runtime().is_some(),
            "service model binding required"
        );
        let writer = self.store.lock()?;
        anyhow::ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        let objects = airk_retrieval::embedding_worker::connector()?.provision_bundled()?;
        anyhow::ensure!(
            objects.shared.is_some()
                && self
                    .store
                    .shared_model_default()
                    .is_some_and(|default| *default == objects.local_set()),
            "shared helper default model differs from paired metadata"
        );
        self.store.installed_model(&objects)?;
        let mut policy = self.vector_policy()?;
        policy.model = Some(objects);
        self.store.configure_vector_policy_bytes(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        self.vector_status()
    }

    fn vector_policy(&self) -> Result<VectorPolicy> {
        // Validate collection ownership without opening obsolete derived evidence.
        self.current_scope()?;
        let bytes = self.store.vector_policy_bytes(&self.root_id())?;
        let policy = match bytes {
            Some(bytes) => serde_json::from_slice::<VectorPolicy>(&bytes)?,
            None => VectorPolicy {
                enabled: true,
                model: None,
                selection: None,
            },
        };
        if let Some(model) = &policy.model {
            model.validate()?;
            anyhow::ensure!(
                model.shared.is_some() && self.store.shared_model_runtime().is_some(),
                "installed model policy requires its bound service runtime"
            );
        }
        if let Some(selection) = &policy.selection {
            selection.validate()?;
        }
        Ok(policy)
    }

    pub(crate) fn selected_shared_model(
        &self,
    ) -> Result<Option<airk_core::model_objects::ModelObjectSet>> {
        Ok(self.vector_policy()?.model)
    }
    pub fn vectors_enabled(&self) -> Result<bool> {
        Ok(self.vector_policy()?.enabled)
    }

    /// Saved policy used for the next publication; does not interpret old indexes.
    pub fn vector_selection(&self) -> Result<VectorSelectionPolicy> {
        self.vector_policy()?
            .selection
            .map(Ok)
            .unwrap_or_else(|| self.default_vector_selection())
    }

    fn default_vector_selection(&self) -> Result<VectorSelectionPolicy> {
        Ok(
            if self.rebuild_extraction_policy()?.configured_detail
                == airk_core::extraction::IndexDetail::AgentLean
            {
                VectorSelectionPolicy::agent_lean()
            } else {
                VectorSelectionPolicy::default()
            },
        )
    }

    fn published_vector_status(&self) -> Result<Option<crate::Status>> {
        if self.current_scope()?.is_some() {
            self.status().map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn vector_status(&self) -> Result<VectorStatus> {
        self.status_for_vector_policy(self.vector_policy()?)
    }

    fn status_for_vector_policy(&self, policy: VectorPolicy) -> Result<VectorStatus> {
        let enabled = policy.enabled;
        let selection_digest = policy
            .selection
            .map(Ok)
            .unwrap_or_else(|| self.default_vector_selection())?
            .digest()?;
        let status = self.published_vector_status()?;
        let published = status.as_ref().is_some_and(|s| s.local_model_ready);
        let model_pending_reindex = if self.store.shared_model_runtime().is_some() {
            let applied = if self.current_scope()?.is_some() {
                self.current_reader()?
                    .collection_state()?
                    .and_then(|s| s.configuration.model)
            } else {
                None
            };
            applied != self.configured_model_objects(false)?
        } else {
            false
        };
        let applied_selection_digest = status.as_ref().and_then(|s| s.vector_selection_digest);
        let selection_pending_reindex = applied_selection_digest != Some(selection_digest);
        Ok(VectorStatus {
            enabled,
            published,
            indexed_vectors: status.as_ref().map_or(0, |s| s.indexed_vectors),
            semantic_readiness: status.as_ref().map(|s| s.semantic_readiness.clone()),
            discovery_readiness: status.as_ref().map(|s| s.discovery_readiness.clone()),
            selection_digest,
            applied_selection_digest,
            selection_pending_reindex,
            pending_reindex: enabled != published
                || selection_pending_reindex
                || model_pending_reindex,
            model_pending_reindex,
            default_search_mode: if enabled
                && status.as_ref().is_some_and(|s| {
                    s.semantic_readiness.state == airk_core::readiness::SemanticState::Disabled
                }) {
                "unavailable"
            } else if !enabled
                || status.as_ref().is_some_and(|s| {
                    s.semantic_readiness.state != airk_core::readiness::SemanticState::Ready
                        && s.semantic_readiness.default_search_policy
                            == airk_core::readiness::DefaultSearchPolicy::LexicalWhilePending
                })
            {
                "lexical"
            } else if status.as_ref().is_some_and(|s| {
                s.semantic_readiness.state != airk_core::readiness::SemanticState::Ready
                    && s.semantic_readiness.default_search_policy
                        == airk_core::readiness::DefaultSearchPolicy::RequireComplete
            }) {
                "requires_complete"
            } else {
                "hybrid"
            },
        })
    }

    pub fn configure_vectors(&self, enabled: bool) -> Result<VectorStatus> {
        let writer = self.store.lock()?;
        let mut policy = self.vector_policy()?;
        policy.enabled = enabled;
        let status = self.status_for_vector_policy(VectorPolicy {
            enabled: policy.enabled,
            model: policy.model.clone(),
            selection: policy.selection.clone(),
        })?;
        self.store.configure_vector_policy_bytes(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        Ok(status)
    }

    pub fn vector_selection_status(&self) -> Result<VectorSelectionStatus> {
        let policy = self.vector_policy()?;
        let origin = if policy.selection.is_some() {
            "explicit"
        } else {
            "default"
        };
        self.selection_status(
            policy
                .selection
                .map(Ok)
                .unwrap_or_else(|| self.default_vector_selection())?,
            origin,
        )
    }

    fn selection_status(
        &self,
        policy: VectorSelectionPolicy,
        origin: &'static str,
    ) -> Result<VectorSelectionStatus> {
        let policy: VectorSelectionPolicy = serde_json::from_slice(&policy.canonical_bytes()?)?;
        let digest = policy.digest()?;
        let status = self.published_vector_status()?;
        let applied_digest = status.as_ref().and_then(|s| s.vector_selection_digest);
        let preview = if origin == "preview" {
            let mut preview = VectorSelectionPreview {
                examined_files: 0,
                eligible_files: 0,
                excluded_files: 0,
                changed_files: 0,
                complete: true,
                affected_paths: Vec::new(),
                paths_truncated: false,
                selected_units: None,
            };
            if let Some(scope) = self.current_scope()? {
                let read = self.store.current_read(&scope)?;
                let page = read.lookup_paths(
                    airk_store::current::postings::Lookup::Files,
                    "",
                    None,
                    4097,
                )?;
                preview.complete = page.paths.len() <= 4096 && page.next_after.is_none();
                let selected = policy.validated()?;
                let applied = read
                    .collection_state()?
                    .context("selection preview configuration missing")?
                    .configuration
                    .vector_selection;
                let applied = applied.validated()?;
                for path in page.paths.into_iter().take(4096) {
                    let artifact = read
                        .artifact(&path)?
                        .context("selection preview artifact missing")?;
                    let profile = artifact
                        .retained
                        .as_ref()
                        .and_then(|r| r.profile.as_ref())
                        .context("selection preview profile missing")?;
                    let decision = selected.decision(&path, profile, &artifact.language)?;
                    let old = applied.decision(&path, profile, &artifact.language)?;
                    preview.examined_files += 1;
                    preview.eligible_files += usize::from(decision.eligible);
                    preview.excluded_files += usize::from(!decision.eligible);
                    if decision.eligible != old.eligible {
                        preview.changed_files += 1;
                        if preview.affected_paths.len() < 32 {
                            preview.affected_paths.push((path, decision));
                        } else {
                            preview.paths_truncated = true;
                        }
                    }
                }
            }
            Some(preview)
        } else {
            None
        };
        Ok(VectorSelectionStatus {
            preview,
            policy,
            digest,
            applied_digest,
            pending_reindex: applied_digest != Some(digest),
            origin,
            publication: status.and_then(|s| s.publication.map(|p| p.token)),
        })
    }

    pub fn validate_vector_selection(
        &self,
        policy: &VectorSelectionPolicy,
    ) -> Result<VectorSelectionStatus> {
        self.vector_policy()?;
        self.selection_status(policy.clone(), "preview")
    }

    pub fn configure_vector_selection(
        &self,
        selection: VectorSelectionPolicy,
    ) -> Result<VectorSelectionStatus> {
        let selection: VectorSelectionPolicy =
            serde_json::from_slice(&selection.canonical_bytes()?)?;
        let writer = self.store.lock()?;
        let mut policy = self.vector_policy()?;
        policy.selection = Some(selection.clone());
        let status = self.selection_status(selection, "explicit")?;
        self.store.configure_vector_policy_bytes(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        Ok(status)
    }

    /// Reads never provision assets or silently downgrade an enabled collection.
    pub fn default_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        if self.vectors_enabled()? {
            self.current_search(query, crate::current::CurrentSearchMode::Default)
        } else {
            self.search(query)
        }
    }
}
