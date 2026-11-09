// SPDX-License-Identifier: Apache-2.0
//! Owned registry helpers shared by the Genesis adapter, the retained native
//! predecessor refusal evidence and readers of existing legacy compatibility
//! grants. AIRK no longer upgrades a registry in place.
use crate::service::ProjectLease;
use airk_core::identity::{Incarnation, Sha256Digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const DATABASE: &str = "service-1.4.sqlite3";
const MAX_DIRECTORY_ENTRIES: usize = 16384;
const MAX_WORKER_LOCKS: usize = 4096;

mod settlement;
pub use settlement::{
    NativeReadRefusalEntrypoint, NativeRefusalDiagnostic, NativeRefusalEntrypoints,
    NativeRefusalProvenance, NativeWriteRefusalEntrypoint, RuntimeRefusal,
};

pub mod compatibility;

mod exclusion;

pub(super) fn validate_registry_files(root: &Path) -> Result<PathBuf> {
    let path = root.join(DATABASE);
    let metadata = fs::symlink_metadata(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() > 0 && metadata.len() <= super::super::MAX_DATABASE_BYTES,
        "invalid catalog source registry size"
    );
    // This explicit owned adapter may observe committed WAL. Ordinary opens
    // continue to reject overlays. No source/collection files are opened here.
    for (suffix, maximum) in [
        ("-wal", 64 * 1024 * 1024),
        ("-shm", 1024 * 1024),
        ("-journal", 64 * 1024 * 1024),
    ] {
        let overlay = root.join(format!("{DATABASE}{suffix}"));
        match fs::symlink_metadata(&overlay) {
            Ok(metadata) => {
                crate::managed_collection::reject_file_aliases(&overlay, &metadata)?;
                ensure!(
                    metadata.len() <= maximum,
                    "registry overlay exceeds upgrade bound"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let wal = super::wal::observe(&root.join(format!("{DATABASE}-wal")))?;
    ensure!(
        wal.is_some() || !root.join(format!("{DATABASE}-shm")).try_exists()?,
        "registry shared-memory overlay has no recognized WAL"
    );
    Ok(path)
}

/// Shared only by owned offline adapters after exact intent/fence and every
/// service/helper/watcher/worker lease have been checked. This is not admission.
pub(super) fn recover_owned_registry(
    root: &Path,
    operation: Incarnation,
    expected_namespace: Incarnation,
    expected_rows: Option<Sha256Digest>,
    backups: &[super::backup::BackupAttempt],
    migration_admitted: bool,
) -> Result<()> {
    let journal = root.join(format!("{DATABASE}-journal"));
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(metadata) if metadata.len() > 0 => metadata,
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        migration_admitted,
        "rollback journal requires owned migration recovery"
    );
    crate::managed_collection::reject_file_aliases(&journal, &metadata)?;
    let path = validate_registry_files(root)?;
    ensure!(
        !root.join(format!("{DATABASE}-wal")).try_exists()?,
        "conflicting migration journal and WAL"
    );
    let attempt = backups
        .iter()
        .find(|attempt| attempt.complete)
        .context("migration recovery requires a qualified snapshot")?;
    let snapshot = super::backup::validate(root, operation, attempt)?;
    let backup = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let namespace: String = backup.query_row(
        "SELECT incarnation FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        namespace == expected_namespace.to_string()
            && Some(super::backup::legacy_data_digest(&backup)?) == expected_rows,
        "migration recovery snapshot identity changed"
    );
    let mut header = [0; 100];
    std::io::Read::read_exact(&mut fs::File::open(&path)?, &mut header)?;
    ensure!(
        &header[..16] == b"SQLite format 3\0"
            && u32::from_be_bytes(header[68..72].try_into()?) == super::super::APPLICATION as u32
            && matches!(u32::from_be_bytes(header[60..64].try_into()?), 11..=13),
        "migration recovery registry header is foreign or damaged"
    );
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
    )?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        matches!(version, 11..=13),
        "migration recovery returned an unsupported registry"
    );
    super::validate_shape(&connection, version)?;
    super::backup::integrity(&connection)?;
    ensure!(
        Some(super::backup::legacy_data_digest(&connection)?) == expected_rows,
        "migration recovery changed legacy rows; snapshot retained for explicit repair"
    );
    Ok(())
}

/// Fresh genesis has no registered projects, but still fences orphan worker
/// lock entries through the bounded native ownership census.
pub(super) fn genesis_worker_leases(root: &Path) -> Result<Vec<ProjectLease>> {
    exclusion::worker_leases(root, std::iter::empty())
}
