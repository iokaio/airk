// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::current::{CurrentHead, CurrentScope, lexical::LexicalManifest, map};
use airk_core::graph_storage::{GraphHead, GraphScope};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn retirement_inventory_is_bounded_by_deletion_batch_not_total_old_files() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let objects = owner.path().join("objects/sources");
    fs::create_dir_all(&objects)?;
    for n in 0..4100u64 {
        fs::write(objects.join(format!("{n:064x}")), b"old")?;
    }
    let (files, _) = retirement_inventory(owner.path())?;
    assert_eq!(files.len(), 4097);
    assert_eq!(fs::read_dir(&objects)?.count(), 4100);
    for path in files.into_iter().take(4096) {
        fs::remove_file(path)?;
    }
    let (files, directories) = retirement_inventory(owner.path())?;
    assert_eq!(files.len(), 4);
    assert_eq!(directories.len(), 3);
    Ok(())
}

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

#[test]
fn installed_data_root_rename_preserves_publication_settings_pause_and_receipts() -> Result<()> {
    use crate::service::{
        ServiceLease, data_scope,
        registry::{ServiceRegistry, data_relocation},
    };
    use airk_core::service::{JobKind, ProjectRegistration, RequestKey};
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let data = base.join("service-1.4");
    let destination = base.join("index-v1");
    let source = base.join("source");
    let control = base.join("control");
    for path in [&data, &source, &control] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("original.rs"), b"source remains unchanged")?;
    let namespace = Incarnation::from_bytes([1; 16]);
    let slot = Incarnation::from_bytes([2; 16]);
    let source_text = source.to_str().unwrap();
    let registration = ProjectRegistration::new(
        source_text.into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register_with_setup(&registration, Some(true))?;
    data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(&data, source_text, "fixture", slot)?;
    let store = collection.selected_store()?;
    let token = initialize(&store, source_text, slot)?;
    fs::write(
        store.directory().join("scan-limits.json"),
        b"saved settings",
    )?;
    let (ns, epoch) = registry.saved_request_epoch()?;
    let caller = Incarnation::from_bytes([3; 16]);
    let key = RequestKey {
        namespace: ns,
        epoch,
        nonce: Incarnation::from_bytes([4; 16]),
    };
    let job = registry.enqueue(registration.binding, caller, &key, JobKind::Index)?;
    registry.pause(registration.binding, true)?;
    let manifest = fs::read(collection.directory.join("active.json"))?;
    drop((store, collection, registry));
    let lease = ServiceLease::acquire(&control)?;
    let quota = Quota::open_service(&data)?;
    let original_policy = quota.policy()?;
    let status = quota.status()?;
    let mut constrained = original_policy.clone();
    constrained.root_bytes = status.used_bytes + status.reserved_bytes;
    quota.configure(constrained)?;
    assert!(
        data_relocation::relocate(&lease, &data, &destination).is_err(),
        "durable allowances still count against metadata growth"
    );
    assert!(data.exists());
    assert!(!destination.exists());
    data_relocation::require_settled(&control)?;
    quota.configure(original_policy)?;
    data_relocation::relocate(&lease, &data, &destination)?;
    data_relocation::relocate(&lease, &data, &destination)?;
    assert!(!data.exists());
    assert!(data_relocation::selected(&control, &data, &destination)?);
    let migrated = ProjectRegistration::new(
        source_text.into(),
        destination.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let registry = ServiceRegistry::open_saved(&control)?;
    assert!(registry.get(migrated.binding)?.paused);
    assert_eq!(registry.job(job.id)?.project, migrated.binding);
    assert_eq!(
        registry.request_receipt(caller, &key)?.0.unwrap().id,
        job.id
    );
    let collection = Collection::initialize(&destination, source_text, "fixture", slot)?;
    assert_eq!(
        fs::read(collection.directory.join("active.json"))?,
        manifest
    );
    let selected = collection.select()?;
    let scope = selected
        .store
        .current_scope(source_text, "fixture")?
        .unwrap();
    assert_eq!(selected.store.current_read(&scope)?.token()?, token);
    assert_eq!(
        fs::read(selected.store.directory().join("scan-limits.json"))?,
        b"saved settings"
    );
    assert_eq!(
        fs::read(source.join("original.rs"))?,
        b"source remains unchanged"
    );
    Ok(())
}
struct ChildGuard(Child);
#[test]
fn full_removal_preserves_sources_refuses_unknown_files_and_aliases_and_resumes_after_restart()
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
        prepare: false,
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
    assert!(
        registry.register(&project).is_err(),
        "pending removal fences re-registration"
    );
    let unknown = selected.directory().join("unknown-original.txt");
    fs::write(&unknown, b"keep unknown data")?;
    assert!(
        registry
            .purge_removed_project_turn(project.binding)
            .is_err()
    );
    assert_eq!(fs::read(&unknown)?, b"keep unknown data");
    fs::remove_file(&unknown)?;
    let alias = selected.directory().join("coverage.json");
    fs::hard_link(source.join("original.rs"), &alias)?;
    assert!(
        registry
            .purge_removed_project_turn(project.binding)
            .is_err()
    );
    assert_eq!(fs::read(source.join("original.rs"))?, b"original source");
    fs::remove_file(&alias)?;
    // Exceed one deletion turn to exercise interrupted batches on real files.
    let objects = selected.directory().join("objects/sources");
    fs::create_dir_all(&objects)?;
    for id in 0..4097 {
        fs::write(objects.join(format!("{id:064x}")), b"derived fixture")?;
    }
    // A real request gate still defers physical deletion after the worker drains.
    let request = collection.select()?;
    assert!(
        registry
            .purge_removed_project_turn(project.binding)
            .is_err()
    );
    assert!(selected.directory().join("current.sqlite3").exists());
    drop(request);
    drop(selected);
    assert!(
        !registry.purge_removed_project_turn(project.binding)?,
        "one slot is removed per turn"
    );
    drop(registry);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let mut complete = false;
    for _ in 0..8 {
        complete = registry.purge_removed_project_turn(project.binding)?;
        if complete {
            break;
        }
        assert!(
            registry.retained_project(project.binding).is_ok(),
            "pending deletion must retain its ownership fence"
        );
    }
    assert!(complete, "bounded removal did not finish after restart");
    assert!(!data.join("collections-1.4/fixture").exists());
    assert!(registry.retained_project(project.binding).is_err());
    assert!(registry.project_was_removed(project.binding)?);
    assert!(registry.source_was_removed(&project.root)?);
    assert!(
        registry.job(job.id).is_err(),
        "detailed job receipts are removed"
    );
    assert!(
        registry
            .register_missing_with_setup(&project, Some(true))
            .is_err(),
        "explicit removal stays opted out"
    );
    assert_eq!(fs::read(source.join("original.rs"))?, b"original source");
    let alternate = ProjectRegistration::new(
        project.root.clone(),
        project.data_root.clone(),
        "alternate".into(),
    )?;
    assert!(
        registry
            .register_missing_with_setup(&alternate, Some(true))
            .is_err(),
        "a default collection must not bypass a removed source's opt-out"
    );
    registry.register(&project)?;
    assert!(!registry.project_was_removed(project.binding)?);
    assert!(!registry.source_was_removed(&project.root)?);
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
