// SPDX-License-Identifier: Apache-2.0
//! Installer phase tests use the real Store Genesis/setup/job owners. The test
//! producer substitutes only artifact qualification; installed native acceptance
//! must still execute the candidate's nonce-bound probe.
use super::*;
use airk_store::service::registry::catalog::genesis::{
    CatalogGenesis,
    activation::{
        AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
        AuthorityV1ReadinessProducer,
    },
};

struct ComponentProducer;
impl AuthorityV1ReadinessProducer for ComponentProducer {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        Ok(Vec::new())
    }
    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification> {
        Ok(AuthorityV1NativeQualification {
            candidate: expected.clone(),
            predecessor_refusals: Vec::new(),
        })
    }
}

fn fixture(paused: bool) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("install");
    local_ipc::prepare_root(&root)?;
    let original_state = base.join("original-state");
    layout::initialize(&root, Some(&original_state), RuntimeChannel::Stable)?;
    let old_control = layout::control(&root)?;
    local_ipc::prepare_root(&old_control)?;
    let service = ServiceLease::acquire(&old_control)?;
    layout::record_control(&service)?;
    let namespace = Incarnation::from_bytes([11; 16]);
    drop(ServiceRegistry::open(&old_control, namespace)?);
    layout::record_registry(&service, namespace)?;
    drop(service);
    let source = base.join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("original.txt"), b"Original rebuild fixture.\n")?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        base.join("original-data").to_str().unwrap().into(),
        "fixture".into(),
    )?;
    fs::create_dir(&registration.data_root)?;
    airk_store::service::data_scope::bind(
        Path::new(&registration.data_root),
        &old_control,
        namespace,
    )?;
    let mut original = ServiceRegistry::open(&old_control, namespace)?;
    original.register(&registration)?;
    original.pause(registration.binding, paused)?;
    let original_revision = original.list(None, None, 32)?.revision;
    drop(original);
    let mut saved_files = std::collections::BTreeMap::new();
    for name in [
        "roots.json",
        "extraction-policy.json",
        "vector-policy.json",
        "compression-policy.json",
        "scan-limits.json",
    ] {
        saved_files.insert(name.into(), None);
    }
    saved_files.insert(
        "vector-policy.json".into(),
        Some(serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":registration.root,"policy":{"enabled":false}
        }))?),
    );
    let saved = settings::SavedIndex {
        registration,
        paused,
        watch: false,
        roots: Default::default(),
        vectors_enabled: false,
        index_detail: airk_core::extraction::IndexDetail::Detailed,
        incarnation: Incarnation::from_bytes([12; 16]),
        settings_digest: Sha256Digest::from_bytes([13; 32]),
        default_model: None,
        saved_files,
    };
    let prior = Selection {
        schema: 1,
        active: "a".repeat(64),
        previous: None,
    };
    replace_json(&root, "selected.json", &prior)?;
    let operation = Incarnation::from_bytes([14; 16]);
    let journal = Journal {
        schema: 1,
        operation,
        install: root.clone(),
        old_control,
        state: root.join(format!("rebuild-state-{operation}")),
        candidate: "b".repeat(64),
        prior,
        manager: false,
        roster: settings::SavedRoster {
            namespace,
            revision: original_revision,
            indexes: vec![saved],
        },
        resources: Default::default(),
        layout_sha256: Sha256Digest::from_bytes(
            Sha256::digest(fs::read(root.join("runtime-layout.json"))?).into(),
        ),
        phase: Phase::Accepted,
        indexes: vec![IndexProgress {
            request: None,
            job: None,
            restored: false,
        }],
        failure: None,
    };
    journal.save(&root)?;
    Ok((owner, root, journal))
}

fn successor_fixture(paused: bool) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    let (owner, root, mut journal) = fixture(paused)?;
    let exclusion = OldExclusion::acquire(&journal)?;
    layout::admit_successor(
        &root,
        &SuccessorPermit {
            journal: &journal,
            _exclusion: &exclusion,
        },
    )?;
    drop(exclusion);
    journal.phase = Phase::Switched;
    journal.save(&root)?;
    local_ipc::prepare_root(&journal.control())?;
    let service = ServiceLease::acquire(&journal.control())?;
    layout::record_control(&service)?;
    let namespace = Incarnation::from_bytes([15; 16]);
    drop(ServiceRegistry::open(&journal.control(), namespace)?);
    layout::record_registry(&service, namespace)?;
    let mut genesis = CatalogGenesis::begin(
        service,
        Sha256Digest::from_bytes([16; 32]),
        journal.operation,
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(Incarnation::from_bytes([17; 16]), &ComponentProducer)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    let lease = ServiceLease::acquire(&journal.control())?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    registry.preserve_private_paths(&lease, &layout::retained_private_paths(&root)?)?;
    let malicious = ProjectRegistration::new(
        journal.roster.indexes[0].registration.data_root.clone(),
        root.parent()
            .unwrap()
            .join("must-remain-uncreated")
            .to_str()
            .unwrap()
            .into(),
        "unsafe-source".into(),
    )?;
    assert!(
        registry
            .preflight_source_isolation(&malicious, &[])
            .is_err(),
        "preserved private data became a source"
    );
    drop(registry);
    drop(lease);
    journal.phase = Phase::Genesis;
    journal.save(&root)?;
    Ok((owner, root, journal))
}

#[test]
fn rebuild_resume_each_setup_boundary_reuses_exact_native_receipts_and_index_job() -> Result<()> {
    for paused in [false, true] {
        for boundary in [
            IndexBoundary::RequestSaved,
            IndexBoundary::SetupAccepted,
            IndexBoundary::StoragePrepared,
            IndexBoundary::SetupComplete,
            IndexBoundary::IndexAccepted,
            IndexBoundary::IndexSaved,
            IndexBoundary::PolicyRestored,
        ] {
            let (_owner, root, mut journal) = successor_fixture(paused)?;
            let original_source =
                Path::new(&journal.roster.indexes[0].registration.root).join("original.txt");
            let original_bytes = fs::read(&original_source)?;
            let preserved_database = fs::read(journal.old_control.join("service-1.4.sqlite3"))?;
            let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
            let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
            let mut selected = 0;
            let result =
                prepare_index_observing(&root, &mut journal, 0, &mut registry, |observed| {
                    if observed == boundary {
                        selected += 1;
                        anyhow::bail!("simulated interruption");
                    }
                    Ok(())
                });
            assert!(result.is_err());
            assert_eq!(selected, 1, "interruption boundary must be exercised once");
            drop(registry);
            let mut resumed = read(&root)?.context("interrupted operation lost its journal")?;
            assert_eq!(resumed.operation, journal.operation);
            let mut registry = ServiceRegistry::open(&resumed.control(), namespace)?;
            prepare_index(&root, &mut resumed, 0, &mut registry)?;
            let progress = resumed.indexes[0].clone();
            assert!(progress.restored);
            let request = progress.request.context("resumed setup missing")?;
            assert_eq!(
                registry
                    .index_setup_receipt(request.caller, &request.key)?
                    .unwrap()
                    .stage,
                SetupStage::Ready
            );
            let jobs = registry.queued_jobs_observation(request.registration.binding, 32)?;
            assert_eq!(
                jobs.iter().filter(|job| job.kind == JobKind::Index).count(),
                1
            );
            assert_eq!(
                Some(
                    jobs.iter()
                        .find(|job| job.kind == JobKind::Index)
                        .unwrap()
                        .id
                ),
                progress.job
            );
            assert_eq!(registry.get(request.registration.binding)?.paused, paused);
            let stored = registry.job(progress.job.unwrap())?;
            prepare_index(&root, &mut resumed, 0, &mut registry)?;
            assert_eq!(registry.job(progress.job.unwrap())?.id, stored.id);
            assert_eq!(fs::read(&original_source)?, original_bytes);
            assert_eq!(
                fs::read(resumed.old_control.join("service-1.4.sqlite3"))?,
                preserved_database
            );
        }
    }
    Ok(())
}

#[test]
fn accepted_rebuild_observation_survives_incomplete_genesis_and_detects_journal_corruption()
-> Result<()> {
    let (_owner, root, mut journal) = fixture(true)?;
    let initial = observation(&root)?.unwrap();
    assert_eq!(initial.stage, RebuildStage::Accepted);
    assert!(initial.successor_namespace.is_none());
    fs::create_dir(&journal.state)?;
    fs::create_dir(journal.control())?;
    journal.phase = Phase::Switched;
    journal.save(&root)?;
    let preparing = observation(&root)?.unwrap();
    assert_eq!(preparing.stage, RebuildStage::Preparing);
    assert!(preparing.successor_namespace.is_none());
    journal.failure = Some(RebuildFailure {
        code: "fixture_interrupted".into(),
        message: "Genesis has not completed".into(),
    });
    journal.save(&root)?;
    let incomplete = observation(&root)?.unwrap();
    assert_eq!(incomplete.stage, RebuildStage::Failed);
    assert!(incomplete.successor_namespace.is_none());
    assert_eq!(
        incomplete.preserved_control,
        journal.old_control.to_str().unwrap()
    );
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(root.join(JOURNAL))?)?;
    value["journal"]["candidate"] = serde_json::json!("c".repeat(64));
    fs::write(root.join(JOURNAL), serde_json::to_vec(&value)?)?;
    assert!(read(&root).is_err());
    Ok(())
}

#[tokio::test]
async fn rebuild_boundaries_reject_scope_drift_and_keep_historical_preservation_after_later_selection()
-> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(false)?;
    let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    prepare_index(&root, &mut journal, 0, &mut registry)?;
    let mut changed = journal.clone();
    changed.indexes[0]
        .request
        .as_mut()
        .unwrap()
        .sources
        .push(SetupSource {
            canonical_path: root
                .parent()
                .unwrap()
                .join("unreviewed")
                .to_str()
                .unwrap()
                .into(),
            aliases: vec!["widened".into()],
            owner: false,
        });
    assert!(changed.validate(&root).is_err());
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    replace_json(&root, "selected.json", &journal.intent().selected())?;
    assert!(require_retirement_allowed(&root).is_err());
    let previous = retirement::retire_previous(&root, &journal.candidate)
        .await
        .unwrap_err();
    assert!(
        previous
            .to_string()
            .contains("rebuild recovery evidence is preserved")
    );
    let legacy = legacy_launchers::retire(&root, &journal.candidate)
        .await
        .unwrap_err();
    assert!(
        legacy
            .to_string()
            .contains("rebuild recovery evidence is preserved")
    );
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: "c".repeat(64),
            previous: Some(journal.candidate.clone()),
        },
    )?;
    assert!(!pending(&root)?);
    assert!(preserved_runtime(&root, &journal.prior.active)?);
    require_retirement_allowed(&root)?;
    let preserved = root
        .join("runtimes")
        .join(&journal.prior.active)
        .join("preserved.bin");
    fs::create_dir_all(preserved.parent().unwrap())?;
    fs::write(&preserved, b"preserved predecessor payload")?;
    let refused = retirement::remove(&root, &journal.prior.active).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("preserved as recovery evidence")
    );
    assert_eq!(fs::read(preserved)?, b"preserved predecessor payload");
    assert!(observation(&root)?.is_some());
    Ok(())
}

#[test]
fn rebuild_failure_messages_are_utf8_bounded_with_disclosed_truncation() {
    let message = bounded_message(&"界".repeat(3000));
    assert!(message.len() <= 4096);
    assert!(message.ends_with(" [truncated]"));
    assert_eq!(
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index"),
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index")
    );
    assert_ne!(
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index"),
        nonce(Incarnation::from_bytes([1; 16]), 0, b"setup")
    );
}

#[test]
fn rebuild_readiness_observes_resume_and_terminal_jobs_before_original_pause() {
    let mut job = airk_core::service::JobRecord {
        id: Sha256Digest::from_bytes([22; 32]),
        project: Sha256Digest::from_bytes([23; 32]),
        kind: JobKind::Index,
        state: JobState::Pending,
        fencing_generation: 0,
        attempts: 0,
        outcome: None,
    };
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Paused
    );
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Pending
    );
    job.state = JobState::Succeeded;
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Ready
    );
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Ready
    );
    job.state = JobState::Failed;
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Failed
    );
    job.state = JobState::Canceled;
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Failed
    );
    assert_eq!(
        index_readiness(None, false, true),
        RebuildReadiness::Unavailable
    );
}

#[test]
fn unfinished_rebuild_never_admits_retirement() -> Result<()> {
    let (_owner, root, mut journal) = fixture(false)?;
    for phase in [
        Phase::Accepted,
        Phase::Switched,
        Phase::Genesis,
        Phase::Setup,
        Phase::Starting,
    ] {
        journal.phase = phase;
        journal.save(&root)?;
        assert!(require_retirement_allowed(&root).is_err());
        assert!(preserved_runtime(&root, &journal.prior.active)?);
    }
    Ok(())
}

#[tokio::test]
async fn reviewed_plain_activate_resumes_only_the_exact_saved_rebuild() -> Result<()> {
    let (_owner, root, journal) = successor_fixture(false)?;
    let namespace_before = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    journal.intent().save(&root)?;
    let before = fs::read(root.join(JOURNAL))?;
    for (candidate, expected) in [
        ("c".repeat(64), journal.prior.active.clone()),
        (journal.candidate.clone(), "d".repeat(64)),
    ] {
        assert!(
            crate::runtime_install::activate_reviewed(&root, &candidate, &expected)
                .await
                .is_err()
        );
        assert_eq!(fs::read(root.join(JOURNAL))?, before);
    }
    assert!(
        crate::runtime_install::activate(&root, &journal.candidate)
            .await
            .is_err()
    );
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    let error =
        crate::runtime_install::activate_reviewed(&root, &journal.candidate, &journal.prior.active)
            .await
            .err()
            .context("inert candidate unexpectedly activated")?;
    assert!(
        !error
            .to_string()
            .contains("requires its exact reviewed predecessor")
    );
    // This inert fixture has no approved candidate executable. Reaching the
    // existing native candidate check proves dispatch entered the same owner,
    // rather than rejecting the GUI's reviewed Resume or creating a new state.
    let retained = read(&root)?.unwrap();
    assert_eq!(retained.operation, journal.operation);
    assert_eq!(retained.control(), journal.control());
    assert!(retained.failure.is_some());
    assert_eq!(
        namespace_before,
        ServiceRegistry::open_saved(&retained.control())?.incarnation()?
    );
    Ok(())
}

#[test]
fn pruned_native_rebuild_job_is_unavailable_without_inventing_execution_failure() -> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(false)?;
    let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    prepare_index(&root, &mut journal, 0, &mut registry)?;
    let request = journal.indexes[0].request.clone().unwrap();
    let job = journal.indexes[0].job.unwrap();
    let mut worker = ProjectLease::acquire(&journal.control(), request.registration.binding)?;
    registry.admit_worker(&mut worker)?;
    assert_eq!(registry.claim_job(&worker)?.unwrap().id, job);
    registry.complete_job(
        &worker,
        job,
        airk_core::service::JobOutcome::Succeeded { revision: None },
    )?;
    drop(worker);
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    assert_eq!(observation(&root)?.unwrap().stage, RebuildStage::Ready);
    let mut connection = rusqlite::Connection::open(journal.control().join("service-1.4.sqlite3"))?;
    let transaction = connection.transaction()?;
    for number in 0..1023u32 {
        let id = framed_digest(b"rebuild-pruning-fixture/1", &[&number.to_le_bytes()]);
        transaction.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','pending',0,0)",
            rusqlite::params![id.to_string(), request.registration.binding.to_string()])?;
    }
    transaction.commit()?;
    drop(connection);
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([78; 16]),
    };
    let refusal = registry
        .enqueue(
            request.registration.binding,
            request.caller,
            &key,
            JobKind::Inspect,
        )
        .unwrap_err();
    assert!(matches!(
        refusal.downcast_ref(),
        Some(airk_core::service::JobError::KeyExpired)
    ));
    assert!(registry.job(job).is_err());
    let unavailable = observation(&root)?.unwrap();
    assert_eq!(unavailable.stage, RebuildStage::Unavailable);
    assert_eq!(
        unavailable.indexes[0].readiness,
        RebuildReadiness::Unavailable
    );
    assert_eq!(
        unavailable.indexes[0].failure.as_ref().unwrap().code,
        "rebuild_job_unavailable"
    );
    assert!(unavailable.indexes[0].job.is_none());
    assert_eq!(
        unavailable.indexes[0].original,
        journal.roster.indexes[0].registration
    );
    journal.failure = Some(RebuildFailure {
        code: "fixture_execution_failed".into(),
        message: "Retained native failure".into(),
    });
    journal.save(&root)?;
    assert_eq!(observation(&root)?.unwrap().stage, RebuildStage::Failed);
    Ok(())
}

pub(in crate::runtime_install) fn historical_retirement_fixture()
-> Result<(tempfile::TempDir, PathBuf, Vec<String>, String, String)> {
    fn payload(root: &Path, label: &str) -> Result<String> {
        let source = root.join(format!("retirement-fixture-{label}"));
        local_ipc::prepare_root(&source)?;
        crate::runtime_install::tests::fixture(&source)?;
        fs::write(source.join("LICENSE"), label.as_bytes())?;
        let mut manifest: InstallManifest =
            serde_json::from_slice(&fs::read(source.join("airk-install.json"))?)?;
        manifest.files.insert(
            "LICENSE".into(),
            runtime_pair::digest_file(&source.join("LICENSE"))?,
        );
        let bytes = serde_json::to_vec(&manifest)?;
        let identity = format!("{:x}", Sha256::digest(&bytes));
        fs::write(source.join("airk-install.json"), bytes)?;
        replace_json(
            &source,
            "approval.json",
            &Approval {
                schema: 1,
                manifest_sha256: identity.clone(),
                version: manifest.version,
                verification_basis: VerificationBasis::OrganizationDigest,
            },
        )?;
        fs::rename(source, root.join("runtimes").join(&identity))?;
        Ok(identity)
    }
    let (owner, root, mut journal) = successor_fixture(false)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let original = payload(&root, "preserved-original")?;
    let original_previous = payload(&root, "preserved-original-previous")?;
    let rebuilt = payload(&root, "rebuilt")?;
    let later = payload(&root, "ordinary-later-upgrade")?;
    journal.prior.active = original.clone();
    journal.prior.previous = Some(original_previous.clone());
    journal.candidate = rebuilt.clone();
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: later.clone(),
            previous: Some(rebuilt.clone()),
        },
    )?;
    let mut preserved = vec![original, original_previous];
    preserved.sort();
    Ok((owner, root, preserved, rebuilt, later))
}
