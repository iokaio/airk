// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::framed_digest,
    install_record::{InstallRecord, SETUP_RECEIPT_SCHEMA, SetupPhase, SetupStep},
};
use serde_json::{Value, json};

struct Fixture {
    _owner: tempfile::TempDir,
    pointer: PathBuf,
    operation: SetupOperation,
    original: InstallationImage,
    candidate: InstallationImage,
}

fn fixture(fresh: bool) -> Result<Fixture> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?.join("private-installation");
    #[cfg(windows)]
    let root = PathBuf::from(root.to_str().unwrap().trim_start_matches(r"\\?\"));
    airk_ipc::endpoint::prepare_root(&root)?;
    let authority = root.join("install-pointer");
    airk_ipc::endpoint::prepare_root(&authority)?;
    let literal: Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/i4/i4_cases.json"
    ))?;
    let mut record: Value = serde_json::from_str(literal["records"][0]["text"].as_str().unwrap())?;
    for role in ["program", "data", "state", "cache", "logs"] {
        record["locations"][role] = json!(root.join(role).to_string_lossy());
        if !fresh {
            airk_ipc::endpoint::prepare_root(&root.join(role))?;
        }
    }
    if cfg!(unix) {
        record["signing"] = json!({"platform":"linux","signed":false,"integrity":"record-sha256"});
    }
    let original_record: InstallRecord = serde_json::from_value(record)?;
    let mut pointer: Value =
        serde_json::from_str(literal["records"][0]["pointer"].as_str().unwrap())?;
    pointer["program"] = json!(original_record.locations.program);
    pointer["record_sha256"] = json!(files::digest_bytes(&canonical(&original_record)));
    let original = InstallationImage {
        pointer: serde_json::from_value(pointer)?,
        record: original_record,
    };
    original.validate().map_err(refusal)?;
    let mut operation: SetupOperation = serde_json::from_str(
        literal["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["expect"] == "ok")
            .unwrap()["text"]
            .as_str()
            .unwrap(),
    )?;
    operation.locations = original.record.locations.clone();
    operation.kind = OperationKind::Install;
    operation.requested_by = "inno".into();
    operation.operation_id = Incarnation::from_bytes([0x0e; 16]);
    let mut candidate = original.clone();
    candidate.record.installed_by.operation_id = operation.operation_id;
    candidate.record.installed_by.installed_unix_ms += 1;
    if fresh {
        let hash = framed_digest(b"airk/install-id/1", &[operation.operation_id.as_bytes()]);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash.as_bytes()[..16]);
        candidate.record.install_id = Incarnation::from_bytes(bytes);
        candidate.pointer.install_id = candidate.record.install_id;
    }
    candidate.pointer.record_sha256 = files::digest_bytes(&canonical(&candidate.record));
    candidate.pointer.written_unix_ms += 1;
    candidate.validate().map_err(refusal)?;
    let pointer = authority.join("current.json");
    if !fresh {
        write(&pointer, &canonical(&original.pointer))?;
        write(
            &root.join("program").join(RECORD_FILE),
            &canonical(&original.record),
        )?;
    }
    Ok(Fixture {
        _owner: owner,
        pointer,
        operation,
        original,
        candidate,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    airk_ipc::private_record::write_private(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        bytes,
        MAX_RECOVERY_BYTES,
        true,
    )
}

fn admit(f: &Fixture, fresh: bool) -> Result<Owner> {
    let installed = Installed {
        pointer: f.original.pointer.clone(),
        record: f.original.record.clone(),
    };
    Owner::admit(
        &f.pointer,
        &f.operation,
        f.operation.created_unix_ms,
        if fresh { None } else { Some(&installed) },
        Some(f.candidate.clone()),
        None,
    )
}

#[test]
fn publication_filesystem_interruptions_recover_only_the_original_exact_operation() -> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    drop(owner);
    let owner = Owner::load(&f.pointer, &f.operation)?.unwrap();
    assert_eq!(
        owner.existing(&f.pointer)?.unwrap().record,
        f.original.record
    );
    let program_record = Path::new(&f.operation.locations.program).join(RECORD_FILE);
    write(&program_record, &canonical(&f.candidate.record))?;
    assert!(
        InstallRecord::decode(&std::fs::read(&program_record)?, &f.original.pointer).is_err(),
        "ordinary I4 stays strict between writes"
    );
    drop(owner);
    let owner = Owner::load(&f.pointer, &f.operation)?.unwrap();
    assert_eq!(
        owner.existing(&f.pointer)?.unwrap().record,
        f.original.record
    );
    write(&f.pointer, &canonical(&f.candidate.pointer))?;
    drop(owner);
    let owner = Owner::load(&f.pointer, &f.operation)?.unwrap();
    assert_eq!(
        owner.existing(&f.pointer)?.unwrap().record,
        f.candidate.record
    );
    let mut competing = f.operation.clone();
    competing.operation_id = Incarnation::from_bytes([0x42; 16]);
    assert!(Owner::load(&f.pointer, &competing).is_err());
    assert_eq!(std::fs::read(&f.pointer)?, canonical(&f.candidate.pointer));
    Ok(())
}

#[test]
fn publication_filesystem_recovery_refuses_a_third_record_without_overwriting_it() -> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    let path = Path::new(&f.operation.locations.program).join(RECORD_FILE);
    write(&path, b"foreign record\n")?;
    assert!(owner.existing(&f.pointer).is_err());
    assert_eq!(std::fs::read(&path)?, b"foreign record\n");
    Ok(())
}

#[test]
fn newly_created_root_ids_survive_resume_and_unknown_appearing_roots_refuse() -> Result<()> {
    let f = fixture(true)?;
    let mut owner = admit(&f, true)?;
    assert_eq!(
        admission_time(
            &f.pointer,
            &f.operation,
            files::digest_bytes(&canonical(&f.operation))
        )?,
        Some(f.operation.created_unix_ms),
        "admission survives interruption before the first main receipt"
    );
    owner.ensure_roots()?;
    let before = snapshot(&f.operation.locations, &mut Vec::new())?;
    drop(owner);
    let mut resumed = Owner::load(&f.pointer, &f.operation)?.unwrap();
    resumed.ensure_roots()?;
    assert_eq!(snapshot(&f.operation.locations, &mut Vec::new())?, before);
    drop(resumed);
    let g = fixture(true)?;
    let owner = admit(&g, true)?;
    airk_ipc::endpoint::prepare_root(Path::new(&g.operation.locations.data))?;
    drop(owner);
    assert!(Owner::load(&g.pointer, &g.operation).is_err());
    Ok(())
}

#[test]
fn replacing_a_retained_root_at_the_same_path_refuses_recovery() -> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    drop(owner);
    let data = Path::new(&f.operation.locations.data);
    std::fs::rename(data, data.with_file_name("displaced-data"))?;
    airk_ipc::endpoint::prepare_root(data)?;
    assert!(Owner::load(&f.pointer, &f.operation).is_err());
    Ok(())
}

#[test]
fn keep_data_receipt_and_real_root_continuity_admit_reinstall_after_metadata_removal() -> Result<()>
{
    let mut f = fixture(false)?;
    f.operation.kind = OperationKind::Uninstall;
    f.operation.options.delete_indexes = IndexDeletion::Keep;
    f.operation.options.start = false;
    let installed = Installed {
        pointer: f.original.pointer.clone(),
        record: f.original.record.clone(),
    };
    for path in [
        &f.operation.locations.data,
        &f.operation.locations.state,
        &f.operation.locations.cache,
        &f.operation.locations.logs,
    ] {
        std::fs::write(
            Path::new(path).join("preserved"),
            b"retained private contents",
        )?;
    }
    let mut owner = Owner::admit(
        &f.pointer,
        &f.operation,
        f.operation.created_unix_ms,
        Some(&installed),
        None,
        None,
    )?;
    owner.advance(Stage::Prepared)?;
    owner.retain_before_removal()?;
    std::fs::remove_file(&f.pointer)?;
    std::fs::remove_file(Path::new(&f.operation.locations.program).join(RECORD_FILE))?;
    owner.advance(Stage::RemovalApplied)?;
    let receipt = SetupReceipt {
        schema: SETUP_RECEIPT_SCHEMA.into(),
        operation_id: f.operation.operation_id,
        operation_sha256: files::digest_bytes(&canonical(&f.operation)),
        kind: OperationKind::Uninstall,
        expires_unix_ms: f.operation.expires_unix_ms,
        steps: vec![
            SetupStep {
                phase: SetupPhase::PointerRemoved,
                at_unix_ms: f.operation.created_unix_ms,
                outcome: "removed".into(),
                detail: String::new(),
            },
            SetupStep {
                phase: SetupPhase::RecordRemoved,
                at_unix_ms: f.operation.created_unix_ms + 1,
                outcome: "removed".into(),
                detail: String::new(),
            },
            SetupStep {
                phase: SetupPhase::Complete,
                at_unix_ms: f.operation.created_unix_ms + 2,
                outcome: "closed".into(),
                detail: String::new(),
            },
        ],
        retired: vec![],
        leftovers: vec![],
        failure: None,
        closed: true,
    };
    files::write_receipt(
        &OpenedDirectory::open(Path::new(&f.operation.locations.state))?,
        &receipt,
    )?;
    owner.settle(&receipt)?;
    drop(owner);
    assert!(retained_for(&f.pointer, &f.operation.locations)?.is_some());
    let cache = Path::new(&f.operation.locations.cache);
    std::fs::rename(cache, cache.with_file_name("displaced-cache"))?;
    airk_ipc::endpoint::prepare_root(cache)?;
    assert!(retained_for(&f.pointer, &f.operation.locations).is_err());
    assert_eq!(
        std::fs::read(cache.with_file_name("displaced-cache").join("preserved"))?,
        b"retained private contents"
    );
    Ok(())
}

#[test]
fn owned_failed_resume_retains_exact_failure_bytes_before_reopening_the_same_operation()
-> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    let mut receipt = SetupReceipt {
        schema: SETUP_RECEIPT_SCHEMA.into(),
        operation_id: f.operation.operation_id,
        operation_sha256: files::digest_bytes(&canonical(&f.operation)),
        kind: f.operation.kind,
        expires_unix_ms: f.operation.expires_unix_ms,
        steps: vec![SetupStep {
            phase: SetupPhase::Complete,
            at_unix_ms: f.operation.created_unix_ms,
            outcome: "closed".into(),
            detail: String::new(),
        }],
        retired: vec![],
        leftovers: vec![],
        failure: Some(airk_core::install_record::SetupFailure {
            code: "service_not_healthy".into(),
            message: "original failure detail".into(),
            at_unix_ms: f.operation.created_unix_ms,
        }),
        closed: true,
    };
    files::write_receipt(
        &OpenedDirectory::open(Path::new(&f.operation.locations.state))?,
        &receipt,
    )?;
    let original_bytes = canonical(&receipt);
    owner.reopen_failed(&mut receipt)?;
    assert!(!receipt.closed);
    assert!(receipt.failure.is_none());
    assert!(receipt.steps.is_empty());
    let archive = Path::new(&f.operation.locations.state)
        .join("setup/failures")
        .join(format!(
            "{}-{}.json",
            f.operation.operation_id,
            files::digest_bytes(&original_bytes)
        ));
    assert_eq!(std::fs::read(archive)?, original_bytes);
    let live = files::read_receipt(
        Path::new(&f.operation.locations.state),
        &f.operation.operation_id,
    )?
    .unwrap();
    assert_eq!(canonical(&live), canonical(&receipt));
    Ok(())
}

#[test]
fn prepared_ownership_refuses_start_even_after_the_original_operation_lifetime() -> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    assert_eq!(
        owns_control(&f.pointer, Path::new(&f.operation.locations.state))?,
        Some(true)
    );
    assert_eq!(
        owns_control(&f.pointer, &f._owner.path().join("unrelated-control"))?,
        Some(false)
    );
    assert!(startup_allowed(&f.pointer, Path::new(&f.operation.locations.state)).is_err());
    assert_eq!(
        admission_time(
            &f.pointer,
            &f.operation,
            files::digest_bytes(&canonical(&f.operation))
        )?,
        Some(f.operation.created_unix_ms)
    );
    assert!(
        airk_core::install_record::SetupOperation::decode(
            &canonical(&f.operation),
            f.operation.expires_unix_ms + 1
        )
        .is_err()
    );
    assert!(
        Owner::load(&f.pointer, &f.operation)?.is_some(),
        "the original owner remains recoverable using its bound admission time"
    );
    Ok(())
}

#[test]
fn a_root_appearing_during_admission_cannot_be_bound_as_original_ownership() -> Result<()> {
    let f = fixture(true)?;
    let before = admission_roots(&f.operation.locations)?;
    airk_ipc::endpoint::prepare_root(Path::new(&f.operation.locations.data))?;
    assert!(
        Owner::admit(
            &f.pointer,
            &f.operation,
            f.operation.created_unix_ms,
            None,
            Some(f.candidate.clone()),
            Some(&before)
        )
        .is_err()
    );
    assert!(!f.pointer.parent().unwrap().join(ACTIVE_FILE).exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn live_authority_rebinding_refuses_before_any_metadata_write_to_the_replacement() -> Result<()> {
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    let folder = f.pointer.parent().unwrap();
    let displaced = folder.with_file_name("displaced-authority");
    let original = std::fs::read(folder.join(ACTIVE_FILE))?;
    std::fs::rename(folder, &displaced)?;
    airk_ipc::endpoint::prepare_root(folder)?;
    assert!(owner.save(true).is_err());
    assert!(owner.revalidate().is_err());
    assert!(std::fs::read_dir(folder)?.next().is_none());
    assert_eq!(std::fs::read(displaced.join(ACTIVE_FILE))?, original);
    Ok(())
}

#[test]
fn removal_keeps_other_root_ids_and_uses_only_a_qualified_program_root_disposition() -> Result<()> {
    let mut f = fixture(false)?;
    f.operation.kind = OperationKind::Uninstall;
    f.operation.options.delete_indexes = IndexDeletion::Keep;
    let installed = Installed {
        pointer: f.original.pointer.clone(),
        record: f.original.record.clone(),
    };
    let mut owner = Owner::admit(
        &f.pointer,
        &f.operation,
        f.operation.created_unix_ms,
        Some(&installed),
        None,
        None,
    )?;
    owner.advance(Stage::Prepared)?;
    let before = snapshot(&f.operation.locations, &mut Vec::new())?.retained;
    std::fs::remove_file(&f.pointer)?;
    std::fs::remove_file(Path::new(&f.operation.locations.program).join(RECORD_FILE))?;
    owner.advance(Stage::RemovalApplied)?;
    let outcome = owner.remove_program_root(Path::new(&f.operation.locations.program));
    #[cfg(windows)]
    {
        outcome?;
        assert!(!Path::new(&f.operation.locations.program).exists());
    }
    #[cfg(unix)]
    {
        assert!(outcome.is_err());
        assert!(Path::new(&f.operation.locations.program).is_dir());
    }
    owner.revalidate()?;
    assert_eq!(
        snapshot(&f.operation.locations, &mut Vec::new())?.retained,
        before
    );
    Ok(())
}

#[test]
fn os_management_refuses_unsettled_and_wrong_operation_or_stage() -> Result<()> {
    let mut f = fixture(false)?;
    f.operation.options.login_startup = true;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    assert!(ordinary_management_allowed(&f.pointer).is_err());
    assert!(
        owner
            .authorize_login(f.candidate.record.install_id, "register")
            .is_err()
    );
    owner.advance(Stage::Prepared)?;
    write(
        &Path::new(&f.operation.locations.program).join(RECORD_FILE),
        &canonical(&f.candidate.record),
    )?;
    write(&f.pointer, &canonical(&f.candidate.pointer))?;
    owner.advance(Stage::PairPublished)?;
    owner.authorize_login(f.candidate.record.install_id, "register")?;
    assert!(
        owner
            .authorize_login(Incarnation::from_bytes([0x99; 16]), "register")
            .is_err()
    );
    assert!(
        owner
            .authorize_login(f.candidate.record.install_id, "unregister")
            .is_err()
    );
    Ok(())
}

#[test]
fn moved_program_retains_original_inventory_before_any_cleanup_and_resumes_without_it() -> Result<()>
{
    let mut f = fixture(false)?;
    let moved = Path::new(&f.operation.locations.program).with_file_name("new-program");
    airk_ipc::endpoint::prepare_root(&moved)?;
    f.operation.locations.program = moved.to_string_lossy().into_owned();
    f.candidate.record.locations = f.operation.locations.clone();
    f.candidate.pointer.program = f.operation.locations.program.clone();
    let hash = framed_digest(b"airk/install-id/1", &[f.operation.operation_id.as_bytes()]);
    let mut id = [0; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    f.candidate.record.install_id = Incarnation::from_bytes(id);
    f.candidate.pointer.install_id = f.candidate.record.install_id;
    f.candidate.pointer.record_sha256 = files::digest_bytes(&canonical(&f.candidate.record));
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    let (prior, root) = owner.moved_prior()?.unwrap();
    assert_eq!(prior.record, f.original.record);
    let bytes = serde_json::to_vec(&json!({
        "schema":"airk-service-payload/1","version":"1.9.0","channel":"stable","target":"x86_64-pc-windows-msvc",
        "executables":{"airk":"airk.exe","airkctl":"airkctl.exe"},
        "files":{"airk.exe":{"sha256":"11".repeat(32),"bytes":7},"airkctl.exe":{"sha256":"22".repeat(32),"bytes":3}},
        "admin_assets":f.original.record.admin_assets,"signing":f.original.record.signing,
    }))?;
    let inventory = Path::new(&prior.record.locations.program).join("payload-inventory.json");
    write(&inventory, &bytes)?;
    let expected = files::digest_bytes(&bytes);
    let first = owner.prior_inventory(&root, expected)?;
    assert_eq!(first.executables.airk, "airk.exe");
    std::fs::remove_file(&inventory)?;
    let resumed = owner.prior_inventory(&root, expected)?;
    assert_eq!(resumed.executables.airkctl, "airkctl.exe");
    assert!(
        owner
            .prior_inventory(
                &root,
                airk_core::identity::Sha256Digest::from_bytes([0x99; 32])
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(
            Path::new(&f.operation.locations.state)
                .join("setup/prior-inventories")
                .join(format!("{}.json", f.operation.operation_id))
        )?,
        bytes
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn pointer_saved_resume_reseals_same_original_missing_custody_and_refuses_foreign_pair()
-> Result<()> {
    use crate::setup_role::{inno, plan::Action};
    use airk_core::install_record::inno_handoff::{
        HANDOFF_FILE, HANDOFF_SCHEMA, InnoHandoff, MAX_HANDOFF_BYTES, ORIGINAL_OPERATION,
    };
    let f = fixture(false)?;
    let mut owner = admit(&f, false)?;
    owner.ensure_roots()?;
    owner.advance(Stage::Prepared)?;
    write(
        &Path::new(&f.operation.locations.program).join(RECORD_FILE),
        &canonical(&f.candidate.record),
    )?;
    write(&f.pointer, &canonical(&f.candidate.pointer))?;
    owner.advance(Stage::PairPublished)?;
    let program = OpenedDirectory::open(Path::new(&f.operation.locations.program))?;
    assert!(
        airk_ipc::private_record::read_private_at(&program, HANDOFF_FILE, MAX_HANDOFF_BYTES)?
            .is_none()
    );
    assert!(inno::resume_seal_required(true, Action::RegisterLogin));
    assert!(inno::resume_seal_required(true, Action::Start));
    assert!(!inno::resume_seal_required(false, Action::RegisterLogin));
    assert!(!inno::resume_seal_required(true, Action::Close));
    // This seam supplies the previously verified writer. Actual C10/file checks
    // stay in the production writer and the startup custody regression.
    let writer = |operation: &SetupOperation, image: &InstallationImage| -> Result<()> {
        assert_eq!(operation, &f.operation);
        assert_eq!(image, &f.candidate);
        let handoff = InnoHandoff {
            schema: HANDOFF_SCHEMA.into(),
            install_id: image.record.install_id,
            operation_id: operation.operation_id,
            version: image.record.version.clone(),
            capability: ORIGINAL_OPERATION.into(),
            uninstaller: airk_core::install_record::Executable {
                path: "unins000.exe".into(),
                sha256: files::digest_bytes(b"verified exe"),
                bytes: 12,
            },
            data: airk_core::install_record::Executable {
                path: "unins000.dat".into(),
                sha256: files::digest_bytes(b"verified dat"),
                bytes: 12,
            },
        };
        airk_ipc::private_record::write_private_at(
            &program,
            HANDOFF_FILE,
            &canonical(&handoff),
            MAX_HANDOFF_BYTES,
            false,
        )
    };
    inno::seal_owner_with(&mut owner, &f.operation, &f.pointer, writer)?;
    let saved =
        airk_ipc::private_record::read_private_at(&program, HANDOFF_FILE, MAX_HANDOFF_BYTES)?
            .unwrap();
    InnoHandoff::decode(&saved, &f.candidate).map_err(refusal)?;
    let mut sibling = f.operation.clone();
    sibling.operation_id = Incarnation::from_bytes([0x88; 16]);
    assert!(
        inno::seal_owner_with(&mut owner, &sibling, &f.pointer, |_, _| panic!(
            "sibling must never reach writer"
        ))
        .is_err()
    );
    write(
        &Path::new(&f.operation.locations.program).join(RECORD_FILE),
        &canonical(&f.original.record),
    )?;
    write(&f.pointer, &canonical(&f.original.pointer))?;
    assert!(
        inno::seal_owner_with(&mut owner, &f.operation, &f.pointer, |_, _| panic!(
            "foreign pair must never reach writer"
        ))
        .is_err()
    );
    assert_eq!(
        airk_ipc::private_record::read_private_at(&program, HANDOFF_FILE, MAX_HANDOFF_BYTES)?
            .unwrap(),
        saved
    );
    Ok(())
}
