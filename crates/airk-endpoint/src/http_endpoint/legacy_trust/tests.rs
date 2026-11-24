// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::http_endpoint::{read_private, write_private};
use serde_json::json;

const CA: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";

fn names(root: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// An HTTPS build's `http-1` folder, written by hand: the endpoint record,
/// the CA, the TLS bundle, a retired key, a rotation journal and lock, and a
/// trust receipt in `phase` for `user`, naming `store`.
fn old_install(phase: &str, user: &str, store: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    crate::local_ipc::prepare_root(&control)?;
    let root = control.join("http-1");
    crate::local_ipc::prepare_root(&root)?;
    for (name, bytes) in [
        ("endpoint.json", b"{}".to_vec()),
        ("ca.pem", b"-----BEGIN CERTIFICATE-----\n".to_vec()),
        ("tls-identity.json", b"{\"private\":1}".to_vec()),
        ("retired-private-key.json", b"{}".to_vec()),
        ("issuer-rotation.json", br#"{"phase":"stopping"}"#.to_vec()),
        ("issuer-rotation.lock", Vec::new()),
        ("user-trust-1.lock", Vec::new()),
    ] {
        write_private(&root, name, &bytes, 65_536, false)?;
    }
    let receipt = json!({"schema":1,"binding":{"installation":"01".repeat(32),"user":user,
        "namespace":"02".repeat(16)},"ca_sha256":CA,"store":store,"phase":phase});
    write_private(
        &root,
        RECEIPT,
        &serde_json::to_vec(&receipt)?,
        16_384,
        false,
    )?;
    Ok((owner, control))
}

fn me() -> String {
    crate::local_ipc::current_user().unwrap()
}

#[test]
fn an_installed_receipt_removes_its_exact_ca_then_the_tls_files() -> Result<()> {
    let (_owner, control) = old_install("installed", &me(), "CurrentUser/Root")?;
    let mut scripts = Vec::new();
    let mut run = |_: &Path, args: &[String]| {
        scripts.push(args.last().unwrap().clone());
        Ok((true, r#"{"outcome":"removed"}"#.to_owned()))
    };
    let report = retire_with(&control, Host::Windows, &mut run)?;
    assert_eq!(scripts.len(), 1);
    assert!(scripts[0].ends_with(&format!("'{CA}'")));
    assert_eq!(report.certificate, Some((CA.to_owned(), "removed")));
    assert!(report.residue.is_empty(), "{:?}", report.residue);
    assert_eq!(names(&control.join("http-1")), ["endpoint.json"]);
    // A second run finds nothing to do and runs nothing.
    let report = retire_with(&control, Host::Windows, &mut |_, _| panic!("ran"))?;
    assert_eq!(report, LegacyTrust::default());
    Ok(())
}

#[test]
fn an_interrupted_receipt_is_residue_and_is_not_acted_on() -> Result<()> {
    for phase in ["prepared", "remove_prepared"] {
        let (_owner, control) = old_install(phase, &me(), "CurrentUser/Root")?;
        let root = control.join("http-1");
        let receipt = read_private(&root, RECEIPT, 16_384)?;
        let report = retire_with(&control, Host::Windows, &mut |_, _| {
            panic!("acted on {phase}")
        })?;
        assert_eq!(report.certificate, None);
        assert!(
            report.residue[0].contains("unknown outcome"),
            "{:?}",
            report.residue
        );
        // The receipt and the CA stay; the TLS and rotation files go.
        assert_eq!(
            names(&root),
            [
                "ca.pem",
                "endpoint.json",
                "user-trust-1.json",
                "user-trust-1.lock"
            ]
        );
        assert_eq!(read_private(&root, RECEIPT, 16_384)?, receipt);
    }
    Ok(())
}

#[test]
fn another_users_receipt_a_failed_removal_and_another_store_keep_the_trust_files() -> Result<()> {
    let (_owner, control) = old_install("installed", "someone-else", "CurrentUser/Root")?;
    let report = retire_with(&control, Host::Windows, &mut |_, _| panic!("foreign"))?;
    assert!(report.residue[0].contains("another user"));
    assert!(names(&control.join("http-1")).contains(&"ca.pem".to_owned()));

    let (_owner, control) = old_install("installed", &me(), "CurrentUser/Root")?;
    let failed = retire_with(&control, Host::Windows, &mut |_, _| {
        Ok((false, "denied".to_owned()))
    });
    assert!(failed.is_err());
    let left = names(&control.join("http-1"));
    assert!(left.contains(&"ca.pem".to_owned()) && left.contains(&RECEIPT.to_owned()));

    let (_owner, control) = old_install("installed", &me(), "LocalMachine/Root")?;
    assert!(retire_with(&control, Host::Windows, &mut |_, _| panic!("machine store")).is_err());
    Ok(())
}

#[test]
fn without_user_trust_only_files_are_deleted() -> Result<()> {
    // Linux never had trust; a receipt there names nothing to remove.
    let (_owner, control) = old_install("installed", &me(), "CurrentUser/Root")?;
    let report = retire_with(&control, Host::Unsupported, &mut |_, _| panic!("linux"))?;
    assert_eq!(report.certificate, None);
    assert_eq!(names(&control.join("http-1")), ["endpoint.json"]);
    // A removed receipt is settled; links and folders under the names stay.
    let (_owner, control) = old_install("removed", &me(), "CurrentUser/Root")?;
    let root = control.join("http-1");
    std::fs::create_dir(root.join("issuer-rotations"))?;
    std::fs::remove_file(root.join("tls-identity.json"))?;
    std::fs::create_dir(root.join("tls-identity.json"))?;
    let report = retire_with(&control, Host::Windows, &mut |_, _| panic!("removed"))?;
    assert!(report.residue[0].starts_with("issuer-rotations"));
    assert_eq!(
        names(&root),
        ["endpoint.json", "issuer-rotations", "tls-identity.json"]
    );
    // No folder: nothing to do.
    let empty = tempfile::tempdir()?;
    assert_eq!(
        retire_with(empty.path(), Host::Windows, &mut |_, _| panic!("none"))?,
        LegacyTrust::default()
    );
    Ok(())
}
