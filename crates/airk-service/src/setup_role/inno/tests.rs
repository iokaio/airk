// SPDX-License-Identifier: Apache-2.0
use super::*;
use sha2::{Digest, Sha256};
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[test]
fn startup_refuses_before_finalized_inno_custody_and_sibling_receipt_but_accepts_exact_files() {
    let temporary = tempfile::tempdir().unwrap();
    let program = temporary.path().canonicalize().unwrap().join("program");
    let program = PathBuf::from(program.to_str().unwrap().trim_start_matches(r"\\?\"));
    airk_ipc::endpoint::prepare_root(&program).unwrap();
    let root = airk_process::directory::OpenedDirectory::open(&program).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/i4/i4_cases.json"
    ))
    .unwrap();
    let mut record: InstallRecord =
        serde_json::from_str(fixture["records"][0]["text"].as_str().unwrap()).unwrap();
    let mut pointer: InstallPointer =
        serde_json::from_str(fixture["records"][0]["pointer"].as_str().unwrap()).unwrap();
    record.locations.program = program.to_string_lossy().into_owned();
    pointer.program = record.locations.program.clone();
    pointer.record_sha256 = digest(&canonical(&record));
    let image = InstallationImage { pointer, record };
    image.validate().unwrap();
    let accept = |_: &Signing, paths: &[&Path]| {
        assert_eq!(paths, &[program.join("unins000.exe").as_path()]);
        Ok(())
    };
    assert!(require_start_custody_with(&image, accept).is_err());
    for (name, bytes) in [
        ("unins000.exe", b"finalized uninstaller".as_slice()),
        ("unins000.dat", b"finalized script".as_slice()),
    ] {
        airk_ipc::private_record::write_private_at(&root, name, bytes, 4096, false).unwrap();
    }
    let mut handoff = InnoHandoff {
        schema: HANDOFF_SCHEMA.into(),
        install_id: image.record.install_id,
        operation_id: image.record.installed_by.operation_id,
        version: image.record.version.clone(),
        capability: ORIGINAL_OPERATION.into(),
        uninstaller: Executable {
            path: "unins000.exe".into(),
            sha256: digest(b"finalized uninstaller"),
            bytes: 21,
        },
        data: Executable {
            path: "unins000.dat".into(),
            sha256: digest(b"finalized script"),
            bytes: 16,
        },
    };
    airk_ipc::private_record::write_private_at(
        &root,
        HANDOFF_FILE,
        &canonical(&handoff),
        MAX_HANDOFF_BYTES,
        true,
    )
    .unwrap();
    require_start_custody_with(&image, accept).unwrap();
    handoff.operation_id = Incarnation::from_bytes([0x99; 16]);
    airk_ipc::private_record::write_private_at(
        &root,
        HANDOFF_FILE,
        &canonical(&handoff),
        MAX_HANDOFF_BYTES,
        true,
    )
    .unwrap();
    assert!(require_start_custody_with(&image, accept).is_err());
    assert_eq!(
        std::fs::read(program.join("unins000.dat")).unwrap(),
        b"finalized script"
    );
}

#[test]
fn seal_accepts_the_split_layout_without_runtime_pair_metadata_and_refuses_changed_images() {
    // The Windows split payload ships bin\airkctl.exe and service\airk.exe with
    // no airk-runtime.json; the I4 record is the pair identity (1.8.202 failed here).
    let temporary = tempfile::tempdir().unwrap();
    let program = temporary.path().join("program");
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/i4/i4_cases.json"
    ))
    .unwrap();
    let mut record: InstallRecord =
        serde_json::from_str(fixture["records"][0]["text"].as_str().unwrap()).unwrap();
    for (executable, bytes) in [
        (&mut record.executables.airk, b"service image".as_slice()),
        (&mut record.executables.airkctl, b"cli image".as_slice()),
    ] {
        let path = program.join(&executable.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        executable.sha256 = digest(bytes);
        executable.bytes = bytes.len() as u64;
    }
    assert!(!program.join("service").join("airk-runtime.json").exists());
    require_placed_executables(&program, &record).unwrap();
    std::fs::write(program.join(&record.executables.airkctl.path), b"cli imagf").unwrap();
    let refused = require_placed_executables(&program, &record).unwrap_err();
    assert!(
        refused.to_string().contains("inno_seal_refused"),
        "{refused:#}"
    );
    std::fs::remove_file(program.join(&record.executables.airk.path)).unwrap();
    assert!(require_placed_executables(&program, &record).is_err());
}
