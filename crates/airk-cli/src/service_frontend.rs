// SPDX-License-Identifier: Apache-2.0
//! Frontend resolution selects existing service registrations, never local stores.
use airk_core::service::{ControlMethod, ControlResponse, ProjectBinding, ProjectRegistration};
use anyhow::Result;
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
    let registration = saved(&control, binding, root, data, collection).await?;
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
    let registration = saved(&control, Some(binding), Path::new(""), Path::new(""), "").await?;
    Ok(Project {
        install: None,
        control,
        registration,
    })
}

fn selected_binding(
    binding: Option<ProjectBinding>,
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<ProjectBinding> {
    match binding {
        Some(binding) => Ok(binding),
        None => Ok(
            airk_kernel::service_projects::planned_registration(root, data, collection)?.binding,
        ),
    }
}
fn registered(response: ControlResponse, binding: ProjectBinding) -> Result<ProjectRegistration> {
    let project = match response {
        ControlResponse::Registered { project } => project,
        ControlResponse::Failure { code } => anyhow::bail!(
            "service project lookup failed: {code}; review registration if the binding is no longer registered"
        ),
        _ => anyhow::bail!("unexpected service project lookup response"),
    };
    anyhow::ensure!(
        project.registration.binding == binding,
        "service returned another project registration"
    );
    Ok(project.registration)
}
async fn saved(
    control: &Path,
    binding: Option<ProjectBinding>,
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<ProjectRegistration> {
    let binding = selected_binding(binding, root, data, collection)?;
    // The console transport authenticates the current executable/version and
    // fences this metadata lookup to its health observation's service instance.
    registered(
        crate::console_client::request(control, ControlMethod::Project { project: binding })
            .await?,
        binding,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved_fixture(
        control: &Path,
        binding: Option<ProjectBinding>,
        root: &Path,
        data: &Path,
        collection: &str,
    ) -> Result<ProjectRegistration> {
        let binding = selected_binding(binding, root, data, collection)?;
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
        registered(
            ControlResponse::Registered {
                project: registry.get(binding)?,
            },
            binding,
        )
    }

    #[test]
    fn single_registration_response_rejects_unknown_and_foreign_projects() -> Result<()> {
        let project = ProjectRegistration::new("/source".into(), "/data".into(), "fixture".into())?;
        let binding = project.binding;
        assert!(
            registered(
                ControlResponse::Failure {
                    code: "unknown_project".into()
                },
                binding
            )
            .is_err()
        );
        let busy = registered(
            ControlResponse::Failure {
                code: "registry_busy".into(),
            },
            binding,
        )
        .unwrap_err();
        assert!(busy.to_string().contains("registry_busy"));
        assert!(!busy.to_string().contains("project is not registered"));
        let response = ControlResponse::Registered {
            project: airk_core::service::RegisteredProject {
                registration: project,
                fencing_generation: 0,
                paused: false,
            },
        };
        assert!(registered(response, ProjectBinding::from_bytes([99; 32])).is_err());
        Ok(())
    }

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
            saved_fixture(
                &control,
                None,
                &base.join("source"),
                &base.join("data"),
                "fixture"
            )?,
            registration
        );
        assert!(
            saved_fixture(
                &control,
                None,
                &base.join("other"),
                &base.join("data"),
                "fixture"
            )
            .is_err()
        );
        let missing = base.join("never-created");
        assert!(saved_fixture(&control, None, &base.join("source"), &missing, "fixture").is_err());
        assert!(!missing.exists());
        std::fs::rename(base.join("source"), base.join("offline"))?;
        assert_eq!(
            saved_fixture(
                &control,
                Some(registration.binding),
                &missing,
                &missing,
                "ignored-display"
            )?,
            registration
        );
        assert!(
            saved_fixture(
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
