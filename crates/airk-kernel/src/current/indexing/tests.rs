// SPDX-License-Identifier: Apache-2.0
use super::*;

type CaptureFault = Box<dyn FnMut(&airk_store::FileStore, &Artifact) -> Result<()>>;
thread_local! {
    static CAPTURE_FAULT: std::cell::RefCell<Option<CaptureFault>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn after_capture(store: &airk_store::FileStore, artifact: &Artifact) -> Result<()> {
    CAPTURE_FAULT.with_borrow_mut(|fault| match fault.as_mut() {
        Some(fault) => fault(store, artifact),
        None => Ok(()),
    })
}

struct CaptureFaultGuard;
impl Drop for CaptureFaultGuard {
    fn drop(&mut self) {
        CAPTURE_FAULT.with_borrow_mut(|fault| *fault = None);
    }
}

fn capture_fault(fault: CaptureFault) -> CaptureFaultGuard {
    CAPTURE_FAULT.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(fault);
    });
    CaptureFaultGuard
}

fn retained_object_path(
    store: &airk_store::FileStore,
    artifact: &Artifact,
    kind: &str,
) -> std::path::PathBuf {
    let reference = artifact.retained.as_ref().unwrap();
    let (digest, encoding) = if kind == "sources" {
        (&artifact.content_hash, reference.source_encoding.as_ref())
    } else {
        (&reference.object, reference.payload_encoding.as_ref())
    };
    store
        .directory()
        .join("objects")
        .join(kind)
        .join(encoding.map_or(digest, |encoding| &encoding.stored_digest))
}

#[test]
fn progressive_capture_loss_is_repaired_by_one_fresh_scan() -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    for kind in ["sources", "artifacts"] {
        for changed in [false, true] {
            let root = tempfile::tempdir()?;
            let data = model_data_directory()?;
            for index in 0..35 {
                std::fs::write(
                    root.path().join(format!("file_{index:02}.txt")),
                    format!(
                        "original fixture {index} {}\n",
                        "evidence ".repeat(index + 1)
                    ),
                )?;
            }
            write_static_test_model(&data.path().join("models/local"))?;
            let attempts = Arc::new(AtomicUsize::new(0));
            let observed_attempts = attempts.clone();
            let kernel = Kernel::open(root.path(), Some(data.path().into()))?.with_index_progress(
                move |event| {
                    if event.phase == "scan_started" {
                        observed_attempts.fetch_add(1, Ordering::AcqRel);
                    }
                },
            );
            let lost = Arc::new(Mutex::new(None::<(String, String)>));
            let observed_loss = lost.clone();
            let live_root = root.path().to_owned();
            let fault = capture_fault(Box::new(move |store, artifact| {
                let mut loss = observed_loss.lock().unwrap();
                if loss.is_none() {
                    std::fs::remove_file(retained_object_path(store, artifact, kind))?;
                    *loss = Some((artifact.path.clone(), artifact.content_hash.clone()));
                    if changed {
                        std::fs::write(
                            live_root.join(&artifact.path),
                            "updated repaired evidence\n",
                        )?;
                    }
                }
                Ok(())
            }));
            let report = kernel.scheduled_index(false, false)?;
            drop(fault);
            assert_eq!(attempts.load(Ordering::Acquire), 2);
            assert_eq!(report.retained_repair_retries, 1);
            assert!(report.maintenance_error.is_none(), "{report:?}");
            let (path, original_hash) = lost.lock().unwrap().clone().unwrap();
            let expected = std::fs::read_to_string(root.path().join(&path))?;
            let expected_hash = format!("{:x}", Sha256::digest(expected.as_bytes()));
            assert_eq!(expected_hash != original_hash, changed);
            kernel.resume_current_enrichment()?;
            drop(kernel);
            let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
            assert_eq!(reopened.status()?.files, 35);
            let evidence = reopened.read_file(&path, 1, 1, 4096)?;
            assert_eq!(evidence.hits[0].text, expected);
            assert_eq!(evidence.hits[0].citation.content_hash, expected_hash);
            let semantic = reopened.semantic_search(&airk_core::SearchQuery {
                text: if changed { "updated" } else { "original" }.into(),
                path_prefix: Some(path),
                limit: 1,
                max_bytes: 4096,
            })?;
            assert_eq!(semantic.hits[0].citation.content_hash, expected_hash);
            assert!(!reopened.schedule_status()?.unwrap().blocked);
        }
    }
    Ok(())
}

#[test]
fn repeated_progressive_capture_loss_stops_after_one_repair_attempt() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for index in 0..35 {
        std::fs::write(
            root.path().join(format!("file_{index:02}.txt")),
            format!(
                "original fixture {index} {}\n",
                "evidence ".repeat(index + 1)
            ),
        )?;
    }
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed_attempts = attempts.clone();
    let kernel =
        Kernel::open(root.path(), Some(data.path().into()))?.with_index_progress(move |event| {
            if event.phase == "scan_started" {
                observed_attempts.fetch_add(1, Ordering::AcqRel);
            }
        });
    kernel.configure_vectors(false)?;
    let losses = Arc::new(AtomicUsize::new(0));
    let observed_losses = losses.clone();
    let capture_attempts = attempts.clone();
    let fault = capture_fault(Box::new(move |store, artifact| {
        let attempt = capture_attempts.load(Ordering::Acquire);
        if observed_losses.load(Ordering::Acquire) < attempt {
            std::fs::remove_file(retained_object_path(store, artifact, "artifacts"))?;
            observed_losses.fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
    }));
    let error = kernel.scheduled_index(false, false).unwrap_err();
    drop(fault);
    let message = format!("{error:#}");
    assert!(message.contains("after one fresh scan"), "{message}");
    assert!(
        message.contains("publish progressive base batch: prepare current metadata"),
        "{message}"
    );
    assert!(message.contains("retained object missing"), "{message}");
    assert_eq!(attempts.load(Ordering::Acquire), 2);
    assert_eq!(losses.load(Ordering::Acquire), 2);
    assert!(kernel.schedule_status()?.unwrap().blocked);
    assert!(
        kernel.retained_repair_can_rescan()?,
        "failed preparation must leave no journal"
    );
    Ok(())
}

#[test]
fn applying_missing_capture_never_reconstructs_from_changed_live_source() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(root.path().join("alpha.txt"), "original journal evidence\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.scheduled_index(false, false)?;
    let snapshot = kernel.load_optional()?.unwrap();
    let artifact = snapshot.artifacts["alpha.txt"].clone();
    let missing = retained_object_path(&kernel.store, &artifact, "sources");
    let scope = kernel.current_scope()?.unwrap();
    let quota = Quota::open(&kernel.data_root)?;
    let writer = kernel.store.lock()?;
    let reservation = kernel.publication_reservation()?;
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        CurrentUpdater {
            store: &kernel.store,
            quota: &quota,
            scope: &scope,
            writer: &writer,
            reservation: &reservation,
            configuration: None,
        }
        .apply_observed(
            &[ArtifactChange::Upsert(Box::new(CapturedInput { artifact }))],
            None,
            &mut |stage| {
                if stage == super::super::CurrentUpdateStage::Applying {
                    std::fs::remove_file(&missing).unwrap();
                    panic!("injected stop after durable Applying");
                }
            },
        )
    }));
    assert!(interrupted.is_err());
    drop(writer);
    drop(reservation);
    std::fs::write(
        root.path().join("alpha.txt"),
        "updated live source must not enter journal\n",
    )?;
    let captures = Arc::new(AtomicUsize::new(0));
    let observed_captures = captures.clone();
    let fault = capture_fault(Box::new(move |_, _| {
        observed_captures.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }));
    let error = kernel.index_current(false, true).unwrap_err();
    drop(fault);
    assert!(
        format!("{error:#}").contains("no pending journal"),
        "{error:#}"
    );
    assert_eq!(captures.load(Ordering::Acquire), 0);
    assert!(!missing.exists());
    assert!(kernel.status().is_err());
    assert!(!kernel.retained_repair_can_rescan()?);
    let writer = kernel.store.lock()?;
    let capacity = kernel.store.current_database_capacity(&scope)?;
    let current = kernel
        .store
        .current_write(&writer, &quota, &scope, capacity)?;
    assert_eq!(
        current.phase()?,
        airk_store::current::TransactionPhase::Applying
    );
    assert!(current.pending()?.is_some());
    assert_eq!(
        std::fs::read_to_string(root.path().join("alpha.txt"))?,
        "updated live source must not enter journal\n"
    );
    Ok(())
}

fn model_data_directory() -> Result<tempfile::TempDir> {
    // macOS temporary paths can contain the /var symlink. Model admission
    // requires physical ancestors, as do the current_runtime test fixtures.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn write_static_test_model(directory: &std::path::Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    std::fs::create_dir_all(directory)?;
    let vocabulary = serde_json::to_vec(&serde_json::json!({
        "authenticate": [1.0, 0.0], "original": [1.0, 0.0], "updated": [0.0, 1.0]
    }))?;
    std::fs::write(directory.join("vocabulary.json"), &vocabulary)?;
    std::fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "name": "original-progressive-fixture",
            "license": "Apache-2.0",
            "dimensions": 2,
            "sha256": format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    Ok(())
}

#[test]
fn scheduled_scan_repairs_missing_retained_inputs_from_verified_sources() -> Result<()> {
    use airk_core::compression::CompressionMode;
    for compression in [CompressionMode::Off, CompressionMode::Automatic] {
        for kind in ["sources", "artifacts"] {
            let root = tempfile::tempdir()?;
            let data = model_data_directory()?;
            let source = "original repair fixture alpha evidence\n".repeat(1024);
            let neighbor = "unrelated original neighbor evidence\n".repeat(1024);
            std::fs::write(root.path().join("alpha.txt"), &source)?;
            std::fs::write(root.path().join("neighbor.txt"), &neighbor)?;
            write_static_test_model(&data.path().join("models/local"))?;
            let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
            kernel.configure_compression(compression)?;
            kernel.scheduled_index(false, false)?;
            kernel.resume_current_enrichment()?;
            let snapshot = kernel.load_optional()?.unwrap();
            let artifact = &snapshot.artifacts["alpha.txt"];
            let retained = artifact.retained.as_ref().unwrap();
            let (digest, encoding) = if kind == "sources" {
                (&artifact.content_hash, retained.source_encoding.as_ref())
            } else {
                (&retained.object, retained.payload_encoding.as_ref())
            };
            if compression == CompressionMode::Automatic {
                assert!(encoding.is_some());
            }
            let object = kernel
                .store
                .directory()
                .join("objects")
                .join(kind)
                .join(encoding.map_or(digest, |value| &value.stored_digest));
            let scope = kernel.current_scope()?.unwrap();
            let before = kernel.store.current_read(&scope)?;
            let identity = before.file_metadata(&kernel.store, "alpha.txt")?.unwrap();
            let neighbor_identity = before
                .file_metadata(&kernel.store, "neighbor.txt")?
                .unwrap();
            drop(before);
            std::fs::remove_file(&object)?;
            assert!(kernel.read_file("alpha.txt", 1, 1, 4096).is_err());
            assert!(
                !object.exists(),
                "read-only requests must not repair objects"
            );

            let report = kernel.scheduled_index(false, false)?;
            assert_eq!((report.indexed, report.reused), (1, 1));
            assert_eq!(report.repaired_retained_artifacts, 1);
            assert_eq!(report.retained_repair_retries, 0);
            assert!(report.maintenance_error.is_none(), "{report:?}");
            assert!(object.is_file(), "{compression:?} {kind}");
            let after = kernel.store.current_read(&scope)?;
            assert_eq!(
                serde_json::to_value(after.file_metadata(&kernel.store, "alpha.txt")?)?,
                serde_json::to_value(Some(identity))?
            );
            assert_eq!(
                serde_json::to_value(after.file_metadata(&kernel.store, "neighbor.txt")?)?,
                serde_json::to_value(Some(neighbor_identity))?
            );
            drop(after);
            assert_eq!(
                kernel.read_file("alpha.txt", 1, 1, 4096)?.hits[0].text,
                "original repair fixture alpha evidence\n"
            );
            assert_eq!(
                kernel.read_file("neighbor.txt", 1, 1, 4096)?.hits[0].text,
                "unrelated original neighbor evidence\n"
            );
            assert_eq!(
                std::fs::read_to_string(root.path().join("alpha.txt"))?,
                source
            );
            assert_eq!(
                std::fs::read_to_string(root.path().join("neighbor.txt"))?,
                neighbor
            );
            assert!(!kernel.schedule_status()?.unwrap().blocked);
            drop(kernel);
            let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
            assert_eq!(
                reopened.status()?.semantic_readiness.state,
                airk_core::readiness::SemanticState::Ready
            );
            let semantic = reopened.semantic_search(&airk_core::SearchQuery {
                text: "authenticate".into(),
                path_prefix: Some("alpha.txt".into()),
                limit: 1,
                max_bytes: 4096,
            })?;
            assert_eq!(semantic.hits[0].citation.path, "alpha.txt");
            assert_eq!(
                semantic.hits[0].citation.content_hash,
                artifact.content_hash
            );
        }
    }
    Ok(())
}

#[test]
fn scheduled_scan_does_not_replace_corrupt_retained_inputs() -> Result<()> {
    for kind in ["sources", "artifacts"] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("alpha.txt"), "original repair evidence\n")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.scheduled_index(false, false)?;
        let snapshot = kernel.load_optional()?.unwrap();
        let artifact = &snapshot.artifacts["alpha.txt"];
        let digest = if kind == "sources" {
            &artifact.content_hash
        } else {
            &artifact.retained.as_ref().unwrap().object
        };
        let object = kernel
            .store
            .directory()
            .join("objects")
            .join(kind)
            .join(digest);
        std::fs::write(&object, b"corrupt derived fixture")?;
        let error = kernel.scheduled_index(false, false).unwrap_err();
        assert!(
            format!("{error:#}").contains("mismatch"),
            "integrity failure must remain visible: {error:#}"
        );
        assert_eq!(std::fs::read(object)?, b"corrupt derived fixture");
        assert_eq!(
            kernel.load_optional()?.unwrap().artifacts["alpha.txt"].content_hash,
            artifact.content_hash
        );
        assert!(kernel.schedule_status()?.unwrap().blocked);
    }
    Ok(())
}

#[test]
fn missing_retained_source_never_relabels_changed_live_bytes_as_old_evidence() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    let data = model_data_directory()?;
    std::fs::write(root.path().join("alpha.txt"), "original alpha evidence\n")?;
    write_static_test_model(&data.path().join("models/local"))?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    kernel.resume_current_enrichment()?;
    let original = kernel.load_optional()?.unwrap();
    let old = &original.artifacts["alpha.txt"];
    let old_object = kernel
        .store
        .directory()
        .join("objects/sources")
        .join(&old.content_hash);
    std::fs::remove_file(&old_object)?;
    let updated = "updated current replacement\n";
    std::fs::write(root.path().join("alpha.txt"), updated)?;
    let report = kernel.scheduled_index(false, false)?;
    assert_eq!((report.indexed, report.reused), (1, 0));
    assert!(report.generation > original.generation);
    kernel.resume_current_enrichment()?;
    assert!(
        !old_object.exists(),
        "new bytes must not recreate the old hash"
    );
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    let exact = reopened.read_file("alpha.txt", 1, 1, 4096)?;
    let expected_hash = format!("{:x}", Sha256::digest(updated.as_bytes()));
    assert_eq!(exact.hits[0].text, updated);
    assert_eq!(exact.hits[0].citation.content_hash, expected_hash);
    assert_ne!(exact.hits[0].citation.content_hash, old.content_hash);
    let semantic = reopened.semantic_search(&airk_core::SearchQuery {
        text: "updated".into(),
        path_prefix: Some("alpha.txt".into()),
        limit: 1,
        max_bytes: 4096,
    })?;
    assert_eq!(semantic.hits[0].text, updated);
    assert_eq!(semantic.hits[0].citation.content_hash, expected_hash);
    Ok(())
}

#[test]
fn missing_prior_extraction_with_changed_or_deleted_source_stops_without_relabeling() -> Result<()>
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    for deleted in [false, true] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let source = root.path().join("alpha.txt");
        let neighbor = root.path().join("neighbor.txt");
        std::fs::write(&source, "original alpha evidence\n")?;
        std::fs::write(&neighbor, "unrelated neighbor evidence\n")?;
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = attempts.clone();
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?.with_index_progress(
            move |event| {
                if event.phase == "scan_started" {
                    observed_attempts.fetch_add(1, Ordering::AcqRel);
                }
            },
        );
        kernel.configure_vectors(false)?;
        kernel.scheduled_index(false, false)?;
        attempts.store(0, Ordering::Release);
        let before = kernel.current_reader()?;
        let original = before.artifact("alpha.txt")?.unwrap();
        let identity = before.file_metadata(&kernel.store, "alpha.txt")?.unwrap();
        let neighbor_identity = before
            .file_metadata(&kernel.store, "neighbor.txt")?
            .unwrap();
        drop(before);
        let missing = retained_object_path(&kernel.store, &original, "artifacts");
        std::fs::remove_file(&missing)?;
        if deleted {
            std::fs::remove_file(&source)?;
        } else {
            std::fs::write(&source, "updated live alpha evidence\n")?;
        }

        // Replacing or deleting this row still needs its old extraction to
        // authenticate prior posting selectors. New live bytes cannot supply it.
        let error = kernel.scheduled_index(false, false).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("after one fresh scan"), "{message}");
        assert!(message.contains("missing retained extraction"), "{message}");
        assert!(message.contains("alpha.txt"), "{message}");
        assert!(
            message.contains(&missing.display().to_string()),
            "{message}"
        );
        assert_eq!(attempts.load(Ordering::Acquire), 2);
        assert!(kernel.schedule_status()?.unwrap().blocked);
        assert!(kernel.retained_repair_can_rescan()?);
        assert!(
            !missing.exists(),
            "later bytes must not recreate old evidence"
        );
        drop(kernel);

        let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
        let after = reopened.current_reader()?;
        assert_eq!(
            serde_json::to_value(after.artifact("alpha.txt")?)?,
            serde_json::to_value(Some(original))?
        );
        assert_eq!(
            serde_json::to_value(after.file_metadata(&reopened.store, "alpha.txt")?)?,
            serde_json::to_value(Some(identity))?
        );
        assert_eq!(
            serde_json::to_value(after.file_metadata(&reopened.store, "neighbor.txt")?)?,
            serde_json::to_value(Some(neighbor_identity))?
        );
        drop(after);
        assert!(reopened.read_file("alpha.txt", 1, 1, 4096).is_err());
        assert_eq!(
            reopened.read_file("neighbor.txt", 1, 1, 4096)?.hits[0].text,
            "unrelated neighbor evidence\n"
        );
        assert!(reopened.schedule_status()?.unwrap().blocked);
        if deleted {
            assert!(
                !source.exists(),
                "indexing must not restore a deleted source"
            );
        } else {
            assert_eq!(
                std::fs::read_to_string(source)?,
                "updated live alpha evidence\n"
            );
        }
        assert_eq!(
            std::fs::read_to_string(neighbor)?,
            "unrelated neighbor evidence\n"
        );
    }
    Ok(())
}

#[test]
fn slow_semantic_publication_does_not_consume_the_next_source_batch_interval() -> Result<()> {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let root = tempfile::tempdir()?;
    let data = model_data_directory()?;
    write_static_test_model(&data.path().join("models/local"))?;
    for index in 0..35 {
        std::fs::write(
            root.path().join(format!("file_{index:02}.txt")),
            "authenticate\n",
        )?;
    }
    let delayed = Arc::new(AtomicBool::new(false));
    let base_commits = Arc::new(Mutex::new(Vec::new()));
    let observed_delay = delayed.clone();
    let observed_commits = base_commits.clone();
    let kernel =
        Kernel::open(root.path(), Some(data.path().into()))?.with_index_progress(move |event| {
            if event.phase == "base_committed" {
                observed_commits
                    .lock()
                    .unwrap()
                    .push(event.committed_artifacts);
            }
            if event.phase == "enrichment_committed" && !observed_delay.swap(true, Ordering::AcqRel)
            {
                // Slow neural inference/publication occupies the same synchronous
                // flush interval. It must not age the following source batch.
                std::thread::sleep(std::time::Duration::from_millis(2100));
            }
        });
    kernel.scheduled_index(false, false)?;
    assert!(delayed.load(Ordering::Acquire));
    assert_eq!(
        *base_commits.lock().unwrap(),
        vec![32],
        "remaining files should share the final source batch after slow enrichment"
    );
    let status = kernel.status()?;
    assert_eq!(status.files, 35);
    assert_eq!(status.discovery_readiness.committed_artifacts, 35);
    assert_eq!(status.semantic_readiness.completed_artifacts, 8);
    assert_eq!(status.semantic_readiness.selected_artifacts, 35);
    for file in [0, 31, 32, 34] {
        let evidence = kernel.read_file(&format!("file_{file:02}.txt"), 1, 1, 4096)?;
        assert_eq!(evidence.hits[0].text, "authenticate\n");
    }
    Ok(())
}

#[test]
fn many_progressive_publications_do_not_spend_the_capture_reservation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for index in 0..40 {
        std::fs::write(
            root.path().join(format!("file_{index:02}.txt")),
            format!("authenticate original fixture {index}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let previous = kernel.load_optional()?.unwrap();
    let scope = kernel.current_scope()?.unwrap();
    let read = kernel.store.current_read(&scope)?;
    let configuration = read.collection_state()?.unwrap().configuration;
    drop(read);
    let quota = Quota::open(&kernel.data_root)?;
    let writer = kernel.store.lock()?;
    // Stand in for the scan's still-live capture allowance. Temporary graph
    // capacities must not accumulate here as small publications complete.
    let capture = quota.reserve(kernel.collection(), 1 << 20)?;
    let selection = kernel.vector_selection()?;
    let execution = kernel.execution_status()?;
    let mut stats = EmbeddingCacheStats::default();
    let mut progressive = ProgressiveBase {
        kernel: &kernel,
        quota: &quota,
        writer: &writer,
        scope: &scope,
        configuration: &configuration,
        model: None,
        objects: None,
        selection: &selection,
        stats: &mut stats,
        execution: &execution,
        previous: &previous,
        snapshot: previous.clone(),
        semantic_work: None,
        semantic_failed: false,
        scan_work: progress::ScanWork::start()?,
        batch: Default::default(),
        batch_bytes: 0,
        last_flush: std::time::Instant::now(),
        last_pause_check: std::time::Instant::now(),
        next_batch: 1,
        processed: 0,
        committed: 0,
        current: Default::default(),
        first_ready_us: None,
        first_vector_us: None,
        started: std::time::Instant::now(),
    };
    for artifact in previous.artifacts.values() {
        progressive.processed += 1;
        progressive.batch.insert(
            artifact.path.clone(),
            ArtifactChange::Upsert(Box::new(CapturedInput {
                artifact: artifact.clone(),
            })),
        );
        progressive.flush()?;
    }
    assert_eq!(progressive.committed, 40);
    assert_eq!(capture.charged(), 0);
    assert_eq!(kernel.status()?.files, 40);
    assert!(
        kernel.read_file("file_39.txt", 1, 1, 4096)?.hits[0]
            .text
            .contains("fixture 39")
    );
    Ok(())
}

#[test]
fn progressive_flush_preserves_the_next_captured_input() -> Result<()> {
    // The extraction callback retains the next file before observe can flush
    // the preceding batch. Exercise both pre-insertion flush boundaries.
    for boundary in ["time", "bytes"] {
        let donor_root = tempfile::tempdir()?;
        let donor_data = tempfile::tempdir()?;
        for name in ["first", "second"] {
            std::fs::write(
                donor_root.path().join(format!("{name}.txt")),
                format!("original {name} capture\n"),
            )?;
        }
        let donor = Kernel::open(donor_root.path(), Some(donor_data.path().into()))?;
        donor.configure_vectors(false)?;
        donor.index()?;
        let captured = donor.load_optional()?.unwrap();
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("first.txt"), "original committed seed\n")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let previous = kernel.load_optional()?.unwrap();
        let scope = kernel.current_scope()?.unwrap();
        let read = kernel.store.current_read(&scope)?;
        let configuration = read.collection_state()?.unwrap().configuration;
        drop(read);
        let quota = Quota::open(&kernel.data_root)?;
        let writer = kernel.store.lock()?;
        let captured_objects = writer.capture_objects();
        let reservation = quota.reserve(kernel.collection(), 16 << 20)?;
        let selection = kernel.vector_selection()?;
        let execution = kernel.execution_status()?;
        let mut stats = EmbeddingCacheStats::default();
        let mut progressive = ProgressiveBase {
            kernel: &kernel,
            quota: &quota,
            writer: &writer,
            scope: &scope,
            configuration: &configuration,
            model: None,
            objects: None,
            selection: &selection,
            stats: &mut stats,
            execution: &execution,
            previous: &previous,
            snapshot: previous.clone(),
            semantic_work: None,
            semantic_failed: false,
            scan_work: progress::ScanWork::start()?,
            batch: Default::default(),
            batch_bytes: 0,
            last_flush: std::time::Instant::now(),
            last_pause_check: std::time::Instant::now(),
            next_batch: 32,
            processed: 0,
            committed: 0,
            current: Default::default(),
            first_ready_us: None,
            first_vector_us: None,
            started: std::time::Instant::now(),
        };

        for (index, artifact) in captured.artifacts.values().enumerate() {
            let profile = artifact
                .retained
                .as_ref()
                .unwrap()
                .profile
                .as_ref()
                .unwrap();
            let retained = kernel.store.retain_payload_with_budget(
                &donor.store.load_retained(artifact)?,
                &donor.store.load_extraction_payload(artifact)?,
                profile,
                Some(&donor.store.read_retained_source(artifact)?),
                Some(&reservation),
            )?;
            if index == 1 {
                if boundary == "time" {
                    progressive.last_flush =
                        std::time::Instant::now() - std::time::Duration::from_secs(3);
                } else {
                    progressive.batch_bytes = 64 << 20;
                }
            }
            progressive.observe(&retained)?;
        }
        progressive.flush()?;
        assert_eq!(progressive.committed, 2, "{boundary}");
        drop(progressive);
        // Prior-content reuse can still need a superseded source during this scan.
        assert_eq!(
            kernel
                .store
                .read_retained_source(&previous.artifacts["first.txt"])?,
            b"original committed seed\n"
        );
        let read = kernel.store.current_read(&scope)?;
        let plan = read.reclamation_plan(&kernel.store)?;
        drop(read);
        let capacity = kernel.store.current_database_capacity(&scope)?;
        let mut current = kernel
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        let nested_capture = writer.capture_objects();
        drop(captured_objects);
        let deferred = current.reclaim_objects(&plan)?;
        assert!(deferred.pending);
        assert_eq!(deferred.removed_files, 0);
        drop(nested_capture);
        let reclaimed = current.reclaim_objects(&plan)?;
        assert!(reclaimed.removed_files > 0);
        assert!(!reclaimed.pending);
        assert!(
            kernel
                .store
                .read_retained_source(&previous.artifacts["first.txt"])
                .is_err()
        );
        drop(current);
        drop(writer);
        for name in ["first", "second"] {
            let result = kernel.read_file(&format!("{name}.txt"), 1, 1, 4096)?;
            assert!(
                result.hits[0]
                    .text
                    .contains(&format!("original {name} capture"))
            );
        }
    }
    Ok(())
}
