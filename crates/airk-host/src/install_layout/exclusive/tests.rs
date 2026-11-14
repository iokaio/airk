// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_process::directory::OpenedDirectory;
use anyhow::Result;
use std::ffi::OsStr;

#[test]
fn rooted_publication_never_replaces_an_existing_file_or_directory() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let parent = OpenedDirectory::open(&root)?;
    std::fs::write(root.join("source"), b"new bytes")?;
    std::fs::write(root.join("destination"), b"original bytes")?;
    assert!(
        publish_new_at(
            parent.as_file(),
            &root,
            OsStr::new("source"),
            OsStr::new("destination")
        )
        .is_err()
    );
    assert_eq!(std::fs::read(root.join("destination"))?, b"original bytes");
    assert_eq!(std::fs::read(root.join("source"))?, b"new bytes");
    std::fs::create_dir(root.join("source-dir"))?;
    std::fs::create_dir(root.join("destination-dir"))?;
    assert!(
        publish_new_at(
            parent.as_file(),
            &root,
            OsStr::new("source-dir"),
            OsStr::new("destination-dir")
        )
        .is_err()
    );
    assert!(root.join("source-dir").is_dir());
    Ok(())
}

#[test]
fn rooted_publication_moves_only_the_named_child_and_preserves_native_identity() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let parent = OpenedDirectory::open(&root)?;
    std::fs::create_dir(root.join("staged"))?;
    let original = OpenedDirectory::open(&root.join("staged"))?.identity()?;
    publish_new_at(
        parent.as_file(),
        &root,
        OsStr::new("staged"),
        OsStr::new("selected"),
    )?;
    assert_eq!(
        parent.open_child(OsStr::new("selected"))?.identity()?,
        original
    );
    assert!(!root.join("staged").exists());
    parent.sync_all()?;
    Ok(())
}

#[test]
fn rooted_publication_refuses_traversal_and_multiple_components() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let parent = OpenedDirectory::open(&root)?;
    std::fs::write(root.join("source"), b"preserved")?;
    for name in ["", ".", "..", "../outside", "nested/child"] {
        assert!(
            publish_new_at(
                parent.as_file(),
                &root,
                OsStr::new("source"),
                OsStr::new(name)
            )
            .is_err(),
            "{name}"
        );
    }
    assert_eq!(std::fs::read(root.join("source"))?, b"preserved");
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_rooted_publication_refuses_a_different_named_parent() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("first"))?;
    std::fs::create_dir(root.join("other"))?;
    let parent = OpenedDirectory::open(&root.join("first"))?;
    std::fs::write(root.join("other/source"), b"foreign")?;
    assert!(
        publish_new_at(
            parent.as_file(),
            &root.join("other"),
            OsStr::new("source"),
            OsStr::new("destination")
        )
        .is_err()
    );
    assert_eq!(std::fs::read(root.join("other/source"))?, b"foreign");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_rooted_publication_uses_the_original_object_after_path_rebinding() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::create_dir(root.join("parent"))?;
    let parent = OpenedDirectory::open(&root.join("parent"))?;
    std::fs::write(root.join("parent/source"), b"original")?;
    std::fs::rename(root.join("parent"), root.join("moved"))?;
    std::fs::create_dir(root.join("parent"))?;
    std::fs::write(root.join("parent/source"), b"replacement")?;
    publish_new_at(
        parent.as_file(),
        &root.join("parent"),
        OsStr::new("source"),
        OsStr::new("destination"),
    )?;
    assert_eq!(std::fs::read(root.join("moved/destination"))?, b"original");
    assert_eq!(std::fs::read(root.join("parent/source"))?, b"replacement");
    Ok(())
}
