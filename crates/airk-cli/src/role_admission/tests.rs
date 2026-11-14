// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::ffi::OsString;

const NOW: u64 = 1_800_000_000_000;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn nonce() -> Incarnation {
    Incarnation::from_bytes([0x1a; 16])
}

fn private_root() -> Result<(tempfile::TempDir, PathBuf)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    crate::local_ipc::prepare_root(&root.join("start"))?;
    Ok((owner, root))
}

fn write_request(root: &Path, body: &str) -> Result<PathBuf> {
    let path = start_request_path(root, nonce());
    std::fs::write(&path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

fn request(mode: &str, created: u64, expires: u64) -> String {
    format!(
        "{{\"schema\":\"airk-start-request/1\",\"nonce\":\"{}\",\"mode\":\"{mode}\",\"created_unix_ms\":{created},\"expires_unix_ms\":{expires}}}\n",
        nonce()
    )
}

fn refusal(result: Result<RoleRequest>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn ordinary_arguments_select_no_role() {
    assert!(parse(&args(&["airk"])).is_none());
    assert!(parse(&args(&["airk", "serve", "--runtime-dir", "x"])).is_none());
    assert!(parse(&args(&["airk", "__airk_role_x"])).is_none());
}

#[test]
fn bare_unknown_and_pending_roles_are_refused() {
    assert!(
        refusal(parse(&args(&["airk", SELECTOR])).unwrap()).starts_with("role_admission_refused")
    );
    assert!(
        refusal(parse(&args(&["airk", SELECTOR, "shell"])).unwrap())
            .starts_with("role_admission_refused")
    );
    assert_eq!(
        refusal(parse(&args(&["airk", SELECTOR, "pdf-child"])).unwrap()),
        "role_not_available: pdf-child"
    );
}

#[test]
fn service_detached_requires_its_authority_arguments() {
    let root = std::env::temp_dir().join("airk-role-fixture");
    let root = root.to_str().unwrap();
    let nonce = nonce().to_string();
    for invalid in [
        vec!["airk", SELECTOR, "service-detached"],
        vec!["airk", SELECTOR, "service-detached", "--runtime-dir", root],
        vec![
            "airk",
            SELECTOR,
            "service-detached",
            "--start-request",
            &nonce,
        ],
        vec![
            "airk",
            SELECTOR,
            "service-detached",
            "--runtime-dir",
            "relative",
            "--start-request",
            &nonce,
        ],
        vec![
            "airk",
            SELECTOR,
            "service-detached",
            "--runtime-dir",
            root,
            "--start-request",
            "ABC",
        ],
        vec![
            "airk",
            SELECTOR,
            "service-detached",
            "--runtime-dir",
            root,
            "--runtime-dir",
            root,
        ],
        vec![
            "airk",
            SELECTOR,
            "service-detached",
            "--runtime-dir",
            root,
            "--start-request",
            &nonce,
            "--extra",
            "1",
        ],
        vec!["airk", SELECTOR, "service-detached", "--runtime-dir"],
    ] {
        let message = refusal(parse(&args(&invalid)).unwrap());
        assert!(
            message.starts_with("role_admission_refused"),
            "{invalid:?}: {message}"
        );
    }
    assert_eq!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-detached",
            "--runtime-dir",
            root,
            "--start-request",
            &nonce
        ]))
        .unwrap()
        .unwrap(),
        RoleRequest::ServiceDetached {
            runtime_dir: PathBuf::from(root),
            nonce: nonce.parse().unwrap()
        }
    );
}

#[test]
fn a_valid_request_is_consumed_once_with_its_mode() -> Result<()> {
    let (_owner, root) = private_root()?;
    let path = write_request(&root, &request("explicit", NOW, NOW + 30_000))?;
    assert_eq!(
        consume_start_request(&root, nonce(), NOW + 1)?,
        StartMode::Explicit
    );
    assert!(!path.exists());
    let replay = consume_start_request(&root, nonce(), NOW + 2)
        .unwrap_err()
        .to_string();
    assert!(replay.contains("already consumed"), "{replay}");
    write_request(&root, &request("automatic", NOW, NOW + 30_000))?;
    assert_eq!(
        consume_start_request(&root, nonce(), NOW)?,
        StartMode::Automatic
    );
    Ok(())
}

#[test]
fn an_invalid_request_is_refused_and_still_consumed() -> Result<()> {
    let (_owner, root) = private_root()?;
    for (body, now, code) in [
        (
            request("explicit", NOW, NOW + 30_000),
            NOW + 30_000,
            "start_request_expired",
        ),
        (
            request("explicit", NOW, NOW + 30_001),
            NOW + 1,
            "start_request_lifetime_too_long",
        ),
        (
            request("forced", NOW, NOW + 30_000),
            NOW + 1,
            "install_file_malformed",
        ),
        (
            "x".repeat(MAX_START_REQUEST_BYTES + 1),
            NOW + 1,
            "install_file_oversize",
        ),
    ] {
        let path = write_request(&root, &body)?;
        let message = consume_start_request(&root, nonce(), now)
            .unwrap_err()
            .to_string();
        assert_eq!(message, format!("role_admission_refused: {code}"));
        assert!(!path.exists());
        assert!(!path.with_extension("json.consumed").exists());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_request_readable_by_others_is_refused() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let (_owner, root) = private_root()?;
    let path = write_request(&root, &request("explicit", NOW, NOW + 30_000))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    let message = consume_start_request(&root, nonce(), NOW + 1)
        .unwrap_err()
        .to_string();
    assert!(message.contains("not owner-private"), "{message}");
    assert!(!path.exists());
    Ok(())
}

#[test]
fn listed_roles_match_the_reviewed_table() {
    let table: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/i4/role_admission.json"
    ))
    .unwrap();
    let names: Vec<&str> = table["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|role| role["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ROLES);
    for role in table["roles"].as_array().unwrap() {
        let name = role["name"].as_str().unwrap();
        let outcome = parse(&args(&["airk", SELECTOR, name])).unwrap();
        match role["status"].as_str().unwrap() {
            "served" => assert!(
                refusal(outcome).starts_with("role_admission_refused"),
                "{name}"
            ),
            "pending" => assert_eq!(refusal(outcome), format!("role_not_available: {name}")),
            other => panic!("unknown status {other}"),
        }
    }
}

#[test]
fn record_error_requires_an_absolute_root_and_an_operation() {
    let root = std::env::temp_dir().join("airk-role-fixture");
    let root = root.to_str().unwrap();
    for invalid in [
        vec!["airk", SELECTOR, "record-error"],
        vec!["airk", SELECTOR, "record-error", "--runtime-dir", root],
        vec!["airk", SELECTOR, "record-error", "--operation", "setup"],
        vec![
            "airk",
            SELECTOR,
            "record-error",
            "--runtime-dir",
            "relative",
            "--operation",
            "x",
        ],
        vec![
            "airk",
            SELECTOR,
            "record-error",
            "--runtime-dir",
            root,
            "--operation",
            "x",
            "--extra",
            "1",
        ],
    ] {
        let message = refusal(parse(&args(&invalid)).unwrap());
        assert!(
            message.starts_with("role_admission_refused"),
            "{invalid:?}: {message}"
        );
    }
    assert_eq!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "record-error",
            "--runtime-dir",
            root,
            "--operation",
            "service_setup"
        ]))
        .unwrap()
        .unwrap(),
        RoleRequest::RecordError {
            runtime_dir: PathBuf::from(root),
            operation: "service_setup".into(),
        }
    );
}

#[test]
fn service_foreground_takes_an_absolute_root_and_a_bounded_idle_grace() {
    let root = if cfg!(windows) {
        r"C:\airk\control"
    } else {
        "/airk/control"
    };
    assert_eq!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-foreground",
            "--runtime-dir",
            root
        ]))
        .unwrap()
        .unwrap(),
        RoleRequest::ServiceForeground {
            runtime_dir: PathBuf::from(root),
            idle_exit_seconds: None,
        }
    );
    assert_eq!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-foreground",
            "--runtime-dir",
            root,
            "--idle-exit-seconds",
            "30"
        ]))
        .unwrap()
        .unwrap(),
        RoleRequest::ServiceForeground {
            runtime_dir: PathBuf::from(root),
            idle_exit_seconds: Some(30),
        }
    );
    for refused in [
        &["airk", SELECTOR, "service-foreground"][..],
        &[
            "airk",
            SELECTOR,
            "service-foreground",
            "--runtime-dir",
            "relative",
        ],
        &[
            "airk",
            SELECTOR,
            "service-foreground",
            "--runtime-dir",
            root,
            "--idle-exit-seconds",
            "0",
        ],
        &[
            "airk",
            SELECTOR,
            "service-foreground",
            "--runtime-dir",
            root,
            "--bogus",
            "x",
        ],
    ] {
        let error = parse(&args(refused)).unwrap().unwrap_err().to_string();
        assert!(
            error.starts_with("role_admission_refused"),
            "{refused:?}: {error}"
        );
    }
}

#[test]
fn setup_takes_an_absolute_operation_and_one_mode() {
    let operation = std::env::temp_dir()
        .join("airk-setup-fixture")
        .join("operation.json");
    let operation_text = operation.to_str().unwrap();
    for (extra, mode) in [
        (None, crate::setup_role::SetupMode::Complete),
        (Some("--plan"), crate::setup_role::SetupMode::Plan),
        (Some("--prepare"), crate::setup_role::SetupMode::Prepare),
    ] {
        let mut line = vec!["airk", SELECTOR, "setup", "--operation", operation_text];
        line.extend(extra);
        assert_eq!(
            parse(&args(&line)).unwrap().unwrap(),
            RoleRequest::Setup(crate::setup_role::SetupRequest {
                operation: operation.clone(),
                mode,
                wrapper_script: None,
                wrapper_silent: false,
            })
        );
    }
    for line in [
        vec!["airk", SELECTOR, "setup"],
        vec!["airk", SELECTOR, "setup", "--operation", "operation.json"],
        vec!["airk", SELECTOR, "setup", "--operation"],
        vec![
            "airk",
            SELECTOR,
            "setup",
            "--operation",
            operation_text,
            "--plan",
            "--prepare",
        ],
        vec![
            "airk",
            SELECTOR,
            "setup",
            "--operation",
            operation_text,
            "--operation",
            operation_text,
        ],
        vec![
            "airk",
            SELECTOR,
            "setup",
            "--operation",
            operation_text,
            "--yes",
        ],
    ] {
        assert!(
            refusal(parse(&args(&line)).unwrap()).starts_with("role_admission_refused"),
            "{line:?}"
        );
    }
}

#[test]
fn removal_admission_and_inno_sealing_are_exclusive_private_setup_modes() {
    let operation = std::env::temp_dir()
        .join("fixture-é-日本語-🙂")
        .join("original-operation-request.json");
    let path = operation.to_str().unwrap();
    for (flag, expected) in [
        ("--seal-inno", crate::setup_role::SetupMode::SealInno),
        (
            "--admit-removal",
            crate::setup_role::SetupMode::AdmitRemoval,
        ),
    ] {
        let line = ["airk", SELECTOR, "setup", "--operation", path, flag];
        assert_eq!(
            parse(&args(&line)).unwrap().unwrap(),
            RoleRequest::Setup(crate::setup_role::SetupRequest {
                operation: operation.clone(),
                mode: expected,
                wrapper_script: None,
                wrapper_silent: false,
            })
        );
        for second in ["--plan", "--prepare", "--seal-inno", "--admit-removal"] {
            let line = ["airk", SELECTOR, "setup", "--operation", path, flag, second];
            assert!(refusal(parse(&args(&line)).unwrap()).starts_with("role_admission_refused"));
        }
        assert!(
            refusal(
                parse(&args(&[
                    "airk",
                    SELECTOR,
                    "setup",
                    "--operation",
                    "relative.json",
                    flag
                ]))
                .unwrap()
            )
            .starts_with("role_admission_refused")
        );
    }
}

#[test]
fn service_login_takes_one_install_id() {
    let id = "0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d";
    assert_eq!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-login",
            "--install-id",
            id
        ]))
        .unwrap()
        .unwrap(),
        RoleRequest::ServiceLogin {
            install_id: id.parse().unwrap()
        }
    );
    for line in [
        vec!["airk", SELECTOR, "service-login"],
        vec!["airk", SELECTOR, "service-login", "--install-id"],
        vec!["airk", SELECTOR, "service-login", "--install-id", "0D0D"],
        vec![
            "airk",
            SELECTOR,
            "service-login",
            "--install-id",
            id,
            "--install-id",
            id,
        ],
        vec!["airk", SELECTOR, "service-login", "--runtime-dir", "/x"],
    ] {
        assert!(
            refusal(parse(&args(&line)).unwrap()).starts_with("role_admission_refused"),
            "{line:?}"
        );
    }
}

#[test]
fn static_mac_login_has_no_embedded_per_install_identity() {
    assert_eq!(
        parse(&args(&["airk", SELECTOR, "service-login-current"]))
            .unwrap()
            .unwrap(),
        RoleRequest::ServiceLoginCurrent
    );
    assert!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-login-current",
            "--install-id",
            "0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d"
        ]))
        .unwrap()
        .is_err()
    );
    assert!(
        parse(&args(&[
            "airk",
            SELECTOR,
            "service-login-control",
            "--action",
            "register"
        ]))
        .unwrap()
        .is_err()
    );
}

#[test]
fn recovery_fields_and_wrapper_lease_are_closed_exclusive_private_modes() {
    let operation = std::env::temp_dir().join("original-operation-request.json");
    let path = operation.to_str().unwrap();
    for (name, field) in [
        (
            "requested-by",
            crate::setup_role::RecoveryField::RequestedBy,
        ),
        (
            "placement-required",
            crate::setup_role::RecoveryField::PlacementRequired,
        ),
        (
            "completion-required",
            crate::setup_role::RecoveryField::CompletionRequired,
        ),
        (
            "wrapper-lock-handle",
            crate::setup_role::RecoveryField::WrapperLockHandle,
        ),
    ] {
        assert_eq!(
            parse(&args(&[
                "airk",
                SELECTOR,
                "setup",
                "--operation",
                path,
                "--recovery-field",
                name
            ]))
            .unwrap()
            .unwrap(),
            RoleRequest::Setup(crate::setup_role::SetupRequest {
                operation: operation.clone(),
                mode: crate::setup_role::SetupMode::RecoveryField(field),
                wrapper_script: None,
                wrapper_silent: false,
            })
        );
    }
    for extra in [
        vec!["--recovery-field", "../../arbitrary"],
        vec!["--recovery-field", "program", "--prepare"],
        vec!["--wrapper-silent"],
        vec!["--wrapper-lease-script", "relative/script"],
    ] {
        let mut line = vec!["airk", SELECTOR, "setup", "--operation", path];
        line.extend(extra);
        assert!(parse(&args(&line)).unwrap().is_err());
    }
}
