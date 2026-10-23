// SPDX-License-Identifier: Apache-2.0
use anyhow::{Result, ensure};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::Path,
};
use tokio::net::{UnixListener, UnixStream};

pub type Stream = UnixStream;

pub fn current_user() -> Result<String> {
    // SAFETY: geteuid takes no pointers and has no failure mode.
    Ok(unsafe { libc::geteuid() }.to_string())
}

pub fn prepare_root(root: &Path) -> Result<()> {
    if !root.try_exists()? {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
    }
    ensure!(
        !fs::symlink_metadata(root)?.file_type().is_symlink(),
        "redirected native runtime directory"
    );
    // Resolve host-provided parent aliases (including macOS /var) once, then
    // require every actual ancestor to be root/user owned and protected from rename.
    let canonical = root.canonicalize()?;
    // SAFETY: geteuid has no pointer arguments.
    let uid = unsafe { libc::geteuid() };
    for ancestor in canonical.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        let mode = metadata.permissions().mode();
        ensure!(
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && (metadata.uid() == uid || metadata.uid() == 0)
                && (mode & 0o022 == 0 || (metadata.uid() == 0 && mode & 0o1000 != 0)),
            "unsafe native runtime ancestor"
        );
    }
    let metadata = fs::symlink_metadata(root)?;
    // SAFETY: geteuid has no pointer arguments.
    ensure!(
        metadata.is_dir()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.permissions().mode() & 0o077 == 0,
        "native runtime directory must be owner-only"
    );
    Ok(())
}

pub struct Listener {
    listener: UnixListener,
    path: std::path::PathBuf,
    inode: (u64, u64),
}

impl Drop for Listener {
    fn drop(&mut self) {
        use std::os::unix::fs::FileTypeExt;
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.file_type().is_socket()
            && (metadata.dev(), metadata.ino()) == self.inode
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Listener {
    pub fn bind(address: &str) -> Result<Self> {
        prepare_root(
            Path::new(address)
                .parent()
                .ok_or_else(|| anyhow::anyhow!("socket parent missing"))?,
        )?;
        // Existing sockets are never unlinked on timeout or failed startup.
        let listener = UnixListener::bind(address)?;
        let metadata = fs::symlink_metadata(address)?;
        Ok(Self {
            listener,
            path: address.into(),
            inode: (metadata.dev(), metadata.ino()),
        })
    }
    pub async fn accept(&mut self) -> Result<Stream> {
        let (stream, _) = self.listener.accept().await?;
        // SAFETY: geteuid has no pointer arguments.
        if stream.peer_cred().map_err(|_| super::PeerRejected)?.uid() != unsafe { libc::geteuid() }
        {
            return Err(super::PeerRejected.into());
        }
        Ok(stream)
    }
}

pub async fn connect(address: &str) -> Result<Stream> {
    prepare_root(
        Path::new(address)
            .parent()
            .ok_or_else(|| anyhow::anyhow!("socket parent missing"))?,
    )?;
    let stream = UnixStream::connect(address).await?;
    // SAFETY: geteuid has no pointer arguments.
    ensure!(
        stream.peer_cred()?.uid() == unsafe { libc::geteuid() },
        "foreign native server"
    );
    Ok(stream)
}
