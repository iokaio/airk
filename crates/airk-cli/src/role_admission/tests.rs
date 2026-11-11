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
    for pending in ["service-login", "pdf-child", "setup"] {
        assert_eq!(
            refusal(parse(&args(&["airk", SELECTOR, pending])).unwrap()),
            format!("role_not_available: {pending}")
        );
    }
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
