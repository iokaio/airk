// SPDX-License-Identifier: Apache-2.0
//! Explicit extraction-policy controls; saved policy takes effect on indexing.
use crate::Kernel;
use airk_core::{
    extraction::AdapterDescriptor,
    identity::{RevisionToken, Sha256Digest, framed_digest},
};
pub use airk_ingest::adapters::profile::ExtractionPolicy;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
pub struct ExtractionPolicyStatus {
    pub policy: ExtractionPolicy,
    pub configured_detail: airk_core::extraction::IndexDetail,
    pub published_detail: Option<airk_core::extraction::IndexDetail>,
    pub digest: Sha256Digest,
    pub origin: String,
    pub applied_digest: Option<Sha256Digest>,
    pub pending_reindex: bool,
    pub publication: Option<RevisionToken>,
    pub adapters: Vec<AdapterDescriptor>,
}

pub(crate) fn policy_digest(policy: &ExtractionPolicy) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"extraction-policy/1",
        &[&serde_json::to_vec(policy)?],
    ))
}
impl Kernel {
    pub fn applied_index_detail(&self) -> Result<Option<airk_core::extraction::IndexDetail>> {
        let Some(scope) = self.current_scope()? else {
            return Ok(None);
        };
        Ok(self
            .store
            .current_read(&scope)?
            .collection_state()?
            .map(|state| state.configuration.index_detail))
    }
    /// Rebuild reads saved policy without consulting obsolete derived catalogs.
    pub(crate) fn rebuild_extraction_policy(&self) -> Result<ExtractionPolicyStatus> {
        let bytes = self.store.extraction_policy_bytes(&self.root_id())?;
        let registry = airk_ingest::adapters::registry::AdapterRegistry::baseline();
        let policy = match &bytes {
            Some(bytes) => ExtractionPolicy::parse(bytes, &registry)?,
            None => ExtractionPolicy {
                version: 3,
                detail: Some(
                    self.applied_index_detail()?
                        .unwrap_or(airk_core::extraction::IndexDetail::AgentLean),
                ),
                rules: vec![],
            },
        };
        self.extraction_policy_status_with_catalog(
            policy,
            if bytes.is_some() {
                "explicit"
            } else {
                "default"
            },
            None,
        )
    }

    pub fn extraction_adapters() -> Vec<AdapterDescriptor> {
        airk_ingest::adapters::registry::AdapterRegistry::baseline()
            .descriptors()
            .cloned()
            .collect()
    }
    pub fn extraction_policy(&self) -> Result<ExtractionPolicyStatus> {
        self.rebuild_extraction_policy()
    }
    fn extraction_policy_status(
        &self,
        policy: ExtractionPolicy,
        origin: &str,
    ) -> Result<ExtractionPolicyStatus> {
        self.extraction_policy_status_with_catalog(policy, origin, None)
    }
    fn extraction_policy_status_with_catalog(
        &self,
        policy: ExtractionPolicy,
        origin: &str,
        catalog: Option<airk_store::blobs::Catalog>,
    ) -> Result<ExtractionPolicyStatus> {
        let registry = airk_ingest::adapters::registry::AdapterRegistry::baseline();
        policy.validate(&registry)?;
        let _ = catalog;
        let current = self
            .current_scope()?
            .map(|scope| self.store.current_read(&scope))
            .transpose()?;
        let state = current
            .as_ref()
            .map(|read| read.collection_state())
            .transpose()?
            .flatten();
        let applied_digest = state
            .as_ref()
            .map(|s| s.configuration.extraction_policy_digest);
        let digest = policy_digest(&policy)?;
        let publication = current
            .as_ref()
            .map(|r| r.token().map(crate::current::runtime::revision))
            .transpose()?;
        Ok(ExtractionPolicyStatus {
            configured_detail: policy.detail.unwrap_or_default(),
            published_detail: state.as_ref().map(|s| s.configuration.index_detail),
            policy,
            digest,
            origin: origin.into(),
            applied_digest,
            pending_reindex: applied_digest != Some(digest),
            publication,
            adapters: registry.descriptors().cloned().collect(),
        })
    }
    pub fn validate_extraction_policy(
        &self,
        policy: &ExtractionPolicy,
    ) -> Result<ExtractionPolicyStatus> {
        self.extraction_policy_status(policy.clone(), "preview")
    }
    pub fn configure_index_detail(
        &self,
        detail: airk_core::extraction::IndexDetail,
    ) -> Result<ExtractionPolicyStatus> {
        let writer = self.store.lock()?;
        let mut policy = self.extraction_policy()?.policy;
        policy.version = 3;
        policy.detail = Some(detail);
        self.validate_extraction_policy(&policy)?;
        let quota = self.collection_quota()?;
        self.store.configure_extraction_policy_bytes(
            &writer,
            &quota,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        self.extraction_policy_status(policy, "explicit")
    }
    pub fn configure_extraction_policy(
        &self,
        policy: ExtractionPolicy,
    ) -> Result<ExtractionPolicyStatus> {
        let writer = self.store.lock()?;
        self.validate_extraction_policy(&policy)?;
        let quota = self.collection_quota()?;
        self.store.configure_extraction_policy_bytes(
            &writer,
            &quota,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        self.extraction_policy_status(policy, "explicit")
    }
}
