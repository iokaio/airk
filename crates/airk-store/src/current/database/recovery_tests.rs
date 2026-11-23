// SPDX-License-Identifier: Apache-2.0
//! The reader open waits out another connection's WAL-index recovery, and
//! nothing else. Clock and sleep are injected; no test sleeps.
use super::*;
use std::cell::Cell;

fn sqlite(code: std::ffi::c_int) -> anyhow::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into()
}

/// Runs the decision over a scripted sequence of open results; each sleep
/// advances the fake clock by what was asked.
fn run(script: Vec<std::ffi::c_int>) -> (Result<&'static str>, usize, Duration) {
    let opens = Cell::new(0usize);
    let now = Cell::new(Duration::ZERO);
    let result = wait_out_wal_recovery(
        || {
            let index = opens.get();
            opens.set(index + 1);
            match script.get(index) {
                Some(&code) => Err(sqlite(code)),
                None => Ok("snapshot"),
            }
        },
        || now.get(),
        |step| now.set(now.get() + step),
    );
    (result, opens.get(), now.get())
}

#[test]
fn a_reader_open_waits_out_wal_recovery_and_then_opens() {
    let recovery = rusqlite::ffi::SQLITE_BUSY_RECOVERY;
    let (result, opens, waited) = run(vec![recovery, recovery, recovery]);
    assert_eq!(result.unwrap(), "snapshot");
    assert_eq!(opens, 4);
    assert_eq!(waited, Duration::from_millis(15));
}

#[test]
fn a_reader_open_never_waits_on_other_contention_or_failures() {
    for code in [
        rusqlite::ffi::SQLITE_BUSY,
        rusqlite::ffi::SQLITE_LOCKED,
        rusqlite::ffi::SQLITE_BUSY_SNAPSHOT,
        rusqlite::ffi::SQLITE_CORRUPT,
        rusqlite::ffi::SQLITE_IOERR,
    ] {
        let (result, opens, waited) = run(vec![code]);
        let error = result.unwrap_err();
        assert!(
            matches!(error.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(failure, _)) if failure.extended_code == code),
            "code {code} must be returned unchanged"
        );
        assert_eq!(
            (opens, waited),
            (1, Duration::ZERO),
            "code {code} must not wait"
        );
    }
}

#[test]
fn a_recovery_longer_than_half_a_second_is_returned_as_contention() {
    let (result, opens, waited) = run(vec![rusqlite::ffi::SQLITE_BUSY_RECOVERY; 1_000]);
    let error = result.unwrap_err();
    assert!(wal_recovery_in_progress(&error));
    assert_eq!(waited, Duration::from_millis(500));
    // 100 waits of 5 ms, then the last open's error is returned; the reader
    // marks it as the typed, retryable `CurrentReadContention` as before.
    assert_eq!(opens, 101);
}
