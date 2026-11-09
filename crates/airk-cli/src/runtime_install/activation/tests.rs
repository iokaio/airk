// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, PathBuf, Intent)> {
    fixture_for(false)
}

fn fixture_for(historical: bool) -> Result<(tempfile::TempDir, PathBuf, Intent)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?.join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let mut identities = Vec::new();
    for number in 0..2 {
        let stage = root.join(format!("fixture-{number}"));
        local_ipc::prepare_root(&stage)?;
        if historical {
            super::super::tests::historical_pair_fixture(&stage)?;
        } else {
            super::super::tests::fixture(&stage)?;
        }
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
                version: inventory.version.clone(),
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
        checkpoint: None,
    };
    Ok((owner, root, intent))
}

#[tokio::test]
async fn canonical_launcher_phase_preserves_modern_predecessor_bytes_and_repairs_only_historical_payloads()
-> Result<()> {
    for historical in [false, true] {
        for retained in [false, true] {
            let (_owner, root, mut intent) = fixture_for(historical)?;
            // Enter the real durable handoff phase without executing a fixture
            // predecessor. The inert candidate must fail later qualification.
            fs::remove_file(root.join("selected.json"))?;
            intent.prior = None;
            intent.phase(&root, Phase::Launcher)?;
            let launcher = launcher_path(&root);
            let original = b"Genuine retained predecessor launcher bytes";
            let receipt = root.join("launcher.json");
            if retained {
                fs::write(&launcher, original)?;
                replace_json(
                    &root,
                    "launcher.json",
                    &LauncherReceipt {
                        schema: 1,
                        sha256: format!("{:x}", Sha256::digest(original)),
                    },
                )?;
            }
            let original_receipt = retained.then(|| fs::read(&receipt)).transpose()?;
            let (main, _, inventory) = verify_runtime_inventory(&root, &intent.candidate)?;
            let main_before = fs::read(&main)?;
            assert!(resume(&root).await.is_err());
            let saved = pending(&root)?.context("inert candidate lost recoverable activation")?;
            assert!(
                saved.phase == Phase::Probe,
                "launcher handoff did not complete"
            );
            assert!(selection(&root)?.is_none());
            assert_eq!(fs::read(&main)?, main_before);
            if historical {
                let expected = launcher_image(runtime_pair::payload_root(&main)?, &inventory)?;
                assert_eq!(fs::read(&launcher)?, fs::read(expected)?);
                verified_launcher(&root)?;
            } else if retained {
                assert_eq!(fs::read(&launcher)?, original);
                assert_eq!(fs::read(&receipt)?, original_receipt.unwrap());
            } else {
                assert!(!launcher.try_exists()? && !receipt.try_exists()?);
            }
            assert!(!root.join("launcher-repair.json").try_exists()?);
        }
    }
    Ok(())
}

#[test]
fn single_main_role_dispatch_preserves_inventory_and_refuses_unowned_aliases() -> Result<()> {
    let (_owner, root, intent) = fixture()?;
    let (main, _) = verify_runtime(&root, &intent.candidate)?;
    let directory = runtime_pair::payload_root(&main)?;
    let original = fs::read(directory.join("airk-install.json"))?;
    crate::process_roles::install(&main)?;
    assert_eq!(fs::read(directory.join("airk-install.json"))?, original);
    verify_runtime(&root, &intent.candidate)?;
    let selected = crate::process_roles::executable(&main, crate::process_roles::Role::Service)?;
    assert_eq!(selected, main);
    let alias = directory.join(crate::process_roles::Role::Service.name());
    assert!(!alias.exists());
    fs::write(&alias, b"modified alias must be preserved")?;
    assert!(verify_runtime(&root, &intent.candidate).is_err());
    assert!(retirement::remove(&root, &intent.candidate).is_err());
    assert!(main.exists());
    fs::remove_file(&alias)?;
    crate::process_roles::install(&main)?;
    retirement::remove(&root, &intent.candidate)?;
    assert!(!directory.exists());
    verify_runtime(&root, &intent.prior.unwrap().active)?;
    Ok(())
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

#[test]
fn superseding_a_saved_upgrade_requires_exact_preprobe_identity() -> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    let active = intent.prior.as_ref().unwrap().active.clone();
    intent.save(&root)?;
    require_superseded(&root, &intent.candidate, &active)?;
    assert!(require_superseded(&root, &"f".repeat(64), &active).is_err());
    assert!(require_superseded(&root, &intent.candidate, &"f".repeat(64)).is_err());
    intent.phase(&root, Phase::Probe)?;
    assert!(require_superseded(&root, &intent.candidate, &active).is_err());
    assert!(pending(&root)?.is_some());
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
    assert!(!layout::control(&root)?.join("service-1.4.sqlite3").exists());
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
        runtime_pair::payload_root(&main)?.file_name().unwrap(),
        intent.candidate.as_str()
    );
    assert!(selection(&root)? == intent.prior);
    intent.phase(&root, Phase::Rollback)?;
    let (main, _) = startup(&root)?.context("predecessor startup missing")?;
    assert_eq!(
        runtime_pair::payload_root(&main)?.file_name().unwrap(),
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
async fn failed_initial_probe_preserves_recovery_without_a_public_selection_or_source_changes()
-> Result<()> {
    let (_owner, root, mut intent) = fixture()?;
    fs::remove_file(root.join("selected.json"))?;
    intent.prior = None;
    intent.save(&root)?;
    let main = root
        .join("runtimes")
        .join(&intent.candidate)
        .join(main_name());
    let original = fs::read(&main)?;
    // The inert approved payload cannot qualify. Authority-1 Genesis must retain
    // its exact recoverable operation rather than pretend an admitted namespace
    // was never touched. Neither route may publish selection or change payloads.
    assert!(resume(&root).await.is_err());
    assert!(selection(&root)?.is_none());
    assert_eq!(fs::read(main)?, original);
    let retained = pending(&root)?.context("failed fresh qualification lost its operation")?;
    assert_eq!(retained.candidate, intent.candidate);
    assert_eq!(retained.control, intent.control);
    assert!(retained.prior.is_none());
    assert!(retained.phase == Phase::Probe);
    assert!(result(&root)?.is_none());
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

fn durable_files(root: &Path) -> Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
    fn collect(
        path: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(&entry.path(), files)?;
            } else {
                // Ownership locks are fixed empty files; never read a lock range.
                let mut bytes = if entry.metadata()?.len() == 0 {
                    Vec::new()
                } else {
                    fs::read(entry.path())?
                };
                // The pre-existing drain/quiescence path commits a page-neutral
                // registry transaction. Normalize only SQLite's header file
                // change counter (offset 24) and version-valid-for (offset 92);
                // every page byte, including the rest of the header, must match.
                if entry.path().extension() == Some("sqlite3".as_ref()) && bytes.len() >= 100 {
                    bytes[24..28].fill(0);
                    bytes[92..96].fill(0);
                }
                files.insert(entry.path(), bytes);
            }
        }
        Ok(())
    }
    let mut files = std::collections::BTreeMap::new();
    collect(root, &mut files)?;
    Ok(files)
}

/// A committed ordinary update drains and requires the existing catalog. A
/// registry without one is refused instead of being upgraded in place, and
/// neither outcome changes installation, selection or control state.
#[tokio::test]
async fn ordinary_update_refuses_a_registry_without_a_catalog_and_keeps_existing_catalogs()
-> Result<()> {
    use airk_core::identity::Incarnation;
    for catalog in [false, true] {
        let (_owner, root, mut intent) = fixture()?;
        let control = layout::control(&root)?;
        local_ipc::prepare_root(&control)?;
        let namespace = Incarnation::from_bytes([187; 16]);
        {
            let lease = airk_store::service::ServiceLease::acquire(&control)?;
            layout::record_control(&lease)?;
            drop(airk_store::service::registry::ServiceRegistry::open(
                &control, namespace,
            )?);
            layout::record_registry(&lease, namespace)?;
        }
        if catalog {
            drop(crate::catalog_test_fixture::authority_one(
                &control, namespace,
            )?);
        }
        intent.phase = Phase::Probe;
        replace_json(&root, "selected.json", &intent.selected())?;
        intent.save(&root)?;
        // Create the empty ownership lock files first; compare durable bytes only.
        let observed = quiescent(&root)?;
        assert_eq!(observed.catalog.is_some(), catalog);
        drop(observed);
        let before = durable_files(&root)?;
        let error = catalog_upgrade::activate(&root, &intent)
            .await
            .err()
            .context("inert fixture candidate cannot complete an ordinary update")?;
        let unsupported = format!("{error:#}").contains("catalog_upgrade_unsupported");
        assert_eq!(unsupported, !catalog, "{error:#}");
        assert_eq!(durable_files(&root)?, before);
        // Only the catalog fixture's own Genesis settlement left a journal.
        assert_eq!(control.join("index-catalog-genesis.json").exists(), catalog);
        assert!(!control.join("index-catalog-upgrade.json").exists());
        assert!(selection(&root)? == Some(intent.selected()));
        assert!(pending(&root)?.is_some_and(|saved| saved.phase == Phase::Probe));
    }
    Ok(())
}
