// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, PathBuf, String)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("installation");
    local_ipc::prepare_root(&root)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let directory = root.join("fixture");
    local_ipc::prepare_root(&directory)?;
    let identity = super::super::tests::fixture(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    replace_json(
        &directory,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(directory, root.join("runtimes").join(&identity))?;
    Ok((owner, root, identity))
}

fn old_launcher(root: &Path) -> Result<Vec<u8>> {
    let bytes = b"Original known preceding launcher.".to_vec();
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
fn missing_launcher_and_owned_partial_repair_from_approved_payload() -> Result<()> {
    let (_owner, root, identity) = fixture()?;
    old_launcher(&root)?;
    fs::remove_file(launcher_path(&root))?;
    let intent = begin(&root, &identity)?;
    fs::write(intent.partial(&root), b"unfinished")?;
    assert_eq!(pending(&root)?, Some(identity.clone()));
    assert!(require_settled(&root).is_err());
    let unknown = root.join(".airk-launcher-user-note.part");
    fs::write(&unknown, b"Preserve unrelated user bytes.")?;
    repair(&root, &identity)?;
    let main = verify_runtime(&root, &identity)?.0;
    assert_eq!(
        runtime_pair::digest_file(&verified_launcher(&root)?)?,
        runtime_pair::digest_file(&main)?
    );
    assert!(pending(&root)?.is_none());
    assert!(!intent.partial(&root).exists());
    assert_eq!(fs::read(&unknown)?, b"Preserve unrelated user bytes.");
    Ok(())
}

#[test]
fn unknown_changed_oversized_and_aliased_launcher_files_are_preserved() -> Result<()> {
    let (_owner, root, identity) = fixture()?;
    fs::write(launcher_path(&root), b"Unknown launcher.")?;
    assert!(repair(&root, &identity).is_err());
    assert!(!root.join(INTENT).exists());
    let old = old_launcher(&root)?;
    let intent = begin(&root, &identity)?;
    fs::write(launcher_path(&root), b"User modified the launcher.")?;
    assert!(repair(&root, &identity).is_err());
    assert!(abort(&root).is_err());
    assert_eq!(
        fs::read(launcher_path(&root))?,
        b"User modified the launcher."
    );
    fs::write(launcher_path(&root), &old)?;
    let partial = intent.partial(&root);
    fs::write(&partial, vec![0; intent.bytes as usize + 1])?;
    assert!(repair(&root, &identity).is_err());
    assert!(abort(&root).is_err());
    fs::write(&partial, b"unfinished")?;
    let alias = root.join("unrelated-alias");
    fs::hard_link(&partial, &alias)?;
    assert!(repair(&root, &identity).is_err());
    assert!(abort(&root).is_err());
    assert_eq!(fs::read(&alias)?, b"unfinished");
    fs::remove_file(&alias)?;
    abort(&root)?;
    assert!(!root.join(INTENT).exists());
    assert!(!partial.exists());
    assert_eq!(fs::read(launcher_path(&root))?, old);
    Ok(())
}

#[test]
fn launcher_rename_child_fixture() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_LAUNCHER_RENAME_FIXTURE") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let identity = std::env::var("AIRK_LAUNCHER_RUNTIME_FIXTURE")?;
    let _guard = install_lock(&root)?;
    let intent = begin(&root, &identity)?;
    copy(&root, &intent, &verify_runtime(&root, &identity)?.0)?;
    fs::rename(intent.partial(&root), launcher_path(&root))?;
    sync_directory(&root)?;
    std::process::exit(80);
}

#[test]
fn process_death_after_rename_resumes_receipt_without_overwriting_the_image() -> Result<()> {
    let (_owner, root, identity) = fixture()?;
    old_launcher(&root)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "runtime_install::launcher_repair::tests::launcher_rename_child_fixture",
            "--nocapture",
        ])
        .env("AIRK_LAUNCHER_RENAME_FIXTURE", &root)
        .env("AIRK_LAUNCHER_RUNTIME_FIXTURE", &identity)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = crate::native_process::spawn(&mut command)?.wait()?;
    assert_eq!(status.code(), Some(80));
    assert!(verified_launcher(&root).is_err());
    assert!(abort(&root).is_err());
    let before = fs::read(launcher_path(&root))?;
    repair(&root, &identity)?;
    assert_eq!(fs::read(verified_launcher(&root)?)?, before);
    require_settled(&root)?;
    Ok(())
}

#[test]
fn uninstall_settles_only_existing_owned_repair_bytes() -> Result<()> {
    let (_owner, root, identity) = fixture()?;
    let old = old_launcher(&root)?;
    let intent = begin(&root, &identity)?;
    fs::write(intent.partial(&root), b"unfinished")?;
    settle_for_uninstall(&root)?;
    assert_eq!(fs::read(launcher_path(&root))?, old);
    let intent = begin(&root, &identity)?;
    copy(&root, &intent, &verify_runtime(&root, &identity)?.0)?;
    fs::rename(intent.partial(&root), launcher_path(&root))?;
    settle_for_uninstall(&root)?;
    verified_launcher(&root)?;
    require_settled(&root)?;
    Ok(())
}
