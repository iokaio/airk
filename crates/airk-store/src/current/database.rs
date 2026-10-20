// SPDX-License-Identifier: Apache-2.0
//! The authoritative database is recoverable; private overlays may use OFF.
use super::CurrentScope;
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    hooks::{AuthAction, AuthContext, Authorization},
};
use std::io::{Read, Write};
use std::{fs, path::Path, time::Duration};

pub const DATABASE_VERSION: u32 = 120;
pub const PAGE_BYTES: u64 = 4096;
const APPLICATION_ID: i32 = 0x4149_524b;
pub const MAX_DATABASE_BYTES: u64 = 16 * 1024 * 1024 * 1024 * 1024;
pub const OWNER_FILE: &str = "current-owner.json";

/// The 1.2 current database has its own filename and never shares a collection
/// directory with the 1.1 snapshot/catalog layout. The caller must first reset
/// the old managed derived collection under its writer and reader gates.
pub(crate) fn reject_legacy_collection(directory: &Path) -> Result<()> {
    for name in [
        "index.sqlite3",
        "index.sqlite3-wal",
        "index.sqlite3-shm",
        "index.sqlite3-journal",
        "snapshot.json",
        "generations",
    ] {
        ensure!(
            !directory.join(name).try_exists()?,
            "AIRK 1.1.x collection state is incompatible with 1.2.0; reset the managed collection and rebuild"
        );
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    scope: CurrentScope,
    capacity: u64,
}

pub(crate) fn check_owner(directory: &Path, scope: &CurrentScope) -> Result<u64> {
    let owner = read_owner(directory)?;
    ensure!(owner.scope == *scope, "foreign current ownership header");
    Ok(owner.capacity)
}

pub(crate) fn discover_scope(
    directory: &Path,
    root: &str,
    collection: &str,
) -> Result<CurrentScope> {
    let owner = read_owner(directory)?;
    ensure!(
        owner.scope.root == root && owner.scope.collection == collection,
        "foreign current ownership header"
    );
    Ok(owner.scope)
}

pub(crate) fn owned_scope(directory: &Path) -> Result<CurrentScope> {
    Ok(read_owner(directory)?.scope)
}

fn read_owner(directory: &Path) -> Result<Owner> {
    let path = directory.join(OWNER_FILE);
    let metadata = fs::symlink_metadata(&path)
        .context("current ownership header missing; run airk index --full")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 65_536,
        "invalid current ownership header"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65_537).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 65_536,
        "current ownership header exceeds bound"
    );
    let owner: Owner = serde_json::from_slice(&bytes)?;
    validate_capacity(owner.capacity)?;
    owner.scope.validate()?;
    ensure!(
        owner.version == DATABASE_VERSION && serde_json::to_vec(&owner)? == bytes,
        "foreign current ownership header"
    );
    Ok(owner)
}

pub(crate) fn create_owner(directory: &Path, scope: &CurrentScope, capacity: u64) -> Result<()> {
    if directory.join(OWNER_FILE).try_exists()? {
        ensure!(
            check_owner(directory, scope)? == capacity,
            "current database capacity differs from captured owner"
        );
        return Ok(());
    }
    let bytes = serde_json::to_vec(&Owner {
        version: DATABASE_VERSION,
        scope: scope.clone(),
        capacity,
    })?;
    ensure!(
        bytes.len() <= 65_536,
        "current ownership header exceeds bound"
    );
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    crate::durability::persist_synced(file, &directory.join(OWNER_FILE))
}

/// The exclusive collection writer calls this only after reserving an allowance
/// for the larger limit. The new owner bound is durable before SQLite may use it.
pub(crate) fn grow_owner(
    directory: &Path,
    scope: &CurrentScope,
    previous: u64,
    next: u64,
) -> Result<()> {
    validate_capacity(next)?;
    ensure!(next > previous, "current database capacity must grow");
    ensure!(
        check_owner(directory, scope)? == previous,
        "current database capacity changed under writer"
    );
    let bytes = serde_json::to_vec(&Owner {
        version: DATABASE_VERSION,
        scope: scope.clone(),
        capacity: next,
    })?;
    ensure!(
        bytes.len() <= 65_536,
        "current ownership header exceeds bound"
    );
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    crate::durability::persist_synced(file, &directory.join(OWNER_FILE))
}

pub(crate) fn validate_capacity(bytes: u64) -> Result<u64> {
    ensure!(
        (PAGE_BYTES * 16..=MAX_DATABASE_BYTES).contains(&bytes) && bytes.is_multiple_of(PAGE_BYTES),
        "invalid current database capacity"
    );
    Ok(bytes / PAGE_BYTES)
}

pub(crate) fn plain_database(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_DATABASE_BYTES,
        "invalid current database file"
    );
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = path.with_file_name(format!(
            "{}{}",
            path.file_name()
                .context("database name missing")?
                .to_string_lossy(),
            suffix
        ));
        if let Ok(metadata) = fs::symlink_metadata(sidecar) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "invalid current database sidecar"
            );
            ensure!(
                suffix == "-journal",
                "current database uses unsupported WAL state; rebuild with airk index --full"
            );
        }
    }
    Ok(())
}

pub(crate) fn require_version(connection: &Connection) -> Result<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let application: i32 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    ensure!(
        version == DATABASE_VERSION && application == APPLICATION_ID,
        "obsolete current index format; run airk index --full"
    );
    let page: i64 = connection.pragma_query_value(None, "page_size", |r| r.get(0))?;
    ensure!(
        u64::try_from(page)? == PAGE_BYTES,
        "unsupported current database page size"
    );
    Ok(())
}

pub(crate) fn open_reader(path: &Path) -> Result<Connection> {
    plain_database(path)?;
    let journal = path.with_file_name("current.sqlite3-journal");
    ensure!(
        !journal.try_exists()?,
        "current index requires writer recovery before reads"
    );
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-4096; BEGIN;")?;
    require_version(&connection)?;
    Ok(connection)
}

pub(crate) fn open_writer(path: &Path, capacity: u64, create: bool) -> Result<Connection> {
    open_writer_mode(path, capacity, create, false)
}

pub(crate) fn open_bootstrap(path: &Path, capacity: u64) -> Result<(Connection, bool)> {
    let connection = open_writer_mode(path, capacity, false, true)?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok((connection, version == 0))
}

fn open_writer_mode(
    path: &Path,
    capacity: u64,
    create: bool,
    bootstrap: bool,
) -> Result<Connection> {
    let pages = validate_capacity(capacity)?;
    if !create {
        plain_database(path)?;
    }
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let connection = Connection::open_with_flags(path, flags)?;
    connection.busy_timeout(Duration::ZERO)?;
    if create {
        connection.pragma_update(None, "page_size", PAGE_BYTES as i64)?;
    } else {
        // SQLite hot-journal rollback may occur here. The caller already owns
        // writer + exclusive request gate and a durable recovery allowance.
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if bootstrap && version == 0 {
            let objects: i64 =
                connection.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
            ensure!(
                objects == 0,
                "bootstrap database contains an unrecognized schema"
            );
        } else {
            require_version(&connection)?;
        }
    }
    connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA; PRAGMA locking_mode=NORMAL; PRAGMA auto_vacuum=NONE; PRAGMA temp_store=MEMORY; PRAGMA trusted_schema=OFF; PRAGMA cache_size=-16384; PRAGMA cache_spill=ON; PRAGMA mmap_size=0;")?;
    let actual: i64 =
        connection.query_row(&format!("PRAGMA max_page_count={pages}"), [], |r| r.get(0))?;
    ensure!(
        u64::try_from(actual)? == pages,
        "current SQLite database exceeds admitted page capacity"
    );
    let journal = path.with_file_name("current.sqlite3-journal");
    if !create && journal.try_exists()? {
        // SQLite leaves cold (including zero-header) journals untouched when
        // opening a writer. Readers deliberately reject every journal. Write
        // the unchanged format version through SQLite's normal commit path to
        // retire it under the caller's exclusive gate and recovery allowance.
        // Bootstrap's zero version stays zero; no head or generation advances.
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        connection.pragma_update(None, "user_version", version)?;
        ensure!(
            !journal.try_exists()?,
            "SQLite did not retire the cold current rollback journal"
        );
    }
    Ok(connection)
}

pub(crate) fn restrict(connection: &Connection) -> Result<()> {
    connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
        AuthAction::Attach { .. }
        | AuthAction::Detach { .. }
        | AuthAction::Savepoint { .. }
        | AuthAction::CreateTrigger { .. }
        | AuthAction::CreateTempTrigger { .. }
        | AuthAction::CreateVtable { .. }
        | AuthAction::CreateTempTable { .. }
        | AuthAction::CreateTempIndex { .. }
        | AuthAction::CreateTempView { .. } => Authorization::Deny,
        AuthAction::Pragma {
            pragma_value: Some(_),
            ..
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }))?;
    Ok(())
}

/// A worst-case rollback journal for this pinned pager, including an initial
/// header, a possible empty first spill, per-record headers and sector padding.
/// No savepoints/ATTACH/auto-vacuum or continuation after an error is permitted.
pub fn journal_bound(original_pages: u64, sector_bytes: u64) -> Result<u64> {
    ensure!(
        (32..=65_536).contains(&sector_bytes) && sector_bytes.is_power_of_two(),
        "invalid SQLite effective sector size"
    );
    original_pages
        .checked_mul(PAGE_BYTES + 8 + 2 * sector_bytes)
        .and_then(|n| n.checked_add(4 * sector_bytes))
        .context("current rollback journal capacity overflow")
}

/// The pointer never leaves this exclusive mutable borrow. This mirrors
/// sqlite3.c setSectorSize() in bundled SQLite 3.53.3, including PSOW handling.
pub(crate) fn effective_sector(connection: &mut Connection) -> Result<u64> {
    use rusqlite::ffi;
    let mut file: *mut ffi::sqlite3_file = std::ptr::null_mut();
    // SAFETY: connection owns the live handle; &mut excludes statements and
    // concurrent calls. FILE_POINTER writes exactly one sqlite3_file pointer.
    let status = unsafe {
        ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            ffi::SQLITE_FCNTL_FILE_POINTER,
            (&mut file as *mut *mut ffi::sqlite3_file).cast(),
        )
    };
    ensure!(
        status == ffi::SQLITE_OK && !file.is_null(),
        "SQLite database file inspection failed"
    );
    // SAFETY: a successful FILE_POINTER returns this connection's live file;
    // SQLite owns its method table until the connection closes. Neither pointer
    // is retained, mutated, freed, or made available to a caller.
    let (sector, properties) = unsafe {
        let methods = (*file)
            .pMethods
            .as_ref()
            .context("SQLite file methods missing")?;
        let sector = methods
            .xSectorSize
            .context("SQLite sector inspection unavailable")?;
        let properties = methods
            .xDeviceCharacteristics
            .context("SQLite device inspection unavailable")?;
        (sector(file), properties(file))
    };
    let effective = if properties & ffi::SQLITE_IOCAP_POWERSAFE_OVERWRITE != 0 || sector < 32 {
        512
    } else {
        sector.min(65_536) as u64
    };
    ensure!(
        effective.is_power_of_two(),
        "unsupported SQLite sector alignment"
    );
    Ok(effective)
}

pub(crate) fn allowance(connection: &mut Connection, capacity: u64) -> Result<u64> {
    validate_capacity(capacity)?;
    let pages: i64 = connection.pragma_query_value(None, "page_count", |r| r.get(0))?;
    let pages = u64::try_from(pages)?;
    let used = pages
        .checked_mul(PAGE_BYTES)
        .context("current database size overflow")?;
    let growth = capacity
        .checked_sub(used)
        .context("current database exceeds capacity")?;
    let journal = journal_bound(pages, effective_sector(connection)?)?;
    growth
        .checked_add(journal)
        .context("current write capacity overflow")
}

/// Bound crash recovery before SQLite is allowed to read a hot journal. The
/// owner header and durable grant are checked by the caller before this probe.
pub(crate) fn recovery_bound(path: &Path, capacity: u64, allowance: u64) -> Result<()> {
    plain_database(path)?;
    ensure!(
        fs::metadata(path)?.len() <= capacity,
        "current database exceeds recovery capacity"
    );
    let journal = path.with_file_name("current.sqlite3-journal");
    let Ok(metadata) = fs::symlink_metadata(&journal) else {
        return Ok(());
    };
    ensure!(
        metadata.len() <= allowance,
        "current rollback journal exceeds durable allowance"
    );
    ensure!(
        metadata.len() <= journal_bound(capacity / PAGE_BYTES, 65_536)?,
        "current rollback journal exceeds admitted recovery bound"
    );
    if metadata.len() >= 28 {
        let mut header = [0u8; 28];
        fs::File::open(journal)?.read_exact(&mut header)?;
        // A zero magic is an uncommitted/cold header. SQLite decides whether it
        // is hot; we still bounded its entire file above.
        if header[..8] == [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7] {
            let original = u32::from_be_bytes(header[16..20].try_into()?) as u64;
            let sector = u32::from_be_bytes(header[20..24].try_into()?) as u64;
            let page = u32::from_be_bytes(header[24..28].try_into()?) as u64;
            ensure!(
                page == PAGE_BYTES
                    && original <= capacity / PAGE_BYTES
                    && metadata.len() <= journal_bound(original, sector)?,
                "current rollback journal header exceeds recovery bound"
            );
        }
    }
    Ok(())
}
