// SPDX-License-Identifier: Apache-2.0
//! Native permanent-source enforcement and resumable removal publication.
use crate::Kernel;
use airk_core::{
    catalog_policy::SourcePolicyApplyReport,
    identity::{Incarnation, Sha256Digest},
    source_policy::SourcePolicy,
};
use airk_store::current::{
    ArtifactChange, CurrentRead,
    policy_capsule::{PolicyCapsule, PolicyRemovalPlan},
    postings::Lookup,
    resources::ResourceChange,
};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

pub use airk_store::current::source_policy::SourcePolicyReadUnavailable;

fn require_public_capture_path(path: &std::path::Path) -> Result<()> {
    let absolute = std::path::absolute(path)?;
    ensure!(
        !airk_core::source_policy::retained_agent_edit_path(
            absolute
                .to_str()
                .ok_or(airk_core::scoped_reads::ScopeReadRefused)?
        ),
        airk_core::scoped_reads::ScopeReadRefused
    );
    Ok(())
}

pub(crate) struct PolicyInputEpoch(Arc<AtomicBool>);
impl PolicyInputEpoch {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(!self.0.load(Ordering::Acquire), SourcePolicyReadUnavailable);
        Ok(())
    }
}

type RuntimeMap = std::collections::BTreeMap<PathBuf, Weak<Mutex<PolicyRuntime>>>;
static RUNTIMES: OnceLock<Mutex<RuntimeMap>> = OnceLock::new();

/// Every hosted owner shares the process-wide binding map. Its entries are
/// weak handles inserted whole, so a panic in one owner while it held the map
/// lock (for example while loading a capsule) cannot leave a half-updated
/// entry. Keep every live binding: discarding one would let a later Kernel
/// for the same directory create a second, unfenced runtime. A poisoned
/// runtime itself stays poisoned and fails closed for its own collection.
fn lock_runtimes(runtimes: &Mutex<RuntimeMap>) -> std::sync::MutexGuard<'_, RuntimeMap> {
    runtimes.lock().unwrap_or_else(|poisoned| {
        let mut map = poisoned.into_inner();
        map.retain(|_, runtime| runtime.strong_count() > 0);
        runtimes.clear_poison();
        map
    })
}

/// Forget expired policy bindings under `collection_directory` when its owner
/// exits. A binding still held by another live Kernel in this process stays,
/// so every Kernel for that directory keeps sharing one epoch witness.
pub(crate) fn release_expired_runtimes(collection_directory: &std::path::Path) {
    let Some(runtimes) = RUNTIMES.get() else {
        return;
    };
    let mut prefixes = vec![collection_directory.to_path_buf()];
    prefixes.extend(std::path::absolute(collection_directory).ok());
    prefixes.extend(collection_directory.canonicalize().ok());
    release_expired_under(&mut lock_runtimes(runtimes), &prefixes);
}

fn release_expired_under(map: &mut RuntimeMap, prefixes: &[PathBuf]) {
    map.retain(|directory, runtime| {
        runtime.strong_count() > 0 || !prefixes.iter().any(|prefix| directory.starts_with(prefix))
    });
}

pub(crate) struct PolicyRuntime {
    directory: PathBuf,
    expected: Option<Arc<SourcePolicy>>,
    authoritative: bool,
    capsule: Option<PolicyCapsule>,
    retained_policy_digest: Option<Sha256Digest>,
    required_policy_digest: Option<Sha256Digest>,
    retained_policy_unknown: bool,
    fence: Arc<AtomicBool>,
}

impl PolicyRuntime {
    pub(crate) fn shared_load(directory: PathBuf) -> Result<Arc<Mutex<Self>>> {
        // Independently opened Kernels in one process share the exact same epoch
        // witness. Cross-process native owners additionally require worker drain
        // and a fresh authoritative registry binding before admission.
        let mut existing = std::path::absolute(&directory)?;
        let mut missing = Vec::new();
        let directory = loop {
            match existing.canonicalize() {
                Ok(mut canonical) => {
                    for component in missing.into_iter().rev() {
                        canonical.push(component);
                    }
                    break canonical;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(
                        existing
                            .file_name()
                            .context("policy path has no existing ancestor")?
                            .to_owned(),
                    );
                    ensure!(existing.pop(), "policy path has no existing ancestor");
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mut runtimes = lock_runtimes(RUNTIMES.get_or_init(Default::default));
        if let Some(current) = runtimes.get(&directory).and_then(Weak::upgrade) {
            return Ok(current);
        }
        runtimes.retain(|_, runtime| runtime.strong_count() > 0);
        ensure!(
            runtimes.len() < 4096,
            "source policy runtime binding capacity exceeded"
        );
        let state = Arc::new(Mutex::new(Self::load(directory.clone())?));
        runtimes.insert(directory, Arc::downgrade(&state));
        Ok(state)
    }
    pub(crate) fn load(directory: PathBuf) -> Result<Self> {
        let capsule = PolicyCapsule::load(&directory)?;
        Ok(Self {
            directory,
            expected: capsule
                .as_ref()
                .map(|capsule| Arc::new(capsule.policy.clone())),
            authoritative: false,
            capsule,
            retained_policy_digest: None,
            required_policy_digest: None,
            retained_policy_unknown: false,
            fence: Arc::new(AtomicBool::new(false)),
        })
    }
    fn ready(&self) -> Result<Option<Arc<SourcePolicy>>> {
        match (&self.expected, &self.capsule) {
            (None, None)
                if self.retained_policy_digest.is_none()
                    && self.required_policy_digest.is_none()
                    && !self.retained_policy_unknown =>
            {
                Ok(None)
            }
            (Some(expected), Some(capsule))
                if **expected == capsule.policy
                    && capsule.application.is_some()
                    && self
                        .required_policy_digest
                        .is_none_or(|digest| capsule.policy.identity().ok() == Some(digest))
                    && self
                        .retained_policy_digest
                        .is_none_or(|digest| capsule.policy.identity().ok() == Some(digest)) =>
            {
                Ok(Some(expected.clone()))
            }
            _ => Err(SourcePolicyReadUnavailable.into()),
        }
    }
}

impl Kernel {
    pub(crate) fn source_policy_input_epoch(&self) -> Result<PolicyInputEpoch> {
        let runtime = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        runtime.ready()?;
        Ok(PolicyInputEpoch(runtime.fence.clone()))
    }
    pub(crate) fn observe_retained_source_policy(&self) -> Result<()> {
        let required = self
            .service_collection
            .as_ref()
            .and_then(|collection| collection.required_source_policy());
        let observed = (|| -> Result<_> {
            Ok(if let Some(scope) = self.current_scope()? {
                self.store
                    .current_read(&scope)?
                    .collection_state()?
                    .and_then(|state| state.configuration.source_policy_digest)
            } else {
                None
            })
        })();
        let mut runtime = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        if let Some(required) = required {
            runtime.required_policy_digest = Some(required.digest);
        }
        match observed {
            Ok(Some(marker)) => runtime.retained_policy_digest = Some(marker),
            Ok(None) => (),
            // Preserve diagnostics/recovery construction. Unreadable retained
            // metadata is unknown, never evidence of an empty policy. Native
            // registry absence can separately establish an empty-policy repair.
            // A diagnostic reopen in this process shares the native owner's
            // live binding. A failed retained read cannot erase that authority.
            // Confirmed retained digests still fence conflicting empty policy.
            Err(_) if !runtime.authoritative => runtime.retained_policy_unknown = true,
            Err(_) => (),
        }
        Ok(())
    }
    /// The native worker binds this immediately after construction, before any
    /// evidence, source, rebuild or helper work. The registry supplies authority;
    /// the capsule is only a durable enforcement copy and cannot widen it.
    pub fn with_expected_source_policy(&self, expected: Option<SourcePolicy>) -> Result<Self> {
        if let Some(policy) = &expected {
            self.validate_source_policy_binding(policy)?;
        }
        let mut state = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        state.fence.store(true, Ordering::Release);
        state.fence = Arc::new(AtomicBool::new(false));
        state.expected = expected.map(Arc::new);
        state.authoritative = true;
        if state.expected.is_none() {
            state.retained_policy_unknown = false;
        }
        Ok(self.clone())
    }

    fn validate_source_policy_binding(&self, policy: &SourcePolicy) -> Result<()> {
        policy.validate()?;
        let binding = airk_core::service::ProjectRegistration::new(
            self.root_id(),
            self.data_root
                .to_str()
                .context("non-UTF-8 policy storage")?
                .into(),
            self.collection.clone(),
        )?
        .binding;
        ensure!(
            policy.owner_binding == binding
                && policy.collection == self.collection
                && policy
                    .roots
                    .iter()
                    .any(|root| root.owner && root.canonical_path == self.root_id()),
            "permanent source policy belongs to another native index"
        );
        Ok(())
    }

    pub(crate) fn source_policy_for_work(&self) -> Result<Option<Arc<SourcePolicy>>> {
        self.source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?
            .ready()
    }

    pub(crate) fn source_policy_digest(&self) -> Result<Option<Sha256Digest>> {
        self.source_policy_for_work()?
            .map(|policy| policy.identity())
            .transpose()
    }

    pub(crate) fn require_source_policy_scope(&self) -> Result<()> {
        let state = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        self.require_scoped_policy_binding(state.expected.as_deref())
    }

    pub(crate) fn require_source_policy_source(&self, root: &str, path: &str) -> Result<()> {
        ensure!(
            !airk_core::source_policy::source_root_is_reserved(root)
                && !airk_core::source_policy::source_root_is_reserved(path),
            airk_core::scoped_reads::ScopeReadRefused
        );
        if let Some(policy) = self.source_policy_for_work()? {
            ensure!(
                airk_ingest::permanent_policy::allows_checked(&policy, root, path, false)?,
                SourcePolicyReadUnavailable
            );
            airk_ingest::permanent_policy::qualify_source_access(&policy, root, path)?;
        }
        Ok(())
    }

    pub(crate) fn selected_source_policy(
        &self,
        root: &str,
    ) -> Result<airk_ingest::source_scope::SelectedSourcePolicy> {
        airk_ingest::source_scope::SelectedSourcePolicy::with_source_policy(
            std::path::Path::new(root),
            &self.data_root,
            self.source_policy_for_work()?.as_deref(),
        )
    }

    pub(crate) fn require_source_policy_input(&self, path: &std::path::Path) -> Result<()> {
        self.require_source_policy_capture_path(path)?;
        self.require_source_policy_capture_path(&path.canonicalize()?)
    }

    /// Authorize the exact spelling supplied to protected capture, without
    /// resolving it a second time and changing the path that was authorized.
    pub(crate) fn require_source_policy_capture_path(&self, path: &std::path::Path) -> Result<()> {
        require_public_capture_path(path)?;
        let Some(policy) = self.source_policy_for_work()? else {
            return Ok(());
        };
        let absolute = std::path::absolute(path)?;
        ensure!(
            airk_ingest::permanent_policy::allows_absolute_input_checked(&policy, &absolute)?,
            SourcePolicyReadUnavailable
        );
        Ok(())
    }

    pub(crate) fn require_source_policy_directory_capture_path(
        &self,
        path: &std::path::Path,
    ) -> Result<()> {
        require_public_capture_path(path)?;
        let Some(policy) = self.source_policy_for_work()? else {
            return Ok(());
        };
        let absolute = std::path::absolute(path)?;
        ensure!(
            airk_ingest::permanent_policy::allows_directory_input_checked(&policy, &absolute)?,
            SourcePolicyReadUnavailable
        );
        Ok(())
    }

    pub(crate) fn attach_source_policy_read(&self, read: &Arc<CurrentRead>) -> Result<()> {
        let state = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        let policy = state.ready()?;
        self.require_scoped_policy_binding(policy.as_deref())?;
        read.attach_source_policy_fence(state.fence.clone())?;
        if let Some(policy) = policy {
            let applied = read
                .collection_state()?
                .context(SourcePolicyReadUnavailable)?;
            ensure!(
                applied.configuration.source_policy_digest == Some(policy.identity()?),
                SourcePolicyReadUnavailable
            );
            read.attach_source_policy(
                policy,
                airk_ingest::permanent_policy::allows,
                state.fence.clone(),
            )?;
        }
        Ok(())
    }

    pub fn source_policy_status(&self) -> Result<Option<SourcePolicyApplyReport>> {
        let state = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        if let Some(policy) = state.ready()? {
            ensure!(
                self.policy_observation_has_full_scope(&policy),
                airk_core::scoped_reads::ScopeReadRefused
            );
        }
        Ok(state
            .capsule
            .as_ref()
            .and_then(|capsule| capsule.application.clone()))
    }

    /// Metadata-only observation of the selected index; selectors/source paths
    /// are deliberately absent so status cannot disclose other granted prefixes.
    pub fn source_policy_observation(&self) -> Result<Option<serde_json::Value>> {
        let state = self
            .source_policy
            .lock()
            .map_err(|_| SourcePolicyReadUnavailable)?;
        if self
            .require_scoped_policy_binding(state.expected.as_deref())
            .is_err()
        {
            return Ok(Some(
                serde_json::json!({"state":"scope_unavailable","application":null,"application_omission":{"reason":"scope_unavailable"}}),
            ));
        }
        let Some(policy) = &state.expected else {
            return Ok((state.capsule.is_some() || state.required_policy_digest.is_some() || state.retained_policy_digest.is_some() || state.retained_policy_unknown).then(||serde_json::json!({"state":"authority_mismatch","policy_digest":state.required_policy_digest.or(state.retained_policy_digest),"blocked":true,"removal_pending":true})));
        };
        let matching = state
            .capsule
            .as_ref()
            .filter(|capsule| capsule.policy == **policy);
        let report = matching.and_then(|capsule| capsule.application.as_ref());
        let blocked = report.is_none() || airk_ingest::permanent_policy::restricts(policy);
        let mut observation = serde_json::json!({"policy_digest":policy.identity()?,"policy_epoch":policy.epoch,"operation_id":matching.map(|capsule|capsule.operation_id),"blocked":blocked,"removal_pending":report.is_none()});
        if airk_ingest::permanent_policy::restricts(policy) {
            observation["control_dependencies"] =
                serde_json::to_value(airk_ingest::permanent_policy::control_dependencies())?;
        }
        if self.policy_observation_has_full_scope(policy) {
            observation["application"] = serde_json::to_value(report)?;
        } else {
            observation["application"] = serde_json::Value::Null;
            observation["application_omission"] = serde_json::json!({"reason":"selected_scope","detail":"Index-wide removal and reclamation observations require complete source scope."});
        }
        Ok(Some(observation))
    }

    /// Only an exact registry-admitted Configure job calls this operation. The
    /// caller has drained old workers and closed all ordinary read delivery.
    /// Persist the deny before publication, and never read or change source files.
    pub fn apply_source_policy(
        &self,
        policy: &SourcePolicy,
        operation_id: Incarnation,
    ) -> Result<SourcePolicyApplyReport> {
        self.require_unrestricted_read()?;
        self.validate_source_policy_binding(policy)?;
        crate::service_execution::checkpoint()?;
        let mut selected = self.refresh_service_snapshot()?;
        if let Some(scope) = selected.current_scope()? {
            crate::current::recover_current(
                &selected.store,
                &selected.collection_quota()?,
                &scope,
            )?;
        }
        let writer = selected.store.lock()?;
        let quota = selected.collection_quota()?;
        let reservation = selected.publication_reservation_with_limit(64 << 20)?;
        let mut capsule = {
            let mut state = self
                .source_policy
                .lock()
                .map_err(|_| SourcePolicyReadUnavailable)?;
            if state.authoritative {
                ensure!(
                    state.expected.as_deref() == Some(policy),
                    "policy configuration differs from admitted native authority"
                );
            }
            if let Some(current) = &state.capsule {
                ensure!(
                    current.policy.index_id == policy.index_id
                        && current.policy.epoch <= policy.epoch,
                    "policy update would replace identity or regress its epoch"
                );
                if current.policy.epoch == policy.epoch {
                    ensure!(
                        current.policy == *policy && current.operation_id == operation_id,
                        "policy epoch belongs to another operation"
                    );
                }
            }
            let capsule = match &state.capsule {
                Some(current)
                    if current.policy == *policy && current.operation_id == operation_id =>
                {
                    current.clone()
                }
                _ => PolicyCapsule {
                    schema: 1,
                    policy: policy.clone(),
                    operation_id,
                    application: None,
                    planned_removal: None,
                },
            };
            state.fence.store(true, Ordering::Release);
            state.fence = Arc::new(AtomicBool::new(false));
            state.expected = Some(Arc::new(policy.clone()));
            // Charge metadata before replacing its durable entry. A failed save
            // leaves this process unavailable; registry admission remains denied.
            reservation.charge(airk_core::source_policy::MAX_POLICY_BYTES as u64 + 32768)?;
            if let Some(collection) = &selected.service_collection {
                collection.retain_source_policy_with_quota(policy, &quota)?;
                state.required_policy_digest = Some(policy.identity()?);
            }
            capsule.save(&state.directory)?;
            state.capsule = Some(capsule.clone());
            capsule
        };
        if capsule.application.is_none() {
            for rule in &policy.denies {
                let root = policy
                    .roots
                    .iter()
                    .find(|root| root.id == rule.source_root_id)
                    .context("policy root missing")?;
                airk_ingest::permanent_policy::qualify(
                    policy,
                    &root.canonical_path,
                    &rule.path,
                    rule.kind == airk_core::index_catalog::PathScope::Subtree,
                )?;
            }
            let scope = selected.current_scope()?;
            let mut removed_artifacts = 0;
            let removal_publication = if let Some(scope) = scope {
                let read = selected.store.current_build_read(&scope, &writer)?;
                let state = read
                    .collection_state()?
                    .context("policy removal requires applied collection metadata")?;
                let paths = read.lookup_paths(Lookup::Files, "", None, 100_000)?;
                ensure!(
                    paths.next_after.is_none() && paths.paths.len() as u64 == state.counts.files,
                    "policy removal inventory exceeds its complete proof budget"
                );
                if state.configuration.source_policy_digest == Some(policy.identity()?) {
                    let plan = capsule
                        .planned_removal
                        .as_ref()
                        .context("policy publication has no saved native removal plan")?;
                    let current = crate::current::runtime::revision(read.token()?);
                    ensure!(
                        current.incarnation == plan.base.incarnation
                            && current.generation > plan.base.generation,
                        "policy removal publication does not advance its saved base"
                    );
                    for path in &paths.paths {
                        let source = airk_store::roots::source_location(
                            &selected.root_id(),
                            &state.configuration.roots,
                            path,
                        )?;
                        ensure!(
                            airk_ingest::permanent_policy::allows_checked(
                                policy,
                                &source.root,
                                &source.relative_path,
                                false
                            )?,
                            "policy removal publication still retains denied evidence"
                        );
                    }
                    // A crash can occur after publication and before saving the
                    // application receipt. Reuse its exact planned accounting.
                    removed_artifacts = plan.removed_artifacts;
                    Some(current)
                } else {
                    let base = crate::current::runtime::revision(read.token()?);
                    if let Some(plan) = &capsule.planned_removal {
                        ensure!(
                            plan.base == base,
                            "policy removal base changed before publication"
                        );
                    }
                    let mut changes = Vec::new();
                    let mut removed = std::collections::BTreeSet::new();
                    let mut removed_eligible = 0u64;
                    for path in paths.paths {
                        let source = airk_store::roots::source_location(
                            &selected.root_id(),
                            &state.configuration.roots,
                            &path,
                        )?;
                        if !airk_ingest::permanent_policy::allows_checked(
                            policy,
                            &source.root,
                            &source.relative_path,
                            false,
                        )? {
                            let artifact = read
                                .artifact(&path)?
                                .context("policy removal artifact missing")?;
                            let profile = artifact
                                .retained
                                .as_ref()
                                .and_then(|retained| retained.profile.as_ref())
                                .context("policy removal extraction profile missing")?;
                            removed_eligible += u64::from(
                                state
                                    .configuration
                                    .vector_selection
                                    .decision(&path, profile, &artifact.language)?
                                    .eligible,
                            );
                            removed.insert(path.clone());
                            changes.push(ArtifactChange::Delete(path));
                        }
                    }
                    removed_artifacts = changes.len() as u64;
                    let mut retired_components = Vec::new();
                    if read.resource_reference("analysis")?.is_some() {
                        retired_components.push("analysis".into());
                    }
                    if read.resource_reference("embedding-cache")?.is_some() {
                        retired_components.push("embedding_cache".into());
                    }
                    if let Some(plan) = &capsule.planned_removal {
                        ensure!(
                            plan.removed_artifacts == removed_artifacts
                                && plan.retired_components == retired_components,
                            "policy removal plan changed before publication"
                        );
                    }
                    capsule.planned_removal = Some(PolicyRemovalPlan {
                        base,
                        removed_artifacts,
                        retired_components,
                    });
                    {
                        let mut runtime = self
                            .source_policy
                            .lock()
                            .map_err(|_| SourcePolicyReadUnavailable)?;
                        capsule.save(&runtime.directory)?;
                        runtime.capsule = Some(capsule.clone());
                    }
                    let mut configuration = state.configuration;
                    configuration.source_policy_digest = Some(policy.identity()?);
                    // Imported assertions, pending semantic work and embedding caches
                    // may retain cross-file references. Retire their complete current
                    // components rather than inventing new validity for a subset.
                    let mut resources: Vec<_> = ["analysis", "embedding-cache"]
                        .into_iter()
                        .map(|key| ResourceChange {
                            key: key.into(),
                            reference: None,
                        })
                        .collect();
                    if let Some(change) = crate::current::enrichment::trim_work_resource(
                        &read,
                        &selected.store,
                        &writer,
                        &reservation,
                        &removed,
                        state
                            .counts
                            .vector_eligible
                            .checked_sub(removed_eligible)
                            .context("policy eligible count underflow")?,
                    )? {
                        resources.push(change);
                    }
                    if let Some(change) = crate::current::progress::trim_work_resource(
                        &read,
                        &selected.store,
                        &writer,
                        &reservation,
                        state
                            .counts
                            .files
                            .checked_sub(removed_artifacts)
                            .context("policy file count underflow")?,
                    )? {
                        resources.push(change);
                    }
                    drop(read);
                    let committed = crate::current::CurrentUpdater {
                        store: &selected.store,
                        quota: &quota,
                        scope: &scope,
                        writer: &writer,
                        reservation: &reservation,
                        configuration: Some(&configuration),
                    }
                    .apply_with_resources(&changes, None, &resources)?;
                    Some(crate::current::runtime::revision(committed.commit.token))
                }
            } else {
                None
            };
            capsule.application = Some(SourcePolicyApplyReport {
                observed_unix_ms: crate::current::enrichment::now_unix_ms()?,
                policy_digest: policy.identity()?,
                operation_id,
                no_index: removal_publication.is_none(),
                removal_publication,
                removed_artifacts,
                physical_cleanup_pending: true,
                reclaimed_files: 0,
                reclaimed_bytes: Some(0),
                cleanup_error: None,
                retired_components: capsule
                    .planned_removal
                    .as_ref()
                    .map_or_else(Vec::new, |plan| plan.retired_components.clone()),
            });
            let mut state = self
                .source_policy
                .lock()
                .map_err(|_| SourcePolicyReadUnavailable)?;
            capsule.save(&state.directory)?;
            state.capsule = Some(capsule.clone());
            if !capsule
                .application
                .as_ref()
                .is_some_and(|report| report.no_index)
            {
                state.retained_policy_digest = Some(policy.identity()?);
            }
            state.retained_policy_unknown = false;
        }
        drop(writer);
        // The write phase pinned this slot against replacement. Cleanup below
        // acquires its own selection+writer before dropping its selection to
        // upgrade the object gate. Retaining our write-phase pin would prevent
        // that upgrade forever. Other callers' selections remain untouched.
        drop(selected.service_selection.take());
        let report = capsule
            .application
            .as_mut()
            .expect("saved policy removal receipt");
        if report.physical_cleanup_pending {
            let cleanup = if report.no_index {
                if selected.service_collection.is_some() {
                    selected.retire_service_incarnations().map(|cleanup| {
                        (u64::from(cleanup.removed_files), None, cleanup.pending > 0)
                    })
                } else {
                    Ok((0, Some(0), false))
                }
            } else if selected.service_collection.is_some() {
                selected.maintain_service_collection().map(|cleanup| {
                    (
                        u64::from(cleanup.removed_files),
                        None,
                        cleanup.pending > 0 || cleanup.pending_objects,
                    )
                })
            } else {
                selected.maintain_current().map(|cleanup| {
                    (
                        cleanup.removed_files as u64,
                        Some(cleanup.removed_bytes),
                        cleanup.pending,
                    )
                })
            };
            match cleanup {
                Ok((files, bytes, pending)) => {
                    report.reclaimed_files = report
                        .reclaimed_files
                        .checked_add(files)
                        .context("policy reclaimed-file count overflow")?;
                    report.reclaimed_bytes = report
                        .reclaimed_bytes
                        .zip(bytes)
                        .map(|(previous, bytes)| {
                            previous
                                .checked_add(bytes)
                                .context("policy reclaimed-byte count overflow")
                        })
                        .transpose()?;
                    report.physical_cleanup_pending = pending;
                    report.cleanup_error = None;
                }
                Err(error) => {
                    report.physical_cleanup_pending = true;
                    report.cleanup_error =
                        Some(airk_retrieval::truncate_utf8(&format!("{error:#}"), 1024).0);
                }
            }
            report.observed_unix_ms =
                crate::current::enrichment::now_unix_ms()?.max(report.observed_unix_ms);
            let mut state = self
                .source_policy
                .lock()
                .map_err(|_| SourcePolicyReadUnavailable)?;
            capsule.save(&state.directory)?;
            state.capsule = Some(capsule.clone());
        }
        let report = capsule
            .application
            .context("policy application receipt missing")?;
        report.validate()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        index_catalog::{CatalogRevision, DenyId, IndexId, PathScope, SourceRootId},
        source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourceRoot},
    };

    #[test]
    fn retained_archive_gates_apply_without_permanent_source_policy() -> Result<()> {
        let (_owner, kernel, _unused_policy) = fixture(false)?;
        assert!(kernel.source_policy_for_work()?.is_none());
        let archive = kernel.root().join(".AiRk/agent-edits/v1/old");
        std::fs::create_dir_all(&archive)?;
        let displaced = archive.join("config.json");
        std::fs::write(&displaced, b"private")?;
        assert!(
            kernel
                .require_source_policy_capture_path(&displaced)
                .is_err()
        );
        assert!(
            kernel
                .require_source_policy_directory_capture_path(&archive)
                .is_err()
        );
        assert!(
            kernel
                .require_source_policy_source(archive.to_str().unwrap(), "config.json")
                .is_err()
        );
        assert!(
            kernel
                .require_source_policy_source(
                    &kernel.root_id(),
                    ".airk/agent-edits/v1/old/config.json"
                )
                .is_err()
        );
        assert!(
            kernel
                .require_source_policy_capture_path(&kernel.root().join("public.rs"))
                .is_ok()
        );
        assert_eq!(std::fs::read(&displaced)?, b"private");
        Ok(())
    }

    fn fixture(index: bool) -> Result<(tempfile::TempDir, Kernel, SourcePolicy)> {
        let owner = crate::unlinked_tempdir()?;
        let root = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir_all(root.join("blocked"))?;
        std::fs::create_dir(&data)?;
        std::fs::write(
            root.join("blocked/private.rs"),
            "pub struct PrivateFixture;\n",
        )?;
        std::fs::write(root.join("public.rs"), "pub struct PublicFixture;\n")?;
        let kernel = Kernel::open(&root, Some(data))?;
        kernel.configure_vectors(false)?;
        if index {
            kernel.index()?;
        }
        let source = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let binding = airk_core::service::ProjectRegistration::new(
            kernel.root_id(),
            kernel.data_root.to_str().unwrap().into(),
            kernel.collection.clone(),
        )?
        .binding;
        let policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([1; 16])),
            owner_binding: binding,
            namespace: "collections-1.4".into(),
            collection: kernel.collection.clone(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id: source,
                canonical_path: kernel.root_id(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: "blocked".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([1; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        Ok((owner, kernel, policy))
    }

    fn detached_runtime(directory: &str) -> Arc<Mutex<PolicyRuntime>> {
        Arc::new(Mutex::new(PolicyRuntime {
            directory: directory.into(),
            expected: None,
            authoritative: false,
            capsule: None,
            retained_policy_digest: None,
            required_policy_digest: None,
            retained_policy_unknown: false,
            fence: Arc::new(AtomicBool::new(false)),
        }))
    }

    #[test]
    fn poisoned_runtime_map_keeps_live_bindings_and_release_drops_only_expired() {
        let live = detached_runtime("data/one/incarnations/1");
        let sibling = detached_runtime("data/one-two");
        let map = Mutex::new(RuntimeMap::from([
            ("data/one/incarnations/1".into(), Arc::downgrade(&live)),
            ("data/one/incarnations/2".into(), Weak::new()),
            ("data/one-two".into(), Arc::downgrade(&sibling)),
        ]));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = map.lock().unwrap();
            panic!("owner panicked while holding the shared policy binding map");
        }));
        assert!(panicked.is_err() && map.is_poisoned());
        {
            let recovered = lock_runtimes(&map);
            let binding = recovered[std::path::Path::new("data/one/incarnations/1")]
                .upgrade()
                .expect("live binding survives poison recovery");
            assert!(Arc::ptr_eq(&binding, &live), "no second runtime");
            assert_eq!(recovered.len(), 2);
        }
        assert!(!map.is_poisoned());
        let mut map = map.into_inner().unwrap();
        map.insert("data/one/incarnations/3".into(), Weak::new());
        map.insert("data/two".into(), Weak::new());
        drop(sibling);
        release_expired_under(&mut map, &[PathBuf::from("data/one")]);
        let kept: Vec<_> = map.keys().cloned().collect();
        assert_eq!(
            kept,
            [
                PathBuf::from("data/one/incarnations/1"),
                PathBuf::from("data/one-two"),
                PathBuf::from("data/two"),
            ]
        );
    }

    #[test]
    fn collection_release_forgets_expired_bindings_and_keeps_live_ones() -> Result<()> {
        let data = crate::unlinked_tempdir()?;
        let live_directory = data.path().join("collections-1.4").join("live");
        let gone_directory = data.path().join("collections-1.4").join("gone");
        std::fs::create_dir_all(&live_directory)?;
        std::fs::create_dir_all(&gone_directory)?;
        let live = PolicyRuntime::shared_load(live_directory.clone())?;
        drop(PolicyRuntime::shared_load(gone_directory.clone())?);
        crate::release_collection_caches(data.path(), "gone");
        crate::release_collection_caches(data.path(), "live");
        let again = PolicyRuntime::shared_load(live_directory.clone())?;
        assert!(
            Arc::ptr_eq(&live, &again),
            "a live binding is never replaced"
        );
        let map = lock_runtimes(RUNTIMES.get().context("runtime map initialized")?);
        assert!(!map.contains_key(&gone_directory.canonicalize()?));
        assert!(
            map.get(&live_directory.canonicalize()?)
                .and_then(Weak::upgrade)
                .is_some_and(|runtime| Arc::ptr_eq(&runtime, &live))
        );
        Ok(())
    }

    #[test]
    fn first_policy_admission_fences_independently_opened_kernels_and_retained_readers()
    -> Result<()> {
        let (_owner, kernel, policy) = fixture(true)?;
        let independent = Kernel::open(&kernel.root, Some(kernel.data_root.clone()))?;
        let retained = independent.current_reader()?;
        let before = std::fs::read(kernel.root.join("blocked/private.rs"))?;
        kernel.with_expected_source_policy(Some(policy.clone()))?;
        assert!(
            retained
                .token()
                .unwrap_err()
                .is::<SourcePolicyReadUnavailable>()
        );
        assert!(
            independent
                .list_files("", 10, 8192)
                .err()
                .context("pending policy unexpectedly returned files")?
                .is::<SourcePolicyReadUnavailable>()
        );
        let pending = kernel.status_summary()?;
        assert_eq!(pending["index_state"], "unavailable");
        assert_eq!(pending["source_policy"]["removal_pending"], true);
        assert!(pending["publication"].is_null());
        let operation = Incarnation::from_bytes([8; 16]);
        // Revocation prevents delivery but does not release an independently
        // held request gate. Native admission drains its owners; this library
        // fixture must release its retained reader before publication resumes.
        let blocked = kernel.apply_source_policy(&policy, operation).unwrap_err();
        assert!(
            matches!(
                blocked.downcast_ref::<airk_store::current::CurrentGateContention>(),
                Some(airk_store::current::CurrentGateContention::Readers)
            ),
            "{blocked:#}"
        );
        assert!(retained.token().is_err());
        drop(retained);
        let report = kernel.apply_source_policy(&policy, operation)?;
        assert_eq!(report.removed_artifacts, 1);
        assert_eq!(
            independent.list_files("", 10, 8192)?.paths,
            vec!["public.rs"]
        );
        assert_eq!(
            std::fs::read(kernel.root.join("blocked/private.rs"))?,
            before
        );
        assert_eq!(
            kernel
                .apply_source_policy(&policy, operation)?
                .removal_publication,
            report.removal_publication
        );
        Ok(())
    }

    #[test]
    fn publication_before_receipt_crash_recovers_exact_planned_removal_accounting() -> Result<()> {
        let (_owner, kernel, policy) = fixture(true)?;
        let operation = Incarnation::from_bytes([9; 16]);
        let report = kernel.apply_source_policy(&policy, operation)?;
        let mut runtime = kernel.source_policy.lock().unwrap();
        let mut capsule = runtime.capsule.clone().unwrap();
        assert!(capsule.planned_removal.is_some());
        capsule.application = None;
        // Isolated original fixture fault: reproduce the durable pending
        // capsule left when a process exits after publication before its receipt.
        std::fs::write(
            runtime.directory.join("source-policy.json"),
            serde_json::to_vec(&capsule)?,
        )?;
        runtime.capsule = Some(capsule);
        drop(runtime);
        assert!(kernel.current_reader().is_err());
        let recovered = kernel.apply_source_policy(&policy, operation)?;
        assert_eq!(recovered.removed_artifacts, report.removed_artifacts);
        assert_eq!(recovered.removal_publication, report.removal_publication);
        assert_eq!(kernel.status()?.files, 1);
        Ok(())
    }

    #[test]
    fn unblocked_empty_policy_observation_keeps_pending_and_active_restrictions_distinct()
    -> Result<()> {
        let (_owner, kernel, policy) = fixture(false)?;
        kernel.apply_source_policy(&policy, Incarnation::from_bytes([12; 16]))?;
        assert_eq!(
            kernel.source_policy_observation()?.unwrap()["blocked"],
            true
        );
        let unblocked = policy
            .preview_change(
                policy.identity()?,
                airk_core::source_policy::PolicyChange::Unblock {
                    rule_id: policy.denies[0].rule_id,
                },
            )?
            .after;
        kernel.with_expected_source_policy(Some(unblocked.clone()))?;
        let pending = kernel.source_policy_observation()?.unwrap();
        assert_eq!(pending["blocked"], true);
        assert_eq!(pending["removal_pending"], true);
        kernel.apply_source_policy(&unblocked, Incarnation::from_bytes([13; 16]))?;
        let applied = kernel.source_policy_observation()?.unwrap();
        assert_eq!(applied["blocked"], false);
        assert_eq!(applied["removal_pending"], false);
        assert_eq!(applied["policy_epoch"], serde_json::json!(unblocked.epoch));
        assert!(applied["application"].is_object());
        Ok(())
    }

    #[test]
    fn no_index_policy_and_rebuild_keep_the_same_source_deny() -> Result<()> {
        let (_owner, kernel, policy) = fixture(false)?;
        let report = kernel.apply_source_policy(&policy, Incarnation::from_bytes([10; 16]))?;
        assert!(report.no_index);
        assert_eq!(report.removed_artifacts, 0);
        assert!(report.removal_publication.is_none());
        kernel.index()?;
        assert_eq!(kernel.list_files("", 10, 8192)?.paths, vec!["public.rs"]);
        std::fs::write(
            kernel.root.join("blocked/future.rs"),
            "pub struct FuturePrivate;\n",
        )?;
        std::fs::write(kernel.root.join(".airkignore"), "!blocked/**\n")?;
        kernel.reindex()?;
        let files = kernel.list_files("", 10, 8192)?.paths;
        assert!(!files.iter().any(|path| path.starts_with("blocked/")));
        assert!(
            kernel
                .require_source_policy_input(&kernel.root.join("blocked/private.rs"))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn missing_capsule_after_restart_never_becomes_an_empty_policy() -> Result<()> {
        let (_owner, kernel, policy) = fixture(true)?;
        kernel.apply_source_policy(&policy, Incarnation::from_bytes([11; 16]))?;
        let root = kernel.root.clone();
        let data = kernel.data_root.clone();
        let capsule = kernel
            .source_policy
            .lock()
            .unwrap()
            .directory
            .join("source-policy.json");
        drop(kernel);
        std::fs::remove_file(capsule)?;
        let reopened = Kernel::open(&root, Some(data.clone()))?;
        assert!(
            reopened.index().is_err(),
            "missing enforcement metadata must not re-admit source capture"
        );
        assert!(reopened.reindex().is_err());
        assert!(
            reopened
                .prepare_full_rebuild()
                .unwrap_err()
                .is::<SourcePolicyReadUnavailable>()
        );
        let status = reopened.status_summary()?;
        assert_eq!(status["index_state"], "unavailable");
        assert_eq!(status["error"]["code"], "source_policy_unavailable");
        assert!(reopened.list_files("", 10, 8192).is_err());
        assert!(root.join("blocked/private.rs").is_file());
        let owner = reopened.store.directory().join("current-owner.json");
        let mut obsolete: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner)?)?;
        obsolete["version"] = serde_json::json!(120);
        let bytes = serde_json::to_vec(&obsolete)?;
        std::fs::write(&owner, &bytes)?;
        drop(reopened);
        let diagnostics = Kernel::open_collection_for_diagnostics(&root, Some(data), "default")?;
        assert!(
            diagnostics
                .prepare_full_rebuild()
                .unwrap_err()
                .is::<SourcePolicyReadUnavailable>()
        );
        assert_eq!(
            std::fs::read(&owner)?,
            bytes,
            "unknown policy must not lose its retained marker through reset preparation"
        );
        Ok(())
    }
}
