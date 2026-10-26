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
#[derive(Debug)]
pub struct AirkBackend {
    state: Mutex<State>,
    ceiling: u64,
    denied: Arc<AtomicU64>,
}
impl AirkBackend {
    pub fn new(file: File, ceiling: u64) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > ceiling || ceiling < 64 << 10 {
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
            ceiling,
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
        if end > self.ceiling {
            self.denied.store(end, Ordering::Relaxed);
            Err(io::Error::other(CapacityExceeded {
                requested: end,
                admitted: self.ceiling,
            }))
        } else {
            Ok(())
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
