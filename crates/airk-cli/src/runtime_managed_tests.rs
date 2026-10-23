// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture(lifecycle: ManagedLifecycle) -> Result<(tempfile::TempDir, PathBuf, Managed)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("metadata");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    let source = owner.path().join("provider");
    fs::create_dir(&source)?;
    let identity = super::super::tests::fixture(&source)?;
    let source = source.canonicalize()?;
    let policy = Managed {
        schema: 1,
        install: root.canonicalize()?,
        source: source.clone(),
        approval: Approval {
            schema: 1,
            manifest_sha256: identity,
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
        executable_sha256: runtime_pair::digest_file(&source.join(main_name()))?,
        lifecycle,
    };
    replace_json(&root, POLICY, &policy)?;
    Ok((owner, root, policy))
}

#[test]
fn pinned_provider_files_are_verified_without_copying_into_user_installation() -> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let original = fs::read(policy.source.join(main_name()))?;
    let (main, approval) = verify_runtime(&root, &policy.approval.manifest_sha256)?;
    assert_eq!(main, policy.source.join(main_name()));
    assert_eq!(approval, policy.approval);
    assert!(!root.join("runtimes").exists());
    assert!(!launcher_path(&root).exists());
    assert!(require_unmanaged(&root).is_err());
    assert_eq!(fs::read(&main)?, original);
    Ok(())
}

#[test]
fn unlisted_or_changed_provider_files_are_preserved_and_block_execution() -> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let unknown = policy.source.join("unlisted-library.dll");
    fs::write(&unknown, b"Preserve unlisted provider bytes.")?;
    assert!(verify_runtime(&root, &policy.approval.manifest_sha256).is_err());
    assert_eq!(fs::read(&unknown)?, b"Preserve unlisted provider bytes.");
    fs::remove_file(unknown)?;
    let main = policy.source.join(main_name());
    fs::write(&main, b"Changed provider executable.")?;
    assert!(verify_runtime(&root, &policy.approval.manifest_sha256).is_err());
    assert_eq!(fs::read(&main)?, b"Changed provider executable.");
    assert!(
        read(&root)?.is_some(),
        "policy diagnosis should survive unavailable provider code"
    );
    Ok(())
}

#[test]
fn foreign_policy_cannot_rebind_an_installation() -> Result<()> {
    let (_owner, root, mut policy) = fixture(ManagedLifecycle::External)?;
    policy.install = root.join("foreign-installation");
    replace_json(&root, POLICY, &policy)?;
    let before = fs::read(root.join(POLICY))?;
    assert!(read(&root).is_err());
    assert!(require_unmanaged(&root).is_err());
    assert_eq!(fs::read(root.join(POLICY))?, before);
    Ok(())
}

#[test]
fn unavailable_external_supervisor_never_spawns_a_user_process_or_selects_unverified_health()
-> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::External)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        assert!(ensure(&root).await.is_err());
    });
    assert!(!root.join("selected.json").exists());
    assert!(!root.join("bootstrap.json").exists());
    assert!(!fs::read_dir(&root)?.any(|entry| {
        entry.is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with("startup-"))
    }));
    assert_eq!(
        runtime_pair::digest_file(&policy.source.join(main_name()))?,
        policy.executable_sha256
    );
    Ok(())
}

#[test]
fn invalid_managed_selection_is_a_policy_failure_instead_of_supervisor_unavailability() -> Result<()>
{
    let (_owner, root, _policy) = fixture(ManagedLifecycle::External)?;
    fs::write(root.join("selected.json"), b"Unrecognized selection bytes.")?;
    let error = tokio::runtime::Runtime::new()?
        .block_on(async { ensure(&root).await })
        .err()
        .context("invalid managed selection was accepted")?;
    assert!(error.downcast_ref::<SupervisorUnavailable>().is_none());
    assert_eq!(
        fs::read(root.join("selected.json"))?,
        b"Unrecognized selection bytes."
    );
    Ok(())
}

#[test]
fn detach_preserves_provider_payload_state_and_an_external_owner() -> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::External)?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let before = fs::read(policy.source.join(main_name()))?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let result = detach(&root).await?;
        assert_eq!(result["external_supervisor_continues"], true);
        Ok::<(), anyhow::Error>(())
    })?;
    assert!(airk_store::service::ServiceLease::acquire(&control).is_err());
    assert_eq!(fs::read(policy.source.join(main_name()))?, before);
    assert!(control.exists());
    assert!(!root.join(POLICY).exists());
    assert!(!root.join(DETACH).exists());
    drop(lease);
    Ok(())
}

#[test]
fn user_managed_detach_does_not_require_provider_files_to_remain_present() -> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let missing = policy.source.join(main_name());
    let retained = policy.source.join("provider-retained-main");
    fs::rename(&missing, &retained)?;
    tokio::runtime::Runtime::new()?.block_on(async { detach(&root).await })?;
    assert!(retained.exists());
    assert!(!missing.exists());
    assert!(!root.join(POLICY).exists());
    Ok(())
}

#[test]
fn interrupted_detach_resumes_after_reference_removal_without_touching_provider() -> Result<()> {
    let (_owner, root, policy) = fixture(ManagedLifecycle::External)?;
    replace_json(&root, DETACH, &policy)?;
    fs::remove_file(root.join(POLICY))?;
    let before = fs::read(policy.source.join(main_name()))?;
    tokio::runtime::Runtime::new()?.block_on(async { detach(&root).await })?;
    assert_eq!(fs::read(policy.source.join(main_name()))?, before);
    assert!(!root.join(DETACH).exists());
    Ok(())
}
