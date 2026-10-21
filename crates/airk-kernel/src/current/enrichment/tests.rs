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
    let quota = Quota::open(&kernel.data_root)?;
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
