// SPDX-License-Identifier: Apache-2.0
//! Owner-private HTTPS identity and credentials; never a substitute for grant validation.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::PREFERRED_HTTP_PORT,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::{Ipv4Addr, TcpListener},
    path::Path,
};

#[path = "http_endpoint_metadata.rs"]
mod metadata;
#[path = "http_endpoint_store.rs"]
mod store;
#[path = "http_endpoint_tls.rs"]
mod tls;
#[path = "http_endpoint_trust.rs"]
pub mod trust;
pub use metadata::{
    load_descriptor, publish_descriptor, read_registration_file, read_registrations_file,
    read_selection_file,
};
pub use store::{
    AuthenticatedCredential, ConnectionOwnership, CredentialStore, IssuedCredential,
    read_client_token,
};
pub use tls::{TlsBundleIdentity, TlsIdentity};
#[path = "http_endpoint_rotation.rs"]
pub mod rotation;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationBinding {
    pub installation: Sha256Digest,
    pub user: String,
    pub namespace: Incarnation,
}
impl InstallationBinding {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.user == crate::local_ipc::current_user()?,
            "HTTP identity owner mismatch"
        );
        Ok(())
    }
}

/// Reconstruct from the verified native control root, not from untrusted JSON.
pub fn installation_binding(control: &Path, namespace: Incarnation) -> Result<InstallationBinding> {
    crate::runtime_install::validate_control_root(control)?;
    let control = control.canonicalize()?;
    let user = crate::local_ipc::current_user()?;
    let bytes = serde_json::to_vec(&control)?;
    Ok(InstallationBinding {
        installation: airk_core::identity::framed_digest(
            b"airk-http-installation/1",
            &[&bytes, user.as_bytes(), namespace.as_bytes()],
        ),
        user,
        namespace,
    })
}

/// The socket remains owned throughout publication and conversion to async I/O.
pub struct BoundEndpoint(TcpListener);
impl BoundEndpoint {
    pub fn bind(saved_port: Option<u16>) -> io::Result<Self> {
        let port = saved_port.unwrap_or(PREFERRED_HTTP_PORT);
        if port == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid saved HTTP port",
            ));
        }
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => Ok(Self(listener)),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map(Self)
            }
            Err(error) => Err(error),
        }
    }
    pub fn port(&self) -> io::Result<u16> {
        Ok(self.0.local_addr()?.port())
    }
    pub fn into_tokio(self) -> io::Result<tokio::net::TcpListener> {
        self.0.set_nonblocking(true)?;
        tokio::net::TcpListener::from_std(self.0)
    }
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn private_file(path: &Path) -> Result<()> {
    crate::runtime_install::safe_file(path)?;
    #[cfg(windows)]
    crate::local_ipc::require_owner_private(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.uid().to_string() == crate::local_ipc::current_user()?
                && metadata.permissions().mode() & 0o077 == 0,
            "HTTP file is not owner-private"
        );
    }
    Ok(())
}

pub(crate) fn read_private(root: &Path, name: &str, limit: usize) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    crate::local_ipc::require_existing_root(root)?;
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => {
            result?;
        }
    }
    private_file(&path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(&path)?;
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "HTTP private record exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "HTTP private record exceeds bound");
    private_file(&path)?;
    Ok(Some(bytes))
}

fn write_private(root: &Path, name: &str, bytes: &[u8], limit: usize, replace: bool) -> Result<()> {
    use std::io::Write;
    ensure!(bytes.len() <= limit, "HTTP private record exceeds bound");
    crate::local_ipc::require_existing_root(root)?;
    let path = root.join(name);
    if std::fs::symlink_metadata(&path).is_ok() {
        private_file(&path)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    private_file(temporary.path())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(&path).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(&path)
            .map_err(|error| error.error)?;
    }
    private_file(&path)?;
    #[cfg(unix)]
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "http_endpoint_tests.rs"]
mod tests;
