// SPDX-License-Identifier: Apache-2.0
//! Verified native artifacts produce authority evidence; saved identities alone
//! never authenticate the process that performs the catalog observation.
use super::*;
use airk_store::service::registry::catalog::genesis::activation::{
    AuthorityV1Basis, AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
    AuthorityV1ReadinessProducer,
};
use airk_store::service::registry::catalog::upgrade::{
    NativeReadRefusalEntrypoint, NativeRefusalDiagnostic, NativeRefusalEntrypoints,
    NativeRefusalProvenance, NativeWriteRefusalEntrypoint,
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
        predecessor: None,
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

pub(super) fn settle_upgrade(
    root: &Path,
    intent: &Intent,
    owner: CatalogUpgrade,
    receipt: UpgradeReceipt,
) -> Result<()> {
    let producer = NativeProducer::new(root, intent)?;
    let host = crate::catalog_host_context::current_native_host(receipt.plan.namespace)?;
    ensure!(
        receipt.authority_v1_host.is_none_or(|saved| saved == host),
        "catalog compatibility host differs from the original native owner"
    );
    if receipt.authority_v1.is_none() {
        owner.promote_authority_v1(host)?;
        owner.record_authority_v1_readiness(
            host,
            airk_kernel::service_broker::new_instance_id()?,
            &producer,
        )?;
    }
    owner.settle_authority_v1(host)?;
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
    predecessor: Option<Sha256Digest>,
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
            predecessor: None,
            expected_selection: selection(root)?,
            successor: true,
        })
    }
    pub(super) fn new(root: &'a Path, intent: &'a Intent) -> Result<Self> {
        Self::bound(root, intent, false)
    }

    fn fresh(root: &'a Path, intent: &'a Intent) -> Result<Self> {
        ensure!(
            intent.prior.is_none(),
            "fresh qualification has a predecessor"
        );
        Self::bound(root, intent, true)
    }

    fn bound(root: &'a Path, intent: &'a Intent, fresh: bool) -> Result<Self> {
        intent.validate(root)?;
        let expected_selection = selection(root)?;
        ensure!(
            intent.phase == Phase::Probe
                && (expected_selection == Some(intent.selected())
                    || (fresh && intent.prior.is_none() && expected_selection.is_none())),
            "authority qualification requires the exact installer selection"
        );
        manager(root, intent)?.require_stopped_for_maintenance()?;
        let (candidate, _) = verify_runtime(root, &intent.candidate)?;
        let installer = engine_identity(&candidate)?;
        let predecessor = intent
            .prior
            .as_ref()
            .map(|prior| -> Result<_> {
                let (main, _) = verify_runtime(root, &prior.active)?;
                engine_identity(&main)
            })
            .transpose()?;
        ensure!(
            predecessor != Some(installer),
            "candidate cannot certify itself as a refused predecessor"
        );
        Ok(Self {
            root,
            intent: Some(intent),
            managed: None,
            installer,
            predecessor,
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

    fn predecessor(&self) -> Result<Option<PathBuf>> {
        if self.managed.is_some() || self.successor {
            return Ok(None);
        }
        let intent = self
            .intent
            .context("native copied installer intent missing")?;
        let Some(prior) = &intent.prior else {
            ensure!(
                self.predecessor.is_none(),
                "fresh installation proof changed"
            );
            return Ok(None);
        };
        let (main, _) = verify_runtime(self.root, &prior.active)?;
        ensure!(
            engine_identity(&main)? == self.predecessor.context("predecessor proof missing")?,
            "predecessor inventory changed"
        );
        Ok(Some(main))
    }
}

impl NativeProducer<'_> {
    fn capture_refusals(
        &self,
        upgrade_id: Option<airk_core::identity::Incarnation>,
        nonce: airk_core::identity::Incarnation,
    ) -> Result<Vec<RuntimeRefusal>> {
        let mut predecessor_refusals = Vec::new();
        if let Some(predecessor) = self.predecessor()? {
            let mut diagnostics = Vec::new();
            let mut hidden = Vec::new();
            for write in [false, true] {
                let mut command = Command::new(&predecessor);
                command
                    .args(["service", "requests", "--runtime-dir"])
                    .arg(self.control());
                if write {
                    command
                        .arg("forget")
                        .arg(airk_kernel::service_broker::new_instance_id()?.to_string());
                } else {
                    command.arg("list");
                }
                let output = capture(self.control(), command, 16384, Duration::from_secs(30))?;
                self.predecessor()?;
                let initial = output.stderr.clone();
                let mut output = output;
                let mut entrypoint = "service requests";
                if !output.success && admission_refusal(&String::from_utf8_lossy(&output.stderr)) {
                    let upgrade_id = upgrade_id
                        .context("fresh genesis cannot supply a legacy predecessor probe")?;
                    // P5-aware predecessors honor the live owner fence before
                    // reading SQLite. Their existing native upgrade commands
                    // instead parse the saved upgrade format first. The real
                    // fence remains installed throughout both attempts.
                    let mut command = Command::new(&predecessor);
                    entrypoint = if write {
                        "runtime catalog-upgrade-migrate"
                    } else {
                        "runtime catalog-upgrade-probe"
                    };
                    command
                        .args([
                            "runtime",
                            if write {
                                "catalog-upgrade-migrate"
                            } else {
                                "catalog-upgrade-probe"
                            },
                            "--control-root",
                        ])
                        .arg(self.control())
                        .arg("--upgrade-id")
                        .arg(upgrade_id.to_string());
                    if !write {
                        command.arg("--nonce").arg(nonce.to_string());
                    }
                    self.predecessor()?;
                    output = capture(self.control(), command, 16384, Duration::from_secs(30))?;
                    self.predecessor()?;
                }
                let error = String::from_utf8_lossy(&output.stderr);
                ensure!(
                    !output.success && authority_refusal(&error),
                    "predecessor did not refuse the authority contract: {error}"
                );
                let fallback = entrypoint != "service requests";
                hidden.push(fallback);
                diagnostics.push(NativeRefusalDiagnostic {
                    initial_admission_pending_digest: fallback
                        .then(|| framed_digest(b"airk/catalog-refusal-diagnostic/1", &[&initial])),
                    final_classified_diagnostic_digest: framed_digest(
                        b"airk/catalog-refusal-diagnostic/1",
                        &[&output.stderr],
                    ),
                });
            }
            predecessor_refusals.push(RuntimeRefusal::from_native(
                self.predecessor.context("predecessor proof missing")?,
                NativeRefusalProvenance {
                    native_entrypoints: NativeRefusalEntrypoints {
                        read: if hidden[0] {
                            NativeReadRefusalEntrypoint::HiddenUpgradeProbe
                        } else {
                            NativeReadRefusalEntrypoint::ServiceRequestsList
                        },
                        write: if hidden[1] {
                            NativeWriteRefusalEntrypoint::HiddenUpgradeMigrate
                        } else {
                            NativeWriteRefusalEntrypoint::ServiceRequestsForget
                        },
                    },
                    read: diagnostics.remove(0),
                    write: diagnostics.remove(0),
                },
            )?);
        }
        self.candidate()?;
        Ok(predecessor_refusals)
    }
}

pub(super) fn preseal_refusals(
    root: &Path,
    intent: &Intent,
    upgrade_id: airk_core::identity::Incarnation,
    nonce: airk_core::identity::Incarnation,
) -> Result<Vec<RuntimeRefusal>> {
    let producer = NativeProducer::new(root, intent)?;
    let refusals = producer.capture_refusals(Some(upgrade_id), nonce)?;
    ensure!(
        refusals.len() == 1,
        "preseal readiness requires its exact native predecessor"
    );
    Ok(refusals)
}

impl AuthorityV1ReadinessProducer for NativeProducer<'_> {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        self.candidate()?;
        self.predecessor()?;
        Ok(self.predecessor.into_iter().collect())
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
                ensure!(
                    self.predecessor.is_none(),
                    "fresh genesis has a preceding installation"
                );
                probe.arg("--operation").arg(operation.to_string());
            }
            AuthorityV1Basis::LegacyUpgrade {
                upgrade_id,
                native_host,
                ..
            } => {
                ensure!(
                    self.predecessor.is_some(),
                    "legacy qualification has no predecessor"
                );
                probe
                    .arg("--operation")
                    .arg(upgrade_id.to_string())
                    .arg("--native-host")
                    .arg(native_host.to_string());
            }
        }
        let result = capture(self.control(), probe, 16384, Duration::from_secs(30))?;
        self.candidate()?;
        require_success(&result, "native authority candidate probe failed")?;
        let observed: AuthorityV1CandidateObservation = serde_json::from_slice(&result.stdout)?;
        ensure!(
            &observed == expected,
            "native authority observation changed"
        );
        let upgrade_id = match expected.basis {
            AuthorityV1Basis::LegacyUpgrade { upgrade_id, .. } => Some(upgrade_id),
            AuthorityV1Basis::FreshGenesis { .. } => None,
        };
        let predecessor_refusals = self.capture_refusals(upgrade_id, expected.nonce)?;
        self.candidate()?;
        Ok(AuthorityV1NativeQualification {
            candidate: observed,
            predecessor_refusals,
        })
    }
}

fn authority_refusal(error: &str) -> bool {
    registry_refusal(error)
        || (!error.contains("Usage:")
            && !error.contains("unexpected argument")
            && !error.contains("unrecognized subcommand")
            && (error.contains("unsupported index catalog authority contract")
                || error.contains("unsupported catalog authority contract")
                || error.contains("unknown field `authority_v1_host`")
                || error.contains("unknown field `authority_v1`")
                // P5's strict saved RuntimeRefusal decoder can reject this
                // added receipt field before reaching the authority-1 fields.
                // This is a saved-format refusal from the actual old artifact,
                // not an argument-parser or connection error.
                || error.contains("unknown field `native_provenance`")
                || error
                    .contains("unsupported registry table, index, foreign-key or trigger shape")))
}

fn admission_refusal(error: &str) -> bool {
    !error.contains("Usage:")
        && !error.contains("unexpected argument")
        && !error.contains("unrecognized subcommand")
        && error.contains("index catalog upgrade is pending; resume owned installation")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authority_refusal_requires_format_failure_not_parser_or_connection_failure() {
        for value in [
            "unsupported index catalog authority contract",
            "Error: unknown field `native_provenance`, expected one of `payload_identity`, `read_refusal_digest`, `write_refusal_digest` at line 1 column 2764",
            "unsupported catalog authority contract",
            "unsupported saved service registry format",
            "unsupported registry table, index, foreign-key or trigger shape",
            "unknown field `authority_v1_host`, expected `schema`",
        ] {
            assert!(authority_refusal(value));
        }
        for value in [
            "connection refused",
            "Usage: unsupported catalog authority contract",
            "unrecognized subcommand 'unsupported catalog authority contract'",
            "unexpected argument unsupported index catalog authority contract",
            "Usage: unsupported registry table, index, foreign-key or trigger shape",
            "index catalog upgrade is pending; resume owned installation",
            "Usage: unknown field `authority_v1_host`",
            "Usage: unknown field `native_provenance`",
            "unexpected argument: unknown field `native_provenance`",
            "unrecognized subcommand 'unknown field `native_provenance`'",
            "unknown field `unrelated_user_input`",
        ] {
            assert!(!authority_refusal(value));
        }
        assert!(admission_refusal(
            "index catalog upgrade is pending; resume owned installation"
        ));
        assert!(!admission_refusal(
            "Usage: index catalog upgrade is pending; resume owned installation"
        ));
    }
}
