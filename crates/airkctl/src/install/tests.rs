// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::install_record::canonical;
use serde_json::{Value, json};

const CASES: &str = include_str!("../../../../fixtures/split/i4/i4_cases.json");

fn case(group: &str, name: &str) -> Value {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let text = cases[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .to_owned();
    serde_json::from_str(&text).unwrap()
}

/// Write an owner-private file; its folder was created owner-only (the
/// temporary folder may grant other accounts access, which the reader refuses).
fn private(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A complete owner-private installation built from the reviewed valid cases.
struct Fixture {
    _owner: tempfile::TempDir,
    pointer: PathBuf,
    program: PathBuf,
}

fn fixture(signing: Value) -> Fixture {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().canonicalize().unwrap();
    // The core accepts `/…` or `X:\…` locations, not Windows verbatim paths.
    let root = PathBuf::from(root.to_str().unwrap().trim_start_matches(r"\\?\"));
    let program = root.join("program");
    let pointer = root
        .join("AIRK")
        .join("install-pointer")
        .join("current.json");
    for folder in [
        program.clone(),
        program.join("bin"),
        program.join("service"),
        root.join("AIRK"),
        root.join("AIRK").join("install-pointer"),
    ] {
        airk_ipc::endpoint::prepare_root(&folder).unwrap();
    }
    let mut record = case("records", "valid record");
    for (name, content) in [
        ("airkctl", b"ctl image".as_slice()),
        ("airk", b"service image"),
    ] {
        let path = record["executables"][name]["path"]
            .as_str()
            .unwrap()
            .to_owned();
        private(&program.join(&path), content);
        record["executables"][name]["sha256"] = json!(hex(content));
        record["executables"][name]["bytes"] = json!(content.len());
    }
    for (name, leaf) in [
        ("data", "data"),
        ("state", "state"),
        ("cache", "cache"),
        ("logs", "logs"),
    ] {
        record["locations"][name] = json!(root.join(leaf).to_str().unwrap());
    }
    record["locations"]["program"] = json!(program.to_str().unwrap());
    record["signing"] = signing;
    let record: InstallRecord = serde_json::from_value(record).unwrap();
    let record_bytes = canonical(&record);
    private(&program.join("install-record.json"), &record_bytes);
    let mut value = case("pointers", "valid pointer");
    value["program"] = json!(program.to_str().unwrap());
    value["record_sha256"] = json!(hex(&record_bytes));
    let value: InstallPointer = serde_json::from_value(value).unwrap();
    private(&pointer, &canonical(&value));
    Fixture {
        _owner: owner,
        pointer,
        program,
    }
}

fn windows_signing() -> Value {
    case("records", "valid record")["signing"].clone()
}

fn accept(_: &Signing, _: &[&Path]) -> Result<(), (String, String)> {
    Ok(())
}

fn code(discovery: Discovery) -> String {
    match discovery {
        Discovery::Unverified { code, .. } => code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_missing_pointer_is_not_found() {
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("AIRK/install-pointer/current.json");
    assert!(matches!(read(&path, accept), Discovery::NotFound));
}

#[test]
fn metadata_only_repair_observation_never_marks_a_damaged_pair_verified() {
    let installed = fixture(windows_signing());
    std::fs::write(installed.program.join("bin/airkctl.exe"), b"damaged CLI").unwrap();
    let image = airk_ipc::install_reader::read_metadata(&installed.pointer)
        .unwrap()
        .unwrap();
    image.validate().unwrap();
    assert!(matches!(
        read(&installed.pointer, accept),
        Discovery::Unverified { .. }
    ));
    assert_eq!(image.pointer.program, installed.program.to_string_lossy());
}

#[test]
fn a_complete_installation_verifies_its_pair() {
    let installed = fixture(windows_signing());
    let verified = match read(&installed.pointer, accept) {
        Discovery::Verified(verified) => verified,
        other => panic!("expected a verified installation, got {other:?}"),
    };
    assert_eq!(verified.airk, installed.program.join("service/airk.exe"));
    assert_eq!(verified.airkctl, installed.program.join("bin/airkctl.exe"));
    assert_eq!(verified.record.version, "1.9.0");
}

#[test]
fn unsigned_images_are_refused_and_linux_rests_on_the_digest() {
    // The fixture images are not signed: a record naming the C10 Windows
    // identity is refused by Authenticode on Windows and as another
    // platform's record elsewhere.
    let installed = fixture(windows_signing());
    let expected = if cfg!(windows) {
        "signature_unverified"
    } else {
        "signing_platform_mismatch"
    };
    assert_eq!(code(read(&installed.pointer, platform_signature)), expected);
    let linux =
        fixture(json!({"platform": "linux", "signed": false, "integrity": "record-sha256"}));
    let outcome = read(&linux.pointer, platform_signature);
    if cfg!(all(unix, not(target_os = "macos"))) {
        assert!(matches!(outcome, Discovery::Verified(_)));
    } else {
        assert_eq!(code(outcome), "signing_platform_mismatch");
    }
}

#[test]
fn a_record_naming_another_signing_identity_is_refused() {
    let mut signing = windows_signing();
    signing["publisher"] = json!("CN=Someone Else, O=Someone Else, C=US");
    let installed = fixture(signing);
    let expected = if cfg!(windows) {
        "signing_identity_mismatch"
    } else {
        "signing_platform_mismatch"
    };
    assert_eq!(code(read(&installed.pointer, platform_signature)), expected);
    let macos = fixture(json!({"platform": "macos", "team_id": "ZZZZZ99999"}));
    let expected = if cfg!(target_os = "macos") {
        "signing_identity_mismatch"
    } else {
        "signing_platform_mismatch"
    };
    assert_eq!(code(read(&macos.pointer, platform_signature)), expected);
}

#[test]
fn the_signature_check_sees_both_verified_images() {
    let installed = fixture(windows_signing());
    let seen = std::cell::RefCell::new(Vec::new());
    let outcome = read(&installed.pointer, |_: &Signing, executables: &[&Path]| {
        seen.borrow_mut()
            .extend(executables.iter().map(|path| path.to_path_buf()));
        Ok(())
    });
    assert!(matches!(outcome, Discovery::Verified(_)));
    assert_eq!(
        seen.into_inner(),
        vec![
            installed.program.join("bin/airkctl.exe"),
            installed.program.join("service/airk.exe"),
        ]
    );
}

#[test]
fn a_changed_executable_is_refused() {
    let installed = fixture(windows_signing());
    let airk = installed.program.join("service/airk.exe");
    private(&airk, b"service imagX");
    assert_eq!(
        code(read(&installed.pointer, accept)),
        "installation_unverified"
    );
    private(&airk, b"service image and more");
    assert_eq!(
        code(read(&installed.pointer, accept)),
        "installation_unverified"
    );
}

#[test]
fn a_record_changed_after_its_pointer_is_refused() {
    let installed = fixture(windows_signing());
    let path = installed.program.join("install-record.json");
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 2;
    bytes[last] = b' ';
    private(&path, &bytes);
    assert_eq!(
        code(read(&installed.pointer, accept)),
        "install_record_digest_mismatch"
    );
}

#[test]
fn an_oversize_pointer_is_refused_before_decoding() {
    let installed = fixture(windows_signing());
    private(&installed.pointer, &vec![b' '; MAX_POINTER_BYTES + 1]);
    assert_eq!(
        code(read(&installed.pointer, accept)),
        "installation_unverified"
    );
}

#[cfg(unix)]
#[test]
fn a_shared_or_linked_pointer_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let installed = fixture(windows_signing());
    std::fs::set_permissions(&installed.pointer, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        code(read(&installed.pointer, accept)),
        "installation_unverified"
    );
    std::fs::set_permissions(&installed.pointer, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = installed.pointer.with_file_name("link.json");
    std::os::unix::fs::symlink(&installed.pointer, &link).unwrap();
    assert_eq!(code(read(&link, accept)), "installation_unverified");
}

#[test]
fn the_pointer_path_is_fixed_per_user() {
    let path = pointer_path().unwrap();
    assert!(path.is_absolute());
    assert!(path.ends_with(Path::new("install-pointer").join("current.json")));
}
