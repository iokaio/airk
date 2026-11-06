// SPDX-License-Identifier: Apache-2.0
//! Legacy admission must distinguish genuine absence from unavailable metadata.
use super::*;

#[test]
fn genuinely_absent_native_markers_keep_legacy_mode_without_creating_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("uncreated-runtime");
    assert!(!shared_mode(&root)?);
    assert!(
        !root.exists(),
        "classification must not initialize runtime storage"
    );
    std::fs::create_dir(&root)?;
    assert!(!shared_mode(&root)?);
    assert_eq!(std::fs::read_dir(&root)?.count(), 0);
    Ok(())
}

#[test]
fn nonregular_or_corrupt_native_markers_never_select_legacy_mode() -> Result<()> {
    for marker in ["service-1.4.sqlite3", "service-1.4.owner.json"] {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path();
        let directory = root.join(marker);
        std::fs::create_dir(&directory)?;
        std::fs::write(directory.join("preserved"), b"not a native metadata file")?;
        assert!(
            shared_mode(root).is_err(),
            "nonregular marker admitted: {marker}"
        );
        assert_eq!(
            std::fs::read(directory.join("preserved"))?,
            b"not a native metadata file"
        );
        assert_eq!(std::fs::read_dir(root)?.count(), 1);
    }
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let database = root.join("service-1.4.sqlite3");
    let owner = root.join("service-1.4.owner.json");
    std::fs::write(&database, b"unrecognized registry")?;
    std::fs::write(&owner, b"unrecognized native owner")?;
    assert!(
        shared_mode(root).is_err(),
        "corruption must still reach a refusal"
    );
    assert_eq!(std::fs::read(&database)?, b"unrecognized registry");
    assert_eq!(std::fs::read(&owner)?, b"unrecognized native owner");
    assert_eq!(std::fs::read_dir(root)?.count(), 2);
    Ok(())
}

#[cfg(unix)]
#[test]
fn dangling_native_registry_or_owner_marker_never_selects_legacy_mode() -> Result<()> {
    use std::os::unix::fs::symlink;
    for marker in ["service-1.4.sqlite3", "service-1.4.owner.json"] {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path();
        let target = root.join("missing-native-metadata");
        let alias = root.join(marker);
        symlink(&target, &alias)?;
        assert!(
            !alias.try_exists()?,
            "fixture must reproduce following-existence ambiguity"
        );
        assert!(
            shared_mode(root).is_err(),
            "dangling marker selected legacy mode: {marker}"
        );
        assert_eq!(std::fs::read_link(&alias)?, target);
        assert!(!target.exists());
        assert_eq!(
            std::fs::read_dir(root)?.count(),
            1,
            "classification created native state"
        );
    }
    Ok(())
}
