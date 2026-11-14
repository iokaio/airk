// SPDX-License-Identifier: Apache-2.0
use anyhow::{Result, ensure};
use std::{ffi::OsStr, fs::File, path::Path};

/// The original path-based no-replace publisher. It makes no ownership or
/// directory pinning claim; its platform behavior and errors are retained.
pub fn publish_new(from: &Path, to: &Path) -> Result<()> {
    rename(None, from, None, to, false)
}

fn component(name: &OsStr) -> Result<()> {
    super::validate_child_name(name)?;
    Ok(())
}

/// Publish within a held parent without replacing an existing destination.
/// Unix uses the original dirfd, even if its path was renamed. Windows pins
/// the supplied absolute parent path and verifies its identity against the
/// held parent before using MoveFileExW; it is not a relative-handle rename.
pub fn publish_new_at(parent: &File, parent_path: &Path, from: &OsStr, to: &OsStr) -> Result<()> {
    publish_at(parent, parent_path, from, to, false)
}

pub(super) fn publish_at(
    parent: &File,
    parent_path: &Path,
    from: &OsStr,
    to: &OsStr,
    replace: bool,
) -> Result<()> {
    component(from)?;
    component(to)?;
    ensure!(
        parent.metadata()?.is_dir(),
        "publication parent is not a directory"
    );
    #[cfg(windows)]
    {
        let named = super::OpenedDirectory::open(parent_path)?;
        ensure!(
            named.identity()? == super::identity(parent)?,
            "publication parent identity changed"
        );
        rename(
            None,
            &parent_path.join(from),
            None,
            &parent_path.join(to),
            replace,
        )
    }
    #[cfg(unix)]
    {
        let _ = parent_path;
        rename(
            Some(parent),
            Path::new(from),
            Some(parent),
            Path::new(to),
            replace,
        )
    }
}

fn rename(
    from_parent: Option<&File>,
    from: &Path,
    to_parent: Option<&File>,
    to: &Path,
    replace: bool,
) -> Result<()> {
    #[cfg(windows)]
    {
        let _ = (from_parent, to_parent);
        let from = native_path(from)?;
        let to = native_path(to)?;
        // SAFETY: terminated paths; zero flags forbid replacing a destination.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    if replace {
                        windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    } else {
                        0
                    },
                )
            } != 0,
            "exclusive state publication failed: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    {
        use std::{
            ffi::CString,
            os::{fd::AsRawFd, unix::ffi::OsStrExt},
        };
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        let source = from_parent.map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd);
        let destination = to_parent.map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd);
        #[cfg(target_os = "linux")]
        // SAFETY: live directory descriptors, terminated paths, no-replace flag.
        let result = unsafe {
            if replace {
                libc::renameat(source, from.as_ptr(), destination, to.as_ptr())
            } else {
                libc::renameat2(
                    source,
                    from.as_ptr(),
                    destination,
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            }
        };
        #[cfg(target_os = "macos")]
        // SAFETY: live directory descriptors, terminated paths, exclusive flag.
        let result = unsafe {
            if replace {
                libc::renameat(source, from.as_ptr(), destination, to.as_ptr())
            } else {
                libc::renameatx_np(
                    source,
                    from.as_ptr(),
                    destination,
                    to.as_ptr(),
                    libc::RENAME_EXCL,
                )
            }
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        anyhow::bail!("exclusive state publication is unsupported on this platform");
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        ensure!(
            result == 0,
            "filesystem cannot publish state exclusively: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn native_path(path: &Path) -> Result<Vec<u16>> {
    use std::{
        os::windows::ffi::OsStrExt,
        path::{Component, Prefix},
    };
    let absolute = std::path::absolute(path)?;
    let input = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    ensure!(!input.contains(&0), "NUL in publication path");
    let mut path = match absolute.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) => r"\\?\".encode_utf16().chain(input).collect::<Vec<_>>(),
            Prefix::UNC(_, _) => r"\\?\UNC\"
                .encode_utf16()
                .chain(input.into_iter().skip(2))
                .collect(),
            Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _) => input,
            _ => anyhow::bail!("unsupported publication namespace"),
        },
        _ => anyhow::bail!("publication path has no volume"),
    };
    ensure!(
        path.len() < 32767,
        "publication path exceeds native allowance"
    );
    path.push(0);
    Ok(path)
}
