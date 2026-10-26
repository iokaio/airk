// SPDX-License-Identifier: Apache-2.0
//! Lightweight metadata and separately verified offline inference have distinct boundaries.
mod common;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn copied_lightweight_main_exposes_catalog_without_installing_or_falling_back_to_models()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let data = root.path().join("missing-state");
    let binary = owner.path().join(airk_cli::runtime_install::main_name());
    fs::create_dir_all(binary.parent().unwrap())?;
    fs::copy(env!("CARGO_BIN_EXE_airk"), &binary)?;
    let report = Command::new(&binary)
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(&data)
        .arg("model-catalog")
        .env(
            "AIRK_BUNDLED_MODEL_DIR",
            root.path().join("missing-build-inputs"),
        )
        .output()?;
    ensure!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    let report: Value = serde_json::from_slice(&report.stdout)?;
    assert_eq!(
        report["bundled_model"]["manifest"],
        report["catalog"]["profiles"][0]["manifest"]
    );
    assert!(
        !data.exists(),
        "catalog reads must not install assets or create state"
    );
    let absent = Command::new(&binary)
        .args(["runtime", "self-test"])
        .output()?;
    assert!(
        !absent.status.success(),
        "a copied main cannot claim a complete inference pair"
    );
    assert!(
        !owner
            .path()
            .join(if cfg!(windows) {
                "airk-embed.exe"
            } else {
                "airk-embed"
            })
            .exists()
    );
    assert!(!data.exists());
    Ok(())
}

#[test]
fn opted_out_native_indexing_serves_lexical_evidence_without_starting_inference() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.rs"),
        "pub fn offline_split_fixture() {}\n",
    )?;
    let service = common::Service::start(root.path(), data.path(), "disabled");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let found = common::call(
        root.path(),
        data.path(),
        "disabled",
        &[
            "tools",
            "call",
            "airk_search",
            "--args",
            r#"{"query":"offline_split_fixture","mode":"lexical"}"#,
        ],
    )?;
    let found: Value = serde_json::from_str(found["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(found["hits"][0]["citation"]["path"], "original.rs");
    let Response::EmbeddingStatus {
        available,
        process_id,
        ..
    } = service.request(Method::EmbeddingStatus)
    else {
        anyhow::bail!("embedding observation missing");
    };
    assert!(!available && process_id.is_none());
    assert!(!data.path().join("models/local").exists());
    assert!(!service.control.join("model-files-1.4").exists());
    Ok(())
}

#[cfg(feature = "embedding-worker")]
#[test]
fn graceful_helper_stop_closes_idle_project_sessions_before_the_control_deadline() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind};
    use sha2::{Digest, Sha256};
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let original = "authenticate login\n";
    fs::write(root.path().join("original.txt"), original)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.0,1.0]}"#;
    fs::write(input.path().join("vocabulary.json"), vocabulary)?;
    fs::write(
        input.path().join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original idle-session fixture","license":"Apache-2.0",
            "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2
        }))?,
    )?;
    let service = common::Service::with_helper(root.path(), data.path(), "idle-stop");
    service.configure(airk_cli::service_configuration::Draft::InstallModel {
        directory: input.path().to_path_buf(),
    });
    service.job(JobKind::Index);
    service.wait_semantic(1)?;
    let until = Instant::now() + Duration::from_secs(10);
    let worker = loop {
        match service.request(Method::EmbeddingStatus) {
            Response::EmbeddingStatus {
                available: true,
                worker: Some(worker),
                sessions,
                ..
            } => {
                assert!(sessions > 0, "the helper must retain an idle model session");
                break worker;
            }
            Response::Failure { code }
                if code == "embedding_observation_busy" && Instant::now() < until =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            observed => anyhow::bail!(
                "idle helper unavailable: {}",
                serde_json::to_string(&observed)?
            ),
        }
    };
    assert!(
        matches!(service.request(Method::StopEmbedding { worker, force: false }),
        Response::EmbeddingStopped { worker: stopped } if stopped == worker)
    );
    assert!(matches!(
        service.request(Method::EmbeddingStatus),
        Response::EmbeddingStatus {
            available: false,
            process_id: None,
            ..
        }
    ));
    let lexical = common::call(
        root.path(),
        data.path(),
        "idle-stop",
        &[
            "tools",
            "call",
            "airk_search",
            "--args",
            r#"{"query":"authenticate","mode":"lexical"}"#,
        ],
    )?;
    let lexical: Value = serde_json::from_str(lexical["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(lexical["hits"][0]["citation"]["path"], "original.txt");
    assert_eq!(
        fs::read_to_string(root.path().join("original.txt"))?,
        original
    );
    Ok(())
}

#[cfg(feature = "embedding-worker")]
#[test]
fn paired_offline_setup_shares_default_assets_and_preserves_explicit_custom_inputs() -> Result<()> {
    use airk_core::{
        model_objects::ModelObjectSet,
        service::{ControlMethod as Method, ControlResponse as Response, JobKind, JobState},
    };
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.rs"),
        "pub fn shared_offline_fixture() -> bool { true }\n",
    )?;
    let service = common::Service::with_helper(root.path(), data.path(), "one");
    let second = airk_kernel::service_projects::registration(root.path(), data.path(), "two")?;
    assert!(matches!(
        service.request(Method::Register {
            project: second.clone()
        }),
        Response::Registered { .. }
    ));
    let enqueue = |project| {
        let Response::Job { job } = service.request(Method::Enqueue {
            project,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: service.key(),
            kind: JobKind::ModelSetup,
        }) else {
            panic!("model setup not accepted");
        };
        job
    };
    let first = enqueue(service.project.binding);
    let second_job = enqueue(second.binding);
    service.wait(first.id);
    service.wait(second_job.id);
    let defaults = fs::read_dir(service.control.join("model-files-1.4"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with("default-")
                    && name.to_string_lossy().ends_with(".json")
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        defaults.len(),
        1,
        "compatible projects must share one installed default"
    );
    let objects = ModelObjectSet::from_bytes(&fs::read(&defaults[0])?)?;
    let installed =
        airk_store::service::model_files::InstalledModel::open(&service.control, &objects)?;
    installed.verify(1 << 30)?;
    // Verify the actual shared files, independently of their installed-set receipt.
    for (name, asset) in &objects.assets {
        let bytes = fs::read(installed.directory().join(name.filename()))?;
        assert_eq!(bytes.len() as u64, asset.bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            asset.sha256.to_string()
        );
    }
    assert!(!data.path().join("models/local").exists());
    let custom = input.path().join("custom");
    fs::create_dir(&custom)?;
    for name in objects.assets.keys() {
        fs::copy(
            installed.directory().join(name.filename()),
            custom.join(name.filename()),
        )?;
    }
    let mut definition: Value = serde_json::from_slice(&fs::read(custom.join("model.json"))?)?;
    definition["max_tokens"] = 128.into();
    fs::write(custom.join("model.json"), serde_json::to_vec(&definition)?)?;
    let original_definition = fs::read(custom.join("model.json"))?;
    let configured = common::configure_cli(
        &service,
        root.path(),
        data.path(),
        "one",
        serde_json::json!({"operation":"install_model","directory":custom}),
    )?;
    assert_eq!(configured.state, JobState::Succeeded);
    service.wait(enqueue(service.project.binding).id);
    assert_eq!(fs::read(custom.join("model.json"))?, original_definition);
    fs::write(custom.join("tokenizer.json"), b"broken fixture")?;
    let rejected = common::configure_cli(
        &service,
        root.path(),
        data.path(),
        "one",
        serde_json::json!({"operation":"install_model","directory":custom}),
    )?;
    assert_eq!(rejected.state, JobState::Failed);
    assert_eq!(fs::read(custom.join("tokenizer.json"))?, b"broken fixture");
    assert_eq!(fs::read(custom.join("model.json"))?, original_definition);
    installed.verify(1 << 30)?;
    assert!(!data.path().join("models/local").exists());
    Ok(())
}
