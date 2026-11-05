// SPDX-License-Identifier: Apache-2.0
//! Durable enforcement copy outside replaceable index incarnations. The native
//! registry remains authority; absence or disagreement never means an empty policy.
use airk_core::{
    catalog_policy::SourcePolicyApplyReport,
    identity::{Incarnation, RevisionToken},
    source_policy::{MAX_POLICY_BYTES, SourcePolicy},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

const FILE: &str = "source-policy.json";
const MAX_BYTES: usize = MAX_POLICY_BYTES + 16384;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyCapsule {
    pub schema: u32,
    pub policy: SourcePolicy,
    pub operation_id: Incarnation,
    pub application: Option<SourcePolicyApplyReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_removal: Option<PolicyRemovalPlan>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRemovalPlan {
    pub base: RevisionToken,
    pub removed_artifacts: u64,
    pub retired_components: Vec<String>,
}
impl PolicyCapsule {
    pub(crate) fn validate_registration(
        &self,
        registration: &airk_core::service::ProjectRegistration,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.policy.owner_binding == registration.binding
                && self.policy.collection == registration.collection
                && self.policy.roots.iter().filter(|root| root.owner).count() == 1
                && self
                    .policy
                    .roots
                    .iter()
                    .any(|root| root.owner && root.canonical_path == registration.root),
            "retained source policy belongs to another registration"
        );
        Ok(())
    }

    /// The owning retirement lease must remain held. Missing application bytes
    /// cannot be reconstructed from a registry report: preserve a pending deny.
    pub(crate) fn retirement_copy(
        directory: &Path,
        registration: &airk_core::service::ProjectRegistration,
        snapshot: &airk_core::catalog_policy::PolicySnapshot,
    ) -> Result<Option<Self>> {
        snapshot.validate()?;
        if let Some(existing) = Self::load(directory)? {
            existing.validate_registration(registration)?;
            if existing.policy == snapshot.policy && existing.operation_id == snapshot.operation_id
            {
                return Ok(None);
            }
        }
        let copy = Self {
            schema: 1,
            policy: snapshot.policy.clone(),
            operation_id: snapshot.operation_id,
            application: None,
            planned_removal: None,
        };
        copy.validate_registration(registration)?;
        Ok(Some(copy))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported source policy capsule");
        self.policy.validate()?;
        if let Some(plan) = &self.planned_removal {
            ensure!(
                plan.removed_artifacts <= 100_000
                    && plan.retired_components.len() <= 2
                    && plan
                        .retired_components
                        .iter()
                        .all(|name| matches!(name.as_str(), "analysis" | "embedding_cache"))
                    && plan
                        .retired_components
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == plan.retired_components.len(),
                "invalid policy removal plan"
            );
        }
        if let Some(report) = &self.application {
            report.validate()?;
            ensure!(
                report.policy_digest == self.policy.identity()?
                    && report.operation_id == self.operation_id,
                "source policy capsule receipt mismatch"
            );
            if let Some(plan) = &self.planned_removal {
                ensure!(
                    report.removed_artifacts == plan.removed_artifacts
                        && report.retired_components == plan.retired_components,
                    "policy application differs from saved removal plan"
                );
            }
        }
        airk_core::index_catalog::encoded_size(self, MAX_BYTES)?;
        Ok(())
    }
    pub fn load(directory: &Path) -> Result<Option<Self>> {
        let path = directory.join(FILE);
        crate::managed_collection::validate_ancestors(directory)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_BYTES as u64,
            "invalid or oversized source policy capsule"
        );
        crate::managed_collection::reject_file_aliases(&path, &metadata)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected source policy capsule"
            );
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_BYTES,
            "source policy capsule exceeds byte budget"
        );
        let capsule: Self =
            serde_json::from_slice(&bytes).context("invalid source policy capsule")?;
        capsule.validate()?;
        Ok(Some(capsule))
    }
    /// Callers retain the native worker/configuration exclusion and quota grant.
    /// This method writes only AIRK-owned derived metadata, never source paths.
    pub fn save(&self, directory: &Path) -> Result<()> {
        self.validate()?;
        crate::managed_collection::validate_ancestors(directory)?;
        fs::create_dir_all(directory)?;
        if let Some(existing) = Self::load(directory)? {
            ensure!(
                existing.policy.index_id == self.policy.index_id
                    && existing.policy.owner_binding == self.policy.owner_binding
                    && existing.policy.collection == self.policy.collection
                    && existing.policy.epoch <= self.policy.epoch,
                "source policy capsule identity or epoch changed"
            );
            if existing.policy.epoch == self.policy.epoch {
                ensure!(
                    existing.policy == self.policy && existing.operation_id == self.operation_id,
                    "source policy epoch was reused"
                );
                ensure!(
                    existing.application.is_none() || self.application.is_some(),
                    "completed policy application cannot become pending again"
                );
            }
        }
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&serde_json::to_vec(self)?)?;
        crate::durability::persist_synced(temporary, &directory.join(FILE))
    }
}
