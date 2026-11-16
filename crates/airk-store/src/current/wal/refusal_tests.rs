// SPDX-License-Identifier: Apache-2.0
//! Service WAL admission never treats a legacy rollback overlay as recovery.
use super::validate_files;
use anyhow::Result;
use std::fs;

#[test]
fn service_wal_admission_refuses_empty_and_zeroed_rollback_sidecars_without_writing() -> Result<()>
{
    for bytes in [0, 512] {
        let owner = tempfile::tempdir()?;
        let database = owner.path().join("current.sqlite3");
        let original = b"literal main bytes: admission must not open or repair this file";
        fs::write(&database, original)?;
        // The existing admission seam checks file bounds before SQLite opens.
        validate_files(owner.path(), 64 << 20)?;
        let journal = owner.path().join("current.sqlite3-journal");
        let injected = vec![0u8; bytes];
        fs::write(&journal, &injected)?;
        let error = validate_files(owner.path(), 64 << 20).unwrap_err();
        assert_eq!(
            error.to_string(),
            "rollback sidecar cannot overlay service WAL storage"
        );
        assert_eq!(fs::read(&database)?.as_slice(), original);
        assert_eq!(fs::read(&journal)?, injected);
        assert_eq!(
            fs::read_dir(owner.path())?.count(),
            2,
            "no recovery side effects"
        );
    }
    Ok(())
}
