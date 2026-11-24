// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Sha256Digest;
use std::sync::Arc;

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

/// Local trust plan C: an endpoint record an HTTPS build wrote is read as a
/// legacy predecessor. It is never a current descriptor; its port and
/// generation carry forward, the next publication must chain to its digest
/// with a higher generation, and then the record is current HTTP.
#[test]
fn a_legacy_https_record_is_carried_forward_by_the_next_http_generation() {
    use airk_core::service_http::{
        EndpointDescriptor, EndpointState, LoopbackMcpUrl, SavedEndpoint,
    };
    let root = private_root();
    let binding = binding();
    let old = serde_json::json!({
        "schema": 1, "installation": binding.installation, "user": binding.user,
        "namespace": binding.namespace, "generation": 4,
        "service_instance": Incarnation::from_bytes([12; 16]),
        "executable_sha256": Sha256Digest::from_bytes([13; 32]),
        "url": "https://127.0.0.1:57431/mcp", "ca_sha256": Sha256Digest::from_bytes([14; 32]),
        "certificate_sha256": Sha256Digest::from_bytes([15; 32]), "state": "stopping",
        "predecessor_digest": null,
    });
    let bytes = serde_json::to_vec(&old).unwrap();
    write_private(root.path(), "endpoint.json", &bytes, 4096, false).unwrap();
    assert_eq!(load_descriptor(root.path(), &binding).unwrap(), None);
    let saved = load_saved(root.path(), &binding).unwrap().unwrap();
    let SavedEndpoint::Legacy(legacy) = &saved else {
        panic!("an HTTPS record must decode as legacy");
    };
    assert_eq!((legacy.port, legacy.generation), (57431, 4));
    let expected = saved.digest().unwrap();
    let mut next = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 4,
        service_instance: Incarnation::from_bytes([16; 16]),
        executable_sha256: Sha256Digest::from_bytes([17; 32]),
        url: LoopbackMcpUrl::new(saved.port()).unwrap(),
        state: EndpointState::Ready,
        predecessor_digest: Some(expected),
    };
    // The same generation, a missing or wrong predecessor digest: refused.
    assert!(publish_descriptor(root.path(), &binding, &next, Some(expected)).is_err());
    next.generation = 5;
    assert!(publish_descriptor(root.path(), &binding, &next, None).is_err());
    let mut crossed = next.clone();
    crossed.predecessor_digest = Some(Sha256Digest::from_bytes([18; 32]));
    assert!(
        publish_descriptor(root.path(), &binding, &crossed, crossed.predecessor_digest).is_err()
    );
    publish_descriptor(root.path(), &binding, &next, Some(expected)).unwrap();
    assert_eq!(
        load_descriptor(root.path(), &binding).unwrap(),
        Some(next.clone())
    );
    assert_eq!(next.url.to_string(), "http://127.0.0.1:57431/mcp");
    // Another installation's legacy record is refused like a current one.
    write_private(root.path(), "endpoint.json", &bytes, 4096, true).unwrap();
    let mut foreign = binding;
    foreign.namespace = Incarnation::from_bytes([19; 16]);
    assert!(load_saved(root.path(), &foreign).is_err());
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
