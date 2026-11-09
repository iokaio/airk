// SPDX-License-Identifier: Apache-2.0
//! Verified native artifacts produce authority evidence; saved identities alone
//! never authenticate the process that performs the catalog observation.
use super::*;
use airk_store::service::registry::catalog::genesis::activation::{
    AuthorityV1Basis, AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
    AuthorityV1ReadinessProducer,
};

pub(super) fn activate_fresh(root: &Path, intent: &Intent) -> Result<()> {
    activate_genesis(root, intent, false)
}

pub(super) fn activate_rebuild_genesis(root: &Path, intent: &Intent) -> Result<()> {
    rebuild::require_genesis(root, intent)?;
    activate_genesis(root, intent, true)
}

fn activate_genesis(root: &Path, intent: &Intent, successor: bool) -> Result<()> {
    use airk_store::service::{
        ServiceLease,
        registry::catalog::genesis::{self, CatalogGenesis, GenesisStage},
    };
    let producer = if successor {
        NativeProducer::successor(root, intent)?
    } else {
        NativeProducer::fresh(root, intent)?
    };
    ensure!(
        successor || intent.prior.is_none(),
        "fresh genesis cannot replace a predecessor"
    );
    let saved = genesis::pending(&intent.control)?;
    if saved
        .as_ref()
        .is_some_and(|receipt| receipt.stage == GenesisStage::Settled)
    {
        state_contract::require_initialized(root, &intent.control)?;
        return Ok(());
    }
    let operation = saved
        .as_ref()
        .map(|receipt| receipt.operation)
        .map_or_else(airk_kernel::service_broker::new_instance_id, Ok)?;
    ensure!(
        layout::initialization_control(root)? == intent.control,
        "fresh control binding changed"
    );
    let service = ServiceLease::acquire(&intent.control)?;
    ensure!(
        service.root() == intent.control,
        "fresh control directory was redirected"
    );
    if let Some(receipt) = &saved {
        layout::validate_catalog_control(&intent.control, receipt.namespace)?;
    } else {
        // Initialize only this owned layout before the Genesis admission fence.
        // Existing metadata is validated read-only before the normal initializer
        // can run, so a fresh intent cannot migrate an older saved registry.
        if intent.control.join("service-1.4.sqlite3").try_exists()? {
            drop(airk_store::service::registry::ServiceRegistry::open_saved(
                &intent.control,
            )?);
        }
        layout::record_control(&service)?;
        let registry =
            airk_store::service::registry::ServiceRegistry::open(&intent.control, operation)?;
        let namespace = registry.incarnation()?;
        drop(registry);
        layout::record_registry(&service, namespace)?;
    }
    let mut owner = CatalogGenesis::begin(service, producer.installer, operation)?;
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
            &producer,
        )?;
    }
    ensure!(
        receipt.authority_v1.is_some(),
        "native genesis qualification missing"
    );
    state_contract::initialize(root, &owner)?;
    owner.settle_authority_v1()?;
    Ok(())
}

/// The actual managed policy/initial reservation replaces copied Intent/selection;
/// no synthetic preceding selection is created to enter the native producer.
pub(in crate::runtime_install) fn activate_managed(
    authority: &InstallerAuthority<'_>,
    policy: &managed::Managed,
    origin: &airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
    owner: &mut airk_store::service::registry::catalog::genesis::CatalogGenesis,
) -> Result<()> {
    use airk_store::service::registry::catalog::genesis::GenesisStage;
    managed::catalog::verify_origin(authority, policy, origin)?;
    let producer = NativeProducer {
        root: authority.root,
        intent: None,
        managed: Some((authority, policy, origin)),
        installer: origin.installer_identity,
        expected_selection: None,
        successor: false,
    };
    let mut receipt = owner.receipt()?;
    ensure!(
        receipt.coordinator_origin.as_ref() == Some(origin),
        "managed Genesis origin changed"
    );
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
            &producer,
        )?;
    }
    ensure!(
        receipt.authority_v1.is_some(),
        "managed native qualification missing"
    );
    state_contract::initialize(authority.root, owner)?;
    owner.settle_authority_v1()?;
    Ok(())
}

pub(super) struct NativeProducer<'a> {
    root: &'a Path,
    intent: Option<&'a Intent>,
    managed: Option<(
        &'a InstallerAuthority<'a>,
        &'a managed::Managed,
        &'a airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
    )>,
    installer: Sha256Digest,
    expected_selection: Option<Selection>,
    successor: bool,
}

impl<'a> NativeProducer<'a> {
    fn successor(root: &'a Path, intent: &'a Intent) -> Result<Self> {
        rebuild::require_genesis(root, intent)?;
        intent.validate(root)?;
        manager(root, intent)?.require_stopped_for_maintenance()?;
        let (main, _) = verify_runtime(root, &intent.candidate)?;
        Ok(Self {
            root,
            intent: Some(intent),
            managed: None,
            installer: engine_identity(&main)?,
            expected_selection: selection(root)?,
            successor: true,
        })
    }
    fn fresh(root: &'a Path, intent: &'a Intent) -> Result<Self> {
        ensure!(
            intent.prior.is_none(),
            "fresh qualification has a predecessor"
        );
        intent.validate(root)?;
        let expected_selection = selection(root)?;
        ensure!(
            intent.phase == Phase::Probe
                && (expected_selection == Some(intent.selected()) || expected_selection.is_none()),
            "authority qualification requires the exact installer selection"
        );
        manager(root, intent)?.require_stopped_for_maintenance()?;
        let (candidate, _) = verify_runtime(root, &intent.candidate)?;
        let installer = engine_identity(&candidate)?;
        Ok(Self {
            root,
            intent: Some(intent),
            managed: None,
            installer,
            expected_selection,
            successor: false,
        })
    }

    fn control(&self) -> &Path {
        match self.intent {
            Some(intent) => &intent.control,
            None => &self.managed.expect("native producer origin").2.control,
        }
    }

    fn candidate(&self) -> Result<PathBuf> {
        if self.successor {
            rebuild::require_genesis(self.root, self.intent.context("rebuild intent missing")?)?;
        }
        if let Some((authority, policy, origin)) = self.managed {
            managed::catalog::verify_origin(authority, policy, origin)?;
            // The fresh managed Genesis owner holds actual ServiceLease and
            // provider/worker exclusions. This fixed candidate probe starts no
            // service, including for an externally supervised reservation.
            return Ok(policy.saved_executable());
        }
        let intent = self
            .intent
            .context("native copied installer intent missing")?;
        intent.validate(self.root)?;
        ensure!(
            selection(self.root)? == self.expected_selection,
            "candidate selection changed during authority qualification"
        );
        manager(self.root, intent)?.require_stopped_for_maintenance()?;
        let (main, _) = verify_runtime(self.root, &intent.candidate)?;
        ensure!(
            engine_identity(&main)? == self.installer,
            "candidate inventory changed"
        );
        Ok(main)
    }
}

impl AuthorityV1ReadinessProducer for NativeProducer<'_> {
    /// Fresh, rebuild and managed Genesis have no refused predecessor.
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        self.candidate()?;
        Ok(Vec::new())
    }

    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification> {
        ensure!(
            expected.installer_identity == self.installer,
            "saved installer identity differs from the verified candidate"
        );
        let candidate = self.candidate()?;
        let mut probe = Command::new(&candidate);
        probe
            .args(["runtime", "catalog-authority-probe", "--control-root"])
            .arg(self.control())
            .arg("--nonce")
            .arg(expected.nonce.to_string());
        match expected.basis {
            AuthorityV1Basis::FreshGenesis { operation, .. } => {
                probe.arg("--operation").arg(operation.to_string());
            }
            AuthorityV1Basis::LegacyUpgrade { .. } => bail!(CATALOG_UPGRADE_UNSUPPORTED),
        }
        let result = capture(self.control(), probe, 16384, Duration::from_secs(30))?;
        self.candidate()?;
        require_success(&result, "native authority candidate probe failed")?;
        let observed: AuthorityV1CandidateObservation = serde_json::from_slice(&result.stdout)?;
        ensure!(
            &observed == expected,
            "native authority observation changed"
        );
        self.candidate()?;
        Ok(AuthorityV1NativeQualification {
            candidate: observed,
            predecessor_refusals: Vec::new(),
        })
    }
}
