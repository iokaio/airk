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

fn open(store: &FileStore) -> Result<File> {
    let directory = store.directory().canonicalize()?;
    let parent = directory
        .parent()
        .context("current collection parent missing")?;
    ensure!(
        parent
            .file_name()
            .is_some_and(|name| name == "collections-1.2"),
        "current store requires a managed collection"
    );
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("current collection name missing")?
        .to_owned();
    crate::validate_collection(&name)?;
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
    pub(crate) fn shared(store: &FileStore) -> Result<Self> {
        let lifecycle = store.lifecycle(false)?;
        let file = open(store)?;
        file.try_lock_shared()
            .context("current index is busy applying an update")?;
        Ok(Self {
            file,
            _lifecycle: lifecycle,
        })
    }

    pub(crate) fn exclusive(store: &FileStore, writer: &WriterLock) -> Result<Self> {
        check_writer(store, writer)?;
        let file = open(store)?;
        file.try_lock()
            .context("current index has active readers; retry after they finish")?;
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
