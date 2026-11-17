# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path.cwd();changes={}
name='crates/airk-store/src/service_registry.rs';old=(root/name).read_text(encoding='utf-8');anchor='''pub const MAX_PROJECTS: usize = 1024;'''
new=old.replace(anchor,anchor+'''\n\n#[path = "service_scope.rs"]
mod scope;
pub use scope::{SavedScopeObservation, SavedScopeResolution};''',1);changes[name]=(old,new)
name='crates/airk-store/src/service_scope.rs';new='''// SPDX-License-Identifier: Apache-2.0
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
        let registration_state = self.registration_state;
        let transaction = self.connection.transaction()?;
        let (namespace, revision): (String, i64) = transaction.query_row(
            "SELECT incarnation,revision FROM service_meta WHERE id=1", [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let namespace = namespace.parse()?;
        let revision = u64::try_from(revision).context("invalid service registry revision")?;
        let mut seen = 0usize;
        for inactive in [false, true] {
            let sql = if inactive && registration_state {
                "SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE registered=0 ORDER BY binding"
            } else if inactive {
                "SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE 0 ORDER BY binding"
            } else if registration_state {
                "SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE registered=1 ORDER BY binding"
            } else {
                "SELECT substr(payload,1,20481),epoch,paused FROM projects ORDER BY binding"
            };
            let mut statement = transaction.prepare(sql)?;
            let mut rows = statement.query([])?;
            let mut first = None;
            let mut matching = 0usize;
            while let Some(row) = rows.next()? {
                let (payload, epoch, paused) = project_row(row)?;
                let project = decode(&payload, epoch, paused)?;
                seen += 1;
                ensure!(seen <= MAX_PROJECTS, "configured source lookup exceeds registry bound");
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
                return Ok(SavedScopeObservation { namespace, revision, resolution });
            }
        }
        // Legacy saved formats may have no removal-history table. Preserve the
        // existing root-only historical tombstone even with a collection filter.
        let present: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='service_removed_projects' AND type='table')",
            [], |row| row.get(0),
        )?;
        let removed = present && transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE root_identity=?1)",
            [super::project_removal::removed_root_identity(root.to_str().context("non-UTF-8 configured source")?).to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(SavedScopeObservation {
            namespace, revision,
            resolution: if removed { SavedScopeResolution::Removed } else { SavedScopeResolution::Missing },
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
        for row in [&first, &second] { registry.register(row)?; }
        registry.pause(first.binding, true)?;
        assert!(matches!(registry.observe_source_scope(Path::new(&first.root), None)?.resolution, SavedScopeResolution::Ambiguous));
        let observed = registry.observe_source_scope(Path::new(&first.root), Some("first"))?;
        assert_eq!(observed.namespace, namespace);
        let SavedScopeResolution::Registered(found) = observed.resolution else { anyhow::bail!("active filtered registration missing") };
        assert_eq!(found.registration, first);
        assert!(found.paused);
        assert_eq!(found.fencing_generation, registry.get(first.binding)?.fencing_generation);
        registry.connection.execute("UPDATE projects SET registered=0,paused=1 WHERE binding=?1", [second.binding.to_string()])?;
        assert!(matches!(registry.observe_source_scope(Path::new(&first.root), None)?.resolution, SavedScopeResolution::Registered(_)));
        assert!(matches!(registry.observe_source_scope(Path::new(&first.root), Some("second"))?.resolution, SavedScopeResolution::Removed));
        Ok(())
    }

    #[test]
    fn exact_scope_preserves_native_path_equivalence_without_filesystem_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), Incarnation::from_bytes([32; 16]))?;
        let saved = project("/offline/source/", "/derived/offline", "offline")?;
        registry.register(&saved)?;
        for query in ["/offline/source", "/offline//source/.", "/Offline/source", "/offline/other"] {
            let equal = Path::new(&saved.root) == Path::new(query);
            assert_eq!(matches!(registry.observe_source_scope(Path::new(query), None)?.resolution, SavedScopeResolution::Registered(_)), equal);
        }
        #[cfg(windows)]
        {
            let windows = project(r"C:\\offline\\source", "/derived/windows", "windows")?;
            registry.register(&windows)?;
            for query in [r"C:/offline/source", r"C:\\offline\\source\\", r"c:\\offline\\source", r"\\\\?\\C:\\offline\\source"] {
                assert_eq!(matches!(registry.observe_source_scope(Path::new(query), None)?.resolution, SavedScopeResolution::Registered(_)), Path::new(&windows.root) == Path::new(query));
            }
        }
        Ok(())
    }

    #[test]
    fn exact_scope_keeps_root_history_even_with_different_requested_collection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), Incarnation::from_bytes([33; 16]))?;
        let root = "/fictional/removed";
        registry.connection.execute("INSERT INTO service_removed_projects(binding,root_identity,removed_ms) VALUES(?1,?2,0)", params!["a".repeat(64), super::super::project_removal::removed_root_identity(root).to_string()])?;
        assert!(matches!(registry.observe_source_scope(Path::new(root), Some("different"))?.resolution, SavedScopeResolution::Removed));
        let live = project(root, "/derived/readded", "live")?;
        registry.register(&live)?;
        assert!(matches!(registry.observe_source_scope(Path::new(root), None)?.resolution, SavedScopeResolution::Registered(_)));
        Ok(())
    }

    #[test]
    fn exact_scope_retains_unrelated_record_corruption_detection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), Incarnation::from_bytes([34; 16]))?;
        let wanted = project("/fictional/wanted", "/derived/wanted", "wanted")?;
        let unrelated = project("/fictional/unrelated", "/derived/unrelated", "unrelated")?;
        registry.register(&wanted)?;registry.register(&unrelated)?;
        registry.connection.execute("UPDATE projects SET payload=?1 WHERE binding=?2", params![b"invalid registration".as_slice(), unrelated.binding.to_string()])?;
        assert!(registry.observe_source_scope(Path::new(&wanted.root), None).is_err());
        Ok(())
    }
}
'''
changes[name]=('',new)
patch=''.join(''.join(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile=('a/'+n if a else '/dev/null'),tofile='b/'+n)) for n,(a,b) in changes.items())
(root/'artifacts/optimizations/f05-draft/store-scope.patch').write_text(patch,encoding='utf-8',newline='\n')
for n,(_,body) in changes.items():
 p=root/'artifacts/optimizations/f05-draft/source'/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(body,encoding='utf-8',newline='\n')
