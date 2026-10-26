// SPDX-License-Identifier: Apache-2.0
//! Vector policy clients preserve opt-out and let native source jobs apply selection.
mod common;
use airk_core::service::JobState;
use anyhow::Result;
use serde_json::json;
use std::fs;

#[test]
fn native_selection_preserves_opt_out_and_publishes_through_source_jobs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let source = root.path().join("original.rs");
    let original = b"pub fn original_selection_fixture() -> u32 { 7 }\n";
    fs::write(&source, original)?;
    let service = common::Service::start(root.path(), state.path(), "default");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    common::call(root.path(), state.path(), "default", &["index"])?;
    let initial = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "vector_selection"],
    )?;
    assert_eq!(initial["origin"], "default");
    assert_eq!(initial["pending_reindex"], false);
    let policy = json!({"version":1,"exclusions":[{"id":"selected-rust","basename":"original.rs","language":"rust"}]});
    let file = input.path().join("selection.json");
    fs::write(&file, policy.to_string())?;
    let preview = common::call(
        root.path(),
        state.path(),
        "default",
        &[
            "settings",
            "vector_selection",
            "--validate-file",
            file.to_str().unwrap(),
        ],
    )?;
    assert_eq!(preview["origin"], "preview");
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "vector_selection"]
        )?["digest"],
        initial["digest"]
    );
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["info", "original.rs"]
        )?["vector_selection"]["eligible"],
        true
    );
    let configured = common::configure_cli(
        &service,
        root.path(),
        state.path(),
        "default",
        json!({"operation":"vector_selection","policy":policy}),
    )?;
    assert_eq!(configured.state, JobState::Succeeded);
    common::call(root.path(), state.path(), "default", &["index"])?;
    let applied = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "vector_selection"],
    )?;
    assert_eq!(applied["digest"], preview["digest"]);
    assert_eq!(applied["applied_digest"], preview["digest"]);
    assert_eq!(applied["pending_reindex"], false);
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "vector_status"]
        )?["enabled"],
        false
    );
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["info", "original.rs"]
        )?["vector_selection"]["excluded_by"],
        "selected-rust"
    );
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: true });
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "vector_selection"]
        )?["digest"],
        preview["digest"]
    );
    assert_eq!(fs::read(source)?, original);
    Ok(())
}

#[test]
fn native_selection_rejects_invalid_bounded_inputs_and_supports_explicit_reset() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let service = common::Service::start(root.path(), state.path(), "default");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    let saved = common::configure_cli(
        &service,
        root.path(),
        state.path(),
        "default",
        json!({"operation":"vector_selection","policy":{"version":1,"exclusions":[{"id":"text","suffix":".txt"}]}}),
    )?;
    assert_eq!(saved.state, JobState::Succeeded);
    common::call(root.path(), state.path(), "default", &["index"])?;
    let before = common::call(
        root.path(),
        state.path(),
        "default",
        &["settings", "vector_selection"],
    )?;
    let file = input.path().join("selection.json");
    for invalid in [
        r#"{"version":3,"exclusions":[]}"#,
        r#"{"version":1,"exclusions":[],"unknown":true}"#,
        r#"{"version":1,"exclusions":[{"id":"all"}]}"#,
        r#"{"version":1,"exclusions":[{"id":"same","suffix":".txt"},{"id":"same","suffix":".rs"}]}"#,
        r#"{"version":1,"exclusions":[{"id":"outside","path_prefix":"../"}]}"#,
    ] {
        fs::write(&file, invalid)?;
        assert!(
            !common::command(root.path(), state.path(), "default")
                .args(["settings", "vector_selection", "--validate-file"])
                .arg(&file)
                .output()?
                .status
                .success()
        );
        assert!(common::configure_cli(&service, root.path(), state.path(), "default",
            json!({"operation":"vector_selection","policy":serde_json::from_str::<serde_json::Value>(invalid)?})).is_err());
    }
    fs::write(&file, vec![b' '; 131073])?;
    assert!(
        !common::command(root.path(), state.path(), "default")
            .args(["settings", "vector_selection", "--validate-file"])
            .arg(&file)
            .output()?
            .status
            .success()
    );
    let oversized = json!({"operation":"vector_selection","policy":{"version":1,"exclusions":(0..512).map(|id|json!({"id":format!("rule-{id}"),"path_prefix":"original-".repeat(16)})).collect::<Vec<_>>()}});
    assert!(
        common::configure_cli(&service, root.path(), state.path(), "default", oversized).is_err()
    );
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "vector_selection"]
        )?["digest"],
        before["digest"]
    );
    let wrong = "fe".repeat(32);
    assert!(
        !common::native_command()
            .arg("--service-runtime-dir")
            .arg(&service.control)
            .args(["--service-project", &wrong, "settings", "vector_selection"])
            .output()?
            .status
            .success()
    );
    let reset = common::configure_cli(
        &service,
        root.path(),
        state.path(),
        "default",
        json!({"operation":"vector_selection","policy":{"version":1,"exclusions":[]}}),
    )?;
    assert_eq!(reset.state, JobState::Succeeded);
    assert_eq!(
        common::call(
            root.path(),
            state.path(),
            "default",
            &["settings", "vector_selection"]
        )?["policy"]["exclusions"],
        json!([])
    );
    Ok(())
}
