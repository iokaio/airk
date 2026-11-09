// SPDX-License-Identifier: Apache-2.0
//! Definition/receipt tests; actual OS process ancestry is installed qualification.
use super::*;

fn shared(platform: Platform, root: &Path) -> ServiceManager {
    let mut manager = super::tests::fixture(platform, root);
    manager.id = "airksvc-direct-fixture".into();
    manager.executable = crate::install_files::launcher_path(root);
    manager.args = vec![
        "runtime".into(),
        "foreground".into(),
        "--install-dir".into(),
        manager.data.clone(),
    ];
    manager
}

fn captured(manager: &ServiceManager) -> RuntimeExecution {
    let runtime_sha256 = "ab".repeat(32);
    RuntimeExecution {
        executable: Path::new(&manager.data)
            .join("runtimes")
            .join(&runtime_sha256)
            .join(crate::install_files::main_name()),
        arguments: manager.runtime_arguments(),
        runtime_sha256,
        executable_sha256: "cd".repeat(32),
    }
}

fn save(manager: &ServiceManager, runtime: Option<RuntimeExecution>) -> Result<()> {
    let (executable, arguments) = runtime
        .as_ref()
        .map(|saved| (&saved.executable, &saved.arguments))
        .unwrap_or((&manager.executable, &manager.args));
    let plan = manager.plan_for_execution(executable, arguments)?;
    let bytes = manager.definition_bytes(&plan.definition);
    let receipt = Receipt {
        schema: if runtime.is_some() { 2 } else { 1 },
        id: manager.id.clone(),
        data_root: manager.data.clone(),
        definition_sha256: format!("{:x}", Sha256::digest(&bytes)),
        runtime,
    };
    write_new(&manager.definition, &bytes)?;
    write_new(&manager.receipt, &serde_json::to_vec(&receipt)?)
}

#[test]
fn task_and_unit_capture_direct_main_without_wrapper_or_idle_exit() -> Result<()> {
    for platform in [Platform::Windows, Platform::Linux] {
        let root = tempfile::tempdir()?;
        let manager = shared(platform, root.path());
        let captured = captured(&manager);
        manager.validate_runtime_execution(&captured)?;
        save(&manager, Some(captured.clone()))?;
        let plan = manager.plan()?;
        assert_eq!(plan.executable, captured.executable);
        assert_eq!(plan.arguments, captured.arguments);
        assert!(plan.definition.contains("managed-serve"));
        for absent in [
            "airk-launcher",
            "foreground",
            "idle-exit-seconds",
            "airk-service",
        ] {
            assert!(!plan.definition.contains(absent), "{absent}");
        }
        assert!(manager.check_owned()?);
    }
    Ok(())
}

#[test]
fn captured_command_remains_inspectable_when_payload_and_selection_are_missing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manager = shared(Platform::Linux, root.path());
    let captured = captured(&manager);
    assert!(!captured.executable.exists());
    save(&manager, Some(captured.clone()))?;
    // Inspection/removal authority comes from the retained receipt/definition,
    // not executing the missing old image or adopting a different selection.
    assert!(manager.check_owned()?);
    assert_eq!(
        manager.execution()?,
        (captured.executable, captured.arguments)
    );
    assert!(manager.check_launchable().is_err());
    Ok(())
}

#[test]
fn predecessor_receipt_keeps_exact_legacy_command_for_removal_only() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manager = shared(Platform::Windows, root.path());
    save(&manager, None)?;
    assert!(manager.check_owned()?);
    let plan = manager.plan()?;
    assert_eq!(plan.executable, manager.executable);
    assert_eq!(plan.arguments, manager.args);
    assert!(plan.definition.contains("airk-launcher"));
    assert!(manager.check_launchable().is_err());
    Ok(())
}

#[test]
fn captured_runtime_rejects_crossed_paths_arguments_and_schema() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manager = shared(Platform::Linux, root.path());
    let valid = captured(&manager);
    let mut crossed = valid.clone();
    crossed.executable = root.path().join("foreign/airk");
    assert!(manager.validate_runtime_execution(&crossed).is_err());
    crossed = valid.clone();
    crossed.arguments[1] = "foreground".into();
    assert!(manager.validate_runtime_execution(&crossed).is_err());
    crossed = valid.clone();
    crossed.executable_sha256 = "not-a-digest".into();
    assert!(manager.validate_runtime_execution(&crossed).is_err());
    save(&manager, Some(valid))?;
    let mut receipt: serde_json::Value = serde_json::from_slice(&fs::read(&manager.receipt)?)?;
    receipt["schema"] = 1.into();
    fs::write(&manager.receipt, serde_json::to_vec(&receipt)?)?;
    assert!(manager.check_owned().is_err());
    Ok(())
}
