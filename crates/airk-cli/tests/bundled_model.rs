// SPDX-License-Identifier: Apache-2.0
//! Embedded inference and explicit custom models preserve source and storage boundaries.
mod common;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, process::Command};

#[cfg(all(unix, feature = "bundled-model", feature = "local-inference"))]
#[test]
fn embedded_model_smoke_creates_private_runtime_under_inherited_unix_umasks() -> Result<()> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};

    let owner = tempfile::tempdir()?;
    fs::set_permissions(owner.path(), fs::Permissions::from_mode(0o755))?;
    for mask in [0o022, 0o002, 0o000] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
        command
            .arg("--verify-model-smoke")
            .env("TMPDIR", owner.path());
        // SAFETY: only the spawned child's umask changes; umask is async-signal-safe
        // and the closure neither allocates nor touches the parent test's state.
        unsafe {
            command.pre_exec(move || {
                libc::umask(mask);
                Ok(())
            });
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "embedded smoke failed under Unix umask {mask:o}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["source"], "embedded");
        assert_eq!(report["dimensions"], 384);
        assert_eq!(report["repeat_equal"], true);
        assert_eq!(
            fs::metadata(owner.path())?.permissions().mode() & 0o777,
            0o755
        );
        assert!(fs::read_dir(owner.path())?.next().is_none());
    }
    Ok(())
}

fn assert_no_extracted_default_assets(control: &std::path::Path) -> Result<()> {
    let directory = control.join("model-files-1.4");
    if !directory.try_exists()? {
        return Ok(());
    }
    // Engine ownership initializes locks/quota metadata even with zero model
    // admissions. Actual default extraction would publish a default receipt or
    // immutable set containing model/tokenizer assets, neither of which is allowed.
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        ensure!(
            !entry.file_name().to_string_lossy().starts_with("default-"),
            "default model receipt was materialized"
        );
        ensure!(
            !matches!(
                entry.file_name().to_str(),
                Some("model.json" | "model.onnx" | "vocabulary.json" | "tokenizer.json")
            ),
            "model asset was extracted into the ownership metadata directory"
        );
        ensure!(
            !entry.file_type()?.is_symlink(),
            "redirected model ownership metadata"
        );
    }
    if directory.join("sets").try_exists()? {
        ensure!(
            fs::read_dir(directory.join("sets"))?.next().is_none(),
            "default model assets were extracted into an installed set"
        );
    }
    Ok(())
}

#[test]
fn copied_main_exposes_catalog_and_reports_its_actual_embedded_inference_capability() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let data = root.path().join("missing-state");
    let binary = owner.path().join(airk_cli::install_files::main_name());
    fs::create_dir_all(binary.parent().unwrap())?;
    fs::copy(env!("CARGO_BIN_EXE_airk"), &binary)?;
    // `model-catalog` is an airkctl command after the split (M10, covered by
    // `airkctl_model_catalog_matches_airk`); the copied image's own claim is
    // its embedded inference inventory, checked by the self-test below.
    let self_test = Command::new(&binary)
        .args(["__airk_role", "setup-interim", "runtime", "self-test"])
        .output()?;
    #[cfg(feature = "local-inference")]
    assert!(
        self_test.status.success(),
        "shipping main embeds its verified inference inventory: {}",
        String::from_utf8_lossy(&self_test.stderr)
    );
    #[cfg(not(feature = "local-inference"))]
    assert!(
        !self_test.status.success(),
        "lightweight main lacks inference"
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
fn opted_out_native_indexing_serves_lexical_evidence_without_loading_a_model() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.rs"),
        "pub fn offline_split_fixture() {}\n",
    )?;
    let service = common::Service::reviewed(root.path(), data.path(), "disabled");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    // Native setup can admit metadata provisioning/Close envelopes before the
    // opt-out is applied. Drain accepted setup work and prove none loaded a
    // model, then measure the subsequent explicit Index/read independently.
    service.settle_reviewed_index(
        root.path(),
        data.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    #[cfg(feature = "local-inference")]
    let baseline = match service.request(Method::EmbeddingStatus) {
        Response::EmbeddingStatus {
            available: true,
            process_id: None,
            loads: 0,
            sessions: 0,
            provider: Some(provider),
            ..
        } => {
            assert_eq!(provider.queued_outstanding, 0);
            assert_eq!(provider.running_outstanding, 0);
            assert_eq!(provider.callback_outstanding, 0);
            assert_eq!(provider.delivery_outstanding, 0);
            assert_eq!(provider.model_references_outstanding, 0);
            provider
        }
        observed => anyhow::bail!(
            "opt-out setup loaded/admitted active model work: {}",
            serde_json::to_string(&observed)?
        ),
    };
    assert_no_extracted_default_assets(&service.control)?;
    service.job(JobKind::Index);
    let found = common::scoped_call(
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
    assert_eq!(found["execution"]["served_mode"], "lexical");
    let status = common::scoped_call(
        root.path(),
        data.path(),
        "disabled",
        &[
            "tools",
            "call",
            "airk_status",
            "--args",
            r#"{"response_mode":"full"}"#,
        ],
    )?;
    let status: Value = serde_json::from_str(
        status["content"][0]["text"]
            .as_str()
            .context("missing full opt-out status")?,
    )?;
    assert_eq!(status["indexed_vectors"], 0);
    let Response::EmbeddingStatus {
        available,
        process_id,
        loads,
        sessions,
        provider,
        ..
    } = service.request(Method::EmbeddingStatus)
    else {
        anyhow::bail!("embedding observation missing");
    };
    assert!(process_id.is_none());
    assert_eq!(loads, 0);
    assert_eq!(sessions, 0);
    #[cfg(feature = "local-inference")]
    {
        let provider = provider.expect("shipping service must report its in-process provider");
        assert!(available && provider.admission_open && provider.failure.is_none());
        assert_eq!(
            provider.kind,
            airk_core::inference_provider::ProviderKind::InProcess
        );
        assert_eq!(
            provider.provider, baseline.provider,
            "provider generation changed during opt-out observation"
        );
        // Accounting::admit increments this monotonically for EVERY protocol
        // envelope, including metadata and Close; it is not concurrent occupancy.
        // Same-provider equality proves this Index/read admitted no new envelope.
        assert_eq!(provider.admitted_high_water, baseline.admitted_high_water);
        assert_eq!(provider.queued_outstanding, 0);
        assert_eq!(provider.running_outstanding, 0);
        assert_eq!(provider.callback_outstanding, 0);
        assert_eq!(provider.delivery_outstanding, 0);
        assert_eq!(provider.model_references_outstanding, 0);
    }
    #[cfg(not(feature = "local-inference"))]
    assert!(!available && provider.is_none());
    assert!(!data.path().join("models/local").exists());
    assert_no_extracted_default_assets(&service.control)?;
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn graceful_in_process_provider_stop_preserves_lexical_project_reads() -> Result<()> {
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
    let service = common::Service::reviewed(root.path(), data.path(), "idle-stop");
    // The reviewed setup already schedules initial model/index work. Observe
    // that accepted work before capturing the custom model settings guard;
    // replaying a failed Configure would hide a real stale-preview refusal.
    service.settle_reviewed_index(
        root.path(),
        data.path(),
        Instant::now() + Duration::from_secs(90),
    )?;
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
                loads,
                ..
            } if loads > 0 => break worker,
            // Publication proves completed inference. Observe the same provider
            // generation rather than assuming a separate helper process exists.
            Response::EmbeddingStatus {
                available: true,
                worker: Some(_),
                loads: 0,
                ..
            } if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Response::Failure { code }
                if code == "embedding_observation_busy" && Instant::now() < until =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            observed => anyhow::bail!(
                "idle provider unavailable: {}",
                serde_json::to_string(&observed)?
            ),
        }
    };
    let Response::Health {
        observation: health,
    } = service.request(Method::Health)
    else {
        anyhow::bail!("service health missing before provider stop");
    };
    // Submit this mutation exactly once. An uncertain response authorizes only
    // observing drain completion, never another StopEmbedding request.
    let stopped = service.request(Method::StopEmbedding {
        worker,
        force: false,
    });
    assert!(
        matches!(stopped, Response::EmbeddingStopped { worker: stopped } if stopped == worker)
            || matches!(&stopped, Response::Failure { code } if code == "embedding_stop_outcome_unknown")
    );
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        match service.request(Method::EmbeddingStatus) {
            Response::EmbeddingStatus {
                available: false,
                process_id: None,
                worker: None,
                ..
            } => break,
            Response::EmbeddingStatus {
                process_id: None,
                worker: Some(observed),
                ..
            } if observed == worker => {}
            Response::Failure { code } if code == "embedding_observation_busy" => {}
            observed => anyhow::bail!(
                "crossed provider drain: {}",
                serde_json::to_string(&observed)?
            ),
        }
        ensure!(Instant::now() < until, "provider drain did not settle");
        std::thread::sleep(Duration::from_millis(25));
    }
    let Response::Health { observation: after } = service.request(Method::Health) else {
        anyhow::bail!("service health missing after provider stop");
    };
    assert_eq!(after.instance, health.instance);
    assert_eq!(after.executable_sha256, health.executable_sha256);
    assert_eq!(after.runtime_sha256, health.runtime_sha256);
    let lexical = common::scoped_call(
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

#[cfg(feature = "local-inference")]
#[test]
fn embedded_default_setup_is_shared_without_extraction_and_preserves_custom_inputs() -> Result<()> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState,
    };
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.rs"),
        "pub fn shared_offline_fixture() -> bool { true }\n",
    )?;
    let service = common::Service::reviewed(root.path(), data.path(), "one");
    let second_root = tempfile::tempdir()?;
    let second_data = tempfile::tempdir()?;
    fs::write(
        second_root.path().join("second.txt"),
        "shared offline fixture\n",
    )?;
    let second = service.additional_project(second_root.path(), second_data.path(), "two")?;
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
    assert_eq!(service.wait(first.id).state, JobState::Succeeded);
    assert_eq!(service.wait(second_job.id).state, JobState::Succeeded);
    let objects = airk_cli::bundled_assets::EmbeddedAssets::plan()?;
    // Both actual canonical projects completed default setup through this one
    // service. The shipping default lives in its main image, not a shared
    // extracted weight directory or a helper process.
    assert_no_extracted_default_assets(&service.control)?;
    assert!(!data.path().join("models/local").exists());
    assert!(!second_data.path().join("models/local").exists());
    service.job(JobKind::Index);
    let status = service.wait_semantic(1)?;
    assert_eq!(
        status["semantic_readiness"]["pipeline_digest"],
        objects.pipeline_digest.to_string()
    );
    let Response::EmbeddingStatus {
        available: true,
        process_id: None,
        loads,
        provider: Some(provider),
        ..
    } = service.request(Method::EmbeddingStatus)
    else {
        anyhow::bail!("embedded default inference provider missing");
    };
    assert!(loads > 0 && provider.admitted_high_water > 0);
    assert_eq!(
        provider.kind,
        airk_core::inference_provider::ProviderKind::InProcess
    );
    assert_no_extracted_default_assets(&service.control)?;
    let custom = input.path().join("custom");
    fs::create_dir(&custom)?;
    // Deliberate caller-owned extraction exercises the supported explicit
    // directory-model route; default setup itself must never perform this.
    airk_cli::bundled_assets::extract(&custom)?;
    for (name, asset) in &objects.assets {
        let bytes = fs::read(custom.join(name.filename()))?;
        assert_eq!(bytes.len() as u64, asset.bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            asset.sha256.to_string()
        );
    }
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
    // An edited ONNX definition must fail before replacing the working model.
    let mut definition: Value = serde_json::from_slice(&original_definition)?;
    definition["max_tokens"] = 128.into();
    let edited_definition = serde_json::to_vec(&definition)?;
    fs::write(custom.join("model.json"), &edited_definition)?;
    let rejected = common::configure_cli(
        &service,
        root.path(),
        data.path(),
        "one",
        serde_json::json!({"operation":"install_model","directory":custom}),
    )?;
    assert_eq!(rejected.state, JobState::Failed);
    assert_eq!(fs::read(custom.join("model.json"))?, edited_definition);
    assert_eq!(
        service.wait_semantic(1)?["semantic_readiness"]["pipeline_digest"],
        objects.pipeline_digest.to_string()
    );
    fs::write(custom.join("model.json"), &original_definition)?;
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
    assert_eq!(
        service.wait_semantic(1)?["semantic_readiness"]["pipeline_digest"],
        objects.pipeline_digest.to_string()
    );
    assert!(!data.path().join("models/local").exists());
    Ok(())
}
