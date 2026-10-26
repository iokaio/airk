// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, PathBuf, Intent)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let mut identities = Vec::new();
    for number in 0..2 {
        let stage = root.join(format!("fixture-{number}"));
        local_ipc::prepare_root(&stage)?;
        super::super::tests::fixture(&stage)?;
        fs::write(
            stage.join("LICENSE"),
            format!("Fictional approved inventory {number}."),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(stage.join("notices"), fs::Permissions::from_mode(0o700))?;
        }
        let mut inventory: InstallManifest =
            serde_json::from_slice(&fs::read(stage.join("airk-install.json"))?)?;
        inventory.files.insert(
            "LICENSE".into(),
            runtime_pair::digest_file(&stage.join("LICENSE"))?,
        );
        let bytes = serde_json::to_vec(&inventory)?;
        let identity = format!("{:x}", Sha256::digest(&bytes));
        fs::write(stage.join("airk-install.json"), bytes)?;
        replace_json(
            &stage,
            "approval.json",
            &Approval {
                schema: 1,
                manifest_sha256: identity.clone(),
                version: env!("CARGO_PKG_VERSION").into(),
                verification_basis: VerificationBasis::OrganizationDigest,
            },
        )?;
        fs::rename(stage, root.join("runtimes").join(&identity))?;
        identities.push(identity);
    }
    let prior = Selection {
        schema: 1,
        active: identities[0].clone(),
        previous: None,
    };
    replace_json(&root, "selected.json", &prior)?;
    let intent = Intent {
        schema: 1,
        install: root.canonicalize()?,
        control: layout::control(&root)?,
        candidate: identities[1].clone(),
        prior: Some(prior),
        manager: false,
        phase: Phase::Drain,
    };
    Ok((owner, root, intent))
}

#[test]
fn reviewed_activation_refuses_a_changed_selection_and_crossed_pending_intent() -> Result<()> {
    let (_owner, root, intent) = fixture()?;
    let expected = &intent.prior.as_ref().unwrap().active;
    assert!(!reviewed_request(&root, &intent.candidate, expected)?);
    assert!(reviewed_request(&root, &intent.candidate, &"f".repeat(64)).is_err());
    assert!(!root.join(INTENT).exists());
    assert!(selection(&root)? == intent.prior);
    intent.save(&root)?;
    assert!(!reviewed_request(&root, &intent.candidate, expected)?);
    assert!(reviewed_request(&root, &intent.candidate, &"f".repeat(64)).is_err());
    assert!(selection(&root)? == intent.prior);
    Ok(())
}

#[tokio::test]
async fn completed_reviewed_activation_acknowledges_without_starting_or_reinitializing_state()
-> Result<()> {
    let (_owner, root, intent) = fixture()?;
    let expected = &intent.prior.as_ref().unwrap().active;
    replace_json(&root, "selected.json", &intent.selected())?;
    record(&root, &intent, Outcome::Selected)?;
    let sentinel = b"Original invalid startup marker must not be interpreted by a completed replay";
    fs::write(root.join("bootstrap.json"), sentinel)?;
    assert!(reviewed_request(&root, &intent.candidate, expected)?);
    assert!(
        super::super::activate_reviewed(&root, &intent.candidate, expected).await?
            == intent.selected()
    );
    assert_eq!(fs::read(root.join("bootstrap.json"))?, sentinel);
    assert!(!layout::control(&root)?.join("service-1.4.airk").exists());
    assert!(reviewed_request(&root, &intent.candidate, &"f".repeat(64)).is_err());
    Ok(())
}

#[test]
fn foreground_follows_only_probing_or_restoring_intents_without_advancing_public_selection()
-> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    for phase in [Phase::Drain, Phase::Launcher] {
        intent.phase(&root, phase)?;
        assert!(startup(&root).is_err());
        assert!(selection(&root)? == intent.prior);
    }
    intent.phase(&root, Phase::Probe)?;
    let (main, _) = startup(&root)?.context("candidate startup missing")?;
    assert_eq!(
        main.parent().unwrap().file_name().unwrap(),
        intent.candidate.as_str()
    );
    assert!(selection(&root)? == intent.prior);
    intent.phase(&root, Phase::Rollback)?;
    let (main, _) = startup(&root)?.context("predecessor startup missing")?;
    assert_eq!(
        main.parent().unwrap().file_name().unwrap(),
        intent.prior.as_ref().unwrap().active.as_str()
    );
    assert!(selection(&root)? == intent.prior);
    Ok(())
}

#[tokio::test]
async fn committed_selection_cannot_be_aborted_or_undone_by_an_old_activation_record() -> Result<()>
{
    let (_owner, root, mut intent) = fixture()?;
    intent.phase(&root, Phase::Probe)?;
    replace_json(&root, "selected.json", &intent.selected())?;
    assert!(pending(&root)?.is_some());
    assert!(abort(&root).await.is_err());
    assert!(selection(&root)? == Some(intent.selected()));
    assert!(root.join(INTENT).is_file());
    // Any unrelated public selection invalidates the intent instead of being overwritten.
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: "a".repeat(64),
            previous: None,
        },
    )?;
    assert!(pending(&root).is_err());
    Ok(())
}

#[test]
fn foreign_malformed_and_oversized_activation_records_are_preserved() -> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    intent.install = root.join("foreign");
    intent.save(&root)?;
    let original = fs::read(root.join(INTENT))?;
    assert!(pending(&root).is_err());
    assert_eq!(fs::read(root.join(INTENT))?, original);
    fs::write(root.join(INTENT), b"Unknown activation metadata.")?;
    assert!(pending(&root).is_err());
    assert!(require_settled(&root).is_err());
    fs::write(root.join(INTENT), vec![b' '; MAX_INTENT as usize + 1])?;
    assert!(pending(&root).is_err());
    assert_eq!(fs::metadata(root.join(INTENT))?.len(), MAX_INTENT + 1);
    Ok(())
}

#[test]
fn historical_receipts_and_control_binding_cannot_be_silently_replaced() -> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    record(&root, &intent, Outcome::Selected)?;
    assert!(result(&root)?.is_some());
    fs::write(
        root.join("activation-result.json"),
        b"Unknown historical receipt.",
    )?;
    assert!(record(&root, &intent, Outcome::AbortedOrFailed).is_err());
    assert_eq!(
        fs::read(root.join("activation-result.json"))?,
        b"Unknown historical receipt."
    );
    intent.phase = Phase::Probe;
    intent.control = root.canonicalize()?.join("different-ledger");
    intent.save(&root)?;
    assert!(startup(&root).is_err());
    assert!(selection(&root)? == intent.prior);
    Ok(())
}

#[tokio::test]
async fn failed_initial_probe_rolls_back_without_a_public_selection_or_source_changes() -> Result<()>
{
    let (_owner, root, mut intent) = fixture()?;
    fs::remove_file(root.join("selected.json"))?;
    intent.prior = None;
    intent.save(&root)?;
    let main = root
        .join("runtimes")
        .join(&intent.candidate)
        .join(main_name());
    let original = fs::read(&main)?;
    // The owned inert fixture cannot execute. The failure must clear only its
    // activation intent, retain approved payloads and leave no selected runtime.
    assert!(resume(&root).await.is_err());
    assert!(pending(&root)?.is_none());
    assert!(selection(&root)?.is_none());
    assert_eq!(fs::read(main)?, original);
    assert!(matches!(
        result(&root)?.unwrap().outcome,
        Outcome::AbortedOrFailed
    ));
    Ok(())
}

#[tokio::test]
async fn abort_does_not_stop_an_unknown_live_owner_or_release_its_namespace() -> Result<()> {
    let (_owner, root, intent) = fixture()?;
    intent.save(&root)?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let _unknown = airk_store::service::ServiceLease::acquire(&control)?;
    assert!(abort(&root).await.is_err());
    assert!(airk_store::service::ServiceLease::acquire(&control).is_err());
    assert!(selection(&root)? == intent.prior);
    assert!(pending(&root)?.is_some_and(|intent| intent.phase == Phase::Rollback));
    Ok(())
}

#[test]
fn activation_phase_child_fixture() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_ACTIVATION_PHASE_FIXTURE") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let mut intent = pending(&root)?.context("fixture intent missing")?;
    intent.phase(&root, Phase::Probe)?;
    std::process::exit(82);
}

#[test]
fn process_death_after_durable_probe_phase_keeps_the_predecessor_public() -> Result<()> {
    let (_owner, root, intent) = fixture()?;
    intent.save(&root)?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "runtime_install::activation::tests::activation_phase_child_fixture",
        ])
        .env("AIRK_ACTIVATION_PHASE_FIXTURE", &root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x08000000);
    }
    assert_eq!(child.status()?.code(), Some(82));
    assert!(pending(&root)?.is_some_and(|intent| intent.phase == Phase::Probe));
    assert!(selection(&root)? == intent.prior);
    assert!(startup(&root)?.is_some());
    Ok(())
}
