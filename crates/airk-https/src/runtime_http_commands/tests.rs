// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::{ControlRequest, ScopedConnectRequest},
    service_http::{EndpointState, HttpConnectionInfo, LoopbackMcpUrl},
};
use std::fs;

fn setup(control: &Path) -> Result<(ServiceHealth, HttpConnectionInfo)> {
    let id = Incarnation::from_bytes([1; 16]);
    let namespace = Incarnation::from_bytes([2; 16]);
    let instance = Incarnation::from_bytes([3; 16]);
    let digest = Sha256Digest::from_bytes([4; 32]);
    let health = ServiceHealth {
        catalog_clock: Some(airk_core::service::CatalogClock {
            namespace,
            revision: CatalogRevision::INITIAL,
        }),
        storage_formats: None,
        build: None,
        build_error: None,
        logs: airk_core::service_logs::LogTelemetry {
            available: false,
            dropped_records: 0,
            write_failures: 0,
        },
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: airk_core::service::PROTOCOL_VERSION,
        instance,
        process_id: std::process::id(),
        executable_sha256: digest.to_string(),
        runtime_sha256: Some(digest.to_string()),
        user: local_ipc::current_user()?,
        uptime_ms: 0,
        policy: airk_kernel::service_broker::Broker::new(instance, Default::default())?
            .status(std::time::Instant::now()),
    };
    let connection = HttpConnectionInfo {
        schema: 1,
        connection_id: id,
        endpoint: EndpointDescriptor {
            schema: 1,
            installation: airk_endpoint::http_endpoint::installation_binding(control, namespace)?
                .installation,
            user: health.user.clone(),
            namespace,
            generation: 1,
            service_instance: instance,
            executable_sha256: digest,
            url: LoopbackMcpUrl::new(57421)?,
            ca_sha256: digest,
            certificate_sha256: digest,
            state: EndpointState::Ready,
            predecessor_digest: None,
        },
        selection: HttpSelection::Scoped {
            selection: ScopedConnectRequest {
                schema: 1,
                context_key: digest,
                namespace,
                expected_catalog_revision: CatalogRevision::INITIAL,
                grant_id: GrantId::from_identity(id),
                expected_grant_revision: CatalogRevision::INITIAL,
                index_id: IndexId::from_identity(id),
                project_ids: vec![QueryProjectId::from_identity(id)],
            },
        },
        ca_certificate_file: control
            .join("http-1")
            .join("ca.pem")
            .to_string_lossy()
            .into_owned(),
        credential_file: control
            .join("http-1")
            .join(format!("credential-{id}.token"))
            .to_string_lossy()
            .into_owned(),
        open: None,
    };
    Ok((health, connection))
}
#[tokio::test]
async fn enrollment_uses_real_native_control_pins_instance_and_preserves_complete_connection()
-> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("control");
    local_ipc::prepare_root(&root)?;
    let (health, info) = setup(&root)?;
    let expected = serde_json::to_value(&info)?;
    let selection = info.selection.clone();
    let id = info.connection_id;
    let previous = Some(Incarnation::from_bytes([89; 16]));
    let instance = health.instance;
    let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(&root, "control")?)?;
    let request_selection = selection.clone();
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await?;
        let request: ControlRequest = local_ipc::read_message(&mut stream, Duration::from_secs(2))
            .await?
            .context("missing enrollment request")?;
        ensure!(
            request.expected_instance == Some(instance),
            "enrollment lost elected native instance"
        );
        let ControlMethod::HttpEnroll {
            connection_id,
            selection,
            client,
            explorer_descriptor,
            reattach_connection_id,
        } = request.method
        else {
            bail!("wrong native operation");
        };
        ensure!(
            connection_id == id
                && *selection == request_selection
                && client == airk_core::usage::ClientFamily::Codex
                && explorer_descriptor.is_none()
                && reattach_connection_id == previous,
            "native enrollment changed frozen inputs"
        );
        local_ipc::write_message(
            &mut stream,
            &ControlResponse::HttpConnection {
                connection: Box::new(info),
            },
            Duration::from_secs(2),
        )
        .await
    });
    let actual = enroll_at(
        &root,
        &health,
        selection,
        id,
        airk_core::usage::ClientFamily::Codex,
        None,
        previous,
    )
    .await?;
    server.await??;
    assert_eq!(actual, expected);
    assert!(actual.get("credential_file").is_some());
    assert!(actual.get("token").is_none());
    Ok(())
}
#[test]
fn crossed_instance_selection_identity_and_private_file_references_are_refused() -> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("control");
    local_ipc::prepare_root(&root)?;
    let (health, original) = setup(&root)?;
    for mutation in 0..5 {
        let mut info = original.clone();
        match mutation {
            0 => info.endpoint.service_instance = Incarnation::from_bytes([9; 16]),
            1 => info.connection_id = Incarnation::from_bytes([9; 16]),
            2 => info.credential_file = root.join("foreign.token").to_string_lossy().into_owned(),
            3 => info.endpoint.executable_sha256 = Sha256Digest::from_bytes([9; 32]),
            _ => {
                if let HttpSelection::Scoped { selection } = &mut info.selection {
                    selection.context_key = Sha256Digest::from_bytes([9; 32]);
                }
            }
        }
        assert!(
            connection(
                ControlResponse::HttpConnection {
                    connection: Box::new(info)
                },
                &root,
                &health,
                original.connection_id,
                &original.selection
            )
            .is_err()
        );
    }
    let error = refused::<()>(ControlResponse::Failure {
        code: "fixture_complete_native_refusal".into(),
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("fixture_complete_native_refusal"));
    Ok(())
}
#[test]
fn http_setup_cli_accepts_protected_files_and_never_accepts_a_bearer_argument() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: RuntimeCommand,
    }
    let id = "01".repeat(16);
    let digest = "02".repeat(32);
    let existing = Arguments::try_parse_from([
        "airk-runtime",
        "http-connection-info",
        "--install-dir",
        "/installed",
        "--connection-id",
        &id,
    ])?;
    assert!(matches!(
        existing.command,
        RuntimeCommand::HttpConnectionInfo { .. }
    ));
    for forbidden in ["--token", "--credential-file", "--selection-file"] {
        assert!(
            Arguments::try_parse_from([
                "airk-runtime",
                "http-connection-info",
                "--install-dir",
                "/installed",
                "--connection-id",
                &id,
                forbidden,
                "untrusted",
            ])
            .is_err()
        );
    }
    let args = Arguments::try_parse_from([
        "airk-runtime",
        "http-legacy-connection",
        "--install-dir",
        "/installed",
        "--registration-file",
        "/private/registration.json",
        "--configuration-receipt",
        &digest,
        "--origin-sha256",
        &digest,
        "--connection-id",
        &id,
        "--client",
        "codex",
    ])?;
    assert!(matches!(
        args.command,
        RuntimeCommand::HttpLegacyConnection {
            origin_sha256: Some(_),
            ..
        }
    ));
    assert!(
        Arguments::try_parse_from([
            "airk-runtime",
            "http-connection",
            "--install-dir",
            "/installed",
            "--selection-file",
            "/private/selection.json",
            "--connection-id",
            &id,
            "--client",
            "codex",
            "--token",
            "secret"
        ])
        .is_err()
    );
    assert!(client("codex").is_ok());
    assert!(client("bad client").is_err());
    Ok(())
}

#[test]
fn enrollment_refreshes_only_catalog_clock_and_preserves_exact_authority() -> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("control");
    local_ipc::prepare_root(&root)?;
    let (health, original) = setup(&root)?;
    let mut refreshed = original.clone();
    if let HttpSelection::Scoped { selection } = &mut refreshed.selection {
        selection.expected_catalog_revision = CatalogRevision::INITIAL.next()?;
    }
    let expected = serde_json::to_value(&refreshed)?;
    assert_eq!(
        connection(
            ControlResponse::HttpConnection {
                connection: Box::new(refreshed.clone())
            },
            &root,
            &health,
            original.connection_id,
            &original.selection
        )?,
        expected
    );
    assert!(
        !same_authority(&refreshed.selection, &original.selection),
        "saved clock cannot move backwards"
    );
    for change in 0..5 {
        let mut crossed = refreshed.clone();
        if let HttpSelection::Scoped { selection } = &mut crossed.selection {
            match change {
                0 => selection.context_key = Sha256Digest::from_bytes([9; 32]),
                1 => selection.expected_grant_revision = CatalogRevision::INITIAL.next()?,
                2 => selection.project_ids.push(QueryProjectId::from_identity(
                    Incarnation::from_bytes([9; 16]),
                )),
                3 => selection.index_id = IndexId::from_identity(Incarnation::from_bytes([9; 16])),
                _ => selection.grant_id = GrantId::from_identity(Incarnation::from_bytes([9; 16])),
            }
        }
        assert!(!same_authority(&original.selection, &crossed.selection));
    }
    let mut crossed = original;
    crossed.endpoint.installation = Sha256Digest::from_bytes([9; 32]);
    assert!(check_endpoint(&root, &crossed.endpoint, &health).is_err());
    Ok(())
}

#[test]
fn combined_clock_refresh_preserves_the_explicit_index_subset_and_scope() -> Result<()> {
    let id = Incarnation::from_bytes([1; 16]);
    let original = HttpSelection::Combined {
        selection: airk_core::service::CombinedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([2; 32]),
            namespace: id,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(id),
            expected_grant_revision: CatalogRevision::INITIAL,
            project_ids: vec![QueryProjectId::from_identity(id)],
            index_ids: Some(vec![IndexId::from_identity(id)]),
        },
    };
    let mut next = original.clone();
    if let HttpSelection::Combined { selection } = &mut next {
        selection.expected_catalog_revision = CatalogRevision::INITIAL.next()?;
    }
    assert!(same_authority(&original, &next));
    assert!(!same_authority(&next, &original));
    if let HttpSelection::Combined { selection } = &mut next {
        selection.index_ids = None;
    }
    assert!(
        !same_authority(&original, &next),
        "refresh cannot widen an explicit index subset"
    );
    Ok(())
}

#[test]
fn user_trust_actions_require_an_explicit_cli_verb() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: RuntimeCommand,
    }
    for (text, expected) in [
        ("inspect", HttpTrustAction::Inspect),
        ("install", HttpTrustAction::Install),
        ("remove", HttpTrustAction::Remove),
    ] {
        let digest = "02".repeat(32);
        let mut args = vec![
            "airk-runtime",
            "http-trust",
            "--install-dir",
            "/installed",
            "--action",
            text,
        ];
        if expected != HttpTrustAction::Inspect {
            args.extend(["--expected-ca-sha256", digest.as_str()]);
        }
        let parsed = Arguments::try_parse_from(args)?;
        assert!(matches!(parsed.command,RuntimeCommand::HttpTrust{action,..} if action==expected));
    }
    assert!(
        Arguments::try_parse_from(["airk-runtime", "http-trust", "--install-dir", "/installed"])
            .is_err()
    );
    assert!(
        Arguments::try_parse_from([
            "airk-runtime",
            "http-trust",
            "--install-dir",
            "/installed",
            "--action",
            "automatic"
        ])
        .is_err()
    );
    Ok(())
}
#[tokio::test]
async fn trust_inspect_missing_installation_does_not_create_state_or_elect_a_service() -> Result<()>
{
    let parent = tempfile::tempdir()?;
    let missing = parent.path().join("not-installed");
    assert!(
        trust(&missing, HttpTrustAction::Inspect, None)
            .await
            .is_err()
    );
    assert!(!missing.exists());
    assert_eq!(std::fs::read_dir(parent.path())?.count(), 0);
    Ok(())
}

#[test]
fn reviewed_issuer_digest_is_required_for_mutations_and_rotation_is_refused() -> Result<()> {
    let first = Sha256Digest::from_bytes([1; 32]);
    let rotated = Sha256Digest::from_bytes([2; 32]);
    check_reviewed_ca(HttpTrustAction::Inspect, None, first)?;
    for action in [HttpTrustAction::Install, HttpTrustAction::Remove] {
        assert!(check_reviewed_ca(action, None, first).is_err());
        assert!(check_reviewed_ca(action, Some(first), rotated).is_err());
        check_reviewed_ca(action, Some(first), first)?;
    }
    Ok(())
}

#[test]
fn legacy_union_inputs_are_private_bounded_unique_and_have_no_default_project() -> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().join("private");
    local_ipc::prepare_root(&root)?;
    let file = root.join("registrations.json");
    let registrations=(0..2).map(|member|serde_json::json!({"registration":airk_core::service::ProjectRegistration::new(format!("/original/{member}"),format!("/saved/{member}"),"fixture".into()).unwrap(),"configuration_receipt":Sha256Digest::from_bytes([member+1;32])})).collect::<Vec<_>>();
    let input =
        serde_json::json!({"schema":1,"registrations":registrations,"no_default_project":true});
    let save = |value: &serde_json::Value| -> Result<()> {
        fs::write(&file, serde_json::to_vec(value)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    };
    save(&input)?;
    assert_eq!(
        airk_endpoint::http_endpoint::read_registrations_file(&file)?
            .registrations
            .len(),
        2
    );
    for mutation in 0..4 {
        let mut bad = input.clone();
        match mutation {
            0 => bad["no_default_project"] = false.into(),
            1 => bad["registrations"][1] = bad["registrations"][0].clone(),
            2 => bad["registrations"] = serde_json::json!([registrations[0].clone()]),
            _ => bad["extra"] = "x".repeat(49_152).into(),
        };
        save(&bad)?;
        assert!(airk_endpoint::http_endpoint::read_registrations_file(&file).is_err());
    }
    assert!(
        airk_endpoint::http_endpoint::read_registrations_file(Path::new("relative.json")).is_err()
    );
    Ok(())
}
#[test]
fn legacy_union_cli_requires_a_protected_original_registration_set() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: RuntimeCommand,
    }
    let id = "01".repeat(16);
    let parsed = Arguments::try_parse_from([
        "airk-runtime",
        "http-legacy-union-connection",
        "--install-dir",
        "/installed",
        "--registrations-file",
        "/private/original.json",
        "--connection-id",
        &id,
        "--client",
        "codex",
    ])?;
    assert!(matches!(
        parsed.command,
        RuntimeCommand::HttpLegacyUnionConnection { .. }
    ));
    assert!(
        Arguments::try_parse_from([
            "airk-runtime",
            "http-trust",
            "--install-dir",
            "/installed",
            "--action",
            "install"
        ])
        .is_err()
    );
    assert!(
        Arguments::try_parse_from([
            "airk-runtime",
            "http-trust",
            "--install-dir",
            "/installed",
            "--action",
            "remove"
        ])
        .is_err()
    );
    Ok(())
}
