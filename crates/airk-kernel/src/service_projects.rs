// SPDX-License-Identifier: Apache-2.0
//! Canonical service registrations are resolved before touching collection state.
use airk_core::service::ProjectRegistration;
use anyhow::{Context, Result, ensure};
use std::path::Path;

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
