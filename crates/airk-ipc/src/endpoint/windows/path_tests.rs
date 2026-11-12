// SPDX-License-Identifier: Apache-2.0
//! Actual native filesystem/ACL boundaries; no service or user configuration.
use super::*;
use std::os::windows::ffi::OsStringExt;

fn display(path: &Path) -> Result<String> {
    let value = filesystem_path(path)?;
    Ok(String::from_utf16(&value[..value.len() - 1])?)
}
#[test]
fn native_filesystem_paths_preserve_disk_unc_and_reject_device_or_traversal() -> Result<()> {
    assert_eq!(
        display(Path::new(r"C:\missing\leaf"))?,
        r"\\?\C:\missing\leaf"
    );
    assert_eq!(
        display(Path::new(r"\\server\share\missing\leaf"))?,
        r"\\?\UNC\server\share\missing\leaf"
    );
    assert_eq!(
        display(Path::new(r"\\?\C:\missing\leaf"))?,
        r"\\?\C:\missing\leaf"
    );
    assert_eq!(
        display(Path::new(r"\\?\UNC\server\share\missing\leaf"))?,
        r"\\?\UNC\server\share\missing\leaf"
    );
    assert!(filesystem_path(Path::new(r"\\.\pipe\not-a-file")).is_err());
    assert!(filesystem_path(Path::new(r"\\?\C:leaf")).is_err());
    assert!(filesystem_path(Path::new(r"C:\parent\..\leaf")).is_err());
    let nul = std::ffi::OsString::from_wide(&[67, 58, 92, 0, 120]);
    assert!(filesystem_path(Path::new(&nul)).is_err());
    Ok(())
}

#[test]
fn native_private_create_and_acl_checks_support_long_unprefixed_paths() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut root = owner.path().to_owned();
    // Keep the input unprefixed, as the runtime Stage CLI receives it.
    let raw = root.as_os_str().encode_wide().collect::<Vec<_>>();
    use std::path::{Component, Prefix};
    root = match root.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(_) => {
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&raw[4..]))
            }
            Prefix::VerbatimUNC(_, _) => {
                let unc = [92u16, 92]
                    .into_iter()
                    .chain(raw.into_iter().skip(8))
                    .collect::<Vec<_>>();
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&unc))
            }
            Prefix::Disk(_) | Prefix::UNC(_, _) => root,
            _ => anyhow::bail!("unsupported owned fixture volume"),
        },
        _ => anyhow::bail!("fixture path has no native volume"),
    };
    ensure!(root.is_absolute(), "fixture path must remain absolute");
    while root.as_os_str().encode_wide().count() < 300 {
        root.push("owned-long-path-component-0123456789abcdef");
        prepare_root(&root)?;
    }
    let leaf = root.join("exclusive-native-leaf");
    create_root(&leaf)?;
    require_existing_root(&leaf)?;
    assert!(
        create_root(&leaf).is_err(),
        "exclusive creation overwrote an existing directory"
    );
    prepare_root(&leaf)?;
    let file = leaf.join("private-descriptor.json");
    std::fs::write(&file, b"preserved descriptor")?;
    require_owner_private(&file)?;
    assert!(
        require_existing_root(&file).is_err(),
        "file accepted as private directory"
    );
    assert_eq!(std::fs::read(&file)?, b"preserved descriptor");
    assert!(!leaf.join("uncreated").exists());
    Ok(())
}

fn grant(folder: &Path, entry: &str) -> Result<()> {
    let status = std::process::Command::new("icacls")
        .arg(folder)
        .args(["/grant", entry, "/q"])
        .status()?;
    ensure!(status.success(), "icacls {entry} failed");
    Ok(())
}

#[test]
fn location_parents_accept_read_only_grants_and_refuse_write_grants() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let folder = temporary.path().join("parent");
    prepare_root(&folder)?;
    require_not_writable_by_others(&folder)?;
    // BUILTIN\Users read and execute, inherited below: accepted for a parent,
    // refused by the owner-private rule.
    grant(&folder, "*S-1-5-32-545:(OI)(CI)(RX)")?;
    require_not_writable_by_others(&folder)?;
    assert!(require_owner_private(&folder).is_err());
    grant(&folder, "*S-1-5-32-545:(OI)(CI)(M)")?;
    let refusal = format!("{:#}", require_not_writable_by_others(&folder).unwrap_err());
    assert!(refusal.contains("S-1-5-32-545"), "{refusal}");
    assert!(refusal.contains("/remove:g *S-1-5-32-545"), "{refusal}");
    Ok(())
}
