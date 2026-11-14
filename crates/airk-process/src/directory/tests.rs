// SPDX-License-Identifier: Apache-2.0
use super::*;

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
    for name in ["", ".", "..", "child/escape"] {
        assert_eq!(
            parent.open_child(OsStr::new(name)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
            "{name}"
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
