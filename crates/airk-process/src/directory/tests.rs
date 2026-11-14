// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn a_pinned_directory_flushes_after_a_synced_entry_write() -> io::Result<()> {
    use std::io::Write;
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let directory = OpenedDirectory::open(&path)?;
    let before = directory.identity()?;
    let mut file = File::create(path.join("entry"))?;
    file.write_all(b"inert durable entry")?;
    file.sync_all()?;
    directory.sync_all()?;
    assert_eq!(directory.identity()?, before);
    assert_eq!(std::fs::read(path.join("entry"))?, b"inert durable entry");
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_exact_directory_removal_preserves_a_replacement_or_nonempty_root() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let parent = OpenedDirectory::open(&path)?;
    std::fs::create_dir(path.join("owned"))?;
    let original = parent.open_child(OsStr::new("owned"))?.identity()?;
    std::fs::rename(path.join("owned"), path.join("displaced"))?;
    std::fs::create_dir(path.join("owned"))?;
    assert!(
        parent
            .remove_child_if_matches(OsStr::new("owned"), original)
            .is_err()
    );
    assert!(path.join("owned").is_dir());
    assert!(path.join("displaced").is_dir());
    std::fs::write(path.join("owned/preserved"), b"foreign content")?;
    let replacement = parent.open_child(OsStr::new("owned"))?.identity()?;
    assert!(
        parent
            .remove_child_if_matches(OsStr::new("owned"), replacement)
            .is_err()
    );
    assert_eq!(
        std::fs::read(path.join("owned/preserved"))?,
        b"foreign content"
    );
    parent.remove_child_if_matches(OsStr::new("displaced"), original)?;
    assert!(!path.join("displaced").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_identity_bound_root_removal_refuses_without_unlinking_any_directory() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    std::fs::create_dir(path.join("owned"))?;
    let parent = OpenedDirectory::open(&path)?;
    let expected = parent.open_child(OsStr::new("owned"))?.identity()?;
    assert_eq!(
        parent
            .remove_child_if_matches(OsStr::new("owned"), expected)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
    assert!(path.join("owned").is_dir());
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_file_disposition_preserves_mismatch_and_excludes_writers_during_the_check()
-> io::Result<()> {
    use std::io::Read;
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let parent = OpenedDirectory::open(&path)?;
    std::fs::write(path.join("payload"), b"expected payload")?;
    assert!(!parent.remove_file_if(OsStr::new("payload"), |_, _| Ok(false))?);
    assert_eq!(std::fs::read(path.join("payload"))?, b"expected payload");
    assert!(parent.remove_file_if(OsStr::new("payload"), |file, proof| {
        proof.validate(file)?;
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(path.join("payload"))
                .is_err()
        );
        assert!(std::fs::rename(path.join("payload"), path.join("rebound")).is_err());
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes == b"expected payload")
    })?);
    assert!(!path.join("payload").exists());
    Ok(())
}

#[test]
fn an_open_directory_reports_the_same_native_identity_as_its_handle_and_path() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let first = OpenedDirectory::open(&path)?;
    let second = OpenedDirectory::open(&path)?;
    assert_eq!(first.identity()?, second.identity()?);
    assert_eq!(first.identity()?, identity(first.as_file())?);
    Ok(())
}

#[test]
fn opened_regular_file_identity_uses_native_objects_rather_than_matching_contents() -> io::Result<()>
{
    let owner = tempfile::tempdir()?;
    std::fs::write(owner.path().join("a"), b"same inert bytes")?;
    std::fs::write(owner.path().join("b"), b"same inert bytes")?;
    let a = File::open(owner.path().join("a"))?;
    let b = File::open(owner.path().join("b"))?;
    let second_a = File::open(owner.path().join("a"))?;
    assert_eq!(identity(&a)?, identity(&second_a)?);
    assert_ne!(identity(&a)?, identity(&b)?);
    Ok(())
}

#[test]
fn children_are_single_components_and_regular_files_are_not_directories() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    std::fs::create_dir(path.join("child"))?;
    std::fs::write(path.join("file"), b"inert fixture")?;
    let parent = OpenedDirectory::open(&path)?;
    let child = parent.open_child(OsStr::new("child"))?;
    assert_eq!(
        child.identity()?,
        OpenedDirectory::open(&path.join("child"))?.identity()?
    );
    assert_ne!(child.identity()?, parent.identity()?);
    for name in ["", ".", "..", "child/escape", "child/", "child/."] {
        assert_eq!(
            parent.open_child(OsStr::new(name)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "{name}"
        );
    }
    #[cfg(windows)]
    for name in ["child:stream", "child\0suffix"] {
        assert_eq!(
            parent.open_child(OsStr::new(name)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(parent.open_child(path.as_os_str()).is_err());
    assert!(parent.open_child(OsStr::new("file")).is_err());
    assert!(OpenedDirectory::open(Path::new("relative")).is_err());
    Ok(())
}

#[test]
fn open_children_retain_their_ancestors_after_the_callers_parent_is_dropped() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    std::fs::create_dir(path.join("parent"))?;
    std::fs::create_dir(path.join("parent").join("child"))?;
    let parent = OpenedDirectory::open(&path.join("parent"))?;
    let child = parent.open_child(OsStr::new("child"))?;
    let expected = child.identity()?;
    drop(parent);
    assert_eq!(child.identity()?, expected);
    #[cfg(windows)]
    assert!(std::fs::rename(path.join("parent"), path.join("moved")).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_renames_do_not_rebind_relative_child_opens_to_a_replacement_path() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("parent"))?;
    std::fs::create_dir(root.join("parent").join("child"))?;
    let parent = OpenedDirectory::open(&root.join("parent"))?;
    let original = parent.identity()?;
    let expected_child = parent.open_child(OsStr::new("child"))?.identity()?;
    std::fs::rename(root.join("parent"), root.join("moved"))?;
    std::fs::create_dir(root.join("parent"))?;
    std::fs::create_dir(root.join("parent").join("child"))?;
    assert_eq!(parent.identity()?, original);
    assert_ne!(
        OpenedDirectory::open(&root.join("parent"))?.identity()?,
        original
    );
    assert_eq!(
        parent.open_child(OsStr::new("child"))?.identity()?,
        expected_child
    );
    assert_ne!(
        OpenedDirectory::open(&root.join("parent").join("child"))?.identity()?,
        expected_child
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_symlink_directories_and_symlink_ancestors_refuse() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("real"))?;
    std::fs::create_dir(root.join("real").join("child"))?;
    std::os::unix::fs::symlink(root.join("real"), root.join("alias"))?;
    assert!(OpenedDirectory::open(&root.join("alias")).is_err());
    assert!(OpenedDirectory::open(&root.join("alias").join("child")).is_err());
    let parent = OpenedDirectory::open(&root)?;
    assert!(parent.open_child(OsStr::new("alias")).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_pins_names_until_all_directory_handles_are_dropped() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("parent"))?;
    let parent = OpenedDirectory::open(&root.join("parent"))?;
    let original = parent.identity()?;
    assert!(std::fs::rename(root.join("parent"), root.join("moved")).is_err());
    drop(parent);
    std::fs::rename(root.join("parent"), root.join("moved"))?;
    std::fs::create_dir(root.join("parent"))?;
    assert_eq!(
        OpenedDirectory::open(&root.join("moved"))?.identity()?,
        original
    );
    assert_ne!(
        OpenedDirectory::open(&root.join("parent"))?.identity()?,
        original
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_pins_an_empty_child_and_its_ancestors_after_the_parent_value_is_dropped()
-> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let parent_path = root.join("parent");
    let child_path = parent_path.join("child");
    std::fs::create_dir(&parent_path)?;
    std::fs::create_dir(&child_path)?;
    let parent = OpenedDirectory::open(&parent_path)?;
    let child = parent.open_child(OsStr::new("child"))?;
    let expected = child.identity()?;
    drop(parent);
    assert!(std::fs::rename(&child_path, parent_path.join("moved-child")).is_err());
    assert!(std::fs::rename(&parent_path, root.join("moved-parent")).is_err());
    assert_eq!(child.identity()?, expected);
    drop(child);
    std::fs::rename(&child_path, parent_path.join("moved-child"))?;
    std::fs::rename(&parent_path, root.join("moved-parent"))?;
    assert_eq!(
        OpenedDirectory::open(&root.join("moved-parent").join("moved-child"))?.identity()?,
        expected
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_junction_directories_and_junction_ancestors_refuse() -> io::Result<()> {
    use std::{os::windows::process::CommandExt, process::Command};
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("real"))?;
    std::fs::create_dir(root.join("real").join("child"))?;
    // Junction creation is unprivileged. No symlink privilege/developer-mode
    // skip can silently turn this refusal test into a pass.
    let made = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(owner.path().join("alias"))
        .arg(owner.path().join("real"))
        .creation_flags(0x0800_0000)
        .output()?;
    assert!(
        made.status.success(),
        "junction fixture: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    assert!(OpenedDirectory::open(&root.join("alias")).is_err());
    assert!(OpenedDirectory::open(&root.join("alias").join("child")).is_err());
    let parent = OpenedDirectory::open(&root)?;
    assert!(parent.open_child(OsStr::new("alias")).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_network_and_device_prefixes_refuse_before_opening() {
    for path in [
        r"\\server\share\folder",
        r"\\?\UNC\server\share\folder",
        r"\\.\PhysicalDrive0",
    ] {
        assert_eq!(
            OpenedDirectory::open(Path::new(path)).unwrap_err().kind(),
            io::ErrorKind::Unsupported,
            "{path}"
        );
    }
}

#[cfg(windows)]
#[test]
fn pending_file_disposition_restores_false_error_and_foreign_streams() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let root = OpenedDirectory::open(&path)?;
    let file = path.join("payload");
    std::fs::write(&file, b"original")?;
    let extra = std::path::PathBuf::from(format!("{}:foreign", file.display()));
    std::fs::write(&extra, b"foreign stream")?;
    let mut called = false;
    assert!(
        root.remove_file_if(OsStr::new("payload"), |_, _| {
            called = true;
            Ok(true)
        })
        .is_err()
    );
    assert!(
        !called,
        "foreign stream is refused before ownership predicate"
    );
    assert_eq!(std::fs::read(&file)?, b"original");
    assert_eq!(std::fs::read(&extra)?, b"foreign stream");
    std::fs::remove_file(&extra)?;
    assert!(
        root.remove_file_if(OsStr::new("payload"), |_, _| Err(io::Error::other(
            "injected predicate failure"
        )))
        .is_err()
    );
    assert_eq!(std::fs::read(&file)?, b"original");
    assert!(!root.remove_file_if(OsStr::new("payload"), |_, _| Ok(false))?);
    assert_eq!(std::fs::read(&file)?, b"original");
    std::fs::hard_link(&file, path.join("alias"))?;
    assert!(
        root.remove_file_if(OsStr::new("payload"), |_, _| Ok(true))
            .is_err()
    );
    assert_eq!(std::fs::read(path.join("alias"))?, b"original");
    Ok(())
}

#[cfg(windows)]
#[test]
fn pending_file_disposition_blocks_new_alternate_stream_before_predicate_completion()
-> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let root = OpenedDirectory::open(&path)?;
    std::fs::write(path.join("payload"), b"original")?;
    assert!(root.remove_file_if(OsStr::new("payload"), |file, proof| {
        proof.validate(file)?;
        root.require_default_stream(OsStr::new("payload"), file)?;
        let stream = std::path::PathBuf::from(format!("{}:racing", path.join("payload").display()));
        let error = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stream)
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
        Ok(true)
    })?);
    assert!(!path.join("payload").exists());
    Ok(())
}

#[cfg(windows)]
#[test]
fn pending_file_disposition_unwind_restores_original_object() -> io::Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let root = OpenedDirectory::open(&path)?;
    std::fs::write(path.join("payload"), b"unapproved original")?;
    let original = identity(&std::fs::File::open(path.join("payload"))?)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = root.remove_file_if(OsStr::new("payload"), |file, proof| {
            proof.validate(file)?;
            root.require_default_stream(OsStr::new("payload"), file)?;
            panic!("injected ownership predicate panic");
        });
    }));
    assert!(result.is_err());
    assert_eq!(std::fs::read(path.join("payload"))?, b"unapproved original");
    assert_eq!(
        identity(&std::fs::File::open(path.join("payload"))?)?,
        original
    );
    let extra =
        std::path::PathBuf::from(format!("{}:after-unwind", path.join("payload").display()));
    std::fs::write(&extra, b"still writable after cancellation")?;
    assert!(
        root.remove_file_if(OsStr::new("payload"), |_, _| Ok(true))
            .is_err()
    );
    assert_eq!(std::fs::read(&extra)?, b"still writable after cancellation");
    Ok(())
}
