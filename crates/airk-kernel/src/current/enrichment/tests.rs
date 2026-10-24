// SPDX-License-Identifier: Apache-2.0
use super::*;
use sha2::{Digest, Sha256};
use std::{fs, io::ErrorKind};

fn pending_fixture() -> Result<(tempfile::TempDir, Kernel)> {
    // Model admission requires physical ancestors, including macOS /var aliases.
    let directory = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = directory.path().join("source");
    let data = directory.path().join("data");
    let model = data.join("models/local");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&model)?;
    fs::write(root.join("fixture.txt"), "authenticate\n")?;
    let vocabulary = serde_json::to_vec(&serde_json::json!({"authenticate":[1.0,0.0]}))?;
    fs::write(model.join("vocabulary.json"), &vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-semantic-failure-fixture","license":"Apache-2.0",
            "dimensions":2,"sha256":format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    let kernel = Kernel::open(&root, Some(data))?;
    kernel.scheduled_index(false, false)?;
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Pending
    );
    Ok((directory, kernel))
}

#[test]
fn bounded_drain_prioritizes_edits_and_docs_yields_on_pause_and_sweeps_once() -> Result<()> {
    let (_directory, kernel) = pending_fixture()?;
    for index in 0..64 {
        fs::write(
            kernel.root.join(format!("a_{index:02}.txt")),
            "authenticate\n",
        )?;
    }
    fs::write(
        kernel.root.join("README.md"),
        "# Authenticate\nauthenticate\n",
    )?;
    fs::write(kernel.root.join("z_recent.txt"), "authenticate\n")?;
    kernel.scheduled_index(false, false)?;
    fs::write(kernel.root.join("z_recent.txt"), "authenticate updated\n")?;
    assert!(
        kernel
            .scheduled_index_selected(false, false, Some(&["z_recent.txt".into()]), true)?
            .path_scoped
    );
    let orphan = kernel
        .store
        .directory()
        .join("objects/sources")
        .join("f".repeat(64));
    fs::write(&orphan, "unreferenced original fixture")?;
    let observed = orphan.clone();
    let early_sweep = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sweep_observation = early_sweep.clone();
    let with_progress = kernel.clone().with_index_progress(move |event| {
        if event.phase == "enrichment_committed" && !observed.exists() {
            sweep_observation.store(true, std::sync::atomic::Ordering::Release);
        }
    });
    let outcome =
        with_progress.run_scheduled_enrichment_bounded(8, Some(std::time::Duration::ZERO))?;
    assert_eq!(outcome.state, "enriched", "{:?}", outcome.error);
    let report = outcome.report.unwrap();
    assert_eq!(report.semantic_readiness.unwrap().completed_artifacts, 32);
    assert!(report.publish_us > 0);
    assert!(report.indexed_vectors.unwrap() > 0);
    assert!(report.maintenance_error.is_none());
    assert!(
        !early_sweep.load(std::sync::atomic::Ordering::Acquire),
        "sweep must wait for the drain boundary"
    );
    assert!(!orphan.exists());
    let read = kernel.current_reader()?;
    let work = read_work(&read, &kernel.store)?.unwrap();
    assert!(
        !work
            .pending
            .iter()
            .any(|pending| matches!(pending.path.as_str(), "README.md" | "z_recent.txt"))
    );
    work.validate()?;
    drop(read);
    let queue = airk_store::scheduler::Queue::new(&kernel.data_root);
    let owner = kernel.root_id();
    let paused = kernel.clone().with_index_progress(move |event| {
        if event.phase == "enrichment_committed" {
            queue.pause("default", &owner, true).unwrap();
        }
    });
    let outcome =
        paused.run_scheduled_enrichment_bounded(8, Some(std::time::Duration::from_secs(60)))?;
    assert_eq!(
        outcome
            .report
            .unwrap()
            .semantic_readiness
            .unwrap()
            .completed_artifacts,
        64
    );
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "paused");
    kernel.schedule_pause(false)?;
    let outcome =
        kernel.run_scheduled_enrichment_bounded(8, Some(std::time::Duration::from_secs(60)))?;
    assert_eq!(
        outcome.report.unwrap().semantic_readiness.unwrap().state,
        SemanticState::Ready
    );
    Ok(())
}

#[test]
fn secondary_gate_contention_preserves_primary_permission_and_corruption_errors() -> Result<()> {
    let (_directory, kernel) = pending_fixture()?;
    let read = kernel.current_reader()?;
    let token = read.token()?;
    let artifact = read.artifact("fixture.txt")?.unwrap();
    let path = kernel
        .store
        .directory()
        .join("objects/artifacts")
        .join(&artifact.retained.as_ref().unwrap().object);
    let bytes = fs::read(&path)?;
    fs::write(&path, b"corrupt extraction fixture")?;
    let corrupt = kernel.store.load_retained(&artifact).unwrap_err();
    fs::write(&path, bytes)?;
    let permission = anyhow::Error::new(std::io::Error::new(
        ErrorKind::PermissionDenied,
        "primary source permission fixture",
    ));
    for original in [permission, corrupt] {
        let original_message = format!("{original:#}");
        let permission = original
            .downcast_ref::<std::io::Error>()
            .map(|error| error.kind());
        let error = kernel.semantic_failure(original);
        assert!(
            error
                .downcast_ref::<crate::CurrentGateContention>()
                .is_none(),
            "{error:#}"
        );
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(|error| error.kind()),
            permission
        );
        let message = format!("{error:#}");
        assert!(message.contains(&original_message), "{message}");
        assert!(
            message.contains("record failed semantic enrichment"),
            "{message}"
        );
        assert!(
            message.contains("current index has active readers"),
            "{message}"
        );
        assert_eq!(read.token()?, token);
        assert!(read_work(&read, &kernel.store)?.unwrap().failure.is_none());
    }
    drop(read);
    assert_eq!(
        kernel.status()?.publication.unwrap().token,
        super::super::runtime::revision(token)
    );
    Ok(())
}

#[test]
fn drain_validation_failure_is_not_replaced_by_failure_recording_contention() -> Result<()> {
    let (_directory, kernel) = pending_fixture()?;
    let execution = crate::execution::acquire(&kernel.data_root)?;
    let read = kernel.current_reader()?;
    let configuration = read.collection_state()?.unwrap().configuration;
    let model = EmbeddingModel::open_objects(
        kernel.store.directory(),
        configuration.model.as_ref().unwrap(),
    )?;
    let mut selection = configuration.vector_selection;
    selection
        .exclusions
        .push(airk_core::vector_selection::VectorExclusionRule {
            id: "different-selection".into(),
            basename: Some("unrelated.txt".into()),
            ..Default::default()
        });
    let error = kernel
        .drain_with_failure_record(
            &model,
            &selection,
            &mut EmbeddingCacheStats::default(),
            1,
            None,
            DrainAdmission {
                execution: &execution,
                deadline: None,
            },
        )
        .unwrap_err();
    assert!(
        error
            .downcast_ref::<crate::CurrentGateContention>()
            .is_none(),
        "{error:#}"
    );
    assert!(
        format!("{error:#}").contains("pending semantic work differs from current configuration"),
        "{error:#}"
    );
    assert!(
        format!("{error:#}").contains("record failed semantic enrichment"),
        "{error:#}"
    );
    assert!(read_work(&read, &kernel.store)?.unwrap().failure.is_none());
    Ok(())
}

#[test]
fn resolved_gate_contention_does_not_publish_failed_semantic_state() -> Result<()> {
    let (_directory, kernel) = pending_fixture()?;
    let read = kernel.current_reader()?;
    let token = read.token()?;
    let scope = read.head().scope.clone();
    let quota = kernel.collection_quota()?;
    let writer = kernel.store.lock()?;
    let capacity = kernel.store.current_database_capacity(&scope)?;
    let original = kernel
        .store
        .current_write(&writer, &quota, &scope, capacity)
        .err()
        .context("fixture gate was not held")?;
    assert!(
        original
            .downcast_ref::<crate::CurrentGateContention>()
            .is_some()
    );
    drop(writer);
    drop(read);
    // Contention can clear before the failure handler runs. It still must not
    // turn temporary publication exclusion into durable failed semantic work.
    let error = kernel.semantic_failure(original);
    assert!(
        error
            .downcast_ref::<crate::CurrentGateContention>()
            .is_some()
    );
    let read = kernel.current_reader()?;
    assert_eq!(read.token()?, token);
    assert!(read_work(&read, &kernel.store)?.unwrap().failure.is_none());
    drop(read);
    kernel.resume_current_enrichment()?;
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    Ok(())
}

#[test]
fn model_open_failure_keeps_its_cause_when_recording_also_fails() -> Result<()> {
    let (_directory, kernel) = pending_fixture()?;
    let read = kernel.current_reader()?;
    let objects = read
        .collection_state()?
        .unwrap()
        .configuration
        .model
        .unwrap();
    let object = objects.assets.values().next().unwrap();
    let path = kernel
        .store
        .directory()
        .join("objects/models")
        .join(object.sha256.to_string());
    let bytes = fs::read(&path)?;
    fs::write(&path, vec![b'x'; bytes.len()])?;
    drop(read);
    let original = EmbeddingModel::open_objects(kernel.store.directory(), &objects)
        .err()
        .context("corrupt fixture model opened")?;
    let original_message = format!("{original:#}");
    for scheduled in [false, true] {
        let error = if scheduled {
            kernel.resume_current_enrichment_batches(1)
        } else {
            kernel.resume_current_enrichment()
        }
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(&original_message), "{message}");
        assert!(
            message.contains("record failed semantic enrichment"),
            "{message}"
        );
        assert!(
            error
                .downcast_ref::<crate::CurrentGateContention>()
                .is_none()
        );
    }
    assert_eq!(fs::read(&path)?, vec![b'x'; bytes.len()]);
    Ok(())
}
