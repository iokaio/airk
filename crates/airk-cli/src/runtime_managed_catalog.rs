// SPDX-License-Identifier: Apache-2.0
//! Managed first-pin admission uses the existing layout and Genesis journals.
//! Stored origin bytes identify a request; only verified native artifacts and
//! held installer/service ownership authorize its execution.
use super::*;
use airk_core::identity::{Sha256Digest, framed_digest};
use airk_store::service::{
    ServiceLease,
    registry::catalog::genesis::{self, CatalogGenesis, ManagedGenesisOrigin},
};

pub(in crate::runtime_install) struct VerifiedManagedReservation {
    origin: ManagedGenesisOrigin,
}

impl VerifiedManagedReservation {
    pub(in crate::runtime_install) fn origin(&self) -> &ManagedGenesisOrigin {
        &self.origin
    }

    pub(super) fn prepare(
        authority: &InstallerAuthority<'_>,
        policy: &Managed,
        state: &Path,
    ) -> Result<Self> {
        let existing = layout::managed_origin(authority.root)?;
        let operation = match &existing {
            Some(origin) => origin.operation,
            None => airk_kernel::service_broker::new_instance_id()?,
        };
        let (main, _, inventory) = policy.verify_inventory()?;
        let (_, _, pair) = runtime_pair::read_installed_pair(&main, &inventory.version)?;
        let origin = ManagedGenesisOrigin {
            schema: pair.schema,
            install: authority.root.canonicalize()?,
            state: state.to_path_buf(),
            control: state.join("control"),
            operation,
            namespace: operation,
            policy_sha256: policy_digest(policy)?,
            policy_version: policy.approval.version.clone(),
            policy_verification_basis: serde_json::from_value(serde_json::to_value(
                policy.approval.verification_basis,
            )?)?,
            policy_lifecycle: serde_json::from_value(serde_json::to_value(policy.lifecycle)?)?,
            source: policy.source.clone(),
            runtime_sha256: policy.approval.manifest_sha256.parse()?,
            main_sha256: inventory
                .files
                .get(main_name())
                .context("managed main identity missing")?
                .parse()?,
            helper_sha256: if pair.is_single_executable() {
                None
            } else {
                Some(
                    inventory
                        .files
                        .get(helper_name())
                        .context("legacy managed helper identity missing")?
                        .parse()?,
                )
            },
            installer_identity: runtime_pair::engine_identity_at_version(
                &main,
                &policy.approval.version,
            )?,
        };
        origin.validate()?;
        genesis::preflight_managed_origin(&origin)?;
        if let Some(existing) = existing {
            ensure!(
                origin == existing,
                "managed configure differs from the reserved operation"
            );
        }
        Ok(Self { origin })
    }
}

fn policy_digest(policy: &Managed) -> Result<Sha256Digest> {
    // Semantic native identity: domain-framed compact serde encoding of Managed.
    // This is deliberately not the SHA of the policy file's whitespace/bytes.
    Ok(framed_digest(
        b"airk/managed-policy/1",
        &[&serde_json::to_vec(policy)?],
    ))
}

pub(super) fn saved_policy(root: &Path) -> Result<Option<Managed>> {
    if let Some(policy) = read(root)? {
        if let Some(origin) = layout::managed_origin(root)? {
            require_policy_identity(&policy, &origin)?;
        }
        return Ok(Some(policy));
    }
    let Some(origin) = layout::managed_origin(root)? else {
        return Ok(None);
    };
    // This recovers saved STOP identity only after positive native settlement.
    // It neither approves a runnable payload nor publishes missing policy.
    genesis::require_managed_settled(&origin.control, &origin)?;
    let policy = Managed {
        schema: 1,
        install: origin.install.clone(),
        source: origin.source.clone(),
        approval: Approval {
            schema: 1,
            manifest_sha256: origin.runtime_sha256.to_string(),
            version: origin.policy_version.clone(),
            verification_basis: serde_json::from_value(serde_json::Value::String(
                origin.policy_verification_basis.clone(),
            ))?,
        },
        executable_sha256: origin.main_sha256.to_string(),
        lifecycle: serde_json::from_value(serde_json::Value::String(
            origin.policy_lifecycle.clone(),
        ))?,
    };
    ensure!(
        policy_digest(&policy)? == origin.policy_sha256,
        "managed saved lifecycle policy differs"
    );
    Ok(Some(policy))
}

/// Detachment preserves the immutable reservation and its catalog. Only a
/// positively settled original operation can release the user references; no
/// provider file needs to remain available merely to stop its saved identity.
pub(super) fn require_detach_policy(root: &Path, policy: &Managed) -> Result<()> {
    if let Some(origin) = layout::managed_origin(root)? {
        genesis::require_managed_settled(&origin.control, &origin)
            .context("managed_catalog_detach_requires_supported_transition")?;
        require_policy_identity(policy, &origin)?;
        ensure!(
            layout::saved_control(root)? == origin.control,
            "managed detach control binding changed"
        );
    }
    Ok(())
}

fn require_policy_identity(policy: &Managed, origin: &ManagedGenesisOrigin) -> Result<()> {
    origin.validate()?;
    ensure!(
        policy.install == origin.install
            && policy.source == origin.source
            && policy_digest(policy)? == origin.policy_sha256,
        "managed saved lifecycle policy differs from immutable origin"
    );
    Ok(())
}

pub(in crate::runtime_install) enum ManagedCatalogDisposition {
    Disabled,
    Settled,
    /// Existing native legacy startup/read gates remain authoritative. This is
    /// not Genesis, upgrade readiness or all-provider-history completion.
    LegacyReadCompatible,
}

fn saved_genesis(origin: &ManagedGenesisOrigin) -> Result<Option<genesis::GenesisReceipt>> {
    if !exists(&origin.control)? {
        return Ok(None);
    }
    genesis::pending(&origin.control)
}

pub(in crate::runtime_install) fn verify_origin(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    origin: &ManagedGenesisOrigin,
) -> Result<()> {
    origin.validate()?;
    ensure!(
        authority.root.canonicalize()? == origin.install
            && layout::managed_origin(authority.root)?.as_ref() == Some(origin)
            && policy_digest(policy)? == origin.policy_sha256,
        "managed catalog reservation or approved policy changed"
    );
    // The owner performs the full cancellable inventory preflight once before
    // admission/start. Repeated qualification checks rehash only bounded native
    // metadata and the executable pair, never every model/inference asset.
    policy.metadata_approval()?;
    let inventory = approved_inventory_metadata(&policy.source, &policy.approval)?;
    let main = policy.saved_executable();
    let (_, helper, pair) = runtime_pair::read_installed_pair(&main, &inventory.version)?;
    let helper_matches = match (pair.schema, origin.schema, origin.helper_sha256) {
        (2, 2, None) => !inventory.files.contains_key(helper_name()),
        (1, 1, Some(expected)) => {
            inventory.files.get(helper_name()) == Some(&expected.to_string())
                && runtime_pair::digest_file(&helper)? == expected.to_string()
        }
        _ => false,
    };
    ensure!(
        policy.source == origin.source
            && policy.approval.manifest_sha256 == origin.runtime_sha256.to_string()
            && inventory.files.get(main_name()) == Some(&origin.main_sha256.to_string())
            && helper_matches
            && runtime_pair::digest_file(&main)? == origin.main_sha256.to_string()
            && runtime_pair::engine_identity_at_version(&main, &policy.approval.version)?
                == origin.installer_identity,
        "managed catalog approved executable contract changed"
    );
    ensure!(
        layout::saved_control(authority.root)? == origin.control,
        "managed catalog control binding changed"
    );
    Ok(())
}

#[derive(Debug)]
pub(super) struct ManagementUnavailable(pub(super) &'static str);
impl std::fmt::Display for ManagementUnavailable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(self.0)
    }
}
impl std::error::Error for ManagementUnavailable {}

/// No catalog reads/probes/writes for ordinary None origins in disabled builds.
/// An already accepted Some is still fenced when a disabled build encounters it.
pub(super) fn before_start(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    cancel: Option<&setup::SetupCancellation>,
) -> Result<ManagedCatalogDisposition> {
    before_start_enabled(
        authority,
        policy,
        cancel,
        airk_store::service::registry::catalog::upgrade::PRODUCTION_ACTIVATION_ENABLED,
    )
}

fn before_start_enabled(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    cancel: Option<&setup::SetupCancellation>,
    enabled: bool,
) -> Result<ManagedCatalogDisposition> {
    let origin = layout::managed_origin(authority.root)?;
    if !enabled {
        if let Some(origin) = &origin {
            verify_origin(authority, policy, origin)?;
            layout::require_managed_ready(authority.root)?;
        }
        return Ok(ManagedCatalogDisposition::Disabled);
    }
    let Some(origin) = origin else {
        let control = layout::control(authority.root)?;
        // Preserve supported legacy initialization/read startup, but do not
        // reinterpret crossed, pending or damaged catalog metadata as legacy.
        lifecycle::inspect(authority.root)?;
        let mut upgrade_receipt = None;
        if exists(&control)? {
            ensure!(
                genesis::pending(&control)?.is_none(),
                ManagementUnavailable("managed_catalog_origin_missing_for_saved_genesis")
            );
            upgrade_receipt = airk_store::service::registry::catalog::upgrade::pending(&control)?;
            if let Some(receipt) = &upgrade_receipt {
                ensure!(
                    receipt.stage
                        == airk_store::service::registry::catalog::upgrade::UpgradeStage::Restored
                        || receipt.stage == airk_store::service::registry::catalog::upgrade::UpgradeStage::Settled,
                    ManagementUnavailable("managed_catalog_requires_exact_saved_upgrade_owner")
                );
            }
        }
        if control.join("service-1.4.sqlite3").try_exists()? {
            let saved = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
            if let Some(state) = saved.catalog_state()? {
                let receipt = upgrade_receipt.as_ref().ok_or(ManagementUnavailable(
                    "managed_legacy_catalog_lacks_settled_lineage",
                ))?;
                ensure!(state.activated && state.authority_contract == 0 && state.policy_contract == 0
                    && receipt.stage == airk_store::service::registry::catalog::upgrade::UpgradeStage::Settled
                    && receipt.authority_v1.is_none() && receipt.authority_v1_host.is_none()
                    && state.namespace == receipt.plan.namespace && state.upgrade_id == receipt.plan.id
                    && state.legacy_schema_digest == receipt.plan.legacy_schema_digest
                    && receipt.backups.iter().any(|attempt| attempt.complete && attempt.sha256 == Some(state.backup_digest)),
                ManagementUnavailable("managed_catalog_origin_does_not_match_existing_catalog")
                );
            } else if let Some(receipt) = upgrade_receipt {
                ensure!(
                    receipt.stage
                        == airk_store::service::registry::catalog::upgrade::UpgradeStage::Restored,
                    ManagementUnavailable("managed_legacy_state_differs_from_saved_catalog")
                );
            }
        }
        return Ok(ManagedCatalogDisposition::LegacyReadCompatible);
    };
    verify_origin(authority, policy, &origin)?;
    if let Some(receipt) = saved_genesis(&origin)?
        && receipt.stage == genesis::GenesisStage::Settled
    {
        genesis::require_managed_settled(&origin.control, &origin)?;
        state_contract::require_initialized(authority.root, &origin.control)?;
        return Ok(ManagedCatalogDisposition::Settled);
    }
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    // A reserved first pin never starts normal service before its Genesis is
    // settled. Acquisition is actual exclusion, not a claim about old helpers;
    // old layouts without this reservation cannot enter this fresh lane. The
    // same exclusion permits an explicitly reserved external first pin to settle
    // metadata; its later health path still cannot elect a service. Existing
    // external installations without an origin never enter this fresh lane.
    let mut owner = admit(authority, policy, &origin)?;
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    activation::activate_managed_genesis(authority, policy, &origin, &mut owner)?;
    // owner owns the real ServiceLease until it is dropped here, before normal
    // startup/callback. Accepted cancellation preserves its exact saved receipt.
    drop(owner);
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    genesis::require_managed_settled(&origin.control, &origin)?;
    Ok(ManagedCatalogDisposition::Settled)
}

#[cfg(test)]
#[path = "runtime_managed_catalog_tests.rs"]
mod tests;

pub(super) fn admit(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    origin: &ManagedGenesisOrigin,
) -> Result<CatalogGenesis> {
    verify_origin(authority, policy, origin)?;
    layout::prepare_managed_control(authority, origin)?;
    let lease = ServiceLease::acquire(&origin.control)
        .context("acquire reserved managed control ownership")?;
    if genesis::pending(&origin.control)?.is_some() {
        layout::validate_catalog_control(&origin.control, origin.namespace)?;
        return CatalogGenesis::begin_managed(lease, origin);
    }
    // Native state/registry ownership is initialized before Genesis, using the
    // namespace durably reserved by the INITIAL layout, including crash retry.
    layout::record_control(&lease)?;
    let registry =
        airk_store::service::registry::ServiceRegistry::open(&origin.control, origin.namespace)?;
    ensure!(
        registry.incarnation()? == origin.namespace,
        "managed namespace changed"
    );
    drop(registry);
    layout::record_registry(&lease, origin.namespace)?;
    CatalogGenesis::begin_managed(lease, origin)
}

/// Explicit configure owns admission before publishing policy. A crash before
/// this call is recovered only from the immutable initial layout reservation.
pub(super) fn admit_initial(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    reservation: &VerifiedManagedReservation,
) -> Result<()> {
    if policy.lifecycle == ManagedLifecycle::External {
        return Ok(());
    }
    let owner = admit(authority, policy, reservation.origin())?;
    drop(owner);
    Ok(())
}

pub(in crate::runtime_install) fn resume_before_lifecycle(
    authority: &InstallerAuthority<'_>,
    cancel: Option<&setup::SetupCancellation>,
) -> Result<()> {
    let Some(origin) = layout::managed_origin(authority.root)? else {
        return Ok(());
    };
    if let Some(receipt) = saved_genesis(&origin)?
        && receipt.stage == genesis::GenesisStage::Settled
    {
        // Normal lifecycle STOP settlement still precedes runnable inventory
        // verification. This is a read-only exact native receipt check.
        return genesis::require_managed_settled(&origin.control, &origin);
    }
    let policy = read(authority.root)?.ok_or(ManagementUnavailable(
        "repeat_same_approved_managed_configure_to_resume",
    ))?;
    if airk_store::service::registry::catalog::upgrade::PRODUCTION_ACTIVATION_ENABLED {
        let check = || cancel.map_or(Ok(()), setup::SetupCancellation::checkpoint);
        policy.verify_inventory_checked(&check)?;
    }
    // Fresh reservations prohibit ordinary startup before settlement, hence no
    // normal lifecycle request can be admitted in this unfinished first pin.
    before_start(authority, &policy, cancel).map(|_| ())
}
