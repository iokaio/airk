// SPDX-License-Identifier: Apache-2.0
//! Explicit post-HTTPS-migration removal; activation never invokes this implicitly.
use super::*;

const INTENT: &str = "legacy-launcher-retirement.json";

pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        !present(&root.join(INTENT))?,
        "legacy_launcher_retirement_pending: resume retire-legacy-launchers before another activation"
    );
    Ok(())
}

pub(super) fn pending(root: &Path) -> Result<bool> {
    present(&root.join(INTENT))
}

pub(super) fn require_absent(root: &Path) -> Result<()> {
    require_settled(root)?;
    let (_, channel) = crate::install_layout::retained_parameters(root)?;
    ensure!(
        !present(&launcher_path(root))?
            && !present(&root.join("launcher.json"))?
            && crate::universal_launcher::retirement_image_for(root, channel)?.is_none(),
        "legacy_launchers_must_be_removed_after_configuration_migration"
    );
    Ok(())
}

pub(super) fn finish_for_uninstall(root: &Path, retired: &[String]) -> Result<()> {
    let path = root.join(INTENT);
    if !present(&path)? {
        return Ok(());
    }
    let saved: Intent = serde_json::from_slice(&read_bounded(&path, 16384)?)?;
    ensure!(
        saved.schema == 1
            && saved.install == root.canonicalize()?
            && retired.contains(&saved.active),
        "foreign_legacy_launcher_retirement_preserved_during_uninstall"
    );
    for hash in [&saved.launcher_sha256, &saved.receipt_sha256]
        .into_iter()
        .flatten()
    {
        digest(hash)?;
    }
    validate_roster(&saved)?;
    ensure!(
        !present(&launcher_path(root))? && !present(&root.join("launcher.json"))?,
        "legacy_launcher_uninstall_cleanup_incomplete"
    );
    fs::remove_file(path)?;
    sync_directory(root)
}

fn selected_https(root: &Path, expected: &str) -> Result<()> {
    digest(expected)?;
    ensure!(
        selection_record(root)?.active == expected,
        "legacy_launcher_retirement_selection_changed"
    );
    let (main, _) = verify_runtime(root, expected)?;
    ensure!(
        runtime_pair::single_executable(&main)?,
        "legacy_launcher_retirement_requires_https_runtime"
    );
    Ok(())
}

pub(crate) fn inspect(root: &Path, expected: &str) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = existing_install_lock(root)?;
    selected_https(root, expected)?;
    inspect_owned_locked(root, expected)
}

pub(crate) fn inspect_predecessor_cleanup(
    root: &Path,
    expected: &str,
) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = existing_install_lock(root)?;
    ensure!(
        selection_record(root)?.active == expected && state_contract::required(root)?,
        "predecessor launcher cleanup requires the exact selected prebaseline installation"
    );
    if let Some(metadata) = activation::rebuild::client_metadata(root)? {
        ensure!(
            metadata["predecessor"].as_str() == Some(expected),
            "saved predecessor launcher binding changed"
        );
        return Ok(metadata["legacy_launchers"].clone());
    }
    inspect_clean_break_locked(root, expected)
}

/// The owning reset already holds the installation lock. Historical approved
/// pair layouts are disposal metadata here, never permission to start them.
pub(super) fn inspect_clean_break_locked(root: &Path, expected: &str) -> Result<serde_json::Value> {
    ensure!(
        selection_record(root)?.active == expected,
        "clean-break predecessor selection changed"
    );
    verify_runtime(root, expected)?;
    inspect_owned_locked(root, expected)
}

fn inspect_owned_locked(root: &Path, expected: &str) -> Result<serde_json::Value> {
    let launcher = if present(&launcher_path(root))? {
        let path = verified_launcher(root)?;
        Some(serde_json::json!({"sha256":runtime_pair::digest_file(&path)?,"path":path}))
    } else {
        None
    };
    let (_, channel) = crate::install_layout::retained_parameters(root)?;
    let universal = crate::universal_launcher::retirement_image_for(root, channel)?
        .filter(|path| path.is_file());
    let universal = universal
        .map(|path| -> Result<serde_json::Value> {
            Ok(serde_json::json!({"sha256":runtime_pair::digest_file(&path)?,"path":path}))
        })
        .transpose()?;
    let runtime_images = retirement::superseded_images(root, expected)?;
    Ok(
        serde_json::json!({"schema":1,"active":expected,"launcher":launcher,"universal_launcher":universal,"legacy_runtime_images":runtime_images}),
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    install: PathBuf,
    active: String,
    launcher_sha256: Option<String>,
    receipt_sha256: Option<String>,
    #[serde(default)]
    images: Option<Vec<Image>>,
    #[serde(default)]
    image_exclusions: Vec<Image>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Image {
    pub(super) path: PathBuf,
    pub(super) sha256: String,
}

fn save(root: &Path, intent: &Intent) -> Result<()> {
    ensure!(
        serde_json::to_vec(intent)?.len() <= 16384,
        "legacy_launcher_retirement_metadata_limit"
    );
    replace_json(root, INTENT, intent)
}

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn prepare(root: &Path, expected: &str) -> Result<Intent> {
    let path = root.join(INTENT);
    if present(&path)? {
        let saved: Intent = serde_json::from_slice(&read_bounded(&path, 16384)?)?;
        ensure!(
            saved.schema == 1 && saved.install == root.canonicalize()? && saved.active == expected,
            "legacy_launcher_retirement_binding_changed"
        );
        for hash in [&saved.launcher_sha256, &saved.receipt_sha256]
            .into_iter()
            .flatten()
        {
            digest(hash)?;
        }
        verify_remaining(root, &saved)?;
        validate_roster(&saved)?;
        return Ok(saved);
    }
    let launcher = launcher_path(root);
    let receipt = root.join("launcher.json");
    let mut launcher_sha256 = if present(&launcher)? {
        Some(runtime_pair::digest_file(&verified_launcher(root)?)?)
    } else {
        None
    };
    let receipt_sha256 = if present(&receipt)? {
        let saved: LauncherReceipt = serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(saved.schema == 1, "foreign_launcher_receipt_preserved");
        digest(&saved.sha256)?;
        if let Some(actual) = &launcher_sha256 {
            ensure!(actual == &saved.sha256, "modified_launcher_preserved");
        } else {
            launcher_sha256 = Some(saved.sha256);
        }
        Some(runtime_pair::digest_file(&receipt)?)
    } else {
        None
    };
    let intent = Intent {
        schema: 1,
        install: root.canonicalize()?,
        active: expected.into(),
        launcher_sha256,
        receipt_sha256,
        images: None,
        image_exclusions: Vec::new(),
    };
    save(root, &intent)?;
    Ok(intent)
}

fn validate_roster(intent: &Intent) -> Result<()> {
    let images = intent.images.as_deref().unwrap_or(&[]);
    let (_, channel) = crate::install_layout::retained_parameters(&intent.install)?;
    let universal = crate::runtime_install::discover_user_paths(channel)?
        .launcher
        .join(crate::universal_launcher::launcher_name());
    ensure!(
        images.len() <= 2 && intent.image_exclusions.len() <= 2,
        "legacy_launcher_image_roster_limit"
    );
    for (index, image) in images.iter().enumerate() {
        digest(&image.sha256)?;
        ensure!(
            (image.path == launcher_path(&intent.install)
                && intent.launcher_sha256.as_ref() == Some(&image.sha256))
                || image.path == universal,
            "foreign_legacy_launcher_image_path"
        );
        ensure!(
            image.path.is_absolute()
                && images[..index].iter().all(|prior| prior.path != image.path),
            "invalid_legacy_launcher_image_roster"
        );
    }
    for (index, proof) in intent.image_exclusions.iter().enumerate() {
        ensure!(
            images.contains(proof) && !intent.image_exclusions[..index].contains(proof),
            "crossed_legacy_launcher_image_exclusion"
        );
    }
    Ok(())
}

fn bind_images(root: &Path, intent: &mut Intent, current: Vec<Image>) -> Result<Vec<Image>> {
    if let Some(saved) = &intent.images {
        ensure!(
            current.iter().all(|image| saved.contains(image)),
            "legacy_launcher_image_roster_changed"
        );
        for image in saved {
            if present(&image.path)? {
                ensure!(
                    current.contains(image),
                    "legacy_launcher_image_ownership_changed"
                );
                safe_file(&image.path)?;
                ensure!(
                    runtime_pair::digest_file(&image.path)? == image.sha256,
                    "changed_legacy_launcher_image_preserved"
                );
            }
        }
    } else {
        intent.images = Some(current);
        validate_roster(intent)?;
        save(root, intent)?;
    }
    validate_roster(intent)?;
    Ok(intent.images.clone().unwrap_or_default())
}

fn record_exclusions(root: &Path, intent: &mut Intent, exclusion: &ImageExclusion) -> Result<()> {
    // Call only while the write/image exclusion handles remain held, before any
    // unlink. The exact image roster, not a transferable boolean, survives death.
    for image in &exclusion.images {
        ensure!(
            intent.images.as_deref().unwrap_or(&[]).contains(image),
            "crossed_legacy_launcher_image_exclusion"
        );
        if !intent.image_exclusions.contains(image) {
            intent.image_exclusions.push(image.clone());
        }
    }
    validate_roster(intent)?;
    save(root, intent)
}

fn verify_remaining(root: &Path, intent: &Intent) -> Result<()> {
    for (path, expected) in [
        (launcher_path(root), &intent.launcher_sha256),
        (root.join("launcher.json"), &intent.receipt_sha256),
    ] {
        if present(&path)? {
            safe_file(&path)?;
            ensure!(
                expected.as_ref() == Some(&runtime_pair::digest_file(&path)?),
                "changed_legacy_launcher_state_preserved"
            );
        }
    }
    Ok(())
}

fn finish_local(root: &Path, intent: &Intent) -> Result<()> {
    verify_remaining(root, intent)?;
    let launcher = launcher_path(root);
    if present(&launcher)? {
        fs::remove_file(&launcher)?;
        sync_directory(root)?;
    }
    // Recheck after the first durable deletion; a changed receipt never inherits
    // the deleted image's authority, including on resumed partial cleanup.
    verify_remaining(root, intent)?;
    let receipt = root.join("launcher.json");
    if present(&receipt)? {
        fs::remove_file(receipt)?;
        sync_directory(root)?;
    }
    Ok(())
}

fn qualify_observation(
    image: &Image,
    audit: &crate::legacy_processes::Audit,
    exclusions: &[Image],
) -> Result<()> {
    ensure!(
        audit.failure.is_none() && audit.matching_executable_pids.is_empty(),
        "legacy_launcher_processes_remain: {}; close the old AIRK clients and retry retire-legacy-launchers; observation={}",
        image.path.display(),
        serde_json::to_string(&audit)?
    );
    ensure!(
        audit.complete || cfg!(windows) && exclusions.contains(image),
        "legacy_launcher_process_observation_incomplete: {}; close old clients and retry; observation={}",
        image.path.display(),
        serde_json::to_string(&audit)?
    );
    Ok(())
}

pub(super) fn observe(images: &[Image], exclusions: &[Image]) -> Result<Vec<serde_json::Value>> {
    images.iter().map(|image| {
        let audit = crate::legacy_processes::audit(&image.path);
        qualify_observation(image, &audit, exclusions)?;
        Ok(serde_json::json!({"path":image.path,"sha256":image.sha256,"held_image_exclusion":exclusions.contains(image),"observation":audit}))
    }).collect()
}

pub(super) fn stop_legacy(paths: &[PathBuf]) -> Result<()> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    upgrade_stop::stop_paths(paths)?;
    // macOS captures a task-name right and uses the audit-token signal API.
    // Older hosts without that capability refuse rather than using a bare PID.
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = paths;
    Ok(())
}

pub(super) struct ImageExclusion {
    _handles: Vec<File>,
    pub(super) images: Vec<Image>,
}

pub(super) fn image_exclusion(images: &[Image]) -> Result<ImageExclusion> {
    #[allow(unused_mut)]
    let mut handles = Vec::new();
    #[allow(unused_mut)]
    let mut proven = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        for image in images {
            let path = &image.path;
            if present(path)? {
                safe_file(path)?;
                let mut handle = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(1 | 4)
                    .open(path)
                    .context(
                        "legacy_launcher_image_in_use: close old clients and retry retirement",
                    )?;
                let mut hash = Sha256::new();
                let mut buffer = [0u8; 65536];
                let mut total = 0u64;
                loop {
                    let count = handle.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    total += count as u64;
                    ensure!(
                        total <= 2 * 1024 * 1024 * 1024,
                        "legacy_launcher_image_exclusion_size_limit"
                    );
                    hash.update(&buffer[..count]);
                }
                ensure!(
                    format!("{:x}", hash.finalize()) == image.sha256,
                    "legacy_launcher_image_changed_after_exclusion"
                );
                handles.push(handle);
                proven.push(image.clone());
            }
        }
    }
    #[cfg(not(windows))]
    let _ = images;
    Ok(ImageExclusion {
        _handles: handles,
        images: proven,
    })
}

pub(crate) async fn retire(root: &Path, expected: &str) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = install_lock(root)?;
    retire_locked(root, expected)
}

/// The native reset continuation already holds this installation's lock.
pub(super) fn retire_locked(root: &Path, expected: &str) -> Result<serde_json::Value> {
    retire_locked_recording(root, expected, &mut |_| Ok(()))
}

pub(super) fn retire_locked_recording(
    root: &Path,
    expected: &str,
    record: &mut dyn FnMut(&serde_json::Value) -> Result<()>,
) -> Result<serde_json::Value> {
    activation::rebuild::require_retirement_allowed(root)?;
    selected_https(root, expected)?;
    require_issuer_rotation_settled(root)?;
    managed::require_unmanaged(root)?;
    activation::require_settled(root)?;
    payload_repair::require_settled(root)?;
    launcher_repair::require_settled(root)?;
    lifecycle::require_settled(root)?;
    let (_, channel) = crate::install_layout::retained_parameters(root)?;
    let mut intent = prepare(root, expected)?;
    let mut current = Vec::new();
    if let Some(hash) = &intent.launcher_sha256 {
        current.push(Image {
            path: launcher_path(root),
            sha256: hash.clone(),
        });
    }
    if let Some(universal) = crate::universal_launcher::retirement_image_for(root, channel)? {
        let sha256 = crate::universal_launcher::retirement_digest_for(root, channel, &universal)?;
        current.push(Image {
            path: universal,
            sha256,
        });
    }
    let images = bind_images(root, &mut intent, current)?;
    let paths: Vec<_> = images.iter().map(|image| image.path.clone()).collect();
    // Exact absence is idempotent and must not restart a healthy replacement.
    let has_local = present(&launcher_path(root))? || present(&root.join("launcher.json"))?;
    let has_universal = paths.iter().any(|path| *path != launcher_path(root))
        || present(&root.join("universal-uninstall.json"))?;
    let mut observations = Vec::new();
    if has_local || has_universal {
        // Verify the selected manager's direct entry without changing its state.
        // These disjoint legacy files never require stopping modern workers or
        // overriding a durable User Stop merely to manufacture quiescence.
        crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()?;
        stop_legacy(&paths)?;
        let image_exclusion = image_exclusion(&images)?;
        record_exclusions(root, &mut intent, &image_exclusion)?;
        observe(&images, &intent.image_exclusions)?;
        verify_remaining(root, &intent)?;
        crate::universal_launcher::retire_for(root, channel)?;
        finish_local(root, &intent)?;
        observations = observe(&images, &intent.image_exclusions)?;
    } else {
        observations.extend(observe(&images, &intent.image_exclusions)?);
        // Validate any partial/foreign universal publication through its existing
        // exact receipt-backed remover, even when discovery returned no image.
        crate::universal_launcher::retire_for(root, channel)?;
    }
    ensure!(
        !present(&launcher_path(root))?
            && !present(&root.join("launcher.json"))?
            && crate::universal_launcher::retirement_image_for(root, channel)?.is_none()
            && !present(&root.join("universal-uninstall.json"))?,
        "legacy_launcher_retirement_absence_not_proven"
    );
    ensure!(
        selection_record(root)?.active == expected,
        "legacy_launcher_retirement_selection_changed"
    );
    let result = serde_json::json!({"schema":1,"active":expected,"legacy_launchers_retired":true,
        "launcher":null,"universal_launcher":null,"processes":{"scope":"verified_launcher_images","remaining":0,"observations":observations,"image_roster":images,"image_exclusions":intent.image_exclusions}});
    record(&result)?;
    fs::remove_file(root.join(INTENT))?;
    sync_directory(root)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
