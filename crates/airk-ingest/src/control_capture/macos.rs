// SPDX-License-Identifier: Apache-2.0
//! Fixed macOS OS-control anchor. No configurable mapping or source authority.
use super::Metadata;
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CStr, CString, c_void},
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Component, Path},
    sync::atomic::AtomicBool,
};

// Public Darwin filesec ABI. libc0.2.190 does not declare these functions.
unsafe extern "C" {
    fn filesec_init() -> *mut c_void;
    fn filesec_free(value: *mut c_void);
    fn filesec_query_property(
        value: *mut c_void,
        property: libc::c_int,
        valid: *mut libc::c_int,
    ) -> libc::c_int;
    #[cfg_attr(target_arch = "x86_64", link_name = "fstatx64_np")]
    fn fstatx_np(fd: libc::c_int, stat: *mut libc::stat, security: *mut c_void) -> libc::c_int;
    #[cfg_attr(target_arch = "x86_64", link_name = "lstatx64_np")]
    fn lstatx_np(
        path: *const libc::c_char,
        stat: *mut libc::stat,
        security: *mut c_void,
    ) -> libc::c_int;
}
struct Security(std::ptr::NonNull<c_void>);
impl Drop for Security {
    fn drop(&mut self) {
        unsafe { filesec_free(self.0.as_ptr()) };
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    links: u64,
    flags: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<&libc::stat> for Stamp {
    fn from(value: &libc::stat) -> Self {
        Self {
            device: value.st_dev as u64,
            inode: value.st_ino,
            uid: value.st_uid,
            gid: value.st_gid,
            mode: u32::from(value.st_mode),
            links: u64::from(value.st_nlink),
            flags: value.st_flags,
            modified: (value.st_mtime, value.st_mtime_nsec),
            changed: (value.st_ctime, value.st_ctime_nsec),
        }
    }
}
fn protected_directory(stamp: &Stamp) -> Result<()> {
    super::decision::directory(stamp.uid, stamp.mode)?;
    Ok(())
}
fn security_observation(fd: Option<&File>) -> Result<libc::stat> {
    // A bare acl_get_fd_np NULL cannot distinguish missing ACL from failed I/O.
    // Successful fstatx/lstatx plus filesec_query_property explicitly reports
    // ACL presence. Only a known absence is supported; ANY present ACL refuses.
    let security = Security(
        std::ptr::NonNull::new(unsafe { filesec_init() })
            .context("native security allocation failed")?,
    );
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let status = match fd {
        Some(file) => unsafe {
            fstatx_np(file.as_raw_fd(), metadata.as_mut_ptr(), security.0.as_ptr())
        },
        None => unsafe { lstatx_np(c"/etc".as_ptr(), metadata.as_mut_ptr(), security.0.as_ptr()) },
    };
    ensure!(
        status == 0,
        "native control security observation failed: {}",
        std::io::Error::last_os_error()
    );
    let mut valid = -1;
    let queried = unsafe { filesec_query_property(security.0.as_ptr(), 5, &mut valid) };
    super::decision::no_acl(queried, valid)?;
    Ok(unsafe { metadata.assume_init() })
}
fn directory_stamp(file: &File) -> Result<Stamp> {
    crate::native_name_boundary::qualify_directory_handle(file)?;
    let stamp = Stamp::from(&security_observation(Some(file))?);
    protected_directory(&stamp)?;
    Ok(stamp)
}
fn open_root() -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open("/")
}
fn open_directory(parent: &File, name: &CStr) -> std::io::Result<File> {
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
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn metadata_at(parent: &File, name: &CStr) -> std::io::Result<libc::stat> {
    let mut result = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { result.assume_init() })
}
fn alias_stamp(root: &File) -> Result<Stamp> {
    let before = metadata_at(root, c"etc")?;
    let stamp = Stamp::from(&before);
    let mut target = [0u8; 32];
    let length = unsafe {
        libc::readlinkat(
            root.as_raw_fd(),
            c"etc".as_ptr(),
            target.as_mut_ptr().cast(),
            target.len(),
        )
    };
    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length < target.len())
        .context("system alias target unavailable or truncated")?;
    super::decision::alias(stamp.uid, stamp.mode, stamp.links, &target[..length])?;
    let security = Stamp::from(&security_observation(None)?);
    ensure!(
        security == stamp && Stamp::from(&metadata_at(root, c"etc")?) == stamp,
        "system control alias changed"
    );
    Ok(stamp)
}
pub(super) struct SystemEtc {
    root: File,
    private: File,
    target: File,
    stamps: [Stamp; 3],
    alias: Stamp,
    failed: AtomicBool,
}
impl SystemEtc {
    pub(super) fn capture() -> Result<Option<Self>> {
        let root = open_root()?;
        let before = metadata_at(&root, c"etc")?;
        if before.st_mode & libc::S_IFMT != libc::S_IFLNK {
            return Ok(None);
        }
        let root_stamp = directory_stamp(&root)?;
        let alias = alias_stamp(&root)?;
        let private = open_directory(&root, c"private")?;
        let private_stamp = directory_stamp(&private)?;
        let target = open_directory(&private, c"etc")?;
        let target_stamp = directory_stamp(&target)?;
        let value = Self {
            root,
            private,
            target,
            stamps: [root_stamp, private_stamp, target_stamp],
            alias,
            failed: AtomicBool::new(false),
        };
        value.recheck()?;
        Ok(Some(value))
    }
    pub(super) fn recheck(&self) -> Result<()> {
        super::decision::observe_once(&self.failed, || self.recheck_once())
    }
    fn recheck_once(&self) -> Result<()> {
        ensure!(
            directory_stamp(&self.root)? == self.stamps[0]
                && directory_stamp(&self.private)? == self.stamps[1]
                && directory_stamp(&self.target)? == self.stamps[2],
            "held system control anchor changed"
        );
        let root = open_root()?;
        let private = open_directory(&root, c"private")?;
        let target = open_directory(&private, c"etc")?;
        ensure!(
            directory_stamp(&root)? == self.stamps[0]
                && directory_stamp(&private)? == self.stamps[1]
                && directory_stamp(&target)? == self.stamps[2]
                && alias_stamp(&root)? == self.alias,
            "named system control anchor changed"
        );
        Ok(())
    }
    fn names(relative: &Path) -> Result<Vec<CString>> {
        ensure!(
            relative.as_os_str().len() <= 4096 && relative.components().count() <= 256,
            "control suffix exceeds bound"
        );
        relative
            .components()
            .map(|part| {
                let Component::Normal(name) = part else {
                    anyhow::bail!("invalid control suffix");
                };
                Ok(CString::new(name.as_bytes())?)
            })
            .collect()
    }
    pub(super) fn qualify(&self, relative: &Path, directory: bool) -> Result<()> {
        self.recheck()?;
        let mut names = Self::names(relative)?;
        if !directory {
            names.pop();
        }
        let mut parent = self.target.try_clone()?;
        for name in names {
            match open_directory(&parent, &name) {
                Ok(next) => {
                    crate::native_name_boundary::qualify_directory_handle(&next)?;
                    parent = next;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        self.recheck()
    }
    pub(super) fn metadata(&self, relative: &Path) -> std::io::Result<Metadata> {
        self.recheck().map_err(std::io::Error::other)?;
        let result = (|| -> Result<Metadata> {
            let mut names = Self::names(relative)?;
            let Some(leaf) = names.pop() else {
                return Ok(self.target.metadata()?.into());
            };
            let mut parent = self.target.try_clone()?;
            for name in names {
                let next = open_directory(&parent, &name)?;
                crate::native_name_boundary::qualify_directory_handle(&next)?;
                parent = next;
            }
            let value = metadata_at(&parent, &leaf)?;
            Ok(Metadata {
                file: value.st_mode & libc::S_IFMT == libc::S_IFREG,
                directory: value.st_mode & libc::S_IFMT == libc::S_IFDIR,
                symlink: value.st_mode & libc::S_IFMT == libc::S_IFLNK,
                bytes: value
                    .st_size
                    .try_into()
                    .context("negative control length")?,
            })
        })();
        self.recheck().map_err(std::io::Error::other)?;
        result.map_err(|error| match error.downcast::<std::io::Error>() {
            Ok(io) => io,
            Err(other) => std::io::Error::other(other),
        })
    }
    pub(super) fn read(
        &self,
        relative: &Path,
        original: &Path,
        limit: usize,
        remaining: usize,
    ) -> Result<Vec<u8>> {
        self.recheck()?;
        let result =
            crate::source_read::read_control_at(&self.target, relative, original, limit, remaining);
        match (result, self.recheck()) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(recheck)) => {
                Err(error.context(format!("control anchor also changed: {recheck:#}")))
            }
        }
    }
}
#[cfg(test)]
mod tests;
