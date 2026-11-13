// SPDX-License-Identifier: Apache-2.0
//! Owner-private HTTPS identity and credentials; never a substitute for grant validation.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::PREFERRED_HTTP_PORT,
};
use anyhow::Result;

use std::{
    io,
    net::{Ipv4Addr, TcpListener},
    path::Path,
};

mod metadata;
mod store;
mod tls;
pub mod trust;
pub use airk_core::service_http_credentials::same_authority;
pub use metadata::{
    load_descriptor, publish_descriptor, read_registration_file, read_registrations_file,
    read_selection_file,
};
pub use store::{
    AttachmentRetirementProof, AuthenticatedCredential, ConnectionOwnership, CredentialStore,
    IssuedCredential, OpenDefaultsChange, OpenDefaultsRefused, authority_digest, read_client_token,
};
pub use tls::{TlsBundleIdentity, TlsIdentity};
pub mod rotation;

pub use airk_core::service_http_credentials::InstallationBinding;

/// The binding belongs to the current user.
pub(crate) fn validate_binding(binding: &InstallationBinding) -> Result<()> {
    binding.require_user(&crate::local_ipc::current_user()?)
}

/// Reconstruct from the verified native control root, not from untrusted JSON.
pub fn installation_binding(control: &Path, namespace: Incarnation) -> Result<InstallationBinding> {
    airk_host::install_layout::validate_control_root(control)?;
    let control = control.canonicalize()?;
    let user = crate::local_ipc::current_user()?;
    InstallationBinding::derive(&control, &user, namespace)
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

/// Moved to `airk-ipc` (U90) so `airkctl` keeps the same owner-private records.
pub use airk_ipc::private_record::read_private;
use airk_ipc::private_record::{private_file, write_private};

fn digest(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[cfg(test)]
mod tests;
