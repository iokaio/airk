// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn fixture(root: &Path) -> Result<String> {
    local_ipc::prepare_root(&root.join("notices"))?;
    fs::create_dir_all(
        root.join(main_name())
            .parent()
            .context("fixture executable parent")?,
    )?;
    fs::write(root.join(main_name()), b"Original inert main fixture.")?;
    fs::write(root.join(helper_name()), b"Original inert helper fixture.")?;
    fs::write(root.join("LICENSE"), b"Original fictional license fixture.")?;
    fs::write(root.join("notices/inventory.json"), b"[]")?;
    let pair = runtime_pair::RuntimePair {
        schema: 1,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: runtime_pair::digest_file(&root.join(main_name()))?,
        embed_sha256: runtime_pair::digest_file(&root.join(helper_name()))?,
    };
    fs::write(root.join("airk-runtime.json"), serde_json::to_vec(&pair)?)?;
    let files = [
        main_name(),
        helper_name(),
        "LICENSE",
        "notices/inventory.json",
        "airk-runtime.json",
    ]
    .into_iter()
    .map(|name| Ok((name.into(), runtime_pair::digest_file(&root.join(name))?)))
    .collect::<Result<BTreeMap<_, _>>>()?;
    let manifest = InstallManifest {
        schema: 1,
        version: pair.version,
        os: pair.os,
        arch: pair.arch,
        files,
    };
    write_manifest(root, &manifest)
}

fn write_manifest(root: &Path, manifest: &InstallManifest) -> Result<String> {
    let bytes = serde_json::to_vec(manifest)?;
    fs::write(root.join("airk-install.json"), &bytes)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[test]
fn startup_circuit_retains_failed_attempts_across_reopen_and_backwards_clock() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    let identity = "a".repeat(64);
    startup_attempt(&root, &identity, 10_000)?;
    assert!(startup_attempt(&root, &identity, 10_001).is_err());
    startup_attempt(&root, &identity, 12_000)?;
    startup_attempt(&root, &identity, 16_000)?;
    assert!(startup_attempt(&root, &identity, 24_000).is_err());
    assert!(startup_attempt(&root, &identity, 0).is_err());
    startup_attempt(&root, &identity, 140_000)?;
    // A different approved runtime has its own bounded rollback/startup budget.
    startup_attempt(&root, &"b".repeat(64), 140_000)?;
    Ok(())
}

pub(super) fn unselected_fixture() -> Result<(tempfile::TempDir, PathBuf, String)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let temporary = root.join("fixture");
    local_ipc::prepare_root(&temporary)?;
    let identity = fixture(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temporary.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    replace_json(
        &temporary,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(temporary, root.join("runtimes").join(&identity))?;
    Ok((owner, root, identity))
}

#[tokio::test]
async fn unlisted_installed_payload_files_block_execution_but_preserve_resumable_uninstall()
-> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    verify_runtime(&root, &identity)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    let directory = root.join("runtimes").join(&identity);
    let unknown = directory.join("unlisted-library.dll");
    fs::write(&unknown, b"Preserve unknown neighboring runtime content.")?;
    assert!(verify_runtime(&root, &identity).is_err());
    assert!(selected(&root).is_err());
    let result = retirement::uninstall(&root).await?;
    assert_eq!(result["uninstalled"], false);
    assert_eq!(result["deferred_runtimes"][0], identity);
    assert!(directory.join(main_name()).is_file());
    assert_eq!(
        fs::read(&unknown)?,
        b"Preserve unknown neighboring runtime content."
    );
    assert!(root.join("uninstall.json").is_file());
    fs::remove_file(unknown)?;
    assert_eq!(retirement::uninstall(&root).await?["uninstalled"], true);
    assert!(!directory.exists());
    Ok(())
}

#[test]
fn retirement_preflights_receipts_and_resumes_after_the_payload_directory_is_gone() -> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    let directory = root.join("runtimes").join(&identity);
    let approval = fs::read(directory.join("approval.json"))?;
    fs::write(
        directory.join("approval.json"),
        b"Changed approval must be preserved.",
    )?;
    assert!(retirement::remove(&root, &identity).is_err());
    assert!(directory.join(main_name()).is_file());
    fs::write(directory.join("approval.json"), approval)?;
    let startup = root.join(format!("startup-{identity}.json"));
    fs::write(&startup, b"Unknown startup receipt.")?;
    assert!(retirement::remove(&root, &identity).is_err());
    assert!(directory.join(main_name()).is_file());
    fs::remove_file(&startup)?;
    let manifest = fs::read(directory.join("airk-install.json"))?;
    retirement::remove(&root, &identity)?;
    // Recreate the exact durable boundary after directory removal but before
    // startup/intent removal. No payload file is needed to complete this state.
    let intent = root.join(format!("retire-{identity}.json"));
    fs::write(&intent, manifest)?;
    replace_json(
        &root,
        &format!("startup-{identity}.json"),
        &StartupAttempts {
            attempts_unix_ms: vec![1, 2, 3, 4],
        },
    )?;
    assert!(retirement::remove(&root, &identity).is_err());
    assert!(intent.exists() && startup.exists());
    replace_json(
        &root,
        &format!("startup-{identity}.json"),
        &StartupAttempts {
            attempts_unix_ms: vec![1, 2, 3],
        },
    )?;
    retirement::remove(&root, &identity)?;
    assert!(!intent.exists() && !startup.exists() && !directory.exists());
    Ok(())
}

#[tokio::test]
async fn uninstall_before_first_activation_preserves_live_owners_then_removes_only_owned_payloads()
-> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let service = airk_store::service::ServiceLease::acquire(&control)?;
    assert!(retirement::uninstall(&root).await.is_err());
    assert!(
        root.join("runtimes")
            .join(&identity)
            .join(main_name())
            .is_file()
    );
    assert!(!root.join("uninstall.json").exists());
    drop(service);
    let retained = control.join("retained-fixture.txt");
    fs::write(&retained, b"Preserve service state.")?;
    let result = retirement::uninstall(&root).await?;
    assert_eq!(result["uninstalled"], true);
    assert_eq!(fs::read(retained)?, b"Preserve service state.");
    assert!(!root.join("runtimes").join(identity).exists());
    assert_eq!(retirement::uninstall(&root).await?["uninstalled"], true);
    Ok(())
}

#[tokio::test]
async fn uninstall_empty_initialized_layout_without_a_runtime_directory_is_idempotent() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    for _ in 0..2 {
        assert_eq!(retirement::uninstall(&root).await?["uninstalled"], true);
        assert!(!root.join("runtimes").exists());
        assert!(layout::control(&root)?.is_dir());
    }
    Ok(())
}

#[test]
fn native_self_test_binds_actual_version_capabilities_and_bounded_completion() -> Result<()> {
    let result = serde_json::json!({"protocol":1,"version":"1.4.0","self_test":"passed","state_layout":1,"foreground_dispatch":1,"managed_reference":1,"activation_resume":1});
    validate_self_test_result(&serde_json::to_vec(&result)?, "1.4.0")?;
    assert!(validate_self_test_result(&serde_json::to_vec(&result)?, "9.9.9").is_err());
    let mut changed = result.clone();
    changed.as_object_mut().unwrap().remove("activation_resume");
    assert!(validate_self_test_result(&serde_json::to_vec(&changed)?, "1.4.0").is_err());
    changed = result.clone();
    changed.as_object_mut().unwrap().remove("state_layout");
    assert!(validate_self_test_result(&serde_json::to_vec(&changed)?, "1.4.0").is_err());
    changed = result;
    changed["self_test"] = "incomplete".into();
    assert!(validate_self_test_result(&serde_json::to_vec(&changed)?, "1.4.0").is_err());
    assert!(validate_self_test_result(&vec![b' '; 4097], "1.4.0").is_err());
    Ok(())
}

#[test]
fn runtime_removal_preserves_unknown_or_changed_files_and_resumes_partial_owned_deletion()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let staging = root.join("fixture");
    local_ipc::prepare_root(&staging)?;
    let identity = fixture(&staging)?;
    // Staged installer directories are private, including their nested notice dirs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staging.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    let approval = Approval {
        schema: 1,
        manifest_sha256: identity.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        verification_basis: VerificationBasis::OrganizationDigest,
    };
    fs::write(
        staging.join("approval.json"),
        serde_json::to_vec(&approval)?,
    )?;
    let directory = root.join("runtimes").join(&identity);
    fs::rename(&staging, &directory)?;
    fs::write(
        directory.join("user-note.md"),
        b"Preserve this unrecognized file.",
    )?;
    assert!(retirement::remove(&root, &identity).is_err());
    assert!(directory.join(main_name()).is_file());
    assert_eq!(
        fs::read(directory.join("user-note.md"))?,
        b"Preserve this unrecognized file."
    );
    fs::remove_file(directory.join("user-note.md"))?;
    fs::write(directory.join("LICENSE"), b"User-modified fixture.")?;
    assert!(retirement::remove(&root, &identity).is_err());
    assert!(
        directory.join(main_name()).is_file(),
        "changed inventory was discovered after deletion started"
    );
    fs::write(
        directory.join("LICENSE"),
        b"Original fictional license fixture.",
    )?;
    // The durable intent admits retry after interruption, even with the main already removed.
    fs::remove_file(directory.join(main_name()))?;
    retirement::remove(&root, &identity)?;
    assert!(!directory.exists());
    assert!(!root.join(format!("retire-{identity}.json")).exists());
    Ok(())
}

#[test]
fn approved_offline_inventory_rejects_wrong_identity_tampering_aliases_and_escaping_paths()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let approved = fixture(&root)?;
    assert!(manifest(&root, &approved).is_ok());
    for alias in [
        "notices//a",
        "notices/./a",
        "file:stream",
        "NUL.txt",
        "trailing.",
        "space ",
    ] {
        assert!(
            payload_path(&root, alias).is_err(),
            "accepted aliased payload name: {alias}"
        );
    }
    assert!(manifest(&root, &"0".repeat(64)).is_err());
    fs::write(root.join(helper_name()), b"Changed fixture.")?;
    assert!(manifest(&root, &approved).is_err());
    let approved = fixture(&root)?;
    fs::hard_link(root.join("LICENSE"), root.join("alias"))?;
    assert!(manifest(&root, &approved).is_err());
    fs::remove_file(root.join("alias"))?;
    let (_, mut inventory) = manifest(&root, &approved)?;
    inventory.files.insert("../outside".into(), "0".repeat(64));
    let changed = write_manifest(&root, &inventory)?;
    assert!(manifest(&root, &changed).is_err());
    inventory.files.remove("../outside");
    inventory.arch = "foreign".into();
    let changed = write_manifest(&root, &inventory)?;
    assert!(manifest(&root, &changed).is_err());
    Ok(())
}

#[test]
fn failed_native_self_test_preserves_selection_and_never_publishes_staging() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    fs::create_dir(&source)?;
    let approved = fixture(&source)?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    fs::write(root.join("selected.json"), b"Prior selection fixture.")?;
    // Inert bytes cannot pass the native executable self-test on any host.
    assert!(
        stage(
            &source,
            &root,
            &approved,
            VerificationBasis::OrganizationDigest
        )
        .is_err()
    );
    assert_eq!(
        fs::read(root.join("selected.json"))?,
        b"Prior selection fixture."
    );
    assert_eq!(fs::read_dir(root.join("runtimes"))?.count(), 0);
    assert!(!launcher_path(&root).exists());
    assert!(!root.join("control").exists());
    assert!(verify_runtime(&root, &approved).is_err());
    assert!(
        !root.join("runtimes").join(&approved).exists(),
        "inspection created an absent runtime"
    );
    Ok(())
}

#[test]
fn shutdown_identity_does_not_require_launchable_payload_but_rejects_changed_receipts() -> Result<()>
{
    let (_owner, root, identity) = unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    let directory = root.join("runtimes").join(&identity);
    let expected = runtime_pair::digest_file(&directory.join(main_name()))?;
    fs::remove_file(directory.join("LICENSE"))?;
    fs::remove_file(directory.join(main_name()))?;
    assert!(selected(&root).is_err());
    let (approval, hash) = stop_identity(&root)?;
    assert_eq!(hash, expected);
    assert_eq!(approval.manifest_sha256, identity);
    let path = directory.join("airk-install.json");
    let original = fs::read(&path)?;
    let mut changed = original.clone();
    changed.push(b' ');
    fs::write(&path, changed)?;
    assert!(stop_identity(&root).is_err());
    fs::write(&path, &original)?;
    replace_json(
        &directory,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity,
            version: "different-version".into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    assert!(stop_identity(&root).is_err());
    assert!(!directory.join(main_name()).exists());
    assert!(!directory.join("LICENSE").exists());
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

#[tokio::test]
async fn damaged_payload_cannot_authorize_stopping_an_unowned_login_job() -> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    let directory = root.join("runtimes").join(&identity);
    fs::remove_file(directory.join("LICENSE"))?;
    let manager = crate::service_manager::ServiceManager::for_runtime(&root)?;
    assert!(stop_login(&root, &manager, false).await.is_err());
    assert!(directory.join(main_name()).is_file());
    assert!(!root.join("startup-owner.json").exists());
    assert!(!directory.join("LICENSE").exists());
    Ok(())
}
