// SPDX-License-Identifier: Apache-2.0
//! Same-user native endpoints. Authentication occurs before deserializing messages.
use anyhow::{Context, Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as platform;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use self::unix as platform;
pub use platform::require_existing_root;
pub use platform::{Listener, Stream, connect, current_user, prepare_root, require_owner_private};
#[cfg(windows)]
pub use platform::{create_root, process_user, require_job_user, require_not_writable_by_others};

#[cfg(windows)]
pub use platform::with_current_user_job_security;

/// Windows access rights that let a holder change a folder or what it holds:
/// write/add file, append/add subfolder, write extended attributes, delete
/// child, write attributes, delete, write DACL, write owner, and the generic
/// all/write and maximum-allowed requests.
const WRITE_CAPABLE_RIGHTS: u32 = 0x0002
    | 0x0004
    | 0x0010
    | 0x0040
    | 0x0100
    | 0x0001_0000
    | 0x0004_0000
    | 0x0008_0000
    | 0x0200_0000
    | 0x1000_0000
    | 0x4000_0000;

/// Pure rule for a location's parent folder (U39 "not writable by others"):
/// the allowed entry's `mask` grants another account (any SID other than the
/// user, SYSTEM or Administrators) a right that could change the folder.
/// Read, list, traverse and synchronize grants are accepted.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn foreign_write_grant(sid: &str, user: &str, mask: u32) -> bool {
    const SYSTEM: &str = "S-1-5-18";
    const ADMINISTRATORS: &str = "S-1-5-32-544";
    sid != user && sid != SYSTEM && sid != ADMINISTRATORS && mask & WRITE_CAPABLE_RIGHTS != 0
}

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

/// Only the elected service owner may replace its fixed stale control socket; the
/// caller passes the root of the service lease it holds.
pub fn prepare_control_endpoint(root: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let path = endpoint(root, "control")?;
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
    let _ = root;
    Ok(())
}

/// Called only before allocating any new children, after predecessor OS leases release.
/// Unknown entries survive. A pass removes at most 64 recognized stale sockets.
pub fn cleanup_predecessor_endpoints(root: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        let directory = unix_socket_root(&root.canonicalize()?)?;
        let mut removed = 0;
        for (count, entry) in std::fs::read_dir(directory)?.enumerate() {
            ensure!(
                count < 1024,
                "predecessor endpoint census exceeds complete scan bound"
            );
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(role) = name.strip_suffix(".sock") else {
                continue;
            };
            let recognized = predecessor_child_role(role);
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

#[cfg(any(unix, test))]
fn predecessor_child_role(role: &str) -> bool {
    (role.starts_with("q-") || role.starts_with("e-"))
        && role.len() == 34
        && role[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Retire only the receipt-bound namespace after the old service/children exit.
/// Unknown entries remain explicit blockers rather than becoming a deletion scope.
/// The caller passes the root of the service lease it holds.
pub fn retire_endpoint_namespace(root: &Path) -> Result<bool> {
    if !cleanup_predecessor_endpoints(root)? {
        return Ok(false);
    }
    prepare_control_endpoint(root)?;
    #[cfg(unix)]
    {
        let directory = unix_socket_root(&root.canonicalize()?)?;
        let entries = std::fs::read_dir(&directory)?
            .take(2)
            .collect::<std::io::Result<Vec<_>>>()?;
        ensure!(
            entries.len() == 1 && entries[0].file_name() == "identity",
            "unrecognized predecessor endpoint entries preserved"
        );
        std::fs::remove_file(directory.join("identity"))?;
        std::fs::remove_dir(directory)?;
    }
    Ok(true)
}

/// The owning adapter must prove actual process exit before invoking this function.
pub fn remove_exited_endpoint(root: &Path, role: &str) -> Result<()> {
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
    Ok(read_message_sized(stream, deadline, limit)
        .await?
        .map(|(value, _)| value))
}

/// As [`read_message_limited`], also returning the frame's payload length so
/// a reader can hold a decoded message to its own, smaller budget.
pub async fn read_message_sized<S: AsyncRead + Unpin, T: DeserializeOwned>(
    stream: &mut S,
    deadline: Duration,
    limit: usize,
) -> Result<Option<(T, usize)>> {
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
        Ok(Some((serde_json::from_slice(&payload)?, bytes)))
    })
    .await
    .context("native frame read deadline exceeded")?
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
    crate::frame::write_frame_limited(&mut frame, value, limit)?;
    tokio::time::timeout(deadline, async {
        stream.write_all(&frame).await?;
        stream.flush().await
    })
    .await
    .context("native frame write deadline exceeded")??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_parents_refuse_only_write_capable_grants_to_other_accounts() {
        const USER: &str = "S-1-5-21-1-2-3-1002";
        const SANDBOX: &str = "S-1-5-21-1-2-3-1011";
        // icacls RX (read, list, traverse, synchronize) and R.
        for read_only in [0x0012_00a9, 0x0012_0089] {
            assert!(!foreign_write_grant(SANDBOX, USER, read_only));
        }
        // icacls M, F, W, D, and single write-type rights.
        for write in [
            0x0013_01bf,
            0x001f_01ff,
            0x0010_0116,
            0x0001_0000,
            0x0002,
            0x0004,
            0x0040,
            0x0004_0000,
            0x0008_0000,
            0x1000_0000,
            0x4000_0000,
            0x0200_0000,
        ] {
            assert!(foreign_write_grant(SANDBOX, USER, write), "{write:#x}");
        }
        for trusted in [USER, "S-1-5-18", "S-1-5-32-544"] {
            assert!(!foreign_write_grant(trusted, USER, 0x001f_01ff));
        }
    }

    #[test]
    fn clean_break_endpoint_roles_require_exact_original_child_identity() {
        for prefix in ["q-", "e-"] {
            assert!(predecessor_child_role(&format!(
                "{prefix}{}",
                "a".repeat(32)
            )));
            assert!(!predecessor_child_role(&format!(
                "{prefix}{}",
                "a".repeat(31)
            )));
            assert!(!predecessor_child_role(&format!(
                "{prefix}{}",
                "A".repeat(32)
            )));
        }
        assert!(!predecessor_child_role("control"));
        assert!(!predecessor_child_role("foreign.sock"));
    }

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
    #[tokio::test]
    async fn a_sized_read_reports_the_payload_length_and_honors_its_limit() -> Result<()> {
        let (mut sender, mut receiver) = tokio::io::duplex(1024);
        write_message(&mut sender, &"abc", Duration::from_secs(5)).await?;
        let read: Option<(String, usize)> =
            read_message_sized(&mut receiver, Duration::from_secs(5), 64).await?;
        assert_eq!(read, Some(("abc".to_owned(), 5)));
        write_message(&mut sender, &"abcd", Duration::from_secs(5)).await?;
        assert!(
            read_message_sized::<_, String>(&mut receiver, Duration::from_secs(5), 5)
                .await
                .is_err()
        );
        drop(sender);
        Ok(())
    }
}
