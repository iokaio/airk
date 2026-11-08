// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn existing_setup_connection_requires_installation_and_refuses_enrollment_inputs() {
    on_cli_stack(|| {
        let id = "03".repeat(16);
        for parent_first in [false, true] {
            let global = ["--service-install-dir", "owned-install"];
            let local = [
                "setup",
                "--client",
                "codex",
                "--existing-connection-id",
                id.as_str(),
            ];
            let arguments = std::iter::once("airk").chain(
                if parent_first {
                    global.iter().chain(local.iter())
                } else {
                    local.iter().chain(global.iter())
                }
                .copied(),
            );
            let cli = Cli::try_parse_from(arguments).unwrap();
            cli.validate_setup().unwrap();
            assert!(matches!(
                cli.command,
                Command::Setup {
                    existing_connection_id: Some(_),
                    connection_id: None,
                    ..
                }
            ));
        }
        let missing =
            Cli::try_parse_from(["airk", "setup", "--existing-connection-id", &id]).unwrap();
        assert!(missing.validate_setup().is_err());
        for extra in [
            vec!["--connection-id", id.as_str()],
            vec!["--explorer-descriptor", "descriptor.json"],
            vec!["--service-runtime-dir", "control"],
        ] {
            let mut arguments = vec![
                "airk",
                "--service-install-dir",
                "owned-install",
                "setup",
                "--existing-connection-id",
                id.as_str(),
            ];
            arguments.extend(extra);
            assert!(!Cli::try_parse_from(arguments).is_ok_and(|cli| cli.validate_setup().is_ok()));
        }
        let scoped=json!({"schema":1,"context_key":"04".repeat(32),"namespace":id,"expected_catalog_revision":1,"grant_id":format!("grt_{id}"),"expected_grant_revision":1,"index_id":format!("idx_{id}"),"project_ids":[format!("prj_{id}")]}).to_string();
        for before in [false, true] {
            let mut argv = vec!["airk", "--service-install-dir", "owned-install"];
            if before {
                argv.extend(["--catalog-session", scoped.as_str()]);
            }
            argv.extend(["setup", "--existing-connection-id", id.as_str()]);
            if !before {
                argv.extend(["--catalog-session", scoped.as_str()]);
            }
            assert!(!Cli::try_parse_from(argv).is_ok_and(|cli| cli.validate_setup().is_ok()));
        }
    });
}

#[test]
fn setup_outputs_https_and_environment_references_without_launcher_or_token_bytes() -> Result<()> {
    let id = "03".repeat(16);
    let digest = "04".repeat(32);
    let connection: airk_core::service_http::HttpConnectionInfo = serde_json::from_value(json!({
        "schema":1,"connection_id":id,"endpoint":{"schema":1,"installation":digest,"user":"501",
            "namespace":id,"generation":1,"service_instance":id,"executable_sha256":digest,
            "url":"https://127.0.0.1:57422/mcp","ca_sha256":digest,"certificate_sha256":digest,
            "state":"ready","predecessor_digest":null},
        "selection":{"kind":"scoped","selection":{"schema":1,"context_key":digest,"namespace":id,
            "expected_catalog_revision":1,"grant_id":format!("grt_{id}"),"expected_grant_revision":1,"index_id":format!("idx_{id}"),"project_ids":[format!("prj_{id}")]}},
        "ca_certificate_file":"/private/http-1/ca.pem","credential_file":"/private/http-1/credential.token"
    }))?;
    for client in [SetupClient::Codex, SetupClient::Claude, SetupClient::Vscode] {
        let (configuration, environment) = http_configuration(&connection, client)?;
        assert!(configuration.contains("https://127.0.0.1:57422/mcp"));
        if matches!(client, SetupClient::Vscode) {
            let config: Value = serde_json::from_str(&configuration)?;
            assert_eq!(
                config["servers"]["airk"]["headers"]["Authorization"],
                format!("Bearer ${{input:airkToken_{id}}}")
            );
            assert!(config["servers"]["airk"].get("command").is_none());
            assert_eq!(
                config["inputs"],
                json!([{"id":format!("airkToken_{id}"),
                "type":"command","command":"airk.httpCredential","args":{"connection_id":id}}])
            );
            assert!(environment["token_variable"].is_null());
            assert_eq!(environment["requires_airk_extension"], true);
        } else {
            assert!(configuration.contains("AIRK_MCP_TOKEN_03030303030303030303030303030303"));
            assert!(!configuration.contains("command"));
        }
        assert!(!configuration.contains("credential.token"));
        assert_eq!(environment["preserve_existing_trust"], true);
        assert_eq!(environment["token_source_file"], connection.credential_file);
    }
    for client in [
        SetupClient::Cursor,
        SetupClient::Gemini,
        SetupClient::Cline,
        SetupClient::Roo,
        SetupClient::Junie,
        SetupClient::CopilotCli,
        SetupClient::Opencode,
        SetupClient::OpencodeV2,
        SetupClient::Kilo,
    ] {
        assert!(http_configuration(&connection, client).is_err());
    }
    Ok(())
}
