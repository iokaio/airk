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
    service::{JobKind, JobState, ProjectRegistration, RequestKey},
    service_resources::ResourceConfiguration,
};
use airk_store::service::{
    EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease, registry::ServiceRegistry,
};

#[path = "runtime_rebuild_settings.rs"]
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
            self.candidate != self.prior.active,
            "rebuild requires a distinct candidate"
        );
        ensure!(
            self.state == root.join(format!("rebuild-state-{}", self.operation))
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
        ensure!(
            !self.roster.indexes.is_empty()
                && self.roster.indexes.len() <= 64
                && self.indexes.len() == self.roster.indexes.len(),
            "invalid rebuild roster"
        );
        let mut bindings = std::collections::BTreeSet::new();
        for (position, progress) in self.indexes.iter().enumerate() {
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
                selected == Some(self.prior.clone()) || selected == Some(self.intent().selected()),
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
    let value: Envelope = serde_json::from_slice(&bytes)?;
    ensure!(
        value.sha256
            == Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&value.journal)?).into())
            && serde_json::to_vec(&value)? == bytes,
        "rebuild journal digest or encoding changed"
    );
    value.journal.validate(root)?;
    Ok(Some(value.journal))
}

pub(in crate::runtime_install) fn pending(root: &Path) -> Result<bool> {
    Ok(read(root)?.is_some_and(|value| value.phase != Phase::Complete))
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
    digest(candidate)?;
    digest(expected)?;
    if let Some(saved) = read(root)? {
        ensure!(
            saved.candidate == candidate && saved.prior.active == expected,
            "another rebuild already owns this installation; inspect its receipt"
        );
        if saved.phase == Phase::Complete {
            ensure!(
                selection(root)? == Some(saved.intent().selected()),
                "completed rebuild selection has since changed"
            );
            return Ok(saved.intent().selected());
        }
        return resume(root).await;
    }
    managed::require_unmanaged(root)?;
    require_settled(root)?;
    lifecycle::require_settled(root)?;
    lifecycle::require_start_allowed(root)?;
    let prior = selection(root)?.context("rebuild requires an installed predecessor")?;
    ensure!(
        prior.active == expected && candidate != expected,
        "rebuild selection changed since review"
    );
    require_candidate(root, candidate)?;
    verify_runtime(root, &prior.active)?;
    let old_control = layout::control(root)?;
    let registry = ServiceRegistry::open_saved(&old_control)?;
    ensure!(
        !registry.service_explicitly_stopped()?,
        "rebuild cannot clear User Stop"
    );
    drop(registry);
    let roster = settings::capture(&old_control)?;
    ensure!(
        airk_store::service::registry::checkpoint::snapshot(&old_control)?
            .unfinished
            .is_empty(),
        "rebuild cannot discard unresolved accepted operations"
    );
    let operation = airk_kernel::service_broker::new_instance_id()?;
    let journal = Journal {
        schema: 1,
        operation,
        install: root.canonicalize()?,
        old_control: old_control.clone(),
        state: root.join(format!("rebuild-state-{operation}")),
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
            })
            .collect(),
        roster,
        resources: crate::service_resource_policy::load(&old_control)?,
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

pub(in crate::runtime_install) async fn resume(root: &Path) -> Result<Selection> {
    let mut journal = read(root)?.context("accepted rebuild missing")?;
    if journal.phase == Phase::Complete {
        return Ok(journal.intent().selected());
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

async fn advance(root: &Path, journal: &mut Journal) -> Result<Selection> {
    journal.validate(root)?;
    require_candidate(root, &journal.candidate)?;
    require_issuer_rotation_settled(root)?;
    lifecycle::require_settled(root)?;
    lifecycle::require_start_allowed(root)?;
    let owner = crate::service_manager::ServiceManager::for_runtime(root)?;
    ensure!(
        owner.owns_restart()? == journal.manager,
        "rebuild startup ownership changed"
    );
    if journal.phase == Phase::Accepted {
        if !layout::successor_handoff_started(root, journal.operation, journal.layout_sha256)? {
            ensure!(
                layout::saved_control(root)? == journal.old_control,
                "rebuild original control changed"
            );
            require_issuer_rotation_settled(root)?;
            payload_repair::require_settled(root)?;
            launcher_repair::require_settled(root)?;
            if journal.manager {
                owner.stop()?;
            }
            upgrade_stop::stop(root, &journal.prior.active)?;
            let (main, approval) = verify_runtime(root, &journal.prior.active)?;
            drain(root, &main, &approval).await?;
        }
        owner.require_stopped_for_maintenance()?;
        let exclusion = OldExclusion::acquire(journal)?;
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
        let mut preserved = layout::retained_private_paths(root)?;
        preserved.sort();
        preserved.dedup();
        registry.preserve_private_paths(&_service, &preserved)?;
        for position in 0..journal.roster.indexes.len() {
            prepare_index(root, journal, position, &mut registry)?;
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
        replace_json(root, "selected.json", &intent.selected())?;
        record(root, &intent, Outcome::Selected)?;
        if exists(&root.join(INTENT))? {
            finish(root)?;
        }
        journal.phase = Phase::Complete;
        journal.failure = None;
        journal.save(root)?;
    }
    Ok(intent.selected())
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

fn replacement_data(root: &Path, operation: Incarnation, position: usize) -> Result<PathBuf> {
    // Registered index data must remain outside installation/control state.
    // Its existing parent is pinned by ordinary native setup; each fresh leaf
    // is owned by that exact durable setup request rather than a second creator.
    Ok(root
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
    let request = journal.indexes[position]
        .request
        .clone()
        .context("rebuild setup missing")?;
    let receipt = registry.index_setup_receipt(request.caller, &request.key)?;
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
        registry.configure_watch(request.registration.binding, saved.watch)?;
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
    registry.pause(request.registration.binding, saved.paused)?;
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
            && layout::saved_control(root)? == journal.control(),
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
        let mut index_failure = registry_unavailable.then(|| RebuildFailure {
            code: "rebuild_registry_unavailable".into(),
            message: "Native rebuild registry metadata is unavailable; inspect the operation-level diagnostic.".into(),
        });
        let job = match (successor.as_ref(), progress.job) {
            (Some(registry), Some(id)) => match registry.job(id) {
                Ok(job) => Some(job),
                Err(error) => {
                    index_failure = Some(RebuildFailure {
                        code: "rebuild_job_unavailable".into(),
                        message: bounded_message(&error.to_string()),
                    });
                    None
                }
            },
            _ => None,
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
            replacement: progress
                .request
                .as_ref()
                .map(|request| request.registration.clone()),
            sources: saved.roots.clone(),
            settings_sha256: saved.settings_digest,
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
    } else if indexes
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
    let Some(root) = layout::installation_for_control(control)? else {
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

pub(in crate::runtime_install) fn preserved_runtime(root: &Path, identity: &str) -> Result<bool> {
    Ok(preserved_runtime_ids(root)?
        .iter()
        .any(|saved| saved == identity))
}

pub(in crate::runtime_install) fn preserved_runtime_ids(root: &Path) -> Result<Vec<String>> {
    let Some(journal) = read(root)? else {
        return Ok(Vec::new());
    };
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
        ensure!(
            journal.phase == Phase::Complete
                && selected.active != journal.candidate
                && selected.active != journal.prior.active
                && journal.prior.previous.as_ref() != Some(&selected.active),
            "rebuild recovery evidence is preserved; separate reviewed cleanup is required"
        );
        ensure!(
            layout::saved_control(root)? == journal.control(),
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
    if layout::saved_control(root)? != journal.control() {
        return Ok(None);
    }
    Ok(Some(
        root.parent()
            .context("rebuild installation has no storage parent")?
            .join(format!("rebuild-default-{}", journal.operation)),
    ))
}

#[cfg(test)]
#[path = "runtime_rebuild_tests.rs"]
pub(in crate::runtime_install) mod tests;
