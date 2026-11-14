// SPDX-License-Identifier: Apache-2.0
//! Delete contents relative to the pinned Unix directory object. No receipt,
//! pathname authority or native identity implementation is duplicated here.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    },
};

struct Entries(*mut libc::DIR, bool);
impl Drop for Entries {
    fn drop(&mut self) {
        // SAFETY: fdopendir transferred this owned descriptor into the stream.
        unsafe {
            libc::closedir(self.0);
        }
    }
}

#[cfg(target_os = "linux")]
unsafe fn errno() -> *mut libc::c_int {
    // SAFETY: libc owns a thread-local errno slot.
    unsafe { libc::__errno_location() }
}
#[cfg(target_os = "macos")]
unsafe fn errno() -> *mut libc::c_int {
    // SAFETY: libc owns a thread-local errno slot.
    unsafe { libc::__error() }
}

impl Iterator for Entries {
    type Item = Result<CString>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.1 {
            return None;
        }
        loop {
            // SAFETY: errno is this thread's slot; stream is live, and the
            // returned d_name stays valid until the next readdir invocation.
            unsafe {
                *errno() = 0;
                let entry = libc::readdir(self.0);
                if entry.is_null() {
                    self.1 = true;
                    let code = *errno();
                    return if code == 0 {
                        None
                    } else {
                        Some(Err(std::io::Error::from_raw_os_error(code).into()))
                    };
                }
                let name = CStr::from_ptr((*entry).d_name.as_ptr());
                if name.to_bytes() != b"." && name.to_bytes() != b".." {
                    return Some(Ok(name.to_owned()));
                }
            }
        }
    }
}

fn names(parent: &File) -> Result<Entries> {
    // Open a fresh directory description: dup would share an enumeration
    // offset with the pin and could skip entries on a resumed purge.
    let dot = c".";
    // SAFETY: live directory descriptor, literal name, readonly nofollow flags.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            dot.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    ensure!(
        fd >= 0,
        "purge directory open failed: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: fresh owned fd; fdopendir owns it on success.
    let directory = unsafe { libc::fdopendir(fd) };
    if directory.is_null() {
        // SAFETY: ownership was not transferred on failure.
        unsafe {
            libc::close(fd);
        }
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(Entries(directory, false))
}

fn remove(parent: &File, name: &CStr) -> Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: initialized output on successful nofollow stat of one component.
    ensure!(
        unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0,
        "purge child observation failed: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful fstatat initialized stat.
    let directory = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT == libc::S_IFDIR;
    if directory {
        // SAFETY: live directory descriptor and one component; no symlink follow.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        ensure!(
            fd >= 0,
            "purge child open failed: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: successful openat returned a fresh owned descriptor.
        let child = unsafe { File::from_raw_fd(fd) };
        for entry in names(&child)? {
            remove(&child, &entry?)?;
        }
        child.sync_all()?;
    }
    // SAFETY: one component relative to held parent; unlink never follows it.
    ensure!(
        unsafe {
            libc::unlinkat(
                parent.as_raw_fd(),
                name.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        } == 0,
        "purge child removal failed: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}

pub(super) fn purge(parent: &File, keep: &[&str]) -> Result<usize> {
    let entries = names(parent).context("enumerate pinned purge root")?;
    let mut count = 0;
    for entry in entries {
        let entry = entry?;
        if keep
            .iter()
            .any(|name| OsStr::new(name).as_bytes() == entry.to_bytes())
        {
            continue;
        }
        remove(parent, &entry)?;
        count += 1;
    }
    Ok(count)
}
