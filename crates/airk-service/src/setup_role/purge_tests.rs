// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_process::directory::OpenedDirectory;

#[test]
fn purge_preserves_all_selected_root_objects_and_the_state_setup_receipt() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    for role in ["data", "state", "cache", "logs"] {
        let path = root.join(role);
        std::fs::create_dir(&path)?;
        std::fs::write(path.join("content"), b"private fixture contents")?;
        let before = OpenedDirectory::open(&path)?.identity()?;
        if role == "state" {
            std::fs::create_dir(path.join("setup"))?;
            std::fs::write(path.join("setup/original.json"), b"original receipt")?;
        }
        let keep = if role == "state" {
            &["setup"][..]
        } else {
            &[][..]
        };
        assert_eq!(effects::purge(&path, &[], keep)?, 1);
        assert_eq!(OpenedDirectory::open(&path)?.identity()?, before, "{role}");
        assert!(!path.join("content").exists());
        if role == "state" {
            assert_eq!(
                std::fs::read(path.join("setup/original.json"))?,
                b"original receipt"
            );
        }
        assert_eq!(effects::purge(&path, &[], keep)?, 0);
    }
    Ok(())
}

#[test]
fn purge_refuses_an_ancestor_of_a_protected_root_without_deleting_contents() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    std::fs::write(root.join("preserved"), b"owned contents")?;
    assert!(effects::purge(&root, &[root.join("program")], &[]).is_err());
    assert_eq!(std::fs::read(root.join("preserved"))?, b"owned contents");
    Ok(())
}

#[cfg(unix)]
#[test]
fn pinned_purge_does_not_follow_root_rebinding_or_descendant_symlinks() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let selected = root.join("data");
    std::fs::create_dir(&selected)?;
    std::fs::write(selected.join("old-index"), b"owned index")?;
    let source = root.join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("document"), b"read-only source fixture")?;
    std::os::unix::fs::symlink(&source, selected.join("source-link"))?;
    let held = OpenedDirectory::open(&selected)?;
    std::fs::rename(&selected, root.join("moved-data"))?;
    std::fs::create_dir(&selected)?;
    std::fs::write(selected.join("unrelated"), b"replacement root")?;
    assert_eq!(effects::purge_held(&held, &selected, &[], &[])?, 2);
    assert_eq!(
        std::fs::read(source.join("document"))?,
        b"read-only source fixture"
    );
    assert_eq!(
        std::fs::read(selected.join("unrelated"))?,
        b"replacement root"
    );
    assert!(std::fs::read_dir(root.join("moved-data"))?.next().is_none());
    Ok(())
}
