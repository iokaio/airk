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
pub(crate) struct RootObservation {
    pub resolution: RootResolution,
    pub fence: Option<crate::project_client::SourceFence>,
    pub stamp: Option<airk_store::service::registry::SavedScopeStamp>,
    pub observed_unix_ms: Option<u64>,
    root: PathBuf,
    collection: Option<String>,
    captured: std::time::Instant,
}
impl RootObservation {
    pub(crate) fn age_ms(&self) -> u64 {
        std::time::Instant::now()
            .saturating_duration_since(self.captured)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    pub(crate) fn expires_at(&self) -> std::time::Instant {
        self.captured + std::time::Duration::from_secs(5)
    }
    pub(crate) fn fresh_at(&self, now: std::time::Instant) -> bool {
        now.saturating_duration_since(self.captured) < std::time::Duration::from_secs(5)
    }
}
fn scope_observed_unix_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
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
        Ok(self.observe(control)?.resolution)
    }
    pub(crate) fn observe(&self, control: &Path) -> Result<RootObservation> {
        self.observe_uncached(control, None)
    }
    fn observe_uncached(
        &self,
        control: &Path,
        deadline: Option<std::time::Instant>,
    ) -> Result<RootObservation> {
        let captured = std::time::Instant::now();
        let observed_unix_ms = scope_observed_unix_ms();
        if !control.join("service-1.4.sqlite3").try_exists()? {
            return Ok(RootObservation {
                resolution: RootResolution::Missing,
                fence: None,
                stamp: None,
                observed_unix_ms,
                root: self.root.clone(),
                collection: self.collection.clone(),
                captured,
            });
        }
        #[cfg(test)]
        record_scope_work(|work| work.saved_open_calls += 1);
        let mut registry =
            airk_store::service::registry::ServiceRegistry::open_saved_before(control, deadline)?;
        self.observe_registry(&mut registry, deadline, captured, observed_unix_ms)
    }
    pub(crate) fn observe_cached(
        &self,
        control: &Path,
        cached: Option<std::sync::Arc<RootObservation>>,
        deadline: std::time::Instant,
    ) -> Result<std::sync::Arc<RootObservation>> {
        #[cfg(test)]
        record_scope_work(|work| work.lookups += 1);
        if std::time::Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let captured = std::time::Instant::now();
        let observed_unix_ms = scope_observed_unix_ms();
        if !control.join("service-1.4.sqlite3").try_exists()? {
            if std::time::Instant::now() >= deadline {
                return Err(airk_store::service::registry::SavedScopeExpired.into());
            }
            #[cfg(test)]
            record_scope_work(|work| work.misses += 1);
            return Ok(std::sync::Arc::new(RootObservation {
                resolution: RootResolution::Missing,
                fence: None,
                stamp: None,
                observed_unix_ms,
                root: self.root.clone(),
                collection: self.collection.clone(),
                captured,
            }));
        }
        #[cfg(test)]
        record_scope_work(|work| work.saved_open_calls += 1);
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            control,
            Some(deadline),
        )?;
        #[cfg(test)]
        record_scope_work(|work| work.stamp_calls += 1);
        let stamp = registry.source_scope_stamp(Some(deadline))?;
        if let Some(cached) = cached
            && cached.root == self.root
            && cached.collection == self.collection
            && cached.stamp == Some(stamp)
            && cached.fresh_at(std::time::Instant::now())
        {
            if std::time::Instant::now() >= deadline {
                return Err(airk_store::service::registry::SavedScopeExpired.into());
            }
            #[cfg(test)]
            record_scope_work(|work| work.hits += 1);
            return Ok(cached); // no project row decoded; original observation age retained.
        }
        #[cfg(test)]
        record_scope_work(|work| work.misses += 1);
        Ok(std::sync::Arc::new(self.observe_registry(
            &mut registry,
            Some(deadline),
            captured,
            observed_unix_ms,
        )?))
    }
    fn observe_registry(
        &self,
        registry: &mut airk_store::service::registry::ServiceRegistry,
        deadline: Option<std::time::Instant>,
        captured: std::time::Instant,
        observed_unix_ms: Option<u64>,
    ) -> Result<RootObservation> {
        #[cfg(test)]
        record_scope_work(|work| work.full_observation_calls += 1);
        let observed = registry.observe_source_scope_before(
            &self.root,
            self.collection.as_deref(),
            deadline,
        )?;
        let stamp = Some(airk_store::service::registry::SavedScopeStamp {
            namespace: observed.namespace,
            revision: observed.revision,
        });
        use airk_store::service::registry::SavedScopeResolution;
        let (resolution, fence) = match observed.resolution {
            SavedScopeResolution::Registered(project) => {
                let fence = crate::project_client::SourceFence {
                    namespace: observed.namespace,
                    revision: observed.revision,
                    project: project.registration.binding,
                    fencing_generation: project.fencing_generation,
                };
                (
                    RootResolution::Registered(project.registration),
                    Some(fence),
                )
            }
            SavedScopeResolution::Missing => (RootResolution::Missing, None),
            SavedScopeResolution::Removed => (RootResolution::Removed, None),
            SavedScopeResolution::Ambiguous => (RootResolution::Ambiguous, None),
        };
        Ok(RootObservation {
            resolution,
            fence,
            stamp,
            observed_unix_ms,
            root: self.root.clone(),
            collection: self.collection.clone(),
            captured,
        })
    }
}

/// Exact saved-scope metadata. Not a connection grant or execution proof.
#[derive(serde::Serialize)]
pub(crate) struct SourceScopeMetadata {
    schema: u32,
    root: PathBuf,
    collection: Option<String>,
    namespace: Option<airk_core::identity::Incarnation>,
    revision: Option<u64>,
    observed_unix_ms: Option<u64>,
    outcome: &'static str,
    registration: Option<ProjectRegistration>,
    fencing_generation: Option<u64>,
}
impl RootScope {
    pub(crate) fn metadata_before(
        &self,
        control: &Path,
        deadline: std::time::Instant,
    ) -> Result<SourceScopeMetadata> {
        if std::time::Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let observed = self.observe_uncached(control, Some(deadline))?;
        if std::time::Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let (outcome, registration) = match observed.resolution {
            RootResolution::Registered(project) => ("registered", Some(project)),
            RootResolution::Missing => ("missing", None),
            RootResolution::Removed => ("removed", None),
            RootResolution::Ambiguous => ("ambiguous", None),
        };
        Ok(SourceScopeMetadata {
            schema: 1,
            root: self.root.clone(),
            collection: self.collection.clone(),
            namespace: observed.stamp.map(|stamp| stamp.namespace),
            revision: observed.stamp.map(|stamp| stamp.revision),
            observed_unix_ms: observed.observed_unix_ms,
            outcome,
            registration,
            fencing_generation: observed.fence.map(|fence| fence.fencing_generation),
        })
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

#[cfg(test)]
mod root_resolution_cache_tests {
    use super::*;
    use airk_core::{identity::Incarnation, service::ProjectRegistration};
    use airk_store::service::registry::ServiceRegistry;
    use std::{
        fs,
        sync::Arc,
        time::{Duration, Instant},
    };
    fn fixture() -> Result<(tempfile::TempDir, PathBuf, RootScope, ProjectRegistration)> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let source = base.join("source");
        let data = base.join("data");
        let control = base.join("control");
        for path in [&source, &data, &control] {
            fs::create_dir(path)?;
        }
        let project = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "fixture".into(),
        )?;
        Ok((owner, control, RootScope::new(&source, None)?, project))
    }
    #[test]
    fn unchanged_authenticated_stamp_reuses_owned_scope_and_registration_invalidates_missing()
    -> Result<()> {
        let (_owner, control, scope, project) = fixture()?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([71; 16]))?;
        let first =
            scope.observe_cached(&control, None, Instant::now() + Duration::from_secs(2))?;
        assert!(matches!(first.resolution, RootResolution::Missing));
        let hit = scope.observe_cached(
            &control,
            Some(first.clone()),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert!(
            Arc::ptr_eq(&first, &hit),
            "hit must reuse the owned observation without full row decode"
        );
        assert_eq!(
            first.captured, hit.captured,
            "stamp polling must not rejuvenate the original observation"
        );
        assert_eq!(first.observed_unix_ms, hit.observed_unix_ms);
        registry.register(&project)?;
        let registered = scope.observe_cached(
            &control,
            Some(first),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert!(matches!(
            registered.resolution,
            RootResolution::Registered(_)
        ));
        let before = registered.stamp;
        registry.pause(project.binding, true)?;
        let paused = scope.observe_cached(
            &control,
            Some(registered),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert_ne!(paused.stamp, before);
        Ok(())
    }
    #[test]
    fn same_revision_replacement_namespace_never_reuses_old_scope() -> Result<()> {
        let (owner, control, scope, project) = fixture()?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([72; 16]))?;
        registry.register(&project)?;
        let first =
            scope.observe_cached(&control, None, Instant::now() + Duration::from_secs(2))?;
        drop(registry);
        fs::rename(&control, owner.path().join("old-control"))?;
        fs::create_dir(&control)?;
        let mut replacement = ServiceRegistry::open(&control, Incarnation::from_bytes([73; 16]))?;
        replacement.register(&project)?;
        let second = scope.observe_cached(
            &control,
            Some(first.clone()),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert_eq!(
            first.stamp.unwrap().revision,
            second.stamp.unwrap().revision
        );
        assert_ne!(
            first.stamp.unwrap().namespace,
            second.stamp.unwrap().namespace
        );
        assert!(!Arc::ptr_eq(&first, &second));
        Ok(())
    }
    #[test]
    fn cheap_stamp_is_not_a_fresh_row_audit_and_expiry_forces_validation() -> Result<()> {
        let (_owner, control, scope, project) = fixture()?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([74; 16]))?;
        registry.register(&project)?;
        let mut first =
            scope.observe_cached(&control, None, Instant::now() + Duration::from_secs(2))?;
        let db = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
        db.execute(
            "UPDATE projects SET payload=x'7b7d' WHERE binding=?1",
            [project.binding.to_string()],
        )?;
        let hit = scope.observe_cached(
            &control,
            Some(first.clone()),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert!(
            Arc::ptr_eq(&first, &hit),
            "same stamp fast path performs no full project decode"
        );
        drop(hit);
        Arc::get_mut(&mut first)
            .context("observation unexpectedly shared")?
            .captured = Instant::now() - Duration::from_secs(5);
        assert!(
            scope
                .observe_cached(
                    &control,
                    Some(first),
                    Instant::now() + Duration::from_secs(2)
                )
                .is_err(),
            "expired snapshot must detect out-of-band malformed row instead of resetting its age"
        );
        Ok(())
    }
    #[test]
    fn expired_admission_is_known_unsubmitted_and_creates_no_registry() -> Result<()> {
        let (_owner, control, scope, _project) = fixture()?;
        let error = scope
            .observe_cached(&control, None, Instant::now())
            .err()
            .context("expired scope admitted")?;
        assert!(
            error
                .downcast_ref::<airk_store::service::registry::SavedScopeExpired>()
                .is_some()
        );
        assert!(fs::read_dir(control)?.next().is_none());
        Ok(())
    }
    #[test]
    fn contended_stamp_refuses_instead_of_serving_cached_scope_and_recovers() -> Result<()> {
        let (_owner, control, scope, project) = fixture()?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([75; 16]))?;
        registry.register(&project)?;
        let first =
            scope.observe_cached(&control, None, Instant::now() + Duration::from_secs(2))?;
        let db = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
        db.execute_batch("BEGIN EXCLUSIVE")?;
        let blocked = scope.observe_cached(
            &control,
            Some(first.clone()),
            Instant::now() + Duration::from_millis(20),
        );
        db.execute_batch("ROLLBACK")?;
        assert!(
            blocked.is_err(),
            "contended authentication must not substitute cached authority"
        );
        let recovered = scope.observe_cached(
            &control,
            Some(first.clone()),
            Instant::now() + Duration::from_secs(2),
        )?;
        assert!(Arc::ptr_eq(&first, &recovered));
        assert_eq!(first.captured, recovered.captured);
        Ok(())
    }
}

// Counts actual RootScope calls, not physical I/O or store-internal decoding.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ScopeWork {
    lookups: usize,
    saved_open_calls: usize,
    stamp_calls: usize,
    full_observation_calls: usize,
    hits: usize,
    misses: usize,
}
#[cfg(test)]
std::thread_local! { static SCOPE_WORK:std::cell::Cell<ScopeWork> = const { std::cell::Cell::new(ScopeWork { lookups:0,saved_open_calls:0,stamp_calls:0,full_observation_calls:0,hits:0,misses:0 }) }; }
#[cfg(test)]
fn record_scope_work(update: impl FnOnce(&mut ScopeWork)) {
    SCOPE_WORK.with(|cell| {
        let mut work = cell.get();
        update(&mut work);
        cell.set(work);
    });
}
#[cfg(test)]
fn take_scope_work() -> ScopeWork {
    SCOPE_WORK.with(|cell| cell.replace(ScopeWork::default()))
}
#[cfg(test)]
mod actual_scope_work_tests {
    use super::*;
    #[test]
    fn cached_hit_still_authenticates_one_open_and_stamp_without_full_observation() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let source = base.join("source");
        let data = base.join("data");
        let control = base.join("control");
        for path in [&source, &data, &control] {
            std::fs::create_dir(path)?;
        }
        let scope = RootScope::new(&source, None)?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_core::identity::Incarnation::from_bytes([92; 16]),
        )?;
        let project = ProjectRegistration::new(
            source.to_string_lossy().into_owned(),
            data.to_string_lossy().into_owned(),
            "work-count".into(),
        )?;
        registry.register(&project)?;
        let deadline = || std::time::Instant::now() + std::time::Duration::from_secs(2);
        take_scope_work();
        let first = scope.observe_cached(&control, None, deadline())?;
        let miss = take_scope_work();
        assert_eq!(
            miss,
            ScopeWork {
                lookups: 1,
                saved_open_calls: 1,
                stamp_calls: 1,
                full_observation_calls: 1,
                hits: 0,
                misses: 1
            }
        );
        let hit = scope.observe_cached(&control, Some(first.clone()), deadline())?;
        assert!(std::sync::Arc::ptr_eq(&first, &hit));
        let work = take_scope_work();
        assert_eq!(
            work,
            ScopeWork {
                lookups: 1,
                saved_open_calls: 1,
                stamp_calls: 1,
                full_observation_calls: 0,
                hits: 1,
                misses: 0
            }
        );
        assert_eq!(first.captured, hit.captured);
        eprintln!("actual root miss work: {miss:?}; hit work: {work:?}");
        registry.pause(project.binding, true)?;
        let changed = scope.observe_cached(&control, Some(hit), deadline())?;
        assert!(!std::sync::Arc::ptr_eq(&first, &changed));
        assert_eq!(take_scope_work(), miss);
        Ok(())
    }
}

#[cfg(test)]
mod exact_scope_metadata_tests {
    use super::*;
    use airk_core::identity::Incarnation;
    use airk_store::service::registry::ServiceRegistry;
    fn observe(scope: &RootScope, control: &Path) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(scope.metadata_before(
            control,
            std::time::Instant::now() + std::time::Duration::from_secs(2),
        )?)?)
    }
    #[test]
    fn actual_registry_scope_metadata_tracks_removed_reregistered_ambiguity_and_namespace()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let source = base.join("source");
        let control = base.join("control");
        let data = base.join("data");
        for path in [&source, &control, &data] {
            std::fs::create_dir(path)?;
        }
        let scope = RootScope::new(&source, None)?;
        let absent = observe(&scope, &control)?;
        assert_eq!(absent["outcome"], "missing");
        assert!(absent["namespace"].is_null());
        assert!(!control.join("service-1.4.sqlite3").exists());
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([81; 16]))?;
        let first = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "first".into(),
        )?;
        registry.register(&first)?;
        let registered = observe(&scope, &control)?;
        assert_eq!(registered["outcome"], "registered");
        assert_eq!(registered["registration"], serde_json::to_value(&first)?);
        assert_eq!(
            registered["namespace"],
            serde_json::to_value(registry.incarnation()?)?
        );
        assert_eq!(
            registered["fencing_generation"],
            registry.get(first.binding)?.fencing_generation
        );
        std::fs::rename(&source, base.join("offline"))?;
        let epoch = registry.get(first.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(first.binding, epoch, 0)?);
        assert_eq!(observe(&scope, &control)?["outcome"], "removed");
        assert!(registry.purge_removed_project_turn(first.binding)?);
        assert_eq!(observe(&scope, &control)?["outcome"], "removed");
        registry.register(&first)?;
        assert_eq!(observe(&scope, &control)?["outcome"], "registered");
        let second =
            ProjectRegistration::new(first.root.clone(), first.data_root.clone(), "second".into())?;
        registry.register(&second)?;
        assert_eq!(observe(&scope, &control)?["outcome"], "ambiguous");
        let chosen = RootScope::new(&source, Some("second".into()))?;
        assert_eq!(
            observe(&chosen, &control)?["registration"],
            serde_json::to_value(&second)?
        );
        let old = observe(&chosen, &control)?;
        drop(registry);
        std::fs::rename(&control, base.join("old-control"))?;
        std::fs::create_dir(&control)?;
        let mut replacement = ServiceRegistry::open(&control, Incarnation::from_bytes([82; 16]))?;
        replacement.register(&second)?;
        let new = observe(&chosen, &control)?;
        assert_ne!(old["namespace"], new["namespace"]);
        assert_eq!(new["registration"], serde_json::to_value(&second)?);
        assert!(
            chosen
                .metadata_before(
                    &control,
                    std::time::Instant::now() - std::time::Duration::from_secs(1)
                )
                .is_err()
        );
        Ok(())
    }
}
