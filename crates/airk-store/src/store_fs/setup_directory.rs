// SPDX-License-Identifier: Apache-2.0
//! Descriptor-anchored setup directory handle. Every mutation is relative to an
//! actual retained directory object; logical paths are never mutation operands.
use anyhow::Result;
#[cfg(unix)]
use anyhow::{Context, ensure};
use std::fs::File;
#[cfg(all(unix, test))]
use std::io::Write;
#[cfg(unix)]
use std::{
    ffi::{CStr, CString, OsStr, OsString},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};

#[cfg(windows)]
mod windows;
#[cfg(all(windows, test))]
pub(crate) use windows::{native_file_identity, nt_result};

pub(crate) fn file_identity(file: &File) -> Result<(u64, u64)> {
    #[cfg(unix)]
    {
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let (volume, id, _) = windows::native_file_identity(file)?;
        Ok((volume, id))
    }
}

pub(crate) enum SetupFileMode {
    Read,
    ReadWrite,
    CreateNew,
}

pub(crate) struct SetupDirectory {
    pub(crate) file: File,
}
#[cfg(unix)]
impl SetupDirectory {
    pub(crate) fn open_canonical(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute() && path.canonicalize()? == path,
            "setup directory must be canonical"
        );
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Self::from_file(file)
    }
    pub(crate) fn from_file(file: File) -> Result<Self> {
        ensure!(
            file.metadata()?.is_dir(),
            "setup descriptor is not a directory"
        );
        Ok(Self { file })
    }
    pub(crate) fn try_clone(&self) -> Result<Self> {
        Self::from_file(self.file.try_clone()?)
    }
    pub(crate) fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
    pub(crate) fn identity(&self) -> Result<(u64, u64)> {
        let metadata = self.file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    pub(crate) fn open_dir(&self, name: &OsStr) -> Result<Self> {
        let name = component(name)?;
        // SAFETY: this directory descriptor and single-component C string live
        // through openat; NOFOLLOW prevents rebinding through a linked child.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful openat returned a new owned descriptor.
        Self::from_file(unsafe { File::from_raw_fd(fd) })
    }
    pub(crate) fn create_dir(&self, name: &OsStr) -> Result<Self> {
        let encoded = component(name)?;
        // SAFETY: retained parent and terminated single-component name.
        if unsafe { libc::mkdirat(self.as_raw_fd(), encoded.as_ptr(), 0o700) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
        }
        let result = self.open_dir(name)?;
        ensure!(
            result.identity()?.0 == self.identity()?.0,
            "setup crosses a filesystem boundary"
        );
        self.sync()?;
        Ok(result)
    }
    pub(crate) fn open_entry(&self, name: &OsStr) -> Result<File> {
        self.open_raw(name, libc::O_RDONLY | libc::O_NONBLOCK)
    }
    pub(crate) fn open_file(&self, name: &OsStr, mode: SetupFileMode) -> Result<File> {
        let flags = match mode {
            SetupFileMode::Read => libc::O_RDONLY,
            SetupFileMode::ReadWrite => libc::O_RDWR,
            SetupFileMode::CreateNew => libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        };
        let file = self.open_raw(name, flags | libc::O_NONBLOCK)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.nlink() == 1,
            "linked or non-file setup metadata"
        );
        Ok(file)
    }
    fn open_raw(&self, name: &OsStr, flags: i32) -> Result<File> {
        let name = component(name)?;
        // SAFETY: owned directory, bounded terminated name, mode is supplied
        // for O_CREAT and ignored otherwise. No parent path is resolved.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful openat returned a new owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub(crate) fn read_file(&self, name: &OsStr, maximum: u64) -> Result<Vec<u8>> {
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
        let mut file = self.open_file(name, SetupFileMode::CreateNew)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(file)
    }
    pub(crate) fn unlink_file_if_identity(&self, name: &OsStr, owned: &File) -> Result<()> {
        let current = self.open_file(name, SetupFileMode::Read)?;
        ensure!(
            file_identity(&current)? == file_identity(owned)?,
            "setup quota reservation identity changed"
        );
        // Unix unlink is a name operation under the same native ledger lock;
        // retain the existing cooperating-owner guarantee without claiming an
        // atomic unlink-by-inode primitive on this platform.
        self.unlink_file(name)
    }
    pub(crate) fn unlink_file(&self, name: &OsStr) -> Result<()> {
        let name = component(name)?;
        // SAFETY: caller owns the native entry and supplies one retained parent.
        if unsafe { libc::unlinkat(self.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.sync()
    }
    /// Remove only the exact empty child under the caller's native exclusion.
    /// Unix checks then unlinks a name; it has no atomic unlink-by-inode API.
    pub(crate) fn unlink_empty_dir_if_identity(&self, name: &OsStr, owned: &Self) -> Result<()> {
        let current = self.open_dir(name)?;
        ensure!(
            current.identity()? == owned.identity()?,
            "setup empty-directory identity changed"
        );
        ensure!(
            current.entries()?.next().transpose()?.is_none(),
            "setup deletion directory is not empty"
        );
        let name = component(name)?;
        // SAFETY: retained parent, one child component and empty-directory-only
        // unlink. The caller retains native exclusion across identity and unlink.
        if unsafe { libc::unlinkat(self.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.sync()
    }
    pub(crate) fn publish_no_replace(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
    ) -> Result<()> {
        let actual = self.open_entry(name)?;
        ensure!(
            file_identity(&actual)? == file_identity(owned)?,
            "native setup publication source identity changed"
        );
        // Unix rename remains a name operation under native writer/claim
        // ownership. This observation adds no claim of atomic rename-by-inode.
        let name = component(name)?;
        let target = component(target)?;
        #[cfg(target_os = "macos")]
        // SAFETY: both retained descriptors and terminated child names live
        // through the atomic no-replacement rename.
        let result = unsafe {
            libc::renameatx_np(
                self.as_raw_fd(),
                name.as_ptr(),
                destination.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        // SAFETY: same descriptor/name invariants as above.
        let result = unsafe {
            libc::renameat2(
                self.as_raw_fd(),
                name.as_ptr(),
                destination.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let result =
            { anyhow::bail!("native setup requires an atomic no-replacement directory rename") };
        ensure!(result == 0, std::io::Error::last_os_error());
        destination.sync()?;
        self.sync()
    }
    /// Atomic name replacement under the native owner's writer/manifest lock.
    /// Both leaves are checked as exact regular objects in this held directory;
    /// Unix does not provide a compare-and-replace-by-inode primitive.
    pub(crate) fn replace_file(
        &self,
        name: &OsStr,
        owned: &File,
        target: &OsStr,
        expected: &File,
    ) -> Result<()> {
        self.replace_file_to(name, owned, self, target, expected)
    }
    /// Expected-object check under owning exclusion plus descriptor-relative
    /// rename. Unix does not supply atomic compare-and-replace-by-inode.
    pub(crate) fn replace_file_to(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
        expected: &File,
    ) -> Result<()> {
        let source = self.open_file(name, SetupFileMode::Read)?;
        let current = destination.open_file(target, SetupFileMode::Read)?;
        ensure!(
            file_identity(&source)? == file_identity(owned)?
                && file_identity(&current)? == file_identity(expected)?,
            "native metadata replacement identity changed"
        );
        let name = component(name)?;
        let target = component(target)?;
        // SAFETY: two retained directories and validated single-component names.
        ensure!(
            unsafe {
                libc::renameat(
                    self.as_raw_fd(),
                    name.as_ptr(),
                    destination.as_raw_fd(),
                    target.as_ptr(),
                )
            } == 0,
            std::io::Error::last_os_error()
        );
        destination.sync()?;
        self.sync()
    }
    pub(crate) fn sync(&self) -> Result<()> {
        self.file.sync_all().map_err(Into::into)
    }
    pub(crate) fn entries(&self) -> Result<SetupEntries> {
        // Open "." to obtain an independent directory offset. dup alone would
        // share the cursor with later bounded inventories on this same handle.
        let name = c".";
        // SAFETY: retained directory and static terminated name.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: fd is new and owned; fdopendir takes ownership on success.
        let directory = unsafe { libc::fdopendir(fd) };
        if directory.is_null() {
            let error = std::io::Error::last_os_error();
            // SAFETY: fdopendir failed and did not consume fd.
            drop(unsafe { File::from_raw_fd(fd) });
            return Err(error.into());
        }
        Ok(SetupEntries {
            directory,
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
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: retained descriptor and correctly sized writable output.
        if unsafe { libc::fstatvfs(self.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("native setup capacity inspection failed");
        }
        // SAFETY: successful fstatvfs initialized the output.
        let stat = unsafe { stat.assume_init() };
        let block = u128::from(stat.f_frsize);
        let total =
            u64::try_from(u128::from(stat.f_blocks) * block).context("setup capacity overflow")?;
        let free =
            u64::try_from(u128::from(stat.f_bfree) * block).context("setup capacity overflow")?;
        let available =
            u64::try_from(u128::from(stat.f_bavail) * block).context("setup capacity overflow")?;
        ensure!(
            available <= free && available <= total,
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

#[cfg(unix)]
pub(crate) struct SetupEntries {
    directory: *mut libc::DIR,
    done: bool,
}
#[cfg(unix)]
impl Iterator for SetupEntries {
    type Item = Result<OsString>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            #[cfg(target_os = "macos")]
            // SAFETY: errno storage belongs to the current thread.
            let errno = unsafe { libc::__error() };
            #[cfg(not(target_os = "macos"))]
            // SAFETY: errno storage belongs to the current thread.
            let errno = unsafe { libc::__errno_location() };
            // SAFETY: errno is valid thread-local storage and this iterator is
            // the sole consumer of its owned DIR cursor.
            unsafe {
                *errno = 0;
            }
            let entry = unsafe { libc::readdir(self.directory) };
            if entry.is_null() {
                self.done = true;
                // SAFETY: errno is still current thread's storage.
                return if unsafe { *errno } == 0 {
                    None
                } else {
                    Some(Err(std::io::Error::last_os_error().into()))
                };
            }
            // SAFETY: readdir returned a live, terminated name until next call.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            return Some(Ok(OsString::from_vec(name.to_vec())));
        }
    }
}
#[cfg(unix)]
impl Drop for SetupEntries {
    fn drop(&mut self) {
        // SAFETY: this iterator exclusively owns the DIR and its descriptor.
        let _ = unsafe { libc::closedir(self.directory) };
    }
}
#[cfg(unix)]
fn component(name: &OsStr) -> Result<CString> {
    let bytes = name.as_bytes();
    ensure!(
        !bytes.is_empty() && bytes != b"." && bytes != b".." && !bytes.contains(&b'/'),
        "native setup requires a single child name"
    );
    CString::new(bytes).context("native setup child name contains NUL")
}
