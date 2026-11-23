// SPDX-License-Identifier: Apache-2.0
use super::*;

/// Local trust plan B: setup installs the reviewed payload only. The interim
/// enrollment inputs (client, connection, scope, sessions, launch) no longer
/// parse, so no earlier invocation can silently enroll or launch.
#[test]
fn setup_no_longer_accepts_enrollment_scope_session_or_launch_inputs() -> Result<()> {
    on_cli_stack(|| {
        let id = "24".repeat(16);
        let project = format!("prj_{}", "25".repeat(16));
        let index = format!("idx_{}", "22".repeat(16));
        let session=json!({"schema":1,"context_key":"26".repeat(32),"namespace":id,"expected_catalog_revision":1,"grant_id":format!("grt_{id}"),"expected_grant_revision":1,"index_id":format!("idx_{id}"),"project_ids":[project]}).to_string();
        for removed in [
            vec!["airk", "setup", "--client", "codex"],
            vec!["airk", "setup", "--connection-id", &id],
            vec![
                "airk",
                "--service-install-dir",
                "install",
                "setup",
                "--existing-connection-id",
                &id,
            ],
            vec![
                "airk",
                "setup",
                "--explorer-descriptor",
                "/private/window.json",
            ],
            vec!["airk", "setup", "--project", &project],
            vec!["airk", "setup", "--index", &index],
            vec!["airk", "setup", "--create-root", "source"],
            vec!["airk", "setup", "--launch"],
            vec!["airk", "--catalog-session", &session, "setup"],
            vec!["airk", "--combined-session", &session, "setup"],
        ] {
            assert!(Cli::try_parse_from(&removed).is_err(), "{removed:?}");
        }
        assert!(
            Cli::try_parse_from(["airk", "setup", "--", "unrequested child argument"]).is_err()
        );
        // No payload: nothing to install, and nothing to validate.
        Cli::try_parse_from(["airk", "setup", "--yes"])?.validate_setup()?;
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
            Cli::try_parse_from([
                "airk",
                "--service-runtime-dir",
                "control",
                "setup",
                "--runtime-source",
                "payload",
                "--approved-sha256",
                &digest,
            ])?
            .validate_setup()
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["airk", "uninstall", "--yes"])?
                .validate_setup()
                .is_ok()
        );
        Ok(())
    })
}
