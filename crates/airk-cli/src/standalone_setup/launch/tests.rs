// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::ScopedConnectRequest,
    service_http::{EndpointDescriptor, EndpointState, LoopbackMcpUrl},
};

fn connection(root: &Path) -> Result<HttpConnectionInfo> {
    let id = Incarnation::from_bytes([131; 16]);
    let namespace = Incarnation::from_bytes([132; 16]);
    let ca = b"Native public CA fixture.\n";
    Ok(HttpConnectionInfo {
        schema: 1,
        connection_id: id,
        endpoint: EndpointDescriptor {
            schema: 1,
            installation: Sha256Digest::from_bytes([133; 32]),
            user: crate::local_ipc::current_user()?,
            namespace,
            generation: 1,
            service_instance: Incarnation::from_bytes([134; 16]),
            executable_sha256: Sha256Digest::from_bytes([135; 32]),
            url: LoopbackMcpUrl::new(57421)?,
            ca_sha256: Sha256Digest::from_bytes(Sha256::digest(ca).into()),
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
    })
}

#[test]
fn standalone_client_arguments_hold_references_and_never_the_secret() -> Result<()> {
    let root = tempfile::tempdir()?;
    let connection = connection(root.path())?;
    let arguments = client_arguments(
        Client::Codex,
        &connection,
        None,
        &["user argument with spaces".into()],
    )?;
    assert_eq!(arguments[0], "--no-daemon");
    assert_eq!(arguments[1], "-c");
    let value = arguments[2]
        .to_str()
        .unwrap()
        .strip_prefix("mcp_servers.airk=")
        .unwrap();
    let parsed = toml::from_str::<toml::Value>(&format!("server={value}"))?;
    assert_eq!(parsed["server"]["enabled"].as_bool(), Some(true));
    assert_eq!(arguments[3], "user argument with spaces");
    assert_eq!(
        parsed["server"]["url"].as_str(),
        Some(connection.endpoint.url.to_string().as_str())
    );
    assert_eq!(
        parsed["server"]["bearer_token_env_var"].as_str(),
        Some(variable(&connection).as_str())
    );
    let config = root.path().join("connection with spaces.json");
    let arguments = client_arguments(Client::Claude, &connection, Some(&config), &[])?;
    assert_eq!(
        arguments,
        vec![OsString::from("--mcp-config"), config.into_os_string()]
    );
    Ok(())
}

#[test]
fn standalone_additive_ca_preserves_existing_bundles_and_bounds_all_input() -> Result<()> {
    let combined = combine_certificates(
        b"native",
        [
            b"enterprise\n".to_vec(),
            b"existing".to_vec(),
            b"enterprise\n".to_vec(),
        ],
    )?;
    assert_eq!(combined, b"enterprise\nexisting\nnative\n");
    assert!(combine_certificates(b"native", [vec![b'x'; MAX_CA_BYTES]]).is_err());
    assert!(combine_certificates(b"", []).is_err());
    Ok(())
}

#[test]
fn standalone_launch_plan_reads_owned_token_into_child_environment_only() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    // tempfile's cleanup owner has platform-default permissions. The native
    // credential root is a fresh child with AIRK's explicit owner-only ACL.
    let root = temporary.path().canonicalize()?.join("credentials");
    crate::local_ipc::prepare_root(&root)?;
    let connection = connection(&root)?;
    let binding = crate::http_endpoint::InstallationBinding {
        installation: connection.endpoint.installation,
        user: connection.endpoint.user.clone(),
        namespace: connection.endpoint.namespace,
    };
    let mut credentials = crate::http_endpoint::CredentialStore::open(&root, binding)?;
    credentials.enroll_default(
        connection.connection_id,
        connection.selection.clone(),
        1_800_000_000_000,
    )?;
    write_private_file(&root, "ca.pem", b"Native public CA fixture.\n", 8192, false)?;
    let enterprise = root.join("enterprise.pem");
    std::fs::write(&enterprise, b"Enterprise root fixture.\n")?;
    let inherited = BTreeMap::from([(
        OsString::from("NODE_EXTRA_CA_CERTS"),
        enterprise.clone().into_os_string(),
    )]);
    let planned = plan(Client::Claude, &connection, &[], &inherited)?;
    let expected = crate::http_endpoint::read_client_token(&root, connection.connection_id)?;
    assert_eq!(
        planned
            .environment
            .get(&OsString::from(variable(&connection)))
            .unwrap(),
        &OsString::from(&expected)
    );
    assert!(!format!("{:?}", planned.arguments).contains(&expected));
    let ca = Path::new(
        planned
            .environment
            .get(&OsString::from("NODE_EXTRA_CA_CERTS"))
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(ca)?,
        b"Enterprise root fixture.\nNative public CA fixture.\n"
    );
    assert_eq!(std::fs::read(enterprise)?, b"Enterprise root fixture.\n");
    let config: Value = serde_json::from_slice(&std::fs::read(&planned.arguments[1])?)?;
    assert_eq!(
        config["mcpServers"]["airk"]["headers"]["Authorization"],
        format!("Bearer ${{{}}}", variable(&connection))
    );
    assert!(!serde_json::to_string(&config)?.contains(&expected));
    assert_eq!(inherited.len(), 1, "parent environment was not mutated");
    Ok(())
}

#[test]
fn standalone_windows_client_resolution_uses_absolute_path_shims_without_current_directory()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let trusted = owner.path().canonicalize()?.join("trusted tools");
    std::fs::create_dir(&trusted)?;
    std::fs::write(trusted.join("claude.cmd"), b"fictional shim")?;
    assert_eq!(
        windows_client_executable(
            Client::Claude,
            [PathBuf::new(), PathBuf::from("."), trusted.clone()]
        )?,
        trusted.join("claude.cmd")
    );
    let native = trusted.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    std::fs::create_dir_all(native.parent().unwrap())?;
    std::fs::write(&native, b"fictional native npm target")?;
    assert_eq!(
        windows_client_executable(Client::Claude, [trusted.clone()])?,
        native
    );
    std::fs::write(trusted.join("claude.exe"), b"fictional executable")?;
    assert_eq!(
        windows_client_executable(Client::Claude, [trusted.clone()])?,
        trusted.join("claude.exe")
    );
    assert!(windows_client_executable(Client::Codex, [PathBuf::from("."), trusted]).is_err());
    Ok(())
}
