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
}

/// Missing roots may select already-bound retained state. Resolve the nearest
/// existing ancestor to reject path aliases and redirected parents without
/// creating the root or treating its absence as observed file deletions.
pub(crate) fn retained_root(root: &Path) -> Result<(PathBuf, bool)> {
    match root.canonicalize() {
        Ok(canonical) => {
            ensure!(canonical.is_dir(), SourceUnavailable);
            Ok((canonical, true))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(
                root.is_absolute()
                    && !root.components().any(|component| matches!(
                        component,
                        Component::CurDir | Component::ParentDir
                    )),
                "missing retained source requires its exact absolute registered path"
            );
            let mut ancestor = root.to_path_buf();
            let mut missing = Vec::new();
            loop {
                match std::fs::symlink_metadata(&ancestor) {
                    Ok(metadata) => {
                        ensure!(
                            metadata.is_dir() && !metadata.file_type().is_symlink(),
                            SourceUnavailable
                        );
                        let mut resolved = ancestor.canonicalize().context(SourceUnavailable)?;
                        for name in missing.into_iter().rev() {
                            resolved.push(name);
                        }
                        ensure!(resolved == root, SourceUnavailable);
                        return Ok((resolved, false));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing.push(
                            ancestor
                                .file_name()
                                .context(SourceUnavailable)?
                                .to_os_string(),
                        );
                        ensure!(ancestor.pop(), SourceUnavailable);
                    }
                    Err(error) => return Err(error).context(SourceUnavailable),
                }
            }
        }
        Err(error) => Err(error).context(SourceUnavailable),
    }
}

pub(crate) fn require_source(root: &Path) -> Result<()> {
    let canonical = root.canonicalize().context(SourceUnavailable)?;
    ensure!(canonical == root && canonical.is_dir(), SourceUnavailable);
    Ok(())
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
