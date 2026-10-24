// SPDX-License-Identifier: Apache-2.0
//! Explicit, preview-bound restaging preserves the complete damaged slot.
use super::*;
use airk_core::identity::{Sha256Digest, framed_digest};
use std::collections::BTreeSet;

const INTENT: &str = "payload-repair.json";
const RESULT: &str = "payload-repair-result.json";
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    root: PathBuf,
    selection: Selection,
    approval: Approval,
    preview: Sha256Digest,
}
#[derive(Serialize)]
struct Captured {
    files: BTreeMap<String, (u64, String)>,
    directories: BTreeSet<String>,
    bytes: u64,
}
fn backup(root: &Path, identity: &str) -> Result<PathBuf> {
    digest(identity)?;
    payload_path(root, &format!("runtimes/damaged-{identity}"))
}
fn capture(directory: &Path) -> Result<Captured> {
    ensure!(
        crate::user_paths::canonical_location(directory)? == directory,
        "damaged slot is redirected"
    );
    let mut captured = Captured {
        files: BTreeMap::new(),
        directories: BTreeSet::new(),
        bytes: 0,
    };
    fn visit(root: &Path, directory: &Path, depth: usize, captured: &mut Captured) -> Result<()> {
        ensure!(depth <= 16, "damaged slot depth exceeds repair allowance");
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            ensure!(
                captured.files.len() + captured.directories.len() < 4100,
                "damaged slot inventory exceeds repair allowance"
            );
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("non-UTF-8 damaged slot entry")?
                .replace('\\', "/");
            let path = payload_path(root, &relative)?;
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "redirected damaged slot entry preserved"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected damaged slot entry preserved"
                );
            }
            if metadata.is_dir() {
                captured.directories.insert(relative);
                visit(root, &path, depth + 1, captured)?;
            } else {
                safe_file(&path)?;
                ensure!(
                    captured.files.len() < MAX_FILES + 2
                        && metadata.len() <= 2 * 1024 * 1024 * 1024,
                    "damaged slot file allowance exceeded"
                );
                captured.bytes = captured
                    .bytes
                    .checked_add(metadata.len())
                    .context("damaged slot size overflow")?;
                ensure!(
                    captured.bytes <= MAX_PAYLOAD,
                    "damaged slot exceeds repair allowance"
                );
                let hash = runtime_pair::digest_file(&path)?;
                ensure!(
                    fs::metadata(&path)?.len() == metadata.len(),
                    "damaged slot changed during review"
                );
                captured.files.insert(relative, (metadata.len(), hash));
            }
        }
        Ok(())
    }
    visit(directory, directory, 0, &mut captured)?;
    Ok(captured)
}
fn guard(root: &Path, selection: &Selection, captured: &Captured) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/runtime-payload-repair/1",
        &[
            root.to_str().context("non-UTF-8 installation")?.as_bytes(),
            &serde_json::to_vec(selection)?,
            &serde_json::to_vec(captured)?,
        ],
    ))
}
fn pending(root: &Path) -> Result<Option<Intent>> {
    if !exists(&root.join(INTENT))? {
        return Ok(None);
    }
    let intent: Intent = serde_json::from_slice(&read_bounded(&root.join(INTENT), 16_384)?)?;
    ensure!(
        intent.schema == 1
            && intent.root == root.canonicalize()?
            && intent.selection == selection_record(root)?
            && intent.approval.schema == 1
            && intent.approval.manifest_sha256 == intent.selection.active,
        "payload repair intent changed; preserve the original installation"
    );
    digest(&intent.selection.active)?;
    Ok(Some(intent))
}
pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        pending(root)?.is_none(),
        "resume the reviewed payload repair before launching or changing selection"
    );
    Ok(())
}
pub(super) fn saved_identity(root: &Path) -> Result<Option<snapshot::SavedRuntime>> {
    let Some(intent) = pending(root)? else {
        return Ok(None);
    };
    let preserved = backup(root, &intent.selection.active)?;
    let directory = if exists(&preserved)? {
        preserved
    } else {
        root.join("runtimes").join(&intent.selection.active)
    };
    let (approval, executable_sha256) =
        saved_payload_identity(&directory, &intent.selection.active)?;
    ensure!(approval == intent.approval, "saved repair approval changed");
    Ok(Some(snapshot::SavedRuntime {
        executable: root
            .join("runtimes")
            .join(&intent.selection.active)
            .join(main_name()),
        selection: intent.selection,
        approval,
        executable_sha256,
    }))
}
pub(super) fn preserved(root: &Path) -> Result<Option<PathBuf>> {
    if !exists(&root.join(RESULT))? {
        return Ok(None);
    }
    let result: Intent = serde_json::from_slice(&read_bounded(&root.join(RESULT), 16_384)?)?;
    ensure!(
        result.schema == 1
            && result.root == root.canonicalize()?
            && result.selection.schema == 1
            && result.approval.schema == 1
            && result.approval.manifest_sha256 == result.selection.active,
        "foreign payload repair result preserved"
    );
    let directory = backup(&result.root, &result.selection.active)?;
    ensure!(
        guard(&result.root, &result.selection, &capture(&directory)?)? == result.preview,
        "preserved damaged payload changed after repair"
    );
    Ok(Some(directory))
}
pub(super) fn status(root: &Path) -> Result<serde_json::Value> {
    let intent = pending(root)?;
    let (phase, value) = if let Some(intent) = intent {
        ("pending", Some(intent))
    } else if exists(&root.join(RESULT))? {
        let result: Intent = serde_json::from_slice(&read_bounded(&root.join(RESULT), 16_384)?)?;
        ensure!(
            result.schema == 1 && result.root == root.canonicalize()?,
            "foreign payload repair result preserved"
        );
        ("completed", Some(result))
    } else {
        ("none", None)
    };
    Ok(match value {
        Some(value) => serde_json::json!({"phase":phase,"runtime":value.selection.active,
        "preview":value.preview,"preserved_directory":backup(root, &value.selection.active)?}),
        None => serde_json::json!({"phase":phase}),
    })
}
pub(super) fn preview(root: &Path, source: &Path, approved: &str) -> Result<serde_json::Value> {
    managed::require_unmanaged(root)?;
    let root = root.canonicalize()?;
    let (_, inventory) = manifest(&source.canonicalize()?, approved)?;
    let intent = pending(&root)?;
    let (selection, approval, directory) = if let Some(intent) = &intent {
        let backup = backup(&root, &intent.selection.active)?;
        let directory = if exists(&backup)? {
            backup
        } else {
            root.join("runtimes").join(&intent.selection.active)
        };
        (intent.selection.clone(), intent.approval.clone(), directory)
    } else {
        let saved = snapshot::identity(&root)?;
        let directory = root.join("runtimes").join(&saved.selection.active);
        (saved.selection, saved.approval, directory)
    };
    ensure!(
        selection.active == approved && approval.version == inventory.version,
        "repair source must match the selected approved inventory exactly"
    );
    let captured = capture(&directory)?;
    let preview = guard(&root, &selection, &captured)?;
    if let Some(intent) = intent {
        ensure!(
            intent.preview == preview,
            "preserved damaged slot changed during repair"
        );
    }
    Ok(
        serde_json::json!({"schema":1,"runtime":approved,"preview":preview,"preserved_directory":backup(&root, approved)?,
        "preserved_bytes":captured.bytes,"preserved_files":captured.files.len(),"starts_service":false,"requires_stopped_owners":true}),
    )
}
pub(super) fn apply(
    root: &Path,
    source: &Path,
    approved: &str,
    expected: Sha256Digest,
) -> Result<serde_json::Value> {
    apply_with(
        root,
        source,
        approved,
        expected,
        |source, root, approval| {
            let (state, channel) = layout::retained_parameters(root)?;
            staging::stage_locked(
                source,
                root,
                &approval.manifest_sha256,
                approval.verification_basis,
                Some(&state),
                channel,
                false,
            )
        },
    )
}
fn apply_with(
    root: &Path,
    source: &Path,
    approved: &str,
    expected: Sha256Digest,
    stage: impl FnOnce(&Path, &Path, &Approval) -> Result<Approval>,
) -> Result<serde_json::Value> {
    let _lock = install_lock(root)?;
    managed::require_unmanaged(root)?;
    if exists(&root.join(RESULT))? {
        let result: Intent = serde_json::from_slice(&read_bounded(&root.join(RESULT), 16_384)?)?;
        ensure!(
            result.schema == 1 && result.root == root.canonicalize()?,
            "foreign payload repair result preserved"
        );
        if result.preview == expected && result.selection.active == approved {
            ensure!(
                selection_record(root)? == result.selection,
                "runtime selection changed after this repair"
            );
            let (_, inventory) = manifest(&source.canonicalize()?, approved)?;
            ensure!(
                inventory.version == result.approval.version
                    && verify_runtime(root, approved)?.1 == result.approval,
                "completed payload repair identity changed"
            );
            ensure!(
                guard(
                    &result.root,
                    &result.selection,
                    &capture(&backup(&result.root, approved)?)?
                )? == expected,
                "preserved damaged payload changed after repair"
            );
            if let Some(saved) = pending(root)? {
                ensure!(
                    saved.preview == expected,
                    "another payload repair is pending"
                );
                fs::remove_file(root.join(INTENT))?;
                sync_directory(root)?;
            }
            return Ok(
                serde_json::json!({"schema":1,"repaired":true,"runtime":approved,"preserved_directory":backup(root, approved)?,"service_started":false,"replayed":true}),
            );
        }
        ensure!(
            pending(root)?.is_some(),
            "review the retained payload repair result before another repair"
        );
    }
    activation::require_settled(root)?;
    if let Some(runtime) = launcher_repair::pending(root)? {
        ensure!(
            pending(root)?.is_some() && runtime == approved,
            "finish unrelated launcher repair first"
        );
    }
    lifecycle::require_settled(root)?;
    ensure!(
        !exists(&root.join("uninstall.json"))?,
        "runtime uninstall is pending"
    );
    crate::service_manager::ServiceManager::for_runtime(root)?.require_stopped_for_maintenance()?;
    ensure!(
        lifecycle::inspect(root)?["explicitly_stopped"] == true,
        "explicitly Stop the service before repairing its payload"
    );
    let _quiescence = quiescent(root)
        .context("all service/project/helper owners must stop before payload repair")?;
    let root = root.canonicalize()?;
    let reviewed = preview(&root, source, approved)?;
    ensure!(
        reviewed["preview"] == expected.to_string(),
        "payload repair preview changed; review it again"
    );
    let mut intent = pending(&root)?;
    let destination = root.join("runtimes").join(approved);
    let preserved = backup(&root, approved)?;
    if intent.is_none() {
        ensure!(
            !exists(&preserved)?,
            "a preserved damaged slot already exists; review it before another repair"
        );
        ensure!(
            fs::read_dir(root.join("runtimes"))?.take(5).count() < 4,
            "repair needs one free retained runtime slot"
        );
        ensure!(
            staging::pending(&root)?.is_empty(),
            "finish pending staging before payload repair"
        );
        let saved = snapshot::identity(&root)?;
        let saved = Intent {
            schema: 1,
            root: root.clone(),
            selection: saved.selection,
            approval: saved.approval,
            preview: expected,
        };
        replace_json(&root, INTENT, &saved)?;
        intent = Some(saved);
    }
    let intent = intent.context("payload repair intent missing")?;
    ensure!(
        intent.preview == expected && intent.selection.active == approved,
        "payload repair request conflicts with saved intent"
    );
    if !exists(&preserved)? {
        ensure!(
            guard(&root, &intent.selection, &capture(&destination)?)? == expected,
            "damaged payload changed after review"
        );
        fs::rename(&destination, &preserved)?;
        sync_directory(&root.join("runtimes"))?;
    }
    // Resume staging's own bounded copy transaction. The entire preceding slot
    // stays untouched; no modified or unknown original is overwritten/deleted.
    let approval = stage(source, &root, &intent.approval)?;
    ensure!(
        approval == intent.approval,
        "repaired approval differs from saved approval"
    );
    verify_runtime(&root, approved)?;
    launcher_repair::repair(&root, approved)?;
    ensure!(
        guard(&root, &intent.selection, &capture(&preserved)?)? == expected,
        "preserved damaged payload changed during repair"
    );
    if exists(&root.join(RESULT))? {
        ensure!(
            read_bounded(&root.join(RESULT), 16_384)? == serde_json::to_vec(&intent)?,
            "foreign payload repair result preserved"
        );
    } else {
        replace_json(&root, RESULT, &intent)?;
    }
    fs::remove_file(root.join(INTENT))?;
    sync_directory(&root)?;
    Ok(
        serde_json::json!({"schema":1,"repaired":true,"runtime":approved,"preserved_directory":preserved,"service_started":false}),
    )
}

#[cfg(test)]
#[path = "runtime_payload_repair_tests.rs"]
mod tests;
