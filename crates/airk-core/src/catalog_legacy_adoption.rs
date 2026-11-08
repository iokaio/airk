// SPDX-License-Identifier: Apache-2.0
//! Saved-evidence adoption metadata. These wire values do not prove ownership,
//! quiescence, archive integrity or a Current publication; native adapters do.
use crate::{
    catalog_management::MAX_RECEIPT_BYTES,
    catalog_setup::{IndexSetupReceipt, IndexSetupRequest, SetupStage},
    identity::{RevisionToken, Sha256Digest, framed_digest},
    index_catalog::encoded_size,
    service::ProjectRegistration,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn source_inventory_digest(
    root: &str,
    collection: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/catalog-legacy-adoption-sources/1",
        &[
            root.as_bytes(),
            collection.as_bytes(),
            &serde_json::to_vec(aliases)?,
        ],
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAdoptionPlan {
    pub schema: u32,
    pub setup: IndexSetupRequest,
    pub original_registration: ProjectRegistration,
    pub origin_sha256: Sha256Digest,
    pub config_sha256: Sha256Digest,
    pub executable_sha256: Sha256Digest,
    pub quiescence_sha256: Sha256Digest,
    pub archive_inventory_sha256: Sha256Digest,
    pub original_publication_sha256: Sha256Digest,
    pub source_inventory_sha256: Sha256Digest,
}
impl LegacyAdoptionPlan {
    pub fn validate(&self) -> Result<()> {
        self.setup.validate()?;
        self.original_registration.validate()?;
        ensure!(
            self.schema == 1 && !self.setup.setup_watch,
            "legacy adoption cannot start a watcher or ordinary setup work"
        );
        ensure!(
            self.setup.registration.root == self.original_registration.root
                && self.setup.registration.collection == self.original_registration.collection,
            "legacy adoption must preserve the exact original source and collection"
        );
        ensure!(
            self.setup.registration.data_root != self.original_registration.data_root,
            "legacy adoption destination must preserve the original index tree separately"
        );
        let aliases = self
            .setup
            .sources
            .iter()
            .flat_map(|source| {
                source
                    .aliases
                    .iter()
                    .map(|alias| (alias.clone(), source.canonical_path.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        ensure!(
            self.source_inventory_sha256
                == source_inventory_digest(
                    &self.original_registration.root,
                    &self.original_registration.collection,
                    &aliases
                )?,
            "legacy adoption source inventory differs from exact canonical sources"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-legacy-adoption-plan/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAdoptionDraft {
    pub schema: u32,
    pub plan: LegacyAdoptionPlan,
    pub review_digest: Sha256Digest,
}
impl LegacyAdoptionDraft {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported legacy adoption draft");
        self.plan.validate()?;
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}
/// The actual converted head and its canonical adoption resource. The old
/// publication remains separately identified; it is never relabeled Current.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAdoptionPublication {
    pub schema: u32,
    pub plan_digest: Sha256Digest,
    pub archive_inventory_sha256: Sha256Digest,
    pub original_publication_sha256: Sha256Digest,
    pub current: RevisionToken,
    pub resource_sha256: Sha256Digest,
    pub compatibility_sha256: Sha256Digest,
}
impl LegacyAdoptionPublication {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.current.generation > 0,
            "invalid legacy adoption publication"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn validate_plan(&self, plan: &LegacyAdoptionPlan) -> Result<()> {
        self.validate()?;
        ensure!(
            self.plan_digest == plan.identity()?
                && self.archive_inventory_sha256 == plan.archive_inventory_sha256
                && self.original_publication_sha256 == plan.original_publication_sha256,
            "legacy adoption publication differs from reviewed origin/archive"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyAdoptionReceipt {
    pub schema: u32,
    pub plan_digest: Sha256Digest,
    pub setup: IndexSetupReceipt,
}
impl LegacyAdoptionReceipt {
    pub fn validate(&self) -> Result<()> {
        self.setup.validate()?;
        ensure!(
            self.schema == 1
                && self.setup.stage == SetupStage::Ready
                && self
                    .setup
                    .preparation
                    .as_ref()
                    .and_then(|p| p.legacy_adoption)
                    == Some(self.plan_digest)
                && self
                    .setup
                    .completion
                    .as_ref()
                    .and_then(|c| c.legacy_adoption.as_ref())
                    .is_some_and(|p| p.plan_digest == self.plan_digest),
            "legacy adoption receipt lacks its native publication"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog_setup::SetupSource, identity::Incarnation, index_catalog::CatalogRevision,
        service::RequestKey,
    };
    fn plan() -> Result<LegacyAdoptionPlan> {
        let digest = Sha256Digest::from_bytes([1; 32]);
        let original_registration =
            ProjectRegistration::new("/source".into(), "/old-index".into(), "saved".into())?;
        Ok(LegacyAdoptionPlan {
            schema: 1,
            setup: IndexSetupRequest {
                caller: Incarnation::from_bytes([2; 16]),
                key: RequestKey {
                    namespace: Incarnation::from_bytes([3; 16]),
                    epoch: 0,
                    nonce: Incarnation::from_bytes([4; 16]),
                },
                expected_catalog_revision: CatalogRevision::INITIAL,
                name: "Adopted saved index".into(),
                registration: ProjectRegistration::new(
                    "/source".into(),
                    "/private-new-index".into(),
                    "saved".into(),
                )?,
                sources: vec![SetupSource {
                    canonical_path: "/source".into(),
                    aliases: vec![],
                    owner: true,
                }],
                setup_watch: false,
            },
            original_registration,
            origin_sha256: digest,
            config_sha256: digest,
            executable_sha256: digest,
            quiescence_sha256: digest,
            archive_inventory_sha256: digest,
            original_publication_sha256: digest,
            source_inventory_sha256: source_inventory_digest("/source", "saved", &BTreeMap::new())?,
        })
    }
    #[test]
    fn plan_binds_original_scope_and_all_native_evidence() -> Result<()> {
        let plan = plan()?;
        plan.validate()?;
        let mut changed = plan.clone();
        changed.config_sha256 = Sha256Digest::from_bytes([5; 32]);
        assert_ne!(changed.identity()?, plan.identity()?);
        let mut changed = plan.clone();
        changed.setup.setup_watch = true;
        assert!(changed.validate().is_err());
        let mut changed = plan.clone();
        changed.setup.registration =
            ProjectRegistration::new("/other".into(), "/private-new-index".into(), "saved".into())?;
        assert!(changed.validate().is_err());
        let mut changed = plan.clone();
        changed.source_inventory_sha256 = Sha256Digest::from_bytes([6; 32]);
        assert!(changed.validate().is_err());
        Ok(())
    }
    #[test]
    fn publication_round_trip_preserves_distinct_old_and_current_identity() -> Result<()> {
        let plan = plan()?;
        let publication = LegacyAdoptionPublication {
            schema: 1,
            plan_digest: plan.identity()?,
            archive_inventory_sha256: plan.archive_inventory_sha256,
            original_publication_sha256: plan.original_publication_sha256,
            current: RevisionToken {
                incarnation: Incarnation::from_bytes([7; 16]),
                generation: 2,
                revision: Sha256Digest::from_bytes([8; 32]),
            },
            resource_sha256: Sha256Digest::from_bytes([9; 32]),
            compatibility_sha256: Sha256Digest::from_bytes([10; 32]),
        };
        publication.validate_plan(&plan)?;
        let round: LegacyAdoptionPublication =
            serde_json::from_slice(&serde_json::to_vec(&publication)?)?;
        assert_eq!(round, publication);
        let mut changed = publication;
        changed.archive_inventory_sha256 = Sha256Digest::from_bytes([11; 32]);
        assert!(changed.validate_plan(&plan).is_err());
        Ok(())
    }
}
