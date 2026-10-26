// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn private_stage_directory_creation_is_exclusive_and_owner_verified() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let directory = owner.path().join("stage");
    create_private_directory(&directory)?;
    local_ipc::prepare_root(&directory)?;
    fs::write(directory.join("keep"), b"preserve")?;
    assert!(create_private_directory(&directory).is_err());
    assert_eq!(fs::read(directory.join("keep"))?, b"preserve");
    Ok(())
}

fn fixture(root: &Path) -> Result<(PathBuf, PathBuf, String, StageIntent, InstallManifest)> {
    let root = root.canonicalize()?;
    let install = root.join("install");
    local_ipc::prepare_root(&install)?;
    local_ipc::prepare_root(&install.join("runtimes"))?;
    let source = root.join("source");
    local_ipc::prepare_root(&source)?;
    let identity = super::super::tests::fixture(&source)?;
    let (bytes, inventory) = manifest(&source, &identity)?;
    let lengths = inventory
        .files
        .keys()
        .map(|name| Ok((name.clone(), fs::metadata(source.join(name))?.len())))
        .collect::<Result<_>>()?;
    let intent = StageIntent {
        schema: 1,
        root: install.canonicalize()?.to_str().unwrap().into(),
        leaf: format!("stage-{}", "1".repeat(32)),
        approval: Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: inventory.version.clone(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
        manifest_json: String::from_utf8(bytes)?,
        lengths,
    };
    fs::write(
        StageIntent::path(&install, &identity),
        serde_json::to_vec(&intent)?,
    )?;
    Ok((install, source, identity, intent, inventory))
}

#[test]
fn owned_partial_stage_resumes_without_replacing_completed_files_or_source_bytes() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let (root, source, identity, intent, inventory) = fixture(owner.path())?;
    let directory = intent.directory(&root);
    local_ipc::prepare_root(&directory)?;
    local_ipc::prepare_root(&directory.join(PARTS))?;
    let original = fs::read(source.join(main_name()))?;
    fs::write(
        directory.join(PARTS).join(&inventory.files[main_name()]),
        &original[..3],
    )?;
    fs::copy(source.join("LICENSE"), directory.join("LICENSE"))?;
    let resumed = StageIntent::read(&root, &identity)?.0;
    let approval = copy(&source, &root, &resumed, &inventory, |main| {
        manifest(runtime_pair::payload_root(main)?, &identity)?;
        Ok(())
    })?;
    assert_eq!(approval, intent.approval);
    assert!(!directory.exists());
    assert!(pending(&root)?.is_empty());
    assert_eq!(fs::read(source.join(main_name()))?, original);
    assert_eq!(
        fs::read(root.join("runtimes").join(&identity).join(main_name()))?,
        original
    );
    // A crash after promotion leaves only the old intent; reuse finalizes it.
    fs::write(
        StageIntent::path(&root, &identity),
        serde_json::to_vec(&intent)?,
    )?;
    stage(
        &source,
        &root,
        &identity,
        VerificationBasis::TrustedDistribution,
    )?;
    assert!(pending(&root)?.is_empty());
    Ok(())
}

#[test]
fn abort_preserves_unknown_changed_and_aliased_files_then_resumes_owned_partial_removal()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let (root, source, identity, intent, inventory) = fixture(owner.path())?;
    let directory = intent.directory(&root);
    local_ipc::prepare_root(&directory)?;
    local_ipc::prepare_root(&directory.join(PARTS))?;
    fs::copy(source.join("LICENSE"), directory.join("LICENSE"))?;
    fs::write(
        directory.join(PARTS).join(&inventory.files[main_name()]),
        b"x",
    )?;
    let unknown = directory.join("user.txt");
    fs::write(&unknown, b"User-owned fixture.")?;
    assert!(abort(&root, &identity).is_err());
    assert_eq!(fs::read(&unknown)?, b"User-owned fixture.");
    fs::remove_file(unknown)?;
    let legal = fs::read(source.join("LICENSE"))?;
    fs::write(directory.join("LICENSE"), b"Modified by user fixture.")?;
    assert!(abort(&root, &identity).is_err());
    assert_eq!(
        fs::read(directory.join("LICENSE"))?,
        b"Modified by user fixture."
    );
    fs::write(directory.join("LICENSE"), &legal)?;
    let alias = owner.path().join("aliased-license");
    fs::hard_link(directory.join("LICENSE"), &alias)?;
    assert!(abort(&root, &identity).is_err());
    fs::remove_file(alias)?;
    // Simulate interruption after the first owned unlink. The external intent remains.
    fs::remove_file(directory.join("LICENSE"))?;
    abort(&root, &identity)?;
    assert!(!directory.exists());
    assert!(pending(&root)?.is_empty());
    assert_eq!(fs::read(source.join("LICENSE"))?, legal);
    Ok(())
}

#[test]
fn interrupted_intents_reserve_slots_before_creating_directories() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let (root, source, identity, mut intent, mut inventory) = fixture(owner.path())?;
    fs::remove_file(StageIntent::path(&root, &identity))?;
    for index in 0..4 {
        inventory.version = format!("1.3.2+staging-{index}");
        intent.manifest_json = serde_json::to_string(&inventory)?;
        intent.approval.manifest_sha256 =
            format!("{:x}", Sha256::digest(intent.manifest_json.as_bytes()));
        intent.approval.version = inventory.version.clone();
        intent.leaf = format!("stage-{index:032x}");
        fs::write(
            StageIntent::path(&root, &intent.approval.manifest_sha256),
            serde_json::to_vec(&intent)?,
        )?;
    }
    assert_eq!(occupied_slots(&root)?, 4);
    assert_eq!(pending(&root)?.len(), 4);
    assert!(
        stage(
            &source,
            &root,
            &identity,
            VerificationBasis::OrganizationDigest
        )
        .unwrap_err()
        .to_string()
        .contains("four retained")
    );
    assert_eq!(fs::read_dir(root.join("runtimes"))?.count(), 0);
    for identity in pending(&root)? {
        abort(&root, &identity)?;
    }
    assert_eq!(occupied_slots(&root)?, 0);
    Ok(())
}

#[test]
fn stage_copy_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_STAGE_CRASH_FIXTURE") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let identity = std::env::var("AIRK_STAGE_CRASH_ID")?;
    let source = PathBuf::from(
        std::env::var_os("AIRK_STAGE_CRASH_SOURCE").context("source fixture missing")?,
    );
    let (intent, inventory) = StageIntent::read(&root, &identity)?;
    copy(&source, &root, &intent, &inventory, |_| {
        std::process::exit(79)
    })?;
    bail!("crash fixture unexpectedly promoted")
}

#[test]
fn portable_payload_topology_rejects_case_aliases_and_reserved_staging_names() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let (_root, source, identity, _intent, _inventory) = fixture(owner.path())?;
    let original = fs::read(source.join("airk-install.json"))?;
    for name in [
        "license",
        "NOTICES/other.json",
        ".airk-stage-parts/owned",
        "notices/café.json",
    ] {
        let mut inventory: InstallManifest = serde_json::from_slice(&original)?;
        inventory
            .files
            .insert(name.into(), inventory.files["LICENSE"].clone());
        let bytes = serde_json::to_vec(&inventory)?;
        let approved = format!("{:x}", Sha256::digest(&bytes));
        fs::write(source.join("airk-install.json"), bytes)?;
        assert!(
            manifest(&source, &approved).is_err(),
            "aliased or reserved payload topology accepted: {name}"
        );
    }
    fs::write(source.join("airk-install.json"), original)?;
    manifest(&source, &identity)?;
    Ok(())
}

#[test]
fn native_self_test_cannot_promote_unlisted_or_modified_payload_output() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let (root, source, identity, intent, inventory) = fixture(owner.path())?;
    let unknown = intent.directory(&root).join("self-test-output.txt");
    assert!(
        copy(&source, &root, &intent, &inventory, |_| {
            fs::write(&unknown, b"Unknown native output.")?;
            Ok(())
        })
        .is_err()
    );
    assert!(!root.join("runtimes").join(&identity).exists());
    assert!(abort(&root, &identity).is_err());
    assert_eq!(fs::read(&unknown)?, b"Unknown native output.");
    fs::remove_file(unknown)?;
    abort(&root, &identity)?;
    Ok(())
}

#[test]
fn process_death_before_promotion_retains_a_resumable_complete_owned_copy() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let (root, source, identity, intent, inventory) = fixture(owner.path())?;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "runtime_install::staging::tests::stage_copy_child",
        ])
        .env("AIRK_STAGE_CRASH_FIXTURE", &root)
        .env("AIRK_STAGE_CRASH_ID", &identity)
        .env("AIRK_STAGE_CRASH_SOURCE", &source)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            bail!("staging crash fixture exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(79));
    assert!(StageIntent::path(&root, &identity).exists());
    assert!(intent.directory(&root).exists());
    assert!(!root.join("runtimes").join(&identity).exists());
    let (resumed, _) = StageIntent::read(&root, &identity)?;
    copy(&source, &root, &resumed, &inventory, |main| {
        manifest(runtime_pair::payload_root(main)?, &identity)?;
        Ok(())
    })?;
    assert!(root.join("runtimes").join(&identity).exists());
    assert!(pending(&root)?.is_empty());
    Ok(())
}
