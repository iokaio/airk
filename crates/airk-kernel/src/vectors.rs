// SPDX-License-Identifier: Apache-2.0
//! Explicit collection vector configuration, applied only by successful indexing.
use crate::Kernel;
use airk_core::{
    SearchQuery, SearchResponse,
    identity::{RevisionToken, Sha256Digest},
    vector_selection::VectorSelectionPolicy,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorPolicy {
    enabled: bool,
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
    pub default_search_mode: &'static str,
}

#[derive(Serialize)]
pub struct VectorSelectionStatus {
    pub policy: VectorSelectionPolicy,
    pub digest: Sha256Digest,
    pub applied_digest: Option<Sha256Digest>,
    pub pending_reindex: bool,
    pub origin: &'static str,
    pub publication: Option<RevisionToken>,
}

impl Kernel {
    pub fn install_vector_model(&self, source: &std::path::Path) -> Result<VectorStatus> {
        let _writer = self.store.lock()?;
        anyhow::ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        anyhow::ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain its source root"
        );
        let set = airk_retrieval::model::inspect_object_plan(source)?;
        airk_store::model_objects::install_input_model(source, &set, &quota, &self.collection)?;
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
                selection: None,
            },
        };
        if let Some(selection) = &policy.selection {
            selection.validate()?;
        }
        Ok(policy)
    }

    pub fn vectors_enabled(&self) -> Result<bool> {
        Ok(self.vector_policy()?.enabled)
    }

    /// Saved policy used for the next publication; does not interpret old indexes.
    pub fn vector_selection(&self) -> Result<VectorSelectionPolicy> {
        Ok(self.vector_policy()?.selection.unwrap_or_default())
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
        let selection_digest = policy.selection.unwrap_or_default().digest()?;
        let status = self.published_vector_status()?;
        let published = status.as_ref().is_some_and(|s| s.local_model_ready);
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
            pending_reindex: enabled != published || selection_pending_reindex,
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
            selection: policy.selection.clone(),
        })?;
        self.store.configure_vector_policy_bytes(
            &writer,
            &airk_store::quota::Quota::open(&self.data_root)?,
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
        self.selection_status(policy.selection.unwrap_or_default(), origin)
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
        Ok(VectorSelectionStatus {
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
            &airk_store::quota::Quota::open(&self.data_root)?,
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
