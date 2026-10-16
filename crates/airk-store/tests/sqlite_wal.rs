// SPDX-License-Identifier: Apache-2.0
//! Exercise the bundled SQLite VFS independently of AIRK's retry wrappers.
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::{sync::Barrier, time::Duration};

#[test]
fn concurrent_wal_readers_on_canonical_long_paths_preserve_transactions() -> Result<()> {
    let root = tempfile::tempdir()?;
    let directory = root
        .path()
        .canonicalize()?
        .join("long-path-segment".repeat(8));
    let directory = directory.join("another-path-segment".repeat(6));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("index.sqlite3");
    // On Windows, canonicalization supplies the extended local path prefix.
    // Keep it: stripping that prefix would conceal the VFS bug and break long paths.
    assert!(path.as_os_str().len() > 260);
    let writer = Connection::open(&path)?;
    writer.busy_timeout(Duration::from_secs(2))?;
    writer.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
         CREATE TABLE revision(id INTEGER PRIMARY KEY, generation INTEGER, witness INTEGER);
         INSERT INTO revision VALUES(1,0,0);",
    )?;
    drop(writer);
    let start = Barrier::new(5);
    std::thread::scope(|scope| -> Result<()> {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| -> Result<()> {
                    start.wait();
                    for _ in 0..512 {
                        let connection =
                            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                        connection.busy_timeout(Duration::from_secs(2))?;
                        connection.execute_batch("BEGIN DEFERRED")?;
                        let before: i64 = connection
                            .query_row("SELECT generation FROM revision WHERE id=1", [], |row| {
                                row.get(0)
                            })
                            .context("begin canonical-path WAL read")?;
                        std::thread::yield_now();
                        let witness: i64 = connection.query_row(
                            "SELECT witness FROM revision WHERE id=1",
                            [],
                            |row| row.get(0),
                        )?;
                        assert_eq!(before, witness);
                        connection.execute_batch("COMMIT")?;
                    }
                    Ok(())
                })
            })
            .collect();
        start.wait();
        let writes = (|| -> Result<()> {
            for generation in 1..=64 {
                let mut writer = Connection::open(&path)?;
                writer.busy_timeout(Duration::from_secs(2))?;
                let transaction = writer.transaction()?;
                transaction.execute(
                    "UPDATE revision SET generation=?1,witness=?1 WHERE id=1",
                    [generation],
                )?;
                transaction
                    .commit()
                    .context("commit concurrent WAL writer")?;
                std::thread::yield_now();
            }
            Ok(())
        })();
        // Join every reader even if one fails, before releasing the database.
        let results: Vec<_> = readers
            .into_iter()
            .map(|reader| reader.join().expect("WAL reader panicked"))
            .collect();
        writes?;
        for result in results {
            result?;
        }
        Ok(())
    })?;
    let writer = Connection::open(&path)?;
    let integrity: String = writer.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    assert_eq!(integrity, "ok");
    assert_eq!(
        writer.query_row("SELECT generation FROM revision", [], |row| row
            .get::<_, i64>(0))?,
        64
    );
    Ok(())
}
