// SPDX-License-Identifier: Apache-2.0
//! Explicit bounded repository-marker hints. Never registration or Git execution.
use airk_core::{index_catalog::encoded_size, source_observations::*};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MARKERS: [&str; 4] = [".git", ".hg", ".svn", ".jj"];

/// Inputs are location hints, not authority. No candidate location is opened in
/// classification; only the explicitly selected root is inspected below.
#[derive(Default)]
pub struct BroadRootLocations {
    pub home: Option<PathBuf>,
    pub temporary: Option<PathBuf>,
    pub synced: Vec<PathBuf>,
}
impl BroadRootLocations {
    pub fn native() -> Self {
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let synced = ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"]
            .into_iter()
            .filter_map(std::env::var_os)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect();
        Self {
            home,
            temporary: Some(std::env::temp_dir()),
            synced,
        }
    }
}

fn comparison_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = path.to_str() {
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = path.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.into()
}
fn same_hint(root: &Path, hint: &Path) -> Result<bool> {
    Ok(airk_store::service::registry::source_relation(
        &comparison_path(root),
        &comparison_path(hint),
    )? == Some(SourceRelation::Same))
}
pub fn broad_root_reasons(
    root: &Path,
    locations: &BroadRootLocations,
) -> Result<Vec<BroadRootKind>> {
    ensure!(
        locations.synced.len() <= 16,
        "too many synced location hints"
    );
    let mut reasons = BTreeSet::new();
    if root.is_absolute() && root.parent().is_none() {
        reasons.insert(BroadRootKind::VolumeRoot);
    }
    if let Some(home) = &locations.home
        && same_hint(root, home)?
    {
        reasons.insert(BroadRootKind::Home);
    }
    if let Some(temporary) = &locations.temporary
        && same_hint(root, temporary)?
    {
        reasons.insert(BroadRootKind::Temporary);
    }
    for home in locations.home.iter().chain(&locations.synced) {
        for (name, kind) in [
            ("Desktop", BroadRootKind::Desktop),
            ("Documents", BroadRootKind::Documents),
            ("Downloads", BroadRootKind::Downloads),
        ] {
            if same_hint(root, &home.join(name))? {
                reasons.insert(kind);
            }
        }
    }
    for synced in &locations.synced {
        if same_hint(root, synced)? {
            reasons.insert(BroadRootKind::Synced);
        }
    }
    Ok(reasons.into_iter().collect())
}

fn alias(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn stopped(observation: &mut RepositoryObservation, deadline: Instant) -> bool {
    if Instant::now() >= deadline {
        observation.deadline_exhausted = true;
        true
    } else {
        false
    }
}

fn markers(
    root: &Path,
    relative: &Path,
    observation: &mut RepositoryObservation,
    deadline: Instant,
) -> Result<()> {
    for name in MARKERS {
        if stopped(observation, deadline) {
            break;
        }
        observation.marker_checks += 1;
        let locator = relative.join(name);
        let mut marker = RepositoryMarker {
            relative_root: relative
                .to_str()
                .context("repository child is not UTF-8")?
                .into(),
            marker: name.into(),
            entry_kind: MarkerEntryKind::Unknown,
            gitdir_target: None,
            target_state: GitTargetState::NotApplicable,
        };
        match fs::symlink_metadata(root.join(&locator)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                observation.observation_errors += 1;
                marker.target_state = GitTargetState::Unavailable;
            }
            Ok(metadata) => {
                marker.entry_kind = if alias(&metadata) {
                    MarkerEntryKind::Alias
                } else if metadata.is_dir() {
                    MarkerEntryKind::Directory
                } else if metadata.is_file() {
                    MarkerEntryKind::File
                } else {
                    MarkerEntryKind::Other
                };
                if name == ".git" && marker.entry_kind == MarkerEntryKind::File {
                    observation.git_marker_reads += 1;
                    observation.git_marker_read_budget_bytes += 4097;
                    match airk_ingest::read_git_marker(root, &locator) {
                        Err(_) => {
                            observation.observation_errors += 1;
                            marker.target_state = GitTargetState::Unavailable;
                        }
                        Ok(bytes) => {
                            observation.git_marker_bytes_captured += bytes.len();
                            marker.target_state = if bytes.len() > 4096 {
                                GitTargetState::Truncated
                            } else if let Ok(text) = std::str::from_utf8(&bytes)
                                && let Some(target) = text
                                    .strip_prefix("gitdir:")
                                    .map(str::trim)
                                    .filter(|target| {
                                        !target.is_empty() && !target.contains(['\r', '\n', '\0'])
                                    })
                            {
                                marker.gitdir_target = Some(target.into());
                                GitTargetState::Captured
                            } else {
                                GitTargetState::Invalid
                            };
                        }
                    }
                }
            }
        }
        observation.repository_markers.push(marker);
        if encoded_size(observation, MAX_REPOSITORY_BYTES - 512).is_err() {
            observation.repository_markers.pop();
            observation.reply_truncated = true;
            return Ok(());
        }
    }
    Ok(())
}

pub fn observe_repository(root: &Path, inspect_children: bool) -> Result<RepositoryObservation> {
    observe_repository_with_locations(root, inspect_children, &BroadRootLocations::native())
}
pub fn observe_repository_with_locations(
    root: &Path,
    inspect_children: bool,
    locations: &BroadRootLocations,
) -> Result<RepositoryObservation> {
    let deadline = Instant::now() + Duration::from_secs(2);
    observe_repository_at(root, inspect_children, locations, deadline)
}

fn observe_repository_at(
    root: &Path,
    inspect_children: bool,
    locations: &BroadRootLocations,
    deadline: Instant,
) -> Result<RepositoryObservation> {
    let root = root.canonicalize()?;
    ensure!(root.is_dir(), "repository observation requires a directory");
    let display = root
        .to_str()
        .context("repository observation root is not UTF-8")?;
    ensure!(
        display.len() <= 4096,
        "repository observation root exceeds byte bound"
    );
    let mut observation = RepositoryObservation {
        schema: 1,
        root: display.into(),
        observed_unix_ms: u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?,
        broad_root_reasons: broad_root_reasons(&root, locations)?,
        repository_markers: vec![],
        children_requested: inspect_children,
        child_entries_observed: 0,
        child_directories_inspected: 0,
        child_aliases_skipped: 0,
        non_utf8_children_skipped: 0,
        marker_checks: 0,
        git_marker_reads: 0,
        git_marker_bytes_captured: 0,
        git_marker_read_budget_bytes: 0,
        observation_errors: 0,
        children_truncated: false,
        reply_truncated: false,
        deadline_exhausted: false,
    };
    encoded_size(&observation, MAX_REPOSITORY_BYTES)?;
    markers(&root, Path::new(""), &mut observation, deadline)?;
    if inspect_children && !observation.reply_truncated && !observation.deadline_exhausted {
        match fs::read_dir(&root) {
            Err(_) => observation.observation_errors += 1,
            Ok(entries) => {
                for (number, entry) in entries.take(MAX_CHILD_ENTRIES + 1).enumerate() {
                    if stopped(&mut observation, deadline) {
                        break;
                    }
                    observation.child_entries_observed += 1;
                    if number == MAX_CHILD_ENTRIES {
                        observation.children_truncated = true;
                        break;
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => {
                            observation.observation_errors += 1;
                            continue;
                        }
                    };
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else {
                        observation.non_utf8_children_skipped += 1;
                        continue;
                    };
                    if MARKERS.contains(&name) || name == ".airk" {
                        continue;
                    }
                    let metadata = match fs::symlink_metadata(entry.path()) {
                        Ok(metadata) => metadata,
                        Err(_) => {
                            observation.observation_errors += 1;
                            continue;
                        }
                    };
                    if alias(&metadata) {
                        observation.child_aliases_skipped += 1;
                        continue;
                    }
                    if !metadata.is_dir() {
                        continue;
                    }
                    observation.child_directories_inspected += 1;
                    markers(&root, Path::new(name), &mut observation, deadline)?;
                    if observation.reply_truncated {
                        break;
                    }
                }
            }
        }
    }
    observation.repository_markers.sort_by(|left, right| {
        (&left.relative_root, &left.marker).cmp(&(&right.relative_root, &right.marker))
    });
    encoded_size(&observation, MAX_REPOSITORY_BYTES)?;
    Ok(observation)
}

#[cfg(test)]
#[path = "source_observations_tests.rs"]
mod tests;
