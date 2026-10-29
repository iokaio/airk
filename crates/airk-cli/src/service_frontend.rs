// SPDX-License-Identifier: Apache-2.0
//! Frontend resolution selects existing service registrations, never local stores.
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// A configured source scope is stable across storage replacement and re-registration.
#[derive(Clone)]
pub struct RootScope {
    pub root: PathBuf,
    pub collection: Option<String>,
    pub key: String,
}
pub enum RootResolution {
    Registered(ProjectRegistration),
    Missing,
    Removed,
    Ambiguous,
}
impl RootScope {
    pub fn new(root: &Path, collection: Option<String>) -> Result<Self> {
        if let Some(value) = &collection {
            anyhow::ensure!(
                !value.is_empty()
                    && value.len() <= 128
                    && value != "."
                    && value != ".."
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
                "invalid configured index collection"
            );
        }
        let root = crate::user_paths::canonical_location(&std::path::absolute(root)?)?;
        let key = airk_core::identity::framed_digest(
            b"airk-mcp-source-scope/1",
            &[
                root.as_os_str().as_encoded_bytes(),
                collection.as_deref().unwrap_or("").as_bytes(),
            ],
        )
        .to_string();
        Ok(Self {
            root,
            collection,
            key,
        })
    }
    pub fn resolve(&self, control: &Path) -> Result<RootResolution> {
        if !control.join("service-1.4.sqlite3").try_exists()? {
            return Ok(RootResolution::Missing);
        }
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
        let mut matching = Vec::new();
        let mut seen = 0;
        let mut revision = None;
        for inactive in [false, true] {
            let mut after = None;
            loop {
                let page = if inactive {
                    registry.list_inactive(after, revision, 32)?
                } else {
                    registry.list(after, revision, 32)?
                };
                revision = Some(page.revision);
                for item in page.projects {
                    seen += 1;
                    anyhow::ensure!(
                        seen <= 1024,
                        "configured source lookup exceeds registry bound"
                    );
                    let registration = item.registration;
                    if Path::new(&registration.root) == self.root
                        && self
                            .collection
                            .as_ref()
                            .is_none_or(|name| *name == registration.collection)
                    {
                        matching.push(registration);
                    }
                }
                after = page.after;
                if after.is_none() {
                    break;
                }
            }
            if !matching.is_empty() {
                return Ok(if inactive {
                    RootResolution::Removed
                } else if matching.len() == 1 {
                    RootResolution::Registered(matching.remove(0))
                } else {
                    RootResolution::Ambiguous
                });
            }
        }
        Ok(
            if registry
                .source_was_removed(self.root.to_str().context("non-UTF-8 configured source")?)?
            {
                RootResolution::Removed
            } else {
                RootResolution::Missing
            },
        )
    }
}

/// Resolve only installation/control metadata; no project is needed for MCP discovery.
pub fn root_mcp_control(install: Option<&Path>) -> Result<(PathBuf, PathBuf)> {
    let install = install
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(crate::runtime_install::caller_install_root)?;
    let control = crate::runtime_install::installed_metadata_control(&install)?;
    Ok((install, control))
}

pub struct Project {
    pub install: Option<PathBuf>,
    pub control: PathBuf,
    pub registration: ProjectRegistration,
}

/// MCP initialization/help uses saved bindings only; native reads elect lazily.
pub fn resolve_mcp(
    install: Option<&Path>,
    control: Option<&Path>,
    binding: Option<ProjectBinding>,
    root: &Path,
    data: Option<&Path>,
    collection: &str,
) -> Result<Project> {
    if let Some(control) = control {
        let control = control.canonicalize()?;
        crate::runtime_install::validate_control_root(&control)?;
        let registration = saved(
            &control,
            Some(binding.context("project binding required")?),
            root,
            data.unwrap_or(root),
            collection,
        )?;
        return Ok(Project {
            install: None,
            control,
            registration,
        });
    }
    let install = install
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(crate::runtime_install::caller_install_root)?;
    let control = crate::runtime_install::installed_metadata_control(&install)?;
    let default_data = crate::runtime_install::default_project_data(&install)?;
    let registration = saved(
        &control,
        binding,
        root,
        data.unwrap_or(&default_data),
        collection,
    )?;
    Ok(Project {
        install: Some(install),
        control,
        registration,
    })
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
    fn root_scope_survives_replaced_storage_and_refuses_ambiguous_indexes() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let base = fixture.path().canonicalize()?;
        for name in ["source", "other", "data-a", "data-b", "control"] {
            std::fs::create_dir(base.join(name))?;
        }
        let control = base.join("control");
        let scope = RootScope::new(&base.join("source"), None)?;
        assert!(matches!(scope.resolve(&control)?, RootResolution::Missing));
        assert!(
            !control.join("service-1.4.sqlite3").exists(),
            "observation must not create a registry"
        );
        let first = airk_kernel::service_projects::registration(
            &scope.root,
            &base.join("data-a"),
            "first",
        )?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        registry.register(&first)?;
        assert!(
            matches!(scope.resolve(&control)?,RootResolution::Registered(p) if p.binding==first.binding)
        );
        drop(registry);
        std::fs::rename(&control, base.join("old-control"))?;
        std::fs::create_dir(&control)?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        assert!(matches!(scope.resolve(&control)?, RootResolution::Missing));
        let replacement = airk_kernel::service_projects::registration(
            &scope.root,
            &base.join("data-b"),
            "replacement",
        )?;
        registry.register(&replacement)?;
        assert_ne!(first.binding, replacement.binding);
        assert_eq!(scope.key, RootScope::new(&scope.root, None)?.key);
        assert!(
            matches!(scope.resolve(&control)?,RootResolution::Registered(p) if p.binding==replacement.binding)
        );
        let foreign = airk_kernel::service_projects::registration(
            &base.join("other"),
            &base.join("data-a"),
            "foreign",
        )?;
        registry.register(&foreign)?;
        assert!(
            matches!(scope.resolve(&control)?,RootResolution::Registered(p) if p.binding==replacement.binding)
        );
        registry.register(&first)?;
        assert!(matches!(
            scope.resolve(&control)?,
            RootResolution::Ambiguous
        ));
        let selected = RootScope::new(&scope.root, Some("replacement".into()))?;
        assert_ne!(scope.key, selected.key);
        assert!(
            matches!(selected.resolve(&control)?,RootResolution::Registered(p) if p.binding==replacement.binding)
        );
        let removed = RootScope::new(&base.join("other"), None)?;
        let epoch = registry.get(foreign.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(foreign.binding, epoch, 0)?);
        assert!(matches!(
            removed.resolve(&control)?,
            RootResolution::Removed
        ));
        assert!(registry.purge_removed_project_turn(foreign.binding)?);
        assert!(registry.retained_project(foreign.binding).is_err());
        assert!(matches!(
            removed.resolve(&control)?,
            RootResolution::Removed
        ));
        registry.register(&foreign)?;
        assert!(matches!(
            removed.resolve(&control)?,
            RootResolution::Registered(_)
        ));
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
