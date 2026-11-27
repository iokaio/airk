// SPDX-License-Identifier: Apache-2.0
//! Private same-owner restart proof; no service/catalog/database is copied.
use super::*;

/// Change set 1 restarts this installation with catalog-only extra profiles.
pub(super) mod change_set;
/// Change set 2 emits Context option-B profiles in change set 1's input format.
mod change_set_2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pinned {
    path: PathBuf,
    sha256: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    ready: Pinned,
    closed: Pinned,
    status: Pinned,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub control: PathBuf,
    pub configuration: airk_core::arbiter::Configuration,
    prior_instance: airk_core::identity::Incarnation,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Closed {
    schema: String,
    evaluation: bool,
    release_qualified: bool,
    control_root: PathBuf,
    instance: airk_core::identity::Incarnation,
    shutdown_completed: bool,
    service_lease_reacquired: bool,
}
fn read(pin: &Pinned) -> Result<Vec<u8>> {
    let (_, mut file) = source_file(&pin.path, &artifact_root()?)?;
    let bytes = bounded(&mut file)?;
    ensure!(sha(&bytes) == pin.sha256, "Restart proof bytes changed");
    Ok(bytes)
}
fn settled(ready: &Ready, closed: &Closed, status: &airk_core::arbiter::Status) -> bool {
    closed_matches(ready.health.instance, &ready.control_root, closed)
        && status.initialization.is_none()
        && [&status.focus, &status.context]
            .iter()
            .all(|stage| stage.queued == 0 && stage.running == 0)
        && ready
            .health
            .catalog_clock
            .is_some_and(|clock| clock.namespace == status.configuration.catalog)
}
/// One-time, recorded exceptions to [`settled`]'s idle-stage rule (Answer evaluation, 22 November
/// 2026; the primary agent's decision under the user's authorization). Each entry names one prior
/// lifetime, the SHA-256 of its pinned final-status receipt and of its own `fixture/closed.json`.
/// L38-cs2a2 was stopped while one Focus rank read from a no-session Search replay was still in
/// flight, so its final status shows Focus running 1; everything else closed. No other lifetime,
/// digest or stage state is accepted, and [`settled`] itself is unchanged.
const UNIDLE_CLOSE_EXCEPTIONS: &[(&str, &str, &str)] = &[(
    "L38-cs2a2",
    "cbde25ff97d41ad3fbf043042742dcac8b30696c1e14a0d2415399972a6997ea",
    "f8bacf1cd2c7a24248b4f9052b66ef4cdf7c1cc31d7f992b83e79e4311083830",
)];
/// The recorded exception admits exactly: the named lifetime with both digests, its own closure
/// complete (shutdown completed, lease reacquired, same control root), Focus queued 0 and running at
/// most 1, Context idle. Restart blockers are refused separately by `verify_restart_settled`.
#[allow(clippy::too_many_arguments)]
fn unidle_exception_accepts(
    table: &[(&str, &str, &str)],
    lifetime: &str,
    status_sha256: &Sha256Digest,
    lifetime_closed_sha256: &Sha256Digest,
    lifetime_closed: &Closed,
    control: &Path,
    focus: (usize, usize),
    context: (usize, usize),
) -> bool {
    table.iter().any(|(name, status, closed)| {
        *name == lifetime
            && status
                .parse::<Sha256Digest>()
                .is_ok_and(|d| &d == status_sha256)
            && closed
                .parse::<Sha256Digest>()
                .is_ok_and(|d| &d == lifetime_closed_sha256)
    }) && lifetime_closed.schema == "airk.private-arbiter-fixture-closed/1"
        && lifetime_closed.evaluation
        && !lifetime_closed.release_qualified
        && lifetime_closed.shutdown_completed
        && lifetime_closed.service_lease_reacquired
        && lifetime_closed.control_root == control
        && focus.0 == 0
        && focus.1 <= 1
        && context == (0, 0)
}
/// Reads the named prior lifetime's own closure (`<lifetime>/fixture/closed.json`, beside the pinned
/// `<lifetime>/commands/...final-status` receipt) and applies [`unidle_exception_accepts`].
fn unidle_exception(
    request: &Request,
    ready: &Ready,
    closed: &Closed,
    status: &airk_core::arbiter::Status,
) -> Result<bool> {
    let Some(lifetime_dir) = request.status.path.parent().and_then(Path::parent) else {
        return Ok(false);
    };
    let Some(lifetime) = lifetime_dir.file_name().and_then(OsStr::to_str) else {
        return Ok(false);
    };
    if !UNIDLE_CLOSE_EXCEPTIONS
        .iter()
        .any(|(name, ..)| *name == lifetime)
    {
        return Ok(false);
    }
    let (_, mut file) = source_file(
        &lifetime_dir.join("fixture").join("closed.json"),
        &artifact_root()?,
    )?;
    let bytes = bounded(&mut file)?;
    let lifetime_closed: Closed = serde_json::from_slice(&bytes)?;
    Ok(
        closed_matches(ready.health.instance, &ready.control_root, closed)
            && status.initialization.is_none()
            && ready
                .health
                .catalog_clock
                .is_some_and(|clock| clock.namespace == status.configuration.catalog)
            && unidle_exception_accepts(
                UNIDLE_CLOSE_EXCEPTIONS,
                lifetime,
                &request.status.sha256,
                &sha(&bytes),
                &lifetime_closed,
                &ready.control_root,
                (status.focus.queued, status.focus.running),
                (status.context.queued, status.context.running),
            ),
    )
}
fn closed_matches(
    instance: airk_core::identity::Incarnation,
    control: &Path,
    closed: &Closed,
) -> bool {
    closed.schema == "airk.private-arbiter-fixture-closed/1"
        && closed.evaluation
        && !closed.release_qualified
        && closed.shutdown_completed
        && closed.service_lease_reacquired
        && closed.instance == instance
        && closed.control_root == control
}

#[test]
fn same_owner_restart_requires_actual_complete_closure_and_original_identity() {
    let instance = airk_core::identity::Incarnation::from_bytes([83; 16]);
    let control = PathBuf::from("original-control");
    let mut closed = Closed {
        schema: "airk.private-arbiter-fixture-closed/1".into(),
        evaluation: true,
        release_qualified: false,
        control_root: control.clone(),
        instance,
        shutdown_completed: true,
        service_lease_reacquired: true,
    };
    assert!(closed_matches(instance, &control, &closed));
    closed.shutdown_completed = false;
    assert!(!closed_matches(instance, &control, &closed));
    closed.shutdown_completed = true;
    closed.service_lease_reacquired = false;
    assert!(!closed_matches(instance, &control, &closed));
    closed.service_lease_reacquired = true;
    assert!(!closed_matches(
        airk_core::identity::Incarnation::from_bytes([84; 16]),
        &control,
        &closed
    ));
    assert!(!closed_matches(
        instance,
        Path::new("foreign-control"),
        &closed
    ));
}

#[test]
fn unidle_close_exception_admits_only_the_recorded_lifetime_digests_and_one_focus_read() {
    let control = PathBuf::from("original-control");
    let closed = Closed {
        schema: "airk.private-arbiter-fixture-closed/1".into(),
        evaluation: true,
        release_qualified: false,
        control_root: control.clone(),
        instance: airk_core::identity::Incarnation::from_bytes([86; 16]),
        shutdown_completed: true,
        service_lease_reacquired: true,
    };
    let status: Sha256Digest = "1111111111111111111111111111111111111111111111111111111111111111"
        .parse()
        .unwrap();
    let own: Sha256Digest = "2222222222222222222222222222222222222222222222222222222222222222"
        .parse()
        .unwrap();
    let other: Sha256Digest = "3333333333333333333333333333333333333333333333333333333333333333"
        .parse()
        .unwrap();
    let table: &[(&str, &str, &str)] = &[(
        "L38-cs2a2",
        "1111111111111111111111111111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222222222222222222222222222",
    )];
    let accepts =
        |lifetime: &str, s: &Sha256Digest, c: &Sha256Digest, closed: &Closed, focus, context| {
            unidle_exception_accepts(table, lifetime, s, c, closed, &control, focus, context)
        };
    assert!(accepts("L38-cs2a2", &status, &own, &closed, (0, 1), (0, 0)));
    assert!(accepts("L38-cs2a2", &status, &own, &closed, (0, 0), (0, 0)));
    assert!(
        !accepts("L37-cs2b", &status, &own, &closed, (0, 1), (0, 0)),
        "another lifetime"
    );
    assert!(
        !accepts("L38-cs2a2", &other, &own, &closed, (0, 1), (0, 0)),
        "another status digest"
    );
    assert!(
        !accepts("L38-cs2a2", &status, &other, &closed, (0, 1), (0, 0)),
        "another closed.json"
    );
    assert!(
        !accepts("L38-cs2a2", &status, &own, &closed, (0, 2), (0, 0)),
        "Focus running 2"
    );
    assert!(
        !accepts("L38-cs2a2", &status, &own, &closed, (1, 1), (0, 0)),
        "Focus queued"
    );
    assert!(
        !accepts("L38-cs2a2", &status, &own, &closed, (0, 1), (0, 1)),
        "Context running"
    );
    let mut broken = Closed {
        shutdown_completed: false,
        ..closed
    };
    assert!(
        !accepts("L38-cs2a2", &status, &own, &broken, (0, 1), (0, 0)),
        "shutdown incomplete"
    );
    broken.shutdown_completed = true;
    broken.service_lease_reacquired = false;
    assert!(
        !accepts("L38-cs2a2", &status, &own, &broken, (0, 1), (0, 0)),
        "lease not reacquired"
    );
    broken.service_lease_reacquired = true;
    broken.control_root = PathBuf::from("foreign-control");
    assert!(
        !accepts("L38-cs2a2", &status, &own, &broken, (0, 1), (0, 0)),
        "foreign control"
    );
    // The shipped table holds exactly the one recorded entry with full digests.
    assert_eq!(UNIDLE_CLOSE_EXCEPTIONS.len(), 1);
    assert!(
        UNIDLE_CLOSE_EXCEPTIONS
            .iter()
            .all(|(name, s, c)| *name == "L38-cs2a2"
                && s.parse::<Sha256Digest>().is_ok()
                && c.parse::<Sha256Digest>().is_ok())
    );
}
#[test]
fn work_accepted_between_preflight_and_actual_startup_is_refused() -> Result<()> {
    use airk_core::service::{JobKind, ProjectRegistration, RequestKey};
    let root = tempfile::tempdir()?;
    let caller = airk_core::identity::Incarnation::from_bytes([85; 16]);
    let mut registry = airk_store::service::registry::ServiceRegistry::open(root.path(), caller)?;
    let project = ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
    registry.register(&project)?;
    let preflight = airk_store::service::ServiceLease::acquire(root.path())?;
    EvaluationOwner::verify_restart_settled(&preflight)?;
    drop(preflight);
    let (namespace, epoch) = registry.request_epoch()?;
    registry.enqueue(
        project.binding,
        caller,
        &RequestKey {
            namespace,
            epoch,
            nonce: caller,
        },
        JobKind::Rebuild,
    )?;
    let actual_owner = airk_store::service::ServiceLease::acquire(root.path())?;
    assert!(EvaluationOwner::verify_restart_settled(&actual_owner).is_err());
    registry.pause(project.binding, true)?;
    assert!(
        EvaluationOwner::verify_restart_settled(&actual_owner).is_err(),
        "paused pending work must also refuse"
    );
    Ok(())
}
#[test]
fn verify_admitted_after_stationary_restart_does_not_refuse_reopen() -> Result<()> {
    use airk_core::{
        arbiter::{ModelChoice, Task},
        catalog_setup::{IndexSetupRequest, SetupCompletion, SetupPreparationReview, SetupSource},
        identity::Incarnation,
        service::{JobKind, ProjectRegistration, RequestKey},
    };
    use airk_store::service::{
        ServiceLease,
        arbiter::ConfigurationStore,
        registry::{ServiceRegistry, catalog::setup::NativeIndexPreparation},
    };
    let fixture = tempfile::tempdir()?;
    let directory = fixture.path().canonicalize()?;
    let control = directory.join("control");
    let source = directory.join("source");
    let data = directory.join("data");
    std::fs::create_dir(&control)?;
    std::fs::create_dir(&source)?;
    let lease = Arc::new(first_start::ensure_catalog(ServiceLease::acquire(
        &control,
    )?)?);
    let instance = Incarnation::from_bytes([86; 16]);
    let mut registry = ServiceRegistry::open(&control, instance)?;
    let catalog = registry
        .catalog_state()?
        .context("activated catalog missing")?;
    let (namespace, epoch) = registry.request_epoch()?;
    assert_eq!(namespace, catalog.namespace);
    let registration = ProjectRegistration::new(
        source
            .to_str()
            .context("fixture source path is not UTF-8")?
            .into(),
        data.to_str()
            .context("fixture data path is not UTF-8")?
            .into(),
        "fixture".into(),
    )?;
    let setup = IndexSetupRequest {
        caller: instance,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([87; 16]),
        },
        expected_catalog_revision: catalog.revision,
        name: "Restart race fixture".into(),
        registration: registration.clone(),
        sources: vec![SetupSource {
            canonical_path: registration.root.clone(),
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&setup)?;
    // Only the registry metadata and job admission are tested here.
    let native = NativeIndexPreparation::from_validated_native_review(
        registration.clone(),
        setup.sources.clone(),
        Sha256Digest::from_bytes([88; 32]),
        Sha256Digest::from_bytes([89; 32]),
        SetupPreparationReview {
            expected_catalog_revision: reserved.catalog_revision,
            expected_source_revision: reserved.source_revision,
        },
    )?;
    let prepared = registry.record_index_preparation(
        instance,
        &setup.key,
        reserved.request_digest,
        &native,
    )?;
    let ready = registry.complete_index_setup(
        instance,
        &setup.key,
        reserved.request_digest,
        &SetupCompletion {
            legacy_adoption: None,
            prepared_digest: prepared
                .preparation
                .context("prepared review missing")?
                .digest,
            expected_catalog_revision: prepared.catalog_revision,
            expected_source_revision: prepared.source_revision,
        },
    )?;
    let gate = airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
        instance,
        namespace,
        ready.catalog_revision,
        1,
    )?;
    let binding = airk_endpoint::http_endpoint::installation_binding(lease.root(), namespace)?;
    let config = ConfigurationStore::open(lease.clone(), binding.installation, namespace)?;
    let saved = config.initialize_checked(None, || Ok(()))?;
    // Reviewed setup requests ModelSetup and Index even with source watching
    // off. Withdraw these unadmitted intents through the native fixture lease.
    assert!(registry.project_setup_pending(registration.binding)?);
    assert!(
        registry
            .pending_job_observation(registration.binding)?
            .is_none()
    );
    let mut project_lease =
        airk_store::service::ProjectLease::acquire(lease.root(), registration.binding)?;
    registry.admit_worker(&mut project_lease)?;
    registry.set_background_pending(&project_lease, JobKind::ModelSetup, false)?;
    registry.set_background_pending(&project_lease, JobKind::Index, false)?;
    assert!(!registry.project_background_pending(registration.binding)?);
    assert!(!registry.has_unsettled_configuration(registration.binding)?);
    assert!(registry.pending_projects(1)?.is_empty());
    drop(project_lease);

    let settlement = EvaluationOwner::stationary_restart(&lease, &saved)
        .context("capture stationary proof before Verify admission")?;
    let for_foreign_lease = EvaluationOwner::stationary_restart(&lease, &saved)
        .context("capture stationary proof for foreign-lease refusal")?;
    let for_changed_config = EvaluationOwner::stationary_restart(&lease, &saved)
        .context("capture stationary proof for changed-configuration refusal")?;

    let verify = registry.enqueue(
        registration.binding,
        instance,
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([90; 16]),
        },
        JobKind::Verify,
    )?;
    assert_eq!(verify.kind, JobKind::Verify);
    assert_eq!(registry.pending_projects(1)?, [registration.binding]);
    assert!(EvaluationOwner::stationary_restart(&lease, &saved).is_err());

    // Reuse the native Focus ABI with the current policy. This test loads no weights.
    let mut profile = airk_retrieval::arbiter::runtime::catalog_profiles()?
        .into_iter()
        .find(|profile| profile.model == ModelChoice::FocusQwenReranker06B)
        .context("native Focus test descriptor missing")?;
    let policy = airk_retrieval::arbiter::compiler::focus::policy_digest()?;
    profile.policy = policy;
    profile.task_policies = [(Task::FocusRank, policy)].into_iter().collect();
    profile.qualification = None;
    profile.qualified_targets.clear();
    profile.downloads.clear();
    let reopened = EvaluationOwner::reopen_same_configuration(
        lease.clone(),
        gate.clone(),
        vec![profile.clone()],
        settlement,
    )
    .context("reopen using stationary proof after Verify admission")?;
    assert_eq!(reopened.configuration()?.into_result(), saved);
    assert_eq!(registry.pending_projects(1)?, [registration.binding]);
    reopened.into_owner_for(&lease, &gate)?;

    let foreign = tempfile::tempdir()?;
    let foreign_lease = Arc::new(ServiceLease::acquire(foreign.path())?);
    let Err(wrong_lease) = EvaluationOwner::reopen_same_configuration(
        foreign_lease,
        gate.clone(),
        vec![],
        for_foreign_lease,
    ) else {
        anyhow::bail!("restart proof admitted a different service lease");
    };
    assert_eq!(
        wrong_lease.to_string(),
        "Restart settlement belongs to another service lease"
    );

    let mut changed = saved.clone();
    changed.revision += 1;
    config.apply(saved.revision, &changed)?;
    let Err(wrong_configuration) =
        EvaluationOwner::reopen_same_configuration(lease, gate, vec![profile], for_changed_config)
    else {
        anyhow::bail!("restart proof admitted a changed configuration");
    };
    assert_eq!(
        wrong_configuration.to_string(),
        "Private evaluation requires an unconfigured owner or exact saved restart configuration"
    );
    Ok(())
}

fn env_request() -> Result<Option<Vec<u8>>> {
    let Some(path) = std::env::var_os("AIRK_ARBITER_EVALUATION_RESTART") else {
        return Ok(None);
    };
    let digest: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_RESTART_SHA256")?.parse()?;
    read(&Pinned {
        path: PathBuf::from(path),
        sha256: digest,
    })
    .map(Some)
}

pub(super) fn from_env(expected: Sha256Digest, prepared: &[Prepared]) -> Result<Option<Saved>> {
    let Some(raw) = env_request()? else {
        return Ok(None);
    };
    cpu_restart(&serde_json::from_slice(&raw)?, expected, prepared).map(Some)
}

fn cpu_restart(request: &Request, expected: Sha256Digest, prepared: &[Prepared]) -> Result<Saved> {
    let ready: Ready = serde_json::from_slice(&read(&request.ready)?)?;
    let closed: Closed = serde_json::from_slice(&read(&request.closed)?)?;
    let status: airk_core::arbiter::Status = serde_json::from_slice(&read(&request.status)?)?;
    status.validate()?;
    let prior_work = request
        .ready
        .path
        .parent()
        .context("Prior Ready parent missing")?
        .canonicalize()?;
    ensure!(
        request.ready.path.file_name() == Some(OsStr::new("ready.json"))
            && request.closed.path.canonicalize()?
                == prior_work.join("closed.json").canonicalize()?
            && ready.control_root.canonicalize()? == prior_work.join("control").canonicalize()?
            && prior_work.starts_with(artifact_root()?),
        "Restart must reopen its original private control root"
    );
    ensure!(
        ready.schema == "airk.private-arbiter-fixture/1"
            && ready.evaluation
            && !ready.release_qualified
            && ready.inputs_sha256 == expected
            && ready.profiles.len() == prepared.len()
            && ready
                .profiles
                .iter()
                .zip(prepared)
                .all(|(old, current)| old.manifest == current.profile.manifest
                    && old.model == current.profile.model
                    && old.profile_sha256 == current.sha256)
            && (settled(&ready, &closed, &status)
                || unidle_exception(request, &ready, &closed, &status)?),
        "Restart closure, profiles or saved configuration differ"
    );
    // Actual exclusion is acquired again by normal serve_owned. No caller proof replaces it.
    let lease = airk_store::service::ServiceLease::acquire(&ready.control_root)?;
    EvaluationOwner::verify_restart_settled(&lease)?;
    drop(lease);
    Ok(Saved {
        control: ready.control_root,
        configuration: status.configuration,
        prior_instance: ready.health.instance,
    })
}

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
mod cuda_restart {
    use super::*;
    use airk_core::{
        arbiter::{Availability, Configuration, Stage, StageStatus, Status, device::GpuBackend},
        catalog_management::{CatalogEntity, CatalogItem},
        identity::Incarnation,
        index_catalog::SourceImportState,
    };
    use airk_store::service::{ServiceLease, registry::ServiceRegistry};

    /// CUDA restarts one evaluation installation may chain. Raised from 16 to 64 on
    /// 26 November 2026 by user decision: the settled Windows publications had
    /// reached 16 and re-indexing them to continue was not warranted. Every link
    /// is still read and checked.
    const MAX_SUCCESSORS: usize = 64;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CudaRequest {
        ready: Pinned,
        closed: Pinned,
        status: Pinned,
        cuda_lineage: Vec<Successor>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Successor {
        ready: Pinned,
        closed: Pinned,
        restart_owner: Pinned,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SameInstallRequest {
        Cuda(CudaRequest),
        Cpu(Request),
    }

    pub(in crate::service_runtime::evaluation_fixture_tests) fn from_same_install_env(
        expected: Sha256Digest,
        cpu: &[Prepared],
        cuda: &[Prepared],
    ) -> Result<(Option<Saved>, bool)> {
        let Some(raw) = env_request()? else {
            return Ok((None, false));
        };
        match serde_json::from_slice(&raw)? {
            SameInstallRequest::Cpu(request) => {
                Ok((Some(cpu_restart(&request, expected, cpu)?), false))
            }
            SameInstallRequest::Cuda(request) => {
                Ok((Some(checked_restart(&request, expected, cpu, cuda)?), true))
            }
        }
    }

    fn read_lifetime(ready_pin: &Pinned, closed_pin: &Pinned) -> Result<(PathBuf, Ready, Closed)> {
        let work = ready_pin
            .path
            .parent()
            .context("CUDA Ready parent missing")?
            .canonicalize()?;
        ensure!(
            ready_pin.path.file_name() == Some(OsStr::new("ready.json"))
                && closed_pin.path.canonicalize()? == work.join("closed.json").canonicalize()?,
            "CUDA predecessor Ready and Closed must come from the same lifetime"
        );
        let ready: Ready = serde_json::from_slice(&read(ready_pin)?)?;
        let closed: Closed = serde_json::from_slice(&read(closed_pin)?)?;
        ensure!(
            ready.schema == "airk.private-arbiter-fixture/1"
                && ready.evaluation
                && !ready.release_qualified
                && ready.health.process_id > 0
                && closed_matches(ready.health.instance, &ready.control_root, &closed),
            "CUDA predecessor did not prove complete native shutdown and lease reacquisition"
        );
        Ok((work, ready, closed))
    }

    fn read_roster(
        work: &Path,
        ready: &Ready,
        expected: Sha256Digest,
        cpu: &[Prepared],
        historical_cuda: &mut BTreeMap<Sha256Digest, (Profile, Sha256Digest)>,
    ) -> Result<bool> {
        ensure!(
            ready.inputs_sha256 == expected
                && (cpu.len()..=cpu.len() + 3).contains(&ready.profiles.len())
                && ready.profiles.iter().zip(cpu).all(|(prior, current)| {
                    prior.model == current.profile.model
                        && prior.manifest == current.profile.manifest
                        && prior.profile_sha256 == current.sha256
                }),
            "CUDA predecessor CPU identity or profile roster length differs"
        );
        let artifacts = artifact_root()?;
        let mut models = BTreeSet::new();
        for (index, binding) in ready.profiles.iter().enumerate().skip(cpu.len()) {
            let filename = format!("profile-{index}.json");
            ensure!(
                binding.snapshot == filename,
                "CUDA predecessor profile snapshot name differs"
            );
            let (_parent, mut file) = source_file(&work.join(filename), &artifacts)?;
            let raw = bounded(&mut file)?;
            ensure!(
                sha(&raw) == binding.profile_sha256,
                "CUDA predecessor profile snapshot hash differs"
            );
            let profile: Profile = serde_json::from_slice(&raw)?;
            profile.validate()?;
            ensure!(
                profile.manifest == binding.manifest
                    && profile.model == binding.model
                    && profile.profile_id.starts_with("private-")
                    && profile.qualification.is_none()
                    && profile.qualified_targets.is_empty()
                    && profile.downloads.is_empty()
                    && profile
                        .device_execution
                        .as_ref()
                        .is_some_and(|device| device.backend == GpuBackend::Cuda)
                    && cpu.iter().any(|entry| entry.profile.model == profile.model)
                    && cpu
                        .iter()
                        .all(|entry| entry.profile.manifest != profile.manifest)
                    && models.insert(profile.model)
                    && historical_cuda
                        .get(&profile.manifest)
                        .is_none_or(|(_, old)| *old == binding.profile_sha256),
                "CUDA predecessor profile binding is malformed, duplicate or crossed"
            );
            historical_cuda.insert(profile.manifest, (profile, binding.profile_sha256));
        }
        Ok(!models.is_empty())
    }

    fn linked_configuration(
        saved: &Saved,
        prior_instance: Incarnation,
        configuration: &Configuration,
        minimum_revision: u64,
    ) -> bool {
        saved.prior_instance == prior_instance
            && saved.configuration.installation == configuration.installation
            && saved.configuration.catalog == configuration.catalog
            && (minimum_revision..=configuration.revision).contains(&saved.configuration.revision)
            && (saved.configuration.revision != configuration.revision
                || saved.configuration == *configuration)
    }

    fn linked(
        prior: &Ready,
        successor: &Ready,
        saved: &Saved,
        configuration: &Configuration,
        minimum_revision: u64,
    ) -> bool {
        let catalog = Some(configuration.catalog);
        linked_configuration(
            saved,
            prior.health.instance,
            configuration,
            minimum_revision,
        ) && prior.health.instance != successor.health.instance
            && saved.control == successor.control_root
            && prior.control_root == successor.control_root
            && prior.health.user == successor.health.user
            && successor.health.catalog_clock.map(|clock| clock.namespace) == catalog
    }

    fn disabled_idle(
        stage: &airk_core::arbiter::StageConfiguration,
        observed: &StageStatus,
    ) -> bool {
        !stage.enabled
            && observed.availability == Availability::Disabled
            && observed.active_profile.is_none()
            && observed.queued == 0
            && observed.running == 0
    }

    fn inert_successor(
        selected: Sha256Digest,
        stage: Stage,
        old: &Profile,
        old_sha: Sha256Digest,
        current: impl Iterator<Item = (Sha256Digest, ModelChoice, bool, Sha256Digest)>,
    ) -> bool {
        if old.model.stage() != Some(stage) {
            return false;
        }
        let mut supported = false;
        for (manifest, model, same_task_policies, sha) in current {
            if manifest == selected && sha != old_sha {
                return false;
            }
            supported |= manifest == selected || (model == old.model && same_task_policies);
        }
        supported
    }

    fn profiles_compatible(
        status: &Status,
        cpu: &[Prepared],
        cuda: &[Prepared],
        historical: &BTreeMap<Sha256Digest, (Profile, Sha256Digest)>,
    ) -> Result<()> {
        let current = || cpu.iter().chain(cuda);
        for (stage, observed) in [
            (Stage::Focus, &status.focus),
            (Stage::Context, &status.context),
        ] {
            let configuration = status.configuration.stage(stage);
            ensure!(
                disabled_idle(configuration, observed),
                "CUDA predecessor stages must be disabled, inactive and idle"
            );
            let Some(selected) = configuration.selected_profile else {
                continue;
            };
            if let Some((old, old_sha)) = historical.get(&selected) {
                ensure!(
                    inert_successor(
                        selected,
                        stage,
                        old,
                        *old_sha,
                        current().map(|entry| (
                            entry.profile.manifest,
                            entry.profile.model,
                            entry.profile.task_policies == old.task_policies,
                            entry.sha256,
                        )),
                    ),
                    "Inert CUDA selection has no authenticated compatible successor profile"
                );
            } else {
                ensure!(
                    cpu.iter().any(|entry| entry.profile.manifest == selected
                        && entry.profile.model.stage() == Some(stage)),
                    "CUDA predecessor selected an unknown or wrong-stage profile"
                );
            }
        }
        Ok(())
    }

    fn checked_restart(
        request: &CudaRequest,
        expected: Sha256Digest,
        cpu: &[Prepared],
        cuda: &[Prepared],
    ) -> Result<Saved> {
        ensure!(
            request.cuda_lineage.len() <= MAX_SUCCESSORS,
            "CUDA restart lineage bound exceeded"
        );
        let (mut work, mut ready, mut closed) = read_lifetime(&request.ready, &request.closed)?;
        let root = ready.control_root.canonicalize()?;
        ensure!(
            ready.control_root.is_absolute()
                && root == work.join("control").canonicalize()?
                && ready.health.user == crate::local_ipc::current_user()?,
            "CUDA restart requires the original private control root and stable owner"
        );
        let status: Status = serde_json::from_slice(&read(&request.status)?)?;
        status.validate()?;
        let configuration = &status.configuration;
        ensure!(
            ready
                .health
                .catalog_clock
                .is_some_and(|clock| clock.namespace == configuration.catalog),
            "CUDA restart origin catalog differs"
        );
        let mut historical = BTreeMap::new();
        let mut has_cuda = read_roster(&work, &ready, expected, cpu, &mut historical)?;
        let mut instances = BTreeSet::from([ready.health.instance]);
        let mut revision = 1;
        for link in &request.cuda_lineage {
            let (next_work, next_ready, next_closed) = read_lifetime(&link.ready, &link.closed)?;
            ensure!(
                link.restart_owner.path.canonicalize()?
                    == next_work.join("restart-owner.json").canonicalize()?
                    && next_ready.control_root.canonicalize()? == root
                    && instances.insert(next_ready.health.instance),
                "CUDA successor restart ownership is missing, crossed or repeated"
            );
            let saved: Saved = serde_json::from_slice(&read(&link.restart_owner)?)?;
            saved.configuration.validate()?;
            ensure!(
                linked(&ready, &next_ready, &saved, configuration, revision),
                "CUDA restart lineage is not an exact successor chain"
            );
            revision = saved.configuration.revision;
            has_cuda = read_roster(&next_work, &next_ready, expected, cpu, &mut historical)?;
            (work, ready, closed) = (next_work, next_ready, next_closed);
        }
        let commands = work
            .parent()
            .context("CUDA predecessor lifetime missing")?
            .join("commands");
        let final_status = request.status.path.canonicalize()?;
        ensure!(
            has_cuda
                && final_status.parent() == Some(commands.canonicalize()?.as_path())
                && final_status
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.ends_with("-final-status.stdout"))
                && settled(&ready, &closed, &status),
            "CUDA restart requires the actual immediate predecessor final status and idle closure"
        );
        profiles_compatible(&status, cpu, cuda, &historical)?;
        let lease = Arc::new(ServiceLease::acquire(&ready.control_root)?);
        EvaluationOwner::verify_restart_settled(&lease)?;
        verify_cuda_enrollments(&lease, configuration)?;
        let store = airk_store::service::arbiter::ConfigurationStore::open(
            lease,
            configuration.installation,
            configuration.catalog,
        )?;
        ensure!(
            store.load()? == *configuration,
            "CUDA restart saved configuration has changed on disk"
        );
        Ok(Saved {
            control: ready.control_root,
            configuration: status.configuration,
            prior_instance: ready.health.instance,
        })
    }

    #[test]
    fn cuda_restart_inert_selection_requires_compatible_successor_and_no_alias() -> Result<()> {
        let old = airk_retrieval::arbiter::runtime::catalog_profiles()?
            .into_iter()
            .find(|profile| profile.model == ModelChoice::ContextQwenCompact)
            .context("test Compact profile missing")?;
        let sha = Sha256Digest::from_bytes([10; 32]);
        let other = Sha256Digest::from_bytes([11; 32]);
        let matches = (other, old.model, true, other);
        let check = |stage, options: Vec<_>| {
            inert_successor(old.manifest, stage, &old, sha, options.into_iter())
        };
        assert!(check(Stage::Context, vec![matches]));
        assert!(!check(Stage::Focus, vec![matches]));
        assert!(!check(
            Stage::Context,
            vec![(other, old.model, false, other)]
        ));
        assert!(!check(
            Stage::Context,
            vec![(other, ModelChoice::FocusQwenReranker06B, true, other)]
        ));
        assert!(!check(
            Stage::Context,
            vec![matches, (old.manifest, old.model, true, other)]
        ));
        assert!(check(
            Stage::Context,
            vec![(old.manifest, old.model, true, sha)]
        ));
        Ok(())
    }

    #[test]
    fn cuda_restart_refuses_draining_active_and_enabled_predecessors() {
        let mut stage = airk_core::arbiter::StageConfiguration::default();
        let mut observed = StageStatus {
            selected_profile: None,
            active_profile: None,
            availability: Availability::Disabled,
            reason: "test".into(),
            queued: 0,
            running: 0,
            profiles: vec![],
        };
        assert!(disabled_idle(&stage, &observed));
        observed.running = 1;
        assert!(!disabled_idle(&stage, &observed));
        observed.running = 0;
        observed.availability = Availability::Draining;
        assert!(!disabled_idle(&stage, &observed));
        observed.availability = Availability::Disabled;
        observed.active_profile = Some(Sha256Digest::from_bytes([3; 32]));
        assert!(!disabled_idle(&stage, &observed));
        observed.active_profile = None;
        stage.enabled = true;
        assert!(!disabled_idle(&stage, &observed));
    }

    #[test]
    fn cuda_restart_refuses_crossed_successor_lineage_or_changed_same_revision() {
        let prior = Incarnation::from_bytes([14; 16]);
        let next = Incarnation::from_bytes([15; 16]);
        let installation = Sha256Digest::from_bytes([16; 32]);
        let mut current = Configuration::disabled(installation, prior);
        current.revision = 4;
        let mut saved = Saved {
            control: "original-control".into(),
            configuration: current.clone(),
            prior_instance: prior,
        };
        assert!(linked_configuration(&saved, prior, &current, 3));
        assert!(!linked_configuration(&saved, next, &current, 3));
        saved.configuration.focus.selected_profile = Some(installation);
        assert!(!linked_configuration(&saved, prior, &current, 3));
        saved.configuration.revision = 3;
        assert!(linked_configuration(&saved, prior, &current, 3));
        assert!(!linked_configuration(&saved, prior, &current, 4));
        saved.configuration.catalog = next;
        assert!(!linked_configuration(&saved, prior, &current, 3));
    }

    #[test]
    fn cuda_restart_requests_are_distinct_from_legacy_cpu_inputs() -> Result<()> {
        let pin = serde_json::json!({"path":"private", "sha256":"1".repeat(64)});
        let mut request = serde_json::json!({"ready":pin, "closed":pin, "status":pin});
        assert!(matches!(
            serde_json::from_value(request.clone())?,
            SameInstallRequest::Cpu(_)
        ));
        request["cuda_lineage"] =
            serde_json::json!([{"ready":pin, "closed":pin, "restart_owner":pin}]);
        assert!(matches!(
            serde_json::from_value(request.clone())?,
            SameInstallRequest::Cuda(_)
        ));
        assert!(serde_json::from_value::<Request>(request.clone()).is_err());
        request["cuda_lineage"][0]
            .as_object_mut()
            .context("successor")?
            .remove("restart_owner");
        assert!(serde_json::from_value::<SameInstallRequest>(request).is_err());
        Ok(())
    }

    pub(in crate::service_runtime::evaluation_fixture_tests) fn verify_cuda_enrollments(
        lease: &ServiceLease,
        configuration: &Configuration,
    ) -> Result<()> {
        let mut registry = ServiceRegistry::open_saved(lease.root())?;
        let catalog = registry
            .catalog_state()?
            .context("CUDA restart catalog missing")?;
        let installation =
            airk_endpoint::http_endpoint::installation_binding(lease.root(), catalog.namespace)?;
        ensure!(
            catalog.activated
                && catalog.namespace == configuration.catalog
                && installation.installation == configuration.installation,
            "CUDA restart native installation or catalog changed"
        );
        for enrollment in configuration
            .focus
            .indexes
            .iter()
            .chain(&configuration.context.indexes)
        {
            let detail = registry.catalog_detail(
                &CatalogEntity::Index {
                    id: enrollment.index_id,
                },
                Some(catalog.revision),
            )?;
            ensure!(
                matches!(detail.item, Some(CatalogItem::Index(index)) if !index.tombstone && index.source_import == SourceImportState::Complete),
                "CUDA restart enrollment must name a live immutable index in the original catalog"
            );
        }
        Ok(())
    }
}

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
pub(super) use cuda_restart::{from_same_install_env, verify_cuda_enrollments};

pub(super) fn matches_control(work: &Path, ready: &Ready) -> Result<bool> {
    if ready.control_root == work.join("control") {
        return Ok(true);
    }
    let Some(raw) = crate::ipc::private_record::read_private(work, "restart-owner.json", LIMIT)?
    else {
        return Ok(false);
    };
    let saved: Saved = serde_json::from_slice(&raw)?;
    Ok(saved.control == ready.control_root
        && saved.prior_instance != ready.health.instance
        && ready
            .health
            .catalog_clock
            .is_some_and(|clock| clock.namespace == saved.configuration.catalog))
}
