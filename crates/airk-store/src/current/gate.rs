// SPDX-License-Identifier: Apache-2.0
//! Collection request exclusion lives outside every resettable derived directory.
use crate::{FileStore, WriterLock};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
    sync::Arc,
};

pub(crate) struct Gate {
    file: File,
    _lifecycle: Option<Arc<File>>,
}

/// Retryable only when the current-index gate returned `WouldBlock`.
/// I/O failures at the same gate deliberately do not carry this marker.
#[derive(Clone, Copy, Debug)]
pub enum CurrentGateContention {
    Applying,
    Readers,
}

impl std::fmt::Display for CurrentGateContention {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Applying => "current index is busy applying an update",
            Self::Readers => "current index has active readers; retry after they finish",
        })
    }
}

fn acquired(
    result: std::result::Result<(), std::fs::TryLockError>,
    contention: CurrentGateContention,
) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error @ std::fs::TryLockError::WouldBlock) => {
            Err(anyhow::Error::new(error).context(contention))
        }
        Err(error) => Err(error).context(contention.to_string()),
    }
}

fn open(store: &FileStore) -> Result<File> {
    crate::managed_collection::validate_ancestors(store.directory())?;
    let directory = store.directory().canonicalize()?;
    let managed = crate::managed_collection::ManagedPath::parse(&directory)?;
    let parent = if managed.slot.is_some() {
        managed.logical
    } else {
        managed.logical.parent().context("gate parent missing")?
    };
    let name = managed.slot.unwrap_or(managed.collection).to_owned();
    let locks = parent.join(".current-gates");
    fs::create_dir_all(&locks)?;
    for path in [parent.to_path_buf(), directory, locks.clone()] {
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink(),
            "linked current gate directory"
        );
    }
    let path = locks.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "invalid current gate file"
        );
    }
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}

pub(crate) fn check_writer(store: &FileStore, writer: &WriterLock) -> Result<PathBuf> {
    let directory = store.directory().canonicalize()?;
    ensure!(
        writer.path == directory.join("writer.lock"),
        "foreign current-store writer guard"
    );
    Ok(directory)
}

impl Gate {
    /// Convert this writer's object-protection lease into reclamation exclusion.
    /// If a reader is present, restore shared protection and defer cleanup.
    pub(crate) fn try_exclusive(&self) -> Result<bool> {
        self.file.unlock()?;
        match self.file.try_lock() {
            Ok(()) => Ok(true),
            Err(std::fs::TryLockError::WouldBlock) => {
                self.file.try_lock_shared()?;
                Ok(false)
            }
            Err(error) => {
                self.file.try_lock_shared()?;
                Err(error.into())
            }
        }
    }
    pub(crate) fn shared(store: &FileStore) -> Result<Self> {
        let lifecycle = store.lifecycle(false)?;
        let file = open(store)?;
        acquired(file.try_lock_shared(), CurrentGateContention::Applying)?;
        Ok(Self {
            file,
            _lifecycle: lifecycle,
        })
    }

    pub(crate) fn exclusive(store: &FileStore, writer: &WriterLock) -> Result<Self> {
        check_writer(store, writer)?;
        let file = open(store)?;
        acquired(file.try_lock(), CurrentGateContention::Readers)?;
        Ok(Self {
            file,
            _lifecycle: None,
        })
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        // Explicit unlock also releases this operation's Unix flock when a
        // concurrent fork briefly inherited the same open file description.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gate_would_block_has_retryable_identity() {
        for gate in [
            CurrentGateContention::Applying,
            CurrentGateContention::Readers,
        ] {
            let error = acquired(Err(std::fs::TryLockError::WouldBlock), gate)
                .unwrap_err()
                .context("outer operation");
            assert!(error.downcast_ref::<CurrentGateContention>().is_some());
            assert!(matches!(
                error.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            ));
            for kind in [
                std::io::ErrorKind::PermissionDenied,
                std::io::ErrorKind::NotFound,
                std::io::ErrorKind::WouldBlock,
            ] {
                let error = acquired(
                    Err(std::fs::TryLockError::Error(std::io::Error::from(kind))),
                    gate,
                )
                .unwrap_err();
                assert!(
                    error.downcast_ref::<CurrentGateContention>().is_none(),
                    "{error:#}"
                );
            }
        }
    }
}
