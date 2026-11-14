// SPDX-License-Identifier: Apache-2.0
//! Native operations against caller-pinned objects, never recovery decisions.
use super::super::edit_record::NativeIdentity;
use airk_process::directory::{ObjectIdentity, OpenedDirectory};
use std::{ffi::OsStr, fs::File, io, path::Path};

pub(super) fn identity(file: &File) -> io::Result<NativeIdentity> {
    Ok(match airk_process::directory::identity(file)? {
        ObjectIdentity::Unix { device, inode } => NativeIdentity::Unix { device, inode },
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => NativeIdentity::Windows {
            volume_serial,
            file_id: airk_core::identity::Incarnation::from_bytes(file_id),
        },
    })
}

#[cfg(windows)]
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let text: Vec<_> = path.as_os_str().encode_wide().collect();
    if text.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in edit path",
        ));
    }
    Ok(text.into_iter().chain(Some(0)).collect())
}

#[cfg(unix)]
fn component_name(value: &OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(value.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in edit name"))
}

pub(super) fn create_directory(
    parent: &OpenedDirectory,
    name: &OsStr,
    path: &Path,
) -> io::Result<OpenedDirectory> {
    #[cfg(windows)]
    airk_ipc::endpoint::create_root(path).map_err(|error| io::Error::other(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let name = component_name(name)?;
        // SAFETY: live parent fd, single validated component and private mode.
        if unsafe { libc::mkdirat(parent.as_file().as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let _ = path;
    }
    parent.open_child(name)
}

pub(super) fn open_file(
    parent: &OpenedDirectory,
    name: &OsStr,
    path: &Path,
    create: bool,
    write: bool,
) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        let mut options = std::fs::OpenOptions::new();
        options
            .read(true)
            .write(write)
            .create_new(create)
            .share_mode(if name == OsStr::new("owner.json") {
                3
            } else {
                7
            })
            .custom_flags(0x0020_0000);
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other(
                "redirected or nonregular config edit object",
            ));
        }
        require_default_stream(path, &file)?;
        let _ = (parent, name);
        Ok(file)
    }
    #[cfg(unix)]
    {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::fs::MetadataExt;
        let component = component_name(name)?;
        let flags = (if write { libc::O_RDWR } else { libc::O_RDONLY })
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if create {
                libc::O_CREAT | libc::O_EXCL
            } else {
                0
            };
        // SAFETY: owned directory fd, one validated component and no-follow flags.
        let fd = unsafe {
            libc::openat(
                parent.as_file().as_raw_fd(),
                component.as_ptr(),
                flags,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openat returned a fresh descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err(io::Error::other("aliased or nonregular edit object"));
        }
        let _ = path;
        Ok(file)
    }
}

#[cfg(windows)]
fn require_default_stream(path: &Path, file: &File) -> io::Result<()> {
    airk_process::directory::require_default_stream(path, file)
}

pub(super) fn flush_directory(directory: &OpenedDirectory, _path: &Path) -> io::Result<()> {
    directory.sync_all()
}

#[cfg(any(unix, test))]
pub(super) fn checked_available_bytes(blocks: u128, fragment_bytes: u128) -> io::Result<u64> {
    let overflow = || io::Error::other("available disk count overflow");
    let bytes = blocks.checked_mul(fragment_bytes).ok_or_else(overflow)?;
    u64::try_from(bytes).map_err(|_| overflow())
}

pub(super) fn available_bytes(directory: &OpenedDirectory, path: &Path) -> io::Result<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
        };
        let text = wide(path)?;
        let mut volume = vec![0u16; 32768];
        // SAFETY: terminated input and writable bounded output.
        if unsafe { GetVolumePathNameW(text.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut filesystem = [0u16; 64];
        // SAFETY: volume and output are live and bounded; other outputs are unused.
        if unsafe {
            GetVolumeInformationW(
                volume.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let end = filesystem
            .iter()
            .position(|ch| *ch == 0)
            .unwrap_or(filesystem.len());
        // Only the observed local NTFS API profile is admitted initially.
        if String::from_utf16_lossy(&filesystem[..end]) != "NTFS"
            || unsafe { GetDriveTypeW(volume.as_ptr()) } != 3
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "config retention requires a qualified local volume",
            ));
        }
        let mut bytes = 0u64;
        // SAFETY: terminated path and initialized writable scalar.
        if unsafe {
            GetDiskFreeSpaceExW(
                text.as_ptr(),
                &mut bytes,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _ = directory;
        Ok(bytes)
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let mut statistics: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: owned directory fd and initialized output.
        if unsafe { libc::fstatvfs(directory.as_file().as_raw_fd(), &mut statistics) } != 0 {
            return Err(io::Error::last_os_error());
        }
        #[cfg(target_os = "linux")]
        {
            let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
            // SAFETY: same owned fd and initialized output.
            if unsafe { libc::fstatfs(directory.as_file().as_raw_fd(), &mut filesystem) } != 0 {
                return Err(io::Error::last_os_error());
            }
            if filesystem.f_type as u64 != 0xef53 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "config retention requires a qualified local volume",
                ));
            }
        }
        #[cfg(target_os = "macos")]
        {
            let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
            // SAFETY: same owned fd and initialized output.
            if unsafe { libc::fstatfs(directory.as_file().as_raw_fd(), &mut filesystem) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let kind: Vec<_> = filesystem
                .f_fstypename
                .iter()
                .take_while(|ch| **ch != 0)
                .map(|ch| *ch as u8)
                .collect();
            if kind != b"apfs" {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "config retention requires a qualified local volume",
                ));
            }
        }
        let _ = path;
        checked_available_bytes(
            u128::from(statistics.f_bavail),
            u128::from(statistics.f_frsize),
        )
    }
}

pub(super) fn publish(
    parent: &OpenedDirectory,
    transaction: &OpenedDirectory,
    target_name: &OsStr,
    target: &Path,
    staged: &Path,
    backup: &Path,
) -> io::Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
        let target = wide(target)?;
        let staged = wide(staged)?;
        let backup = wide(backup)?;
        // SAFETY: all three same-volume names remain under caller-held
        // no-delete-share directory pins. Zero flags never ignore ACL errors.
        if unsafe {
            ReplaceFileW(
                target.as_ptr(),
                staged.as_ptr(),
                backup.as_ptr(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _ = (parent, transaction, target_name);
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let target_name = component_name(target_name)?;
        let prepared = component_name(OsStr::new("prepared"))?;
        // SAFETY: pinned directory fds and live single-component names; exchange
        // requires both entries and therefore cannot recreate a deleted target.
        if unsafe {
            libc::renameat2(
                transaction.as_file().as_raw_fd(),
                prepared.as_ptr(),
                parent.as_file().as_raw_fd(),
                target_name.as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let _ = (target, staged, backup);
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let target_name = component_name(target_name)?;
        let prepared = component_name(OsStr::new("prepared"))?;
        // SAFETY: pinned directory fds, live names and the native RENAME_SWAP flag.
        if unsafe {
            libc::renameatx_np(
                transaction.as_file().as_raw_fd(),
                prepared.as_ptr(),
                parent.as_file().as_raw_fd(),
                target_name.as_ptr(),
                libc::RENAME_SWAP,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let _ = (target, staged, backup);
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (parent, transaction, target_name, target, staged, backup);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native retained publication is unavailable",
        ))
    }
}
