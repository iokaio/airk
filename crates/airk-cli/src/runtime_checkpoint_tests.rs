// SPDX-License-Identifier: Apache-2.0
use super::*;

fn pending_diagnostic_observation() -> airk_core::service_checkpoint::UpgradeCheckpointObservation {
    airk_core::service_checkpoint::UpgradeCheckpointObservation {
        schema: 1,
        operation_id: "11111111111111111111111111111111".parse().unwrap(),
        service_instance: "22222222222222222222222222222222".parse().unwrap(),
        execution_sha256: "33".repeat(32).parse().unwrap(),
        candidate_execution_sha256: "44".repeat(32).parse().unwrap(),
        state: airk_core::service_checkpoint::CheckpointState::Pending,
        pending_operations: Some(3),
        sealed_workers: 2,
        state_sha256: None,
        native_packet_sha256: None,
        provider: None,
        blocked_code: Some("accepted_metadata_or_source_coordination_pending".into()),
        blocked_details: Some(r#"{"active_requests":0,"management_can_idle_exit":true,"source_admission_observed":false}"#.into()),
    }
}

#[test]
fn checkpoint_wait_failure_retains_complete_validated_pending_observation() -> Result<()> {
    let observation = pending_diagnostic_observation();
    observation.validate()?;
    let failure = checkpoint_wait_failure(
        "native_checkpoint_pending",
        Some(Box::new(observation.clone())),
    );
    let encoded: serde_json::Value = serde_json::from_str(&failure.to_string())?;
    assert_eq!(encoded["code"], "native_checkpoint_pending");
    assert_eq!(encoded["observation"], serde_json::to_value(&observation)?);
    assert_eq!(encoded["observation"]["state"], "pending");
    assert!(encoded["observation"]["native_packet_sha256"].is_null());
    Ok(())
}

#[test]
fn checkpoint_stage_diagnostics_are_bounded_exclusive_and_preserve_native_identity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let observation = pending_diagnostic_observation();
    checkpoint_diagnostic(directory.path(), 0, &observation)?;
    let path = directory.path().join(format!(
        "checkpoint-{}-{}-0.json",
        observation.service_instance, observation.operation_id
    ));
    let before = fs::read(&path)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&before)?,
        serde_json::to_value(&observation)?
    );
    assert!(checkpoint_diagnostic(directory.path(), 0, &observation).is_err());
    assert_eq!(fs::read(path)?, before);
    assert!(checkpoint_diagnostic(directory.path(), 32, &observation).is_err());
    let mut malformed = observation;
    malformed.schema = 2;
    let absent = directory.path().join("invalid-observation");
    assert!(checkpoint_diagnostic(&absent, 1, &malformed).is_err());
    assert!(!absent.exists());
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}

fn fresh_intent(root: &Path, candidate: String) -> Result<Intent> {
    Ok(Intent {
        schema: 1,
        install: root.canonicalize()?,
        control: layout::control(root)?,
        candidate,
        prior: None,
        manager: false,
        phase: Phase::Drain,
        checkpoint: None,
    })
}

#[test]
fn existing_empty_control_without_selection_uses_owned_policy_not_a_generated_os_plan() -> Result<()>
{
    let (_owner, root, candidate) = crate::runtime_install::tests::unselected_fixture()?;
    let mut intent = fresh_intent(&root, candidate)?;
    local_ipc::prepare_root(&intent.control)?;
    assert!(selection(&root)?.is_none());
    initialize(&root, &mut intent)?;
    assert!(
        intent.checkpoint.is_some(),
        "existing control must keep its checkpoint path"
    );
    assert!(!read(&root)?.unwrap().journal.manager_owned);
    preflight(&root, &intent)?;
    assert!(selection(&root)?.is_none());
    assert!(
        !intent.control.join("service-1.4.sqlite3").try_exists()?,
        "read-only preflight manufactured a registry"
    );
    let before = fs::read(root.join(JOURNAL))?;
    let mut changed = intent.clone();
    changed.manager = true;
    assert!(preflight(&root, &changed).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    Ok(())
}

#[tokio::test]
async fn safely_aborted_first_activation_can_begin_again_with_retained_control() -> Result<()> {
    let (_owner, root, candidate) = crate::runtime_install::tests::unselected_fixture()?;
    let mut original = fresh_intent(&root, candidate.clone())?;
    initialize(&root, &mut original)?;
    assert!(original.checkpoint.is_none());
    original.save(&root)?;
    drop(quiescent(&root)?);
    original.phase(&root, Phase::Rollback)?;
    // Exercise the canonical no-predecessor abort, including native owner
    // exclusion and its durable outcome. The inert approved fixture is never
    // executed: no service or predecessor exists to probe or restart.
    super::super::rollback(&root, &original).await?;
    assert!(pending(&root)?.is_none());
    assert!(selection(&root)?.is_none());
    assert!(original.control.is_dir());
    assert!(!root.join(JOURNAL).try_exists()?);

    let mut retry = fresh_intent(&root, candidate)?;
    initialize(&root, &mut retry)?;
    assert!(retry.checkpoint.is_some());
    retry.save(&root)?;
    preflight(&root, &retry)?;
    stage(&root, &retry, Stage::Quiescing)?;
    prepare_graceful(&root, &retry).await?;
    let exclusion = quiescent(&root)?;
    capture(&root, &retry, &exclusion)?;
    arm_recovery(&root, &retry, &exclusion)?;
    verify(&root, &retry)?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(
        root.join(name(retry.checkpoint.as_ref().unwrap())?),
    )?)?;
    assert!(manifest.boundary == Boundary::NoRunningServiceCommittedStores);
    assert!(manifest.state.registry_schema.is_none() && manifest.state.namespace.is_none());
    assert!(
        manifest.native.is_none(),
        "no running service cannot supply a graceful checkpoint ACK"
    );
    assert!(selection(&root)?.is_none());
    Ok(())
}

#[tokio::test]
async fn fresh_activation_has_no_upgrade_journal_or_recovery_fence() -> Result<()> {
    let (_owner, root, candidate) = crate::runtime_install::tests::unselected_fixture()?;
    let mut intent = fresh_intent(&root, candidate)?;
    initialize(&root, &mut intent)?;
    initialize(&root, &mut intent)?;
    assert!(intent.checkpoint.is_none());
    preflight(&root, &intent)?;
    prepare_graceful(&root, &intent).await?;
    stage(&root, &intent, Stage::Quiescing)?;
    assert!(!intent.control.try_exists()?);
    assert!(!root.join(JOURNAL).try_exists()?);
    // The ordinary installer exclusion may create the owned control/leases;
    // it must not manufacture a registry snapshot or upgrade completion.
    let exclusion = quiescent(&root)?;
    capture(&root, &intent, &exclusion)?;
    arm_recovery(&root, &intent, &exclusion)?;
    verify(&root, &intent)?;
    drop(exclusion);
    for stage_value in [
        Stage::OldStopped,
        Stage::ActivationPrepared,
        Stage::CandidateStarted,
        Stage::Committed,
        Stage::Aborted,
    ] {
        stage(&root, &intent, stage_value)?;
    }
    settle_admission(&root, &intent).await?;
    assert!(expected_namespace(&root, &intent)?.is_none());
    assert!(status(&root)?.is_none());
    assert!(!root.join(JOURNAL).try_exists()?);
    assert!(!intent.control.join("service-1.4.sqlite3").try_exists()?);
    assert!(!service_runtime::checkpoint::admission_closed(
        &intent.control
    )?);
    Ok(())
}

#[test]
fn fresh_exemption_preserves_existing_journal_and_lost_initialized_control() -> Result<()> {
    let (_owner, root, candidate) = crate::runtime_install::tests::unselected_fixture()?;
    let mut intent = fresh_intent(&root, candidate)?;
    let journal = Journal {
        schema: 1,
        operation: "e".repeat(32),
        install: root.clone(),
        control: intent.control.clone(),
        candidate: "d".repeat(64),
        predecessor: None,
        manager_owned: false,
        manager_definition_sha256: "b".repeat(64),
        stage: Stage::StagedVerified,
        revision: 0,
        previous_digest: None,
        checkpoint_sha256: None,
        observed_unix_ms: 1,
        predecessor_shutdown_failure: None,
    };
    save(&root, journal, None)?;
    let before = fs::read(root.join(JOURNAL))?;
    assert!(initialize(&root, &mut intent).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    assert!(!intent.control.try_exists()?);

    let (_other, root, candidate) = crate::runtime_install::tests::unselected_fixture()?;
    let mut intent = fresh_intent(&root, candidate)?;
    local_ipc::prepare_root(&intent.control)?;
    {
        let lease = airk_store::service::ServiceLease::acquire(&intent.control)?;
        drop(airk_store::service::registry::ServiceRegistry::open(
            &intent.control,
            airk_kernel::service_broker::new_instance_id()?,
        )?);
        layout::record_control(&lease)?;
    }
    fs::rename(&intent.control, root.join("retained-initialized-control"))?;
    assert!(initialize(&root, &mut intent).is_err());
    assert!(intent.checkpoint.is_none());
    assert!(!root.join(JOURNAL).try_exists()?);
    assert!(!intent.control.try_exists()?);
    Ok(())
}

#[test]
fn target_contract_preflight_allows_fresh_control_but_preserves_lost_owner() -> Result<()> {
    let (_owner, root, _) = crate::runtime_install::tests::unselected_fixture()?;
    let control = layout::control(&root)?;
    let absent_target = root.join("must-not-execute");
    assert!(!control.try_exists()?);
    require_target(&control, &absent_target)?;
    assert!(
        !control.try_exists()?,
        "read-only preflight created control"
    );

    local_ipc::prepare_root(&control)?;
    {
        let lease = airk_store::service::ServiceLease::acquire(&control)?;
        drop(airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?);
        layout::record_control(&lease)?;
    }
    let retained = root.join("retained-control-evidence");
    fs::rename(&control, &retained)?;
    assert!(require_target(&control, &absent_target).is_err());
    assert!(!control.try_exists()?);
    assert!(retained.join("service-1.4.sqlite3").is_file());
    Ok(())
}

fn fixture() -> Result<(tempfile::TempDir, PathBuf, Intent)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let intent = Intent {
        schema: 1,
        install: root.clone(),
        control: root.clone(),
        candidate: "a".repeat(64),
        prior: None,
        manager: false,
        phase: Phase::Drain,
        checkpoint: Some("c".repeat(32)),
    };
    let journal = Journal {
        schema: 1,
        operation: "c".repeat(32),
        install: root.clone(),
        control: root.clone(),
        candidate: "a".repeat(64),
        predecessor: None,
        manager_owned: false,
        manager_definition_sha256: "b".repeat(64),
        stage: Stage::StagedVerified,
        revision: 0,
        previous_digest: None,
        checkpoint_sha256: None,
        observed_unix_ms: 1,
        predecessor_shutdown_failure: None,
    };
    save(&root, journal, None)?;
    Ok((owner, root, intent))
}

#[test]
fn cas_refuses_stale_writer_and_preserves_original_journal() -> Result<()> {
    let (_owner, root, intent) = fixture()?;
    let before = read(&root)?.context("journal missing")?;
    stage(&root, &intent, Stage::Quiescing)?;
    let bytes = fs::read(root.join(JOURNAL))?;
    assert!(save(&root, before.journal, Some(&before.sha256)).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, bytes);
    stage(&root, &intent, Stage::Quiescing)?;
    assert_eq!(fs::read(root.join(JOURNAL))?, bytes);
    // Recovering the older durable activation phase cannot move proof back.
    stage(&root, &intent, Stage::StagedVerified)?;
    assert_eq!(fs::read(root.join(JOURNAL))?, bytes);
    Ok(())
}

#[test]
fn first_failed_graceful_shutdown_diagnostic_survives_recovery_without_proving_exit() -> Result<()>
{
    let (_owner, root, mut intent) = fixture()?;
    intent.prior = Some(Selection {
        schema: 1,
        active: "d".repeat(64),
        previous: None,
    });
    let mut original = read(&root)?.context("missing test journal")?;
    original.journal.predecessor = Some("d".repeat(64));
    save(&root, original.journal, Some(&original.sha256))?;
    stage(&root, &intent, Stage::Quiescing)?;
    predecessor_shutdown_failed(&root, &intent, &anyhow::anyhow!("é".repeat(700)))?;
    let captured = read(&root)?.context("missing captured diagnostic")?;
    let failure = captured
        .journal
        .predecessor_shutdown_failure
        .as_ref()
        .context("failure lost")?;
    assert!(failure.message.len() <= 1024 && failure.truncated && failure.bytes > 1024);
    assert_eq!(captured.journal.stage, Stage::Quiescing);
    assert!(captured.journal.checkpoint_sha256.is_none());
    assert!(
        !root
            .join(name(intent.checkpoint.as_ref().unwrap())?)
            .exists()
    );
    let before = fs::read(root.join(JOURNAL))?;
    predecessor_shutdown_failed(
        &root,
        &intent,
        &anyhow::anyhow!("a later retry must not erase the first cause"),
    )?;
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    let observed = status(&root)?.context("status missing")?;
    assert_eq!(observed["saved_only"], true);
    assert_eq!(observed["journal"]["journal"]["stage"], "quiescing");
    assert_eq!(observed["manifest"], serde_json::Value::Null);
    Ok(())
}

#[test]
fn checkpoint_ready_requires_manifest_and_crossed_operation_cannot_advance() -> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    stage(&root, &intent, Stage::Quiescing)?;
    assert!(stage(&root, &intent, Stage::CheckpointReady).is_err());
    assert!(stage(&root, &intent, Stage::CandidateStarted).is_err());
    let bytes = fs::read(root.join(JOURNAL))?;
    intent.checkpoint = Some("d".repeat(32));
    assert!(stage(&root, &intent, Stage::Aborted).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, bytes);
    Ok(())
}

#[test]
fn legacy_intent_decoder_preserves_exact_absent_checkpoint_field() -> Result<()> {
    let (_owner, _root, mut intent) = fixture()?;
    intent.checkpoint = None;
    let bytes = serde_json::to_vec(&intent)?;
    assert!(!String::from_utf8_lossy(&bytes).contains("checkpoint"));
    let old: Intent = serde_json::from_slice(&bytes)?;
    assert!(old.checkpoint.is_none());
    assert_eq!(serde_json::to_vec(&old)?, bytes);
    Ok(())
}

#[test]
fn predecessor_rollback_bytes_remain_strict_schema_one_while_new_installer_recovers_exact_journal()
-> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyIntent {
        schema: u32,
        install: PathBuf,
        control: PathBuf,
        candidate: String,
        prior: Option<Selection>,
        manager: bool,
        phase: Phase,
    }
    let (_owner, root, mut intent) = fixture()?;
    intent.phase = Phase::Rollback;
    intent.save(&root)?;
    let bytes = fs::read(root.join(INTENT))?;
    let legacy: LegacyIntent = serde_json::from_slice(&bytes)?;
    assert_eq!(legacy.schema, intent.schema);
    assert_eq!(legacy.install, intent.install);
    assert_eq!(legacy.control, intent.control);
    assert_eq!(legacy.candidate, intent.candidate);
    assert!(legacy.prior.is_none());
    assert_eq!(legacy.manager, intent.manager);
    assert!(matches!(legacy.phase, Phase::Rollback));
    let recovered = pending(&root)?.context("rollback intent absent")?;
    assert_eq!(recovered.checkpoint, intent.checkpoint);
    assert_eq!(fs::read(root.join(INTENT))?, bytes);
    let mut crossed = recovered;
    crossed.checkpoint = None;
    crossed.candidate = "e".repeat(64);
    assert!(reattach_legacy_rollback(&root, &mut crossed).is_err());
    Ok(())
}
