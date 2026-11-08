// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::ScopedConnectRequest,
    service_http::HttpSelection,
};
use std::{io::Cursor, sync::Arc};

fn binding() -> InstallationBinding {
    InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user().unwrap(),
        namespace: Incarnation::from_bytes([2; 16]),
    }
}
fn selection() -> HttpSelection {
    let id = Incarnation::from_bytes([3; 16]);
    HttpSelection::Scoped {
        selection: ScopedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([4; 32]),
            namespace: binding().namespace,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(id),
            expected_grant_revision: CatalogRevision::INITIAL,
            index_id: IndexId::from_identity(id),
            project_ids: vec![QueryProjectId::from_identity(id)],
        },
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
fn readonly_connection_ownership_never_turns_missing_ledger_revocation_or_missing_token_into_absence()
 {
    let root = private_root();
    let id = Incarnation::from_bytes([44; 16]);
    assert!(CredentialStore::read_existing(root.path(), binding()).is_err());
    assert!(!root.path().join("credentials.json").exists());
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    assert!(
        store.ownership(id, now()).is_err(),
        "an unpersisted empty ledger invented authenticated absence"
    );
    let time = now();
    let issued = store.enroll_default(id, selection(), time).unwrap();
    let before = std::fs::read(root.path().join("credentials.json")).unwrap();
    let reader = CredentialStore::read_existing(root.path(), binding()).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Active
    );
    assert_eq!(
        reader
            .ownership(Incarnation::from_bytes([45; 16]), time)
            .unwrap(),
        ConnectionOwnership::NotOwned
    );
    assert_eq!(
        std::fs::read(root.path().join("credentials.json")).unwrap(),
        before
    );
    std::fs::remove_file(&issued.credential_file).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Active,
        "a missing secret erased native ownership"
    );
    assert_eq!(
        reader.ownership(id, u64::MAX).unwrap(),
        ConnectionOwnership::Expired
    );
    store.revoke(id).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Revoked,
        "a stale reader retained old revocation state"
    );
    assert!(
        !issued.credential_file.exists(),
        "readonly lookup recreated a deleted token"
    );
    assert!(
        store.enroll_default(id, selection(), time).is_err(),
        "revoked connection was automatically reissued"
    );
    std::fs::write(root.path().join("credentials.json"), b"{}").unwrap();
    assert!(CredentialStore::read_existing(root.path(), binding()).is_err());
    assert!(reader.ownership(id, time).is_err());
    assert_eq!(
        std::fs::read(root.path().join("credentials.json")).unwrap(),
        b"{}"
    );
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
fn enrollment_restart_recovery_revocation_and_expiry_are_fail_closed() {
    let root = private_root();
    let id = Incarnation::from_bytes([8; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    let issued = store.enroll(id, selection(), 1000, 1).unwrap();
    assert_eq!(issued.generation, 1);
    let token = read_client_token(root.path(), id).unwrap();
    assert_eq!(store.authenticate(&token, 999).unwrap().connection_id, id);
    assert!(store.authenticate(&token, 1000).is_err());
    assert!(store.authenticate(&format!("{token}0"), 2).is_err());
    let bytes = std::fs::read(root.path().join("credentials.json")).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains(&token));
    let mut reopened = CredentialStore::open(root.path(), binding()).unwrap();
    reopened.enroll(id, selection(), 1000, 2).unwrap();
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
    assert!(reopened.enroll(id, selection(), 1001, 2).is_err());
    reopened.revoke(id).unwrap();
    assert!(store.authenticate(&token, 2).is_err());
    assert!(
        CredentialStore::open(root.path(), binding())
            .unwrap()
            .authenticate(&token, 2)
            .is_err()
    );
    assert!(reopened.enroll(id, selection(), 1000, 2).is_err());
}

#[test]
fn credential_retains_its_enrolled_client_family_across_restarts() {
    use airk_core::usage::ClientFamily;
    let root = private_root();
    let id = Incarnation::from_bytes([26; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store
        .enroll_default_for_client(id, selection(), None, ClientFamily::Vscode, 1)
        .unwrap();
    let token = read_client_token(root.path(), id).unwrap();
    let mut reopened = CredentialStore::open(root.path(), binding()).unwrap();
    assert_eq!(
        reopened.authenticate(&token, 2).unwrap().client,
        ClientFamily::Vscode
    );
    reopened
        .enroll_default_for_client(id, selection(), None, ClientFamily::Vscode, 2)
        .unwrap();
    assert!(
        reopened
            .enroll_default_for_client(id, selection(), None, ClientFamily::Codex, 3)
            .is_err()
    );
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
}

#[test]
fn imported_enrollment_retry_preserves_token_and_only_advances_catalog_clock() -> anyhow::Result<()>
{
    use airk_core::{
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        index_catalog::{
            GrantedMembership, Membership, PathScope, ReviewedGrant, ScopeAtom, SourceRootId,
        },
        service::ProjectRegistration,
        service_http::NativeImportedHttpSelection,
    };
    let root = private_root();
    let id = Incarnation::from_bytes([18; 16]);
    let host = Sha256Digest::from_bytes([19; 32]);
    let registration = ProjectRegistration::new(
        "/original/source".into(),
        "/original/data".into(),
        "original".into(),
    )?;
    let provenance = GrantProvenance::ImportedLegacySnapshot {
        upgrade_id: id,
        plan_digest: host,
        legacy_binding: registration.binding,
        configuration_digest: host,
    };
    let original = NativeImportedHttpSelection {
        namespace: binding().namespace,
        expected_catalog_revision: CatalogRevision::INITIAL,
        registration,
        configuration_receipt: Sha256Digest::from_bytes([20; 32]),
        grant: PersistedGrant {
            grant: ReviewedGrant {
                id: native_imported_grant_id(binding().namespace, host, &provenance)?,
                host,
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: QueryProjectId::from_identity(id),
                        index_id: IndexId::from_identity(id),
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: SourceRootId::from_identity(id),
                            kind: PathScope::Subtree,
                            path: String::new(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            provenance,
        },
    };
    let selected = |value: NativeImportedHttpSelection| HttpSelection::ImportedLegacy {
        selection: Box::new(value),
    };
    // Inert storage-capacity fixture: no native grant is issued by this test.
    // Large exact unions must fail before leaving an immutable orphan token.
    let mut members = Vec::new();
    for number in 0..2 {
        let mut member = original.clone();
        member.registration = ProjectRegistration::new(
            format!("/{number}/{}", "s".repeat(3000)),
            format!("/{}", "d".repeat(3000)),
            "original".into(),
        )?;
        if let GrantProvenance::ImportedLegacySnapshot { legacy_binding, .. } =
            &mut member.grant.provenance
        {
            *legacy_binding = member.registration.binding;
        }
        member.grant.grant.id =
            native_imported_grant_id(member.namespace, host, &member.grant.provenance)?;
        members.push(member);
    }
    let union = HttpSelection::ImportedLegacyUnion {
        selection: airk_core::service_http::NativeImportedHttpUnion {
            namespace: binding().namespace,
            selections: members,
            no_default_project: true,
        },
    };
    union.validate()?;
    let capacity_root = private_root();
    let mut capacity = CredentialStore::open(capacity_root.path(), binding())?;
    let mut refused = false;
    for number in 1..=128 {
        let connection_id = Incarnation::from_bytes([number; 16]);
        if capacity
            .enroll_default(connection_id, union.clone(), 1)
            .is_err()
        {
            assert!(
                !capacity_root
                    .path()
                    .join(format!("credential-{connection_id}.token"))
                    .exists()
            );
            assert!(
                number < 128,
                "aggregate byte bound should trigger before entry count"
            );
            refused = true;
            break;
        }
    }
    assert!(refused);
    let mut store = CredentialStore::open(root.path(), binding())?;
    store.enroll_default(id, selected(original.clone()), 1)?;
    let token = read_client_token(root.path(), id)?;
    // A lost setup reply retries through a reopened store and exact connection ID.
    let mut store = CredentialStore::open(root.path(), binding())?;
    store.enroll_default(id, selected(original.clone()), 2)?;
    assert_eq!(read_client_token(root.path(), id)?, token);
    let mut advanced = original.clone();
    advanced.expected_catalog_revision = serde_json::from_value(serde_json::json!(2))?;
    store.enroll_default(id, selected(advanced.clone()), 3)?;
    assert_eq!(
        store.authenticate(&token, 4)?.selection,
        selected(advanced.clone())
    );
    assert!(store.enroll_default(id, selected(original), 5).is_err());
    let mut changed = advanced.clone();
    changed.configuration_receipt = Sha256Digest::from_bytes([21; 32]);
    assert!(store.enroll_default(id, selected(changed), 5).is_err());
    let mut changed = advanced;
    if let GrantProvenance::ImportedLegacySnapshot {
        configuration_digest,
        ..
    } = &mut changed.grant.provenance
    {
        *configuration_digest = Sha256Digest::from_bytes([22; 32]);
    }
    changed.grant.grant.id =
        native_imported_grant_id(changed.namespace, host, &changed.grant.provenance)?;
    assert!(store.enroll_default(id, selected(changed), 5).is_err());
    assert_eq!(read_client_token(root.path(), id)?, token);
    Ok(())
}

#[test]
fn orphan_token_and_cross_installation_never_authorize() {
    let root = private_root();
    let id = Incarnation::from_bytes([8; 16]);
    write_private(
        root.path(),
        &format!("credential-{id}.token"),
        b"orphan",
        103,
        false,
    )
    .unwrap();
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    assert!(store.enroll(id, selection(), 1000, 1).is_err());
    assert!(!root.path().join("credentials.json").exists());
    let id2 = Incarnation::from_bytes([9; 16]);
    store.enroll(id2, selection(), 1000, 1).unwrap();
    let mut other = binding();
    other.installation = Sha256Digest::from_bytes([5; 32]);
    assert!(CredentialStore::open(root.path(), other).is_err());
}

#[test]
fn default_enrollment_keeps_expiry_and_token_on_later_retry() {
    let root = private_root();
    let id = Incarnation::from_bytes([10; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store.enroll_default(id, selection(), 1).unwrap();
    let token = read_client_token(root.path(), id).unwrap();
    let expiry = store.authenticate(&token, 2).unwrap().expires_unix_ms;
    store.enroll_default(id, selection(), 86_400_000).unwrap();
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
    assert_eq!(
        store
            .authenticate(&token, 86_400_000)
            .unwrap()
            .expires_unix_ms,
        expiry
    );
    let mut advanced = selection();
    let HttpSelection::Scoped { selection: scope } = &mut advanced else {
        unreachable!()
    };
    scope.expected_catalog_revision = CatalogRevision::INITIAL.next().unwrap();
    store
        .enroll_default(id, advanced.clone(), 86_400_001)
        .unwrap();
    assert_eq!(
        store.authenticate(&token, 86_400_002).unwrap().selection,
        advanced
    );
    assert!(store.enroll_default(id, selection(), 86_400_003).is_err());
    assert!(store.enroll_default(id, advanced, expiry).is_err());
}

#[test]
fn default_enrollment_rejects_authority_change_and_revocation() {
    let root = private_root();
    let id = Incarnation::from_bytes([10; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store.enroll_default(id, selection(), 1).unwrap();
    let mut different = selection();
    let HttpSelection::Scoped { selection: scope } = &mut different else {
        unreachable!()
    };
    scope.expected_grant_revision = CatalogRevision::INITIAL.next().unwrap();
    assert!(store.enroll_default(id, different, 2).is_err());
    let mut different = selection();
    let HttpSelection::Scoped { selection: scope } = &mut different else {
        unreachable!()
    };
    scope
        .project_ids
        .push(QueryProjectId::from_identity(Incarnation::from_bytes(
            [7; 16],
        )));
    assert!(store.enroll_default(id, different, 2).is_err());
    store.revoke(id).unwrap();
    assert!(store.enroll_default(id, selection(), 2).is_err());
}

#[test]
fn explorer_attachment_is_optional_immutable_connection_metadata() {
    let root = private_root();
    let id = Incarnation::from_bytes([11; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    let window = root
        .path()
        .join("explorer-assistance/windows/11111111111111111111111111111111.json");
    // Store only retains the caller-reviewed reference; actual window ownership
    // and expiry verification belong to native enrollment and request wrapper.
    let issued = store
        .enroll_default_attached(id, selection(), Some(window.clone()), 1)
        .unwrap();
    assert_eq!(issued.explorer_descriptor, Some(window.clone()));
    let token = read_client_token(root.path(), id).unwrap();
    assert_eq!(
        store.authenticate(&token, 2).unwrap().explorer_descriptor,
        Some(window.clone())
    );
    store
        .enroll_default_attached(id, selection(), Some(window), 2)
        .unwrap();
    assert!(store.enroll_default(id, selection(), 3).is_err());
    assert!(
        store
            .enroll_default_attached(
                id,
                selection(),
                Some(root.path().join("other-window.json")),
                3
            )
            .is_err()
    );
    assert!(
        store
            .enroll_default_attached(
                Incarnation::from_bytes([12; 16]),
                selection(),
                Some("relative.json".into()),
                3
            )
            .is_err()
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
