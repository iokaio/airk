// SPDX-License-Identifier: Apache-2.0
//! Trusted project setup reserves storage before creating any directories.
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

pub fn prepare(root: &Path, data: &Path, collection: &str) -> Result<ProjectRegistration> {
    airk_kernel::service_projects::planned_registration(root, data, collection)
}

/// Only a completed authenticated registry reservation authorizes creation.
/// A failed creation leaves that exact reservation intact for explicit retry.
pub fn prepare_storage(control: &Path, binding: ProjectBinding) -> Result<ProjectRegistration> {
    crate::runtime_install::validate_control_root(control)?;
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let project = registry.get(binding)?.registration;
    create_reserved_storage(&project)?;
    Ok(project)
}

fn create_reserved_storage(project: &ProjectRegistration) -> Result<()> {
    airk_kernel::service_projects::validate_planned(project)?;
    let data = Path::new(&project.data_root);
    let mut ancestor = data.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .context("storage ancestor missing")?
                        .to_owned(),
                );
                ensure!(ancestor.pop(), "storage ancestor missing");
            }
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        ancestor.is_dir() && ancestor.canonicalize()? == ancestor,
        "reserved storage ancestor changed"
    );
    for name in missing.into_iter().rev() {
        ensure!(
            ancestor.canonicalize()? == ancestor,
            "reserved storage ancestor redirected"
        );
        ancestor.push(name);
        match fs::create_dir(&ancestor) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        ensure!(
            ancestor.is_dir() && ancestor.canonicalize()? == ancestor,
            "reserved storage redirected"
        );
    }
    airk_kernel::service_projects::validate(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_is_read_only_and_reserved_creation_is_repeatable() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        fs::create_dir(&root)?;
        let data = directory.path().join("new").join("data");
        let project = prepare(&root, &data, "fixture")?;
        assert!(!data.exists());
        create_reserved_storage(&project)?;
        create_reserved_storage(&project)?;
        assert!(data.is_dir());
        assert!(fs::read_dir(&root)?.next().is_none());
        Ok(())
    }

    #[test]
    fn preparation_refuses_source_storage_overlap_without_writes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().canonicalize()?;
        let child = source.join("data");
        assert!(prepare(&source, &child, "fixture").is_err());
        assert!(!child.exists());
        assert!(prepare(&child, &source, "fixture").is_err());
        Ok(())
    }

    #[test]
    fn another_projects_source_blocks_storage_reservation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for name in ["control", "source-a", "source-b", "data"] {
            fs::create_dir(directory.path().join(name))?;
        }
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &directory.path().join("control"),
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let first = prepare(
            &directory.path().join("source-a"),
            &directory.path().join("data"),
            "first",
        )?;
        registry.register(&first)?;
        let nested = directory.path().join("source-a").join("never-created");
        let second = prepare(&directory.path().join("source-b"), &nested, "second")?;
        assert!(registry.register(&second).is_err());
        assert!(!nested.exists());
        Ok(())
    }

    #[test]
    fn missing_storage_reservation_blocks_later_overlapping_sources() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let base = directory.path().canonicalize()?;
        for name in ["control", "source", "other-data", "future"] {
            fs::create_dir(base.join(name))?;
        }
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &base.join("control"),
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let reserved = prepare(
            &base.join("source"),
            &base.join("future").join("data"),
            "reserved",
        )?;
        registry.register(&reserved)?;
        let competing = prepare(&base.join("future"), &base.join("other-data"), "competing")?;
        assert!(registry.register(&competing).is_err());
        assert!(!base.join("future").join("data").exists());
        create_reserved_storage(&reserved)?;
        registry.register(&reserved)?;
        assert_eq!(registry.get(reserved.binding)?.registration, reserved);
        Ok(())
    }
}
