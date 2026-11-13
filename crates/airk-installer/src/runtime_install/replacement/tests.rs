// SPDX-License-Identifier: Apache-2.0
use super::*;

const CANDIDATE: &str = "1.8.19";

#[test]
fn expected_predecessor_fences_absence_same_build_and_namespace_but_exempts_consumed_replay()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let install = base.join("install");
    let candidate = "a".repeat(64);
    let token = "c".repeat(64);
    let absent = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?
        .expected_predecessor;
    installation(&base, "install")?;
    let before = file_snapshot(&base)?;
    let error = apply_expected(&install, &candidate, Some(&token), None, Some(&absent))
        .unwrap_err()
        .to_string();
    assert!(error.contains("replacement_predecessor_changed"), "{error}");
    // The pre-existing adjacent lock is coordination only, not cleanup state.
    let after = file_snapshot(&base)?;
    for (path, bytes) in &before {
        assert_eq!(after.get(path), Some(bytes));
    }
    assert!(!base.join(".install.replacement-1").exists());
    let first = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    let mut changed = first.index_locations.clone();
    changed.namespace = Some(airk_core::identity::Incarnation::from_bytes([224; 16]));
    let directories = first
        .locations
        .iter()
        .map(|root| Ok((root.clone(), operation::directory_identity(root)?)))
        .collect::<Result<Vec<_>>>()?;
    assert_ne!(
        first.expected_predecessor,
        predecessor_stamp(
            &install,
            first.installed_runtime.as_deref(),
            &changed,
            &directories
        )?
    );
    fs::rename(&install, base.join("prior-install"))?;
    installation(&base, "install")?;
    let error = apply_expected(
        &install,
        &candidate,
        Some(&token),
        None,
        Some(&first.expected_predecessor),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("replacement_predecessor_changed"), "{error}");
    assert!(!base.join(".install.replacement-1").exists());
    let current = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    apply_expected(
        &install,
        &candidate,
        Some(&token),
        None,
        Some(&current.expected_predecessor),
    )?;
    installation(&base, "install")?;
    assert_eq!(
        apply_expected(&install, &candidate, Some(&token), None, Some(&absent))?["replaced"],
        false
    );
    Ok(())
}

fn file_snapshot(root: &Path) -> Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<()> {
        if !path.try_exists()? {
            return Ok(());
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(root, &path, files)?;
            } else {
                files.insert(path.strip_prefix(root)?.to_path_buf(), fs::read(&path)?);
            }
        }
        Ok(())
    }
    let mut files = std::collections::BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn native_logs(install: &Path) -> Result<PathBuf> {
    let control = layout::control(install)?;
    let (log, guard) = airk_host::service_logs::Log::start(
        control.clone(),
        airk_core::identity::Incarnation::from_bytes([223; 16]),
    );
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    drop(guard);
    let logs = airk_host::install_layout::log_parent(&control)?;
    assert!(airk_host::service_logs::has_saved_owner(&logs, &control)?);
    Ok(logs)
}

#[test]
fn raw_only_native_logs_have_read_only_retirement_proof() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "install")?;
    let control = layout::control(&install)?;
    airk_host::service_logs::record_client_error(
        &control,
        "replacement",
        br#"{"code":"fixture_error","detail":{"chain":"fixture diagnostic","truncated":false}}"#,
    )?;
    let logs = airk_host::install_layout::log_parent(&control)?;
    assert!(
        !logs.join("logs-1.4.owner.json").exists(),
        "fixture must have only raw logs"
    );
    let before = file_snapshot(&logs)?;
    assert!(owned_by_airk(&logs, &install)?);
    assert_eq!(file_snapshot(&logs)?, before);
    native_logs(&install)?;
    for name in airk_host::service_logs::OWNER_RECORDS {
        let saved = fs::read(logs.join(name))?;
        fs::write(logs.join(name), b"{}")?;
        let before = file_snapshot(&logs)?;
        assert!(
            owned_by_airk(&logs, &install).is_err(),
            "invalid {name} is never ignored"
        );
        assert_eq!(file_snapshot(&logs)?, before);
        fs::write(logs.join(name), saved)?;
    }
    let mut planned = plan_for_operation(
        &install,
        &"b".repeat(64),
        CANDIDATE,
        Some(&"c".repeat(64)),
        None,
    )?;
    Operation::begin(&mut planned)?;
    let receipt = serde_json::to_value(
        planned
            .operation
            .as_ref()
            .context("operation")?
            .saved
            .as_ref()
            .context("admitted receipt")?,
    )?;
    let state = airk_host::install_layout::Layout::read(&install)?
        .context("layout")?
        .state;
    for name in ["logs-1.4.owner.json", "job-errors-1.owner.json"] {
        let expected = serde_json::json!([
            state,
            logs.strip_prefix(&state)?.join(name),
            runtime_pair::digest_file(&logs.join(name))?
        ]);
        assert!(
            receipt["seals"]
                .as_array()
                .context("seals")?
                .contains(&expected),
            "recovery must seal nested {name}"
        );
    }
    Ok(())
}

fn assert_managed_replacement_refused(root: &Path, provider: &Path) -> Result<()> {
    assert!(managed::require_unmanaged(root).is_err());
    let before = (file_snapshot(root)?, file_snapshot(provider)?);
    let candidate = "b".repeat(64);
    let token = "c".repeat(64);
    // Exercise both public entry paths before assertions, so a planner bug does
    // not hide admission's journal/mutation behavior in the diagnostic batch.
    let planned = plan(root, &candidate, Some(&token), None);
    let applied = apply(root, &candidate, Some(&token), None);
    assert_eq!(
        (file_snapshot(root)?, file_snapshot(provider)?),
        before,
        "managed metadata, state and provider bytes must survive installer admission"
    );
    let root = root.canonicalize()?;
    let journal = root.parent().context("fixture parent")?.join(format!(
        ".{}.replacement-1",
        root.file_name().context("fixture leaf")?.to_string_lossy()
    ));
    assert!(
        !journal.exists(),
        "managed refusal must precede journal creation"
    );
    for result in [planned.map(|_| ()), applied.map(|_| ())] {
        let error = result
            .err()
            .context("managed installer admission must refuse")?;
        assert!(error.to_string().contains("managed"), "{error:#}");
    }
    Ok(())
}

#[test]
fn replacement_refuses_external_managed_policy_before_mutation_or_journal() -> Result<()> {
    let (_owner, root, policy) = managed::tests::fixture(managed::ManagedLifecycle::External)?;
    assert_managed_replacement_refused(
        &root,
        runtime_pair::payload_root(&policy.saved_executable())?,
    )
}

#[test]
fn replacement_refuses_user_on_demand_managed_policy_before_mutation_or_journal() -> Result<()> {
    let (_owner, root, policy) = managed::tests::fixture(managed::ManagedLifecycle::UserOnDemand)?;
    assert_managed_replacement_refused(
        &root,
        runtime_pair::payload_root(&policy.saved_executable())?,
    )
}

#[test]
fn replacement_refuses_pending_managed_detach_after_policy_removal() -> Result<()> {
    let (_owner, root, policy) = managed::tests::fixture(managed::ManagedLifecycle::External)?;
    replace_json(&root, "managed-detach.json", &policy)?;
    fs::remove_file(root.join("managed-runtime.json"))?;
    assert_managed_replacement_refused(
        &root,
        runtime_pair::payload_root(&policy.saved_executable())?,
    )
}

#[test]
fn replacement_refuses_reserved_managed_origin_before_policy_publication() -> Result<()> {
    let (_owner, root, policy) =
        managed::tests::reserved_fixture(managed::ManagedLifecycle::External)?;
    assert!(!root.join("managed-runtime.json").exists());
    assert!(airk_host::install_layout::managed_origin(&root)?.is_some());
    assert_managed_replacement_refused(
        &root,
        runtime_pair::payload_root(&policy.saved_executable())?,
    )
}

#[test]
fn replacement_rechecks_managed_policy_after_predecessor_stop() -> Result<()> {
    let (_owner, root, _policy) = managed::tests::fixture(managed::ManagedLifecycle::External)?;
    let policy_path = root.join("managed-runtime.json");
    let policy_bytes = fs::read(&policy_path)?;
    fs::remove_file(&policy_path)?;
    let planned = plan_for(&root, &"b".repeat(64), CANDIDATE)?;
    let before = file_snapshot(&root)?;
    let error = apply_plan_with_stop(
        planned,
        Duration::ZERO,
        &mut |from, to| fs::rename(from, to),
        &mut |_| {
            fs::write(&policy_path, &policy_bytes).expect("restore canonical managed policy");
            None
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("managed"), "{error}");
    let mut expected = before;
    expected.insert(PathBuf::from("managed-runtime.json"), policy_bytes);
    assert_eq!(
        file_snapshot(&root)?,
        expected,
        "no location moved after managed pin appeared"
    );
    Ok(())
}

#[test]
fn resumed_replacement_preserves_managed_prefixes_in_retired_installation() -> Result<()> {
    for prefix in ["policy", "detach", "origin", "state_origin"] {
        let (_owner, root, policy) = if matches!(prefix, "origin" | "state_origin") {
            managed::tests::reserved_fixture(managed::ManagedLifecycle::External)?
        } else {
            managed::tests::fixture(managed::ManagedLifecycle::External)?
        };
        if prefix == "detach" {
            replace_json(&root, "managed-detach.json", &policy)?;
            fs::remove_file(root.join("managed-runtime.json"))?;
        }
        if prefix == "state_origin" {
            fs::remove_file(root.join(airk_host::install_layout::LAYOUT))?;
        }
        let provider = policy.saved_executable();
        let before = (file_snapshot(&root)?, fs::read(&provider)?);
        let retired = root.with_extension("retired");
        fs::rename(&root, &retired)?;
        let error = require_unmanaged_retired(&root, &retired)
            .unwrap_err()
            .to_string();
        assert!(error.contains("managed"), "{prefix}: {error}");
        assert_eq!((file_snapshot(&retired)?, fs::read(&provider)?), before);
    }
    Ok(())
}

#[test]
fn replacement_refuses_default_named_locations_owned_by_another_installation() -> Result<()> {
    use airk_core::service::ProjectRegistration;
    use airk_store::service::{ProjectLease, ServiceLease, data_scope, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, own_state) = installation(&base.join("a-custom-scope"), "airk-install")?;
    let (foreign_install, foreign_state) = installation(&base.join("b"), "airk-install")?;
    let foreign_control = layout::control(&foreign_install)?;
    let service = ServiceLease::acquire(&foreign_control)?;
    let namespace = ServiceRegistry::open_saved(&foreign_control)?.incarnation()?;
    let mut registry = ServiceRegistry::open(&foreign_control, namespace)?;
    let source = base.join("b-source");
    fs::create_dir(&source)?;
    fs::write(source.join("keep.txt"), b"foreign original source bytes")?;
    let foreign_data = base.join("airk-data");
    fs::create_dir(&foreign_data)?;
    data_scope::bind(&foreign_data, &foreign_control, namespace)?;
    let registration = ProjectRegistration::new(
        source.to_string_lossy().into_owned(),
        foreign_data.to_string_lossy().into_owned(),
        "foreign".into(),
    )?;
    registry.register(&registration)?;
    let mut project = ProjectLease::acquire(&foreign_control, registration.binding)?;
    registry.admit_worker(&mut project)?;
    registry.record_watch_roots(&project, &[])?;
    drop(project);
    drop(registry);
    drop(service);
    assert_eq!(
        airk_host::install_layout::installation_for_control(&foreign_control)?,
        Some(foreign_install.clone())
    );
    assert!(data_scope::has_saved_binding(
        &foreign_data,
        &foreign_control,
        namespace
    )?);
    let foreign_logs = native_logs(&foreign_install)?;
    let before = file_snapshot(&base)?;
    // The platform roster is the only injected observation. Ownership still
    // reads the actual native B state/control/data receipts; no ownership bool
    // is mocked and no real user-profile default is touched.
    for defaults in [
        vec![foreign_state.clone()],
        vec![foreign_data.clone()],
        vec![foreign_logs.clone()],
        vec![foreign_state.clone(), foreign_data.clone(), foreign_logs],
    ] {
        let planned = plan_for_operation_at(
            &install,
            &"b".repeat(64),
            CANDIDATE,
            Some(&"c".repeat(64)),
            None,
            Some(&defaults),
        );
        assert_eq!(file_snapshot(&base)?, before, "planning is read-only");
        assert!(own_state.exists());
        assert!(foreign_state.exists() && foreign_data.exists());
        let error = planned
            .err()
            .context("foreign native ownership must override default names")?;
        assert!(
            error.to_string().contains("remove_installation_refused"),
            "{error:#}"
        );
    }
    Ok(())
}

#[test]
fn explicit_reinstall_replaces_the_identical_pre_release_runtime_but_reconnect_does_not() {
    let candidate = "a".repeat(64);
    assert!(
        !decide_install(
            true,
            Some(&candidate),
            Some(CANDIDATE),
            &candidate,
            true,
            CANDIDATE,
            false
        )
        .0
    );
    assert!(
        decide_install(
            true,
            Some(&candidate),
            Some(CANDIDATE),
            &candidate,
            true,
            CANDIDATE,
            true
        )
        .0
    );
}

#[test]
fn explicit_same_build_reinstall_requires_full_uninstall_after_one_nine_too() {
    let candidate = "a".repeat(64);
    assert!(
        decide_install(
            true,
            Some(&candidate),
            Some("1.9.1"),
            &candidate,
            true,
            "1.9.1",
            true
        )
        .0
    );
    assert!(
        !decide_install(
            true,
            Some(&candidate),
            Some("1.9.1"),
            &candidate,
            true,
            "1.9.1",
            false
        )
        .0
    );
}

#[test]
fn completed_installation_and_its_generation_cannot_uninstall_their_successor_again() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let first = "1".repeat(64);
    let generation = "2".repeat(64);
    assert_eq!(
        apply(&install, &candidate, Some(&first), Some(&generation))?["replaced"],
        true
    );
    let (successor, _) = installation(&base, "airk-install")?;
    fs::write(
        successor.join("successor.txt"),
        b"successor must survive replay",
    )?;
    for operation in [&first, &generation] {
        let planned = plan_for_operation(
            &install,
            &candidate,
            CANDIDATE,
            Some(operation),
            Some(&generation),
        )?;
        assert!(!planned.required);
        assert_eq!(planned.reason, "installation_already_replaced");
        let receipt = apply(&install, &candidate, Some(operation), Some(&generation))?;
        assert_eq!(receipt["replaced"], false);
        assert_eq!(
            fs::read(successor.join("successor.txt"))?,
            b"successor must survive replay"
        );
    }
    assert!(
        plan_for_operation(
            &install,
            &"b".repeat(64),
            CANDIDATE,
            Some(&first),
            Some(&generation)
        )
        .is_err(),
        "operation cannot cross candidates"
    );
    assert_eq!(
        apply(&install, &candidate, Some(&"3".repeat(64)), None)?["replaced"],
        true,
        "a new standalone install must fully uninstall even an identical build"
    );
    assert!(!successor.exists());
    let (successor, _) = installation(&base, "airk-install")?;
    fs::write(
        successor.join("successor.txt"),
        b"after standalone reinstall",
    )?;
    assert_eq!(
        apply(&install, &candidate, Some(&generation), Some(&generation))?["replaced"],
        false,
        "standalone must retain the consumed editor generation"
    );
    assert_eq!(
        fs::read(successor.join("successor.txt"))?,
        b"after standalone reinstall"
    );
    let other_generation = "8".repeat(64);
    assert_eq!(
        apply(
            &install,
            &candidate,
            Some(&other_generation),
            Some(&other_generation)
        )?["replaced"],
        true
    );
    let (successor, _) = installation(&base, "airk-install")?;
    assert_eq!(
        apply(&install, &candidate, Some(&generation), Some(&generation))?["replaced"],
        false,
        "both editor generations must remain consumed"
    );
    assert!(successor.exists());
    Ok(())
}

#[test]
fn a_fresh_install_consumes_its_operation_before_the_successor_is_staged() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let install = owner.path().join("new-parent").join("install");
    let candidate = "a".repeat(64);
    let operation = "4".repeat(64);
    let result = apply(&install, &candidate, Some(&operation), Some(&operation))?;
    assert_eq!(result["replaced"], false);
    assert_eq!(result["plan"]["reason"], "no_installation");
    fs::create_dir(&install)?;
    fs::write(install.join("successor.txt"), b"fresh successor")?;
    let replay = apply(&install, &candidate, Some(&operation), Some(&operation))?;
    assert_eq!(replay["plan"]["reason"], "installation_already_replaced");
    assert_eq!(fs::read(install.join("successor.txt"))?, b"fresh successor");
    Ok(())
}

#[test]
fn a_failed_directory_removal_stays_pending_and_exact_retry_finishes_the_retired_roster()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "5".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    let error = apply_plan_with_removal(
        plan,
        Duration::ZERO,
        &mut |from, to| fs::rename(from, to),
        &mut |path| {
            for name in ["selected.json", "airk-state-1.4.owner.json"] {
                let seal = path.join(name);
                if seal.exists() {
                    fs::remove_file(seal)?;
                }
            }
            Err(std::io::Error::other(
                "fixture: deletion denied after deleting owner seals",
            ))
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("remove_installation_incomplete") && error.contains("deletion denied"),
        "{error}"
    );
    assert_eq!(
        pending_operation(&install, &candidate)?,
        Some(token.clone())
    );
    assert!(
        plan_for_operation(&install, &candidate, CANDIDATE, Some(&"6".repeat(64)), None).is_err(),
        "another operation cannot overwrite unfinished cleanup"
    );
    let result = apply(&install, &candidate, Some(&token), None)?;
    assert_eq!(result["replaced"], true);
    assert_eq!(pending_operation(&install, &candidate)?, None);
    assert!(
        result["removed"]
            .as_array()
            .is_some_and(|paths| !paths.is_empty())
    );
    assert!(!install.exists());
    Ok(())
}

/// Manual install report (12 Nov): an attempt admitted its receipt, was refused
/// before its freeze, and its setup operation was lost with the installer's
/// temporary folder. Every later installer was refused forever.
#[test]
fn a_new_operation_takes_over_an_attempt_refused_before_its_freeze() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let (first_candidate, first) = ("a".repeat(64), "1".repeat(64));
    let mut plan = plan_for_operation(&install, &first_candidate, CANDIDATE, Some(&first), None)?;
    Operation::begin(&mut plan)?;
    assert_eq!(
        pending_operation(&install, &first_candidate)?,
        Some(first.clone())
    );
    let (candidate, second) = ("b".repeat(64), "2".repeat(64));
    let review = plan_for_operation(&install, &candidate, CANDIDATE, Some(&second), None)?;
    assert!(review.required);
    assert_ne!(review.reason, "resume_installation_replacement");
    let result = apply(&install, &candidate, Some(&second), None)?;
    assert_eq!(result["replaced"], true);
    assert!(!install.exists() && !state.exists());
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(
        base.join(".airk-install.replacement-1")
            .join("receipt.json"),
    )?)?;
    assert_eq!(receipt["operation"], second.as_str());
    assert_eq!(receipt["completed"], true);
    assert_eq!(
        receipt["consumed"],
        serde_json::json!([[first, first_candidate], [second, candidate]])
    );
    Ok(())
}

#[test]
fn a_new_operation_cannot_take_over_an_unfrozen_attempt_whose_owner_changed() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let first = "3".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&first), None)?;
    Operation::begin(&mut plan)?;
    fs::write(install.join("selected.json"), b"changed successor identity")?;
    let error = apply(&install, &candidate, Some(&"8".repeat(64)), None)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("replacement_in_progress:"), "{error}");
    assert_eq!(
        fs::read(install.join("selected.json"))?,
        b"changed successor identity"
    );
    assert!(state.exists());
    assert_eq!(pending_operation(&install, &candidate)?, Some(first));
    Ok(())
}

#[test]
fn interrupted_cleanup_refuses_a_changed_successor_at_an_original_location() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "7".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    fs::write(install.join("selected.json"), b"changed successor identity")?;
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("replacement_resume_owner_changed"),
        "{error}"
    );
    assert_eq!(
        fs::read(install.join("selected.json"))?,
        b"changed successor identity"
    );
    Ok(())
}

#[test]
fn a_completed_operation_refuses_another_selected_successor() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "9".repeat(64);
    apply(&install, &candidate, Some(&token), None)?;
    fs::create_dir(&install)?;
    fs::write(
        install.join("selected.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"active":"b".repeat(64),"previous":null}),
        )?,
    )?;
    let before = fs::read(install.join("selected.json"))?;
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("replacement_successor_changed"), "{error}");
    assert_eq!(fs::read(install.join("selected.json"))?, before);
    Ok(())
}

#[test]
fn pending_cleanup_refuses_a_fresh_namespace_in_the_original_directory() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "a".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    let pin = state.join("airk-registry-1.4.owner.json");
    assert!(
        pin.exists(),
        "fixture must have the real top-level namespace pin"
    );
    fs::write(&pin, b"different namespace admission")?;
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("replacement_resume_owner_changed"),
        "{error}"
    );
    assert_eq!(fs::read(pin)?, b"different namespace admission");
    Ok(())
}

#[test]
fn ordinary_same_runtime_reconnect_does_not_inspect_missing_custom_storage() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    fs::remove_dir_all(state)?;
    let planned = plan_for(&install, &"a".repeat(64), CANDIDATE)?;
    assert!(!planned.required);
    assert_eq!(planned.reason, "same_runtime");
    Ok(())
}

#[test]
fn a_selected_legacy_install_without_a_layout_cannot_claim_complete_index_cleanup() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let before = fs::read(state.join("control").join("service-1.4.sqlite3"))?;
    fs::remove_file(install.join("runtime-layout.json"))?;
    let error = plan_for(&install, &"b".repeat(64), CANDIDATE)
        .err()
        .context("unproven legacy state must refuse")?
        .to_string();
    assert!(
        error.contains("remove_installation_legacy_state_unproven"),
        "{error}"
    );
    assert_eq!(
        fs::read(state.join("control").join("service-1.4.sqlite3"))?,
        before
    );
    assert!(install.join("selected.json").exists());
    Ok(())
}

#[test]
fn an_unfrozen_pending_reset_refuses_sources_added_after_its_review() -> Result<()> {
    use airk_core::service::ProjectRegistration;
    use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "d".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    let control = state.join("control");
    let mut registry = ServiceRegistry::open(
        &control,
        airk_core::identity::Incarnation::from_bytes([222; 16]),
    )?;
    let namespace = registry.incarnation()?;
    let source = base.join("new-source");
    fs::create_dir(&source)?;
    fs::write(
        source.join("original.txt"),
        b"source added after reset review",
    )?;
    let data = base.join("new-indexes");
    fs::create_dir(&data)?;
    let registration = ProjectRegistration::new(
        source.to_string_lossy().into_owned(),
        data.to_string_lossy().into_owned(),
        "new-source".into(),
    )?;
    data_scope::bind(&data, &control, namespace)?;
    registry.register(&registration)?;
    let mut project = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut project)?;
    registry.record_watch_roots(&project, &[])?;
    drop(project);
    drop(registry);
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("replacement_resume_index_roster_changed"),
        "{error}"
    );
    assert_eq!(
        fs::read(source.join("original.txt"))?,
        b"source added after reset review"
    );
    assert!(install.exists() && state.exists() && data.exists());
    Ok(())
}

#[test]
fn a_new_operation_never_claims_an_existing_retired_folder() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let token = "e".repeat(64);
    let aside = install.with_file_name(format!("airk-install.removed-{token}"));
    fs::create_dir(&aside)?;
    let selected = fs::read(install.join("selected.json"))?;
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("replacement_aside_collision"), "{error}");
    assert!(aside.is_dir() && fs::read_dir(&aside)?.next().is_none());
    assert_eq!(fs::read(install.join("selected.json"))?, selected);
    assert!(state.exists());
    assert_eq!(
        pending_operation(&install, &candidate)?,
        None,
        "no new receipt may confer ownership of that existing aside"
    );
    Ok(())
}

#[test]
fn original_control_is_reauthenticated_even_when_custom_storage_is_already_aside() -> Result<()> {
    use airk_core::service::ProjectRegistration;
    use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let control = state.join("control");
    let namespace = airk_core::identity::Incarnation::from_bytes([222; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let source = base.join("source-before-partial-move");
    let data = base.join("data-before-partial-move");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(
        source.join("source.txt"),
        b"original source must survive changed control",
    )?;
    let registration = ProjectRegistration::new(
        source.to_string_lossy().into_owned(),
        data.to_string_lossy().into_owned(),
        "partial-control".into(),
    )?;
    data_scope::bind(&data, &control, namespace)?;
    registry.register(&registration)?;
    let mut project = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut project)?;
    registry.record_watch_roots(&project, &[])?;
    drop(project);
    drop(registry);
    let candidate = "a".repeat(64);
    let token = "f".repeat(64);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    Operation::freeze(&mut plan)?;
    let images = fixture_image_roster(&install)?;
    upgrade_stop::stop_paths(
        &images
            .iter()
            .map(|image| image.path.clone())
            .collect::<Vec<_>>(),
    )?;
    let held = legacy_launchers::image_exclusion(&images)?;
    Operation::admit_images(&mut plan, &images, &held.images)?;
    unlink_image_names(&images, &held.images)?;
    let data_aside = data.with_file_name(format!("data-before-partial-move.removed-{token}"));
    rename_owned(&data, &data_aside)?;
    fs::write(
        control.join("service-1.4.owner.json"),
        b"changed native control authority",
    )?;
    let error = apply(&install, &candidate, Some(&token), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("replacement_resume_control_unproven"),
        "{error}"
    );
    assert!(install.exists() && state.exists() && data_aside.exists());
    assert_eq!(
        fs::read(source.join("source.txt"))?,
        b"original source must survive changed control"
    );
    drop(held);
    Ok(())
}

fn fixture_image_roster(install: &Path) -> Result<Vec<legacy_launchers::Image>> {
    let mut paths = Vec::new();
    executables(install, &mut paths)?;
    paths
        .into_iter()
        .map(|path| {
            safe_file(&path)?;
            Ok(legacy_launchers::Image {
                path: path.canonicalize()?,
                sha256: runtime_pair::digest_file(&path)?,
            })
        })
        .collect()
}

fn fixture_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[test]
fn owned_image_names_are_removed_under_exclusion_before_parent_directory_moves() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let candidate = "a".repeat(64);
    let image = install.join("runtimes").join(&candidate).join("airk.exe");
    fixture_executable(&image)?;
    let token = "ba".repeat(32);
    let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    Operation::begin(&mut plan)?;
    let operation = plan.operation.clone().context("fixture operation")?;
    let mut moves = 0;
    let result = apply_plan_with(plan, Duration::ZERO, &mut |from, to| {
        assert!(
            !image.exists(),
            "executable name must be retired before a parent moves"
        );
        moves += 1;
        rename_owned(from, to)
    })?;
    assert_eq!(result["replaced"], true);
    assert_eq!(moves, 2);
    assert!(!install.exists() && !state.exists());
    operation.complete()?;
    let complete = Operation::observe(&install, &candidate, Some(&token), None)?
        .context("completed operation")?;
    let saved = serde_json::to_value(complete.saved)?;
    assert_eq!(saved["completed"], true);
    assert_eq!(saved["image_retirement_admitted"], true);
    assert_eq!(saved["images"].as_array().map(Vec::len), Some(1));
    Ok(())
}

#[test]
fn interrupted_image_unlink_resumes_only_the_recorded_unchanged_surviving_images() -> Result<()> {
    for change_survivor in [false, true] {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let (install, state) = installation(&base, "airk-install")?;
        let candidate = "a".repeat(64);
        let first = install.join("runtimes").join(&candidate).join("airk.exe");
        fixture_executable(&first)?;
        let second = install.join("previous-worker.exe");
        fs::write(&second, b"inert previous worker image")?;
        fixture_executable(&second)?;
        let source = base.join("original-source.txt");
        fs::write(&source, b"original source bytes")?;
        let token = "bc".repeat(32);
        let mut plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
        Operation::begin(&mut plan)?;
        Operation::freeze(&mut plan)?;
        let images = fixture_image_roster(&install)?;
        assert_eq!(images.len(), 2);
        upgrade_stop::stop_paths(
            &images
                .iter()
                .map(|image| image.path.clone())
                .collect::<Vec<_>>(),
        )?;
        let held = legacy_launchers::image_exclusion(&images)?;
        Operation::admit_images(&mut plan, &images, &held.images)?;
        let error = unlink_image_names_with(&images, &held.images, &mut |_| {
            anyhow::bail!("fixture interruption after first image unlink")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("fixture interruption"), "{error}");
        drop(held);
        let survivors = images
            .iter()
            .filter(|image| image.path.exists())
            .collect::<Vec<_>>();
        assert_eq!(survivors.len(), 1);
        assert_eq!(
            pending_operation(&install, &candidate)?,
            Some(token.clone())
        );
        if change_survivor {
            fs::write(
                &survivors[0].path,
                b"changed surviving image must be preserved",
            )?;
            let error = apply(&install, &candidate, Some(&token), None)
                .unwrap_err()
                .to_string();
            assert!(error.contains("replacement_image_changed"), "{error}");
            assert_eq!(
                fs::read(&survivors[0].path)?,
                b"changed surviving image must be preserved"
            );
            assert!(install.exists() && state.exists());
        } else {
            assert_eq!(
                apply(&install, &candidate, Some(&token), None)?["replaced"],
                true
            );
            assert!(!install.exists() && !state.exists());
            assert_eq!(pending_operation(&install, &candidate)?, None);
        }
        assert_eq!(fs::read(&source)?, b"original source bytes");
    }
    Ok(())
}

#[test]
fn explicit_reinstall_cleans_existing_storage_when_another_registered_root_is_missing() -> Result<()>
{
    use airk_core::service::ProjectRegistration;
    use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let control = state.join("control");
    let namespace = airk_core::identity::Incarnation::from_bytes([222; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let source = base.join("source-with-missing-storage");
    fs::create_dir(&source)?;
    fs::write(
        source.join("source.txt"),
        b"preserve source with missing storage",
    )?;
    let missing = base.join("missing-indexes");
    let present = base.join("present-indexes");
    for (name, data) in [("missing-storage", &missing), ("present-storage", &present)] {
        fs::create_dir(data)?;
        data_scope::bind(data, &control, namespace)?;
        let registration = ProjectRegistration::new(
            source.to_string_lossy().into_owned(),
            data.to_string_lossy().into_owned(),
            name.into(),
        )?;
        registry.register(&registration)?;
        let mut project = ProjectLease::acquire(&control, registration.binding)?;
        registry.admit_worker(&mut project)?;
        registry.record_watch_roots(&project, &[])?;
    }
    drop(registry);
    fs::remove_dir_all(&missing)?;
    let candidate = "a".repeat(64);
    let token = "ab".repeat(32);
    let plan = plan_for_operation(&install, &candidate, CANDIDATE, Some(&token), None)?;
    assert!(
        plan.index_locations.data.contains(&missing),
        "logical registration remains part of source protection"
    );
    assert!(
        !plan.locations.contains(&missing),
        "missing storage cannot be a physical retirement operand"
    );
    assert!(plan.locations.contains(&present));
    let result = apply(&install, &candidate, Some(&token), None)?;
    assert_eq!(result["replaced"], true);
    assert!(!install.exists() && !state.exists() && !present.exists() && !missing.exists());
    assert_eq!(
        fs::read(source.join("source.txt"))?,
        b"preserve source with missing storage"
    );
    Ok(())
}

#[test]
fn version_one_nine_zero_is_still_an_empty_replacement_boundary() {
    assert!(
        decide(
            true,
            Some(&"a".repeat(64)),
            Some("1.9.0"),
            &"b".repeat(64),
            false,
            "1.9.0"
        )
        .0
    );
}

#[test]
fn a_process_stop_failure_blocks_every_move_and_cannot_be_reported_as_success() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    let mut moves = Vec::new();
    let result = apply_plan_with_stop(
        plan,
        Duration::ZERO,
        &mut |from, to| {
            moves.push(from.to_path_buf());
            fs::rename(from, to)
        },
        &mut |_| Some("fixture: verified predecessor did not exit".into()),
    );
    assert!(
        result.is_err(),
        "shutdown failure must fail replacement: {result:?}"
    );
    assert!(moves.is_empty(), "no rename before shutdown proof");
    assert!(install.join("selected.json").try_exists()? && state.try_exists()?);
    Ok(())
}

#[test]
fn a_staged_candidate_folder_does_not_authorize_reusing_unselected_old_state() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    fs::remove_file(install.join("selected.json"))?;
    let candidate = "b".repeat(64);
    fs::create_dir_all(install.join("runtimes").join(&candidate))?;
    assert!(plan_for(&install, &candidate, CANDIDATE)?.required);
    Ok(())
}

#[test]
fn replacement_captures_a_missing_receipt_owned_stable_launcher_reference() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let launcher = launcher_path(&install);
    assert!(!launcher.exists());
    fs::write(
        install.join("launcher.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,"sha256":"c".repeat(64)}))?,
    )?;
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    assert!(
        plan.client_references
            .executables
            .contains(&plain(launcher)),
        "a missing image still has an ownership-proven broken client reference"
    );
    Ok(())
}

#[test]
fn replacement_includes_registered_custom_storage_and_preserves_all_source_roots() -> Result<()> {
    use airk_core::{identity::Incarnation, service::ProjectRegistration};
    use airk_store::service::{ProjectLease, ServiceLease, data_scope, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let install = base.join("install");
    fs::create_dir(&install)?;
    let state = base.join("state");
    layout::initialize(&install, Some(&state), RuntimeChannel::Stable)?;
    let control = layout::control(&install)?;
    local_ipc::prepare_root(&control)?;
    let service = ServiceLease::acquire(&control)?;
    airk_host::install_layout::record_control(&service)?;
    let namespace = Incarnation::from_bytes([221; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    airk_host::install_layout::record_registry(&service, namespace)?;
    let source = base.join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("original.txt"), b"original source bytes")?;
    let data = base.join("custom-indexes");
    let registration = ProjectRegistration::new(
        source.to_string_lossy().into_owned(),
        data.to_string_lossy().into_owned(),
        "fixture".into(),
    )?;
    fs::create_dir(&data)?;
    data_scope::bind(&data, &control, namespace)?;
    registry.register(&registration)?;
    let mut project = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut project)?;
    registry.record_watch_roots(&project, &[])?;
    drop(project);
    drop(registry);
    drop(service);
    let active = "a".repeat(64);
    fs::create_dir_all(install.join("runtimes").join(&active))?;
    fs::write(
        install.join("selected.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,"active":active,"previous":null}))?,
    )?;
    fs::write(
        install
            .join("runtimes")
            .join(&active)
            .join("airk-install.json"),
        br#"{"schema":1,"version":"1.8.11","files":{}}"#,
    )?;
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    assert!(
        plan.locations.contains(&data),
        "registered external data must be in the removal plan: {:?}",
        plan.locations
    );
    let result = apply_plan(plan, Duration::ZERO)?;
    assert_eq!(result["replaced"], true);
    assert!(!data.exists(), "custom AIRK storage must be removed");
    assert_eq!(
        fs::read(source.join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}

#[test]
fn replacement_decision_covers_every_installed_state() {
    let candidate = "b".repeat(64);
    let other = "a".repeat(64);
    // Nothing installed.
    assert_eq!(
        decide(false, None, None, &candidate, false, CANDIDATE),
        (false, "no_installation")
    );
    // Ordinary reconnect of the selected build preserves state.
    assert!(
        !decide(
            true,
            Some(&candidate),
            Some(CANDIDATE),
            &candidate,
            false,
            CANDIDATE
        )
        .0
    );
    assert!(decide(true, None, None, &candidate, true, CANDIDATE).0);
    // Any different runtime before 1.9.0 is replaced, including newer 1.8.x,
    // unreadable records and leftovers without a selection.
    for installed in [
        Some("1.5.3"),
        Some("1.8.11"),
        Some("1.8.18"),
        Some("1.8.99"),
        Some("2.0.0"),
        Some("not-a-version"),
        None,
    ] {
        assert_eq!(
            decide(true, Some(&other), installed, &candidate, false, CANDIDATE),
            (true, "replace_before_1_9_0")
        );
    }
    assert!(decide(true, None, None, &candidate, false, CANDIDATE).0);
    // Ordinary upgrades start only after 1.9.0 on both sides.
    assert_eq!(
        decide(
            true,
            Some(&other),
            Some("1.9.1"),
            &candidate,
            false,
            "1.9.2"
        ),
        (false, "ordinary_upgrade")
    );
    assert!(
        decide(
            true,
            Some(&other),
            Some("1.8.19"),
            &candidate,
            false,
            "1.9.0"
        )
        .0
    );
}

fn installation(base: &Path, install_leaf: &str) -> Result<(PathBuf, PathBuf)> {
    let install = base.join(install_leaf);
    let state = base.join("airk-state");
    let active = "a".repeat(64);
    fs::create_dir_all(install.join("runtimes").join(&active))?;
    layout::initialize(&install, Some(&state), RuntimeChannel::Stable)?;
    let control = layout::control(&install)?;
    local_ipc::prepare_root(&control)?;
    let service = airk_store::service::ServiceLease::acquire(&control)?;
    airk_host::install_layout::record_control(&service)?;
    let namespace = airk_core::identity::Incarnation::from_bytes([222; 16]);
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control, namespace,
    )?);
    airk_host::install_layout::record_registry(&service, namespace)?;
    drop(service);
    fs::write(
        install.join("selected.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,"active":active,"previous":null}))?,
    )?;
    fs::write(
        install
            .join("runtimes")
            .join(&active)
            .join("airk-install.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,"version":"1.8.11","files":{}}))?,
    )?;
    fs::write(
        install.join("runtimes").join(&active).join("airk.exe"),
        b"inert, never executed",
    )?;
    Ok((install, state))
}

#[test]
fn replacement_removes_only_airk_locations_of_a_pre_1_9_installation() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let source = base.join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("README.md"), b"original user source")?;
    let candidate = "b".repeat(64);
    let plan = plan_for(&install, &candidate, CANDIDATE)?;
    assert!(plan.required);
    assert_eq!(plan.installed_version.as_deref(), Some("1.8.11"));
    assert_eq!(
        plan.locations,
        vec![
            airk_host::user_paths::canonical_location(&install)?,
            airk_host::user_paths::canonical_location(&state)?
        ]
    );
    let result = apply_plan(plan, Duration::ZERO)?;
    assert_eq!(result["replaced"], true);
    assert_eq!(result["residue"], serde_json::json!([]));
    assert!(
        !install.try_exists()? && !state.try_exists()?,
        "both AIRK locations are removed"
    );
    let left: Vec<_> = fs::read_dir(&base)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    assert_eq!(
        left,
        vec![std::ffi::OsString::from("source")],
        "nothing is left beside the source folder"
    );
    assert_eq!(fs::read(source.join("README.md"))?, b"original user source");
    // Repeating finds nothing to replace.
    assert!(!plan_for(&install, &candidate, CANDIDATE)?.required);
    Ok(())
}

#[test]
fn replacement_refuses_a_damaged_layout_naming_an_unrelated_folder_and_changes_nothing()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let documents = base.join("documents");
    fs::create_dir(&documents)?;
    fs::write(documents.join("README.md"), b"original user document")?;
    fs::write(
        install.join("runtime-layout.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,"install":install,"state":documents}))?,
    )?;
    let error = plan_for(&install, &"b".repeat(64), CANDIDATE)
        .err()
        .context("damaged layout must refuse during planning")?
        .to_string();
    assert!(error.contains("is not an AIRK location"), "{error}");
    assert_eq!(
        fs::read(documents.join("README.md"))?,
        b"original user document"
    );
    assert!(
        install.join("selected.json").try_exists()? && state.try_exists()?,
        "nothing moved"
    );
    Ok(())
}

// The macOS adapter consults launchd for ownership; Windows/Linux read only the
// install-side receipt, so this needs no OS manager.
#[cfg(not(target_os = "macos"))]
#[test]
fn replacement_alters_no_startup_registration_without_an_ownership_receipt() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    assert!(remove_owned_startup(&install)?.is_none());
    assert!(remove_owned_startup(&base.join("never-installed"))?.is_none());
    assert!(
        install.join("selected.json").try_exists()?,
        "checking ownership changes nothing"
    );
    Ok(())
}

#[test]
fn replacement_keeps_ordinary_reconnect_but_not_unproven_staged_state() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let active = "a".repeat(64);
    assert_eq!(
        plan_for(&install, &active, CANDIDATE)?.reason,
        "same_runtime"
    );
    // A fresh install of this candidate has staged it but not yet selected it.
    fs::remove_file(install.join("selected.json"))?;
    let candidate = "c".repeat(64);
    fs::create_dir_all(install.join("runtimes").join(&candidate))?;
    assert_eq!(
        plan_for(&install, &candidate, CANDIDATE)?.reason,
        "replace_before_1_9_0"
    );
    assert!(install.try_exists()?);
    Ok(())
}

#[test]
fn replacement_plan_names_every_client_reference_it_breaks() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let runtime = install
        .join("runtimes")
        .join("a".repeat(64))
        .join("airk.exe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755))?;
    }
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    assert!(plan.required);
    assert_eq!(
        plan.client_references,
        ClientReferences {
            executables: vec![plain(
                airk_host::user_paths::canonical_location(runtime.parent().unwrap())?
                    .join("airk.exe")
            )],
            http_endpoint: None,
            ca_sha256: None,
        },
        "no saved endpoint descriptor means no endpoint or CA reference"
    );
    let value = serde_json::to_value(&plan)?;
    assert_eq!(
        value["client_references"],
        serde_json::json!({
            "executables": [plain(airk_host::user_paths::canonical_location(runtime.parent().unwrap())?.join("airk.exe"))],
            "http_endpoint": null,
            "ca_sha256": null
        })
    );
    assert!(
        !value["client_references"]["executables"][0]
            .as_str()
            .unwrap()
            .starts_with(r"\?\"),
        "agent configurations name plain paths"
    );
    // Nothing to replace names nothing.
    let same = plan_for(&install, &"a".repeat(64), CANDIDATE)?;
    assert_eq!(same.client_references, ClientReferences::default());
    Ok(())
}

#[test]
fn replacement_proves_ownership_and_blocks_folder_removal_until_trust_is_retired() -> Result<()> {
    use std::cell::RefCell;
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    // A damaged layout naming an unrelated folder: nothing at all changes.
    let (install, state) = installation(&base, "airk-install")?;
    let mut unowned_plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    let documents = base.join("documents");
    fs::create_dir(&documents)?;
    unowned_plan.locations.push(documents);
    let effects = RefCell::new(Vec::new());
    let error = apply_ordered(
        unowned_plan,
        || {
            effects.borrow_mut().push("startup");
            Ok(serde_json::Value::Null)
        },
        || {
            effects.borrow_mut().push("trust");
            (serde_json::Value::Null, Vec::new())
        },
        Duration::ZERO,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("is not an AIRK location"), "{error}");
    assert!(
        effects.borrow().is_empty(),
        "no registration or trust change before every location is proven AIRK's own"
    );
    assert!(install.join("selected.json").try_exists()? && state.try_exists()?);

    // A trust failure must retain its original receipt and all folders for retry.
    let result = apply_ordered(
        plan_for(&install, &"b".repeat(64), CANDIDATE)?,
        || {
            effects.borrow_mut().push("startup");
            assert!(
                install.try_exists().unwrap(),
                "startup is removed before any move"
            );
            Ok(serde_json::json!({"fixture":"startup removed"}))
        },
        || {
            effects.borrow_mut().push("trust");
            assert!(
                state.try_exists().unwrap(),
                "trust is retired before control moves"
            );
            (
                serde_json::Value::Null,
                vec!["user trust: fixture issuer was not removed".into()],
            )
        },
        Duration::ZERO,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(*effects.borrow(), ["startup", "trust"]);
    assert!(
        result.contains("remove_installation_trust_incomplete")
            && result.contains("fixture issuer was not removed"),
        "{result}"
    );
    assert!(install.try_exists()? && state.try_exists()?);
    let result = apply_ordered(
        plan_for(&install, &"b".repeat(64), CANDIDATE)?,
        || Ok(serde_json::Value::Null),
        || (serde_json::Value::Null, Vec::new()),
        Duration::ZERO,
    )?;
    assert_eq!(result["replaced"], true);
    assert_eq!(result["residue"], serde_json::json!([]));
    assert!(!install.try_exists()? && !state.try_exists()?);
    Ok(())
}

#[test]
fn replacement_refuses_to_remove_the_image_it_runs_from() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let inside = install
        .join("runtimes")
        .join("a".repeat(64))
        .join("airk.exe");
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    let error = require_outside(&inside, &plan.locations)
        .unwrap_err()
        .to_string();
    assert!(error.contains("remove_installation_refused"), "{error}");
    require_outside(&std::env::current_exe()?, &plan.locations)?;
    Ok(())
}

#[test]
fn replacement_refuses_unreadable_records_and_replaces_only_provably_absent_ones() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let inventory = install
        .join("runtimes")
        .join("a".repeat(64))
        .join("airk-install.json");
    let candidate = "b".repeat(64);
    // A corrupt inventory is a typed refusal, not "unknown version".
    fs::write(&inventory, b"{ not json")?;
    let error = plan_for(&install, &candidate, CANDIDATE)
        .err()
        .context("corrupt inventory must refuse")?
        .to_string();
    assert!(error.starts_with("remove_installation_refused:"), "{error}");
    // An inventory without a version is refused too.
    fs::write(&inventory, br#"{"schema":1,"files":{}}"#)?;
    assert!(plan_for(&install, &candidate, CANDIDATE).is_err());
    // A provably absent inventory leaves the version unknown and replaces.
    fs::remove_file(&inventory)?;
    let plan = plan_for(&install, &candidate, CANDIDATE)?;
    assert!(plan.required && plan.installed_version.is_none());
    // A corrupt selection is refused.
    fs::write(install.join("selected.json"), b"\xff")?;
    let error = plan_for(&install, &candidate, CANDIDATE)
        .err()
        .context("corrupt selection must refuse")?
        .to_string();
    assert!(error.contains("selected.json"), "{error}");
    assert!(install.join("runtimes").try_exists()?, "nothing changed");
    Ok(())
}

#[test]
fn a_layout_folder_without_an_airk_ownership_record_is_refused_by_name() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, _) = installation(&base, "airk-install")?;
    let mut plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    // A custom logs folder that only contains an entry named `control`.
    let logs = base.join("custom-logs");
    fs::create_dir_all(logs.join("control"))?;
    fs::write(logs.join("keep.txt"), b"not AIRK's")?;
    plan.locations.push(logs.clone());
    let error = apply_plan(plan, Duration::ZERO).unwrap_err().to_string();
    assert!(
        error.starts_with("remove_installation_refused:")
            && error.contains(
                &airk_host::user_paths::canonical_location(&logs)?
                    .display()
                    .to_string()
            ),
        "{error}"
    );
    assert_eq!(fs::read(logs.join("keep.txt"))?, b"not AIRK's");
    assert!(install.try_exists()?, "nothing moved");
    // A filename and invalid receipt do not grant retirement authority.
    fs::write(logs.join("logs-1.4.owner.json"), b"{}")?;
    let mut plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    plan.locations.push(logs.clone());
    assert!(require_owned(&plan).is_err());
    assert_eq!(fs::read(logs.join("keep.txt"))?, b"not AIRK's");
    // The log owner creates the genuine receipt for this exact control root.
    let owned_logs = native_logs(&install)?;
    let before = file_snapshot(&owned_logs)?;
    assert!(owned_by_airk(&owned_logs, &install)?);
    assert_eq!(
        file_snapshot(&owned_logs)?,
        before,
        "ownership is read-only"
    );
    let mut plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    plan.locations.push(owned_logs);
    assert!(require_owned(&plan).is_ok());
    Ok(())
}

#[test]
fn a_partial_move_is_rolled_back_newest_first() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let retired_image = install
        .join("runtimes")
        .join("a".repeat(64))
        .join("airk.exe");
    fixture_executable(&retired_image)?;
    let blocked = airk_host::user_paths::canonical_location(&state)?;
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    let mut calls = Vec::new();
    let error = apply_plan_with(plan, Duration::ZERO, &mut |from, to| {
        calls.push((from.to_path_buf(), to.to_path_buf()));
        if from == blocked {
            return Err(std::io::Error::other("fixture: folder in use"));
        }
        fs::rename(from, to)
    })
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("remove_installation_blocked:"), "{error}");
    assert!(
        error.contains("Every folder already moved was restored"),
        "{error}"
    );
    assert!(
        error.contains("Owned executable names were already retired")
            && error.contains("not a runnable installation"),
        "{error}"
    );
    assert!(
        !retired_image.exists(),
        "folder rollback must not resurrect retired executable names"
    );
    assert!(
        install.join("selected.json").try_exists()? && state.try_exists()?,
        "both locations are back in place"
    );
    let aside: Vec<_> = fs::read_dir(&base)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".removed-"))
        .collect();
    assert!(aside.is_empty(), "no folder is left aside");
    let last = calls.last().context("rollback rename")?;
    assert!(last.0.to_string_lossy().contains(".removed-"), "{calls:?}");
    Ok(())
}

#[test]
fn a_failed_rollback_names_every_folder_left_aside() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let blocked = airk_host::user_paths::canonical_location(&state)?;
    let plan = plan_for(&install, &"b".repeat(64), CANDIDATE)?;
    let error = apply_plan_with(plan, Duration::ZERO, &mut |from, to| {
        if from == blocked {
            return Err(std::io::Error::other("fixture: folder in use"));
        }
        if from.to_string_lossy().contains(".removed-") {
            return Err(std::io::Error::other("fixture: restore denied"));
        }
        fs::rename(from, to)
    })
    .unwrap_err()
    .to_string();
    let aside = fs::read_dir(&base)?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name().to_string_lossy().contains(".removed-"))
        .context("the install folder stays aside")?
        .path();
    assert!(
        error.contains("remain aside") && error.contains(&aside.display().to_string()),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_concurrent_replacement_is_refused_before_any_side_effect() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let (install, state) = installation(&base, "airk-install")?;
    let _first = replacement_lock(&install)?.context("first replacement holds the lock")?;
    let error = apply(&install, &"b".repeat(64), None, None)
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("replacement_in_progress:"), "{error}");
    assert!(
        install.join("selected.json").try_exists()? && state.try_exists()?,
        "the loser changes nothing"
    );
    Ok(())
}
