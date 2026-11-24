// SPDX-License-Identifier: Apache-2.0
//! Owner-private HTTPS identity and endpoint metadata, and the removal of the
//! credential ledger older installations left (local trust plan B).
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

pub mod legacy_trust;
mod metadata;
pub mod retired_credentials;
mod tls;
pub use metadata::{load_descriptor, publish_descriptor};
pub use tls::{TlsBundleIdentity, TlsIdentity};

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

/// The URL of the saved endpoint record of the installation at `control`,
/// read against its existing native namespace without starting a service or
/// creating anything. `None` when no record exists.
pub fn saved_endpoint_url(control: &Path) -> Result<Option<String>> {
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let binding = installation_binding(control, registry.incarnation()?)?;
    Ok(load_descriptor(&control.join("http-1"), &binding)?.map(|value| value.url.to_string()))
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
use airk_ipc::private_record::write_private;

fn digest(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[cfg(test)]
mod tests;
