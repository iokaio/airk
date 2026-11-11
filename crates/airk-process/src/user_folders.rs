// SPDX-License-Identifier: Apache-2.0
//! Native per-user folders (split U84): the Windows Known Folders through
//! the process token and the Unix passwd home, never inherited environment
//! overrides. Moved from `airk-cli` `user_paths.rs` by
//! `scripts/dev/split_u84b_user_folders.py`; shared by the service and `airkctl`.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The Known Folders AIRK resolves on Windows.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownFolder {
    Profile,
    LocalAppData,
    RoamingAppData,
}

/// A native user directory: absolute, UTF-8, at most 4,096 bytes, no control characters.
pub fn checked(path: PathBuf) -> Result<PathBuf> {
    let text = path
        .to_str()
        .context("native user directory is not UTF-8")?;
    ensure!(
        path.is_absolute() && text.len() <= 4096 && !text.chars().any(char::is_control),
        "invalid native user directory"
    );
    Ok(path)
}

#[cfg(windows)]
pub fn known_folder(folder: KnownFolder) -> Result<PathBuf> {
    let id = match folder {
        KnownFolder::Profile => &windows_sys::Win32::UI::Shell::FOLDERID_Profile,
        KnownFolder::LocalAppData => &windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData,
        KnownFolder::RoamingAppData => &windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData,
    };
    use std::os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Security::{TOKEN_IMPERSONATE, TOKEN_QUERY},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    struct Allocated(*mut u16);
    impl Drop for Allocated {
        fn drop(&mut self) {
            // SAFETY: SHGetKnownFolderPath uses COM task allocation, including on failure.
            unsafe {
                windows_sys::Win32::System::Com::CoTaskMemFree(self.0.cast());
            }
        }
    }
    let mut token = std::ptr::null_mut();
    // SAFETY: scalar rights, this process's pseudo-handle and writable token output.
    ensure!(
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_IMPERSONATE,
                &mut token,
            )
        } != 0,
        "native user token unavailable: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful OpenProcessToken transfers one owned real handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: valid folder GUID, current process identity and writable output.
    // DONT_VERIFY performs discovery without creating or probing the target directory.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::SHGetKnownFolderPath(
            id,
            0x4000,
            token.as_raw_handle(),
            &mut pointer,
        )
    };
    let allocation = Allocated(pointer);
    ensure!(
        result >= 0 && !allocation.0.is_null(),
        "native user folder is unavailable (HRESULT {result:#x})"
    );
    let mut length = 0usize;
    // SAFETY: the successful SDK result is a null-terminated UTF-16 allocation.
    while length < 32768 && unsafe { *allocation.0.add(length) } != 0 {
        length += 1;
    }
    ensure!(length < 32768, "native user folder exceeds path allowance");
    // SAFETY: the bounded prefix belongs to the still-owned native allocation.
    checked(PathBuf::from(OsString::from_wide(unsafe {
        std::slice::from_raw_parts(allocation.0, length)
    })))
}

#[cfg(windows)]
pub fn home() -> Result<PathBuf> {
    known_folder(KnownFolder::Profile)
}

#[cfg(unix)]
pub fn home() -> Result<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStringExt};
    // SAFETY: scalar identity and limit queries take no writable pointers.
    let uid = unsafe { libc::geteuid() };
    let suggested = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut size = if suggested > 0 {
        (suggested as usize).clamp(1024, 1024 * 1024)
    } else {
        16384
    };
    loop {
        let mut buffer = vec![0u8; size];
        // SAFETY: output structure is initialized by getpwuid_r on success.
        let mut password: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        // SAFETY: writable output, correctly bounded scratch allocation and result pointer.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                &mut password,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && size < 1024 * 1024 {
            size = (size * 2).min(1024 * 1024);
            continue;
        }
        ensure!(
            status == 0
                && !result.is_null()
                && password.pw_uid == uid
                && !password.pw_dir.is_null(),
            "native user home is unavailable"
        );
        // SAFETY: successful getpwuid_r stores a terminated string in our live scratch buffer.
        let bytes = unsafe { CStr::from_ptr(password.pw_dir) }.to_bytes();
        ensure!(
            bytes.len() <= 4096,
            "native user home exceeds path allowance"
        );
        return checked(PathBuf::from(OsString::from_vec(bytes.to_vec())));
    }
}

#[cfg(not(any(windows, unix)))]
pub fn home() -> Result<PathBuf> {
    anyhow::bail!("native user directories are unsupported on this platform")
}

/// An absolute location whose existing ancestor is canonicalized and whose
/// missing suffix is kept; parent traversal is refused (U86).
pub fn canonical_location(path: &Path) -> Result<PathBuf> {
    checked(path.to_path_buf())?;
    ensure!(
        !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)),
        "parent traversal is not a native location"
    );
    let mut parent = path.to_path_buf();
    let mut suffix = Vec::new();
    while !parent.try_exists()? {
        suffix.push(
            parent
                .file_name()
                .context("missing native location ancestor")?
                .to_owned(),
        );
        ensure!(parent.pop(), "missing native location ancestor");
    }
    ensure!(
        parent.is_dir(),
        "native location ancestor must be a directory"
    );
    let mut resolved = parent.canonicalize()?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    checked(resolved)
}
