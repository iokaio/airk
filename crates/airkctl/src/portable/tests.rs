// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn portable_operation_has_explicit_ownership_and_no_os_integration() {
    #[cfg(windows)]
    let base = "C:\\portable";
    #[cfg(unix)]
    let base = "/portable";
    let locations = Locations {
        program: format!("{base}/program"),
        data: format!("{base}/data"),
        state: format!("{base}/state"),
        cache: format!("{base}/cache"),
        logs: format!("{base}/logs"),
    };
    let request = operation(
        locations,
        Incarnation::from_bytes([8; 16]),
        Sha256Digest::from_bytes([9; 32]),
        1000,
    );
    assert_eq!(request.requested_by, "airkctl-portable");
    assert!(!request.options.login_startup);
    assert!(!request.options.start);
    assert_eq!(request.options.trust, TrustChoice::Skip);
    assert_eq!(request.options.delete_indexes, IndexDeletion::Keep);
    assert!(!request.acknowledgements.index_reset);
    assert!(!request.acknowledgements.predecessor_removal);
    SetupOperation::decode(&canonical(&request), 1000).unwrap();
}

#[test]
fn runtime_tree_observation_retains_foreign_files_for_refusal() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    std::fs::write(root.path().join("nested/approved"), b"payload").unwrap();
    std::fs::write(root.path().join("foreign"), b"preserve").unwrap();
    let mut found = BTreeSet::new();
    let mut budget = 8;
    names(root.path(), "", &mut found, &mut budget).unwrap();
    assert_eq!(
        found,
        BTreeSet::from(["foreign".to_owned(), "nested/approved".to_owned()])
    );
    assert_eq!(
        std::fs::read(root.path().join("foreign")).unwrap(),
        b"preserve"
    );
    assert!(names(root.path(), "", &mut BTreeSet::new(), &mut 0).is_err());
}

#[test]
fn expired_original_portable_request_requires_the_exact_admitted_key() {
    #[cfg(windows)]
    let base = "C:\\portable";
    #[cfg(unix)]
    let base = "/portable";
    let request = operation(
        Locations {
            program: format!("{base}/program"),
            data: format!("{base}/data"),
            state: format!("{base}/state"),
            cache: format!("{base}/cache"),
            logs: format!("{base}/logs"),
        },
        Incarnation::from_bytes([8; 16]),
        Sha256Digest::from_bytes([9; 32]),
        1000,
    );
    let raw = canonical(&request);
    let after = request.expires_unix_ms + 1;
    assert!(decode_original(&raw, after, None).is_err());
    assert_eq!(
        decode_original(
            &raw,
            after,
            Some((request.operation_id, digest(&raw), 1000))
        )
        .unwrap(),
        request
    );
    assert!(
        decode_original(
            &raw,
            after,
            Some((Incarnation::from_bytes([7; 16]), digest(&raw), 1000))
        )
        .is_err()
    );
    let mut changed = request.clone();
    changed.locations.logs.push_str("-changed");
    assert!(
        decode_original(
            &canonical(&changed),
            after,
            Some((request.operation_id, digest(&raw), 1000))
        )
        .is_err()
    );
}

#[test]
fn portable_full_candidate_proof_refuses_foreign_files_and_changed_images_before_signature() {
    use airk_core::install_record::InstallRecord;
    let temporary = tempfile::tempdir().unwrap();
    let program = temporary.path().canonicalize().unwrap().join("runtime");
    airk_ipc::endpoint::prepare_root(&program).unwrap();
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../../fixtures/split/i4/i4_cases.json")).unwrap();
    let record: InstallRecord =
        serde_json::from_str(cases["records"][0]["text"].as_str().unwrap()).unwrap();
    let metadata = SplitRuntimeMetadata {
        schema: 3,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: digest(b"service"),
        airkctl_sha256: digest(b"client"),
        i1_protocol: airk_core::service::PROTOCOL_VERSION,
        i2_contract: crate::contract::I2_CONTRACT,
        admin_inventory_sha256: record.admin_assets.inventory_sha256,
    };
    let service = metadata.service_relative_path().unwrap();
    let cli = metadata.cli_relative_path().unwrap();
    for (relative, bytes) in [
        (service, b"service".as_slice()),
        (cli, b"client".as_slice()),
    ] {
        std::fs::create_dir_all(program.join(relative).parent().unwrap()).unwrap();
        std::fs::write(program.join(relative), bytes).unwrap();
    }
    let metadata_bytes = metadata.encode().unwrap();
    std::fs::write(program.join("airk-runtime.json"), &metadata_bytes).unwrap();
    let target = match std::env::consts::OS {
        "windows" => "x86_64-pc-windows-msvc",
        "linux" => "x86_64-unknown-linux-gnu",
        _ => {
            if std::env::consts::ARCH == "aarch64" {
                "aarch64-apple-darwin"
            } else {
                "x86_64-apple-darwin"
            }
        }
    };
    let inventory = serde_json::json!({"schema":"airk-service-payload/1","version":env!("CARGO_PKG_VERSION"),"channel":"stable","target":target,
        "executables":{"airk":service,"airkctl":cli},"admin_assets":record.admin_assets,"signing":record.signing,
        "files":{(service):{"sha256":digest(b"service"),"bytes":7},(cli):{"sha256":digest(b"client"),"bytes":6},
            "airk-runtime.json":{"sha256":digest(&metadata_bytes),"bytes":metadata_bytes.len()}}});
    let inventory_path = program.parent().unwrap().join(INVENTORY_FILE);
    std::fs::write(&inventory_path, serde_json::to_vec(&inventory).unwrap()).unwrap();
    let calls = std::cell::Cell::new(0);
    let verify = |_: &airk_core::install_record::Signing, images: &[&Path]| {
        assert_eq!(images.len(), 2);
        calls.set(calls.get() + 1);
        Ok(())
    };
    preflight_with(&program, &program.join(cli), &[], &inventory_path, verify).unwrap();
    assert_eq!(calls.get(), 1);
    std::fs::write(program.join("foreign"), b"preserve").unwrap();
    assert!(preflight_with(&program, &program.join(cli), &[], &inventory_path, verify).is_err());
    assert_eq!(calls.get(), 1);
    assert_eq!(std::fs::read(program.join("foreign")).unwrap(), b"preserve");
    std::fs::remove_file(program.join("foreign")).unwrap();
    std::fs::write(program.join(service), b"changed").unwrap();
    assert!(preflight_with(&program, &program.join(cli), &[], &inventory_path, verify).is_err());
    assert_eq!(calls.get(), 1);
}
