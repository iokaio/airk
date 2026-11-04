// SPDX-License-Identifier: Apache-2.0
//! Windows setup-only HANDLE-relative filesystem operations. No mutable logical
//! path is used after native review; ordinary FileStore constructors are unchanged.
use super::{SetupDirectory, SetupFileMode};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF,
            FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT,
            FileFsFullSizeInformation, FileRenameInformation, NtCreateFile, NtFlushBuffersFileEx,
            NtQueryVolumeInformationFile, NtSetInformationFile,
        },
        System::SystemServices::FILE_FS_FULL_SIZE_INFORMATION,
    },
    Win32::{
        Foundation::{
            ERROR_NO_MORE_FILES, HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError,
            UNICODE_STRING,
        },
        Storage::FileSystem::*,
        System::IO::IO_STATUS_BLOCK,
    },
};

fn nt_result(status: i32, operation: &'static str) -> Result<()> {
    if status < 0 {
        // SAFETY: status is the completed native system-service return value.
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ))
        .with_context(|| format!("{operation}: NTSTATUS 0x{:08x}", status as u32));
    }
    ensure!(
        status == 0,
        "{operation}: native setup operation did not complete synchronously (NTSTATUS 0x{:08x})",
        status as u32
    );
    Ok(())
}
pub(super) fn native_file_identity(file: &File) -> Result<(u64, u64, u32)> {
    let mut value = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: owned live file handle and properly sized initialized output.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), value.as_mut_ptr()) } != 0,
        std::io::Error::last_os_error()
    );
    let value = unsafe { value.assume_init() };
    Ok((
        u64::from(value.dwVolumeSerialNumber),
        (u64::from(value.nFileIndexHigh) << 32) | u64::from(value.nFileIndexLow),
        value.nNumberOfLinks,
    ))
}
fn plain(file: &File) -> Result<()> {
    ensure!(
        file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "redirected native setup entry"
    );
    Ok(())
}
fn child_name(name: &OsStr) -> Result<Vec<u16>> {
    let name: Vec<_> = name.encode_wide().collect();
    ensure!(
        !name.is_empty()
            && name.len() <= 255
            && !name.iter().any(|unit| [0, 47, 92, 58].contains(unit))
            && name != [46]
            && name != [46, 46],
        "native setup requires one child name"
    );
    Ok(name)
}
impl SetupDirectory {
    pub(crate) fn open_canonical(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute() && path.canonicalize()? == path,
            "native setup path changed"
        );
        let file = OpenOptions::new()
            .read(true)
            .access_mode(FILE_TRAVERSE | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        Self::from_file(file)
    }
    pub(crate) fn from_file(file: File) -> Result<Self> {
        plain(&file)?;
        ensure!(
            file.metadata()?.is_dir(),
            "setup descriptor is not a directory"
        );
        Ok(Self { file })
    }
    pub(crate) fn try_clone(&self) -> Result<Self> {
        Self::from_file(self.file.try_clone()?)
    }
    pub(crate) fn identity(&self) -> Result<(u64, u64)> {
        let (volume, id, _) = native_file_identity(&self.file)?;
        Ok((volume, id))
    }
    fn relative(&self, name: &OsStr, access: u32, disposition: u32, options: u32) -> Result<File> {
        let mut encoded = child_name(name)?;
        let length = u16::try_from(encoded.len() * 2)?;
        let name = UNICODE_STRING {
            Length: length,
            MaximumLength: length,
            Buffer: encoded.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: self.file.as_raw_handle().cast(),
            ObjectName: &name,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut handle: HANDLE = std::ptr::null_mut();
        let mut status_block = IO_STATUS_BLOCK::default();
        // SAFETY: synchronous call; owned parent, relative bounded UTF-16 name,
        // object attributes and output buffers remain live until completion.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                disposition,
                options | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                std::ptr::null(),
                0,
            )
        };
        if status < 0 {
            return nt_result(status, "NtCreateFile relative child").map(|()| unreachable!());
        }
        ensure!(!handle.is_null(), "native setup returned no handle");
        // SAFETY: successful NtCreateFile returned one uniquely owned handle.
        let file = unsafe { File::from_raw_handle(handle.cast()) };
        nt_result(status, "NtCreateFile relative child")?;
        plain(&file)?;
        Ok(file)
    }
    pub(crate) fn open_dir(&self, name: &OsStr) -> Result<Self> {
        Self::from_file(self.relative(
            name,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES,
            FILE_OPEN,
            FILE_DIRECTORY_FILE,
        )?)
    }
    pub(crate) fn create_dir(&self, name: &OsStr) -> Result<Self> {
        let child = Self::from_file(self.relative(
            name,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES,
            FILE_OPEN_IF,
            FILE_DIRECTORY_FILE,
        )?)?;
        ensure!(
            child.identity()?.0 == self.identity()?.0,
            "setup crosses a filesystem boundary"
        );
        self.sync()?;
        Ok(child)
    }
    pub(crate) fn open_entry(&self, name: &OsStr) -> Result<File> {
        self.relative(name, FILE_READ_ATTRIBUTES, FILE_OPEN, 0)
    }
    pub(crate) fn open_file(&self, name: &OsStr, mode: SetupFileMode) -> Result<File> {
        let (access, disposition) = match mode {
            SetupFileMode::Read => (FILE_READ_DATA | FILE_READ_ATTRIBUTES, FILE_OPEN),
            SetupFileMode::ReadWrite => (
                FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES,
                FILE_OPEN,
            ),
            SetupFileMode::CreateNew => (
                FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES,
                FILE_CREATE,
            ),
        };
        let file = self.relative(name, access, disposition, FILE_NON_DIRECTORY_FILE)?;
        ensure!(
            file.metadata()?.is_file() && native_file_identity(&file)?.2 == 1,
            "linked or non-file setup metadata"
        );
        Ok(file)
    }
    pub(crate) fn read_file(&self, name: &OsStr, maximum: u64) -> Result<Vec<u8>> {
        use std::io::Read;
        let file = self.open_file(name, SetupFileMode::Read)?;
        ensure!(
            file.metadata()?.len() <= maximum,
            "setup metadata exceeds bound"
        );
        let mut bytes = Vec::new();
        file.take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= maximum,
            "setup metadata grew beyond bound"
        );
        Ok(bytes)
    }
    #[cfg(test)]
    pub(crate) fn create_file(&self, name: &OsStr, bytes: &[u8]) -> Result<File> {
        use std::io::Write;
        let mut file = self.open_file(name, SetupFileMode::CreateNew)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(file)
    }
    pub(crate) fn unlink_file_if_identity(&self, name: &OsStr, owned: &File) -> Result<()> {
        let file = self.relative(
            name,
            DELETE | FILE_READ_ATTRIBUTES,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE,
        )?;
        ensure!(
            native_file_identity(&file)?.2 == 1,
            "linked setup deletion target"
        );
        let actual = native_file_identity(&file)?;
        let expected = native_file_identity(owned)?;
        ensure!(
            actual.0 == expected.0 && actual.1 == expected.1,
            "setup quota reservation identity changed"
        );
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: live exact opened file and bounded disposition structure.
        ensure!(
            unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle().cast(),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } != 0,
            std::io::Error::last_os_error()
        );
        drop(file);
        self.sync()
    }
    pub(crate) fn publish_no_replace(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
    ) -> Result<()> {
        self.publish_with_hook(name, owned, destination, target, || {})
    }
    fn publish_with_hook(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
        after_source_open: impl FnOnce(),
    ) -> Result<()> {
        let source = self.relative(name, DELETE | FILE_READ_ATTRIBUTES, FILE_OPEN, 0)?;
        let actual = native_file_identity(&source)?;
        let expected = native_file_identity(owned)?;
        ensure!(
            actual.0 == expected.0 && actual.1 == expected.1,
            "native setup publication source identity changed"
        );
        ensure!(
            source.metadata()?.is_dir() || (source.metadata()?.is_file() && actual.2 == 1),
            "linked or unsupported setup publication source"
        );
        // The rename below uses THIS checked HANDLE, never the source name
        // again. A replacement arriving after this point cannot be published.
        after_source_open();
        let target = child_name(target)?;
        let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
        let length = offset
            .checked_add(target.len() * 2 + size_of::<FILE_RENAME_INFORMATION>())
            .context("setup rename size overflow")?;
        let mut buffer = vec![0u64; length.div_ceil(size_of::<u64>())];
        // SAFETY: aligned buffer has room for header and every UTF-16 unit;
        // no-replacement and the retained destination are actual native inputs.
        unsafe {
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = destination.file.as_raw_handle().cast();
            (*info).FileNameLength = (target.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                target.as_ptr(),
                buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                target.len(),
            );
            let mut status = IO_STATUS_BLOCK::default();
            nt_result(
                NtSetInformationFile(
                    source.as_raw_handle().cast(),
                    &mut status,
                    info.cast(),
                    length as u32,
                    FileRenameInformation,
                ),
                "NtSetInformationFile no-replace publication",
            )?;
        }
        destination.sync()?;
        self.sync()
    }
    fn reopen_exact_directory(&self, access: u32, operation: &'static str) -> Result<File> {
        // The owned Windows probe established this native empty-name operation
        // on its tested filesystem. Keep it separate from child-name opens and
        // validate the actual returned object before any flush or enumeration.
        // A new open owns a fresh cursor; neither a mutable path nor "." is used.
        // Match the owned probe's empty UNICODE_STRING shape: a live NUL
        // buffer with zero logical length, never a filesystem child component.
        let mut empty_name = [0u16];
        let name = UNICODE_STRING {
            Length: 0,
            MaximumLength: 0,
            Buffer: empty_name.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: self.file.as_raw_handle().cast(),
            ObjectName: &name,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut handle: HANDLE = std::ptr::null_mut();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: actual retained directory and synchronous native open; all
        // input/output structures remain live until the operation completes.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                access | SYNCHRONIZE,
                &attributes,
                &mut status,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                std::ptr::null(),
                0,
            )
        };
        if result < 0 {
            return nt_result(result, operation).map(|()| unreachable!());
        }
        ensure!(
            !handle.is_null(),
            "{operation}: native reopen returned no handle"
        );
        // SAFETY: successful NtCreateFile returned a new uniquely owned handle.
        let file = unsafe { File::from_raw_handle(handle.cast()) };
        nt_result(result, operation)?;
        plain(&file)?;
        ensure!(
            file.metadata()?.is_dir(),
            "{operation}: reopened object is not a directory"
        );
        let (volume, id, _) = native_file_identity(&file)?;
        ensure!(
            (volume, id) == self.identity()?,
            "{operation}: actual directory identity changed"
        );
        Ok(file)
    }
    pub(crate) fn sync(&self) -> Result<()> {
        // Only actual mutation directories need ADD_FILE for a strict native
        // metadata flush. Read-only ancestry pins retain minimal traversal.
        let writable = self.reopen_exact_directory(
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | FILE_ADD_FILE,
            "NtCreateFile empty-name setup directory flush",
        )?;
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: synchronous flags=0 flush on the same validated directory.
        nt_result(
            unsafe {
                NtFlushBuffersFileEx(
                    writable.as_raw_handle().cast(),
                    0,
                    std::ptr::null(),
                    0,
                    &mut status,
                )
            },
            "NtFlushBuffersFileEx setup directory metadata",
        )
    }
    pub(crate) fn entries(&self) -> Result<SetupEntries> {
        let directory = self.reopen_exact_directory(
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
            "NtCreateFile empty-name setup directory inventory",
        )?;
        Ok(SetupEntries {
            directory,
            buffer: vec![0u64; 8192],
            offset: 0,
            ready: false,
            started: false,
            done: false,
        })
    }
    pub(crate) fn require_growth(&self, bytes: u64) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }
        self.capacity(Path::new("native-setup-directory"))?
            .require_growth(bytes)
    }
    pub(crate) fn capacity(&self, logical: &Path) -> Result<crate::capacity::Capacity> {
        let mut value = FILE_FS_FULL_SIZE_INFORMATION::default();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: actual retained root, synchronous volume query and sized output.
        nt_result(
            unsafe {
                NtQueryVolumeInformationFile(
                    self.file.as_raw_handle().cast(),
                    &mut status,
                    (&mut value as *mut FILE_FS_FULL_SIZE_INFORMATION).cast(),
                    size_of::<FILE_FS_FULL_SIZE_INFORMATION>() as u32,
                    FileFsFullSizeInformation,
                )
            },
            "NtQueryVolumeInformationFile setup capacity",
        )?;
        let block = u64::from(value.SectorsPerAllocationUnit)
            .checked_mul(u64::from(value.BytesPerSector))
            .context("setup capacity overflow")?;
        ensure!(block > 0, "invalid native setup allocation unit");
        let bytes = |units: i64| -> Result<u64> {
            u64::try_from(units)?
                .checked_mul(block)
                .context("setup capacity overflow")
        };
        let total = bytes(value.TotalAllocationUnits)?;
        let free = bytes(value.ActualAvailableAllocationUnits)?;
        let available = bytes(value.CallerAvailableAllocationUnits)?;
        ensure!(
            available <= free && free <= total,
            "filesystem returned inconsistent setup capacity"
        );
        Ok(crate::capacity::Capacity {
            observed_path: logical.into(),
            total_to_caller_bytes: total,
            free_bytes: free,
            available_to_caller_bytes: available,
            safety_headroom_bytes: crate::capacity::SAFETY_HEADROOM_BYTES,
            available_growth_bytes: available
                .saturating_sub(crate::capacity::SAFETY_HEADROOM_BYTES),
            guarantee: "point-in-time OS free space; not reserved against concurrent or external writes",
        })
    }
}
pub(crate) struct SetupEntries {
    directory: File,
    buffer: Vec<u64>,
    offset: usize,
    ready: bool,
    started: bool,
    done: bool,
}
impl Iterator for SetupEntries {
    type Item = Result<OsString>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = (|| -> Result<Option<OsString>> {
            loop {
                if !self.ready {
                    let class = if self.started {
                        FileIdBothDirectoryInfo
                    } else {
                        FileIdBothDirectoryRestartInfo
                    };
                    self.started = true;
                    // SAFETY: independent directory cursor and aligned bounded output.
                    let ok = unsafe {
                        GetFileInformationByHandleEx(
                            self.directory.as_raw_handle().cast(),
                            class,
                            self.buffer.as_mut_ptr().cast(),
                            (self.buffer.len() * size_of::<u64>()) as u32,
                        )
                    };
                    if ok == 0 {
                        let error = std::io::Error::last_os_error();
                        if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                            return Ok(None);
                        }
                        return Err(error.into());
                    }
                    self.offset = 0;
                    self.ready = true;
                }
                let capacity = self.buffer.len() * size_of::<u64>();
                let header = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                ensure!(
                    self.offset
                        .checked_add(header)
                        .is_some_and(|end| end <= capacity),
                    "invalid setup directory inventory header"
                );
                // SAFETY: header bounds checked; native records are eight-byte aligned.
                let entry = unsafe {
                    self.buffer
                        .as_ptr()
                        .cast::<u8>()
                        .add(self.offset)
                        .cast::<FILE_ID_BOTH_DIR_INFO>()
                };
                // Read only validated header fields; a short final record must
                // never form a reference spanning a full flexible-tail struct.
                let bytes = unsafe { std::ptr::addr_of!((*entry).FileNameLength).read_unaligned() }
                    as usize;
                ensure!(
                    bytes > 0
                        && bytes.is_multiple_of(2)
                        && bytes <= 510
                        && self.offset + header + bytes <= capacity,
                    "invalid setup inventory name"
                );
                let next = unsafe { std::ptr::addr_of!((*entry).NextEntryOffset).read_unaligned() }
                    as usize;
                ensure!(
                    next == 0
                        || (next.is_multiple_of(8)
                            && next >= header + bytes
                            && self.offset + next < capacity),
                    "invalid setup inventory next record"
                );
                let units = unsafe {
                    std::slice::from_raw_parts(
                        self.buffer
                            .as_ptr()
                            .cast::<u8>()
                            .add(self.offset + header)
                            .cast::<u16>(),
                        bytes / 2,
                    )
                };
                let name = OsString::from_wide(units);
                if next == 0 {
                    self.ready = false
                } else {
                    self.offset += next
                }
                if name == "." || name == ".." {
                    continue;
                }
                return Ok(Some(name));
            }
        })();
        match result {
            Ok(Some(name)) => Some(Ok(name)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

#[cfg(test)]
#[path = "service_catalog_setup_storage_windows_tests.rs"]
mod tests;
