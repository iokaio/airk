// SPDX-License-Identifier: Apache-2.0
//! Existing-only SQLite rollback under actual native exclusion, with private preview.
use super::*;
use std::io::Seek;

struct CapturedFile {
    file: fs::File,
    path: PathBuf,
    identity: (u64, u64),
    bytes: Vec<u8>,
}
impl CapturedFile {
    fn open(path: PathBuf, limit: u64) -> Result<Self> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000).share_mode(7);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(&path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= limit,
            "invalid stopped registry recovery input"
        );
        crate::managed_collection::reject_file_aliases(&path, &metadata)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected stopped registry input"
            );
        }
        let identity = file_identity(&file)?;
        let mut bytes = Vec::new();
        (&file).take(limit + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= limit,
            "stopped registry input grew beyond bound"
        );
        Ok(Self {
            file,
            path,
            identity,
            bytes,
        })
    }
    fn unchanged(&mut self) -> Result<()> {
        let mut current = Self::open(self.path.clone(), self.bytes.len() as u64)?;
        ensure!(
            current.identity == self.identity && current.bytes == self.bytes,
            "stopped registry recovery input changed"
        );
        current.file.rewind()?;
        self.file.rewind()?;
        let mut held_bytes = Vec::new();
        (&self.file)
            .take(self.bytes.len() as u64 + 1)
            .read_to_end(&mut held_bytes)?;
        ensure!(
            held_bytes == self.bytes,
            "held stopped registry recovery bytes changed"
        );
        Ok(())
    }
}
#[cfg(unix)]
fn file_identity(file: &fs::File) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}
#[cfg(windows)]
fn file_identity(file: &fs::File) -> Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: file owns its live handle; info is a correctly sized output slot.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) } != 0,
        "stopped registry identity unavailable"
    );
    // SAFETY: GetFileInformationByHandle succeeded and initialized the output.
    let info = unsafe { info.assume_init() };
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

impl ServiceRegistry {
    /// The caller retains process/image exclusion and this exact native lease.
    /// SQLite rollback is previewed on private copies before touching the original.
    /// No write handle, initialization, migration or lifecycle authority escapes.
    pub fn recover_existing_after_stop(
        lease: &super::super::ServiceLease,
        expected_namespace: Incarnation,
    ) -> Result<()> {
        let root = lease.root();
        ensure!(
            retained_namespace(root)? == expected_namespace,
            "stopped registry recovery namespace changed"
        );
        let journal_path = root.join("service-1.4.sqlite3-journal");
        match fs::symlink_metadata(&journal_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                drop(Self::open_saved(root)?);
                return Ok(());
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let mut database =
            CapturedFile::open(root.join("service-1.4.sqlite3"), MAX_DATABASE_BYTES)?;
        let mut journal = CapturedFile::open(journal_path, MAX_DATABASE_BYTES * 2)?;
        // The existing private native root supplies this temporary child's ACL.
        // It is diagnostic scratch, never a registry owner or persisted contract.
        let preview = tempfile::Builder::new()
            .prefix(".registry-rollback-preview-")
            .tempdir_in(root)?;
        let preview_database = preview.path().join("service-1.4.sqlite3");
        fs::write(&preview_database, &database.bytes)?;
        fs::write(
            preview.path().join("service-1.4.sqlite3-journal"),
            &journal.bytes,
        )?;
        let connection = Connection::open_with_flags(
            &preview_database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        drop(Self::validate_existing_connection(
            root.to_path_buf(),
            connection,
            expected_namespace,
            Some(ExistingWrite::StoppedRecovery(expected_namespace)),
            None,
        )?);
        database.unchanged()?;
        journal.unchanged()?;
        ensure!(
            retained_namespace(root)? == expected_namespace,
            "stopped registry owner changed during preview"
        );
        // The native lease excludes all compliant registry writers throughout.
        // SQLite alone rolls back the same exact original journal/database pair.
        drop(Self::open_existing(
            root,
            None,
            Some(ExistingWrite::StoppedRecovery(expected_namespace)),
        )?);
        Ok(())
    }
}
