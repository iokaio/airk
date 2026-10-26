// SPDX-License-Identifier: Apache-2.0
//! Frontend resolution selects existing service registrations, never local stores.
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Project {
    pub install: Option<PathBuf>,
    pub control: PathBuf,
    pub registration: ProjectRegistration,
}

pub async fn resolve(
    install: Option<&Path>,
    binding: Option<ProjectBinding>,
    root: &Path,
    data: Option<&Path>,
    collection: &str,
) -> Result<Project> {
    let install = match install {
        Some(path) => path.to_path_buf(),
        None => crate::runtime_install::caller_install_root()?,
    };
    let control = crate::runtime_install::installed_control(&install).await?;
    let default_data;
    let data = match data {
        Some(data) => data,
        None => {
            default_data = crate::runtime_install::default_project_data(&install)?;
            &default_data
        }
    };
    let registration = saved(&control, binding, root, data, collection)?;
    Ok(Project {
        install: Some(install),
        control,
        registration,
    })
}

/// Explicit development transport; it never selects or starts another service.
pub async fn resolve_control(control: &Path, binding: ProjectBinding) -> Result<Project> {
    let control = control.canonicalize()?;
    crate::runtime_install::validate_control_root(&control)?;
    anyhow::ensure!(
        matches!(
            crate::console_client::request(&control, airk_core::service::ControlMethod::Health)
                .await?,
            airk_core::service::ControlResponse::Health { .. }
        ),
        "native service health unavailable"
    );
    let registration = saved(&control, Some(binding), Path::new(""), Path::new(""), "")?;
    Ok(Project {
        install: None,
        control,
        registration,
    })
}

fn saved(
    control: &Path,
    binding: Option<ProjectBinding>,
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<ProjectRegistration> {
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let binding = match binding {
        Some(binding) => binding,
        None => {
            airk_kernel::service_projects::planned_registration(root, data, collection)?.binding
        }
    };
    Ok(registry.get(binding).context("project is not registered with this service; review registration before indexing or connecting")?.registration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_binding_retains_offline_sources_but_other_roots_cannot_adopt_it() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let base = fixture.path().canonicalize()?;
        for name in ["control", "source", "other", "data"] {
            std::fs::create_dir(base.join(name))?;
        }
        let control = base.join("control");
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let registration = airk_kernel::service_projects::registration(
            &base.join("source"),
            &base.join("data"),
            "fixture",
        )?;
        registry.register(&registration)?;
        assert_eq!(
            saved(
                &control,
                None,
                &base.join("source"),
                &base.join("data"),
                "fixture"
            )?,
            registration
        );
        assert!(
            saved(
                &control,
                None,
                &base.join("other"),
                &base.join("data"),
                "fixture"
            )
            .is_err()
        );
        let missing = base.join("never-created");
        assert!(saved(&control, None, &base.join("source"), &missing, "fixture").is_err());
        assert!(!missing.exists());
        std::fs::rename(base.join("source"), base.join("offline"))?;
        assert_eq!(
            saved(
                &control,
                Some(registration.binding),
                &missing,
                &missing,
                "ignored-display"
            )?,
            registration
        );
        assert!(
            saved(
                &control,
                None,
                &base.join("source"),
                &base.join("data"),
                "fixture"
            )
            .is_err()
        );
        Ok(())
    }
}
