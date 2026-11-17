// SPDX-License-Identifier: Apache-2.0
//! Metadata-only naming-boundary qualification for permanent source policy.
//!
//! This proves a supported case mode, not Unicode-comparator equivalence. Callers
//! must propagate failure rather than prune a directory or publish a partial walk.
use anyhow::Result;
use std::{
    fs::File,
    path::{Component, Path},
};

#[cfg(windows)]
pub const PROFILE: &str = "airk-native-name-boundary/windows-insensitive-v1";
#[cfg(target_os = "macos")]
pub const PROFILE: &str = "airk-native-name-boundary/apple-local-insensitive-v1";
#[cfg(target_os = "linux")]
pub const PROFILE: &str = "airk-native-name-boundary/linux-known-noncasefold-v1";
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub const PROFILE: &str = "airk-native-name-boundary/unsupported-v1";

/// No source locator is included in this error's public display.
#[derive(Debug)]
pub struct NativeNameBoundaryUnavailable {
    reason: &'static str,
}

impl NativeNameBoundaryUnavailable {
    pub fn new(reason: &'static str) -> Self {
        Self {
            reason: if reason.len() <= 128 && reason.is_ascii() {
                reason
            } else {
                "unsupported_boundary"
            },
        }
    }
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl std::fmt::Display for NativeNameBoundaryUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native naming boundary unavailable: {}", self.reason)
    }
}

impl std::error::Error for NativeNameBoundaryUnavailable {}

fn unavailable(reason: &'static str) -> anyhow::Error {
    NativeNameBoundaryUnavailable::new(reason).into()
}

/// Qualify the source root and existing directory ancestors of a portable locator.
/// An exact-file locator qualifies its parent without opening the file. Missing
/// future suffixes use the last existing parent; capture must qualify them again
/// after creation. Every existing source-root ancestor is required to exist.
pub fn qualify(root: &Path, relative: &str, directory: bool) -> Result<()> {
    if !root.is_absolute()
        || root.as_os_str().len() > 32768
        || relative.len() > 4096
        || relative.starts_with('/')
        || relative.contains('\0')
        || cfg!(windows) && relative.contains(['\\', ':'])
        || (!relative.is_empty() && relative.split('/').any(|p| matches!(p, "" | "." | "..")))
    {
        return Err(unavailable("invalid_locator"));
    }
    if root
        .components()
        .any(|p| matches!(p, Component::CurDir | Component::ParentDir))
    {
        return Err(unavailable("invalid_root"));
    }
    let mut suffix: Vec<_> = relative.split('/').filter(|p| !p.is_empty()).collect();
    if !directory {
        suffix.pop();
    }
    if root.components().count().saturating_add(suffix.len()) > 256 {
        return Err(unavailable("ancestor_budget"));
    }
    qualify_path(root, &suffix)
}

/// Qualify a pinned, nonredirected directory before reading a child name or bytes.
pub(crate) fn qualify_directory_handle(file: &File) -> Result<()> {
    if !file
        .metadata()
        .map_err(|_| unavailable("directory_metadata"))?
        .is_dir()
    {
        return Err(unavailable("not_directory"));
    }
    platform_qualify(file)
}

#[cfg(unix)]
fn qualify_path(root: &Path, suffix: &[&str]) -> Result<()> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };
    let first = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| unavailable("root_open"))?;
    qualify_directory_handle(&first)?;
    let mut held = vec![first];
    let root_names: Vec<_> = root
        .components()
        .filter_map(|p| match p {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let names = root_names
        .iter()
        .copied()
        .map(|name| (name, false))
        .chain(suffix.iter().map(|name| (std::ffi::OsStr::new(name), true)));
    for (name, may_be_missing) in names {
        let name = CString::new(name.as_bytes()).map_err(|_| unavailable("invalid_component"))?;
        // SAFETY: valid held directory and NUL-terminated name; ownership is
        // transferred only after checking the returned descriptor.
        let fd = unsafe {
            libc::openat(
                held.last().unwrap().as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            if may_be_missing
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound
            {
                return Ok(());
            }
            return Err(unavailable("ancestor_open"));
        }
        // SAFETY: successful openat returned a fresh owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        qualify_directory_handle(&file)?;
        held.push(file);
    }
    Ok(())
}

#[cfg(windows)]
fn qualify_path(root: &Path, suffix: &[&str]) -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut path = std::path::PathBuf::new();
    let mut held = Vec::new();
    let parts = root.components().map(|p| (p, false)).chain(
        suffix
            .iter()
            .map(|p| (Component::Normal(std::ffi::OsStr::new(p)), true)),
    );
    for (part, may_be_missing) in parts {
        path.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let file = match std::fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if may_be_missing && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(_) => return Err(unavailable("ancestor_open")),
        };
        qualify_directory_handle(&file)?;
        held.push(file);
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn qualify_path(_: &Path, _: &[&str]) -> Result<()> {
    Err(unavailable("unsupported_platform"))
}

#[cfg(windows)]
fn platform_qualify(file: &File) -> Result<()> {
    use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_CASE_SENSITIVE_INFO, FileCaseSensitiveInfo,
        GetFileInformationByHandleEx,
    };
    if file
        .metadata()
        .map_err(|_| unavailable("directory_metadata"))?
        .file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err(unavailable("redirected_directory"));
    }
    let mut info = FILE_CASE_SENSITIVE_INFO::default();
    // SAFETY: handle is borrowed and output storage has the documented layout.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileCaseSensitiveInfo,
            (&mut info as *mut FILE_CASE_SENSITIVE_INFO).cast(),
            std::mem::size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(unavailable("case_mode_query"));
    }
    require_insensitive_flags(info.Flags)
}

#[cfg(any(windows, test))]
fn require_insensitive_flags(flags: u32) -> Result<()> {
    if flags == 0 {
        Ok(())
    } else {
        Err(unavailable("sensitive_or_unknown_case_mode"))
    }
}

#[cfg(target_os = "macos")]
fn platform_qualify(file: &File) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: output points to suitably sized storage and fd remains borrowed.
    if unsafe { libc::fstatfs(file.as_raw_fd(), info.as_mut_ptr()) } != 0 {
        return Err(unavailable("filesystem_query"));
    }
    let info = unsafe { info.assume_init() };
    let name: Vec<u8> = info
        .f_fstypename
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    if info.f_flags & libc::MNT_LOCAL as u32 == 0 || !matches!(name.as_slice(), b"apfs" | b"hfs") {
        return Err(unavailable("unsupported_filesystem"));
    }
    // Only exactly zero proves insensitive; -1 includes unsupported/unknown.
    if unsafe { libc::fpathconf(file.as_raw_fd(), libc::_PC_CASE_SENSITIVE) } != 0 {
        return Err(unavailable("sensitive_or_unknown_case_mode"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_qualify(file: &File) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: output points to suitably sized storage and fd remains borrowed.
    if unsafe { libc::fstatfs(file.as_raw_fd(), info.as_mut_ptr()) } != 0 {
        return Err(unavailable("filesystem_query"));
    }
    let info = unsafe { info.assume_init() };
    if !supported_linux_filesystem(info.f_type as u64) {
        return Err(unavailable("unsupported_filesystem"));
    }
    let mut flags: libc::c_long = 0;
    // ext4 and newer tmpfs can casefold individual directories. Unknown ioctl
    // support (including older tmpfs) is refused; overlay/FUSE/network filesystems
    // cannot be qualified from the upper filesystem's flags and are not accepted.
    // Linux UAPI include/uapi/linux/fs.h: FS_CASEFOLD_FL = 0x40000000.
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::FS_IOC_GETFLAGS, &mut flags) } < 0 {
        return Err(unavailable("case_mode_query"));
    }
    if flags & 0x4000_0000 != 0 {
        return Err(unavailable("casefold_directory"));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn supported_linux_filesystem(magic: u64) -> bool {
    matches!(magic, 0x0000_ef53 | 0x0102_1994) // ext2/3/4 family; tmpfs
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_qualify(_: &File) -> Result<()> {
    Err(unavailable("unsupported_platform"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_case_modes_and_filesystems_refuse() {
        assert!(require_insensitive_flags(0).is_ok());
        assert!(require_insensitive_flags(1).is_err());
        assert!(require_insensitive_flags(2).is_err());
        assert!(supported_linux_filesystem(0xef53));
        assert!(supported_linux_filesystem(0x0102_1994));
        assert!(!supported_linux_filesystem(0x794c_7630)); // overlay
        assert!(!supported_linux_filesystem(0x9123_683e)); // btrfs
    }

    #[test]
    fn malformed_locators_refuse_before_probe() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for locator in ["../escape", "a//b", "/absolute", "a\0b"] {
            let error = qualify(temp.path(), locator, false).unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<NativeNameBoundaryUnavailable>()
                    .unwrap()
                    .reason(),
                "invalid_locator"
            );
        }
        Ok(())
    }

    #[test]
    fn disposable_native_probe_is_explicit_about_unknown_boundaries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        let observed = qualify(&root, "future/child.txt", false);
        if let Err(error) = observed {
            assert!(
                error
                    .downcast_ref::<NativeNameBoundaryUnavailable>()
                    .is_some()
            );
        }
        // Exact files never need to be opened for this metadata-only proof.
        std::fs::write(root.join("source.txt"), b"unchanged")?;
        let _ = qualify(&root, "source.txt", false);
        assert_eq!(std::fs::read(root.join("source.txt"))?, b"unchanged");
        Ok(())
    }
}
