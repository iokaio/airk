// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{os::windows::fs::OpenOptionsExt, time::Instant};

struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("current.sqlite3");
        let connection = open_writer(&path, 8 * 1024 * 1024, true)?;
        connection.pragma_update(None, "application_id", APPLICATION_ID)?;
        connection.pragma_update(None, "user_version", DATABASE_VERSION)?;
        connection.execute_batch(
            "CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1);",
        )?;
        drop(connection);
        Ok(Self {
            _directory: directory,
            path,
        })
    }

    fn block_writes(&self) -> Result<fs::File> {
        // A real Windows handle that permits other readers but denies writers.
        Ok(fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&self.path)?)
    }

    fn flags() -> OpenFlags {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    }
}

#[test]
fn sqlite_writer_reopens_after_windows_sharing_conflict_clears() -> Result<()> {
    let fixture = Fixture::new()?;
    let blocker = fixture.block_writes()?;
    // Establish that SQLite itself silently falls back despite READ_WRITE.
    let fallback = Connection::open_with_flags(&fixture.path, Fixture::flags())?;
    assert!(fallback.is_readonly("main")?);
    drop(fallback);
    let result = std::thread::scope(|scope| {
        let release = scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(blocker);
        });
        let opened = open_writer(&fixture.path, 8 * 1024 * 1024, false);
        release.join().unwrap();
        opened
    });
    let connection = result?;
    assert!(!connection.is_readonly("main")?);
    connection.execute("INSERT INTO evidence VALUES(2)", [])?;
    drop(connection);
    let read = open_reader(&fixture.path)?;
    let values = read
        .prepare("SELECT value FROM evidence ORDER BY value")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assert_eq!(values, [1, 2]);
    assert!(
        !fixture
            .path
            .with_file_name("current.sqlite3-journal")
            .exists()
    );
    Ok(())
}

#[test]
fn sqlite_writer_sharing_deadline_preserves_database_and_reports_native_cause() -> Result<()> {
    for existing_journal in [None, Some(vec![0u8; 512])] {
        let fixture = Fixture::new()?;
        let before = fs::read(&fixture.path)?;
        let journal = fixture.path.with_file_name("current.sqlite3-journal");
        if let Some(bytes) = &existing_journal {
            fs::write(&journal, bytes)?;
        }
        let blocker = fixture.block_writes()?;
        let budget = Duration::from_millis(75);
        let started = Instant::now();
        let error =
            open_windows_writer_connection(&fixture.path, Fixture::flags(), budget).unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            Some(32)
        );
        assert!(started.elapsed() >= budget);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(format!("{error:#}").contains("75 ms"));
        assert_eq!(fs::read(&fixture.path)?, before);
        if let Some(bytes) = &existing_journal {
            assert_eq!(fs::read(&journal)?, *bytes);
        } else {
            assert!(!journal.exists());
        }
        drop(blocker);
        assert!(!open_writer(&fixture.path, 8 * 1024 * 1024, false)?.is_readonly("main")?);
        assert!(!journal.exists());
    }
    Ok(())
}

#[test]
fn sqlite_writer_permission_failure_is_immediate_and_preserves_database() -> Result<()> {
    struct RestorePermissions<'a>(&'a Path, fs::Permissions);
    impl Drop for RestorePermissions<'_> {
        fn drop(&mut self) {
            fs::set_permissions(self.0, self.1.clone()).unwrap();
        }
    }
    let fixture = Fixture::new()?;
    let before = fs::read(&fixture.path)?;
    let original = fs::metadata(&fixture.path)?.permissions();
    let _restore = RestorePermissions(&fixture.path, original.clone());
    let mut readonly = original;
    readonly.set_readonly(true);
    fs::set_permissions(&fixture.path, readonly)?;
    // Zero sharing budget distinguishes a permission failure from a generic
    // readonly retry/deadline error without a wall-clock assertion.
    let error = open_windows_writer_connection(&fixture.path, Fixture::flags(), Duration::ZERO)
        .unwrap_err();
    let native = error.downcast_ref::<std::io::Error>().unwrap();
    assert_eq!(native.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(native.raw_os_error(), Some(5));
    assert!(!format!("{error:#}").contains("sharing retry"));
    assert_eq!(fs::read(&fixture.path)?, before);
    assert!(
        !fixture
            .path
            .with_file_name("current.sqlite3-journal")
            .exists()
    );
    Ok(())
}
