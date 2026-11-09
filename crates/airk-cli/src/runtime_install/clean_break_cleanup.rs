// SPDX-License-Identifier: Apache-2.0
//! Retire the exact predecessor's owned storage after replacement indexes succeed.
//! The installer owns admission; no source directory is a deletion operand.
use super::*;
use airk_core::{identity::Incarnation, service::ProjectRegistration};
use airk_store::service::{
    EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease, collection::Collection,
    data_scope, registry::ServiceRegistry,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PLAN: u64 = 512 * 1024;
const MAX_COLLECTIONS: usize = 1024;
const FILES_PER_TURN: usize = 64;
const MAX_CENSUS_ENTRIES: usize = 4096;
const STATE_FILES: &[&str] = &[
    "airk-state-1.4.owner.json",
    "airk-state-1.4.owner.part",
    "airk-state-1.4.lock",
    "airk-control-1.4.owner.json",
    "airk-control-1.4.owner.part",
    "airk-registry-1.4.owner.json",
    "airk-registry-1.4.owner.part",
];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Executables,
    Collections,
    DataRoots,
    Control,
    ControlFinal,
    State,
    Complete,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u32,
    install: PathBuf,
    successor_control: PathBuf,
    old_control: PathBuf,
    namespace: Incarnation,
    registrations: Vec<ProjectRegistration>,
    source_roots: Vec<PathBuf>,
    data_roots: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired_data_roots: Vec<PathBuf>,
    state_files: BTreeMap<PathBuf, String>,
    logs: Option<PathBuf>,
    launchers: Option<serde_json::Value>,
    runtimes: Option<serde_json::Value>,
    endpoints_retired: bool,
    phase: Phase,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    plan: Plan,
    sha256: String,
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn journal_name(old_control: &Path) -> String {
    format!(
        "clean-break-cleanup-{}.json",
        sha(old_control.as_os_str().as_encoded_bytes())
    )
}

fn save(root: &Path, plan: &Plan) -> Result<()> {
    let data = serde_json::to_vec(plan)?;
    let envelope = serde_json::json!({"plan":plan,"sha256":sha(&data)});
    ensure!(
        serde_json::to_vec(&envelope)?.len() as u64 <= MAX_PLAN,
        "clean_break_cleanup_roster_limit"
    );
    replace_json(root, &journal_name(&plan.old_control), &envelope)
}

fn source_separation(target: &Path, roots: &[PathBuf]) -> Result<()> {
    for source in roots {
        ensure!(
            !target.starts_with(source) && !source.starts_with(target),
            "clean_break_cleanup_source_overlap: {}",
            target.display()
        );
    }
    Ok(())
}

fn validate_scope(root: &Path, plan: &Plan) -> Result<()> {
    ensure!(
        serde_json::to_vec(plan)?.len() as u64 <= MAX_PLAN,
        "clean_break_cleanup_roster_limit"
    );
    ensure!(
        plan.schema == 1
            && plan.install == root.canonicalize()?
            && plan.successor_control == layout::saved_control(root)?
            && plan.old_control != plan.successor_control
            && plan
                .old_control
                .file_name()
                .is_some_and(|name| name == "control")
            && plan.old_control.is_absolute()
            && plan.registrations.len() <= MAX_COLLECTIONS,
        "clean_break_cleanup_owner_changed"
    );
    let state = plan
        .old_control
        .parent()
        .context("predecessor state missing")?;
    for registration in &plan.registrations {
        registration.validate()?;
        ensure!(
            plan.source_roots
                .contains(&PathBuf::from(&registration.root)),
            "cleanup omitted original source"
        );
    }
    for path in plan.state_files.keys() {
        ensure!(
            path.parent() == Some(state)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| STATE_FILES.contains(&name)),
            "clean_break_cleanup_state_file_scope_changed"
        );
    }
    if let Some(logs) = &plan.logs {
        ensure!(
            logs.starts_with(state) && logs != state,
            "clean_break_cleanup_log_scope_changed"
        );
    }
    // Resolve each saved source once, retaining its original spelling too.
    let mut sources = plan.source_roots.clone();
    for source in &plan.source_roots {
        sources.push(crate::user_paths::canonical_location(source)?);
    }
    sources.sort();
    sources.dedup();
    let mut retired = BTreeSet::new();
    for data in &plan.retired_data_roots {
        ensure!(
            plan.data_roots.contains(data) && retired.insert(data) && !exists(data)?,
            "clean_break_cleanup_retired_storage_changed: {}",
            data.display()
        );
    }
    for target in std::iter::once(&plan.old_control)
        .chain(plan.logs.iter())
        .chain(plan.data_roots.iter())
    {
        ensure!(
            !plan.successor_control.starts_with(target)
                && !plan.install.starts_with(target)
                && !target.starts_with(plan.install.join("runtimes")),
            "clean_break_cleanup_active_scope"
        );
        source_separation(target, &sources)?;
        if plan.data_roots.contains(target) {
            if !plan.retired_data_roots.contains(target) {
                data_scope::validate_clean_break_storage(
                    target,
                    &plan.old_control,
                    plan.namespace,
                    &plan.registrations,
                )
                .with_context(|| {
                    format!("clean_break_cleanup_storage_scope: {}", target.display())
                })?;
            }
        } else if exists(target)? {
            local_ipc::require_existing_root(target).with_context(|| {
                format!("clean_break_cleanup_private_root: {}", target.display())
            })?;
        }
    }
    Ok(())
}

fn capture(root: &Path, old_control: &Path, selected: &[ProjectRegistration]) -> Result<Plan> {
    let old_control = old_control.canonicalize()?;
    let state = old_control.parent().context("predecessor state missing")?;
    let owner_path = state.join("airk-state-1.4.owner.json");
    let owner: serde_json::Value = serde_json::from_slice(&read_bounded(&owner_path, 32 * 1024)?)?;
    ensure!(
        owner["schema"] == 1
            && owner["ready"] == true
            && owner["install"].as_str().map(Path::new) == Some(root.canonicalize()?.as_path())
            && owner["state"].as_str().map(Path::new) == Some(state),
        "clean_break_cleanup_predecessor_state_unproven"
    );
    // Capture the deletion scope only while all predecessor native writers are
    // excluded. The fresh service owns a different namespace and cannot supply
    // this proof on behalf of an old source/helper/project process.
    let _service = ServiceLease::acquire(&old_control)?;
    let _source = SourceWatchLease::acquire(&old_control)?;
    let _embedding = EmbeddingLease::acquire(&old_control)?;
    let registry = ServiceRegistry::open_saved(&old_control)?;
    let namespace = registry.incarnation()?;
    let bindings = registry.bindings()?;
    let _projects = bindings
        .iter()
        .map(|binding| ProjectLease::acquire(&old_control, *binding))
        .collect::<Result<Vec<_>>>()?;
    let registrations = bindings
        .into_iter()
        .map(|binding| {
            registry
                .retained_project(binding)
                .map(|project| project.registration)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut source_roots = BTreeSet::new();
    for registration in &registrations {
        source_roots.extend(
            airk_store::service::collection::clean_break_retained_source_roots(
                &registry,
                registration,
                namespace,
                &registrations,
            )?
            .into_iter()
            .map(PathBuf::from),
        );
        ensure!(
            serde_json::to_vec(&source_roots)?.len() as u64 <= MAX_PLAN,
            "clean_break_cleanup_source_roster_limit"
        );
    }
    for registration in selected {
        ensure!(
            registrations.iter().any(|old| old == registration),
            "clean_break_cleanup_saved_registration_changed"
        );
    }
    let mut data_roots = BTreeSet::new();
    for registration in &registrations {
        let data = PathBuf::from(&registration.data_root);
        if exists(&data)? {
            data_scope::check(&data, &old_control, namespace).with_context(|| {
                format!("clean_break_cleanup_storage_owner: {}", data.display())
            })?;
            data_roots.insert(data.canonicalize()?);
        }
    }
    // Audit every sibling before mutating a shared data root. Inactive saved
    // registrations are included above; an orphan with no complete saved source
    // selection needs review, not guessed roots from a directory name.
    for data in &data_roots {
        data_scope::validate_clean_break_storage(data, &old_control, namespace, &registrations)
            .with_context(|| format!("clean_break_cleanup_storage_census: {}", data.display()))?;
    }
    let mut state_files = BTreeMap::new();
    for name in STATE_FILES {
        let path = state.join(name);
        if exists(&path)? {
            state_files.insert(path.clone(), sha(&read_bounded(&path, 32 * 1024)?));
        }
    }
    let logs = owner["logs"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| {
            let default = state.join("logs");
            exists(&default).unwrap_or(true).then_some(default)
        })
        .filter(|path| exists(path).unwrap_or(true));
    if let Some(logs) = &logs {
        ensure!(
            logs.starts_with(state) && logs != state,
            "clean_break_cleanup_external_log_scope_unproven"
        );
    }
    let plan = Plan {
        schema: 1,
        install: root.canonicalize()?,
        successor_control: layout::saved_control(root)?,
        old_control,
        namespace,
        registrations,
        source_roots: source_roots.into_iter().collect(),
        data_roots: data_roots.into_iter().collect(),
        retired_data_roots: Vec::new(),
        state_files,
        logs,
        launchers: None,
        runtimes: None,
        endpoints_retired: false,
        phase: Phase::Executables,
    };
    validate_scope(root, &plan)?;
    save(root, &plan)?;
    Ok(plan)
}

/// A bounded no-follow deletion turn in an already proven private control/log
/// directory. The caller retains the durable scope proof through partial cleanup.
fn private_turn(root: &Path, protected: &BTreeSet<PathBuf>) -> Result<bool> {
    if !exists(root)? {
        return Ok(true);
    }
    local_ipc::require_existing_root(root)?;
    #[cfg(unix)]
    let (canonical_root, root_owner) = {
        use std::os::unix::fs::MetadataExt;
        (root.canonicalize()?, fs::symlink_metadata(root)?.uid())
    };
    let mut stack = vec![(root.to_path_buf(), false)];
    let (mut files, mut visited, mut pending, mut queued, mut progressed) =
        (0usize, 0usize, false, 1usize, false);
    while let Some((path, expanded)) = stack.pop() {
        visited += 1;
        // A directory is visited twice for postorder removal; this is still a
        // census of at most MAX_CENSUS_ENTRIES actual filesystem entries.
        ensure!(
            visited <= MAX_CENSUS_ENTRIES * 2,
            "clean_break_cleanup_scan_limit"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "clean_break_cleanup_link_preserved: {}",
            path.display()
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "clean_break_cleanup_redirect_preserved"
            );
        }
        if metadata.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                ensure!(
                    private_descendant_allowed(
                        root_owner,
                        metadata.uid(),
                        metadata.mode(),
                        metadata.file_type().is_symlink(),
                        path.canonicalize()? == canonical_root.join(path.strip_prefix(root)?),
                    ),
                    "clean_break_cleanup_descendant_ownership_changed: {}",
                    path.display()
                );
            }
            #[cfg(not(unix))]
            local_ipc::require_existing_root(&path).with_context(|| {
                format!("clean_break_cleanup_private_descendant: {}", path.display())
            })?;
            if expanded {
                if path != root && fs::read_dir(&path)?.next().is_none() {
                    fs::remove_dir(&path)?;
                    progressed = true;
                }
            } else {
                stack.push((path.clone(), true));
                let entries = fs::read_dir(&path)?.map(|entry| entry.map(|entry| entry.path()));
                if !enqueue_entries(&mut stack, entries, &mut queued)? {
                    pending = true;
                }
            }
        } else {
            safe_file(&path)?;
            if protected.contains(&path) {
                continue;
            }
            if files == FILES_PER_TURN {
                pending = true;
                continue;
            }
            fs::remove_file(path)?;
            files += 1;
            progressed = true;
        }
    }
    scan_outcome(pending, progressed)
}

#[cfg(any(unix, test))]
/// The caller already proved an owner-private root. Ordinary native-created
/// descendants may be readable; ownership, containment and object type remain
/// mandatory rather than inferring authority from their permission bits.
fn private_descendant_allowed(
    root_owner: u32,
    entry_owner: u32,
    mode: u32,
    redirected: bool,
    contained: bool,
) -> bool {
    entry_owner == root_owner && mode & 0o170000 == 0o040000 && !redirected && contained
}

fn scan_outcome(pending: bool, progressed: bool) -> Result<bool> {
    ensure!(
        !pending || progressed,
        "clean_break_cleanup_scan_limit_without_progress"
    );
    Ok(!pending)
}

fn enqueue_entries(
    stack: &mut Vec<(PathBuf, bool)>,
    entries: impl IntoIterator<Item = std::io::Result<PathBuf>>,
    queued: &mut usize,
) -> Result<bool> {
    for entry in entries {
        if *queued >= MAX_CENSUS_ENTRIES {
            return Ok(false);
        }
        stack.push((entry?, false));
        *queued += 1;
    }
    Ok(true)
}

fn retire_data_root(
    data: &Path,
    service: &ServiceLease,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<bool> {
    data_scope::retire_clean_break_storage_turn(data, service, namespace, registrations)
        .with_context(|| format!("clean_break_cleanup_storage_retirement: {}", data.display()))
}

fn observation(plan: &Plan) -> serde_json::Value {
    let complete = plan.phase == Phase::Complete;
    let mut result = serde_json::json!({"complete":complete,"phase":plan.phase,
        "old_control":plan.old_control,"collections_total":plan.registrations.len(),
        "data_roots":plan.data_roots,"retired_data_roots":plan.retired_data_roots,"launchers":plan.launchers,"runtimes":plan.runtimes,"endpoints_retired":plan.endpoints_retired,
        "scope":"owned_predecessor_native_state_indexes_and_executables"});
    if complete {
        result["retired_collections"] = serde_json::json!(
            plan.registrations
                .iter()
                .map(|registration| Path::new(&registration.data_root)
                    .join("collections-1.4")
                    .join(&registration.collection))
                .collect::<Vec<_>>()
        );
        result["retired_data_roots"] = serde_json::json!(plan.data_roots);
    }
    result
}

fn require_retired_executables(root: &Path, plan: &Plan) -> Result<()> {
    legacy_launchers::require_absent(root)?;
    let receipt = plan
        .runtimes
        .as_ref()
        .context("clean break runtime retirement proof missing")?;
    ensure!(
        receipt["complete"] == true && receipt["selected_service_restarted"] == false,
        "clean break runtime retirement proof incomplete"
    );
    for identity in receipt["retired_runtimes"]
        .as_array()
        .context("retired runtime roster missing")?
    {
        let identity = identity
            .as_str()
            .context("invalid retired runtime identity")?;
        digest(identity)?;
        ensure!(
            !exists(&root.join("runtimes").join(identity))?,
            "retired runtime returned before state cleanup"
        );
    }
    Ok(())
}

/// Called under the installation lock only after all replacement jobs succeeded.
/// Returns complete=false for a bounded continuation, never a successful subset.
pub(super) fn cleanup(
    root: &Path,
    old_control: &Path,
    registrations: &[ProjectRegistration],
) -> Result<serde_json::Value> {
    let path = root.join(journal_name(old_control));
    let mut plan = if exists(&path)? {
        let envelope: Envelope = serde_json::from_slice(&read_bounded(&path, MAX_PLAN)?)?;
        ensure!(
            sha(&serde_json::to_vec(&envelope.plan)?) == envelope.sha256
                && envelope.plan.old_control == old_control,
            "clean_break_cleanup_plan_changed"
        );
        envelope.plan
    } else {
        capture(root, old_control, registrations)?
    };
    validate_scope(root, &plan)?;
    if plan.phase == Phase::Executables {
        let expected = selection_record(root)?.active;
        if plan.launchers.is_none() || legacy_launchers::pending(root)? {
            legacy_launchers::retire_locked_recording(root, &expected, &mut |receipt| {
                if let Some(saved) = &plan.launchers {
                    ensure!(
                        saved["processes"]["image_roster"] == receipt["processes"]["image_roster"],
                        "launcher retirement resumed a different image roster"
                    );
                } else {
                    plan.launchers = Some(receipt.clone());
                }
                // Publish the complete proof before its native accepted intent
                // can be removed. Retrying a pending intent keeps this original
                // receipt; it does not replace it with an empty-roster result.
                save(root, &plan)
            })?;
        }
        if plan.runtimes.is_none() || retirement::clean_break_retirement_pending(root)? {
            let receipt =
                retirement::retire_clean_break_locked_recording(root, &expected, &mut |receipt| {
                    if let Some(saved) = &plan.runtimes {
                        ensure!(
                            saved["retired_runtimes"] == receipt["retired_runtimes"]
                                && saved["processes"]["image_roster"]
                                    == receipt["processes"]["image_roster"],
                            "runtime retirement resumed a different immutable roster"
                        );
                    } else {
                        plan.runtimes = Some(receipt.clone());
                    }
                    save(root, &plan)
                })?;
            ensure!(
                receipt["complete"] == true,
                "clean_break_runtime_retirement_incomplete: {receipt}"
            );
        }
        plan.phase = Phase::Collections;
        save(root, &plan)?;
    }
    if plan.phase == Phase::Collections {
        require_retired_executables(root, &plan)?;
        let _service = ServiceLease::acquire(&plan.old_control)?;
        let namespace_owner = data_scope::CleanBreakNamespaceOwner::acquire(
            &_service,
            plan.namespace,
            &plan.registrations,
        )?;
        ensure!(
            airk_store::service::registry::retained_namespace(&plan.old_control)? == plan.namespace,
            "clean_break_cleanup_namespace_changed"
        );
        for registration in &plan.registrations {
            let collection = Path::new(&registration.data_root)
                .join("collections-1.4")
                .join(&registration.collection);
            if !exists(&collection)? {
                continue;
            }
            Collection::purge_for_clean_break_namespace_turn(registration, &namespace_owner)?;
            // One canonical collection turn per metadata-owner tick, including
            // when that turn removes the last files of a small collection.
            return Ok(observation(&plan));
        }
        plan.phase = Phase::DataRoots;
        save(root, &plan)?;
    }
    if plan.phase == Phase::DataRoots {
        let _service = ServiceLease::acquire(&plan.old_control)?;
        ensure!(
            airk_store::service::registry::retained_namespace(&plan.old_control)? == plan.namespace,
            "clean_break_cleanup_namespace_changed"
        );
        if let Some(data) = plan
            .data_roots
            .iter()
            .find(|data| !plan.retired_data_roots.contains(data))
            .cloned()
        {
            if retire_data_root(&data, &_service, plan.namespace, &plan.registrations)? {
                plan.retired_data_roots.push(data);
                save(root, &plan)?;
            }
            // One owning storage turn per metadata tick, including completion.
            return Ok(observation(&plan));
        }
        plan.phase = Phase::Control;
        save(root, &plan)?;
    }
    if plan.phase == Phase::Control {
        let _service = ServiceLease::acquire(&plan.old_control)?;
        let _source = SourceWatchLease::acquire(&plan.old_control)?;
        let _embedding = EmbeddingLease::acquire(&plan.old_control)?;
        if !plan.endpoints_retired {
            if !crate::local_ipc::retire_endpoint_namespace(&_service)? {
                return Ok(observation(&plan));
            }
            plan.endpoints_retired = true;
            save(root, &plan)?;
        }
        let protected = [
            "service-1.4.owner.json",
            "service-1.4.sqlite3",
            "service-1.4.sqlite3-wal",
            "service-1.4.sqlite3-shm",
            "instance-1.4.lock",
            "source-watch-1.4.lock",
            "embedding-1.4.lock",
        ]
        .into_iter()
        .map(|name| plan.old_control.join(name))
        .collect();
        if !private_turn(&plan.old_control, &protected)? {
            return Ok(observation(&plan));
        }
        plan.phase = Phase::ControlFinal;
        save(root, &plan)?;
    }
    if plan.phase == Phase::ControlFinal {
        // The durable plan was saved while the old OS leases were still held.
        // Only after proving all approved old entry points remain absent do we
        // unlink the final registry/lease files. Old installed startup also
        // refuses the now-successor-owned layout; it cannot recreate this state.
        require_retired_executables(root, &plan)?;
        if !private_turn(&plan.old_control, &BTreeSet::new())? {
            return Ok(observation(&plan));
        }
        if exists(&plan.old_control)? {
            fs::remove_dir(&plan.old_control)?;
        }
        plan.phase = Phase::State;
        save(root, &plan)?;
    }
    if plan.phase == Phase::State {
        if let Some(logs) = &plan.logs {
            if !private_turn(logs, &BTreeSet::new())? {
                return Ok(observation(&plan));
            }
            if exists(logs)? {
                fs::remove_dir(logs)?;
            }
        }
        for (path, expected) in &plan.state_files {
            if exists(path)? {
                ensure!(
                    sha(&read_bounded(path, 32 * 1024)?) == *expected,
                    "clean_break_cleanup_state_owner_changed"
                );
                fs::remove_file(path)?;
            }
        }
        let state = plan
            .old_control
            .parent()
            .context("predecessor state missing")?;
        if state != plan.install && exists(state)? {
            fs::remove_dir(state).context("unknown predecessor state entry preserved")?;
        }
        plan.phase = Phase::Complete;
        save(root, &plan)?;
    }
    Ok(observation(&plan))
}

#[cfg(test)]
mod tests;
