// SPDX-License-Identifier: Apache-2.0
//! Definition/receipt tests; actual OS process ancestry is installed qualification.
use super::*;

fn shared(platform: Platform, root: &Path) -> ServiceManager {
    let mut manager = super::tests::fixture(platform, root);
    manager.id = "airksvc-direct-fixture".into();
    manager.executable = airk_host::install_files::launcher_path(root);
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
            .join(airk_host::install_files::main_name()),
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

/// A pre-split installation's receipt and Task Scheduler definition, in the
/// exact form pre-split AIRK wrote them (observed on a real 1.8.2x install,
/// with this fixture's root, principal and runtime digest substituted).
/// Split setup must accept it for retirement and must never launch it.
#[cfg(windows)]
#[test]
fn a_pre_split_windows_registration_is_owned_for_removal_but_never_launched() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut manager = shared(Platform::Windows, root.path());
    manager.principal = "S-1-5-21-1-2-3-1002".into();
    let data = manager.data.clone();
    let runtime_sha256 = "40".repeat(32);
    let executable = format!(r"{data}\runtimes\{runtime_sha256}\airk.exe");
    let captured = RuntimeExecution {
        executable: PathBuf::from(&executable),
        arguments: vec![
            "runtime".into(),
            "managed-serve".into(),
            "--install-dir".into(),
            data.clone(),
        ],
        runtime_sha256: runtime_sha256.clone(),
        executable_sha256: "47".repeat(32),
    };
    manager.validate_runtime_execution(&captured)?;
    let id = &manager.id;
    let user = &manager.principal;
    let command = executable.trim_start_matches(r"\\?\");
    let definition = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><RegistrationInfo><Description>AIRK managed per-user scheduler {id}</Description></RegistrationInfo><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers><Principals><Principal id=\"User\"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings><Actions Context=\"User\"><Exec><Command>{command}</Command><Arguments>&quot;runtime&quot; &quot;managed-serve&quot; &quot;--install-dir&quot; &quot;{data}&quot;</Arguments></Exec></Actions></Task>\n"
    );
    let bytes = manager.definition_bytes(&definition);
    let receipt = serde_json::json!({
        "schema": 2,
        "id": id,
        "data_root": data,
        "definition_sha256": format!("{:x}", Sha256::digest(&bytes)),
        "runtime": {
            "executable": executable,
            "arguments": captured.arguments,
            "runtime_sha256": runtime_sha256,
            "executable_sha256": captured.executable_sha256,
        },
    });
    write_new(&manager.definition, &bytes)?;
    write_new(&manager.receipt, &serde_json::to_vec(&receipt)?)?;
    assert!(manager.check_owned()?);
    assert_eq!(
        manager.execution()?,
        (captured.executable, captured.arguments)
    );
    assert!(manager.check_launchable().is_err());
    Ok(())
}

#[test]
fn split_mac_job_requires_exact_static_role_and_placed_bundle() {
    let main = Path::new("/Applications/AIRK.app/Contents/MacOS/airk");
    let bundle = Path::new("/Applications/AIRK.app");
    let text = "job = {\n\tprogram = /Applications/AIRK.app/Contents/MacOS/airk\n\tpath = /Applications/AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.stable.plist\n\targuments = {\n\t\tairk\n\t\t__airk_role\n\t\tservice-login-current\n\t}\n}\n";
    verify_split_mac_job(text, main, bundle, "stable").unwrap();
    assert!(
        verify_split_mac_job(
            &text.replace("service-login-current", "login-serve"),
            main,
            bundle,
            "stable"
        )
        .is_err()
    );
    assert!(verify_split_mac_job(text, Path::new("/other/airk"), bundle, "stable").is_err());
    assert!(verify_split_mac_job(text, main, bundle, "preview").is_err());
}
