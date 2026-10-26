// SPDX-License-Identifier: Apache-2.0
//! Point-in-time OS free-space observations, separate from logical quota ownership.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Sample a managed database's logical length without reading its contents.
/// Missing files are distinct from unsafe aliases and failed observations.
pub fn logical_database_bytes(path: &Path) -> Result<Option<u64>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            crate::managed_collection::reject_file_aliases(path, &metadata)?;
            Ok(Some(metadata.len()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[test]
fn logical_database_sample_distinguishes_missing_and_refuses_aliases() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("usage.airk");
    assert_eq!(logical_database_bytes(&path)?, None);
    std::fs::write(&path, b"sample")?;
    assert_eq!(logical_database_bytes(&path)?, Some(6));
    std::fs::hard_link(&path, root.path().join("alias"))?;
    assert!(logical_database_bytes(&path).is_err());
    assert!(logical_database_bytes(root.path()).is_err());
    Ok(())
}

/// Leave room for filesystem metadata and recovery. This is not an OS reservation.
pub const SAFETY_HEADROOM_BYTES: u64 = 16 * 1024 * 1024;
#[cfg(test)]
static TEST_HEADROOM: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<PathBuf, u64>>,
> = std::sync::OnceLock::new();
#[cfg(test)]
pub(crate) struct TestHeadroom(PathBuf);
#[cfg(test)]
impl Drop for TestHeadroom {
    fn drop(&mut self) {
        if let Some(values) = TEST_HEADROOM.get() {
            values.lock().unwrap().remove(&self.0);
        }
    }
}
#[cfg(test)]
pub(crate) fn test_growth_headroom(path: &Path, bytes: u64) -> Result<TestHeadroom> {
    let path = path.canonicalize()?;
    TEST_HEADROOM
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(path.clone(), bytes);
    Ok(TestHeadroom(path))
}

#[derive(Clone, Debug)]
pub struct StoragePressure {
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub safety_headroom_bytes: u64,
}
impl std::fmt::Display for StoragePressure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "insufficient physical storage headroom: requested {} bytes, {} available after {} safety bytes; prior generation remains authoritative",
            self.required_bytes, self.available_bytes, self.safety_headroom_bytes
        )
    }
}
impl std::error::Error for StoragePressure {}

/// Keep provider-specific pressure recognition in the storage adapter.
/// No string matching: unrelated permission/corruption failures are not retried.
pub fn is_storage_pressure(error: &anyhow::Error) -> bool {
    if error
        .downcast_ref::<crate::quota::QuotaPressure>()
        .is_some()
        || error.downcast_ref::<StoragePressure>().is_some()
    {
        return true;
    }
    error.chain().any(|cause| {
        cause.is::<crate::quota::QuotaPressure>()
            || cause.is::<StoragePressure>()
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::StorageFull)
    })
}

/// Stable path-free job diagnostics. Preserve the typed cause rather than
/// conflating a reservation ceiling with physical disk exhaustion.
pub fn storage_pressure_code(error: &anyhow::Error) -> Option<&'static str> {
    // anyhow context retains typed admission witnesses even when redb's I/O
    // boundary rendered its original error to text. Error::chain alone exposes
    // the context wrapper rather than that witness's concrete type.
    if let Some(quota) = error.downcast_ref::<crate::quota::QuotaPressure>() {
        return Some(match quota.scope {
            crate::quota::PressureScope::Root => "storage_quota_root",
            crate::quota::PressureScope::Collection => "storage_quota_collection",
            crate::quota::PressureScope::Reservation => "storage_quota_reservation",
        });
    }
    if error.downcast_ref::<StoragePressure>().is_some() {
        return Some("physical_storage_headroom");
    }
    for cause in error.chain() {
        if let Some(quota) = cause.downcast_ref::<crate::quota::QuotaPressure>() {
            return Some(match quota.scope {
                crate::quota::PressureScope::Root => "storage_quota_root",
                crate::quota::PressureScope::Collection => "storage_quota_collection",
                crate::quota::PressureScope::Reservation => "storage_quota_reservation",
            });
        }
        if cause.is::<StoragePressure>() {
            return Some("physical_storage_headroom");
        }
        if cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::StorageFull)
        {
            return Some("filesystem_storage_full");
        }
    }
    None
}

#[derive(Clone, Debug, Serialize)]
pub struct Capacity {
    pub observed_path: PathBuf,
    pub total_to_caller_bytes: u64,
    pub free_bytes: u64,
    pub available_to_caller_bytes: u64,
    pub safety_headroom_bytes: u64,
    pub available_growth_bytes: u64,
    pub guarantee: &'static str,
}

impl Capacity {
    pub fn require_growth(&self, bytes: u64) -> Result<()> {
        ensure!(
            bytes <= self.available_growth_bytes,
            StoragePressure {
                required_bytes: bytes,
                available_bytes: self.available_growth_bytes,
                safety_headroom_bytes: self.safety_headroom_bytes
            }
        );
        Ok(())
    }
}

/// Resolve the nearest existing directory; read-only planning creates no paths.
pub fn probe(path: &Path) -> Result<Capacity> {
    let mut existing = std::path::absolute(path)?;
    while !existing.try_exists()? {
        ensure!(
            existing.pop(),
            "physical capacity path has no existing ancestor"
        );
    }
    if existing.is_file() {
        ensure!(existing.pop(), "capacity file has no parent");
    }
    ensure!(existing.is_dir(), "physical capacity requires a directory");
    let existing = existing.canonicalize()?;
    let (total, free, available) = platform_probe(&existing)?;
    ensure!(
        available <= free && available <= total,
        "filesystem returned inconsistent capacity"
    );
    let available_growth_bytes = available.saturating_sub(SAFETY_HEADROOM_BYTES);
    #[cfg(test)]
    let available_growth_bytes = TEST_HEADROOM
        .get()
        .and_then(|values| values.lock().unwrap().get(&existing).copied())
        .map_or(available_growth_bytes, |bytes| {
            available_growth_bytes.min(bytes)
        });
    Ok(Capacity {
        observed_path: existing,
        total_to_caller_bytes: total,
        free_bytes: free,
        available_to_caller_bytes: available,
        safety_headroom_bytes: SAFETY_HEADROOM_BYTES,
        available_growth_bytes,
        guarantee: "point-in-time OS free space; not reserved against concurrent or external writes",
    })
}

pub(crate) fn require_growth(path: &Path, bytes: u64) -> Result<()> {
    if bytes != 0 {
        probe(path)?.require_growth(bytes)?;
    }
    Ok(())
}

/// Managed directories on another mounted filesystem cannot use root capacity.
pub(crate) fn ensure_same_filesystem(root: &Path, directory: &Path) -> Result<()> {
    ensure!(
        filesystem_key(root)? == filesystem_key(directory)?,
        "nested mounted filesystems in managed data are unsupported; select a separate data root"
    );
    Ok(())
}

#[cfg(unix)]
fn platform_probe(path: &Path) -> Result<(u64, u64, u64)> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).context("NUL in capacity path")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the NUL-terminated path and writable statvfs storage remain live.
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("query filesystem capacity");
    }
    // SAFETY: statvfs succeeded and initialized the complete structure.
    let stats = unsafe { stats.assume_init() };
    let block = u128::from(stats.f_frsize);
    ensure!(block > 0, "filesystem reported zero allocation unit");
    let bytes = |blocks: u128| -> Result<u64> {
        Ok(u64::try_from(
            blocks
                .checked_mul(block)
                .context("filesystem capacity overflow")?,
        )?)
    };
    Ok((
        bytes(u128::from(stats.f_blocks))?,
        bytes(u128::from(stats.f_bfree))?,
        bytes(u128::from(stats.f_bavail))?,
    ))
}

#[cfg(unix)]
fn filesystem_key(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.dev())
}

#[cfg(windows)]
fn wide(path: &Path) -> Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    ensure!(!value.contains(&0), "NUL in capacity path");
    value.push(0);
    Ok(value)
}

#[cfg(windows)]
fn platform_probe(path: &Path) -> Result<(u64, u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let path = wide(path)?;
    let (mut available, mut total, mut free) = (0, 0, 0);
    // SAFETY: the path is terminated; all three output pointers are valid u64s.
    if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) } == 0 {
        return Err(std::io::Error::last_os_error()).context("query filesystem capacity");
    }
    Ok((total, free, available))
}

#[cfg(windows)]
fn filesystem_key(path: &Path) -> Result<Vec<u16>> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW;
    let path = wide(path)?;
    let mut volume = vec![0u16; 32768];
    // SAFETY: both buffers remain live and the output capacity is exact.
    if unsafe { GetVolumePathNameW(path.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0 {
        return Err(std::io::Error::last_os_error()).context("identify managed filesystem");
    }
    volume.truncate(
        volume
            .iter()
            .position(|ch| *ch == 0)
            .context("unterminated volume path")?,
    );
    Ok(String::from_utf16(&volume)?
        .to_lowercase()
        .encode_utf16()
        .collect())
}

#[cfg(not(any(unix, windows)))]
fn platform_probe(_: &Path) -> Result<(u64, u64, u64)> {
    anyhow::bail!("physical capacity is unavailable on this platform")
}
#[cfg(not(any(unix, windows)))]
fn filesystem_key(_: &Path) -> Result<u64> {
    anyhow::bail!("filesystem identity is unavailable on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_capacity_is_consistent_and_missing_directory_probe_is_read_only() -> Result<()> {
        let root = tempfile::tempdir()?;
        let missing = root.path().join("not-created/child");
        let capacity = probe(&missing)?;
        assert!(!missing.parent().unwrap().exists());
        assert!(capacity.total_to_caller_bytes > 0);
        assert!(capacity.free_bytes >= capacity.available_to_caller_bytes);
        assert_eq!(capacity.observed_path, root.path().canonicalize()?);
        assert_eq!(
            capacity.available_growth_bytes,
            capacity
                .available_to_caller_bytes
                .saturating_sub(SAFETY_HEADROOM_BYTES)
        );
        ensure_same_filesystem(root.path(), root.path())?;
        Ok(())
    }
    #[test]
    fn growth_checks_actual_write_and_headroom_without_reserving_policy_limit() {
        let capacity = Capacity {
            observed_path: PathBuf::from("fixture"),
            total_to_caller_bytes: 100,
            free_bytes: 20,
            available_to_caller_bytes: 15,
            safety_headroom_bytes: 5,
            available_growth_bytes: 10,
            guarantee: "fixture",
        };
        assert!(capacity.require_growth(10).is_ok());
        assert!(capacity.require_growth(11).is_err());
        assert!(capacity.require_growth(u64::MAX).is_err());
        assert_eq!(
            storage_pressure_code(
                &capacity
                    .require_growth(11)
                    .unwrap_err()
                    .context("outer operation")
            ),
            Some("physical_storage_headroom")
        );
        for (scope, code) in [
            (crate::quota::PressureScope::Root, "storage_quota_root"),
            (
                crate::quota::PressureScope::Collection,
                "storage_quota_collection",
            ),
            (
                crate::quota::PressureScope::Reservation,
                "storage_quota_reservation",
            ),
        ] {
            let error = anyhow::Error::new(crate::quota::QuotaPressure {
                scope,
                required_bytes: 100,
                available_bytes: 50,
                protected_bytes: None,
            })
            .context("outer operation");
            assert_eq!(storage_pressure_code(&error), Some(code));
        }
        assert_eq!(
            storage_pressure_code(
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
            ),
            None
        );
        assert!(is_storage_pressure(
            &capacity.require_growth(11).unwrap_err()
        ));
        assert!(!is_storage_pressure(&anyhow::anyhow!(
            "storage quota exhausted"
        )));
        assert!(!is_storage_pressure(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
        ));
    }
}
