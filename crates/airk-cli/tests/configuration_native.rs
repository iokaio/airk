// SPDX-License-Identifier: Apache-2.0
//! Shared quota changes cannot overwrite another project's intervening policy.
mod common;
use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind, JobState};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::fs;

#[test]
fn shared_quota_configuration_rejects_a_stale_project_preview_and_preserves_legacy_state()
-> Result<()> {
    let source = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let inputs = tempfile::tempdir()?;
    fs::write(source.path().join("source.txt"), "Original polar source\n")?;
    fs::write(other.path().join("other.txt"), "Original other source\n")?;
    fs::create_dir(data.path().join(".quota-1.2"))?;
    fs::write(
        data.path().join(".quota-1.2/policy.json"),
        "Preserve legacy quota bytes",
    )?;
    let service = common::Service::start(source.path(), data.path(), "quota-a");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let before = common::published_status(source.path(), data.path(), "quota-a")?;
    let initial = common::call(
        source.path(),
        data.path(),
        "quota-a",
        &["settings", "quota_policy"],
    )?;
    let project =
        airk_kernel::service_projects::registration(other.path(), data.path(), "quota-b")?;
    assert!(matches!(
        service.request(Method::Register {
            project: project.clone()
        }),
        Response::Registered { .. }
    ));
    service.job(JobKind::Inspect);
    let policy_a = inputs.path().join("quota-a.json");
    let policy_b = inputs.path().join("quota-b.json");
    let mut a = initial.clone();
    a["root_bytes"] = json!(initial["root_bytes"].as_u64().unwrap() + (1 << 20));
    let mut b = initial.clone();
    b["root_bytes"] = json!(initial["root_bytes"].as_u64().unwrap() + (2 << 20));
    fs::write(&policy_a, a.to_string())?;
    fs::write(&policy_b, b.to_string())?;
    let draft_a = inputs.path().join("draft-a.json");
    let draft_b = inputs.path().join("draft-b.json");
    fs::write(
        &draft_a,
        json!({"operation":"quota_policy","path":policy_a}).to_string(),
    )?;
    fs::write(
        &draft_b,
        json!({"operation":"quota_policy","path":policy_b}).to_string(),
    )?;
    let prepared = common::native_command()
        .args(["service", "requests", "--runtime-dir"])
        .arg(&service.control)
        .args([
            "prepare-configuration",
            "--project",
            &service.project.binding.to_string(),
            "--file",
        ])
        .arg(&draft_a)
        .output()?;
    ensure!(
        prepared.status.success(),
        "{}",
        String::from_utf8_lossy(&prepared.stderr)
    );
    let prepared: Value = serde_json::from_slice(&prepared.stdout)?;
    let nonce = prepared["prepared"].as_str().unwrap();
    let changed = common::native_command()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .args([
            "--service-project",
            &project.binding.to_string(),
            "configure",
            "--file",
        ])
        .arg(&draft_b)
        .output()?;
    ensure!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed: Value = serde_json::from_slice(&changed.stdout)?;
    assert_eq!(
        service
            .wait_outcome(serde_json::from_value(changed["job"]["id"].clone())?)
            .state,
        JobState::Succeeded
    );
    let retried = common::native_command()
        .args(["service", "requests", "--runtime-dir"])
        .arg(&service.control)
        .args(["retry", nonce])
        .output()?;
    ensure!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    let retried: Value = serde_json::from_slice(&retried.stdout)?;
    assert_eq!(
        service
            .wait_outcome(serde_json::from_value(retried["job"]["id"].clone())?)
            .state,
        JobState::Failed
    );
    assert_eq!(
        common::call(
            source.path(),
            data.path(),
            "quota-a",
            &["settings", "quota_policy"]
        )?,
        b
    );
    assert_eq!(
        common::published_status(source.path(), data.path(), "quota-a")?["publication"],
        before["publication"]
    );
    assert_eq!(
        fs::read_to_string(data.path().join(".quota-1.2/policy.json"))?,
        "Preserve legacy quota bytes"
    );
    assert_eq!(
        fs::read_to_string(source.path().join("source.txt"))?,
        "Original polar source\n"
    );
    assert_eq!(
        fs::read_to_string(other.path().join("other.txt"))?,
        "Original other source\n"
    );
    Ok(())
}
