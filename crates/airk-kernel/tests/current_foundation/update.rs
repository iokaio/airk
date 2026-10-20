// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_kernel::current::{CurrentUpdater, SemanticUpdate};
use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel};
use airk_store::current::CurrentToken;
use airk_store::current::postings::Lookup;

fn configuration() -> airk_store::current::collection::CollectionConfiguration {
    airk_store::current::collection::CollectionConfiguration {
        roots: Default::default(),
        extractor_version: "current-update-test/1".into(),
        extraction_policy_digest: Sha256Digest::from_bytes([42; 32]),
        index_detail: airk_core::extraction::IndexDetail::Balanced,
        vector_selection: Default::default(),
        model: None,
    }
}

fn upsert(fixture: &Fixture, path: &str, text: &str) -> Result<ArtifactChange> {
    Ok(ArtifactChange::Upsert(Box::new(
        retained_lexical_artifact(fixture, path, text)?.1,
    )))
}

fn updater<'a>(
    fixture: &'a Fixture,
    lock: &'a airk_store::WriterLock,
    reservation: &'a Reservation,
) -> CurrentUpdater<'a> {
    CurrentUpdater {
        store: &fixture.store,
        quota: &fixture.quota,
        scope: &fixture.scope,
        writer: lock,
        reservation,
        configuration: None,
    }
}

fn search(fixture: &Fixture, text: &str) -> Result<airk_kernel::current::CurrentLexicalEvidence> {
    let read = fixture.store.current_read(&fixture.scope)?;
    let query = SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    let lexical =
        airk_kernel::current::open_selected_lexical(&fixture.store, &read, Arc::new(|_| Ok(())))?;
    airk_kernel::current::materialize_lexical(
        &fixture.store,
        &read,
        &query,
        lexical.search(&query)?,
    )
}

#[test]
fn coordinator_publishes_only_changed_paths_and_reclaims_old_lexical_files() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = [
        upsert(&fixture, "a.txt", "alpha body")?,
        upsert(&fixture, "b.txt", "beta body")?,
    ];
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    let applied = configuration();
    let mut update = updater(&fixture, &lock, &reservation);
    update.configuration = Some(&applied);
    let first = update.apply(&initial, None)?;
    assert_eq!(first.lexical.changed.changed_artifacts, 2);
    assert_eq!(first.lexical.changed.added_documents, 2);
    assert_eq!(first.graph.sealed_records, 0);
    drop(lock);
    drop(reservation);
    assert_eq!(search(&fixture, "alpha")?.hits[0].citation.path, "a.txt");
    let read = fixture.store.current_read(&fixture.scope)?;
    let counts = read.collection_state()?.unwrap().counts;
    assert_eq!(
        (
            counts.files,
            counts.chunks,
            counts.facts,
            counts.source_bytes
        ),
        (2, 2, 0, 19)
    );
    let unchanged = serde_json::to_value(read.artifact("b.txt")?)?;
    drop(read);
    let changes = [upsert(&fixture, "a.txt", "gamma body")?];
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    let next = updater(&fixture, &lock, &reservation).apply(&changes, None)?;
    assert_eq!(next.lexical.changed.changed_artifacts, 1);
    assert_eq!(next.lexical.changed.added_documents, 1);
    assert_eq!(next.graph.sealed_records, 0);
    drop(lock);
    drop(reservation);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.collection_state()?.unwrap().counts, counts);
    assert!(read.require_token(&first.commit.token).is_err());
    assert_eq!(serde_json::to_value(read.artifact("b.txt")?)?, unchanged);
    assert_eq!(
        read.lookup_paths(Lookup::Files, "", None, 10)?.paths,
        ["a.txt", "b.txt"]
    );
    assert!(!fixture.store.directory().join("lexical-0").exists());
    assert!(!fixture.store.directory().join("lexical-1").exists());
    drop(read);
    assert!(search(&fixture, "alpha")?.hits.is_empty());
    assert_eq!(search(&fixture, "gamma")?.hits[0].text, "gamma body");
    assert_eq!(search(&fixture, "beta")?.hits[0].citation.path, "b.txt");
    Ok(())
}

#[test]
fn coordinator_reuses_shared_vectors_tombstones_and_resurrects_exact_ids() -> Result<()> {
    let model_dir = tempfile::tempdir()?;
    let vocabulary = serde_json::to_vec(&serde_json::json!({
        "alpha": [1.,0.,0.,0.,0.,0.,0.,0.],
        "beta": [0.,1.,0.,0.,0.,0.,0.,0.],
        "gamma": [0.,0.,1.,0.,0.,0.,0.,0.]
    }))?;
    std::fs::write(model_dir.path().join("vocabulary.json"), &vocabulary)?;
    std::fs::write(
        model_dir.path().join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "name": "original-current-update-fixture", "license": "Apache-2.0",
            "dimensions": DIMENSIONS, "sha256": format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    let model = EmbeddingModel::open(model_dir.path())?;
    let objects = airk_retrieval::model::inspect_object_plan(model_dir.path())?;
    let fixture = Fixture::with_pipeline(objects.pipeline_digest)?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 1 << 20)?;
    airk_store::model_objects::ModelObjects::new(fixture.store.directory()).retain(
        model_dir.path(),
        &objects,
        &reservation,
    )?;
    drop(reservation);
    let selection = airk_core::vector_selection::VectorSelectionPolicy::default();
    let mut applied = configuration();
    applied.model = Some(objects.clone());
    let initial = fixture.store.current_read(&fixture.scope)?.token()?;
    let lock = fixture.store.lock()?;
    fixture
        .store
        .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?
        .initialize_collection(&initial, &applied)?;
    drop(lock);
    let apply = |changes: Vec<ArtifactChange>| -> Result<EmbeddingCacheStats> {
        let reservation = fixture
            .quota
            .reserve(&fixture.scope.collection, 512 << 20)?;
        let lock = fixture.store.lock()?;
        let mut stats = EmbeddingCacheStats::default();
        updater(&fixture, &lock, &reservation).apply(
            &changes,
            Some(SemanticUpdate {
                model: &model,
                objects: objects.clone(),
                selection: &selection,
                cache: EmbeddingCacheContext {
                    cache: None,
                    bypass: false,
                    stats: &mut stats,
                    max_workers: 1,
                    working_memory_bytes: 128 << 20,
                },
            }),
        )?;
        Ok(stats)
    };
    let cold = apply(vec![
        upsert(&fixture, "a.txt", "alpha")?,
        upsert(&fixture, "b.txt", "alpha")?,
        upsert(&fixture, "c.txt", "beta")?,
    ])?;
    assert_eq!(cold.vector_graph_nodes, 2);
    assert_eq!(cold.vector_reused_inputs, 1);
    use airk_kernel::current::{CurrentSearchMode, search_current};
    let current_search = |mode, prefix, max_bytes, expected: Option<&CurrentToken>| {
        search_current(
            &fixture.store,
            &fixture.scope,
            expected,
            mode,
            &SearchQuery {
                text: "alpha".into(),
                path_prefix: prefix,
                limit: 10,
                max_bytes,
            },
            Arc::new(|_| Ok(())),
        )
    };
    let vector = current_search(CurrentSearchMode::Vector, None, 4096, None)?;
    assert_eq!(vector.hits.len(), 3);
    assert_eq!(vector.hits[0].citation.path, "a.txt");
    assert_eq!(vector.hits[1].citation.path, "b.txt");
    let hybrid = current_search(CurrentSearchMode::Hybrid, None, 4096, Some(&vector.token))?;
    assert_eq!(hybrid.hits.len(), 3);
    assert_eq!(hybrid.token, vector.token);
    // Scope selection must rank beta even though alpha is globally nearer.
    let scoped = current_search(CurrentSearchMode::Vector, Some("c.".into()), 4096, None)?;
    assert_eq!(scoped.hits.len(), 1);
    assert_eq!(scoped.hits[0].text, "beta");
    assert!(!scoped.truncated);
    assert!(
        current_search(
            CurrentSearchMode::Vector,
            Some("absent/".into()),
            4096,
            None
        )?
        .hits
        .is_empty()
    );
    let tiny = current_search(CurrentSearchMode::Hybrid, None, 3, None)?;
    assert_eq!(tiny.hits.len(), 1);
    assert_eq!(tiny.hits[0].text, "alp");
    assert!(tiny.truncated && tiny.hits[0].truncated);
    let descriptor = model.cache_descriptor()?;
    let alpha = airk_core::embedding::embedding_key(&descriptor, b"alpha")?;
    let beta = airk_core::embedding::embedding_key(&descriptor, b"beta")?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let alpha_id = read.vector_binding(alpha)?.unwrap().graph_id;
    assert_eq!(
        read.collection_state()?.unwrap().configuration.model,
        Some(objects.clone())
    );
    let beta_id = read.vector_binding(beta)?.unwrap().graph_id;
    let pointers = read.vector_occurrence_pointers(alpha)?.unwrap();
    assert_eq!(pointers.len(), 2);
    assert_eq!(
        read.occurrence_hit(&fixture.store, &pointers[0])?.text,
        "alpha"
    );
    drop(read);
    let changed = apply(vec![
        upsert(&fixture, "a.txt", "gamma")?,
        ArtifactChange::Delete("b.txt".into()),
    ])?;
    assert_eq!(changed.vector_graph_nodes, 1);
    assert!(current_search(CurrentSearchMode::Vector, None, 4096, Some(&vector.token)).is_err());
    let refreshed = current_search(CurrentSearchMode::Hybrid, None, 4096, None)?;
    assert!(
        refreshed
            .hits
            .iter()
            .all(|hit| hit.citation.path != "b.txt" && hit.text != "alpha")
    );
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(!read.read(alpha_id)?.live);
    assert!(read.occurrence_hit(&fixture.store, &pointers[0]).is_err());
    assert_eq!(read.vector_binding(beta)?.unwrap().graph_id, beta_id);
    assert!(read.read(beta_id)?.live);
    drop(read);
    let restored = apply(vec![upsert(&fixture, "d.txt", "alpha")?])?;
    assert_eq!(restored.vector_graph_nodes, 0);
    assert_eq!(restored.inferences, 0);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.vector_binding(alpha)?.unwrap().graph_id, alpha_id);
    assert!(read.read(alpha_id)?.live);
    assert_eq!(
        read.vector_occurrence_pointers(alpha)?.unwrap()[0].path,
        "d.txt"
    );
    drop(read);
    assert_eq!(search(&fixture, "alpha")?.hits[0].citation.path, "d.txt");
    let restored = current_search(CurrentSearchMode::Vector, None, 4096, None)?;
    assert_eq!(restored.hits[0].citation.path, "d.txt");
    // Shared current gates prevent observing an applying writer, and a corrupt
    // retained source cannot be replaced with the live workspace's contents.
    let lock = fixture.store.lock()?;
    let writer = fixture
        .store
        .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(current_search(CurrentSearchMode::Vector, None, 4096, None).is_err());
    drop(writer);
    drop(lock);
    let source = fixture
        .store
        .current_read(&fixture.scope)?
        .artifact("d.txt")?
        .unwrap()
        .content_hash;
    std::fs::write(
        fixture
            .store
            .directory()
            .join("objects/sources")
            .join(source),
        b"wrong",
    )?;
    assert!(current_search(CurrentSearchMode::Vector, None, 4096, None).is_err());
    Ok(())
}

#[test]
fn current_search_handles_lexical_only_state_and_validates_exact_tokens() -> Result<()> {
    use airk_kernel::current::{CurrentSearchMode, search_current};
    let fixture = Fixture::new()?;
    let lock = fixture.store.lock()?;
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let config = configuration();
    let mut update = updater(&fixture, &lock, &reservation);
    update.configuration = Some(&config);
    let result = update.apply(&[upsert(&fixture, "a.txt", "alpha body")?], None)?;
    drop(lock);
    let query = SearchQuery {
        text: "alpha".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 100,
    };
    let search = |mode, token| {
        search_current(
            &fixture.store,
            &fixture.scope,
            token,
            mode,
            &query,
            Arc::new(|_| Ok(())),
        )
    };
    let response = search(CurrentSearchMode::Lexical, Some(&result.commit.token))?;
    assert_eq!(response.hits[0].text, "alpha body");
    assert!(!response.truncated);
    assert!(search(CurrentSearchMode::Vector, None).is_err());
    assert!(search(CurrentSearchMode::Hybrid, None).is_err());
    let mut foreign = result.commit.token;
    foreign.sequence += 1;
    assert!(search(CurrentSearchMode::Lexical, Some(&foreign)).is_err());
    Ok(())
}

#[test]
fn coordinator_admission_and_captured_source_failures_leave_the_head_unchanged() -> Result<()> {
    let fixture = Fixture::new()?;
    let changes = [upsert(&fixture, "a.txt", "alpha")?];
    let before = fixture.store.current_read(&fixture.scope)?.token()?;
    // Metadata plus even the minimum private graph cannot fit this admission,
    // independently of how the overlay adapts to a larger reservation.
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 16 << 10)?;
    let lock = fixture.store.lock()?;
    assert!(
        updater(&fixture, &lock, &reservation)
            .apply(&changes, None)
            .is_err()
    );
    drop(lock);
    drop(reservation);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
    assert!(!fixture.store.directory().join("lexical-1").exists());
    let ArtifactChange::Upsert(input) = &changes[0] else {
        unreachable!()
    };
    std::fs::write(
        fixture
            .store
            .directory()
            .join("objects/sources")
            .join(&input.artifact.content_hash),
        b"wrong",
    )?;
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    assert!(
        updater(&fixture, &lock, &reservation)
            .apply(&changes, None)
            .is_err()
    );
    drop(lock);
    drop(reservation);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
    assert!(search(&fixture, "alpha")?.hits.is_empty());
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CrashCase {
    data: std::path::PathBuf,
    scope: CurrentScope,
    input: CapturedInput,
    applying: bool,
}

#[test]
fn coordinator_process_child() -> Result<()> {
    let Ok(encoded) = std::env::var("AIRK_CURRENT_UPDATER_CASE") else {
        return Ok(());
    };
    let case: CrashCase = serde_json::from_str(&encoded)?;
    let quota = Quota::open(&case.data)?;
    let store = FileStore::new(
        case.data
            .join("collections-1.2")
            .join(&case.scope.collection),
    );
    let reservation = quota.reserve(&case.scope.collection, 512 << 20)?;
    let lock = store.lock()?;
    let applied = configuration();
    CurrentUpdater {
        store: &store,
        quota: &quota,
        scope: &case.scope,
        writer: &lock,
        reservation: &reservation,
        configuration: Some(&applied),
    }
    .apply_observed(
        &[ArtifactChange::Upsert(Box::new(case.input))],
        None,
        &mut |stage| {
            use airk_kernel::current::CurrentUpdateStage;
            if stage
                == if case.applying {
                    CurrentUpdateStage::Applying
                } else {
                    CurrentUpdateStage::Prepared
                }
            {
                std::process::exit(73);
            }
        },
    )?;
    anyhow::bail!("requested current update boundary was not observed")
}

#[test]
fn coordinator_process_exit_replays_captured_components_without_live_sources() -> Result<()> {
    for applying in [false, true] {
        let fixture = Fixture::new()?;
        let before = fixture.store.current_read(&fixture.scope)?.token()?;
        let (_, input) = retained_lexical_artifact(&fixture, "a.txt", "captured alpha")?;
        let case = CrashCase {
            data: fixture.quota.root().into(),
            scope: fixture.scope.clone(),
            input,
            applying,
        };
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "current_update::coordinator_process_child",
                "--nocapture",
            ])
            .env("AIRK_CURRENT_UPDATER_CASE", serde_json::to_string(&case)?)
            .output()?;
        ensure!(
            output.status.code() == Some(73),
            "current updater child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if applying {
            assert!(fixture.store.current_read(&fixture.scope).is_err());
        } else {
            assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
        }
        let root = std::path::Path::new(&fixture.scope.root);
        std::fs::create_dir_all(root)?;
        std::fs::write(root.join("a.txt"), b"later live edit")?;
        let recovered =
            airk_kernel::current::recover_current(&fixture.store, &fixture.quota, &fixture.scope)?;
        assert_eq!(
            recovered.action,
            airk_kernel::current::CurrentRecoveryAction::Applied
        );
        let result = search(&fixture, "alpha")?;
        assert_eq!(result.token, recovered.token);
        assert_eq!(result.hits[0].text, "captured alpha");
        let read = fixture.store.current_read(&fixture.scope)?;
        assert_eq!(read.collection_state()?.unwrap().counts.files, 1);
        assert!(read.require_token(&before).is_err());
        drop(read);
        assert_eq!(
            airk_kernel::current::recover_current(&fixture.store, &fixture.quota, &fixture.scope)?
                .action,
            airk_kernel::current::CurrentRecoveryAction::AlreadyReady
        );
        assert_eq!(std::fs::read(root.join("a.txt"))?, b"later live edit");
    }
    Ok(())
}

#[test]
fn collection_configuration_and_counters_follow_atomic_updates() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = fixture.store.current_read(&fixture.scope)?.token()?;
    let config = configuration();
    let lock = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
    let initialized = current.initialize_collection(&initial, &config)?;
    assert!(
        current
            .initialize_collection(&initialized.token, &config)
            .is_err()
    );
    drop(current);
    drop(lock);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.collection_state()?.unwrap().counts.files, 0);
    assert_eq!(read.head().lexical_sequence, 0);
    drop(read);
    let changes = vec![
        upsert(&fixture, "a.txt", "alpha")?,
        upsert(&fixture, "b.txt", "beta")?,
    ];
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    let first = updater(&fixture, &lock, &reservation).apply(&changes, None)?;
    drop(lock);
    drop(reservation);
    let mut next_config = config.clone();
    next_config
        .roots
        .insert("docs".into(), fixture.scope.root.clone());
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    let mut update = updater(&fixture, &lock, &reservation);
    update.configuration = Some(&next_config);
    assert!(update.apply(&changes[..1], None).is_err());
    drop(lock);
    drop(reservation);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, first.commit.token);
    assert!(
        read.collection_state()?
            .unwrap()
            .configuration
            .roots
            .is_empty()
    );
    drop(read);
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 512 << 20)?;
    let lock = fixture.store.lock()?;
    let mut update = updater(&fixture, &lock, &reservation);
    update.configuration = Some(&next_config);
    let second = update.apply(&changes, None)?;
    drop(lock);
    drop(reservation);
    let read = fixture.store.current_read(&fixture.scope)?;
    let state = read.collection_state()?.unwrap();
    assert_eq!(state.configuration.roots, next_config.roots);
    assert_eq!(
        (
            state.counts.files,
            state.counts.chunks,
            state.counts.source_bytes
        ),
        (2, 2, 9)
    );
    drop(read);
    let lock = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.commit_artifact(&second.commit.token, "a.txt", None)?;
    drop(current);
    drop(lock);
    let state = fixture
        .store
        .current_read(&fixture.scope)?
        .collection_state()?
        .unwrap();
    assert_eq!(
        (
            state.counts.files,
            state.counts.chunks,
            state.counts.source_bytes
        ),
        (1, 1, 4)
    );
    Ok(())
}
