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
        let path = root.join(format!("{role}.sock"));
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

/// One deadline covers a whole frame, including a peer that trickles its bytes.
pub async fn read_message<S: AsyncRead + Unpin, T: DeserializeOwned>(
    stream: &mut S,
    deadline: Duration,
) -> Result<Option<T>> {
    tokio::time::timeout(deadline, async {
        let mut header = [0; 4];
        let count = stream.read(&mut header[..1]).await?;
        if count == 0 {
            return Ok(None);
        }
        stream.read_exact(&mut header[1..]).await?;
        let bytes = u32::from_be_bytes(header) as usize;
        ensure!(
            bytes > 0 && bytes <= airk_core::service::MAX_FRAME_BYTES,
            "invalid native frame length"
        );
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
    let mut frame = Vec::new();
    crate::service_ipc::write_frame(&mut frame, value)?;
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
