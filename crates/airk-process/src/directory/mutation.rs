// SPDX-License-Identifier: Apache-2.0
use super::*;

impl OpenedDirectory {
    /// Open one existing regular single-linked file without following its leaf.
    pub fn open_file(&self, name: &OsStr) -> io::Result<File> {
        self.file(name, false)
    }

    /// Exclusively create one file; caller checks its exact-handle privacy.
    pub fn create_file(&self, name: &OsStr) -> io::Result<File> {
        self.file(name, true)
    }

    fn file(&self, name: &OsStr, create: bool) -> io::Result<File> {
        validate_child_name(name)?;
        #[cfg(unix)]
        let file = {
            use std::{
                ffi::CString,
                os::{
                    fd::{AsRawFd, FromRawFd},
                    unix::ffi::OsStrExt,
                },
            };
            let name = CString::new(name.as_bytes()).map_err(|_| invalid("NUL in file name"))?;
            let flags = (if create {
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY
            }) | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK;
            // SAFETY: held parent fd, validated component, fresh returned descriptor.
            let fd =
                unsafe { libc::openat(self.as_file().as_raw_fd(), name.as_ptr(), flags, 0o600) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful openat returns a new owned descriptor.
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            std::fs::OpenOptions::new()
                .read(true)
                .write(create)
                .create_new(create)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))?
        };
        require_file(&file)?;
        Ok(file)
    }

    /// Create with caller-supplied Windows file security initialization.
    pub fn create_file_with(
        &self,
        name: &OsStr,
        initialize_windows: impl FnOnce(&Path) -> io::Result<File>,
    ) -> io::Result<File> {
        validate_child_name(name)?;
        #[cfg(unix)]
        {
            let _ = initialize_windows;
            self.create_file(name)
        }
        #[cfg(windows)]
        {
            let file = initialize_windows(&self.path.join(name))?;
            require_file(&file)?;
            Ok(file)
        }
    }

    /// Initialize a fresh child. The Windows callback supplies owning ACL
    /// policy while all parent names remain pinned; Unix uses mkdirat(mode).
    pub fn create_child_directory(
        &self,
        name: &OsStr,
        mode: u32,
        initialize_windows: impl FnOnce(&Path) -> io::Result<()>,
    ) -> io::Result<Self> {
        validate_child_name(name)?;
        #[cfg(unix)]
        {
            use std::{
                ffi::CString,
                os::{fd::AsRawFd, unix::ffi::OsStrExt},
            };
            let component =
                CString::new(name.as_bytes()).map_err(|_| invalid("NUL in directory name"))?;
            // SAFETY: held parent fd, single validated name and explicit mode.
            if unsafe {
                libc::mkdirat(
                    self.as_file().as_raw_fd(),
                    component.as_ptr(),
                    mode as libc::mode_t,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            let _ = initialize_windows;
        }
        #[cfg(windows)]
        {
            let _ = mode;
            initialize_windows(&self.path.join(name))?;
        }
        self.open_child(name)
    }

    #[cfg(windows)]
    pub fn require_default_stream(&self, name: &OsStr, file: &File) -> io::Result<()> {
        validate_child_name(name)?;
        super::require_default_stream(&self.path.join(name), file)
    }

    /// Unlink one original-parent directory entry. Unix does not promise an
    /// atomic expected-object comparison; caller owns authorization/serialization.
    pub fn unlink_file(&self, name: &OsStr) -> io::Result<()> {
        validate_child_name(name)?;
        #[cfg(unix)]
        {
            use std::{
                ffi::CString,
                os::{fd::AsRawFd, unix::ffi::OsStrExt},
            };
            let name = CString::new(name.as_bytes()).map_err(|_| invalid("NUL in file name"))?;
            // SAFETY: original held parent fd and single terminated child; no follow.
            if unsafe { libc::unlinkat(self.as_file().as_raw_fd(), name.as_ptr(), 0) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.sync_all()
        }
        #[cfg(windows)]
        {
            self.remove_file_if(name, |_, _| Ok(true)).map(|_| ())
        }
    }

    /// Publish one staged regular entry within this exact parent object.
    pub fn publish_file(&self, from: &OsStr, to: &OsStr, replace: bool) -> anyhow::Result<()> {
        publication::publish_at(self.as_file(), &self.path, from, to, replace)
    }
}

pub(super) fn require_file(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("rooted file is not regular"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("rooted file has aliases"));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(invalid("rooted file is redirected"));
        }
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: owned live handle and exact initialized native output.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if information.nNumberOfLinks != 1 {
            return Err(invalid("rooted file has aliases"));
        }
    }
    Ok(())
}
