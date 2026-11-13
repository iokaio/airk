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
use anyhow::{Result, ensure};
use std::path::Path;

const REGISTRY: &str = "service-1.4.sqlite3";

/// The reason an admitted start refuses a control root it cannot give a catalog.
pub(super) const REFUSED: &str = "catalog_genesis_refused";
/// The reason when Genesis could not run now (a held lease, a disk error).
pub(super) const FAILED: &str = "catalog_genesis_failed";

/// Run fresh Genesis when the lease's control root has no catalog, holding
/// `lease` throughout; return the same lease. An existing activated catalog
/// returns the lease untouched; an interrupted Genesis of this build resumes.
///
/// The caller runs this before anything else opens the registry: `begin`
/// fences ordinary registry admission until settlement, so an interrupted
/// Genesis is only completed here.
pub(super) fn ensure_catalog(lease: ServiceLease) -> Result<ServiceLease> {
    let root = lease.root().to_path_buf();
    let root = root.as_path();
    let saved = genesis::pending(root)?;
    if saved
        .as_ref()
        .is_some_and(|receipt| receipt.stage == GenesisStage::Settled)
    {
        // A settled receipt is only trusted with its live, activated catalog.
        let live = root.join(REGISTRY).try_exists()?
            && ServiceRegistry::open_saved(root)?
                .catalog_state()?
                .is_some_and(|catalog| catalog.activated);
        ensure!(
            live,
            "{FAILED}: the index catalog recorded as created is missing or not activated"
        );
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
    run_genesis(lease, saved.map(|receipt| receipt.operation)).map_err(classify)
}

/// The leading code of a Genesis failure: only the store's "this registry
/// was already used" refusals (and a receipt left by another build) need a
/// reinstall; anything else is a failed attempt the next start resumes.
fn classify(error: anyhow::Error) -> anyhow::Error {
    let text = format!("{error:#}");
    if text.starts_with(REFUSED) || text.starts_with(FAILED) {
        return error;
    }
    if text.contains("requires the real catalog upgrade")
        || text.contains("refuses an existing catalog or earlier registry format")
        || text.contains("genesis belongs to another operation or installer")
    {
        error.context(format!(
            "{REFUSED}: this AIRK Service state has no index catalog and cannot be given one;              it was used or left half-prepared by another build. Uninstall AIRK Service, choose              to delete its data, and install it again"
        ))
    } else {
        error.context(format!(
            "{FAILED}: the index catalog could not be created yet; the next start continues"
        ))
    }
}

fn run_genesis(lease: ServiceLease, resumed: Option<Incarnation>) -> Result<ServiceLease> {
    let root = lease.root().to_path_buf();
    let root = root.as_path();
    let installer = airk_host::runtime_pair::engine_identity()?;
    let operation = match resumed {
        Some(operation) => operation,
        None => airk_kernel::service_broker::new_instance_id()?,
    };
    let mut owner = CatalogGenesis::begin(lease, installer, operation)?;
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
