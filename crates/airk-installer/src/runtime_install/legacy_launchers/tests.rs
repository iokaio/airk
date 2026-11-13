// SPDX-License-Identifier: Apache-2.0
use super::*;

fn old_launcher(root: &Path) -> Result<Vec<u8>> {
    let bytes = b"Original approved historical launcher".to_vec();
    fs::write(launcher_path(root), &bytes)?;
    replace_json(
        root,
        "launcher.json",
        &LauncherReceipt {
            schema: 1,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        },
    )?;
    Ok(bytes)
}

#[test]
fn clean_break_launcher_proof_handoff_survives_death_before_intent_removal() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    old_launcher(&root)?;
    let _lock = install_lock(&root)?;
    let captured = root.join("captured-retirement.json");
    let first = retire_locked_recording(&root, &identity, &mut |receipt| {
        fs::write(&captured, serde_json::to_vec(receipt)?)?;
        bail!("injected death after durable proof and before native intent removal")
    });
    assert!(first.is_err());
    assert!(pending(&root)?);
    assert!(!launcher_path(&root).exists());
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&captured)?)?;
    assert_eq!(
        original["processes"]["image_roster"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    retire_locked_recording(&root, &identity, &mut |receipt| {
        assert_eq!(
            receipt["processes"]["image_roster"],
            original["processes"]["image_roster"]
        );
        assert_eq!(fs::read(&captured)?, serde_json::to_vec(&original)?);
        Ok(())
    })?;
    assert!(!pending(&root)?);
    Ok(())
}

#[test]
fn retained_launcher_cleanup_is_durable_idempotent_and_preserves_main_and_data() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    let (main, _) = verify_runtime(&root, &identity)?;
    let main_before = fs::read(&main)?;
    let data = root.join("original-source.md");
    fs::write(
        &data,
        b"Original source and index state are not cleanup targets",
    )?;
    old_launcher(&root)?;
    let intent = prepare(&root, &identity)?;
    assert!(require_settled(&root).is_err());
    let saved = fs::read(root.join(INTENT))?;
    // Simulate death after the image unlink and before its receipt unlink.
    fs::remove_file(launcher_path(&root))?;
    let resumed = prepare(&root, &identity)?;
    assert_eq!(fs::read(root.join(INTENT))?, saved);
    finish_local(&root, &resumed)?;
    finish_local(&root, &intent)?;
    assert!(!launcher_path(&root).exists() && !root.join("launcher.json").exists());
    assert_eq!(fs::read(main)?, main_before);
    assert_eq!(
        fs::read(data)?,
        b"Original source and index state are not cleanup targets"
    );
    Ok(())
}

#[test]
fn changed_unknown_or_aliased_launcher_never_gets_deleted() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    fs::write(
        launcher_path(&root),
        b"unknown launcher with no ownership receipt",
    )?;
    assert!(prepare(&root, &identity).is_err());
    assert!(!root.join(INTENT).exists());
    let original = old_launcher(&root)?;
    let intent = prepare(&root, &identity)?;
    fs::write(launcher_path(&root), b"changed after accepted cleanup")?;
    assert!(finish_local(&root, &intent).is_err());
    assert_eq!(
        fs::read(launcher_path(&root))?,
        b"changed after accepted cleanup"
    );
    fs::write(launcher_path(&root), original)?;
    let alias = root.join("foreign-alias");
    fs::hard_link(launcher_path(&root), &alias)?;
    assert!(finish_local(&root, &intent).is_err());
    assert!(alias.exists() && root.join("launcher.json").exists());
    Ok(())
}

#[test]
fn crossed_transaction_or_receipt_is_preserved_even_after_image_deletion() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    old_launcher(&root)?;
    let intent = prepare(&root, &identity)?;
    assert!(prepare(&root, &"f".repeat(64)).is_err());
    fs::remove_file(launcher_path(&root))?;
    fs::write(root.join("launcher.json"), b"changed receipt")?;
    assert!(prepare(&root, &identity).is_err());
    assert!(finish_local(&root, &intent).is_err());
    assert_eq!(fs::read(root.join("launcher.json"))?, b"changed receipt");
    Ok(())
}

#[test]
fn schema_one_or_changed_selection_cannot_authorize_https_cleanup() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_historical_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    old_launcher(&root)?;
    let before = fs::read(launcher_path(&root))?;
    assert!(selected_https(&root, &identity).is_err());
    assert!(selected_https(&root, &"f".repeat(64)).is_err());
    assert_eq!(fs::read(launcher_path(&root))?, before);
    assert!(!root.join(INTENT).exists());
    Ok(())
}

#[test]
fn completed_uninstall_settles_only_its_exact_pending_launcher_transaction() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    old_launcher(&root)?;
    let intent = prepare(&root, &identity)?;
    assert!(finish_for_uninstall(&root, std::slice::from_ref(&identity)).is_err());
    finish_local(&root, &intent)?;
    assert!(finish_for_uninstall(&root, &["f".repeat(64)]).is_err());
    assert!(root.join(INTENT).exists());
    finish_for_uninstall(&root, std::slice::from_ref(&identity))?;
    require_settled(&root)?;
    finish_for_uninstall(&root, &[])?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn old_owned_launcher_process_is_stopped_while_foreign_image_is_preserved() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    let launcher = launcher_path(&root);
    fs::copy(std::env::current_exe()?, &launcher)?;
    replace_json(
        &root,
        "launcher.json",
        &LauncherReceipt {
            schema: 1,
            sha256: runtime_pair::digest_file(&launcher)?,
        },
    )?;
    let foreign = root.join("foreign.exe");
    fs::copy(&launcher, &foreign)?;
    let spawn = |path: &Path| -> Result<airk_host::native_process::ContainedChild> {
        let mut command = Command::new(path);
        command
            .args(["--exact", "service_manager::tests::manager_command_fixture"])
            .env("AIRK_MANAGER_COMMAND_FIXTURE", "blocked-input")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        airk_host::native_process::ContainedChild::spawn(&mut command)
    };
    let mut old = spawn(&launcher)?;
    let mut unrelated = spawn(&foreign)?;
    let mut intent = prepare(&root, &identity)?;
    let images = bind_images(
        &root,
        &mut intent,
        vec![Image {
            path: launcher.clone(),
            sha256: runtime_pair::digest_file(&launcher)?,
        }],
    )?;
    stop_legacy(std::slice::from_ref(&launcher))?;
    assert!(old.try_wait()?.is_some());
    assert!(unrelated.try_wait()?.is_none());
    let exclusion = image_exclusion(&images)?;
    record_exclusions(&root, &mut intent, &exclusion)?;
    let attempted = spawn(&launcher);
    assert!(
        attempted.is_err(),
        "held write exclusion allowed a new executable mapping"
    );
    finish_local(&root, &intent)?;
    let resumed = prepare(&root, &identity)?;
    assert_eq!(resumed.image_exclusions, images);
    observe(&images, &resumed.image_exclusions)?;
    assert!(!launcher.exists());
    unrelated.kill()?;
    unrelated.wait()?;
    Ok(())
}

#[test]
fn absent_historical_image_needs_exact_exclusion_or_complete_observation() -> Result<()> {
    let image = Image {
        path: PathBuf::from(if cfg!(windows) {
            r"C:\owned\old.exe"
        } else {
            "/owned/old"
        }),
        sha256: "a".repeat(64),
    };
    let mut audit = crate::legacy_processes::Audit {
        matching_executable_pids: vec![],
        scanned: 12,
        complete: false,
        unobserved: 1,
        failure: None,
    };
    assert!(qualify_observation(&image, &audit, &[]).is_err());
    let mut crossed = image.clone();
    crossed.sha256 = "b".repeat(64);
    assert!(qualify_observation(&image, &audit, &[crossed]).is_err());
    let mut crossed = image.clone();
    crossed.path = crossed.path.with_file_name("different-image");
    assert!(qualify_observation(&image, &audit, &[crossed]).is_err());
    assert_eq!(
        qualify_observation(&image, &audit, std::slice::from_ref(&image)).is_ok(),
        cfg!(windows)
    );
    audit.complete = true;
    audit.unobserved = 0;
    qualify_observation(&image, &audit, &[])?;
    audit.matching_executable_pids.push(42);
    assert!(qualify_observation(&image, &audit, std::slice::from_ref(&image)).is_err());
    Ok(())
}

#[test]
fn durable_roster_cannot_be_extended_or_crossed_after_partial_deletion() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    old_launcher(&root)?;
    let image = Image {
        path: launcher_path(&root),
        sha256: runtime_pair::digest_file(&launcher_path(&root))?,
    };
    let mut intent = prepare(&root, &identity)?;
    bind_images(&root, &mut intent, vec![image.clone()])?;
    let saved = fs::read(root.join(INTENT))?;
    let mut crossed = image.clone();
    crossed.sha256 = "f".repeat(64);
    assert!(bind_images(&root, &mut intent, vec![crossed.clone()]).is_err());
    assert_eq!(fs::read(root.join(INTENT))?, saved);
    intent.image_exclusions.push(crossed);
    assert!(validate_roster(&intent).is_err());
    intent.image_exclusions.clear();
    finish_local(&root, &intent)?;
    let mut resumed = prepare(&root, &identity)?;
    assert_eq!(bind_images(&root, &mut resumed, vec![])?, vec![image]);
    assert!(
        resumed.image_exclusions.is_empty(),
        "deletion alone manufactured a held-image witness"
    );
    Ok(())
}

#[test]
fn selected_modern_runtime_never_publishes_retained_launcher_again() -> Result<()> {
    let (owner, root, identity) = super::super::tests::unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity,
            previous: None,
        },
    )?;
    let image = launcher_path(&root);
    fs::write(&image, b"retained historical image")?;
    replace_json(
        &root,
        "launcher.json",
        &LauncherReceipt {
            schema: 1,
            sha256: runtime_pair::digest_file(&image)?,
        },
    )?;
    let directory = owner.path().join("universal-must-not-be-created");
    assert!(crate::universal_launcher::publish_at(&directory, &root).is_err());
    assert!(
        crate::universal_launcher::publish_for_default(
            &root,
            airk_host::user_paths::Channel::Stable
        )?
        .is_none()
    );
    assert!(!directory.exists());
    assert_eq!(fs::read(image)?, b"retained historical image");
    Ok(())
}

#[test]
fn absent_foreign_process_path_cannot_be_added_to_saved_launcher_authority() -> Result<()> {
    let (owner, root, identity) = super::super::tests::unselected_fixture()?;
    old_launcher(&root)?;
    let mut intent = prepare(&root, &identity)?;
    let foreign = Image {
        path: owner.path().join("absent-foreign.exe"),
        sha256: "f".repeat(64),
    };
    intent.images = Some(vec![foreign.clone()]);
    intent.image_exclusions = vec![foreign];
    assert!(validate_roster(&intent).is_err());
    replace_json(&root, INTENT, &intent)?;
    assert!(prepare(&root, &identity).is_err());
    Ok(())
}
