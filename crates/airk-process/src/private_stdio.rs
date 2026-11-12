// SPDX-License-Identifier: Apache-2.0
//! Private session streams: the frame channel moves to private,
//! non-inheritable duplicates of stdin/stdout; the process's standard
//! input then reads the null device and its standard output writes to
//! stderr, so no stray print or child can corrupt a frame. Moved from
//! `airk-cli`'s session (split step 3f.3) for `airkctl session`.
pub use platform::private_streams;

#[cfg(unix)]
mod platform {
    use anyhow::{Result, ensure};
    use std::{
        fs::File,
        os::fd::{AsRawFd, FromRawFd},
    };

    fn private_duplicate(fd: libc::c_int) -> Result<File> {
        // SAFETY: duplicates a standard descriptor; the new close-on-exec
        // descriptor is owned by the returned File.
        let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        ensure!(
            copy >= 0,
            "duplicate session stream: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: `copy` is a fresh descriptor owned by nothing else.
        Ok(unsafe { File::from_raw_fd(copy) })
    }

    /// Frames move to private descriptors; fd 0 reads /dev/null and fd 1 writes
    /// to stderr for everything else in this process and its children.
    pub fn private_streams() -> Result<(File, File)> {
        let input = private_duplicate(0)?;
        let output = private_duplicate(1)?;
        let null = File::open("/dev/null")?;
        // SAFETY: dup2 onto the standard descriptors from live descriptors.
        let redirected = unsafe { libc::dup2(null.as_raw_fd(), 0) == 0 && libc::dup2(2, 1) == 1 };
        ensure!(
            redirected,
            "redirect session standard streams: {}",
            std::io::Error::last_os_error()
        );
        Ok((input, output))
    }
}

#[cfg(windows)]
mod platform {
    use anyhow::{Result, ensure};
    use std::{
        fs::File,
        os::windows::io::{FromRawHandle, IntoRawHandle},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
        System::{
            Console::{
                GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
                SetStdHandle,
            },
            Threading::GetCurrentProcess,
        },
    };

    /// Move one standard handle to a private, non-inheritable duplicate and
    /// close the original so no child can inherit or use it.
    fn take(which: STD_HANDLE) -> Result<File> {
        // SAFETY: standard-handle query and duplication into this process; the
        // duplicate is owned by the returned File, the original is closed once.
        unsafe {
            let original = GetStdHandle(which);
            ensure!(
                !original.is_null() && original as isize != -1,
                "session standard stream is unavailable"
            );
            let mut copy: HANDLE = std::ptr::null_mut();
            ensure!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    original,
                    GetCurrentProcess(),
                    &mut copy,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                ) != 0,
                "duplicate session stream: {}",
                std::io::Error::last_os_error()
            );
            CloseHandle(original);
            Ok(File::from_raw_handle(copy))
        }
    }

    /// Frames move to private handles; standard input reads NUL and standard
    /// output writes to stderr for everything else in this process and its children.
    pub fn private_streams() -> Result<(File, File)> {
        let input = take(STD_INPUT_HANDLE)?;
        let output = take(STD_OUTPUT_HANDLE)?;
        let null = File::open("NUL")?.into_raw_handle();
        // SAFETY: replaces process standard handles with live handles that this
        // process keeps open for its lifetime.
        let redirected = unsafe {
            SetStdHandle(STD_INPUT_HANDLE, null) != 0
                && SetStdHandle(STD_OUTPUT_HANDLE, GetStdHandle(STD_ERROR_HANDLE)) != 0
        };
        ensure!(
            redirected,
            "redirect session standard streams: {}",
            std::io::Error::last_os_error()
        );
        Ok((input, output))
    }
}
