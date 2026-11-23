// SPDX-License-Identifier: Apache-2.0
use super::*;

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
