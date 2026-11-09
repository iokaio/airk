// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn standalone_setup_accepts_explicit_scope_without_serialized_grant_json() -> Result<()> {
    on_cli_stack(|| {
        let project = format!("prj_{}", "21".repeat(16));
        let index = format!("idx_{}", "22".repeat(16));
        let cli = Cli::try_parse_from([
            "airk",
            "setup",
            "--client",
            "codex",
            "--project",
            &project,
            "--index",
            &index,
            "--yes",
            "--launch",
            "--",
            "--model",
            "a model with spaces",
        ])?;
        cli.validate_setup()?;
        let Command::Setup {
            client: Some(SetupClient::Codex),
            standalone,
            ..
        } = cli.command
        else {
            anyhow::bail!("standalone setup not parsed")
        };
        assert_eq!(standalone.projects, vec![project.parse()?]);
        assert_eq!(standalone.indexes, vec![index.parse()?]);
        assert!(standalone.yes && standalone.launch);
        assert_eq!(
            standalone.client_args,
            vec![
                std::ffi::OsString::from("--model"),
                std::ffi::OsString::from("a model with spaces")
            ]
        );
        assert!(
            Cli::try_parse_from([
                "airk",
                "setup",
                "--create-root",
                "source",
                "--watch",
                "--yes"
            ])?
            .validate_setup()
            .is_ok()
        );
        Ok(())
    })
}

#[test]
fn standalone_bootstrap_requires_a_distributor_pin_and_preserves_scope_modes() -> Result<()> {
    on_cli_stack(|| {
        let digest = "23".repeat(32);
        assert!(Cli::try_parse_from(["airk", "setup", "--runtime-source", "payload"]).is_err());
        assert!(Cli::try_parse_from(["airk", "setup", "--approved-sha256", &digest]).is_err());
        let cli = Cli::try_parse_from([
            "airk",
            "setup",
            "--runtime-source",
            "payload",
            "--approved-sha256",
            &digest,
            "--install-only",
            "--yes",
        ])?;
        cli.validate_setup()?;
        let Command::Setup { standalone, .. } = cli.command else {
            anyhow::bail!("setup missing")
        };
        assert!(standalone.install_only && standalone.yes);
        assert_eq!(standalone.approved_sha256, Some(digest.parse()?));
        assert!(Cli::try_parse_from(["airk", "setup", "--install-only"]).is_err());
        assert!(Cli::try_parse_from(["airk", "setup", "--rebuild-indexes"]).is_err());
        assert!(Cli::try_parse_from(["airk", "setup", "--storage", "data"]).is_err());
        assert!(Cli::try_parse_from(["airk", "setup", "--watch"]).is_err());
        assert!(
            Cli::try_parse_from(["airk", "uninstall", "--yes"])?
                .validate_setup()
                .is_ok()
        );
        Ok(())
    })
}

#[test]
fn standalone_existing_connection_and_frozen_sessions_cannot_silently_change_scope() -> Result<()> {
    on_cli_stack(|| {
        let id = "24".repeat(16);
        let project = format!("prj_{}", "25".repeat(16));
        let cli = Cli::try_parse_from([
            "airk",
            "--service-install-dir",
            "install",
            "setup",
            "--existing-connection-id",
            &id,
            "--project",
            &project,
            "--yes",
        ])?;
        assert!(cli.validate_setup().is_err());
        let cli = Cli::try_parse_from([
            "airk",
            "--service-runtime-dir",
            "control",
            "setup",
            "--project",
            &project,
            "--yes",
            "--launch",
        ])?;
        assert!(cli.validate_setup().is_err());
        assert!(
            Cli::try_parse_from(["airk", "setup", "--", "unrequested child argument"]).is_err()
        );
        let session=json!({"schema":1,"context_key":"26".repeat(32),"namespace":id,"expected_catalog_revision":1,"grant_id":format!("grt_{id}"),"expected_grant_revision":1,"index_id":format!("idx_{id}"),"project_ids":[project]}).to_string();
        let cli = Cli::try_parse_from([
            "airk",
            "--catalog-session",
            &session,
            "setup",
            "--create-root",
            "source",
        ])?;
        assert!(cli.validate_setup().is_err());
        Ok(())
    })
}
