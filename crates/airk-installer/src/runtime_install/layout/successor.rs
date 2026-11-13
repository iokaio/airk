// SPDX-License-Identifier: Apache-2.0
//! Explicit recovery layout handoff. The old state is preserved, never relocated.
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_host::install_layout::MAX_LAYOUT;
use airk_host::install_layout::successor_receipt::{
    MAX_PRESERVED_PATHS, Transition, operation_for, read, receipt_name, require_private_union,
    successor_state,
};

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
