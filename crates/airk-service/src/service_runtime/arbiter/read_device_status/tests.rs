// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest};
#[test]
fn strict_diagnostic_configuration_reads_never_initialize_migrate_or_repair_bytes() -> Result<()> {
    let root = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
    let store = airk_store::service::arbiter::ConfigurationStore::open(
        lease,
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    )?;
    let binding = DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([3; 32]),
        source: Sha256Digest::from_bytes([4; 32]),
    };
    assert!(strict_current_configuration(&store.device(binding.clone()))?.is_none());
    assert!(!root.path().join("arbiter-v3").exists());
    store.initialize_checked(None, || Ok(()))?;
    let path = root.path().join("arbiter-v3/configuration.json");
    let old = std::fs::read(&path)?;
    let files = || -> Result<std::collections::BTreeMap<std::ffi::OsString, Vec<u8>>> {
        std::fs::read_dir(path.parent().unwrap())?
            .map(|row| {
                let row = row?;
                ensure!(row.file_type()?.is_file(), "unexpected fixture directory");
                Ok((row.file_name(), std::fs::read(row.path())?))
            })
            .collect()
    };
    // Explicit management initialization owns both files before any evidence read.
    let initialized = files()?;
    assert_eq!(
        initialized,
        std::collections::BTreeMap::from([
            (std::ffi::OsString::from("configuration.json"), old.clone()),
            (std::ffi::OsString::from("configuration.lock"), Vec::new()),
        ])
    );
    assert!(strict_current_configuration(&store.device(binding.clone()))?.is_none());
    assert_eq!(std::fs::read(&path)?, old);
    assert_eq!(files()?, initialized);
    std::fs::write(&path, b"original corrupt current document")?;
    let corrupt = files()?;
    assert!(strict_current_configuration(&store.device(binding)).is_err());
    assert_eq!(std::fs::read(&path)?, b"original corrupt current document");
    assert_eq!(files()?, corrupt);
    Ok(())
}
#[test]
fn diagnostic_clock_refuses_expiry_wrong_clock_and_oversized_parent() -> Result<()> {
    let clock = if cfg!(windows) {
        ReadClock::WindowsPerformanceCounter
    } else {
        ReadClock::UnixMonotonic
    };
    let mut attempt = ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock,
        expires_monotonic_ns: 0,
        remaining_ns: 1,
        native_retry_expires_monotonic_ns: None,
    };
    assert!(live(attempt).is_err());
    attempt.expires_monotonic_ns = u64::MAX;
    // Scoped device status serves Answer, whose read attempt may run to its
    // own 605 s ceiling (L2); one nanosecond more is refused. Every other
    // tool's attempt is still refused above 125 s.
    attempt.remaining_ns = airk_core::read_attempt::MAX_ANSWER_READ_BUDGET.as_nanos() as u64;
    assert!(live(attempt).is_ok());
    attempt.remaining_ns += 1;
    assert!(live(attempt).is_err());
    attempt.remaining_ns = 126_000_000_000;
    assert!(live(attempt).is_ok());
    assert!(
        attempt
            .remaining_for_tool("search", clock, airk_process::monotonic::now_ns()?)
            .is_err()
    );
    assert!(
        attempt
            .remaining_for_tool("airk_answer", clock, airk_process::monotonic::now_ns()?)
            .is_ok()
    );
    attempt.remaining_ns = 1;
    attempt.clock = if cfg!(windows) {
        ReadClock::UnixMonotonic
    } else {
        ReadClock::WindowsPerformanceCounter
    };
    assert!(live(attempt).is_err());
    Ok(())
}

#[test]
fn strict_diagnostic_current4_requires_a_real_file_and_exact_binding_without_recreating_missing_file()
-> Result<()> {
    let root = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
    let store = airk_store::service::arbiter::ConfigurationStore::open(
        lease,
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    )?;
    let binding = DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([3; 32]),
        source: Sha256Digest::from_bytes([4; 32]),
    };
    let document = store
        .device(binding.clone())
        .initialize_checked(None, || Ok(()))?;
    let path = root.path().join("arbiter-v3/configuration.json");
    let bytes = std::fs::read(&path)?;
    assert_eq!(
        strict_current_configuration(&store.device(binding.clone()))?,
        Some(document)
    );
    assert_eq!(std::fs::read(&path)?, bytes);
    let wrong = DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([9; 32]),
        ..binding.clone()
    };
    assert!(strict_current_configuration(&store.device(wrong)).is_err());
    assert_eq!(std::fs::read(&path)?, bytes);
    // Remove only this test's just-created current file. Its native directory and
    // lock remain, exercising the separate missing-file management fallback.
    std::fs::remove_file(&path)?;
    let names = || -> Result<Vec<_>> {
        let mut out = std::fs::read_dir(path.parent().unwrap())?
            .map(|row| row.map(|v| v.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        out.sort();
        Ok(out)
    };
    let before = names()?;
    assert!(strict_current_configuration(&store.device(binding))?.is_none());
    assert!(!path.exists());
    assert_eq!(names()?, before);
    Ok(())
}
#[test]
fn same_revision_selection_generation_epoch_ready_intent_and_reason_changes_refuse_partial_snapshot()
-> Result<()> {
    use std::cell::Cell;
    // Independent injected native stamp components; the persisted revision stays5.
    let stamp = Cell::new((7u64, 11u64, 11u64, false, true, 13u64, 17u64));
    let revision = Cell::new(5u64);
    let original = stamp.get();
    assert_eq!(
        with_selection_snapshot(|| Ok(stamp.get()), || Ok("complete snapshot"))?,
        "complete snapshot"
    );
    for changed in [
        (8, 11, 11, false, true, 13, 17),
        (7, 12, 11, false, true, 13, 17),
        (7, 11, 12, false, true, 13, 17),
        (7, 11, 11, true, true, 13, 17),
        (7, 11, 11, false, false, 13, 17),
        (7, 11, 11, false, true, 14, 17),
        (7, 11, 11, false, true, 13, 18),
    ] {
        stamp.set(original);
        let observed = Cell::new(false);
        let output = with_selection_snapshot(
            || Ok(stamp.get()),
            || {
                observed.set(true);
                stamp.set(changed);
                Ok("must not deliver partial snapshot")
            },
        );
        assert!(output.is_err());
        assert!(observed.get());
        assert_eq!(revision.get(), 5);
    }
    Ok(())
}
