// SPDX-License-Identifier: Apache-2.0
use super::*;
use rusqlite::{Connection, params};

fn fixture() -> Result<(tempfile::TempDir, Connection, std::path::PathBuf)> {
    let root = tempfile::tempdir()?;
    let database = root.path().join("fixture.sqlite3");
    let connection = Connection::open(&database)?;
    connection.execute_batch("PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE sample(id INTEGER PRIMARY KEY,value BLOB NOT NULL);")?;
    for id in 0..100 {
        connection.execute("INSERT INTO sample VALUES(?1,zeroblob(1000))", [id])?;
    }
    Ok((
        root,
        connection,
        database.with_file_name("fixture.sqlite3-wal"),
    ))
}

#[test]
fn actual_sqlite_commits_are_recognized_without_modifying_the_overlay() -> Result<()> {
    let (_root, connection, wal) = fixture()?;
    let before = fs::read(&wal)?;
    let observed = observe(&wal)?.expect("WAL missing");
    assert!(observed.valid_frames > 0 && observed.last_commit_frame > 0);
    assert_eq!(observed.uncommitted_frames, 0);
    assert_eq!(observed.stale_epoch_tail_bytes, 0);
    assert_eq!(fs::read(&wal)?, before);
    assert_eq!(
        connection.query_row("SELECT count(*) FROM sample", [], |row| row
            .get::<_, i64>(0))?,
        100
    );
    Ok(())
}

#[test]
fn corrupt_or_incomplete_current_frames_are_preserved_and_refused() -> Result<()> {
    let (_root, _connection, wal) = fixture()?;
    let original = fs::read(&wal)?;
    for offset in [0, 24, 32 + 24 + 7, 32 + 8] {
        let mut damaged = original.clone();
        damaged[offset] ^= 1;
        fs::write(&wal, &damaged)?;
        assert!(observe(&wal).is_err(), "damaged WAL offset {offset}");
        assert_eq!(fs::read(&wal)?, damaged);
    }
    let incomplete = &original[..original.len() - 1];
    fs::write(&wal, incomplete)?;
    assert!(observe(&wal).is_err());
    assert_eq!(fs::read(&wal)?, incomplete);
    fs::write(&wal, original)?;
    Ok(())
}

#[test]
fn sqlite_reset_retained_old_epoch_tail_is_disclosed_and_never_replayed() -> Result<()> {
    let (_root, connection, wal) = fixture()?;
    connection.execute_batch("PRAGMA wal_checkpoint(RESTART);")?;
    connection.execute(
        "UPDATE sample SET value=?1 WHERE id=0",
        params![b"changed".as_slice()],
    )?;
    let before = fs::read(&wal)?;
    let observed = observe(&wal)?.expect("WAL missing");
    assert!(observed.valid_frames > 0);
    assert!(observed.stale_epoch_tail_bytes > 0);
    assert_eq!(
        observed.valid_frames * FRAME_BYTES as u64 + observed.stale_epoch_tail_bytes + 32,
        observed.file_bytes
    );
    assert_eq!(fs::read(&wal)?, before);
    assert_eq!(
        connection.query_row("SELECT value FROM sample WHERE id=0", [], |row| row
            .get::<_, Vec<u8>>(0))?,
        b"changed"
    );
    Ok(())
}
