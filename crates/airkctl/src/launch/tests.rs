// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::ScopedConnectRequest,
    service_http::{EndpointDescriptor, EndpointState, HttpSelection, LoopbackMcpUrl},
};

const CA: &[u8] = b"Native public CA fixture.\n";

fn connection(root: &Path) -> HttpConnectionInfo {
    let id = Incarnation::from_bytes([131; 16]);
    let namespace = Incarnation::from_bytes([132; 16]);
    HttpConnectionInfo {
        schema: 1,
        connection_id: id,
        endpoint: EndpointDescriptor {
            schema: 1,
            installation: Sha256Digest::from_bytes([133; 32]),
            user: airk_ipc::endpoint::current_user().unwrap(),
            namespace,
            generation: 1,
            service_instance: Incarnation::from_bytes([134; 16]),
            executable_sha256: Sha256Digest::from_bytes([135; 32]),
            url: LoopbackMcpUrl::new(57421).unwrap(),
            ca_sha256: Sha256Digest::from_bytes(Sha256::digest(CA).into()),
            certificate_sha256: Sha256Digest::from_bytes([136; 32]),
            state: EndpointState::Ready,
            predecessor_digest: None,
        },
        selection: HttpSelection::Scoped {
            selection: ScopedConnectRequest {
                schema: 1,
                context_key: Sha256Digest::from_bytes([137; 32]),
                namespace,
                expected_catalog_revision: CatalogRevision::INITIAL,
                grant_id: GrantId::from_identity(id),
                expected_grant_revision: CatalogRevision::INITIAL,
                index_id: IndexId::from_identity(id),
                project_ids: vec![QueryProjectId::from_identity(id)],
            },
        },
        ca_certificate_file: root.join("ca.pem").to_string_lossy().into_owned(),
        credential_file: root
            .join(format!("credential-{id}.token"))
            .to_string_lossy()
            .into_owned(),
        open: None,
    }
}

/// An owner-private credential root holding the CA and a well-formed token.
fn credential_root(temporary: &Path) -> (PathBuf, HttpConnectionInfo, String) {
    let root = temporary.canonicalize().unwrap().join("credentials");
    airk_ipc::endpoint::prepare_root(&root).unwrap();
    let connection = connection(&root);
    let token = format!("airk1.{}.{}", connection.connection_id, "ab".repeat(32));
    assert_eq!(token.len(), airk_core::service_http_credentials::MAX_TOKEN);
    create_private(&root, "ca.pem", CA).unwrap();
    create_private(
        &root,
        &airk_core::service_http_credentials::filename(connection.connection_id),
        token.as_bytes(),
    )
    .unwrap();
    (root, connection, token)
}

#[test]
fn launch_arguments_hold_references_and_never_the_secret() {
    let root = tempfile::tempdir().unwrap();
    let connection = connection(root.path());
    let arguments = client_arguments(
        Client::Codex,
        &connection,
        None,
        root.path(),
        &["user argument with spaces".into()],
    )
    .unwrap();
    assert_eq!(arguments[0], "--no-daemon");
    assert_eq!(arguments[1], "-c");
    let value = arguments[2].to_str().unwrap();
    assert!(
        value.starts_with("mcp_servers.airk={enabled=true,url="),
        "{value}"
    );
    assert!(
        value.contains(&format!(
            "bearer_token_env_var={}",
            serde_json::to_string(&variable(&connection)).unwrap()
        )),
        "{value}"
    );
    assert!(
        value.contains(&connection.endpoint.url.to_string()),
        "{value}"
    );
    assert_eq!(arguments[3], "user argument with spaces");
    let config = root.path().join("connection with spaces.json");
    let arguments =
        client_arguments(Client::Claude, &connection, Some(&config), root.path(), &[]).unwrap();
    assert_eq!(
        arguments,
        vec![OsString::from("--mcp-config"), config.into_os_string()]
    );
    let arguments = client_arguments(
        Client::Vscode,
        &connection,
        None,
        root.path(),
        &["-r".into()],
    )
    .unwrap();
    assert_eq!(
        arguments,
        vec![
            OsString::from("--new-window"),
            OsString::from("-r"),
            root.path().as_os_str().to_owned()
        ]
    );
}

#[test]
fn launch_additive_ca_preserves_existing_bundles_and_bounds_all_input() {
    let combined = combine_certificates(
        b"native",
        [
            b"enterprise\n".to_vec(),
            b"existing".to_vec(),
            b"enterprise\n".to_vec(),
        ],
    )
    .unwrap();
    assert_eq!(combined, b"enterprise\nexisting\nnative\n");
    assert!(combine_certificates(b"native", [vec![b'x'; MAX_CA_BYTES]]).is_err());
    assert!(combine_certificates(b"", []).is_err());
}

#[test]
fn launch_plan_reads_owned_token_into_child_environment_only() {
    let temporary = tempfile::tempdir().unwrap();
    let (root, connection, token) = credential_root(temporary.path());
    let enterprise = root.join("enterprise.pem");
    std::fs::write(&enterprise, b"Enterprise root fixture.\n").unwrap();
    let inherited = BTreeMap::from([(
        OsString::from("NODE_EXTRA_CA_CERTS"),
        enterprise.clone().into_os_string(),
    )]);
    let planned = plan(Client::Claude, &connection, &root, &[], &inherited).unwrap();
    assert_eq!(
        planned
            .environment
            .get(&OsString::from(variable(&connection)))
            .unwrap(),
        &OsString::from(&token)
    );
    assert!(!format!("{:?}", planned.arguments).contains(&token));
    let ca = Path::new(
        planned
            .environment
            .get(&OsString::from("NODE_EXTRA_CA_CERTS"))
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(ca).unwrap(),
        b"Enterprise root fixture.\nNative public CA fixture.\n"
    );
    assert_eq!(
        std::fs::read(&enterprise).unwrap(),
        b"Enterprise root fixture.\n"
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&planned.arguments[1]).unwrap()).unwrap();
    assert_eq!(
        config["mcpServers"]["airk"]["headers"]["Authorization"],
        format!("Bearer ${{{}}}", variable(&connection))
    );
    assert!(!serde_json::to_string(&config).unwrap().contains(&token));
    assert_eq!(inherited.len(), 1, "parent environment was not mutated");
    // A second plan reuses the same content-addressed files.
    let again = plan(Client::Claude, &connection, &root, &[], &inherited).unwrap();
    assert_eq!(again.arguments, planned.arguments);
}

#[test]
fn launch_plan_for_vscode_carries_trust_but_no_token() {
    let temporary = tempfile::tempdir().unwrap();
    let (root, connection, token) = credential_root(temporary.path());
    let planned = plan(Client::Vscode, &connection, &root, &[], &BTreeMap::new()).unwrap();
    assert_eq!(planned.environment.len(), 1);
    let ca = planned
        .environment
        .get(&OsString::from("NODE_EXTRA_CA_CERTS"))
        .unwrap();
    assert_eq!(std::fs::read(ca).unwrap(), CA);
    assert!(!format!("{:?} {:?}", planned.arguments, planned.environment).contains(&token));
}

#[test]
fn launch_plan_refuses_crossed_references_and_a_changed_ca() {
    let temporary = tempfile::tempdir().unwrap();
    let (root, mut connection, _) = credential_root(temporary.path());
    let original = connection.clone();
    connection.ca_certificate_file = root.join("other.pem").to_string_lossy().into_owned();
    assert!(plan(Client::Codex, &connection, &root, &[], &BTreeMap::new()).is_err());
    let mut changed = original;
    changed.endpoint.ca_sha256 = Sha256Digest::from_bytes([9; 32]);
    let error = plan(Client::Codex, &changed, &root, &[], &BTreeMap::new())
        .err()
        .unwrap();
    assert!(error.contains("CA changed"), "{error}");
}

#[test]
fn launch_windows_client_resolution_uses_absolute_path_shims_without_current_directory() {
    let owner = tempfile::tempdir().unwrap();
    let trusted = owner.path().canonicalize().unwrap().join("trusted tools");
    std::fs::create_dir(&trusted).unwrap();
    std::fs::write(trusted.join("claude.cmd"), b"fictional shim").unwrap();
    assert_eq!(
        windows_client_executable(
            Client::Claude,
            [PathBuf::new(), PathBuf::from("."), trusted.clone()]
        )
        .unwrap(),
        trusted.join("claude.cmd")
    );
    let native = trusted.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    std::fs::create_dir_all(native.parent().unwrap()).unwrap();
    std::fs::write(&native, b"fictional native npm target").unwrap();
    assert_eq!(
        windows_client_executable(Client::Claude, [trusted.clone()]).unwrap(),
        native
    );
    std::fs::write(trusted.join("claude.exe"), b"fictional executable").unwrap();
    assert_eq!(
        windows_client_executable(Client::Claude, [trusted.clone()]).unwrap(),
        trusted.join("claude.exe")
    );
    std::fs::write(trusted.join("code.cmd"), b"fictional VS Code shim").unwrap();
    assert_eq!(
        windows_client_executable(Client::Vscode, [trusted.clone()]).unwrap(),
        trusted.join("code.cmd")
    );
    assert!(windows_client_executable(Client::Codex, [PathBuf::from("."), trusted]).is_err());
}
