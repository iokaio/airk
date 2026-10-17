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
    pub fn extraction_adapters() -> Vec<AdapterDescriptor> {
        airk_ingest::adapters::registry::AdapterRegistry::baseline()
            .descriptors()
            .cloned()
            .collect()
    }
    pub fn extraction_policy(&self) -> Result<ExtractionPolicyStatus> {
        let bytes = self.store.extraction_policy_bytes(&self.root_id())?;
        let registry = airk_ingest::adapters::registry::AdapterRegistry::baseline();
        let catalog = self.store.catalog()?;
        let policy = match &bytes {
            Some(bytes) => ExtractionPolicy::parse(bytes, &registry)?,
            None => {
                let detail = catalog
                    .as_ref()
                    .map(|c| {
                        c.inventory
                            .as_ref()
                            .and_then(|i| i.index_detail)
                            .unwrap_or_default()
                    })
                    .unwrap_or(airk_core::extraction::IndexDetail::Balanced);
                ExtractionPolicy {
                    detail: (detail != airk_core::extraction::IndexDetail::Detailed)
                        .then_some(detail),
                    version: if detail == airk_core::extraction::IndexDetail::Detailed {
                        1
                    } else {
                        3
                    },
                    rules: vec![],
                }
            }
        };
        self.extraction_policy_status_with_catalog(
            policy,
            if bytes.is_some() {
                "explicit"
            } else {
                "default"
            },
            catalog,
        )
    }
    fn extraction_policy_status(
        &self,
        policy: ExtractionPolicy,
        origin: &str,
    ) -> Result<ExtractionPolicyStatus> {
        self.extraction_policy_status_with_catalog(policy, origin, self.store.catalog()?)
    }
    fn extraction_policy_status_with_catalog(
        &self,
        policy: ExtractionPolicy,
        origin: &str,
        catalog: Option<airk_store::blobs::Catalog>,
    ) -> Result<ExtractionPolicyStatus> {
        let registry = airk_ingest::adapters::registry::AdapterRegistry::baseline();
        policy.validate(&registry)?;
        if let Some(catalog) = &catalog {
            anyhow::ensure!(
                catalog.snapshot.root == self.root_id()
                    && catalog.snapshot.collection == self.collection,
                "foreign extraction policy collection"
            );
        }
        let applied_digest = catalog
            .as_ref()
            .and_then(|catalog| catalog.inventory.as_ref())
            .and_then(|inventory| inventory.extraction_policy_digest);
        let digest = policy_digest(&policy)?;
        let publication = catalog
            .as_ref()
            .and_then(|catalog| catalog.receipt.as_ref())
            .map(|receipt| receipt.token);
        Ok(ExtractionPolicyStatus {
            configured_detail: policy.detail.unwrap_or_default(),
            published_detail: catalog.as_ref().map(|c| {
                c.inventory
                    .as_ref()
                    .and_then(|i| i.index_detail)
                    .unwrap_or_default()
            }),
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
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
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
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        self.store.configure_extraction_policy_bytes(
            &writer,
            &quota,
            &self.root_id(),
            &serde_json::to_vec(&policy)?,
        )?;
        self.extraction_policy_status(policy, "explicit")
    }
}
