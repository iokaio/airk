// SPDX-License-Identifier: Apache-2.0
//! Service publication ownership and crash-surviving sidecar admission.
use super::{CurrentScope, database, wal_budget};
use crate::{
    FileStore, WriterLock,
    quota::{CurrentGrant, CurrentGrantScope, Quota},
};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

pub(super) const VERSION: u32 = 140;
pub(super) const WAL_BYTES: u64 = 64 << 20;
pub(super) const SHM_BYTES: u64 = 128 << 10;
// Quotas count logical bytes; reserve allocation slack for these three files as
// well. This does not claim to impose a physical filesystem quota.
const SLACK: u64 = 3 * 65536;

pub(super) fn enabled(directory: &Path) -> bool {
    crate::managed_collection::ManagedPath::parse(directory)
        .is_ok_and(|path| path.namespace == "collections-1.4")
}
pub(super) fn id(scope: &CurrentScope) -> Sha256Digest {
    framed_digest(
        b"service-wal-capacity/1.4",
        &[
            scope.root.as_bytes(),
            scope.collection.as_bytes(),
            scope.incarnation.as_bytes(),
        ],
    )
}
pub(super) fn total(capacity: u64) -> Result<u64> {
    capacity
        .checked_add(WAL_BYTES + SHM_BYTES + SLACK)
        .context("WAL allowance overflow")
}
pub(super) fn validate_files(directory: &Path, capacity: u64) -> Result<()> {
    for (name, limit) in [
        ("current.sqlite3", capacity),
        ("current.sqlite3-wal", WAL_BYTES),
        ("current.sqlite3-shm", SHM_BYTES),
    ] {
        let path = directory.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit,
            "invalid or unadmitted service database file: {name}"
        );
        crate::managed_collection::reject_file_aliases(&path, &metadata)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(metadata.nlink() == 1, "hard-linked service database file");
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected service database file"
            );
        }
    }
    ensure!(
        !directory.join("current.sqlite3-journal").try_exists()?,
        "rollback sidecar cannot overlay service WAL storage"
    );
    Ok(())
}

/// Must precede the first SQLite open, including recovery. Dropping the returned
/// grant preserves it. Only explicit owned-store retirement can release it.
pub(super) fn reserve(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    capacity: u64,
) -> Result<CurrentGrant> {
    validate_files(store.directory(), capacity)?;
    let binding = CurrentGrantScope {
        collection: scope.collection.clone(),
        incarnation: scope.incarnation,
        transaction: id(scope),
    };
    let bound = total(capacity)?;
    let mut grant = match quota.current_grant(writer, &binding)? {
        Some(grant) => grant,
        None => {
            ensure!(
                !store.directory().join("current.sqlite3").try_exists()?,
                "service database has no durable WAL recovery allowance"
            );
            quota.reserve_current(writer, binding, bound)?
        }
    };
    if grant.capacity() < bound {
        grant.resize(writer, bound)?;
    }
    Ok(grant)
}

pub(super) fn vfs() -> Result<String> {
    wal_budget::register(WAL_BYTES, SHM_BYTES)
}

/// Retain already admitted sidecar names across the last connection's close.
/// This flag belongs to a connection, so every service reader/writer must set it.
/// SQLite still performs its final checkpoint; normal size limits still apply.
pub(super) fn retain_sidecars(connection: &mut rusqlite::Connection) -> Result<()> {
    use rusqlite::ffi;
    for requested in [1, -1] {
        let mut setting: std::ffi::c_int = requested;
        // SAFETY: this exclusive connection borrow precedes statements. SQLite
        // accepts a writable int for PERSIST_WAL; no pointer escapes the call.
        let status = unsafe {
            ffi::sqlite3_file_control(
                connection.handle(),
                c"main".as_ptr(),
                ffi::SQLITE_FCNTL_PERSIST_WAL,
                (&mut setting as *mut std::ffi::c_int).cast(),
            )
        };
        ensure!(
            status == ffi::SQLITE_OK && setting == 1,
            "service SQLite VFS cannot retain admitted WAL sidecars"
        );
    }
    Ok(())
}

pub(super) fn version(directory: &Path) -> u32 {
    if enabled(directory) {
        VERSION
    } else {
        database::DATABASE_VERSION
    }
}
