// SPDX-License-Identifier: Apache-2.0
//! Resumable, inventory-proven removal. Never recursively deletes an installation or indexes.
use super::*;
use std::collections::BTreeSet;

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

const PREVIOUS_RETIREMENT: &str = "previous-retirement.json";
mod superseded;
pub(super) fn superseded_images(root: &Path, active: &str) -> Result<Vec<serde_json::Value>> {
    superseded::inspect_images(root, active)
}
pub(super) fn absent_optional_process_aliases(
    root: &Path,
    active: &str,
) -> Result<Vec<legacy_launchers::Image>> {
    superseded::absent_optional_aliases(root, active)
}

#[cfg(test)]
mod previous_tests {
    use super::*;

    #[test]
    fn uninstall_inventory_binds_current_main_and_historical_roles_and_refuses_modified_images()
    -> Result<()> {
        for historical in [false, true] {
            let (_owner, root, identity) = if historical {
                super::super::tests::unselected_historical_fixture()?
            } else {
                super::super::tests::unselected_fixture()?
            };
            let directory = root.join("runtimes").join(&identity);
            let main = directory.join(main_name());
            airk_host::process_roles::install(&main)?;
            let paths = uninstall_process_paths(&root, std::slice::from_ref(&identity))?;
            let expected = if historical {
                if cfg!(target_os = "macos") { 3 } else { 5 }
            } else {
                1
            };
            assert_eq!(paths.len(), expected);
            assert!(paths.contains(&main));
            if historical {
                assert!(paths.contains(&directory.join(helper_name())));
                if cfg!(target_os = "macos") {
                    assert!(paths.contains(&directory.join(MAC_LAUNCHER_IMAGE)));
                } else {
                    for role in [
                        airk_host::process_roles::Role::Service,
                        airk_host::process_roles::Role::Index,
                        airk_host::process_roles::Role::Source,
                    ] {
                        assert!(paths.contains(&directory.join(role.name())));
                    }
                }
            } else {
                assert!(!directory.join(helper_name()).exists());
                assert!(!launcher_path(&root).exists());
            }
            // An unapproved role copy is refused for current main-only layouts;
            // an altered owned alias is refused for historical pair layouts.
            let changed = directory.join(airk_host::process_roles::Role::Index.name());
            fs::write(&changed, b"modified unapproved process image")?;
            assert!(uninstall_process_paths(&root, &[identity]).is_err());
            assert_eq!(fs::read(changed)?, b"modified unapproved process image");
        }
        Ok(())
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn uninstall_stops_owned_clients_and_prevents_relaunch_without_stopping_foreign_processes()
    -> Result<()> {
        // These are process-path ownership component fixtures. The historical
        // receipt uses current test-executable bytes, not a predecessor build.
        for historical in [false, true] {
            let (owner, root, old) = if historical {
                super::super::tests::unselected_historical_fixture()?
            } else {
                super::super::tests::unselected_fixture()?
            };
            let directory = root.join("runtimes").join(&old);
            let main = directory.join(main_name());
            fs::copy(std::env::current_exe()?, &main)?;
            let hash = runtime_pair::digest_file(&main)?;
            let pair_path = directory.join("airk-runtime.json");
            let mut pair: runtime_pair::RuntimePair =
                serde_json::from_slice(&fs::read(&pair_path)?)?;
            pair.airk_sha256 = hash.clone();
            fs::write(&pair_path, serde_json::to_vec(&pair)?)?;
            let mut inventory: InstallManifest =
                serde_json::from_slice(&fs::read(directory.join("airk-install.json"))?)?;
            inventory.files.insert(main_name().into(), hash.clone());
            if historical && cfg!(target_os = "macos") {
                // A sealed Mac predecessor has no role aliases. Its standalone
                // launcher is a separate inventoried image, so make that fixture
                // runnable and seal its actual test-executable bytes as well.
                fs::copy(&main, directory.join(MAC_LAUNCHER_IMAGE))?;
                inventory
                    .files
                    .insert(MAC_LAUNCHER_IMAGE.into(), hash.clone());
            }
            inventory.files.insert(
                "airk-runtime.json".into(),
                runtime_pair::digest_file(&pair_path)?,
            );
            let bytes = serde_json::to_vec(&inventory)?;
            let identity = format!("{:x}", Sha256::digest(&bytes));
            fs::write(directory.join("airk-install.json"), bytes)?;
            replace_json(
                &directory,
                "approval.json",
                &Approval {
                    schema: 1,
                    manifest_sha256: identity.clone(),
                    version: inventory.version.clone(),
                    verification_basis: VerificationBasis::OrganizationDigest,
                },
            )?;
            let installed = root.join("runtimes").join(&identity);
            fs::rename(directory, &installed)?;
            let main = installed.join(main_name());
            let index = if historical && !cfg!(target_os = "macos") {
                airk_host::process_roles::install(&main)?;
                installed.join(airk_host::process_roles::Role::Index.name())
            } else {
                main.clone()
            };
            let launcher = if historical {
                let launcher = launcher_path(&root);
                fs::copy(&main, &launcher)?;
                replace_json(
                    &root,
                    "launcher.json",
                    &LauncherReceipt {
                        schema: 1,
                        sha256: hash,
                    },
                )?;
                launcher
            } else {
                assert!(!launcher_path(&root).exists());
                main.clone()
            };
            let foreign = owner.path().join(if cfg!(windows) {
                "foreign-airk.exe"
            } else {
                "foreign-airk"
            });
            fs::copy(&main, &foreign)?;
            let spawn = |image: &Path| -> Result<airk_host::native_process::ContainedChild> {
                let mut command = Command::new(image);
                command
                    .args(["--exact", "service_manager::tests::manager_command_fixture"])
                    .env("AIRK_MANAGER_COMMAND_FIXTURE", "blocked-input")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                airk_host::native_process::ContainedChild::spawn(&mut command)
            };
            let mut client = spawn(&main)?;
            let mut worker = spawn(&index)?;
            let mut forwarding = spawn(&launcher)?;
            let mut unrelated = spawn(&foreign)?;
            assert!(client.try_wait()?.is_none() && worker.try_wait()?.is_none());
            let result = uninstall(&root).await?;
            assert_eq!(result["uninstalled"], true);
            assert!(
                client.try_wait()?.is_some()
                    && worker.try_wait()?.is_some()
                    && forwarding.try_wait()?.is_some()
            );
            assert!(unrelated.try_wait()?.is_none());
            assert!(!launcher.exists() && !main.exists() && !index.exists());
            assert!(Command::new(&launcher).spawn().is_err());
            assert!(selected(&root).is_err());
            assert_eq!(uninstall(&root).await?["uninstalled"], true);
            unrelated.kill()?;
            unrelated.wait()?;
        }
        Ok(())
    }

    fn fixture() -> Result<(tempfile::TempDir, PathBuf, String, String)> {
        let (owner, root, old) = super::super::tests::unselected_fixture()?;
        let active = "a".repeat(64);
        local_ipc::prepare_root(&root.join("runtimes").join(&active))?;
        fs::write(
            root.join("runtimes").join(&active).join("keep"),
            b"selected",
        )?;
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: active.clone(),
                previous: Some(old.clone()),
            },
        )?;
        Ok((owner, root, old, active))
    }

    #[test]
    fn previous_retirement_clears_rollback_removes_payload_and_preserves_active_and_data()
    -> Result<()> {
        let (_owner, root, old, active) = fixture()?;
        fs::write(root.join("saved-settings"), b"preserve")?;
        assert!(previous_intent(&root, &"b".repeat(64)).is_err());
        let intent = previous_intent(&root, &active)?.context("missing retirement")?;
        assert!(require_previous_settled(&root).is_err());
        finish_previous(&root, &intent)?;
        assert!(!root.join("runtimes").join(old).exists());
        assert!(selection_record(&root)?.previous.is_none());
        assert_eq!(
            fs::read(root.join("runtimes").join(&active).join("keep"))?,
            b"selected"
        );
        assert_eq!(fs::read(root.join("saved-settings"))?, b"preserve");
        require_previous_settled(&root)?;
        assert!(previous_intent(&root, &active)?.is_none());
        Ok(())
    }

    #[test]
    fn previous_retirement_preserves_unknown_files_and_resumes_after_rollback_is_cleared()
    -> Result<()> {
        let (_owner, root, old, active) = fixture()?;
        let unknown = root.join("runtimes").join(&old).join("unknown.dll");
        fs::write(&unknown, b"preserve unknown")?;
        let intent = previous_intent(&root, &active)?.context("missing retirement")?;
        assert!(finish_previous(&root, &intent).is_err());
        assert!(selection_record(&root)?.previous.is_none());
        assert!(root.join("runtimes").join(&old).join(main_name()).is_file());
        assert_eq!(fs::read(&unknown)?, b"preserve unknown");
        fs::remove_file(unknown)?;
        let resumed = previous_intent(&root, &active)?.context("lost retirement")?;
        // Simulate death after payload retirement but before the outer receipt was removed.
        remove(&root, &old)?;
        finish_previous(&root, &resumed)?;
        require_previous_settled(&root)?;
        Ok(())
    }

    #[test]
    fn previous_retirement_refuses_foreign_receipts_and_changed_selection() -> Result<()> {
        let (_owner, root, old, active) = fixture()?;
        let mut intent = previous_intent(&root, &active)?.context("missing retirement")?;
        intent.install = root.join("other");
        replace_json(&root, PREVIOUS_RETIREMENT, &intent)?;
        assert!(previous_intent(&root, &active).is_err());
        assert!(root.join("runtimes").join(old).join(main_name()).is_file());
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviousRetirement {
    schema: u32,
    install: PathBuf,
    active: String,
    previous: String,
}

pub(super) fn require_previous_settled(root: &Path) -> Result<()> {
    superseded::require_settled(root)?;
    ensure!(
        !present(&root.join(PREVIOUS_RETIREMENT))?,
        "previous runtime retirement is pending; resume retire-previous before another activation"
    );
    Ok(())
}

fn previous_intent(root: &Path, expected: &str) -> Result<Option<PreviousRetirement>> {
    digest(expected)?;
    let selected = selection_record(root)?;
    ensure!(
        selected.active == expected,
        "runtime selection changed before retirement"
    );
    let path = root.join(PREVIOUS_RETIREMENT);
    if present(&path)? {
        let intent: PreviousRetirement = serde_json::from_slice(&read_bounded(&path, 16384)?)?;
        digest(&intent.previous)?;
        ensure!(
            intent.schema == 1
                && intent.install == root.canonicalize()?
                && intent.active == expected
                && intent.previous != expected
                && selected
                    .previous
                    .as_ref()
                    .is_none_or(|previous| previous == &intent.previous),
            "previous runtime retirement ownership or selection changed"
        );
        return Ok(Some(intent));
    }
    let Some(previous) = selected.previous else {
        return Ok(None);
    };
    digest(&previous)?;
    ensure!(previous != expected, "active runtime cannot be retired");
    // Preserve inventory before clearing rollback, including for interrupted deletion.
    validated_inventory(root, &previous)?;
    let intent = PreviousRetirement {
        schema: 1,
        install: root.canonicalize()?,
        active: expected.into(),
        previous,
    };
    replace_json(root, PREVIOUS_RETIREMENT, &intent)?;
    Ok(Some(intent))
}

fn finish_previous(root: &Path, intent: &PreviousRetirement) -> Result<()> {
    let selected = selection_record(root)?;
    ensure!(
        selected.active == intent.active
            && selected
                .previous
                .as_ref()
                .is_none_or(|previous| previous == &intent.previous),
        "runtime selection changed during retirement"
    );
    replace_json(
        root,
        "selected.json",
        &Selection {
            schema: 1,
            active: intent.active.clone(),
            previous: None,
        },
    )?;
    // The existing inventory-proven remover preserves unknown/changed files and resumes partial deletion.
    if present(&root.join("runtimes").join(&intent.previous))?
        || present(&root.join(format!("retire-{}.json", intent.previous)))?
    {
        remove(root, &intent.previous)?;
    }
    fs::remove_file(root.join(PREVIOUS_RETIREMENT))?;
    sync_directory(root)
}

pub async fn retire_previous(root: &Path, expected: &str) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = install_lock(root)?;
    activation::rebuild::require_retirement_allowed(root)?;
    require_issuer_rotation_settled(root)?;
    managed::require_unmanaged(root)?;
    activation::require_settled(root)?;
    payload_repair::require_settled(root)?;
    lifecycle::require_settled(root)?;
    let (selected_main, _) = verify_runtime(root, expected)?;
    if runtime_pair::single_executable(&selected_main)? {
        return superseded::retire(root, expected);
    }
    lifecycle::require_start_allowed(root)?;
    let Some(intent) = previous_intent(root, expected)? else {
        return Ok(serde_json::json!({"active":expected,"retired_previous":null}));
    };
    activation::checkpoint_retiring(root, expected)?;
    let (main, approval) = verify_runtime(root, expected)?;
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    let owned_manager = manager.owns_restart()?;
    let was_healthy = health(root, &approval, &runtime_pair::digest_file(&main)?)
        .await
        .is_ok();
    if owned_manager {
        manager.stop()?;
    }
    upgrade_stop::stop(root, expected)?;
    drain(root, &main, &approval).await?;
    manager.require_stopped_for_maintenance()?;
    let cleanup = {
        let _quiescence = quiescent(root)?;
        finish_previous(root, &intent)
    };
    // Restore the healthy replacement even when locked/unknown old files defer cleanup.
    let restarted = if owned_manager {
        activation::start_selected(root, &approval, &main, Some(true)).await
    } else {
        // An exact healthy instance was deliberately stopped by this installer.
        // Its controlled restart is not another failed on-demand startup attempt.
        start(root, &approval, &main, !was_healthy).await
    };
    restarted
        .context("replacement runtime remains selected but restart after retirement is pending")?;
    cleanup.context("previous runtime cleanup is incomplete; retry retire-previous")?;
    Ok(serde_json::json!({"active":expected,"retired_previous":intent.previous}))
}

/// Synchronous modern retirement under the reset continuation's existing lock.
/// It uses the same inventory/process owner and complete receipt as the CLI.
pub(super) fn clean_break_retirement_pending(root: &Path) -> Result<bool> {
    superseded::pending(root)
}

pub(super) fn retire_clean_break_locked_recording(
    root: &Path,
    expected: &str,
    record: &mut dyn FnMut(&serde_json::Value) -> Result<()>,
) -> Result<serde_json::Value> {
    activation::rebuild::require_retirement_allowed(root)?;
    require_issuer_rotation_settled(root)?;
    managed::require_unmanaged(root)?;
    activation::require_settled(root)?;
    payload_repair::require_settled(root)?;
    lifecycle::require_settled(root)?;
    let (main, _) = verify_runtime(root, expected)?;
    ensure!(
        runtime_pair::single_executable(&main)?,
        "clean break requires a single executable"
    );
    superseded::retire_recording(root, expected, record)
}

fn validate_startup(root: &Path, identity: &str) -> Result<()> {
    let path = root.join(format!("startup-{identity}.json"));
    if present(&path)? {
        let attempts: StartupAttempts = serde_json::from_slice(&read_bounded(&path, 4096)?)?;
        ensure!(
            attempts.attempts_unix_ms.len() <= 3,
            "changed startup receipt preserved"
        );
    }
    Ok(())
}

fn finish(root: &Path, identity: &str) -> Result<()> {
    validate_startup(root, identity)?;
    let startup = root.join(format!("startup-{identity}.json"));
    if present(&startup)? {
        fs::remove_file(startup)?;
        sync_directory(root)?;
    }
    fs::remove_file(root.join(format!("retire-{identity}.json")))?;
    sync_directory(root)
}

fn validated_inventory(root: &Path, identity: &str) -> Result<InstallManifest> {
    digest(identity)?;
    let intent = root.join(format!("retire-{identity}.json"));
    if !present(&intent)? {
        let (main, _, _) = owned_inventory(root, identity)?;
        let directory = runtime_pair::payload_root(&main)?;
        let bytes = read_bounded(&directory.join("airk-install.json"), MAX_MANIFEST)?;
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&intent)?;
        sync_directory(root)?;
    }
    let bytes = read_bounded(&intent, MAX_MANIFEST)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == identity,
        "runtime retirement receipt changed"
    );
    let inventory: InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        inventory.schema == 1 && inventory.files.len() <= MAX_FILES,
        "invalid runtime retirement receipt"
    );
    for (name, hash) in &inventory.files {
        payload_path(root, name)?;
        digest(hash)?;
    }
    Ok(inventory)
}

pub fn remove(root: &Path, identity: &str) -> Result<()> {
    ensure!(
        !activation::rebuild::preserved_runtime(root, identity)?,
        "rebuild predecessor is preserved as recovery evidence; use separately reviewed cleanup"
    );
    remove_with(root, identity, true)
}

fn preflight_remove(root: &Path, identity: &str) -> Result<()> {
    remove_with(root, identity, false)
}

fn remove_with(root: &Path, identity: &str, perform: bool) -> Result<()> {
    require_issuer_rotation_settled(root)?;
    let mut inventory = validated_inventory(root, identity)?;
    airk_host::process_roles::inventory(&mut inventory.files);
    // Preflight ancillary receipts before deleting any payload bytes. This also
    // runs when a crash already removed the last runtime directory entry.
    validate_startup(root, identity)?;
    let directory = root.join("runtimes").join(identity);
    if !present(&directory)? {
        return if perform {
            finish(root, identity)
        } else {
            Ok(())
        };
    }
    local_ipc::prepare_root(&directory)?;
    let mut allowed_files: BTreeSet<_> = inventory.files.keys().cloned().collect();
    allowed_files.extend(["approval.json".into(), "airk-install.json".into()]);
    let mut directories = BTreeSet::new();
    for file in &allowed_files {
        let mut parent = Path::new(file).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    let mut stack = vec![directory.clone()];
    let mut count = 0;
    while let Some(parent) = stack.pop() {
        for entry in fs::read_dir(parent)? {
            count += 1;
            ensure!(count <= 4096, "runtime removal inventory exceeds bound");
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(&directory)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                ensure!(
                    directories.contains(relative),
                    "unknown directory preserved during runtime removal"
                );
                // Reparse/ownership checks precede descent, including on Windows.
                local_ipc::prepare_root(&path)?;
                stack.push(path);
            } else {
                safe_file(&path)?;
                let name = relative
                    .to_str()
                    .context("non-UTF-8 runtime entry")?
                    .replace('\\', "/");
                ensure!(
                    allowed_files.contains(&name),
                    "unknown file preserved during runtime removal"
                );
                if let Some(expected) = inventory.files.get(&name) {
                    ensure!(
                        runtime_pair::digest_file(&path)? == *expected,
                        "changed runtime file preserved during removal"
                    );
                } else {
                    let bytes = read_bounded(&path, MAX_MANIFEST)?;
                    if name == "airk-install.json" {
                        ensure!(
                            format!("{:x}", Sha256::digest(bytes)) == identity,
                            "changed runtime inventory preserved"
                        );
                    } else {
                        let approval: Approval = serde_json::from_slice(&bytes)?;
                        ensure!(
                            approval.schema == 1
                                && approval.manifest_sha256 == identity
                                && approval.version == inventory.version,
                            "foreign runtime approval preserved"
                        );
                    }
                }
            }
        }
    }
    if !perform {
        return Ok(());
    }
    // Executing Windows images fail before legal/metadata files are touched.
    let mut names = vec![main_name(), helper_name()];
    names.extend(
        inventory
            .files
            .keys()
            .map(String::as_str)
            .filter(|name| *name != main_name() && *name != helper_name()),
    );
    for name in names {
        let file = payload_path(&directory, name)?;
        if present(&file)? {
            safe_file(&file)?;
            ensure!(
                runtime_pair::digest_file(&file)? == inventory.files[name],
                "changed runtime file preserved during removal"
            );
            fs::remove_file(file)?;
        }
    }
    let mut directories: Vec<_> = directories.into_iter().collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in directories {
        let path = directory.join(relative);
        if present(&path)? {
            fs::remove_dir(path)?;
        }
    }
    for name in ["airk-install.json", "approval.json"] {
        let path = directory.join(name);
        if present(&path)? {
            safe_file(&path)?;
            let bytes = read_bounded(&path, MAX_MANIFEST)?;
            if name == "airk-install.json" {
                ensure!(
                    format!("{:x}", Sha256::digest(bytes)) == identity,
                    "changed runtime inventory preserved"
                );
            } else {
                let approval: Approval = serde_json::from_slice(&bytes)?;
                ensure!(
                    approval.schema == 1
                        && approval.manifest_sha256 == identity
                        && approval.version == inventory.version,
                    "foreign runtime approval preserved"
                );
            }
            fs::remove_file(path)?;
        }
    }
    fs::remove_dir(&directory)?;
    sync_directory(&root.join("runtimes"))?;
    finish(root, identity)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UninstallIntent {
    schema: u32,
    runtimes: Vec<String>,
}

fn uninstall_process_paths(root: &Path, runtimes: &[String]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for identity in runtimes {
        let directory = root.join("runtimes").join(identity);
        if !present(&directory)? {
            continue;
        }
        let mut inventory = validated_inventory(root, identity)?;
        airk_host::process_roles::inventory(&mut inventory.files);
        for name in [
            main_name(),
            helper_name(),
            MAC_LAUNCHER_IMAGE,
            airk_host::process_roles::Role::Service.name(),
            airk_host::process_roles::Role::Index.name(),
            airk_host::process_roles::Role::Source.name(),
        ] {
            let image = directory.join(name);
            if !present(&image)? {
                continue;
            }
            safe_file(&image)?;
            ensure!(
                inventory.files.get(name) == Some(&runtime_pair::digest_file(&image)?),
                "modified uninstall process image preserved"
            );
            paths.push(image);
        }
    }
    if present(&launcher_path(root))? {
        paths.push(verified_launcher(root)?);
    }
    let (_, channel) = airk_host::install_layout::retained_parameters(root)?;
    if let Some(universal) = crate::universal_launcher::current_for(root, channel)? {
        safe_file(&universal)?;
        paths.push(universal);
    }
    Ok(paths)
}

pub async fn uninstall(root: &Path) -> Result<serde_json::Value> {
    require_issuer_rotation_settled(root)?;
    if managed::is_configured(root)? {
        return managed::detach(root).await;
    }
    activation::require_settled(root)?;
    let preserved_payload = payload_repair::preserved(root)?;
    let intent_path = root.join("uninstall.json");
    let intent: UninstallIntent = if present(&intent_path)? {
        serde_json::from_slice(&read_bounded(&intent_path, 4096)?)?
    } else {
        let _unselected = if present(&root.join("selected.json"))? {
            let selection = selection_record(root)?;
            owned_inventory(root, &selection.active)?;
            None
        } else {
            // An interrupted first install has no authenticated process identity.
            // Never infer ownership from a PID or stop a live unknown process.
            Some(quiescent(root).context("unselected installation still has live owners")?)
        };
        launcher_repair::settle_for_uninstall(root)?;
        for identity in staging::pending(root)? {
            staging::abort(root, &identity)?;
        }
        let mut runtimes = Vec::new();
        let directory = root.join("runtimes");
        if present(&directory)? {
            local_ipc::prepare_root(&directory)?;
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                if preserved_payload.as_ref().is_some_and(|preserved| {
                    entry.file_name() == preserved.file_name().unwrap_or_default()
                        && entry.path().canonicalize().ok().as_ref() == Some(preserved)
                }) {
                    continue;
                }
                let identity = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("unknown runtime directory preserved"))?;
                digest(&identity)?;
                owned_inventory(root, &identity)?;
                runtimes.push(identity);
            }
        }
        let intent = UninstallIntent {
            schema: 1,
            runtimes,
        };
        replace_json(root, "uninstall.json", &intent)?;
        intent
    };
    ensure!(intent.schema == 1, "invalid runtime uninstall intent");
    // Persist uninstall intent before stopping clients: a racing launcher must
    // refuse another service start while the verified images are being retired.
    let paths = uninstall_process_paths(root, &intent.runtimes)?;
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    let owned_manager = manager.owned_registration_exists()?;
    if owned_manager {
        manager.stop()?;
    }
    upgrade_stop::stop_paths(&paths)?;
    if owned_manager {
        stop_login(root, &manager, true).await?;
    }
    if present(&root.join("selected.json"))? {
        let selection = selection_record(root)?;
        let (main, approval, _) = owned_inventory(root, &selection.active)?;
        drain(root, &main, &approval).await?;
    }
    let (_, channel) = airk_host::install_layout::retained_parameters(root)?;
    crate::universal_launcher::retire_for(root, channel)?;
    let _quiescence = quiescent(root)
        .context("service/project/embedding ownership must release before runtime removal")?;
    airk_endpoint::http_endpoint::trust::remove_owned(&airk_host::install_layout::saved_control(
        root,
    )?)?;
    let selected = root.join("selected.json");
    if present(&selected)? {
        let selection: Selection = serde_json::from_slice(&read_bounded(&selected, 4096)?)?;
        ensure!(
            selection.schema == 1
                && intent.runtimes.contains(&selection.active)
                && selection
                    .previous
                    .as_ref()
                    .is_none_or(|prior| intent.runtimes.contains(prior)),
            "changed selection preserved during uninstall"
        );
        fs::remove_file(selected)?;
    }
    let mut deferred = Vec::new();
    for identity in &intent.runtimes {
        digest(identity)?;
        if (root.join("runtimes").join(identity).exists()
            || root.join(format!("retire-{identity}.json")).exists())
            && remove_with(root, identity, true).is_err()
        {
            deferred.push(identity.clone());
        }
    }
    if !deferred.is_empty() {
        return Ok(
            serde_json::json!({"uninstalled":false,"deferred_runtimes":deferred,"resume":"runtime uninstall","project_data_preserved":true}),
        );
    }
    let launcher = launcher_path(root);
    if launcher.try_exists()? {
        verified_launcher(root)?;
        if fs::remove_file(launcher).is_err() {
            return Ok(
                serde_json::json!({"uninstalled":false,"deferred_launcher":true,"resume":"runtime uninstall","project_data_preserved":true}),
            );
        }
    }
    let receipt = root.join("launcher.json");
    if receipt.try_exists()? {
        let receipt_value: LauncherReceipt =
            serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(
            receipt_value.schema == 1,
            "foreign launcher receipt preserved"
        );
        digest(&receipt_value.sha256)?;
        fs::remove_file(receipt)?;
    }
    legacy_launchers::finish_for_uninstall(root, &intent.runtimes)?;
    superseded::finish_for_uninstall(root, &intent.runtimes)?;
    fs::remove_file(intent_path)?;
    sync_directory(root)?;
    Ok(
        serde_json::json!({"uninstalled":true,"project_data_preserved":true,"service_metadata_preserved":true,"preserved_damaged_payload":preserved_payload}),
    )
}
