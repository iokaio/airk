// SPDX-License-Identifier: Apache-2.0
//! Fresh-state redb registry metadata.
use super::{MAX_PROJECTS, ProjectLease};
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use airk_core::service::{
    JobError, JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, ProjectPage,
    ProjectRegistration, RegisteredProject, RequestKey,
};
use airk_core::service_watch::{WatchBackend, WatchFailure, WatchObservation};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};

const FILE: &str = "service-1.4.airk";
const KIND: &str = "airk-service-registry/1";
const CAPACITY: u64 = crate::ledger::MAX_FILE_BYTES;
const STATE_BYTES: usize = 32 << 20;
#[path = "service_registry_native_lifecycle.rs"]
mod lifecycle;
#[path = "service_registry_native_background.rs"]
mod native_background;
#[path = "service_registry_native_configuration.rs"]
mod native_configuration;
#[path = "service_registry_native_events.rs"]
mod native_events;
#[path = "service_registry_native_jobs.rs"]
mod native_jobs;
#[path = "service_registry_native_maintenance.rs"]
mod native_maintenance;
#[path = "service_registry_native_watch.rs"]
mod native_watch;
#[path = "service_registry_native_observations.rs"]
mod observations;
#[path = "service_registry_native_providers.rs"]
mod providers;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    registration: ProjectRegistration,
    epoch: u64,
    paused: bool,
    registered: bool,
    cleanup_guard: Option<Sha256Digest>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    incarnation: Incarnation,
    request_namespace: Incarnation,
    revision: u64,
    request_epoch: u64,
    next_job_sequence: u64,
    projects: BTreeMap<String, Registration>,
    jobs: BTreeMap<String, NativeJob>,
    accepted_requests: BTreeMap<String, String>,
    retry_origins: BTreeMap<String, Sha256Digest>,
    private_paths: std::collections::BTreeSet<String>,
    #[serde(default)]
    watches: BTreeMap<String, NativeWatch>,
    lifecycle: lifecycle::LifecycleState,
    providers: BTreeMap<String, super::ProviderDrain>,
    progress: BTreeMap<String, airk_core::service_progress::ProgressObservation>,
    inspections: BTreeMap<String, airk_core::service_inspection::InspectionObservation>,
    background: BTreeMap<String, native_background::Background>,
    source_outcomes: BTreeMap<String, (u64, JobRecord)>,
    events: native_events::Events,
    maintenance: native_maintenance::MaintenanceState,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeJob {
    sequence: u64,
    caller: Incarnation,
    key: RequestKey,
    record: JobRecord,
    cancellation_requested: bool,
    source_revision: Option<u64>,
    configuration: Option<native_configuration::Configuration>,
    plan: Option<native_maintenance::Link>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeWatch {
    enabled: bool,
    requested: u64,
    completed: u64,
    configuration_epoch: u64,
    roots: Vec<String>,
    roots_observed: bool,
    native_roots: u16,
    backend: WatchBackend,
    failure: Option<WatchFailure>,
    observed_ms: Option<u64>,
    last_reconciled_ms: Option<u64>,
    attempts: u8,
    next_ms: u64,
    blocked_revision: Option<u64>,
    last_event: Option<(Incarnation, u64, u64)>,
}
impl Default for NativeWatch {
    fn default() -> Self {
        Self {
            enabled: false,
            requested: 0,
            completed: 0,
            configuration_epoch: 0,
            roots: Vec::new(),
            roots_observed: false,
            native_roots: 0,
            backend: WatchBackend::Disabled,
            failure: None,
            observed_ms: None,
            last_reconciled_ms: None,
            attempts: 0,
            next_ms: 0,
            blocked_revision: None,
            last_event: None,
        }
    }
}
impl State {
    fn empty(incarnation: Incarnation) -> Self {
        Self {
            version: 1,
            incarnation,
            request_namespace: incarnation,
            revision: 0,
            request_epoch: 0,
            next_job_sequence: 0,
            projects: BTreeMap::new(),
            jobs: BTreeMap::new(),
            accepted_requests: BTreeMap::new(),
            retry_origins: BTreeMap::new(),
            private_paths: Default::default(),
            watches: BTreeMap::new(),
            lifecycle: lifecycle::LifecycleState::default(),
            providers: BTreeMap::new(),
            progress: BTreeMap::new(),
            inspections: BTreeMap::new(),
            background: BTreeMap::new(),
            source_outcomes: BTreeMap::new(),
            events: native_events::Events::default(),
            maintenance: native_maintenance::MaintenanceState::default(),
        }
    }
    fn validate(&self) -> Result<()> {
        self.lifecycle.validate()?;
        providers::validate(self)?;
        observations::validate(self)?;
        native_background::validate(self)?;
        self.events.validate()?;
        native_configuration::validate(self)?;
        self.maintenance.validate(self)?;
        ensure!(
            self.private_paths.len() <= 17
                && self
                    .private_paths
                    .iter()
                    .all(|path| !path.is_empty() && path.len() <= 4096),
            "private runtime path inventory exceeds bound"
        );
        ensure!(
            self.version == 1
                && self.projects.len() <= MAX_PROJECTS
                && self.request_namespace != Incarnation::from_bytes([0; 16]),
            "invalid native service registry state"
        );
        for (binding, entry) in &self.projects {
            entry.registration.validate()?;
            ensure!(
                binding == &entry.registration.binding.to_string(),
                "native service registration key mismatch"
            );
        }
        ensure!(
            self.jobs.len() <= 1024
                && self.accepted_requests.len() <= 4096
                && self.watches.len() <= MAX_PROJECTS,
            "native service job/request capacity exceeded"
        );
        ensure!(
            self.accepted_requests
                .values()
                .all(|id| self.jobs.contains_key(id))
                && self
                    .retry_origins
                    .keys()
                    .all(|key| self.accepted_requests.contains_key(key)),
            "request receipt lost its job or retry origin"
        );
        let mut root_bytes = 0usize;
        for (binding, watch) in &self.watches {
            ensure!(
                self.projects.contains_key(binding)
                    && watch.completed <= watch.requested
                    && watch.attempts <= 3
                    && watch.roots.len() <= 16
                    && watch.native_roots <= 17
                    && watch.last_event.is_none_or(|(_, _, sequence)| sequence > 0),
                "invalid native service watch record"
            );
            let mut unique = std::collections::BTreeSet::new();
            for root in &watch.roots {
                ensure!(
                    !root.is_empty()
                        && root.len() <= 4096
                        && !root.chars().any(char::is_control)
                        && unique.insert(root),
                    "invalid native watch root"
                );
                root_bytes = root_bytes.saturating_add(root.len());
            }
        }
        ensure!(
            root_bytes <= 8 * 1024 * 1024,
            "native service watch root budget exceeded"
        );
        for (id, job) in &self.jobs {
            ensure!(
                id == &job.record.id.to_string()
                    && self.projects.contains_key(&job.record.project.to_string())
                    && job.record.attempts <= 3
                    && matches!(
                        (&job.record.state, &job.record.outcome),
                        (JobState::Pending | JobState::Running, None)
                            | (JobState::Succeeded, Some(JobOutcome::Succeeded { .. }))
                            | (JobState::Failed, Some(JobOutcome::Failed { .. }))
                            | (JobState::Canceled, Some(JobOutcome::Canceled))
                    ),
                "invalid native service job record"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= STATE_BYTES,
            "native service registry state exceeds byte budget"
        );
        Ok(())
    }
    fn bump(&mut self) -> Result<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .context("native service registry revision exhausted")?;
        Ok(())
    }
}

/// One immutable, fully validated state per live canonical registry root.
/// Every hit still reopens/checks redb, hashes the exact bounded body, validates
/// the owned file identity and checks the handle's owner/namespace fence.
#[derive(Default)]
struct StateCache {
    entry: Mutex<Option<CachedState>>,
    disabled: bool,
}
struct CachedState {
    digest: Sha256Digest,
    identity: String,
    state: Arc<State>,
}
impl StateCache {
    fn clear(&self) {
        if let Ok(mut entry) = self.entry.lock() {
            *entry = None;
        }
    }
    fn load(&self, identity: &str, bytes: &[u8]) -> Result<Arc<State>> {
        let mut profile = crate::current::kv::profile_start();
        let digest = framed_digest(
            b"airk-native-service-state-cache/1",
            &[KIND.as_bytes(), identity.as_bytes(), bytes],
        );
        {
            let mut entry = self
                .entry
                .lock()
                .map_err(|_| anyhow::anyhow!("service state cache poisoned"))?;
            if let Some(saved) = &*entry
                && saved.digest == digest
                && saved.identity == identity
            {
                crate::current::kv::profile_tick("registry.cache_hit", &mut profile);
                return Ok(saved.state.clone());
            }
            // Drop the old image before decoding a replacement; the cache has
            // no read transaction, object lease or open file to retain.
            *entry = None;
        }
        let state: State = crate::ledger::decode(bytes, STATE_BYTES)?;
        crate::current::kv::profile_tick("registry.cache_decode", &mut profile);
        state.validate()?;
        crate::current::kv::profile_tick("registry.cache_validate", &mut profile);
        let state = Arc::new(state);
        // Larger states remain bounded by STATE_BYTES but need not be retained
        // for this optional memory optimization.
        if !self.disabled && u32::from_le_bytes(bytes[..4].try_into()?) <= 4 << 20 {
            *self
                .entry
                .lock()
                .map_err(|_| anyhow::anyhow!("service state cache poisoned"))? =
                Some(CachedState {
                    digest,
                    identity: identity.to_owned(),
                    state: state.clone(),
                });
        }
        Ok(state)
    }
}
type StateCaches = BTreeMap<PathBuf, Weak<StateCache>>;
fn cache_for_root(caches: &mut StateCaches, root: &Path, maximum: usize) -> Arc<StateCache> {
    caches.retain(|_, cache| cache.strong_count() > 0);
    if let Some(cache) = caches.get(root).and_then(Weak::upgrade) {
        return cache;
    }
    let retained = caches.len() < maximum;
    let cache = Arc::new(StateCache {
        disabled: !retained,
        ..Default::default()
    });
    if retained {
        caches.insert(root.to_owned(), Arc::downgrade(&cache));
    }
    cache
}
fn state_cache(root: &Path) -> Result<Arc<StateCache>> {
    static CACHES: OnceLock<Mutex<StateCaches>> = OnceLock::new();
    let mut caches = CACHES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("service state cache registry poisoned"))?;
    Ok(cache_for_root(&mut caches, root, 64))
}
pub struct NativeServiceRegistry {
    root: PathBuf,
    cache: Arc<StateCache>,
    incarnation: Incarnation,
    request_namespace: Incarnation,
    read_only: bool,
}

const LEGACY_FILES: [&str; 4] = [
    "service-1.4.sqlite3-wal",
    "service-1.4.sqlite3-shm",
    "service-1.4.sqlite3-journal",
    "service-1.4.sqlite3",
];
const RESET_RECEIPT: &str = "service-1.4.fresh-setup.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshSetup {
    version: u32,
    root: PathBuf,
    old_incarnation: Incarnation,
    incarnation: Incarnation,
    files: BTreeMap<String, crate::current::legacy_retirement::Witness>,
}

/// Metadata-only reset authority; ownership identity remains stable while keys rotate.
pub fn fresh_setup_namespaces(root: &Path) -> Result<Option<(Incarnation, Incarnation)>> {
    let root = root.canonicalize()?;
    let path = root.join(RESET_RECEIPT);
    if !path.try_exists()? {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() <= 16384,
        "fresh setup receipt exceeds byte bound"
    );
    let bytes = fs::read(&path)?;
    let value: FreshSetup = serde_json::from_slice(&bytes)?;
    ensure!(
        value.version == 1
            && value.root == root
            && value.old_incarnation != value.incarnation
            && value.files.contains_key("service-1.4.sqlite3")
            && value
                .files
                .keys()
                .all(|name| LEGACY_FILES.contains(&name.as_str()))
            && serde_json::to_vec(&value)? == bytes,
        "foreign fresh setup authority"
    );
    let owner = super::owner(&root, true, value.old_incarnation)?;
    ensure!(
        owner.incarnation == value.old_incarnation,
        "fresh setup owner changed"
    );
    Ok(Some((value.old_incarnation, value.incarnation)))
}
fn saved_request_namespace(root: &Path) -> Result<Incarnation> {
    let saved: State = crate::ledger::read(&path(root), KIND, CAPACITY, STATE_BYTES)?;
    saved.validate()?;
    Ok(saved.request_namespace)
}

fn write_receipt(root: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    use std::io::Write as _;
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&serde_json::to_vec(value)?)?;
    temporary.as_file().sync_all()?;
    temporary.persist(root.join(name))?;
    crate::maintenance::sync_parent(root)
}

/// Removes fixed owned registry images under service, helper and worker exclusion.
/// A durable file-identity receipt permits resuming after any individual removal;
/// replacement files, unknown files and existing redb state are never adopted.
pub fn reset_owned_sqlite(
    root: &Path,
    lease: &crate::service::ServiceLease,
    incarnation: Incarnation,
) -> Result<Vec<PathBuf>> {
    let root = root.canonicalize()?;
    ensure!(
        lease.root() == root,
        "service reset lease belongs to another runtime"
    );
    let _initialization = crate::service::acquire_file(&root, "service-1.4.initialize.lock")?;
    let _registry = crate::service::acquire_file(&root, "service-1.4.registry.lock")?;
    ensure!(!path(&root).try_exists()?, "fresh registry already exists");
    let _watch = crate::service::SourceWatchLease::acquire(&root)?;
    let _embedding = crate::service::EmbeddingLease::acquire(&root)?;
    let mut workers = Vec::new();
    // Worker lock inodes are permanent. The held service lease prevents a new
    // compliant old service from creating/admitting another worker during reset.
    for (count, entry) in fs::read_dir(&root)?.enumerate() {
        ensure!(
            count < 16384,
            "service reset directory exceeds entry budget"
        );
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(binding) = name
            .strip_prefix("worker-1.4-")
            .and_then(|name| name.strip_suffix(".lock"))
        {
            let _: ProjectBinding = binding.parse().context("invalid old worker lock name")?;
            workers.push(crate::service::acquire_file(&root, name)?);
        }
    }
    let owner = super::owner(&root, true, incarnation)?;
    ensure!(
        matches!(owner.format, 1 | 2),
        "unsupported legacy owner receipt"
    );
    let receipt_path = root.join(RESET_RECEIPT);
    let receipt: FreshSetup = match fs::symlink_metadata(&receipt_path) {
        Ok(metadata) => {
            crate::managed_collection::reject_file_aliases(&receipt_path, &metadata)?;
            let _ = crate::current::legacy_retirement::witness(&receipt_path)?;
            ensure!(
                metadata.len() <= 16384,
                "fresh setup receipt exceeds byte budget"
            );
            serde_json::from_slice(&fs::read(&receipt_path)?)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(
                owner.incarnation != incarnation,
                "fresh setup requires a new registry request namespace"
            );
            ensure!(
                root.join("service-1.4.sqlite3").try_exists()?,
                "owned legacy registry is missing"
            );
            let mut files = BTreeMap::new();
            for name in LEGACY_FILES {
                let path = root.join(name);
                match fs::symlink_metadata(&path) {
                    Ok(_) => {
                        files.insert(
                            name.to_owned(),
                            crate::current::legacy_retirement::witness(&path)?,
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let receipt = FreshSetup {
                version: 1,
                root: root.clone(),
                old_incarnation: owner.incarnation,
                incarnation,
                files,
            };
            write_receipt(&root, RESET_RECEIPT, &receipt)?;
            receipt
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        receipt.version == 1
            && receipt.old_incarnation != receipt.incarnation
            && receipt.root == root
            && receipt.incarnation == incarnation
            && receipt.old_incarnation == owner.incarnation
            && receipt.files.contains_key("service-1.4.sqlite3")
            && receipt
                .files
                .keys()
                .all(|name| LEGACY_FILES.contains(&name.as_str())),
        "foreign fresh setup receipt"
    );
    // Validate all surviving images before deleting any. Missing images are the
    // only permitted difference when resuming a partially completed cleanup.
    let mut present = Vec::new();
    for name in LEGACY_FILES {
        let candidate = root.join(name);
        ensure!(
            candidate.is_absolute() && candidate.parent() == Some(root.as_path()),
            "legacy reset path escapes runtime"
        );
        match fs::symlink_metadata(&candidate) {
            Ok(_) => {
                ensure!(
                    receipt.files.get(name)
                        == Some(&crate::current::legacy_retirement::witness(&candidate)?),
                    "legacy registry changed after fresh setup preview"
                );
                present.push(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    for candidate in &present {
        fs::remove_file(candidate)?;
        crate::maintenance::sync_parent(&root)?;
    }
    let root_text = root
        .to_str()
        .context("service runtime root must be UTF-8")?
        .to_owned();
    let mut replacement = super::Owner {
        format: 2,
        root: root_text,
        incarnation: owner.incarnation,
        ready: false,
        digest: None,
    };
    replacement.digest = Some(replacement.checksum());
    write_receipt(&root, "service-1.4.owner.json", &replacement)?;
    // Retain the completed receipt: deleting it would make a crash between the
    // final file removal and namespace initialization indistinguishable.
    drop(workers);
    Ok(present)
}

fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}
fn request_identity(caller: Incarnation, key: &RequestKey) -> String {
    format!("{}:{}:{}:{}", key.namespace, key.epoch, key.nonce, caller)
}
fn private_paths(state: &mut State, runtime: &Path) -> Result<Vec<PathBuf>> {
    for path in super::source_isolation::private_paths(runtime)? {
        state.private_paths.insert(
            path.to_str()
                .context("non-UTF-8 private runtime path")?
                .to_owned(),
        );
    }
    ensure!(
        state.private_paths.len() <= 17,
        "private runtime path inventory exceeds bound"
    );
    Ok(state.private_paths.iter().map(PathBuf::from).collect())
}

fn with_read_cached<R>(
    root: &Path,
    cache: &StateCache,
    inspect: impl FnOnce(&State) -> Result<R>,
) -> Result<R> {
    let mut profile = crate::current::kv::profile_start();
    let outcome = (|| {
        let _guard = crate::service::acquire_file_for(
            root,
            "service-1.4.registry.lock",
            Duration::from_secs(2),
        )?;
        crate::current::kv::profile_tick("registry.read_lock", &mut profile);
        let file = path(root);
        let metadata = fs::symlink_metadata(&file)?;
        crate::managed_collection::reject_file_aliases(&file, &metadata)?;
        let identity = crate::managed_collection::file_identity(&file, &metadata)?;
        let result =
            crate::ledger::read_encoded_with(&file, KIND, CAPACITY, STATE_BYTES, |encoded, _| {
                let state = cache.load(&identity, encoded)?;
                crate::current::kv::profile_tick("registry.read_validated", &mut profile);
                inspect(&state)
            })?;
        let after = fs::symlink_metadata(&file)?;
        crate::managed_collection::reject_file_aliases(&file, &after)?;
        ensure!(
            crate::managed_collection::file_identity(&file, &after)? == identity,
            "service registry file changed during read"
        );
        Ok(result)
    })();
    if outcome.is_err() {
        cache.clear();
    }
    outcome
}
fn with_write<R>(root: &Path, operation: impl FnOnce(&mut State) -> Result<R>) -> Result<R> {
    let _guard = crate::service::acquire_file_for(
        root,
        "service-1.4.registry.lock",
        Duration::from_secs(2),
    )?;
    let quota = crate::quota::Quota::open(root)?;
    crate::ledger::write_root_growing(
        &path(root),
        KIND,
        STATE_BYTES,
        &quota,
        |state: &mut State| {
            let mut profile = crate::current::kv::profile_start();
            state.validate()?;
            crate::current::kv::profile_tick("registry.write_validate_before", &mut profile);
            let prior = native_events::Baseline::capture(state);
            let result = operation(state)?;
            native_events::record_changes(prior, state)?;
            crate::current::kv::profile_tick("registry.write_mutate", &mut profile);
            state.validate()?;
            crate::current::kv::profile_tick("registry.write_validate_after", &mut profile);
            Ok(result)
        },
    )
}

fn receipt_pressure(state: &State) -> bool {
    state.accepted_requests.len() >= 4096 || state.jobs.len() >= 1024
}

fn claimable_job(
    state: &State,
    lease: &ProjectLease,
    selected: Option<Sha256Digest>,
) -> Result<Option<Sha256Digest>> {
    let epoch = lease.epoch.ok_or(JobError::StaleWorker)?;
    let project = state
        .projects
        .get(&lease.binding.to_string())
        .filter(|entry| entry.registered != lease.derived)
        .context(JobError::StaleWorker)?;
    ensure!(project.epoch == epoch, JobError::StaleWorker);
    if state
        .jobs
        .values()
        .any(|job| job.record.project == lease.binding && job.record.state == JobState::Running)
    {
        return Ok(None);
    }
    let paused = project.paused;
    Ok(state
        .jobs
        .values()
        .filter(|job| {
            job.record.project == lease.binding
                && job.record.state == JobState::Pending
                && selected.is_none_or(|id| job.record.id == id)
                && (!paused || native_configuration::runs_while_paused(job))
                && native_configuration::derived(job) == lease.derived
                && native_maintenance::eligible(state, job)
        })
        .min_by_key(|job| job.sequence)
        .map(|job| job.record.id))
}

fn enqueue_record(
    state: &mut State,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    kind: JobKind,
) -> Result<JobRecord> {
    enqueue_typed(state, project, caller, key, kind, None)
}
fn enqueue_typed(
    state: &mut State,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    kind: JobKind,
    configuration: Option<&airk_core::service_configuration::ConfigurationWork>,
) -> Result<JobRecord> {
    enqueue_request(state, project, caller, key, kind, configuration, None)
}
fn enqueue_request(
    state: &mut State,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    kind: JobKind,
    configuration: Option<&airk_core::service_configuration::ConfigurationWork>,
    retry: Option<Sha256Digest>,
) -> Result<JobRecord> {
    ensure!(
        key.namespace == state.request_namespace,
        JobError::KeyExpired
    );
    ensure!(
        (kind == JobKind::Configure) == configuration.is_some(),
        "configuration jobs require a complete typed request"
    );
    let derived = configuration.is_some_and(|work| work.request.configuration.is_derived_cleanup());
    if let Some(work) = configuration.filter(|_| derived) {
        ensure!(
            work.request.incarnation == state.request_namespace && work.request.settings == project,
            "derived cleanup authority does not match this registry and project"
        );
    }

    let request_id = request_identity(caller, key);
    if let Some(job_id) = state.accepted_requests.get(&request_id) {
        ensure!(
            state.retry_origins.get(&request_id).copied() == retry,
            JobError::KeyConflict
        );
        let existing_job = state
            .jobs
            .get(job_id)
            .context("accepted service request lost its job")?;
        let existing = &existing_job.record;
        ensure!(
            existing_job
                .configuration
                .as_ref()
                .map(|saved| &saved.work.request)
                == configuration.map(|work| &work.request),
            JobError::KeyConflict
        );
        ensure!(
            existing.project == project && existing.kind == kind,
            JobError::KeyConflict
        );
        return Ok(existing.clone());
    }
    ensure!(key.epoch == state.request_epoch, JobError::KeyExpired);
    ensure!(
        state
            .projects
            .get(&project.to_string())
            .is_some_and(|entry| entry.registered != derived),
        "service project is not registered"
    );
    ensure!(state.accepted_requests.len() < 4096, JobError::Capacity);
    if !matches!(kind, JobKind::Repair | JobKind::PlanInspect)
        && let Some(existing) = state
            .jobs
            .values()
            .filter(|job| {
                job.record.project == project
                    && job.record.kind == kind
                    && job.record.state == JobState::Pending
                    && job.configuration.as_ref().map(|saved| &saved.work) == configuration
            })
            .min_by_key(|job| job.sequence)
    {
        let record = existing.record.clone();
        let id = record.id.to_string();
        state.accepted_requests.insert(request_id, id.clone());
        if let Some(retry) = retry {
            state
                .retry_origins
                .insert(request_identity(caller, key), retry);
        }
        if native_background::source_kind(kind) {
            let revision = state
                .watches
                .entry(project.to_string())
                .or_default()
                .requested;
            let job = state.jobs.get_mut(&id).unwrap();
            job.source_revision = Some(job.source_revision.unwrap_or(0).max(revision));
        }
        state.bump()?;
        return Ok(record);
    }
    ensure!(state.jobs.len() < 1024, JobError::Capacity);
    state.next_job_sequence = state
        .next_job_sequence
        .checked_add(1)
        .context("service job sequence exhausted")?;
    let id = framed_digest(
        b"airk-service-job/1",
        &[
            project.as_bytes(),
            caller.as_bytes(),
            key.namespace.as_bytes(),
            &key.epoch.to_le_bytes(),
            key.nonce.as_bytes(),
        ],
    );
    let record = JobRecord {
        id,
        project,
        kind,
        state: JobState::Pending,
        fencing_generation: 0,
        attempts: 0,
        outcome: None,
    };
    state.jobs.insert(
        id.to_string(),
        NativeJob {
            sequence: state.next_job_sequence,
            caller,
            key: key.clone(),
            record: record.clone(),
            cancellation_requested: false,
            source_revision: if native_background::source_kind(kind) {
                Some(
                    state
                        .watches
                        .entry(project.to_string())
                        .or_default()
                        .requested,
                )
            } else {
                None
            },
            configuration: configuration
                .cloned()
                .map(|work| native_configuration::Configuration { work, result: None }),
            plan: None,
        },
    );
    state.accepted_requests.insert(request_id, id.to_string());
    if let Some(retry) = retry {
        state
            .retry_origins
            .insert(request_identity(caller, key), retry);
    }
    state.bump()?;
    Ok(record)
}

fn watch_project(state: &State, project: ProjectBinding) -> Result<super::WatchProject> {
    let entry = state
        .projects
        .get(&project.to_string())
        .filter(|entry| entry.registered)
        .context("service project is not registered")?;
    let registered = RegisteredProject {
        registration: entry.registration.clone(),
        fencing_generation: entry.epoch,
        paused: entry.paused,
    };

    let watch = state
        .watches
        .get(&project.to_string())
        .cloned()
        .unwrap_or_default();
    let observation = WatchObservation {
        enabled: watch.enabled,
        requested_revision: watch.requested,
        completed_revision: watch.completed,
        automatic_retries_exhausted: watch.enabled
            && watch.requested > watch.completed
            && watch.attempts >= 3,
        automatic_retry_canceled: watch.enabled
            && watch.requested > watch.completed
            && watch
                .blocked_revision
                .is_some_and(|revision| revision >= watch.requested),
        configuration_observed: watch.roots_observed,
        configured_roots: 1 + watch.roots.len() as u16,
        native_roots: watch.native_roots,
        backend: watch.backend,
        failure: watch.failure,
        observed_unix_ms: watch.observed_ms,
        last_reconciled_unix_ms: watch.last_reconciled_ms,
    };
    observation.validate()?;
    Ok(super::WatchProject {
        project: registered,
        configuration_epoch: watch.configuration_epoch,
        additional_roots: watch.roots,
        observation,
    })
}

impl NativeServiceRegistry {
    fn check_identity(&self, state: &State) -> Result<()> {
        let owner = super::owner(&self.root, true, self.incarnation)?;
        ensure!(
            state.incarnation == self.incarnation
                && state.request_namespace == self.request_namespace
                && owner.incarnation == self.incarnation
                && owner.format == 2
                && owner.ready,
            "service registry incarnation changed; reopen the fresh namespace"
        );
        Ok(())
    }
    fn read<R>(&self, inspect: impl FnOnce(&State) -> Result<R>) -> Result<R> {
        with_read_cached(&self.root, &self.cache, |state| {
            self.check_identity(state)?;
            inspect(state)
        })
    }
    #[track_caller]
    fn write<R>(&self, operation: impl FnOnce(&mut State) -> Result<R>) -> Result<R> {
        let caller = std::panic::Location::caller();
        let mut profile = crate::current::kv::profile_start();
        ensure!(!self.read_only, "saved service registry is read-only");
        let outcome = with_write(&self.root, |state| {
            self.check_identity(state)?;
            operation(state)
        });
        if outcome.is_err() {
            self.cache.clear();
        }
        if profile.is_some() {
            let component = Path::new(caller.file())
                .file_stem()
                .unwrap()
                .to_string_lossy();
            crate::current::kv::profile_tick(
                &format!("registry.write.{component}.{}", caller.line()),
                &mut profile,
            );
        }
        outcome
    }

    /// Opens only the fresh redb namespace. Legacy state requires an ownership
    /// checked reset, fresh project registration and complete source indexing.
    pub fn open(root: &Path, incarnation: Incarnation) -> Result<Self> {
        let root = root.canonicalize()?;
        let _initialization = crate::service::acquire_file_for(
            &root,
            "service-1.4.initialize.lock",
            Duration::from_secs(2),
        )?;
        let _registry = crate::service::acquire_file_for(
            &root,
            "service-1.4.registry.lock",
            Duration::from_secs(2),
        )?;
        let file = path(&root);
        let legacy = root.join("service-1.4.sqlite3");
        ensure!(
            !legacy.try_exists()?,
            "existing registry requires ownership-checked fresh setup and full indexing; stale agent bindings must be replaced"
        );
        let owner = super::owner(&root, file.try_exists()?, incarnation)?;
        ensure!(
            owner.format == 2,
            "native service registry owner format mismatch"
        );
        let registry_incarnation = owner.incarnation;
        if file.try_exists()? {
            let quota = crate::quota::Quota::open(&root)?;
            crate::ledger::write_root_growing(
                &file,
                KIND,
                STATE_BYTES,
                &quota,
                |saved: &mut State| {
                    saved.validate()?;
                    ensure!(
                        saved.incarnation == owner.incarnation,
                        "native service registry owner differs from ledger"
                    );
                    Ok(())
                },
            )?;
        } else {
            ensure!(!owner.ready, "completed native service registry is missing");
            let quota = crate::quota::Quota::open(&root)?;
            crate::ledger::create_root_growing(&file, KIND, STATE_BYTES, &quota, {
                let mut state = State::empty(owner.incarnation);
                if let Some((old, new)) = fresh_setup_namespaces(&root)? {
                    ensure!(old == owner.incarnation, "fresh setup ownership changed");
                    state.request_namespace = new;
                    let seed = framed_digest(b"native-registry-revision-seed/1", &[new.as_bytes()]);
                    state.revision =
                        u64::from_be_bytes(seed.as_bytes()[..8].try_into()?) & ((1u64 << 62) - 1);
                }
                state
            })?;
        }
        if !owner.ready {
            owner.complete(&root)?;
        }
        let request_namespace = saved_request_namespace(&root)?;
        let cache = state_cache(&root)?;
        Ok(Self {
            root,
            cache,
            incarnation: registry_incarnation,
            request_namespace,
            read_only: false,
        })
    }

    /// Read-only saved-state open uses the same scoped file lock as mutations;
    /// it never opens a custom-backend writer concurrently.
    pub fn open_saved(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let owner = super::owner(&root, true, Incarnation::from_bytes([0; 16]))?;
        ensure!(
            owner.ready && owner.format == 2,
            "native registry is incomplete"
        );
        let cache = state_cache(&root)?;
        let (incarnation, request_namespace) = with_read_cached(&root, &cache, |state| {
            ensure!(
                state.incarnation == owner.incarnation,
                "native service registry ownership mismatch"
            );
            Ok((state.incarnation, state.request_namespace))
        })?;
        Ok(Self {
            root,
            cache,
            incarnation,
            request_namespace,
            read_only: true,
        })
    }

    pub fn incarnation(&self) -> Incarnation {
        self.incarnation
    }

    pub fn request_epoch(&mut self) -> Result<(Incarnation, u64)> {
        ensure!(!self.read_only, "saved service registry is read-only");
        self.saved_request_epoch()
    }
    fn prepare_job_request(
        &self,
        caller: Incarnation,
        key: &RequestKey,
        protected: Option<Sha256Digest>,
    ) -> Result<()> {
        ensure!(
            key.namespace == self.request_namespace,
            JobError::KeyExpired
        );
        let accepted = self.read(|state| {
            let accepted = state
                .accepted_requests
                .contains_key(&request_identity(caller, key));
            ensure!(
                accepted || key.epoch == state.request_epoch,
                JobError::KeyExpired
            );
            Ok(accepted)
        })?;
        if !accepted && self.prune_job_receipts(protected)? {
            return Err(JobError::KeyExpired.into());
        }
        Ok(())
    }
    fn prune_job_receipts(&self, protected: Option<Sha256Digest>) -> Result<bool> {
        ensure!(!self.read_only, "saved service registry is read-only");
        if !self.read(|state| Ok(receipt_pressure(state)))? {
            return Ok(false);
        }
        self.write(|state| {
            if receipt_pressure(state) {
                let mut retired = state
                    .jobs
                    .values()
                    .filter(|job| {
                        protected != Some(job.record.id)
                            && matches!(
                                job.record.state,
                                JobState::Succeeded | JobState::Failed | JobState::Canceled
                            )
                    })
                    .map(|job| (job.sequence, job.record.id.to_string()))
                    .collect::<Vec<_>>();
                retired.sort_by_key(|(sequence, _)| *sequence);
                ensure!(!retired.is_empty(), JobError::Capacity);
                for (_, id) in retired {
                    state.jobs.remove(&id);
                    state
                        .progress
                        .retain(|_, observation| observation.progress.job.to_string() != id);
                    state
                        .accepted_requests
                        .retain(|_, accepted| accepted != &id);
                    if state.accepted_requests.len() < 3072 && state.jobs.len() < 1024 {
                        break;
                    }
                }
                state.request_epoch = state
                    .request_epoch
                    .checked_add(1)
                    .context("service request epoch exhausted")?;
                state.bump()?;
                state
                    .retry_origins
                    .retain(|key, _| state.accepted_requests.contains_key(key));
                return Ok(true);
            }
            Ok(false)
        })
    }

    pub fn saved_request_epoch(&self) -> Result<(Incarnation, u64)> {
        Ok((
            self.request_namespace,
            self.read(|state| Ok(state.request_epoch))?,
        ))
    }

    pub fn enqueue(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        kind: JobKind,
    ) -> Result<JobRecord> {
        ensure!(!self.read_only, "saved service registry is read-only");
        ensure!(
            !matches!(
                kind,
                JobKind::Configure | JobKind::Repair | JobKind::PlanInspect
            ),
            "native job requires a typed configuration or maintenance payload"
        );
        ensure!(
            key.namespace == self.request_namespace,
            JobError::KeyExpired
        );
        self.prepare_job_request(caller, key, None)?;
        self.write(|state| enqueue_record(state, project, caller, key, kind))
    }

    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {
        self.read(|state| {
            state
                .jobs
                .get(&id.to_string())
                .map(|job| job.record.clone())
                .context(JobError::UnknownJob)
        })
    }

    pub fn pending_projects(&self, limit: usize) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        self.read(|state| {
            let mut jobs = state
                .jobs
                .values()
                .filter(|job| {
                    job.record.state == JobState::Running
                        || job.record.state == JobState::Pending
                            && native_maintenance::eligible(state, job)
                            && state
                                .projects
                                .get(&job.record.project.to_string())
                                .is_some_and(|entry| {
                                    (entry.registered || native_configuration::derived(job))
                                        && (!entry.paused
                                            || native_configuration::runs_while_paused(job))
                                })
                })
                .collect::<Vec<_>>();
            jobs.sort_by_key(|job| job.sequence);
            let mut projects = std::collections::BTreeSet::new();
            Ok(jobs
                .into_iter()
                .filter_map(|job| {
                    projects
                        .insert(job.record.project)
                        .then_some(job.record.project)
                })
                .take(limit)
                .collect())
        })
    }

    pub fn claim_job(&mut self, lease: &ProjectLease) -> Result<Option<JobRecord>> {
        self.claim_selected_job(lease, None)
    }
    pub fn claim_job_id(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
    ) -> Result<Option<JobRecord>> {
        self.claim_selected_job(lease, Some(id))
    }
    fn claim_selected_job(
        &mut self,
        lease: &ProjectLease,
        selected: Option<Sha256Digest>,
    ) -> Result<Option<JobRecord>> {
        ensure!(!self.read_only, "saved service registry is read-only");
        ensure!(lease.root == self.root, JobError::StaleWorker);
        let epoch = lease.epoch.ok_or(JobError::StaleWorker)?;
        // This observation has no effects. A later arrival is picked up by the next
        // poll; the write path rechecks the same fencing and eligibility predicate.
        if self.read(|state| Ok(claimable_job(state, lease, selected)?.is_none()))? {
            return Ok(None);
        }
        self.write(|state| {
            let Some(id) = claimable_job(state, lease, selected)? else {
                return Ok(None);
            };
            let record = &mut state.jobs.get_mut(&id.to_string()).unwrap().record;
            ensure!(record.attempts < 3, JobError::Capacity);
            record.state = JobState::Running;
            record.fencing_generation = epoch;
            record.attempts += 1;
            let result = record.clone();
            if native_background::source_kind(result.kind) {
                native_background::set(state, lease.binding, JobKind::Index, false)?;
            }
            native_maintenance::claimed(state, id)?;
            state.bump()?;
            Ok(Some(result))
        })
    }

    pub fn complete_job(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, None, None)
    }
    pub fn complete_job_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), None)
    }
    pub fn complete_job_with_maintenance_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), maintenance)
    }
    fn complete_job_observed(
        &self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: Option<u64>,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
    ) -> Result<JobRecord> {
        if let JobOutcome::Failed { code } = &outcome {
            ensure!(
                !code.is_empty()
                    && code.len() <= 128
                    && code.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                "job failures require a bounded content-free code"
            );
        }
        ensure!(
            serde_json::to_vec(&outcome)?.len() <= 2048,
            "service job receipt exceeds bound"
        );
        ensure!(!self.read_only, "saved service registry is read-only");
        ensure!(lease.root == self.root, JobError::StaleWorker);
        let epoch = lease.epoch.ok_or(JobError::StaleWorker)?;
        self.write(|state| {
            ensure!(
                state
                    .projects
                    .get(&lease.binding.to_string())
                    .is_some_and(|project| project.epoch == epoch),
                JobError::StaleWorker
            );
            let job = state
                .jobs
                .get_mut(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == lease.binding
                    && job.record.state == JobState::Running
                    && job.record.fencing_generation == epoch,
                JobError::StaleWorker
            );
            job.record.state = match outcome {
                JobOutcome::Succeeded { .. } => JobState::Succeeded,
                JobOutcome::Failed { .. } => JobState::Failed,
                JobOutcome::Canceled => JobState::Canceled,
            };
            job.record.outcome = Some(outcome);
            let result = job.record.clone();
            native_background::completed(state, id, now_ms)?;
            native_configuration::completed(state, id)?;
            native_maintenance::completed(
                state,
                id,
                now_ms.map_or_else(native_maintenance::clock_ms, Ok)?,
                maintenance,
            )?;
            state.bump()?;
            Ok(result)
        })
    }

    pub fn watch(&self, project: ProjectBinding) -> Result<super::WatchProject> {
        self.read(|state| watch_project(state, project))
    }

    pub fn configure_watch(&mut self, project: ProjectBinding, enabled: bool) -> Result<()> {
        self.get(project)?;
        self.write(|state| {
            ensure!(
                state
                    .projects
                    .get(&project.to_string())
                    .is_some_and(|entry| entry.registered),
                "service project is not registered"
            );
            let watch = state.watches.entry(project.to_string()).or_default();
            if watch.enabled != enabled {
                watch.configuration_epoch = watch
                    .configuration_epoch
                    .checked_add(1)
                    .context("watch configuration epoch exhausted")?;
                if enabled {
                    watch.requested = watch
                        .requested
                        .checked_add(1)
                        .context("watch source revision exhausted")?;
                }
                watch.enabled = enabled;
                watch.attempts = 0;
                watch.next_ms = 0;
                watch.blocked_revision = None;
                watch.native_roots = 0;
                watch.backend = if enabled {
                    WatchBackend::PeriodicFallback
                } else {
                    WatchBackend::Disabled
                };
                state.bump()?;
            }
            Ok(())
        })
    }

    pub fn record_watch_roots(
        &mut self,
        lease: &ProjectLease,
        additional: &[String],
    ) -> Result<Sha256Digest> {
        ensure!(lease.root == self.root, JobError::StaleWorker);
        let epoch = lease.epoch.ok_or(JobError::StaleWorker)?;
        let mut roots = additional.to_vec();
        roots.sort();
        let encoded = serde_json::to_vec(&roots)?;
        ensure!(
            encoded.len() <= 96 * 1024 && roots.len() <= 16,
            "watch roots exceed byte or count bound"
        );
        ensure!(
            roots.windows(2).all(|pair| pair[0] != pair[1])
                && roots.iter().all(|root| !root.is_empty()
                    && root.len() <= 4096
                    && !root.chars().any(char::is_control)),
            "invalid or duplicate watch root"
        );
        let digest = framed_digest(
            b"airk-service-watch-roots/1",
            &[lease.binding.as_bytes(), &encoded],
        );
        self.write(|state| {
            ensure!(
                state
                    .projects
                    .get(&lease.binding.to_string())
                    .is_some_and(|entry| entry.registered && entry.epoch == epoch),
                JobError::StaleWorker
            );
            let registration = state
                .projects
                .get(&lease.binding.to_string())
                .unwrap()
                .registration
                .clone();
            let existing = state
                .projects
                .iter()
                .map(|(binding, entry)| {
                    (
                        entry.registration.clone(),
                        state
                            .watches
                            .get(binding)
                            .map_or_else(Vec::new, |watch| watch.roots.clone()),
                    )
                })
                .collect::<Vec<_>>();
            let protected = private_paths(state, &self.root)?;
            super::source_isolation::validate_native(
                &self.root,
                &registration,
                &roots,
                &existing,
                &protected,
            )?;
            let used = state
                .watches
                .values()
                .flat_map(|watch| watch.roots.iter())
                .map(String::len)
                .sum::<usize>();
            let watch = state.watches.entry(lease.binding.to_string()).or_default();
            let previous = watch.roots.iter().map(String::len).sum::<usize>();
            ensure!(
                used.saturating_sub(previous)
                    .saturating_add(roots.iter().map(String::len).sum::<usize>())
                    <= 8 * 1024 * 1024,
                "global watch root metadata budget exhausted"
            );
            if watch.roots != roots || !watch.roots_observed {
                watch.roots = roots;
                watch.roots_observed = true;
                watch.configuration_epoch = watch
                    .configuration_epoch
                    .checked_add(1)
                    .context("watch configuration epoch exhausted")?;
                if watch.enabled {
                    watch.requested = watch
                        .requested
                        .checked_add(1)
                        .context("watch source revision exhausted")?;
                }
                watch.attempts = 0;
                watch.next_ms = 0;
                watch.blocked_revision = None;
                watch.native_roots = 0;
                watch.backend = if watch.enabled {
                    WatchBackend::PeriodicFallback
                } else {
                    WatchBackend::Disabled
                };
                watch.failure = None;
                watch.observed_ms = None;
                state.bump()?;
            }
            Ok(())
        })?;
        Ok(digest)
    }

    pub fn source_event(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
        worker: Incarnation,
        sequence: u64,
    ) -> Result<bool> {
        ensure!(sequence > 0, "invalid source event sequence");
        self.write(|state| {
            let Some(watch) = state.watches.get_mut(&project.to_string()) else {
                return Ok(false);
            };
            if !watch.enabled || watch.configuration_epoch != configuration_epoch {
                return Ok(false);
            }
            if watch
                .last_event
                .is_some_and(|(prior_worker, prior_epoch, prior_sequence)| {
                    prior_worker == worker
                        && prior_epoch == configuration_epoch
                        && prior_sequence >= sequence
                })
            {
                return Ok(false);
            }
            watch.requested = watch
                .requested
                .checked_add(1)
                .context("source watcher revision exhausted")?;
            watch.attempts = 0;
            watch.next_ms = 0;
            watch.blocked_revision = None;
            watch.last_event = Some((worker, configuration_epoch, sequence));
            state.bump()?;
            Ok(true)
        })
    }

    pub fn source_changed(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
    ) -> Result<()> {
        self.write(|state| {
            let watch = state
                .watches
                .get_mut(&project.to_string())
                .context("source watcher is not configured")?;
            ensure!(
                watch.enabled && watch.configuration_epoch == configuration_epoch,
                "source watcher configuration changed or is disabled"
            );
            watch.requested = watch
                .requested
                .checked_add(1)
                .context("source watcher revision exhausted")?;
            watch.attempts = 0;
            watch.next_ms = 0;
            watch.blocked_revision = None;
            state.bump()
        })
    }

    pub fn register(&mut self, registration: &ProjectRegistration) -> Result<()> {
        ensure!(!self.read_only, "saved service registry is read-only");
        registration.validate()?;
        crate::service::data_scope::check(
            Path::new(&registration.data_root),
            &self.root,
            self.incarnation,
        )?;
        self.write(|state| {
            ensure!(
                state.incarnation == self.incarnation,
                "registry incarnation changed"
            );
            let existing = state
                .projects
                .iter()
                .map(|(binding, entry)| {
                    (
                        entry.registration.clone(),
                        state
                            .watches
                            .get(binding)
                            .map_or_else(Vec::new, |watch| watch.roots.clone()),
                    )
                })
                .collect::<Vec<_>>();
            let protected = private_paths(state, &self.root)?;
            super::source_isolation::validate_native(
                &self.root,
                registration,
                &[],
                &existing,
                &protected,
            )?;
            let key = registration.binding.to_string();
            let cleanup_pending = state.jobs.values().any(|job| {
                job.record.project == registration.binding
                    && matches!(job.record.state, JobState::Pending | JobState::Running)
                    && native_configuration::derived(job)
            });
            if let Some(prior) = state.projects.get_mut(&key) {
                ensure!(
                    prior.registration == *registration,
                    "service registration binding collision"
                );
                if !prior.registered {
                    ensure!(
                        prior.cleanup_guard.is_none() && !cleanup_pending,
                        "finish interrupted derived cleanup before re-registration"
                    );
                    prior.registered = true;
                    prior.paused = false;
                    state.bump()?;
                }
                return Ok(());
            }
            ensure!(
                state.projects.len() < MAX_PROJECTS,
                "service project registration capacity exhausted"
            );
            ensure!(
                !state.projects.values().any(|entry| {
                    entry.registration.data_root == registration.data_root
                        && entry.registration.collection == registration.collection
                }),
                "collection storage is already registered to another source root"
            );
            state.projects.insert(
                key,
                Registration {
                    registration: registration.clone(),
                    epoch: 0,
                    paused: false,
                    registered: true,
                    cleanup_guard: None,
                },
            );
            state.bump()
        })
    }

    pub fn get(&self, binding: ProjectBinding) -> Result<RegisteredProject> {
        self.read(|state| {
            let entry = state
                .projects
                .get(&binding.to_string())
                .filter(|entry| entry.registered)
                .context("service project is not registered")?;
            ensure!(
                entry.registration.binding == binding,
                "foreign service registration"
            );
            Ok(RegisteredProject {
                registration: entry.registration.clone(),
                fencing_generation: entry.epoch,
                paused: entry.paused,
            })
        })
    }

    pub fn is_registered(&self, binding: ProjectBinding) -> Result<bool> {
        self.read(|state| {
            Ok(state
                .projects
                .get(&binding.to_string())
                .is_some_and(|entry| entry.registered))
        })
    }

    pub fn retained_project(&self, binding: ProjectBinding) -> Result<RegisteredProject> {
        self.read(|state| {
            let entry = state
                .projects
                .get(&binding.to_string())
                .context("retained service project is missing")?;
            Ok(RegisteredProject {
                registration: entry.registration.clone(),
                fencing_generation: entry.epoch,
                paused: entry.paused,
            })
        })
    }

    pub fn bindings(&self) -> Result<Vec<ProjectBinding>> {
        self.read(|state| {
            state
                .projects
                .values()
                .map(|entry| Ok(entry.registration.binding))
                .collect()
        })
    }

    pub fn list(
        &mut self,
        after: Option<ProjectBinding>,
        expected_revision: Option<u64>,
        limit: usize,
    ) -> Result<ProjectPage> {
        self.list_role(after, expected_revision, limit, false)
    }

    pub fn list_inactive(
        &mut self,
        after: Option<ProjectBinding>,
        expected_revision: Option<u64>,
        limit: usize,
    ) -> Result<ProjectPage> {
        self.list_role(after, expected_revision, limit, true)
    }

    fn list_role(
        &self,
        after: Option<ProjectBinding>,
        expected_revision: Option<u64>,
        limit: usize,
        inactive: bool,
    ) -> Result<ProjectPage> {
        ensure!(
            (1..=32).contains(&limit) && (after.is_none() || expected_revision.is_some()),
            "invalid service registry page request"
        );
        self.read(|state| {
            ensure!(
                expected_revision.is_none_or(|expected| expected == state.revision),
                "service registry cursor expired"
            );
            let start = after.map_or(String::new(), |binding| binding.to_string());
            let mut projects = Vec::new();
            let mut bytes = 0usize;
            let mut more = false;
            for (key, entry) in state
                .projects
                .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
            {
                if (inactive && entry.registered) || (!inactive && !entry.registered) {
                    continue;
                }
                let item = RegisteredProject {
                    registration: entry.registration.clone(),
                    fencing_generation: entry.epoch,
                    paused: entry.paused,
                };
                let encoded = serde_json::to_vec(&item)?.len();
                if projects.len() == limit || bytes.saturating_add(encoded) > 48 * 1024 {
                    more = true;
                    break;
                }
                bytes += encoded;
                projects.push(item);
                let _ = key;
            }
            let after = more
                .then(|| projects.last().map(|p| p.registration.binding))
                .flatten();
            ensure!(
                !more || after.is_some(),
                "service registration cannot fit page budget"
            );
            Ok(ProjectPage {
                revision: state.revision,
                projects,
                after,
            })
        })
    }

    pub fn pause(&mut self, binding: ProjectBinding, paused: bool) -> Result<()> {
        ensure!(!self.read_only, "saved service registry is read-only");
        self.write(|state| {
            let entry = state
                .projects
                .get_mut(&binding.to_string())
                .filter(|entry| entry.registered)
                .context("service project is not registered")?;
            if entry.paused != paused {
                entry.paused = paused;
                state.bump()?;
            }
            Ok(())
        })
    }

    pub fn admit_worker(&mut self, lease: &mut ProjectLease) -> Result<u64> {
        self.admit_worker_role(lease, false)
    }
    pub fn admit_derived_worker(&mut self, lease: &mut ProjectLease) -> Result<u64> {
        self.admit_worker_role(lease, true)
    }
    fn admit_worker_role(&mut self, lease: &mut ProjectLease, derived: bool) -> Result<u64> {
        ensure!(!self.read_only, "saved service registry is read-only");
        ensure!(
            lease.root == self.root && lease.epoch.is_none(),
            "foreign or already admitted service worker lease"
        );
        let generation = self.write(|state| {
            let entry = state
                .projects
                .get_mut(&lease.binding.to_string())
                .filter(|entry| entry.registered != derived)
                .context("service worker registration state changed")?;
            entry.epoch = entry
                .epoch
                .checked_add(1)
                .context("service fencing generation exhausted")?;
            let epoch = entry.epoch;
            state.bump()?;
            Ok(epoch)
        })?;
        lease.epoch = Some(generation);
        lease.derived = derived;
        Ok(generation)
    }
}

#[cfg(test)]
#[path = "service_registry_native_flow_tests.rs"]
mod flow_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(source: &Path, data: &Path) -> Result<ProjectRegistration> {
        ProjectRegistration::new(
            source.to_string_lossy().into_owned(),
            data.to_string_lossy().into_owned(),
            "fixture".into(),
        )
    }

    #[test]
    fn cache_capacity_fallback_never_retains_untracked_state() -> Result<()> {
        let mut caches = StateCaches::new();
        let first = cache_for_root(&mut caches, Path::new("first"), 1);
        let state = State::empty(Incarnation::from_bytes([85; 16]));
        let encoded = lz4_flex::compress_prepend_size(&serde_json::to_vec(&state)?);
        first.load("owned-file", &encoded)?;
        assert!(first.entry.lock().unwrap().is_some());
        let existing = cache_for_root(&mut caches, Path::new("first"), 1);
        assert!(Arc::ptr_eq(&first, &existing));
        for _ in 0..3 {
            let fallback = cache_for_root(&mut caches, Path::new("second"), 1);
            assert_eq!(
                fallback.load("owned-file", &encoded)?.request_namespace,
                state.request_namespace
            );
            assert!(
                fallback.entry.lock().unwrap().is_none(),
                "capacity fallback retained an untracked state"
            );
        }
        assert_eq!(caches.len(), 1);
        assert!(
            first.entry.lock().unwrap().is_some(),
            "capacity fallback evicted an existing root"
        );
        drop(first);
        drop(existing);
        let admitted = cache_for_root(&mut caches, Path::new("second"), 1);
        admitted.load("owned-file", &encoded)?;
        assert!(admitted.entry.lock().unwrap().is_some());
        Ok(())
    }
    #[test]
    fn validated_state_cache_checks_exact_bytes_and_namespace_after_timestamp_restoration()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let registry = NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([83; 16]))?;
        let (namespace, epoch) = registry.saved_request_epoch()?;
        assert_eq!(registry.saved_request_epoch()?, (namespace, epoch));
        let file = path(root.path());
        let modified = fs::metadata(&file)?.modified()?;
        let quota = crate::quota::Quota::open(root.path())?;
        crate::ledger::write_root_growing(
            &file,
            KIND,
            STATE_BYTES,
            &quota,
            |state: &mut State| {
                state.request_epoch += 1;
                state.bump()?;
                Ok(())
            },
        )?;
        fs::File::options()
            .write(true)
            .open(&file)?
            .set_times(fs::FileTimes::new().set_modified(modified))?;
        assert_eq!(
            registry.saved_request_epoch()?,
            (namespace, epoch + 1),
            "cache trusted a restored timestamp instead of exact state bytes"
        );
        crate::ledger::write_root_growing(
            &file,
            KIND,
            STATE_BYTES,
            &quota,
            |state: &mut State| {
                state.version = 99;
                Ok(())
            },
        )?;
        assert!(
            registry.saved_request_epoch().is_err(),
            "cached validation hid changed invalid state"
        );
        assert!(registry.saved_request_epoch().is_err());
        crate::ledger::write_root_growing(
            &file,
            KIND,
            STATE_BYTES,
            &quota,
            |state: &mut State| {
                state.version = 1;
                state.request_namespace = Incarnation::from_bytes([84; 16]);
                state.bump()?;
                Ok(())
            },
        )?;
        assert!(
            registry.saved_request_epoch().is_err(),
            "old handle crossed the namespace fence through the cache"
        );
        let fresh = NativeServiceRegistry::open_saved(root.path())?;
        assert_eq!(
            fresh.saved_request_epoch()?,
            (Incarnation::from_bytes([84; 16]), epoch + 1)
        );
        Ok(())
    }
    #[test]
    fn redb_registry_registers_pages_pauses_and_fences_after_reopen() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let data = root.path().with_extension("airk-data");
        let incarnation = Incarnation::from_bytes([47; 16]);
        let mut registry = NativeServiceRegistry::open(root.path(), incarnation)?;
        let project = registration(source.path(), &data)?;
        registry.register(&project)?;
        assert_eq!(registry.get(project.binding)?.registration, project);
        let page = registry.list(None, None, 1)?;
        assert_eq!(page.projects.len(), 1);
        assert!(page.after.is_none());
        registry.pause(project.binding, true)?;
        assert!(registry.get(project.binding)?.paused);
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        assert_eq!(registry.admit_worker(&mut lease)?, 1);
        drop(lease);
        drop(registry);
        let reopened = NativeServiceRegistry::open_saved(root.path())?;
        assert_eq!(reopened.incarnation(), incarnation);
        assert_eq!(
            reopened
                .retained_project(project.binding)?
                .fencing_generation,
            1
        );
        assert!(reopened.get(project.binding)?.paused);
        Ok(())
    }

    #[test]
    fn native_request_epochs_are_stable_and_saved_registry_cannot_mutate() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let data = root.path().with_extension("airk-data");
        let incarnation = Incarnation::from_bytes([49; 16]);
        let mut registry = NativeServiceRegistry::open(root.path(), incarnation)?;
        let project = registration(source.path(), &data)?;
        registry.register(&project)?;
        let (namespace, first_epoch) = registry.request_epoch()?;
        let (_, second_epoch) = registry.request_epoch()?;
        assert_eq!(first_epoch, second_epoch);
        let caller_a = Incarnation::from_bytes([50; 16]);
        let caller_b = Incarnation::from_bytes([51; 16]);
        let key_a = RequestKey {
            namespace,
            epoch: first_epoch,
            nonce: Incarnation::from_bytes([52; 16]),
        };
        let key_b = RequestKey {
            namespace,
            epoch: second_epoch,
            nonce: Incarnation::from_bytes([53; 16]),
        };
        let job_a = registry.enqueue(project.binding, caller_a, &key_a, JobKind::Index)?;
        let job_b = registry.enqueue(project.binding, caller_b, &key_b, JobKind::Index)?;
        assert_eq!(
            job_a.id,
            registry
                .enqueue(project.binding, caller_a, &key_a, JobKind::Index)?
                .id
        );
        assert_eq!(job_a.state, job_b.state);
        drop(registry);
        let mut saved = NativeServiceRegistry::open_saved(root.path())?;
        assert!(saved.pause(project.binding, true).is_err());
        assert!(saved.register(&project).is_err());
        Ok(())
    }

    #[test]
    fn native_watch_roots_and_events_are_fenced_and_deduplicated() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let extra = tempfile::tempdir()?;
        let data = root.path().with_extension("airk-data");
        let incarnation = Incarnation::from_bytes([54; 16]);
        let mut registry = NativeServiceRegistry::open(root.path(), incarnation)?;
        let project = registration(source.path(), &data)?;
        registry.register(&project)?;
        registry.configure_watch(project.binding, true)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let roots = vec![extra.path().to_string_lossy().into_owned()];
        registry.record_watch_roots(&lease, &roots)?;
        let watch = registry.watch(project.binding)?;
        assert!(watch.observation.enabled);
        assert!(watch.observation.configuration_observed);
        assert_eq!(watch.additional_roots, roots);
        assert!(registry.source_event(
            project.binding,
            watch.configuration_epoch,
            incarnation,
            1
        )?);
        assert!(!registry.source_event(
            project.binding,
            watch.configuration_epoch,
            incarnation,
            1
        )?);
        assert_eq!(
            registry
                .watch(project.binding)?
                .observation
                .requested_revision,
            watch.observation.requested_revision + 1
        );
        Ok(())
    }

    #[test]
    fn legacy_registry_refusal_preserves_exact_bytes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let legacy = root.path().join("service-1.4.sqlite3");
        let bytes = b"owned legacy image, never touched";
        fs::write(&legacy, bytes)?;
        assert!(
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([48; 16])).is_err()
        );
        assert_eq!(fs::read(legacy)?, bytes);
        assert!(!path(root.path()).exists());
        Ok(())
    }
    #[cfg(windows)]
    #[test]
    fn canonical_verbatim_control_prefix_cannot_hide_nested_data_or_sources() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([67; 16]))?;
        let nested_data = registration(source.path(), &root.path().join("private-data"))?;
        assert!(registry.register(&nested_data).is_err());
        let nested_source = registration(
            &root.path().join("private-source"),
            &root.path().with_extension("airk-data"),
        )?;
        assert!(registry.register(&nested_source).is_err());
        assert!(registry.bindings()?.is_empty());
        Ok(())
    }

    fn legacy_fixture(root: &Path, incarnation: Incarnation) -> Result<()> {
        let root = root.canonicalize()?;
        let _owner = super::super::owner(&root, false, incarnation)?;
        for name in LEGACY_FILES {
            fs::write(root.join(name), format!("fictional legacy fixture: {name}"))?;
        }
        Ok(())
    }

    #[test]
    fn fresh_setup_preserves_unrelated_files_and_rebinds_empty_namespace() -> Result<()> {
        let root = tempfile::tempdir()?;
        let old = Incarnation::from_bytes([61; 16]);
        let next = Incarnation::from_bytes([62; 16]);
        legacy_fixture(root.path(), old)?;
        fs::write(root.path().join("foreign.sqlite3"), b"unrelated database")?;
        fs::write(
            root.path().join("settings.json"),
            b"saved non-database settings",
        )?;
        let lease = crate::service::ServiceLease::acquire(root.path())?;
        assert!(reset_owned_sqlite(root.path(), &lease, old).is_err());
        assert_eq!(reset_owned_sqlite(root.path(), &lease, next)?.len(), 4);
        assert!(reset_owned_sqlite(root.path(), &lease, next)?.is_empty());
        assert_eq!(
            fs::read(root.path().join("foreign.sqlite3"))?,
            b"unrelated database"
        );
        assert_eq!(
            fs::read(root.path().join("settings.json"))?,
            b"saved non-database settings"
        );
        let registry = NativeServiceRegistry::open(root.path(), next)?;
        assert!(registry.bindings()?.is_empty());
        assert_eq!(registry.incarnation(), old);
        assert_eq!(registry.saved_request_epoch()?.0, next);
        assert!(reset_owned_sqlite(root.path(), &lease, next).is_err());
        Ok(())
    }

    #[test]
    fn fresh_setup_refuses_live_workers_helpers_and_aliased_files_before_deletion() -> Result<()> {
        let root = tempfile::tempdir()?;
        let old = Incarnation::from_bytes([63; 16]);
        let next = Incarnation::from_bytes([64; 16]);
        legacy_fixture(root.path(), old)?;
        let lease = crate::service::ServiceLease::acquire(root.path())?;
        let source = tempfile::tempdir()?;
        let project = registration(source.path(), &root.path().with_extension("airk-data"))?;
        let worker = ProjectLease::acquire(root.path(), project.binding)?;
        assert!(reset_owned_sqlite(root.path(), &lease, next).is_err());
        drop(worker);
        let helper = crate::service::EmbeddingLease::acquire(root.path())?;
        assert!(reset_owned_sqlite(root.path(), &lease, next).is_err());
        drop(helper);
        let watcher = crate::service::SourceWatchLease::acquire(root.path())?;
        assert!(reset_owned_sqlite(root.path(), &lease, next).is_err());
        drop(watcher);
        let main = root.path().join("service-1.4.sqlite3");
        fs::hard_link(&main, root.path().join("unrelated-alias"))?;
        assert!(reset_owned_sqlite(root.path(), &lease, next).is_err());
        assert!(
            LEGACY_FILES
                .iter()
                .all(|name| root.path().join(name).is_file())
        );
        assert!(!root.path().join(RESET_RECEIPT).exists());
        Ok(())
    }

    #[test]
    fn fresh_setup_resumes_partial_deletion_and_refuses_replaced_survivors() -> Result<()> {
        let root = tempfile::tempdir()?;
        let old = Incarnation::from_bytes([65; 16]);
        let next = Incarnation::from_bytes([66; 16]);
        legacy_fixture(root.path(), old)?;
        let canonical = root.path().canonicalize()?;
        let files = LEGACY_FILES
            .iter()
            .map(|name| {
                Ok((
                    (*name).to_owned(),
                    crate::current::legacy_retirement::witness(&canonical.join(name))?,
                ))
            })
            .collect::<Result<_>>()?;
        write_receipt(
            &canonical,
            RESET_RECEIPT,
            &FreshSetup {
                version: 1,
                root: canonical.clone(),
                old_incarnation: old,
                incarnation: next,
                files,
            },
        )?;
        fs::remove_file(canonical.join(LEGACY_FILES[0]))?;
        let lease = crate::service::ServiceLease::acquire(&canonical)?;
        let main = canonical.join("service-1.4.sqlite3");
        fs::write(&main, b"unexpected changed bytes")?;
        assert!(reset_owned_sqlite(&canonical, &lease, next).is_err());
        assert!(canonical.join(LEGACY_FILES[1]).is_file());
        // Missing files are permitted only under the original durable receipt.
        fs::remove_file(main)?;
        assert_eq!(reset_owned_sqlite(&canonical, &lease, next)?.len(), 2);
        NativeServiceRegistry::open(&canonical, next)?;
        Ok(())
    }
}
