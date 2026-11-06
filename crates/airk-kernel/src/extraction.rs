// SPDX-License-Identifier: Apache-2.0
//! Explicit extraction-policy controls; saved policy takes effect on indexing.
use crate::Kernel;
use airk_core::{
    extraction::AdapterDescriptor,
    identity::{RevisionToken, Sha256Digest, framed_digest},
};
pub use airk_ingest::adapters::profile::ExtractionPolicy;
use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct ExtractionPolicyStatus {
    pub policy: ExtractionPolicy,
    pub configured_detail: airk_core::extraction::IndexDetail,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_retained_profile: Option<airk_core::retention::RetainedProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_retained_profile: Option<airk_core::retention::RetainedProfile>,
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
                retained_profile: None,
                legacy_fallback_detail: None,
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
        self.require_unrestricted_read()?;
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
            configured_detail: policy.legacy_detail(),
            configured_retained_profile: policy.retained_profile,
            published_retained_profile: state
                .as_ref()
                .and_then(|s| s.configuration.retained_profile),
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
        policy.retained_profile = None;
        policy.legacy_fallback_detail = None;
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
        if policy.version == 4 {
            let existing = self.extraction_policy()?;
            anyhow::ensure!(
                existing.published_retained_profile == policy.retained_profile
                    && existing.configured_retained_profile == policy.retained_profile
                    && existing.policy.legacy_fallback_detail == policy.legacy_fallback_detail
                    && policy.retained_profile.is_some(),
                "changing retained profiles requires explicit reviewed native migration"
            );
        }
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

impl Kernel {
    fn reviewed_retention_policy(
        &self,
        target: airk_core::retention::RetainedProfile,
    ) -> Result<(ExtractionPolicyStatus, ExtractionPolicy)> {
        self.require_unrestricted_read()?;
        anyhow::ensure!(
            self.service_collection.is_some(),
            "retained migration requires native owned collection capability"
        );
        let status = self.extraction_policy()?;
        let fallback = status
            .published_detail
            .context("publish a legacy baseline before reviewing retained migration")?;
        let mut desired = status.policy.clone();
        desired.version = 4;
        desired.detail = None;
        desired.retained_profile = Some(target);
        desired.legacy_fallback_detail = Some(fallback);
        desired.validate(&airk_ingest::adapters::registry::AdapterRegistry::baseline())?;
        Ok((status, desired))
    }
    /// Bounded metadata-only review; no extraction, source I/O or mutation.
    pub fn preview_retained_migration(
        &self,
        target: airk_core::retention::RetainedProfile,
    ) -> Result<airk_core::retention::RetentionMigrationReview> {
        let (status, desired) = self.reviewed_retention_policy(target)?;
        let review = airk_core::retention::RetentionMigrationReview {
            schema: 1,
            expected_publication: status
                .publication
                .context("current publication is unavailable")?,
            expected_applied_policy: status
                .applied_digest
                .context("applied extraction policy unavailable")?,
            reviewed_configured_policy: status.digest,
            desired_policy: policy_digest(&desired)?,
            target,
        };
        review.validate()?;
        Ok(review)
    }
    /// Executed under existing exact Configure admission. Source publication remains separate.
    pub fn configure_retained_migration(
        &self,
        review: &airk_core::retention::RetentionMigrationReview,
    ) -> Result<airk_core::retention::RetentionMigrationResult> {
        anyhow::ensure!(
            self.service_selection.is_none(),
            "migration must run on the native unpinned owner; external selection pins remain authoritative"
        );
        let writer = self.store.lock()?;
        let (status, desired) = self.reviewed_retention_policy(review.target)?;
        review.verify_current(
            &status
                .publication
                .context("current publication unavailable")?,
            status
                .applied_digest
                .context("applied policy unavailable")?,
            status.digest,
        )?;
        anyhow::ensure!(
            policy_digest(&desired)? == review.desired_policy,
            "reviewed target policy changed"
        );
        let result = airk_core::retention::RetentionMigrationResult::for_review(review)?;
        // Restoring the already-applied choice can still require replacing a
        // different pending desired policy. Publication work and policy save are distinct.
        if status.digest == review.desired_policy && !result.source_pass_requested {
            return Ok(result);
        }
        let quota = self.collection_quota()?;
        self.service_collection
            .as_ref()
            .context("native collection missing")?
            .require_extraction_contract(
                &self.store,
                &writer,
                &quota,
                airk_core::retention::PROFILE_CONTRACT,
            )?;
        if status.digest != review.desired_policy {
            self.store.configure_extraction_policy_bytes(
                &writer,
                &quota,
                &self.root_id(),
                &serde_json::to_vec(&desired)?,
            )?;
        }
        Ok(result)
    }
}

impl Kernel {
    /// Parse fixed private policy metadata only. Body and configured digests have
    /// distinct canonical encodings and are deliberately reported separately.
    pub fn split_retained_policy_reference(
        body: &[u8],
    ) -> Result<Option<airk_core::retention::SplitExtractionPolicyReference>> {
        let policy = ExtractionPolicy::parse(
            body,
            &airk_ingest::adapters::registry::AdapterRegistry::baseline(),
        )?;
        let Some(target) = policy.retained_profile else {
            return Ok(None);
        };
        let reference = airk_core::retention::SplitExtractionPolicyReference {
            schema: 1,
            policy_body_digest: framed_digest(b"airk/split-extraction-policy-body/1", &[body]),
            configured_policy_digest: policy_digest(&policy)?,
            encoded_bytes: u32::try_from(body.len())?,
            retained_profile: target,
            legacy_fallback_detail: policy
                .legacy_fallback_detail
                .context("retained policy fallback absent")?,
            required_extraction_contract: airk_core::retention::PROFILE_CONTRACT,
        };
        reference.validate_body(body)?;
        Ok(Some(reference))
    }
    pub(crate) fn install_split_retained_policy(
        &self,
        work: &airk_store::service::registry::catalog::split::ClaimedSplitWork,
    ) -> Result<()> {
        use airk_core::catalog_split::SplitJobPurpose;
        anyhow::ensure!(
            work.job.purpose == SplitJobPurpose::DestinationBuild
                && self.split_authority.is_some()
                && self.service_selection.is_none(),
            "split retained policy needs exact unpinned destination build owner"
        );
        let Some(reference) = &work.snapshot.request.extraction_policy else {
            anyhow::ensure!(
                work.extraction_policy.is_none(),
                "legacy split unexpectedly carries private policy"
            );
            return Ok(());
        };
        let bytes = work
            .extraction_policy
            .as_ref()
            .context("native saved split policy absent")?
            .bytes_for(&work.snapshot.request)?;
        anyhow::ensure!(
            Self::split_retained_policy_reference(bytes)?.as_ref() == Some(reference),
            "saved split policy semantics changed"
        );
        let quota = self.collection_quota()?;
        anyhow::ensure!(
            quota.funding_use().is_some(),
            "split retained metadata needs admitted funding"
        );
        let writer = self.store.lock()?;
        self.service_collection
            .as_ref()
            .context("split native collection absent")?
            .require_extraction_contract(
                &self.store,
                &writer,
                &quota,
                airk_core::retention::PROFILE_CONTRACT,
            )?;
        if self
            .store
            .extraction_policy_bytes(&self.root_id())?
            .as_deref()
            != Some(bytes)
        {
            self.store.configure_extraction_policy_bytes(
                &writer,
                &quota,
                &self.root_id(),
                bytes,
            )?;
        }
        Ok(())
    }
}
