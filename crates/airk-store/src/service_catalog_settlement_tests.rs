// SPDX-License-Identifier: Apache-2.0
use super::super::tests::{fixture, plan};
use super::*;
use crate::service::registry::ServiceRegistry;

#[test]
fn native_refusal_provenance_binds_all_diagnostics_and_direction_valid_entrypoints() -> Result<()> {
    let digest = |value| Sha256Digest::from_bytes([value; 32]);
    for read_hidden in [false, true] {
        for write_hidden in [false, true] {
            let provenance = NativeRefusalProvenance {
                native_entrypoints: NativeRefusalEntrypoints {
                    read: if read_hidden {
                        NativeReadRefusalEntrypoint::HiddenUpgradeProbe
                    } else {
                        NativeReadRefusalEntrypoint::ServiceRequestsList
                    },
                    write: if write_hidden {
                        NativeWriteRefusalEntrypoint::HiddenUpgradeMigrate
                    } else {
                        NativeWriteRefusalEntrypoint::ServiceRequestsForget
                    },
                },
                read: NativeRefusalDiagnostic {
                    initial_admission_pending_digest: read_hidden.then(|| digest(2)),
                    final_classified_diagnostic_digest: digest(3),
                },
                write: NativeRefusalDiagnostic {
                    initial_admission_pending_digest: write_hidden.then(|| digest(4)),
                    final_classified_diagnostic_digest: digest(5),
                },
            };
            let proof = RuntimeRefusal::from_native(digest(1), provenance.clone())?;
            proof.validate()?;
            assert_ne!(proof.read_refusal_digest, proof.write_refusal_digest);
            assert_eq!(
                serde_json::from_slice::<RuntimeRefusal>(&serde_json::to_vec(&proof)?)?,
                proof
            );
            let mut changed = proof.clone();
            changed.payload_identity = digest(6);
            assert!(changed.validate().is_err());
            if read_hidden {
                let mut changed = proof.clone();
                changed
                    .native_provenance
                    .as_mut()
                    .unwrap()
                    .read
                    .initial_admission_pending_digest = Some(digest(10));
                assert!(changed.validate().is_err());
            }
            let mut changed = proof.clone();
            changed
                .native_provenance
                .as_mut()
                .unwrap()
                .read
                .final_classified_diagnostic_digest = digest(7);
            assert!(changed.validate().is_err());
            let mut changed = proof.clone();
            changed
                .native_provenance
                .as_mut()
                .unwrap()
                .write
                .final_classified_diagnostic_digest = digest(8);
            assert!(changed.validate().is_err());
            let mut changed = provenance.clone();
            changed.read.initial_admission_pending_digest =
                if read_hidden { None } else { Some(digest(9)) };
            assert!(RuntimeRefusal::from_native(digest(1), changed).is_err());
            let mut changed = provenance;
            changed.write.initial_admission_pending_digest =
                if write_hidden { None } else { Some(digest(9)) };
            assert!(RuntimeRefusal::from_native(digest(1), changed).is_err());
        }
    }
    Ok(())
}

#[test]
fn legacy_refusal_without_provenance_retains_exact_original_bytes_and_digests() -> Result<()> {
    let bytes = format!(
        "{{\"payload_identity\":\"{}\",\"read_refusal_digest\":\"{}\",\"write_refusal_digest\":\"{}\"}}",
        Sha256Digest::from_bytes([1; 32]),
        Sha256Digest::from_bytes([2; 32]),
        Sha256Digest::from_bytes([3; 32])
    );
    let proof: RuntimeRefusal = serde_json::from_str(&bytes)?;
    assert!(proof.native_provenance.is_none());
    proof.validate()?;
    assert_eq!(serde_json::to_vec(&proof)?, bytes.as_bytes());
    Ok(())
}

fn readiness(owner: &CatalogUpgrade, plan: &UpgradePlan) -> Result<ReadinessEvidence> {
    // Unit fixture only. Actual predecessor/candidate process qualification is a
    // separate CLI gate; these digests must not be reported as runtime evidence.
    Ok(ReadinessEvidence {
        schema: 1,
        candidate: observe(owner.control(), plan.id, Incarnation::from_bytes([24; 16]))?,
        predecessor_refusals: vec![RuntimeRefusal {
            payload_identity: Sha256Digest::from_bytes([25; 32]),
            read_refusal_digest: Sha256Digest::from_bytes([26; 32]),
            write_refusal_digest: Sha256Digest::from_bytes([27; 32]),
            native_provenance: None,
        }],
    })
}

#[test]
fn production_activation_is_disabled_and_missing_or_stale_readiness_is_refused() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan.clone())?;
    owner.migrate()?;
    assert!(owner.settle_at(|_| Ok(())).is_err());
    let mut evidence = readiness(&owner, &plan)?;
    evidence.predecessor_refusals.clear();
    assert!(owner.record_readiness(evidence).is_err());
    let mut evidence = readiness(&owner, &plan)?;
    evidence.candidate.catalog.revision = 2.try_into()?;
    assert!(owner.record_readiness(evidence).is_err());
    owner.record_readiness(readiness(&owner, &plan)?)?;
    assert!(owner.settle().is_err());
    assert!(!registry_read(root.path())?.query_row(
        "SELECT activated FROM index_catalog_meta",
        [],
        |row| row.get::<_, bool>(0)
    )?);
    assert!(ServiceRegistry::open_saved(root.path()).is_err());
    Ok(())
}

#[test]
fn settlement_recovers_both_activation_fence_boundaries_and_preserves_first_new_write() -> Result<()>
{
    for fault in ["after_catalog_activation", "after_admission_clear"] {
        let (root, registration) = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let plan = plan(&owner, registration)?;
        owner.admit(plan.clone())?;
        owner.migrate()?;
        owner.record_readiness(readiness(&owner, &plan)?)?;
        assert!(
            owner
                .settle_at(|stage| {
                    ensure!(stage != fault, "injected settlement interruption");
                    Ok(())
                })
                .is_err()
        );
        if fault == "after_admission_clear" {
            let registry = ServiceRegistry::open(root.path(), owner.namespace())?;
            registry
                .connection
                .execute("UPDATE projects SET paused=0", [])?;
        }
        drop(owner);
        let before = fs::read(root.path().join(DATABASE))?;
        assert!(
            CatalogUpgrade::resume_for_installer(root.path(), Sha256Digest::from_bytes([197; 32]))
                .is_err()
        );
        assert!(fs::read(root.path().join(DATABASE))? == before);
        let owner = CatalogUpgrade::resume_for_installer(root.path(), plan.installer_identity)?;
        assert!(owner.catalog_state()?.unwrap().activated);
        assert_eq!(owner.settle_at(|_| Ok(()))?.stage, UpgradeStage::Settled);
        let registry = ServiceRegistry::open_saved(root.path())?;
        assert!(registry.catalog_state()?.unwrap().activated);
        if fault == "after_admission_clear" {
            let before = fs::read(root.path().join(DATABASE))?;
            let error = owner.restore().unwrap_err();
            assert!(error.to_string().contains("forward repair"), "{error:#}");
            assert_eq!(fs::read(root.path().join(DATABASE))?, before);
            assert!(
                !registry
                    .connection
                    .query_row("SELECT paused FROM projects", [], |row| row
                        .get::<_, bool>(0))?
            );
        }
    }
    Ok(())
}

#[test]
fn qualified_snapshot_restore_resumes_boundaries_without_touching_collection_state() -> Result<()> {
    for fault in [
        "restore_intent",
        "after_snapshot_restore",
        "after_restore_admission_clear",
    ] {
        let (root, registration) = fixture()?;
        let sentinel = root.path().join("fictional-collection-sentinel");
        fs::write(&sentinel, b"collection state must survive")?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let plan = plan(&owner, registration)?;
        owner.admit(plan)?;
        let receipt = owner.migrate()?;
        assert!(
            owner
                .restore_at(|stage| {
                    ensure!(stage != fault, "injected restore interruption");
                    Ok(())
                })
                .is_err()
        );
        if fault == "after_restore_admission_clear" {
            let registry = ServiceRegistry::open(root.path(), owner.namespace())?;
            registry
                .connection
                .execute("UPDATE projects SET paused=0", [])?;
        }
        drop(owner);
        let owner =
            CatalogUpgrade::resume_for_installer(root.path(), receipt.plan.installer_identity)?;
        assert_eq!(owner.restore()?.stage, UpgradeStage::Restored);
        let registry = ServiceRegistry::open_saved(root.path())?;
        assert!(registry.catalog_state()?.is_none());
        if fault == "after_restore_admission_clear" {
            assert!(
                !registry
                    .connection
                    .query_row("SELECT paused FROM projects", [], |row| row
                        .get::<_, bool>(0))?
            );
        } else {
            assert_eq!(
                super::super::super::backup::legacy_data_digest(&registry.connection)?,
                receipt.legacy_data_digest.unwrap()
            );
        }
        assert_eq!(fs::read(&sentinel)?, b"collection state must survive");
        assert_eq!(owner.restore()?.stage, UpgradeStage::Restored);
    }
    Ok(())
}

#[test]
fn a_settled_catalog_can_restore_before_the_first_new_or_legacy_write_only() -> Result<()> {
    for write in [
        None,
        Some("UPDATE projects SET epoch=epoch+1"),
        Some("UPDATE query_projects SET name='User changed this Project'"),
    ] {
        let (root, registration) = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let plan = plan(&owner, registration)?;
        owner.admit(plan.clone())?;
        owner.migrate()?;
        owner.record_readiness(readiness(&owner, &plan)?)?;
        owner.settle_at(|_| Ok(()))?;
        if let Some(write) = write {
            let registry = ServiceRegistry::open(root.path(), owner.namespace())?;
            registry.connection.execute_batch(write)?;
            assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
            let before = fs::read(root.path().join(DATABASE))?;
            assert!(
                owner
                    .restore()
                    .unwrap_err()
                    .to_string()
                    .contains("forward repair")
            );
            assert_eq!(fs::read(root.path().join(DATABASE))?, before);
        } else {
            owner.restore()?;
            assert!(
                ServiceRegistry::open_saved(root.path())?
                    .catalog_state()?
                    .is_none()
            );
        }
    }
    Ok(())
}
