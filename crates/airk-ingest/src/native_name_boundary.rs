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
pub struct NativeNameBoundaryUnavailable {
    reason: &'static str,
    open: Option<PrivateOpenFailure>,
}

struct PrivateOpenFailure {
    native_code: Option<i32>,
    class: &'static str,
    component: usize,
    attempted: String,
    path_truncated: bool,
}

const MAX_PRIVATE_OPEN_PATH_BYTES: usize = 4096;

impl std::fmt::Debug for NativeNameBoundaryUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeNameBoundaryUnavailable")
            .field("reason", &self.reason)
            .finish()
    }
}

impl NativeNameBoundaryUnavailable {
    pub fn new(reason: &'static str) -> Self {
        Self {
            reason: if reason.len() <= 128 && reason.is_ascii() {
                reason
            } else {
                "unsupported_boundary"
            },
            open: None,
        }
    }
    pub fn reason(&self) -> &'static str {
        self.reason
    }
    /// Only the existing owner-private raw-error sink consumes this detail.
    /// Public Display/Debug and the standard error source remain unchanged.
    pub fn private_open_detail(&self) -> Option<String> {
        let open = self.open.as_ref()?;
        Some(format!(
            "native naming open detail: native_code={:?}; class={}; component={}; attempted_path={}; path_truncated={}",
            open.native_code, open.class, open.component, open.attempted, open.path_truncated
        ))
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

fn unavailable_open(
    reason: &'static str,
    error: std::io::Error,
    attempted: std::path::PathBuf,
    class: &'static str,
    component: usize,
) -> anyhow::Error {
    let mut failure = NativeNameBoundaryUnavailable::new(reason);
    // Debug spelling escapes non-text native names. Only a bounded diagnostic
    // prefix is retained; it is never reopened or used as source authority.
    let mut attempted = format!("{attempted:?}");
    let path_truncated = attempted.len() > MAX_PRIVATE_OPEN_PATH_BYTES;
    if path_truncated {
        let mut end = MAX_PRIVATE_OPEN_PATH_BYTES;
        while !attempted.is_char_boundary(end) {
            end -= 1;
        }
        attempted.truncate(end);
    }
    failure.open = Some(PrivateOpenFailure {
        native_code: error.raw_os_error(),
        class,
        component,
        attempted,
        path_truncated,
    });
    failure.into()
}

#[cfg(unix)]
fn open_directory_at(parent: &File, name: &std::ffi::CStr) -> std::io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    // SAFETY: borrowed live parent and NUL-terminated name. A successful fresh
    // descriptor is transferred exactly once; every original no-follow flag stays.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: successful openat returned a fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
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
        os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    };
    let first = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|error| unavailable_open("root_open", error, "/".into(), "root_anchor", 0))?;
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
    for (index, (name, may_be_missing)) in names.enumerate() {
        let name = CString::new(name.as_bytes()).map_err(|_| unavailable("invalid_component"))?;
        let file = match open_directory_at(held.last().unwrap(), &name) {
            Ok(file) => file,
            Err(error) => {
                if may_be_missing && error.kind() == std::io::ErrorKind::NotFound {
                    return Ok(());
                }
                let attempted = root_names
                    .iter()
                    .copied()
                    .chain(suffix.iter().copied().map(std::ffi::OsStr::new))
                    .take(index + 1)
                    .fold(std::path::PathBuf::from("/"), |path, name| path.join(name));
                return Err(unavailable_open(
                    "ancestor_open",
                    error,
                    attempted,
                    if may_be_missing {
                        "relative_suffix"
                    } else {
                        "root_ancestor"
                    },
                    index + 1,
                ));
            }
        };
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
    let mut component = 0;
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
            Err(error) => {
                return Err(unavailable_open(
                    "ancestor_open",
                    error,
                    path.clone(),
                    if may_be_missing {
                        "relative_suffix"
                    } else if component == 0 {
                        "root_anchor"
                    } else {
                        "root_ancestor"
                    },
                    component,
                ));
            }
        };
        qualify_directory_handle(&file)?;
        held.push(file);
        component += 1;
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
    fn open_diagnostic_keeps_public_identity_and_error_classification_unchanged() {
        let secret = std::path::PathBuf::from("private-diagnostic-locator");
        let error = unavailable_open(
            "ancestor_open",
            std::io::Error::from_raw_os_error(13),
            secret,
            "relative_suffix",
            2,
        );
        let native = error
            .downcast_ref::<NativeNameBoundaryUnavailable>()
            .unwrap();
        assert_eq!(native.reason(), "ancestor_open");
        assert_eq!(
            error.to_string(),
            "native naming boundary unavailable: ancestor_open"
        );
        assert!(!format!("{native:?}").contains("private-diagnostic-locator"));
        assert_eq!(error.chain().count(), 1);
        assert!(!error.is::<std::io::Error>());
        let detail = native.private_open_detail().unwrap();
        assert!(detail.contains("native_code=Some(13)"));
        assert!(detail.contains("class=relative_suffix; component=2"));
        assert!(detail.contains("private-diagnostic-locator"));
    }

    #[test]
    fn private_open_path_prefix_is_utf8_bounded_and_explicitly_truncated() {
        let error = unavailable_open(
            "ancestor_open",
            std::io::Error::from_raw_os_error(13),
            std::path::PathBuf::from("\u{1f980}".repeat(8192)),
            "root_ancestor",
            256,
        );
        let native = error
            .downcast_ref::<NativeNameBoundaryUnavailable>()
            .unwrap();
        let open = native.open.as_ref().unwrap();
        assert!(open.path_truncated);
        assert!(open.attempted.len() <= 4096);
        assert!(open.attempted.is_char_boundary(open.attempted.len()));
        let detail = native.private_open_detail().unwrap();
        assert!(detail.contains("native_code=Some(13)"));
        assert!(detail.contains("path_truncated=true"));
        assert!(detail.len() <= 4096 + 256);
        assert_eq!(
            error.to_string(),
            "native naming boundary unavailable: ancestor_open"
        );
        assert!(!error.is::<std::io::Error>());
    }

    #[cfg(unix)]
    #[test]
    fn actual_openat_failure_retains_exact_errno_and_keeps_symlink_unfollowed() -> Result<()> {
        use anyhow::Context;
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        let owner = tempfile::tempdir()?;
        let target = owner.path().join("target");
        std::fs::create_dir(&target)?;
        std::fs::write(target.join("source.txt"), b"source remains unchanged")?;
        let link = owner.path().join("redirected");
        symlink(&target, &link)?;
        let parent = File::open(owner.path())?;
        let name = std::ffi::CString::new(link.file_name().unwrap().as_bytes())?;
        let error = open_directory_at(&parent, &name).unwrap_err();
        let code = error.raw_os_error().context("native errno absent")?;
        assert!(matches!(code, libc::ELOOP | libc::ENOTDIR));
        let failure = unavailable_open("ancestor_open", error, link.clone(), "relative_suffix", 1);
        let detail = failure
            .downcast_ref::<NativeNameBoundaryUnavailable>()
            .unwrap()
            .private_open_detail()
            .unwrap();
        assert!(detail.contains(&format!("native_code=Some({code})")));
        assert!(detail.contains("redirected"));
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(
            std::fs::read(target.join("source.txt"))?,
            b"source remains unchanged"
        );
        assert!(!failure.is::<std::io::Error>());
        Ok(())
    }

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
