// SPDX-License-Identifier: Apache-2.0
use super::super::{io, writer_test_support::Editable};
use super::*;

#[test]
fn config_retained_available_bytes_widens_native_fields_and_refuses_overflow() {
    // Darwin's available-block field is u32 while the fragment size is u64.
    let native_blocks: u32 = 6_000_000;
    let native_fragment: u64 = 8192;
    assert_eq!(
        platform::checked_available_bytes(u128::from(native_blocks), u128::from(native_fragment))
            .unwrap(),
        49_152_000_000
    );
    assert_eq!(
        platform::checked_available_bytes(u128::from(u64::MAX), 1).unwrap(),
        u64::MAX
    );
    assert_eq!(platform::checked_available_bytes(0, u128::MAX).unwrap(), 0);
    for (blocks, fragment) in [(u128::from(u64::MAX), 2), (u128::MAX, 2)] {
        let error = platform::checked_available_bytes(blocks, fragment).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(error.to_string(), "available disk count overflow");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn config_retained_available_bytes_observes_actual_pinned_apfs_directory() {
    let fixture = Editable::new();
    let parent = fixture.path().join("disk-observation");
    airk_ipc::endpoint::prepare_root(&parent).unwrap();
    let parent = fs::canonicalize(parent).unwrap();
    let directory = OpenedDirectory::open(&parent).unwrap();
    // Exercise the actual fstatvfs fields without assuming a free-space floor
    // or comparing changing disk-space snapshots.
    let _available: u64 = platform::available_bytes(&directory, &parent).unwrap();
}

fn one_receipt(parent: &Path) -> (PathBuf, Receipt) {
    let root = parent.join(".airk/agent-edits/v1");
    let paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(paths.len(), 1);
    let receipt = Receipt::decode(&fs::read(paths[0].join("receipt.json")).unwrap()).unwrap();
    (paths[0].clone(), receipt)
}

fn retained_path(directory: &Path, receipt: &Receipt) -> PathBuf {
    directory.join(if receipt.primitive == Primitive::ReplaceFile {
        "before"
    } else {
        "prepared"
    })
}

#[test]
fn config_retained_writer_publishes_exact_bytes_and_retains_the_actual_original_object() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let original = airk_process::directory::identity(&File::open(&path).unwrap()).unwrap();
    assert!(
        io::edit_file_for(
            Operation::Connect,
            &path,
            Some(fixture.path()),
            false,
            |_| Ok("prepared\n".into())
        )
        .unwrap()
    );
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    let (directory, receipt) = one_receipt(fixture.path());
    assert_eq!(receipt.state, State::PublishedRetained);
    assert_eq!(receipt.operation, Operation::Connect);
    assert_eq!(
        fs::read(retained_path(&directory, &receipt)).unwrap(),
        b"original\n"
    );
    let retained = airk_process::directory::identity(
        &File::open(retained_path(&directory, &receipt)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        original, retained,
        "retain the original object, not a copied before-image"
    );
}

#[test]
fn config_retained_writer_keeps_in_place_concurrent_bytes_and_reports_published_conflict() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let result = io::with_before_publish(
        |path| fs::write(path, b"concurrent\n").unwrap(),
        || {
            io::edit_file_for(
                Operation::Repair,
                &path,
                Some(fixture.path()),
                false,
                |_| Ok("prepared\n".into()),
            )
        },
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::ConflictRetained);
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    let (directory, receipt) = one_receipt(fixture.path());
    assert_eq!(receipt.state, State::ConflictRetained);
    assert_eq!(
        fs::read(retained_path(&directory, &receipt)).unwrap(),
        b"concurrent\n"
    );
}

#[test]
fn config_retained_writer_keeps_atomic_editor_replacement_and_reports_conflict() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let result = io::with_before_publish(
        |path| {
            let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
            file.write_all(b"editor replacement\n").unwrap();
            file.as_file().sync_all().unwrap();
            file.persist(path).unwrap();
        },
        || {
            io::edit_file_for(
                Operation::Remove,
                &path,
                Some(fixture.path()),
                false,
                |_| Ok("prepared\n".into()),
            )
        },
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::ConflictRetained);
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    let (directory, receipt) = one_receipt(fixture.path());
    assert_eq!(receipt.state, State::ConflictRetained);
    assert_eq!(
        fs::read(retained_path(&directory, &receipt)).unwrap(),
        b"editor replacement\n"
    );
}

#[cfg(windows)]
#[test]
fn config_retained_writer_review_releases_source_for_editor_atomic_save() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"reviewed original\n").unwrap();
    let review = super::review(&path).unwrap();
    let original_identity = review.original.identity.clone();
    let mut editor = tempfile::NamedTempFile::new_in(fixture.path()).unwrap();
    editor.write_all(b"editor atomic save\n").unwrap();
    editor.as_file().sync_all().unwrap();
    editor.persist(&path).unwrap();
    assert_eq!(review.before(), "reviewed original\n");
    assert_eq!(fs::read(&path).unwrap(), b"editor atomic save\n");
    let current = fs::File::open(&path).unwrap();
    assert_ne!(
        super::platform::identity(&current).unwrap(),
        original_identity
    );
    assert_eq!(
        fs::read_dir(fixture.path().join(".airk/agent-edits/v1"))
            .unwrap()
            .count(),
        1,
        "review alone creates only the ownership marker, never a publication receipt"
    );
}

#[test]
fn config_retained_writer_never_recreates_a_late_user_deletion() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let result = io::with_before_publish(
        |path| fs::remove_file(path).unwrap(),
        || {
            io::edit_file_for(
                Operation::Repair,
                &path,
                Some(fixture.path()),
                false,
                |_| Ok("prepared\n".into()),
            )
        },
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    assert!(!path.exists());
    let (_, receipt) = one_receipt(fixture.path());
    assert_eq!(receipt.state, State::RefusedRetained);
    assert_eq!(receipt.after.unwrap().target, Observation::Absent);
}

#[test]
fn config_retained_writer_noop_observer_preserves_exact_proposal_and_receipt() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = count.clone();
    let result = io::with_before_publish(
        move |_| observed.set(observed.get() + 1),
        || {
            io::edit_file_for(
                Operation::Repair,
                &path,
                Some(fixture.path()),
                false,
                |_| Ok("prepared\n".into()),
            )
        },
    );
    assert!(result.unwrap());
    assert_eq!(count.get(), 1);
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    assert_eq!(
        one_receipt(fixture.path()).1.state,
        State::PublishedRetained
    );
}

#[test]
fn config_retained_writer_unknown_context_refuses_before_original_bytes_or_callback() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("private");
    airk_ipc::endpoint::prepare_root(&parent).unwrap();
    let path = parent.join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let result = io::edit_file_for(Operation::Connect, &path, Some(&parent), false, |_| {
        panic!("unqualified context must not reach the source-edit callback")
    });
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    assert_eq!(fs::read(&path).unwrap(), b"original\n");
    assert!(!parent.join(".airk").exists());
}

#[test]
fn config_retained_writer_receipt_failure_keeps_both_objects_and_blocks_replay_before_source_read()
{
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let result = with_final_record_failure(|| {
        io::edit_file_for(
            Operation::Connect,
            &path,
            Some(fixture.path()),
            false,
            |_| Ok("prepared\n".into()),
        )
    });
    assert_eq!(result.unwrap_err().kind, EditFailureKind::OutcomeUnknown);
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    let (directory, receipt) = one_receipt(fixture.path());
    assert_eq!(
        receipt.state,
        State::PublicationStarted,
        "the last synced record never claims final success"
    );
    assert_eq!(
        fs::read(retained_path(&directory, &receipt)).unwrap(),
        b"original\n"
    );
    // Invalid source UTF-8 distinguishes pending admission from a source read:
    // the root barrier must refuse before decoding this later user input.
    fs::write(&path, [0xff]).unwrap();
    let refused = io::edit_file_for(
        Operation::Connect,
        &path,
        Some(fixture.path()),
        false,
        |_| panic!("pending publication cannot reach another source-edit callback"),
    )
    .unwrap_err();
    assert_eq!(refused.kind, EditFailureKind::Refused);
    assert!(refused.to_string().contains("retained transaction"));
    assert_eq!(fs::read(&path).unwrap(), [0xff]);
    assert_eq!(one_receipt(fixture.path()).0, directory);
}

#[test]
fn config_retained_writer_preserves_open_writer_growth_and_refuses_new_admission_over_budget() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let original = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(
        io::edit_file_for(
            Operation::Repair,
            &path,
            Some(fixture.path()),
            false,
            |_| Ok("prepared\n".into())
        )
        .unwrap()
    );
    let (directory, receipt) = one_receipt(fixture.path());
    let retained = retained_path(&directory, &receipt);
    assert_eq!(
        airk_process::directory::identity(&original).unwrap(),
        airk_process::directory::identity(&File::open(&retained).unwrap()).unwrap()
    );
    original.set_len(64 * 1024 * 1024 + 1).unwrap();
    original.sync_all().unwrap();
    let refused = io::edit_file_for(
        Operation::Repair,
        &path,
        Some(fixture.path()),
        false,
        |_| panic!("an enlarged retained object must stop admission before source reads"),
    )
    .unwrap_err();
    assert_eq!(refused.kind, EditFailureKind::Refused);
    assert_eq!(
        fs::metadata(&retained).unwrap().len(),
        67_108_865,
        "never truncate or delete the open writer's actual object"
    );
    assert_eq!(fs::read(&path).unwrap(), b"prepared\n");
    assert_eq!(one_receipt(fixture.path()).0, directory);
}

#[test]
fn config_retained_writer_capacity_refuses_seventeenth_transaction_without_eviction() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, "initial\n").unwrap();
    for version in 0..16 {
        assert!(
            io::edit_file_for(
                Operation::Repair,
                &path,
                Some(fixture.path()),
                false,
                |_| Ok(format!("version {version}\n"))
            )
            .unwrap()
        );
    }
    let result = io::edit_file_for(
        Operation::Repair,
        &path,
        Some(fixture.path()),
        false,
        |_| panic!("capacity refusal must precede the next source-edit callback"),
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    let count = fs::read_dir(fixture.path().join(".airk/agent-edits/v1"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .count();
    assert_eq!(count, 16);
    assert_eq!(fs::read_to_string(path).unwrap(), "version 15\n");
}

#[cfg(windows)]
#[test]
fn config_retained_writer_refuses_unqualified_named_streams_without_archiving_source() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let stream = PathBuf::from(format!("{}:synthetic_fixture", path.display()));
    fs::write(&stream, b"independent stream\n").unwrap();
    let result = io::edit_file_for(
        Operation::Repair,
        &path,
        Some(fixture.path()),
        false,
        |_| panic!("named streams require manual editing before source capture"),
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    assert_eq!(fs::read(&path).unwrap(), b"original\n");
    assert_eq!(fs::read(stream).unwrap(), b"independent stream\n");
    assert!(!fixture.path().join(".airk").exists());
}

#[test]
fn config_retained_writer_nested_project_file_uses_its_actual_parent_binding() {
    let fixture = Editable::new();
    let parent = fixture.path().join(".vscode");
    airk_ipc::endpoint::prepare_root(&parent).unwrap();
    let path = parent.join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    assert!(
        io::edit_file_for(
            Operation::Connect,
            &path,
            Some(fixture.path()),
            false,
            |_| Ok("prepared\n".into())
        )
        .unwrap()
    );
    let (directory, receipt) = one_receipt(&parent);
    assert_eq!(receipt.state, State::PublishedRetained);
    assert_eq!(
        receipt.parent,
        platform::identity(OpenedDirectory::open(&parent).unwrap().as_file()).unwrap()
    );
    assert_eq!(
        fs::read(retained_path(&directory, &receipt)).unwrap(),
        b"original\n"
    );
}

#[test]
fn config_retained_writer_never_adopts_or_rewrites_unknown_root_markers() {
    let fixture = Editable::new();
    let path = fixture.path().join("mcp.json");
    fs::write(&path, b"original\n").unwrap();
    let marker = fixture.path().join(".airk/agent-edits/v1/owner.json");
    for relative in [".airk", ".airk/agent-edits", ".airk/agent-edits/v1"] {
        airk_ipc::endpoint::prepare_root(&fixture.path().join(relative)).unwrap();
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .unwrap();
    }
    file.write_all(br#"{"schema":"airk.agent-config-edit-root/99"}"#)
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    let result = io::edit_file_for(
        Operation::Remove,
        &path,
        Some(fixture.path()),
        false,
        |_| panic!("unknown marker must refuse before source decoding/editing"),
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    assert_eq!(fs::read(&path).unwrap(), b"original\n");
    assert_eq!(
        fs::read(marker).unwrap(),
        br#"{"schema":"airk.agent-config-edit-root/99"}"#
    );
}
