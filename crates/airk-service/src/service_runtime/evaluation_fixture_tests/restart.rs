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
pub(super) fn from_env(expected: Sha256Digest, prepared: &[Prepared]) -> Result<Option<Saved>> {
    let Some(path) = std::env::var_os("AIRK_ARBITER_EVALUATION_RESTART") else {
        return Ok(None);
    };
    let digest: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_RESTART_SHA256")?.parse()?;
    let request: Request = serde_json::from_slice(&read(&Pinned {
        path: PathBuf::from(path),
        sha256: digest,
    })?)?;
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
                || unidle_exception(&request, &ready, &closed, &status)?),
        "Restart closure, profiles or saved configuration differ"
    );
    // Actual exclusion is acquired again by normal serve_owned. No caller proof replaces it.
    let lease = airk_store::service::ServiceLease::acquire(&ready.control_root)?;
    EvaluationOwner::verify_restart_settled(&lease)?;
    drop(lease);
    Ok(Some(Saved {
        control: ready.control_root,
        configuration: status.configuration,
        prior_instance: ready.health.instance,
    }))
}
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
