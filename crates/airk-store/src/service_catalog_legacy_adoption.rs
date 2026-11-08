// SPDX-License-Identifier: Apache-2.0
//! Adoption uses the normal canonical reservation and source CAS, while its
//! opaque actual-publication proof forbids ordinary background setup work.
use super::*;
use airk_core::catalog_legacy_adoption::{
    LegacyAdoptionPlan, LegacyAdoptionPublication, LegacyAdoptionReceipt,
};

/// Source identity captured from an actual authenticated private validation
/// archive. It deliberately contains no fake live source directory handles.
pub struct SavedAdoptionSources {
    plan: LegacyAdoptionPlan,
}
impl SavedAdoptionSources {
    pub fn from_verified_archive(
        plan: &LegacyAdoptionPlan,
        archive: &crate::legacy_adoption::VerifiedLegacyArchive,
    ) -> Result<Self> {
        plan.validate()?;
        let scope = archive.scope();
        let original = archive.identity();
        ensure!(
            scope.root == plan.original_registration.root
                && scope.collection == plan.original_registration.collection
                && original.archive_inventory_sha256 == plan.archive_inventory_sha256
                && original.original_root_sha256 == plan.original_publication_sha256,
            "saved adoption archive differs from exact original scope or publication"
        );
        ensure!(
            airk_core::catalog_legacy_adoption::source_inventory_digest(
                &scope.root,
                &scope.collection,
                &scope.roots
            )? == plan.source_inventory_sha256,
            "saved adoption archive source inventory differs from original declaration"
        );
        Ok(Self { plan: plan.clone() })
    }
    pub fn plan(&self) -> &LegacyAdoptionPlan {
        &self.plan
    }
}

/// No Deserialize and no public asserted-digest constructor. The native Current
/// verifier inside Store constructs this only after reading the actual head
/// and its canonical archive-bound adoption resource.
pub struct NativeLegacyAdoptionCompletion {
    plan: LegacyAdoptionPlan,
    publication: LegacyAdoptionPublication,
    pin: PublicationPin,
}
enum PublicationPin {
    Verified(Box<crate::legacy_adoption::publication::VerifiedLegacyPublication>),
    #[cfg(test)]
    MetadataOnly,
}
impl NativeLegacyAdoptionCompletion {
    pub fn from_verified_publication(
        plan: &LegacyAdoptionPlan,
        verified: crate::legacy_adoption::publication::VerifiedLegacyPublication,
    ) -> Result<Self> {
        plan.validate()?;
        ensure!(
            verified.root() == plan.setup.registration.root
                && verified.collection() == plan.setup.registration.collection
                && verified.data_root() == plan.setup.registration.data_root,
            "verified adoption publication belongs to another destination"
        );
        ensure!(
            airk_core::catalog_legacy_adoption::source_inventory_digest(
                verified.root(),
                verified.collection(),
                verified.roots()
            )? == plan.source_inventory_sha256,
            "verified adoption publication changed saved source inventory"
        );
        let publication = LegacyAdoptionPublication {
            schema: 1,
            plan_digest: plan.identity()?,
            archive_inventory_sha256: verified.archive_inventory_sha256(),
            original_publication_sha256: verified.original_publication_sha256(),
            current: verified.current_revision_token(),
            resource_sha256: verified.resource_sha256(),
            compatibility_sha256: verified.compatibility_sha256(),
        };
        publication.validate_plan(plan)?;
        Ok(Self {
            plan: plan.clone(),
            publication,
            pin: PublicationPin::Verified(Box::new(verified)),
        })
    }
    pub fn publication(&self) -> &LegacyAdoptionPublication {
        &self.publication
    }
    pub fn plan(&self) -> &LegacyAdoptionPlan {
        &self.plan
    }
    /// Recheck the held actual publication before another native authority
    /// transaction consumes this capability. No source reads or mutations.
    pub fn revalidate(&self) -> Result<()> {
        match &self.pin {
            PublicationPin::Verified(verified) => verified.revalidate()?,
            #[cfg(test)]
            PublicationPin::MetadataOnly => {}
        }
        self.publication.validate_plan(&self.plan)?;
        Ok(())
    }
    pub(super) fn check(
        &self,
        before: &IndexSetupReceipt,
        completion: &SetupCompletion,
    ) -> Result<()> {
        self.revalidate()?;
        ensure!(
            before.request == self.plan.setup
                && before.legacy_adoption == Some(self.plan.identity()?)
                && before
                    .preparation
                    .as_ref()
                    .is_some_and(|p| p.legacy_adoption == Some(self.publication.plan_digest)),
            "adoption completion differs from native reserved scope or preparation"
        );
        ensure!(
            completion.legacy_adoption.as_ref() == Some(&self.publication),
            "adoption completion differs from actual verified Current publication"
        );
        Ok(())
    }
}
#[cfg(test)]
#[path = "service_catalog_legacy_adoption_tests.rs"]
mod tests;
/// Isolated metadata fixture for sibling admission regressions. Its publication
/// stand-in deliberately does not qualify archive/native runtime integration.
#[cfg(test)]
pub(in crate::service::registry::catalog) fn completed_authority_fixture() -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    NativeLegacyAdoptionCompletion,
    LegacyAdoptionReceipt,
)> {
    tests::completed_authority_fixture()
}
impl ServiceRegistry {
    /// Commits the schema-13 rollback fence with the exact immutable adoption
    /// intent before filesystem preparation. No background work or grant.
    pub fn reserve_legacy_adoption(
        &mut self,
        plan: &LegacyAdoptionPlan,
    ) -> Result<IndexSetupReceipt> {
        plan.validate()?;
        self.reserve_setup_inner(&plan.setup, Some(plan.identity()?))
    }
    pub fn complete_legacy_adoption(
        &mut self,
        native: &NativeLegacyAdoptionCompletion,
    ) -> Result<LegacyAdoptionReceipt> {
        let request = &native.plan.setup;
        let before = self
            .index_setup_receipt(request.caller, &request.key)?
            .context("legacy adoption reservation is absent")?;
        let completion = if let Some(previous) = &before.completion {
            previous.clone()
        } else {
            SetupCompletion {
                prepared_digest: before
                    .preparation
                    .as_ref()
                    .context("legacy adoption lacks native prepared storage")?
                    .digest,
                expected_catalog_revision: self
                    .catalog_state()?
                    .context("legacy adoption catalog is absent")?
                    .revision,
                expected_source_revision: before.source_revision,
                legacy_adoption: Some(native.publication.clone()),
            }
        };
        let setup = self.complete_setup_inner(
            request.caller,
            &request.key,
            request.identity()?,
            &completion,
            Some(native),
        )?;
        let receipt = LegacyAdoptionReceipt {
            schema: 1,
            plan_digest: native.plan.identity()?,
            setup,
        };
        receipt.validate()?;
        Ok(receipt)
    }
}

/// Metadata-only second-index fixture; never supplies a production Current capability.
#[cfg(test)]
pub(in crate::service::registry::catalog) fn complete_metadata_authority_fixture(
    registry: &mut ServiceRegistry,
    plan: LegacyAdoptionPlan,
) -> Result<(NativeLegacyAdoptionCompletion, LegacyAdoptionReceipt)> {
    tests::complete_metadata_authority_fixture(registry, plan)
}
