// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::current::{CurrentHead, CurrentScope, lexical::LexicalManifest, map};
use airk_core::graph_storage::{GraphHead, GraphScope};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn selection_waits_for_short_metadata_handoffs_and_marks_only_real_lock_contention() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut lease = Some(acquire_file(owner.path(), "manifest.lock")?);
    let mut attempts = 0;
    // Release only after observing real native contention. Thread startup and
    // timer coalescing must not determine whether a short handoff fits 100 ms.
    let selected = retry_manifest_lock(|| {
        attempts += 1;
        let acquired = acquire_file(owner.path(), "manifest.lock");
        if attempts == 1 {
            assert!(matches!(
                acquired
                    .as_ref()
                    .err()
                    .and_then(|error| error.downcast_ref::<std::fs::TryLockError>()),
                Some(std::fs::TryLockError::WouldBlock)
            ));
            drop(lease.take());
        }
        acquired
    })?;
    assert_eq!(attempts, 2);
    let began = Instant::now();
    let error = manifest_lock(owner.path())
        .err()
        .context("live metadata owner was ignored")?;
    assert!(error.downcast_ref::<SelectionContention>().is_some());
    assert!(
        began.elapsed() >= Duration::from_millis(100) && began.elapsed() < Duration::from_secs(2)
    );
    drop(selected);
    // Native ownership also releases across threads, independently of how
    // quickly the operating system schedules that thread.
    let lease = acquire_file(owner.path(), "manifest.lock")?;
    std::thread::spawn(move || drop(lease)).join().unwrap();
    drop(manifest_lock(owner.path())?);
    fs::write(
        owner.path().join("manifest.lock"),
        b"unknown original bytes",
    )?;
    let error = manifest_lock(owner.path())
        .err()
        .context("invalid lock was accepted")?;
    assert!(error.downcast_ref::<SelectionContention>().is_none());
    assert_eq!(
        fs::read(owner.path().join("manifest.lock"))?,
        b"unknown original bytes"
    );
    Ok(())
}

fn initialize(store: &FileStore, root: &str, slot: Incarnation) -> Result<CurrentToken> {
    let scope = CurrentScope {
        root: root.into(),
        collection: "fixture".into(),
        incarnation: slot,
    };
    let graph = GraphHead::empty(GraphScope {
        root: root.into(),
        collection: "fixture".into(),
        incarnation: slot,
        graph_instance: slot,
        specification: Sha256Digest::from_bytes([5; 32]),
        record_bytes: 16,
    })?;
    let lexical = LexicalManifest::empty()?;
    let head = CurrentHead {
        version: 2,
        scope: scope.clone(),
        sequence: 0,
        artifact_root: map::empty_root(&scope)?,
        lexical_sequence: 0,
        lexical_commit: lexical.digest()?,
        graph_head: graph,
    };
    let quota = Quota::open_service(store.managed_path()?.root)?;
    let writer = store.lock()?;
    let mut current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
    current.initialize(head.clone())?;
    current.install_initial_lexical_manifest(&head.token()?, &lexical.canonical_bytes()?)?;
    head.token()
}
struct ChildGuard(Child);
#[test]
fn derived_cleanup_requires_inactive_fenced_approval_preserves_settings_and_resumes_after_cancel()
-> Result<()> {
    use crate::service::{ProjectLease, registry::ServiceRegistry};
    use airk_core::{
        service::{JobOutcome, ProjectRegistration, RequestKey},
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
    };
    let temporary = tempfile::tempdir()?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    let control = temporary.path().join("control");
    for path in [&source, &data, &control] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("original.rs"), b"original source")?;
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    let root = source.to_str().unwrap();
    let namespace = Incarnation::from_bytes([1; 16]);
    let slot = Incarnation::from_bytes([2; 16]);
    let engine = Sha256Digest::from_bytes([3; 32]);
    let project =
        ProjectRegistration::new(root.into(), data.to_str().unwrap().into(), "fixture".into())?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&project)?;
    crate::service::data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(&data, root, "fixture", slot)?;
    let selected = collection.selected_store()?;
    initialize(&selected, root, slot)?;
    let policy = serde_json::to_vec(&serde_json::json!({"owner_root":root,"enabled":false}))?;
    fs::write(selected.directory().join("vector-policy.json"), &policy)?;
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_worker(&mut lease)?;
    assert!(collection.derived_cleanup_preview(&lease).is_err());
    let (ns, epoch) = registry.request_epoch()?;
    let request = ConfigurationRequest {
        incarnation: slot,
        settings: engine,
        configuration: ProjectConfiguration::Unregister,
    };
    let key = RequestKey {
        namespace: ns,
        epoch,
        nonce: Incarnation::from_bytes([3; 16]),
    };
    let job = registry.enqueue_configuration(project.binding, namespace, &key, &request, engine)?;
    registry.claim_job_id(&lease, job.id)?.unwrap();
    registry.record_configuration_result(
        &lease,
        job.id,
        &serde_json::json!({"unregistration_requested":true}),
    )?;
    registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    drop(lease);
    let submit = |registry: &mut ServiceRegistry, configuration, nonce: u8| -> Result<_> {
        let (namespace, epoch) = registry.request_epoch()?;
        registry.enqueue_configuration(
            project.binding,
            namespace,
            &RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([nonce; 16]),
            },
            &ConfigurationRequest {
                incarnation: namespace,
                settings: project.binding,
                configuration,
            },
            engine,
        )
    };
    let preview_job = submit(
        &mut registry,
        ProjectConfiguration::PrepareDerivedCleanup,
        4,
    )?;
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_derived_worker(&mut lease)?;
    registry.claim_job_id(&lease, preview_job.id)?.unwrap();
    let unknown = selected.directory().join("unknown-original.txt");
    fs::write(&unknown, b"keep unknown data")?;
    assert!(collection.derived_cleanup_preview(&lease).is_err());
    assert_eq!(fs::read(&unknown)?, b"keep unknown data");
    fs::remove_file(&unknown)?;
    let alias = selected.directory().join("coverage.json");
    fs::hard_link(source.join("original.rs"), &alias)?;
    assert!(collection.derived_cleanup_preview(&lease).is_err());
    assert_eq!(fs::read(source.join("original.rs"))?, b"original source");
    fs::remove_file(alias)?;
    let preview = collection.derived_cleanup_preview(&lease)?;
    registry.record_configuration_result(
        &lease,
        preview_job.id,
        &serde_json::to_value(&preview)?,
    )?;
    registry.complete_job(
        &lease,
        preview_job.id,
        JobOutcome::Succeeded { revision: None },
    )?;
    let job = submit(
        &mut registry,
        ProjectConfiguration::DeleteDerived {
            preview: preview.guard,
        },
        5,
    )?;
    registry.claim_job_id(&lease, job.id)?.unwrap();
    assert!(
        collection
            .delete_derived_turn(&lease, job.id, engine)
            .is_err()
    );
    assert!(selected.directory().join("current.sqlite3").is_file());
    let report = collection.delete_derived_turn(&lease, job.id, preview.guard)?;
    assert!(!report.complete);
    registry.complete_job(&lease, job.id, JobOutcome::Canceled)?;
    assert!(
        registry.register(&project).is_err(),
        "cancellation must retain the cleanup barrier"
    );
    drop(lease);
    let job = submit(
        &mut registry,
        ProjectConfiguration::DeleteDerived {
            preview: preview.guard,
        },
        6,
    )?;
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_derived_worker(&mut lease)?;
    registry.claim_job_id(&lease, job.id)?.unwrap();
    assert!(collection.derived_cleanup_preview(&lease)?.resuming);
    let mut complete = None;
    for _ in 0..10 {
        let report = collection.delete_derived_turn(&lease, job.id, preview.guard)?;
        if report.complete {
            complete = Some(report);
            break;
        }
    }
    let report = complete.context("cleanup did not complete")?;
    assert!(!selected.directory().try_exists()?);
    assert_eq!(fs::read(source.join("original.rs"))?, b"original source");
    let retained = collection.selected_store()?;
    assert_eq!(
        fs::read(retained.directory().join("vector-policy.json"))?,
        policy
    );
    assert!(!retained.directory().join("current.sqlite3").try_exists()?);
    registry.record_configuration_result(&lease, job.id, &serde_json::to_value(&report)?)?;
    // Captured completion and the barrier recover together after owner death.
    drop(lease);
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_derived_worker(&mut lease)?;
    registry.recover_jobs(&lease)?;
    assert_eq!(
        registry.job(job.id)?.state,
        airk_core::service::JobState::Succeeded
    );
    drop(lease);
    registry.register(&project)?;
    Ok(())
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn collection_handoff_child() -> Result<()> {
    let Some(data) = std::env::var_os("AIRK_HANDOFF_DATA") else {
        return Ok(());
    };
    let root = std::env::var("AIRK_HANDOFF_ROOT")?;
    let ready =
        PathBuf::from(std::env::var_os("AIRK_HANDOFF_READY").context("missing readiness path")?);
    let mode = std::env::var("AIRK_HANDOFF_MODE")?;
    let collection = Collection::existing(Path::new(&data), &root, "fixture")?;
    let selected = collection.select()?;
    let scope = selected
        .store
        .current_scope(&root, "fixture")?
        .context("initial scope missing")?;
    let expected = selected.store.current_read(&scope)?.token()?;
    let settings = settings_digest(&selected.store, &root)?;
    drop(selected);
    let target = Incarnation::from_bytes([2; 16]);
    let mut replacement =
        collection.begin_replacement(target, Sha256Digest::from_bytes([3; 32]))?;
    initialize(&replacement.store(), &root, target)?;
    replacement.commit_observed(Some(expected), settings, &mut |stage| {
        if matches!(
            (mode.as_str(), stage),
            ("before", HandoffStage::BeforeSwitch) | ("after", HandoffStage::AfterSwitch)
        ) {
            fs::write(&ready, b"ready")?;
            loop {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(())
    })?;
    Ok(())
}

#[test]
fn manifest_process_death_selects_complete_old_or_new_and_pins_retirement() -> Result<()> {
    for mode in ["before", "after"] {
        let fixture = tempfile::tempdir()?;
        let data = fixture.path().join("data");
        let source = fixture.path().join("source");
        fs::create_dir(&data)?;
        fs::create_dir(&source)?;
        fs::write(
            source.join("original.txt"),
            b"Untouched original handoff fixture.",
        )?;
        let source = source.canonicalize()?;
        let root = source.to_str().context("non-UTF-8 fixture")?;
        let collection =
            Collection::initialize(&data, root, "fixture", Incarnation::from_bytes([1; 16]))?;
        let old = collection.select()?;
        let old_token = initialize(&old.store, root, Incarnation::from_bytes([1; 16]))?;
        let ready = fixture.path().join("ready");
        let mut child = ChildGuard(
            Command::new(std::env::current_exe()?)
                .arg("--exact")
                .arg("service::collection::tests::collection_handoff_child")
                .arg("--nocapture")
                .env("AIRK_HANDOFF_DATA", &data)
                .env("AIRK_HANDOFF_ROOT", root)
                .env("AIRK_HANDOFF_READY", &ready)
                .env("AIRK_HANDOFF_MODE", mode)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let until = Instant::now() + Duration::from_secs(20);
        while !ready.exists() {
            ensure!(
                child.0.try_wait()?.is_none() && Instant::now() < until,
                "handoff child did not reach boundary"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(child);
        collection.recover_abandoned_replacement()?;
        let current = collection.select()?;
        let scope = current
            .store
            .current_scope(root, "fixture")?
            .context("selected scope missing")?;
        let token = current.store.current_read(&scope)?.token()?;
        if mode == "before" {
            assert_eq!(token, old_token);
        } else {
            assert_ne!(token.incarnation, old_token.incarnation);
            assert!(collection.completed_rebuild(Sha256Digest::from_bytes([3; 32]))?);
            assert_eq!(collection.retire()?.removed_directories, 0);
        }
        let old_scope = old
            .store
            .current_scope(root, "fixture")?
            .context("old leased scope missing")?;
        assert_eq!(old.store.current_read(&old_scope)?.token()?, old_token);
        drop(old);
        drop(current);
        assert_eq!(collection.retire()?.removed_directories, 1);
        assert_eq!(
            fs::read(source.join("original.txt"))?,
            b"Untouched original handoff fixture."
        );
    }
    Ok(())
}

#[test]
fn foreign_selection_and_unknown_retirement_files_fail_without_deletion() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let data = fixture.path().join("data");
    fs::create_dir(&data)?;
    let collection = Collection::initialize(
        &data,
        "/original-root",
        "fixture",
        Incarnation::from_bytes([1; 16]),
    )?;
    let old = collection.selected_store()?;
    let initial = initialize(&old, "/original-root", Incarnation::from_bytes([1; 16]))?;
    assert!(Collection::existing(&data, "/other-root", "fixture").is_err());
    let mut replacement = collection.begin_replacement(
        Incarnation::from_bytes([2; 16]),
        Sha256Digest::from_bytes([4; 32]),
    )?;
    initialize(
        &replacement.store(),
        "/original-root",
        Incarnation::from_bytes([2; 16]),
    )?;
    replacement.commit(Some(initial), settings_digest(&old, "/original-root")?)?;
    drop(replacement);
    let unknown = old.directory().join("keep-my-note.txt");
    fs::write(&unknown, b"Preserve this unrelated note.")?;
    assert!(collection.retire().is_err());
    assert_eq!(fs::read(unknown)?, b"Preserve this unrelated note.");
    assert!(old.directory().join("current.sqlite3").is_file());
    Ok(())
}
