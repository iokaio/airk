// SPDX-License-Identifier: Apache-2.0
//! One bounded saved-registry scope observation; no index/source access or migration.
use super::*;

/// Owned metadata from one SQLite read snapshot, not execution authorization.
pub struct SavedScopeObservation {
    pub namespace: Incarnation,
    pub revision: u64,
    pub resolution: SavedScopeResolution,
}

pub enum SavedScopeResolution {
    Registered(Box<RegisteredProject>),
    Missing,
    Removed,
    Ambiguous,
}

impl ServiceRegistry {
    /// Match with native Path equality, including saved historical root spelling.
    /// Active rows win before inactive rows/history; validate all visited rows as
    /// the existing paginated resolver does. This is a scan, not an indexed lookup.
    pub fn observe_source_scope(
        &mut self,
        root: &Path,
        collection: Option<&str>,
    ) -> Result<SavedScopeObservation> {
        self.observe_source_scope_before(root, collection, None)
    }
    pub fn observe_source_scope_before(
        &mut self,
        root: &Path,
        collection: Option<&str>,
        deadline: Option<std::time::Instant>,
    ) -> Result<SavedScopeObservation> {
        self.connection.busy_timeout(scope_remaining(deadline)?)?;
        #[cfg(test)]
        record_work(|work| work.observations += 1);
        let registration_state = self.registration_state;
        let transaction = self.connection.transaction()?;
        let (namespace, revision): (String, i64) = transaction.query_row(
            "SELECT incarnation,revision FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let namespace = namespace.parse()?;
        let revision = u64::try_from(revision).context("invalid service registry revision")?;
        let mut seen = 0usize;
        for inactive in [false, true] {
            scope_remaining(deadline)?;
            let sql = if inactive && registration_state {
                "SELECT substr(payload,1,20481),epoch,paused,binding FROM projects WHERE registered=0 ORDER BY binding"
            } else if inactive {
                "SELECT substr(payload,1,20481),epoch,paused,binding FROM projects WHERE 0 ORDER BY binding"
            } else if registration_state {
                "SELECT substr(payload,1,20481),epoch,paused,binding FROM projects WHERE registered=1 ORDER BY binding"
            } else {
                "SELECT substr(payload,1,20481),epoch,paused,binding FROM projects ORDER BY binding"
            };
            let mut statement = transaction.prepare(sql)?;
            let mut rows = statement.query([])?;
            let mut first = None;
            let mut matching = 0usize;
            while let Some(row) = rows.next()? {
                scope_remaining(deadline)?;
                let (payload, epoch, paused) = project_row(row)?;
                #[cfg(test)]
                record_work(|work| work.decoded_rows += 1);
                let project = decode(&payload, epoch, paused)?;
                let binding: String = row.get(3)?;
                ensure!(
                    binding == project.registration.binding.to_string(),
                    "foreign service registration"
                );
                seen += 1;
                ensure!(
                    seen <= MAX_PROJECTS,
                    "configured source lookup exceeds registry bound"
                );
                if Path::new(&project.registration.root) == root
                    && collection.is_none_or(|name| name == project.registration.collection)
                {
                    matching += 1;
                    if first.is_none() {
                        first = Some(project);
                    }
                }
            }
            // Continue validating every row in this role even after ambiguity.
            // Do not inspect inactive rows if any active registration matched.
            if matching != 0 {
                let resolution = if inactive {
                    SavedScopeResolution::Removed
                } else if matching == 1 {
                    SavedScopeResolution::Registered(Box::new(first.expect("one matching project")))
                } else {
                    SavedScopeResolution::Ambiguous
                };
                scope_remaining(deadline)?;
                return Ok(SavedScopeObservation {
                    namespace,
                    revision,
                    resolution,
                });
            }
        }
        // Legacy saved formats may have no removal-history table. Preserve the
        // existing root-only historical tombstone even with a collection filter.
        let root_text = root.to_str().context("non-UTF-8 configured source")?;
        let present: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='service_removed_projects' AND type='table')",
            [], |row| row.get(0),
        )?;
        let removed = present
            && transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE root_identity=?1)",
                [super::project_removal::removed_root_identity(root_text).to_string()],
                |row| row.get::<_, bool>(0),
            )?;
        scope_remaining(deadline)?;
        Ok(SavedScopeObservation {
            namespace,
            revision,
            resolution: if removed {
                SavedScopeResolution::Removed
            } else {
                SavedScopeResolution::Missing
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(root: &str, data: &str, collection: &str) -> Result<ProjectRegistration> {
        ProjectRegistration::new(root.into(), data.into(), collection.into())
    }

    #[test]
    fn exact_scope_keeps_active_ambiguity_pause_and_collection_precedence() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let namespace = Incarnation::from_bytes([31; 16]);
        let mut registry = ServiceRegistry::open(directory.path(), namespace)?;
        let first = project("/fictional/source", "/derived/first", "first")?;
        let second = project("/fictional/source", "/derived/second", "second")?;
        for row in [&first, &second] {
            registry.register(row)?;
        }
        registry.pause(first.binding, true)?;
        assert!(matches!(
            registry
                .observe_source_scope(Path::new(&first.root), None)?
                .resolution,
            SavedScopeResolution::Ambiguous
        ));
        let observed = registry.observe_source_scope(Path::new(&first.root), Some("first"))?;
        assert_eq!(observed.namespace, namespace);
        assert_eq!(observed.revision, registry.list(None, None, 32)?.revision);
        let SavedScopeResolution::Registered(found) = observed.resolution else {
            anyhow::bail!("active filtered registration missing")
        };
        assert_eq!(found.registration, first);
        assert!(found.paused);
        assert_eq!(
            found.fencing_generation,
            registry.get(first.binding)?.fencing_generation
        );
        registry.connection.execute(
            "UPDATE projects SET registered=0,paused=1 WHERE binding=?1",
            [second.binding.to_string()],
        )?;
        assert!(matches!(
            registry
                .observe_source_scope(Path::new(&first.root), None)?
                .resolution,
            SavedScopeResolution::Registered(_)
        ));
        assert!(matches!(
            registry
                .observe_source_scope(Path::new(&first.root), Some("second"))?
                .resolution,
            SavedScopeResolution::Removed
        ));
        Ok(())
    }

    #[test]
    fn exact_scope_preserves_native_path_equivalence_without_filesystem_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([32; 16]))?;
        let saved = project("/offline/source/", "/derived/offline", "offline")?;
        registry.register(&saved)?;
        for query in [
            "/offline/source",
            "/offline//source/.",
            "/Offline/source",
            "/offline/other",
        ] {
            let equal = Path::new(&saved.root) == Path::new(query);
            assert_eq!(
                matches!(
                    registry
                        .observe_source_scope(Path::new(query), None)?
                        .resolution,
                    SavedScopeResolution::Registered(_)
                ),
                equal
            );
        }
        #[cfg(windows)]
        {
            let windows = project(r"C:\offline\source", "/derived/windows", "windows")?;
            registry.register(&windows)?;
            for query in [
                r"C:/offline/source",
                r"C:\offline\source\",
                r"c:\offline\source",
                r"\\?\C:\offline\source",
            ] {
                assert_eq!(
                    matches!(
                        registry
                            .observe_source_scope(Path::new(query), None)?
                            .resolution,
                        SavedScopeResolution::Registered(_)
                    ),
                    Path::new(&windows.root) == Path::new(query)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn exact_scope_keeps_root_history_even_with_different_requested_collection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([33; 16]))?;
        let root = "/fictional/removed";
        registry.connection.execute("INSERT INTO service_removed_projects(binding,root_identity,removed_ms) VALUES(?1,?2,0)", params!["a".repeat(64), super::super::project_removal::removed_root_identity(root).to_string()])?;
        assert!(matches!(
            registry
                .observe_source_scope(Path::new(root), Some("different"))?
                .resolution,
            SavedScopeResolution::Removed
        ));
        let live = project(root, "/derived/readded", "live")?;
        registry.register(&live)?;
        assert!(matches!(
            registry
                .observe_source_scope(Path::new(root), None)?
                .resolution,
            SavedScopeResolution::Registered(_)
        ));
        Ok(())
    }

    #[test]
    fn exact_scope_retains_unrelated_record_corruption_detection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([34; 16]))?;
        let wanted = project("/fictional/wanted", "/derived/wanted", "wanted")?;
        let unrelated = project("/fictional/unrelated", "/derived/unrelated", "unrelated")?;
        registry.register(&wanted)?;
        registry.register(&unrelated)?;
        registry.connection.execute(
            "UPDATE projects SET payload=?1 WHERE binding=?2",
            params![
                b"invalid registration".as_slice(),
                unrelated.binding.to_string()
            ],
        )?;
        assert!(
            registry
                .observe_source_scope(Path::new(&wanted.root), None)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn exact_scope_preserves_role_aware_1024_bound() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([35; 16]))?;
        let wanted = project("/fictional/wanted", "/derived/wanted", "wanted")?;
        registry.register(&wanted)?;
        {
            let transaction = registry.connection.transaction()?;
            for index in 0..MAX_PROJECTS {
                let row = project(
                    &format!("/fictional/unrelated-{index}"),
                    "/derived/unrelated",
                    "other",
                )?;
                transaction.execute("INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,0,0,1)",
                    params![row.binding.to_string(), serde_json::to_vec(&row)?])?;
            }
            transaction.commit()?;
        }
        assert!(
            registry
                .observe_source_scope(Path::new(&wanted.root), None)
                .is_err(),
            "active match cannot hide too many active rows"
        );
        registry.connection.execute(
            "UPDATE projects SET registered=0,paused=1 WHERE binding<>?1",
            [wanted.binding.to_string()],
        )?;
        assert!(
            matches!(
                registry
                    .observe_source_scope(Path::new(&wanted.root), None)?
                    .resolution,
                SavedScopeResolution::Registered(_)
            ),
            "inactive rows must not consume a successful active role's budget"
        );
        assert!(
            registry
                .observe_source_scope(Path::new("/fictional/missing"), None)
                .is_err(),
            "a missing active match must charge both roles"
        );
        Ok(())
    }

    #[test]
    fn exact_scope_saved_formats_are_read_only_and_preserve_namespace_and_pause() -> Result<()> {
        for version in [2i64, 4, 5, 6, 10] {
            let directory = tempfile::tempdir()?;
            let namespace = Incarnation::from_bytes([36; 16]);
            let wanted = project("/fictional/legacy", "/derived/legacy", "legacy")?;
            let mut registry = ServiceRegistry::open(directory.path(), namespace)?;
            registry.register(&wanted)?;
            registry.pause(wanted.binding, true)?;
            if version < 7 {
                registry
                    .connection
                    .execute_batch("DROP TABLE service_watch_traversal;")?;
            }
            if version < 6 {
                registry
                    .connection
                    .execute_batch("ALTER TABLE projects DROP COLUMN cleanup_guard;")?;
            }
            if version < 5 {
                registry
                    .connection
                    .execute_batch("ALTER TABLE projects DROP COLUMN registered;")?;
            }
            if version < 10 {
                registry
                    .connection
                    .execute_batch("DROP TABLE service_removed_projects;")?;
            }
            registry
                .connection
                .pragma_update(None, "user_version", version)?;
            drop(registry);
            let path = directory.path().join("service-1.4.sqlite3");
            let before = fs::read(&path)?;
            let mut saved = ServiceRegistry::open_saved(directory.path())?;
            let observed = saved.observe_source_scope(Path::new(&wanted.root), None)?;
            assert_eq!(observed.namespace, namespace);
            let SavedScopeResolution::Registered(found) = observed.resolution else {
                anyhow::bail!("legacy active registration missing")
            };
            assert_eq!(found.registration, wanted);
            assert!(found.paused);
            assert!(matches!(
                saved
                    .observe_source_scope(Path::new("/fictional/missing"), None)?
                    .resolution,
                SavedScopeResolution::Missing
            ));
            let saved_version: i64 =
                saved
                    .connection
                    .pragma_query_value(None, "user_version", |row| row.get(0))?;
            assert_eq!(saved_version, version);
            drop(saved);
            assert_eq!(
                fs::read(path)?,
                before,
                "saved observation may not migrate or write"
            );
        }
        Ok(())
    }

    #[test]
    fn idempotent_active_registration_clearing_same_root_tombstone_advances_scope_revision()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let control = base.join("control");
        let source = base.join("source");
        let first_data = base.join("first-data");
        let second_data = base.join("second-data");
        for path in [&control, &source, &first_data, &second_data] {
            fs::create_dir(path)?;
        }
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([61; 16]))?;
        let first = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            first_data.to_str().unwrap().into(),
            "first".into(),
        )?;
        let second = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            second_data.to_str().unwrap().into(),
            "second".into(),
        )?;
        registry.register(&first)?;
        registry.register(&second)?;
        let epoch = registry.get(second.binding)?.fencing_generation;
        // A real source disappearance may be processed for one collection before
        // its sibling; retirement consumes that source child's fenced observation.
        fs::remove_dir(&source)?;
        assert!(registry.retire_missing_source(second.binding, epoch, 0)?);
        assert!(registry.purge_removed_project_turn(second.binding)?);
        assert!(registry.source_was_removed(&first.root)?);
        let before = registry.observe_source_scope(&source, Some("second"))?;
        assert!(matches!(before.resolution, SavedScopeResolution::Removed));
        fs::create_dir(&source)?;
        registry.register(&first)?;
        let after = registry.observe_source_scope(&source, Some("second"))?;
        assert!(matches!(after.resolution, SavedScopeResolution::Missing));
        assert_eq!(after.namespace, before.namespace);
        assert_eq!(
            after.revision,
            before.revision + 1,
            "negative resolution changed without exactly one revision fence"
        );
        registry.register(&first)?;
        assert_eq!(
            registry
                .observe_source_scope(&source, Some("second"))?
                .revision,
            after.revision,
            "true idempotent registration must remain a no-op"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedScopeStamp {
    pub namespace: Incarnation,
    pub revision: u64,
}
#[derive(Debug)]
pub struct SavedScopeExpired;
impl std::fmt::Display for SavedScopeExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source scope observation admission expired")
    }
}
impl std::error::Error for SavedScopeExpired {}
/// Only pre-query metadata admission failures, never uncertain query outcomes.
pub fn source_scope_retryable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<SavedScopeExpired>().is_some()
        || error
            .downcast_ref::<rusqlite::Error>()
            .is_some_and(|error| {
                matches!(error,
            rusqlite::Error::SqliteFailure(failure, _) if matches!(failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
            })
}
pub(super) fn scope_remaining(deadline: Option<std::time::Instant>) -> Result<Duration> {
    let remaining = deadline.map_or(Duration::from_secs(2), |until| {
        until.saturating_duration_since(std::time::Instant::now())
    });
    if remaining.is_zero() {
        return Err(SavedScopeExpired.into());
    }
    Ok(remaining.min(Duration::from_secs(2)))
}
impl ServiceRegistry {
    /// One authenticated scalar row, zero project payload decodes. Not a deep audit.
    pub fn source_scope_stamp(
        &self,
        deadline: Option<std::time::Instant>,
    ) -> Result<SavedScopeStamp> {
        self.connection.busy_timeout(scope_remaining(deadline)?)?;
        #[cfg(test)]
        record_work(|work| work.stamps += 1);
        let (namespace, revision): (String, i64) = self.connection.query_row(
            "SELECT incarnation,revision FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let stamp = SavedScopeStamp {
            namespace: namespace.parse()?,
            revision: u64::try_from(revision)?,
        };
        ensure!(
            stamp.namespace == retained_namespace(&self.root)?,
            "source stamp owner changed"
        );
        scope_remaining(deadline)?;
        Ok(stamp)
    }
}

// Thread-local counters observe actual saved-open/stamp/full-decode entry points
// only in unit tests. They are not physical I/O or whole-process SQL counters.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ScopeWork {
    opens: usize,
    stamps: usize,
    observations: usize,
    decoded_rows: usize,
}
#[cfg(test)]
std::thread_local! { static WORK: std::cell::Cell<ScopeWork> = const { std::cell::Cell::new(ScopeWork { opens:0, stamps:0, observations:0, decoded_rows:0 }) }; }
#[cfg(test)]
fn record_work(update: impl FnOnce(&mut ScopeWork)) {
    WORK.with(|cell| {
        let mut work = cell.get();
        update(&mut work);
        cell.set(work);
    });
}
#[cfg(test)]
pub(super) fn record_saved_open() {
    record_work(|work| work.opens += 1);
}
#[cfg(test)]
fn take_work() -> ScopeWork {
    WORK.with(|cell| cell.replace(ScopeWork::default()))
}
#[cfg(test)]
mod work_tests {
    use super::*;
    #[test]
    fn actual_saved_stamp_decodes_zero_projects_and_full_observation_decodes_all_rows() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(owner.path(), Incarnation::from_bytes([91; 16]))?;
        for index in 0..3 {
            registry.register(&ProjectRegistration::new(
                format!("/fictional/source-{index}"),
                format!("/derived/data-{index}"),
                format!("collection-{index}"),
            )?)?;
        }
        drop(registry);
        take_work();
        let mut saved = ServiceRegistry::open_saved(owner.path())?;
        let stamp = saved.source_scope_stamp(None)?;
        let work = take_work();
        assert_eq!(
            work,
            ScopeWork {
                opens: 1,
                stamps: 1,
                observations: 0,
                decoded_rows: 0
            }
        );
        eprintln!("actual saved stamp work: {work:?}");
        let observed = saved.observe_source_scope(Path::new("/fictional/source-0"), None)?;
        assert_eq!(observed.namespace, stamp.namespace);
        assert_eq!(observed.revision, stamp.revision);
        let work = take_work();
        assert_eq!(
            work,
            ScopeWork {
                opens: 0,
                stamps: 0,
                observations: 1,
                decoded_rows: 3
            }
        );
        eprintln!("actual full scope work: {work:?}");
        Ok(())
    }
}
