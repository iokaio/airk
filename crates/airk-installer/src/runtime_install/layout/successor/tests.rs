// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_host::install_layout::{record_control, record_registry};

#[test]
fn clean_break_private_union_refuses_seventeenth_path_before_transition() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let state = owner.path().join("successor");
    let mut paths: Vec<_> = (0..14)
        .map(|i| owner.path().join(format!("old-{i}")))
        .collect();
    require_private_union(&paths, &state)?;
    paths.push(state.clone());
    require_private_union(&paths, &state)?; // Exact overlap is counted once.
    paths.push(owner.path().join("old-14"));
    assert!(require_private_union(&paths, &state).is_err());
    assert!(!state.exists());
    Ok(())
}

#[test]
fn clean_break_preflight_refuses_live_older_protection_root_without_mutation() -> Result<()> {
    let (_owner, root, state, namespace) = fixture()?;
    let control = state.join("control");
    let extra = root.join("older-private-state");
    fs::create_dir(&extra)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(&control, namespace)?;
    registry.preserve_private_paths(&lease, std::slice::from_ref(&extra))?;
    drop(registry);
    drop(lease);
    let layout = fs::read(root.join(LAYOUT))?;
    let operation = Incarnation::from_bytes([109; 16]);
    let error = preflight_clean_break(&root, operation).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("clean_break_retained_root_unsupported")
    );
    assert_eq!(fs::read(root.join(LAYOUT))?, layout);
    assert!(!root.join(receipt_name(operation)).exists());
    assert!(extra.exists());
    fs::remove_dir(&extra)?;
    preflight_clean_break(&root, operation)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn clean_break_preflight_refuses_dangling_retained_symlink() -> Result<()> {
    let (_owner, root, state, namespace) = fixture()?;
    let control = state.join("control");
    let extra = root.join("older-private-state");
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(&control, namespace)?;
    registry.preserve_private_paths(&lease, std::slice::from_ref(&extra))?;
    drop(registry);
    drop(lease);
    std::os::unix::fs::symlink(root.join("missing-target"), &extra)?;
    assert!(!extra.try_exists()?);
    let operation = Incarnation::from_bytes([110; 16]);
    let error = preflight_clean_break(&root, operation).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("clean_break_retained_root_unsupported")
    );
    assert!(fs::symlink_metadata(&extra)?.file_type().is_symlink());
    assert!(!root.join(receipt_name(operation)).exists());
    Ok(())
}

fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf, Incarnation)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    let root = root.canonicalize()?;
    let state = airk_host::user_paths::canonical_location(&owner.path().join("original-state"))?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let control = control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    record_control(&lease)?;
    let namespace = Incarnation::from_bytes([101; 16]);
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control, namespace,
    )?);
    record_registry(&lease, namespace)?;
    drop(lease);
    Ok((owner, root, state, namespace))
}

fn files(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    fn walk(root: &Path, path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                walk(root, &entry.path(), result)?;
            } else {
                result.push((
                    entry.path().strip_prefix(root)?.into(),
                    fs::read(entry.path())?,
                ));
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    walk(root, root, &mut result)?;
    result.sort();
    Ok(result)
}

#[test]
fn successor_layout_resumes_each_pointer_and_claim_boundary_without_touching_old_state()
-> Result<()> {
    for boundary in ["admitted", "pending_pointer", "claimed", "ready"] {
        let (_owner, root, old_state, namespace) = fixture()?;
        let original_layout = fs::read(root.join(LAYOUT))?;
        let original_digest = Sha256Digest::from_bytes(Sha256::digest(&original_layout).into());
        let original_state = files(&old_state)?;
        let operation = Incarnation::from_bytes([102; 16]);
        let state = successor_state(&root, operation);
        let transition = prepare(&root, operation, &old_state.join("control"), &state)?;
        assert_eq!(fs::read(root.join(LAYOUT))?, original_layout);
        assert!(!state.exists());
        assert!(!successor_handoff_started(
            &root,
            operation,
            original_digest
        )?);
        if boundary != "admitted" {
            replace_json(&root, LAYOUT, &transition.successor)?;
            assert!(
                control(&root).is_err(),
                "pending layout must not serve old state"
            );
            assert!(successor_handoff_started(
                &root,
                operation,
                original_digest
            )?);
        }
        if matches!(boundary, "claimed" | "ready") {
            transition.successor.claim()?;
        }
        if boundary == "ready" {
            replace_json(&root, LAYOUT, &transition.successor.owner())?;
        }
        // Resume reloads the same operation; no new state/namespace is chosen.
        let resumed = prepare(&root, operation, &old_state.join("control"), &state)?;
        apply(&root, &resumed)?;
        assert!(successor_handoff_started(
            &root,
            operation,
            original_digest
        )?);
        apply(&root, &resumed)?;
        assert_eq!(control(&root)?, state.join("control"));
        assert!(
            !state.join("control").exists(),
            "layout alone cannot create a registry"
        );
        assert_eq!(
            files(&old_state)?,
            original_state,
            "old evidence changed at {boundary}"
        );
        assert_eq!(resumed.previous_layout.as_bytes(), original_layout);
        assert!(
            validate_control(&old_state.join("control")).is_err(),
            "old owner must not resume serving through the successor install"
        );
        assert_eq!(
            airk_store::service::registry::ServiceRegistry::open_saved(&old_state.join("control"))?
                .incarnation()?,
            namespace,
        );
        assert!(airk_host::install_layout::retained_private_paths(&root)?.contains(&old_state));
    }
    Ok(())
}

#[test]
fn successor_layout_refuses_unowned_destinations_changed_selection_and_crossed_operations()
-> Result<()> {
    let (_owner, root, old_state, _) = fixture()?;
    let operation = Incarnation::from_bytes([103; 16]);
    let state = successor_state(&root, operation);
    fs::create_dir(&state)?;
    fs::write(state.join("unknown"), b"preserved")?;
    let before = fs::read(root.join(LAYOUT))?;
    assert!(prepare(&root, operation, &old_state.join("control"), &state).is_err());
    assert_eq!(fs::read(state.join("unknown"))?, b"preserved");
    assert_eq!(fs::read(root.join(LAYOUT))?, before);
    let operation = Incarnation::from_bytes([104; 16]);
    let state = successor_state(&root, operation);
    let transition = prepare(&root, operation, &old_state.join("control"), &state)?;
    let original_digest = Sha256Digest::from_bytes(Sha256::digest(&before).into());
    assert!(!successor_handoff_started(
        &root,
        Incarnation::from_bytes([108; 16]),
        original_digest
    )?);
    assert!(prepare(&root, operation, &old_state.join("other-control"), &state).is_err());
    assert!(
        prepare(
            &root,
            operation,
            &old_state.join("control"),
            &root.join("unowned")
        )
        .is_err()
    );
    fs::write(root.join(LAYOUT), b"foreign selected layout")?;
    assert!(apply(&root, &transition).is_err());
    assert!(successor_handoff_started(&root, operation, original_digest).is_err());
    assert_eq!(fs::read(root.join(LAYOUT))?, b"foreign selected layout");
    assert!(!state.exists());
    Ok(())
}

#[test]
fn successor_layout_keeps_prior_recovery_state_private_and_refuses_a_lost_receipt() -> Result<()> {
    let (_owner, root, old_state, _) = fixture()?;
    let first = Incarnation::from_bytes([105; 16]);
    let first_state = successor_state(&root, first);
    let transition = prepare(&root, first, &old_state.join("control"), &first_state)?;
    apply(&root, &transition)?;
    let first_control = first_state.join("control");
    local_ipc::prepare_root(&first_control)?;
    let lease = airk_store::service::ServiceLease::acquire(&first_control)?;
    record_control(&lease)?;
    drop(airk_store::service::registry::ServiceRegistry::open(
        &first_control,
        Incarnation::from_bytes([106; 16]),
    )?);
    record_registry(&lease, Incarnation::from_bytes([106; 16]))?;
    drop(lease);
    let second = Incarnation::from_bytes([107; 16]);
    let second_state = successor_state(&root, second);
    apply(
        &root,
        &prepare(&root, second, &first_control, &second_state)?,
    )?;
    let private = airk_host::install_layout::retained_private_paths(&root)?;
    assert!(private.contains(&old_state) && private.contains(&first_state));
    let receipt = root.join(receipt_name(second));
    let saved = fs::read(&receipt)?;
    fs::remove_file(&receipt)?;
    assert!(
        control(&root).is_err(),
        "a lost recovery receipt cannot become ordinary fresh state"
    );
    fs::write(receipt, saved)?;
    assert_eq!(control(&root)?, second_state.join("control"));
    Ok(())
}

#[test]
fn successor_layout_preserves_active_inactive_and_saved_external_private_roots() -> Result<()> {
    let (owner, root, old_state, namespace) = fixture()?;
    let base = owner.path().canonicalize()?;
    let old_control = old_state.join("control");
    let lease = airk_store::service::ServiceLease::acquire(&old_control)?;
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open(&old_control, namespace)?;
    let active = airk_core::service::ProjectRegistration::new(
        base.join("active-source").to_str().unwrap().into(),
        base.join("active-data").to_str().unwrap().into(),
        "active".into(),
    )?;
    let inactive = airk_core::service::ProjectRegistration::new(
        base.join("removed-source").to_str().unwrap().into(),
        base.join("retained-data").to_str().unwrap().into(),
        "inactive".into(),
    )?;
    registry.register(&active)?;
    registry.register(&inactive)?;
    let generation = registry.get(inactive.binding)?.fencing_generation;
    let watch_epoch = registry.watch(inactive.binding)?.configuration_epoch;
    assert!(
        registry.retire_missing_source(inactive.binding, generation, watch_epoch)?,
        "retained registration was not retired"
    );
    let saved_private = base.join("earlier-private-state");
    registry.preserve_private_paths(&lease, std::slice::from_ref(&saved_private))?;
    drop((registry, lease));
    let operation = Incarnation::from_bytes([109; 16]);
    let state = successor_state(&root, operation);
    let transition = prepare(&root, operation, &old_control, &state)?;
    for expected in [
        PathBuf::from(active.data_root.as_str()),
        PathBuf::from(inactive.data_root.as_str()),
        saved_private.clone(),
    ] {
        assert!(transition.preserved_private_paths.contains(&expected));
    }
    apply(&root, &transition)?;
    let control = state.join("control");
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    record_control(&lease)?;
    let new_namespace = Incarnation::from_bytes([110; 16]);
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open(&control, new_namespace)?;
    record_registry(&lease, new_namespace)?;
    registry.preserve_private_paths(
        &lease,
        &airk_host::install_layout::retained_private_paths(&root)?,
    )?;
    for protected in [
        active.data_root.as_str(),
        inactive.data_root.as_str(),
        saved_private.to_str().unwrap(),
    ] {
        let proposed = airk_core::service::ProjectRegistration::new(
            Path::new(protected).join("source").to_str().unwrap().into(),
            base.join("unrelated-data").to_str().unwrap().into(),
            "refused".into(),
        )?;
        assert!(
            registry
                .preflight_source_isolation(&proposed, &[])
                .unwrap_err()
                .is::<airk_store::service::registry::SourceBoundary>()
        );
    }
    assert!(!base.join("active-source").exists() && !base.join("removed-source").exists());
    Ok(())
}

#[test]
fn successor_layout_refuses_private_path_capacity_before_pointer_or_receipt_mutation() -> Result<()>
{
    let (owner, root, old_state, namespace) = fixture()?;
    let base = owner.path().canonicalize()?;
    let control = old_state.join("control");
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(&control, namespace)?;
    let paths = (0..12)
        .map(|i| base.join(format!("earlier-private-{i}")))
        .collect::<Vec<_>>();
    registry.preserve_private_paths(&lease, &paths)?;
    drop((registry, lease));
    let operation = Incarnation::from_bytes([111; 16]);
    let before = fs::read(root.join(LAYOUT))?;
    let state = successor_state(&root, operation);
    let error = prepare(&root, operation, &control, &state)
        .err()
        .context("over-capacity successor accepted")?;
    assert!(error.to_string().contains("sixteen-path bound"));
    assert_eq!(fs::read(root.join(LAYOUT))?, before);
    assert!(!root.join(receipt_name(operation)).exists() && !state.exists());
    Ok(())
}
