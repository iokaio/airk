// SPDX-License-Identifier: Apache-2.0
//! Shared quota changes cannot overwrite another project's intervening policy.
mod common;
use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind, JobState};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::fs;

#[test]
fn accepted_index_waits_for_restored_source_without_another_request() -> Result<()> {
    use std::time::{Duration, Instant};
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("source");
    let absent = owner.path().join("offline");
    fs::create_dir(&root)?;
    fs::write(root.join("original.txt"), "Preserve the restored source\n")?;
    let data = tempfile::tempdir()?;
    let service = common::Service::start(&root, data.path(), "restore-source");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    fs::rename(&root, &absent)?;
    let caller = airk_kernel::service_broker::new_instance_id()?;
    let key = service.key();
    let Response::Job { job } = service.request(Method::Enqueue {
        project: service.project.binding,
        caller,
        key: key.clone(),
        kind: JobKind::Index,
    }) else {
        anyhow::bail!("index not accepted")
    };
    let until = Instant::now() + Duration::from_secs(45);
    loop {
        if let Response::Console { page } = service.request(Method::Console {
            after: None,
            revision: None,
            limit: 16,
        }) && page.projects.iter().any(|p| {
            p.recovery.iter().any(|r| {
                r.job == Some(job.id) && r.reason == "source_unavailable" && r.state == "waiting"
            })
        }) {
            break;
        }
        ensure!(
            Instant::now() < until,
            "missing source did not produce a durable wait"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    fs::rename(&absent, &root)?;
    let completed = service.wait(job.id);
    assert_eq!(completed.id, job.id);
    assert_eq!(completed.attempts, 2);
    let Response::RequestReceipt {
        job: Some(observed),
        expired: false,
        removed_project: None,
    } = service.request(Method::RequestReceipt {
        caller,
        key,
        project: None,
    })
    else {
        anyhow::bail!("original receipt unavailable")
    };
    assert_eq!(observed.id, job.id);
    assert_eq!(
        fs::read(root.join("original.txt"))?,
        b"Preserve the restored source\n"
    );
    Ok(())
}

#[test]
#[cfg(feature = "embedding-worker")]
fn semantic_preference_accepted_while_paused_prepares_itself_and_publishes() -> Result<()> {
    use airk_core::service_configuration::{ConfigurationRequest, ProjectConfiguration};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("file.txt"),
        "Native durable semantic preference\n",
    )?;
    let service = common::Service::with_helper(root.path(), data.path(), "desired-semantic");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    service.request(Method::Pause {
        project: service.project.binding,
        paused: true,
    });
    let key = service.key();
    let caller = airk_kernel::service_broker::new_instance_id()?;
    let response = service.request(Method::ConfigureProject {
        project: service.project.binding,
        caller,
        request: Box::new(ConfigurationRequest {
            prepare: true,
            incarnation: key.namespace,
            settings: service.project.binding,
            configuration: ProjectConfiguration::Vectors { enabled: true },
        }),
        key,
    });
    let Response::Job { job } = response else {
        anyhow::bail!("durable preference rejected")
    };
    assert_eq!(job.state, JobState::Pending);
    // No extension inspection/preparation or follow-up enqueue is performed.
    service.request(Method::Pause {
        project: service.project.binding,
        paused: false,
    });
    service.wait(job.id);
    service.wait_semantic(1)?;
    assert_eq!(
        fs::read(root.path().join("file.txt"))?,
        b"Native durable semantic preference\n"
    );
    Ok(())
}

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

#[test]
fn reviewed_setup_creates_reserved_storage_and_publishes_text_without_editor_followup() -> Result<()>
{
    setup_without_editor(false)
}

#[test]
#[cfg(feature = "embedding-worker")]
fn reviewed_setup_publishes_semantic_readiness_without_editor_followup() -> Result<()> {
    setup_without_editor(true)
}

fn setup_without_editor(helper: bool) -> Result<()> {
    use std::time::{Duration, Instant};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let original = b"Native setup preserves this original source.\n";
    fs::write(other.path().join("original.txt"), original)?;
    let service = if helper {
        #[cfg(feature = "embedding-worker")]
        {
            common::Service::with_helper(source.path(), data.path(), "existing")
        }
        #[cfg(not(feature = "embedding-worker"))]
        {
            unreachable!("helper scenario requires embedding-worker")
        }
    } else {
        common::Service::start(source.path(), data.path(), "existing")
    };
    let absent = data.path().join("reserved").join("new-data");
    let project = airk_cli::service_registration::prepare(other.path(), &absent, "setup")?;
    assert!(!absent.exists());
    let reply = service.request(Method::ReserveProject {
        project: project.clone(),
        setup_watch: Some(false),
        only_if_missing: false,
    });
    ensure!(
        matches!(reply, Response::Registered { .. }),
        "setup registration failed"
    );
    // No prepare-storage, model_setup or index command follows this request.
    // Without a helper, model failure must leave text work queued. With one,
    // semantic work and its cached readiness must finish without UI inspection.
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        if let Response::Inspection {
            observation: Some(observation),
            ..
        } = service.request(Method::Inspection {
            project: project.binding,
        }) && observation.snapshot.index_state
            == airk_core::service_inspection::IndexState::Ready
            && observation.snapshot.indexed_files == Some(1)
            && (!helper
                || observation
                    .snapshot
                    .semantic
                    .as_ref()
                    .is_some_and(|semantic| {
                        semantic.state == airk_core::readiness::SemanticState::Ready
                    }))
        {
            assert!(observation.snapshot.index_detail.is_some());
            break;
        }
        ensure!(
            Instant::now() < until,
            "initial native setup did not publish text"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(absent.is_dir());
    assert_eq!(fs::read(other.path().join("original.txt"))?, original);
    Ok(())
}
