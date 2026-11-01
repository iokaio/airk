// SPDX-License-Identifier: Apache-2.0
//! Positive handle evidence only; never grants cached/relaxed read authority.
use anyhow::Result;
#[cfg(any(target_os = "linux", windows))]
use sha2::{Digest, Sha256};
use std::fs::File;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    RootDirectory,
    Directory,
    File,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unknown {
    UnsupportedPlatform,
    UnsupportedFilesystem,
    UnsupportedAcl,
    NotPrivate,
    RedirectedOrAliased,
    Changed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Evidence {
    identity: [u64; 3],
    security: [u8; 32],
    kind: Kind,
}

/// Non-Clone, non-serializable, borrowed from the actual inspected handle.
/// It certifies this object, not a pathname, tree census, writer provenance or
/// future files. No constructor accepts a serialized eligibility claim.
pub(crate) struct PrivateObject<'a> {
    file: &'a File,
    evidence: Evidence,
}
pub(crate) enum Observation<'a> {
    Private(PrivateObject<'a>),
    Unknown(Unknown),
}
impl PrivateObject<'_> {
    pub(crate) fn recheck(&self) -> Result<std::result::Result<(), Unknown>> {
        Ok(match platform(self.file, self.evidence.kind)? {
            Ok(current) if current == self.evidence => Ok(()),
            Ok(_) => Err(Unknown::Changed),
            Err(reason) => Err(reason),
        })
    }
    pub(crate) fn identity(&self) -> [u64; 3] {
        self.evidence.identity
    }
    /// Caller obtains current through its existing no-follow namespace guard.
    /// A pinned old handle staying private does not authenticate a replaced path.
    pub(crate) fn check_current(&self, current: &File) -> Result<bool> {
        Ok(
            matches!(platform(current, self.evidence.kind)?, Ok(evidence) if evidence == self.evidence),
        )
    }
}
pub(crate) fn inspect(file: &File, kind: Kind) -> Result<Observation<'_>> {
    Ok(match platform(file, kind)? {
        Ok(evidence) => Observation::Private(PrivateObject { file, evidence }),
        Err(reason) => Observation::Unknown(reason),
    })
}

#[cfg(target_os = "linux")]
fn platform(file: &File, kind: Kind) -> Result<std::result::Result<Evidence, Unknown>> {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let before = file.metadata()?;
    // SAFETY: fstatfs writes exactly one initialized output; file owns its fd.
    let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(file.as_raw_fd(), filesystem.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let filesystem = unsafe { filesystem.assume_init() };
    // Narrow supported local Linux filesystems; network/FUSE/unknown stays Unknown.
    if ![0xef53, 0x58465342, 0x9123683e, 0x01021994].contains(&i128::from(filesystem.f_type)) {
        return Ok(Err(Unknown::UnsupportedFilesystem));
    }
    let directory = kind != Kind::File;
    // SAFETY: geteuid has no pointers or failure mode.
    if before.uid() != unsafe { libc::geteuid() }
        || (directory && !before.is_dir())
        || (!directory && !before.is_file())
        || before.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
    {
        return Ok(Err(Unknown::NotPrivate));
    }
    if !directory && before.nlink() != 1 {
        return Ok(Err(Unknown::RedirectedOrAliased));
    }
    for name in [
        b"system.posix_acl_access\0".as_slice(),
        b"system.posix_acl_default\0".as_slice(),
    ] {
        if !directory && name == b"system.posix_acl_default\0" {
            continue;
        }
        // SAFETY: terminated attribute name and owned fd; null buffer requests size only.
        let size = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr().cast(),
                std::ptr::null_mut(),
                0,
            )
        };
        if size >= 0 {
            return Ok(Err(Unknown::UnsupportedAcl));
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENODATA) {
            return if error.raw_os_error() == Some(libc::ENOTSUP) {
                Ok(Err(Unknown::UnsupportedAcl))
            } else {
                Err(error.into())
            };
        }
    }
    let after = file.metadata()?;
    if (
        before.dev(),
        before.ino(),
        before.uid(),
        before.mode(),
        before.nlink(),
    ) != (
        after.dev(),
        after.ino(),
        after.uid(),
        after.mode(),
        after.nlink(),
    ) {
        return Ok(Err(Unknown::Changed));
    }
    let mut security = Sha256::new();
    security.update(b"airk.private-object.linux.v1\0");
    security.update(before.uid().to_le_bytes());
    security.update(before.mode().to_le_bytes());
    Ok(Ok(Evidence {
        identity: [before.dev(), before.ino(), 0],
        security: security.finalize().into(),
        kind,
    }))
}

#[cfg(windows)]
#[path = "private_storage_windows.rs"]
mod windows;
#[cfg(windows)]
fn platform(file: &File, kind: Kind) -> Result<std::result::Result<Evidence, Unknown>> {
    windows::inspect(file, kind)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn platform(_file: &File, _kind: Kind) -> Result<std::result::Result<Evidence, Unknown>> {
    Ok(Err(Unknown::UnsupportedPlatform))
}

#[cfg(windows)]
pub(crate) fn same_volume(left: &File, right: &File) -> bool {
    match (windows::local_volume(left), windows::local_volume(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Native root/creation-boundary policy, distinct from strict per-object census.
/// This handle proof never certifies unvisited descendants or historical bytes.
pub(crate) struct NativePrivateBoundary<'a> {
    file: &'a File,
    evidence: Evidence,
}
impl NativePrivateBoundary<'_> {
    pub(crate) fn identity(&self) -> [u64; 3] {
        self.evidence.identity
    }
    pub(crate) fn recheck(&self) -> Result<bool> {
        Ok(
            matches!(native_platform(self.file,self.evidence.kind)?,Ok(current) if current==self.evidence),
        )
    }
}
pub(crate) fn native_boundary(
    file: &File,
    kind: Kind,
) -> Result<Option<NativePrivateBoundary<'_>>> {
    Ok(native_platform(file, kind)?
        .ok()
        .map(|evidence| NativePrivateBoundary { file, evidence }))
}
fn native_platform(file: &File, kind: Kind) -> Result<std::result::Result<Evidence, Unknown>> {
    #[cfg(windows)]
    {
        windows::native_boundary(file, kind)
    }
    #[cfg(target_os = "linux")]
    {
        // Root mode0700/no-positive-ACL remains real boundary. Descendants are
        // admitted by its native producer policy, not by exact leaf mode here.
        platform(file, kind)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (file, kind);
        Ok(Err(Unknown::UnsupportedPlatform))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn mode_change_invalidates_borrowed_positive_evidence() -> Result<()> {
        let root = tempfile::tempdir()?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        let file = File::open(root.path())?;
        let Observation::Private(proof) = inspect(&file, Kind::RootDirectory)? else {
            anyhow::bail!("test filesystem lacks required positive ACL/filesystem evidence");
        };
        assert!(proof.recheck()?.is_ok());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o750))?;
        assert!(proof.recheck()?.is_err());
        Ok(())
    }
    #[test]
    fn default_acl_and_path_replacement_invalidate_positive_use() -> Result<()> {
        use std::os::fd::AsRawFd;
        let root = tempfile::tempdir()?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        let directory = File::open(root.path())?;
        let Observation::Private(proof) = inspect(&directory, Kind::RootDirectory)? else {
            anyhow::bail!("positive ACL test requires supported local Linux filesystem");
        };
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, permission) in [(1u16, 7u16), (4, 0), (32, 0)] {
            acl.extend(tag.to_le_bytes());
            acl.extend(permission.to_le_bytes());
            acl.extend(u32::MAX.to_le_bytes());
        }
        // SAFETY: owned descriptor, terminated xattr name and bounded ACL bytes.
        if unsafe {
            libc::fsetxattr(
                directory.as_raw_fd(),
                c"system.posix_acl_default".as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        assert!(
            proof.recheck()?.is_err(),
            "default inheritance must not stay eligible"
        );
        let path = root.path().join("state");
        fs::write(&path, b"first inert object")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        // The directory default ACL is minimal; remove it before file proof.
        if unsafe {
            libc::fremovexattr(directory.as_raw_fd(), c"system.posix_acl_default".as_ptr())
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let original = File::open(&path)?;
        let Observation::Private(proof) = inspect(&original, Kind::File)? else {
            anyhow::bail!("inert private file did not qualify");
        };
        fs::rename(&path, root.path().join("old-state"))?;
        fs::write(&path, b"replacement inert object")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert!(
            proof.recheck()?.is_ok(),
            "old pinned object still exists privately"
        );
        assert!(
            !proof.check_current(&File::open(&path)?)?,
            "old proof cannot authenticate new path object"
        );
        Ok(())
    }

    #[test]
    fn hardlink_is_never_positive_even_under_private_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("state");
        fs::write(&path, b"inert fixture")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        fs::hard_link(&path, root.path().join("alias"))?;
        assert!(matches!(
            inspect(&File::open(path)?, Kind::File)?,
            Observation::Unknown(_)
        ));
        Ok(())
    }
}

#[cfg(all(test, windows))]
pub(crate) fn test_private_root(path: &std::path::Path) -> Result<()> {
    windows::tests::private_root(path)
}

#[cfg(all(test, windows))]
pub(crate) fn test_foreign_readable_root(path: &std::path::Path) -> Result<()> {
    windows::tests::foreign_readable_root(path)
}
