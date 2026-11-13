// SPDX-License-Identifier: Apache-2.0
//! M22/M23 (U100): the service's own split locations with sizes, and purges
//! of cache contents or closed log files only. Measurement and planning never
//! follow links or junctions; a purge applies exactly the current plan its
//! digest names. Every function here blocks; call it off the async workers.
use airk_core::{
    identity::Sha256Digest,
    install_record::Locations,
    storage_admin::{
        MAX_INVENTORY_ENTRIES, MAX_PURGE_FILES, PurgeFile, PurgeRule, PurgedFile, STORAGE_SCHEMA,
        StorageInventory, StorageLocation, StorageLocationKind, StoragePurgePlan,
        StoragePurgeReceipt,
    },
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

/// The purge's current plan differs from the one the caller reviewed.
pub(super) const PLAN_CHANGED: &str = "storage_purge_plan_changed";
/// Program, data and state are never purged here.
pub(super) const NOT_PURGEABLE: &str = "storage_location_not_purgeable";
const LINKED: &str = "storage_location_linked";
const OVERLAPS: &str = "storage_location_overlaps";
const LOGS_ELSEWHERE: &str = "storage_logs_location_differs";

/// What the inventory does not see, stated so it is never read as
/// "everything AIRK stores".
const NOT_COVERED: [&str; 4] = [
    "index data roots chosen outside the data location",
    "agent configurations written into workspaces or agent applications",
    "exports, backups and copies you saved elsewhere",
    "content already sent to agent providers",
];

/// The writer of the log rings (U100: closed files are listed and deleted on
/// its thread, under each ring's writer lock).
pub(super) trait ClosedLogs {
    /// The folder the rings live in.
    fn folder(&self) -> Result<PathBuf>;
    /// The closed files, by path relative to `folder`, sorted.
    fn list(&self) -> Result<Vec<PurgeFile>>;
    /// Delete exactly `plan` if it is still the closed files; one result per
    /// planned file, in plan order.
    fn delete(&self, plan: &[PurgeFile]) -> Result<Vec<PurgedFile>>;
}

impl ClosedLogs for airk_host::service_logs::Log {
    fn folder(&self) -> Result<PathBuf> {
        self.log_folder()
    }
    fn list(&self) -> Result<Vec<PurgeFile>> {
        self.closed_log_files()
    }
    fn delete(&self, plan: &[PurgeFile]) -> Result<Vec<PurgedFile>> {
        self.delete_closed_logs(plan)
    }
}

/// The control-plane failure code of an error from this module: its leading
/// `code:` when it has one, else `fallback`.
pub(super) fn failure_code(error: &anyhow::Error, fallback: &str) -> String {
    let message = error.to_string();
    let code = message.split(':').next().unwrap_or_default();
    if !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        code.to_owned()
    } else {
        fallback.to_owned()
    }
}

fn location(kind: StorageLocationKind, locations: &Locations) -> &str {
    match kind {
        StorageLocationKind::Program => &locations.program,
        StorageLocationKind::Data => &locations.data,
        StorageLocationKind::State => &locations.state,
        StorageLocationKind::Cache => &locations.cache,
        StorageLocationKind::Logs => &locations.logs,
    }
}

/// A symbolic link, junction or other reparse point: never followed.
fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

#[derive(Debug, PartialEq, Eq)]
struct Measured {
    bytes: u64,
    files: u64,
    complete: bool,
}

/// Size of one location within the shared entry budget. A missing folder is
/// empty and complete; a linked location, an unreadable part or an exhausted
/// budget leaves the sizes as lower bounds (`complete: false`).
fn measure(root: &Path, budget: &mut usize) -> Measured {
    let mut measured = Measured {
        bytes: 0,
        files: 0,
        complete: true,
    };
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return measured,
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => {}
        Ok(metadata) if metadata.is_file() && !is_link(&metadata) => {
            measured.bytes = metadata.len();
            measured.files = 1;
            return measured;
        }
        _ => {
            measured.complete = false;
            return measured;
        }
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            measured.complete = false;
            continue;
        };
        for entry in entries {
            if *budget == 0 {
                measured.complete = false;
                return measured;
            }
            *budget -= 1;
            let Ok(entry) = entry else {
                measured.complete = false;
                continue;
            };
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                measured.complete = false;
                continue;
            };
            if is_link(&metadata) {
                // Counted against the budget, never followed or measured.
            } else if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                measured.files += 1;
                measured.bytes = measured.bytes.saturating_add(metadata.len());
            }
        }
    }
    measured
}

/// M22: the five locations in order, with sizes and the reviewed purge rule.
pub(super) fn inventory(locations: &Locations) -> Result<StorageInventory> {
    let mut budget = MAX_INVENTORY_ENTRIES;
    let inventory = StorageInventory {
        schema: STORAGE_SCHEMA,
        locations: StorageLocationKind::ALL
            .into_iter()
            .map(|kind| {
                let path = location(kind, locations);
                let measured = measure(Path::new(path), &mut budget);
                StorageLocation {
                    kind,
                    path: path.to_owned(),
                    bytes: measured.bytes,
                    files: measured.files,
                    complete: measured.complete,
                    purge: kind.purge_rule(),
                }
            })
            .collect(),
        not_covered: NOT_COVERED.iter().map(|text| (*text).to_owned()).collect(),
    };
    inventory.validate()?;
    Ok(inventory)
}

fn normal(path: &Path) -> Result<PathBuf> {
    airk_host::user_paths::canonical_location(&std::path::absolute(path)?)
}

/// The locations of the installation this service serves: the install
/// record's, when its `state` is `control` (`storage_not_installed` for a
/// service run from another control folder, or with no installation).
pub(super) fn served_locations(control: &Path) -> Result<Locations> {
    let locations = crate::setup_role::installed_locations()?
        .context("storage_not_installed: no AIRK Service installation")?;
    ensure!(
        normal(control)? == normal(Path::new(&locations.state))?,
        "storage_not_installed: this service does not run from the installation's state folder"
    );
    Ok(locations)
}

/// The cache shares no folder with another location (neither contains the
/// other), so purging it can never reach program, data, state or logs.
fn require_separate_cache(locations: &Locations) -> Result<()> {
    let cache = normal(Path::new(&locations.cache))?;
    for kind in StorageLocationKind::ALL {
        if kind == StorageLocationKind::Cache {
            continue;
        }
        let other = normal(Path::new(location(kind, locations)))?;
        ensure!(
            !other.starts_with(&cache) && !cache.starts_with(&other),
            "{OVERLAPS}: the cache location shares a folder with the {kind:?} location"
        );
    }
    Ok(())
}

/// Pure: a file name a plan can carry (`StoragePurgePlan::validate`).
fn plannable(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

/// Every regular file under the cache, sorted, at most `MAX_PURGE_FILES`.
/// Links are neither followed nor listed. `truncated` when files were left
/// out (the bound, the entry budget, an unreadable folder or a name a plan
/// cannot carry); a later plan picks them up.
fn cache_files(root: &Path) -> Result<(Vec<PurgeFile>, bool)> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), false));
        }
        Err(error) => return Err(error).context("cache location unreadable"),
        Ok(metadata) if is_link(&metadata) => {
            bail!("{LINKED}: the cache location is a link or junction")
        }
        Ok(metadata) => ensure!(metadata.is_dir(), "cache location is not a folder"),
    }
    let mut files = Vec::new();
    let mut truncated = false;
    let mut budget = MAX_INVENTORY_ENTRIES;
    let mut pending = vec![(root.to_path_buf(), String::new())];
    'walk: while let Some((directory, prefix)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            truncated = true;
            continue;
        };
        for entry in entries {
            if budget == 0 {
                truncated = true;
                break 'walk;
            }
            budget -= 1;
            let Ok(entry) = entry else {
                truncated = true;
                continue;
            };
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                truncated = true;
                continue;
            };
            if is_link(&metadata) {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| plannable(name)) else {
                truncated = true;
                continue;
            };
            let relative = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if relative.len() > 4096 {
                truncated = true;
            } else if metadata.is_dir() {
                pending.push((entry.path(), relative));
            } else if metadata.is_file() {
                files.push(PurgeFile {
                    relative,
                    bytes: metadata.len(),
                });
            }
        }
    }
    files.sort();
    if files.len() > MAX_PURGE_FILES {
        files.truncate(MAX_PURGE_FILES);
        truncated = true;
    }
    Ok((files, truncated))
}

fn require_logs_location(logs: &dyn ClosedLogs, locations: &Locations) -> Result<()> {
    ensure!(
        normal(&logs.folder()?)? == normal(Path::new(&locations.logs))?,
        "{LOGS_ELSEWHERE}: this service writes its logs outside the installation's logs location"
    );
    Ok(())
}

/// M23: the exact files a purge of `kind` would delete now. Deletes nothing.
pub(super) fn purge_plan(
    kind: StorageLocationKind,
    locations: &Locations,
    logs: &dyn ClosedLogs,
) -> Result<StoragePurgePlan> {
    let path = location(kind, locations);
    let (files, truncated) = match kind.purge_rule() {
        PurgeRule::NotPurgeable => {
            bail!("{NOT_PURGEABLE}: the {kind:?} location is never purged")
        }
        PurgeRule::CacheContents => {
            require_separate_cache(locations)?;
            cache_files(Path::new(path))?
        }
        PurgeRule::ClosedLogs => {
            require_logs_location(logs, locations)?;
            let mut files = logs.list()?;
            files.sort();
            ensure!(files.len() <= MAX_PURGE_FILES, "too many closed log files");
            (files, false)
        }
    };
    let plan = StoragePurgePlan {
        schema: STORAGE_SCHEMA,
        kind,
        path: path.to_owned(),
        files,
        truncated,
    };
    plan.validate()?;
    Ok(plan)
}

/// M23: delete exactly the plan `plan_digest` names if it is still the
/// current plan (`storage_purge_plan_changed` otherwise).
pub(super) fn purge(
    kind: StorageLocationKind,
    plan_digest: Sha256Digest,
    locations: &Locations,
    logs: &dyn ClosedLogs,
) -> Result<StoragePurgeReceipt> {
    let plan = purge_plan(kind, locations, logs)?;
    ensure!(
        plan.digest()? == plan_digest,
        "{PLAN_CHANGED}: the files to delete changed since the plan; review a new plan"
    );
    let files = match kind.purge_rule() {
        PurgeRule::NotPurgeable => bail!("{NOT_PURGEABLE}"),
        PurgeRule::CacheContents => delete_cache(Path::new(&plan.path), &plan.files),
        PurgeRule::ClosedLogs => logs.delete(&plan.files)?,
    };
    ensure!(
        files.len() == plan.files.len()
            && files
                .iter()
                .zip(&plan.files)
                .all(|(done, planned)| done.relative == planned.relative),
        "purge results do not match the plan"
    );
    let complete = !plan.truncated && files.iter().all(|file| file.deleted);
    Ok(StoragePurgeReceipt {
        schema: STORAGE_SCHEMA,
        kind,
        path: plan.path,
        plan_digest,
        files,
        complete,
    })
}

/// The folder still is a real directory (not replaced by a link).
fn real_directory(path: &Path) -> std::result::Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(()),
        Ok(_) => Err("changed: a folder on its path is no longer a plain folder".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err("gone".into()),
        Err(error) => Err(bounded(format!("in_use_or_refused: {error}"))),
    }
}

fn bounded(text: String) -> String {
    text.chars().take(256).collect()
}

/// Delete one planned cache file after re-checking, without following links,
/// that each folder on its path is plain and that it is the planned file.
fn delete_cache_file(root: &Path, file: &PurgeFile) -> std::result::Result<(), String> {
    let mut path = root.to_path_buf();
    real_directory(&path)?;
    let mut parts = file.relative.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        if parts.peek().is_some() {
            real_directory(&path)?;
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !is_link(&metadata) => {
            if metadata.len() != file.bytes {
                return Err("changed: its size differs from the plan".into());
            }
        }
        Ok(_) => return Err("changed: no longer a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err("gone".into()),
        Err(error) => return Err(bounded(format!("in_use_or_refused: {error}"))),
    }
    fs::remove_file(&path).map_err(|error| bounded(format!("in_use_or_refused: {error}")))
}

/// Delete the planned cache files, then the folders below the cache that
/// those deletions left empty (deepest first; a folder still holding
/// anything stays).
fn delete_cache(root: &Path, files: &[PurgeFile]) -> Vec<PurgedFile> {
    let mut folders = BTreeSet::new();
    let results = files
        .iter()
        .map(|file| {
            let outcome = delete_cache_file(root, file);
            if outcome.is_ok() {
                let parts: Vec<&str> = file.relative.split('/').collect();
                for depth in 1..parts.len() {
                    folders.insert((std::cmp::Reverse(depth), parts[..depth].join("/")));
                }
            }
            PurgedFile {
                relative: file.relative.clone(),
                deleted: outcome.is_ok(),
                retained: outcome.err(),
            }
        })
        .collect();
    for (_, relative) in folders {
        let folder = relative
            .split('/')
            .fold(root.to_path_buf(), |path, part| path.join(part));
        if real_directory(&folder).is_ok() {
            let _ = fs::remove_dir(folder);
        }
    }
    results
}
