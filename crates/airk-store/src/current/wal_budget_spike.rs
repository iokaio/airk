// SPDX-License-Identifier: Apache-2.0
//! Native file-admission regressions shared with the runtime adapter.
use super::wal_budget::register;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn connection(path: &Path, vfs: &str) -> Result<Connection> {
    let connection = Connection::open_with_flags_and_vfs(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        vfs,
    )?;
    connection.busy_timeout(std::time::Duration::ZERO)?;
    Ok(connection)
}

#[test]
fn wal_budget_spike_rejects_growth_before_disk_write_and_preserves_last_commit() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("capped.sqlite3");
    let cap = 65536;
    let vfs = register(cap, 128 << 10)?;
    let writer = connection(&path, &vfs)?;
    writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE counter(id INTEGER PRIMARY KEY,value INTEGER); INSERT INTO counter VALUES(1,0); PRAGMA wal_checkpoint(TRUNCATE)")?;
    let reader = connection(&path, &vfs)?;
    reader.execute_batch("BEGIN")?;
    let zero: i64 = reader.query_row("SELECT value FROM counter", [], |row| row.get(0))?;
    assert_eq!(zero, 0);
    let mut committed = 0;
    let mut full = false;
    let wal = path.with_file_name("capped.sqlite3-wal");
    for value in 1..100 {
        match writer.execute("UPDATE counter SET value=?1", [value]) {
            Ok(_) => committed = value,
            Err(error) => {
                assert!(
                    matches!(error,rusqlite::Error::SqliteFailure(code,_) if code.code==rusqlite::ErrorCode::DiskFull)
                );
                full = true;
                break;
            }
        }
        assert!(std::fs::metadata(&wal)?.len() <= cap);
    }
    assert!(full && committed > 0);
    assert!(std::fs::metadata(&wal)?.len() <= cap);
    let latest = connection(&path, &vfs)?;
    let saved: i64 = latest.query_row("SELECT value FROM counter", [], |row| row.get(0))?;
    assert_eq!(saved, committed);
    assert_eq!(
        reader.query_row("SELECT value FROM counter", [], |row| row.get::<_, i64>(0))?,
        0
    );
    reader.execute_batch("ROLLBACK")?;
    writer.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    writer.execute("UPDATE counter SET value=1000", [])?;
    assert!(std::fs::metadata(&wal)?.len() <= cap);
    let integrity: String = latest.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    assert_eq!(integrity, "ok");
    Ok(())
}

#[test]
fn wal_budget_spike_rejects_unadmitted_shared_memory_before_mapping() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("no-shm.sqlite3");
    let vfs = register(65536, 0)?;
    let writer = connection(&path, &vfs)?;
    writer.execute_batch("PRAGMA journal_mode=WAL")?;
    assert!(
        writer
            .execute_batch("CREATE TABLE blocked(id INTEGER)")
            .is_err()
    );
    let shm = path.with_file_name("no-shm.sqlite3-shm");
    assert!(!shm.exists() || std::fs::metadata(shm)?.len() == 0);
    Ok(())
}
