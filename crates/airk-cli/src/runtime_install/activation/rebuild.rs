// SPDX-License-Identifier: Apache-2.0
//! Explicit early rebuild, owned by the installer rather than an activation-error catch.
//! Old state is retained; the successor uses ordinary Genesis, setup and Index owners.
use super::*;
use airk_core::{
    catalog_setup::{IndexSetupRequest, SetupCompletion, SetupSource, SetupStage},
    identity::{Incarnation, Sha256Digest, framed_digest},
    rebuild::{
        RebuildFailure, RebuildIndexObservation, RebuildObservation, RebuildProvenance,
        RebuildReadiness, RebuildStage,
    },
    service::{JobKind, JobRecord, JobState, ProjectRegistration, RequestKey},
    service_resources::ResourceConfiguration,
};
use airk_store::service::{
    EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease, registry::ServiceRegistry,
};

mod settings;

const JOURNAL: &str = "rebuild.json";
const MAX_JOURNAL: u64 = 512 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Accepted,
    Switched,
    Genesis,
    Setup,
    Starting,
    Complete,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexProgress {
    request: Option<IndexSetupRequest>,
    job: Option<Sha256Digest>,
    restored: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    preferences_restored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completed: Option<airk_core::service::JobRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    initial_job: Option<airk_core::service::JobRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure: Option<RebuildFailure>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    operation: Incarnation,
    install: PathBuf,
    old_control: PathBuf,
    state: PathBuf,
    candidate: String,
    prior: Selection,
    manager: bool,
    roster: settings::SavedRoster,
    resources: ResourceConfiguration,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    clean_break: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_indexes: Option<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cleanup_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    superseded_activation: Option<Intent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cleanup_observation: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cleanup_failure: Option<RebuildFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_registrations: Option<Vec<ProjectRegistration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    http_ownership_snapshot: Option<airk_core::service_http_cleanup::PredecessorHttpSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    predecessor_stopped: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    predecessor_lifecycle_sequence: Option<u64>,
    layout_sha256: Sha256Digest,
    phase: Phase,
    indexes: Vec<IndexProgress>,
    failure: Option<RebuildFailure>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    journal: Journal,
    sha256: Sha256Digest,
}

impl Journal {
    fn control(&self) -> PathBuf {
        self.state.join("control")
    }
    fn intent(&self) -> Intent {
        Intent {
            schema: 1,
            install: self.install.clone(),
            control: self.control(),
            candidate: self.candidate.clone(),
            prior: Some(self.prior.clone()),
            manager: self.manager,
            phase: super::Phase::Probe,
            checkpoint: None,
        }
    }
    fn selected(&self) -> Selection {
        let mut selected = self.intent().selected();
        if self.clean_break {
            selected.previous = None;
        }
        selected
    }
    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            self.schema == 1 && self.install == root.canonicalize()?,
            "foreign rebuild owner"
        );
        digest(&self.candidate)?;
        digest(&self.prior.active)?;
        ensure!(
            self.prior.schema == 1,
            "unsupported rebuild predecessor selection"
        );
        if let Some(previous) = &self.prior.previous {
            digest(previous)?;
        }
        ensure!(
            self.clean_break || self.candidate != self.prior.active,
            "rebuild requires a distinct candidate"
        );
        ensure!(
            self.state == rebuild_state(root, self.operation)?
                && self.old_control.is_absolute()
                && self.old_control != self.control(),
            "rebuild state binding changed"
        );
        self.roster.validate()?;
        ensure!(
            serde_json::to_vec(&self.roster)?.len() <= 128 * 1024,
            "saved rebuild configuration exceeds supported bound"
        );
        self.resources.validate()?;
        if let Some(snapshot) = &self.http_ownership_snapshot {
            snapshot.validate()?;
            ensure!(
                self.clean_break
                    && snapshot.predecessor == self.prior.active
                    && Path::new(&snapshot.control_dir) == self.old_control
                    && snapshot
                        .namespace
                        .is_none_or(|namespace| namespace == self.roster.namespace),
                "HTTP disposal snapshot crossed original reset namespace"
            );
        }
        if let Some(metadata) = &self.client_metadata {
            ensure!(
                self.clean_break
                    && metadata["schema"] == 1
                    && metadata["predecessor"].as_str() == Some(self.prior.active.as_str())
                    && metadata["control_dir"].as_str().map(Path::new)
                        == Some(self.old_control.as_path())
                    && metadata["namespace"] == serde_json::to_value(self.roster.namespace)?
                    && serde_json::to_vec(metadata)?.len() <= 64 * 1024,
                "predecessor client metadata changed original reset binding"
            );
        }
        ensure!(
            (self.clean_break || !self.roster.indexes.is_empty())
                && self.roster.indexes.len() <= 64
                && self.indexes.len() == self.roster.indexes.len(),
            "invalid rebuild roster"
        );
        let mut bindings = std::collections::BTreeSet::new();
        if self.clean_break {
            ensure!(
                self.capture_complete.is_some()
                    && (self.capture_complete == Some(true)
                        || self.phase == Phase::Accepted && self.indexes.is_empty()),
                "incomplete clean-break capture advanced past stop"
            );
        }
        for (position, progress) in self.indexes.iter().enumerate() {
            if let Some(completed) = &progress.completed {
                ensure!(
                    progress.job == Some(completed.id)
                        && completed.state == JobState::Succeeded
                        && progress
                            .request
                            .as_ref()
                            .is_some_and(
                                |request| request.registration.binding == completed.project
                            ),
                    "clean-break completed receipt crossed accepted index"
                );
            }
            ensure!(
                !progress.preferences_restored || progress.completed.is_some(),
                "clean-break preferences lack successful initial Index"
            );
            if let Some(request) = &progress.request {
                request.validate()?;
                let original = &self.roster.indexes[position].registration;
                ensure!(
                    request.registration.root == original.root
                        && request.registration.collection == original.collection
                        && request.registration.data_root
                            == replacement_data(root, self.operation, position)?
                                .to_str()
                                .context("non-UTF-8 rebuild data")?
                        && bindings.insert(request.registration.binding),
                    "rebuild replacement crossed saved scope"
                );
                ensure!(
                    request.key.nonce == nonce(self.operation, position, b"setup")
                        && request.caller == crate::local_ipc::caller_identity()?
                        && !request.setup_watch,
                    "rebuild setup identity changed"
                );
                let mut expected = std::collections::BTreeMap::<String, Vec<String>>::new();
                expected.insert(original.root.clone(), Vec::new());
                for (alias, path) in &self.roster.indexes[position].roots {
                    expected
                        .entry(path.clone())
                        .or_default()
                        .push(alias.clone());
                }
                ensure!(
                    request.sources
                        == expected
                            .into_iter()
                            .map(|(canonical_path, aliases)| SetupSource {
                                owner: canonical_path == original.root,
                                canonical_path,
                                aliases
                            })
                            .collect::<Vec<_>>(),
                    "rebuild setup changed saved source membership"
                );
            }
            ensure!(
                !progress.restored || progress.job.is_some() && progress.request.is_some(),
                "incomplete restored rebuild index"
            );
        }
        if self.phase != Phase::Complete {
            let selected = selection(root)?;
            ensure!(
                selected == Some(self.prior.clone())
                    || selected == Some(self.selected())
                    || self.clean_break
                        && selected
                            .as_ref()
                            .is_some_and(|selected| selected.active == self.candidate),
                "runtime selection changed during rebuild"
            );
        }
        Ok(())
    }
    fn save(&self, root: &Path) -> Result<()> {
        self.validate(root)?;
        let envelope = Envelope {
            sha256: Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(self)?).into()),
            journal: self.clone(),
        };
        ensure!(
            serde_json::to_vec(&envelope)?.len() as u64 <= MAX_JOURNAL,
            "rebuild roster exceeds bound"
        );
        replace_json(root, JOURNAL, &envelope)
    }
}

fn read(root: &Path) -> Result<Option<Journal>> {
    if !exists(&root.join(JOURNAL))? {
        return Ok(None);
    }
    safe_file(&root.join(JOURNAL))?;
    let bytes = read_bounded(&root.join(JOURNAL), MAX_JOURNAL)?;
    let mut value: Envelope = serde_json::from_slice(&bytes)?;
    ensure!(
        value.sha256
            == Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&value.journal)?).into())
            && serde_json::to_vec(&value)? == bytes,
        "rebuild journal digest or encoding changed"
    );
    // Reconcile only an unused, equivalent operation-bound leaf from admission
    // before any layout handoff or setup acceptance. Reads never rewrite it.
    let journal = &mut value.journal;
    let state = rebuild_state(root, journal.operation)?;
    if journal.state != state {
        ensure!(
            journal.install == root.canonicalize()?
                && journal.phase == Phase::Accepted
                && journal
                    .indexes
                    .iter()
                    .all(|index| index.request.is_none() && index.job.is_none())
                && crate::user_paths::canonical_location(&journal.state)? == state
                && !layout::successor_handoff_started(
                    root,
                    journal.operation,
                    journal.layout_sha256
                )?,
            "rebuild state location differs from its exact accepted operation"
        );
        journal.state = state;
    }
    journal.validate(root)?;
    Ok(Some(value.journal))
}

pub(in crate::runtime_install) fn pending(root: &Path) -> Result<bool> {
    Ok(read(root)?.is_some_and(|value| value.phase != Phase::Complete))
}

pub(in crate::runtime_install) fn initialized_probe_operation(
    root: &Path,
    control: &Path,
    candidate: &str,
) -> Result<Option<Incarnation>> {
    let Some(journal) = read(root)? else {
        return Ok(None);
    };
    if !journal.clean_break
        || journal.phase != Phase::Starting
        || journal.candidate != candidate
        || journal.control() != control
    {
        return Ok(None);
    }
    let Some(intent) = super::pending(root)? else {
        return Ok(None);
    };
    ensure!(
        serde_json::to_value(&intent)? == serde_json::to_value(journal.intent())?
            && crate::install_layout::saved_control(root)? == control
            && layout::successor_handoff_started(root, journal.operation, journal.layout_sha256)?,
        "clean-break bootstrap does not match the exact admitted Probe operation"
    );
    Ok(Some(journal.operation))
}

pub(in crate::runtime_install) fn activation_predecessor(
    root: &Path,
    reviewed: Option<&str>,
) -> Result<String> {
    if let Some(reviewed) = reviewed {
        return Ok(reviewed.into());
    }
    ensure!(
        read(root)?.is_some_and(|journal| journal.clean_break),
        "saved rebuild requires its exact reviewed predecessor"
    );
    Ok(selection_record(root)?.active)
}

/// Only an explicit owning mutation may retry setup that failed before a native
/// job was accepted. Ordinary observations and automatic ensure stay inert.
pub(in crate::runtime_install) fn repair_pending(root: &Path, candidate: &str) -> Result<bool> {
    Ok(read(root)?
        .is_some_and(|journal| journal.candidate == candidate && setup_repair_pending(&journal)))
}

fn setup_repair_pending(journal: &Journal) -> bool {
    journal.clean_break
        && journal.phase == Phase::Complete
        && !journal.cleanup_complete
        && journal
            .indexes
            .iter()
            .any(|index| !index.restored && index.failure.is_some())
}

/// An inert receipt cannot construct this exclusion proof. All predecessor
/// service/project/helper/watch leases remain borrowed through the layout effect.
pub(in crate::runtime_install) struct SuccessorPermit<'a> {
    journal: &'a Journal,
    _exclusion: &'a OldExclusion,
}
impl SuccessorPermit<'_> {
    pub(in crate::runtime_install) fn operation(&self) -> Incarnation {
        self.journal.operation
    }
    pub(in crate::runtime_install) fn old_control(&self) -> &Path {
        &self.journal.old_control
    }
    pub(in crate::runtime_install) fn state(&self) -> &Path {
        &self.journal.state
    }
    pub(in crate::runtime_install) fn layout_sha256(&self) -> Sha256Digest {
        self.journal.layout_sha256
    }
}

struct OldExclusion {
    _service: ServiceLease,
    _projects: Vec<ProjectLease>,
    _embedding: EmbeddingLease,
    _watch: SourceWatchLease,
}
impl OldExclusion {
    fn acquire(journal: &Journal) -> Result<Self> {
        let service = ServiceLease::acquire(&journal.old_control)?;
        let registry = ServiceRegistry::open_saved(&journal.old_control)?;
        ensure!(
            registry.incarnation()? == journal.roster.namespace,
            "rebuild predecessor namespace changed"
        );
        let bindings = registry.bindings()?;
        drop(registry);
        let projects = bindings
            .into_iter()
            .map(|binding| ProjectLease::acquire(&journal.old_control, binding))
            .collect::<Result<Vec<_>>>()?;
        let embedding = EmbeddingLease::acquire(&journal.old_control)?;
        let watch = SourceWatchLease::acquire(&journal.old_control)?;
        ensure!(
            crate::embedding_pool_process::shared::quiesce_for_maintenance(&service)?,
            std::fs::TryLockError::WouldBlock
        );
        Ok(Self {
            _service: service,
            _projects: projects,
            _embedding: embedding,
            _watch: watch,
        })
    }
}

/// Caller retains the installation lock. Repeating the same explicit request
/// resumes its accepted operation; a different candidate cannot replace it.
pub(in crate::runtime_install) async fn activate(
    root: &Path,
    candidate: &str,
    expected: &str,
) -> Result<Selection> {
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    digest(candidate)?;
    digest(expected)?;
    if let Some(saved) = read(root)? {
        let replace_completed = saved.phase == Phase::Complete
            && (!saved.clean_break || saved.cleanup_complete)
            && state_contract::required(root)?;
        if !replace_completed {
            ensure!(
                saved.candidate == candidate
                    && (saved.prior.active == expected
                        || saved.clean_break
                            && expected == candidate
                            && selection_record(root)?.active == candidate),
                "another rebuild already owns this installation; inspect its receipt"
            );
            if saved.phase == Phase::Complete && !setup_repair_pending(&saved) {
                ensure!(
                    selection(root)? == Some(saved.selected()),
                    "completed rebuild selection has since changed"
                );
                return Ok(saved.selected());
            }
            return resume(root).await;
        }
    }
    let clean_break = state_contract::required(root)?;
    if clean_break {
        state_contract::candidate(root, candidate)?;
    }
    managed::require_unmanaged(root)?;
    if !clean_break {
        require_settled(root)?;
        lifecycle::require_settled(root)?;
    }
    if !clean_break {
        lifecycle::require_start_allowed(root)?;
    }
    let prior = selection(root)?.context("rebuild requires an installed predecessor")?;
    ensure!(
        prior.active == expected && (clean_break || candidate != expected),
        "rebuild selection changed since review"
    );
    require_candidate(root, candidate)?;
    verify_runtime(root, &prior.active)?;
    let old_control = layout::control(root)?;
    let mut registry = if clean_break {
        settings::open_predecessor(&old_control)?
    } else {
        ServiceRegistry::open_saved(&old_control)?
    };
    let predecessor_stopped = registry.service_explicitly_stopped()?;
    let predecessor_lifecycle_sequence = registry.lifecycle_sequence()?;
    if !clean_break {
        ensure!(!predecessor_stopped, "rebuild cannot clear User Stop");
    }
    let expected_indexes = if clean_break {
        Some(settings::preflight_clean_break(&old_control)?)
    } else {
        None
    };
    let client_registrations = if clean_break {
        let revision = registry.list(None, None, 1)?.revision;
        let bindings = registry.bindings()?;
        let values = bindings
            .into_iter()
            .map(|binding| {
                registry
                    .retained_project(binding)
                    .map(|project| project.registration)
            })
            .collect::<Result<Vec<_>>>()?;
        registry.list(None, Some(revision), 1)?;
        Some(values)
    } else {
        None
    };
    let roster = if clean_break {
        settings::SavedRoster {
            namespace: registry.incarnation()?,
            revision: registry.list(None, None, 1)?.revision,
            indexes: Vec::new(),
        }
    } else {
        settings::capture(&old_control)?
    };
    drop(registry);
    if !clean_break {
        ensure!(
            airk_store::service::registry::checkpoint::snapshot(&old_control)?
                .unfinished
                .is_empty(),
            "rebuild cannot discard unresolved accepted operations"
        );
    }
    let superseded_activation = if clean_break {
        super::pending(root)?
    } else {
        None
    };
    if let Some(saved) = &superseded_activation {
        ensure!(
            saved.control == old_control
                && (saved
                    .prior
                    .as_ref()
                    .is_some_and(|selected| selected.active == prior.active)
                    || saved.candidate == prior.active),
            "clean break cannot discard an unbound activation"
        );
    }
    let operation = airk_kernel::service_broker::new_instance_id()?;
    if clean_break {
        layout::preflight_clean_break(root, operation)?;
        upgrade_stop::preflight_all_predecessors(root, candidate)?;
    }
    let client_metadata = if clean_break {
        Some(capture_client_metadata(
            root,
            &prior.active,
            candidate,
            &old_control,
            roster.namespace,
        )?)
    } else {
        None
    };
    let journal = Journal {
        schema: 1,
        operation,
        install: root.canonicalize()?,
        old_control: old_control.clone(),
        state: rebuild_state(root, operation)?,
        candidate: candidate.into(),
        prior,
        manager: crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()?,
        indexes: roster
            .indexes
            .iter()
            .map(|_| IndexProgress {
                request: None,
                job: None,
                restored: false,
                preferences_restored: false,
                completed: None,
                initial_job: None,
                failure: None,
            })
            .collect(),
        roster,
        resources: crate::service_resource_policy::load(&old_control)?,
        clean_break,
        capture_complete: clean_break.then_some(false),
        expected_indexes,
        cleanup_complete: false,
        superseded_activation,
        cleanup_observation: None,
        cleanup_failure: None,
        client_registrations,
        client_metadata,
        http_ownership_snapshot: None,
        predecessor_stopped: clean_break.then_some(predecessor_stopped),
        predecessor_lifecycle_sequence: clean_break.then_some(predecessor_lifecycle_sequence),
        layout_sha256: Sha256Digest::from_bytes(
            Sha256::digest(read_bounded(&root.join("runtime-layout.json"), 32 * 1024)?).into(),
        ),
        phase: Phase::Accepted,
        failure: None,
    };
    ensure!(
        !journal.state.try_exists()?,
        "rebuild successor state already exists"
    );
    for position in 0..journal.roster.indexes.len() {
        let data = replacement_data(root, journal.operation, position)?;
        match fs::symlink_metadata(&data) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => bail!("rebuild replacement storage already exists"),
        }
    }
    journal.save(root)?;
    resume(root).await
}

fn capture_client_metadata(
    root: &Path,
    predecessor: &str,
    candidate: &str,
    control: &Path,
    namespace: Incarnation,
) -> Result<serde_json::Value> {
    let (main, _) = verify_runtime(root, predecessor)?;
    let executable =
        serde_json::json!({"path":main.canonicalize()?,"sha256":runtime_pair::digest_file(&main)?});
    let legacy = legacy_launchers::inspect_clean_break_locked(root, predecessor)?;
    let mut command_images = vec![executable.clone()];
    for field in ["launcher", "universal_launcher"] {
        if !legacy[field].is_null() {
            command_images.push(legacy[field].clone());
        }
    }
    // The full legacy inspection can include a staged successor in its
    // nonselected inventory. Only this explicit original-command subset proves
    // removable predecessor configuration; it cannot adopt successor commands.
    for image in &command_images {
        let path = Path::new(
            image["path"]
                .as_str()
                .context("predecessor command path missing")?,
        );
        ensure!(
            predecessor == candidate || !path.starts_with(root.join("runtimes").join(candidate)),
            "clean-break original commands crossed staged successor"
        );
    }
    Ok(serde_json::json!({
        "schema":1,"predecessor":predecessor,"control_dir":control,"namespace":namespace,
        "predecessor_executable":executable,"legacy_launchers":legacy,
        "command_images":command_images
    }))
}

pub(in crate::runtime_install) async fn resume(root: &Path) -> Result<Selection> {
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    let mut journal = read(root)?.context("accepted rebuild missing")?;
    if journal.phase == Phase::Complete {
        if !setup_repair_pending(&journal) {
            return Ok(journal.selected());
        }
        let (main, approval) = verify_runtime(root, &journal.candidate)?;
        drain(root, &main, &approval).await?;
        let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
        if journal.manager {
            manager.stop()?;
        }
        manager.require_stopped_for_maintenance()?;
        let service = ServiceLease::acquire(&journal.control())?;
        journal.phase = Phase::Setup;
        journal.failure = None;
        journal.save(root)?;
        drop(service);
    }
    match advance(root, &mut journal).await {
        Ok(result) => Ok(result),
        Err(error) => {
            journal.failure = Some(RebuildFailure {
                code: "native_rebuild_failed".into(),
                message: bounded_message(&error.to_string()),
            });
            if let Err(recording) = journal.save(root) {
                return Err(error.context(format!(
                    "rebuild failure observation could not be persisted: {recording}"
                )));
            }
            Err(error)
        }
    }
}

fn predecessor_stop_allows_reset(
    captured_stopped: bool,
    captured_sequence: u64,
    current_stopped: bool,
    current_sequence: u64,
) -> bool {
    !current_stopped || captured_stopped && captured_sequence == current_sequence
}

async fn advance(root: &Path, journal: &mut Journal) -> Result<Selection> {
    journal.validate(root)?;
    require_candidate(root, &journal.candidate)?;
    require_issuer_rotation_settled(root)?;
    if !journal.clean_break {
        lifecycle::require_settled(root)?;
    }
    if !journal.clean_break || !matches!(journal.phase, Phase::Accepted | Phase::Switched) {
        lifecycle::require_start_allowed(root)?;
    }
    let owner = crate::service_manager::ServiceManager::for_runtime(root)?;
    ensure!(
        owner.owns_restart()? == journal.manager,
        "rebuild startup ownership changed"
    );
    if journal.phase == Phase::Accepted {
        if !layout::successor_handoff_started(root, journal.operation, journal.layout_sha256)? {
            if journal.clean_break {
                layout::preflight_clean_break(root, journal.operation)?;
                upgrade_stop::preflight_all_predecessors(root, &journal.candidate)?;
            }
            ensure!(
                crate::install_layout::saved_control(root)? == journal.old_control,
                "rebuild original control changed"
            );
            require_issuer_rotation_settled(root)?;
            payload_repair::require_settled(root)?;
            launcher_repair::require_settled(root)?;
            if journal.manager {
                owner.stop()?;
            }
            if let Some(saved) = &journal.superseded_activation
                && saved.candidate != journal.prior.active
            {
                upgrade_stop::stop(root, &saved.candidate)?;
            }
            upgrade_stop::stop(root, &journal.prior.active)?;
            let (main, approval) = verify_runtime(root, &journal.prior.active)?;
            drain(root, &main, &approval).await?;
            if journal.clean_break {
                upgrade_stop::stop_all_predecessors(root, &journal.candidate)?;
            }
        }
        owner.require_stopped_for_maintenance()?;
        let exclusion = OldExclusion::acquire(journal)?;
        if journal.clean_break {
            let old_registry = ServiceRegistry::open_saved(&journal.old_control)?;
            let old_stopped = old_registry.service_explicitly_stopped()?;
            let old_sequence = old_registry.lifecycle_sequence()?;
            ensure!(
                predecessor_stop_allows_reset(
                    journal
                        .predecessor_stopped
                        .context("clean-break predecessor Stop observation missing")?,
                    journal
                        .predecessor_lifecycle_sequence
                        .context("clean-break lifecycle observation missing")?,
                    old_stopped,
                    old_sequence
                ),
                "clean_break_new_user_stop: a Stop accepted after reset admission remains authoritative"
            );
            if journal.http_ownership_snapshot.is_none() {
                journal.http_ownership_snapshot = Some(http_cleanup::capture(
                    root,
                    &journal.prior.active,
                    &journal.old_control,
                ));
                journal.save(root)?;
            }
            if journal.capture_complete != Some(true) {
                journal.roster = settings::capture_clean_break(&journal.old_control)?;
                ensure!(
                    journal.roster.indexes.len() <= 64,
                    "clean_break_complete_roster_limit: more than 64 indexes; no subset was selected"
                );
                journal.resources = crate::service_resource_policy::load(&journal.old_control)?;
                journal.indexes = journal
                    .roster
                    .indexes
                    .iter()
                    .map(|_| IndexProgress {
                        request: None,
                        job: None,
                        restored: false,
                        preferences_restored: false,
                        completed: None,
                        initial_job: None,
                        failure: None,
                    })
                    .collect();
                journal.capture_complete = Some(true);
                journal.expected_indexes = Some(journal.roster.indexes.len());
                journal.save(root)?;
            }
            journal.roster.check_clean_break(&journal.old_control)?;
            if exists(&root.join(INTENT))? {
                fs::remove_file(root.join(INTENT))?;
                sync_directory(root)?;
            }
        } else {
            journal.roster.check(&journal.old_control)?;
            ensure!(
                airk_store::service::registry::checkpoint::snapshot(&journal.old_control)?
                    .unfinished
                    .is_empty(),
                "rebuild predecessor still has unresolved accepted operations"
            );
            ensure!(
                serde_json::to_value(crate::service_resource_policy::load(&journal.old_control)?)?
                    == serde_json::to_value(&journal.resources)?,
                "saved service resource policy changed"
            );
        }
        layout::admit_successor(
            root,
            &SuccessorPermit {
                journal,
                _exclusion: &exclusion,
            },
        )?;
        journal.phase = Phase::Switched;
        journal.failure = None;
        journal.save(root)?;
    }
    let intent = journal.intent();
    if journal.phase == Phase::Switched {
        owner.require_stopped_for_maintenance()?;
        local_ipc::prepare_root(&journal.control())?;
        intent.save(root)?;
        catalog_upgrade::activate_rebuild_genesis(root, &intent)?;
        ensure!(
            ServiceRegistry::open_saved(&journal.control())?.incarnation()?
                != journal.roster.namespace,
            "rebuild successor reused its predecessor namespace"
        );
        crate::service_resource_policy::save(&journal.control(), &journal.resources)?;
        journal.phase = Phase::Genesis;
        journal.save(root)?;
    }
    if matches!(journal.phase, Phase::Genesis | Phase::Setup) {
        owner.require_stopped_for_maintenance()?;
        let _service = ServiceLease::acquire(&journal.control())?;
        let mut registry = ServiceRegistry::open_saved(&journal.control())?;
        // Store's ordinary mutation owner verifies this namespace and complete
        // activated catalog. No old registry bytes are imported or relabeled.
        let namespace = registry.incarnation()?;
        drop(registry);
        registry = ServiceRegistry::open(&journal.control(), namespace)?;
        let mut preserved = crate::install_layout::retained_private_paths(root)?;
        preserved.sort();
        preserved.dedup();
        registry.preserve_private_paths(&_service, &preserved)?;
        for position in 0..journal.roster.indexes.len() {
            journal.indexes[position].failure = None;
            if let Err(error) = prepare_index(root, journal, position, &mut registry) {
                if !journal.clean_break {
                    return Err(error);
                }
                journal.indexes[position].failure = Some(RebuildFailure {
                    code: "clean_break_index_preparation_failed".into(),
                    message: bounded_message(&format!("{error:#}")),
                });
                journal.save(root)?;
            }
        }
        journal.phase = Phase::Starting;
        journal.save(root)?;
    }
    if journal.phase == Phase::Starting {
        // Starting is idempotent by the canonical exact candidate health check.
        launcher_repair::repair(root, &journal.candidate)?;
        let (main, approval) = verify_runtime(root, &journal.candidate)?;
        crate::process_roles::install(&main)?;
        probe(root, &approval, &main, journal.manager).await?;
        replace_json(root, "selected.json", &journal.selected())?;
        record(root, &intent, Outcome::Selected)?;
        if exists(&root.join(INTENT))? {
            finish(root)?;
        }
        journal.phase = Phase::Complete;
        journal.failure = None;
        journal.save(root)?;
    }
    Ok(journal.selected())
}

fn require_candidate(root: &Path, candidate: &str) -> Result<()> {
    let (main, approval) = verify_runtime(root, candidate)?;
    ensure!(
        approval.version == env!("CARGO_PKG_VERSION") && runtime_pair::single_executable(&main)?,
        "rebuild requires the current single-executable candidate"
    );
    let observation = self_test_observation(&main)?;
    let capabilities: airk_core::index_catalog::CatalogCapabilities = serde_json::from_value(
        observation
            .get("index_catalog")
            .cloned()
            .context("rebuild candidate has no native catalog capabilities")?,
    )?;
    airk_store::service::registry::runtime_contract::RequiredCapabilities {
        minimum_registry_schema: airk_core::index_catalog::REGISTRY_SCHEMA,
        authority_contract: airk_store::service::registry::catalog::SUPPORTED_AUTHORITY_CONTRACT,
        policy_contract: airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
    }
    .require_supported(&capabilities)
}

fn nonce(operation: Incarnation, position: usize, purpose: &[u8]) -> Incarnation {
    let digest = framed_digest(
        b"airk/rebuild-request/1",
        &[
            operation.as_bytes(),
            &(position as u64).to_le_bytes(),
            purpose,
        ],
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    Incarnation::from_bytes(bytes)
}

fn rebuild_state(root: &Path, operation: Incarnation) -> Result<PathBuf> {
    Ok(root
        .canonicalize()?
        .join(format!("rebuild-state-{operation}")))
}

fn replacement_data(root: &Path, operation: Incarnation, position: usize) -> Result<PathBuf> {
    // Registered index data must remain outside installation/control state.
    // Its existing parent is pinned by ordinary native setup; each fresh leaf
    // is owned by that exact durable setup request rather than a second creator.
    Ok(root
        .canonicalize()?
        .parent()
        .context("rebuild installation has no storage parent")?
        .join(format!("rebuild-index-{operation}-{position}")))
}

fn prepare_index(
    root: &Path,
    journal: &mut Journal,
    position: usize,
    registry: &mut ServiceRegistry,
) -> Result<()> {
    prepare_index_observing(root, journal, position, registry, |_| Ok(()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IndexBoundary {
    RequestSaved,
    SetupAccepted,
    StoragePrepared,
    SetupComplete,
    IndexAccepted,
    IndexSaved,
    PolicyRestored,
}

fn prepare_index_observing(
    root: &Path,
    journal: &mut Journal,
    position: usize,
    registry: &mut ServiceRegistry,
    mut observe: impl FnMut(IndexBoundary) -> Result<()>,
) -> Result<()> {
    let saved = journal.roster.indexes[position].clone();
    if journal.indexes[position].restored {
        let progress = &journal.indexes[position];
        let request = progress
            .request
            .as_ref()
            .context("restored rebuild setup missing")?;
        let job = registry.job(progress.job.context("restored rebuild job missing")?)?;
        ensure!(
            job.project == request.registration.binding,
            "restored rebuild job changed owner"
        );
        return Ok(());
    }
    if journal.indexes[position].request.is_none() {
        let (namespace, epoch) = registry.saved_request_epoch()?;
        let mut paths = std::collections::BTreeMap::<String, Vec<String>>::new();
        paths.insert(saved.registration.root.clone(), Vec::new());
        for (alias, path) in &saved.roots {
            paths.entry(path.clone()).or_default().push(alias.clone());
        }
        let request = IndexSetupRequest {
            caller: crate::local_ipc::caller_identity()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: nonce(journal.operation, position, b"setup"),
            },
            expected_catalog_revision: registry
                .catalog_state()?
                .context("rebuild catalog missing")?
                .revision,
            name: saved.registration.collection.clone(),
            registration: ProjectRegistration::new(
                saved.registration.root.clone(),
                replacement_data(root, journal.operation, position)?
                    .to_str()
                    .context("non-UTF-8 rebuild data")?
                    .into(),
                saved.registration.collection.clone(),
            )?,
            sources: paths
                .into_iter()
                .map(|(canonical_path, aliases)| SetupSource {
                    owner: canonical_path == saved.registration.root,
                    canonical_path,
                    aliases,
                })
                .collect(),
            setup_watch: false,
        };
        request.validate()?;
        journal.indexes[position].request = Some(request);
        journal.phase = Phase::Setup;
        journal.save(root)?;
        observe(IndexBoundary::RequestSaved)?;
    }
    let mut request = journal.indexes[position]
        .request
        .clone()
        .context("rebuild setup missing")?;
    let receipt = registry.index_setup_receipt(request.caller, &request.key)?;
    if journal.clean_break && receipt.is_none() {
        // No native admission exists: update only the review precondition, not
        // the caller/key, source selection or original setup identity.
        request.expected_catalog_revision = registry
            .catalog_state()?
            .context("rebuild catalog missing")?
            .revision;
        journal.indexes[position].request = Some(request.clone());
        journal.save(root)?;
    }
    if receipt
        .as_ref()
        .is_none_or(|receipt| receipt.stage != SetupStage::Ready)
    {
        let review = crate::catalog_index_setup::review(&journal.control(), &request)?;
        if receipt.is_none() {
            registry.reserve_index_setup(&request)?;
            observe(IndexBoundary::SetupAccepted)?;
        }
        let prepared = crate::catalog_index_setup::prepare(registry, &journal.control(), review)?
            .record(registry)?;
        observe(IndexBoundary::StoragePrepared)?;
        let completion = SetupCompletion {
            legacy_adoption: None,
            prepared_digest: prepared
                .preparation
                .as_ref()
                .context("rebuild preparation missing")?
                .digest,
            expected_catalog_revision: registry
                .catalog_state()?
                .context("rebuild catalog missing")?
                .revision,
            expected_source_revision: prepared.source_revision,
        };
        registry.complete_index_setup(
            request.caller,
            &request.key,
            request.identity()?,
            &completion,
        )?;
        observe(IndexBoundary::SetupComplete)?;
    }
    // There is no service owner yet, so no worker can consume default policy
    // between setup completion and exact saved configuration restoration.
    if journal.indexes[position].job.is_none() {
        registry.pause(request.registration.binding, true)?;
        settings::apply_saved_policy(&journal.control(), &request.registration, &saved)?;
        registry.configure_watch(
            request.registration.binding,
            !journal.clean_break && saved.watch,
        )?;
        let (namespace, epoch) = registry.saved_request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: nonce(journal.operation, position, b"index"),
        };
        let job = registry.enqueue(
            request.registration.binding,
            request.caller,
            &key,
            JobKind::Index,
        )?;
        observe(IndexBoundary::IndexAccepted)?;
        journal.indexes[position].job = Some(job.id);
        journal.save(root)?;
        observe(IndexBoundary::IndexSaved)?;
    }
    registry.pause(
        request.registration.binding,
        !journal.clean_break && saved.paused,
    )?;
    journal.indexes[position].restored = true;
    journal.save(root)?;
    observe(IndexBoundary::PolicyRestored)
}

pub(super) fn require_genesis(root: &Path, intent: &Intent) -> Result<()> {
    let journal = read(root)?.context("native successor operation missing")?;
    ensure!(
        journal.phase == Phase::Switched
            && journal.control() == intent.control
            && journal.candidate == intent.candidate
            && intent.prior == Some(journal.prior.clone())
            && crate::install_layout::saved_control(root)? == journal.control(),
        "successor Genesis binding changed"
    );
    Ok(())
}

pub(in crate::runtime_install) fn observation(root: &Path) -> Result<Option<RebuildObservation>> {
    let Some(journal) = read(root)? else {
        return Ok(None);
    };
    // Before Genesis completes, its native registry may deliberately be absent
    // or admission-fenced. Keep the accepted progress/failure observable without
    // reclassifying a corrupt initialized registry as a fresh installation.
    let mut failure = journal.failure.clone();
    let execution_failed = journal.failure.is_some();
    let mut registry_unavailable = false;
    let successor = if journal.phase == Phase::Accepted
        || (journal.phase == Phase::Switched
            && !journal.control().join("service-1.4.sqlite3").try_exists()?
            && !journal
                .control()
                .join("service-1.4.owner.json")
                .try_exists()?)
    {
        None
    } else {
        match ServiceRegistry::open_saved(&journal.control()) {
            Ok(registry) => Some(registry),
            Err(error)
                if journal.phase == Phase::Switched
                    && error.is::<airk_store::service::registry::catalog::AdmissionPending>() =>
            {
                None
            }
            Err(error) => {
                registry_unavailable = true;
                failure.get_or_insert_with(|| RebuildFailure {
                    code: "rebuild_registry_unavailable".into(),
                    message: bounded_message(&error.to_string()),
                });
                None
            }
        }
    };
    let mut indexes = Vec::new();
    for (saved, progress) in journal.roster.indexes.iter().zip(&journal.indexes) {
        let mut index_failure = progress.failure.clone().or_else(|| registry_unavailable.then(|| RebuildFailure {
            code: "rebuild_registry_unavailable".into(),
            message: "Native rebuild registry metadata is unavailable; inspect the operation-level diagnostic.".into(),
        }));
        let job = match (successor.as_ref(), progress.job) {
            (Some(registry), Some(id)) => match registry.job(id) {
                Ok(job) => Some(job),
                Err(_) if progress.completed.is_some() => progress.completed.clone(),
                Err(error) => {
                    index_failure = Some(RebuildFailure {
                        code: "rebuild_job_unavailable".into(),
                        message: bounded_message(&error.to_string()),
                    });
                    None
                }
            },
            _ => progress.completed.clone(),
        };
        let paused = match (successor.as_ref(), progress.request.as_ref()) {
            (Some(registry), Some(request)) => match registry.get(request.registration.binding) {
                Ok(project) => project.paused,
                Err(error) => {
                    index_failure = Some(RebuildFailure {
                        code: "rebuild_registration_unavailable".into(),
                        message: bounded_message(&error.to_string()),
                    });
                    saved.paused
                }
            },
            _ => saved.paused,
        };
        let readiness = index_readiness(job.as_ref(), paused, index_failure.is_some());
        indexes.push(RebuildIndexObservation {
            original: saved.registration.clone(),
            initial_job: progress.initial_job.clone(),
            replacement: progress
                .request
                .as_ref()
                .map(|request| request.registration.clone()),
            sources: saved.roots.clone(),
            settings_sha256: saved.settings_digest,
            settings_available: journal.clean_break.then_some(saved.settings_available),
            discarded_preferences: saved.discarded_preferences.clone(),
            original_paused: saved.paused,
            paused,
            watching: saved.watch,
            failure: job
                .as_ref()
                .filter(|job| matches!(job.state, JobState::Failed | JobState::Canceled))
                .map(|job| RebuildFailure {
                    code: "native_index_job_failed".into(),
                    message: format!("{:?}", job.outcome),
                })
                .or(index_failure),
            job,
            readiness,
        });
    }
    let stage = if execution_failed
        || indexes
            .iter()
            .any(|index| index.readiness == RebuildReadiness::Failed)
    {
        RebuildStage::Failed
    } else if registry_unavailable
        || indexes
            .iter()
            .any(|index| index.readiness == RebuildReadiness::Unavailable)
    {
        RebuildStage::Unavailable
    } else if journal.phase == Phase::Complete
        && indexes
            .iter()
            .all(|index| index.readiness == RebuildReadiness::Ready)
    {
        RebuildStage::Ready
    } else if journal.phase == Phase::Complete
        && indexes.iter().all(|index| {
            matches!(
                index.readiness,
                RebuildReadiness::Ready | RebuildReadiness::Paused
            )
        })
    {
        RebuildStage::Paused
    } else if journal.phase == Phase::Accepted {
        RebuildStage::Accepted
    } else if matches!(journal.phase, Phase::Starting | Phase::Complete) {
        RebuildStage::Indexing
    } else {
        RebuildStage::Preparing
    };
    let result = RebuildObservation {
        schema: 1,
        mode: journal
            .clean_break
            .then_some(airk_core::rebuild::RebuildMode::CleanBreak),
        indexes_complete: journal.capture_complete,
        expected_indexes: journal.expected_indexes,
        cleanup: journal
            .clean_break
            .then_some(airk_core::rebuild::RebuildCleanup {
                complete: journal.cleanup_complete,
                scope: "administrative".into(),
                state: if journal.cleanup_complete {
                    "complete"
                } else if journal.cleanup_failure.is_some() {
                    "blocked"
                } else if journal
                    .indexes
                    .iter()
                    .all(|index| index.completed.is_some())
                    && journal.phase == Phase::Complete
                {
                    "removing"
                } else {
                    "waiting_for_indexes"
                }
                .into(),
                failure: journal.cleanup_failure.clone(),
                observation: journal.cleanup_observation.clone(),
            }),
        operation_id: journal.operation,
        stage,
        provenance: RebuildProvenance::NativeRebuildFromSavedConfiguration,
        candidate: journal.candidate.parse()?,
        predecessor: journal.prior.active.parse()?,
        original_namespace: journal.roster.namespace,
        preserved_state: journal
            .old_control
            .parent()
            .context("old state missing")?
            .to_string_lossy()
            .into_owned(),
        preserved_control: journal.old_control.to_string_lossy().into_owned(),
        preserved_index_roots: journal
            .roster
            .indexes
            .iter()
            .map(|index| index.registration.data_root.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        successor_state: journal.state.to_string_lossy().into_owned(),
        successor_control: journal.control().to_string_lossy().into_owned(),
        successor_namespace: successor
            .as_ref()
            .map(ServiceRegistry::incarnation)
            .transpose()?,
        original_failure: None,
        failure,
        indexes,
        index_scope: None,
    };
    result.validate()?;
    Ok(Some(result))
}

fn index_readiness(
    job: Option<&airk_core::service::JobRecord>,
    paused: bool,
    unavailable: bool,
) -> RebuildReadiness {
    match job.map(|job| job.state) {
        Some(JobState::Failed | JobState::Canceled) => RebuildReadiness::Failed,
        _ if unavailable => RebuildReadiness::Unavailable,
        Some(JobState::Succeeded) => RebuildReadiness::Ready,
        _ if paused => RebuildReadiness::Paused,
        _ => RebuildReadiness::Pending,
    }
}

fn bounded_message(message: &str) -> String {
    if message.len() <= 4096 {
        message.into()
    } else {
        format!(
            "{} [truncated]",
            message.chars().take(960).collect::<String>()
        )
    }
}

pub(in crate::runtime_install) fn observation_for_control(
    control: &Path,
    binding: airk_core::service::ProjectBinding,
) -> Result<Option<RebuildObservation>> {
    let Some(root) = crate::install_layout::installation_for_control(control)? else {
        return Ok(None);
    };
    let Some(value) = observation(&root)? else {
        return Ok(None);
    };
    ensure!(
        Path::new(&value.successor_control) == control,
        "rebuild observation control changed"
    );
    value.scoped_for(binding)
}

/// A turn mutates only installer-owned reset progress after native jobs have
/// recorded their real completion. It never runs from a read handler.
pub(in crate::runtime_install) fn continue_clean_break(
    root: &Path,
    owner: &ServiceLease,
    namespace: Incarnation,
) -> Result<()> {
    continue_clean_break_observing(
        root,
        owner,
        namespace,
        |registry, binding, paused, watch| {
            registry.pause(binding, paused)?;
            registry.configure_watch(binding, watch)
        },
    )
}

fn continue_clean_break_observing(
    root: &Path,
    owner: &ServiceLease,
    namespace: Incarnation,
    mut restore: impl FnMut(
        &mut ServiceRegistry,
        airk_core::service::ProjectBinding,
        bool,
        bool,
    ) -> Result<()>,
) -> Result<()> {
    let control = owner.root();
    if !exists(&root.join(JOURNAL))? {
        return Ok(());
    }
    let mut journal = read(root)?.context("clean-break operation missing")?;
    if !journal.clean_break || journal.phase != Phase::Complete || journal.cleanup_complete {
        return Ok(());
    }
    let lock = match install_lock(root) {
        Ok(lock) => lock,
        Err(error) if ownership_contention(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    journal = read(root)?.context("clean-break operation disappeared")?;
    ensure!(
        journal.control() == control
            && crate::install_layout::saved_control(root)? == control
            && selection_record(root)?.active == journal.candidate
            && !state_contract::required(root)?,
        "clean-break continuation crossed selected state authority"
    );
    let indexes_complete =
        ServiceRegistry::with_existing_maintenance(owner, namespace, |registry| {
            let mut first_restore_failure = None;
            for position in 0..journal.indexes.len() {
                if journal.indexes[position].preferences_restored {
                    continue;
                }
                let progress = &journal.indexes[position];
                let (Some(request), Some(id)) = (&progress.request, progress.job) else {
                    continue;
                };
                let binding = request.registration.binding;
                let mut job = match &progress.completed {
                    Some(completed) => completed.clone(),
                    None => match registry.job(id) {
                        Ok(job) => job,
                        Err(error) => {
                            journal.indexes[position].failure = Some(RebuildFailure {
                                code: "clean_break_index_receipt_unavailable".into(),
                                message: bounded_message(&format!("{error:#}")),
                            });
                            journal.save(root)?;
                            continue;
                        }
                    },
                };
                ensure!(
                    job.project == binding && job.id == id,
                    "clean-break completion crossed index"
                );
                if matches!(job.state, JobState::Failed | JobState::Canceled) {
                    let Some(retry) = registry.successful_retry(id)? else {
                        continue;
                    };
                    ensure!(
                        retry.project == binding
                            && retry.kind == job.kind
                            && retry.state == JobState::Succeeded,
                        "clean-break retry crossed accepted lineage"
                    );
                    journal.indexes[position].initial_job.get_or_insert(job);
                    journal.indexes[position].job = Some(retry.id);
                    job = retry;
                }
                if job.state != JobState::Succeeded {
                    continue;
                }
                // Keep an exact durable native receipt before ordinary bounded job-history retirement.
                record_completed_job(&mut journal.indexes[position], job)?;
                journal.save(root)?;
                let restored = restore(
                    registry,
                    binding,
                    journal.roster.indexes[position].paused,
                    journal.roster.indexes[position].watch,
                );
                let restored = record_preferences_result(&mut journal.indexes[position], restored);
                journal.save(root)?;
                if let Err(error) = restored
                    && first_restore_failure.is_none()
                {
                    first_restore_failure = Some(error);
                }
            }
            if let Some(error) = first_restore_failure {
                return Err(error);
            }
            Ok(journal
                .indexes
                .iter()
                .all(|index| index.preferences_restored && index.completed.is_some()))
        })?;
    if !indexes_complete {
        return Ok(());
    }
    let registrations = journal
        .roster
        .indexes
        .iter()
        .map(|index| index.registration.clone())
        .collect::<Vec<_>>();
    match clean_break_cleanup::cleanup(root, &journal.old_control, &registrations) {
        Ok(observation) => {
            journal.cleanup_complete = observation
                .get("complete")
                .and_then(serde_json::Value::as_bool)
                .context("clean-break cleanup omitted complete outcome")?;
            journal.cleanup_failure = None;
            journal.cleanup_observation = Some(observation);
            journal.save(root)?;
        }
        Err(error) => {
            journal.cleanup_failure = Some(RebuildFailure {
                code: "clean_break_cleanup_blocked".into(),
                message: bounded_message(&format!("{error:#}")),
            });
            journal.save(root)?;
            return Err(error);
        }
    }
    drop(lock);
    Ok(())
}

fn record_preferences_result(progress: &mut IndexProgress, restored: Result<()>) -> Result<()> {
    ensure!(
        progress
            .completed
            .as_ref()
            .is_some_and(|job| progress.job == Some(job.id) && job.state == JobState::Succeeded),
        "clean-break preferences require the exact successful accepted job"
    );
    match restored {
        Err(error) => {
            progress.failure = Some(RebuildFailure {
                code: "clean_break_preference_restore_blocked".into(),
                message: bounded_message(&format!("{error:#}")),
            });
            Err(error)
        }
        Ok(()) => {
            if progress
                .failure
                .as_ref()
                .is_some_and(|failure| failure.code == "clean_break_preference_restore_blocked")
            {
                progress.failure = None;
            }
            progress.preferences_restored = true;
            Ok(())
        }
    }
}

fn record_completed_job(progress: &mut IndexProgress, job: JobRecord) -> Result<()> {
    ensure!(
        progress.job == Some(job.id) && job.state == JobState::Succeeded,
        "clean-break completion must match successful accepted job"
    );
    if progress
        .failure
        .as_ref()
        .is_some_and(|failure| failure.code == "clean_break_index_receipt_unavailable")
    {
        progress.failure = None;
    }
    progress.completed = Some(job);
    Ok(())
}

pub(in crate::runtime_install) fn continuation_pending(
    root: &Path,
    control: &Path,
) -> Result<bool> {
    Ok(read(root)?.is_some_and(|journal| {
        journal.clean_break && !journal.cleanup_complete && journal.control() == control
    }))
}

pub(in crate::runtime_install) fn client_registrations(
    root: &Path,
) -> Result<Option<Vec<ProjectRegistration>>> {
    Ok(client_capture_journal(root)?.and_then(|journal| journal.client_registrations))
}

fn client_capture_journal(root: &Path) -> Result<Option<Journal>> {
    let Some(journal) = read(root)? else {
        return Ok(None);
    };
    let selected = selection_record(root)?;
    if !journal.clean_break
        || !(selected.active == journal.candidate
            || journal.phase != Phase::Complete && selected.active == journal.prior.active)
        || journal.cleanup_complete && state_contract::required(root)?
    {
        return Ok(None);
    }
    Ok(Some(journal))
}

pub(in crate::runtime_install) fn client_metadata(
    root: &Path,
) -> Result<Option<serde_json::Value>> {
    let Some(journal) = client_capture_journal(root)? else {
        return Ok(None);
    };
    Ok(journal.client_metadata.map(|mut metadata| {
        metadata["operation_id"] = serde_json::json!(journal.operation);
        metadata
    }))
}

pub(in crate::runtime_install) fn http_snapshot_report(
    root: &Path,
) -> Result<Option<airk_core::service_http_cleanup::PredecessorHttpSnapshot>> {
    Ok(client_capture_journal(root)?.and_then(|journal| journal.http_ownership_snapshot))
}

pub(in crate::runtime_install) fn predecessor_http_snapshot(
    root: &Path,
    predecessor: &str,
) -> Result<
    Option<(
        Incarnation,
        airk_core::service_http_cleanup::PredecessorHttpSnapshot,
    )>,
> {
    let Some(journal) = client_capture_journal(root)? else {
        return Ok(None);
    };
    if journal.prior.active != predecessor {
        return Ok(None);
    }
    Ok(journal
        .http_ownership_snapshot
        .map(|snapshot| (journal.operation, snapshot)))
}

pub(in crate::runtime_install) fn require_upgrade_allowed(
    root: &Path,
    candidate: &str,
) -> Result<()> {
    if let Some(journal) = read(root)? {
        ensure!(
            !journal.clean_break || journal.cleanup_complete || journal.candidate == candidate,
            "clean_break_cleanup_pending: finish the initial rebuild and owned-artifact cleanup before changing runtimes"
        );
    }
    Ok(())
}

pub(in crate::runtime_install) fn preserved_runtime(root: &Path, identity: &str) -> Result<bool> {
    Ok(preserved_runtime_ids(root)?
        .iter()
        .any(|saved| saved == identity))
}

pub(in crate::runtime_install) fn preserved_runtime_ids(root: &Path) -> Result<Vec<String>> {
    let Some(journal) = read(root)? else {
        return Ok(Vec::new());
    };
    if journal.clean_break {
        return Ok(Vec::new());
    }
    let mut preserved = vec![journal.prior.active];
    if let Some(previous) = journal.prior.previous {
        preserved.push(previous);
    }
    preserved.sort();
    preserved.dedup();
    Ok(preserved)
}

pub(in crate::runtime_install) fn require_retirement_allowed(root: &Path) -> Result<()> {
    if let Some(journal) = read(root)? {
        let selected = selection_record(root)?;
        if journal.clean_break {
            ensure!(
                journal.phase == Phase::Complete && selected.active == journal.candidate,
                "clean-break replacement is not selected"
            );
            return Ok(());
        }
        ensure!(
            journal.phase == Phase::Complete
                && selected.active != journal.candidate
                && selected.active != journal.prior.active
                && journal.prior.previous.as_ref() != Some(&selected.active),
            "rebuild recovery evidence is preserved; separate reviewed cleanup is required"
        );
        ensure!(
            crate::install_layout::saved_control(root)? == journal.control(),
            "historical rebuild successor state changed before retirement"
        );
        // Subsequent ordinary upgrades may retire their own predecessor and
        // obsolete launcher. The exact original recovery inventories remain
        // protected by preserved_runtime at roster admission and removal.
    }
    Ok(())
}

pub(in crate::runtime_install) fn successor_default_data(root: &Path) -> Result<Option<PathBuf>> {
    let Some(journal) = read(root)? else {
        return Ok(None);
    };
    if crate::install_layout::saved_control(root)? != journal.control() {
        return Ok(None);
    }
    Ok(Some(
        journal
            .install
            .parent()
            .context("rebuild installation has no storage parent")?
            .join(format!("rebuild-default-{}", journal.operation)),
    ))
}

#[cfg(test)]
pub(in crate::runtime_install) mod tests;
