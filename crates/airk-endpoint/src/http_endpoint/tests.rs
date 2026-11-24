// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{io::Cursor, sync::Arc};

fn binding() -> InstallationBinding {
    InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user().unwrap(),
        namespace: Incarnation::from_bytes([2; 16]),
    }
}
struct PrivateRoot {
    _parent: tempfile::TempDir,
    path: std::path::PathBuf,
}
impl PrivateRoot {
    fn path(&self) -> &Path {
        &self.path
    }
}
fn private_root() -> PrivateRoot {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("http");
    crate::local_ipc::prepare_root(&path).unwrap();
    PrivateRoot {
        _parent: directory,
        path,
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

#[test]
fn descriptor_cas_preserves_identity_and_requires_generation_for_relocation() {
    use airk_core::service_http::{EndpointDescriptor, EndpointState, LoopbackMcpUrl};
    let root = private_root();
    let binding = binding();
    let original = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([12; 16]),
        executable_sha256: Sha256Digest::from_bytes([13; 32]),
        url: LoopbackMcpUrl::new(57421).unwrap(),
        ca_sha256: Sha256Digest::from_bytes([14; 32]),
        certificate_sha256: Sha256Digest::from_bytes([15; 32]),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    publish_descriptor(root.path(), &binding, &original, None).unwrap();
    let expected = original.digest().unwrap();
    let mut next = original.clone();
    next.predecessor_digest = Some(expected);
    next.url = LoopbackMcpUrl::new(57422).unwrap();
    assert!(publish_descriptor(root.path(), &binding, &next, Some(expected)).is_err());
    assert_eq!(
        load_descriptor(root.path(), &binding).unwrap(),
        Some(original.clone())
    );
    next.generation = 2;
    publish_descriptor(root.path(), &binding, &next, Some(expected)).unwrap();
    assert!(publish_descriptor(root.path(), &binding, &next, Some(expected)).is_err());
    assert_eq!(
        load_descriptor(root.path(), &binding).unwrap(),
        Some(next.clone())
    );
    let expected = next.digest().unwrap();
    next.predecessor_digest = Some(expected);
    next.certificate_sha256 = Sha256Digest::from_bytes([16; 32]);
    next.state = EndpointState::Stopping;
    publish_descriptor(root.path(), &binding, &next, Some(expected)).unwrap();
    assert_eq!(load_descriptor(root.path(), &binding).unwrap(), Some(next));
    let mut foreign = binding;
    foreign.namespace = Incarnation::from_bytes([17; 16]);
    assert!(load_descriptor(root.path(), &foreign).is_err());
}

#[test]
fn occupied_saved_port_falls_back_without_releasing_foreign_listener() {
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = foreign.local_addr().unwrap().port();
    let endpoint = BoundEndpoint::bind(Some(port)).unwrap();
    assert_ne!(endpoint.port().unwrap(), port);
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, endpoint.port().unwrap())).is_err());
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err());
    assert!(BoundEndpoint::bind(Some(0)).is_err());
    foreign.set_nonblocking(true).unwrap();
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn concurrent_port_fallback_retains_distinct_sockets_without_contacting_occupant() {
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let occupied = foreign.local_addr().unwrap().port();
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let endpoints = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    BoundEndpoint::bind(Some(occupied)).unwrap()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    let ports: std::collections::BTreeSet<_> = endpoints
        .iter()
        .map(|endpoint| endpoint.port().unwrap())
        .collect();
    assert_eq!(ports.len(), endpoints.len());
    assert!(!ports.contains(&occupied));
    for port in ports {
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err());
    }
    foreign.set_nonblocking(true).unwrap();
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn issuer_persists_leaf_renews_and_mismatched_public_export_fails() {
    let root = private_root();
    let now = now();
    let original = TlsIdentity::load_or_create(root.path(), binding(), now).unwrap();
    let restarted = TlsIdentity::load_or_create(root.path(), binding(), now + 1000).unwrap();
    assert_eq!(original.ca_sha256(), restarted.ca_sha256());
    assert_eq!(
        original.certificate_sha256(),
        restarted.certificate_sha256()
    );
    let renewed =
        TlsIdentity::load_or_create(root.path(), binding(), now + 24 * 86_400_000).unwrap();
    assert_eq!(original.ca_sha256(), renewed.ca_sha256());
    assert_ne!(original.certificate_sha256(), renewed.certificate_sha256());
    write_private(root.path(), "ca.pem", b"different issuer", 8192, true).unwrap();
    assert!(TlsIdentity::load_or_create(root.path(), binding(), now).is_err());
}

#[test]
fn expiring_issuer_fails_without_silent_trust_replacement() {
    let root = private_root();
    let now = now();
    TlsIdentity::load_or_create(root.path(), binding(), now).unwrap();
    let bundle = std::fs::read(root.path().join("tls-identity.json")).unwrap();
    let ca = std::fs::read(root.path().join("ca.pem")).unwrap();
    assert!(TlsIdentity::load_or_create(root.path(), binding(), now + 3620 * 86_400_000).is_err());
    assert_eq!(
        std::fs::read(root.path().join("tls-identity.json")).unwrap(),
        bundle
    );
    assert_eq!(std::fs::read(root.path().join("ca.pem")).unwrap(), ca);
}

fn handshake(identity: &TlsIdentity, ca: Vec<u8>, name: &str) -> anyhow::Result<()> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(ca))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut client = rustls::ClientConnection::new(
        Arc::new(config),
        rustls::pki_types::ServerName::try_from(name.to_owned())?,
    )?;
    let mut server = rustls::ServerConnection::new(identity.server_config())?;
    for _ in 0..8 {
        let mut wire = Vec::new();
        client.write_tls(&mut wire)?;
        server.read_tls(&mut Cursor::new(wire))?;
        server.process_new_packets()?;
        let mut wire = Vec::new();
        server.write_tls(&mut wire)?;
        client.read_tls(&mut Cursor::new(wire))?;
        client.process_new_packets()?;
        if !client.is_handshaking() && !server.is_handshaking() {
            return Ok(());
        }
    }
    anyhow::bail!("handshake did not complete")
}
fn ca_der(root: &Path) -> Vec<u8> {
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("tls-identity.json")).unwrap()).unwrap();
    serde_json::from_value(bundle["ca_der"].clone()).unwrap()
}
#[test]
fn real_rustls_handshake_validates_ip_and_issuer() {
    let root = private_root();
    let identity = TlsIdentity::load_or_create(root.path(), binding(), now()).unwrap();
    assert!(handshake(&identity, ca_der(root.path()), "127.0.0.1").is_ok());
    assert!(handshake(&identity, ca_der(root.path()), "localhost").is_err());
    assert!(handshake(&identity, ca_der(root.path()), "127.0.0.2").is_err());
    let other = private_root();
    TlsIdentity::load_or_create(other.path(), binding(), now()).unwrap();
    assert!(handshake(&identity, ca_der(other.path()), "127.0.0.1").is_err());
}

#[cfg(unix)]
#[test]
fn private_record_rejects_links_and_public_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = private_root();
    write_private(root.path(), "record", b"private", 32, false).unwrap();
    std::fs::hard_link(root.path().join("record"), root.path().join("hardlink")).unwrap();
    assert!(read_private(root.path(), "record", 32).is_err());
    std::fs::remove_file(root.path().join("hardlink")).unwrap();
    symlink(root.path().join("record"), root.path().join("symlink")).unwrap();
    assert!(read_private(root.path(), "symlink", 32).is_err());
    std::fs::set_permissions(
        root.path().join("record"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(read_private(root.path(), "record", 32).is_err());
}
