// SPDX-License-Identifier: Apache-2.0
//! Explicit recovery layout handoff. The old state is preserved, never relocated.
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};

const MAX_TRANSITION: u64 = MAX_LAYOUT * 8;
// Match the canonical Store private-path admission. Recovery does not widen it.
const MAX_PRESERVED_PATHS: usize = 16;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transition {
    schema: u32,
    operation: Incarnation,
    install: PathBuf,
    /// Preserve the original encoding as recovery evidence as well as its value.
    previous_layout: String,
    successor: Layout,
    preserved_private_paths: Vec<PathBuf>,
    digest: Sha256Digest,
}

fn receipt_name(operation: Incarnation) -> String {
    format!("rebuild-layout-{operation}.json")
}

fn successor_state(root: &Path, operation: Incarnation) -> PathBuf {
    root.join(format!("rebuild-state-{operation}"))
}

fn operation_for(layout: &Layout) -> Result<Option<Incarnation>> {
    if layout.state.parent() != Some(layout.install.as_path()) {
        return Ok(None);
    }
    let Some(name) = layout.state.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(operation) = name.strip_prefix("rebuild-state-") else {
        return Ok(None);
    };
    Ok(Some(
        operation
            .parse()
            .context("invalid successor-state operation")?,
    ))
}

impl Transition {
    fn checksum(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/rebuild-layout/1",
            &[&serde_json::to_vec(&(
                self.schema,
                self.operation,
                &self.install,
                &self.previous_layout,
                &self.successor,
                &self.preserved_private_paths,
            ))?],
        ))
    }

    fn previous(&self) -> Result<Layout> {
        Ok(serde_json::from_str(&self.previous_layout)?)
    }

    fn validate(&self, root: &Path) -> Result<()> {
        let previous = self.previous()?;
        ensure!(
            self.schema == 1
                && self.digest == self.checksum()?
                && self.install == root.canonicalize()?
                && previous.schema == 1
                && previous.install == self.install
                && previous.ready
                && self.successor.schema == 1
                && self.successor.install == self.install
                && self.successor.state == successor_state(&self.install, self.operation)
                && self.successor.state != previous.state
                && self.successor.channel == previous.channel
                && self.successor.logs == Some(self.successor.state.join("logs"))
                && !self.successor.ready
                && self.successor.managed_catalog_origin.is_none(),
            "successor layout operation or preserved layout changed"
        );
        ensure!(
            !self.preserved_private_paths.is_empty()
                && self.preserved_private_paths.len() <= MAX_PRESERVED_PATHS
                && self.preserved_private_paths.contains(&previous.install)
                && self.preserved_private_paths.contains(&previous.state),
            "successor layout omitted preserved private state"
        );
        for path in &self.preserved_private_paths {
            let spelling = path.to_str().context("non-UTF-8 preserved state path")?;
            ensure!(
                path.is_absolute()
                    && spelling.len() <= 4096
                    && !spelling.chars().any(char::is_control)
                    && !path
                        .components()
                        .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
                "invalid preserved private state path"
            );
        }
        require_private_union(&self.preserved_private_paths, &self.successor.state)?;
        ensure!(
            self.previous_layout.len() as u64 <= MAX_LAYOUT
                && serde_json::to_vec(self)?.len() as u64 <= MAX_TRANSITION,
            "successor layout receipt exceeds bound"
        );
        preflight_layout(&self.successor)
    }
}

fn read(root: &Path, operation: Incarnation) -> Result<Transition> {
    let transition: Transition = serde_json::from_slice(&read_bounded(
        &root.join(receipt_name(operation)),
        MAX_TRANSITION,
    )?)?;
    transition.validate(root)?;
    ensure!(
        transition.operation == operation,
        "successor receipt operation changed"
    );
    Ok(transition)
}

/// Observe the exact recovery pointer before ordinary layout validation. A
/// pending successor deliberately cannot pass `saved_control`, but its owner
/// must still be able to resume the same interrupted claim. This observation
/// grants no old-process exclusion or permission to execute the handoff.
pub(in crate::runtime_install) fn successor_handoff_started(
    root: &Path,
    operation: Incarnation,
    previous_layout_sha256: Sha256Digest,
) -> Result<bool> {
    let selected = read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?;
    let selected_digest = Sha256Digest::from_bytes(Sha256::digest(&selected).into());
    if !exists(&root.join(receipt_name(operation)))? {
        ensure!(
            selected_digest == previous_layout_sha256,
            "recovery layout receipt is missing after the original layout changed"
        );
        return Ok(false);
    }
    let transition = read(root, operation)?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(transition.previous_layout.as_bytes()).into())
            == previous_layout_sha256,
        "recovery layout receipt differs from the accepted original layout"
    );
    if selected == transition.previous_layout.as_bytes() {
        return Ok(false);
    }
    ensure!(
        selected == serde_json::to_vec(&transition.successor)?
            || selected == serde_json::to_vec(&transition.successor.owner())?,
        "runtime layout changed after rebuild admission; preserving all state"
    );
    Ok(true)
}

/// This receipt identifies a transition, not its execution authority. Only the
/// activation owner's opaque permit can invoke the owning handoff below.
pub(super) fn validate_binding(root: &Path, layout: &Layout) -> Result<()> {
    if let Some(operation) = operation_for(layout)? {
        let transition = read(root, operation)?;
        ensure!(
            *layout == transition.successor || *layout == transition.successor.owner(),
            "selected successor layout differs from its preserved operation"
        );
    }
    Ok(())
}

pub(super) fn preserved_private_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    let Some(operation) = operation_for(&layout)? else {
        return Ok(Vec::new());
    };
    Ok(read(root, operation)?.preserved_private_paths)
}

fn require_private_union(preserved: &[PathBuf], state: &Path) -> Result<()> {
    let mut complete = preserved.to_vec();
    complete.push(state.into());
    complete.push(state.join("logs"));
    complete.sort();
    complete.dedup();
    ensure!(
        complete.len() <= MAX_PRESERVED_PATHS,
        "rebuild private-path union exceeds the existing sixteen-path bound"
    );
    Ok(())
}

/// Saved protection paths do not grant deletion authority. Refuse a reset whose
/// cleanup would leave a still-existing older namespace or unknown private root.
/// This metadata-only check runs before accepting or stopping the predecessor.
pub(in super::super) fn preflight_clean_break(root: &Path, operation: Incarnation) -> Result<()> {
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    let previous = Layout::read(root)?.context("clean-break layout missing")?;
    let preserved = capture_private_paths(root, &previous)?;
    require_private_union(&preserved, &successor_state(root, operation))?;
    let mut owned = vec![
        previous.install.clone(),
        previous.state.clone(),
        previous.state.join("control"),
    ];
    owned.extend(previous.logs.clone());
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(
        &previous.state.join("control"),
    )?;
    for binding in registry.bindings()? {
        owned.push(PathBuf::from(
            registry.retained_project(binding)?.registration.data_root,
        ));
    }
    for path in preserved {
        ensure!(
            owned.contains(&path) || !exists(&path)?,
            "clean_break_retained_root_unsupported: retained private root requires separately verified cleanup: {}",
            path.display()
        );
    }
    Ok(())
}

fn prepare(
    root: &Path,
    operation: Incarnation,
    old_control: &Path,
    state: &Path,
) -> Result<Transition> {
    ensure!(
        state == successor_state(&root.canonicalize()?, operation),
        "successor state must match the accepted recovery operation"
    );
    let path = root.join(receipt_name(operation));
    if exists(&path)? {
        let transition = read(root, operation)?;
        ensure!(
            transition.previous()?.state.join("control") == old_control
                && transition.successor.state == state,
            "recovery layout permit changed"
        );
        return Ok(transition);
    }
    let previous = Layout::read(root)?.context("rebuild requires an existing layout")?;
    ensure!(
        previous.ready && previous.claimed()? && previous.state.join("control") == old_control,
        "rebuild does not own the original initialized layout"
    );
    validate_control(old_control)?;
    ensure!(
        !exists(state)?,
        "successor state already exists without an accepted layout transition; preserving it"
    );
    let successor = Layout {
        schema: 1,
        install: previous.install.clone(),
        state: state.into(),
        logs: Some(state.join("logs")),
        channel: previous.channel,
        ready: false,
        managed_catalog_origin: None,
    };
    let preserved_private_paths = capture_private_paths(root, &previous)?;
    let mut transition = Transition {
        schema: 1,
        operation,
        install: previous.install,
        previous_layout: String::from_utf8(read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?)?,
        successor,
        preserved_private_paths,
        digest: Sha256Digest::from_bytes([0; 32]),
    };
    transition.digest = transition.checksum()?;
    transition.validate(root)?;
    // Publish the immutable transition before the mutable layout pointer. A
    // crash before publication leaves the old layout selected; afterward the
    // same operation resumes rather than allocating another state directory.
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&serde_json::to_vec(&transition)?)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&path)?;
    sync_directory(root)?;
    Ok(transition)
}

fn capture_private_paths(root: &Path, previous: &Layout) -> Result<Vec<PathBuf>> {
    let mut paths = vec![previous.install.clone(), previous.state.clone()];
    paths.extend(previous.logs.clone());
    paths.extend(
        previous
            .managed_catalog_origin
            .as_ref()
            .map(|origin| origin.source.clone()),
    );
    if let Some(operation) = operation_for(previous)? {
        paths.extend(read(root, operation)?.preserved_private_paths);
    }
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(
        &previous.state.join("control"),
    )?;
    paths.extend(registry.saved_private_paths()?);
    // Retired registrations still own retained derived data. Never narrow this
    // to the active roster or infer a parent directory around these exact paths.
    for binding in registry.bindings()? {
        paths.push(PathBuf::from(
            registry.retained_project(binding)?.registration.data_root,
        ));
    }
    paths.sort();
    paths.dedup();
    ensure!(
        paths.len() <= MAX_PRESERVED_PATHS,
        "rebuild preserved private paths exceed the existing sixteen-path bound"
    );
    Ok(paths)
}

fn apply(root: &Path, transition: &Transition) -> Result<()> {
    transition.validate(root)?;
    let selected = read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?;
    let pending = serde_json::to_vec(&transition.successor)?;
    let ready = serde_json::to_vec(&transition.successor.owner())?;
    if selected == transition.previous_layout.as_bytes() {
        // Ordinary reads now fail closed until the native claim completes.
        // The original state/control/pin and their files remain unchanged.
        replace_json(root, LAYOUT, &transition.successor)?;
    } else {
        ensure!(
            selected == pending || selected == ready,
            "runtime layout changed after rebuild admission; preserving all state"
        );
    }
    transition.successor.claim()?;
    if selected != ready {
        replace_json(root, LAYOUT, &transition.successor.owner())?;
    }
    Ok(())
}

/// Caller retains the installation lock and exact old service/project/provider/
/// watcher exclusion through this handoff. The permit is neither serialized nor
/// reconstructible from a transition receipt or a caller supplied directory.
pub(in crate::runtime_install) fn admit_successor(
    root: &Path,
    permit: &super::super::activation::SuccessorPermit<'_>,
) -> Result<()> {
    if !exists(&root.join(receipt_name(permit.operation())))? {
        let bytes = read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?;
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == permit.layout_sha256(),
            "original runtime layout changed before successor reservation"
        );
    }
    let transition = prepare(
        root,
        permit.operation(),
        permit.old_control(),
        permit.state(),
    )?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(transition.previous_layout.as_bytes()).into())
            == permit.layout_sha256(),
        "original runtime layout changed after rebuild admission"
    );
    ensure!(
        capture_private_paths(root, &transition.previous()?)? == transition.preserved_private_paths,
        "retained private roots changed after rebuild admission"
    );
    apply(root, &transition)
}

#[cfg(test)]
mod tests;
