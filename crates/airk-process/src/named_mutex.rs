// SPDX-License-Identifier: Apache-2.0
//! Raw Windows named mutex creation; user/ACL policy belongs to the caller.
use std::{
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, OwnedHandle},
    },
};
use windows_sys::Win32::{Security::SECURITY_ATTRIBUTES, System::Threading::CreateMutexW};

pub fn create(attributes: &SECURITY_ATTRIBUTES, name: &str) -> io::Result<OwnedHandle> {
    let name = std::ffi::OsStr::new(name).encode_wide().collect::<Vec<_>>();
    if name.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in mutex name",
        ));
    }
    let name = name.into_iter().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: live native security attributes/descriptor and terminated name.
    let handle = unsafe { CreateMutexW(attributes, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreateMutexW transfers this owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
