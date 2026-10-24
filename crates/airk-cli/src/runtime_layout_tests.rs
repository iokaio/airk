// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    let state = crate::user_paths::canonical_location(&owner.path().join("state"))?;
    Ok((owner, root.canonicalize()?, state))
}

#[test]
fn source_isolation_uses_both_real_layout_receipts_and_retains_proven_private_paths() -> Result<()>
{
    let (owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let control = control(&root)?;
    local_ipc::prepare_root(&control)?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let source = owner.path().canonicalize()?.join("source");
    let data = owner.path().canonicalize()?.join("data");
    let safe = airk_core::service::ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "safe".into(),
    )?;
    registry.register(&safe)?;
    for private in [root.join("runtimes"), state.join("logs")] {
        let candidate = airk_core::service::ProjectRegistration::new(
            private.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "private".into(),
        )?;
        assert!(
            registry
                .register(&candidate)
                .err()
                .unwrap()
                .is::<airk_store::service::registry::SourceBoundary>()
        );
    }
    let receipt = state.join(OWNER);
    let original = fs::read(&receipt)?;
    fs::write(&receipt, b"Unknown ownership must be preserved.")?;
    assert!(registry.register(&safe).is_err());
    assert_eq!(fs::read(&receipt)?, b"Unknown ownership must be preserved.");
    fs::write(&receipt, original)?;
    fs::remove_file(&receipt)?;
    let candidate = airk_core::service::ProjectRegistration::new(
        state.join("logs").to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "private".into(),
    )?;
    assert!(
        registry
            .register(&candidate)
            .err()
            .unwrap()
            .is::<airk_store::service::registry::SourceBoundary>()
    );
    Ok(())
}

#[test]
fn separate_writable_state_is_pinned_without_creating_control_or_opening_an_index() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    assert!(!state.join("control").exists());
    assert!(!root.join("control").exists());
    assert_eq!(inspect(&root)?["state_claimed"], true);
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert!(
        initialize(
            &root,
            Some(&root.join("other-state")),
            RuntimeChannel::Stable
        )
        .is_err()
    );
    assert!(initialize(&root, None, RuntimeChannel::Preview).is_err());
    assert!(abort(&root).is_err());
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn interrupted_state_claim_never_falls_back_and_resumes_before_and_after_lock_creation()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    let layout = Layout {
        schema: 1,
        logs: None,
        install: root.clone(),
        state: state.clone(),
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&root, LAYOUT, &layout)?;
    assert!(control(&root).is_err());
    assert_eq!(inspect(&root)?["state_claimed"], false);
    local_ipc::prepare_root(&state)?;
    fs::write(state.join(CLAIM), [])?;
    assert!(control(&root).is_err());
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn failed_or_abandoned_claim_preserves_foreign_state_and_permits_a_new_reviewed_location()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    local_ipc::prepare_root(&state)?;
    let source = state.join("original-document.txt");
    fs::write(&source, b"Preserve original unrelated bytes.")?;
    assert!(initialize(&root, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(control(&root).is_err());
    assert!(!state.join(OWNER).exists());
    assert!(!state.join(CLAIM).exists());
    abort(&root)?;
    assert_eq!(fs::read(&source)?, b"Preserve original unrelated bytes.");
    let replacement = root.join("reviewed-state");
    initialize(&root, Some(&replacement), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, replacement.join("control"));
    Ok(())
}

#[test]
fn another_installation_or_channel_cannot_adopt_claimed_state() -> Result<()> {
    let (owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let before = fs::read(state.join(OWNER))?;
    let second = owner.path().join("second-install");
    local_ipc::prepare_root(&second)?;
    assert!(initialize(&second, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(control(&second).is_err());
    abort(&second)?;
    assert_eq!(fs::read(state.join(OWNER))?, before);
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn lost_layout_requires_reviewed_claimed_state_instead_of_an_empty_ledger() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    fs::write(root.join("selected.json"), b"Existing selection marker.")?;
    fs::write(
        state.join("original-work-metadata"),
        b"Preserve accepted work.",
    )?;
    fs::remove_file(root.join(LAYOUT))?;
    assert!(control(&root).is_err());
    assert!(initialize(&root, None, RuntimeChannel::Stable).is_err());
    assert!(
        initialize(
            &root,
            Some(&root.join("different-state")),
            RuntimeChannel::Stable
        )
        .is_err()
    );
    assert!(!root.join(LAYOUT).exists());
    assert!(!root.join("control").exists());
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    assert_eq!(
        fs::read(state.join("original-work-metadata"))?,
        b"Preserve accepted work."
    );
    Ok(())
}

#[test]
fn state_owner_partial_child_fixture() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_STATE_OWNER_PARTIAL_FIXTURE") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let _guard = install_lock(&root)?;
    let layout = Layout::read(&root)?.context("pending fixture layout missing")?;
    local_ipc::prepare_root(&layout.state)?;
    fs::write(layout.state.join(CLAIM), [])?;
    let bytes = serde_json::to_vec(&layout.owner())?;
    let mut partial = File::create(layout.state.join(OWNER_PART))?;
    partial.write_all(&bytes[..bytes.len() / 2])?;
    partial.sync_all()?;
    std::process::exit(81);
}

#[test]
fn process_death_during_owner_write_resumes_only_a_matching_owned_prefix() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    let layout = Layout {
        schema: 1,
        logs: None,
        install: root.clone(),
        state: state.clone(),
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&root, LAYOUT, &layout)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "runtime_install::layout::tests::state_owner_partial_child_fixture",
            "--nocapture",
        ])
        .env("AIRK_STATE_OWNER_PARTIAL_FIXTURE", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    assert_eq!(
        crate::native_process::spawn(&mut command)?.wait()?.code(),
        Some(81)
    );
    assert!(state.join(OWNER_PART).exists());
    assert!(control(&root).is_err());
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    assert!(!state.join(OWNER_PART).exists());
    Ok(())
}

#[test]
fn foreign_or_modified_owner_partials_are_preserved_and_exclusive_rename_never_replaces()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    local_ipc::prepare_root(&state)?;
    let partial = state.join(OWNER_PART);
    fs::write(&partial, [])?;
    assert!(initialize(&root, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(!root.join(LAYOUT).exists());
    let layout = Layout {
        schema: 1,
        logs: None,
        install: root.clone(),
        state: state.clone(),
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&root, LAYOUT, &layout)?;
    fs::write(&partial, b"Preserve user-modified metadata.")?;
    assert!(initialize(&root, None, RuntimeChannel::Stable).is_err());
    assert_eq!(fs::read(&partial)?, b"Preserve user-modified metadata.");
    let destination = state.join("foreign-destination");
    fs::write(&destination, b"Preserve existing destination.")?;
    assert!(publish_new(&partial, &destination).is_err());
    assert_eq!(fs::read(&destination)?, b"Preserve existing destination.");
    assert_eq!(fs::read(&partial)?, b"Preserve user-modified metadata.");
    Ok(())
}

#[test]
fn modified_or_aliased_state_ownership_is_preserved_and_never_redirects_control() -> Result<()> {
    let (owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let receipt = state.join(OWNER);
    let original = fs::read(&receipt)?;
    fs::write(&receipt, b"Unrecognized user metadata.")?;
    assert!(control(&root).is_err());
    assert!(initialize(&root, None, RuntimeChannel::Stable).is_err());
    assert_eq!(fs::read(&receipt)?, b"Unrecognized user metadata.");
    fs::write(&receipt, &original)?;
    let alias = owner.path().join("unrelated-receipt-alias");
    fs::hard_link(&receipt, &alias)?;
    assert!(control(&root).is_err());
    assert_eq!(fs::read(&alias)?, original);
    fs::remove_file(alias)?;
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn existing_control_state_requires_explicit_transition_instead_of_silent_relocation() -> Result<()>
{
    let (_owner, root, state) = fixture()?;
    let old = root.join("control");
    local_ipc::prepare_root(&old)?;
    fs::write(
        old.join("original-owned-fixture"),
        b"Preserve existing control state.",
    )?;
    assert!(initialize(&root, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(!root.join(LAYOUT).exists());
    assert!(!state.exists());
    initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, old);
    assert_eq!(
        fs::read(old.join("original-owned-fixture"))?,
        b"Preserve existing control state."
    );
    Ok(())
}

#[test]
fn pinned_external_logs_are_written_and_exported_without_control_fallback_or_source_capture()
-> Result<()> {
    let (owner, root, state) = fixture()?;
    let logs = crate::user_paths::canonical_location(&owner.path().join("native-global-logs"))?;
    let layout = Layout {
        schema: 1,
        install: root.clone(),
        state: state.clone(),
        logs: Some(logs.clone()),
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&root, LAYOUT, &layout)?;
    initialize(&root, None, RuntimeChannel::Stable)?;
    let control = control(&root)?;
    local_ipc::prepare_root(&control)?;
    assert_eq!(log_parent(&control)?, logs);
    assert!(!logs.exists(), "binding inspection created log storage");
    assert!(crate::service_logs::recent(&control, 32).is_err());
    assert!(!logs.exists(), "cached export created log storage");
    let (log, guard) = crate::service_logs::Log::start(
        control.clone(),
        airk_kernel::service_broker::new_instance_id()?,
    );
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    drop(guard);
    assert!(log.telemetry().available);
    assert!(logs.join("logs-1.4/current.jsonl").is_file());
    assert!(!control.join("logs-1.4").exists());
    assert!(!control.join("logs-1.4.owner.json").exists());
    assert_eq!(crate::service_logs::recent(&control, 32)?.records.len(), 1);
    let mut registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let safe = airk_core::service::ProjectRegistration::new(
        owner
            .path()
            .canonicalize()?
            .join("source")
            .to_str()
            .unwrap()
            .into(),
        owner
            .path()
            .canonicalize()?
            .join("data")
            .to_str()
            .unwrap()
            .into(),
        "fixture".into(),
    )?;
    registry.register(&safe)?;
    let forbidden = airk_core::service::ProjectRegistration::new(
        logs.to_str().unwrap().into(),
        safe.data_root.clone(),
        "private".into(),
    )?;
    assert!(
        registry
            .register(&forbidden)
            .unwrap_err()
            .is::<airk_store::service::registry::SourceBoundary>()
    );
    let receipt = state.join(OWNER);
    let original = fs::read(&receipt)?;
    let mut changed: serde_json::Value = serde_json::from_slice(&original)?;
    changed["logs"] = serde_json::to_value(state.join("other-logs"))?;
    fs::write(&receipt, serde_json::to_vec(&changed)?)?;
    assert!(log_parent(&control).is_err());
    assert!(crate::service_logs::recent(&control, 32).is_err());
    assert!(!control.join("logs-1.4").exists());
    assert!(!state.join("other-logs").exists());
    fs::remove_file(&receipt)?;
    assert!(
        log_parent(&control).is_err(),
        "missing ownership fell back to a new ring"
    );
    assert!(
        registry
            .register(&forbidden)
            .unwrap_err()
            .is::<airk_store::service::registry::SourceBoundary>()
    );
    fs::write(&receipt, original)?;
    assert_eq!(crate::service_logs::recent(&control, 32)?.records.len(), 1);
    Ok(())
}

#[test]
fn new_custom_layout_pins_state_logs_and_old_development_bindings_do_not_move_silently()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let layout = Layout::read(&root)?.unwrap();
    assert_eq!(layout.logs.as_deref(), Some(state.join("logs").as_path()));
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert!(Layout::read(&root)? == Some(layout));
    let (_old_owner, old_root, old_state) = fixture()?;
    let old = Layout {
        schema: 1,
        install: old_root.clone(),
        state: old_state,
        logs: None,
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&old_root, LAYOUT, &old)?;
    initialize(&old_root, None, RuntimeChannel::Stable)?;
    let control = control(&old_root)?;
    local_ipc::prepare_root(&control)?;
    assert_eq!(log_parent(&control)?, control);
    assert!(Layout::read(&old_root)?.unwrap().logs.is_none());
    Ok(())
}

#[test]
fn adopting_existing_development_control_never_creates_logs_in_a_prior_source_root() -> Result<()> {
    let (owner, root, _state) = fixture()?;
    let old_control = root.join("control");
    local_ipc::prepare_root(&old_control)?;
    let prior_source = root.join("logs");
    let data = owner.path().canonicalize()?.join("data");
    for path in [&prior_source, &data] {
        local_ipc::prepare_root(path)?;
    }
    fs::write(
        prior_source.join("memo.txt"),
        b"Original source must remain read-only.",
    )?;
    let mut registry = airk_store::service::registry::ServiceRegistry::open(
        &old_control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let project =
        airk_kernel::service_projects::registration(&prior_source, &data, "prior-source")?;
    registry.register(&project)?;
    initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    assert_eq!(log_parent(&old_control)?, old_control);
    let (log, guard) = crate::service_logs::Log::start(
        old_control.clone(),
        airk_kernel::service_broker::new_instance_id()?,
    );
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    drop(guard);
    assert!(log.telemetry().available);
    assert_eq!(fs::read_dir(&prior_source)?.count(), 1);
    assert_eq!(
        fs::read(prior_source.join("memo.txt"))?,
        b"Original source must remain read-only."
    );
    assert!(old_control.join("logs-1.4/current.jsonl").is_file());
    Ok(())
}

#[test]
fn log_filesystem_failure_does_not_disable_the_owned_control_binding() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let control = control(&root)?;
    local_ipc::prepare_root(&control)?;
    let logs = state.join("logs");
    fs::write(&logs, b"Preserve a conflicting user-created file.")?;
    assert!(log_parent(&control).is_err());
    assert_eq!(super::control(&root)?, control);
    let (log, guard) = crate::service_logs::Log::start(
        control.clone(),
        airk_kernel::service_broker::new_instance_id()?,
    );
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    drop(guard);
    assert!(!log.telemetry().available && log.telemetry().write_failures > 0);
    assert!(!control.join("logs-1.4").exists());
    assert_eq!(
        fs::read(logs)?,
        b"Preserve a conflicting user-created file."
    );
    Ok(())
}
