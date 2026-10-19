// SPDX-License-Identifier: Apache-2.0
//! Default hybrid retrieval and a persisted collection-level vector opt-out.
use crate::Kernel;
use airk_core::{SearchQuery, SearchResponse};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorPolicy {
    enabled: bool,
}

#[derive(Serialize)]
pub struct VectorStatus {
    pub enabled: bool,
    pub published: bool,
    pub indexed_vectors: usize,
    pub pending_reindex: bool,
    pub default_search_mode: &'static str,
}

impl Kernel {
    pub fn install_vector_model(&self, source: &std::path::Path) -> Result<VectorStatus> {
        let _writer = self.store.lock()?;
        self.load_optional()?;
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

    pub fn vectors_enabled(&self) -> Result<bool> {
        let bytes = self.store.vector_policy_bytes(&self.root_id())?;
        Ok(match bytes {
            Some(bytes) => serde_json::from_slice::<VectorPolicy>(&bytes)?.enabled,
            None => true,
        })
    }

    pub fn vector_status(&self) -> Result<VectorStatus> {
        let enabled = self.vectors_enabled()?;
        let status = if self.load_optional()?.is_some() {
            Some(self.status()?)
        } else {
            None
        };
        let published = status.as_ref().is_some_and(|s| s.local_model_ready);
        Ok(VectorStatus {
            enabled,
            published,
            indexed_vectors: status.as_ref().map_or(0, |s| s.indexed_vectors),
            pending_reindex: enabled != published,
            default_search_mode: if enabled { "hybrid" } else { "lexical" },
        })
    }

    pub fn configure_vectors(&self, enabled: bool) -> Result<VectorStatus> {
        let writer = self.store.lock()?;
        self.load_optional()?;
        self.store.configure_vector_policy_bytes(
            &writer,
            &airk_store::quota::Quota::open(&self.data_root)?,
            &self.root_id(),
            &serde_json::to_vec(&VectorPolicy { enabled })?,
        )?;
        self.vector_status()
    }

    /// Reads never provision assets or silently downgrade an enabled collection.
    pub fn default_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        if self.vectors_enabled()? {
            self.hybrid_search(query)
        } else {
            self.search(query)
        }
    }
}
