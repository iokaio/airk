// SPDX-License-Identifier: Apache-2.0
//! Bounded service-only registry. It never opens collection indexes or source files.
use super::ProjectLease;
use airk_core::{
    identity::Incarnation,
    service::{
        MAX_PROJECT_REGISTRATION_BYTES, ProjectBinding, ProjectPage, ProjectRegistration,
        RegisteredProject,
    },
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const APPLICATION: i64 = 0x41535256;
const MAX_DATABASE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_PROJECTS: usize = 1024;

#[path = "service_jobs.rs"]
mod jobs;

pub struct ServiceRegistry {
    root: PathBuf,
    connection: Connection,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    format: u32,
    root: String,
    incarnation: Incarnation,
}

fn owner(root: &Path, existed: bool, incarnation: Incarnation) -> Result<Owner> {
    let path = root.join("service-1.4.owner.json");
    let root_text = root
        .to_str()
        .context("service runtime root must be UTF-8")?;
    ensure!(
        root_text.len() <= 4096,
        "service runtime root exceeds budget"
    );
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 16384,
                "invalid service registry owner"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected service registry owner"
                );
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?.take(16385).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 16384,
                "service registry owner exceeds budget"
            );
            let owner: Owner = serde_json::from_slice(&bytes)?;
            ensure!(
                owner.format == 1 && owner.root == root_text,
                "foreign service registry owner"
            );
            Ok(owner)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(!existed, "service registry has no ownership receipt");
            let owner = Owner {
                format: 1,
                root: root_text.into(),
                incarnation,
            };
            let mut temporary = tempfile::NamedTempFile::new_in(root)?;
            temporary.write_all(&serde_json::to_vec(&owner)?)?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(path)?;
            crate::maintenance::sync_parent(root)?;
            Ok(owner)
        }
        Err(error) => Err(error.into()),
    }
}

fn bump(connection: &Connection) -> Result<()> {
    let revision: i64 =
        connection.query_row("SELECT revision FROM service_meta WHERE id=1", [], |row| {
            row.get(0)
        })?;
    let next = revision
        .checked_add(1)
        .context("service registry revision exhausted")?;
    connection.execute("UPDATE service_meta SET revision=?1 WHERE id=1", [next])?;
    Ok(())
}

fn project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Vec<u8>, i64, bool)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn decode(bytes: &[u8], epoch: i64, paused: bool) -> Result<RegisteredProject> {
    ensure!(
        bytes.len() <= MAX_PROJECT_REGISTRATION_BYTES && epoch >= 0,
        "invalid service registration record"
    );
    let registration: ProjectRegistration = serde_json::from_slice(bytes)?;
    registration.validate()?;
    Ok(RegisteredProject {
        registration,
        fencing_generation: epoch as u64,
        paused,
    })
}

impl ServiceRegistry {
    /// Native adapter establishes owner-only directory permissions before opening this ledger.
    /// Incarnation is allocated by the kernel; existing ledgers preserve their saved identity.
    pub fn open(root: &Path, incarnation: Incarnation) -> Result<Self> {
        let root = root.canonicalize()?;
        let path = root.join("service-1.4.sqlite3");
        let existed = path.try_exists()?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= MAX_DATABASE_BYTES,
                "invalid service registry file"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected service registry"
                );
            }
        }
        for suffix in ["-wal", "-shm"] {
            ensure!(
                !path
                    .with_file_name(format!("service-1.4.sqlite3{suffix}"))
                    .try_exists()?,
                "unexpected service registry overlay"
            );
        }
        let owner = owner(&root, existed, incarnation)?;
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let mut initialize = !existed;
        if existed {
            let application: i64 =
                connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
            let version: i64 =
                connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if application == 0 && version == 0 {
                let tables: i64 = connection.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                ensure!(tables == 0, "unsupported uninitialized service registry");
                initialize = true;
            } else {
                ensure!(
                    application == APPLICATION && version == 1,
                    "unsupported service registry format"
                );
                let actual: String = connection.query_row(
                    "SELECT incarnation FROM service_meta WHERE id=1",
                    [],
                    |row| row.get(0),
                )?;
                ensure!(
                    actual.parse::<Incarnation>()? == owner.incarnation,
                    "service registry ownership mismatch"
                );
            }
        }
        connection.execute_batch(
            "PRAGMA page_size=4096; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            PRAGMA max_page_count=8192; PRAGMA cache_size=-8192; PRAGMA foreign_keys=ON;",
        )?;
        let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
        ensure!(page_size == 4096, "unsupported service registry page size");
        if initialize {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE service_meta(id INTEGER PRIMARY KEY CHECK(id=1), incarnation TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK(revision>=0), request_epoch INTEGER NOT NULL CHECK(request_epoch>=0));
                CREATE TABLE projects(binding TEXT PRIMARY KEY, payload BLOB NOT NULL CHECK(length(payload)<=20480),
                epoch INTEGER NOT NULL CHECK(epoch>=0), paused INTEGER NOT NULL CHECK(paused IN(0,1))) WITHOUT ROWID;")?;
            transaction.execute(
                "INSERT INTO service_meta VALUES(1,?1,0,0)",
                [owner.incarnation.to_string()],
            )?;
            transaction.pragma_update(None, "application_id", APPLICATION)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
        }
        let registry = Self { root, connection };
        ensure!(
            registry.incarnation()? == owner.incarnation,
            "service registry identity disagrees with owner"
        );
        registry.connection.execute_batch(jobs::SCHEMA)?;
        Ok(registry)
    }

    pub fn incarnation(&self) -> Result<Incarnation> {
        let text: String = self.connection.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        text.parse()
    }

    pub fn register(&mut self, registration: &ProjectRegistration) -> Result<()> {
        registration.validate()?;
        let payload = serde_json::to_vec(registration)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT substr(payload,1,20481) FROM projects WHERE binding=?1",
                [registration.binding.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            ensure!(
                existing == payload,
                "service registration binding collision"
            );
        } else {
            let collision: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE json_extract(payload,'$.data_root')=?1 AND json_extract(payload,'$.collection')=?2)",
                params![registration.data_root,registration.collection], |row| row.get(0),
            )?;
            ensure!(
                !collision,
                "collection storage is already registered to another source root"
            );
            let count: i64 =
                transaction.query_row("SELECT count(*) FROM projects", [], |row| row.get(0))?;
            ensure!(
                count >= 0 && count < MAX_PROJECTS as i64,
                "service project registration capacity exhausted"
            );
            transaction.execute(
                "INSERT INTO projects VALUES(?1,?2,0,0)",
                params![registration.binding.to_string(), payload],
            )?;
            bump(&transaction)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, binding: ProjectBinding) -> Result<RegisteredProject> {
        let (bytes, epoch, paused) = self
            .connection
            .query_row(
                "SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE binding=?1",
                [binding.to_string()],
                project_row,
            )
            .context("service project is not registered")?;
        let project = decode(&bytes, epoch, paused)?;
        ensure!(
            project.registration.binding == binding,
            "foreign service registration"
        );
        Ok(project)
    }

    pub fn list(
        &mut self,
        after: Option<ProjectBinding>,
        expected_revision: Option<u64>,
        limit: usize,
    ) -> Result<ProjectPage> {
        ensure!(
            (1..=32).contains(&limit) && (after.is_none() || expected_revision.is_some()),
            "invalid service registry page request"
        );
        let transaction = self.connection.transaction()?;
        let revision: i64 =
            transaction.query_row("SELECT revision FROM service_meta WHERE id=1", [], |row| {
                row.get(0)
            })?;
        let revision = u64::try_from(revision).context("invalid service registry revision")?;
        ensure!(
            expected_revision.is_none_or(|expected| expected == revision),
            "service registry cursor expired"
        );
        let mut statement = transaction.prepare(
            "SELECT substr(payload,1,20481),epoch,paused FROM projects
            WHERE binding>?1 ORDER BY binding LIMIT ?2",
        )?;
        let mut rows = statement.query(params![
            after.map(|id| id.to_string()).unwrap_or_default(),
            (limit + 1) as i64
        ])?;
        let mut projects = Vec::new();
        let mut bytes = 0usize;
        let mut more = false;
        while let Some(row) = rows.next()? {
            let (payload, epoch, paused) = project_row(row)?;
            let project = decode(&payload, epoch, paused)?;
            let encoded = serde_json::to_vec(&project)?.len();
            if projects.len() == limit || bytes.saturating_add(encoded) > 48 * 1024 {
                more = true;
                break;
            }
            bytes += encoded;
            projects.push(project);
        }
        let after = if more {
            Some(
                projects
                    .last()
                    .context("service registration cannot fit page budget")?
                    .registration
                    .binding,
            )
        } else {
            None
        };
        Ok(ProjectPage {
            revision,
            projects,
            after,
        })
    }

    pub fn pause(&mut self, binding: ProjectBinding, paused: bool) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            transaction.execute(
                "UPDATE projects SET paused=?1 WHERE binding=?2",
                params![paused, binding.to_string()]
            )? == 1,
            "service project is not registered"
        );
        bump(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn admit_worker(&mut self, lease: &mut ProjectLease) -> Result<u64> {
        ensure!(
            lease.root == self.root && lease.epoch.is_none(),
            "foreign or already admitted service worker lease"
        );
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch: i64 = transaction.query_row(
            "SELECT epoch FROM projects WHERE binding=?1",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        let next = epoch
            .checked_add(1)
            .context("service fencing generation exhausted")?;
        transaction.execute(
            "UPDATE projects SET epoch=?1 WHERE binding=?2",
            params![next, lease.binding.to_string()],
        )?;
        bump(&transaction)?;
        transaction.commit()?;
        lease.epoch = Some(next as u64);
        Ok(next as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(name: &str) -> ProjectRegistration {
        ProjectRegistration::new(
            format!("/original/{name}"),
            "/derived/fixture".into(),
            name.into(),
        )
        .unwrap()
    }
    #[test]
    fn interrupted_owned_initialization_recovers_but_unknown_empty_files_are_preserved()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let first = Incarnation::from_bytes([1; 16]);
        owner(&root, false, first)?;
        fs::write(root.join("service-1.4.sqlite3"), [])?;
        let ledger = ServiceRegistry::open(&root, Incarnation::from_bytes([2; 16]))?;
        assert_eq!(ledger.incarnation()?, first);
        drop(ledger);
        let foreign = tempfile::tempdir()?;
        fs::write(foreign.path().join("service-1.4.sqlite3"), [])?;
        assert!(ServiceRegistry::open(foreign.path(), first).is_err());
        assert!(!foreign.path().join("service-1.4.owner.json").exists());
        assert_eq!(
            fs::metadata(foreign.path().join("service-1.4.sqlite3"))?.len(),
            0
        );
        Ok(())
    }
    #[test]
    fn reopen_preserves_registrations_and_exclusive_ownership_advances_fencing() -> Result<()> {
        let root = tempfile::tempdir()?;
        let first = Incarnation::from_bytes([1; 16]);
        let registration = project("one");
        let mut ledger = ServiceRegistry::open(root.path(), first)?;
        ledger.register(&registration)?;
        let mut lease = ProjectLease::acquire(root.path(), registration.binding)?;
        assert_eq!(ledger.admit_worker(&mut lease)?, 1);
        assert!(ProjectLease::acquire(root.path(), registration.binding).is_err());
        assert!(ledger.admit_worker(&mut lease).is_err());
        drop(lease);
        drop(ledger);
        let mut ledger = ServiceRegistry::open(root.path(), Incarnation::from_bytes([2; 16]))?;
        assert_eq!(ledger.incarnation()?, first);
        assert_eq!(ledger.get(registration.binding)?.fencing_generation, 1);
        let mut lease = ProjectLease::acquire(root.path(), registration.binding)?;
        assert_eq!(ledger.admit_worker(&mut lease)?, 2);
        Ok(())
    }
    #[test]
    fn paged_registry_expires_on_mutation_and_rejects_rebound_or_foreign_state() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut ledger = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
        let one = project("one");
        let two = project("two");
        ledger.register(&one)?;
        ledger.register(&two)?;
        let page = ledger.list(None, None, 1)?;
        assert!(page.after.is_some());
        assert_eq!(
            ledger
                .list(page.after, Some(page.revision), 1)?
                .projects
                .len(),
            1
        );
        ledger.pause(one.binding, true)?;
        assert!(ledger.list(page.after, Some(page.revision), 1).is_err());
        let mut rebound = one.clone();
        rebound.root = "/another/root".into();
        assert!(ledger.register(&rebound).is_err());
        assert_eq!(ledger.get(one.binding)?.registration, one);
        assert!(ledger.get(one.binding)?.paused);
        Ok(())
    }
}
