// SPDX-License-Identifier: Apache-2.0
//! Fresh catalog Genesis on an admitted service's first start (split contracts
//! §4 (I3), "Genesis on first start"; docs/split/inventory/dependencies.md B1).
//!
//! Before the split, the staged installer ran fresh Genesis before the first
//! service probe. The split setup role places files and starts the service, so
//! the installed service owns it: an admitted start on a control root without a
//! catalog runs the store's own `CatalogGenesis` (backup, migrate, authority-1
//! promotion, qualification, settlement) before any supervisor, log or registry
//! actor starts. Without it the catalog never exists, every catalog request
//! fails and no HTTPS listener (and so no AIRK Admin) starts.
//!
//! The store keeps its rules unchanged: Genesis admits only an empty schema-11
//! registry with no history. A control root that already ran a service without
//! a catalog (split builds 1.8.24–1.8.31) is refused with
//! `catalog_genesis_refused`, and the start fails with that reason instead of
//! serving without a catalog.
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_store::service::{
    ServiceLease,
    registry::{
        ServiceRegistry,
        catalog::genesis::{
            self, CatalogGenesis, GenesisStage,
            activation::{
                AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
                AuthorityV1ReadinessProducer, observe_authority_v1,
            },
        },
    },
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

const REGISTRY: &str = "service-1.4.sqlite3";

/// The reason an admitted start refuses a control root it cannot give a catalog.
pub(super) const REFUSED: &str = "catalog_genesis_refused";

/// Run fresh Genesis when the lease's control root has no catalog, holding
/// `lease` throughout; return the same lease. A settled Genesis or an existing catalog returns the
/// lease untouched; an interrupted Genesis of this installer resumes.
pub(super) fn ensure_catalog(lease: ServiceLease) -> Result<ServiceLease> {
    let root = lease.root().to_path_buf();
    let root = root.as_path();
    let saved = genesis::pending(root)?;
    if saved
        .as_ref()
        .is_some_and(|receipt| receipt.stage == GenesisStage::Settled)
    {
        return Ok(lease);
    }
    if saved.is_none()
        && root.join(REGISTRY).try_exists()?
        && ServiceRegistry::open_saved(root)?
            .catalog_state()?
            .is_some()
    {
        return Ok(lease);
    }
    let installer = airk_host::runtime_pair::engine_identity()?;
    let operation = match &saved {
        Some(receipt) => receipt.operation,
        None => airk_kernel::service_broker::new_instance_id()?,
    };
    let mut owner = CatalogGenesis::begin(lease, installer, operation).with_context(|| {
        format!(
            "{REFUSED}: this AIRK Service state has no index catalog and cannot be given one; \
             it was used by an earlier build without an index catalog. Uninstall AIRK Service, \
             choose to delete its data, and install it again"
        )
    })?;
    let mut receipt = owner.receipt()?;
    if receipt.authority_v1.is_none() {
        if matches!(
            receipt.stage,
            GenesisStage::Admitted | GenesisStage::BackedUp | GenesisStage::Migrating
        ) {
            owner.backup()?;
            owner.migrate()?;
        }
        owner.promote_authority_v1()?;
        receipt = owner.record_authority_v1_readiness(
            airk_kernel::service_broker::new_instance_id()?,
            &RunningService {
                root,
                operation,
                installer,
            },
        )?;
    }
    ensure!(
        receipt.authority_v1.is_some(),
        "native genesis qualification missing"
    );
    owner.settle_authority_v1()?;
    Ok(owner.into_service_lease())
}

/// The candidate is the running service itself: it observes the owned
/// registry in process (the pre-split `catalog-authority-probe`, without a
/// child process) and proves it is the executable Genesis admitted.
struct RunningService<'a> {
    root: &'a Path,
    operation: Incarnation,
    installer: Sha256Digest,
}

impl AuthorityV1ReadinessProducer for RunningService<'_> {
    /// Fresh Genesis has no refused predecessor.
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        Ok(Vec::new())
    }

    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification> {
        ensure!(
            expected.installer_identity == self.installer
                && airk_host::runtime_pair::engine_identity()? == self.installer,
            "catalog authority candidate differs from the admitted installer"
        );
        let observation = observe_authority_v1(self.root, self.operation, expected.nonce)?;
        airk_host::install_layout::validate_catalog_control(
            self.root,
            observation.catalog.namespace,
        )?;
        Ok(AuthorityV1NativeQualification {
            candidate: observation,
            predecessor_refusals: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests;
