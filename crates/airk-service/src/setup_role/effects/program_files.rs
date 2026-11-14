// SPDX-License-Identifier: Apache-2.0
//! Remove approved payload names only inside the original opened program root.
use airk_core::identity::Sha256Digest;
use airk_process::directory::OpenedDirectory;
use anyhow::{Result, ensure};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    Absent,
    Changed,
    Removed,
}

fn matches(file: &mut File, digest: Sha256Digest, bytes: Option<u64>) -> io::Result<bool> {
    use sha2::{Digest, Sha256};
    let limit =
        bytes.unwrap_or(airk_core::install_record::payload_inventory::MAX_INVENTORY_BYTES as u64);
    let length = file.metadata()?.len();
    if length > limit || bytes.is_some_and(|bytes| bytes != length) {
        return Ok(false);
    }
    let mut hash = Sha256::new();
    let read = io::copy(&mut file.take(limit.saturating_add(1)), &mut hash)?;
    Ok(read == length && Sha256Digest::from_bytes(hash.finalize().into()) == digest)
}

fn parent(root: &OpenedDirectory, relative: &str) -> Result<(OpenedDirectory, String)> {
    ensure!(
        airk_core::install_record::payload_inventory::relative_inside(relative),
        "program removal path is invalid"
    );
    let mut parts = relative.split('/').peekable();
    let mut parent = root.clone();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return Ok((parent, part.to_owned()));
        }
        parent = parent.open_child(OsStr::new(part))?;
    }
    anyhow::bail!("program removal path is empty")
}

pub(crate) fn remove(
    root: &OpenedDirectory,
    relative: &str,
    digest: Sha256Digest,
    bytes: Option<u64>,
) -> Result<Removal> {
    let (parent, name) = match parent(root, relative) {
        Ok(parent) => parent,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            return Ok(Removal::Absent);
        }
        Err(error) => return Err(error),
    };
    #[cfg(windows)]
    let outcome = parent.remove_file_if(OsStr::new(&name), |file, proof| {
        airk_ipc::endpoint::require_private_pending_metadata_handle(file, proof)
            .map_err(|error| io::Error::other(format!("{error:#}")))?;
        parent.require_default_stream(OsStr::new(&name), file)?;
        matches(file, digest, bytes)
    });
    #[cfg(unix)]
    let outcome = unix(&parent, &name, digest, bytes);
    match outcome {
        Ok(true) => Ok(Removal::Removed),
        Ok(false) => Ok(Removal::Changed),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Removal::Absent),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn unix(
    parent: &OpenedDirectory,
    name: &str,
    digest: Sha256Digest,
    bytes: Option<u64>,
) -> io::Result<bool> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::MetadataExt,
        },
    };
    let name = CString::new(name).map_err(io::Error::other)?;
    // SAFETY: held original directory fd, one terminated child, nofollow flags.
    let fd = unsafe {
        libc::openat(
            parent.as_file().as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat returned an unowned fresh descriptor.
    let mut file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 || !matches(&mut file, digest, bytes)? {
        return Ok(false);
    }
    // Approved tracked name inside the original parent object; not an atomic
    // digest+unlink claim against arbitrary same-user writers on Unix.
    // SAFETY: unlink one child of the held parent; never follows its target.
    if unsafe { libc::unlinkat(parent.as_file().as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    parent.sync_all()?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) fn empty_folder(root: &OpenedDirectory, relative: &str) -> Result<()> {
    let (parent, name) = parent(root, relative)?;
    let expected = parent.open_child(OsStr::new(&name))?.identity()?;
    parent.remove_child_if_matches(OsStr::new(&name), expected)?;
    Ok(())
}

#[cfg(test)]
mod tests;
