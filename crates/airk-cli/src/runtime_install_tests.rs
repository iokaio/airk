// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn launcher_source_selection_binds_the_declared_mac_image_and_preserves_legacy_layouts()
-> Result<()> {
    assert_eq!(standalone_launcher_name("macos", "1.8.7")?, None);
    assert_eq!(
        standalone_launcher_name("macos", "1.8.8")?,
        Some(MAC_LAUNCHER_IMAGE)
    );
    assert_eq!(
        standalone_launcher_name("macos", "1.9.0")?,
        Some(MAC_LAUNCHER_IMAGE)
    );
    assert_eq!(standalone_launcher_name("linux", "1.9.0")?, None);
    assert_eq!(standalone_launcher_name("windows", "1.9.0")?, None);
    for version in ["1.8", "1.8.8.1", "1.8.8-dev"] {
        assert!(standalone_launcher_name("macos", version).is_err());
    }
    let owner = tempfile::tempdir()?;
    let directory = owner.path().canonicalize()?;
    fs::create_dir_all(directory.join("AIRK.app/Contents/MacOS"))?;
    fs::write(directory.join(main_name()), b"bundled main fixture")?;
    fs::write(
        directory.join(MAC_LAUNCHER_IMAGE),
        b"independently signed image fixture",
    )?;
    let mut inventory = InstallManifest {
        schema: 1,
        version: "1.8.8".into(),
        os: "macos".into(),
        arch: std::env::consts::ARCH.into(),
        files: BTreeMap::from([
            (
                main_name().into(),
                runtime_pair::digest_file(&directory.join(main_name()))?,
            ),
            (
                MAC_LAUNCHER_IMAGE.into(),
                runtime_pair::digest_file(&directory.join(MAC_LAUNCHER_IMAGE))?,
            ),
        ]),
    };
    assert_eq!(
        launcher_image(&directory, &inventory)?,
        directory.join(MAC_LAUNCHER_IMAGE)
    );
    let expected = inventory.files.remove(MAC_LAUNCHER_IMAGE).unwrap();
    assert!(launcher_image(&directory, &inventory).is_err());
    inventory.files.insert(MAC_LAUNCHER_IMAGE.into(), expected);
    fs::write(
        directory.join(MAC_LAUNCHER_IMAGE),
        b"changed image must not inherit approval",
    )?;
    assert!(launcher_image(&directory, &inventory).is_err());
    inventory.version = "1.8.7".into();
    assert_eq!(
        launcher_image(&directory, &inventory)?,
        directory.join(main_name())
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn mac_initial_and_repaired_launchers_use_the_independently_admitted_image() -> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    let (main, _, inventory) = verify_runtime_inventory(&root, &identity)?;
    let image = launcher_image(runtime_pair::payload_root(&main)?, &inventory)?;
    assert_ne!(
        runtime_pair::digest_file(&image)?,
        runtime_pair::digest_file(&main)?
    );
    prepare_launcher(&root, &main, &inventory)?;
    assert_eq!(
        runtime_pair::digest_file(&verified_launcher(&root)?)?,
        runtime_pair::digest_file(&image)?
    );
    fs::remove_file(launcher_path(&root))?;
    launcher_repair::repair(&root, &identity)?;
    assert_eq!(
        runtime_pair::digest_file(&verified_launcher(&root)?)?,
        runtime_pair::digest_file(&image)?
    );
    Ok(())
}

#[test]
fn catalog_activation_checks_formats_instead_of_trusting_equal_application_versions() -> Result<()>
{
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::CatalogRevision,
    };
    let mut state = airk_store::service::registry::catalog::CatalogState {
        schema: 12,
        namespace: Incarnation::from_bytes([1; 16]),
        revision: CatalogRevision::INITIAL,
        activated: true,
        accepted_new_writes: true,
        authority_contract: 0,
        policy_contract: 0,
        upgrade_id: Incarnation::from_bytes([2; 16]),
        legacy_schema_digest: Sha256Digest::from_bytes([3; 32]),
        backup_digest: Sha256Digest::from_bytes([4; 32]),
    };
    let old = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"self_test":"passed"});
    assert!(require_catalog_capabilities(&state, &old).is_err());
    let current = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"index_catalog":airk_store::service::registry::catalog::capabilities()});
    require_catalog_capabilities(&state, &current)?;
    state.authority_contract = 1;
    let mut earlier_capabilities = airk_store::service::registry::catalog::capabilities();
    earlier_capabilities.authority_contract = 0;
    let earlier_same_version = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"index_catalog":earlier_capabilities});
    assert!(require_catalog_capabilities(&state, &earlier_same_version).is_err());
    assert_eq!(
        require_catalog_capabilities(&state, &current).is_ok(),
        airk_store::service::registry::catalog::capabilities().supports(
            state.schema,
            state.authority_contract,
            state.policy_contract,
        )
    );
    state.authority_contract = 0;
    state.policy_contract = 1;
    assert!(require_catalog_capabilities(&state, &current).is_err());
    state.policy_contract = 0;
    state.schema = 13;
    assert!(require_catalog_capabilities(&state, &current).is_err());
    Ok(())
}

#[test]
fn legacy_catalog_admission_does_not_start_an_extra_payload_probe() -> Result<()> {
    let (_owner, root, _) = unselected_fixture()?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?);
    let admission = quiescent(&root)?;
    assert!(admission.catalog.is_none());
    admission.require_catalog_compatible(&root.join("missing-payload-must-not-be-started"))?;
    Ok(())
}

#[tokio::test]
async fn stopped_upgrade_checks_v6_without_migration_and_reports_damage_immediately() -> Result<()>
{
    let (_owner, root, _) = unselected_fixture()?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?);
    let path = control.join("service-1.4.sqlite3");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch("DROP TABLE service_watch_traversal; PRAGMA user_version=6;")?;
    drop(connection);
    let before = fs::read(&path)?;
    wait_quiescent(&root).await?;
    assert_eq!(fs::read(&path)?, before);
    fs::write(&path, b"Preserve damaged registry")?;
    let started = std::time::Instant::now();
    let error = wait_quiescent(&root).await.err().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(format!("{error:#}").contains("database"));
    assert_eq!(fs::read(path)?, b"Preserve damaged registry");
    Ok(())
}

pub(super) fn fixture(root: &Path) -> Result<String> {
    local_ipc::prepare_root(&root.join("notices"))?;
    let mut directory = root.to_path_buf();
    for component in Path::new(main_name())
        .parent()
        .context("fixture executable parent")?
        .components()
    {
        directory.push(component);
        local_ipc::prepare_root(&directory)?;
    }
    fs::write(root.join(main_name()), b"Original inert main fixture.")?;
    fs::write(root.join(helper_name()), b"Original inert helper fixture.")?;
    if cfg!(target_os = "macos") {
        fs::write(
            root.join(MAC_LAUNCHER_IMAGE),
            b"Original inert standalone launcher fixture.",
        )?;
    }
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
    let mut names = vec![
        main_name(),
        helper_name(),
        "LICENSE",
        "notices/inventory.json",
        "airk-runtime.json",
    ];
    if cfg!(target_os = "macos") {
        names.push(MAC_LAUNCHER_IMAGE);
    }
    let files = names
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
    let root = owner.path().canonicalize()?.join("install");
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
    let root = owner.path().canonicalize()?.join("install");
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
    let root = owner.path().canonicalize()?.join("install");
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
    let root = owner.path().canonicalize()?.join("install");
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
    let source = owner.path().canonicalize()?.join("source");
    fs::create_dir(&source)?;
    let approved = fixture(&source)?;
    let root = owner.path().canonicalize()?.join("install");
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

#[test]
fn manifest_hashes_every_inventory_file_once_and_still_checks_pair_fields() -> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    let mut hashes = BTreeMap::<PathBuf, usize>::new();
    let (_, inventory) = manifest_with_hasher(&payload, &identity, |path| {
        *hashes.entry(path.to_path_buf()).or_default() += 1;
        runtime_pair::digest_file(path)
    })?;
    assert_eq!(hashes.len(), inventory.files.len());
    for name in inventory.files.keys() {
        assert_eq!(hashes.get(&payload.join(name)), Some(&1), "{name}");
    }
    assert_eq!(hashes.get(&payload.join(main_name())), Some(&1));
    assert_eq!(hashes.get(&payload.join(helper_name())), Some(&1));
    // Authenticate a deliberately inconsistent pair receipt through a new
    // manifest; successful file hashes alone must not qualify this pair.
    let mut pair: runtime_pair::RuntimePair =
        serde_json::from_slice(&fs::read(payload.join("airk-runtime.json"))?)?;
    pair.embed_sha256 = "a".repeat(64);
    fs::write(
        payload.join("airk-runtime.json"),
        serde_json::to_vec(&pair)?,
    )?;
    let mut changed = inventory;
    changed.files.insert(
        "airk-runtime.json".into(),
        runtime_pair::digest_file(&payload.join("airk-runtime.json"))?,
    );
    let approved = write_manifest(&payload, &changed)?;
    let error = manifest(&payload, &approved)
        .err()
        .context("inconsistent pair was accepted")?;
    assert!(
        error
            .to_string()
            .contains("runtime executable pair digest mismatch")
    );
    Ok(())
}

#[test]
fn locked_selection_keeps_full_topology_and_distinct_role_verification() -> Result<()> {
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
    let main = root.join("runtimes").join(&identity).join(main_name());
    crate::process_roles::install(&main)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let verified = selected_verified(&authority)?;
    assert_eq!(verified.main_sha256, runtime_pair::digest_file(&main)?);
    let role =
        crate::process_roles::executable(&verified.main, crate::process_roles::Role::Service)?;
    if cfg!(target_os = "macos") {
        assert_eq!(role, verified.main);
        let alias =
            runtime_pair::payload_root(&main)?.join(crate::process_roles::Role::Service.name());
        fs::write(&alias, b"unexpected independent service alias")?;
    } else {
        assert_ne!(role, verified.main);
        fs::write(&role, b"modified independent service alias")?;
    }
    assert!(
        crate::process_roles::executable(&verified.main, crate::process_roles::Role::Service)
            .is_err()
    );
    assert!(
        selected_verified(&authority).is_err(),
        "topology must still hash an independently modified alias"
    );
    Ok(())
}

#[test]
fn locked_selection_still_refuses_unlisted_and_tampered_payload_files() -> Result<()> {
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
    let payload = root.join("runtimes").join(&identity);
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    assert!(selected_verified(&authority).is_ok());
    let unknown = payload.join("unlisted-neighbor.bin");
    fs::write(&unknown, b"original unknown neighboring fixture")?;
    assert!(selected_verified(&authority).is_err());
    fs::remove_file(unknown)?;
    fs::write(payload.join(helper_name()), b"modified helper fixture")?;
    assert!(selected_verified(&authority).is_err());
    Ok(())
}

#[tokio::test]
async fn selection_after_installer_wait_is_fully_current_not_the_pre_wait_identity() -> Result<()> {
    let (_owner, root, first) = unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: first.clone(),
            previous: None,
        },
    )?;
    let pre_wait = selected(&root)?.2;
    let lock = install_lock(&root)?;
    let next = root.join("next-original-fixture");
    local_ipc::prepare_root(&next)?;
    fixture(&next)?;
    fs::write(next.join("LICENSE"), b"Different original license fixture")?;
    let mut inventory: InstallManifest =
        serde_json::from_slice(&fs::read(next.join("airk-install.json"))?)?;
    inventory.files.insert(
        "LICENSE".into(),
        runtime_pair::digest_file(&next.join("LICENSE"))?,
    );
    let second = write_manifest(&next, &inventory)?;
    replace_json(
        &next,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: second.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(&next, root.join("runtimes").join(&second))?;
    let (entered, blocked) = tokio::sync::oneshot::channel();
    let waiting_root = root.clone();
    let waiter = tokio::spawn(async move {
        let _ = entered.send(());
        let lock = connection_install_lock(&waiting_root, Duration::from_secs(5)).await?;
        let authority = InstallerAuthority {
            root: &waiting_root,
            _lock: &lock,
        };
        let verified = selected_verified(&authority)?;
        Ok::<_, anyhow::Error>((
            verified.approval.manifest_sha256.clone(),
            verified.main_sha256.clone(),
        ))
    });
    blocked.await?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: second.clone(),
            previous: Some(first),
        },
    )?;
    drop(lock);
    let (observed, _) = waiter.await??;
    assert_eq!(observed, second);
    assert_ne!(
        observed, pre_wait.manifest_sha256,
        "installed_control must retain its reconnect guard rather than accept another selection"
    );
    Ok(())
}

#[tokio::test]
async fn direct_stopped_ensure_refuses_start_before_qualifying_a_missing_payload() -> Result<()> {
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
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?);
    let database = control.join("service-1.4.sqlite3");
    let connection = rusqlite::Connection::open(&database)?;
    connection.execute(
        "UPDATE service_lifecycle_state SET stopped=1 WHERE id=1",
        [],
    )?;
    drop(connection);
    fs::remove_file(root.join("runtimes").join(&identity).join(main_name()))?;
    let error = ensure_running(&root)
        .await
        .err()
        .context("stopped service was started")?;
    assert!(error.to_string().contains("service_explicitly_stopped"));
    assert!(!root.join("bootstrap.json").exists());
    assert!(!root.join(format!("startup-{identity}.json")).exists());
    Ok(())
}

#[test]
fn final_image_checkpoint_refuses_mutation_during_later_inventory_hash() -> Result<()> {
    for image in [main_name(), helper_name()] {
        let (_owner, root, identity) = unselected_fixture()?;
        let payload = root.join("runtimes").join(&identity);
        let changed = payload.join(image);
        let mut injected = false;
        let result = manifest_with_hasher(&payload, &identity, |path| {
            let hash = runtime_pair::digest_file(path)?;
            if path == payload.join("notices/inventory.json") {
                fs::write(
                    &changed,
                    b"changed after early path checks, before final image checkpoint",
                )?;
                injected = true;
            }
            Ok(hash)
        });
        assert!(injected);
        assert!(
            result.is_err(),
            "a previously guarded image must be freshly hashed at the final checkpoint"
        );
    }
    Ok(())
}

#[test]
fn final_image_checkpoint_refuses_a_new_hardlink_after_early_leaf_guard() -> Result<()> {
    let (owner, root, identity) = unselected_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    let mut injected = false;
    let result = manifest_with_hasher(&payload, &identity, |path| {
        let hash = runtime_pair::digest_file(path)?;
        if path == payload.join("notices/inventory.json") {
            fs::hard_link(
                payload.join(main_name()),
                owner.path().join("independent-hardlink"),
            )?;
            injected = true;
        }
        Ok(hash)
    });
    assert!(injected);
    assert!(
        result.is_err(),
        "final leaf guards must reject newly aliased images"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn final_image_checkpoint_refuses_a_new_symlink_after_early_leaf_guard() -> Result<()> {
    let (owner, root, identity) = unselected_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    let original = owner.path().join("independent-original-image");
    fs::copy(payload.join(main_name()), &original)?;
    let result = manifest_with_hasher(&payload, &identity, |path| {
        let hash = runtime_pair::digest_file(path)?;
        if path == payload.join("notices/inventory.json") {
            fs::remove_file(payload.join(main_name()))?;
            std::os::unix::fs::symlink(&original, payload.join(main_name()))?;
        }
        Ok(hash)
    });
    assert!(result.is_err());
    Ok(())
}

#[test]
fn nonimage_failure_now_precedes_a_deferred_image_failure() -> Result<()> {
    let (_owner, root, identity) = unselected_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    fs::write(payload.join(main_name()), b"modified image")?;
    fs::write(payload.join("notices/inventory.json"), b"modified nonimage")?;
    let error = manifest(&payload, &identity)
        .err()
        .context("combined damage was accepted")?;
    assert!(error.to_string().contains("notices/inventory.json"));
    Ok(())
}

fn selected_connection_fixture() -> Result<(tempfile::TempDir, PathBuf, String)> {
    let (owner, root, identity) = unselected_fixture()?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    Ok((owner, root, identity))
}

#[test]
fn metadata_preflight_reads_no_executable_bytes_and_common_preparation_verifies_once() -> Result<()>
{
    let (_owner, root, identity) = selected_connection_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    let main = fs::read(payload.join(main_name()))?;
    let helper = fs::read(payload.join(helper_name()))?;
    fs::remove_file(payload.join(main_name()))?;
    fs::remove_file(payload.join(helper_name()))?;
    let preflight = selected_preflight(&root)?;
    // A missing image cannot have been opened/hashed by metadata preflight.
    fs::write(payload.join(main_name()), main)?;
    fs::write(payload.join(helper_name()), helper)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let mut full_passes = 0;
    let prepared = prepare_connection_with(&authority, &preflight, |authority| {
        full_passes += 1;
        selected_verified(authority)
    })?;
    assert_eq!(
        full_passes, 1,
        "exactly one real complete verifier under lock"
    );
    let PreparedConnection::NoEffects(verified) = prepared else {
        anyhow::bail!("healthy route requested settlement")
    };
    assert_eq!(verified.selection.active, identity);
    assert_eq!(verified.approval, preflight.approval);
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn metadata_preflight_cannot_qualify_a_corrupt_payload_for_connection_effects() -> Result<()> {
    let (_owner, root, identity) = selected_connection_fixture()?;
    fs::write(
        root.join("runtimes").join(&identity).join("LICENSE"),
        b"changed ancillary fixture",
    )?;
    let preflight = selected_preflight(&root)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let mut full_passes = 0;
    let result = prepare_connection_with(&authority, &preflight, |authority| {
        full_passes += 1;
        selected_verified(authority)
    });
    assert!(result.is_err());
    assert_eq!(full_passes, 1);
    assert!(!root.join("bootstrap.json").exists());
    assert!(!root.join(format!("startup-{identity}.json")).exists());
    Ok(())
}

#[test]
fn preflight_scope_from_another_install_never_reaches_full_verification() -> Result<()> {
    let (_owner_a, root_a, _) = selected_connection_fixture()?;
    let (_owner_b, root_b, _) = selected_connection_fixture()?;
    let preflight = selected_preflight(&root_b)?;
    let lock = install_lock(&root_a)?;
    let authority = InstallerAuthority {
        root: &root_a,
        _lock: &lock,
    };
    let mut full_passes = 0;
    let result = prepare_connection_with(&authority, &preflight, |authority| {
        full_passes += 1;
        selected_verified(authority)
    });
    assert!(result.is_err());
    assert_eq!(
        full_passes, 0,
        "root/control scope is not just matching payload bytes"
    );
    Ok(())
}

#[test]
fn same_selected_identity_with_pending_lifecycle_never_retains_its_proof() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::RequestKey,
        service_lifecycle::{LifecycleAction, LifecycleRequest},
    };
    for action in [LifecycleAction::Stop, LifecycleAction::Restart] {
        let (_owner, root, identity) = selected_connection_fixture()?;
        let preflight = selected_preflight(&root)?;
        let lock = install_lock(&root)?;
        let control = layout::control(&root)?;
        local_ipc::prepare_root(&control)?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let (namespace, epoch) = registry.saved_request_epoch()?;
        registry.begin_lifecycle(&LifecycleRequest {
            caller: Incarnation::from_bytes([21; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([22; 16]),
            },
            action,
            expected_instance: Incarnation::from_bytes([23; 16]),
            runtime: identity.parse()?,
            manager: false,
        })?;
        drop(registry);
        let authority = InstallerAuthority {
            root: &root,
            _lock: &lock,
        };
        assert!(matches!(
            prepare_connection(&authority, &preflight)?,
            PreparedConnection::SettlementRequired
        ));
        assert_eq!(selection_record(&root)?.active, identity);
        assert!(
            !root.join("bootstrap.json").exists(),
            "preparation must not resume the pending action"
        );
    }
    Ok(())
}

#[test]
fn pending_activation_and_uncertain_registry_take_no_proof_to_settlement() -> Result<()> {
    let (_owner, root, identity) = selected_connection_fixture()?;
    let prior = "c".repeat(64);
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: Some(prior.clone()),
        },
    )?;
    let preflight = selected_preflight(&root)?;
    replace_json(
        &root,
        "activation.json",
        &serde_json::json!({
            "schema":1,"install":root,"control":preflight.control,"candidate":identity,
            "prior":{"schema":1,"active":prior,"previous":null},"manager":false,"phase":"probe"
        }),
    )?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    assert!(matches!(
        prepare_connection(&authority, &preflight)?,
        PreparedConnection::SettlementRequired
    ));
    fs::remove_file(root.join("activation.json"))?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    fs::write(
        control.join("service-1.4.sqlite3"),
        b"damaged saved registry fixture",
    )?;
    assert!(
        matches!(
            prepare_connection(&authority, &preflight)?,
            PreparedConnection::SettlementRequired
        ),
        "unreadable saved state is never a no-effect witness"
    );
    assert_eq!(
        fs::read(control.join("service-1.4.sqlite3"))?,
        b"damaged saved registry fixture"
    );
    Ok(())
}

#[tokio::test]
async fn ancillary_corruption_waits_for_lock_but_manifest_corruption_fails_pre_wait() -> Result<()>
{
    use std::future::Future;
    let (_owner, root, identity) = selected_connection_fixture()?;
    let payload = root.join("runtimes").join(&identity);
    fs::write(payload.join("LICENSE"), b"changed ancillary fixture")?;
    let lock = install_lock(&root)?;
    let mut future = Box::pin(installed_control(&root));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        future.as_mut().poll(&mut context).is_pending(),
        "authenticated metadata can now wait before ancillary corruption is inspected"
    );
    drop(future);
    // Cancelling the waiter did not release the other caller's actual lock.
    assert!(install_lock(&root).is_err());
    fs::write(
        payload.join("airk-install.json"),
        b"changed manifest metadata",
    )?;
    let mut future = Box::pin(installed_control(&root));
    let result = future.as_mut().poll(&mut context);
    assert!(
        matches!(result, std::task::Poll::Ready(Err(_))),
        "manifest authentication must fail before waiting"
    );
    drop(future);
    drop(lock);
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[tokio::test]
async fn incompatible_caller_is_rejected_pre_wait_without_connection_effects() -> Result<()> {
    use std::future::Future;
    let (_owner, root, old_identity) = selected_connection_fixture()?;
    let payload = root.join("runtimes").join(&old_identity);
    let mut pair: runtime_pair::RuntimePair =
        serde_json::from_slice(&fs::read(payload.join("airk-runtime.json"))?)?;
    pair.version = "0.0.1".into();
    fs::write(
        payload.join("airk-runtime.json"),
        serde_json::to_vec(&pair)?,
    )?;
    let mut inventory: InstallManifest =
        serde_json::from_slice(&fs::read(payload.join("airk-install.json"))?)?;
    inventory.version = pair.version.clone();
    inventory.files.insert(
        "airk-runtime.json".into(),
        runtime_pair::digest_file(&payload.join("airk-runtime.json"))?,
    );
    let identity = write_manifest(&payload, &inventory)?;
    replace_json(
        &payload,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: pair.version,
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(&payload, root.join("runtimes").join(&identity))?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity,
            previous: None,
        },
    )?;
    let lock = install_lock(&root)?;
    let mut future = Box::pin(installed_control(&root));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let result = future.as_mut().poll(&mut context);
    let std::task::Poll::Ready(Err(error)) = result else {
        anyhow::bail!("incompatible caller waited or succeeded")
    };
    assert!(
        error
            .to_string()
            .contains("matching runtime version required")
    );
    drop(future);
    drop(lock);
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn changed_selection_since_metadata_preflight_refuses_before_lifecycle_effects() -> Result<()> {
    let (_owner, root, first) = selected_connection_fixture()?;
    let preflight = selected_preflight(&root)?;
    let lock = install_lock(&root)?;
    let next = root.join("next-connection-fixture");
    local_ipc::prepare_root(&next)?;
    fixture(&next)?;
    fs::write(
        next.join("LICENSE"),
        b"new approved identity while caller waits",
    )?;
    let mut inventory: InstallManifest =
        serde_json::from_slice(&fs::read(next.join("airk-install.json"))?)?;
    inventory.files.insert(
        "LICENSE".into(),
        runtime_pair::digest_file(&next.join("LICENSE"))?,
    );
    let second = write_manifest(&next, &inventory)?;
    replace_json(
        &next,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: second.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(&next, root.join("runtimes").join(&second))?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: second,
            previous: Some(first),
        },
    )?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let mut full_passes = 0;
    let result = prepare_connection_with(&authority, &preflight, |authority| {
        full_passes += 1;
        selected_verified(authority)
    });
    let error = result
        .err()
        .context("changed selected identity was accepted")?;
    assert_eq!(
        full_passes, 1,
        "the CURRENT payload is actually qualified before comparison"
    );
    assert!(
        error
            .to_string()
            .contains("installed service changed; reconnect")
    );
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[tokio::test]
async fn policy_appearing_while_connection_waits_refuses_before_connection_effects() -> Result<()> {
    use std::future::Future;
    let (_owner, root, identity) = selected_connection_fixture()?;
    let provider = tempfile::tempdir()?;
    let source = provider.path().canonicalize()?;
    let lock = install_lock(&root)?;
    let mut future = Box::pin(installed_control(&root));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    let approval = selected_preflight(&root)?.approval;
    replace_json(
        &root,
        "managed-runtime.json",
        &serde_json::json!({
            "schema":1,"install":root.canonicalize()?,"source":source,
            "approval":approval,"executable_sha256":"a".repeat(64),"lifecycle":"external"
        }),
    )?;
    assert!(
        managed::read(&root)?.is_some(),
        "fixture is a valid policy, not malformed JSON refusal"
    );
    drop(lock);
    let result = tokio::time::timeout(Duration::from_secs(2), future).await?;
    let error = result.err().context("new managed authority was ignored")?;
    assert!(
        error
            .to_string()
            .contains("installed service changed; reconnect")
    );
    assert_eq!(selection_record(&root)?.active, identity);
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[tokio::test]
async fn valid_control_rebind_while_connection_waits_refuses_before_connection_effects()
-> Result<()> {
    use std::future::Future;
    let (_owner, root, identity) = selected_connection_fixture()?;
    let old_control = layout::saved_control(&root)?;
    let lock = install_lock(&root)?;
    let mut future = Box::pin(installed_control(&root));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    let state_owner = tempfile::tempdir()?;
    let state = state_owner.path().canonicalize()?;
    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("runtime-layout.json"))?)?;
    saved["state"] = serde_json::to_value(&state)?;
    replace_json(&state, "airk-state-1.4.owner.json", &saved)?;
    replace_json(&root, "runtime-layout.json", &saved)?;
    let rebound = layout::saved_control(&root)?;
    assert_eq!(rebound, state.join("control"));
    assert_ne!(
        rebound, old_control,
        "fixture supplies a valid changed saved scope"
    );
    drop(lock);
    let result = tokio::time::timeout(Duration::from_secs(2), future).await?;
    let error = result
        .err()
        .context("changed control authority was ignored")?;
    assert!(
        error
            .to_string()
            .contains("installed service changed; reconnect")
    );
    assert_eq!(selection_record(&root)?.active, identity);
    assert!(!root.join("bootstrap.json").exists());
    assert!(
        !rebound.exists(),
        "scope rejection must not initialize the new service"
    );
    Ok(())
}

#[test]
fn healthy_startup_reset_avoids_noops_without_tightening_damaged_receipts() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let identity = "a".repeat(64);
    let path = root.join(format!("startup-{identity}.json"));
    clear_startup_attempts(&root, &identity)?;
    assert!(!path.exists(), "an absent healthy circuit needs no receipt");
    let canonical = serde_json::to_vec(&StartupAttempts::default())?;
    fs::write(&path, &canonical)?;
    clear_startup_attempts(&root, &identity)?;
    assert_eq!(
        fs::read(&path)?,
        canonical,
        "canonical empty default stays unchanged"
    );
    let empty = br#"{ "attempts_unix_ms": [] }"#;
    fs::write(&path, empty)?;
    clear_startup_attempts(&root, &identity)?;
    assert_eq!(
        fs::read(&path)?,
        empty,
        "valid empty bytes remain unchanged"
    );
    let oversized = [vec![b' '; 4097], canonical.clone()].concat();
    fs::write(&path, &oversized)?;
    clear_startup_attempts(&root, &identity)?;
    assert_eq!(
        fs::read(&path)?,
        canonical,
        "oversized valid whitespace uses the old replacement path"
    );
    for original in [
        b"damaged receipt".as_slice(),
        br#"{"attempts_unix_ms":[1]}"#.as_slice(),
        br#"{"attempts_unix_ms":[1,2,3,4]}"#.as_slice(),
    ] {
        fs::write(&path, original)?;
        clear_startup_attempts(&root, &identity)?;
        let state: StartupAttempts = serde_json::from_slice(&fs::read(&path)?)?;
        assert!(state.attempts_unix_ms.is_empty());
    }
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(clear_startup_attempts(&root, &identity).is_err());
    assert!(path.is_dir(), "unsafe receipt topology remains preserved");
    Ok(())
}

#[test]
#[cfg(any(unix, windows))]
#[cfg_attr(
    windows,
    ignore = "requires Windows symbolic-link creation rights; run explicitly when provisioned"
)]
fn healthy_startup_reset_does_not_treat_a_dangling_link_as_absent() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let identity = "b".repeat(64);
    let path = root.join(format!("startup-{identity}.json"));
    let missing = root.join("missing-target");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&missing, &path)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&missing, &path)?;
    assert!(!path.try_exists()?);
    assert!(fs::symlink_metadata(&path)?.file_type().is_symlink());
    // replace_json previously replaced dangling links rather than retaining them.
    clear_startup_attempts(&root, &identity)?;
    assert!(fs::symlink_metadata(&path)?.is_file());
    assert!(!fs::symlink_metadata(&path)?.file_type().is_symlink());
    assert!(!missing.exists());
    let state: StartupAttempts = serde_json::from_slice(&fs::read(&path)?)?;
    assert!(state.attempts_unix_ms.is_empty());
    Ok(())
}

#[tokio::test]
async fn closed_saved_restart_completion_cannot_admit_an_independent_start() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::RequestKey,
        service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRequest},
    };
    let (_owner, root, identity) = selected_connection_fixture()?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let record = registry.begin_lifecycle(&LifecycleRequest {
        caller: Incarnation::from_bytes([41; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([42; 16]),
        },
        action: LifecycleAction::Restart,
        expected_instance: Incarnation::from_bytes([43; 16]),
        runtime: identity.parse()?,
        manager: false,
    })?;
    let record = registry.advance_lifecycle(&record, LifecyclePhase::Starting)?;
    let cancel = setup::SetupCancellation::default();
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    // The saved operation's real receipt is completed, not abandoned on close.
    // Daemon restart itself is outside this fixture; only its owned completion
    // result is injected into the actual post-settlement adapter.
    let mut settlement = Box::pin(setup::settle_then_checkpoint(
        async {
            released.await?;
            registry.advance_lifecycle(&record, LifecyclePhase::Complete)?;
            Ok(())
        },
        Some(&cancel),
    ));
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(settlement.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    cancel.close();
    release.send(()).expect("saved completion retained");
    assert!(settlement.await.is_err());
    assert!(registry.pending_lifecycle()?.is_none());
    assert!(!registry.service_explicitly_stopped()?);
    assert!(!root.join(format!("startup-{identity}.json")).exists());
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[tokio::test]
async fn saved_stop_still_completes_after_close_and_remains_stopped() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::RequestKey,
        service_lifecycle::{LifecycleAction, LifecycleRequest},
    };
    let (_owner, root, identity) = selected_connection_fixture()?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    registry.begin_lifecycle(&LifecycleRequest {
        caller: Incarnation::from_bytes([51; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([52; 16]),
        },
        action: LifecycleAction::Stop,
        expected_instance: Incarnation::from_bytes([53; 16]),
        runtime: identity.parse()?,
        manager: false,
    })?;
    drop(registry);
    let lock = install_lock(&root)?;
    let _authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let cancel = setup::SetupCancellation::default();
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let mut settlement = Box::pin(setup::settle_then_checkpoint(
        async {
            released.await?;
            lifecycle::settle(&root).await
        },
        Some(&cancel),
    ));
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(settlement.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    cancel.close();
    release.send(()).expect("saved STOP retained");
    let error = settlement
        .await
        .err()
        .context("STOP must retain its stopped barrier")?;
    assert!(format!("{error:#}").contains("service_explicitly_stopped"));
    let registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    assert!(registry.pending_lifecycle()?.is_none());
    assert!(registry.service_explicitly_stopped()?);
    assert!(!root.join(format!("startup-{identity}.json")).exists());
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn frontend_metadata_defers_missing_payload_to_locked_native_verification() -> Result<()> {
    let (_owner, root, identity) = selected_connection_fixture()?;
    let control = layout::control(&root)?;
    let payload = root.join("runtimes").join(&identity);
    fs::remove_file(payload.join(main_name()))?;
    fs::remove_file(payload.join(helper_name()))?;
    fs::write(payload.join("LICENSE"), b"changed inert ancillary fixture")?;
    assert_eq!(installed_metadata_control(&root)?, control);
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    assert!(selected_verified(&authority).is_err());
    assert!(!root.join("bootstrap.json").exists());
    assert!(!root.join(format!("startup-{identity}.json")).exists());
    Ok(())
}

#[test]
fn frontend_metadata_still_authenticates_pair_and_selection() -> Result<()> {
    let (_owner, root, identity) = selected_connection_fixture()?;
    let pair = root
        .join("runtimes")
        .join(&identity)
        .join("airk-runtime.json");
    let original = fs::read(&pair)?;
    fs::write(&pair, b"{}")?;
    assert!(installed_metadata_control(&root).is_err());
    fs::write(&pair, original)?;
    assert_eq!(installed_metadata_control(&root)?, layout::control(&root)?);
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: "0".repeat(64),
            previous: None,
        },
    )?;
    assert!(installed_metadata_control(&root).is_err());
    Ok(())
}

#[test]
fn managed_frontend_metadata_authenticates_both_modes_without_reading_payload() -> Result<()> {
    for lifecycle in ["external", "user_on_demand"] {
        let (owner, root, identity) = selected_connection_fixture()?;
        let provider = owner.path().join("provider");
        fs::rename(root.join("runtimes").join(&identity), &provider)?;
        let provider = provider.canonicalize()?;
        let approval: serde_json::Value =
            serde_json::from_slice(&fs::read(provider.join("approval.json"))?)?;
        let executable = runtime_pair::digest_file(&provider.join(main_name()))?;
        let mut policy = serde_json::json!({
            "schema": 1, "install": root.canonicalize()?, "source": provider,
            "approval": approval, "executable_sha256": executable, "lifecycle": lifecycle,
        });
        replace_json(&root, "managed-runtime.json", &policy)?;
        fs::remove_file(root.join("selected.json"))?;
        fs::remove_file(provider.join("approval.json"))?;
        let managed = managed::read(&root)?.expect("managed policy");
        assert_eq!(installed_metadata_control(&root)?, layout::control(&root)?);
        managed
            .verify()
            .context("valid external provider must qualify before tampering")?;
        let license = fs::read(provider.join("LICENSE"))?;
        fs::write(provider.join("LICENSE"), b"inert changed fixture")?;
        assert_eq!(installed_metadata_control(&root)?, layout::control(&root)?);
        let ancillary_error = managed.verify().unwrap_err();
        assert!(format!("{ancillary_error:#}").contains("LICENSE"));
        fs::write(provider.join("LICENSE"), license)?;
        managed.verify().context("restored provider must qualify")?;
        fs::remove_file(provider.join(main_name()))?;
        fs::remove_file(provider.join(helper_name()))?;
        assert_eq!(installed_metadata_control(&root)?, layout::control(&root)?);
        let missing_error = managed.verify().unwrap_err();
        assert_ne!(format!("{missing_error:#}"), format!("{ancillary_error:#}"));
        assert!(!root.join("bootstrap.json").exists());
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        policy["executable_sha256"] = "0".repeat(64).into();
        replace_json(&root, "managed-runtime.json", &policy)?;
        assert!(
            installed_metadata_control(&root).is_err(),
            "metadata must authenticate executable pin"
        );
        policy["executable_sha256"] = executable.into();
        replace_json(&root, "managed-runtime.json", &policy)?;
        fs::write(provider.join("airk-runtime.json"), b"{}")?;
        assert!(
            installed_metadata_control(&root).is_err(),
            "pair authentication remains mandatory"
        );
    }
    Ok(())
}
