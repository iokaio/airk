// SPDX-License-Identifier: Apache-2.0
//! Native configuration validates selected files and publishes through source jobs.
mod common;
use airk_core::service::{JobOutcome, JobState};
use anyhow::Result;
use serde_json::json;
use std::fs;

#[test]
fn selected_policy_preview_is_read_only_and_guarded_configuration_requests_publication()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let source = root.path().join("original.rs");
    let original = b"pub fn original_fixture() -> u32 { 7 }\n";
    fs::write(&source, original)?;
    let service = common::Service::start(root.path(), state.path(), "default");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    common::call(root.path(), state.path(), "default", &["index"])?;
    let before = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "extraction_policy"],
    )?;
    let policy = input.path().join("policy.json");
    fs::write(&policy, json!({"version":1,"rules":[{"selector":{"kind":"exact","path":"original.rs"},"case_sensitive":true,"adapter_id":"text"}]}).to_string())?;
    let preview = common::call(
        root.path(),
        state.path(),
        "default",
        &[
            "settings",
            "extraction_policy",
            "--validate-file",
            policy.to_str().unwrap(),
        ],
    )?;
    assert_eq!(preview["origin"], "preview");
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "extraction_policy"]
        )?["digest"],
        before["digest"]
    );
    let configured = common::configure_cli(
        &service,
        root.path(),
        state.path(),
        "default",
        json!({"operation":"extraction_policy","path":policy}),
    )?;
    assert!(matches!(
        configured.outcome,
        Some(JobOutcome::Succeeded { .. })
    ));
    common::call(root.path(), state.path(), "default", &["index"])?;
    let applied = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "extraction_policy"],
    )?;
    assert_eq!(applied["digest"], preview["digest"]);
    assert_eq!(applied["applied_digest"], preview["digest"]);
    assert_eq!(applied["pending_reindex"], false);
    assert_eq!(fs::read(source)?, original);
    Ok(())
}

#[test]
fn invalid_bounded_policy_files_and_foreign_bindings_preserve_configuration() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), "fn original() {}\n")?;
    let service = common::Service::start(root.path(), state.path(), "default");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    common::call(root.path(), state.path(), "default", &["index"])?;
    let before = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "extraction_policy"],
    )?;
    let policy = input.path().join("policy.json");
    for invalid in [
        r#"{"version":99,"rules":[]}"#,
        r#"{"version":1,"rules":[],"unknown":true}"#,
        r#"{"version":1,"rules":[{"selector":{"kind":"exact","path":"../outside"},"case_sensitive":true,"adapter_id":"text"}]}"#,
        "{",
        "[]",
    ] {
        fs::write(&policy, invalid)?;
        assert!(
            !common::command(root.path(), state.path(), "default")
                .args(["settings", "extraction_policy", "--validate-file"])
                .arg(&policy)
                .output()?
                .status
                .success()
        );
        let failed = common::configure_cli(
            &service,
            root.path(),
            state.path(),
            "default",
            json!({"operation":"extraction_policy","path":policy}),
        )?;
        assert_eq!(failed.state, JobState::Failed);
        assert_eq!(
            common::call(
                root.path(),
                state.path(),
                "default",
                &["settings", "extraction_policy"]
            )?["digest"],
            before["digest"]
        );
    }
    fs::write(&policy, vec![b' '; 131_073])?;
    assert!(
        common::configure_cli(
            &service,
            root.path(),
            state.path(),
            "default",
            json!({"operation":"extraction_policy","path":policy})
        )
        .is_err()
    );
    assert!(
        !common::command(root.path(), state.path(), "default")
            .arg("configure")
            .output()?
            .status
            .success()
    );
    let wrong = "ef".repeat(32);
    let output = common::native_command()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .args(["--service-project", &wrong, "settings", "extraction_policy"])
        .output()?;
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(root.path().join("original.rs"))?,
        "fn original() {}\n"
    );
    Ok(())
}
