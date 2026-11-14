// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::install_record::inno_handoff::{HANDOFF_SCHEMA, ORIGINAL_OPERATION};
use serde_json::json;

struct Fixture {
    _owner: tempfile::TempDir,
    program: PathBuf,
    verified: Verified,
    handoff: InnoHandoff,
}
fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn write(root: &Path, name: &str, bytes: &[u8]) {
    airk_ipc::private_record::write_private(root, name, bytes, 64 * 1024, true).unwrap();
}
fn fixture() -> Fixture {
    let owner = tempfile::tempdir().unwrap();
    let program = owner.path().canonicalize().unwrap().join("program");
    #[cfg(windows)]
    let program = PathBuf::from(program.to_str().unwrap().trim_start_matches(r"\\?\"));
    airk_ipc::endpoint::prepare_root(&program).unwrap();
    let mut verified = crate::uninstall::tests::verified();
    verified.pointer.program = program.to_string_lossy().into();
    verified.record.locations.program = verified.pointer.program.clone();
    verified.pointer.record_sha256 = hash(&canonical(&verified.record));
    let exe = b"inert uninstaller";
    let data = b"finalized script data";
    write(&program, "unins000.exe", exe);
    write(&program, "unins000.dat", data);
    let handoff = InnoHandoff {
        schema: HANDOFF_SCHEMA.into(),
        install_id: verified.record.install_id,
        operation_id: verified.record.installed_by.operation_id,
        version: verified.record.version.clone(),
        capability: ORIGINAL_OPERATION.into(),
        uninstaller: Executable {
            path: "unins000.exe".into(),
            sha256: hash(exe),
            bytes: exe.len() as u64,
        },
        data: Executable {
            path: "unins000.dat".into(),
            sha256: hash(data),
            bytes: data.len() as u64,
        },
    };
    write(&program, HANDOFF_FILE, &canonical(&handoff));
    Fixture {
        _owner: owner,
        program,
        verified,
        handoff,
    }
}
fn image(f: &Fixture) -> InstallationImage {
    InstallationImage {
        pointer: f.verified.pointer.clone(),
        record: f.verified.record.clone(),
    }
}

#[test]
fn only_the_unique_bound_exe_and_finalized_dat_pass_native_handoff_selection() {
    let f = fixture();
    let observed = std::cell::RefCell::new(Vec::new());
    let selected = inno(&f.program, &image(&f), |_, paths| {
        observed
            .borrow_mut()
            .extend(paths.iter().map(|path| path.to_path_buf()));
        Ok(())
    })
    .unwrap();
    assert_eq!(selected.path, f.program.join("unins000.exe"));
    assert_eq!(*observed.borrow(), vec![f.program.join("unins000.exe")]);
}

#[test]
fn changed_dat_or_duplicate_uninstaller_refuses_before_signature_or_launch() {
    let f = fixture();
    write(&f.program, "unins000.dat", b"changed script bytes");
    assert!(
        inno(&f.program, &image(&f), |_, _| panic!(
            "changed DAT must refuse before signature/launch"
        ))
        .is_err()
    );
    write(&f.program, "unins000.dat", b"finalized script data");
    write(&f.program, "unins001.exe", b"second inert uninstaller");
    assert!(
        inno(&f.program, &image(&f), |_, _| panic!(
            "ambiguity must refuse before signature/launch"
        ))
        .is_err()
    );
}

#[test]
fn missing_legacy_receipt_or_foreign_i4_operation_refuses_handoff() {
    let f = fixture();
    std::fs::remove_file(f.program.join(HANDOFF_FILE)).unwrap();
    assert!(inno(&f.program, &image(&f), |_, _| Ok(())).is_err());
    let mut handoff = f.handoff.clone();
    handoff.operation_id = Incarnation::from_bytes([0x44; 16]);
    write(&f.program, HANDOFF_FILE, &canonical(&handoff));
    assert!(inno(&f.program, &image(&f), |_, _| Ok(())).is_err());
}

#[test]
fn retained_runtime_keeps_approved_dependencies_and_excludes_foreign_files() {
    let mut f = fixture();
    let mut files = serde_json::Map::new();
    for (role, bytes) in [
        ("airk", b"inert native setup".as_slice()),
        ("airkctl", b"inert CLI".as_slice()),
    ] {
        let relative = if role == "airk" {
            f.verified.record.executables.airk.path.clone()
        } else {
            f.verified.record.executables.airkctl.path.clone()
        };
        let parent = f.program.join(&relative).parent().unwrap().to_owned();
        airk_ipc::endpoint::prepare_root(&parent).unwrap();
        write(
            &parent,
            Path::new(&relative).file_name().unwrap().to_str().unwrap(),
            bytes,
        );
        let executable = if role == "airk" {
            &mut f.verified.record.executables.airk
        } else {
            &mut f.verified.record.executables.airkctl
        };
        executable.sha256 = hash(bytes);
        executable.bytes = bytes.len() as u64;
        files.insert(
            relative.clone(),
            json!({"sha256":hash(bytes),"bytes":bytes.len()}),
        );
    }
    let dependency = b"inert native dependency";
    let parent = f.program.join("lib");
    airk_ipc::endpoint::prepare_root(&parent).unwrap();
    write(&parent, "dependency.bin", dependency);
    files.insert(
        "lib/dependency.bin".into(),
        json!({"sha256":hash(dependency),"bytes":dependency.len()}),
    );
    write(&f.program, "foreign.txt", b"unowned file remains");
    f.verified.pointer.record_sha256 = hash(&canonical(&f.verified.record));
    let bytes = serde_json::to_vec(&json!({"schema":"airk-service-payload/1", "version":f.verified.record.version,
        "channel":f.verified.record.channel,"target":f.verified.record.target,
        "executables":{"airkctl":f.verified.record.executables.airkctl.path,"airk":f.verified.record.executables.airk.path},
        "files":files,"admin_assets":f.verified.record.admin_assets,"signing":f.verified.record.signing})).unwrap();
    write(&f.program, INVENTORY_FILE, &bytes);
    let (raw, inventory) = super::inventory(&f.program, &f.verified).unwrap();
    let folder = f
        ._owner
        .path()
        .canonicalize()
        .unwrap()
        .join("private-recovery");
    airk_ipc::endpoint::prepare_root(&folder).unwrap();
    let recovery = retain(
        &f.program,
        &folder,
        &f.verified,
        &raw,
        &inventory,
        |_, images| {
            assert_eq!(images.len(), 2);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read(recovery.join("lib/dependency.bin")).unwrap(),
        dependency
    );
    assert!(!recovery.join("foreign.txt").exists());
    assert_eq!(
        std::fs::read(f.program.join("foreign.txt")).unwrap(),
        b"unowned file remains"
    );
    assert_eq!(std::fs::read(folder.join(INVENTORY_FILE)).unwrap(), bytes);
}
