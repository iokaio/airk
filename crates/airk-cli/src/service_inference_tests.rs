// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
use anyhow::Result;
use std::sync::Arc;

#[test]
fn predecessor_exclusion_is_exact_and_cannot_be_used_as_helper_reap() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let current = Incarnation::from_bytes([1; 16]);
    let prior = Incarnation::from_bytes([2; 16]);
    let generation = Incarnation::from_bytes([3; 16]);
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, current)?;
    let namespace = registry.incarnation()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    assert!(
        PredecessorExclusionWitness::acquire(lease.clone(), current, current, generation).is_err()
    );
    let witness = PredecessorExclusionWitness::acquire(lease, current, prior, generation)?;
    let gate = crate::catalog_admission::NativeCatalogAdmission::new(
        current,
        namespace,
        CatalogRevision::INITIAL,
        0,
    )?;
    gate.observe_provider_excluded(witness.clone())?;
    assert!(gate.provider_was_completed(prior, generation, true)?);
    assert!(!gate.provider_was_completed(prior, generation, false)?);
    assert!(!gate.provider_was_completed(current, generation, true)?);
    let foreign = crate::catalog_admission::NativeCatalogAdmission::new(
        current,
        Incarnation::from_bytes([4; 16]),
        CatalogRevision::INITIAL,
        0,
    )?;
    assert!(foreign.observe_provider_excluded(witness.clone()).is_err());
    let changed = crate::catalog_admission::NativeCatalogAdmission::new(
        Incarnation::from_bytes([5; 16]),
        namespace,
        CatalogRevision::INITIAL,
        0,
    )?;
    assert!(changed.observe_provider_excluded(witness).is_err());
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn zero_provider_factory_is_sealed_and_failed_start_cannot_be_zero_proof() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let service = Incarnation::from_bytes([1; 16]);
    let operation = Incarnation::from_bytes([2; 16]);
    let mut factory = InferenceFactory::new(lease.clone(), service)?;
    assert!(
        InferenceFactory::new(lease.clone(), service).is_err(),
        "a second factory bypassed actual native lease ownership"
    );
    let observed = factory.empty_checkpoint(operation, Vec::new())?.unwrap();
    observed.validate()?;
    assert_eq!(
        observed.completion_basis,
        airk_core::inference_provider::CompletionBasis::NativeFactoryNeverAllocated
    );
    assert!(
        factory.start(1).is_err(),
        "closed factory created new inference owner"
    );
    assert_eq!(
        factory.empty_checkpoint(operation, Vec::new())?,
        Some(observed)
    );
    assert!(
        factory
            .empty_checkpoint(Incarnation::from_bytes([3; 16]), Vec::new())
            .is_err()
    );
    assert!(
        airk_kernel::inference_owner::run_service_inference(lease, || ()).is_err(),
        "zero-owner proof left another safe compute scope open"
    );
    let other_owner = tempfile::tempdir()?;
    let other = other_owner.path().join("control");
    crate::local_ipc::prepare_root(&other)?;
    let other_lease = Arc::new(airk_store::service::ServiceLease::acquire(&other)?);
    let mut failed = InferenceFactory::new(other_lease, service)?;
    assert!(failed.start(0).is_err());
    assert!(
        failed.empty_checkpoint(operation, Vec::new()).is_err(),
        "failed owner start was relabeled never allocated"
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn stop_continues_exact_checkpoint_operation_and_full_worker_capture() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let service = Incarnation::from_bytes([11; 16]);
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let mut factory = InferenceFactory::new(lease, service)?;
    let mut pool = factory.start(1)?;
    let operation = Incarnation::from_bytes([12; 16]);
    // C7 captures all sealed workers, not only workers with model grants.
    let captured = vec![airk_core::service::WorkerIdentity {
        service,
        worker: Incarnation::from_bytes([13; 16]),
        project: airk_core::identity::Sha256Digest::from_bytes([14; 32]),
        fencing_generation: 1,
    }];
    let _ = pool.checkpoint_drain(operation, captured.clone())?;
    assert!(
        pool.checkpoint_drain(Incarnation::from_bytes([15; 16]), captured.clone())
            .is_err()
    );
    assert!(pool.checkpoint_drain(operation, Vec::new()).is_err());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match pool.terminate() {
            Ok(()) => break,
            Err(error) if std::time::Instant::now() < deadline => {
                assert!(error.to_string().contains("still draining"), "{error:#}");
                std::thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
    assert!(pool.retiring());
    assert!(pool.reap_witness().is_none());
    let witness = pool
        .completion_witness()
        .expect("actual engine completion witness");
    assert_eq!(witness.observation().operation_id, operation);
    assert_eq!(witness.observation().captured_workers, captured);
    assert_eq!(witness.namespace(), registry.incarnation()?);
    pool.terminate()?;
    assert_eq!(
        pool.checkpoint_drain(operation, captured)?,
        Some(witness.observation().clone())
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn more_than_128_native_restarts_preserve_worker_authorizations_and_bounded_checkpoint()
-> Result<()> {
    use airk_core::{
        catalog_fences::WorkerProviderHistory, inference_provider::CompletionBasis,
        service::WorkerIdentity,
    };
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let service = Incarnation::from_bytes([31; 16]);
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let namespace = registry.incarnation()?;
    let gate = crate::catalog_admission::NativeCatalogAdmission::new(
        service,
        namespace,
        CatalogRevision::INITIAL,
        0,
    )?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&data)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "restart-owner")?;
    let worker = WorkerIdentity {
        service,
        worker: Incarnation::from_bytes([30; 16]),
        project: project.binding,
        fencing_generation: 1,
    };
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let mut factory = InferenceFactory::new(lease.clone(), service)?;
    let mut recovery = crate::project_supervisor::TestPoolRecovery::new_owned(
        std::time::Instant::now(),
        lease.clone(),
        service,
    );
    let upgrade = Incarnation::from_bytes([32; 16]);
    let mut originals = Vec::new();
    let mut generations = Vec::new();
    let mut completed: Option<crate::inference_completion_ledger::NativeLedgerWitness> = None;
    for number in 33u8..163u8 {
        let mut pool = factory.start(1)?;
        let access = pool.access_worker(worker, project.clone())?;
        assert!(access.in_process);
        generations.push(pool.identity());
        assert!(
            factory.empty_checkpoint(upgrade, vec![worker]).is_err(),
            "live owner became complete"
        );
        let original_operation = Incarnation::from_bytes([number; 16]);
        let _ = pool.checkpoint_drain(original_operation, vec![worker])?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match pool.terminate() {
                Ok(()) => break,
                Err(error) if std::time::Instant::now() < deadline => {
                    assert!(error.to_string().contains("still draining"), "{error:#}");
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        }
        originals.push(pool.completion_witness().unwrap().observation().clone());
        drop(pool);
        assert!(
            factory.empty_checkpoint(upgrade, vec![worker])?.is_none(),
            "undelivered native completion became checkpoint proof"
        );
        for event in factory.take_completed()? {
            recovery.record_completed(event);
        }
        if number == 33 {
            assert!(
                recovery
                    .transfer_completed(None, |_| anyhow::bail!("injected ingestion failure"))
                    .is_err()
            );
            assert!(
                factory.empty_checkpoint(upgrade, vec![worker])?.is_none(),
                "failed ingestion acknowledged native completion"
            );
        }
        let attached = number == 162;
        if attached {
            recovery.transfer_fallback(&gate)?;
            assert!(
                gate.ledger_was_completed(completed.as_ref().unwrap().observation())?,
                "late Gate attachment lost actual native anchor-only fallback"
            );
        }
        recovery.transfer_completed(attached.then_some(&gate), |event| {
            crate::project_supervisor::complete_provider_authorizations(
                service,
                true,
                &mut generations,
                &mut completed,
                event,
            )
        })?;
        assert!(generations.is_empty());
        let anchor = completed.as_ref().unwrap().observation();
        recovery.prune_completed(&generations, &[anchor.head_sha256]);
        if attached {
            gate.prune_provider_reaps(&generations, &[anchor.head_sha256])?;
            assert!(gate.ledger_was_completed(anchor)?);
        } else {
            assert!(
                recovery.ledger_was_completed(anchor) && !gate.ledger_was_completed(anchor)?,
                "raw-service completion requires retained native fallback, not an unattached Gate"
            );
        }
    }
    let obligation = crate::catalog_admission::native_provider_obligation_completed(
        worker,
        generations,
        true,
        Some(completed.as_ref().unwrap().observation().clone()),
    );
    assert!(
        matches!(obligation.history,WorkerProviderHistory::InProcess{generations,completed_authorizations:Some(_)} if generations.is_empty())
    );
    // Verification advances in bounded actor turns across the full immutable
    // archive; the returned checkpoint carries a constant-size native anchor.
    assert!(factory.empty_checkpoint(upgrade, vec![worker])?.is_none());
    let checkpoint = factory.empty_checkpoint(upgrade, vec![worker])?.unwrap();
    checkpoint.validate()?;
    assert_eq!(
        checkpoint.completion_basis,
        CompletionBasis::NativeFactoryCompletionLedger
    );
    assert_eq!(
        checkpoint
            .completion_ledger
            .as_ref()
            .unwrap()
            .completed_owners,
        130
    );
    assert!(serde_json::to_vec(&checkpoint)?.len() < 2048);
    let directory = std::fs::read_dir(root.join("provider-completions"))?
        .next()
        .unwrap()?
        .path();
    for (index, original) in originals.iter().enumerate() {
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(
            directory.join(format!("{:020}.json", index + 1)),
        )?)?;
        assert_eq!(
            serde_json::from_value::<airk_core::inference_provider::CompletionObservation>(
                saved["completion"].clone()
            )?,
            *original
        );
    }
    assert_eq!(
        factory.empty_checkpoint(upgrade, vec![worker])?,
        Some(checkpoint)
    );
    assert!(factory.start(1).is_err());
    assert!(lease.admit_inference_owner().is_err());
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn factory_completion_refuses_compute_admission_outside_its_retained_roster() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let service = Incarnation::from_bytes([41; 16]);
    let _registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let mut factory = InferenceFactory::new(lease.clone(), service)?;
    let mut pool = factory.start(1)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while let Err(error) = pool.terminate() {
        assert!(error.to_string().contains("still draining"), "{error:#}");
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "native stop deadline expired"
        );
        std::thread::yield_now();
    }
    drop(pool);
    for event in factory.take_completed()? {
        event.acknowledge()?;
    }
    // A real safe Kernel scope shares the same lease admission counter. Its
    // completion cannot be invented from an unrelated owner's opaque witness.
    airk_kernel::inference_owner::run_service_inference(lease.clone(), || ())?;
    assert!(
        factory
            .empty_checkpoint(Incarnation::from_bytes([42; 16]), Vec::new())
            .is_err()
    );
    assert!(
        lease.admit_inference_owner().is_err(),
        "failed aggregate left fresh native admission open"
    );
    assert!(factory.start(1).is_err());
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn failed_completion_persistence_retains_owner_and_tamper_refuses_checkpoint() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let service = Incarnation::from_bytes([71; 16]);
    let _registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let mut factory = InferenceFactory::new(lease, service)?;
    let mut pool = factory.start(1)?;
    let blocked = root.join("provider-completions");
    std::fs::write(&blocked, b"owned fixture obstruction")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let failure = loop {
        match pool.terminate() {
            Ok(()) => anyhow::bail!("failed completion evidence write was accepted"),
            Err(error) if error.to_string().contains("still draining") => {
                anyhow::ensure!(std::time::Instant::now() < deadline, "native stop deadline");
                std::thread::yield_now();
            }
            Err(error) => break error,
        }
    };
    assert!(!failure.to_string().is_empty());
    assert!(
        pool.completion_witness().is_some(),
        "failed disk write lost actual native completion owner"
    );
    assert!(
        factory.start(1).is_err(),
        "replacement bypassed unresolved durable completion"
    );
    let operation = Incarnation::from_bytes([72; 16]);
    assert!(factory.empty_checkpoint(operation, Vec::new()).is_err());
    // Repair only our deliberately introduced fixture obstruction. Resuming
    // persistence reuses the same actual native barrier; no inference replay.
    std::fs::remove_file(&blocked)?;
    pool.terminate()?;
    let original = pool.completion_witness().unwrap().observation().clone();
    for event in factory.take_completed()? {
        assert_eq!(event.completion.observation(), &original);
        event.acknowledge()?;
    }
    drop(pool);
    let directory = std::fs::read_dir(&blocked)?.next().unwrap()?.path();
    let record = directory.join("00000000000000000001.json");
    let original_bytes = std::fs::read(&record)?;
    let mut tampered: serde_json::Value = serde_json::from_slice(&original_bytes)?;
    tampered["completion"]["admitted_high_water"] = serde_json::json!(1);
    std::fs::write(&record, serde_json::to_vec(&tampered)?)?;
    assert!(
        factory.empty_checkpoint(operation, Vec::new()).is_err(),
        "changed original barrier became checkpoint evidence"
    );
    assert!(factory.start(1).is_err());
    Ok(())
}
