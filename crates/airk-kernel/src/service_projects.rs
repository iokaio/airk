// SPDX-License-Identifier: Apache-2.0
//! Canonical service registrations are resolved before touching collection state.
use airk_core::service::ProjectRegistration;
use anyhow::{Context, Result, ensure};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct SourceUnavailable;
impl std::fmt::Display for SourceUnavailable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("source root is unavailable or redirected; retained evidence is preserved")
    }
}
impl std::error::Error for SourceUnavailable {}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Available,
    Unavailable,
    Unknown,
}
#[derive(serde::Serialize)]
pub struct SourceObservation {
    pub state: SourceState,
    pub observed_unix_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<SourceProblem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceProblem {
    Missing,
    PermissionDenied,
    Redirected,
    NotDirectory,
    IoUnavailable,
}
impl std::fmt::Display for SourceProblem {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(match self {
            Self::Missing => "registered source is missing",
            Self::PermissionDenied => "registered source permission denied",
            Self::Redirected => "registered source location is redirected",
            Self::NotDirectory => "registered source is no longer a directory",
            Self::IoUnavailable => "registered source filesystem is unavailable",
        })
    }
}
impl std::error::Error for SourceProblem {}
pub(crate) fn problem(error: &anyhow::Error) -> SourceProblem {
    if let Some(reason) = error.downcast_ref::<SourceProblem>() {
        return *reason;
    }
    match error
        .downcast_ref::<std::io::Error>()
        .map(std::io::Error::kind)
    {
        Some(std::io::ErrorKind::NotFound) => SourceProblem::Missing,
        Some(std::io::ErrorKind::PermissionDenied) => SourceProblem::PermissionDenied,
        Some(std::io::ErrorKind::NotADirectory) => SourceProblem::NotDirectory,
        _ => SourceProblem::IoUnavailable,
    }
}

fn same_spelling(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    if let (Some(a), Some(b)) = (a.to_str(), b.to_str()) {
        return a.strip_prefix(r"\\?\").unwrap_or(a) == b.strip_prefix(r"\\?\").unwrap_or(b);
    }
    false
}

/// Resolve availability for a saved binding. An unavailable path is never a
/// new registration: the caller must verify its existing collection owner before
/// opening retained evidence. No source path is created or followed for capture.
pub(crate) fn retained_root(root: &Path) -> Result<(PathBuf, bool)> {
    match root.canonicalize() {
        Ok(canonical) if same_spelling(root, &canonical) => match std::fs::read_dir(&canonical) {
            Ok(_) => Ok((canonical, true)),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                ) =>
            {
                unavailable_path(root)
            }
            Err(error) => Err(error).context(SourceUnavailable),
        },
        Ok(_) => unavailable_path(root),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::NotADirectory
            ) =>
        {
            unavailable_path(root)
        }
        Err(error) => Err(error).context(SourceUnavailable),
    }
}

fn unavailable_path(root: &Path) -> Result<(PathBuf, bool)> {
    ensure!(
        root.is_absolute()
            && !root
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
        "unavailable retained source requires its exact absolute registered path"
    );
    let mut ancestor = root.to_path_buf();
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                match ancestor.canonicalize() {
                    Ok(mut resolved) => {
                        for name in suffix.into_iter().rev() {
                            resolved.push(name);
                        }
                        // Redirected ancestors must not change the saved owner.
                        return Ok((
                            if same_spelling(root, &resolved) {
                                resolved
                            } else {
                                root.into()
                            },
                            false,
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => (),
                    Err(error) => return Err(error).context(SourceUnavailable),
                }
            }
            Ok(_) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(error) => return Err(error).context(SourceUnavailable),
        }
        suffix.push(
            ancestor
                .file_name()
                .context(SourceUnavailable)?
                .to_os_string(),
        );
        ensure!(ancestor.pop(), SourceUnavailable);
    }
}

pub(crate) fn require_source(root: &Path) -> Result<()> {
    (|| -> Result<()> {
        let canonical = root.canonicalize()?;
        ensure!(same_spelling(&canonical, root), SourceProblem::Redirected);
        ensure!(canonical.is_dir(), SourceProblem::NotDirectory);
        // Canonicalization/stat can succeed while directory access is denied.
        // Open the root without enumerating it; failed scans never imply deletion.
        drop(std::fs::read_dir(root)?);
        Ok(())
    })()
    .context(SourceUnavailable)
}

/// Only for a saved authenticated registration. New registration still requires
/// live canonical roots through validate(), and missing state is never initialized.
pub fn validate_retained(project: &ProjectRegistration) -> Result<()> {
    project.validate()?;
    let root = Path::new(&project.root);
    let (resolved, _) = retained_root(root)?;
    let data = Path::new(&project.data_root);
    ensure!(
        resolved == root
            && data.canonicalize()? == data
            && data.is_dir()
            && !root.starts_with(data),
        "retained registration identity changed"
    );
    Ok(())
}

pub fn registration(root: &Path, data: &Path, collection: &str) -> Result<ProjectRegistration> {
    airk_store::validate_collection(collection)?;
    let root = root.canonicalize().context("resolve service source root")?;
    let data = data.canonicalize().context("resolve service data root")?;
    ensure!(
        root.is_dir() && data.is_dir(),
        "service roots must be directories"
    );
    ensure!(
        !root.starts_with(&data),
        "service data must not contain sources"
    );
    // Keep identities lossless across the native and wire boundaries.
    ProjectRegistration::new(
        root.to_str().context("source root is not UTF-8")?.into(),
        data.to_str().context("data root is not UTF-8")?.into(),
        collection.into(),
    )
}

pub fn validate(project: &ProjectRegistration) -> Result<()> {
    project.validate()?;
    ensure!(
        registration(
            Path::new(&project.root),
            Path::new(&project.data_root),
            &project.collection
        )? == *project,
        "service registration must use canonical roots"
    );
    Ok(())
}

/// Prepare a metadata reservation. Only storage may be missing; sources must
/// already exist. The service serializes this reservation against every source
/// and private-state boundary before an adapter may create the storage path.
pub fn planned_registration(
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<ProjectRegistration> {
    airk_store::validate_collection(collection)?;
    let root = root.canonicalize().context("resolve service source root")?;
    ensure!(root.is_dir(), "service source must be a directory");
    let data = airk_store::service::data_scope::canonical_location(data)?;
    ensure!(
        !root.starts_with(&data) && !data.starts_with(&root),
        "service sources and planned storage must be disjoint"
    );
    ProjectRegistration::new(
        root.to_str().context("source root is not UTF-8")?.into(),
        data.to_str().context("data root is not UTF-8")?.into(),
        collection.into(),
    )
}

pub fn validate_planned(project: &ProjectRegistration) -> Result<()> {
    project.validate()?;
    ensure!(
        planned_registration(
            Path::new(&project.root),
            Path::new(&project.data_root),
            &project.collection,
        )? == *project,
        "planned registration identity changed"
    );
    Ok(())
}

#[cfg(test)]
#[path = "service_source_availability_tests.rs"]
mod tests;
