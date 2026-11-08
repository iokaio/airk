// SPDX-License-Identifier: Apache-2.0
//! Public provenance for a native import of an original standalone connection.
//! These descriptive values are not authority: only the native origin owner and
//! an actual pinned adoption publication can create the corresponding grant.
use crate::{
    catalog_legacy_adoption::{LegacyAdoptionPlan, LegacyAdoptionPublication},
    catalog_management::MAX_RECEIPT_BYTES,
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::encoded_size,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// One independently authenticated original declaration in a fixed native batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneConfigurationClaim {
    pub schema: u32,
    pub origin_sha256: Sha256Digest,
    pub config_sha256: Sha256Digest,
    pub executable_sha256: Sha256Digest,
    pub selected_entry_sha256: Sha256Digest,
    pub container_path: Vec<String>,
    pub original_server: String,
    pub no_default_project: bool,
}
impl StandaloneConfigurationClaim {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.container_path.len() <= 16
                && self
                    .container_path
                    .iter()
                    .all(|v| !v.is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control))
                && !self.original_server.is_empty()
                && self.original_server.len() <= 256
                && !self.original_server.chars().any(char::is_control),
            "invalid standalone configuration claim"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandaloneAdoptionProvenance {
    pub schema: u32,
    pub operation_id: Incarnation,
    /// Includes both original and adopted data roots, unchanged source roots and
    /// aliases, and exact origin/config/executable/archive/source digests.
    pub plan: Box<LegacyAdoptionPlan>,
    /// Original publication identity and the distinct actual converted head.
    pub publication: LegacyAdoptionPublication,
    pub original_server: String,
    pub no_default_project: bool,
    pub configuration_claims: Vec<StandaloneConfigurationClaim>,
}
impl StandaloneAdoptionProvenance {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1,
            "unsupported standalone adoption provenance"
        );
        self.plan.validate()?;
        ensure!(
            self.operation_id
                == crate::catalog_management::catalog_operation_id(
                    self.plan.setup.caller,
                    &self.plan.setup.key
                )?,
            "standalone operation differs from canonical adoption identity"
        );
        self.publication.validate_plan(&self.plan)?;
        ensure!(
            !self.configuration_claims.is_empty() && self.configuration_claims.len() <= 32,
            "standalone claim roster exceeds bound"
        );
        for claim in &self.configuration_claims {
            claim.validate()?;
            ensure!(
                claim.no_default_project == self.no_default_project,
                "standalone claims disagree on default project semantics"
            );
        }
        ensure!(
            self.configuration_claims
                .windows(2)
                .all(|v| v[0].origin_sha256 < v[1].origin_sha256),
            "standalone claims must be unique and canonically ordered"
        );
        let primary = self
            .configuration_claims
            .iter()
            .find(|c| c.origin_sha256 == self.plan.origin_sha256)
            .ok_or_else(|| anyhow::anyhow!("standalone primary claim is missing"))?;
        ensure!(
            primary.config_sha256 == self.plan.config_sha256
                && primary.executable_sha256 == self.plan.executable_sha256
                && primary.original_server == self.original_server,
            "standalone primary claim differs from adopted plan"
        );
        ensure!(
            !self.original_server.is_empty()
                && self.original_server.len() <= 256
                && !self.original_server.chars().any(char::is_control),
            "invalid original standalone server key"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn admits_configuration(&self, digest: Sha256Digest) -> bool {
        self.configuration_claims
            .iter()
            .any(|claim| claim.config_sha256 == digest)
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/imported-standalone-adoption/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
