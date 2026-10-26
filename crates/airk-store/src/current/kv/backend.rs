// SPDX-License-Identifier: Apache-2.0
use redb::{BackendError, StorageBackend};
use std::{
    fs::{File, TryLockError},
    io::{self, Read, Seek, SeekFrom, Write},
    ops::Bound,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

/// A typed admission failure, distinct from device exhaustion or corruption.
#[derive(Debug)]
pub struct CapacityExceeded {
    pub requested: u64,
    pub admitted: u64,
}
#[derive(Debug)]
pub(crate) struct ReservationGrowthDenied(pub crate::quota::QuotaPressure);
impl std::fmt::Display for ReservationGrowthDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ReservationGrowthDenied {}
impl std::fmt::Display for CapacityExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "redb growth to {} exceeds admitted capacity {}",
            self.requested, self.admitted
        )
    }
}
impl std::error::Error for CapacityExceeded {}

#[derive(Debug)]
struct State {
    file: File,
    locked: bool,
    closed: bool,
}

/// Owns one validated file handle and an immutable ceiling captured from its grant.
/// The caller validates ownership/aliases and reserves capacity before constructing it.
pub type GrowthAdmission = dyn Fn(u64, u64) -> io::Result<u64> + Send + Sync;

pub struct AirkBackend {
    state: Mutex<State>,
    ceiling: AtomicU64,
    maximum: u64,
    grow: Option<Arc<GrowthAdmission>>,
    admission: Mutex<()>,
    denied: Arc<AtomicU64>,
}
impl std::fmt::Debug for AirkBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirkBackend")
            .field("ceiling", &self.ceiling.load(Ordering::Relaxed))
            .field("maximum", &self.maximum)
            .finish_non_exhaustive()
    }
}
impl AirkBackend {
    pub fn new(file: File, ceiling: u64) -> io::Result<Self> {
        Self::new_growing(file, ceiling, ceiling, None)
    }
    pub fn new_growing(
        file: File,
        initial: u64,
        maximum: u64,
        grow: Option<Arc<GrowthAdmission>>,
    ) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.len() > initial
            || initial < 64 << 10
            || maximum < initial
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid admitted redb file or capacity",
            ));
        }
        Ok(Self {
            state: Mutex::new(State {
                file,
                locked: false,
                closed: false,
            }),
            ceiling: AtomicU64::new(initial),
            maximum,
            grow,
            admission: Mutex::new(()),
            denied: Arc::new(AtomicU64::new(0)),
        })
    }
    fn state(&self) -> io::Result<MutexGuard<'_, State>> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("redb backend mutex poisoned"))?;
        if state.closed {
            return Err(io::Error::other("redb backend closed"));
        }
        Ok(state)
    }
    fn admit(&self, end: u64) -> io::Result<()> {
        let _admission = self
            .admission
            .lock()
            .map_err(|_| io::Error::other("redb admission mutex poisoned"))?;
        let current = self.ceiling.load(Ordering::Relaxed);
        if end <= current {
            return Ok(());
        }
        self.denied.store(end, Ordering::Relaxed);
        if end > self.maximum {
            return Err(io::Error::other(CapacityExceeded {
                requested: end,
                admitted: self.maximum,
            }));
        }
        if let Some(grow) = &self.grow {
            let admitted = match grow(end, current) {
                Ok(admitted) if admitted >= end && admitted <= self.maximum => admitted,
                Ok(admitted) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("growth callback admitted invalid high-water {admitted}"),
                    ));
                }
                Err(error) => {
                    let quota_pressure = error.get_ref().is_some_and(|source| {
                        source.downcast_ref::<ReservationGrowthDenied>().is_some()
                    });
                    if quota_pressure {
                        self.denied
                            .store((end - current) | (1u64 << 63), Ordering::Relaxed);
                    }
                    return Err(error);
                }
            };
            self.ceiling.store(admitted, Ordering::Release);
            self.denied.store(0, Ordering::Relaxed);
            Ok(())
        } else {
            Err(io::Error::other(CapacityExceeded {
                requested: end,
                admitted: current,
            }))
        }
    }
    /// Preserve admission evidence even if redb normalizes the inner I/O error.
    pub(crate) fn denial_witness(&self) -> Arc<AtomicU64> {
        self.denied.clone()
    }
    fn full_range(start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
        if matches!(start, Bound::Unbounded | Bound::Included(0)) && end == Bound::Unbounded {
            Ok(())
        } else {
            Err(BackendError::Unsupported)
        }
    }
    fn try_lock(&self, shared: bool) -> Result<bool, BackendError> {
        let mut state = self.state()?;
        if state.locked {
            return Err(io::Error::other("redb backend already locked").into());
        }
        let result = if shared {
            state.file.try_lock_shared()
        } else {
            state.file.try_lock()
        };
        match result {
            Ok(()) => {
                state.locked = true;
                Ok(true)
            }
            Err(TryLockError::WouldBlock) => Ok(false),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }
}
impl StorageBackend for AirkBackend {
    fn len(&self) -> io::Result<u64> {
        Ok(self.state()?.file.metadata()?.len())
    }
    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let end = offset.checked_add(out.len() as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "redb read offset overflow")
        })?;
        let mut state = self.state()?;
        if end > state.file.metadata()?.len() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        state.file.seek(SeekFrom::Start(offset))?;
        state.file.read_exact(out)
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        self.admit(len)?;
        self.state()?.file.set_len(len)
    }
    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let end = offset.checked_add(data.len() as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "redb write offset overflow")
        })?;
        self.admit(end)?;
        let mut state = self.state()?;
        state.file.seek(SeekFrom::Start(offset))?;
        state.file.write_all(data)
    }
    fn sync_data(&self) -> io::Result<()> {
        self.state()?.file.sync_all()
    }
    fn try_lock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<bool, BackendError> {
        Self::full_range(start, end)?;
        self.try_lock(false)
    }
    fn try_lock_shared_range(
        &self,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<bool, BackendError> {
        Self::full_range(start, end)?;
        self.try_lock(true)
    }
    fn unlock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
        Self::full_range(start, end)?;
        let mut state = self.state()?;
        if state.locked {
            state.file.unlock()?;
            state.locked = false;
        }
        Ok(())
    }
    fn close(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("redb backend mutex poisoned"))?;
        if !state.closed {
            if state.locked {
                state.file.unlock()?;
                state.locked = false;
            }
            state.closed = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growth_and_offset_overflow_fail_without_modifying_file() -> io::Result<()> {
        let file = tempfile::tempfile()?;
        let witness = file.try_clone()?;
        let backend = AirkBackend::new(file, 65536)?;
        backend.set_len(65536)?;
        backend.write(65535, &[7])?;
        assert!(
            backend
                .set_len(65537)
                .unwrap_err()
                .get_ref()
                .unwrap()
                .is::<CapacityExceeded>()
        );
        assert!(backend.write(65536, &[1]).is_err());
        assert!(backend.write(u64::MAX, &[1]).is_err());
        assert_eq!(witness.metadata()?.len(), 65536);
        let mut byte = [0];
        backend.read(65535, &mut byte)?;
        assert_eq!(byte, [7]);
        Ok(())
    }
    #[test]
    fn external_capacity_failure_keeps_its_io_classification() -> io::Result<()> {
        let file = tempfile::tempfile()?;
        let backend = AirkBackend::new_growing(
            file,
            65536,
            1 << 20,
            Some(Arc::new(|_, _| {
                Err(io::Error::from(io::ErrorKind::OutOfMemory))
            })),
        )?;
        let error = backend.set_len(131072).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
        assert_eq!(backend.denial_witness().load(Ordering::Relaxed), 131072);
        Ok(())
    }
    #[test]
    fn admitted_growth_high_water_avoids_recharging_the_same_extent() -> io::Result<()> {
        let file = tempfile::tempfile()?;
        let calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let callback_calls = calls.clone();
        let backend = AirkBackend::new_growing(
            file,
            65536,
            1 << 20,
            Some(Arc::new(move |end, current| {
                callback_calls.fetch_add(1, Ordering::Relaxed);
                Ok((current + (1 << 16)).max(end))
            })),
        )?;
        backend.set_len(65537)?;
        backend.set_len(65538)?;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        Ok(())
    }
    #[test]
    fn independently_opened_handles_conflict_and_close_releases() -> io::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let first = AirkBackend::new(file.reopen()?, 65536)?;
        let second = AirkBackend::new(file.reopen()?, 65536)?;
        assert!(
            first
                .try_lock_range(Bound::Unbounded, Bound::Unbounded)
                .unwrap()
        );
        assert!(
            !second
                .try_lock_range(Bound::Unbounded, Bound::Unbounded)
                .unwrap()
        );
        first.close()?;
        assert!(
            second
                .try_lock_range(Bound::Included(0), Bound::Unbounded)
                .unwrap()
        );
        second.close()?;
        assert!(first.len().is_err());
        Ok(())
    }
}
