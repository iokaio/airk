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
    // F10 receipt follows physical relocation without promoting its audit basis.
    let mut integrity_lease =
        crate::service::ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut integrity_lease)?;
    let integrity_bound = data_scope::bind(&data, &control, namespace)?;
    let integrity_scope = store
        .current_owned_scope()?
        .context("initial scope missing")?;
    let integrity_read = store.current_read(&integrity_scope)?;
    collection
        .integrity_owner(&registry, &integrity_lease, &integrity_bound)?
        .record_damage(&integrity_read, token.digest, 700)?;
    let integrity_before = fs::read(collection.directory.join("integrity-observation.json"))?;
    drop(integrity_read);
    drop(integrity_lease);
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
        fs::read(collection.directory.join("integrity-observation.json"))?,
        integrity_before
    );
    let mut resumed_registry = ServiceRegistry::open(&control, namespace)?;
    let mut resumed_lease = crate::service::ProjectLease::acquire(&control, migrated.binding)?;
    resumed_registry.admit_worker(&mut resumed_lease)?;
    let resumed_bound = data_scope::bind(&destination, &control, namespace)?;
    let resumed_store = collection.selected_store()?;
    let resumed_scope = resumed_store
        .current_owned_scope()?
        .context("relocated scope missing")?;
    let resumed_read = resumed_store.current_read(&resumed_scope)?;
    let resumed_state = collection
        .integrity_owner(&resumed_registry, &resumed_lease, &resumed_bound)?
        .anchor(&resumed_read)?;
    assert_eq!(resumed_state.known_damage.len(), 1);
    assert_eq!(resumed_state.known_damage[0].component, token.digest);
    drop(resumed_read);
    drop(resumed_lease);
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

#[test]
fn integrity_anchor_is_unknown_and_damage_survives_worker_restart() -> Result<()> {
    use super::integrity::{PhysicalAudit, PrivacyEligibility, PublicationBasis};
    use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    use airk_core::service::ProjectRegistration;
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let (data, source, control) = (base.join("data"), base.join("source"), base.join("control"));
    for path in [&data, &source, &control] {
        fs::create_dir(path)?;
    }
    let namespace = Incarnation::from_bytes([21; 16]);
    let slot = Incarnation::from_bytes([22; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(&data, &registration.root, "fixture", slot)?;
    let store = collection.selected_store()?;
    let token = initialize(&store, &registration.root, slot)?;
    let scope = store
        .current_owned_scope()?
        .context("fixture scope missing")?;
    let read = store.current_read(&scope)?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    // Exclusion without registry admission is insufficient.
    assert!(
        collection
            .integrity_owner(&registry, &lease, &bound)
            .is_err()
    );
    registry.admit_worker(&mut lease)?;
    {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let anchor = owner.anchor(&read)?;
        assert_eq!(anchor.head.token()?, token);
        assert_eq!(anchor.basis, PublicationBasis::RetainedHeadMetadataAnchor);
        assert_eq!(anchor.physical_audit, PhysicalAudit::Unknown);
        assert_eq!(
            anchor.privacy,
            PrivacyEligibility::PlatformEvidenceUnavailable
        );
        let damaged = owner.record_damage(&read, token.digest, 123)?;
        assert_eq!(damaged.known_damage.len(), 1);
        assert_eq!(owner.anchor(&read)?.known_damage, damaged.known_damage);
    }
    drop(lease);
    let mut replacement = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut replacement)?;
    let reopened = Collection::existing(&data, &registration.root, "fixture")?;
    let owner = reopened.integrity_owner(&registry, &replacement, &bound)?;
    let state = owner.anchor(&read)?;
    assert_eq!(state.known_damage.len(), 1);
    assert_eq!(state.known_damage[0].component, token.digest);
    assert_eq!(
        read.token()?,
        token,
        "metadata foundation must not alter strict reads"
    );
    Ok(())
}

#[test]
fn integrity_owner_rejects_cross_registry_bound_root() -> Result<()> {
    use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    use airk_core::service::ProjectRegistration;
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let (data, source, control, other) = (
        base.join("data"),
        base.join("source"),
        base.join("control"),
        base.join("other"),
    );
    for path in [&data, &source, &control, &other] {
        fs::create_dir(path)?;
    }
    let namespace = Incarnation::from_bytes([31; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(
        &data,
        &registration.root,
        "fixture",
        Incarnation::from_bytes([32; 16]),
    )?;
    let mut foreign = ProjectLease::acquire(&other, registration.binding)?;
    assert!(registry.admit_worker(&mut foreign).is_err());
    assert!(
        collection
            .integrity_owner(&registry, &foreign, &bound)
            .is_err()
    );
    Ok(())
}

type IntegrityFixture = (
    tempfile::TempDir,
    Collection,
    crate::service::registry::ServiceRegistry,
    crate::service::data_scope::BoundDataRoot,
    crate::service::ProjectLease,
    FileStore,
    crate::current::CurrentScope,
);
fn integrity_fixture() -> Result<IntegrityFixture> {
    use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    use airk_core::service::ProjectRegistration;
    let temporary = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-census-fixture-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let base = temporary.path().canonicalize()?;
    let (data, source, control) = (base.join("data"), base.join("source"), base.join("control"));
    for path in [&data, &source, &control] {
        fs::create_dir(path)?;
    }
    let namespace = Incarnation::from_bytes([51; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let slot = Incarnation::from_bytes([52; 16]);
    let collection = Collection::initialize(&data, &registration.root, "fixture", slot)?;
    let store = collection.selected_store()?;
    initialize(&store, &registration.root, slot)?;
    let scope = store
        .current_owned_scope()?
        .context("fixture scope missing")?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    Ok((temporary, collection, registry, bound, lease, store, scope))
}

#[test]
fn integrity_replacement_preserves_damage_and_refuses_retired_head() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, old, scope) = integrity_fixture()?;
    let read = old.current_read(&scope)?;
    let token = read.token()?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    owner.record_damage(&read, token.digest, 100)?;
    drop(read);
    let next = Incarnation::from_bytes([53; 16]);
    let mut replacement = collection.begin_replacement(next, Sha256Digest::from_bytes([54; 32]))?;
    initialize(&replacement.store(), &scope.root, next)?;
    replacement.commit(Some(token), settings_digest(&old, &scope.root)?)?;
    drop(replacement);
    // Retained old bytes remain readable through their original strict scope,
    // but cannot overwrite selected-incarnation metadata.
    let retired = old.current_read(&scope)?;
    assert!(owner.anchor(&retired).is_err());
    drop(retired);
    let selected = collection.selected_store()?;
    let new_scope = selected
        .current_owned_scope()?
        .context("replacement scope missing")?;
    let read = selected.current_read(&new_scope)?;
    let state = owner.anchor(&read)?;
    assert_eq!(state.head.scope.incarnation, next);
    assert_eq!(state.known_damage.len(), 1);
    assert_eq!(state.known_damage[0].incarnation, scope.incarnation);
    assert_eq!(state.known_damage[0].component, token.digest);
    Ok(())
}

#[test]
fn integrity_abandoned_replacement_recovery_keeps_original_observation() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let state = owner.record_damage(&read, read.token()?.digest, 200)?;
    drop(read);
    let receipt = fs::read(collection.directory.join("integrity-observation.json"))?;
    let replacement = collection.begin_replacement(
        Incarnation::from_bytes([55; 16]),
        Sha256Digest::from_bytes([56; 32]),
    )?;
    // Simulate a process dying after durable replacement intent / slot creation.
    drop(replacement);
    collection.recover_abandoned_replacement()?;
    assert_eq!(
        fs::read(collection.directory.join("integrity-observation.json"))?,
        receipt
    );
    let read = store.current_read(&scope)?;
    assert_eq!(owner.anchor(&read)?.known_damage, state.known_damage);
    Ok(())
}

#[test]
fn integrity_scratch_recovery_and_malformed_record_never_clear_damage() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    owner.record_damage(&read, read.token()?.digest, 300)?;
    let path = collection.directory.join("integrity-observation.json");
    let before = fs::read(&path)?;
    let owned = collection.directory.join(".service-metadata-owned-crash");
    let unknown = collection
        .directory
        .join(".service-metadata-unrelated-crash");
    fs::write(&owned, &before)?;
    fs::write(&unknown, b"preserve unrelated partial bytes")?;
    collection.recover_abandoned_replacement()?;
    assert!(!owned.exists());
    assert_eq!(fs::read(&unknown)?, b"preserve unrelated partial bytes");
    assert_eq!(fs::read(&path)?, before);
    // A malformed durable record is not interpreted as missing/clean.
    fs::write(&path, b"{}")?;
    assert!(owner.anchor(&read).is_err());
    assert_eq!(fs::read(&path)?, b"{}");
    fs::write(&path, &before)?;
    assert_eq!(owner.anchor(&read)?.known_damage.len(), 1);
    Ok(())
}

#[test]
fn integrity_quota_refusal_preserves_the_last_durable_damage_receipt() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    owner.record_damage(&read, read.token()?.digest, 600)?;
    let path = collection.directory.join("integrity-observation.json");
    let before = fs::read(&path)?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let original = quota.policy()?;
    let status = quota.status()?;
    let mut full = original.clone();
    full.root_bytes = status
        .used_bytes
        .checked_add(status.reserved_bytes)
        .context("quota overflow")?;
    quota.configure(full)?;
    let refused = owner.record_damage(&read, Sha256Digest::from_bytes([98; 32]), 601);
    quota.configure(original)?;
    assert!(refused.is_err());
    assert_eq!(fs::read(&path)?, before);
    assert_eq!(owner.anchor(&read)?.known_damage.len(), 1);
    Ok(())
}

#[test]
fn integrity_live_owner_refuses_epoch_removal_and_namespace_replacement() -> Result<()> {
    let (temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    owner.record_damage(&read, read.token()?.digest, 900)?;
    let path = collection.directory.join("integrity-observation.json");
    let before = fs::read(&path)?;
    // Fault injection is confined to this fixture's registry. It models stale
    // epochs and namespace replacement independently of caller cached objects.
    let db = rusqlite::Connection::open(temporary.path().join("control/service-1.4.sqlite3"))?;
    let binding = lease.identity().0.to_string();
    db.execute(
        "UPDATE projects SET epoch=epoch+1 WHERE binding=?1",
        [&binding],
    )?;
    assert!(owner.anchor(&read).is_err());
    db.execute(
        "UPDATE projects SET epoch=epoch-1,registered=0,paused=1 WHERE binding=?1",
        [&binding],
    )?;
    assert!(owner.anchor(&read).is_err());
    // Re-registration alone does not recreate a matching admitted generation.
    db.execute(
        "UPDATE projects SET registered=1,paused=0,epoch=epoch+1 WHERE binding=?1",
        [&binding],
    )?;
    assert!(owner.anchor(&read).is_err());
    db.execute(
        "UPDATE projects SET epoch=epoch-1 WHERE binding=?1",
        [&binding],
    )?;
    db.execute(
        "UPDATE service_meta SET incarnation=?1 WHERE id=1",
        [Incarnation::from_bytes([59; 16]).to_string()],
    )?;
    assert!(owner.anchor(&read).is_err());
    assert_eq!(fs::read(path)?, before);
    Ok(())
}

#[test]
fn integrity_foreign_imported_receipt_is_preserved_and_never_adopted() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    owner.record_damage(&read, read.token()?.digest, 950)?;
    let path = collection.directory.join("integrity-observation.json");
    let mut imported: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    imported["owner"] = serde_json::to_value(Sha256Digest::from_bytes([60; 32]))?;
    // Keep canonical typed serialization so rejection is binding, not encoding.
    let typed: super::integrity::IntegrityObservation = serde_json::from_value(imported)?;
    let copied = serde_json::to_vec(&typed)?;
    fs::write(&path, &copied)?;
    assert!(owner.anchor(&read).is_err());
    assert_eq!(fs::read(path)?, copied);
    assert!(
        read.token().is_ok(),
        "observation receipt must not gate strict evidence reads"
    );
    Ok(())
}

#[test]
fn integrity_real_applying_replay_preserves_damage_and_refuses_unsafe_wal() -> Result<()> {
    use crate::current::{
        TransactionPhase,
        graph::{GraphOverlay, GraphOverlayLimits},
    };
    use airk_core::graph_storage::{GraphRecord, GraphRecordOverlay, PreparedGraphDelta};
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let old = store.current_read(&scope)?;
    let base = old.head().clone();
    let token = old.token()?;
    owner.record_damage(&old, token.digest, 1000)?;
    let receipt_path = collection.directory.join("integrity-observation.json");
    let receipt = fs::read(&receipt_path)?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let reservation = quota.reserve(&scope.collection, 4 << 20)?;
    let overlay = GraphOverlay::new(
        &reservation,
        store.current_read(&scope)?,
        GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?;
    overlay.append(
        0,
        GraphRecord {
            embedding_key: None,
            live: false,
            bytes: vec![0; 16],
        },
    )?;
    overlay.append(
        1,
        GraphRecord {
            embedding_key: Some(Sha256Digest::from_bytes([61; 32])),
            live: true,
            bytes: vec![7; 16],
        },
    )?;
    let sealed = overlay.seal()?;
    let mut target = base.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = store.lock()?;
    let mut current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
    let pending = current.prepare(&token, target, &sealed, &[], None)?;
    assert_eq!(current.phase()?, TransactionPhase::Prepared);
    assert_eq!(owner.anchor(&old)?.head.token()?, token);
    current.mark_applying(&pending)?;
    assert_eq!(current.phase()?, TransactionPhase::Applying);
    // Wrong external commit identity fails before publication and cannot clear
    // either the existing receipt or the authenticated base publication.
    assert!(
        current
            .commit(&pending, Sha256Digest::from_bytes([62; 32]))
            .is_err()
    );
    assert_eq!(fs::read(&receipt_path)?, receipt);
    assert_eq!(owner.anchor(&old)?.head.token()?, token);
    drop(current);
    {
        let applying_read = store.current_read(&scope)?;
        assert!(
            applying_read.settled_native_head()?.is_none(),
            "authenticated Applying base must not be relabelled settled"
        );
    }
    assert!(
        owner.enroll_native_metadata(&writer)?.is_none(),
        "fresh owning metadata enrollment cannot certify Applying as settled"
    );
    assert_eq!(fs::read(&receipt_path)?, receipt);
    drop(writer);
    // Actual interrupted Applying WAL, using the existing oversized-sidecar
    // recovery seam. Even a previously opened read cannot mint a new anchor.
    let wal = store.directory().join("current.sqlite3-wal");
    let bytes = fs::read(&wal)?;
    let file = fs::OpenOptions::new().write(true).open(&wal)?;
    file.set_len(128 << 20)?;
    drop(file);
    assert!(store.current_read(&scope).is_err());
    assert!(owner.anchor(&old).is_err());
    assert_eq!(fs::read(&receipt_path)?, receipt);
    // Restore this fixture's exact owned WAL bytes, not reconstructed source.
    fs::write(&wal, &bytes)?;
    drop(old);
    let writer = store.lock()?;
    let mut recovery = store.current_write(&writer, &quota, &scope, 8 << 20)?;
    assert_eq!(recovery.phase()?, TransactionPhase::Applying);
    recovery.mark_applying(&pending)?;
    recovery.commit(&pending, pending.target.lexical_commit)?;
    recovery.release_orphan_grants()?;
    drop(recovery);
    drop(writer);
    let recovered = store.current_read(&scope)?;
    assert_eq!(recovered.token()?, pending.target.token()?);
    assert_eq!(
        recovered.settled_native_head()?,
        Some(recovered.head().clone())
    );
    let state = owner.anchor(&recovered)?;
    assert_eq!(state.head.token()?, pending.target.token()?);
    assert_eq!(
        state.physical_audit,
        super::integrity::PhysicalAudit::Unknown
    );
    assert_eq!(
        state.basis,
        super::integrity::PublicationBasis::RetainedHeadMetadataAnchor
    );
    assert_eq!(state.known_damage.len(), 1);
    assert_eq!(state.known_damage[0].component, token.digest);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires AIRK_PRIVATE_CENSUS_BASE on positively protected local Linux ancestry"]
fn integrity_census_resumes_bounded_pages_and_requires_complete_live_owner() -> Result<()> {
    use super::integrity::census::{CensusOutcome, CensusUnknown};
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        std::env::var_os("AIRK_PRIVATE_CENSUS_BASE").is_some(),
        "explicit private census qualification base required"
    );
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    // Fixture-only security preparation. Production census never rewrites mode/ACL.
    fn restrict(path: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "unexpected fixture link"
        );
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
        )?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                restrict(&entry?.path())?;
            }
        }
        Ok(())
    }
    restrict(store.directory())?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let mut writer = store.lock()?;
    let mut census = owner.begin_census(&read, &mut writer)?;
    assert!(matches!(census.step(1)?, CensusOutcome::Pending { .. }));
    super::integrity::census::test_invalidate(&census)?;
    assert_eq!(
        census.step(1)?,
        CensusOutcome::Ineligible(CensusUnknown::NamespaceChanged)
    );
    drop(census);
    let mut census = owner.begin_census(&read, &mut writer)?;
    let mut ended = false;
    for _ in 0..100_100 {
        match census.step(1)? {
            CensusOutcome::Pending { .. } => (),
            CensusOutcome::Observed { .. } => {
                ended = true;
                break;
            }
            other => anyhow::bail!("private fixture did not establish census evidence: {other:?}"),
        }
    }
    assert!(ended, "bounded census did not finish actual enumeration");
    let proof = census.into_owner_private()?;
    proof.check()?;
    let observed = proof.record()?;
    assert!(matches!(
        observed.privacy,
        super::integrity::PrivacyEligibility::OwnerCensus { version: 1, .. }
    ));
    assert_eq!(
        observed.physical_audit,
        super::integrity::PhysicalAudit::Unknown
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn integrity_relative_census_open_refuses_symlink_traversal() -> Result<()> {
    // Root-relative openat is tested directly: no namespace or privacy proof is
    // inferred from success, and this fixture need not have private ancestry.
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir()?;
    let root = fs::File::open(temporary.path())?;
    fs::create_dir(temporary.path().join("real"))?;
    symlink("real", temporary.path().join("redirect"))?;
    assert!(
        super::integrity::census::test_open_child(&root, std::ffi::OsStr::new("redirect")).is_err()
    );
    assert!(super::integrity::census::test_open_child(&root, std::ffi::OsStr::new("real")).is_ok());
    Ok(())
}

#[test]
fn integrity_actual_current_writer_and_resource_producers_invalidate_same_owner_epoch() -> Result<()>
{
    let (_temporary, collection, _registry, _bound, _lease, store, scope) = integrity_fixture()?;
    let writer = store.lock()?;
    let initial = writer.census_mutation_epoch();
    assert!(
        store.lock().is_err(),
        "actual OS writer ownership must exclude another writer"
    );
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
    assert!(
        writer.census_mutation_epoch() > initial,
        "real current writer must invalidate before WAL/SQLite effects"
    );
    drop(current);
    let before_resource = writer.census_mutation_epoch();
    let reservation = quota.reserve(&scope.collection, 1 << 20)?;
    let reference =
        store.retain_current_resource_owned(&writer, b"owned resource fixture", &reservation)?;
    assert!(writer.census_mutation_epoch() > before_resource);
    assert_eq!(
        store.read_current_resource(&reference)?,
        b"owned resource fixture"
    );
    let other = FileStore::new(collection.slot(Incarnation::from_bytes([63; 16])));
    assert!(
        other.lock().is_err(),
        "different physical slots share the logical collection writer lock"
    );
    Ok(())
}

#[test]
fn integrity_damage_capacity_refuses_without_evicting_old_detections() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let read = store.current_read(&scope)?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    for n in 0u8..32 {
        owner.record_damage(&read, Sha256Digest::from_bytes([n; 32]), 400 + u64::from(n))?;
    }
    let path = collection.directory.join("integrity-observation.json");
    let before = fs::read(&path)?;
    assert!(
        owner
            .record_damage(&read, Sha256Digest::from_bytes([99; 32]), 500)
            .is_err()
    );
    assert_eq!(fs::read(&path)?, before);
    assert_eq!(owner.anchor(&read)?.known_damage.len(), 32);
    // Duplicate detection at capacity still preserves the first observation.
    assert_eq!(
        owner
            .record_damage(&read, Sha256Digest::from_bytes([1; 32]), 999)?
            .known_damage[1]
            .observed_unix_ms,
        401
    );
    Ok(())
}

#[test]
fn integrity_sidecar_does_not_strand_owned_purge_or_certify_full_reset() -> Result<()> {
    let (_temporary, collection, registry, bound, lease, store, scope) = integrity_fixture()?;
    let registration = registry.get(lease.identity().0)?.registration;
    fs::write(
        Path::new(&registration.root).join("source-must-stay.txt"),
        b"original source",
    )?;
    let read = store.current_read(&scope)?;
    collection
        .integrity_owner(&registry, &lease, &bound)?
        .record_damage(&read, read.token()?.digest, 800)?;
    drop(read);
    // Exercise the actual owned deletion adapter. Request/registration transition
    // is covered by existing unregistration tests, not synthesized here.
    let mut complete = false;
    for _ in 0..8 {
        if Collection::purge_removed_turn(&registration, &lease)? {
            complete = true;
            break;
        }
    }
    assert!(complete, "new metadata name stranded owning removal");
    assert!(!collection.directory.exists());
    assert_eq!(
        fs::read(Path::new(&registration.root).join("source-must-stay.txt"))?,
        b"original source"
    );
    let reset_slot = Incarnation::from_bytes([57; 16]);
    let reset = Collection::initialize(
        Path::new(&registration.data_root),
        &registration.root,
        "fixture",
        reset_slot,
    )?;
    let reset_store = reset.selected_store()?;
    initialize(&reset_store, &registration.root, reset_slot)?;
    let reset_scope = reset_store
        .current_owned_scope()?
        .context("reset scope missing")?;
    let read = reset_store.current_read(&reset_scope)?;
    let state = reset
        .integrity_owner(&registry, &lease, &bound)?
        .anchor(&read)?;
    assert!(
        state.known_damage.is_empty(),
        "only explicit full derived-state destruction removes old receipt"
    );
    assert_eq!(
        state.physical_audit,
        super::integrity::PhysicalAudit::Unknown
    );
    assert_eq!(
        state.basis,
        super::integrity::PublicationBasis::RetainedHeadMetadataAnchor
    );
    assert_eq!(state.head.scope.incarnation, reset_slot);
    Ok(())
}

#[test]
fn native_metadata_enrollment_requires_exact_writer_and_never_promotes_unknown_audit() -> Result<()>
{
    let (temporary, collection, registry, bound, lease, store, _scope) = integrity_fixture()?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let writer = store.lock()?;
    let state = owner
        .enroll_native_metadata(&writer)?
        .context("settled native head missing")?;
    assert_eq!(
        state.physical_audit,
        super::integrity::PhysicalAudit::Unknown
    );
    assert_eq!(
        state.privacy,
        super::integrity::PrivacyEligibility::PlatformEvidenceUnavailable
    );
    assert_eq!(
        state.basis,
        super::integrity::PublicationBasis::RetainedHeadMetadataAnchor
    );
    let before = fs::read(collection.directory.join("integrity-observation.json"))?;
    let foreign = FileStore::new(temporary.path().join("foreign-writer"));
    let foreign_writer = foreign.lock()?;
    assert!(owner.enroll_native_metadata(&foreign_writer).is_err());
    assert_eq!(
        fs::read(collection.directory.join("integrity-observation.json"))?,
        before
    );
    Ok(())
}

#[test]
fn verify_continuation_survives_worker_restart_without_refreshing_audit_age() -> Result<()> {
    use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    use airk_core::service::{JobKind, JobOutcome, ProjectRegistration, RequestKey};
    fn claim(
        registry: &mut ServiceRegistry,
        lease: &ProjectLease,
        nonce: u8,
    ) -> Result<Sha256Digest> {
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([nonce; 16]),
        };
        let job = registry.enqueue(lease.identity().0, namespace, &key, JobKind::Verify)?;
        Ok(registry
            .claim_job_id(lease, job.id)?
            .context("Verify not claimed")?
            .id)
    }
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let (data, source, control) = (base.join("data"), base.join("source"), base.join("control"));
    for path in [&data, &source, &control] {
        fs::create_dir(path)?;
    }
    let namespace = Incarnation::from_bytes([81; 16]);
    let slot = Incarnation::from_bytes([82; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(&data, &registration.root, "fixture", slot)?;
    let store = collection.selected_store()?;
    initialize(&store, &registration.root, slot)?;
    let scope = store
        .current_owned_scope()?
        .context("fixture scope missing")?;
    let read = store.current_read(&scope)?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    let job = claim(&mut registry, &lease, 83)?;
    let before = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        owner.anchor(&read)?;
        drop(read);
        let mut writer = store.lock()?;
        assert!(
            owner
                .verify_turn(
                    &mut writer,
                    Sha256Digest::from_bytes([99; 32]),
                    1,
                    1 << 20,
                    99,
                    || Ok(())
                )
                .is_err(),
            "physical work needs an exact durable job admission"
        );
        let first = owner.verify_turn(&mut writer, job, 1, 1 << 20, 100, || Ok(()))?;
        assert!(first.progress.complete());
        assert!(!first.blocked_large);
        serde_json::to_value(&first.progress)?
    };
    drop(lease);
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    registry.recover_jobs(&lease)?;
    assert_eq!(
        registry
            .claim_job_id(&lease, job)?
            .context("recovered Verify not claimed")?
            .id,
        job
    );
    let resumed_complete = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let resumed = owner.verify_turn(&mut writer, job, 1, 1 << 20, 200, || Ok(()))?;
        assert!(resumed.progress.complete());
        let after = serde_json::to_value(&resumed.progress)?;
        assert_eq!(
            before["lexical_observed_unix_ms"],
            after["lexical_observed_unix_ms"]
        );
        assert_eq!(before["through_unix_ms"], after["through_unix_ms"]);
        assert_eq!(after["lexical_observed_unix_ms"], 100);
        let read = store.current_read(&scope)?;
        let anchor = owner.anchor(&read)?;
        assert_eq!(
            anchor.physical_audit,
            super::integrity::PhysicalAudit::Unknown
        );
        assert!(
            anchor
                .verify
                .context("saved Verify progress missing")?
                .complete()
        );
        resumed
            .completed
            .context("resumed physical completion proof missing")?
    };
    assert!(
        registry
            .complete_job(&lease, job, JobOutcome::Succeeded { revision: None })
            .is_err(),
        "generic success cannot settle audit"
    );
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job,
        JobOutcome::Succeeded {
            revision: Some(resumed_complete.revision()?),
        },
        201,
        None,
        Some(crate::service::registry::AuditEvidence::Complete(
            &resumed_complete,
        )),
    )?;
    assert!(registry.integrity_audit_veto(&lease)?.is_none());
    let fresh_job = claim(&mut registry, &lease, 84)?;
    let fresh_complete = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let fresh = owner.verify_turn(&mut writer, fresh_job, 1, 1 << 20, 300, || Ok(()))?;
        assert_eq!(
            serde_json::to_value(&fresh.progress)?["lexical_observed_unix_ms"],
            300,
            "new explicit Verify job must not silently reuse prior completed audit"
        );
        fresh
            .completed
            .context("fresh physical completion proof missing")?
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        fresh_job,
        JobOutcome::Succeeded {
            revision: Some(fresh_complete.revision()?),
        },
        301,
        None,
        Some(crate::service::registry::AuditEvidence::Complete(
            &fresh_complete,
        )),
    )?;
    let canceled_job = claim(&mut registry, &lease, 85)?;
    let drained = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let checkpoints = std::cell::Cell::new(0);
        let canceled = owner
            .verify_turn(&mut writer, canceled_job, 1, 1 << 20, 400, || {
                checkpoints.set(checkpoints.get() + 1);
                anyhow::ensure!(
                    checkpoints.get() != 3,
                    "injected job cancellation after lexical component"
                );
                Ok(())
            })
            .err()
            .context("audit should have canceled")?;
        crate::service::registry::AuditDrainProof::from_error(&canceled)
            .context("drained cancellation proof missing")?
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        canceled_job,
        JobOutcome::Canceled,
        401,
        None,
        Some(crate::service::registry::AuditEvidence::Drained(&drained)),
    )?;
    assert!(
        registry.integrity_audit_veto(&lease)?.is_none(),
        "safely drained partial audit must not poison normal restart"
    );
    drop(lease);
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    assert_eq!(
        registry.integrity_audit_state(&lease)?,
        crate::service::registry::IntegrityAuditState::Clear
    );
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let read = store.current_read(&scope)?;
    let anchor = owner.anchor(&read)?;
    assert_eq!(
        anchor.physical_audit,
        super::integrity::PhysicalAudit::Unknown
    );
    let partial = serde_json::to_value(anchor.verify.context("partial audit missing")?)?;
    assert_eq!(partial["lexical_observed_unix_ms"], 400);
    assert_eq!(
        partial["objects_complete"], false,
        "cancel did not certify unvisited retained objects"
    );
    let incomplete_job = claim(&mut registry, &lease, 86)?;
    let clean_turn = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        owner.verify_turn(&mut writer, incomplete_job, 1, 1 << 20, 500, || Ok(()))?
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        incomplete_job,
        JobOutcome::Failed {
            code: "integrity_coverage_incomplete".into(),
        },
        501,
        None,
        clean_turn
            .drained
            .as_ref()
            .map(crate::service::registry::AuditEvidence::Drained),
    )?;
    assert!(
        registry.integrity_audit_veto(&lease)?.is_none(),
        "cleanly settled incomplete coverage is not known damage"
    );
    let failed_job = claim(&mut registry, &lease, 87)?;
    registry.complete_job(
        &lease,
        failed_job,
        JobOutcome::Failed {
            code: "unresolved_audit".into(),
        },
    )?;
    let replacement_job = claim(&mut registry, &lease, 88)?;
    let replacement_turn = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        owner.verify_turn(&mut writer, replacement_job, 1, 1 << 20, 600, || Ok(()))?
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        replacement_job,
        JobOutcome::Failed {
            code: "integrity_coverage_incomplete".into(),
        },
        601,
        None,
        replacement_turn
            .drained
            .as_ref()
            .map(crate::service::registry::AuditEvidence::Drained),
    )?;
    assert_eq!(
        registry.integrity_audit_veto(&lease)?,
        Some(replacement_job),
        "partial drain cannot heal an inherited unresolved audit"
    );
    Ok(())
}

#[test]
fn approved_physical_repair_claim_and_success_require_actual_guard_and_component_proof()
-> Result<()> {
    use crate::service::registry::{AuditEvidence, IntegrityAuditState};
    use airk_core::{
        service::{JobKind, JobOutcome, JobState, RequestKey, WorkerIdentity},
        service_inspection::{IndexState, InspectionSnapshot, SourceState},
        service_maintenance::{RepairOperation, RepairRequest, RepairSelection},
    };
    let (_temp, collection, mut registry, bound, lease, store, scope) = integrity_fixture()?;
    let project = lease.identity().0;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let engine = Sha256Digest::from_bytes([91; 32]);
    let plan = registry.create_maintenance_plan(
        namespace,
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([92; 16]),
        },
        &RepairRequest {
            selection: RepairSelection::Selected(vec![project]),
            operation: RepairOperation::Verify,
        },
        1000,
        engine,
    )?;
    registry.advance_maintenance_plans(1100)?;
    let inspect = registry
        .claim_job(&lease)?
        .context("plan inspection missing")?;
    assert_eq!(inspect.kind, JobKind::PlanInspect);
    let token = store.current_read(&scope)?.token()?;
    let revision = airk_core::identity::RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    };
    let snapshot = InspectionSnapshot {
        quota_policy_digest: None,
        project,
        incarnation: scope.incarnation,
        publication: Some(revision),
        settings_digest: Some(engine),
        index_state: IndexState::Ready,
        source_state: SourceState::Available,
        recovery_required: Some(false),
        indexed_files: Some(0),
        indexed_vectors: Some(0),
        vectors_enabled: Some(false),
        index_detail: None,
        semantic: None,
        discovery: None,
        failure: None,
        storage: None,
    };
    registry.record_inspection(
        &lease,
        WorkerIdentity {
            service: namespace,
            worker: Incarnation::from_bytes([93; 16]),
            project,
            fencing_generation: inspect.fencing_generation,
        },
        inspect.id,
        1200,
        &snapshot,
    )?;
    registry.complete_job_at(
        &lease,
        inspect.id,
        JobOutcome::Succeeded {
            revision: Some(revision),
        },
        1300,
    )?;
    let ready = registry.maintenance_plan(plan.id)?;
    registry.approve_maintenance_plan(
        plan.id,
        ready.preview_digest.context("preview missing")?,
        false,
        1400,
    )?;
    registry.advance_maintenance_plans(1500)?;
    let queued = registry
        .pending_job_observation(project)?
        .context("physical Repair missing")?;
    assert_eq!(queued.kind, JobKind::Repair);
    registry.refuse_physical_repair_guard_for_test(true)?;
    assert!(registry.claim_job_id(&lease, queued.id).is_err());
    assert_eq!(registry.job(queued.id)?.state, JobState::Pending);
    registry.refuse_physical_repair_guard_for_test(false)?;
    let claimed = registry
        .claim_job_id(&lease, queued.id)?
        .context("physical Repair not claimed")?;
    assert_eq!(
        registry.integrity_audit_state(&lease)?,
        IntegrityAuditState::LiveAudit { job: claimed.id }
    );
    assert!(
        registry
            .complete_job(
                &lease,
                claimed.id,
                JobOutcome::Succeeded {
                    revision: Some(revision)
                }
            )
            .is_err()
    );
    let complete = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let read = store.current_read(&scope)?;
        owner.anchor(&read)?;
        drop(read);
        let mut writer = store.lock()?;
        let turn = owner.verify_turn(&mut writer, claimed.id, 1, 1 << 20, 1600, || Ok(()))?;
        assert!(turn.progress.complete());
        turn.completed
            .context("physical completion proof missing")?
    };
    let wrong = airk_core::identity::RevisionToken {
        revision: engine,
        ..complete.revision()?
    };
    assert!(
        registry
            .complete_job_with_maintenance_and_audit_at(
                &lease,
                claimed.id,
                JobOutcome::Succeeded {
                    revision: Some(wrong)
                },
                1700,
                None,
                Some(AuditEvidence::Complete(&complete))
            )
            .is_err()
    );
    assert_eq!(registry.job(claimed.id)?.state, JobState::Running);
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        claimed.id,
        JobOutcome::Succeeded {
            revision: Some(complete.revision()?),
        },
        1800,
        None,
        Some(AuditEvidence::Complete(&complete)),
    )?;
    assert_eq!(
        registry.integrity_audit_state(&lease)?,
        IntegrityAuditState::Clear
    );
    assert_eq!(registry.maintenance_plan(plan.id)?.counts.succeeded, 1);
    Ok(())
}

fn audit_retained_input(
    store: &FileStore,
    writer: &crate::WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    path: &str,
    text: &str,
) -> Result<crate::current::CapturedInput> {
    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use sha2::{Digest, Sha256};
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "audit-fixture/1".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: Default::default(),
        configuration_inputs: Default::default(),
    };
    let payload = ExtractionPayload {
        semantic_projection: Default::default(),
        continuity_coverage: Default::default(),
        contract_version: 1,
        extraction_method: "utf8".into(),
        parse_status: "plain".into(),
        provenance: vec![],
        text: text.into(),
        syntax_identities: vec![],
        facts: vec![],
        chunks: vec![LocalChunk {
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            region: None,
        }],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    };
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path,
            source_hash: Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into()),
            source_bytes: text.len(),
        },
    )?;
    let reservation = quota.reserve(&scope.collection, 1 << 20)?;
    let artifact = store.retain_payload_with_budget_owned(
        writer,
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(&reservation),
    )?;
    Ok(crate::current::CapturedInput::new(artifact))
}
#[test]
fn real_retained_pages_release_writer_admit_query_and_reclaim_same_job_with_original_dates()
-> Result<()> {
    use crate::service::{
        ProjectLease, data_scope,
        registry::{AuditEvidence, ServiceRegistry},
    };
    use airk_core::service::{JobKind, JobOutcome, RequestKey};
    let (temporary, collection, mut registry, bound, lease, store, scope) = integrity_fixture()?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    {
        let writer = store.lock()?;
        for (path, text) in [
            ("a.txt", "first retained body"),
            ("b.txt", "second retained body"),
        ] {
            let input = audit_retained_input(&store, &writer, &quota, &scope, path, text)?;
            let mut current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
            let token = current.head()?.token()?;
            current.commit_artifact(&token, path, Some(&input))?;
        }
    }
    assert_eq!(
        store
            .current_read(&scope)?
            .lookup_paths(crate::current::postings::Lookup::Files, "", None, 3)?
            .paths
            .len(),
        2
    );
    let project = lease.identity().0;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let job = registry.enqueue(
        project,
        namespace,
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([97; 16]),
        },
        JobKind::Verify,
    )?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("Verify not claimed")?;
    let before = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let read = store.current_read(&scope)?;
        owner.anchor(&read)?;
        drop(read);
        let mut writer = store.lock()?;
        let (go, queued) = std::sync::mpsc::sync_channel(1);
        let (done, admitted) = std::sync::mpsc::sync_channel(1);
        let query_store = store.clone();
        let query_scope = scope.clone();
        let query = std::thread::spawn(move || -> Result<()> {
            queued.recv_timeout(Duration::from_secs(10))?;
            let read = query_store.current_read(&query_scope)?;
            assert!(read.artifact("b.txt")?.is_some());
            done.send(read.token()?)?;
            Ok(())
        });
        let calls = std::cell::Cell::new(0);
        let first_result = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 100, || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                // The audit already owns its actual CurrentRead and writer.
                go.send(())?;
                admitted.recv_timeout(Duration::from_secs(10))?;
            }
            Ok(())
        });
        drop(go);
        let query_result = query
            .join()
            .map_err(|_| anyhow::anyhow!("actual query fixture panicked"));
        let first = first_result?;
        query_result??;
        assert!(!first.progress.complete());
        assert_eq!(first.verified_files, 1);
        assert!(first.completed.is_none());
        let progress = serde_json::to_value(&first.progress)?;
        assert_eq!(progress["checked_files"], 1);
        assert_eq!(progress["after"], "a.txt");
        drop(writer);
        let next_store = store.clone();
        std::thread::spawn(move || -> Result<()> {
            let _writer = next_store.lock()?;
            Ok(())
        })
        .join()
        .map_err(|_| anyhow::anyhow!("writer release fixture panicked"))??;
        progress
    };
    drop(lease);
    drop(registry);
    let control = temporary.path().canonicalize()?.join("control");
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let mut lease = ProjectLease::acquire(&control, project)?;
    registry.admit_worker(&mut lease)?;
    registry.recover_jobs(&lease)?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("same durable job not reclaimed")?;
    let bound = data_scope::bind(Path::new(&collection.owner.data_root), &control, namespace)?;
    let complete = {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let last = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 200, || Ok(()))?;
        let after = serde_json::to_value(&last.progress)?;
        assert!(last.progress.complete());
        assert_eq!(last.verified_files, 1);
        assert_eq!(after["checked_files"], 2);
        assert_eq!(after["started_unix_ms"], before["started_unix_ms"]);
        assert_eq!(
            after["lexical_observed_unix_ms"],
            before["lexical_observed_unix_ms"]
        );
        assert!(after["checked_bytes"].as_u64() > before["checked_bytes"].as_u64());
        last.completed.context("complete physical proof missing")?
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job.id,
        JobOutcome::Succeeded {
            revision: Some(complete.revision()?),
        },
        201,
        None,
        Some(AuditEvidence::Complete(&complete)),
    )?;
    assert!(registry.integrity_audit_veto(&lease)?.is_none());
    Ok(())
}

fn verify_inputs_fixture(
    inputs: &[(&str, &str)],
) -> Result<(IntegrityFixture, airk_core::service::JobRecord)> {
    use airk_core::service::{JobKind, RequestKey};
    let (temporary, collection, mut registry, bound, lease, store, scope) = integrity_fixture()?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    {
        let writer = store.lock()?;
        for (path, text) in inputs {
            let input = audit_retained_input(&store, &writer, &quota, &scope, path, text)?;
            let mut current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
            let token = current.head()?.token()?;
            current.commit_artifact(&token, path, Some(&input))?;
        }
    }
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let job = registry.enqueue(
        lease.identity().0,
        namespace,
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([109; 16]),
        },
        JobKind::Verify,
    )?;
    let claimed = registry
        .claim_job_id(&lease, job.id)?
        .context("Verify was not claimed")?;
    let read = store.current_read(&scope)?;
    collection
        .integrity_owner(&registry, &lease, &bound)?
        .anchor(&read)?;
    drop(read);
    Ok((
        (temporary, collection, registry, bound, lease, store, scope),
        claimed,
    ))
}

#[test]
fn verify_physical_retained_corruption_preserves_prior_cursor_and_revokes_clean_drain() -> Result<()>
{
    for kind in ["sources", "artifacts"] {
        let ((_temporary, collection, registry, bound, lease, store, scope), job) =
            verify_inputs_fixture(&[
                ("a.txt", "first retained input"),
                ("b.txt", "second retained input"),
            ])?;
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let first = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 100, || Ok(()))?;
        assert_eq!(first.verified_files, 1);
        assert!(first.completed.is_none());
        let prior = serde_json::to_value(&first.progress)?;
        let artifact = store
            .current_read(&scope)?
            .artifact("b.txt")?
            .context("second artifact missing")?;
        let reference = artifact
            .retained
            .as_ref()
            .context("retained fixture missing")?;
        let digest = if kind == "sources" {
            reference
                .source_encoding
                .as_ref()
                .map_or(artifact.content_hash.as_str(), |value| {
                    value.stored_digest.as_str()
                })
        } else {
            reference
                .payload_encoding
                .as_ref()
                .map_or(reference.object.as_str(), |value| {
                    value.stored_digest.as_str()
                })
        };
        let object = store.directory().join("objects").join(kind).join(digest);
        let mut bytes = fs::read(&object)?;
        ensure!(!bytes.is_empty(), "retained fixture object is empty");
        bytes[0] ^= 0x80;
        fs::write(&object, bytes)?;
        let error = owner
            .verify_turn(&mut writer, job.id, 1, 1 << 20, 200, || Ok(()))
            .err()
            .context("physically changed retained object was accepted")?;
        assert!(
            error
                .downcast_ref::<crate::service::registry::AuditDrainProof>()
                .is_none()
        );
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(
            collection.directory.join("integrity-observation.json"),
        )?)?;
        assert_eq!(saved["verify"]["after"], prior["after"]);
        assert_eq!(saved["verify"]["checked_files"], prior["checked_files"]);
        assert_eq!(saved["verify"]["checked_bytes"], prior["checked_bytes"]);
        assert_eq!(saved["verify"]["unresolved"], true);
        assert_eq!(saved["verify"]["objects_complete"], false);
        assert!(bound.live_integrity().revoked());
        assert_eq!(registry.integrity_audit_veto(&lease)?, Some(job.id));
        assert!(
            crate::service::registry::AuditDrainProof::new(&registry, &lease, &bound, job.id)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn verify_physical_fts_corruption_refuses_lexical_completion_and_persists_uncertainty() -> Result<()>
{
    let ((_temporary, collection, registry, bound, lease, store, scope), job) =
        verify_inputs_fixture(&[("a.txt", "physical lexical retained input")])?;
    {
        let connection = rusqlite::Connection::open(store.directory().join("current.sqlite3"))?;
        let (id, mut bytes): (i64, Vec<u8>) = connection.query_row(
            "SELECT id,block FROM current_fts_data WHERE length(block)>0 ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        bytes[0] ^= 0x80;
        assert_eq!(
            connection.execute(
                "UPDATE current_fts_data SET block=?1 WHERE id=?2",
                rusqlite::params![bytes, id]
            )?,
            1
        );
    }
    let direct = store
        .current_read(&scope)?
        .verify_lexical_integrity()
        .err()
        .context("FTS corruption fixture remained valid")?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let mut writer = store.lock()?;
    let error = owner
        .verify_turn(&mut writer, job.id, 1, 1 << 20, 100, || Ok(()))
        .err()
        .context("Verify accepted physical FTS corruption")?;
    assert_eq!(format!("{error:#}"), format!("{direct:#}"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(
        collection.directory.join("integrity-observation.json"),
    )?)?;
    assert_eq!(saved["verify"]["checked_files"], 0);
    assert!(saved["verify"]["after"].is_null());
    assert!(saved["verify"]["lexical_observed_unix_ms"].is_null());
    assert_eq!(saved["verify"]["unresolved"], true);
    assert!(bound.live_integrity().revoked());
    assert_eq!(registry.integrity_audit_veto(&lease)?, Some(job.id));
    assert!(
        crate::service::registry::AuditDrainProof::new(&registry, &lease, &bound, job.id).is_err()
    );
    Ok(())
}

#[test]
fn verify_progress_quota_failure_preserves_prior_receipt_and_revokes_completion() -> Result<()> {
    let later = format!("{}.txt", "z".repeat(200));
    let text = "larger physical retained quota input\n".repeat(64);
    let ((_temporary, collection, registry, bound, lease, store, _scope), job) =
        verify_inputs_fixture(&[("a.txt", "first input"), (&later, &text)])?;
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let mut writer = store.lock()?;
    let first = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 100, || Ok(()))?;
    assert_eq!(first.verified_files, 1);
    assert!(first.completed.is_none());
    let receipt = collection.directory.join("integrity-observation.json");
    let before = fs::read(&receipt)?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let original = quota.policy()?;
    let status = quota.status()?;
    let mut full = original.clone();
    full.root_bytes = status
        .used_bytes
        .checked_add(status.reserved_bytes)
        .context("quota overflow")?;
    quota.configure(full)?;
    let result = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 200, || Ok(()));
    quota.configure(original)?;
    let error = result
        .err()
        .context("Verify progress write unexpectedly fit exhausted quota")?;
    assert!(error.is::<crate::quota::QuotaPressure>(), "{error:#}");
    assert_eq!(fs::read(&receipt)?, before);
    assert!(bound.live_integrity().revoked());
    assert_eq!(registry.integrity_audit_veto(&lease)?, Some(job.id));
    assert!(
        crate::service::registry::AuditDrainProof::new(&registry, &lease, &bound, job.id).is_err()
    );
    Ok(())
}

#[test]
fn verify_oversized_object_does_not_advance_and_larger_allowance_completes_same_job() -> Result<()>
{
    use crate::service::registry::AuditEvidence;
    use airk_core::service::JobOutcome;
    let text = "bounded retained input ".repeat(64);
    let ((_temporary, collection, mut registry, bound, lease, store, scope), job) =
        verify_inputs_fixture(&[("big.txt", &text)])?;
    let artifact = store
        .current_read(&scope)?
        .artifact("big.txt")?
        .context("large artifact missing")?;
    let bytes = artifact.bytes as u64
        + artifact
            .retained
            .as_ref()
            .context("large retained fixture missing")?
            .object_bytes as u64;
    ensure!(bytes > 1, "oversize fixture budget is empty");
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let mut writer = store.lock()?;
    for now in [100, 200] {
        let turn = owner.verify_turn(&mut writer, job.id, 1, bytes - 1, now, || Ok(()))?;
        assert!(turn.blocked_large);
        assert_eq!(turn.verified_files, 0);
        assert!(!turn.progress.complete());
        assert!(turn.completed.is_none());
        let progress = serde_json::to_value(&turn.progress)?;
        assert_eq!(progress["checked_files"], 0);
        assert_eq!(progress["checked_bytes"], 0);
        assert!(progress["after"].is_null());
    }
    let complete = owner.verify_turn(&mut writer, job.id, 1, bytes, 300, || Ok(()))?;
    assert!(!complete.blocked_large);
    assert_eq!(complete.verified_files, 1);
    assert!(complete.progress.complete());
    let proof = complete
        .completed
        .context("larger allowance did not produce physical completion")?;
    drop(writer);
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job.id,
        JobOutcome::Succeeded {
            revision: Some(proof.revision()?),
        },
        301,
        None,
        Some(AuditEvidence::Complete(&proof)),
    )?;
    assert!(registry.integrity_audit_veto(&lease)?.is_none());
    assert!(!bound.live_integrity().revoked());
    Ok(())
}
