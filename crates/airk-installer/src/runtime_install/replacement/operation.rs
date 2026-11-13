// SPDX-License-Identifier: Apache-2.0
//! Installation-attempt identity survives removal of the installation itself.
use super::*;

#[derive(Clone)]
pub(super) struct Operation {
    pub(super) id: String,
    candidate: String,
    generation: Option<String>,
    path: PathBuf,
    consumed: Vec<(String, String)>,
    pub(super) saved: Option<Receipt>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    schema: u32,
    operation: String,
    candidate: String,
    generation: Option<String>,
    install: PathBuf,
    installed_runtime: Option<String>,
    installed_version: Option<String>,
    locations: Vec<PathBuf>,
    indexes: IndexLocations,
    clients: ClientReferences,
    seals: Vec<(PathBuf, PathBuf, String)>,
    directories: Vec<(PathBuf, String)>,
    consumed: Vec<(String, String)>,
    frozen: bool,
    image_retirement_admitted: bool,
    images: Vec<legacy_launchers::Image>,
    image_exclusions: Vec<legacy_launchers::Image>,
    completed: bool,
}

fn receipt_path(install: &Path) -> Result<PathBuf> {
    let install = airk_host::user_paths::canonical_location(install)?;
    let parent = install.parent().context("installation has no parent")?;
    let leaf = install
        .file_name()
        .context("installation has no name")?
        .to_string_lossy();
    Ok(parent
        .join(format!(".{leaf}.replacement-1"))
        .join("receipt.json"))
}

fn read(path: &Path) -> Result<Option<Receipt>> {
    let root = path.parent().context("replacement private root missing")?;
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => crate::local_ipc::require_existing_root(root)
            .context("replacement_journal_private_root")?,
    }
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(Some(serde_json::from_slice(&read_bounded(
            path, MAX_RECORD,
        )?)?)),
    }
}

fn save(path: &Path, receipt: &Receipt) -> Result<()> {
    crate::local_ipc::require_existing_root(
        path.parent().context("replacement private root missing")?,
    )
    .context("replacement_journal_private_root")?;
    replace_json(
        path.parent()
            .context("replacement receipt parent missing")?,
        path.file_name()
            .and_then(|name| name.to_str())
            .context("replacement receipt name invalid")?,
        receipt,
    )
}

fn aside(path: &Path, operation: &str) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("AIRK location has no name")?;
    Ok(path.with_file_name(format!("{name}.removed-{operation}")))
}

pub(super) fn directory_identity(path: &Path) -> Result<String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .custom_flags(0x02000000 | 0x00200000)
            .share_mode(1 | 2 | 4);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_dir(),
            "replacement directory identity is not a directory"
        );
        unix_directory_identity(
            metadata.dev(),
            metadata.ino(),
            metadata
                .created()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()),
        )
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: live read-only directory handle and writable fixed-size SDK output.
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } != 0,
            "replacement directory identity unavailable"
        );
        // SAFETY: successful SDK query initialized the output.
        let info = unsafe { info.assume_init() };
        ensure!(
            info.dwFileAttributes & 0x10 != 0 && info.dwFileAttributes & 0x400 == 0,
            "replacement directory is redirected"
        );
        Ok(format!(
            "{}:{}:{}:{}:{}",
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
            info.ftCreationTime.dwHighDateTime,
            info.ftCreationTime.dwLowDateTime
        ))
    }
    #[cfg(not(any(unix, windows)))]
    anyhow::bail!("replacement directory identity unsupported")
}

#[cfg(any(unix, test))]
fn unix_directory_identity(device: u64, inode: u64, created: Option<Duration>) -> Result<String> {
    let created = created.context("replacement_directory_identity_unavailable: the filesystem supplies no durable creation identity")?;
    Ok(format!(
        "{device}:{inode}:{}:{}",
        created.as_secs(),
        created.subsec_nanos()
    ))
}

impl Operation {
    pub(super) fn pending(install: &Path, candidate: &str) -> Result<Option<String>> {
        let path = receipt_path(install)?;
        let Some(saved) = read(&path)? else {
            return Ok(None);
        };
        ensure!(
            saved.schema == 1
                && saved.install == airk_host::user_paths::canonical_location(install)?,
            "replacement_receipt_identity_changed"
        );
        if saved.completed {
            return Ok(None);
        }
        ensure!(
            saved.candidate == candidate,
            "replacement_in_progress: resume the original candidate before starting another"
        );
        digest(&saved.operation)?;
        Ok(Some(saved.operation))
    }
    pub(super) fn observe(
        install: &Path,
        candidate: &str,
        operation: Option<&str>,
        generation: Option<&str>,
    ) -> Result<Option<Self>> {
        ensure!(
            generation.is_none() || operation.is_some(),
            "replacement generation requires an operation"
        );
        let path = receipt_path(install)?;
        let saved = read(&path)?;
        let mut superseded = false;
        if let Some(saved) = &saved {
            digest(&saved.operation)?;
            digest(&saved.candidate)?;
            if let Some(generation) = &saved.generation {
                digest(generation)?;
            }
            ensure!(
                saved.consumed.len() <= 128,
                "replacement consumed-operation budget"
            );
            for (id, candidate) in &saved.consumed {
                digest(id)?;
                digest(candidate)?;
            }
            ensure!(
                saved.schema == 1
                    && saved.install == airk_host::user_paths::canonical_location(install)?,
                "replacement_receipt_identity_changed"
            );
            saved.validate_images()?;
            if saved.completed
                && operation.is_some_and(|id| saved.consumed.iter().any(|(known, _)| known == id))
            {
                ensure!(
                    saved
                        .consumed
                        .iter()
                        .any(|(id, bound)| operation == Some(id.as_str()) && bound == candidate),
                    "replacement_operation_candidate_changed"
                );
                if let Some(selection) = identity_record(&install.join("selected.json"))? {
                    ensure!(
                        selection["active"].as_str() == Some(candidate),
                        "replacement_successor_changed: the selected runtime differs from the completed installation"
                    );
                }
            } else if !saved.completed
                && !(operation == Some(saved.operation.as_str()) && candidate == saved.candidate)
            {
                // An attempt refused before its freeze changed no location, so
                // a new, separately reviewed operation may take it over; its
                // identity stays consumed. Anything later resumes only as itself.
                ensure!(
                    operation
                        .is_some_and(|id| !saved.consumed.iter().any(|(known, _)| known == id))
                        && saved.unstarted(),
                    "replacement_in_progress: resume the original installation attempt before starting another"
                );
                superseded = true;
            }
        }
        let Some(id) = operation else {
            return Ok(None);
        };
        digest(id)?;
        if let Some(generation) = generation {
            digest(generation)?;
        }
        let consumed = saved
            .as_ref()
            .map(|saved| saved.consumed.clone())
            .unwrap_or_default();
        let replay = saved.filter(|saved| {
            !superseded && (!saved.completed || saved.consumed.iter().any(|(known, _)| known == id))
        });
        Ok(Some(Self {
            id: id.into(),
            candidate: candidate.into(),
            generation: generation.map(str::to_owned),
            path,
            consumed,
            saved: replay,
        }))
    }

    pub(super) fn begin(plan: &mut Plan) -> Result<()> {
        let Some(operation) = &mut plan.operation else {
            return Ok(());
        };
        if operation.saved.is_some() {
            return Ok(());
        }
        let private_root = operation
            .path
            .parent()
            .context("replacement private root missing")?;
        ensure!(
            !plan
                .locations
                .iter()
                .any(|location| private_root.starts_with(location)),
            "replacement_journal_scope_overlap: cleanup would remove its own recovery authority"
        );
        let mut seals = Vec::new();
        let directories = plan
            .locations
            .iter()
            .map(|location| Ok((location.clone(), directory_identity(location)?)))
            .collect::<Result<Vec<_>>>()?;
        for location in &plan.locations {
            match fs::symlink_metadata(aside(location, &operation.id)?) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => anyhow::bail!(
                    "replacement_aside_collision: the planned retired location already exists or cannot be inspected; all original folders are preserved"
                ),
            }
            for relative in [
                "selected.json",
                "runtime-layout.json",
                "airk-state-1.4.owner.json",
                "airk-control-1.4.owner.json",
                "airk-registry-1.4.owner.json",
                "service-data-1.4.owner.json",
                "control/airk-control-1.4.owner.json",
                "control/airk-registry-1.4.owner.json",
            ]
            .into_iter()
            .chain(airk_host::service_logs::OWNER_RECORDS)
            {
                let path = location.join(relative);
                if path.try_exists()? {
                    safe_file(&path)?;
                    seals.push((
                        location.clone(),
                        PathBuf::from(relative),
                        runtime_pair::digest_file(&path)?,
                    ));
                }
            }
        }
        if let Some(layout) = airk_host::install_layout::Layout::read(&plan.install)? {
            let logs = layout.logs.unwrap_or_else(|| layout.state.join("control"));
            for location in &plan.locations {
                if let Ok(relative_logs) = logs.strip_prefix(location) {
                    for name in airk_host::service_logs::OWNER_RECORDS {
                        let relative = relative_logs.join(name);
                        let path = location.join(&relative);
                        if path.try_exists()?
                            && !seals.iter().any(|(root, recorded, _)| {
                                root == location && recorded == &relative
                            })
                        {
                            safe_file(&path)?;
                            seals.push((
                                location.clone(),
                                relative,
                                runtime_pair::digest_file(&path)?,
                            ));
                        }
                    }
                }
            }
        }
        let mut consumed = operation.consumed.clone();
        for id in [Some(operation.id.as_str()), operation.generation.as_deref()]
            .into_iter()
            .flatten()
        {
            if !consumed.iter().any(|(known, _)| known == id) {
                consumed.push((id.into(), operation.candidate.clone()));
            }
        }
        ensure!(
            consumed.len() <= 128,
            "replacement consumed-operation budget exhausted; existing operation identities are preserved"
        );
        let receipt = Receipt {
            schema: 1,
            operation: operation.id.clone(),
            candidate: operation.candidate.clone(),
            generation: operation.generation.clone(),
            install: airk_host::user_paths::canonical_location(&plan.install)?,
            installed_runtime: plan.installed_runtime.clone(),
            installed_version: plan.installed_version.clone(),
            locations: plan.locations.clone(),
            indexes: plan.index_locations.clone(),
            clients: plan.client_references.clone(),
            seals,
            directories,
            consumed,
            frozen: false,
            image_retirement_admitted: false,
            images: Vec::new(),
            image_exclusions: Vec::new(),
            completed: false,
        };
        match fs::symlink_metadata(private_root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                crate::local_ipc::prepare_root(private_root)?
            }
            Err(error) => return Err(error.into()),
            Ok(_) => crate::local_ipc::require_existing_root(private_root)
                .context("replacement_journal_private_root")?,
        }
        save(&operation.path, &receipt)?;
        operation.saved = Some(receipt);
        Ok(())
    }

    pub(super) fn complete(self) -> Result<()> {
        let admitted = self
            .saved
            .as_ref()
            .context("replacement admission missing")?;
        let mut saved = read(&self.path)?.context("replacement admission disappeared")?;
        ensure!(
            saved.schema == 1
                && saved.operation == self.id
                && saved.candidate == self.candidate
                && saved.install == admitted.install,
            "replacement completion crossed admitted operation"
        );
        saved.validate_images()?;
        saved.completed = true;
        save(&self.path, &saved)
    }

    pub(super) fn admit_images(
        plan: &mut Plan,
        images: &[legacy_launchers::Image],
        exclusions: &[legacy_launchers::Image],
    ) -> Result<()> {
        let Some(operation) = &mut plan.operation else {
            return Ok(());
        };
        let saved = operation
            .saved
            .as_mut()
            .context("replacement admission missing")?;
        ensure!(
            saved.frozen,
            "replacement image retirement requires frozen namespace proof"
        );
        if saved.image_retirement_admitted {
            ensure!(
                saved.images.as_slice() == images,
                "replacement immutable image roster changed"
            );
            return Ok(());
        }
        saved.images = images.to_vec();
        saved.image_exclusions = exclusions.to_vec();
        saved.image_retirement_admitted = true;
        saved.validate_images()?;
        save(&operation.path, saved)
    }

    /// Capture the exact stopped registry before any directory moves. On retry,
    /// this seal detects a newly accepted source or custom-storage registration.
    pub(super) fn freeze(plan: &mut Plan) -> Result<()> {
        let Some(operation) = &mut plan.operation else {
            return Ok(());
        };
        let saved = operation
            .saved
            .as_mut()
            .context("replacement admission missing")?;
        if saved.frozen {
            return Ok(());
        }
        // Admission sealed owners and directory identities before stopping.
        // Verify those proofs under process exclusion before authorizing
        // unlink or moves; a substituted same-named directory is not a resume.
        saved.remaining()?;
        for location in &saved.locations {
            for relative in ["control/service-1.4.sqlite3", "service-1.4.sqlite3"] {
                let path = location.join(relative);
                if path.try_exists()? {
                    safe_file(&path)?;
                    saved.seals.push((
                        location.clone(),
                        PathBuf::from(relative),
                        runtime_pair::digest_file(&path)?,
                    ));
                }
            }
        }
        saved.frozen = true;
        save(&operation.path, saved)
    }
}

impl Receipt {
    /// Admitted but never frozen: no executable name was admitted for
    /// retirement and every location is still at its original path with its
    /// recorded directory identity and owner seals, so no folder moved.
    fn unstarted(&self) -> bool {
        !self.frozen
            && !self.image_retirement_admitted
            && self.images.is_empty()
            && self.image_exclusions.is_empty()
            && self.remaining().is_ok_and(|remaining| {
                remaining.len() == self.locations.len()
                    && remaining
                        .iter()
                        .all(|(original, actual)| original == actual)
            })
    }

    fn validate_images(&self) -> Result<()> {
        ensure!(
            self.images.len() <= 1024 && self.image_exclusions.len() <= self.images.len(),
            "replacement image roster budget"
        );
        ensure!(
            !self.image_retirement_admitted || self.frozen,
            "replacement image retirement lacks frozen namespace proof"
        );
        ensure!(
            self.image_retirement_admitted
                || self.images.is_empty() && self.image_exclusions.is_empty(),
            "replacement images lack retirement admission"
        );
        for (index, image) in self.images.iter().enumerate() {
            digest(&image.sha256)?;
            ensure!(
                image.path.is_absolute()
                    && !image
                        .path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    && self
                        .locations
                        .iter()
                        .any(|root| image.path.starts_with(root))
                    && !self.images[..index]
                        .iter()
                        .any(|old| old.path == image.path),
                "replacement image crossed owned immutable roster"
            );
        }
        ensure!(
            self.image_exclusions
                .iter()
                .all(|image| self.images.contains(image)),
            "replacement image exclusion crossed owned roster"
        );
        Ok(())
    }

    fn relocate_image(
        &self,
        image: &legacy_launchers::Image,
        remaining: &[(PathBuf, PathBuf)],
    ) -> Result<legacy_launchers::Image> {
        let owner = self
            .locations
            .iter()
            .find(|root| image.path.starts_with(root))
            .context("replacement image owner missing")?;
        let actual = remaining
            .iter()
            .find(|(original, _)| original == owner)
            .map(|(_, actual)| actual)
            .unwrap_or(owner);
        Ok(legacy_launchers::Image {
            path: actual.join(image.path.strip_prefix(owner)?),
            sha256: image.sha256.clone(),
        })
    }
    pub(super) fn plan(
        &self,
        candidate_version: &'static str,
        operation: Operation,
    ) -> Result<Plan> {
        Ok(Plan {
            install: self.install.clone(),
            expected_predecessor: predecessor_stamp(
                &self.install,
                self.installed_runtime.as_deref(),
                &self.indexes,
                &self.directories,
            )?,
            required: !self.completed,
            reason: if self.completed {
                "installation_already_replaced"
            } else {
                "resume_installation_replacement"
            },
            installed_runtime: self.installed_runtime.clone(),
            installed_version: self.installed_version.clone(),
            candidate_version,
            locations: self.locations.clone(),
            client_references: self.clients.clone(),
            index_locations: self.indexes.clone(),
            operation: Some(operation),
        })
    }

    fn remaining(&self) -> Result<Vec<(PathBuf, PathBuf)>> {
        self.indexes.protect(&self.locations)?;
        let mut result = Vec::new();
        for original in &self.locations {
            ensure!(
                airk_host::user_paths::canonical_location(original)? == *original,
                "replacement_resume_location_changed"
            );
            let removed = aside(original, &self.operation)?;
            let original_exists = original.try_exists()?;
            let removed_exists = removed.try_exists()?;
            ensure!(
                !(original_exists && removed_exists),
                "replacement_resume_conflict: both original and retired location exist"
            );
            let actual = if original_exists { original } else { &removed };
            if original_exists || removed_exists {
                ensure!(
                    actual.is_dir() && actual.canonicalize()? == *actual,
                    "replacement_resume_location_redirected"
                );
                ensure!(
                    self.directories
                        .iter()
                        .any(|(location, identity)| location == original
                            && directory_identity(actual)
                                .is_ok_and(|observed| observed == *identity)),
                    "replacement_resume_directory_changed: {}",
                    actual.display()
                );
                for (location, relative, hash) in &self.seals {
                    if location == original {
                        let path = actual.join(relative);
                        if !original_exists && !path.try_exists()? {
                            continue;
                        }
                        if relative
                            .extension()
                            .is_some_and(|extension| extension == "sqlite3")
                        {
                            for suffix in ["-wal", "-shm", "-journal"] {
                                let overlay = path.with_file_name(format!(
                                    "{}{suffix}",
                                    path.file_name()
                                        .context("registry file name missing")?
                                        .to_string_lossy()
                                ));
                                ensure!(
                                    !overlay.try_exists()?,
                                    "replacement_resume_registry_overlay_unproven: {}",
                                    overlay.display()
                                );
                            }
                        }
                        safe_file(&path)?;
                        ensure!(
                            runtime_pair::digest_file(&path)? == *hash,
                            "replacement_resume_owner_changed: {}",
                            actual.display()
                        );
                    }
                }
                result.push((original.clone(), actual.clone()));
            }
        }
        Ok(result)
    }
}

/// A pending receipt owns only its original sealed locations. A successor at
/// the original path is refused, never reinterpreted as more cleanup work.
pub(super) fn resume(mut plan: Plan) -> Result<serde_json::Value> {
    let observed_operation = plan
        .operation
        .clone()
        .context("replacement operation missing")?;
    let saved = observed_operation
        .saved
        .as_ref()
        .context("replacement receipt missing")?;
    let remaining = saved.remaining()?;
    for (_, actual) in &remaining {
        require_unmanaged_retired(&plan.install, actual)?;
    }
    let locations = remaining
        .iter()
        .map(|(_, actual)| actual.clone())
        .collect::<Vec<_>>();
    let roster = if saved.image_retirement_admitted {
        saved
            .images
            .iter()
            .map(|image| saved.relocate_image(image, &remaining))
            .collect::<Result<Vec<_>>>()?
    } else {
        ensure!(
            remaining
                .iter()
                .all(|(original, actual)| original == actual),
            "replacement_resume_image_phase_unproven"
        );
        let mut paths = Vec::new();
        for location in &locations {
            executables(location, &mut paths)?;
        }
        paths
            .into_iter()
            .map(|path| {
                safe_file(&path)?;
                Ok(legacy_launchers::Image {
                    path: path.canonicalize()?,
                    sha256: runtime_pair::digest_file(&path)?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    for image in &roster {
        if image.path.try_exists()? {
            safe_file(&image.path)?;
            ensure!(
                runtime_pair::digest_file(&image.path)? == image.sha256,
                "replacement_image_changed: {}",
                image.path.display()
            );
        }
    }
    upgrade_stop::stop_paths(
        &roster
            .iter()
            .map(|image| image.path.clone())
            .collect::<Vec<_>>(),
    )?;
    let held = legacy_launchers::image_exclusion(&roster)?;
    let mut exclusions = saved
        .image_exclusions
        .iter()
        .map(|image| saved.relocate_image(image, &remaining))
        .collect::<Result<Vec<_>>>()?;
    for image in &held.images {
        if !exclusions.contains(image) {
            exclusions.push(image.clone());
        }
    }
    legacy_launchers::observe(&roster, &exclusions)?;
    if let Some(control) = saved
        .indexes
        .control
        .as_ref()
        .filter(|control| control.join("service-1.4.sqlite3").exists())
    {
        ensure!(
            IndexLocations::capture_control(control)
                .context("replacement_resume_control_unproven")?
                == saved.indexes,
            "replacement_resume_index_roster_changed"
        );
        if !saved.frozen {
            Operation::freeze(&mut plan)?;
        }
    } else {
        ensure!(
            saved.frozen,
            "replacement_resume_unfrozen_locations_changed"
        );
    }
    if !saved.image_retirement_admitted {
        Operation::admit_images(&mut plan, &roster, &held.images)?;
    }
    let operation = plan
        .operation
        .clone()
        .context("replacement operation missing")?;
    let saved = operation
        .saved
        .as_ref()
        .context("replacement receipt missing")?;
    // Recheck every owner under exclusion before continuing any removal.
    saved.remaining()?;
    for (_, actual) in &remaining {
        require_unmanaged_retired(&plan.install, actual)?;
    }
    if plan.install.exists() {
        remove_owned_startup(&plan.install)?;
        let (_, residue) = retire_owned_trust(&plan.install);
        ensure!(
            residue.is_empty(),
            "replacement_resume_trust_incomplete: {}",
            residue.join("; ")
        );
    }
    unlink_image_names(&roster, &exclusions).context("replacement_image_retirement_incomplete: admitted executable names may already be retired; resume the same installation operation")?;
    let mut removed = Vec::new();
    for (original, actual) in remaining {
        let target = aside(&original, &saved.operation)?;
        if actual == original {
            rename_owned(&actual, &target).context("replacement_cleanup_incomplete: executable names remain retired; resume the same installation operation")?;
        }
        fs::remove_dir_all(&target).context("replacement_cleanup_incomplete: executable names remain retired; resume the same installation operation")?;
        removed.push(target);
    }
    drop(held);
    operation.complete()?;
    Ok(
        serde_json::json!({"replaced":true,"plan":plan,"removed":removed,"residue":[],"client_references":plan.client_references,"startup_registration_removed":null,"user_trust_removed":null}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_operation_budget_refuses_new_admission_without_pruning_or_mutation() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let install = base.join("install");
        let source = base.join("source.txt");
        fs::write(
            &source,
            b"original source bytes survive exhausted admission",
        )?;
        let candidate = "a".repeat(64);
        let first = format!("{:064x}", 1);
        assert_eq!(
            apply(&install, &candidate, Some(&first), None)?["replaced"],
            false
        );
        let path = receipt_path(&install)?;
        let mut saved = read(&path)?.context("completed fresh admission")?;
        assert!(saved.completed);
        saved.consumed = (1..=128)
            .map(|id| (format!("{id:064x}"), candidate.clone()))
            .collect();
        save(&path, &saved)?;
        let before = fs::read(&path)?;
        let new_operation = format!("{:064x}", 129);
        let error = apply(&install, &candidate, Some(&new_operation), None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("consumed-operation budget exhausted"),
            "{error}"
        );
        assert_eq!(
            fs::read(&path)?,
            before,
            "history is never pruned or rewritten"
        );
        assert!(!install.exists(), "refusal must not create a successor");
        assert_eq!(
            fs::read(&source)?,
            b"original source bytes survive exhausted admission"
        );
        assert_eq!(
            apply(&install, &candidate, Some(&first), None)?["replaced"],
            false
        );
        assert_eq!(
            fs::read(&path)?,
            before,
            "consumed replay remains read-only at capacity"
        );
        Ok(())
    }

    #[test]
    fn a_reusable_inode_without_creation_identity_never_authorizes_resumed_deletion() {
        assert!(unix_directory_identity(1, 2, None).is_err());
        assert_ne!(
            unix_directory_identity(1, 2, Some(Duration::from_secs(3))).unwrap(),
            unix_directory_identity(1, 2, Some(Duration::from_secs(4))).unwrap()
        );
    }

    #[test]
    fn a_forged_unprotected_journal_cannot_authorize_original_source_deletion() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let install = base.join("install");
        let source = base.join("original-source");
        fs::create_dir(&install)?;
        fs::create_dir(&source)?;
        fs::write(
            source.join("keep.txt"),
            b"original sources must survive forged cleanup authority",
        )?;
        let candidate = "a".repeat(64);
        let token = "b".repeat(64);
        let forged = Receipt {
            schema: 1,
            operation: token.clone(),
            candidate: candidate.clone(),
            generation: None,
            install: install.clone(),
            installed_runtime: Some(candidate.clone()),
            installed_version: Some("1.8.22".into()),
            locations: vec![source.clone()],
            indexes: IndexLocations::default(),
            clients: ClientReferences::default(),
            seals: Vec::new(),
            directories: vec![(source.clone(), directory_identity(&source)?)],
            consumed: vec![(token.clone(), candidate.clone())],
            frozen: true,
            image_retirement_admitted: true,
            images: Vec::new(),
            image_exclusions: Vec::new(),
            completed: false,
        };
        let path = receipt_path(&install)?;
        let private_root = path.parent().context("fixture private root")?;
        // The former flat file grants no authority and is never imported.
        fs::write(
            base.join(".install.replacement-1.json"),
            serde_json::to_vec(&forged)?,
        )?;
        assert!(
            Operation::observe(&install, &candidate, Some(&token), None)?
                .context("planned operation")?
                .saved
                .is_none()
        );
        assert_eq!(Operation::pending(&install, &candidate)?, None);
        assert!(
            !private_root.exists(),
            "read-only discovery must not create a journal directory"
        );
        crate::local_ipc::prepare_root(private_root)?;
        fs::write(&path, serde_json::to_vec(&forged)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(private_root, fs::Permissions::from_mode(0o777))?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("icacls.exe")
                .arg(private_root)
                .args(["/grant", "*S-1-1-0:(OI)(CI)F"])
                .creation_flags(0x08000000)
                .output()?;
            ensure!(
                output.status.success(),
                "fixture ACL change failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(
            crate::local_ipc::require_existing_root(private_root).is_err(),
            "fixture must be unprotected"
        );
        let planned = plan(&install, &candidate, Some(&token), None)
            .err()
            .context("forged public journal must refuse")?
            .to_string();
        assert!(
            planned.contains("replacement_journal_private_root"),
            "{planned}"
        );
        let applied = apply(&install, &candidate, Some(&token), None)
            .unwrap_err()
            .to_string();
        assert!(
            applied.contains("replacement_journal_private_root"),
            "{applied}"
        );
        assert!(
            crate::local_ipc::require_existing_root(private_root).is_err(),
            "install must not bless or chmod an existing shared journal directory"
        );
        assert_eq!(
            fs::read(source.join("keep.txt"))?,
            b"original sources must survive forged cleanup authority"
        );
        Ok(())
    }

    #[test]
    fn recovery_authority_cannot_live_inside_any_planned_removal_scope() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let install = base.join("install");
        fs::create_dir(&install)?;
        fs::write(base.join("source.txt"), b"keep source and recovery parent")?;
        let candidate = "c".repeat(64);
        let token = "d".repeat(64);
        let mut plan = Plan {
            install: install.clone(),
            expected_predecessor: String::new(),
            required: true,
            reason: "fixture_ancestor_scope",
            installed_runtime: Some(candidate.clone()),
            installed_version: Some("1.8.22".into()),
            candidate_version: "1.8.23",
            locations: vec![base.clone()],
            client_references: ClientReferences::default(),
            index_locations: IndexLocations::default(),
            operation: Operation::observe(&install, &candidate, Some(&token), None)?,
        };
        let error = Operation::begin(&mut plan).unwrap_err().to_string();
        assert!(
            error.contains("replacement_journal_scope_overlap"),
            "{error}"
        );
        assert!(
            !receipt_path(&install)?
                .parent()
                .context("private root")?
                .exists()
        );
        assert_eq!(
            fs::read(base.join("source.txt"))?,
            b"keep source and recovery parent"
        );
        Ok(())
    }
}
