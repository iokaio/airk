// SPDX-License-Identifier: Apache-2.0
//! The file layer: bounded reads, project-folder checks and atomic,
//! compare-and-swap replacement, as the invoking user (the service never
//! writes agent files). Linked configuration files and redirected
//! configuration folders are refused, never followed (agentConfig.ts
//! `readJsonConfig`, agentConfigHealth.ts `directoryProblem`).
use super::locations::UserLocations;
use super::recognize::MAX_BYTES;
use super::{Agent, Blocker};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_USER_FILES: usize = 64;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_PROFILES: usize = 32;

#[cfg(test)]
type BeforePublishObserver = Box<dyn FnOnce(&Path)>;

#[cfg(test)]
thread_local! {
    static BEFORE_PUBLISH_OBSERVER: std::cell::RefCell<Option<BeforePublishObserver>> = const { std::cell::RefCell::new(None) };
}

/// Test-local interleaving after staging, without changing release execution.
/// Visible only within the owning config module and its external test module.
#[cfg(test)]
pub(super) fn with_before_publish<T>(
    observer: impl FnOnce(&Path) + 'static,
    run: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<BeforePublishObserver>);
    impl Drop for Reset {
        fn drop(&mut self) {
            BEFORE_PUBLISH_OBSERVER.with(|slot| {
                let _ = slot.replace(self.0.take());
            });
        }
    }
    let previous = BEFORE_PUBLISH_OBSERVER.with(|slot| slot.replace(Some(Box::new(observer))));
    let _reset = Reset(previous);
    run()
}

#[cfg(test)]
pub(super) fn observe_before_publish(path: &Path) {
    let observer = BEFORE_PUBLISH_OBSERVER.with(|slot| slot.borrow_mut().take());
    if let Some(observer) = observer {
        observer(path);
    }
}

fn absent(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// The observer for `Observations::path_exists`: `Some(false)` only when the
/// path is observed absent; other errors are unknown.
pub(crate) fn path_exists(path: &str) -> Option<bool> {
    match fs::metadata(path) {
        Ok(_) => Some(true),
        Err(error) if absent(&error) => Some(false),
        Err(_) => None,
    }
}

/// A regular, unlinked UTF-8 file of at most 1 MiB; `None` when absent.
pub(crate) fn read_config(path: &Path) -> Result<Option<String>, String> {
    let shown = path.display();
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if absent(&error) => return Ok(None),
        Err(error) => return Err(format!("{shown}: {error}")),
        Ok(metadata) => metadata,
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{shown} is a symbolic link; AIRK does not follow linked configuration files."
        ));
    }
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err(format!("{shown} must be a regular file of at most 1 MiB."));
    }
    let file = fs::File::open(path).map_err(|error| format!("{shown}: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(format!(
                "{shown} has other hard links; AIRK does not edit linked configuration files."
            ));
        }
        let opened = file
            .metadata()
            .map_err(|error| format!("{shown}: {error}"))?;
        if opened.ino() != metadata.ino() || opened.dev() != metadata.dev() {
            return Err(format!("{shown} changed while it was opened."));
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{shown}: {error}"))?;
    if bytes.len() > MAX_BYTES {
        return Err(format!("{shown} exceeds the 1 MiB limit."));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{shown} is not UTF-8 text."))
}

pub(crate) enum FolderState {
    Absent,
    /// A link, or a folder that resolves outside the project.
    Redirected,
    Present,
}

/// Whether every folder between `root` and a project configuration file is
/// a real folder inside `root`.
pub(crate) fn folder_state(root: &Path, folders: &[&str]) -> Result<FolderState, String> {
    let real_root = match root.canonicalize() {
        Ok(real) => real,
        Err(error) if absent(&error) => return Ok(FolderState::Absent),
        Err(error) => return Err(format!("{}: {error}", root.display())),
    };
    let mut current = root.to_path_buf();
    for folder in folders {
        current.push(folder);
        match fs::symlink_metadata(&current) {
            Err(error) if absent(&error) => return Ok(FolderState::Absent),
            Err(error) => return Err(format!("{}: {error}", current.display())),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Ok(FolderState::Redirected);
            }
            Ok(metadata) if !metadata.is_dir() => return Ok(FolderState::Absent),
            Ok(_) => {}
        }
        let real = current
            .canonicalize()
            .map_err(|error| format!("{}: {error}", current.display()))?;
        if real == real_root || !real.starts_with(&real_root) {
            return Ok(FolderState::Redirected);
        }
    }
    Ok(FolderState::Present)
}

/// `false` when absent; an error when the folder is a link or not a folder.
pub(crate) fn direct_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Err(error) if absent(&error) => Ok(false),
        Err(error) => Err(format!("{}: {error}", path.display())),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(format!(
            "{} is a link or not a folder; AIRK does not follow redirected configuration folders.",
            path.display()
        )),
        Ok(_) => Ok(true),
    }
}

/// Make sure the folders from `root` to `parent` are real folders inside the
/// project, creating missing ones when `create`.
fn project_folders(root: &Path, parent: &Path, create: bool) -> Result<(), String> {
    let relative = parent.strip_prefix(root).map_err(|_| {
        format!(
            "{} is outside the project {}.",
            parent.display(),
            root.display()
        )
    })?;
    let real_root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(format!("{} is not a plain project path.", parent.display()));
        };
        current.push(part);
        let shown = current.display();
        match fs::symlink_metadata(&current) {
            Err(error) if error.kind() == ErrorKind::NotFound && create => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(format!("{shown}: {error}")),
                }
            }
            Err(error) if absent(&error) => return Err(format!("{shown} does not exist.")),
            Err(error) => return Err(format!("{shown}: {error}")),
            Ok(_) => {}
        }
        if !direct_directory(&current)? {
            return Err(format!("{shown} does not exist."));
        }
        let real = current
            .canonicalize()
            .map_err(|error| format!("{shown}: {error}"))?;
        if real == real_root || !real.starts_with(&real_root) {
            return Err(format!(
                "{shown} resolves outside the project; AIRK does not follow redirected configuration folders."
            ));
        }
    }
    Ok(())
}

/// A caller may establish eligibility before enrolling a new connection. The
/// actual edit revalidates every boundary and never treats this as authority.
pub(crate) fn preflight_edit(path: &Path) -> Result<(), super::retention::EditFailure> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            super::retention::review(path)?;
            Ok(())
        }
        Err(error) if absent(&error) => Ok(()),
        Err(error) => Err(error.to_string().into()),
    }
}

/// Edit as the invoking user. Existing-file publication retains the actual
/// displaced object; known conflicts and uncertainty preserve every artifact.
pub(crate) fn edit_file_for(
    operation: super::edit_record::Operation,
    path: &Path,
    project_root: Option<&Path>,
    create: bool,
    edit: impl FnOnce(Option<&str>) -> Result<String, String>,
) -> Result<bool, super::retention::EditFailure> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent folder.", path.display()))?;
    match project_root {
        Some(root) => project_folders(root, parent, create)?,
        None => {
            if !direct_directory(parent)? {
                return Err(format!(
                    "{} does not exist; AIRK creates folders only for project files.",
                    parent.display()
                )
                .into());
            }
        }
    }
    let context = match fs::symlink_metadata(path) {
        Ok(_) => Some(super::retention::review(path)?),
        Err(error) if absent(&error) => None,
        Err(error) => return Err(error.to_string().into()),
    };
    let before = context.as_ref().map(|context| context.before());
    let after = edit(before)?;
    if before == Some(after.as_str()) {
        return Ok(false);
    }
    if after.len() > MAX_BYTES {
        return Err("Updated configuration would exceed 1 MiB.".into());
    }
    if let Some(context) = context {
        super::retention::replace(context, &after, operation)?;
    } else {
        create_file(path, parent, &after)?;
    }
    Ok(true)
}

// Preserve the observation API for the original test identities. Production
// callers provide their actual operation to edit_file_for.
#[cfg(test)]
pub(crate) fn edit_file(
    path: &Path,
    project_root: Option<&Path>,
    create: bool,
    edit: impl FnOnce(Option<&str>) -> Result<String, String>,
) -> Result<bool, String> {
    edit_file_for(
        super::edit_record::Operation::Repair,
        path,
        project_root,
        create,
        edit,
    )
    .map_err(|error| error.to_string())
}

fn create_file(path: &Path, parent: &Path, after: &str) -> Result<(), String> {
    // Keep the original absent-target no-clobber path; a concurrent creation
    // never turns into an existing-file replacement or a retry.
    let _parent = airk_process::directory::OpenedDirectory::open(parent)
        .map_err(|error| error.to_string())?;
    let shown = path.display();
    let failed = |error: std::io::Error| format!("{shown}: {error}");
    let mut temp = tempfile::Builder::new()
        .prefix(".airk-config-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(failed)?;
    temp.write_all(after.as_bytes()).map_err(failed)?;
    temp.as_file().sync_all().map_err(failed)?;
    #[cfg(test)]
    observe_before_publish(path);
    temp.persist_noclobber(path)
        .map_err(|error| failed(error.error))?;
    Ok(())
}

fn blocker(code: &str, path: &Path) -> Blocker {
    Blocker {
        code: code.to_string(),
        file: Some(path.display().to_string()),
    }
}

/// Folder entries (name, real folder) and whether the bound cut the listing.
type Listing = (Vec<(String, bool)>, bool);

/// Entries of a real folder, at most `MAX_DIRECTORY_ENTRIES`: name and
/// whether it is a real (unlinked) folder. `None` when absent.
fn entries(folder: &Path) -> Result<Option<Listing>, String> {
    if !direct_directory(folder)? {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut over = false;
    for item in fs::read_dir(folder).map_err(|error| error.to_string())? {
        if out.len() == MAX_DIRECTORY_ENTRIES {
            over = true;
            break;
        }
        let item = item.map_err(|error| error.to_string())?;
        let kind = item.file_type().map_err(|error| error.to_string())?;
        out.push((
            item.file_name().to_string_lossy().into_owned(),
            kind.is_dir() && !kind.is_symlink(),
        ));
    }
    out.sort();
    Ok(Some((out, over)))
}

/// Add the VS Code profile files (`<User>/profiles/<name>/mcp.json`) and
/// Codex named profiles (`*.config.toml` beside `config.toml`) to the base
/// user locations (globalAgentInventory.ts), within its bounds.
pub(crate) fn expand_user_files(
    locations: &UserLocations,
) -> (Vec<(Agent, PathBuf)>, Vec<Blocker>) {
    let mut files: Vec<(Agent, PathBuf)> = locations
        .files
        .iter()
        .map(|(agent, path)| (*agent, PathBuf::from(path)))
        .collect();
    let mut blockers: Vec<Blocker> = locations
        .blockers
        .iter()
        .map(|code| Blocker {
            code: code.clone(),
            file: None,
        })
        .collect();
    for root in &locations.vscode_roots {
        let profiles = Path::new(root).join("profiles");
        match entries(&profiles) {
            Ok(None) => {}
            Ok(Some((items, over))) => {
                if over || items.len() > MAX_PROFILES {
                    blockers.push(blocker("vscode_profile_count_bound", &profiles));
                }
                for (name, folder) in items.into_iter().take(MAX_PROFILES) {
                    if folder {
                        files.push((Agent::Copilot, profiles.join(name).join("mcp.json")));
                    } else {
                        blockers.push(blocker(
                            "unsupported_vscode_profile_entry",
                            &profiles.join(name),
                        ));
                    }
                }
            }
            Err(_) => blockers.push(blocker(
                "vscode_profiles_unreadable_or_redirected",
                &profiles,
            )),
        }
    }
    let codex: Vec<PathBuf> = locations
        .files
        .iter()
        .filter(|(agent, _)| *agent == Agent::Codex)
        .filter_map(|(_, path)| Path::new(path).parent().map(Path::to_path_buf))
        .collect();
    for folder in codex {
        match entries(&folder) {
            Ok(None) => {}
            Ok(Some((items, over))) => {
                if over {
                    blockers.push(blocker("codex_directory_entry_bound", &folder));
                }
                let profiles: Vec<String> = items
                    .into_iter()
                    .filter(|(name, _)| name.ends_with(".config.toml"))
                    .map(|(name, _)| name)
                    .collect();
                if profiles.len() > MAX_PROFILES {
                    blockers.push(blocker("codex_profile_count_bound", &folder));
                }
                files.extend(
                    profiles
                        .into_iter()
                        .take(MAX_PROFILES)
                        .map(|name| (Agent::Codex, folder.join(name))),
                );
            }
            Err(_) => blockers.push(blocker("codex_profiles_unreadable_or_redirected", &folder)),
        }
    }
    let mut seen = Vec::new();
    files.retain(|(_, path)| {
        let fresh = !seen.contains(path);
        if fresh {
            seen.push(path.clone());
        }
        fresh
    });
    if files.len() > MAX_USER_FILES {
        files.truncate(MAX_USER_FILES);
        blockers.push(Blocker {
            code: "configuration_file_count_bound".into(),
            file: None,
        });
    }
    (files, blockers)
}
