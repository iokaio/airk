// SPDX-License-Identifier: Apache-2.0
//! Private execution for the exact claimed split child; no public proof payloads.
use airk_core::{
    catalog_split::SplitJobPurpose, identity::Sha256Digest, service::JobOutcome,
    service_configuration::ProjectConfiguration,
};
use airk_kernel::Kernel;
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) fn execute(
    kernel: &Kernel,
    registry: &mut ServiceRegistry,
    lease: &ProjectLease,
    job: Sha256Digest,
    engine: Sha256Digest,
    runtime: &Path,
) -> Result<JobOutcome> {
    let claimed = registry.claimed_split_work(lease, job, engine)?;
    let state = registry.catalog_split_state(claimed.snapshot.receipt.operation_id)?;
    if claimed.job.purpose == SplitJobPurpose::OriginalWitness {
        let witness = kernel.qualify_split_remainder_witness(
            &state,
            airk_kernel::split::SplitWitnessLimits::default(),
        )?;
        let proof =
            airk_store::service::registry::catalog::split::NativeSplitJobResult::from_witness(
                &claimed, witness,
            )?;
        registry.record_split_job_result(lease, job, proof)?;
        return Ok(JobOutcome::Succeeded { revision: None });
    }
    let funding = crate::catalog_split_control::prepare_claimed_funding(registry, &claimed)?;
    if claimed.job.purpose == SplitJobPurpose::DestinationCleanup {
        for _ in 0..16 {
            airk_kernel::service_execution::checkpoint()?;
            if let Some(proof) =
                registry.cleanup_split_destination(lease, job, engine, funding.as_ref())?
            {
                registry.record_split_job_result(lease, job, proof)?;
                return Ok(JobOutcome::Succeeded { revision: None });
            }
        }
        anyhow::bail!(
            "split destination cleanup reached its bounded turn limit; explicit continuation required"
        );
    }
    ensure!(
        funding.is_some(),
        "split current work lacks prepaid funding"
    );
    let selected = kernel.with_split_job_access(claimed.permit.clone(), funding)?;
    if claimed.job.purpose == SplitJobPurpose::OriginalRemoval {
        let work = registry
            .configuration_work(job)?
            .context("split policy configuration missing")?;
        ensure!(
            matches!(&work.request.configuration,
            ProjectConfiguration::ApplySourcePolicy { policy, .. } if **policy == claimed.snapshot.plan.remainder),
            "split removal differs from accepted native policy work"
        );
        let result = selected.configure_service_project(lease.identity().0, &work.request)?;
        selected.checkpoint_completed_split_work()?;
        registry.record_configuration_result(lease, job, &result)?;
        return Ok(JobOutcome::Succeeded { revision: None });
    }
    let selected = if claimed.job.purpose == SplitJobPurpose::DestinationBuild
        && claimed.snapshot.request.require_semantic_ready
    {
        crate::bundled_assets::bind_service_models(selected, runtime)?
    } else {
        selected
    };
    let policy_operation = if claimed.job.purpose == SplitJobPurpose::OriginalCleanup {
        let original = claimed
            .snapshot
            .receipt
            .jobs
            .iter()
            .rev()
            .find(|reference| reference.purpose == SplitJobPurpose::OriginalRemoval)
            .context("split cleanup lacks original policy child")?;
        let work = registry
            .configuration_work(original.job)?
            .context("split original policy configuration missing")?;
        let ProjectConfiguration::ApplySourcePolicy { operation_id, .. } =
            work.request.configuration
        else {
            anyhow::bail!("split cleanup original child is not policy work");
        };
        operation_id
    } else {
        claimed.snapshot.receipt.operation_id
    };
    let proof = selected.execute_split_current_work(&claimed, policy_operation)?;
    if claimed.job.purpose == SplitJobPurpose::OriginalCleanup {
        ensure!(
            proof.result().cleanup_complete,
            "split original cleanup still has bounded work pending: {}",
            proof
                .result()
                .policy_application
                .as_ref()
                .and_then(|report| report.cleanup_error.as_deref())
                .unwrap_or("another bounded cleanup turn is required")
        );
    }
    registry.record_split_job_result(lease, job, proof)?;
    Ok(JobOutcome::Succeeded { revision: None })
}

#[cfg(test)]
#[path = "project_worker_split_ignore_fixture.rs"]
mod ignore_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::catalog_split::{SplitJobResult, SplitStage};

    #[test]
    fn claimed_paid_build_creates_independent_sealed_evidence_without_changing_original()
    -> Result<()> {
        if ignore_fixture::run(
            "claimed_paid_build_creates_independent_sealed_evidence_without_changing_original",
        )? {
            return Ok(());
        }
        run_paid_build_and_cancel(crate::catalog_split_control::tests::native_fixture(true)?)
    }
    #[test]
    fn claimed_paid_inherited_retention_build_and_cancel_preserves_source_and_two_root_accounting()
    -> Result<()> {
        if ignore_fixture::run(
            "claimed_paid_inherited_retention_build_and_cancel_preserves_source_and_two_root_accounting",
        )? {
            return Ok(());
        }
        run_paid_build_and_cancel(
            crate::catalog_split_control::tests::native_fixture_with_retention(
                true,
                Some(airk_core::retention::RetainedProfile::Lean),
            )?,
        )
    }
    fn run_paid_build_and_cancel(
        mut fixture: crate::catalog_split_control::tests::NativeSplitFixture,
    ) -> Result<()> {
        let accepted = fixture
            .snapshot
            .take()
            .context("prepared native split missing")?;
        // All growth credit is already reserved. Leave too little ordinary
        // headroom for a hidden metadata/current re-open to succeed.
        for root in [
            &accepted.request.original_registration.data_root,
            &accepted.request.destination_registration.data_root,
        ] {
            let quota = airk_store::quota::Quota::open_service(Path::new(root))?;
            let status = quota.status()?;
            let mut policy = quota.policy()?;
            policy.root_bytes = status.used_bytes + status.reserved_bytes + 128;
            quota.configure(policy)?;
            assert!(quota.reserve("competing", 1024).is_err());
        }
        let original = Kernel::open_existing_service_observer(&fixture.original)?;
        let before = original
            .status()?
            .publication
            .context("original publication missing")?
            .token;
        let source = Path::new(&fixture.original.root);
        assert!(source.starts_with(fixture.temporary.path().canonicalize()?));
        let selected_bytes = std::fs::read(source.join("docs/selected.md"))?;
        let remainder_bytes = std::fs::read(source.join("remainder.md"))?;
        let advanced = crate::catalog_split_control::advance_pending(
            &mut fixture.registry,
            &fixture.control,
            fixture.engine,
            accepted.receipt.operation_id,
        )?;
        let crate::catalog_split_control::SplitAdvance::Advanced(building) = advanced else {
            anyhow::bail!("prepared split did not enqueue its native build");
        };
        assert_eq!(building.receipt.stage, SplitStage::Building);
        let job = building
            .receipt
            .jobs
            .last()
            .context("split build child missing")?
            .job;
        let registration = &building.request.destination_registration;
        let mut lease = ProjectLease::acquire(&fixture.control, registration.binding)?;
        fixture.registry.admit_worker(&mut lease)?;
        assert_eq!(
            fixture
                .registry
                .claim_job_id(&lease, job)?
                .context("native split build not claimable")?
                .id,
            job
        );
        let observer = Kernel::open_existing_service_observer(registration)?;
        assert!(observer.require_unrestricted_read().is_err());
        let outcome = execute(
            &observer,
            &mut fixture.registry,
            &lease,
            job,
            fixture.engine,
            &fixture.control,
        )?;
        assert!(matches!(outcome, JobOutcome::Succeeded { .. }));
        let result: SplitJobResult = serde_json::from_value(
            fixture
                .registry
                .configuration_result(job)?
                .context("opaque native build result missing")?,
        )?;
        assert_eq!(result.selected_sources, 1);
        assert_eq!(result.policy_digest, building.plan.destination.identity()?);
        assert_ne!(result.publication, Some(before));
        if let Some(reference) = &building.request.extraction_policy {
            let claimed = fixture
                .registry
                .claimed_split_work(&lease, job, fixture.engine)?;
            let funding =
                crate::catalog_split_control::prepare_claimed_funding(&fixture.registry, &claimed)?
                    .context("inherited paid phase absent")?;
            let collection = airk_store::service::collection::Collection::existing(
                Path::new(&registration.data_root),
                &registration.root,
                &registration.collection,
            )?;
            let store = collection
                .select_for_diagnostics()?
                .with_split_access(claimed.permit)?
                .with_operation_funding(funding)?;
            let scope = store
                .current_scope(&registration.root, &registration.collection)?
                .context("inherited current absent")?;
            let read = store.current_read(&scope)?;
            let applied = read
                .collection_state()?
                .context("inherited applied metadata absent")?
                .configuration;
            assert_eq!(applied.retained_profile, Some(reference.retained_profile));
            assert_eq!(
                applied.extraction_policy_digest,
                reference.configured_policy_digest
            );
            let artifact = read
                .artifact("docs/selected.md")?
                .context("selected inherited artifact absent")?;
            let profile = artifact
                .retained
                .as_ref()
                .and_then(|retained| retained.profile.as_ref())
                .context("selected inherited profile absent")?;
            assert!(
                matches!(airk_core::retention::AppliedRetention::from_profile(profile)?,airk_core::retention::AppliedRetention::Selected{profile:target,family:airk_core::retention::RetainedFamily::Markdown,..} if target==reference.retained_profile)
            );
            assert!(read.artifact("remainder.md")?.is_none());
            let body = store
                .extraction_policy_bytes(&registration.root)?
                .context("inherited private body absent")?;
            reference.validate_body(&body)?;
        }
        fixture.registry.complete_job(&lease, job, outcome)?;
        let validated = fixture.registry.catalog_complete_split_job(
            building.receipt.operation_id,
            building.receipt_digest,
            job,
        )?;
        assert_eq!(validated.receipt.stage, SplitStage::DestinationValidated);
        assert!(
            Kernel::open_existing_service_observer(registration)?
                .require_unrestricted_read()
                .is_err()
        );
        assert_eq!(
            original
                .status()?
                .publication
                .context("original publication disappeared")?
                .token,
            before
        );
        assert_eq!(
            std::fs::read(source.join("docs/selected.md"))?,
            selected_bytes
        );
        assert_eq!(std::fs::read(source.join("remainder.md"))?, remainder_bytes);
        let funding = airk_store::quota::Quota::open_service(Path::new(&registration.data_root))?
            .observe_funding(building.receipt.operation_id)?;
        let participant = funding
            .participants
            .iter()
            .find(|participant| participant.index_id == building.request.destination.index_id)
            .context("destination funded participant missing")?;
        assert_eq!(
            participant.settled_revision,
            participant.phase.as_ref().map(|phase| phase.revision)
        );
        assert!(funding.children.iter().all(|child| !matches!(
            child.kind,
            airk_store::quota::funding::FundingChildKind::Reservation { .. }
        )));
        let cancel = airk_core::catalog_split::SplitRecoveryRequest {
            caller: validated.request.caller,
            key: airk_core::service::RequestKey {
                nonce: airk_core::identity::Incarnation::from_bytes([101; 16]),
                ..validated.request.key.clone()
            },
            operation_id: validated.receipt.operation_id,
            request_digest: validated.receipt.request_digest,
            expected_receipt_digest: validated.receipt_digest,
            expected_finalization_revision: None,
            action: airk_core::catalog_split::SplitRecoveryAction::Cancel,
        };
        crate::catalog_split_control::execute(
            &mut fixture.registry,
            &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitRecover {
                request: Box::new(cancel.clone()),
            },
        )?
        .context("native cancellation route missing")?;
        let mut saw_acknowledging = false;
        let mut completed = false;
        for _ in 0..48 {
            let current = fixture
                .registry
                .catalog_split_status(cancel.operation_id)?
                .context("canceled split disappeared")?;
            if let Some(finalization) = &current.finalization {
                if finalization.progress
                    == airk_core::catalog_split::SplitFinalizationProgress::Acknowledging
                    && !saw_acknowledging
                {
                    let pending_cancel = airk_core::catalog_split::SplitRecoveryRequest {
                        key: airk_core::service::RequestKey {
                            nonce: airk_core::identity::Incarnation::from_bytes([102; 16]),
                            ..cancel.key.clone()
                        },
                        expected_receipt_digest: current.receipt_digest,
                        expected_finalization_revision: Some(finalization.revision),
                        ..cancel.clone()
                    };
                    let error = crate::catalog_split_control::execute(
                        &mut fixture.registry,
                        &fixture.control,
                        &airk_core::service::ControlMethod::CatalogSplitRecover {
                            request: Box::new(pending_cancel.clone()),
                        },
                    );
                    // This public mutation cannot claim AlreadyComplete while
                    // real quota ownership still awaits acknowledgment.
                    let error = error
                        .err()
                        .context("pending native acknowledgment was reported complete")?;
                    assert!(format!("{error:#}").contains("split finalization is still pending"));
                    assert!(
                        fixture
                            .registry
                            .catalog_split_recovery_receipt(
                                pending_cancel.caller,
                                &pending_cancel.key
                            )?
                            .is_none()
                    );
                    assert_eq!(
                        fixture.registry.catalog_split_status(cancel.operation_id)?,
                        Some(current.clone())
                    );
                    saw_acknowledging = true;
                }
                if finalization.progress
                    == airk_core::catalog_split::SplitFinalizationProgress::Complete
                {
                    completed = true;
                    break;
                }
            }
            if let Some(child) = current.receipt.jobs.last()
                && child.purpose == SplitJobPurpose::DestinationCleanup
                && fixture.registry.job(child.job)?.state == airk_core::service::JobState::Pending
            {
                fixture
                    .registry
                    .claim_job_id(&lease, child.job)?
                    .context("native cleanup job not claimable")?;
                let outcome = execute(
                    &observer,
                    &mut fixture.registry,
                    &lease,
                    child.job,
                    fixture.engine,
                    &fixture.control,
                )?;
                fixture.registry.complete_job(&lease, child.job, outcome)?;
            } else {
                crate::catalog_split_control::advance_pending(
                    &mut fixture.registry,
                    &fixture.control,
                    fixture.engine,
                    cancel.operation_id,
                )?;
            }
        }
        assert!(
            saw_acknowledging && completed,
            "canceled split did not finish actual native acknowledgments"
        );
        assert_eq!(
            original
                .status()?
                .publication
                .context("original publication disappeared after cancellation")?
                .token,
            before
        );
        assert_eq!(
            std::fs::read(source.join("docs/selected.md"))?,
            selected_bytes
        );
        assert_eq!(std::fs::read(source.join("remainder.md"))?, remainder_bytes);
        Ok(())
    }

    #[test]
    fn actual_witness_reaches_remainder_after_more_than_256_selected_prefix_files() -> Result<()> {
        if ignore_fixture::run(
            "actual_witness_reaches_remainder_after_more_than_256_selected_prefix_files",
        )? {
            return Ok(());
        }
        let mut fixture = crate::catalog_split_control::tests::native_fixture(false)?;
        let root = Path::new(&fixture.original.root);
        for index in 0..300 {
            std::fs::write(
                root.join(format!("docs/selected-{index:04}.md")),
                format!("selected document {index}\n"),
            )?;
        }
        let original = Kernel::open_service_collection(
            root,
            Some(Path::new(&fixture.original.data_root).to_path_buf()),
            &fixture.original.collection,
        )?;
        original.configure_vectors(false)?;
        original.index()?;
        let original_token = original
            .status()?
            .publication
            .context("original publication absent")?
            .token;
        drop(original);
        let response = crate::catalog_split_control::execute(
            &mut fixture.registry,
            &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitDraft {
                selection: Box::new(fixture.selection.clone()),
            },
        )?
        .context("canonical split draft route missing")?;
        let airk_core::service::ControlResponse::CatalogSplitDraft { draft } = response else {
            anyhow::bail!("draft response differs")
        };
        let response = crate::catalog_split_control::execute(
            &mut fixture.registry,
            &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitApply { draft },
        )?
        .context("canonical apply missing")?;
        let airk_core::service::ControlResponse::CatalogSplitAccepted { snapshot } = response
        else {
            anyhow::bail!("apply response differs")
        };
        let operation = snapshot.receipt.operation_id;
        let mut building = None;
        for _ in 0..12 {
            let current = fixture
                .registry
                .catalog_split_status(operation)?
                .context("split disappeared")?;
            if current.receipt.stage == SplitStage::Building {
                building = Some(current);
                break;
            }
            crate::catalog_split_control::advance_pending(
                &mut fixture.registry,
                &fixture.control,
                fixture.engine,
                operation,
            )?;
        }
        let building = building.context("split never reached actual build")?;
        let build = building
            .receipt
            .jobs
            .last()
            .context("build job missing")?
            .job;
        let destination = &building.request.destination_registration;
        let mut destination_lease = ProjectLease::acquire(&fixture.control, destination.binding)?;
        fixture.registry.admit_worker(&mut destination_lease)?;
        fixture
            .registry
            .claim_job_id(&destination_lease, build)?
            .context("build not claimable")?;
        let observer = Kernel::open_existing_service_observer(destination)?;
        let outcome = execute(
            &observer,
            &mut fixture.registry,
            &destination_lease,
            build,
            fixture.engine,
            &fixture.control,
        )?;
        fixture
            .registry
            .complete_job(&destination_lease, build, outcome)?;
        let validated = fixture.registry.catalog_complete_split_job(
            operation,
            building.receipt_digest,
            build,
        )?;
        assert_eq!(validated.receipt.stage, SplitStage::DestinationValidated);
        drop(observer);
        drop(destination_lease);
        crate::catalog_split_control::advance_pending(
            &mut fixture.registry,
            &fixture.control,
            fixture.engine,
            operation,
        )?;
        let pending = fixture
            .registry
            .catalog_split_status(operation)?
            .context("witness state missing")?;
        let witness = pending
            .receipt
            .jobs
            .last()
            .context("witness child missing")?;
        assert_eq!(witness.purpose, SplitJobPurpose::OriginalWitness);
        let mut lease = ProjectLease::acquire(&fixture.control, fixture.original.binding)?;
        fixture.registry.admit_worker(&mut lease)?;
        fixture
            .registry
            .claim_job_id(&lease, witness.job)?
            .context("witness not claimable")?;
        let original = Kernel::open_existing_service_observer(&fixture.original)?;
        let outcome = execute(
            &original,
            &mut fixture.registry,
            &lease,
            witness.job,
            fixture.engine,
            &fixture.control,
        )?;
        let result: SplitJobResult = serde_json::from_value(
            fixture
                .registry
                .configuration_result(witness.job)?
                .context("opaque witness result absent")?,
        )?;
        let proof = result
            .remainder_witness
            .context("positive native source proof absent")?;
        assert_eq!(proof.logical_path, "remainder.md");
        assert_eq!(proof.source_relative_path, "remainder.md");
        assert_eq!(proof.publication, original_token);
        assert_eq!(proof.job, Some(witness.job));
        assert!(proof.bytes_read > 0);
        fixture
            .registry
            .complete_job(&lease, witness.job, outcome)?;
        let witnessed = fixture.registry.catalog_complete_split_job(
            operation,
            pending.receipt_digest,
            witness.job,
        )?;
        assert_eq!(witnessed.receipt.remainder_witness, Some(proof));
        assert_eq!(
            original
                .status()?
                .publication
                .context("original publication disappeared")?
                .token,
            original_token
        );
        assert_eq!(
            std::fs::read(root.join("remainder.md"))?,
            b"original positive remainder\n"
        );
        drop(original);
        drop(lease);
        let temporary = fixture.temporary.path().to_path_buf();
        drop(fixture);
        assert!(
            !temporary.exists(),
            "SQLite handles must close before TempDir cleanup on Windows"
        );
        Ok(())
    }

    #[test]
    #[ignore = "Subprocess crash entry; exercised by native_canceled_takeover_recovers_abrupt_capture_without_releasing_stage_before_disposal"]
    fn abrupt_paid_capture_child() -> Result<()> {
        let control = std::env::var_os("AIRK_SPLIT_CAPTURE_CRASH_CONTROL")
            .context("crash entry requires the actual parent fixture")?;
        let control = Path::new(&control);
        let job: Sha256Digest = std::env::var("AIRK_SPLIT_CAPTURE_CRASH_JOB")?.parse()?;
        let engine: Sha256Digest = std::env::var("AIRK_SPLIT_CAPTURE_CRASH_ENGINE")?.parse()?;
        let instance = ServiceRegistry::open_saved(control)?.incarnation()?;
        let mut registry = ServiceRegistry::open(control, instance)?;
        let configuration = registry
            .configuration_work(job)?
            .context("actual crash work missing")?;
        let airk_core::service_configuration::ProjectConfiguration::SplitWork {
            operation_id, ..
        } = configuration.request.configuration
        else {
            anyhow::bail!("crash child is not split work")
        };
        let snapshot = registry
            .catalog_split_status(operation_id)?
            .context("crash split absent")?;
        let registration = &snapshot.request.destination_registration;
        let mut lease = ProjectLease::acquire(control, registration.binding)?;
        registry.admit_worker(&mut lease)?;
        registry
            .claim_job_id(&lease, job)?
            .context("crash build not claimable")?;
        let observer = Kernel::open_existing_service_observer(registration)?;
        let _outcome = execute(&observer, &mut registry, &lease, job, engine, control)?;
        // Actual native build is Ready and its opaque result is durable, while
        // the accepted job remains running until the owner records completion.
        let work = registry.claimed_split_work(&lease, job, engine)?;
        let owner = crate::catalog_split_control::prepare_claimed_funding(&registry, &work)?
            .context("crash funding absent")?;
        let collection = airk_store::service::collection::Collection::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        let store = collection
            .select_for_diagnostics()?
            .with_split_access(work.permit.clone())?
            .with_operation_funding(owner.clone())?;
        let writer = store.lock()?;
        let quota = airk_store::quota::Quota::for_store(&store)?;
        let reservation = quota.reserve(&registration.collection, 256 << 10)?;
        reservation.charge(b"preserved crash evidence".len() as u64)?;
        let staging = reservation.staging_directory()?;
        std::fs::create_dir_all(&staging)?;
        std::fs::write(staging.join("owned-sentinel"), b"preserved crash evidence")?;
        let before = quota.observe_funding(operation_id)?;
        assert!(
            store
                .close_abandoned_operation_reservations(&writer, &quota)
                .is_err(),
            "a live native capture must refuse closure"
        );
        assert_eq!(quota.observe_funding(operation_id)?, before);
        assert!(
            airk_store::FileStore::new(store.directory())
                .close_abandoned_operation_reservations(&writer, &quota)
                .is_err(),
            "observer must have no recovery authority"
        );
        let child = before
            .children
            .iter()
            .find(|child| {
                !child.closed
                    && child.charged == b"preserved crash evidence".len() as u64
                    && matches!(
                        child.kind,
                        airk_store::quota::funding::FundingChildKind::Reservation { .. }
                    )
            })
            .context("real funded capture absent")?;
        std::fs::write(
            control.join("capture-crash.json"),
            serde_json::to_vec(
                &serde_json::json!({"child":child.id,"staging":staging,"capacity":child.capacity,"charged":child.charged}),
            )?,
        )?;
        std::process::exit(79)
    }

    #[test]
    fn native_canceled_takeover_recovers_abrupt_capture_without_releasing_stage_before_disposal()
    -> Result<()> {
        if ignore_fixture::run(
            "native_canceled_takeover_recovers_abrupt_capture_without_releasing_stage_before_disposal",
        )? {
            return Ok(());
        }
        let mut fixture = crate::catalog_split_control::tests::native_fixture(true)?;
        let prepared = fixture.snapshot.take().context("prepared split absent")?;
        let crate::catalog_split_control::SplitAdvance::Advanced(building) =
            crate::catalog_split_control::advance_pending(
                &mut fixture.registry,
                &fixture.control,
                fixture.engine,
                prepared.receipt.operation_id,
            )?
        else {
            anyhow::bail!("build not enqueued")
        };
        let build = building.receipt.jobs.last().context("build missing")?.job;
        let stdout_path = fixture.control.join("capture-crash.stdout.log");
        let stderr_path = fixture.control.join("capture-crash.stderr.log");
        let stdout = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout_path)?;
        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr_path)?;
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--ignored",
                "--exact",
                "project_worker_split::tests::abrupt_paid_capture_child",
                "--nocapture",
            ])
            .env("AIRK_SPLIT_CAPTURE_CRASH_CONTROL", &fixture.control)
            .env("AIRK_SPLIT_CAPTURE_CRASH_JOB", build.to_string())
            .env(
                "AIRK_SPLIT_CAPTURE_CRASH_ENGINE",
                fixture.engine.to_string(),
            )
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
        drop(command);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            ensure!(
                std::time::Instant::now() < deadline,
                "contained crash child exceeded 60s; see {:?} and {:?}",
                stdout_path,
                stderr_path
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        // ContainedChild drains/reaps this exact process and descendants on
        // every path, including timeout or native wait errors above.
        drop(child);
        ensure!(
            std::fs::metadata(&stdout_path)?.len() <= 16 << 20
                && std::fs::metadata(&stderr_path)?.len() <= 16 << 20,
            "crash child log exceeds bounded diagnostic read"
        );
        assert_eq!(
            status.code(),
            Some(79),
            "{}\n{}",
            String::from_utf8_lossy(&std::fs::read(&stdout_path)?),
            String::from_utf8_lossy(&std::fs::read(&stderr_path)?)
        );
        let crash: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.control.join("capture-crash.json"))?)?;
        let child: Sha256Digest = serde_json::from_value(crash["child"].clone())?;
        let staging = Path::new(
            crash["staging"]
                .as_str()
                .context("crash stage path absent")?,
        );
        assert!(staging.starts_with(&building.request.destination_registration.data_root));
        let quota = airk_store::quota::Quota::open_service(Path::new(
            &building.request.destination_registration.data_root,
        ))?;
        let before = quota.observe_funding(building.receipt.operation_id)?;
        let abandoned = before
            .children
            .iter()
            .find(|row| row.id == child)
            .context("crash capture lost")?;
        assert!(!abandoned.closed);
        assert_eq!(
            std::fs::read(staging.join("owned-sentinel"))?,
            b"preserved crash evidence"
        );
        let mut lease = ProjectLease::acquire(
            &fixture.control,
            building.request.destination_registration.binding,
        )?;
        fixture.registry.admit_worker(&mut lease)?;
        fixture.registry.recover_jobs(&lease)?;
        let current = fixture
            .registry
            .catalog_split_status(building.receipt.operation_id)?
            .context("split disappeared")?;
        let cancel = airk_core::catalog_split::SplitRecoveryRequest {
            caller: current.request.caller,
            key: airk_core::service::RequestKey {
                nonce: airk_core::identity::Incarnation::from_bytes([104; 16]),
                ..current.request.key.clone()
            },
            operation_id: current.receipt.operation_id,
            request_digest: current.receipt.request_digest,
            expected_receipt_digest: current.receipt_digest,
            expected_finalization_revision: None,
            action: airk_core::catalog_split::SplitRecoveryAction::Cancel,
        };
        crate::catalog_split_control::execute(
            &mut fixture.registry,
            &fixture.control,
            &airk_core::service::ControlMethod::CatalogSplitRecover {
                request: Box::new(cancel),
            },
        )?
        .context("canonical cancel absent")?;
        let mut cleanup = None;
        for _ in 0..6 {
            let current = fixture
                .registry
                .catalog_split_status(building.receipt.operation_id)?
                .context("canceled state absent")?;
            if let Some(job) = current
                .receipt
                .jobs
                .last()
                .filter(|job| job.purpose == SplitJobPurpose::DestinationCleanup)
            {
                cleanup = Some(job.job);
                break;
            }
            crate::catalog_split_control::advance_pending(
                &mut fixture.registry,
                &fixture.control,
                fixture.engine,
                building.receipt.operation_id,
            )?;
        }
        let cleanup = cleanup.context("actual cleanup child absent")?;
        fixture
            .registry
            .claim_job_id(&lease, cleanup)?
            .context("cleanup not claimable")?;
        let work = fixture
            .registry
            .claimed_split_work(&lease, cleanup, fixture.engine)?;
        crate::catalog_split_control::prepare_claimed_funding(&fixture.registry, &work)?
            .context("cleanup funding absent")?;
        let held = quota.observe_funding(building.receipt.operation_id)?;
        let closed = held
            .children
            .iter()
            .find(|row| row.id == child)
            .context("custody transfer prematurely removed capture")?;
        assert!(closed.closed);
        assert_eq!(closed.capacity, abandoned.capacity);
        assert_eq!(closed.charged, abandoned.charged);
        assert_eq!(closed.phase.job, cleanup);
        let current_rows = |record: &airk_store::quota::funding::FundingReceipt| {
            record
                .children
                .iter()
                .filter(|row| {
                    matches!(
                        row.kind,
                        airk_store::quota::funding::FundingChildKind::Current { .. }
                    )
                })
                .map(|row| (row.id, row.capacity, row.charged, row.closed))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            current_rows(&held),
            current_rows(&before),
            "capture custody must not release or alter journal grants"
        );
        assert_eq!(
            std::fs::read(staging.join("owned-sentinel"))?,
            b"preserved crash evidence"
        );
        // A same-prefix entry with no exact admitted child remains unowned.
        // Preflight must preserve both it and all current recovery evidence.
        let logical = staging.parent().context("crash stage collection absent")?;
        let unknown = logical.join(format!(
            ".quota-stage-r-funded-{}",
            Sha256Digest::from_bytes([0; 32])
        ));
        assert_ne!(unknown, staging);
        std::fs::create_dir(&unknown)?;
        std::fs::write(
            unknown.join("foreign-source"),
            b"unadmitted entry remains unchanged",
        )?;
        let native_current = airk_store::service::collection::Collection::existing(
            Path::new(&building.request.destination_registration.data_root),
            &building.request.destination_registration.root,
            &building.request.destination_registration.collection,
        )?
        .select_for_diagnostics()?
        .directory()
        .join("current.sqlite3");
        assert!(native_current.is_file());
        let observer =
            Kernel::open_existing_service_observer(&building.request.destination_registration)?;
        let error = execute(
            &observer,
            &mut fixture.registry,
            &lease,
            cleanup,
            fixture.engine,
            &fixture.control,
        )
        .err()
        .context("unadmitted staging was accepted")?;
        assert!(
            format!("{error:#}").contains("unknown removal entry is preserved"),
            "{error:#}"
        );
        assert!(
            native_current.is_file(),
            "unknown staging must refuse before payload deletion"
        );
        assert_eq!(
            std::fs::read(unknown.join("foreign-source"))?,
            b"unadmitted entry remains unchanged"
        );
        assert_eq!(
            std::fs::read(staging.join("owned-sentinel"))?,
            b"preserved crash evidence"
        );
        std::fs::remove_file(unknown.join("foreign-source"))?;
        std::fs::remove_dir(&unknown)?;
        let outcome = execute(
            &observer,
            &mut fixture.registry,
            &lease,
            cleanup,
            fixture.engine,
            &fixture.control,
        )?;
        assert!(matches!(outcome, JobOutcome::Succeeded { .. }));
        assert!(
            !staging.exists(),
            "only actual completed disposal may remove abandoned staging"
        );
        let after = quota.observe_funding(building.receipt.operation_id)?;
        assert!(after.children.iter().all(|row| row.id != child));
        assert_eq!(
            std::fs::read(Path::new(&fixture.original.root).join("docs/selected.md"))?,
            b"selected native split document\n"
        );
        assert_eq!(
            std::fs::read(Path::new(&fixture.original.root).join("remainder.md"))?,
            b"original positive remainder\n"
        );
        Ok(())
    }
}
