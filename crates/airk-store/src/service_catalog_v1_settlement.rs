// SPDX-License-Identifier: Apache-2.0
//! Authority-1 finalization consumes existing sealed lineage, never P5 backfill.
use super::super::super::genesis::activation::{
    AuthorityV1ActivationRecord, AuthorityV1Basis, AuthorityV1CandidateObservation,
    AuthorityV1Phase, AuthorityV1ReadinessProducer, activated_identity, collect,
};
use super::*;

pub fn observe_authority_v1(
    root: &Path,
    upgrade_id: Incarnation,
    native_host: Sha256Digest,
    nonce: Incarnation,
) -> Result<AuthorityV1CandidateObservation> {
    let root = root.canonicalize()?;
    let receipt = pending(&root)?.context("authority-1 upgrade intent missing")?;
    let owner = super::super::super::super::owner_record(&root, true, receipt.plan.namespace)?;
    ensure!(
        receipt.stage == UpgradeStage::Readiness
            && receipt.plan.id == upgrade_id
            && receipt.authority_v1_host == Some(native_host)
            && owner.catalog_upgrade == Some(upgrade_id),
        "authority-1 probe requires exact pending native ownership"
    );
    let seal = super::super::recovery::verify_authority(&root, &receipt)?;
    let read = registry_read(&root)?;
    let catalog = super::super::super::state(&read)?;
    ensure!(
        !catalog.activated,
        "authority-1 qualification must precede activation"
    );
    Ok(AuthorityV1CandidateObservation {
        schema: 1,
        nonce,
        installer_identity: receipt.plan.installer_identity,
        catalog,
        layout_digest: super::super::super::schema_digest(&read)?,
        basis: AuthorityV1Basis::LegacyUpgrade {
            upgrade_id,
            plan_digest: receipt.plan_digest,
            seal_digest: seal.seal_digest,
            native_host,
        },
    })
}

impl CatalogUpgrade {
    pub fn record_authority_v1_readiness(
        &self,
        native_host: Sha256Digest,
        nonce: Incarnation,
        producer: &impl AuthorityV1ReadinessProducer,
    ) -> Result<UpgradeReceipt> {
        let mut receipt = self.receipt()?;
        ensure!(
            receipt.authority_v1_host == Some(native_host),
            "compatibility native host changed"
        );
        let expected = observe_authority_v1(&self.root, receipt.plan.id, native_host, nonce)?;
        if let Some(record) = &receipt.authority_v1 {
            ensure!(
                record.readiness.candidate == expected,
                "saved authority-1 qualification changed"
            );
            return Ok(receipt);
        }
        let predecessors = receipt
            .readiness
            .as_ref()
            .context("P5 readiness missing")?
            .predecessor_refusals
            .iter()
            .map(|r| r.payload_identity)
            .collect::<Vec<_>>();
        let readiness = collect(producer, expected, Some(&predecessors))?;
        ensure!(
            observe_authority_v1(&self.root, receipt.plan.id, native_host, nonce)?
                == readiness.candidate,
            "sealed catalog changed during native qualification"
        );
        receipt.authority_v1 = Some(AuthorityV1ActivationRecord {
            readiness,
            phase: AuthorityV1Phase::Qualified,
        });
        self.save(&receipt)?;
        Ok(receipt)
    }
    pub fn settle_authority_v1(&self, native_host: Sha256Digest) -> Result<UpgradeReceipt> {
        ensure!(
            PRODUCTION_ACTIVATION_ENABLED,
            "catalog activation remains disabled"
        );
        self.settle_authority_v1_at(native_host, |_| Ok(()))
    }
    fn settle_authority_v1_at(
        &self,
        native_host: Sha256Digest,
        checkpoint: impl Fn(&str) -> Result<()>,
    ) -> Result<UpgradeReceipt> {
        let mut receipt = pending(&self.root)?.context("authority-1 upgrade intent missing")?;
        if receipt.retirement_recovery.is_some() {
            return super::super::recovery::settle_owned(self, native_host, checkpoint);
        }

        ensure!(
            receipt.plan.namespace == self.namespace
                && receipt.authority_v1_host == Some(native_host)
                && matches!(
                    receipt.stage,
                    UpgradeStage::Readiness | UpgradeStage::Settled
                ),
            "authority-1 settlement identity changed"
        );
        let mut record = receipt
            .authority_v1
            .clone()
            .context("authority-1 native qualification missing")?;
        record.validate()?;
        let mut owner = super::super::super::super::owner_record(&self.root, true, self.namespace)?;
        let read = registry_read(&self.root)?;
        super::super::super::validate_existing(&read)?;
        let state = super::super::super::state(&read)?;
        if owner.catalog_upgrade.is_none() {
            ensure!(
                matches!(
                    record.phase,
                    AuthorityV1Phase::Activating | AuthorityV1Phase::Settled
                ),
                "authority-1 admission cleared before activation intent"
            );
            activated_identity(&state, &record, true)?;
            record.phase = AuthorityV1Phase::Settled;
            receipt.authority_v1 = Some(record);
            receipt.stage = UpgradeStage::Settled;
            self.save(&receipt)?;
            return Ok(receipt);
        }
        ensure!(
            owner.catalog_upgrade == Some(receipt.plan.id),
            "authority-1 settlement owner fence changed"
        );
        ensure!(
            record.phase != AuthorityV1Phase::Settled,
            "authority-1 settled intent still has an admission fence"
        );
        self.qualified_backup(&receipt)?;
        if !state.activated {
            ensure!(
                observe_authority_v1(
                    &self.root,
                    receipt.plan.id,
                    native_host,
                    record.readiness.candidate.nonce
                )? == record.readiness.candidate,
                "qualified sealed observation changed before activation"
            );
            record.phase = AuthorityV1Phase::Activating;
            receipt.authority_v1 = Some(record.clone());
            self.save(&receipt)?;
            drop(read);
            let mut connection = Connection::open_with_flags(
                self.root.join(DATABASE),
                OpenFlags::SQLITE_OPEN_READ_WRITE,
            )?;
            connection.busy_timeout(Duration::from_millis(100))?;
            connection.execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
            )?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            ensure!(
                super::super::super::super::owner_record(&self.root, true, self.namespace)?
                    .catalog_upgrade
                    == Some(receipt.plan.id),
                "authority-1 owner fence changed during activation"
            );
            ensure!(
                super::super::super::state(&tx)? == record.readiness.candidate.catalog,
                "sealed catalog changed during activation"
            );
            ensure!(tx.execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1 AND activated=0 AND authority_contract=1 AND accepted_new_writes=0",[])?==1,
                "authority-1 activation marker changed");
            activated_identity(&super::super::super::state(&tx)?, &record, false)?;
            tx.commit()?;
        } else {
            ensure!(
                record.phase == AuthorityV1Phase::Activating,
                "authority-1 catalog activated outside durable intent"
            );
            activated_identity(&state, &record, false)?;
            verify_recovered_authority_v1(&self.root, &receipt)?;
        }
        checkpoint("after_catalog_activation")?;
        owner.catalog_upgrade = None;
        owner.complete(&self.root)?;
        checkpoint("after_admission_clear")?;
        record.phase = AuthorityV1Phase::Settled;
        receipt.authority_v1 = Some(record);
        receipt.stage = UpgradeStage::Settled;
        self.save(&receipt)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::registry::ServiceRegistry;
    struct OwnedLegacyProducer {
        root: PathBuf,
        upgrade: Incarnation,
        host: Sha256Digest,
    }
    // Store lifecycle fixtures only. Real native artifact execution/refusal
    // qualification belongs to the CLI producer and is not claimed here.
    fn unit_refusal() -> RuntimeRefusal {
        RuntimeRefusal {
            payload_identity: Sha256Digest::from_bytes([25; 32]),
            read_refusal_digest: Sha256Digest::from_bytes([26; 32]),
            write_refusal_digest: Sha256Digest::from_bytes([27; 32]),
            native_provenance: None,
        }
    }
    impl AuthorityV1ReadinessProducer for OwnedLegacyProducer {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(vec![unit_refusal().payload_identity])
        }
        fn qualify(
            &self,
            expected: &AuthorityV1CandidateObservation,
        ) -> Result<super::super::super::super::genesis::activation::AuthorityV1NativeQualification>
        {
            Ok(
                super::super::super::super::genesis::activation::AuthorityV1NativeQualification {
                    candidate: observe_authority_v1(
                        &self.root,
                        self.upgrade,
                        self.host,
                        expected.nonce,
                    )?,
                    predecessor_refusals: vec![unit_refusal()],
                },
            )
        }
    }
    fn prepared_legacy_before_migration(
        base: &Path,
    ) -> Result<(CatalogUpgrade, UpgradePlan, Sha256Digest)> {
        ensure!(
            super::super::super::super::capabilities().supports(12, 1, 0),
            "owned legacy v1 qualification requires native contract-1 support"
        );
        let base = base.canonicalize()?;
        let control = base.join("control");
        let source = base.join("source");
        let data = base.join("data");
        for path in [&control, &source, &data] {
            fs::create_dir(path)?;
        }
        let source = source.canonicalize()?;
        let data = data.canonicalize()?;
        let registration = ProjectRegistration::new(
            source.to_str().context("source UTF-8")?.into(),
            data.to_str().context("data UTF-8")?.into(),
            "fixture".into(),
        )?;
        let registry = ServiceRegistry::open(&control, Incarnation::from_bytes([1; 16]))?;
        registry.connection.execute(
            "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,7,1,1)",
            params![
                registration.binding.to_string(),
                serde_json::to_vec(&registration)?
            ],
        )?;
        drop(registry);
        let collection = crate::service::collection::Collection::initialize(
            &data,
            &registration.root,
            "fixture",
            Incarnation::from_bytes([9; 16]),
        )?;
        // The read-only upgrade adapter intentionally refuses to create missing
        // native lifecycle/gate metadata. Complete explicit fixture setup using
        // the same selected-store writer and saved RootConfig as native setup.
        let selected = collection.select()?;
        let writer = selected.store.lock()?;
        let config =
            crate::roots::RootConfig::load(selected.store.directory(), &registration.root)?;
        config.save(selected.store.directory())?;
        drop(writer);
        drop(selected);
        let existing = collection.select_existing_locked()?;
        assert!(
            crate::roots::RootConfig::load(existing.store.directory(), &registration.root)?
                .roots
                .is_empty()
        );
        drop(existing);
        drop(collection);
        let owner = CatalogUpgrade::acquire(&control)?;
        let mut plan = super::super::super::tests::plan(&owner, registration.clone())?;
        let config = owner.source_configuration(&registration)?;
        ensure!(
            config.source_import == SourceImportState::Complete,
            "actual fixture source configuration unavailable"
        );
        plan.indexes[0].source_import = config.source_import;
        plan.indexes[0].configuration_digest = config.digest;
        owner.admit(plan.clone())?;
        Ok((owner, plan, Sha256Digest::from_bytes([99; 32])))
    }
    fn prepared_legacy_imports(base: &Path) -> Result<(CatalogUpgrade, UpgradePlan, Sha256Digest)> {
        let (owner, plan, host) = prepared_legacy_before_migration(base)?;
        owner.migrate()?;
        Ok((owner, plan, host))
    }
    fn prepared_legacy(base: &Path) -> Result<(CatalogUpgrade, UpgradePlan, Sha256Digest)> {
        let (owner, plan, host) = prepared_legacy_imports(base)?;
        owner.record_readiness(ReadinessEvidence {
            schema: 1,
            candidate: super::super::super::observe(
                owner.control(),
                plan.id,
                Incarnation::from_bytes([24; 16]),
            )?,
            predecessor_refusals: vec![unit_refusal()],
        })?;
        Ok((owner, plan, host))
    }
    #[test]
    fn native_host_binding_is_owned_stage_checked_idempotent_and_preserves_catalog() -> Result<()> {
        let base = tempfile::tempdir()?;
        let (owner, _plan, host) = prepared_legacy_before_migration(base.path())?;
        let before = fs::read(owner.control().join(DATABASE))?;
        let intent = fs::read(owner.control().join(INTENT))?;
        assert!(owner.bind_authority_v1_host(host).is_err());
        assert_eq!(fs::read(owner.control().join(DATABASE))?, before);
        assert_eq!(fs::read(owner.control().join(INTENT))?, intent);
        owner.migrate()?;
        let before = fs::read(owner.control().join(DATABASE))?;
        let bound = owner.bind_authority_v1_host(host)?;
        assert_eq!(bound.stage, UpgradeStage::Imports);
        assert_eq!(bound.authority_v1_host, Some(host));
        assert!(bound.readiness.is_none() && bound.authority_v1.is_none());
        let intent = fs::read(owner.control().join(INTENT))?;
        owner.bind_authority_v1_host(host)?;
        assert_eq!(fs::read(owner.control().join(INTENT))?, intent);
        assert!(
            owner
                .bind_authority_v1_host(Sha256Digest::from_bytes([98; 32]))
                .is_err()
        );
        assert_eq!(fs::read(owner.control().join(INTENT))?, intent);
        assert_eq!(fs::read(owner.control().join(DATABASE))?, before);
        Ok(())
    }
    #[test]
    fn bound_imports_resume_preserves_p5_readiness_before_any_sealing() -> Result<()> {
        let base = tempfile::tempdir()?;
        let (owner, plan, host) = prepared_legacy_imports(base.path())?;
        owner.bind_authority_v1_host(host)?;
        let control = owner.control().to_path_buf();
        let before = fs::read(control.join(DATABASE))?;
        drop(owner);
        let owner = CatalogUpgrade::resume_for_installer(&control, plan.installer_identity)?;
        assert_eq!(owner.receipt()?.stage, UpgradeStage::Imports);
        owner.bind_authority_v1_host(host)?;
        let candidate =
            super::super::super::observe(&control, plan.id, Incarnation::from_bytes([24; 16]))?;
        assert_eq!(candidate.catalog.authority_contract, 0);
        owner.record_readiness(ReadinessEvidence {
            schema: 1,
            candidate,
            predecessor_refusals: vec![unit_refusal()],
        })?;
        assert_eq!(
            owner.bind_authority_v1_host(host)?.stage,
            UpgradeStage::Readiness
        );
        assert_eq!(fs::read(control.join(DATABASE))?, before);
        owner.promote_authority_v1(host)?;
        assert!(
            owner.bind_authority_v1_host(host).is_err(),
            "binder cannot authorize sealed1"
        );
        Ok(())
    }
    #[test]
    fn owned_postseal_retry_and_resume_keep_exact_rows_and_reject_foreign_host() -> Result<()> {
        let base = tempfile::tempdir()?;
        let (owner, plan, host) = prepared_legacy(base.path())?;
        let sealed = owner.promote_authority_v1(host)?;
        let control = owner.control().to_path_buf();
        let before = fs::read(control.join(DATABASE))?;
        assert_eq!(owner.promote_authority_v1(host)?, sealed);
        assert_eq!(fs::read(control.join(DATABASE))?, before);
        assert!(
            owner
                .promote_authority_v1(Sha256Digest::from_bytes([98; 32]))
                .is_err()
        );
        drop(owner);
        let resumed = CatalogUpgrade::resume_for_installer(&control, plan.installer_identity)?;
        assert_eq!(resumed.promote_authority_v1(host)?, sealed);
        assert!(
            super::super::super::observe(&control, plan.id, Incarnation::from_bytes([24; 16]))
                .is_err(),
            "P5 zero-grant reader must refuse sealed authority 1"
        );
        Ok(())
    }
    #[test]
    fn owned_v1_settlement_preserves_postclear_revocation_and_new_revision() -> Result<()> {
        for fault in ["after_catalog_activation", "after_admission_clear"] {
            let base = tempfile::tempdir()?;
            let (owner, plan, host) = prepared_legacy(base.path())?;
            owner.promote_authority_v1(host)?;
            let control = owner.control().to_path_buf();
            owner.record_authority_v1_readiness(
                host,
                Incarnation::from_bytes([30; 16]),
                &OwnedLegacyProducer {
                    root: control.clone(),
                    upgrade: plan.id,
                    host,
                },
            )?;
            assert!(
                owner
                    .settle_authority_v1_at(host, |stage| {
                        ensure!(stage != fault, "injected v1 settlement interruption");
                        Ok(())
                    })
                    .is_err()
            );
            if fault == "after_admission_clear" {
                let registry = ServiceRegistry::open(&control, owner.namespace())?;
                registry
                    .connection
                    .execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
                registry
                    .connection
                    .execute("UPDATE index_catalog_meta SET revision=revision+1", [])?;
                assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
                assert!(registry.connection.query_row(
                    "SELECT revoked FROM index_access_grants",
                    [],
                    |r| r.get::<_, bool>(0)
                )?);
            }
            drop(owner);
            let resumed = CatalogUpgrade::resume_for_installer(&control, plan.installer_identity)?;
            assert_eq!(
                resumed.settle_authority_v1_at(host, |_| Ok(()))?.stage,
                UpgradeStage::Settled
            );
            let registry = ServiceRegistry::open_saved(&control)?;
            assert!(registry.catalog_state()?.unwrap().activated);
            if fault == "after_admission_clear" {
                assert!(registry.connection.query_row(
                    "SELECT revoked FROM index_access_grants",
                    [],
                    |r| r.get::<_, bool>(0)
                )?);
            }
        }
        Ok(())
    }
    #[test]
    fn owned_committed_seal_gap_refuses_missing_rows_and_marker_spoof_without_resealing()
    -> Result<()> {
        for mutation in [
            "DELETE FROM index_access_grants",
            "UPDATE index_catalog_meta SET authority_contract=0",
        ] {
            let base = tempfile::tempdir()?;
            let (owner, _plan, host) = prepared_legacy(base.path())?;
            assert!(
                owner
                    .promote_authority_v1_at(host, |_, stage| {
                        ensure!(
                            stage != "after_authority_commit",
                            "injected seal receipt gap"
                        );
                        Ok(())
                    })
                    .is_err()
            );
            let connection = Connection::open(owner.control().join(DATABASE))?;
            connection.execute_batch("PRAGMA foreign_keys=ON;")?;
            connection.execute_batch(mutation)?;
            drop(connection);
            let before = fs::read(owner.control().join(DATABASE))?;
            assert!(owner.promote_authority_v1(host).is_err());
            assert_eq!(fs::read(owner.control().join(DATABASE))?, before);
        }
        Ok(())
    }
    #[test]
    fn legacy_v1_crash_fixture_child() -> Result<()> {
        let Some(base) = std::env::var_os("AIRK_LEGACY_V1_CRASH_FIXTURE") else {
            return Ok(());
        };
        let (owner, _plan, host) = prepared_legacy(Path::new(&base))?;
        owner.promote_authority_v1_at(host, |connection, stage| {
            if stage == "before_authority_commit" {
                connection.cache_flush()?;
                std::process::exit(75);
            }
            Ok(())
        })?;
        anyhow::bail!("legacy v1 crash checkpoint was not reached")
    }
    #[test]
    fn owned_legacy_hot_journal_recovers_zero_grant_source_before_exact_seal_retry() -> Result<()> {
        let base = tempfile::tempdir()?;
        let child = concat!(module_path!(), "::legacy_v1_crash_fixture_child");
        let child = child.split_once("::").context("test module missing")?.1;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", child, "--nocapture"])
            .env("AIRK_LEGACY_V1_CRASH_FIXTURE", base.path())
            .output()?;
        ensure!(
            output.status.code() == Some(75),
            "legacy v1 child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let control = base.path().join("control");
        assert!(control.join(format!("{DATABASE}-journal")).try_exists()?);
        let receipt = pending(&control)?.context("legacy crash intent missing")?;
        let host = receipt
            .authority_v1_host
            .context("preseal native host intent missing")?;
        let owner =
            CatalogUpgrade::resume_for_installer(&control, receipt.plan.installer_identity)?;
        assert_eq!(owner.promote_authority_v1(host)?.imported_grants, 1);
        Ok(())
    }
}
