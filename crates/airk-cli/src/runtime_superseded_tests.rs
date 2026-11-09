// SPDX-License-Identifier: Apache-2.0
//! Native approved-file transaction fixtures; these inert payloads never run.
use super::*;

#[test]
fn clean_break_runtime_proof_handoff_keeps_original_roster_after_payload_deletion() -> Result<()> {
    let (_owner, root, active) = super::super::super::tests::unselected_fixture()?;
    let old = add(&root, "previous", true)?;
    // Real historical startup materializes these owned role images. A missing
    // image cannot acquire a Windows held-image exclusion from this fixture.
    let (old_main, _) = verify_runtime(&root, &old)?;
    crate::process_roles::install(&old_main)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: active.clone(),
            previous: Some(old.clone()),
        },
    )?;
    let _lock = install_lock(&root)?;
    let captured = root.join("captured-runtime-retirement.json");
    let first = retire_recording(&root, &active, &mut |receipt| {
        fs::write(&captured, serde_json::to_vec(receipt)?)?;
        bail!("injected death after durable proof and before native intent removal")
    });
    let error = first.expect_err("injected proof handoff must interrupt retirement");
    assert!(
        format!("{error:#}").contains("injected death after durable proof"),
        "retirement failed before the injected handoff: {error:#}"
    );
    assert!(pending(&root)?);
    assert!(!root.join("runtimes").join(&old).exists());
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&captured)?)?;
    assert_eq!(original["retired_runtimes"], serde_json::json!([old]));
    assert!(
        !original["processes"]["image_roster"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    retire_recording(&root, &active, &mut |receipt| {
        assert_eq!(receipt["retired_runtimes"], original["retired_runtimes"]);
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

fn add(root: &Path, label: &str, historical: bool) -> Result<String> {
    let source = root.join(format!("fixture-{label}"));
    local_ipc::prepare_root(&source)?;
    if historical {
        super::super::super::tests::historical_pair_fixture(&source)?;
    } else {
        super::super::super::tests::fixture(&source)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(source.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    fs::write(source.join("LICENSE"), label.as_bytes())?;
    let mut manifest: InstallManifest =
        serde_json::from_slice(&fs::read(source.join("airk-install.json"))?)?;
    manifest.files.insert(
        "LICENSE".into(),
        runtime_pair::digest_file(&source.join("LICENSE"))?,
    );
    let bytes = serde_json::to_vec(&manifest)?;
    let identity = format!("{:x}", Sha256::digest(&bytes));
    fs::write(source.join("airk-install.json"), bytes)?;
    replace_json(
        &source,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: manifest.version,
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(source, root.join("runtimes").join(&identity))?;
    Ok(identity)
}

#[test]
fn later_upgrade_retires_rebuilt_payload_and_explicitly_preserves_original_recovery_inventories()
-> Result<()> {
    let (_owner, root, preserved, rebuilt, later) =
        crate::runtime_install::activation::rebuild::tests::historical_retirement_fixture()?;
    activation::rebuild::require_retirement_allowed(&root)?;
    let before = preserved
        .iter()
        .map(|identity| -> Result<_> {
            Ok((
                identity.clone(),
                fs::read(
                    root.join("runtimes")
                        .join(identity)
                        .join("airk-install.json"),
                )?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let images = inspect_images(&root, &later)?;
    assert!(
        images.iter().any(|image| image["runtime_sha256"] == rebuilt
            && image["preserved_rebuild_evidence"] == false)
    );
    for identity in &preserved {
        assert!(
            images
                .iter()
                .any(|image| image["runtime_sha256"] == *identity
                    && image["preserved_rebuild_evidence"] == true)
        );
    }
    let intent = prepare(&root, &later)?;
    assert_eq!(intent.runtimes, vec![rebuilt.clone()]);
    let mut crossed = intent.clone();
    crossed.runtimes.push(preserved[0].clone());
    assert!(validate(&root, &crossed, &later).is_err());
    finish_payloads(&root, &intent)?;
    assert!(!root.join("runtimes").join(rebuilt).exists());
    assert!(identities(&root, &later)?.is_empty());
    assert_eq!(
        activation::rebuild::preserved_runtime_ids(&root)?,
        preserved
    );
    for (identity, bytes) in before {
        verify_runtime(&root, &identity)?;
        assert_eq!(
            fs::read(
                root.join("runtimes")
                    .join(&identity)
                    .join("airk-install.json")
            )?,
            bytes
        );
        assert!(remove(&root, &identity).is_err());
        assert!(!root.join(format!("retire-{identity}.json")).exists());
    }
    let retained = inspect_images(&root, &later)?;
    assert!(!retained.is_empty());
    assert!(
        retained
            .iter()
            .all(|image| image["preserved_rebuild_evidence"] == true)
    );
    Ok(())
}

#[test]
fn legacy_to_modern_then_modern_to_modern_removes_all_generations_and_preserves_stopped_state()
-> Result<()> {
    let (_owner, root, legacy) = super::super::super::tests::unselected_historical_fixture()?;
    let current = add(&root, "modern-first", false)?;
    let orphan = add(&root, "older-approved-orphan", false)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: current.clone(),
            previous: Some(legacy.clone()),
        },
    )?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?);
    let database = control.join("service-1.4.sqlite3");
    let db = rusqlite::Connection::open(&database)?;
    db.execute(
        "UPDATE service_lifecycle_state SET stopped=1 WHERE id=1",
        [],
    )?;
    drop(db);
    let before = fs::read(&database)?;
    let source = root.join("source.md");
    fs::write(&source, b"Preserve original source bytes")?;
    let intent = prepare(&root, &current)?;
    assert_eq!(intent.runtimes.len(), 2);
    assert!(intent.runtimes.contains(&legacy) && intent.runtimes.contains(&orphan));
    finish_payloads(&root, &intent)?;
    assert_eq!(identities(&root, &current)?, Vec::<String>::new());
    fs::remove_file(root.join(JOURNAL))?;
    let future = add(&root, "modern-second", false)?;
    let other = add(&root, "unreferenced-second-generation", false)?;
    // Lost historical `previous` pointers must not hide approved orphan payloads.
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: future.clone(),
            previous: None,
        },
    )?;
    let next = prepare(&root, &future)?;
    assert!(next.runtimes.contains(&current) && next.runtimes.contains(&other));
    finish_payloads(&root, &next)?;
    verify_runtime(&root, &future)?;
    assert_eq!(identities(&root, &future)?, Vec::<String>::new());
    assert_eq!(fs::read(source)?, b"Preserve original source bytes");
    assert_eq!(fs::read(database)?, before);
    Ok(())
}

#[test]
fn interrupted_all_generation_cleanup_keeps_exact_roster_and_refuses_new_or_changed_payloads()
-> Result<()> {
    let (_owner, root, old) = super::super::super::tests::unselected_fixture()?;
    let active = add(&root, "active", false)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: active.clone(),
            previous: Some(old.clone()),
        },
    )?;
    let intent = prepare(&root, &active)?;
    let saved = fs::read(root.join(JOURNAL))?;
    let unknown = root.join("runtimes").join(&old).join("unknown-user-file");
    fs::write(&unknown, b"Keep this file")?;
    assert!(preflight_remove(&root, &old).is_err());
    assert!(root.join("runtimes").join(&old).join(main_name()).exists());
    assert_eq!(fs::read(&unknown)?, b"Keep this file");
    fs::remove_file(unknown)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: active.clone(),
            previous: None,
        },
    )?;
    fs::remove_file(root.join("runtimes").join(&old).join(main_name()))?;
    let resumed = prepare(&root, &active)?;
    assert_eq!(fs::read(root.join(JOURNAL))?, saved);
    assert_eq!(resumed.runtimes, intent.runtimes);
    let extra = add(&root, "not-in-accepted-roster", false)?;
    assert!(prepare(&root, &active).is_err());
    remove(&root, &extra)?;
    finish_payloads(&root, &resumed)?;
    assert!(!root.join("runtimes").join(old).exists());
    Ok(())
}

#[test]
fn readonly_lineage_verifies_each_superseded_image_and_excludes_current() -> Result<()> {
    let (_owner, root, old) = super::super::super::tests::unselected_fixture()?;
    let active = add(&root, "active", false)?;
    let images = inspect_images(&root, &active)?;
    assert!(images.iter().all(|image| image["runtime_sha256"] == old));
    assert!(images.iter().any(|image| {
        image["path"]
            == root
                .join("runtimes")
                .join(&old)
                .join(main_name())
                .to_string_lossy()
                .as_ref()
    }));
    assert!(!root.join(format!("retire-{old}.json")).exists());
    fs::write(
        root.join("runtimes").join(old).join(main_name()),
        b"changed",
    )?;
    assert!(inspect_images(&root, &active).is_err());
    Ok(())
}

#[test]
fn saved_obsolete_process_authority_rejects_absent_foreign_active_and_crossed_manifest_paths()
-> Result<()> {
    let (owner, root, _old) = super::super::super::tests::unselected_fixture()?;
    let active = add(&root, "active", false)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: active.clone(),
            previous: None,
        },
    )?;
    let mut intent = prepare(&root, &active)?;
    let original = intent.images[0].clone();
    for path in [
        owner.path().join("absent-foreign.exe"),
        root.join("runtimes").join(&active).join(main_name()),
    ] {
        intent.images[0].path = path;
        assert!(validate(&root, &intent, &active).is_err());
    }
    intent.images[0] = original;
    intent.images[0].sha256 = "f".repeat(64);
    assert!(validate(&root, &intent, &active).is_err());
    Ok(())
}

#[test]
fn saved_cleanup_refuses_omitted_manifests_images_or_runtime_subset_before_effects() -> Result<()> {
    let (_owner, root, old) = super::super::super::tests::unselected_fixture()?;
    let active = add(&root, "active", false)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: active.clone(),
            previous: None,
        },
    )?;
    let original = prepare(&root, &active)?;
    let main = root.join("runtimes").join(&old).join(main_name());
    let before = fs::read(&main)?;
    let mut missing = original.clone();
    missing.manifests.clear();
    assert!(validate(&root, &missing, &active).is_err());
    let mut missing = original.clone();
    missing.images.clear();
    missing.image_exclusions.clear();
    assert!(validate(&root, &missing, &active).is_err());
    let mut missing = original;
    missing.runtimes.clear();
    missing.manifests.clear();
    missing.images.clear();
    missing.image_exclusions.clear();
    replace_json(&root, JOURNAL, &missing)?;
    assert!(prepare(&root, &active).is_err());
    assert_eq!(fs::read(main)?, before);
    Ok(())
}
