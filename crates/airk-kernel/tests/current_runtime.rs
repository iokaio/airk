// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    SearchQuery,
    readiness::{DefaultSearchPolicy, DiscoveryState, SemanticState},
};
use airk_kernel::Kernel;
use anyhow::Result;

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // Model guards require real ancestors; macOS exposes temporary paths through /var.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

#[test]
#[ignore = "large repository capacity qualification; run explicitly in release mode"]
fn twelve_thousand_files_publish_reopen_update_and_reclaim() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    for file in 0..12_000 {
        std::fs::write(
            root.path().join(format!("fixture_{file:05}.rs")),
            format!("pub fn fixture_{file:05}() -> usize {{ {file} }}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    let cold = kernel.index()?;
    assert!(cold.maintenance_error.is_none(), "{cold:?}");
    assert_eq!(kernel.status()?.files, 12_000);
    let current = cold.current.unwrap();
    assert_eq!(current.changed_artifacts, 12_000);
    println!("large repository cold: {current:?}");
    drop(kernel);
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.status()?.files, 12_000);
    assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
    std::fs::write(
        root.path().join("fixture_00000.rs"),
        "pub fn changed_fixture() {}\n",
    )?;
    let changed = kernel.index()?;
    assert!(changed.maintenance_error.is_none());
    assert_eq!(changed.current.unwrap().changed_artifacts, 1);
    let result = kernel.search(&SearchQuery {
        text: "changed_fixture".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    })?;
    assert_eq!(result.hits[0].citation.path, "fixture_00000.rs");
    std::fs::remove_file(root.path().join("fixture_11999.rs"))?;
    let removed = kernel.index()?;
    assert!(removed.maintenance_error.is_none());
    assert_eq!(kernel.status()?.files, 11_999);
    Ok(())
}

#[test]
fn shared_embedding_survives_more_than_128_file_occurrences() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    for file in 0..256 {
        std::fs::write(
            root.path().join(format!("alias_{file:03}.txt")),
            "authenticate\n",
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    assert_eq!(kernel.status()?.files, 256);
    assert_eq!(kernel.status()?.indexed_vectors, 1);
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: Some("alias_255.txt".into()),
        limit: 1,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "alias_255.txt"
    );
    std::fs::remove_file(root.path().join("alias_000.txt"))?;
    kernel.index()?;
    assert_eq!(kernel.status()?.indexed_vectors, 1);
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "alias_255.txt"
    );
    Ok(())
}

#[test]
fn public_runtime_indexes_updates_and_reads_current_evidence() -> Result<()> {
    let directory = unlinked_tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("a.txt"), "alpha original\n")?;
    std::fs::write(root.join("b.txt"), "beta retained\n")?;
    let kernel = Kernel::open(&root, Some(directory.path().join("state")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let before = kernel.status()?;
    assert_eq!(before.files, 2);
    assert!(
        directory
            .path()
            .join("state/collections-1.2/default/current.sqlite3")
            .is_file()
    );
    assert!(
        !directory
            .path()
            .join("state/collections-1.2/default/index.sqlite3")
            .exists()
    );
    let query = SearchQuery {
        text: "alpha".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits[0].text, "alpha original\n");
    kernel.index()?;
    assert_eq!(kernel.status()?.publication, before.publication);
    std::fs::write(root.join("a.txt"), "gamma changed\n")?;
    kernel.index()?;
    assert!(kernel.search(&query)?.hits.is_empty());
    assert_eq!(
        kernel.read_file("b.txt", 1, 1, 4096)?.hits[0].text,
        "beta retained\n"
    );
    std::fs::remove_file(root.join("a.txt"))?;
    kernel.index()?;
    assert_eq!(kernel.list_files("", 10, 4096)?.paths, ["b.txt"]);
    assert_eq!(kernel.status()?.files, 1);
    Ok(())
}

fn model(directory: &std::path::Path, dimensions: usize) -> Result<()> {
    use sha2::{Digest, Sha256};
    std::fs::create_dir_all(directory)?;
    let mut vector = vec![0.; dimensions];
    vector[0] = 1.;
    let vocabulary =
        serde_json::to_vec(&serde_json::json!({"authenticate":vector,"login":vector}))?;
    std::fs::write(directory.join("vocabulary.json"), &vocabulary)?;
    std::fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-current-runtime-fixture","license":"Apache-2.0",
            "dimensions":dimensions,"sha256":format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    Ok(())
}

#[test]
fn scheduled_base_is_readable_before_restart_resumes_semantic_work() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let base = kernel.scheduled_index(false, false)?;
    assert!(base.first_ready_us.is_some());
    assert!(base.fully_ready_us.is_none());
    let pending = kernel.status()?;
    assert_eq!(pending.semantic_readiness.state, SemanticState::Pending);
    assert_eq!(pending.semantic_readiness.selected_artifacts, 1);
    assert_eq!(pending.semantic_readiness.completed_artifacts, 0);
    assert_eq!(pending.indexed_vectors, 0);
    let query = SearchQuery {
        text: "authenticate".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits.len(), 1);
    assert_eq!(
        kernel
            .default_search(&query)?
            .semantic_readiness
            .unwrap()
            .state,
        SemanticState::Pending
    );
    kernel.set_default_semantic_search_policy(DefaultSearchPolicy::LexicalWhilePending)?;
    assert_eq!(kernel.default_search(&query)?.hits.len(), 1);
    kernel.set_default_semantic_search_policy(DefaultSearchPolicy::RequireComplete)?;
    assert!(kernel.default_search(&query).is_err());
    kernel.schedule_pause(true)?;
    assert_eq!(kernel.run_scheduled_once()?.state, "idle");
    kernel.schedule_pause(false)?;
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.run_scheduled_once()?.state, "enriched");
    let ready = reopened.status()?;
    assert_eq!(ready.semantic_readiness.state, SemanticState::Ready);
    assert_eq!(ready.semantic_readiness.completed_artifacts, 1);
    assert_eq!(ready.indexed_vectors, 1);
    assert_ne!(pending.publication, ready.publication);
    assert_eq!(reopened.default_search(&query)?.hits.len(), 1);
    Ok(())
}

#[test]
fn scheduled_cold_scan_commits_bounded_batches_and_closes_discovery() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    for file in 0..40 {
        std::fs::write(
            root.path().join(format!("fixture_{file:02}.txt")),
            format!("distinct fixture {file:02}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    let report = kernel.scheduled_index(false, false)?;
    assert_eq!(report.current.unwrap().changed_artifacts, 40);
    assert!(report.generation >= 3);
    assert!(report.first_ready_us.is_some());
    let status = kernel.status()?;
    assert_eq!(status.files, 40);
    assert_eq!(status.discovery_readiness.state, DiscoveryState::Ready);
    assert_eq!(status.discovery_readiness.total_artifacts, Some(40));
    assert_eq!(status.discovery_readiness.committed_artifacts, 40);
    let query = SearchQuery {
        text: "distinct fixture 39".into(),
        path_prefix: Some("fixture_39.txt".into()),
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.search(&query)?.hits[0].citation.path,
        "fixture_39.txt"
    );
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        reopened.status()?.discovery_readiness.state,
        DiscoveryState::Ready
    );
    assert_eq!(reopened.index()?.current.unwrap().changed_artifacts, 0);
    Ok(())
}

#[test]
fn direct_index_reports_one_lexical_write_for_one_semantic_file() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let report = kernel.index()?;
    assert_eq!(report.current.expect("current work").lexical_documents, 1);
    assert!(report.first_ready_us.is_some());
    assert!(report.fully_ready_us.is_some());
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    Ok(())
}

#[test]
fn superseding_base_discards_old_pending_vector_binding() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    let old = kernel.status()?.publication;
    std::fs::write(root.path().join("a.txt"), "login\n")?;
    kernel.scheduled_index(false, false)?;
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Pending
    );
    assert_ne!(old, kernel.status()?.publication);
    let query = SearchQuery {
        text: "authenticate".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert!(kernel.search(&query)?.hits.is_empty());
    assert_eq!(kernel.run_scheduled_once()?.state, "enriched");
    let semantic = kernel.semantic_search(&query)?;
    assert_eq!(
        semantic.semantic_readiness.unwrap().state,
        SemanticState::Ready
    );
    assert_eq!(semantic.hits[0].text, "login\n");
    Ok(())
}

#[test]
fn semantic_batches_disclose_partial_coverage() -> Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    for file in 0..65 {
        std::fs::write(
            root.path().join(format!("file_{file:02}.txt")),
            "authenticate\n",
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let exact_early = Arc::new(AtomicBool::new(false));
    let semantic_early = Arc::new(AtomicBool::new(false));
    let exact_signal = exact_early.clone();
    let semantic_signal = semantic_early.clone();
    let reader = kernel.clone();
    let observed = kernel.clone().with_index_progress(move |event| {
        if !matches!(event.phase, "base_committed" | "enrichment_committed") {
            return;
        }
        let query = SearchQuery {
            text: if event.phase == "base_committed" {
                "authenticate"
            } else {
                "login"
            }
            .into(),
            path_prefix: None,
            limit: 1,
            max_bytes: 4096,
        };
        let incomplete = reader
            .status()
            .is_ok_and(|status| status.discovery_readiness.total_artifacts.is_none());
        if event.phase == "base_committed" {
            if incomplete
                && reader
                    .search(&query)
                    .is_ok_and(|result| !result.hits.is_empty())
            {
                exact_signal.store(true, Ordering::Release);
            }
        } else if incomplete
            && reader
                .semantic_search(&query)
                .is_ok_and(|result| !result.hits.is_empty())
        {
            semantic_signal.store(true, Ordering::Release);
        }
    });
    let report = observed.scheduled_index(false, false)?;
    assert_eq!(report.embedding_inferences, 0);
    assert!(report.first_vector_us.is_none());
    assert!(
        exact_early.load(Ordering::Acquire),
        "exact evidence must answer before discovery completes"
    );
    assert!(
        !semantic_early.load(Ordering::Acquire),
        "source discovery must not wait for inline semantic enrichment"
    );
    let base = kernel.status()?.semantic_readiness;
    assert_eq!(base.state, SemanticState::Pending);
    assert_eq!(base.completed_artifacts, 0);
    let pending_path = SearchQuery {
        text: "login".into(),
        path_prefix: Some("file_64.txt".into()),
        limit: 10,
        max_bytes: 4096,
    };
    let pending_search = kernel.semantic_search(&pending_path)?;
    assert_eq!(
        pending_search.semantic_readiness.unwrap().state,
        SemanticState::Pending
    );
    assert!(pending_search.hits.is_empty());
    let mut completed = base.completed_artifacts;
    while completed < 65 {
        assert_eq!(kernel.run_scheduled_once()?.state, "enriched");
        completed = (completed + 32).min(65);
        let next = kernel.status()?.semantic_readiness;
        assert_eq!(
            (next.selected_artifacts, next.completed_artifacts),
            (65, completed)
        );
        assert_eq!(
            next.state,
            if completed == 65 {
                SemanticState::Ready
            } else {
                SemanticState::Partial
            }
        );
    }
    let ready = kernel.status()?.semantic_readiness;
    assert_eq!(ready.state, SemanticState::Ready);
    assert_eq!(
        (ready.selected_artifacts, ready.completed_artifacts),
        (65, 65)
    );
    let full = kernel.reindex()?;
    assert_eq!(full.vector_eligible_artifacts, 65);
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    Ok(())
}

#[test]
fn editor_semantic_batches_resume_after_reopen_and_preserve_scope_and_pause() -> Result<()> {
    use std::sync::{Arc, Mutex};
    let root = unlinked_tempdir()?;
    let foreign = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    for file in 0..65 {
        std::fs::write(
            root.path().join(format!("file_{file:02}.txt")),
            "authenticate\n",
        )?;
    }
    std::fs::write(foreign.path().join("foreign.txt"), "authenticate\n")?;
    let other = Kernel::open_collection(foreign.path(), Some(data.path().into()), "foreign")?;
    other.scheduled_index(false, false)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    let mut completed = kernel.status()?.semantic_readiness.completed_artifacts;
    assert!(completed < 65);
    kernel.schedule_pause(true)?;
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "paused");
    assert_eq!(
        kernel.status()?.semantic_readiness.completed_artifacts,
        completed
    );
    kernel.schedule_pause(false)?;
    let queue = airk_store::scheduler::Queue::new(data.path());
    let worker = queue.worker_lock("default")?.expect("fixture worker lock");
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "busy");
    drop(worker);
    drop(kernel);

    // Foreign source work stays queued while this collection drains its own
    // semantic batches. No cross-project source handoff is required.
    let foreign_head = other.status()?.publication;
    std::fs::write(
        foreign.path().join("foreign.txt"),
        "changed outside workspace\n",
    )?;
    let pending = other.schedule_enqueue(false)?;
    assert!(pending.requested_revision > pending.completed_revision);
    assert!(queue.has_pending_source()?);

    let progress = Arc::new(Mutex::new(Vec::new()));
    while completed < 65 {
        let events = progress.clone();
        let reopened = Kernel::open(root.path(), Some(data.path().into()))?.with_index_progress(
            move |event| {
                events
                    .lock()
                    .expect("progress observer lock")
                    .push((event.phase.to_string(), event.completed_artifacts));
            },
        );
        let outcome = reopened
            .run_scheduled_enrichment_bounded(8, Some(std::time::Duration::from_secs(60)))?;
        assert_eq!(outcome.state, "enriched");
        assert!(
            !outcome.report.unwrap().yield_to_source,
            "foreign requests do not interrupt this collection"
        );
        completed = 65;
        assert_eq!(
            reopened.status()?.semantic_readiness.completed_artifacts,
            completed
        );
        assert_eq!(other.status()?.publication, foreign_head);
    }
    let events = progress.lock().expect("progress observer lock");
    assert_eq!(events.len(), 9);
    for phases in events.as_chunks::<3>().0 {
        assert_eq!(phases[0].0, "enrichment_started");
        assert_eq!(phases[1].0, "enrichment_prepared");
        assert_eq!(phases[2].0, "enrichment_committed");
        assert_eq!(phases[0].1, phases[1].1);
        assert!(phases[2].1 > phases[1].1);
    }
    drop(events);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        reopened.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    assert_eq!(reopened.run_scheduled_enrichment()?.state, "idle");
    assert_eq!(other.status()?.publication, foreign_head);
    Ok(())
}

#[test]
fn semantic_capacity_wait_does_not_record_a_failure() -> Result<()> {
    use std::fs::OpenOptions;
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    let before = kernel.status()?.publication;
    let mut slots = Vec::new();
    for slot in 0..4 {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data.path().join(format!("execution-1.2-{slot}.lock")))?;
        file.try_lock()?;
        slots.push(file);
    }
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "busy");
    let status = kernel.status()?;
    assert_eq!(status.publication, before);
    assert!(status.semantic_readiness.failure.is_none());
    assert!(!kernel.schedule_status()?.unwrap().blocked);
    drop(slots);
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "enriched");
    Ok(())
}

#[test]
fn corrupt_model_blocks_enrichment_without_losing_base_evidence() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    let model_objects = data.path().join("collections-1.2/default/objects/models");
    let object = std::fs::read_dir(model_objects)?
        .next()
        .expect("model object")?
        .path();
    std::fs::write(object, b"corrupt fixture")?;
    assert_eq!(kernel.run_scheduled_enrichment()?.state, "semantic_failed");
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Pending
    );
    assert!(
        kernel
            .schedule_status()?
            .expect("scheduled collection")
            .blocked
    );
    assert_eq!(kernel.run_scheduled_once()?.state, "idle");
    let blocked = kernel.run_scheduled_enrichment()?;
    assert_eq!(blocked.state, "blocked");
    assert!(blocked.error.is_some());
    let query = SearchQuery {
        text: "authenticate".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits.len(), 1);
    Ok(())
}

#[test]
fn semantic_failure_child() -> Result<()> {
    let Ok(root) = std::env::var("AIRK_SEMANTIC_FAILURE_TEST_ROOT") else {
        return Ok(());
    };
    let data = std::env::var("AIRK_SEMANTIC_FAILURE_TEST_DATA")?;
    let kernel = Kernel::open(root, Some(data.into()))?;
    assert!(kernel.resume_current_enrichment().is_err());
    Ok(())
}

#[test]
fn failed_semantic_batch_is_persisted_and_does_not_hide_exact_evidence() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    std::fs::write(root.path().join("a.txt"), "authenticate\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.scheduled_index(false, false)?;
    let result = std::process::Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("semantic_failure_child")
        .env("AIRK_SEMANTIC_FAILURE_TEST_ROOT", root.path())
        .env("AIRK_SEMANTIC_FAILURE_TEST_DATA", data.path())
        .env("AIRK_INDEX_WORKERS", "invalid")
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let status = kernel.status()?;
    assert_eq!(status.semantic_readiness.state, SemanticState::Failed);
    assert_eq!(status.semantic_readiness.completed_artifacts, 0);
    let query = SearchQuery {
        text: "authenticate".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits.len(), 1);
    Ok(())
}

#[test]
fn model_replacement_full_rebuild_and_empty_policy_commit_are_current() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let initial = kernel.status()?.publication.unwrap().token;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    assert_ne!(kernel.status()?.publication.unwrap().token, initial);
    assert!(!kernel.extraction_policy()?.pending_reindex);
    std::fs::write(root.path().join("auth.txt"), "authenticate\n")?;
    model(&data.path().join("models/local"), 2)?;
    kernel.configure_vectors(true)?;
    kernel.index()?;
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    let before = kernel.status()?.publication.unwrap().token;
    model(&data.path().join("models/local"), 4)?;
    kernel.index()?;
    assert_ne!(kernel.status()?.publication.unwrap().token, before);
    assert_eq!(kernel.semantic_search(&query)?.hits.len(), 1);
    kernel.reindex()?;
    assert_eq!(kernel.semantic_search(&query)?.hits.len(), 1);
    kernel.configure_vectors(false)?;
    kernel.index()?;
    assert_eq!(kernel.status()?.indexed_vectors, 0);
    assert_eq!(kernel.collections()?[0].indexed_files, Some(1));
    kernel.remove_collection("default")?;
    assert!(root.path().join("auth.txt").is_file());
    Ok(())
}

#[test]
fn old_namespace_is_untouched_and_exact_tokens_and_cursors_expire() -> Result<()> {
    use airk_core::{discovery::DiscoveryOptions, evidence::EvidenceReadOptions};
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    let old = data.path().join("collections/default");
    std::fs::create_dir_all(&old)?;
    std::fs::write(old.join("index.sqlite3"), "old index sentinel")?;
    std::fs::write(old.join("vector-policy.json"), "old policy sentinel")?;
    std::fs::write(root.path().join("a.txt"), "alpha\n")?;
    std::fs::write(root.path().join("b.txt"), "beta\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(kernel.vectors_enabled()?);
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let first = kernel.read_file_conditional("a.txt", 1, 1, 4096, &Default::default())?;
    assert!(
        kernel
            .read_file_conditional(
                "a.txt",
                1,
                1,
                4096,
                &EvidenceReadOptions {
                    known_hash: Some("00".repeat(32).parse()?),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("known_hash does not match")
    );
    let revision = first.read.publication.unwrap().token;
    let page = kernel.list_files_page("", 1, 4096, &Default::default())?;
    let cursor = page.page.cursor;
    std::fs::write(root.path().join("b.txt"), "gamma\n")?;
    kernel.index()?;
    assert!(
        kernel
            .read_file_conditional(
                "a.txt",
                1,
                1,
                4096,
                &EvidenceReadOptions {
                    revision: Some(revision),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        kernel
            .list_files_page(
                "",
                1,
                4096,
                &DiscoveryOptions {
                    cursor,
                    ..Default::default()
                }
            )
            .is_err()
    );
    let unchanged = kernel.read_file_conditional(
        "a.txt",
        1,
        1,
        4096,
        &EvidenceReadOptions {
            if_evidence: first.read.validator,
            ..Default::default()
        },
    )?;
    assert!(unchanged.content.is_none());
    assert_eq!(
        std::fs::read_to_string(old.join("index.sqlite3"))?,
        "old index sentinel"
    );
    assert_eq!(
        std::fs::read_to_string(old.join("vector-policy.json"))?,
        "old policy sentinel"
    );
    assert!(
        kernel
            .history_availability(&airk_kernel::HistoryAvailabilityRequest {
                revision: None,
                cursor: None,
                limit: 1,
                max_bytes: 4096
            })
            .is_err()
    );
    assert!(kernel.checkpoints().is_err());
    Ok(())
}

#[test]
fn one_file_publication_work_does_not_expand_with_unchanged_corpus() -> Result<()> {
    for count in [8, 64] {
        let root = unlinked_tempdir()?;
        let data = unlinked_tempdir()?;
        for file in 0..count {
            std::fs::write(
                root.path().join(format!("original-{file:03}.txt")),
                format!("original stable fixture {file}\n"),
            )?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        let cold = kernel.index()?;
        assert_eq!(cold.current.unwrap().changed_artifacts, count);
        assert!(cold.maintenance_error.is_none());
        assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
        std::fs::write(
            root.path().join("original-000.txt"),
            "replacement exact fixture\n",
        )?;
        let edit = kernel.index()?;
        assert_eq!(edit.indexed, 1);
        assert_eq!(edit.reused, count - 1);
        let work = edit.current.unwrap();
        assert_eq!(
            (
                work.changed_artifacts,
                work.lexical_documents,
                work.graph_records
            ),
            (1, 1, 0)
        );
        assert!(edit.maintenance_error.is_none());
        let directory = data.path().join("collections-1.2/default");
        assert_eq!(
            std::fs::read_dir(directory.join("objects/sources"))?.count(),
            count
        );
        assert_eq!(
            std::fs::read_dir(directory.join("current-metadata"))?.count(),
            count
        );
        assert_eq!(kernel.maintain_current()?.removed_files, 0);
    }
    Ok(())
}

#[test]
fn interrupted_runtime_bootstrap_completes_before_any_source_scan() -> Result<()> {
    #[derive(serde::Serialize)]
    struct Recipe {
        head: airk_store::current::CurrentHead,
        manifest: airk_store::current::lexical::LexicalManifest,
        configuration: airk_store::current::collection::CollectionConfiguration,
    }
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    std::fs::write(
        root.path().join("unscanned.txt"),
        "source must remain unscanned",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
    let owner: serde_json::Value = serde_json::from_slice(&std::fs::read(
        store.directory().join("current-owner.json"),
    )?)?;
    let scope = serde_json::from_value(owner["scope"].clone())?;
    let read = store.current_read(&scope)?;
    let mut head = read.head().clone();
    let manifest = airk_store::current::lexical::LexicalManifest::decode(
        &read.lexical_manifest()?,
        head.lexical_commit,
    )?;
    let configuration = read.collection_state()?.unwrap().configuration;
    head.sequence = 0;
    head.artifact_root = airk_store::current::map::empty_root(&scope)?;
    drop(read);
    let writer = store.lock()?;
    store.write_runtime_bootstrap(
        &writer,
        &serde_json::to_vec(&Recipe {
            head,
            manifest,
            configuration,
        })?,
    )?;
    // Recreate the durable boundary after the recipe, before SQLite ownership.
    for name in [
        "current.sqlite3",
        "current-owner.json",
        "current-bootstrap.json",
    ] {
        std::fs::remove_file(store.directory().join(name))?;
    }
    drop(writer);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.status()?.files, 0);
    assert!(store.runtime_bootstrap()?.is_none());
    let token = reopened.status()?.publication.unwrap().token;
    assert_eq!(
        Kernel::open(root.path(), Some(data.path().into()))?
            .status()?
            .publication
            .unwrap()
            .token,
        token
    );
    reopened.index()?;
    assert_eq!(reopened.status()?.files, 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("unscanned.txt"))?,
        "source must remain unscanned"
    );
    Ok(())
}

#[test]
fn delayed_progressive_publication_keeps_captures_through_enrichment_and_reopen() -> Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    for file in 0..65 {
        std::fs::write(
            root.path().join(format!("file_{file:02}.txt")),
            format!("authenticate original capture {file}\n").repeat(100),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_compression(airk_core::compression::CompressionMode::Automatic)?;
    let delayed = Arc::new(AtomicBool::new(false));
    let signal = delayed.clone();
    let observed = kernel.clone().with_index_progress(move |event| {
        if event.phase == "base_committed" && !signal.swap(true, Ordering::AcqRel) {
            // Retained inputs survive a delayed publication, compression,
            // enrichment and reopen. Unit tests force the time/size boundaries.
            std::thread::sleep(std::time::Duration::from_millis(2100));
        }
    });
    let report = observed.scheduled_index(false, false)?;
    assert!(delayed.load(Ordering::Acquire));
    assert!(report.maintenance_error.is_none(), "{report:?}");
    assert_eq!(
        kernel.status()?.discovery_readiness.state,
        DiscoveryState::Ready
    );
    drop(observed);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    for _ in 0..4 {
        if reopened.status()?.semantic_readiness.state == SemanticState::Ready {
            break;
        }
        assert_eq!(reopened.run_scheduled_enrichment()?.state, "enriched");
    }
    let status = reopened.status()?;
    assert_eq!(status.files, 65);
    assert_eq!(status.semantic_readiness.state, SemanticState::Ready);
    assert_eq!(status.semantic_readiness.completed_artifacts, 65);
    for file in 0..65 {
        let read = reopened.read_file(&format!("file_{file:02}.txt"), 1, 1, 4096)?;
        assert!(
            read.hits[0]
                .text
                .contains(&format!("original capture {file}\n"))
        );
    }
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: Some("file_64.txt".into()),
        limit: 1,
        max_bytes: 4096,
    };
    assert_eq!(
        reopened.semantic_search(&query)?.hits[0].citation.path,
        "file_64.txt"
    );
    assert_eq!(reopened.scheduled_index(false, false)?.reused, 65);
    assert!(!reopened.maintain_current()?.pending);
    Ok(())
}
