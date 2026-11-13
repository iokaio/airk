// SPDX-License-Identifier: Apache-2.0
//! Native provider restart and completion-ledger test. Moved from
//! `service_inference::tests` (split step 2b): it transfers completions
//! through the supervisor's pool recovery, which sits above inference.
use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
use airk_inference::service_inference::InferenceFactory;
use anyhow::Result;
use std::sync::Arc;

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
    let gate = airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
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
    let mut completed: Option<airk_inference::inference_completion_ledger::NativeLedgerWitness> =
        None;
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
    let obligation = airk_catalog_control::catalog_admission::native_provider_obligation_completed(
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
