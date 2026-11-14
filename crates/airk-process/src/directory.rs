// SPDX-License-Identifier: Apache-2.0
//! Opened directory objects and raw native identities. No ownership policy,
//! record serialization or path-derived authority lives here. Explicit raw
//! directory flush/disposition operations retain their own narrow guarantees.
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod mutation;
mod pending;
pub use pending::PendingFileProof;
pub mod publication;
#[cfg(windows)]
mod streams;
#[cfg(windows)]
pub use streams::require_default_stream;

/// Native identity facts from an open handle, without a persisted encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectIdentity {
    Windows {
        volume_serial: u64,
        file_id: [u8; 16],
    },
    Unix {
        device: u64,
        inode: u64,
    },
}

/// Observe an already-open object. This does not assert how its caller opened
/// it, whether a path still names it, or whether the user owns it.
pub fn identity(file: &File) -> io::Result<ObjectIdentity> {
    platform::identity(file)
}

#[derive(Debug)]
struct Node {
    file: File,
    // Windows must retain every ancestor's no-delete-share handle. Unix also
    // keeps this chain, but directory renames remain possible there.
    _parent: Option<Arc<Node>>,
}

/// A directory opened without following any path component's redirection.
/// Children open relative to its pinned object on Unix. Windows retains
/// no-delete-share handles for every component to pin their names as well.
#[derive(Clone, Debug)]
pub struct OpenedDirectory {
    node: Arc<Node>,
    path: PathBuf,
}

#[cfg(windows)]
struct PendingRemoval {
    file: File,
    restore: bool,
}
#[cfg(windows)]
impl Drop for PendingRemoval {
    fn drop(&mut self) {
        if self.restore {
            // Restore on unwind, and retry if an explicit cancellation failed.
            // A hard OS failure remains an unknown outcome, never proof that
            // foreign bytes were preserved.
            let _ = platform::set_disposition(&self.file, false);
        }
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn split_absolute(path: &Path) -> io::Result<(PathBuf, Vec<OsString>)> {
    if !path.is_absolute() {
        return Err(invalid("directory path must be absolute"));
    }
    let mut root = PathBuf::new();
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            #[cfg(windows)]
            Component::Prefix(prefix) => {
                if !matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "directory pinning requires a local drive path",
                    ));
                }
                root.push(prefix.as_os_str());
            }
            Component::RootDir => root.push(component.as_os_str()),
            Component::Normal(name) => names.push(name.to_owned()),
            _ => return Err(invalid("directory path contains a non-normal component")),
        }
    }
    if !root.is_absolute() {
        return Err(invalid("directory path has no supported root"));
    }
    Ok((root, names))
}

pub fn validate_child_name(name: &OsStr) -> io::Result<()> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(invalid("directory child must be one normal component"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        if name.encode_wide().any(|character| {
            [0, u16::from(b':'), u16::from(b'/'), u16::from(b'\\')].contains(&character)
        }) {
            return Err(invalid(
                "directory child contains NUL or an alternate stream",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        if name
            .as_bytes()
            .iter()
            .any(|byte| *byte == 0 || *byte == b'/')
        {
            return Err(invalid("directory child contains NUL or a separator"));
        }
    }
    Ok(())
}

fn require_directory(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(invalid("redirected directory refused"));
        }
    }
    if !metadata.is_dir() {
        return Err(invalid("object is not a directory"));
    }
    Ok(())
}

impl OpenedDirectory {
    pub fn open(path: &Path) -> io::Result<Self> {
        let (root, names) = split_absolute(path)?;
        let file = platform::open_root(&root)?;
        require_directory(&file)?;
        let mut directory = Self {
            node: Arc::new(Node {
                file,
                _parent: None,
            }),
            path: root,
        };
        for name in names {
            directory = directory.open_child(&name)?;
        }
        Ok(directory)
    }

    pub fn open_child(&self, name: &OsStr) -> io::Result<Self> {
        validate_child_name(name)?;
        let path = self.path.join(name);
        let file = platform::open_child(&self.node.file, name, &path)?;
        require_directory(&file)?;
        Ok(Self {
            node: Arc::new(Node {
                file,
                _parent: Some(Arc::clone(&self.node)),
            }),
            path,
        })
    }

    pub fn identity(&self) -> io::Result<ObjectIdentity> {
        identity(&self.node.file)
    }

    /// The pinned object for handle-based native policy checks. No named path
    /// returned here can be mistaken for a continuing authority guarantee.
    pub fn as_file(&self) -> &File {
        &self.node.file
    }

    /// Flush changes to this exact directory object. Windows acquires a
    /// separate write-capable handle while the original ancestor pins remain
    /// live. Permission and unsupported-filesystem errors are returned.
    pub fn sync_all(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.node.file.sync_all()
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::{
                Foundation::GENERIC_WRITE,
                Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
                    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
                },
            };
            let writable = std::fs::OpenOptions::new()
                .write(true)
                .access_mode(
                    GENERIC_WRITE | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL,
                )
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&self.path)?;
            require_directory(&writable)?;
            if identity(&writable)? != self.identity()? {
                return Err(invalid("directory changed before flush"));
            }
            writable.sync_all()
        }
    }

    /// Remove one empty directory only if the opened object has the exact
    /// expected native identity. Unix has no qualified atomic identity-bound
    /// primitive here and returns Unsupported without attempting unlinkat.
    pub fn remove_child_if_matches(
        &self,
        name: &OsStr,
        expected: ObjectIdentity,
    ) -> io::Result<()> {
        validate_child_name(name)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
                FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                READ_CONTROL,
            };
            let child = std::fs::OpenOptions::new()
                .read(true)
                .access_mode(DELETE | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))?;
            require_directory(&child)?;
            if identity(&child)? != expected {
                return Err(invalid("directory removal identity mismatch"));
            }
            platform::dispose(&child)?;
            drop(child);
            self.sync_all()
        }
        #[cfg(unix)]
        {
            let _ = expected;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "atomic identity-bound directory removal is unqualified on Unix",
            ))
        }
    }

    /// Windows-only exact opened-file removal. The caller checks ownership,
    /// aliases and content using the stable open object; false preserves it.
    pub fn remove_file_if(
        &self,
        name: &OsStr,
        predicate: impl FnOnce(&mut File, &PendingFileProof) -> io::Result<bool>,
    ) -> io::Result<bool> {
        validate_child_name(name)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
            };
            let file = std::fs::OpenOptions::new()
                .read(true)
                .access_mode(DELETE | FILE_GENERIC_READ)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))?;
            mutation::require_file(&file)?;
            let original_identity = identity(&file)?;
            // Pending deletion freezes every stream name; default-stream share
            // exclusion alone does not prevent a concurrent new ADS.
            platform::set_disposition(&file, true)?;
            let mut pending = PendingRemoval {
                file,
                restore: true,
            };
            let checked = (|| {
                let proof = PendingFileProof::new(&pending.file, original_identity)?;
                self.require_default_stream(name, &pending.file)?;
                predicate(&mut pending.file, &proof)
            })();
            match checked {
                Ok(true) => {}
                other => {
                    platform::set_disposition(&pending.file,false).map_err(|restore|io::Error::new(restore.kind(),format!("file deletion refusal could not restore pending state: {restore}; original result: {other:?}")))?;
                    pending.restore = false;
                    return other;
                }
            }
            pending.restore = false;
            drop(pending);
            self.sync_all()?;
            Ok(true)
        }
        #[cfg(unix)]
        {
            let _ = predicate;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "exact opened-file disposition is unqualified on Unix",
            ))
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        ffi::CString,
        fs::OpenOptions,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
    };

    pub(super) fn open_root(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }

    pub(super) fn open_child(parent: &File, name: &OsStr, _path: &Path) -> io::Result<File> {
        let name =
            CString::new(name.as_bytes()).map_err(|_| invalid("directory child contains NUL"))?;
        // SAFETY: parent is an owned open directory; name is one NUL-terminated
        // component, flags do not create anything, and a fresh fd is returned.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openat returned an unowned fresh descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    pub(super) fn identity(file: &File) -> io::Result<ObjectIdentity> {
        let metadata = file.metadata()?;
        Ok(ObjectIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        fs::OpenOptions,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
        GetFileInformationByHandleEx, READ_CONTROL,
    };

    pub(super) fn open_root(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            // Metadata-only opens do not participate in Windows sharing
            // checks. Directory read access is required for the omitted
            // FILE_SHARE_DELETE to prevent a rename, including empty roots.
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    pub(super) fn open_child(_parent: &File, _name: &OsStr, path: &Path) -> io::Result<File> {
        // Every ancestor remains open without FILE_SHARE_DELETE, so this path
        // cannot be redirected through an ancestor rename or replacement.
        open_root(path)
    }

    pub(super) fn identity(file: &File) -> io::Result<ObjectIdentity> {
        let mut information = FILE_ID_INFO::default();
        // SAFETY: file is a live owned handle and information is initialized
        // output storage with the exact FILE_ID_INFO size supplied to Windows.
        let observed = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                (&mut information as *mut FILE_ID_INFO).cast(),
                size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if observed == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ObjectIdentity::Windows {
            volume_serial: information.VolumeSerialNumber,
            file_id: information.FileId.Identifier,
        })
    }
    pub(super) fn dispose(file: &File) -> io::Result<()> {
        set_disposition(file, true)
    }
    pub(super) fn set_disposition(file: &File, delete: bool) -> io::Result<()> {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        };
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: delete };
        // SAFETY: caller owns a DELETE-capable exact object handle; initialized
        // disposition structure with its documented size. No pathname removal.
        let result = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
