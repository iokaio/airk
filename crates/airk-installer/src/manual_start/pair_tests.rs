// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::install_record::{
    InstallPointer, InstallRecord, MAX_POINTER_BYTES, MAX_RECORD_BYTES, canonical,
};
use airk_ipc::install_reader::{self, Discovery};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const CASES: &str = include_str!("../../../../fixtures/split/i4/i4_cases.json");

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn layout(
    service: &str,
    cli: &str,
) -> anyhow::Result<(tempfile::TempDir, PathBuf, PathBuf, Discovery)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?.join("private-installation");
    #[cfg(windows)]
    let root = PathBuf::from(root.to_str().unwrap().trim_start_matches(r"\\?\"));
    airk_ipc::endpoint::prepare_root(&root)?;
    let program = root.join("program");
    let pointer_folder = root.join("install-pointer");
    airk_ipc::endpoint::prepare_root(&program)?;
    airk_ipc::endpoint::prepare_root(&pointer_folder)?;
    let service_path = program.join(service);
    let cli_path = program.join(cli);
    for image in [&service_path, &cli_path] {
        let relative = image.parent().unwrap().strip_prefix(&program)?;
        let mut folder = program.clone();
        for component in relative.components() {
            folder.push(component);
            airk_ipc::endpoint::prepare_root(&folder)?;
        }
    }
    let service_bytes = b"inert service image";
    let cli_bytes = b"inert paired CLI image";
    std::fs::write(&service_path, service_bytes)?;
    std::fs::write(&cli_path, cli_bytes)?;
    let fixture: Value = serde_json::from_str(CASES)?;
    let mut record: Value = serde_json::from_str(fixture["records"][0]["text"].as_str().unwrap())?;
    record["version"] = json!(env!("CARGO_PKG_VERSION"));
    for role in ["program", "data", "state", "cache", "logs"] {
        record["locations"][role] = json!(root.join(role).to_string_lossy());
    }
    record["executables"]["airk"] =
        json!({"path": service, "sha256": hash(service_bytes), "bytes": service_bytes.len()});
    record["executables"]["airkctl"] =
        json!({"path": cli, "sha256": hash(cli_bytes), "bytes": cli_bytes.len()});
    let record: InstallRecord = serde_json::from_value(record)?;
    let bytes = canonical(&record);
    airk_ipc::private_record::write_private(
        &program,
        "install-record.json",
        &bytes,
        MAX_RECORD_BYTES,
        false,
    )?;
    let pointer: InstallPointer = serde_json::from_value(json!({
        "schema": "airk-install-pointer/1", "install_id": record.install_id,
        "program": program.to_string_lossy(), "record": "install-record.json",
        "record_sha256": hash(&bytes), "version": record.version, "written_unix_ms": 1800000000001u64,
    }))?;
    let pointer_path = pointer_folder.join("current.json");
    airk_ipc::private_record::write_private(
        &pointer_folder,
        "current.json",
        &canonical(&pointer),
        MAX_POINTER_BYTES,
        false,
    )?;
    let seen = std::cell::RefCell::new(Vec::new());
    let discovery = install_reader::read(&pointer_path, |_, images| {
        seen.borrow_mut()
            .extend(images.iter().map(|path| path.to_path_buf()));
        Ok(())
    });
    assert_eq!(
        *seen.borrow(),
        vec![cli_path.clone(), service_path.clone()],
        "actual I4 reader checks both images before resolver admission"
    );
    Ok((owner, service_path, cli_path, discovery))
}

fn literal_layout(service: &str, cli: &str) -> anyhow::Result<()> {
    let (_owner, service, expected, discovery) = layout(service, cli)?;
    assert_eq!(
        resolve_airkctl(&service, discovery, false)
            .map_err(anyhow::Error::msg)?
            .canonicalize()?,
        expected.canonicalize()?
    );
    Ok(())
}

#[test]
fn windows_service_bin_layout_resolves_the_verified_pair() -> anyhow::Result<()> {
    literal_layout("service/airk.exe", "bin/airkctl.exe")
}

#[test]
fn linux_service_bin_layout_resolves_the_verified_pair() -> anyhow::Result<()> {
    literal_layout("service/airk", "bin/airkctl")
}

#[test]
fn macos_bundle_layout_resolves_the_verified_pair() -> anyhow::Result<()> {
    literal_layout(
        "AIRK.app/Contents/MacOS/airk",
        "AIRK.app/Contents/MacOS/airkctl",
    )
}

#[test]
fn a_different_airk_image_cannot_borrow_the_installed_cli() -> anyhow::Result<()> {
    let (owner, _, _, discovery) = layout("service/airk", "bin/airkctl")?;
    let other = owner.path().join("different-airk");
    std::fs::write(&other, b"different inert image")?;
    assert!(resolve_airkctl(&other, discovery, true).is_err());
    Ok(())
}

#[test]
fn unverified_installation_never_falls_back_to_a_development_sibling() {
    let airk = Path::new("fixture/debug/airk");
    let outcome = resolve_airkctl(
        airk,
        Discovery::Unverified {
            code: "fixture_refused".into(),
            message: "changed image".into(),
        },
        true,
    );
    assert!(outcome.unwrap_err().contains("fixture_refused"));
}

#[test]
fn uninstalled_development_fallback_is_explicit_and_release_has_no_sibling_fallback() {
    let airk = Path::new("fixture/debug/airk");
    assert_eq!(
        resolve_airkctl(airk, Discovery::NotFound, true).unwrap(),
        sibling_airkctl(airk).unwrap()
    );
    assert!(resolve_airkctl(airk, Discovery::NotFound, false).is_err());
    assert!(resolve_airkctl(Path::new("fixture/program/airk"), Discovery::NotFound, true).is_err());
}
