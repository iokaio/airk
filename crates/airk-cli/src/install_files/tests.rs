// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::Result;

#[test]
fn regular_single_name_file_is_accepted() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"original fixture")?;
    safe_file(&path)
}

#[test]
fn directories_missing_paths_and_hard_link_aliases_are_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(safe_file(directory.path()).is_err());
    assert!(safe_file(&directory.path().join("absent")).is_err());
    let original = directory.path().join("original.bin");
    std::fs::write(&original, b"original fixture")?;
    std::fs::hard_link(&original, directory.path().join("alias.bin"))?;
    let error = safe_file(&original).expect_err("an aliased file must be refused");
    assert!(
        error.to_string().contains("aliased runtime file"),
        "{error:#}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("target.bin");
    std::fs::write(&target, b"original fixture")?;
    let link = directory.path().join("link.bin");
    std::os::unix::fs::symlink(&target, &link)?;
    assert!(safe_file(&link).is_err());
    Ok(())
}

#[test]
fn installed_names_match_the_platform_payload_layout() {
    let root = Path::new("install-root");
    if cfg!(windows) {
        assert_eq!(main_name(), "airk.exe");
        assert_eq!(launcher_path(root), root.join("airk-launcher.exe"));
    } else if cfg!(target_os = "macos") {
        assert_eq!(main_name(), "AIRK.app/Contents/MacOS/airk");
        assert_eq!(launcher_path(root), root.join("airk-launcher"));
    } else {
        assert_eq!(main_name(), "airk");
        assert_eq!(launcher_path(root), root.join("airk-launcher"));
    }
    assert_eq!(
        MAC_LAUNCHER_IMAGE,
        "AIRK.app/Contents/MacOS/airk-launcher-image"
    );
}
