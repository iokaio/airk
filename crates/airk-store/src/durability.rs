// SPDX-License-Identifier: Apache-2.0
//! Durable replacement of bounded service metadata; callers serialize writers.
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn persist_synced(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    #[cfg(windows)]
    prepare_temporary(temporary.as_file()).context("prepare metadata temporary file")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync metadata temporary file")?;
    replace(temporary, path).context("replace metadata directory entry")?;
    crate::maintenance::sync_parent(path.parent().context("metadata path has no parent")?)
        .context("sync metadata parent directory")
}

#[cfg(not(windows))]
fn replace(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    temporary.persist(path)?;
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn metadata_replacement_preserves_an_open_reader_and_publishes_new_bytes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("owner.json");
        std::fs::write(&path, b"original committed receipt")?;
        let mut retained = std::fs::File::open(&path)?;
        let mut replacement = tempfile::NamedTempFile::new_in(directory.path())?;
        replacement.write_all(b"next committed receipt")?;
        persist_synced(replacement, &path)?;
        assert_eq!(std::fs::read(&path)?, b"next committed receipt");
        let mut original = Vec::new();
        retained.read_to_end(&mut original)?;
        assert_eq!(original, b"original committed receipt");
        Ok(())
    }

    #[test]
    fn persistent_metadata_clears_the_temporary_hint_and_preserves_readonly_failures() -> Result<()>
    {
        use std::os::windows::fs::MetadataExt;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("owner.json");
        let mut initial = tempfile::NamedTempFile::new_in(directory.path())?;
        initial.write_all(b"committed receipt")?;
        persist_synced(initial, &path)?;
        assert_eq!(std::fs::metadata(&path)?.file_attributes() & 0x100, 0);
        let original_permissions = std::fs::metadata(&path)?.permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions)?;
        let mut replacement = tempfile::NamedTempFile::new_in(directory.path())?;
        replacement.write_all(b"refused replacement")?;
        let result = persist_synced(replacement, &path);
        let bytes = std::fs::read(&path)?;
        std::fs::set_permissions(&path, original_permissions)?;
        assert!(result.is_err());
        assert_eq!(bytes, b"committed receipt");
        assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }
}

#[cfg(windows)]
fn prepare_temporary(file: &std::fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FileBasicInfo, SetFileInformationByHandle,
    };
    // Clear the hint on this exact owned handle before flushing persistent
    // metadata. Zero timestamp fields leave existing times unchanged.
    let information = FILE_BASIC_INFO {
        FileAttributes: FILE_ATTRIBUTE_NORMAL,
        ..Default::default()
    };
    // SAFETY: the owned handle and correctly sized immutable input remain live.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileBasicInfo,
            (&information as *const FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(windows)]
fn replace(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    let (file, mut temporary) = temporary.into_parts();
    // Rust uses FileRenameInfoEx/POSIX replacement when an existing metadata
    // reader prevents MoveFileExW from replacing the name. Readers keep their
    // old file; subsequent opens see the replacement. Do not retry arbitrary I/O
    // failures or use ReplaceFileW's multi-step, partial-failure semantics.
    std::fs::rename(&temporary, path)?;
    // The old random name is no longer ours, including if the flush fails.
    temporary.disable_cleanup(true);
    // Keep the exact renamed file open and flush metadata after the atomic
    // rename as well as data before it. A flush failure is an uncertain outcome,
    // never successful persistence. Native power-loss qualification is separate.
    file.sync_all().context("sync renamed metadata file")?;
    Ok(())
}
