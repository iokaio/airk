// SPDX-License-Identifier: Apache-2.0
//! Same-user native endpoints. Authentication occurs before deserializing messages.
use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(windows)]
#[path = "local_ipc_windows.rs"]
mod platform;
#[cfg(unix)]
#[path = "local_ipc_unix.rs"]
mod platform;
pub use platform::{Listener, Stream, connect, current_user, prepare_root};
#[cfg(windows)]
pub(crate) use platform::{create_root, process_user, require_owner_private};

/// Stable counter identity for the OS-authenticated user; not a bearer credential.
pub fn caller_identity() -> Result<airk_core::identity::Incarnation> {
    let hash = Sha256::digest(current_user()?.as_bytes());
    Ok(airk_core::identity::Incarnation::from_bytes(
        hash[..16].try_into()?,
    ))
}

#[derive(Debug)]
pub struct PeerRejected;
impl std::fmt::Display for PeerRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native peer rejected")
    }
}
impl std::error::Error for PeerRejected {}

pub fn endpoint(root: &Path, role: &str) -> Result<String> {
    ensure!(
        !role.is_empty()
            && role.len() <= 64
            && role.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid native endpoint role"
    );
    let root = root.canonicalize()?;
    #[cfg(windows)]
    {
        let mut hash = Sha256::new();
        hash.update(current_user()?.as_bytes());
        hash.update(root.as_os_str().as_encoded_bytes());
        hash.update(role.as_bytes());
        Ok(format!(r"\\.\pipe\airk-1.4-{:x}", hash.finalize()))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = unix_socket_root(&root)?.join(format!("{role}.sock"));
        ensure!(
            path.as_os_str().as_bytes().len() <= 100,
            "native socket path exceeds portable bound"
        );
        Ok(path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("runtime socket path is not UTF-8"))?
            .into())
    }
}

#[cfg(unix)]
fn unix_socket_root(root: &Path) -> Result<std::path::PathBuf> {
    use std::io::{Read, Write};
    let mut hash = Sha256::new();
    hash.update(current_user()?.as_bytes());
    hash.update([0]);
    hash.update(root.as_os_str().as_encoded_bytes());
    let identity = format!("{:x}", hash.finalize());
    let base = if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        let xdg = std::path::PathBuf::from(xdg);
        if xdg.is_absolute() && xdg.exists() && prepare_root(&xdg).is_ok() {
            xdg
        } else {
            std::path::PathBuf::from("/tmp")
        }
    } else {
        std::path::PathBuf::from("/tmp")
    };
    use std::os::unix::ffi::OsStrExt;
    let candidate = base.join(format!("ak-{}", &identity[..16]));
    let directory = if candidate.as_os_str().as_bytes().len() <= 40 {
        candidate
    } else {
        std::path::PathBuf::from("/tmp").join(format!("ak-{}", &identity[..16]))
    };
    prepare_root(&directory)?;
    let receipt = directory.join("identity");
    if !receipt.try_exists()? {
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(identity.as_bytes())?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&receipt) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error.into()),
        }
    }
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(&receipt)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.nlink() == 1
            && metadata.len() <= 64,
        "invalid native socket namespace receipt"
    );
    let mut saved = String::new();
    std::fs::File::open(receipt)?
        .take(65)
        .read_to_string(&mut saved)?;
    ensure!(
        saved == identity,
        "socket namespace belongs to another runtime"
    );
    Ok(directory)
}

/// Only the elected service owner may replace its fixed stale control socket.
pub fn prepare_control_endpoint(lease: &airk_store::service::ServiceLease) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let path = endpoint(lease.root(), "control")?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_socket()
                        && metadata.nlink() == 1
                        && metadata.uid().to_string() == current_user()?,
                    "unexpected service control entry"
                );
                std::fs::remove_file(path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(not(unix))]
    let _ = lease;
    Ok(())
}

/// Called only before allocating any new children, after predecessor OS leases release.
/// Unknown entries survive. A pass removes at most 64 recognized stale sockets.
pub(crate) fn cleanup_predecessor_endpoints(root: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        let directory = unix_socket_root(&root.canonicalize()?)?;
        let mut removed = 0;
        for entry in std::fs::read_dir(directory)?.take(1024) {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(role) = name.strip_suffix(".sock") else {
                continue;
            };
            let recognized = (role.starts_with("q-") || role.starts_with("e-"))
                && role.len() == 34
                && role[2..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if recognized {
                remove_exited_endpoint(root, role)?;
                removed += 1;
                if removed == 64 {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        Ok(true)
    }
}

/// The owning adapter must prove actual process exit before invoking this function.
pub(crate) fn remove_exited_endpoint(root: &Path, role: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        ensure!(
            role.starts_with("q-") || role.starts_with("e-"),
            "invalid child endpoint role"
        );
        let path = endpoint(root, role)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_socket()
                        && metadata.nlink() == 1
                        && metadata.uid().to_string() == current_user()?,
                    "foreign child endpoint entry"
                );
                std::fs::remove_file(path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(not(unix))]
    let _ = (root, role);
    Ok(())
}

/// One deadline covers a whole frame, including a peer that trickles its bytes.
pub async fn read_message<S: AsyncRead + Unpin, T: DeserializeOwned>(
    stream: &mut S,
    deadline: Duration,
) -> Result<Option<T>> {
    read_message_limited(stream, deadline, airk_core::service::MAX_FRAME_BYTES).await
}

pub async fn read_message_limited<S: AsyncRead + Unpin, T: DeserializeOwned>(
    stream: &mut S,
    deadline: Duration,
    limit: usize,
) -> Result<Option<T>> {
    ensure!(
        (1..=4 * 1024 * 1024).contains(&limit),
        "invalid native frame budget"
    );
    tokio::time::timeout(deadline, async {
        let mut header = [0; 4];
        let count = stream.read(&mut header[..1]).await?;
        if count == 0 {
            return Ok(None);
        }
        stream.read_exact(&mut header[1..]).await?;
        let bytes = u32::from_be_bytes(header) as usize;
        ensure!(bytes > 0 && bytes <= limit, "invalid native frame length");
        let mut payload = vec![0; bytes];
        stream.read_exact(&mut payload).await?;
        Ok(Some(serde_json::from_slice(&payload)?))
    })
    .await
    .map_err(|_| anyhow::anyhow!("native frame read deadline exceeded"))?
}

pub async fn write_message<S: AsyncWrite + Unpin, T: Serialize>(
    stream: &mut S,
    value: &T,
    deadline: Duration,
) -> Result<()> {
    write_message_limited(stream, value, deadline, airk_core::service::MAX_FRAME_BYTES).await
}

pub async fn write_message_limited<S: AsyncWrite + Unpin, T: Serialize>(
    stream: &mut S,
    value: &T,
    deadline: Duration,
    limit: usize,
) -> Result<()> {
    let mut frame = Vec::new();
    crate::service_ipc::write_frame_limited(&mut frame, value, limit)?;
    tokio::time::timeout(deadline, async {
        stream.write_all(&frame).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| anyhow::anyhow!("native frame write deadline exceeded"))??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_user_endpoint_rejects_competing_listener_and_exchanges_bounded_messages()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("private");
        prepare_root(&root)?;
        let address = endpoint(&root, "test")?;
        let mut listener = Listener::bind(&address)?;
        assert!(Listener::bind(&address).is_err());
        let address_copy = address.clone();
        let client = tokio::spawn(async move {
            let mut stream = connect(&address_copy).await?;
            write_message(&mut stream, &vec![1u32, 2, 3], Duration::from_secs(5)).await?;
            let response: Option<Vec<u32>> =
                read_message(&mut stream, Duration::from_secs(5)).await?;
            ensure!(response == Some(vec![3, 2, 1]), "native response mismatch");
            Ok::<_, anyhow::Error>(())
        });
        let mut stream = listener.accept().await?;
        let request: Option<Vec<u32>> = read_message(&mut stream, Duration::from_secs(5)).await?;
        assert_eq!(request, Some(vec![1, 2, 3]));
        write_message(&mut stream, &vec![3u32, 2, 1], Duration::from_secs(5)).await?;
        client.await??;
        Ok(())
    }

    #[tokio::test]
    async fn malicious_frame_and_stalled_peer_fail_within_admission() -> Result<()> {
        let (mut sender, mut receiver) = tokio::io::duplex(32);
        sender.write_all(&u32::MAX.to_be_bytes()).await?;
        assert!(
            read_message::<_, String>(&mut receiver, Duration::from_millis(100))
                .await
                .is_err()
        );
        let (_sender, mut receiver) = tokio::io::duplex(32);
        assert!(
            read_message::<_, String>(&mut receiver, Duration::from_millis(10))
                .await
                .is_err()
        );
        Ok(())
    }
}
