// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::http_endpoint::TlsIdentity;
use airk_core::identity::Incarnation;

#[test]
fn stopped_installation_cleanup_removes_only_receipted_issuer_and_preserves_unknown_outcomes()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    crate::local_ipc::prepare_root(&control)?;
    let registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        Incarnation::from_bytes([7; 16]),
    )?;
    let binding = crate::http_endpoint::installation_binding(&control, registry.incarnation()?)?;
    drop(registry);
    let root = control.join("http-1");
    let identity = TlsIdentity::load_or_create(&root, binding.clone(), 1_800_000_000_000)?;
    let descriptor = airk_core::service_http::EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([8; 16]),
        executable_sha256: Sha256Digest::from_bytes([9; 32]),
        url: airk_core::service_http::LoopbackMcpUrl::new(57422)?,
        ca_sha256: identity.ca_sha256(),
        certificate_sha256: identity.certificate_sha256(),
        state: airk_core::service_http::EndpointState::Stopping,
        predecessor_digest: None,
    };
    crate::http_endpoint::publish_descriptor(&root, &binding, &descriptor, None)?;
    // No service/listener exists. A preexisting CA has no ownership receipt.
    assert!(
        remove_owned_with(&control, &mut |_, _, _| panic!("unowned issuer mutation"))
            .is_ok_and(|result| result.is_none())
    );
    let sha = identity.ca_sha256();
    let mut present = false;
    let removed = std::cell::Cell::new(0);
    let mut run = |_: &Path, args: &[String]| {
        let script = args.last().unwrap();
        if script.contains("} 'install'") {
            present = true;
        }
        if script.contains("} 'remove'") {
            present = false;
            removed.set(removed.get() + 1);
        }
        Ok((
            true,
            format!("{{\"present\":{present},\"trusted\":{present}}}"),
        ))
    };
    manage_with(
        &root,
        binding.clone(),
        sha,
        Action::Install,
        Host::Windows,
        &mut run,
    )?;
    let installed = read_private(&root, RECEIPT, LIMIT)?.unwrap();
    let status = remove_owned_with(&control, &mut |root, binding, sha| {
        manage_with(root, binding, sha, Action::Remove, Host::Windows, &mut run)
    })?
    .unwrap();
    assert_eq!(status.outcome, "removed");
    assert_eq!(removed.get(), 1);
    let mut receipt: Receipt = serde_json::from_slice(&installed)?;
    receipt.phase = Phase::RemovePrepared;
    write_private(&root, RECEIPT, &serde_json::to_vec(&receipt)?, LIMIT, true)?;
    let unknown = read_private(&root, RECEIPT, LIMIT)?;
    assert!(
        remove_owned_with(&control, &mut |_, _, _| panic!("uncertain mutation replay")).is_err()
    );
    assert_eq!(read_private(&root, RECEIPT, LIMIT)?, unknown);
    receipt.phase = Phase::Installed;
    receipt.ca_sha256 = Sha256Digest::from_bytes([99; 32]);
    write_private(&root, RECEIPT, &serde_json::to_vec(&receipt)?, LIMIT, true)?;
    assert!(remove_owned_with(&control, &mut |_, _, _| panic!("crossed issuer mutation")).is_err());
    Ok(())
}

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    InstallationBinding,
    Sha256Digest,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("http");
    let binding = InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user().unwrap(),
        namespace: Incarnation::from_bytes([2; 16]),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let identity = TlsIdentity::load_or_create(&root, binding.clone(), now).unwrap();
    (directory, root, binding, identity.ca_sha256())
}
fn contents(root: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn inspect_is_read_only_and_preexisting_issuer_is_never_owned_or_removed() {
    let (_directory, root, binding, sha) = fixture();
    let before = contents(&root);
    let mut mutations = 0;
    let mut run = |_: &Path, args: &[String]| {
        let script = args.last().unwrap();
        assert!(script.contains("'Root','CurrentUser'"));
        if !script.contains("} 'inspect'") {
            mutations += 1;
        }
        Ok((true, r#"{"present":true,"trusted":true}"#.into()))
    };
    let status = manage_with(
        &root,
        binding.clone(),
        sha,
        Action::Inspect,
        Host::Windows,
        &mut run,
    )
    .unwrap();
    assert!(status.trusted);
    assert!(!status.owned);
    assert_eq!(contents(&root), before);
    let status = manage_with(
        &root,
        binding.clone(),
        sha,
        Action::Install,
        Host::Windows,
        &mut run,
    )
    .unwrap();
    assert_eq!(status.outcome, "already_trusted");
    assert!(!root.join(RECEIPT).exists());
    assert!(manage_with(&root, binding, sha, Action::Remove, Host::Windows, &mut run).is_err());
    assert_eq!(mutations, 0);
}

#[test]
fn exact_generated_issuer_has_durable_install_and_remove_receipts() {
    let (_directory, root, binding, sha) = fixture();
    let mut present = false;
    let mut mutations = Vec::new();
    let mut run = |_: &Path, args: &[String]| {
        let script = args.last().unwrap();
        assert!(script.contains(&sha.to_string()));
        assert!(!script.contains("LocalMachine"));
        if script.contains("} 'install'") {
            assert!(!present);
            present = true;
            mutations.push("install");
        }
        if script.contains("} 'remove'") {
            assert!(present);
            present = false;
            mutations.push("remove");
        }
        Ok((
            true,
            format!("{{\"present\":{present},\"trusted\":{present}}}"),
        ))
    };
    let status = manage_with(
        &root,
        binding.clone(),
        sha,
        Action::Install,
        Host::Windows,
        &mut run,
    )
    .unwrap();
    assert!(status.owned && status.trusted);
    assert_eq!(status.outcome, "installed");
    let bytes = read_private(&root, RECEIPT, LIMIT).unwrap().unwrap();
    let receipt: Receipt = serde_json::from_slice(&bytes).unwrap();
    assert!(receipt.phase == Phase::Installed);
    let status = manage_with(&root, binding, sha, Action::Remove, Host::Windows, &mut run).unwrap();
    assert!(!status.present && !status.owned);
    assert_eq!(status.outcome, "removed");
    assert_eq!(mutations, vec!["install", "remove"]);
}

#[test]
fn interrupted_mutation_remains_unknown_and_never_replays_or_adopts() {
    let (_directory, root, binding, sha) = fixture();
    let mut mutations = 0;
    let mut run = |_: &Path, args: &[String]| {
        if args.last().unwrap().contains("} 'install'") {
            mutations += 1;
            return Ok((false, "bounded fictional OS failure".into()));
        }
        Ok((true, r#"{"present":false,"trusted":false}"#.into()))
    };
    assert!(
        manage_with(
            &root,
            binding.clone(),
            sha,
            Action::Install,
            Host::Windows,
            &mut run
        )
        .is_err()
    );
    let status = manage_with(
        &root,
        binding.clone(),
        sha,
        Action::Install,
        Host::Windows,
        &mut run,
    )
    .unwrap();
    assert_eq!(status.outcome, "outcome_unknown");
    assert!(!status.owned);
    let status = manage_with(&root, binding, sha, Action::Remove, Host::Windows, &mut run).unwrap();
    assert_eq!(status.outcome, "outcome_unknown");
    assert_eq!(mutations, 1);
}

#[test]
fn crossed_issuer_and_unsupported_platform_never_invoke_os_commands() {
    let (_directory, root, binding, sha) = fixture();
    let before = contents(&root);
    let mut never =
        |_: &Path, _: &[String]| -> Result<(bool, String)> { panic!("unqualified trust command") };
    assert!(
        manage_with(
            &root,
            binding.clone(),
            Sha256Digest::from_bytes([0; 32]),
            Action::Install,
            Host::Windows,
            &mut never
        )
        .is_err()
    );
    let status = manage_with(
        &root,
        binding,
        sha,
        Action::Install,
        Host::Unsupported,
        &mut never,
    )
    .unwrap();
    assert_eq!(status.outcome, "unsupported");
    assert!(!status.trusted);
    assert_eq!(contents(&root), before);
    assert_eq!(
        ps_string("issuer' ; dangerous").unwrap(),
        "'issuer'' ; dangerous'"
    );
    assert!(ps_string("issuer\nnext").is_err());
}

#[test]
fn active_mutations_share_rotation_lock_before_receipt_lock_but_inspect_stays_read_only()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    crate::local_ipc::prepare_root(&control)?;
    let root = control.join("http-1");
    let binding = InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user()?,
        namespace: Incarnation::from_bytes([2; 16]),
    };
    let identity = TlsIdentity::load_or_create(&root, binding.clone(), 1_800_000_000_000)?;
    let _rotation = super::super::rotation::lock(&control)?;
    for action in [Action::Install, Action::Remove] {
        assert!(
            manage_with(
                &root,
                binding.clone(),
                identity.ca_sha256(),
                action,
                Host::Windows,
                &mut |_, _| panic!("active mutation crossed rotation lock")
            )
            .is_err()
        );
    }
    let before = contents(&root);
    let observed = manage_with(
        &root,
        binding.clone(),
        identity.ca_sha256(),
        Action::Inspect,
        Host::Windows,
        &mut |_, _| Ok((true, r#"{"present":false,"trusted":false}"#.into())),
    )?;
    assert_eq!(observed.outcome, "not_trusted");
    assert_eq!(contents(&root), before);
    let staged = owner.path().join("staged/new");
    let candidate = TlsIdentity::load_or_create(&staged, binding.clone(), 1_800_000_000_000)?;
    let mut present = false;
    let status = manage_with(
        &staged,
        binding,
        candidate.ca_sha256(),
        Action::Install,
        Host::Windows,
        &mut |_, args| {
            if args.last().unwrap().contains("} 'install'") {
                present = true;
            }
            Ok((
                true,
                format!("{{\"present\":{present},\"trusted\":{present}}}"),
            ))
        },
    )?;
    assert_eq!(status.outcome, "installed");
    Ok(())
}
