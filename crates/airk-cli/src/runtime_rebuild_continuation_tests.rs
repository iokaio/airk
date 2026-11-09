// SPDX-License-Identifier: Apache-2.0
use super::*;

fn completed_continuation_fixture(
    complete_second: bool,
) -> Result<(
    tempfile::TempDir,
    PathBuf,
    Journal,
    ServiceLease,
    Incarnation,
)> {
    let (owner, root, mut journal) = fixture(true)?;
    let mut second = journal.roster.indexes[0].clone();
    second.registration = ProjectRegistration::new(
        second.registration.root.clone(),
        root.parent()
            .unwrap()
            .join("original-data-second")
            .to_str()
            .unwrap()
            .into(),
        "fixture-second".into(),
    )?;
    fs::create_dir(&second.registration.data_root)?;
    airk_store::service::data_scope::bind(
        Path::new(&second.registration.data_root),
        &journal.old_control,
        journal.roster.namespace,
    )?;
    let mut original = ServiceRegistry::open(&journal.old_control, journal.roster.namespace)?;
    original.register(&second.registration)?;
    original.pause(second.registration.binding, true)?;
    journal.roster.revision = original.list(None, None, 32)?.revision;
    drop(original);
    journal.roster.indexes.push(second);
    journal
        .roster
        .indexes
        .sort_by_key(|index| index.registration.binding);
    journal.indexes.push(journal.indexes[0].clone());
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.expected_indexes = Some(2);
    journal.save(&root)?;
    let (owner, root, mut journal) = finish_successor_fixture(owner, root, journal)?;

    // Qualify saved selection metadata without executing an artifact. The real
    // Genesis/setup/job/owner path below is the continuation's production path.
    let staged = root.join("candidate-fixture");
    let candidate = crate::runtime_install::tests::fixture(&staged)?;
    replace_json(
        &staged,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: candidate.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    fs::rename(staged, root.join("runtimes").join(&candidate))?;
    journal.candidate = candidate;
    replace_json(&root, "selected.json", &journal.selected())?;

    let control = journal.control();
    let lease = ServiceLease::acquire(&control)?;
    assert_eq!(lease.root(), control);
    let namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    for position in 0..2 {
        prepare_index(&root, &mut journal, position, &mut registry)?;
    }
    let completed_count = if complete_second { 2 } else { 1 };
    for position in 0..completed_count {
        let binding = journal.indexes[position]
            .request
            .as_ref()
            .unwrap()
            .registration
            .binding;
        let id = journal.indexes[position].job.unwrap();
        let mut worker = ProjectLease::acquire(&control, binding)?;
        registry.admit_worker(&mut worker)?;
        assert_eq!(registry.claim_job(&worker)?.unwrap().id, id);
        registry.complete_job(
            &worker,
            id,
            airk_core::service::JobOutcome::Succeeded { revision: None },
        )?;
        drop(worker);
    }
    drop(registry);
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    Ok((owner, root, journal, lease, namespace))
}

#[test]
fn clean_break_continuation_restores_completed_preferences_under_native_owner() -> Result<()> {
    let (_owner, root, journal, lease, namespace) = completed_continuation_fixture(false)?;
    let control = journal.control();
    let binding = journal.indexes[0]
        .request
        .as_ref()
        .unwrap()
        .registration
        .binding;
    let id = journal.indexes[0].job.unwrap();
    let now = std::time::Instant::now();
    let mut continuation = crate::runtime_install::clean_break_background::Continuation::new(
        Some(
            root.join("runtimes")
                .join(&journal.candidate)
                .join(main_name()),
        ),
        now,
    );
    assert!(
        continuation
            .tick(&lease, Incarnation::from_bytes([99; 16]), now)
            .is_err()
    );
    assert!(read(&root)?.unwrap().indexes[0].completed.is_none());
    assert!(continuation.tick(&lease, namespace, now + Duration::from_secs(1))?);
    let restored = read(&root)?.unwrap();
    assert!(restored.indexes[0].preferences_restored);
    assert_eq!(restored.indexes[0].completed.as_ref().unwrap().id, id);
    assert!(ServiceRegistry::open_saved(&control)?.get(binding)?.paused);
    assert!(!restored.indexes[1].preferences_restored);
    assert!(restored.indexes[1].completed.is_none());
    assert!(
        !restored.cleanup_complete,
        "pending second index forbids cleanup"
    );
    drop(lease);
    Ok(())
}

#[test]
fn clean_break_preference_failure_does_not_starve_completed_sibling() -> Result<()> {
    let (_owner, root, journal, lease, namespace) = completed_continuation_fixture(true)?;
    let control = journal.control();
    let first = journal.indexes[0]
        .request
        .as_ref()
        .unwrap()
        .registration
        .binding;
    let second = journal.indexes[1]
        .request
        .as_ref()
        .unwrap()
        .registration
        .binding;
    // Inject only the first preference effect. Native Genesis, job receipts,
    // owner/namespace admission and sibling writes use their canonical owners.
    let error = continue_clean_break_observing(
        &root,
        &lease,
        namespace,
        |registry, binding, paused, watch| {
            if binding == first {
                anyhow::bail!("fixture first preference blocked");
            }
            registry.pause(binding, paused)?;
            registry.configure_watch(binding, watch)
        },
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("fixture first preference blocked"),
        "{error:#}"
    );
    let observed = read(&root)?.unwrap();
    assert!(observed.indexes[0].completed.is_some());
    assert!(!observed.indexes[0].preferences_restored);
    assert_eq!(
        observed.indexes[0].failure.as_ref().unwrap().code,
        "clean_break_preference_restore_blocked"
    );
    assert!(
        observed.indexes[1].completed.is_some(),
        "healthy sibling completion must be retained"
    );
    assert!(
        observed.indexes[1].preferences_restored,
        "healthy sibling preferences must be restored"
    );
    let registry = ServiceRegistry::open_saved(&control)?;
    assert!(!registry.get(first)?.paused);
    assert!(registry.get(second)?.paused);
    assert!(
        !observed.cleanup_complete,
        "first blocked index forbids cleanup"
    );
    Ok(())
}

#[test]
fn clean_break_preference_failure_retains_job_and_only_clears_its_own_error() -> Result<()> {
    let (_owner, _root, mut journal) = fixture(true)?;
    let progress = &mut journal.indexes[0];
    let id = Sha256Digest::from_bytes([92; 32]);
    progress.job = Some(id);
    progress.completed = Some(JobRecord {
        id,
        project: journal.roster.indexes[0].registration.binding,
        kind: JobKind::Index,
        state: JobState::Succeeded,
        fencing_generation: 1,
        attempts: 1,
        outcome: None,
    });
    let completed = serde_json::to_value(&progress.completed)?;
    let original = "preference write failed: ".to_owned() + &"x".repeat(8192);
    let error =
        record_preferences_result(progress, Err(anyhow::anyhow!(original.clone()))).unwrap_err();
    assert_eq!(error.to_string(), original);
    let failure = progress.failure.as_ref().unwrap();
    assert_eq!(failure.code, "clean_break_preference_restore_blocked");
    assert_eq!(failure.message, bounded_message(&original));
    assert!(!progress.preferences_restored);
    assert_eq!(serde_json::to_value(&progress.completed)?, completed);
    record_preferences_result(progress, Ok(()))?;
    assert!(progress.failure.is_none());
    assert!(progress.preferences_restored);
    assert_eq!(serde_json::to_value(&progress.completed)?, completed);

    progress.failure = Some(RebuildFailure {
        code: "independent_failure".into(),
        message: "Retain unrelated native failure".into(),
    });
    record_preferences_result(progress, Ok(()))?;
    assert_eq!(
        progress.failure.as_ref().unwrap().code,
        "independent_failure"
    );
    progress.job = Some(Sha256Digest::from_bytes([93; 32]));
    assert!(record_preferences_result(progress, Ok(())).is_err());
    assert_eq!(
        progress.failure.as_ref().unwrap().code,
        "independent_failure"
    );
    Ok(())
}
