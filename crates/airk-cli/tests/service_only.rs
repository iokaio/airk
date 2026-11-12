// SPDX-License-Identifier: Apache-2.0
//! `airk` is service-only (split plan §5.1, Phase 4). Every case runs a copy
//! of `airk` alone in an empty folder, so no sibling `airkctl` exists and no
//! installation, service or user state is touched.
use std::process::Command;

fn lone_airk() -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) { "airk.exe" } else { "airk" };
    let copy = folder.path().join(name);
    std::fs::copy(env!("CARGO_BIN_EXE_airk"), &copy).unwrap();
    (folder, copy)
}

#[test]
fn user_style_arguments_are_refused_and_name_airkctl() {
    let (_folder, airk) = lone_airk();
    for arguments in [
        &["status"][..],
        &["--help"],
        &["tools", "list"],
        &["serve", "--runtime-dir", "x"],
        &["mcp"],
        &["bridge", "--runtime-dir", "x"],
    ] {
        let output = Command::new(&airk).args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("airk takes no commands"),
            "{arguments:?}: {stderr}"
        );
        assert!(stderr.contains("airkctl"), "{arguments:?}: {stderr}");
        assert!(stderr.contains("unknown"), "{arguments:?}: {stderr}");
    }
}

#[test]
fn no_arguments_without_airkctl_reports_it_and_starts_nothing() {
    let (folder, airk) = lone_airk();
    let output = Command::new(&airk).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("airkctl was not found beside airk"),
        "{stderr}"
    );
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn a_bare_private_role_selector_is_refused() {
    let (_folder, airk) = lone_airk();
    for arguments in [
        &["__airk_role"][..],
        &["__airk_role", "unknown"],
        &["__airk_role", "service-foreground"],
    ] {
        let output = Command::new(&airk).args(arguments).output().unwrap();
        assert!(!output.status.success(), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("role_admission_refused"),
            "{arguments:?}"
        );
    }
}

#[test]
fn the_interim_selector_reaches_only_the_pre_split_operations() {
    let (_folder, airk) = lone_airk();
    // U93: runtime/setup/uninstall/host/service legacy remain behind it;
    // a command airkctl owns is not among them.
    let help = Command::new(&airk)
        .args(["__airk_role", "setup-interim", "--help"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    for kept in ["runtime", "setup", "uninstall", "host", "service"] {
        assert!(text.contains(kept), "{kept}: {text}");
    }
    let refused = Command::new(&airk)
        .args(["__airk_role", "setup-interim", "status"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
}
