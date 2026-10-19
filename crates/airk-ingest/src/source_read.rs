// SPDX-License-Identifier: Apache-2.0
//! Read-only source handles with no-follow traversal, independent of discovery metadata.
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub(crate) fn read(root: &Path, path: &Path, limit: usize, remaining: usize) -> Result<Vec<u8>> {
    let relative = path
        .strip_prefix(root)
        .context("source escaped workspace")?;
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "invalid source path"
    );
    let mut held = open(root, relative)?;
    let before = held.file.metadata()?;
    ensure!(before.is_file(), "source is not a regular file");
    let mut bytes = Vec::new();
    held.file
        .by_ref()
        .take((limit + 1).min(remaining) as u64)
        .read_to_end(&mut bytes)?;
    let after = held.file.metadata()?;
    ensure!(
        before.len() == after.len()
            && before.modified()? == after.modified()?
            && unchanged(&before, &after),
        "source changed during scan; retry indexing"
    );
    // Check the pathname still names the held file, using the same protected traversal.
    let current = open(root, relative)?;
    ensure!(
        identity(&held.file)? == identity(&current.file)?,
        "source replaced during scan; retry indexing"
    );
    ensure!(
        bytes.len() as u64 == before.len() || bytes.len() > limit,
        "workspace raw source read budget exhausted; previous generation retained"
    );
    Ok(bytes)
}

struct Opened {
    file: File,
    // Windows directory handles deny deletion/rename while the source is read.
    // Unix openat traversal pins directory identities directly through handles.
    _directories: Vec<File>,
}

#[cfg(unix)]
fn open(root: &Path, relative: &Path) -> Result<Opened> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };
    ensure!(root.is_absolute(), "source root must be absolute");
    let mut directories = vec![
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?,
    ];
    let joined = root.join(relative);
    let components: Vec<_> = joined
        .components()
        .filter(|part| !matches!(part, Component::RootDir))
        .collect();
    for (index, part) in components.iter().enumerate() {
        let Component::Normal(name) = part else {
            anyhow::bail!("invalid source traversal");
        };
        let name = CString::new(name.as_bytes())?;
        let directory = index + 1 != components.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: parent and name remain valid; successful openat returns an owned descriptor.
        let fd = unsafe {
            libc::openat(
                directories.last().unwrap().as_raw_fd(),
                name.as_ptr(),
                flags,
            )
        };
        ensure!(
            fd >= 0,
            "open source without following links: {}",
            std::io::Error::last_os_error()
        );
        let file = unsafe { File::from_raw_fd(fd) };
        if directory {
            directories.push(file);
        } else {
            return Ok(Opened {
                file,
                _directories: directories,
            });
        }
    }
    anyhow::bail!("source file path is empty")
}

#[cfg(windows)]
fn open(root: &Path, relative: &Path) -> Result<Opened> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    ensure!(root.is_absolute(), "source root must be absolute");
    let joined = root.join(relative);
    let parts: Vec<_> = joined.components().collect();
    let mut current = std::path::PathBuf::new();
    let mut directories = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        ensure!(
            !matches!(part, Component::ParentDir | Component::CurDir),
            "invalid source traversal"
        );
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let directory = index + 1 != parts.len();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 })
            .custom_flags(
                FILE_FLAG_OPEN_REPARSE_POINT
                    | if directory {
                        FILE_FLAG_BACKUP_SEMANTICS
                    } else {
                        0
                    },
            )
            .open(&current)
            .context("open source through pinned directories")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "source traversal encountered a reparse point"
        );
        ensure!(
            if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
            "source traversal changed type"
        );
        if directory {
            directories.push(file);
        } else {
            return Ok(Opened {
                file,
                _directories: directories,
            });
        }
    }
    anyhow::bail!("source file path is empty")
}

#[cfg(unix)]
fn identity(file: &File) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn identity(file: &File) -> Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: valid owned handle and initialized writable information buffer.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } != 0,
        "read source handle identity: {}",
        std::io::Error::last_os_error()
    );
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(unix)]
fn unchanged(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec()
}
#[cfg(windows)]
fn unchanged(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_sources_are_bounded_and_ancestors_cannot_redirect_reads() -> Result<()> {
        let source = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(root.join("safe.txt"), b"source")?;
        std::fs::write(outside.path().join("secret.txt"), b"not source")?;
        assert_eq!(read(&root, &root.join("safe.txt"), 16, 17)?, b"source");
        assert!(read(&root, &root.join("safe.txt"), 16, 3).is_err());
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.join("redirected"))?;
        #[cfg(windows)]
        {
            // Junctions need no symlink privilege. The command receives only fixture paths.
            let status = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(root.join("redirected"))
                .arg(outside.path())
                .output()?;
            ensure!(status.status.success(), "junction fixture creation failed");
        }
        assert!(read(&root, &root.join("redirected/secret.txt"), 16, 17).is_err());
        assert_eq!(
            std::fs::read(outside.path().join("secret.txt"))?,
            b"not source"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn held_windows_source_denies_write_and_directory_replacement() -> Result<()> {
        let source = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::create_dir(root.join("nested"))?;
        std::fs::write(root.join("nested/source.txt"), b"stable")?;
        let held = open(&root, Path::new("nested/source.txt"))?;
        assert!(std::fs::write(root.join("nested/source.txt"), b"changed").is_err());
        assert!(std::fs::rename(root.join("nested"), root.join("replaced")).is_err());
        drop(held);
        std::fs::write(root.join("nested/source.txt"), b"changed")?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn held_unix_source_does_not_follow_replaced_path() -> Result<()> {
        let source = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(root.join("source.txt"), b"stable")?;
        std::fs::write(outside.path().join("secret.txt"), b"outside")?;
        let mut held = open(&root, Path::new("source.txt"))?;
        std::fs::rename(root.join("source.txt"), root.join("original.txt"))?;
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), root.join("source.txt"))?;
        let mut bytes = Vec::new();
        held.file.read_to_end(&mut bytes)?;
        assert_eq!(bytes, b"stable");
        assert!(open(&root, Path::new("source.txt")).is_err());
        Ok(())
    }
}
